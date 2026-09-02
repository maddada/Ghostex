use std::path::Path;

use super::config::{
    command_agent, HookDefinition, AMP_PLUGIN_MARKER, CAMPFIRE_EXTENSION_MARKER,
    NOTIFY_HOOK_MARKER, NOTIFY_HOOK_VERSION, OMP_EXTENSION_MARKER, OPENCODE_PLUGIN_MARKER,
    PI_EXTENSION_MARKER,
};
use super::probing::path_string;

pub(crate) fn build_notify_hook_script(executable: &str, hook_state_directory: &Path) -> String {
    format!(
        r#"#!/bin/bash
# {NOTIFY_HOOK_MARKER} v{NOTIFY_HOOK_VERSION}
if [ -n "${{1:-}}" ]; then
  INPUT_ARG="$1"
else
  INPUT_ARG=""
  IFS= read -r -t 1 INPUT_ARG || true
fi

# Codex's Interrupt hook output schema permits only `systemMessage`; the
# generic `continue` field used by the other hook events is rejected.
HOOK_RESPONSE='{{"continue":true}}'
if [[ "$INPUT_ARG" == *'"hook_event_name":"Interrupt"'* ]]; then
  HOOK_RESPONSE='{{}}'
fi
# Antigravity CLI (1.1.24+) parses any JSON a PreToolUse hook prints as a tool
# input override and fails the tool call on the unexpected field; it treats
# empty stdout as "no change", so its hooks must stay silent.
if [ "${{GHOSTEX_AGENT:-}}" = "antigravity" ]; then
  HOOK_RESPONSE=''
fi

SESSION_STATE_FILE="${{VSMUX_SESSION_STATE_FILE:-${{GHOSTEX_SESSION_STATE_FILE:-$ghostex_SESSION_STATE_FILE}}}}"
DEFAULT_HOOK_STATE_DIR={hook_state_directory}
HOOK_STATE_DIR="${{GHOSTEX_AGENT_HOOK_STATE_DIR:-$DEFAULT_HOOK_STATE_DIR}}"
if [ "${{GHOSTEX_INTERNAL_PROMPT_GENERATION:-}}" = "1" ] || [ "${{GHOSTEX_INTERNAL_TITLE_GENERATION:-}}" = "1" ]; then
  printf '%s' "$HOOK_RESPONSE"
  exit 0
fi
if [ -z "$SESSION_STATE_FILE" ] && {{ [ -z "${{GHOSTEX_GLOBAL_SESSION_REF:-}}" ] || [ -z "${{GHOSTEX_GXSERVER_BASE_URL:-}}" ] || [ -z "${{GHOSTEX_GXSERVER_AUTH_TOKEN_FILE:-}}" ]; }}; then
  printf '%s' "$HOOK_RESPONSE"
  exit 0
fi

{executable} agent-hook-notify "$SESSION_STATE_FILE" "$INPUT_ARG" "$HOOK_STATE_DIR" >/dev/null 2>/dev/null || true
printf '%s' "$HOOK_RESPONSE"
exit 0
"#,
        executable = shell_quote(executable),
        hook_state_directory = shell_quote(&path_string(hook_state_directory)),
    )
}

pub(crate) fn build_plugin_file_source(agent_id: &str, notify_hook_path: &Path) -> String {
    /*
    CDXC:AgentHooks 2026-06-21-19:26:
    Plugin-file agents must keep the same provider-specific hook scripts as TypeScript gxserver, including launch argv metadata, provider disable flags, transcript fields, and first-prompt payload capture. Shared generic hooks lose restore information and make the Rust hook installer report parity while silently weakening sleep/wake.
    */
    match agent_id {
        "amp" => build_amp_plugin_source(notify_hook_path),
        "omp" => build_omp_extension_source(notify_hook_path),
        "pi" => build_pi_extension_source(notify_hook_path),
        "campfire" => build_campfire_extension_source(notify_hook_path),
        _ => build_pi_extension_source(notify_hook_path),
    }
}

pub(crate) fn build_opencode_plugin_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const PLUGIN_INSTALLED_KEY = Symbol.for("ghostex.session.restore.plugin.installed");

