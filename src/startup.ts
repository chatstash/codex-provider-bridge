import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { getBridgeHome, getHomeDir } from "./paths.js";
import type { StartupInstallResult, StartupStatus, StartupUninstallResult } from "./types.js";

export const STARTUP_TASK_NAME = "codex-provider-bridge";
export const SYSTEMD_SERVICE_NAME = "codex-provider-bridge.service";

interface ExecResult {
  stdout: string | Buffer;
  stderr: string | Buffer;
}

export type ExecFileRunner = (
  file: string,
  args: readonly string[],
  options?: { windowsHide?: boolean; timeout?: number }
) => Promise<ExecResult>;

interface StartupOptions {
  bridgeHome?: string;
  cliPath?: string;
  execFile?: ExecFileRunner;
  nodePath?: string;
  platform?: NodeJS.Platform;
  env?: NodeJS.ProcessEnv;
}

const execFileAsync = promisify(execFile) as ExecFileRunner;

function assertSupported(platform: NodeJS.Platform): void {
  if (platform !== "win32" && platform !== "linux") {
    throw new Error("开机自启目前支持 Windows 任务计划程序和 Linux systemd user service。");
  }
}

function currentCliPath(): string {
  return fileURLToPath(import.meta.url).replace(/[\\/]startup\.js$/, `${path.sep}cli.js`);
}

export function getStartupScriptPath(bridgeHome = getBridgeHome()): string {
  return path.join(bridgeHome, "startup.cmd");
}

export function getSystemdUserDir(options: StartupOptions = {}): string {
  const env = options.env ?? process.env;
  if (env.XDG_CONFIG_HOME) {
    return path.join(env.XDG_CONFIG_HOME, "systemd", "user");
  }
  return path.join(getHomeDir({ env }), ".config", "systemd", "user");
}

export function getSystemdServicePath(options: StartupOptions = {}): string {
  return path.join(getSystemdUserDir(options), SYSTEMD_SERVICE_NAME);
}

function escapeBatchValue(value: string): string {
  return value.replace(/%/g, "%%");
}

function quoteBatchArg(value: string): string {
  return `"${escapeBatchValue(value).replace(/"/g, '""')}"`;
}

function startupScript(bridgeHome: string, nodePath: string, cliPath: string): string {
  return [
    "@echo off",
    `set "CODEX_PROVIDER_BRIDGE_HOME=${escapeBatchValue(bridgeHome)}"`,
    `${quoteBatchArg(nodePath)} ${quoteBatchArg(cliPath)} start`
  ].join("\r\n") + "\r\n";
}

function taskAction(scriptPath: string): string {
  return `"${scriptPath.replace(/"/g, '""')}"`;
}

function escapeSystemdValue(value: string): string {
  return value
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/%/g, "%%")
    .replace(/\$/g, "$$");
}

function quoteSystemdValue(value: string): string {
  return `"${escapeSystemdValue(value)}"`;
}

function systemdService(bridgeHome: string, nodePath: string, cliPath: string): string {
  return [
    "[Unit]",
    "Description=codex-provider-bridge autostart",
    "After=default.target",
    "",
    "[Service]",
    "Type=oneshot",
    "RemainAfterExit=yes",
    `Environment=${quoteSystemdValue(`CODEX_PROVIDER_BRIDGE_HOME=${bridgeHome}`)}`,
    `ExecStart=${quoteSystemdValue(nodePath)} ${quoteSystemdValue(cliPath)} start`,
    `ExecStop=${quoteSystemdValue(nodePath)} ${quoteSystemdValue(cliPath)} stop`,
    "",
    "[Install]",
    "WantedBy=default.target"
  ].join("\n") + "\n";
}

async function runSchtasks(
  args: readonly string[],
  runner: ExecFileRunner,
  timeout = 10000
): Promise<ExecResult> {
  return runner("schtasks.exe", args, { windowsHide: true, timeout });
}

async function runSystemctl(
  args: readonly string[],
  runner: ExecFileRunner,
  timeout = 10000
): Promise<ExecResult> {
  return runner("systemctl", ["--user", ...args], { windowsHide: true, timeout });
}

async function fileExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

