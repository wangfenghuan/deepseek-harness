# DeepSeek Harness Desktop (macOS)

English | [中文](README.zh.md)

A lightweight Tauri 2 launcher that opens the DeepSeek Harness web GUI in a native macOS window. It spawns `npx @deepseek-ai/dsh web` as a child process, loads the served URL in a WebView, and recycles the child process (and its port) when the window closes.

## What this is

The launcher wraps the already-installed dsh CLI — it does **not** bundle Node or the harness packages. You need Node.js ≥ 22.19 with `npx` on the machine, which is why this target is a developer convenience rather than an end-user distribution form.

## How it works

1. The Rust core probes common Node locations (`/opt/homebrew/bin`, `/usr/local/bin`, nvm, volta, fnm, mise, `~/.local/bin`) and rebuilds a `PATH`, because Finder-launched GUI apps inherit only a minimal `PATH`.
2. It spawns `npx --yes @deepseek-ai/dsh web --host 127.0.0.1 --port 0` in its own process group, with `cwd` set to `$HOME` and `DSH_TELEMETRY_DISABLED=1`. `--port 0` lets the OS assign a free port.
3. The CLI prints a readiness line `dsh web: http://127.0.0.1:<port>`; the launcher parses it and navigates the window to that URL.
4. On window close or app exit the launcher sends `SIGTERM` to the process group, escalates to `SIGKILL` after 5 seconds, and forgets the child. The port is released when the process dies.

## Requirements

- macOS 11+ (Apple Silicon; the CI workflow builds `aarch64-apple-darwin`).
- Node.js ≥ 22.19 with `npx` available.

## Build

### GitHub Actions (CI)

[`.github/workflows/desktop-macos.yml`](../.github/workflows/desktop-macos.yml) builds `DeepSeek Harness.app` and the `.dmg` on `macos-15` (Apple Silicon) on every push to the default branch and on `v*` tags, uploads the artifacts, and creates a GitHub Release with the `.dmg` for tags. Before the Tauri build it smoke-tests the exact child command: it starts `npx --yes @deepseek-ai/dsh web --port 0`, greps the readiness line, and curls the URL.

### Locally

```sh
cd desktop
npx --yes @resvg/resvg-js assets/app-icon.svg assets/app-icon-1024.png   # first time only
npx --yes @tauri-apps/cli@2 icon assets/app-icon-1024.png                # generates src-tauri/icons
TAURI_SIGNING_IDENTITY=- npx --yes @tauri-apps/cli@2 build
open src-tauri/target/release/bundle/macos/*.app
```

## First launch

- The app is ad-hoc signed; Gatekeeper may require right-click → Open the first time.
- The first launch downloads `@deepseek-ai/dsh` through npx, which can take a couple of minutes.
- The web GUI starts without an API key; add `DEEPSEEK_API_KEY` to `~/.dsh/.env` (or a credentials file) to enable agent sessions.

## Configuration

- `DSH_HOME` overrides the default `~/.dsh` user-data root (profiles, sessions, credentials).
- The launcher sets `DSH_TELEMETRY_DISABLED=1` (telemetry stays local).

## Sizes

- `.app`: about 50–100 MB (no bundled runtime).
- Runtime: about 300–700 MB (Node dsh server plus the WebView), measured and printed in the CI smoke step.

## Troubleshooting

- A `failed to spawn npx` error means Node was not found on the probed paths; install Node or make it visible in `/opt/homebrew/bin` or `/usr/local/bin`.
- A readiness timeout shows the captured server output on the error page; it usually means the dsh web profile failed to boot (check the `~/.dsh` logs) or the first npx download is still running.
