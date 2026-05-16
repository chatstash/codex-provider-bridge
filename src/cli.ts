#!/usr/bin/env node
import { loadBridgeConfig, mergeConfig, saveBridgeConfig } from "./config.js";
import { doctor, formatDoctorReport } from "./doctor.js";
import { installBridge, restoreBridge } from "./install.js";
import { defaultConfig, getBridgeConfigPath } from "./paths.js";
import { serve } from "./proxy.js";
import { setupBridge } from "./setup.js";

interface ParsedArgs {
  command: string;
  flags: Record<string, string | boolean>;
}

function parseArgs(argv: string[]): ParsedArgs {
  const [command = "help", ...rest] = argv;
  const flags: Record<string, string | boolean> = {};

  for (let index = 0; index < rest.length; index += 1) {
    const item = rest[index];
    if (!item.startsWith("--")) {
      continue;
    }
    const [rawKey, inlineValue] = item.slice(2).split("=", 2);
    const key = rawKey.replace(/-([a-z])/g, (_, char: string) => char.toUpperCase());
    if (inlineValue !== undefined) {
      flags[key] = inlineValue;
    } else if (rest[index + 1] && !rest[index + 1].startsWith("--")) {
      flags[key] = rest[index + 1];
      index += 1;
    } else {
      flags[key] = true;
    }
  }

  return { command, flags };
}

function usage(): string {
  return `codex-provider-bridge

Commands:
  setup       Run the beginner-friendly setup wizard.
  serve       Start the local provider bridge.
  install     Backup and patch ~/.codex/config.toml.
  restore     Restore ~/.codex/config.toml from the last install backup.
  doctor      Check bridge, Codex config, API key, and login status.

Options:
  --host <host>             Default: 127.0.0.1
  --port <port>             Default: 11435
  --upstream <url>          Default: https://sub2api.fcyaxing.com/v1
  --api-key-env <name>      Default: SUB2API_API_KEY
  --api-key <key>           Save API key to local bridge config.
  --provider-id <id>        Default: codex_provider_bridge
  --json                    Print machine-readable output for doctor.
`;
}

function partialConfigFromFlags(flags: Record<string, string | boolean>) {
  const config = {
    host: typeof flags.host === "string" ? flags.host : undefined,
    port: typeof flags.port === "string" ? Number(flags.port) : undefined,
    upstreamBaseUrl: typeof flags.upstream === "string" ? flags.upstream : undefined,
    apiKeyEnv: typeof flags.apiKeyEnv === "string" ? flags.apiKeyEnv : undefined,
    apiKey: typeof flags.apiKey === "string" ? flags.apiKey : undefined,
    providerId: typeof flags.providerId === "string" ? flags.providerId : undefined
  };

  return Object.fromEntries(Object.entries(config).filter(([, value]) => value !== undefined));
}

async function configFromSavedAndFlags(flags: Record<string, string | boolean>) {
  const saved = await loadBridgeConfig();
  return mergeConfig({
    ...saved,
    ...partialConfigFromFlags(flags)
  });
}

async function main(): Promise<void> {
  const { command, flags } = parseArgs(process.argv.slice(2));

  if (command === "help" || flags.help) {
    console.log(usage());
    return;
  }

  if (command === "setup") {
    const result = await setupBridge();
    console.log("");
    console.log(`配置已保存: ${result.bridgeConfigPath}`);
    console.log(`Codex 配置已更新: ${result.codexConfigPath}`);
    console.log(`备份已创建: ${result.backupPath}`);
    console.log("");
    console.log("下一步:");
    console.log("  1. 运行 codex-provider-bridge serve");
    console.log("  2. 重启 Codex");
    console.log("  3. 如果有问题，运行 codex-provider-bridge doctor");
    return;
  }

  if (command === "serve") {
    await serve(await configFromSavedAndFlags(flags));
    return;
  }

  if (command === "install") {
    const config = await configFromSavedAndFlags(flags);
    await saveBridgeConfig(config, getBridgeConfigPath());
    const result = await installBridge({ config });
    console.log(`Codex 配置已更新: ${result.codexConfigPath}`);
    console.log(`备份已创建: ${result.backupPath}`);
    console.log(`桥接配置: ${result.bridgeConfigPath}`);
    console.log("下一步: 运行 codex-provider-bridge serve，然后重启 Codex。");
    return;
  }

  if (command === "restore") {
    const result = await restoreBridge();
    console.log(`Codex 配置已恢复: ${result.codexConfigPath}`);
    console.log(`使用备份: ${result.backupPath}`);
    return;
  }

  if (command === "doctor") {
    const result = await doctor();
    if (flags.json) {
      console.log(JSON.stringify(result, null, 2));
    } else {
      console.log(formatDoctorReport(result));
    }
    return;
  }

  console.error(`Unknown command: ${command}`);
  console.error(usage());
  process.exitCode = 1;
}

main().catch((error) => {
  console.error(formatError(error));
  process.exitCode = 1;
});

function formatError(error: unknown): string {
  const code = typeof error === "object" && error ? (error as NodeJS.ErrnoException).code : undefined;
  if (code === "EADDRINUSE") {
    return "启动失败: 本地端口已被占用。请关闭占用该端口的程序，或运行 codex-provider-bridge setup 换一个端口。";
  }
  if (code === "ENOENT") {
    return `找不到需要的文件: ${error instanceof Error ? error.message : String(error)}
如果是恢复失败，请先运行 codex-provider-bridge install 或 setup 创建备份。`;
  }
  if (error instanceof Error) {
    return `${error.message}
需要帮助时可以运行 codex-provider-bridge doctor 查看状态。`;
  }
  return String(error);
}
