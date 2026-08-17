//! Tauri entry: create the main window, spawn the dsh web sidecar, navigate to
//! its URL once ready, and recycle the child process when the app exits.

mod launcher;
mod node_path;
#[cfg(unix)]
mod process_impl_unix;
#[cfg(windows)]
mod process_impl_windows;

use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, RunEvent, WindowEvent};

use launcher::Launcher;

/// How long to wait for the dsh web server to become ready. The first run also
/// downloads `@deepseek-ai/dsh` through npx, which can take a while.
const READY_TIMEOUT: Duration = Duration::from_secs(180);

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let launcher = Arc::new(Launcher::new());
            app.manage(launcher.clone());
            if let Err(error) = launcher.start(&node_path::candidate_path_string()) {
                status(app, &format!("启动失败：{error}\n\n请确认已安装 Node.js ≥ 22（含 npx）。"), true);
                return Ok(());
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval(
                    "window.__dshStatus && window.__dshStatus('正在启动 dsh 服务（首次运行需通过 npx 下载，请稍候…）')",
                );
                tauri::async_runtime::spawn(async move {
                    match launcher.wait_ready(READY_TIMEOUT) {
                        Ok(url) => match url.parse() {
                            Ok(parsed) => {
                                let _ = window.navigate(parsed);
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                            Err(_) => {
                                let _ = window.eval(&format!(
                                    "window.__dshStatus && window.__dshStatus('启动失败：无法解析服务地址 {}', true)",
                                    escape_js(&url)
                                ));
                            }
                        },
                        Err(error) => {
                            let message = format!("启动失败：{error}");
                            let _ = window.eval(&format!(
                                "window.__dshStatus && window.__dshStatus('{}', true)",
                                escape_js(&message)
                            ));
                        }
                    }
                });
            }
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

/// Surface a message on the loading page through the `__dshStatus` hook.
fn status(app: &tauri::App, message: &str, is_error: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval(&format!(
            "window.__dshStatus && window.__dshStatus('{}', {})",
            escape_js(message),
            is_error
        ));
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
