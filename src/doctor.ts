import fs from "node:fs/promises";
import net from "node:net";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { loadBridgeConfig, publicBridgeConfig, resolveApiKey } from "./config.js";
import { getBridgeConfigPath, getCodexConfigPath } from "./paths.js";
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

async function canConnect(host: string, port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host, port });
    socket.setTimeout(800);
    socket.once("connect", () => {
      socket.destroy();
      resolve(true);
    });
    socket.once("timeout", () => {
      socket.destroy();
      resolve(false);
    });
    socket.once("error", () => resolve(false));
  });
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
    `${mark(result.portOpen)} 本地服务: ${
      result.portOpen
        ? `正在监听 http://${result.config.host}:${result.config.port}/v1`
        : `未监听，请运行 codex-provider-bridge serve`
    }`,
    `${mark(Boolean(result.loginStatus))} Codex 登录: ${result.loginStatus || "未检测到，请先在 Codex 中登录 ChatGPT"}`,
    "",
    result.apiKeyPresent && result.bridgeProviderConfigured && result.modelProviderIsBridge
      ? "下一步: 保持 codex-provider-bridge serve 运行，然后重启 Codex。"
      : "下一步: 运行 codex-provider-bridge setup，根据提示完成配置。"
  ];

  return `${lines.join("\n")}\n`;
}
