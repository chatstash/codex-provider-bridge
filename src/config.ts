import fs from "node:fs/promises";
import path from "node:path";
import { defaultConfig, getBridgeConfigPath } from "./paths.js";
import type { ApiKeySource, BridgeConfig, PublicBridgeConfig } from "./types.js";

export function mergeConfig(raw: Partial<BridgeConfig> = {}): BridgeConfig {
  const defaults = defaultConfig();
  const config: BridgeConfig = {
    host: raw.host || defaults.host,
    port: Number.isFinite(raw.port) ? Number(raw.port) : defaults.port,
    upstreamBaseUrl: raw.upstreamBaseUrl || defaults.upstreamBaseUrl,
    apiKeyEnv: raw.apiKeyEnv || defaults.apiKeyEnv,
    providerId: raw.providerId || defaults.providerId,
    providerName: raw.providerName || defaults.providerName
  };

  if (typeof raw.apiKey === "string" && raw.apiKey.trim()) {
    config.apiKey = raw.apiKey.trim();
  }

  return config;
}

export async function loadBridgeConfig(configPath = getBridgeConfigPath()): Promise<BridgeConfig> {
  try {
    const raw = await fs.readFile(configPath, "utf8");
    return mergeConfig(JSON.parse(raw) as Partial<BridgeConfig>);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return defaultConfig();
    }
    throw error;
  }
}

export async function saveBridgeConfig(config: BridgeConfig, configPath = getBridgeConfigPath()): Promise<void> {
  await fs.mkdir(path.dirname(configPath), { recursive: true });
  await fs.writeFile(configPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");
  await fs.chmod(configPath, 0o600).catch(() => undefined);
}

export function resolveApiKey(
  config: Pick<BridgeConfig, "apiKey" | "apiKeyEnv">,
  env: NodeJS.ProcessEnv = process.env
): { apiKey?: string; source: ApiKeySource | null } {
  const envKey = env[config.apiKeyEnv];
  if (envKey && envKey.trim()) {
    return { apiKey: envKey.trim(), source: "environment" };
  }

  if (config.apiKey && config.apiKey.trim()) {
    return { apiKey: config.apiKey.trim(), source: "config" };
  }

  return { source: null };
}

export function publicBridgeConfig(config: BridgeConfig): PublicBridgeConfig {
  const { apiKey: _apiKey, ...safeConfig } = config;
  return {
    ...safeConfig,
    apiKeySaved: Boolean(_apiKey)
  };
}
