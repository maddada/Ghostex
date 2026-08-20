# 015 — "Export transcript" Agent Action

Status: **implemented — in review** (epic `ghostex-a390`, beads `.1`–`.7` and `.9`–`.12`
in review as of 2026-08-20; `.8` subsumed by `.9`). Includes the no-auto-send revision:
"Start new conversation" stages `@<path> ` as draft input via
`runtimeSettings.firstUserInputDraft` — nothing is ever sent for the user.
All changes are uncommitted in the working tree.

## Goal

A new Agent Action, "Export transcript", that exports the current agent session's
conversation to a markdown file on disk so a *new* agent conversation can be started
with that file mentioned, transferring context between agents. Supported for the same
agents chat view supports. Core implemented in Rust (gxserver + ghostex CLI) so all
clients share it.

Reference implementation: `~/dev/references/codex-session-export` (`codex-md.py`,
Codex-only, parses raw `~/.codex/sessions` JSONL into 20 filterable sections).

## Fixed export selection (not configurable for now)

Enabled: User Messages, Agent Messages, Terminal Commands, Terminal Outputs,
Patches (**file names + changed line numbers only, short format**), Session Metadata.
Disabled: Agent/Internal Reasoning, MCP calls/outputs, Other tools/outputs, Web
searches, Token & rate limits, Turn context, Task events, System messages, Git
snapshots, Session events. Clean Chat: OFF.
Caps (from reference defaults): terminal output cap 8 lines/block; user/agent
message caps: ALL.
Patch Outputs: TBD (likely unnecessary given filenames+lines-only patches — see Q&A).

## Research summary

### Chat-view transcript pipeline (gxserver-rs)

- Supported agents: **claude/openclaude, codex, grok, pi/omp**
  (`session_chat.rs:209-228`, mirror `shared/session-chat.ts`).
- Per-agent decoders normalize raw transcript JSONL into `SessionChatMessage`
  (roles User/Assistant/Reasoning/Tool/System; blocks Text/ToolCall/ToolResult/ImageRef).
- Transcript file resolution per agent (incl. Claude successor-transcript adoption)
  already solved: `resolve_session_chat_transcript_path` (`session_chat.rs:2649`).
- Bulk read exists: `/api/readSessionChat` (limit ≤ 10 000, `beforeOffset` paging),
  CLI verb `ghostex read-session-chat`.
- No existing export/download feature server-side.
- Every *enabled* section above is representable from the normalized stream; the
  sections the normalized pipeline drops (token counts, turn context, git snapshots,
  system messages, session events…) are exactly the *disabled* ones.

### Agent Actions surface (where the button goes)

Touchpoints for a new action (from the existing Rename/Fork/StashPrompt pattern):
1. `shared/ghostex-hotkeys.ts` — hotkey def (`kind: "terminalToolbarAction"`).
2. `sidebar/chat/session-chat-host-actions-cluster.tsx` — `HOST_ACTION_ICONS` entry.
3. `gpui/src/terminal_element.rs` — 5 spots (action enum, render list, tooltip/hotkey
   table, icon table, click dispatch) + `TerminalAgentActionRequest` variant.
4. `gpui/sidebar/chat-main.tsx` — `createGpuiSessionChatHostActions` list.
5. `gpui/src/main.rs` — `receive_session_chat_host_action` whitelist +
   `handle_gpui_engine_terminal_agent_action` runner.
6. Web: `ghostex-web/src/app/session-chat-host.tsx` (`createWebSessionHostActions`,
   `runChatAgentAction`).
7. Mobile: `mobile/src/screens/TerminalScreen.tsx` Agent Actions menu via
   `runGhostexCli`.

### Server/CLI patterns to follow

- New endpoint = 3 registrations: `gxserver-rs/src/protocol.rs` (`endpoint_for`),
  `server.rs` route match, `shared/gxserver-protocol.ts` types.
- File-write precedent (works remote/web/mobile): `handle_save_session_chat_attachment_http`
  (`server.rs:12605`) — sanitize name, unique path under app data dir, `fs::write`.
- "Create new session seeded with a first prompt" precedent:
  `board_start_work.rs` → `createAgentSession` with
  `runtimeSettings.firstUserMessage` (+ `ghostex board start-work` thin CLI verb in
  `ghostex_cli/board.rs`).
- New CLI verb = 3 spots in `ghostex_cli/mod.rs` (+`usage.rs`), calling
  `rpc::call_gxserver_rpc`.