function firstString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function eventProperties(event) {
  return (event && typeof event === "object" && event.properties) || {};
}

function sessionIdFor(event) {
  const props = eventProperties(event);
  return firstString(
    props.info && props.info.id,
    props.sessionID,
    props.sessionId,
    props.session_id,
    props.session && props.session.id,
    event && event.sessionID,
    event && event.sessionId,
    event && event.id
  );
}

function cwdFor(ctx, event) {
  const props = eventProperties(event);
  return firstString(
    props.info && props.info.directory,
    props.cwd,
    props.directory,
    ctx && ctx.directory,
    process.cwd()
  );
}

function resolveExecutable(name) {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeOpenCodeScript(value) {
  if (!value) return false;
  const lower = String(value).toLowerCase();
  return lower.includes("opencode") || lower.includes("open-code");
}

function isOpenCodeInternalWorkerArg(value) {
  if (!value) return false;
  const normalized = String(value).replaceAll("\\", "/");
  return normalized.includes("/$bunfs/") && normalized.includes("/src/cli/cmd/tui/worker.js");
}

function withoutOpenCodeInternalWorkerArgs(argv) {
  const result = [];
  for (let i = 0; i < argv.length; i += 1) {
    const value = argv[i];
    if (i > 0 && isOpenCodeInternalWorkerArg(value)) continue;
    result.push(value);
  }
  return result.length > 0 ? result : [resolveExecutable("opencode")];
}

function normalizedLaunchArgv() {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("opencode")];

  const firstBase = path.basename(raw[0]).toLowerCase();
  if (looksLikeOpenCodeScript(firstBase)) return withoutOpenCodeInternalWorkerArgs(raw);

  let tail = raw.slice(1);
  if (tail.length > 0 && looksLikeOpenCodeScript(tail[0])) {
    tail = tail.slice(1);
  }
  return withoutOpenCodeInternalWorkerArgs([resolveExecutable("opencode"), ...tail]);
}

