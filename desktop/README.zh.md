# DeepSeek Harness 桌面版

[English](README.md) | 中文

一个轻量 Tauri 2 启动器，用原生窗口（macOS 与 Windows）打开 DeepSeek Harness 的 Web 界面。它在子进程拉起 `npx @deepseek-ai/dsh web`，用 WebView 加载服务地址，关闭窗口时回收子进程（及其端口）。

## 这是什么

启动器包装的是本机已安装的 dsh CLI——**不**捆绑 Node 或 harness 包。机器上需要 Node.js ≥ 22.19 且带 `npx`，所以这个形态是开发者便捷工具，而不是面向终端用户的分发形式。

## 工作原理

1. Rust 核心重建包含常见 Node 位置的 `PATH`（macOS：`/opt/homebrew/bin`、`/usr/local/bin`、nvm、volta、fnm、mise、`~/.local/bin`），因为从 Finder 启动的 GUI 应用只继承最简 `PATH`；Windows GUI 应用继承完整用户 `PATH`，直接使用。
2. 它启动子进程 `npx --yes @deepseek-ai/dsh@latest web --host 127.0.0.1 --port 0`（Windows 上经 `cmd /C`），`cwd` 设为用户主目录，并设置 `DSH_TELEMETRY_DISABLED=1`。`--port 0` 让操作系统分配空闲端口。自动更新开启（默认）时 npx 解析 `latest` 标签，有新版本就安装；关闭时用不带版本的 `@deepseek-ai/dsh`，npx 直接复用缓存（见[设置](#设置)）。
3. CLI 会打印就绪行 `dsh web: http://127.0.0.1:<port>`；启动器解析它并把窗口导航到该地址。
4. 关窗或退出时，启动器回收整个子进程树（macOS：向进程组发送 `SIGTERM`，5 秒后升级为 `SIGKILL`；Windows：`taskkill /T /F`）。进程退出后端口自动释放。
5. 同一时间只运行一个实例：再次启动应用（再次双击 `.app`/`.exe`）只会把已有窗口带到前台，不会另起一个进程、也不会再起一个 npx 侧车。

## 环境要求

- macOS 11+（Apple Silicon；CI 流水线构建 `aarch64-apple-darwin`）或 Windows 10/11 x64（WebView2 运行时，Windows 11 自带）。
- 已安装 Node.js ≥ 22.19，且带 `npx`。

## 构建

### GitHub Actions（CI）

- [`.github/workflows/desktop-macos.yml`](../.github/workflows/desktop-macos.yml) 在 `macos-15`（Apple Silicon）上构建 `DeepSeek Harness.app` 与 `.dmg`。
- [`.github/workflows/desktop-windows.yml`](../.github/workflows/desktop-windows.yml) 在 `windows-latest`（x64）上构建 NSIS 安装器（`.exe`）与 MSI（`.msi`）。

两者都在每次推送到默认分支以及打 `v*` 标签时触发，上传产物；打标签时还会发布含安装包的 GitHub Release。

### 本地构建

图标直接提交在仓库里（`assets/app-icns.icns`、`assets/app-ico.ico`，以及 macOS 菜单栏用的 `assets/deepseek-wite.png` / `assets/deepseek-black.png`），不需要图标生成步骤。Tauri 构建时仍需要一个 PNG 图标（`src-tauri/icons/icon.png`），CI 流水线会从 `.icns` / `.ico` 提取；本地请按 CI 同样的方式提取（macOS：`iconutil -c iconset app.icns` 后复制最大的 `icon_*.png`；Windows：用 System.Drawing 解码 `.ico`），然后构建：

```sh
cd desktop
TAURI_SIGNING_IDENTITY=- npx --yes @tauri-apps/cli@2 build
open src-tauri/target/release/bundle/macos/*.app
```

Windows 上同样的命令会生成 `src-tauri\target\release\bundle\msi\*.msi` 与 `src-tauri\target\release\bundle\nsis\*.exe`。

## 首次启动

- macOS：应用是 ad-hoc 签名；Gatekeeper 可能要求第一次「右键 → 打开」。
- 首次启动会通过 npx 下载 `@deepseek-ai/dsh`，可能需要一两分钟。
- Web 界面在没有 API key 时也能打开；在 `~/.dsh/.env`（或凭据文件）中加入 `DEEPSEEK_API_KEY` 即可使用 agent 会话。

## 配置

- `DSH_HOME` 可覆盖默认的 `~/.dsh` 用户数据根目录（profile、会话、凭据）。
- 启动器会设置 `DSH_TELEMETRY_DISABLED=1`（遥测保持本地）。
- macOS 上菜单栏（托盘）图标跟随系统外观：深色模式用浅色 logo、浅色模式用深色 logo（`assets/deepseek-wite.png` / `assets/deepseek-black.png`），系统主题变化时实时更新。

## 设置

从托盘菜单（「设置…」）或启动页右上角的齿轮按钮打开设置窗口。设置持久化到应用配置目录下的 `settings.json`。

- **主题** — 跟随系统 / 深色 / 浅色。作用于启动器自己的窗口（启动页与设置窗口）；dsh web 界面保持它自己的主题。
- **自动更新** — 开启（默认）时每次启动都通过 npx 解析 `@deepseek-ai/dsh@latest`，有新版本就安装；关闭时 npx 使用缓存版本。改动在下次启动时生效。
- **运行日志** — 设置窗口展示捕获到的 dsh 侧车输出（自动刷新的最近日志），带刷新与复制按钮，方便排障。

## 体积

- macOS `.app`：约 50–100 MB（不捆绑运行时）。
- 运行时：约 300–700 MB（Node dsh 服务加 WebView）。

## 故障排查

- 报 `failed to spawn npx` 说明在探测路径里没找到 Node；安装 Node，或让它出现在 `/opt/homebrew/bin`、`/usr/local/bin`（macOS）或用户 `PATH`（Windows）。
- 就绪超时会在错误页展示捕获到的服务输出，通常是 dsh web profile 启动失败（查看 `~/.dsh` 日志）或首次 npx 下载仍在进行。
