# Agent Note: macOS 桌面 Tauri 启动器

Status: implemented

[English](2026-08-17-desktop-macos-tauri-launcher.md) | 中文

## 问题

开发者希望不用每次打开终端输入 `npx @deepseek-ai/dsh web` 就能打开 DeepSeek Harness 的 Web 界面。web profile 是为从检出目录或已发布的 npm 包运行而设计的，而不是为打包设计：把完整产品闭包打成单文件可执行（Python 运行时用的 `pkg --sea` 路线）约 300-450 MB，而且无论怎样 GUI 都要在 Node 服务之上额外加一个 WebView 进程。开发者便捷启动器不需要重新分发运行时。

## 决策

在 `desktop/` 发布一个轻量 Tauri 2 启动器，包装已安装的 dsh web CLI，并保持完整的 sidecar 生命周期：

- 启动时 Rust 核心探测常见 Node 位置（`/opt/homebrew/bin`、`/usr/local/bin`、nvm、volta、fnm、mise、`~/.local/bin`）并重建候选 `PATH`，因为从 Finder 启动的 GUI 应用只继承最简 `PATH`。
- 它在独立进程组中以 `npx --yes @deepseek-ai/dsh web --host 127.0.0.1 --port 0` 启动子进程，`cwd` 设为 `$HOME`，并设置 `DSH_TELEMETRY_DISABLED=1`。不捆绑任何东西；机器上的 Node ≥ 22.19 且带 `npx` 即提供运行时。`--port 0` 让操作系统分配空闲端口。
- GUI 是 WKWebView 窗口，先显示加载页，再导航到从 CLI 就绪行（`dsh web: http://127.0.0.1:<port>`，web profile 默认通过 `printUrl` 打印）解析出的地址。
- 关窗或退出时，启动器向进程组发送 `SIGTERM`，5 秒后升级为 `SIGKILL`，然后忘记子进程。进程退出后端口自动释放。
- 更新跟随 npx：每次启动都使用当前发布的 `@deepseek-ai/dsh`。

全打包路线仍是面向非技术用户的独立未来分发形态；本决策有意不把它用于开发者启动器。

## 备选方案

**用 `pkg --sea` 全打包运行时（约 300-450 MB 的应用，零运行时依赖）。** 对启动器而言被否决：磁盘与内存成本对已装有 Node 的开发者不合适，npx 免费提供自动更新，且无论怎样 GUI 都要多一个 WebView 进程。全打包路线保留为独立分发形态。

**Shell 别名或 `.command` 文件。** 被否决：两者仍然需要一个终端窗口并要手动配置，而这正是本启动器要去掉的摩擦。

**通过用户登录 shell 启动子进程以继承 `PATH`。** 作为主机制被否决：nvm 等在 `.zshrc` 里配置 `PATH`，而非交互式登录 shell 不会 source 它，强制交互式 shell 又不可靠。显式 `PATH` 探测是确定且可测的。

## 后果

**所得**：双击启动、无终端窗口，约 50-100 MB 且不捆绑运行时的应用，通过 npx 自动更新，以及干净的「启动 → 就绪行 → 回收」生命周期，绝不泄漏端口。

**所付**：应用要求机器装有 Node ≥ 22.19 且带 `npx`，首次启动需联网（npx 下载）；应用是 ad-hoc 签名，Gatekeeper 首次需要「右键 → 打开」；GUI 启动的进程不继承 shell 环境变量，所以 `DEEPSEEK_API_KEY` 必须来自 `~/.dsh/.env` 或凭据文件；启动器始终运行已发布的 dsh 版本，可能与本机检出版本存在漂移。

## 测试

CI 流水线（`.github/workflows/desktop-macos.yml`）在 `macos-15`（Apple Silicon）上构建应用，并先对启动器的子命令做冒烟测试：启动 `npx --yes @deepseek-ai/dsh web --port 0`，抓取就绪行，curl 通服务地址，并打印服务进程 RSS。窗口化应用本身不在 CI 自动化，由用户在本机启动确认。