function base64NulSeparated(values) {
  const bytes = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd) {
  const env = { ...process.env, GHOSTEX_AGENT: "opencode" };
  delete env.AMP_API_KEY;
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "opencode";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("opencode");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function hookEventName(subcommand) {
  switch (subcommand) {
    case "session-start":
      return "SessionStart";
    case "stop":
      return "Stop";
    case "session-end":
      return "SessionEnd";
    default:
      return subcommand;
  }
}

function sendHook(subcommand, ctx, event, extra = {}) {
  if (process.env.GHOSTEX_OPENCODE_HOOKS_DISABLED === "1") return;
  const sessionId = sessionIdFor(event);
  if (!sessionId) return;
  const cwd = cwdFor(ctx, event);
  const eventName = hookEventName(subcommand);
  const payload = {
    agent: "opencode",
    cwd,
    event: eventName,
    hook_event_name: eventName,
    session_id: sessionId,
    ...extra,
  };
  try {
    spawnSync(__NOTIFY_HOOK_PATH_JSON__, [], {
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: hookEnvironment(cwd),
      stdio: ["pipe", "ignore", "ignore"],
      timeout: 5000,
    });
  } catch (_) {}
}

function handleEvent(ctx, event) {
  const props = eventProperties(event);
  switch (event && event.type) {
    case "session.created":
      sendHook("session-start", ctx, event);
      break;
    case "session.updated":
      if (props.info && props.info.time && props.info.time.archived) {
        sendHook("session-end", ctx, event);
      } else {
        sendHook("session-start", ctx, event);
      }
      break;
    case "session.status":
      if (props.status && props.status.type === "idle") {
        sendHook("stop", ctx, event);
      } else if (props.status && props.status.type) {
        sendHook("SessionBusy", ctx, event);
      }
      break;
    case "session.idle":
      sendHook("stop", ctx, event);
      break;
    case "session.deleted":
      sendHook("session-end", ctx, event);
      break;
    case "permission.asked":
      sendHook("PermissionRequest", ctx, event);
      break;
    case "question.asked":
      sendHook("AskUserQuestion", ctx, event);
      break;
    case "permission.replied":
    case "question.replied":
    case "question.rejected":
      sendHook("SessionBusy", ctx, event);
      break;
    default:
      break;
  }
}

const GhostexSessionRestore = async (ctx) => {
  if (globalThis[PLUGIN_INSTALLED_KEY]) return {};
  globalThis[PLUGIN_INSTALLED_KEY] = true;
  const bus = ctx && (ctx.bus || ctx.events || ctx.event);
  const on = bus && typeof bus.on === "function" ? bus.on.bind(bus) : ctx && typeof ctx.on === "function" ? ctx.on.bind(ctx) : null;
  if (on) {
    for (const eventName of [
      "session.created",
      "session.updated",
      "session.status",
      "session.idle",
      "session.deleted",
      "permission.asked",
      "permission.replied",
      "question.asked",
      "question.replied",
      "question.rejected",
    ]) {
      on(eventName, (event) => handleEvent(ctx, { ...event, type: event && event.type ? event.type : eventName }));
    }
    return {};
  }

  return {
    event: async ({ event }) => {
      handleEvent(ctx, event);
    },
  };
};

export { GhostexSessionRestore };
export default GhostexSessionRestore;
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(OPENCODE_PLUGIN_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

fn build_amp_plugin_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type {
  PluginAPI,
  AgentEndEvent,
  AgentStartEvent,
  SessionStartEvent,
  ToolCallEvent,
  ToolResultEvent,
} from "@ampcode/plugin";

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function resolveExecutable(name: string): string {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeAmpExecutable(value: string): boolean {
  return path.basename(value).toLowerCase() === "amp";
}

function looksLikeAmpScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/");
  const base = path.basename(normalized).toLowerCase();
  return normalized.includes("/@ampcode/") || (base === "cli.js" && normalized.includes("amp"));
}

function looksLikeJavaScriptRuntime(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "node" || base === "bun" || base === "deno" || base === "tsx" || base === "ts-node";
}

function normalizedLaunchArgv(): string[] {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("amp")];
  if (looksLikeAmpExecutable(raw[0])) return raw;
  if (raw.length > 1 && (looksLikeAmpScript(raw[1]) || looksLikeJavaScriptRuntime(raw[0]))) {
    return [resolveExecutable("amp"), ...raw.slice(2)];
  }
  return [resolveExecutable("amp")];
}

function base64NulSeparated(values: string[]): string {
  const bytes: Buffer[] = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, GHOSTEX_AGENT: "amp" };
  delete env.AMP_API_KEY;
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "amp";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("amp");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function threadIdFrom(event: { thread?: { id?: string } } | undefined, ctx?: { thread?: { id?: string } }): string | null {
  return firstString(event?.thread?.id, ctx?.thread?.id);
}

function sendHook(
  eventName: string,
  sessionId: string | null,
  cwd: string,
  extra: Record<string, unknown> = {},
): void {
  if (process.env.GHOSTEX_AMP_HOOKS_DISABLED === "1") return;
  if (!sessionId) return;
  const payload: Record<string, unknown> = {
    agent: "amp",
    cwd,
    event: eventName,
    hook_event_name: eventName,
    session_id: sessionId,
    ...extra,
  };
  try {
    const child = spawn(__NOTIFY_HOOK_PATH_JSON__, [], {
      stdio: ["pipe", "ignore", "ignore"],
      env: hookEnvironment(cwd),
      detached: true,
    });
    child.on("error", () => {});
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(payload));
    child.unref();
  } catch (_) {}
}

