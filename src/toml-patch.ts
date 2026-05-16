import type { BridgeConfig } from "./types.js";

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function quoteToml(value: string): string {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

function ensureTrailingNewline(content: string): string {
  return content.endsWith("\n") ? content : `${content}\n`;
}

function setTopLevelString(content: string, key: string, value: string): string {
  const lines = ensureTrailingNewline(content).split(/\r?\n/);
  const keyPattern = new RegExp(`^\\s*${escapeRegExp(key)}\\s*=`);
  let inTopLevel = true;

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^\s*\[/.test(line)) {
      inTopLevel = false;
    }
    if (inTopLevel && keyPattern.test(line)) {
      lines[index] = `${key} = ${quoteToml(value)}`;
      return lines.join("\n").replace(/\n+$/, "\n");
    }
  }

  let insertAt = 0;
  while (insertAt < lines.length && lines[insertAt].trim().startsWith("#")) {
    insertAt += 1;
  }
  lines.splice(insertAt, 0, `${key} = ${quoteToml(value)}`);
  return lines.join("\n").replace(/\n+$/, "\n");
}

function findSection(lines: string[], section: string): { start: number; end: number } | null {
  const headerPattern = new RegExp(`^\\s*\\[${escapeRegExp(section)}\\]\\s*(?:#.*)?$`);
  let start = -1;

  for (let index = 0; index < lines.length; index += 1) {
    if (headerPattern.test(lines[index])) {
      start = index;
      break;
    }
  }

  if (start === -1) {
    return null;
  }

  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    if (/^\s*\[/.test(lines[index])) {
      end = index;
      break;
    }
  }

  return { start, end };
}

function ensureSectionBooleans(content: string, section: string, values: Record<string, boolean>): string {
  const normalized = ensureTrailingNewline(content);
  const lines = normalized.split(/\r?\n/);
  if (lines.at(-1) === "") {
    lines.pop();
  }

  const existing = findSection(lines, section);
  if (!existing) {
    const sectionLines = [
      "",
      `[${section}]`,
      ...Object.entries(values).map(([key, value]) => `${key} = ${value ? "true" : "false"}`)
    ];
    return `${lines.join("\n")}${sectionLines.join("\n")}\n`;
  }

  const sectionLines = lines.slice(existing.start, existing.end);
  for (const [key, value] of Object.entries(values)) {
    const keyPattern = new RegExp(`^\\s*${escapeRegExp(key)}\\s*=`);
    const rendered = `${key} = ${value ? "true" : "false"}`;
    const matchIndex = sectionLines.findIndex((line, index) => index > 0 && keyPattern.test(line));
    if (matchIndex === -1) {
      sectionLines.push(rendered);
    } else {
      sectionLines[matchIndex] = rendered;
    }
  }

  lines.splice(existing.start, existing.end - existing.start, ...sectionLines);
  return `${lines.join("\n")}\n`;
}

function replaceOrAppendSection(content: string, section: string, bodyLines: string[]): string {
  const normalized = ensureTrailingNewline(content);
  const lines = normalized.split(/\r?\n/);
  if (lines.at(-1) === "") {
    lines.pop();
  }

  const replacement = [`[${section}]`, ...bodyLines];
  const existing = findSection(lines, section);
  if (!existing) {
    const separator = lines.length > 0 && lines.at(-1)?.trim() !== "" ? [""] : [];
    return `${[...lines, ...separator, ...replacement].join("\n")}\n`;
  }

  lines.splice(existing.start, existing.end - existing.start, ...replacement);
  return `${lines.join("\n")}\n`;
}

export function patchCodexConfig(content: string, config: BridgeConfig): string {
  let next = ensureTrailingNewline(content);
  next = setTopLevelString(next, "model_provider", config.providerId);
  next = ensureSectionBooleans(next, "features", {
    remote_control: true,
    prevent_idle_sleep: true
  });
  next = replaceOrAppendSection(next, `model_providers.${config.providerId}`, [
    `name = ${quoteToml(config.providerName)}`,
    `base_url = ${quoteToml(`http://${config.host}:${config.port}/v1`)}`,
    'wire_api = "responses"',
    "requires_openai_auth = true"
  ]);
  return next;
}

export function hasBridgeProvider(content: string, providerId: string): boolean {
  return findSection(ensureTrailingNewline(content).split(/\r?\n/), `model_providers.${providerId}`) !== null;
}

export function topLevelModelProvider(content: string): string | null {
  const lines = ensureTrailingNewline(content).split(/\r?\n/);
  for (const line of lines) {
    if (/^\s*\[/.test(line)) {
      return null;
    }
    const match = line.match(/^\s*model_provider\s*=\s*"([^"]+)"/);
    if (match) {
      return match[1];
    }
  }
  return null;
}
