# codex-provider-bridge

给 Codex 使用的本地 OpenAI 兼容 Provider 桥接工具。它让 Codex 继续保持 ChatGPT 登录模式，同时把模型请求转发到你自己的 OpenAI 兼容服务。

```text
Codex App -> http://127.0.0.1:11435/v1 -> 你的 OpenAI 兼容 /v1 地址
```

## 为什么用它

- 保留 Codex 的 ChatGPT 登录体验，不需要改动 `auth.json` 或伪装 ChatGPT 登录状态。
- 只在 `~/.codex/config.toml` 中增加一个本地 provider，恢复和排障都比较清晰。
- 对上游服务使用标准 OpenAI 兼容 `/v1` 接口，方便接入自建网关、代理或其他兼容服务。
- 默认只监听 `127.0.0.1`，并在转发时替换为你的上游 API Key，不把请求体或 token 写入日志。
- 提供 `doctor` 体检命令，能一次检查配置、API Key、后台进程、本地端口、Codex provider 和登录状态。
- 支持 Windows 和 Linux 登录后自动启动，日常使用不需要手动打开终端。

## 3 步安装

要求：Node.js 20+，以及你自己的上游服务地址和 API Key。

```powershell
npm install -g github:chatstash/codex-provider-bridge
codex-provider-bridge setup
codex-provider-bridge start
```

运行 `setup` 时需要填写：

- 上游 OpenAI 兼容地址，例如 `https://api.example.com/v1`
- 本地端口，默认 `11435`
- API Key，默认保存到 `~/.codex-provider-bridge/config.json`

安装向导会自动备份并修改 `~/.codex/config.toml`，把 Codex 的 `model_provider` 切到本地桥接 provider。启动后台服务后，重启 Codex 即可。

## 后台运行

正常使用不需要保持终端窗口打开：

```powershell
codex-provider-bridge start
codex-provider-bridge status
codex-provider-bridge stop
```

`start` 会在后台启动本地桥接服务，并把日志写到 `~/.codex-provider-bridge/bridge.log`。如果需要看实时日志或调试问题，可以改用前台模式：

```powershell
codex-provider-bridge serve
```

## 开机自启

如果希望重启电脑后自动启动桥接服务，可以安装当前用户的登录自启项：

```powershell
codex-provider-bridge install-startup
codex-provider-bridge startup-status
```

Windows 会创建一个当前用户的任务计划程序项，登录后自动运行 `codex-provider-bridge start`。

Linux 会创建并启用一个 systemd user service：

```bash
codex-provider-bridge install-startup
codex-provider-bridge startup-status
```

Linux 默认是在当前用户登录后启动。如果你希望机器重启后即使用户还没登录也启动，可以为该用户启用 linger：

```bash
loginctl enable-linger "$USER"
```

移除自启：

```powershell
codex-provider-bridge uninstall-startup
```

## Windows PowerShell 示例

如果不想把 API Key 保存到配置文件，也可以用环境变量覆盖：

```powershell
$env:OPENAI_COMPAT_API_KEY="你的 API Key"
codex-provider-bridge start
```

如果端口被占用，重新运行：

```powershell
codex-provider-bridge setup
```

然后在“本地端口”处输入一个新端口，例如 `11436`。

## 检查状态

```powershell
codex-provider-bridge doctor
```

它会用中文检查配置文件、API Key、Codex provider、后台进程、本地服务端口和 Codex 登录状态。

高级用户可以输出 JSON：

```powershell
codex-provider-bridge doctor --json
```

JSON 和普通输出都会隐藏真实 API Key。

## 恢复原状

```powershell
codex-provider-bridge stop
codex-provider-bridge restore
```

这会停止后台服务，并把 `~/.codex/config.toml` 恢复到上次 `setup` 或 `install` 前的备份。

## 常用命令

```powershell
codex-provider-bridge setup
codex-provider-bridge start
codex-provider-bridge status
codex-provider-bridge install-startup
codex-provider-bridge startup-status
codex-provider-bridge uninstall-startup
codex-provider-bridge doctor
codex-provider-bridge stop
codex-provider-bridge restore
```

也可以在源码目录里运行：

```powershell
npm install
npm run build
npm run setup
npm start
npm run status
npm run install-startup
npm run startup-status
npm run uninstall-startup
npm run doctor
```

## 常见问题

**提示缺少上游地址怎么办？**

运行 `codex-provider-bridge setup`，在“上游 OpenAI 兼容地址”处填写你的 `/v1` 地址。

**提示缺少 API Key 怎么办？**

运行 `codex-provider-bridge setup`，按提示粘贴 API Key；或者设置 `OPENAI_COMPAT_API_KEY` 环境变量。

**Codex 还是没走桥接 provider 怎么办？**

先运行 `codex-provider-bridge status` 确认后台服务正在运行，再执行 `codex-provider-bridge doctor`。如果 doctor 提示 provider 未写入，重新运行 `setup`。

**会修改 ChatGPT 登录信息吗？**

不会。它只修改 `~/.codex/config.toml`，不会编辑 `auth.json`，也不会设置 `chatgpt_base_url`。

**API Key 会被打印出来吗？**

不会。日志、doctor 和 JSON 输出都只显示 API Key 是否存在，不显示真实值。

## 安全说明

- 默认只监听 `127.0.0.1`
- 转发请求时会把传入的 `Authorization` 替换为你的上游 API Key
- 日志只记录方法、路径、状态码和耗时
- 不记录请求体，不记录 token
- 保存的 API Key 位于本机配置文件中，程序会尽量把权限设置为仅当前用户可读写

## 开发

```powershell
npm install
npm test
```

项目使用 TypeScript 和 Node.js 内置 test runner。