export default function ghostexAmpSessionPlugin(amp: PluginAPI) {
  const cwdFromEnv = (): string => firstString(process.env.PWD, process.cwd()) || process.cwd();

  amp.on("session.start", async (event: SessionStartEvent, ctx) => {
    sendHook("SessionStart", threadIdFrom(event, ctx), cwdFromEnv());
  });

  amp.on("agent.start", async (event: AgentStartEvent, ctx) => {
    sendHook("UserPromptSubmit", threadIdFrom(event, ctx), cwdFromEnv());
  });

  amp.on("tool.call", async (event: ToolCallEvent, ctx) => {
    sendHook("PreToolUse", threadIdFrom(undefined, ctx), cwdFromEnv(), { tool: event.tool });
    return { action: "allow" as const };
  });

  amp.on("tool.result", async (event: ToolResultEvent, ctx) => {
    sendHook("PostToolUse", threadIdFrom(undefined, ctx), cwdFromEnv(), {
      tool: event.tool,
      is_error: event.status === "error",
    });
  });

  amp.on("agent.end", async (event: AgentEndEvent, ctx) => {
    sendHook("Stop", threadIdFrom(event, ctx), cwdFromEnv(), { status: event.status });
  });
}
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(AMP_PLUGIN_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

fn build_pi_extension_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type { AgentEndEvent, ExtensionAPI, ExtensionContext } from "@mariozechner/pi-coding-agent";

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function resolveExecutable(name: string): string {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikePiExecutable(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "pi" || base === "pi-coding-agent";
}

function looksLikePiScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/");
  const base = path.basename(normalized).toLowerCase();
  return (
    normalized.includes("/@mariozechner/pi-coding-agent/") ||
    normalized.includes("/packages/coding-agent/") ||
    (base === "cli.js" && normalized.includes("pi-coding-agent")) ||
    (base === "cli.ts" && normalized.includes("coding-agent"))
  );
}

function normalizedLaunchArgv(): string[] {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("pi")];
  if (looksLikePiExecutable(raw[0])) return raw;
  if (raw.length > 1 && looksLikePiScript(raw[1])) {
    return [resolveExecutable("pi"), ...raw.slice(2)];
  }
  return [resolveExecutable("pi"), ...raw.slice(1)];
}

function base64NulSeparated(values: string[]): string {
  const bytes: Buffer[] = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, GHOSTEX_AGENT: "pi" };
  delete env.AMP_API_KEY;
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "pi";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("pi");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function eventName(subcommand: string): string {
  switch (subcommand) {
    case "session-start":
      return "SessionStart";
    case "prompt-submit":
      return "UserPromptSubmit";
    case "stop":
      return "Stop";
    default:
      return subcommand;
  }
}

function textFromContent(content: unknown): string | null {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return null;
  const parts: string[] = [];
  for (const block of content) {
    if (!block || typeof block !== "object") continue;
    const typed = block as { type?: unknown; text?: unknown };
    if (typed.type === "text" && typeof typed.text === "string") parts.push(typed.text);
  }
  return parts.join("\n") || null;
}

function lastAssistantMessage(event: AgentEndEvent): string | undefined {
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index];
    if (!message || typeof message !== "object") continue;
    const typed = message as { role?: unknown; content?: unknown };
    if (typed.role !== "assistant") continue;
    const text = firstString(textFromContent(typed.content));
    if (text) return text;
  }
  return undefined;
}

const SUBAGENT_TOOL_NAMES = new Set(["subagent", "team_spawn", "superpowers_dispatch", "Task"]);

function toolNameFrom(event: unknown): string | null {
  if (!event || typeof event !== "object") return null;
  const typed = event as { toolName?: unknown; tool_name?: unknown; name?: unknown };
  return firstString(typed.toolName, typed.tool_name, typed.name);
}

function isSubagentTool(toolName: string | null): boolean {
  if (!toolName) return false;
  return SUBAGENT_TOOL_NAMES.has(toolName) || /subagent/i.test(toolName);
}

function toolIsError(event: unknown): boolean | undefined {
  if (!event || typeof event !== "object") return undefined;
  const typed = event as { isError?: unknown; is_error?: unknown; error?: unknown };
  if (typeof typed.isError === "boolean") return typed.isError;
  if (typeof typed.is_error === "boolean") return typed.is_error;
  if (typed.error !== undefined && typed.error !== null) return true;
  return undefined;
}

type OptionalEventHandler = (event: unknown, ctx: ExtensionContext) => void | Promise<void>;

function registerOptional(api: ExtensionAPI, name: string, handler: OptionalEventHandler): void {
  try {
    (api.on as unknown as (event: string, handler: OptionalEventHandler) => void)(name, handler);
  } catch (_) {}
}

