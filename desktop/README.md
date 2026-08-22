# DeepSeek Harness Desktop

English | [中文](README.zh.md)

A lightweight Tauri 2 launcher that opens the DeepSeek Harness web GUI in a native window (macOS and Windows). It spawns `npx @deepseek-ai/dsh web` as a child process, loads the served URL in a WebView, and recycles the child process (and its port) when the window closes.

## What this is

The launcher wraps the already-installed dsh CLI — it does **not** bundle Node or the harness packages. You need Node.js ≥ 22.19 with `npx` on the machine, which is why this target is a developer convenience rather than an end-user distribution form.

## How it works

1. The Rust core rebuilds a `PATH` that includes common Node locations (macOS: `/opt/homebrew/bin`, `/usr/local/bin`, nvm, volta, fnm, mise, `~/.local/bin`), because Finder-launched GUI apps inherit only a minimal `PATH`; Windows GUI apps inherit the full user `PATH` and are used as-is.
2. It spawns the official `npx @deepseek-ai/dsh web --no-open` as a child (`cmd /C` on Windows) with `cwd` set to the user's home, `DSH_TELEMETRY_DISABLED=1`, and stdin closed (so npx never blocks on an install prompt). `--no-open` stops `dsh web` from opening the system browser — the launcher shows the UI in its own window instead. npx runs the cached version when one is present and downloads the latest only when the cache is empty; `dsh web` binds to `127.0.0.1` on its default port.
3. The CLI prints a readiness line `dsh web: http://127.0.0.1:<port>`; the launcher parses it and navigates the window to that URL.
4. On window close or app exit the launcher recycles the child process tree (macOS: `SIGTERM` to the process group, `SIGKILL` after 5 seconds; Windows: `taskkill /T /F`). The port is released when the process dies.
5. Only one app instance runs at a time: launching the app again (double-clicking the `.app`/`.exe` a second time) focuses the existing window instead of spawning another process and another npx sidecar.

## Requirements

- macOS 11+ on both Apple Silicon (arm64) and Intel (x86_64); the CI workflow builds both. Or Windows 10/11 x64 (WebView2 runtime, built in on Windows 11).
- Node.js ≥ 22.19 with `npx` available.

## Build

### GitHub Actions (CI)

- [`.github/workflows/desktop-macos.yml`](../.github/workflows/desktop-macos.yml) builds `DeepSeek Harness.app` and the `.dmg` for both Apple Silicon (arm64) and Intel (x86_64) on `macos-15` (cross-compiling the Intel target), plus a `.zip` of each `.app`.
- [`.github/workflows/desktop-windows.yml`](../.github/workflows/desktop-windows.yml) builds the NSIS installer (`.exe`) and MSI (`.msi`) on `windows-latest` (x64).

Both run on every push to the default branch and on `v*` tags, upload their artifacts, and publish a GitHub Release with the installers for tags.

### Locally

The icons are committed directly (`assets/app-icns.icns`, `assets/app-ico.ico`, plus
`assets/deepseek-wite.png` / `assets/deepseek-black.png` for the macOS menu bar), so
no icon-generation step is needed. Tauri still requires one PNG icon at build time
(`src-tauri/icons/icon.png`), which the CI workflows produce from the `.icns` / `.ico`;
locally, extract it the same way the CI does (macOS: `iconutil -c iconset app.icns`
then copy the largest `icon_*.png`; Windows: decode the `.ico` via System.Drawing),
then build:

```sh
cd desktop
TAURI_SIGNING_IDENTITY=- npx --yes @tauri-apps/cli@2 build
open src-tauri/target/release/bundle/macos/*.app
```

On Windows the same commands produce `src-tauri\target\release\bundle\msi\*.msi` and `src-tauri\target\release\bundle\nsis\*.exe`.

## First launch

- macOS: the app is ad-hoc signed; Gatekeeper may require right-click → Open the first time.
- The first launch downloads `@deepseek-ai/dsh` through npx, which can take a couple of minutes.
- The web GUI starts without an API key; add `DEEPSEEK_API_KEY` to `~/.dsh/.env` (or a credentials file) to enable agent sessions.

## Configuration

- `DSH_HOME` overrides the default `~/.dsh` user-data root (profiles, sessions, credentials).
- The launcher sets `DSH_TELEMETRY_DISABLED=1` (telemetry stays local).
- On macOS the menu-bar (tray) icon follows the system appearance: the light logo in dark mode and the dark logo in light mode (`assets/deepseek-wite.png` / `assets/deepseek-black.png`), and it updates live when the system theme changes.

## Settings

Open the settings window from the tray menu ("设置…") or the gear button on the launch page. Settings persist to `settings.json` in the app config directory.

- **Theme** — 跟随系统 / 深色 / 浅色 (follow system / dark / light). Applies to the launcher's own windows (the splash page and the settings window); the dsh web app keeps its own theme.
- **运行日志** — the settings window shows the captured dsh sidecar output (the latest lines, refreshed automatically) with refresh/copy buttons, for troubleshooting.

## Sizes

- macOS `.app`: about 50–100 MB (no bundled runtime).
- Runtime: about 300–700 MB (Node dsh server plus the WebView).

## Troubleshooting

- A `failed to spawn npx` error means Node was not found on the probed paths; install Node or make it visible in `/opt/homebrew/bin` or `/usr/local/bin` (macOS) or on the user `PATH` (Windows).
- The launcher waits as long as needed for the server (the first npx download can take a while); if the dsh web process exits early, the error page shows the captured output, which usually means the dsh web profile failed to boot (check the `~/.dsh` logs).
