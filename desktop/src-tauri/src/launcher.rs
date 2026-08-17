//! Sidecar process management: spawn `npx @deepseek-ai/dsh web` as a child,
//! wait for its readiness line, and recycle the whole process tree on exit.

use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(unix)]
use crate::process_impl_unix as process_impl;
#[cfg(windows)]
use crate::process_impl_windows as process_impl;

/// The CLI prints this readiness line on stdout once the web server is
/// listening (`printUrl` defaults on). The OS-assigned port is read from it.
const READY_PREFIX: &str = "dsh web: http://";
/// Ring-buffer cap for captured child output.
const LOG_CAPACITY: usize = 1000;

/// Child output and readiness, shared between the reader threads and callers.
#[derive(Default)]
struct Shared {
    lines: Vec<String>,
    ready_url: Option<String>,
}

/// Snapshot of the sidecar for the `server_info` command.
#[derive(serde::Serialize)]
pub struct ServerStatus {
    pub url: Option<String>,
    pub pid: Option<u32>,
    pub running: bool,
    pub lines: Vec<String>,
}

/// Owns the dsh child process and its output streams.
pub struct Launcher {
    child: Mutex<Option<Child>>,
    shared: Arc<Mutex<Shared>>,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            shared: Arc::new(Mutex::new(Shared::default())),
        }
    }

    /// Spawn the dsh web server (`npx --yes @deepseek-ai/dsh web --host
    /// 127.0.0.1 --port 0`) as a child with the given `PATH`, cwd set to the
    /// user's home, and telemetry off.
    pub fn start(&self, path: &str) -> Result<(), String> {
        let args = [
            "--yes",
            "@deepseek-ai/dsh",
            "web",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
        ];
        let cwd = env_home();
        let mut command = process_impl::new_command(&args);
        command
            .current_dir(&cwd)
            .env("PATH", path)
            .env("DSH_TELEMETRY_DISABLED", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to spawn npx: {error}"))?;
        let stdout = child.stdout.take().expect("piped stdout");
        spawn_reader(BufReader::new(stdout), Arc::clone(&self.shared), false);
        let stderr = child.stderr.take().expect("piped stderr");
        spawn_reader(BufReader::new(stderr), Arc::clone(&self.shared), true);
        *self.child.lock().expect("child lock") = Some(child);
        Ok(())
    }

    /// Block until the dsh web readiness line appears, the child exits early,
    /// or `timeout` elapses. Returns the served URL.
    pub fn wait_ready(&self, timeout: Duration) -> Result<String, String> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(url) = self.shared.lock().expect("shared lock").ready_url.clone() {
                return Ok(url);
            }
            if let Some(status) = self.exit_status() {
                return Err(format!(
                    "dsh web exited early: {status}\n\n{}",
                    self.log_tail(60)
                ));
            }
            if std::time::Instant::now() >= deadline {
                return Err(format!(
                    "timed out waiting for the dsh web server after {}s\n\n{}",
                    timeout.as_secs(),
                    self.log_tail(60)
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Recycle the child process tree. Idempotent.
    pub fn stop(&self) {
        let mut guard = self.child.lock().expect("child lock");
        process_impl::stop_child(&mut guard);
    }

    /// Last `n` captured output lines, in capture order.
    pub fn log_tail(&self, n: usize) -> String {
        let shared = self.shared.lock().expect("shared lock");
        let skip = shared.lines.len().saturating_sub(n);
        shared.lines[skip..].join("\n")
    }

    /// Snapshot for the `server_info` command.
    pub fn status(&self) -> ServerStatus {
        let shared = self.shared.lock().expect("shared lock");
        let mut child = self.child.lock().expect("child lock");
        ServerStatus {
            url: shared.ready_url.clone(),
            pid: child.as_ref().map(Child::id),
            running: child
                .as_mut()
                .map(|child| child.try_wait().ok().flatten().is_none())
                .unwrap_or(false),
            lines: shared.lines.clone(),
        }
    }

    fn exit_status(&self) -> Option<std::process::ExitStatus> {
        self.child
            .lock()
            .expect("child lock")
            .as_mut()?
            .try_wait()
            .ok()
            .flatten()
    }
}

/// The user's home directory, used as the child's working directory.
fn env_home() -> String {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").unwrap_or_else(|_| "/".to_string())
    }
}

/// Pipe one child stream into the shared log, tagging stderr lines. A read
/// error or EOF (child exited) ends the thread.
fn spawn_reader<R: BufRead + Send + 'static>(reader: R, shared: Arc<Mutex<Shared>>, is_stderr: bool) {
    std::thread::spawn(move || {
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let mut shared = shared.lock().expect("shared lock");
            // Parse the URL from the raw line *before* moving its value into the
            // prefixed form, since extract_url borrows the original string.
            if !is_stderr {
                if let Some(url) = extract_url(&line) {
                    shared.ready_url = Some(url);
                }
            }
            let prefixed = if is_stderr {
                format!("[stderr] {line}")
            } else {
                line
            };
            shared.lines.push(prefixed);
            if shared.lines.len() > LOG_CAPACITY {
                let excess = shared.lines.len() - LOG_CAPACITY;
                shared.lines.drain(..excess);
            }
        }
    });
}

/// Parse the readiness URL out of one stdout line, e.g.
/// `dsh web: http://127.0.0.1:51234` (an optional LAN suffix may follow).
fn extract_url(line: &str) -> Option<String> {
    let start = line.find(READY_PREFIX)?;
    let rest = &line[start + READY_PREFIX.len()..];
    let url: String = rest.chars().take_while(|ch| !ch.is_whitespace()).collect();
    if url.is_empty() {
        None
    } else {
        Some(format!("http://{url}"))
    }
}
