# codex-provider-bridge

给 Codex 使用的本地 OpenAI 兼容 Provider 桥接工具。它让 Codex 继续保持 ChatGPT 登录模式，同时把模型请求转发到你自己的 OpenAI 兼容服务。

```text
Codex App -> http://127.0.0.1:11435/v1 -> 你的 OpenAI 兼容 /v1 地址
```

## 安装

从 GitHub Release 下载对应平台二进制，把文件放到 PATH 中，并重命名为 `codex-provider-bridge`。

- Windows: `codex-provider-bridge-windows-x86_64.exe`
- Linux: `codex-provider-bridge-linux-x86_64`

Windows PowerShell 示例：

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\bin"
Move-Item .\codex-provider-bridge-windows-x86_64.exe "$env:USERPROFILE\bin\codex-provider-bridge.exe"
$env:PATH="$env:USERPROFILE\bin;$env:PATH"
codex-provider-bridge setup
codex-provider-bridge start
```

Linux 示例：

```bash
chmod +x ./codex-provider-bridge-*
mv ./codex-provider-bridge-* ~/.local/bin/codex-provider-bridge
codex-provider-bridge setup
codex-provider-bridge start
```

## 使用

`setup` 会询问：

- 上游 OpenAI 兼容地址，例如 `https://api.example.com/v1`
- 本地端口，默认 `11435`
- API Key，默认保存到 `~/.codex-provider-bridge/config.json`

安装向导会备份并修改 `~/.codex/config.toml`，把 Codex 的 `model_provider` 切到本地桥接 provider。启动后台服务后，重启 Codex 即可。

常用命令：

```powershell
codex-provider-bridge setup
codex-provider-bridge start
codex-provider-bridge status
codex-provider-bridge doctor
codex-provider-bridge stop
codex-provider-bridge restore
```

前台调试：

```powershell
codex-provider-bridge serve
```

输出机器可读体检结果：

```powershell
codex-provider-bridge doctor --json
```

## 开机自启

Windows 会创建当前用户任务计划程序项：

```powershell
codex-provider-bridge install-startup
codex-provider-bridge startup-status
```

Linux 会创建并启用 systemd user service：

```bash
codex-provider-bridge install-startup
codex-provider-bridge startup-status
```

移除自启：

```powershell
codex-provider-bridge uninstall-startup
```

## 配置兼容

- `CODEX_PROVIDER_BRIDGE_HOME` 优先，否则使用 `~/.codex-provider-bridge`
- `CODEX_HOME` 优先，否则使用 `~/.codex`
- 桥接配置文件仍是 `config.json`
- API Key 优先读取环境变量 `OPENAI_COMPAT_API_KEY`，否则读取保存的本机配置

## 安全说明

- 默认只监听 `127.0.0.1`
- 转发请求时会把传入的 `Authorization` 替换为你的上游 API Key
- 日志只记录方法、路径、状态码和耗时
- 不记录请求体，不记录 token
- 保存的 API Key 位于本机配置文件中，程序会尽量把权限设置为仅当前用户可读写

## 当前限制

- 当前 bridge 同时支持普通 HTTP `responses` 转发和 WebSocket Upgrade 透传
- 安装时会把 Codex provider 写成 `supports_websockets = true`
- WebSocket 握手失败时会直接透传上游错误，不会偷偷降级成普通 HTTP
- 如果日志里频繁出现 `504` 且耗时接近 `300s`，通常是上游网关超时，不是本地端口问题

## 开发

```powershell
cargo test
cargo build --release
```

项目已从 TypeScript/npm 重构为 Rust，Release 二进制是推荐安装方式。
