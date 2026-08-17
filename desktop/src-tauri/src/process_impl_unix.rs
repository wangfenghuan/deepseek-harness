//! Platform-specific process lifecycle for Unix-like systems (macOS, Linux).
//!
//! - The child is spawned in its own process group so one signal recycles
//!   npx, dsh, and any agents dsh itself spawns.
//! - Shutdown sends SIGTERM first, then SIGKILL after the grace period.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

const STOP_GRACE: Duration = Duration::from_secs(5);

/// Build the base child command: `npx <args>` in its own process group.
pub fn new_command(args: &[&str]) -> Command {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("npx");
    command.args(args).process_group(0);
    command
}

/// Stop the child and its whole process group. Idempotent.
pub fn stop_child(child: &mut Option<Child>) {
    let Some(child) = child.as_mut() else {
        return;
    };
    if child.try_wait().ok().flatten().is_some() {
        *child = None;
        return;
    }
    let pid = child.id() as i32;
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let deadline = Instant::now() + STOP_GRACE;
    loop {
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    *child = None;
}
