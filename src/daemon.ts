import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import fsSync from "node:fs";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { assertConfigured, loadBridgeConfig, resolveApiKey } from "./config.js";
import { getBridgeConfigPath, getBridgeHome, getDaemonLogPath, getDaemonStatePath } from "./paths.js";
import type { DaemonState, DaemonStatus, StartDaemonResult, StopDaemonResult } from "./types.js";

function bridgeUrl(host: string, port: number): string {
  return `http://${host}:${port}/v1`;
}

async function fileExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

function isProcessRunning(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export async function canConnect(host: string, port: number, timeoutMs = 800): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host, port });
    socket.setTimeout(timeoutMs);
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

async function readDaemonState(statePath: string): Promise<DaemonState | null> {
  try {
    return JSON.parse(await fs.readFile(statePath, "utf8")) as DaemonState;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

export async function daemonStatus(bridgeHome = getBridgeHome()): Promise<DaemonStatus> {
  const statePath = getDaemonStatePath(bridgeHome);
  const logPath = getDaemonLogPath(bridgeHome);
  const state = await readDaemonState(statePath);
  const running = state ? isProcessRunning(state.pid) : false;
  const stale = Boolean(state && !running);

  return {
    statePath,
    logPath: state?.logPath ?? logPath,
    running,
    pid: state?.pid,
    stale
  };
}

export async function startDaemon(bridgeHome = getBridgeHome()): Promise<StartDaemonResult> {
  const configPath = getBridgeConfigPath({ env: { ...process.env, CODEX_PROVIDER_BRIDGE_HOME: bridgeHome } });
  const config = await loadBridgeConfig(configPath);
  assertConfigured(config);

  const key = resolveApiKey(config);
  if (!key.apiKey) {
    throw new Error(`Missing API key. Run "codex-provider-bridge setup" or set ${config.apiKeyEnv}.`);
  }

  const status = await daemonStatus(bridgeHome);
  const url = bridgeUrl(config.host, config.port);
  if (status.running) {
    return { alreadyRunning: true, pid: status.pid, statePath: status.statePath, logPath: status.logPath, url };
  }
  if (status.stale) {
    await fs.rm(status.statePath, { force: true });
  }

  if (await canConnect(config.host, config.port)) {
    throw new Error(`本地端口 ${config.port} 已被占用。请运行 codex-provider-bridge status 查看，或运行 setup 换一个端口。`);
  }

  await fs.mkdir(bridgeHome, { recursive: true });
  const logPath = getDaemonLogPath(bridgeHome);
  const out = fsSync.openSync(logPath, "a");
  const err = fsSync.openSync(logPath, "a");
  const cliPath = fileURLToPath(import.meta.url).replace(/[\\/]daemon\.js$/, `${path.sep}cli.js`);
  let child;
  try {
    child = spawn(process.execPath, [cliPath, "serve"], {
      cwd: process.cwd(),
      detached: true,
      env: {
        ...process.env,
        CODEX_PROVIDER_BRIDGE_HOME: bridgeHome
      },
      stdio: ["ignore", out, err]
    });
  } finally {
    fsSync.closeSync(out);
    fsSync.closeSync(err);
  }

  child.unref();
  if (!child.pid) {
    throw new Error("后台进程启动失败：没有拿到进程 ID。");
  }

  const state: DaemonState = {
    pid: child.pid,
    startedAt: new Date().toISOString(),
    command: `${process.execPath} ${cliPath} serve`,
    logPath,
    host: config.host,
    port: config.port
  };
  await fs.writeFile(status.statePath, `${JSON.stringify(state, null, 2)}\n`, "utf8");

  return {
    alreadyRunning: false,
    pid: child.pid,
    statePath: status.statePath,
    logPath,
    url
  };
}

export async function stopDaemon(bridgeHome = getBridgeHome()): Promise<StopDaemonResult> {
  const status = await daemonStatus(bridgeHome);
  if (!status.pid) {
    return { stopped: false, wasRunning: false, statePath: status.statePath };
  }

  if (!status.running) {
    await fs.rm(status.statePath, { force: true });
    return { stopped: false, wasRunning: false, pid: status.pid, statePath: status.statePath };
  }

  process.kill(status.pid, "SIGTERM");
  for (let attempt = 0; attempt < 20; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 100));
    if (!isProcessRunning(status.pid)) {
      break;
    }
  }

  await fs.rm(status.statePath, { force: true });
  return { stopped: true, wasRunning: true, pid: status.pid, statePath: status.statePath };
}

export async function hasDaemonState(bridgeHome = getBridgeHome()): Promise<boolean> {
  return fileExists(getDaemonStatePath(bridgeHome));
}
