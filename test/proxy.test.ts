import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { once } from "node:events";
import type { AddressInfo } from "node:net";
import { createProxyServer, toUpstreamUrl } from "../src/proxy.js";
import { defaultConfig } from "../src/paths.js";

async function listen(server: http.Server): Promise<number> {
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address() as AddressInfo | null;
  assert.ok(address);
  return address.port;
}

async function close(server: http.Server): Promise<void> {
  if (!server.listening) {
    return;
  }
  server.close();
  await once(server, "close");
}

test("toUpstreamUrl maps /v1 paths onto upstream base", () => {
  assert.equal(
    toUpstreamUrl("/v1/responses?stream=true", "https://example.com/v1"),
    "https://example.com/v1/responses?stream=true"
  );
  assert.equal(
    toUpstreamUrl("/v1", "https://example.com/v1/"),
    "https://example.com/v1"
  );
});

test("proxy replaces authorization and forwards JSON response", async () => {
  let seenAuth = "";
  let seenPath = "";

  const upstream = http.createServer((request, response) => {
    seenAuth = request.headers.authorization ?? "";
    seenPath = request.url ?? "";
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ ok: true }));
  });
  const upstreamPort = await listen(upstream);

  const proxy = createProxyServer({
    config: {
      ...defaultConfig(),
      upstreamBaseUrl: `http://127.0.0.1:${upstreamPort}/v1`
    },
    env: { SUB2API_API_KEY: "sub2api-test-key" },
    logger: { log() {}, error() {} }
  });
  const proxyPort = await listen(proxy);

  const response = await fetch(`http://127.0.0.1:${proxyPort}/v1/responses`, {
    method: "POST",
    headers: {
      authorization: "Bearer chatgpt-token",
      "content-type": "application/json"
    },
    body: JSON.stringify({ model: "gpt-test" })
  });

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { ok: true });
  assert.equal(seenAuth, "Bearer sub2api-test-key");
  assert.equal(seenPath, "/v1/responses");

  await close(proxy);
  await close(upstream);
});

test("proxy streams SSE chunks without buffering entire response", async () => {
  const upstream = http.createServer((request, response) => {
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.write("data: one\n\n");
    setTimeout(() => response.end("data: two\n\n"), 40);
  });
  const upstreamPort = await listen(upstream);

  const proxy = createProxyServer({
    config: {
      ...defaultConfig(),
      upstreamBaseUrl: `http://127.0.0.1:${upstreamPort}/v1`
    },
    env: { SUB2API_API_KEY: "sub2api-test-key" },
    logger: { log() {}, error() {} }
  });
  const proxyPort = await listen(proxy);

  const response = await fetch(`http://127.0.0.1:${proxyPort}/v1/responses`);
  assert.equal(response.status, 200);
  assert.ok(response.body);

  const reader = response.body.getReader();
  const first = await reader.read();
  assert.equal(new TextDecoder().decode(first.value), "data: one\n\n");
  const second = await reader.read();
  assert.equal(new TextDecoder().decode(second.value), "data: two\n\n");

  await close(proxy);
  await close(upstream);
});

test("proxy reports missing API key", async () => {
  const proxy = createProxyServer({
    config: defaultConfig(),
    env: {},
    logger: { log() {}, error() {} }
  });
  const proxyPort = await listen(proxy);
  const response = await fetch(`http://127.0.0.1:${proxyPort}/v1/responses`);

  assert.equal(response.status, 500);
  assert.match(await response.text(), /Missing API key/);

  await close(proxy);
});

test("proxy uses API key saved in config when env is absent", async () => {
  let seenAuth = "";

  const upstream = http.createServer((request, response) => {
    seenAuth = request.headers.authorization ?? "";
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ ok: true }));
  });
  const upstreamPort = await listen(upstream);

  const proxy = createProxyServer({
    config: {
      ...defaultConfig(),
      upstreamBaseUrl: `http://127.0.0.1:${upstreamPort}/v1`,
      apiKey: "saved-config-key"
    },
    env: {},
    logger: { log() {}, error() {} }
  });
  const proxyPort = await listen(proxy);

  const response = await fetch(`http://127.0.0.1:${proxyPort}/v1/responses`);

  assert.equal(response.status, 200);
  assert.equal(seenAuth, "Bearer saved-config-key");

  await close(proxy);
  await close(upstream);
});

test("proxy rejects non-v1 paths", async () => {
  const proxy = createProxyServer({
    config: defaultConfig(),
    env: { SUB2API_API_KEY: "sub2api-test-key" },
    logger: { log() {}, error() {} }
  });
  const proxyPort = await listen(proxy);
  const response = await fetch(`http://127.0.0.1:${proxyPort}/v10/responses`);

  assert.equal(response.status, 404);

  await close(proxy);
});
