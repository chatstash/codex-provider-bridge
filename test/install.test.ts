import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { installBridge, restoreBridge } from "../src/install.js";
import { defaultConfig } from "../src/paths.js";

test("install backs up, patches, and restore recovers config", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-"));
  const bridgeHome = path.join(temp, "bridge-home");
  const codexConfigPath = path.join(temp, ".codex", "config.toml");
  const original = `forced_login_method = "chatgpt"
model_provider = "OpenAI"

[model_providers.OpenAI]
name = "OpenAI"
`;

  await fs.mkdir(path.dirname(codexConfigPath), { recursive: true });
  await fs.writeFile(codexConfigPath, original, "utf8");

  const install = await installBridge({
    bridgeHome,
    codexConfigPath,
    config: { ...defaultConfig(), upstreamBaseUrl: "https://example.com/v1" }
  });
  const patched = await fs.readFile(codexConfigPath, "utf8");

  assert.match(patched, /model_provider = "codex_provider_bridge"/);
  assert.match(patched, /\[model_providers\.OpenAI\]/);
  assert.equal(await fs.readFile(install.backupPath, "utf8"), original);

  await restoreBridge({ bridgeHome, codexConfigPath });
  assert.equal(await fs.readFile(codexConfigPath, "utf8"), original);
});
