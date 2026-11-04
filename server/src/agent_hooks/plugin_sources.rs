use std::path::Path;

use super::chat_channel_source::chat_channel_source;
#[cfg(not(windows))]
use super::config::command_agent;
use super::config::{
    HookDefinition, AMP_PLUGIN_MARKER, NOTIFY_HOOK_MARKER, NOTIFY_HOOK_VERSION,
    OMP_EXTENSION_MARKER, PI_EXTENSION_MARKER,
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
# Empryo hooks run without GHOSTEX_* variables, and Hermes 0.22 runs its hooks in a shared gateway
# whose variables belong to whichever terminal started it; the helper finds their session itself.
if [ -z "$SESSION_STATE_FILE" ] && [ "${{GHOSTEX_AGENT:-}}" != "empryo" ] && [ "${{GHOSTEX_AGENT:-}}" != "hermes-agent" ] && {{ [ -z "${{GHOSTEX_GLOBAL_SESSION_REF:-}}" ] || [ -z "${{GHOSTEX_GXSERVER_BASE_URL:-}}" ] || [ -z "${{GHOSTEX_GXSERVER_AUTH_TOKEN_FILE:-}}" ]; }}; then
  printf '%s' "$HOOK_RESPONSE"
  exit 0
fi

# The helper answers with hook context only for a ZCode coordinator's
# SessionStart; an empty answer keeps the canned response every agent expects.
HELPER_RESPONSE=$({executable} agent-hook-notify "$SESSION_STATE_FILE" "$INPUT_ARG" "$HOOK_STATE_DIR" 2>/dev/null || true)
if [ -n "$HELPER_RESPONSE" ]; then
  printf '%s' "$HELPER_RESPONSE"
else
  printf '%s' "$HOOK_RESPONSE"
fi
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
        _ => build_pi_extension_source(notify_hook_path),
    }
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
  // Windows cannot run the notify hook script itself; gxserver runs it for the agent there.
  const [command, args] =
    process.platform === "win32"
      ? [__GXSERVER_PATH_JSON__, ["agent-hook-notify-native", __NOTIFY_HOOK_PATH_JSON__, "amp"]]
      : [__NOTIFY_HOOK_PATH_JSON__, []];
  try {
    const child = spawn(command, args, {
      stdio: ["pipe", "ignore", "ignore"],
      env: hookEnvironment(cwd),
      detached: true,
      windowsHide: true,
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

  // Only a `done` turn finished; a cancel (Esc) or an error ends it without finishing it.
  amp.on("agent.end", async (event: AgentEndEvent, ctx) => {
    const name = event.status === "cancelled" ? "Interrupt" : event.status === "error" ? "StopFailure" : "Stop";
    sendHook(name, threadIdFrom(event, ctx), cwdFromEnv(), { status: event.status });
  });
}
"###;
    source
        .replace("__MARKER__", &current_plugin_marker(AMP_PLUGIN_MARKER))
        .replace("__GXSERVER_PATH_JSON__", &gxserver_path_json())
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

/// CDXC:AgentHooks 2026-10-09 WHY:
/// Native Windows cannot spawn the notify hook script, so the Pi, OMP and Amp extensions hand it to this gxserver's `agent-hook-notify-native`. Other platforms invoke the script directly and must not embed an unused executable path: moving gxserver would otherwise make source inspection report a working plugin as stale.
fn gxserver_path_json() -> String {
    #[cfg(windows)]
    {
        let gxserver = std::env::current_exe()
            .map(|path| path_string(&path))
            .unwrap_or_default();
        serde_json::to_string(&gxserver).unwrap_or_else(|_| "\"\"".to_string())
    }
    #[cfg(not(windows))]
    {
        "\"\"".to_string()
    }
}

fn build_pi_extension_source(notify_hook_path: &Path) -> String {
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

function looksLikePiExecutable(value: string): boolean {
  const base = path.basename(value).toLowerCase();
  return base === "pi" || base === "pi-coding-agent";
}

function looksLikePiScript(value: string): boolean {
  const normalized = value.replaceAll("\\", "/");
  const base = path.basename(normalized).toLowerCase();
  return (
    normalized.includes("/@earendil-works/pi-coding-agent/") ||
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

type TurnEnd = { event: string; extra: Record<string, unknown> };

// Esc ends a run as `aborted` and a provider failure as `error`; neither is a finished turn.
function turnEndFor(event: AgentEndEvent): TurnEnd {
  let stopReason: unknown;
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index] as { role?: unknown; stopReason?: unknown } | undefined;
    if (message && message.role === "assistant") {
      stopReason = message.stopReason;
      break;
    }
  }
  const name = stopReason === "aborted" ? "Interrupt" : stopReason === "error" ? "StopFailure" : "Stop";
  return { event: name, extra: { last_assistant_message: lastAssistantMessage(event) } };
}

function isIdle(ctx: ExtensionContext): boolean {
  try {
    const typed = ctx as unknown as { isIdle?: () => boolean };
    return typeof typed.isIdle === "function" ? typed.isIdle() : false;
  } catch (_) {
    return false;
  }
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
  // Windows cannot run the notify hook script itself; gxserver runs it for the agent there.
  const [command, args] =
    process.platform === "win32"
      ? [__GXSERVER_PATH_JSON__, ["agent-hook-notify-native", __NOTIFY_HOOK_PATH_JSON__, "pi"]]
      : [__NOTIFY_HOOK_PATH_JSON__, []];
  try {
    spawnSync(command, args, {
      input: JSON.stringify(payload),
      encoding: "utf8",
      env: hookEnvironment(cwd),
      stdio: ["pipe", "ignore", "ignore"],
      timeout: 5000,
      windowsHide: true,
    });
  } catch (_) {}
}

__CHAT_CHANNEL__
export default function ghostexPiSessionExtension(pi: ExtensionAPI) {
  const chat = new GhostexChatChannel(pi);
  // `agent_end` can be followed by an automatic retry or queued follow-ups; Pi 1.0 marks the real
  // end of the work with `agent_settled`. Older Pi never fires it, so the turn ends on `agent_end`
  // until the first `agent_settled` proves the event exists, then waits for it.
  let settledSeen = false;
  let pendingTurnEnd: TurnEnd | null = null;
  let promptOpen = false;

  pi.on("session_start", async (_event, ctx) => {
    sendHook("session-start", ctx);
    chat.start(ctx);
  });

  registerOptional(pi, "session_shutdown", () => {
    chat.stop();
  });

  try {
    (pi.on as unknown as (event: string, handler: (event: unknown) => unknown) => void)("project_trust", (event) =>
      chat.trust((event as { cwd?: unknown } | undefined)?.cwd),
    );
  } catch (_) {}

  registerOptional(pi, "input", (event) => {
    chat.noteInput(event);
  });

  pi.on("before_agent_start", async (event, ctx) => {
    sendHook("prompt-submit", ctx, { prompt: event.prompt });
  });

  registerOptional(pi, "agent_start", () => {
    pendingTurnEnd = null;
    chat.noteStart();
  });

  pi.on("agent_end", async (event, ctx) => {
    const turnEnd = turnEndFor(event);
    if (settledSeen) {
      pendingTurnEnd = turnEnd;
      return;
    }
    sendHook(turnEnd.event, ctx, turnEnd.extra);
  });

  registerOptional(pi, "agent_settled", (_event, ctx) => {
    const firstSettle = !settledSeen;
    settledSeen = true;
    const turnEnd = pendingTurnEnd;
    pendingTurnEnd = null;
    if (turnEnd && !firstSettle) sendHook(turnEnd.event, ctx, turnEnd.extra);
  });

  // A dialog an extension opens mid-run (a question, a confirmation) blocks Pi on the user.
  registerOptional(pi, "ui_prompt_start", (event, ctx) => {
    if (isIdle(ctx)) return;
    promptOpen = true;
    const typed = event as { title?: unknown; kind?: unknown };
    sendHook("Notification", ctx, {
      message: firstString(typed.title) ?? "Pi is waiting for your answer",
      notification_type: firstString(typed.kind) ?? undefined,
    });
  });

  registerOptional(pi, "ui_prompt_end", (_event, ctx) => {
    if (!promptOpen) return;
    promptOpen = false;
    sendHook(isIdle(ctx) ? "SessionIdle" : "PostToolUse", ctx);
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
        .replace("__CHAT_CHANNEL__", &chat_channel_source("pi", "Pi"))
        .replace("__MARKER__", &current_plugin_marker(PI_EXTENSION_MARKER))
        .replace("__GXSERVER_PATH_JSON__", &gxserver_path_json())
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

// OMP spawns its subagents with `task`.
const SUBAGENT_TOOL_NAMES = new Set(["subagent", "team_spawn", "superpowers_dispatch", "Task", "task"]);

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

// OMP's `ask` dialog blocks the run on the user. gxserver stores its question card from the
// pre-tool event's input (derive_session_chat_prompt in server/src/session_chat_interactive.rs),
// so only that tool forwards its input, cut to the fields the card is derived from and to the
// value types it reads. OMP reports a call whose arguments failed its schema too, so a value of
// any other type is dropped rather than forwarded.
function askText(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function askToolInput(toolName: string | null, args: unknown): Record<string, unknown> | undefined {
  if (toolName !== "ask" || !args || typeof args !== "object") return undefined;
  const raw = (args as { questions?: unknown }).questions;
  if (!Array.isArray(raw)) return undefined;
  const questions: Record<string, unknown>[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const typed = entry as { question?: unknown; header?: unknown; options?: unknown; multi?: unknown; recommended?: unknown };
    const options: Record<string, unknown>[] = [];
    for (const option of Array.isArray(typed.options) ? typed.options : []) {
      if (!option || typeof option !== "object") continue;
      const item = option as { label?: unknown; description?: unknown; preview?: unknown };
      const label = askText(item.label);
      if (label === undefined) continue;
      options.push({ label, description: askText(item.description), preview: askText(item.preview) });
    }
    questions.push({
      question: askText(typed.question),
      header: askText(typed.header),
      options,
      multi: typeof typed.multi === "boolean" ? typed.multi : undefined,
      recommended: Number.isInteger(typed.recommended) && (typed.recommended as number) >= 0 ? typed.recommended : undefined,
    });
  }
  return questions.length > 0 ? { questions } : undefined;
}

// A tool event's call id, which pairs a card with its call's own end and answer.
function toolCallId(event: unknown): string | undefined {
  if (!event || typeof event !== "object") return undefined;
  return firstString((event as { toolCallId?: unknown }).toolCallId) ?? undefined;
}

type OptionalEventHandler = (event: unknown, ctx: ExtensionContext) => void | Promise<void>;

function registerOptional(api: ExtensionAPI, name: string, handler: OptionalEventHandler): void {
  try {
    (api.on as unknown as (event: string, handler: OptionalEventHandler) => void)(name, handler);
  } catch (_) {}
}

function hookInvocation(subcommand: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}) {
  if (process.env.GHOSTEX_OMP_HOOKS_DISABLED === "1") return null;
  // A `task` subagent loads this extension in the lead's process and terminal, so its hooks would
  // rebind the Ghostex session to the child's transcript and end the lead's turn with the child's.
  // gxserver reads the children from their own transcripts instead (session_chat_omp_fleet.rs).
  if ((ctx as unknown as { agent?: { kind?: unknown } }).agent?.kind === "sub") return null;

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

// Process completion, not writing stdin, lets the next hook follow this one at gxserver.
async function deliverHook(invocation: NonNullable<ReturnType<typeof hookInvocation>>): Promise<void> {
  await new Promise<void>((resolve) => {
    let settled = false;
    const settle = () => {
      if (settled) return;
      settled = true;
      resolve();
    };
    // Windows cannot run the notify hook script itself; gxserver runs it for the agent there.
    const [command, args] =
      process.platform === "win32"
        ? [__GXSERVER_PATH_JSON__, ["agent-hook-notify-native", __NOTIFY_HOOK_PATH_JSON__, "omp"]]
        : [__NOTIFY_HOOK_PATH_JSON__, []];
    try {
      const child = spawn(command, args, {
        env: invocation.env,
        stdio: ["pipe", "ignore", "ignore"],
        detached: true,
        windowsHide: true,
      });
      child.on("error", settle);
      // A closed stdin does not mean the hook process has finished delivering its event.
      child.stdin.on("error", () => {});
      child.on("exit", settle);
      child.stdin.end(invocation.payload);
    } catch (_) {
      settle();
    }
  });
}

type TurnEnd = { event: string; extra: Record<string, unknown> };

// Esc ends a run as `aborted` and a provider failure as `error`; neither is a finished turn.
function turnEndFor(event: AgentEndEvent): TurnEnd {
  let stopReason: unknown;
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index] as { role?: unknown; stopReason?: unknown } | undefined;
    if (message && message.role === "assistant") {
      stopReason = message.stopReason;
      break;
    }
  }
  const name = stopReason === "aborted" ? "Interrupt" : stopReason === "error" ? "StopFailure" : "Stop";
  return { event: name, extra: { last_assistant_message: lastAssistantMessage(event) } };
}

function isIdle(ctx: ExtensionContext): boolean {
  try {
    const typed = ctx as unknown as { isIdle?: () => boolean };
    return typeof typed.isIdle === "function" ? typed.isIdle() : false;
  } catch (_) {
    return false;
  }
}

__CHAT_CHANNEL__
export default function ghostexOmpSessionExtension(api: ExtensionAPI) {
  let promptOpen = false;
  const chat = new GhostexChatChannel(api);
  // OMP asks for every call of a message at once, without the call's input, and shows one approval
  // dialog at a time in that order. Claude's PermissionRequest comes per dialog it shows, so one is
  // sent for the dialog OMP shows, with the call's complete input from its `tool_call` event; the
  // dialog's answer goes out as `post_approval_response` (the answer event gxserver maps to working)
  // and only after that hook and the hooks held during the dialog ran, the next dialog's request,
  // so another call's start or end cannot overwrite a dialog's wait.
  const callInputs = new Map<string, unknown>();
  let approvals: { id: string; toolName: string | null }[] = [];
  // All hooks use one process-exit chain. While its FIFO head awaits an answer, only approval
  // hooks may enter the chain; other hooks keep their captured input and event order until the
  // answer. Ordinary handlers only enqueue; shutdown awaits delivery before OMP's explicit exit.
  let hooks: Promise<void> = Promise.resolve();
  const heldHooks: (() => Promise<void>)[] = [];
  const sendHook = (event: string, ctx: ExtensionContext, extra: Record<string, unknown> = {}) => {
    const invocation = hookInvocation(event, ctx, extra);
    if (!invocation) return;
    const deliver = () => deliverHook(invocation);
    if (approvals.length > 0 && event !== "PermissionRequest" && event !== "post_approval_response") {
      heldHooks.push(deliver);
    } else {
      hooks = hooks.then(deliver);
    }
  };
  const requestShownApproval = (ctx: ExtensionContext) => {
    const shown = approvals[0];
    if (!shown) return;
    sendHook("PermissionRequest", ctx, {
      tool_name: shown.toolName ?? undefined,
      tool_use_id: shown.id,
      tool_input: callInputs.get(shown.id),
    });
  };
  const flushHeldHooks = () => {
    for (const deliver of heldHooks) hooks = hooks.then(deliver);
    heldHooks.length = 0;
  };

  registerOptional(api, "session_shutdown", async () => {
    chat.stop();
    flushHeldHooks();
    await hooks;
  });

  api.on("session_start", (_event, ctx) => {
    approvals = [];
    callInputs.clear();
    sendHook("session-start", ctx);
    chat.start(ctx);
  });

  registerOptional(api, "input", (event) => {
    chat.noteInput(event);
  });

  registerOptional(api, "agent_start", () => {
    chat.noteStart();
  });

  api.on("before_agent_start", (event, ctx) => {
    sendHook("prompt-submit", ctx, { prompt: event.prompt });
  });

  // OMP has no `agent_settled`: an end it will continue past (a retry, a compaction or todo
  // continuation, a wait on a background job) says so with `willContinue`, so only the others
  // end the turn.
  api.on("agent_end", (event, ctx) => {
    if ((event as { willContinue?: unknown }).willContinue === true) return;
    const turnEnd = turnEndFor(event);
    sendHook(turnEnd.event, ctx, turnEnd.extra);
  });

  registerOptional(api, "ui_prompt_start", (event, ctx) => {
    if (isIdle(ctx)) return;
    promptOpen = true;
    const typed = event as { title?: unknown; kind?: unknown };
    sendHook("Notification", ctx, {
      message: firstString(typed.title) ?? "OMP is waiting for your answer",
      notification_type: firstString(typed.kind) ?? undefined,
    });
  });

  registerOptional(api, "ui_prompt_end", (_event, ctx) => {
    if (!promptOpen) return;
    promptOpen = false;
    sendHook(isIdle(ctx) ? "SessionIdle" : "PostToolUse", ctx);
  });

  registerOptional(api, "tool_execution_start", (event, ctx) => {
    const toolName = toolNameFrom(event);
    sendHook(isSubagentTool(toolName) ? "SubagentStart" : "PreToolUse", ctx, {
      tool_name: toolName ?? undefined,
      tool_use_id: toolCallId(event),
      tool_input: askToolInput(toolName, (event as { args?: unknown }).args),
    });
  });

  registerOptional(api, "tool_execution_end", (event, ctx) => {
    const toolName = toolNameFrom(event);
    const id = toolCallId(event);
    if (id) callInputs.delete(id);
    sendHook(isSubagentTool(toolName) ? "SubagentStop" : "PostToolUse", ctx, {
      tool_name: toolName ?? undefined,
      tool_use_id: id,
      is_error: toolIsError(event),
    });
  });

  registerOptional(api, "tool_call", (event) => {
    const id = toolCallId(event);
    if (id) callInputs.set(id, (event as { input?: unknown }).input);
  });

  registerOptional(api, "tool_approval_requested", (event, ctx) => {
    const id = toolCallId(event);
    if (!id) return;
    approvals.push({ id, toolName: toolNameFrom(event) });
    if (approvals.length === 1) requestShownApproval(ctx);
  });

  registerOptional(api, "tool_approval_resolved", (event, ctx) => {
    const id = toolCallId(event);
    if (!id) return;
    const shown = approvals[0]?.id === id;
    approvals = approvals.filter((approval) => approval.id !== id);
    if (!shown) return;
    sendHook("post_approval_response", ctx, { tool_use_id: id });
    flushHeldHooks();
    requestShownApproval(ctx);
  });

  registerOptional(api, "session_before_compact", (_event, ctx) => {
    sendHook("PreCompact", ctx);
  });

  registerOptional(api, "session_compact", (_event, ctx) => {
    sendHook("PostCompact", ctx);
  });
}
"###;
    source
        .replace("__CHAT_CHANNEL__", &chat_channel_source("omp", "OMP"))
        .replace("__MARKER__", &current_plugin_marker(OMP_EXTENSION_MARKER))
        .replace("__GXSERVER_PATH_JSON__", &gxserver_path_json())
        .replace("__NOTIFY_HOOK_PATH_JSON__", &notify_json)
}

pub(crate) fn current_plugin_marker(marker: &str) -> String {
    // Pi v6 and OMP v5 add the chat channel (chat_channel_source.rs); Pi v7 and OMP v6 add its
    // input box, dialog, compaction and trust commands; OMP v7 keeps subagents' hooks off the lead;
    // OMP v8 reports its `task` spawns as SubagentStart/SubagentStop.
    if marker == PI_EXTENSION_MARKER {
        format!("{marker} v7")
    } else if marker == AMP_PLUGIN_MARKER {
        format!("{marker} v5")
    } else if marker == OMP_EXTENSION_MARKER {
        format!("{marker} v11")
    } else {
        format!("{marker} v2")
    }
}

pub(crate) fn command_for_agent(definition: &HookDefinition, notify_hook_path: &Path) -> String {
    #[cfg(windows)]
    {
        return super::windows::command(definition.agent_id, notify_hook_path);
    }
    #[cfg(not(windows))]
    let notify_hook_path = path_string(notify_hook_path);
    #[cfg(not(windows))]
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