function sendHook(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}): void {
  if (process.env.GHOSTEX_PI_HOOKS_DISABLED === "1") return;

  const sessionId = firstString(ctx.sessionManager.getSessionId());
  if (!sessionId) return;

  const cwd = firstString(ctx.cwd, process.cwd()) || process.cwd();
  const event = eventName(subcommand);
  const payload: Record<string, unknown> = {
    agent: "pi",
    session_id: sessionId,
    cwd,
    hook_event_name: event,
    event,
    transcript_path: ctx.sessionManager.getSessionFile() || undefined,
    ...extra,
  };
  try {
    spawnSync(__NOTIFY_HOOK_PATH_JSON__, [], {
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: hookEnvironment(cwd),
      stdio: ["pipe", "ignore", "ignore"],
      timeout: 5000,
    });
  } catch (_) {}
}

export default function ghostexPiSessionExtension(pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => {
    sendHook("session-start", ctx);
  });

  pi.on("before_agent_start", async (event, ctx) => {
    sendHook("prompt-submit", ctx, { prompt: event.prompt });
  });

  pi.on("agent_end", async (event, ctx) => {
    sendHook("stop", ctx, { last_assistant_message: lastAssistantMessage(event) });
  });

  registerOptional(pi, "tool_execution_start", (event, ctx) => {
    const toolName = toolNameFrom(event);
    sendHook(isSubagentTool(toolName) ? "SubagentStart" : "PreToolUse", ctx, {
      tool_name: toolName ?? undefined,
    });
  });

  registerOptional(pi, "tool_execution_end", (event, ctx) => {
    const toolName = toolNameFrom(event);
    sendHook(isSubagentTool(toolName) ? "SubagentStop" : "PostToolUse", ctx, {
      tool_name: toolName ?? undefined,
      is_error: toolIsError(event),
    });
  });

  registerOptional(pi, "session_before_compact", (_event, ctx) => {
    sendHook("PreCompact", ctx);
  });

  registerOptional(pi, "session_compact", (_event, ctx) => {
    sendHook("PostCompact", ctx);
  });
}
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(PI_EXTENSION_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

/*
Campfire is a white-label build of pi-coding-agent, so its extension API is
pi's, only published under `@earendil-works/pi-coding-agent`. This source is
therefore the pi extension with the package name, agent id, disable flag, and
launch metadata swapped — keep it in step with `build_pi_extension_source`.
*/
fn build_campfire_extension_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type { AgentEndEvent, ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function resolveExecutable(name: string): string {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeCampfireExecutable(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "campfire";
}

function looksLikeCampfireScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/").toLowerCase();
  const base = path.basename(normalized);
  return (
    normalized.includes("/@earendil-works/pi-coding-agent/") ||
    normalized.includes("/campfire/") ||
    ((base === "cli.js" || base === "cli.ts") && normalized.includes("coding-agent"))
  );
}

function normalizedLaunchArgv(): string[] {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("campfire")];
  if (looksLikeCampfireExecutable(raw[0])) return raw;
  if (raw.length > 1 && looksLikeCampfireScript(raw[1])) {
    return [resolveExecutable("campfire"), ...raw.slice(2)];
  }
  return [resolveExecutable("campfire"), ...raw.slice(1)];
}

function base64NulSeparated(values: string[]): string {
  const bytes: Buffer[] = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, GHOSTEX_AGENT: "campfire" };
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "campfire";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("campfire");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function eventName(subcommand: string): string {
  switch (subcommand) {
    case "session-start":
      return "SessionStart";
    case "prompt-submit":
      return "UserPromptSubmit";
    case "stop":
      return "Stop";
    default:
      return subcommand;
  }
}

function textFromContent(content: unknown): string | null {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return null;
  const parts: string[] = [];
  for (const block of content) {
    if (!block || typeof block !== "object") continue;
    const typed = block as { type?: unknown; text?: unknown };
    if (typed.type === "text" && typeof typed.text === "string") parts.push(typed.text);
  }
  return parts.join("\n") || null;
}

function lastAssistantMessage(event: AgentEndEvent): string | undefined {
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index];
    if (!message || typeof message !== "object") continue;
    const typed = message as { role?: unknown; content?: unknown };
    if (typed.role !== "assistant") continue;
    const text = firstString(textFromContent(typed.content));
    if (text) return text;
  }
  return undefined;
}

const SUBAGENT_TOOL_NAMES = new Set(["subagent", "team_spawn", "superpowers_dispatch", "Task"]);

