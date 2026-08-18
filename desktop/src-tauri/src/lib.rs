//! Tauri entry: create the main window, spawn the dsh web sidecar, navigate to
//! its URL once ready, and recycle the child process when the app exits.

mod launcher;
mod node_bootstrap;
mod node_path;
#[cfg(unix)]
mod process_impl_unix;
#[cfg(windows)]
mod process_impl_windows;

use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, RunEvent, WebviewWindow, WindowEvent};

use launcher::Launcher;

/// How long to wait for the dsh web server to become ready. The first run also
/// downloads `@deepseek-ai/dsh` through npx, which can take a while.
const READY_TIMEOUT: Duration = Duration::from_secs(180);

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let launcher = Arc::new(Launcher::new());
            app.manage(launcher.clone());

            let window = app.get_webview_window("main");

            // Relay every new child log line to the loading page by calling
            // the `__dshAppendLog` JS hook via window.eval. This avoids
            // enabling `withGlobalTauri` on the loading page.
            if let Some(w) = &window {
                let w = w.clone();
                launcher.on_line(move |line, is_stderr| {
                    append_log_window(Some(&w), line, is_stderr);
                });
            }

            let app_handle = app.handle().clone();
            let system_path = node_path::candidate_path_string();

            // Kick off the bootstrap on a background thread so the UI stays
            // responsive during the (potentially multi-second) download.
            tauri::async_runtime::spawn(async move {
                // ---- Step 1: locate (or download) a usable node binary ----
                let cache_dir = match app_handle.path().app_cache_dir() {
                    Ok(dir) => dir,
                    Err(e) => {
                        status_window(&window, &format!("启动失败：无法获取缓存目录：{e}"), true);
                        return;
                    }
                };

                status_window(&window, "正在检查 Node.js 环境…", false);
                append_log_window(window.as_ref(), "[info] 检查系统 Node.js 版本…", false);

                let node_bin = match node_bootstrap::ensure_node(
                    &system_path,
                    &cache_dir,
                    |done, total| {
                        // Progress callback — runs on the download thread.
                        if let Some(w) = &window {
                            let js = format!(
                                "window.__dshDownloadProgress && window.__dshDownloadProgress({}, {}, '正在下载 Node.js 便携版…')",
                                done, total
                            );
                            let _ = w.eval(&js);
                        }
                    },
                ) {
                    Ok(p) => {
                        append_log_window(
                            window.as_ref(),
                            &format!("[info] 使用 Node.js: {}", p.display()),
                            false,
                        );
                        p
                    }
                    Err(e) => {
                        status_window(
                            &window,
                            &format!("未找到可用的 Node.js（≥22）且自动下载失败：\n{e}\n\n请手动安装 Node.js 22 或更高版本。"),
                            true,
                        );
                        return;
                    }
                };

                // ---- Step 2: spawn dsh web using the resolved node ----
                status_window(&window, "正在启动 dsh web 服务…", false);
                append_log_window(window.as_ref(), "[info] 正在启动本地 dsh 服务…", false);

                // Extend PATH with the directory containing the resolved node
                // so npx can find it too.
                let node_dir = node_bin
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let launch_path = if node_dir.is_empty() {
                    system_path
                } else {
                    let sep = if cfg!(windows) { ';' } else { ':' };
                    format!("{node_dir}{sep}{system_path}")
                };

                if let Err(e) = launcher.start(&launch_path) {
                    status_window(
                        &window,
                        &format!("启动失败：{e}\n\n请确认已安装 Node.js ≥ 22（含 npx）。"),
                        true,
                    );
                    return;
                }

                // ---- Step 3: wait for the server then navigate ----
                match launcher.wait_ready(READY_TIMEOUT) {
                    Ok(url) => match url.parse() {
                        Ok(parsed) => {
                            if let Some(w) = &window {
                                let _ = w.navigate(parsed);
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        Err(_) => {
                            status_window(
                                &window,
                                &format!("启动失败：无法解析服务地址 {url}"),
                                true,
                            );
                        }
                    },
                    Err(error) => {
                        let message = format!("启动失败：{error}");
                        status_window(&window, &message, true);
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![server_info, open_in_browser])
        .on_window_event(|window, event| {
            // Closing the only window quits the app so the sidecar is recycled.
            if let WindowEvent::CloseRequested { .. } = event {
                window.app_handle().exit(0);
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event {
                if let Some(launcher) = app_handle.try_state::<Arc<Launcher>>() {
                    launcher.stop();
                }
            }
        });
}

/// Surface a status message on the loading page through the `__dshStatus` hook.
fn status_window(window: &Option<WebviewWindow>, message: &str, is_error: bool) {
    if let Some(w) = window {
        let _ = w.eval(&format!(
            "window.__dshStatus && window.__dshStatus('{}', {})",
            escape_js(message),
            is_error
        ));
    }
}

/// Append a log line on the loading page through the `__dshAppendLog` hook.
fn append_log_window(window: Option<&WebviewWindow>, line: &str, is_stderr: bool) {
    if let Some(w) = window {
        let js = format!(
            "window.__dshAppendLog && window.__dshAppendLog('{}', {})",
            escape_js(line),
            is_stderr
        );
        let _ = w.eval(&js);
    }
}

/// Escape a string for safe embedding inside a single-quoted JS literal.
fn escape_js(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// Current sidecar status for the UI / debugging.
#[tauri::command]
fn server_info(launcher: tauri::State<'_, Arc<Launcher>>) -> launcher::ServerStatus {
    launcher.status()
}

/// Open the served URL in the system default browser.
#[tauri::command]
fn open_in_browser(launcher: tauri::State<'_, Arc<Launcher>>) -> Result<(), String> {
    let url = launcher
        .status()
        .url
        .ok_or_else(|| "dsh web 尚未就绪".to_string())?;
    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .status();
    #[cfg(not(windows))]
    let status = std::process::Command::new("open").arg(&url).status();
    status.map(|_| ()).map_err(|error| error.to_string())
}