export async function startupStatus(options: StartupOptions = {}): Promise<StartupStatus> {
  const platform = options.platform ?? process.platform;
  const bridgeHome = options.bridgeHome ?? getBridgeHome({ env: options.env });
  const scriptPath = getStartupScriptPath(bridgeHome);
  const servicePath = getSystemdServicePath(options);
  const runner = options.execFile ?? execFileAsync;

  if (platform !== "win32" && platform !== "linux") {
    return {
      supported: false,
      installed: false,
      scriptPath,
      serviceName: SYSTEMD_SERVICE_NAME,
      servicePath,
      taskName: STARTUP_TASK_NAME
    };
  }

  if (platform === "linux") {
    try {
      const result = await runSystemctl(["is-enabled", SYSTEMD_SERVICE_NAME], runner);
      return {
        supported: true,
        installed: `${result.stdout}`.trim() === "enabled",
        method: "systemd-user",
        serviceName: SYSTEMD_SERVICE_NAME,
        servicePath,
        detail: `${result.stdout}${result.stderr}`.trim()
      };
    } catch {
      return {
        supported: true,
        installed: false,
        method: "systemd-user",
        serviceName: SYSTEMD_SERVICE_NAME,
        servicePath
      };
    }
  }

  try {
    const result = await runSchtasks(["/Query", "/TN", STARTUP_TASK_NAME, "/FO", "LIST", "/V"], runner);
    return {
      supported: true,
      installed: true,
      method: "windows-task-scheduler",
      taskName: STARTUP_TASK_NAME,
      scriptPath,
      detail: `${result.stdout}${result.stderr}`.trim()
    };
  } catch {
    return {
      supported: true,
      installed: false,
      method: "windows-task-scheduler",
      taskName: STARTUP_TASK_NAME,
      scriptPath
    };
  }
}

export async function installStartup(options: StartupOptions = {}): Promise<StartupInstallResult> {
  const platform = options.platform ?? process.platform;
  assertSupported(platform);

  const bridgeHome = options.bridgeHome ?? getBridgeHome({ env: options.env });
  const cliPath = options.cliPath ?? currentCliPath();
  const nodePath = options.nodePath ?? process.execPath;
  const runner = options.execFile ?? execFileAsync;

  if (platform === "linux") {
    const servicePath = getSystemdServicePath(options);
    await fs.mkdir(path.dirname(servicePath), { recursive: true });
    await fs.writeFile(servicePath, systemdService(bridgeHome, nodePath, cliPath), "utf8");
    await runSystemctl(["daemon-reload"], runner);
    await runSystemctl(["enable", SYSTEMD_SERVICE_NAME], runner);

    return {
      supported: true,
      installed: true,
      method: "systemd-user",
      serviceName: SYSTEMD_SERVICE_NAME,
      servicePath
    };
  }

  const scriptPath = getStartupScriptPath(bridgeHome);
  await fs.mkdir(bridgeHome, { recursive: true });
  await fs.writeFile(scriptPath, startupScript(bridgeHome, nodePath, cliPath), "utf8");

  await runSchtasks([
    "/Create",
    "/TN",
    STARTUP_TASK_NAME,
    "/SC",
    "ONLOGON",
    "/TR",
    taskAction(scriptPath),
    "/F"
  ], runner);

  return {
    supported: true,
    installed: true,
    method: "windows-task-scheduler",
    taskName: STARTUP_TASK_NAME,
    scriptPath
  };
}

export async function uninstallStartup(options: StartupOptions = {}): Promise<StartupUninstallResult> {
  const platform = options.platform ?? process.platform;
  assertSupported(platform);

  const status = await startupStatus(options);
  const runner = options.execFile ?? execFileAsync;

  if (platform === "linux") {
    const servicePath = getSystemdServicePath(options);
    const serviceExists = await fileExists(servicePath);
    if (!status.installed && !serviceExists) {
      return {
        ...status,
        removed: false
      };
    }

    if (status.installed) {
      await runSystemctl(["disable", SYSTEMD_SERVICE_NAME], runner);
    }
    await fs.rm(servicePath, { force: true });
    await runSystemctl(["daemon-reload"], runner);

    return {
      ...status,
      installed: false,
      removed: true
    };
  }

  if (!status.installed) {
    return {
      ...status,
      removed: false
    };
  }

  await runSchtasks(["/Delete", "/TN", STARTUP_TASK_NAME, "/F"], runner);

  return {
    ...status,
    installed: false,
    removed: true
  };
}