function toolNameFrom(event: unknown): string | null {
  if (!event || typeof event !== "object") return null;
  const typed = event as { toolName?: unknown; tool_name?: unknown; name?: unknown };
  return firstString(typed.toolName, typed.tool_name, typed.name);
}

function isSubagentTool(toolName: string | null): boolean {
  if (!toolName) return false;
  return SUBAGENT_TOOL_NAMES.has(toolName) || /subagent/i.test(toolName);
}

function toolIsError(event: unknown): boolean | undefined {
  if (!event || typeof event !== "object") return undefined;
  const typed = event as { isError?: unknown; is_error?: unknown; error?: unknown };
  if (typeof typed.isError === "boolean") return typed.isError;
  if (typeof typed.is_error === "boolean") return typed.is_error;
  if (typed.error !== undefined && typed.error !== null) return true;
  return undefined;
}

type OptionalEventHandler = (event: unknown, ctx: ExtensionContext) => void | Promise<void>;

function registerOptional(api: ExtensionAPI, name: string, handler: OptionalEventHandler): void {
  try {
    (api.on as unknown as (event: string, handler: OptionalEventHandler) => void)(name, handler);
  } catch (_) {}
}

function sendHook(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}): void {
  if (process.env.GHOSTEX_CAMPFIRE_HOOKS_DISABLED === "1") return;

  const sessionId = firstString(ctx.sessionManager.getSessionId());
  if (!sessionId) return;

  const cwd = firstString(ctx.cwd, process.cwd()) || process.cwd();
  const event = eventName(subcommand);
  const payload: Record<string, unknown> = {
    agent: "campfire",
    session_id: sessionId,
    cwd,
    hook_event_name: event,
    event,
    transcript_path: ctx.sessionManager.getSessionFile() || undefined,
    ...extra,
  };
  try {
    spawnSync(__NOTIFY_HOOK_PATH_JSON__, [], {
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: hookEnvironment(cwd),
      stdio: ["pipe", "ignore", "ignore"],
      timeout: 5000,
    });
  } catch (_) {}
}

export default function ghostexCampfireSessionExtension(campfire: ExtensionAPI) {
  campfire.on("session_start", async (_event, ctx) => {
    sendHook("session-start", ctx);
  });

  campfire.on("before_agent_start", async (event, ctx) => {
    sendHook("prompt-submit", ctx, { prompt: event.prompt });
  });

  campfire.on("agent_end", async (event, ctx) => {
    sendHook("stop", ctx, { last_assistant_message: lastAssistantMessage(event) });
  });

  registerOptional(campfire, "tool_execution_start", (event, ctx) => {
    const toolName = toolNameFrom(event);
    sendHook(isSubagentTool(toolName) ? "SubagentStart" : "PreToolUse", ctx, {
      tool_name: toolName ?? undefined,
    });
  });

  registerOptional(campfire, "tool_execution_end", (event, ctx) => {
    const toolName = toolNameFrom(event);
    sendHook(isSubagentTool(toolName) ? "SubagentStop" : "PostToolUse", ctx, {
      tool_name: toolName ?? undefined,
      is_error: toolIsError(event),
    });
  });

  registerOptional(campfire, "session_before_compact", (_event, ctx) => {
    sendHook("PreCompact", ctx);
  });

  registerOptional(campfire, "session_compact", (_event, ctx) => {
    sendHook("PostCompact", ctx);
  });
}
"###;
    source
        .replace(
            "__MARKER__",
            &current_plugin_marker(CAMPFIRE_EXTENSION_MARKER),
        )
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

fn build_omp_extension_source(notify_hook_path: &Path) -> String {
    let notify = path_string(notify_hook_path);
    let notify_json = serde_json::to_string(&notify).unwrap_or_else(|_| "\"\"".to_string());
    let source = r###"// __MARKER__
import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type { AgentEndEvent, ExtensionAPI, ExtensionContext } from "@oh-my-pi/pi-coding-agent";

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim().length > 0) return value.trim();
  }
  return null;
}

function resolveExecutable(name: string): string {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, name);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      if (fs.statSync(candidate).isFile()) return candidate;
    } catch (_) {}
  }
  return name;
}

