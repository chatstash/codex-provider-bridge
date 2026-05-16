import path from "node:path";
import {
  DEFAULT_API_KEY_ENV,
  DEFAULT_HOST,
  DEFAULT_PORT,
  DEFAULT_PROVIDER_ID,
  DEFAULT_PROVIDER_NAME,
  DEFAULT_UPSTREAM_BASE_URL
} from "./defaults.js";
import type { BridgeConfig, PathOptions } from "./types.js";

export function getHomeDir(options: PathOptions = {}): string {
  const env = options.env ?? process.env;
  const home = env.HOME || env.USERPROFILE;
  if (!home) {
    throw new Error("Cannot resolve home directory from HOME or USERPROFILE.");
  }
  return home;
}

export function getBridgeHome(options: PathOptions = {}): string {
  const env = options.env ?? process.env;
  if (env.CODEX_PROVIDER_BRIDGE_HOME) {
    return path.resolve(env.CODEX_PROVIDER_BRIDGE_HOME);
  }
  return path.join(getHomeDir(options), ".codex-provider-bridge");
}

export function getBridgeConfigPath(options: PathOptions = {}): string {
  return path.join(getBridgeHome(options), "config.json");
}

export function getDaemonStatePath(bridgeHome: string): string {
  return path.join(bridgeHome, "bridge.pid.json");
}

export function getDaemonLogPath(bridgeHome: string): string {
  return path.join(bridgeHome, "bridge.log");
}

export function getInstallStatePath(bridgeHome: string): string {
  return path.join(bridgeHome, "install-state.json");
}

export function getBackupDir(bridgeHome: string): string {
  return path.join(bridgeHome, "backups");
}

export function getCodexHome(options: PathOptions = {}): string {
  const env = options.env ?? process.env;
  if (env.CODEX_HOME) {
    return path.resolve(env.CODEX_HOME);
  }
  return path.join(getHomeDir(options), ".codex");
}

export function getCodexConfigPath(options: PathOptions = {}): string {
  return path.join(getCodexHome(options), "config.toml");
}

export function defaultConfig(): BridgeConfig {
  return {
    host: DEFAULT_HOST,
    port: DEFAULT_PORT,
    upstreamBaseUrl: DEFAULT_UPSTREAM_BASE_URL,
    apiKeyEnv: DEFAULT_API_KEY_ENV,
    providerId: DEFAULT_PROVIDER_ID,
    providerName: DEFAULT_PROVIDER_NAME
  };
}
