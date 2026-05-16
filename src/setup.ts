import { createInterface } from "node:readline/promises";
import { loadBridgeConfig, mergeConfig } from "./config.js";
import { installBridge } from "./install.js";
import { getBridgeConfigPath, getBridgeHome } from "./paths.js";
import type { BridgeConfig, InstallResult } from "./types.js";

export interface SetupOptions {
  bridgeHome?: string;
  codexConfigPath?: string;
  input?: NodeJS.ReadableStream;
  output?: NodeJS.WritableStream;
  prompt?: (question: string) => string | Promise<string>;
}

export interface SetupResult extends InstallResult {
  config: BridgeConfig;
}

function cleanInput(value: string): string {
  return value.trim();
}

function readPort(value: string, fallback: number): number {
  const trimmed = cleanInput(value);
  if (!trimmed) {
    return fallback;
  }

  const port = Number(trimmed);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("端口必须是 1 到 65535 之间的数字。");
  }
  return port;
}

function readUrl(value: string, fallback: string): string {
  const trimmed = cleanInput(value);
  if (!trimmed) {
    return fallback;
  }

  const parsed = new URL(trimmed);
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error("上游地址必须以 http:// 或 https:// 开头。");
  }
  return parsed.toString().replace(/\/+$/, "");
}

async function ask(rl: ReturnType<typeof createInterface>, question: string): Promise<string> {
  return cleanInput(await rl.question(question));
}

async function readPipedLines(input: NodeJS.ReadableStream): Promise<string[]> {
  const chunks: Buffer[] = [];
  for await (const chunk of input as AsyncIterable<Buffer | string>) {
    chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk);
  }
  return Buffer.concat(chunks).toString("utf8").split(/\r?\n/);
}

export async function setupBridge(options: SetupOptions = {}): Promise<SetupResult> {
  const bridgeHome = options.bridgeHome ?? getBridgeHome();
  const bridgeConfigPath = getBridgeConfigPath({
    env: { ...process.env, CODEX_PROVIDER_BRIDGE_HOME: bridgeHome }
  });
  const saved = await loadBridgeConfig(bridgeConfigPath);
  const input = options.input ?? process.stdin;
  const output = options.output ?? process.stdout;
  const inputIsTty = Boolean((input as { isTTY?: boolean }).isTTY);
  const pipedLines = !options.prompt && !inputIsTty ? await readPipedLines(input) : undefined;
  let pipedLineIndex = 0;
  const rl = options.prompt || pipedLines ? undefined : createInterface({ input, output });
  const askQuestion = async (question: string): Promise<string> => {
    if (options.prompt) {
      return cleanInput(await options.prompt(question));
    }
    if (pipedLines) {
      output.write(question);
      return cleanInput(pipedLines[pipedLineIndex++] ?? "");
    }
    return ask(rl as ReturnType<typeof createInterface>, question);
  };

  try {
    output.write("\ncodex-provider-bridge 安装向导\n");
    output.write("按回车使用默认值。API Key 会保存到本机配置文件，不会打印到日志。\n\n");

    const upstreamBaseUrl = readUrl(
      await askQuestion(`上游 OpenAI 兼容地址 [${saved.upstreamBaseUrl}]: `),
      saved.upstreamBaseUrl
    );
    const port = readPort(await askQuestion(`本地端口 [${saved.port}]: `), saved.port);
    const apiKeyEnv = await askQuestion(`环境变量名 [${saved.apiKeyEnv}]: `) || saved.apiKeyEnv;
    const existingKeyHint = saved.apiKey ? "，留空则沿用已保存密钥" : "";
    const apiKeyInput = await askQuestion(`API Key${existingKeyHint}: `);
    const apiKey = apiKeyInput || saved.apiKey || process.env[apiKeyEnv];
    if (!apiKey) {
      throw new Error(`需要 API Key。请重新运行 setup 并填写，或先设置环境变量 ${apiKeyEnv}。`);
    }

    const config = mergeConfig({
      ...saved,
      upstreamBaseUrl,
      port,
      apiKeyEnv,
      apiKey
    });

    const result = await installBridge({
      bridgeHome,
      codexConfigPath: options.codexConfigPath,
      config
    });

    return { ...result, config };
  } finally {
    rl?.close();
  }
}