## Decisions settled by existing codebase conventions

- **Home: gxserver.** New `/api/exportSessionTranscript` endpoint implemented in
  gxserver-rs (new module next to `session_chat.rs`), plus a thin
  `ghostex export-transcript` CLI verb that calls it (board.rs pattern). The
  transcript files only exist on the daemon's machine, so a server endpoint is the
  only shape that works for web/mobile/remote; the CLI already lives in the same
  crate.

## Decision log (updated as questions are answered)

### Q1 — Export engine data source: **raw per-agent parsers** ✅ (user-decided)
Parse the raw transcript JSONL files directly, codex-md.py style, with a dedicated
export parser per supported agent format (claude, codex, grok, pi). Full fidelity:
the complete section taxonomy (reasoning, MCP, token counts, system messages, git
snapshots, …) is classified internally even though the default selection only
enables a subset — this keeps the door open for a configurable selection later.
Reuse from the existing pipeline: transcript path resolution + successor adoption
(`resolve_session_chat_transcript_path`), `resume_lookup.rs` helpers
(`read_lines_lossy`, `parse_json_line`, `text_from_message`), and the
session→agent-id resolution. The codex section taxonomy maps per agent, e.g. for
Claude: `thinking` blocks → Agent Reasoning, `tool_use` Bash → Terminal Commands,
Edit/Write/MultiEdit/apply_patch → Patches, `tool_result` paired by call-id →
the matching output section.

### Q2 — Output location/naming: **app data exports dir** ✅
`<gxserver app data dir>/exports/<session-title-slug>-<sessionId8>-<yyyyMMdd-HHmmss>.md`,
unique-path suffixing on collision (saveSessionChatAttachment pattern). Daemon-owned,
so identical behavior local/remote/web/mobile; endpoint returns `{ path }` (absolute)
for clients to mention/copy/reveal.

### Q3 — Post-export behavior: **result dialog with choices** ✅ (user-decided)
After the export succeeds, show a small result dialog:
**[Start new conversation] [Copy path] [Reveal in Finder]**.
- *Start new conversation*: agent picker → create a new agent session in the same
  project seeded via `runtimeSettings.firstUserMessage` (board start-work pattern)
  with a prompt referencing the exported file path.
- *Copy path*: absolute path to clipboard.
- *Reveal in Finder*: reveal the file (local machine only; hidden/adapted for
  remote/web/mobile).
Server side stays composable: `/api/exportSessionTranscript` only exports and returns
`{ path }`; the "start new conversation" choice calls the existing create-session
flow with the seeded first message. Dialog uses the native child-window modal
pattern (per AGENTS.md overlay rules).

### Q4 — Patches: **files + lines, short format; outputs only on failure** ✅
Each patch renders as one line: `🔧 path/to/file.rs:120-135 (+12/-3)` — line ranges
when derivable from hunk headers, otherwise `+added/-removed` counts; new files show
`(new file)`, deletions `(deleted)`. Multi-file patches render one line per file.
The Patch Outputs section is dropped entirely EXCEPT failed patches, which render a
one-line `⚠ patch failed: <short reason>` so the next agent knows an edit didn't
land. Failure detection: paired output with is_error/failed status by call-id.

### Q5 — Terminal output cap: **8 lines per block, keep tail** ✅
Each terminal output block keeps its LAST 8 lines with a
`... (N lines trimmed) ...` marker above (reference-tool behavior). No overall
size ceiling for now.

### Q6 — Clients in phase 1: **all clients** ✅ (user-decided)
gpui (Agent Actions overlay + chat-view cluster + result dialog), web, mobile, and
the `ghostex export-transcript` CLI verb — all wired in this feature pass. Server
endpoint is shared by all of them.

## Implementation outline

### 1. Export engine — new module `gxserver-rs/src/session_transcript_export.rs`
- Raw per-agent transcript parsers (Q1): claude, codex, grok, pi/omp. Each parser
  classifies every record into the full section taxonomy (user_message,
  agent_message, agent_reasoning, internal_reasoning, terminal_cmd,
  terminal_output, mcp_call, mcp_output, patch, patch_output, other_tool,
  other_tool_output, web_search, token_count, turn_context, task_event,
  system_message, git_snapshot, session_event, session_meta) even though the
  default selection only renders a subset — configurability comes later for free.