function looksLikeOmpExecutable(value: string): boolean {
  return path.basename(value).toLowerCase() === "omp";
}

function looksLikeOmpScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/").toLowerCase();
  const base = path.basename(normalized);
  return (
    normalized.includes("/@oh-my-pi/pi-coding-agent/") ||
    normalized.includes("/oh-my-pi/") ||
    ((base === "cli.js" || base === "cli.ts") && normalized.includes("pi-coding-agent"))
  );
}

function looksLikeJavaScriptRuntime(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "node" || base === "bun" || base === "deno" || base === "tsx" || base === "ts-node";
}

function normalizedLaunchArgv(): string[] {
  const raw = Array.isArray(process.argv) ? process.argv.map((value) => String(value)) : [];
  if (raw.length === 0) return [resolveExecutable("omp")];
  if (looksLikeOmpExecutable(raw[0])) return raw;
  if (raw.length > 1 && (looksLikeOmpScript(raw[1]) || looksLikeJavaScriptRuntime(raw[0]))) {
    return [resolveExecutable("omp"), ...raw.slice(2)];
  }
  return [resolveExecutable("omp"), ...raw.slice(1)];
}

function base64NulSeparated(values: string[]): string {
  const bytes: Buffer[] = [];
  for (const value of values) {
    bytes.push(Buffer.from(String(value), "utf8"));
    bytes.push(Buffer.from([0]));
  }
  return Buffer.concat(bytes).toString("base64");
}

function hookEnvironment(cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, GHOSTEX_AGENT: "omp" };
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) delete env[key];
  if (!env.GHOSTEX_AGENT_LAUNCH_ARGV_B64) {
    const argv = normalizedLaunchArgv();
    env.GHOSTEX_AGENT_LAUNCH_KIND = "omp";
    env.GHOSTEX_AGENT_LAUNCH_EXECUTABLE = argv[0] || resolveExecutable("omp");
    env.GHOSTEX_AGENT_LAUNCH_ARGV_B64 = base64NulSeparated(argv);
    env.GHOSTEX_AGENT_LAUNCH_CWD = cwd || process.cwd();
  }
  return env;
}

function eventName(subcommand: string): string {
  switch (subcommand) {
    case "session-start":
      return "SessionStart";
    case "prompt-submit":
      return "UserPromptSubmit";
    case "stop":
      return "Stop";
    default:
      return subcommand;
  }
}

function textFromContent(content: unknown): string | null {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return null;
  const parts: string[] = [];
  for (const block of content) {
    if (!block || typeof block !== "object") continue;
    const typed = block as { type?: unknown; text?: unknown };
    if (typed.type === "text" && typeof typed.text === "string") parts.push(typed.text);
  }
  return parts.join("\n") || null;
}

function lastAssistantMessage(event: AgentEndEvent): string | undefined {
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index];
    if (!message || typeof message !== "object") continue;
    const typed = message as { role?: unknown; content?: unknown };
    if (typed.role !== "assistant") continue;
    const text = firstString(textFromContent(typed.content));
    if (text) return text;
  }
  return undefined;
}

const SUBAGENT_TOOL_NAMES = new Set(["subagent", "team_spawn", "superpowers_dispatch", "Task"]);

function toolNameFrom(event: unknown): string | null {
  if (!event || typeof event !== "object") return null;
  const typed = event as { toolName?: unknown; tool_name?: unknown; name?: unknown };
  return firstString(typed.toolName, typed.tool_name, typed.name);
}

function isSubagentTool(toolName: string | null): boolean {
  if (!toolName) return false;
  return SUBAGENT_TOOL_NAMES.has(toolName) || /subagent/i.test(toolName);
}

function toolIsError(event: unknown): boolean | undefined {
  if (!event || typeof event !== "object") return undefined;
  const typed = event as { isError?: unknown; is_error?: unknown; error?: unknown };
  if (typeof typed.isError === "boolean") return typed.isError;
  if (typeof typed.is_error === "boolean") return typed.is_error;
  if (typed.error !== undefined && typed.error !== null) return true;
  return undefined;
}

type OptionalEventHandler = (event: unknown, ctx: ExtensionContext) => void | Promise<void>;

