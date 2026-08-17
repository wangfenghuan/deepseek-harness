# DeepSeek Harness 桌面版（macOS）

[English](README.md) | 中文

一个轻量 Tauri 2 启动器，用原生 macOS 窗口打开 DeepSeek Harness 的 Web 界面。它在子进程拉起 `npx @deepseek-ai/dsh web`，用 WebView 加载服务地址，关闭窗口时回收子进程（及其端口）。

## 这是什么

启动器包装的是本机已安装的 dsh CLI——**不**捆绑 Node 或 harness 包。机器上需要 Node.js ≥ 22.19 且带 `npx`，所以这个形态是开发者便捷工具，而不是面向终端用户的分发形式。

## 工作原理

1. Rust 核心探测常见 Node 位置（`/opt/homebrew/bin`、`/usr/local/bin`、nvm、volta、fnm、mise、`~/.local/bin`）并重建 `PATH`，因为从 Finder 启动的 GUI 应用只继承最简 `PATH`。
2. 它在独立进程组中启动 `npx --yes @deepseek-ai/dsh web --host 127.0.0.1 --port 0`，`cwd` 设为 `$HOME`，并设置 `DSH_TELEMETRY_DISABLED=1`。`--port 0` 让操作系统分配空闲端口。
3. CLI 会打印就绪行 `dsh web: http://127.0.0.1:<port>`；启动器解析它并把窗口导航到该地址。
4. 关窗或退出时，启动器向进程组发送 `SIGTERM`，5 秒后升级为 `SIGKILL`，然后忘记子进程。进程退出后端口自动释放。

## 环境要求

- macOS 11+（Apple Silicon；CI 流水线构建 `aarch64-apple-darwin`）。
- 已安装 Node.js ≥ 22.19，且带 `npx`。

## 构建

### GitHub Actions（CI）

[`.github/workflows/desktop-macos.yml`](../.github/workflows/desktop-macos.yml) 在 `macos-15`（Apple Silicon）上构建 `DeepSeek Harness.app` 与 `.dmg`：每次推送到默认分支以及打 `v*` 标签时触发，上传产物；打标签时还会创建含 `.dmg` 的 GitHub Release。

### 本地构建

```sh
cd desktop
npx --yes @resvg/resvg-js assets/app-icon.svg assets/app-icon-1024.png   # 仅首次
npx --yes @tauri-apps/cli@2 icon assets/app-icon-1024.png                # 生成 src-tauri/icons
TAURI_SIGNING_IDENTITY=- npx --yes @tauri-apps/cli@2 build
open src-tauri/target/release/bundle/macos/*.app
```

## 首次启动

- 应用是 ad-hoc 签名；Gatekeeper 可能要求第一次「右键 → 打开」。
- 首次启动会通过 npx 下载 `@deepseek-ai/dsh`，可能需要一两分钟。
- Web 界面在没有 API key 时也能打开；在 `~/.dsh/.env`（或凭据文件）中加入 `DEEPSEEK_API_KEY` 即可使用 agent 会话。

## 配置

- `DSH_HOME` 可覆盖默认的 `~/.dsh` 用户数据根目录（profile、会话、凭据）。
- 启动器会设置 `DSH_TELEMETRY_DISABLED=1`（遥测保持本地）。

## 体积

- `.app`：约 50–100 MB（不捆绑运行时）。
- 运行时：约 300–700 MB（Node dsh 服务加 WebView），CI 冒烟步骤会实测并打印。

## 故障排查

- 报 `failed to spawn npx` 说明在探测路径里没找到 Node；安装 Node，或让它出现在 `/opt/homebrew/bin`、`/usr/local/bin`。
- 就绪超时会在错误页展示捕获到的服务输出，通常是 dsh web profile 启动失败（查看 `~/.dsh` 日志）或首次 npx 下载仍在进行。
