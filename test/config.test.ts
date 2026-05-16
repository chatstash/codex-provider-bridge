import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { loadBridgeConfig, mergeConfig, publicBridgeConfig, resolveApiKey, saveBridgeConfig } from "../src/config.js";
import { defaultConfig } from "../src/paths.js";

test("saves and loads API key from local bridge config", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-config-"));
  const configPath = path.join(temp, "config.json");
  const config = { ...defaultConfig(), apiKey: "saved-test-key" };

  await saveBridgeConfig(config, configPath);

  assert.deepEqual(await loadBridgeConfig(configPath), config);
});

test("environment API key overrides saved config key", () => {
  const config = { ...defaultConfig(), apiKey: "saved-test-key" };
  const resolved = resolveApiKey(config, { OPENAI_COMPAT_API_KEY: "env-test-key" });

  assert.equal(resolved.apiKey, "env-test-key");
  assert.equal(resolved.source, "environment");
});

test("default config uses neutral API key env and no personal upstream", () => {
  const config = defaultConfig();

  assert.equal(config.apiKeyEnv, "OPENAI_COMPAT_API_KEY");
  assert.equal(config.upstreamBaseUrl, "");
});

test("public config redacts API key", () => {
  const safe = publicBridgeConfig({ ...defaultConfig(), apiKey: "secret-test-key" });

  assert.equal("apiKey" in safe, false);
  assert.equal(safe.apiKeySaved, true);
  assert.equal(JSON.stringify(safe).includes("secret-test-key"), false);
});

test("mergeConfig preserves saved values when partial omits keys", () => {
  const merged = mergeConfig({
    upstreamBaseUrl: "https://example.com/v1",
    port: 12345,
    apiKey: "saved-test-key"
  });

  assert.equal(merged.upstreamBaseUrl, "https://example.com/v1");
  assert.equal(merged.port, 12345);
  assert.equal(merged.apiKey, "saved-test-key");
});
