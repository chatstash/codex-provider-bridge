import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";

test("package metadata does not publish a project license file or license field", async () => {
  const manifest = JSON.parse(await fs.readFile("package.json", "utf8")) as {
    license?: string;
    files?: string[];
  };

  assert.equal("license" in manifest, false);
  assert.equal(manifest.files?.includes("LICENSE"), false);
});

test("repository text does not include personal upstream defaults", async () => {
  const files = [
    "README.md",
    "package.json",
    "src/defaults.ts",
    "src/cli.ts",
    "test/config.test.ts",
    "test/proxy.test.ts",
    "test/toml-patch.test.ts"
  ];
  const text = (await Promise.all(files.map((file) => fs.readFile(file, "utf8")))).join("\n");

  assert.equal(text.includes("sub2api.fcyaxing.com"), false);
  assert.equal(text.includes("SUB2API_API_KEY"), false);
});
