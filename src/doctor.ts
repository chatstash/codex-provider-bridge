import fs from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { loadBridgeConfig, publicBridgeConfig, resolveApiKey } from "./config.js";
import { canConnect, daemonStatus } from "./daemon.js";
import { getBridgeConfigPath, getBridgeHome, getCodexConfigPath } from "./paths.js";
import { startupStatus } from "./startup.js";
import { hasBridgeProvider, topLevelModelProvider } from "./toml-patch.js";
import type { DoctorResult } from "./types.js";

const execFileAsync = promisify(execFile);

async function fileExists(path: string): Promise<boolean> {
  try {
    await fs.access(path);
    return true;
  } catch {
    return false;
  }
}

async function readLoginStatus(): Promise<string | undefined> {
  const candidates = process.platform === "win32"
    ? [
      process.env.LOCALAPPDATA ? `${process.env.LOCALAPPDATA}\\OpenAI\\Codex\\bin\\codex.exe` : "",
      "codex"
    ].filter(Boolean)
    : ["codex"];

  for (const candidate of candidates) {
    try {
      const { stdout, stderr } = await execFileAsync(candidate, ["login", "status"], { timeout: 3000 });
      return `${stdout}${stderr}`.trim();
    } catch {
      continue;
    }
  }
  return undefined;
}

export async function doctor(): Promise<DoctorResult> {
  const bridgeHome = getBridgeHome();
  const bridgeConfigPath = getBridgeConfigPath();
  const codexConfigPath = getCodexConfigPath();
  const config = await loadBridgeConfig(bridgeConfigPath);
  const key = resolveApiKey(config);
  const codexConfigExists = await fileExists(codexConfigPath);
  const codexConfig = codexConfigExists ? await fs.readFile(codexConfigPath, "utf8") : "";

  return {
    bridgeConfigPath,
    codexConfigPath,
    config: publicBridgeConfig(config),
    apiKeyPresent: Boolean(key.apiKey),
    apiKeySource: key.source,
    portOpen: await canConnect(config.host, config.port),
    daemon: await daemonStatus(bridgeHome),
    startup: await startupStatus({ bridgeHome }),
    codexConfigExists,
    bridgeProviderConfigured: hasBridgeProvider(codexConfig, config.providerId),
    modelProviderIsBridge: topLevelModelProvider(codexConfig) === config.providerId,
    loginStatus: await readLoginStatus()
  };
}

function mark(ok: boolean): string {
  return ok ? "[OK]" : "[需要处理]";
}

export function formatDoctorReport(result: DoctorResult): string {
  const lines = [
    "codex-provider-bridge 体检结果",
    "",
    `${mark(true)} 桥接配置: ${result.bridgeConfigPath}`,
    `${mark(result.codexConfigExists)} Codex 配置: ${result.codexConfigPath}`,
    `${mark(result.apiKeyPresent)} API Key: ${
      result.apiKeyPresent
        ? result.apiKeySource === "environment"
          ? `来自环境变量 ${result.config.apiKeyEnv}`
          : "已保存到本机配置"
        : `未找到，请运行 codex-provider-bridge setup 或设置 ${result.config.apiKeyEnv}`
    }`,
    `${mark(result.bridgeProviderConfigured)} Codex provider: ${
      result.bridgeProviderConfigured ? `已写入 ${result.config.providerId}` : "还没有写入桥接 provider"
    }`,
    `${mark(result.modelProviderIsBridge)} 当前模型 provider: ${
      result.modelProviderIsBridge ? result.config.providerId : "Codex 尚未切到桥接 provider"
    }`,
    `${mark(result.daemon.running)} 后台进程: ${
      result.daemon.running
        ? `正在运行，PID ${result.daemon.pid}`
        : result.daemon.stale
          ? `状态文件已失效，可运行 codex-provider-bridge start 重新启动`
          : "未运行，请运行 codex-provider-bridge start"
    }`,
    `${mark(result.portOpen)} 本地服务: ${
      result.portOpen
        ? `正在监听 http://${result.config.host}:${result.config.port}/v1`
        : `未监听，请运行 codex-provider-bridge start`
    }`,
    `${mark(true)} 日志文件: ${result.daemon.logPath}`,
    `${mark(true)} 开机自启: ${
      !result.startup.supported
        ? "当前系统暂不支持"
        : result.startup.installed
          ? `已安装 (${result.startup.method})`
          : "未安装，可运行 codex-provider-bridge install-startup"
    }`,
    `${mark(Boolean(result.loginStatus))} Codex 登录: ${result.loginStatus || "未检测到，请先在 Codex 中登录 ChatGPT"}`,
    "",
    result.apiKeyPresent && result.bridgeProviderConfigured && result.modelProviderIsBridge && result.daemon.running && result.portOpen
      ? "下一步: 重启 Codex。如果需要调试日志，运行 codex-provider-bridge status 查看日志位置。"
      : "下一步: 运行 codex-provider-bridge setup，根据提示完成配置。"
  ];

  return `${lines.join("\n")}\n`;
}
