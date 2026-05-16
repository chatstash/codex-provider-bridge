import fs from "node:fs/promises";
import path from "node:path";
import { loadBridgeConfig, saveBridgeConfig } from "./config.js";
import { getBackupDir, getBridgeConfigPath, getBridgeHome, getCodexConfigPath, getInstallStatePath } from "./paths.js";
import { patchCodexConfig } from "./toml-patch.js";
import type { InstallOptions, InstallResult, RestoreOptions, RestoreResult } from "./types.js";

function timestamp(): string {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

export async function installBridge(options: InstallOptions = {}): Promise<InstallResult> {
  const bridgeHome = options.bridgeHome ?? getBridgeHome();
  const codexConfigPath = options.codexConfigPath ?? getCodexConfigPath();
  const bridgeConfigPath = getBridgeConfigPath({ env: { ...process.env, CODEX_PROVIDER_BRIDGE_HOME: bridgeHome } });
  const config = options.config ?? await loadBridgeConfig(bridgeConfigPath);
  const backupDir = getBackupDir(bridgeHome);
  const backupPath = path.join(backupDir, `config.toml.${timestamp()}.bak`);

  await fs.mkdir(path.dirname(codexConfigPath), { recursive: true });
  await fs.mkdir(backupDir, { recursive: true });

  let current = "";
  try {
    current = await fs.readFile(codexConfigPath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
      throw error;
    }
  }

  await fs.writeFile(backupPath, current, "utf8");
  await fs.writeFile(codexConfigPath, patchCodexConfig(current, config), "utf8");
  await saveBridgeConfig(config, bridgeConfigPath);
  await fs.writeFile(
    getInstallStatePath(bridgeHome),
    `${JSON.stringify({ codexConfigPath, backupPath, installedAt: new Date().toISOString() }, null, 2)}\n`,
    "utf8"
  );

  return { codexConfigPath, bridgeConfigPath, backupPath };
}

export async function restoreBridge(options: RestoreOptions = {}): Promise<RestoreResult> {
  const bridgeHome = options.bridgeHome ?? getBridgeHome();
  const statePath = getInstallStatePath(bridgeHome);
  const state = JSON.parse(await fs.readFile(statePath, "utf8")) as {
    codexConfigPath?: string;
    backupPath?: string;
  };
  const backupPath = state.backupPath;
  const codexConfigPath = options.codexConfigPath ?? state.codexConfigPath;

  if (!backupPath || !codexConfigPath) {
    throw new Error(`Invalid install state in ${statePath}.`);
  }

  const backup = await fs.readFile(backupPath, "utf8");
  await fs.mkdir(path.dirname(codexConfigPath), { recursive: true });
  await fs.writeFile(codexConfigPath, backup, "utf8");

  return { codexConfigPath, backupPath };
}
