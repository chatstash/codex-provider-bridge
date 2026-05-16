import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  installStartup,
  startupStatus,
  uninstallStartup,
  getStartupScriptPath,
  getSystemdServicePath,
  STARTUP_TASK_NAME,
  SYSTEMD_SERVICE_NAME,
  type ExecFileRunner
} from "../src/startup.js";

function makeRunner(calls: Array<{ file: string; args: readonly string[] }>, enabled = true): ExecFileRunner {
  return async (file, args) => {
    calls.push({ file, args });

    if (file === "systemctl" && args.includes("is-enabled")) {
      if (!enabled) {
        throw new Error("disabled");
      }
      return { stdout: "enabled\n", stderr: "" };
    }

    if (file === "schtasks.exe" && args.includes("/Query")) {
      if (!enabled) {
        throw new Error("not found");
      }
      return { stdout: "TaskName: codex-provider-bridge\n", stderr: "" };
    }

    return { stdout: "", stderr: "" };
  };
}

test("installStartup creates a Windows logon task script", async () => {
  const bridgeHome = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-startup-win-"));
  const calls: Array<{ file: string; args: readonly string[] }> = [];

  const result = await installStartup({
    bridgeHome,
    cliPath: "C:\\Tools\\codex-provider-bridge\\cli.js",
    nodePath: "C:\\Program Files\\nodejs\\node.exe",
    platform: "win32",
    execFile: makeRunner(calls)
  });

  assert.equal(result.method, "windows-task-scheduler");
  assert.equal(result.taskName, STARTUP_TASK_NAME);
  assert.match(await fs.readFile(getStartupScriptPath(bridgeHome), "utf8"), /cli\.js" start/);
  assert.deepEqual(calls.at(-1)?.args.slice(0, 4), ["/Create", "/TN", STARTUP_TASK_NAME, "/SC"]);
});

test("installStartup creates and enables a Linux systemd user service", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-startup-linux-"));
  const bridgeHome = path.join(temp, "bridge-home");
  const xdgConfigHome = path.join(temp, "config");
  const calls: Array<{ file: string; args: readonly string[] }> = [];

  const result = await installStartup({
    bridgeHome,
    cliPath: "/opt/codex-provider-bridge/cli.js",
    nodePath: "/usr/bin/node",
    platform: "linux",
    env: { HOME: temp, XDG_CONFIG_HOME: xdgConfigHome },
    execFile: makeRunner(calls)
  });

  const servicePath = getSystemdServicePath({ env: { HOME: temp, XDG_CONFIG_HOME: xdgConfigHome } });
  const service = await fs.readFile(servicePath, "utf8");

  assert.equal(result.method, "systemd-user");
  assert.equal(result.serviceName, SYSTEMD_SERVICE_NAME);
  assert.match(service, /ExecStart="\/usr\/bin\/node" "\/opt\/codex-provider-bridge\/cli\.js" start/);
  assert.match(service, /ExecStop="\/usr\/bin\/node" "\/opt\/codex-provider-bridge\/cli\.js" stop/);
  assert.match(service, /RemainAfterExit=yes/);
  assert.deepEqual(calls.map((call) => call.args.join(" ")), [
    "--user daemon-reload",
    `--user enable ${SYSTEMD_SERVICE_NAME}`
  ]);
});

test("startupStatus reports disabled Linux user service", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-startup-status-"));
  const calls: Array<{ file: string; args: readonly string[] }> = [];

  const status = await startupStatus({
    bridgeHome: path.join(temp, "bridge-home"),
    platform: "linux",
    env: { HOME: temp },
    execFile: makeRunner(calls, false)
  });

  assert.equal(status.supported, true);
  assert.equal(status.installed, false);
  assert.equal(status.method, "systemd-user");
});

test("uninstallStartup removes a Linux service file even when disabled", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-startup-uninstall-"));
  const env = { HOME: temp };
  const servicePath = getSystemdServicePath({ env });
  const calls: Array<{ file: string; args: readonly string[] }> = [];

  await fs.mkdir(path.dirname(servicePath), { recursive: true });
  await fs.writeFile(servicePath, "[Unit]\nDescription=test\n", "utf8");

  const result = await uninstallStartup({
    bridgeHome: path.join(temp, "bridge-home"),
    platform: "linux",
    env,
    execFile: makeRunner(calls, false)
  });

  assert.equal(result.removed, true);
  await assert.rejects(fs.stat(servicePath), /ENOENT/);
  assert.equal(calls.at(-1)?.args.join(" "), "--user daemon-reload");
});
