# codex-provider-bridge

给 Codex 使用的本地 OpenAI 兼容 Provider 桥接工具。它让 Codex 继续保持 ChatGPT 登录模式，同时把模型请求转发到你自己的 OpenAI 兼容服务，例如 sub2api。

```text
Codex App -> http://127.0.0.1:11435/v1 -> https://sub2api.fcyaxing.com/v1
```

## 3 步安装

要求：Node.js 20+，以及一个上游服务的 API Key。

```powershell
npm install -g github:chatstash/codex-provider-bridge
codex-provider-bridge setup
codex-provider-bridge serve
```

运行 `setup` 时一路按回车即可使用默认值，只需要粘贴 API Key。安装向导会：

- 保存配置到 `~/.codex-provider-bridge/config.json`
- 备份 `~/.codex/config.toml`
- 把 Codex 的 `model_provider` 切到本地桥接 provider
- 开启 `remote_control` 和 `prevent_idle_sleep`

启动 `serve` 后保持这个终端窗口打开，然后重启 Codex。

## Windows PowerShell 示例

如果你不想把 API Key 保存到配置文件，也可以用环境变量覆盖：

```powershell
$env:SUB2API_API_KEY="你的 API Key"
codex-provider-bridge serve
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

它会用中文检查配置文件、API Key、Codex provider、本地服务端口和 Codex 登录状态。

高级用户可以输出 JSON：

```powershell
codex-provider-bridge doctor --json
```

JSON 和普通输出都会隐藏真实 API Key。

## 恢复原状

```powershell
codex-provider-bridge restore
```

这会把 `~/.codex/config.toml` 恢复到上次 `setup` 或 `install` 前的备份。

## 常用命令

```powershell
codex-provider-bridge setup
codex-provider-bridge serve
codex-provider-bridge doctor
codex-provider-bridge restore
```

也可以在源码目录里运行：

```powershell
npm install
npm run build
npm run setup
npm run serve
```

## 常见问题

**提示缺少 API Key 怎么办？**

运行 `codex-provider-bridge setup`，按提示粘贴 API Key；或者设置 `SUB2API_API_KEY` 环境变量。

**Codex 还是没走桥接 provider 怎么办？**

先确认 `codex-provider-bridge serve` 正在运行，再执行 `codex-provider-bridge doctor`。如果 doctor 提示 provider 未写入，重新运行 `setup`。

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