- Per-agent tool classification: terminal tools (Bash/shell/exec_command/
  local_shell_call/bashExecution), patch tools (apply_patch, Edit, Write,
  MultiEdit, NotebookEdit, str_replace-style editors), `mcp__*` → MCP, rest →
  other tools. Outputs paired to their call by call-id/tool_use_id (order-based
  fallback), inheriting the call's category.
- Reused plumbing: `resolve_session_chat_transcript_path` (+ Claude successor
  adoption), `resume_lookup.rs` helpers, `session_chat_agent_for_session`.
- Markdown renderer with the fixed default selection (see top), terminal-output
  tail cap 8 (Q5), patch one-liners + failure notes (Q4), session-meta header
  block (title, agent, model when known, cwd, date, source transcript file).
- File write: `<app data dir>/exports/<title-slug>-<sessionId8>-<timestamp>.md`,
  sanitized name + unique-path suffixing (Q2).

### 2. Endpoint — `/api/exportSessionTranscript`
`{ projectId, sessionId }` → `{ path, bytes }` (path absolute on the daemon
machine). Three registrations: `protocol.rs` (`endpoint_for`, RemoteAllowed),
`server.rs` route match (handler next to `handle_save_session_chat_attachment_http`),
`shared/gxserver-protocol.ts` request/response types. Unsupported agent →
structured error.

### 3. CLI — `ghostex export-transcript <session-selector>`
Thin RPC verb modeled on `ghostex_cli/board.rs`: NAMES entry + dispatch in
`ghostex_cli/mod.rs`, help line in `usage.rs`, prints `{ path }` JSON.

### 4. gpui
- Action id `exportTranscript`: hotkey def in `shared/ghostex-hotkeys.ts`, icon in
  `HOST_ACTION_ICONS` (session-chat-host-actions-cluster.tsx), SVG asset,
  `terminal_element.rs` 5 spots + `TerminalAgentActionRequest` variant,
  `createGpuiSessionChatHostActions` entry (chat-main.tsx),
  `receive_session_chat_host_action` whitelist +
  `handle_gpui_engine_terminal_agent_action` runner (main.rs).
- Runner routes through the sidebar runtime (fork pattern:
  `dispatch_gpui_workspace_terminal_runtime_action("exportTranscript", …)` + action
  allowlist extension in `gpui/sidebar/gxserver-runtime.ts`) so local AND remote
  machines both work via the machine-scoped RPC.
- Result dialog (Q3) as a native child-window modal:
  **[Start new conversation] [Copy path] [Reveal in Finder]**.
  Start new conversation → agent picker → `createAgentSession` with
  `runtimeSettings.firstUserMessage` seeded prompt (board start-work pattern) +
  provider start; session lands in the same project.

### 5. Web (`ghostex-web`)
`createWebSessionHostActions` entry + `runChatAgentAction` case →
`rpcForMachine(machineId, "/api/exportSessionTranscript", …)`. Result dialog
in-page: **[Start new conversation] [Copy path]** (no Finder on web; optionally a
Download of the markdown later).

### 6. Mobile
Agent Actions menu entry in `TerminalScreen.tsx` → `runGhostexCli` with the new
`export-transcript` verb over SSH. Result sheet: **[Start new conversation]
[Copy path]**.

### 7. Staged first input — NEVER auto-send (revised 2026-08-20 per user review)
The original design seeded `runtimeSettings.firstUserMessage`, which auto-sends a
full prompt on the user's behalf. **Rejected by the user.** Revised contract:
- "Start new conversation" creates the session and stages ONLY a mention of the
  exported md into the new session's input (draft text like `@<absolute path> `),
  typed into the CLI input but NOT submitted — the user writes their actual prompt
  around it and sends when they want. Nothing is ever sent for them.
- Mechanism: server-owned staged first input (new runtimeSettings key +
  post-readiness typing WITHOUT Enter, mirroring the fork-initial-rename flow which
  types `/rename` after a readiness delay — minus the Enter). One implementation in
  gxserver serves gpui, web, and mobile; a `create-agent` CLI flag forwards it
  (subsumes backlog bead ghostex-a390.8).
- `firstUserMessage` stays untouched for board start-work, which intentionally
  auto-sends.

### Notes / guardrails
- No tests unless explicitly requested (AGENTS.md).
- No fallback shims — unsupported agents get a clear error, not a degraded export.
- Result dialog uses the accepted native child-window pattern; no transparent
  overlay hacks.
- Verification: run `ghostex export-transcript` against real local claude + codex
  (+ grok/pi if present) sessions and inspect the markdown.
