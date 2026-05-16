import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { doctor, formatDoctorReport } from "../src/doctor.js";
import { saveBridgeConfig } from "../src/config.js";
import { defaultConfig } from "../src/paths.js";
import { patchCodexConfig } from "../src/toml-patch.js";

test("doctor and formatted report redact saved API key", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-doctor-"));
  const bridgeHome = path.join(temp, "bridge-home");
  const codexHome = path.join(temp, "codex-home");
  const config = { ...defaultConfig(), apiKey: "doctor-secret-key" };

  await saveBridgeConfig(config, path.join(bridgeHome, "config.json"));
  await fs.mkdir(codexHome, { recursive: true });
  await fs.writeFile(path.join(codexHome, "config.toml"), patchCodexConfig("", config), "utf8");

  const previousBridgeHome = process.env.CODEX_PROVIDER_BRIDGE_HOME;
  const previousCodexHome = process.env.CODEX_HOME;
  const previousApiKey = process.env.SUB2API_API_KEY;
  process.env.CODEX_PROVIDER_BRIDGE_HOME = bridgeHome;
  process.env.CODEX_HOME = codexHome;
  delete process.env.SUB2API_API_KEY;

  try {
    const result = await doctor();
    const json = JSON.stringify(result);
    const report = formatDoctorReport(result);

    assert.equal(result.apiKeyPresent, true);
    assert.equal(result.apiKeySource, "config");
    assert.equal(result.config.apiKeySaved, true);
    assert.equal(json.includes("doctor-secret-key"), false);
    assert.equal(report.includes("doctor-secret-key"), false);
    assert.match(report, /已保存到本机配置/);
  } finally {
    if (previousBridgeHome === undefined) {
      delete process.env.CODEX_PROVIDER_BRIDGE_HOME;
    } else {
      process.env.CODEX_PROVIDER_BRIDGE_HOME = previousBridgeHome;
    }
    if (previousCodexHome === undefined) {
      delete process.env.CODEX_HOME;
    } else {
      process.env.CODEX_HOME = previousCodexHome;
    }
    if (previousApiKey === undefined) {
      delete process.env.SUB2API_API_KEY;
    } else {
      process.env.SUB2API_API_KEY = previousApiKey;
    }
  }
});
