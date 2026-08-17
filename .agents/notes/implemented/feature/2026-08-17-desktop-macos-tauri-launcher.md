# Agent Note: macOS desktop Tauri launcher

Status: implemented

English | [中文](2026-08-17-desktop-macos-tauri-launcher.zh.md)

## Problem

Developers want to open the DeepSeek Harness web GUI without opening a terminal and typing `npx @deepseek-ai/dsh web` every time. The web profile is designed to run from a checkout or the published npm package, not to be bundled: packing the full product closure as a single executable (the `pkg --sea` route the Python runtime uses) lands on the order of 300-450 MB, and the GUI adds a WebView process on top of the Node server regardless. A developer convenience launcher does not need to redistribute the runtime.

## Decision

Ship a lightweight Tauri 2 launcher in `desktop/` that wraps the already-installed dsh web CLI and keeps the full sidecar lifecycle:

- On launch the Rust core probes common Node locations (`/opt/homebrew/bin`, `/usr/local/bin`, nvm, volta, fnm, mise, `~/.local/bin`) and rebuilds a candidate `PATH`, because Finder-launched GUI apps inherit only a minimal `PATH`.
- It spawns `npx --yes @deepseek-ai/dsh web --host 127.0.0.1 --port 0` as a child in its own process group, with `cwd` set to `$HOME` and `DSH_TELEMETRY_DISABLED=1`. Nothing is bundled; the machine's Node ≥ 22.19 with `npx` provides the runtime. `--port 0` lets the OS assign a free port.
- The GUI is a WKWebView window that shows a loading page, then navigates to the URL read from the CLI's readiness line (`dsh web: http://127.0.0.1:<port>`), which the web profile prints by default (`printUrl`).
- On window close or app exit the launcher sends `SIGTERM` to the process group, escalates to `SIGKILL` after 5 seconds, and forgets the child. The port is released when the process dies.
- Updates ride npx: each launch uses the current published `@deepseek-ai/dsh`.

The full-bundling route remains a separate future distribution form for non-technical users; this decision deliberately does not take it for the developer launcher.

## Alternatives considered

**Bundle the whole runtime with `pkg --sea` (a ~300-450 MB app, zero runtime dependency).** Rejected for this launcher: the disk and memory cost is wrong for a developer who already has Node, npx provides automatic updates for free, and the GUI adds a WebView process either way. The bundled route stays available as a separate distribution form.

**A shell alias or a `.command` file.** Rejected: both still require a terminal window and manual setup, which is exactly the friction this launcher removes.

**Launch the child through the user's login shell to inherit `PATH`.** Rejected as the primary mechanism: nvm and friends configure `PATH` in `.zshrc`, which a non-interactive login shell does not source, and forcing an interactive shell is unreliable. Explicit `PATH` probing is deterministic and testable.

## Consequences

**Bought**: double-click launch with no terminal window, a ~50-100 MB app that bundles no runtime, automatic updates through npx, and a clean spawn → readiness-line → recycle lifecycle that never leaks a port.

**Paid**: the app requires Node ≥ 22.19 with `npx` on the machine and a network connection on first launch (npx download); the app is ad-hoc signed, so Gatekeeper needs right-click → Open once; GUI-launched processes do not inherit shell environment variables, so `DEEPSEEK_API_KEY` must come from `~/.dsh/.env` or the credentials file; the launcher always runs the published dsh version, which can drift from a local checkout.

## Testing

The CI workflow (`.github/workflows/desktop-macos.yml`) builds the app on `macos-15` (Apple Silicon) and first smoke-tests the exact child command: it starts `npx --yes @deepseek-ai/dsh web --port 0`, greps the readiness line, curls the served URL, and reports the server process RSS. The windowed app itself is not automated in CI; the user launches it locally to confirm.
