import test from "node:test";
import assert from "node:assert/strict";
import { defaultConfig } from "../src/paths.js";
import { hasBridgeProvider, patchCodexConfig, topLevelModelProvider } from "../src/toml-patch.js";

test("patchCodexConfig adds bridge provider and features while preserving OpenAI provider", () => {
  const input = `forced_login_method = "chatgpt"
model_provider = "OpenAI"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://example.com/v1"
wire_api = "responses"
requires_openai_auth = false
`;

  const output = patchCodexConfig(input, defaultConfig());

  assert.match(output, /model_provider = "codex_provider_bridge"/);
  assert.match(output, /\[features\]\nremote_control = true\nprevent_idle_sleep = true/);
  assert.match(output, /\[model_providers\.OpenAI\]/);
  assert.match(output, /\[model_providers\.codex_provider_bridge\]/);
  assert.match(output, /base_url = "http:\/\/127\.0\.0\.1:11435\/v1"/);
  assert.match(output, /requires_openai_auth = true/);
  assert.equal(topLevelModelProvider(output), "codex_provider_bridge");
  assert.equal(hasBridgeProvider(output, "codex_provider_bridge"), true);
});

test("patchCodexConfig is idempotent for bridge section", () => {
  const once = patchCodexConfig("", defaultConfig());
  const twice = patchCodexConfig(once, defaultConfig());

  assert.equal(twice.match(/\[model_providers\.codex_provider_bridge\]/g)?.length, 1);
  assert.equal(twice.match(/\[features\]/g)?.length, 1);
  assert.equal(topLevelModelProvider(twice), "codex_provider_bridge");
});

test("patchCodexConfig updates existing features", () => {
  const output = patchCodexConfig(`[features]
remote_control = false
other = true
`, defaultConfig());

  assert.match(output, /\[features\]\nremote_control = true\nother = true\nprevent_idle_sleep = true/);
});
