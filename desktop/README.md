# DeepSeek Harness Desktop

English | [中文](README.zh.md)

A lightweight Tauri 2 launcher that opens the DeepSeek Harness web GUI in a native window (macOS and Windows). It spawns `npx @deepseek-ai/dsh web` as a child process, loads the served URL in a WebView, and recycles the child process (and its port) when the window closes.

## What this is

The launcher wraps the already-installed dsh CLI — it does **not** bundle Node or the harness packages. You need Node.js ≥ 22.19 with `npx` on the machine, which is why this target is a developer convenience rather than an end-user distribution form.

## How it works

1. The Rust core rebuilds a `PATH` that includes common Node locations (macOS: `/opt/homebrew/bin`, `/usr/local/bin`, nvm, volta, fnm, mise, `~/.local/bin`), because Finder-launched GUI apps inherit only a minimal `PATH`; Windows GUI apps inherit the full user `PATH` and are used as-is.
2. It spawns `npx --yes @deepseek-ai/dsh web --host 127.0.0.1 --port 0` as a child (`cmd /C` on Windows), with `cwd` set to the user's home and `DSH_TELEMETRY_DISABLED=1`. `--port 0` lets the OS assign a free port.
3. The CLI prints a readiness line `dsh web: http://127.0.0.1:<port>`; the launcher parses it and navigates the window to that URL.
4. On window close or app exit the launcher recycles the child process tree (macOS: `SIGTERM` to the process group, `SIGKILL` after 5 seconds; Windows: `taskkill /T /F`). The port is released when the process dies.

## Requirements

- macOS 11+ (Apple Silicon; the CI workflow builds `aarch64-apple-darwin`) or Windows 10/11 x64 (WebView2 runtime, built in on Windows 11).
- Node.js ≥ 22.19 with `npx` available.

## Build

### GitHub Actions (CI)

- [`.github/workflows/desktop-macos.yml`](../.github/workflows/desktop-macos.yml) builds `DeepSeek Harness.app` and the `.dmg` on `macos-15` (Apple Silicon).
- [`.github/workflows/desktop-windows.yml`](../.github/workflows/desktop-windows.yml) builds the NSIS installer (`.exe`) and MSI (`.msi`) on `windows-latest` (x64).

Both run on every push to the default branch and on `v*` tags, upload their artifacts, and publish a GitHub Release with the installers for tags.

### Locally

```sh
cd desktop
npx --yes @resvg/resvg-js assets/app-icon.svg assets/app-icon-1024.png   # first time only
npx --yes @tauri-apps/cli@2 icon assets/app-icon-1024.png                # generates src-tauri/icons
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

## Sizes

- macOS `.app`: about 50–100 MB (no bundled runtime).
- Runtime: about 300–700 MB (Node dsh server plus the WebView).

## Troubleshooting

- A `failed to spawn npx` error means Node was not found on the probed paths; install Node or make it visible in `/opt/homebrew/bin` or `/usr/local/bin` (macOS) or on the user `PATH` (Windows).
- A readiness timeout shows the captured server output on the error page; it usually means the dsh web profile failed to boot (check the `~/.dsh` logs) or the first npx download is still running.
