//! Tauri entry: create the main window, spawn the dsh web sidecar, navigate to
//! its URL once ready, and recycle the child process when the app exits.

mod launcher;
mod node_bootstrap;
mod node_path;
mod settings;
#[cfg(unix)]
mod process_impl_unix;
#[cfg(windows)]
mod process_impl_windows;
#[cfg(target_os = "macos")]
mod tray_icon;

use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WebviewWindow, WindowEvent,
};
#[cfg(target_os = "macos")]
use tauri::Theme;

use launcher::Launcher;
use settings::{Settings, SettingsStore};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch focuses the existing main window instead of
            // spawning another instance (and another npx sidecar).
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            let launcher = Arc::new(Launcher::new());
            app.manage(launcher.clone());
            app.manage(SettingsStore::load_or_default(&app.handle()));

            // ---- System tray icon with menu ----
            let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "退出 DeepSeek Harness", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &settings_item, &quit_item])?;

            // macOS: the menu-bar icon must stay legible on the system menu
            // bar, which follows the system appearance (dark bar → light
            // icon). Windows keeps the default window icon in the tray.
            let tray_icon = {
                #[cfg(target_os = "macos")]
                {
                    let theme = app
                        .get_webview_window("main")
                        .and_then(|window| window.theme().ok())
                        .unwrap_or(Theme::Light);
                    tray_icon::icon_for_theme(theme)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    app.default_window_icon().unwrap().clone()
                }
            };

            let _tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("DeepSeek Harness")
                .icon(tray_icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "settings" => {
                        let _ = open_settings_window(app.clone());
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // Left-click toggles window visibility.
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            match w.is_visible() {
                                Ok(true) => {
                                    let _ = w.hide();
                                }
                                Ok(false) => {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                                _ => {}
                            }
                        }
                    }
                })
                .build(app)?;

            let window = app.get_webview_window("main");

            // Relay every new child log line to the loading page by calling
            // the `__dshAppendLog` JS hook via window.eval. Rust→JS eval is
            // used here (no withGlobalTauri needed for the log stream), while
            // the settings pages use invoke + events through `withGlobalTauri`.
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
                // No timeout: the first launch downloads the dsh build through
                // npx and can take a while; a dead child reports immediately.
                match launcher.wait_ready() {
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
        .invoke_handler(tauri::generate_handler![
            server_info,
            open_in_browser,
            open_url,
            get_settings,
            set_settings,
            open_settings_window,
            show_main_window
        ])
        .on_window_event(|window, event| {
            match event {
                // Closing any launcher window hides it instead of quitting —
                // the app keeps running in the background with a system tray
                // icon so the dsh sidecar stays alive. Use the tray menu
                // "Quit" or Cmd/Ctrl+Q to fully exit and recycle the sidecar.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                #[cfg(target_os = "macos")]
                WindowEvent::ThemeChanged(theme) => {
                    // Keep the menu-bar icon legible as the system appearance
                    // changes (dark menu bar → light icon, light → dark icon).
                    if let Some(tray) = window.app_handle().tray_by_id("main-tray") {
                        let _ = tray.set_icon(Some(tray_icon::icon_for_theme(*theme)));
                    }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                RunEvent::Exit => {
                    if let Some(launcher) = app_handle.try_state::<Arc<Launcher>>() {
                        launcher.stop();
                    }
                }
                #[cfg(target_os = "macos")]
                RunEvent::Reopen { .. } => {
                    // Clicking the Dock icon shows the window (macOS convention).
                    if let Some(w) = app_handle.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                _ => {}
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

/// Open a URL in the system default browser. Tauri webviews cannot
/// `window.open` external URLs, so links go through this command.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .status();
    #[cfg(not(windows))]
    let status = std::process::Command::new("open").arg(&url).status();
    status.map(|_| ()).map_err(|error| error.to_string())
}

/// Bring the main window to the foreground.
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

/// Current persisted settings.
#[tauri::command]
fn get_settings(store: tauri::State<'_, SettingsStore>) -> Settings {
    store.get()
}

/// Persist new settings and broadcast them so every open launcher window
/// (splash + settings) updates live.
#[tauri::command]
fn set_settings(
    app: tauri::AppHandle,
    store: tauri::State<'_, SettingsStore>,
    settings: Settings,
) -> Result<(), String> {
    store.set(settings.clone())?;
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

/// Show the settings window, creating it on first use. Single instance: a
/// second call focuses the existing window.
#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(
        &app,
        "settings",
        WebviewUrl::App("settings.html".into()),
    )
    .title("设置 - DeepSeek Harness")
    .inner_size(460.0, 540.0)
    .min_inner_size(400.0, 460.0)
    .center()
    .build()
    .map_err(|error| error.to_string())?;
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}
