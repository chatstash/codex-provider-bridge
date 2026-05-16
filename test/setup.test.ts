import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import { loadBridgeConfig } from "../src/config.js";
import { setupBridge } from "../src/setup.js";

function promptFrom(lines: string[]): () => string {
  const answers = [...lines];
  return () => answers.shift() ?? "";
}

function quietOutput(): Writable {
  return new Writable({
    write(_chunk, _encoding, callback) {
      callback();
    }
  });
}

test("setup saves API key, patches config, and creates backup", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-setup-"));
  const bridgeHome = path.join(temp, "bridge-home");
  const codexConfigPath = path.join(temp, ".codex", "config.toml");
  const original = `model_provider = "OpenAI"\n`;

  await fs.mkdir(path.dirname(codexConfigPath), { recursive: true });
  await fs.writeFile(codexConfigPath, original, "utf8");

  const result = await setupBridge({
    bridgeHome,
    codexConfigPath,
    prompt: promptFrom(["https://example.com/v1", "", "", "setup-secret-key"]),
    output: quietOutput()
  });
  const saved = await loadBridgeConfig(result.bridgeConfigPath);
  const patched = await fs.readFile(codexConfigPath, "utf8");

  assert.equal(saved.apiKey, "setup-secret-key");
  assert.equal(saved.upstreamBaseUrl, "https://example.com/v1");
  assert.equal(saved.port, 11435);
  assert.match(patched, /model_provider = "codex_provider_bridge"/);
  assert.equal(await fs.readFile(result.backupPath, "utf8"), original);
});

test("setup requires upstream on first run", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-setup-required-"));

  await assert.rejects(
    setupBridge({
      bridgeHome: path.join(temp, "bridge-home"),
      codexConfigPath: path.join(temp, ".codex", "config.toml"),
      prompt: promptFrom(["", "", "", "setup-secret-key"]),
      output: quietOutput()
    }),
    /需要上游地址/
  );
});

test("setup accepts custom upstream and port", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "codex-provider-bridge-setup-custom-"));
  const result = await setupBridge({
    bridgeHome: path.join(temp, "bridge-home"),
    codexConfigPath: path.join(temp, ".codex", "config.toml"),
    prompt: promptFrom(["https://example.com/v1/", "12345", "EXAMPLE_KEY", "custom-secret-key"]),
    output: quietOutput()
  });
  const saved = await loadBridgeConfig(result.bridgeConfigPath);

  assert.equal(saved.upstreamBaseUrl, "https://example.com/v1");
  assert.equal(saved.port, 12345);
  assert.equal(saved.apiKeyEnv, "EXAMPLE_KEY");
  assert.equal(saved.apiKey, "custom-secret-key");
});
