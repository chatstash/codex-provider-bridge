import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { getBridgeHome, getCodexConfigPath } from "../src/paths.js";

test("resolves bridge home from USERPROFILE", () => {
  assert.equal(
    getBridgeHome({ env: { USERPROFILE: "C:\\Users\\tester" } }),
    path.join("C:\\Users\\tester", ".codex-provider-bridge")
  );
});

test("resolves codex config from CODEX_HOME", () => {
  const codexHome = path.resolve("/tmp/codex-home");
  assert.equal(
    getCodexConfigPath({ env: { CODEX_HOME: codexHome, HOME: "/home/tester" } }),
    path.join(codexHome, "config.toml")
  );
});
