import http from "node:http";
import { Readable } from "node:stream";
import { assertConfigured, loadBridgeConfig, resolveApiKey } from "./config.js";
import type { BridgeConfig } from "./types.js";

export interface ProxyOptions {
  config?: BridgeConfig;
  env?: NodeJS.ProcessEnv;
  logger?: Pick<Console, "log" | "error">;
}

function trimTrailingSlash(value: string): string {
  return value.replace(/\/+$/, "");
}

function isV1Path(incomingUrl: string): boolean {
  const parsed = new URL(incomingUrl, "http://127.0.0.1");
  return parsed.pathname === "/v1" || parsed.pathname.startsWith("/v1/");
}

export function toUpstreamUrl(incomingUrl: string, upstreamBaseUrl: string): string {
  const parsed = new URL(incomingUrl, "http://127.0.0.1");
  const suffix = parsed.pathname === "/v1"
    ? ""
    : parsed.pathname.startsWith("/v1/")
      ? parsed.pathname.slice(3)
      : parsed.pathname;
  return `${trimTrailingSlash(upstreamBaseUrl)}${suffix}${parsed.search}`;
}

function copyHeaders(headers: http.IncomingHttpHeaders, apiKey: string): Headers {
  const outgoing = new Headers();
  const blocked = new Set([
    "authorization",
    "connection",
    "content-length",
    "host",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade"
  ]);

  for (const [key, value] of Object.entries(headers)) {
    if (blocked.has(key.toLowerCase()) || value === undefined) {
      continue;
    }
    if (Array.isArray(value)) {
      for (const item of value) {
        outgoing.append(key, item);
      }
    } else {
      outgoing.set(key, value);
    }
  }

  outgoing.set("authorization", `Bearer ${apiKey}`);
  return outgoing;
}

function sendJson(response: http.ServerResponse, status: number, body: unknown): void {
  response.writeHead(status, { "content-type": "application/json; charset=utf-8" });
  response.end(`${JSON.stringify(body)}\n`);
}

function responseHeaders(upstream: Response): Record<string, string | string[]> {
  const headers: Record<string, string | string[]> = {};
  const blocked = new Set(["content-encoding", "content-length", "transfer-encoding", "connection"]);
  upstream.headers.forEach((value, key) => {
    if (!blocked.has(key.toLowerCase())) {
      headers[key] = value;
    }
  });
  return headers;
}

export function createProxyServer(options: ProxyOptions = {}): http.Server {
  const logger = options.logger ?? console;
  const env = options.env ?? process.env;

  return http.createServer(async (request, response) => {
    const start = Date.now();
    const config = options.config ?? await loadBridgeConfig();
    const method = request.method ?? "GET";
    const path = request.url ?? "/";

    if (!isV1Path(path)) {
      sendJson(response, 404, { error: "codex-provider-bridge only proxies /v1/* requests." });
      return;
    }

    const { apiKey } = resolveApiKey(config, env);
    if (!apiKey) {
      sendJson(response, 500, {
        error: `Missing API key. Run "codex-provider-bridge setup" or set ${config.apiKeyEnv}.`
      });
      return;
    }

    try {
      const target = toUpstreamUrl(path, config.upstreamBaseUrl);
      const body = method === "GET" || method === "HEAD"
        ? undefined
        : Readable.toWeb(request) as unknown as BodyInit;
      const upstream = await fetch(target, {
        method,
        headers: copyHeaders(request.headers, apiKey),
        body,
        duplex: body ? "half" : undefined
      } as RequestInit & { duplex?: "half" });

      response.writeHead(upstream.status, upstream.statusText, responseHeaders(upstream));
      if (!upstream.body) {
        response.end();
      } else {
        Readable.fromWeb(upstream.body as unknown as import("node:stream/web").ReadableStream<Uint8Array>).pipe(response);
      }

      response.on("finish", () => {
        logger.log(`${method} ${path} -> ${upstream.status} ${Date.now() - start}ms`);
      });
    } catch (error) {
      logger.error(`${method} ${path} -> proxy_error ${Date.now() - start}ms`);
      sendJson(response, 502, { error: "Failed to proxy request to upstream provider." });
    }
  });
}

export async function serve(config?: BridgeConfig): Promise<http.Server> {
  config ??= await loadBridgeConfig();
  assertConfigured(config);
  const key = resolveApiKey(config);
  if (!key.apiKey) {
    throw new Error(`Missing API key. Run "codex-provider-bridge setup" or set ${config.apiKeyEnv}.`);
  }
  const server = createProxyServer({ config });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(config.port, config.host, () => {
      server.off("error", reject);
      resolve();
    });
  });
  console.log(`codex-provider-bridge listening on http://${config.host}:${config.port}/v1`);
  console.log(`upstream: ${config.upstreamBaseUrl}`);
  console.log(`api key: ${key.source === "environment" ? `environment ${config.apiKeyEnv}` : "saved local config"}`);
  return server;
}