function registerOptional(api: ExtensionAPI, name: string, handler: OptionalEventHandler): void {
  try {
    (api.on as unknown as (event: string, handler: OptionalEventHandler) => void)(name, handler);
  } catch (_) {}
}

function hookInvocation(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}) {
  if (process.env.GHOSTEX_OMP_HOOKS_DISABLED === "1") return null;

  const sessionId = firstString(ctx.sessionManager.getSessionId());
  if (!sessionId) return null;

  const cwd = firstString(ctx.cwd, process.cwd()) || process.cwd();
  const event = eventName(subcommand);
  const payload: Record<string, unknown> = {
    agent: "omp",
    session_id: sessionId,
    cwd,
    hook_event_name: event,
    event,
    transcript_path: ctx.sessionManager.getSessionFile() || undefined,
    ...extra,
  };
  return {
    cwd,
    payload: JSON.stringify(payload),
    env: hookEnvironment(cwd),
  };
}

async function sendHook(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}): Promise<void> {
  const invocation = hookInvocation(subcommand, ctx, extra);
  if (!invocation) return;
  await new Promise<void>((resolve) => {
    let settled = false;
    const settle = () => {
      if (settled) return;
      settled = true;
      resolve();
    };
    try {
      const child = spawn(__NOTIFY_HOOK_PATH_JSON__, [], {
        env: invocation.env,
        stdio: ["pipe", "ignore", "ignore"],
        detached: true,
      });
      child.on("error", settle);
      child.stdin.on("error", settle);
      child.stdin.on("finish", settle);
      child.unref();
      child.stdin.end(invocation.payload);
    } catch (_) {
      settle();
    }
  });
}

export default function ghostexOmpSessionExtension(api: ExtensionAPI) {
  api.on("session_start", async (_event, ctx) => {
    await sendHook("session-start", ctx);
  });

  api.on("before_agent_start", async (event, ctx) => {
    await sendHook("prompt-submit", ctx, { prompt: event.prompt });
  });

  api.on("agent_end", async (event, ctx) => {
    await sendHook("stop", ctx, { last_assistant_message: lastAssistantMessage(event) });
  });

  registerOptional(api, "tool_execution_start", async (event, ctx) => {
    const toolName = toolNameFrom(event);
    await sendHook(isSubagentTool(toolName) ? "SubagentStart" : "PreToolUse", ctx, {
      tool_name: toolName ?? undefined,
    });
  });

  registerOptional(api, "tool_execution_end", async (event, ctx) => {
    const toolName = toolNameFrom(event);
    await sendHook(isSubagentTool(toolName) ? "SubagentStop" : "PostToolUse", ctx, {
      tool_name: toolName ?? undefined,
      is_error: toolIsError(event),
    });
  });

  registerOptional(api, "session_before_compact", async (_event, ctx) => {
    await sendHook("PreCompact", ctx);
  });

  registerOptional(api, "session_compact", async (_event, ctx) => {
    await sendHook("PostCompact", ctx);
  });
}
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(OMP_EXTENSION_MARKER))
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

pub(crate) fn current_plugin_marker(marker: &str) -> String {
    if matches!(
        marker,
        OPENCODE_PLUGIN_MARKER | AMP_PLUGIN_MARKER | PI_EXTENSION_MARKER
    ) {
        format!("{marker} v4")
    } else if marker == CAMPFIRE_EXTENSION_MARKER {
        format!("{marker} v1")
    } else if marker == OMP_EXTENSION_MARKER {
        format!("{marker} v2")
    } else {
        format!("{marker} v2")
    }
}

pub(crate) fn command_for_agent(definition: &HookDefinition, notify_hook_path: &Path) -> String {
    let notify_hook_path = path_string(notify_hook_path);
    match command_agent(definition.agent_id) {
        Some(agent) => format!(
            "GHOSTEX_AGENT={} {}",
            shell_quote(agent),
            shell_quote(&notify_hook_path)
        ),
        None => shell_quote(&notify_hook_path),
    }
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn yaml_double_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

/// A TOML basic string: backslash and quote escaped, plus the control
/// characters TOML requires an escape sequence for.
pub(crate) fn toml_basic_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}
