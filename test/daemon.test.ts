import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { daemonStatus, stopDaemon } from "../src/daemon.js";
import { getDaemonStatePath } from "../src/paths.js";

test("daemonStatus reports stale pid state", async () => {
  const bridgeHome = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-daemon-"));
  const statePath = getDaemonStatePath(bridgeHome);

  await fs.writeFile(
    statePath,
    `${JSON.stringify({
      pid: 99999999,
      startedAt: new Date().toISOString(),
      command: "node cli.js serve",
      logPath: path.join(bridgeHome, "bridge.log"),
      host: "127.0.0.1",
      port: 11435
    })}\n`,
    "utf8"
  );

  const status = await daemonStatus(bridgeHome);

  assert.equal(status.running, false);
  assert.equal(status.stale, true);
  assert.equal(status.pid, 99999999);
});

test("stopDaemon removes stale pid state", async () => {
  const bridgeHome = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-daemon-stop-"));
  const statePath = getDaemonStatePath(bridgeHome);

  await fs.writeFile(
    statePath,
    `${JSON.stringify({
      pid: 99999999,
      startedAt: new Date().toISOString(),
      command: "node cli.js serve",
      logPath: path.join(bridgeHome, "bridge.log"),
      host: "127.0.0.1",
      port: 11435
    })}\n`,
    "utf8"
  );

  const result = await stopDaemon(bridgeHome);

  assert.equal(result.stopped, false);
  assert.equal(result.wasRunning, false);
  assert.equal(result.pid, 99999999);
  await assert.rejects(fs.stat(statePath), /ENOENT/);
});
