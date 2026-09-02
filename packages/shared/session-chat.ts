// Session Chat — normalized chat projection of an agent terminal session.
// Canonical wire types shared by gxserver (Rust mirror in server/src/session_chat.rs),
// the shared React chat components (packages/core-ui/chat/), and every client host.
// All values must stay plain JSON: they cross the /api/events websocket, the CEF bridge,
// and the gpui remote-machine proxy.

// Ghostex's own prompt queue and the synced composer draft live in
// ./session-chat-queue (canonical) and are re-exported here so consumers keep
// a single import surface. Do NOT confuse SessionChatQueuedPrompt with
// SessionChatMessage.queued — see the note on that field below.
export type {
  GxserverQueueSessionChatPromptParams,
  GxserverQueueSessionChatPromptResult,
  GxserverReadSessionChatQueueParams,
  GxserverReadSessionChatQueueResult,
  GxserverRemoveSessionChatQueuedPromptParams,
  GxserverReorderSessionChatQueueParams,
  GxserverSendSessionChatQueuedPromptParams,
  GxserverSendSessionChatQueuedPromptResult,
  GxserverSessionChatQueueResult,
  GxserverSessionChatRemoveQueuedPromptResult,
  GxserverSetSessionChatDraftParams,
  GxserverSetSessionChatDraftResult,
  GxserverUpdateSessionChatQueuedPromptParams,
  SessionChatDraft,
  SessionChatQueuedPrompt,
  SessionChatQueuedPromptState,
} from './session-chat-queue';

import type { SessionChatDraft, SessionChatQueuedPrompt } from './session-chat-queue';

export const SESSION_CHAT_SUPPORTED_AGENTS = new Set([
  'antigravity',
  'antigravity-cli',
  'agy',
  'claude',
  'openclaude',
  'codex',
  'cursor',
  'grok',
  'grok-build',
  'hermes',
  'hermes-agent',
  'pi',
  'omp',
]);

export type SessionChatTranscriptAgent = 'antigravity' | 'claude' | 'codex' | 'cursor' | 'grok' | 'hermes' | 'pi';

export function resolveSessionChatTranscriptAgent(
  agentId: string | null | undefined,
  agentIcon?: string | null
): SessionChatTranscriptAgent | null {
  const candidates = [agentId, agentIcon];
  for (const candidate of candidates) {
    const normalized = candidate?.trim().toLowerCase();
    if (
      normalized === 'antigravity' ||
      normalized === 'antigravity-cli' ||
      normalized === 'antigravity cli' ||
      normalized === 'agy'
    ) {
      return 'antigravity';
    }
    if (normalized === 'claude' || normalized === 'openclaude') return 'claude';
    if (normalized === 'codex') return 'codex';
    if (normalized === 'cursor' || normalized === 'cursor-agent' || normalized === 'cursor cli') return 'cursor';
    if (normalized === 'grok' || normalized === 'grok-build') return 'grok';
    if (normalized === 'hermes' || normalized === 'hermes-agent' || normalized === 'hermes agent') return 'hermes';
    if (normalized === 'pi' || normalized === 'omp') return 'pi';
  }
  return null;
}

export type SessionChatDisplayAgent = SessionChatTranscriptAgent | 'omp';

/**
 * Resolve the agent identity shown by chat UI without conflating it with the
 * transcript parser family. OMP transcripts use Pi's format, but OMP remains
 * its own product name and logo everywhere the session is presented.
 */
export function resolveSessionChatDisplayAgent(
  agentId: string | null | undefined,
  agentIcon?: string | null
): SessionChatDisplayAgent | null {
  const candidates = [agentId, agentIcon];
  for (const candidate of candidates) {
    if (candidate?.trim().toLowerCase() === 'omp') {
      return 'omp';
    }
  }
  return resolveSessionChatTranscriptAgent(agentId, agentIcon);
}

/**
 * Sidebar artwork id for a chat agent label. Read-state labels are transcript
 * family ids, and two of those differ from the sidebar agent id that owns the
 * brand artwork (`hermes` → `hermes-agent`, `grok` → `grok-build`); the rest
 * match their sidebar id as-is.
 */
export function sessionChatAgentIconId(agentLabel: string | null | undefined): string | null {
  const display = resolveSessionChatDisplayAgent(agentLabel);
  if (display === 'antigravity') return 'antigravity-cli';
  if (display === 'hermes') return 'hermes-agent';
  if (display === 'grok') return 'grok-build';
  return display;
}

export type SessionChatSource = 'transcript' | 'hook' | 'client';

/** Visual palette for the shared chat surface, independent of app chrome. */
export type SessionChatTheme = 'light' | 'dark';

export function normalizeSessionChatTheme(value: unknown): SessionChatTheme {
  return value === 'light' ? 'light' : 'dark';
}

// Higher wins when the same message id/turn arrives from two sources.
export const SESSION_CHAT_SOURCE_PRIORITY: Record<SessionChatSource, number> = {
  transcript: 3,
  hook: 2,
  client: 1,
};

export type SessionChatRole = 'user' | 'assistant' | 'reasoning' | 'tool' | 'system';

export interface SessionChatTextBlock {
  type: 'text';
  text: string;
}

export interface SessionChatToolCallBlock {
  type: 'tool-call';
  name: string;
  input: unknown;
}

export interface SessionChatToolResultBlock {
  type: 'tool-result';
  output: string;
  isError?: boolean;
}

export interface SessionChatImageRefBlock {
  type: 'image-ref';
  path?: string;
  url?: string;
  alt?: string;
}

export type SessionChatBlock =
  SessionChatTextBlock | SessionChatToolCallBlock | SessionChatToolResultBlock | SessionChatImageRefBlock;

export interface SessionChatMessage {
  /** Stable across re-reads: record uuid/payload id, else `${filePath}:${byteOffset16}`. */
  id: string;
  role: SessionChatRole;
  blocks: SessionChatBlock[];
  /** Epoch ms; null sorts before any timestamp. */
  timestamp: number | null;
  source: SessionChatSource;
  /** Optional explicit turn key; same turnId ⇒ same turn (cross-source dedup). */
  turnId?: string;
  /**
   * Byte offset of the record's line in the agent transcript, stamped by the
   * server readers. Identical from every read path (tail, incremental,
   * pagination) for the same line, so it is a file-stable tie-break for equal
   * timestamps — a random-uuid tie-break reorders rows inside one turn.
   * Absent on hook/client-sourced messages.
   */
  byteOffset?: number;
  /**
   * The prompt is still waiting in the agent's own queue and has NOT been
   * handed to the model yet (the user typed it mid-turn). The server retracts
   * the row the moment the queue releases it and the delivered turn replaces
   * it. A client-sourced optimistic echo sets it only when the send was
   * issued mid-response (`sentWhileWorking` on the pending entry): the agent
   * will hold that prompt, so the echo pre-renders the queued row that
   * replaces it — and the transcript's fold logic must not treat it as a new
   * turn that settles the response still streaming above it.
   *
   * NOT Ghostex's prompt queue. This flag is the AGENT CLI's own internal
   * queue (Claude Code's `queue-operation` rows) holding a prompt the user
   * already sent with Enter. Ghostex's queue — prompts the agent has never
   * seen, held above the composer — is `SessionChatQueuedPrompt` in
   * ./session-chat-queue, surfaced as the `queue` field below. Never conflate,
   * rename, extend or reuse this field for that feature.
   */
  queued?: boolean;
}

export type SessionChatTurnLifecycleState = 'working' | 'completed' | 'interrupted';

export interface SessionChatTurnLifecycle {
  state: SessionChatTurnLifecycleState;
  turnId: string;
  timestamp: number | null;
}

export type SessionChatStatus = 'loading' | 'ready' | 'working' | 'empty' | 'starting' | 'error' | 'unsupported';

export interface SessionChatQuestionOption {
  label: string;
  description?: string;
}

export interface SessionChatQuestion {
  question: string;
  header?: string;
  multiSelect: boolean;
  /**
   * False when the asking tool offers no free-text answer (Pi's
   * cursor_ask_question with allowCustom: false); absent for tools that always
   * take one (Claude's "Type something" row).
   */
  allowCustom?: boolean;
  /**
   * The tool that asked, verbatim (AskUserQuestion, cursor_ask_question,
   * clarify, ask, …). The server's answer keystroke plan dispatches on it, so
   * one agent can host multiple asking tools with different terminal UIs.
   * Absent on prompts stored before 2026-08-30.
   */
  toolName?: string;
  /** omp's recommended option index: its ask dialog opens with the cursor on this row. */
  recommended?: number;
  options: SessionChatQuestionOption[];
}

export type SessionChatInteractivePrompt =
  { kind: 'question'; questions: SessionChatQuestion[] } | { kind: 'approval'; tool: string; summary?: string };

/** One answer per question, by 0-based option indices plus optional free text. */
export interface SessionChatQuestionSelection {
  indices: number[];
  other?: string;
}

// ---------------------------------------------------------------------------
// Detected session options (model / reasoning effort)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatDetectedOptions 2026-08-01:
What the agent is ACTUALLY running, read by gxserver from structured transcript
metadata and, when available, the terminal statusline/footer. The field is
omitted when neither source proves a value. There is no guessed value.
*/
export interface SessionChatDetectedChoice {
  /** Catalog id the option pills key their state by (`fable`, `gpt-5.6-sol`). */
  value: string;
  /** The agent-reported label (`Fable 5`), shown verbatim. */
  label: string;
  /**
   * Evidence source; absent only when talking to an older daemon.
   * `statusline` is the JSON Claude Code pipes to its statusLine command,
   * stored by the Ghostex-installed script (CDXC:ClaudeStatusline 2026-09-03).
   */
  source?: 'terminal' | 'transcript' | 'statusline';
}

/**
 * CDXC:ClaudeStatusline 2026-09-03: how full Claude's context window is, from
 * its statusLine payload. Every field is optional there too; the composer's
 * usage ring shows tokens over window size when both exist, else the
 * percentage.
 */
export interface SessionChatContextUsage {
  /** `context_window.used_percentage`, rounded. */
  usedPercentage?: number;
  /** `context_window.total_input_tokens`. */
  usedTokens?: number;
  /** `context_window.context_window_size`. */
  windowSize?: number;
}

export interface SessionChatDetectedOptions {
  model?: SessionChatDetectedChoice;
  effort?: SessionChatDetectedChoice;
  /** Claude's current Shift+Tab permission/input mode, read from its footer. */
  mode?: SessionChatDetectedChoice;
  /** Cursor's terminal-reported model context window, for example `272K` or `1M`. */
  contextWindow?: string;
  /** The complete normalized terminal line that supplied the detected values. */
  terminalStatusLine?: string;
  /** Cursor or Codex's terminal-reported Fast mode, or Claude's statusline-reported fast mode. */
  fast?: boolean;
  /** Claude's statusline-reported context window usage. */
  contextUsage?: SessionChatContextUsage;
  /** ISO-8601 millis; compared against a pending dispatch's own timestamp. */
  detectedAt: string;
}

// ---------------------------------------------------------------------------
// Terminal-state notices
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
State the agent TUI paints only on SCREEN, which a transcript projection can
never show: an expired login, a workspace-trust dialog, a usage-limit banner, a
stream error, the CLI having exited — plus the send watchdog's report that a
message could not be proven delivered. gxserver classifies the terminal capture
it already reads for the option pills, so this costs no extra work.

Carried by read results and by snapshot/replaced/state frames, NEVER by appended
frames. Semantics follow `prompt`, not `selectedOptions`: an OMITTED field means
CLEARED, so a client must reset its card whenever a frame that can carry it does
not.
*/
export interface SessionChatTerminalNoticeAction {
  id: string;
  label: string;
  kind: 'switchToTerminal' | 'sendKeys';
  /** Raw bytes for `sendKeys`, written verbatim through answerSessionChatPrompt. */
  send?: string;
}

/*
CDXC:SessionChatTerminalPicker 2026-08-21:
Rows of an on-screen picker the chat surface can ANSWER, rather than only point
at — Claude Code's resume-usage chooser ("Resume from summary" / "Resume full
session as-is" / "Don't ask me again"), which owns the CLI's input line after a
large session is resumed and whose Enter CONFIRMS a row, so a chat message
delivered into it silently compacts the conversation it was meant to continue.

A notice that carries these renders the same option rows as the AskUserQuestion
card, and the pick goes back through answerSessionChatPrompt's `terminalChoice`
lane. `selected` is where the TUI highlight sat AT DETECTION TIME: shown as the
CLI's own default, never used to compute keystrokes, because the highlight can
move between the detection and the answer.

A daemon that predates this omits the field, and a client that predates it
renders the notice as title + detail + "Open terminal" — exactly what this
state used to get, which was nothing at all.
*/
export interface SessionChatTerminalNoticeChoice {
  /** 0-based row index, which is what an answer addresses. */
  index: number;
  label: string;
  /** True for the row the agent TUI highlighted when this was detected. */
  selected: boolean;
}

export interface SessionChatTerminalNotice {
  /**
   * Open set (`loginExpired`, `trustPrompt`, `permissionsWarning`,
   * `onboarding`, `usageLimit`, `streamError`, `updatePrompt`, `agentExited`,
   * `queuedInput`, `deliveryFailed`, `resumePrompt`, `switchConfirmPrompt`,
   * `sessionPausedPrompt`, `codexInputBlocked`, `cursorInputBlocked`, `grokInputBlocked`,
   * `hermesInputBlocked`, `ompInputBlocked`, `piInputBlocked`). Clients MUST
   * render an unknown kind generically; title/detail/severity are self-sufficient.
   */
  kind: string;
  severity: 'error' | 'warning' | 'info';
  /** Short human line, e.g. "Codex login expired". */
  title: string;
  /** One or two sentences of guidance, including quoted terminal evidence. */
  detail?: string;
  /** SGR-stripped last visible lines (trimmed, capped ~2000 chars). */
  screenTail?: string;
  source: 'screen' | 'watchdog';
  /** ISO-8601 millis; also the key a client's local dismissal remembers. */
  detectedAt: string;
  actions?: SessionChatTerminalNoticeAction[];
  /**
   * Answerable picker rows, in screen order. Absent for every notice that only
   * describes a state; present means the card shows an answer picker.
   */
  choices?: SessionChatTerminalNoticeChoice[];
}

/*
CDXC:SessionChatTerminalActivity 2026-08-22:
Live work the agent CLI reports on its terminal before transcript JSONL catches
up. `claude-status` is the current `⏺ …` assistant line and becomes transient
reasoning history in the client; `shells-running` remains one bottom activity
row only while Claude shows its background-shell status; `compacting` is
structured progress:

    ✶ Compacting conversation… (1m 1s)
      ████████████████████░░░░░░░░░░░░░░░░░░░░ 49%

Deliberately NOT a `terminalNotice`: nothing is wrong, nothing is blocked, and
there is nothing to answer. Both variants render in the transcript.

`percent` and `elapsedSeconds` are read off the screen or omitted; a client must
never estimate them. `detectedAt` is the anchor for a smoothly ticking local
clock: it belongs to the RUN, not the sample, so it holds still while the
numbers move. Carried by read results and by snapshot/replaced/state frames with
`prompt` semantics — an omitted field means CLEARED, which is how a client
learns the work finished.
*/
export interface SessionChatTerminalActivity {
  /** Open set (`compacting`, `claude-status`, `shells-running`). */
  kind: string;
  /** Agent-facing wording, without the spinner glyph or the clock. */
  label: string;
  /** 0-100, only when the screen actually painted a percentage. */
  percent?: number;
  /** Seconds the CLI reported, only when it painted them. */
  elapsedSeconds?: number;
  /** ISO-8601 millis; stable for the whole run, so a local clock can tick. */
  detectedAt: string;
}

/*
CDXC:SessionChatAppCommands 2026-08-23:
Slash commands GHOSTEX typed into the agent without the composer:
provider-specific first-prompt auto-title jobs and the rename modal stage
`/rename <title>` (Pi `/name`, Hermes Agent `/title`), while non-Codex forks
submit a provisional `Fork: <old title>`.

Claude Code records everything it intercepts, so its transcript already carries
those sends. Codex records NOTHING, and a session that renamed itself mid-thread
left no trace in chat at all — the reader saw the title change with no
explanation. These rows are the app saying what it did.

They are an ACKNOWLEDGEMENT with a short server-side TTL, never history: a
client drops one as soon as it finds the agent's own record of the same command,
so the two never both render, and nothing is persisted. Unlike `prompt` /
`terminalNotice`, an omitted field does NOT mean cleared — the rows retire on
their own schedule, so a frame with nothing to add stays silent instead of
racing the client into dropping one it should still show.
*/
export interface SessionChatAppCommand {
  /** Stable within a session; two sends can carry identical text. */
  id: string;
  /** Verbatim command as written to the terminal, e.g. `/rename Fix parser`. */
  command: string;
  /** Assigned session title; arrives after agent metadata resolves a bare `/rename`. */
  title?: string;
  /** ISO-8601 millis. */
  sentAt: string;
}

/*
CDXC:SessionChatAgentFleet 2026-08-23:
Sub-agents Claude Code is running, which exist ONLY on its terminal
screen — nothing about them reaches transcript JSONL:

      ⏺ main
      ◯ general-purpose  Fixing tool-ro… 12m 36s · ↓ 171.9k tokens

Without this a chat surface can say nothing better than "the agent is working"
while three agents are working. The `main` row is the block's header, not a
member: it is the agent the user is already talking to, and the chat IS its
output, so only the rows below it arrive here.

The name arrives with its `(+1)` marker split off into `nested`, because the CLI
space-pads the name column to align every task and leaving the marker in would
misalign it while making one row's agent type read as a different type.

`task` comes ellipsized by the terminal that painted it — the CLI truncated it
to a column, and re-truncating in CSS is the client's business. `elapsedSeconds`
is read off the screen or omitted; a client must never estimate it, but it
SHOULD tick locally from `detectedAt`, which is minted with the seconds it
belongs to. Never treat those two as independent: the clock is
`elapsedSeconds + (now - detectedAt)`, so they only agree while they describe
the same instant. The server holds a fleet still by not republishing it — the
roster and the token counters decide that, the clocks never do.

Carried by read results and by snapshot/replaced/state frames with `prompt`
semantics — omitted ⇒ CLEARED, which is how a client learns the fleet is done.
Never gated on the main agent working: sub-agents outlive the turn that
spawned them.
*/
export interface SessionChatSubAgent {
  /** Agent type as the CLI names it (`general-purpose`). */
  name: string;
  /** What it is doing, already ellipsized by the terminal. */
  task?: string;
  /** Seconds the CLI reported, only when it painted them. */
  elapsedSeconds?: number;
  /**
   * The token counter exactly as painted (`↓ 155.4k tokens`), arrow and all.
   * Kept whole rather than split into a number: the arrow is the direction and
   * the CLI already rounded the figure to fit a narrow column.
   */
  tokens?: string;
  /**
   * The `(+1)` the CLI paints beside a name: further agents running under this
   * one, folded into its row instead of listed. Absent when unmarked; never 0.
   */
  nested?: number;
}

export interface SessionChatAgentFleet {
  /** Screen order, never empty: no sub-agents means no fleet at all. */
  agents: SessionChatSubAgent[];
  /** ISO-8601 millis; stable for the roster, so local clocks can tick. */
  detectedAt: string;
}

// ---------------------------------------------------------------------------
// /api/readSessionChat
// ---------------------------------------------------------------------------

export interface GxserverReadSessionChatParams {
  projectId: string;
  sessionId: string;
  /** Max messages in the tail window. Default 300; page by +200. */
  limit?: number;
  /** Byte offset from a prior page's `beforeOffset` for older history. */
  beforeOffset?: number;
  /**
   * Long-poll (SSH-only clients such as Ghostex mobile): with `fingerprint`,
   * the server holds the request until the chat's fingerprint changes or
   * this many ms elapse (clamped to 30s), then answers with a normal read.
   */
  waitMs?: number;
  /** The `fingerprint` from a previous read result. */
  fingerprint?: string;
}

/**
 * Prefix gxserver stamps on the synthesized divider that marks where stitched
 * scroll-back crosses from one fork ancestor into the next. Mirrors
 * `FORK_BOUNDARY_MESSAGE_ID_PREFIX` in server/src/session_chat_fork_stitch.rs.
 */
export const SESSION_CHAT_FORK_BOUNDARY_ID_PREFIX = 'fork-boundary:';

export interface GxserverReadSessionChatResult {
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  /** Present on daemons whose `hasMore` is computed after transient rows are filtered. */
  hasMoreExact?: boolean;
  beforeOffset: number;
  epoch: number;
  seq: number;
  /**
   * Present only when this session's Codex rollout was opened by `codex fork`.
   * `forkedFromId` is the predecessor rollout named by its `session_meta`, and
   * `ancestorIds` walks that lineage oldest-last. Scroll-back is stitched across
   * those files server side, so this is metadata for labelling the boundary, not
   * something a client has to fetch pages with.
   */
  forkInfo?: { forkedFromId: string; ancestorIds: string[] };
  /** Opaque change token for `waitMs` long-polling. */
  fingerprint?: string;
  status: SessionChatStatus;
  agent?: string;
  agentSessionId?: string;
  prompt?: SessionChatInteractivePrompt;
  /**
   * The session's live agent-hook activity: true while the agent is working.
   * Independent of `status` (which describes the transcript read), so a host
   * that only speaks the chat channel still gets the working indicator.
   */
  working?: boolean;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  /** Live on-screen progress (compaction). Omitted ⇒ cleared. */
  terminalActivity?: SessionChatTerminalActivity;
  /**
   * Commands Ghostex itself typed into this session. NOT prompt semantics:
   * an omitted field leaves whatever the client already has.
   */
  appCommands?: SessionChatAppCommand[];
  /** Sub-agents the screen is painting. Omitted ⇒ cleared. */
  agentFleet?: SessionChatAgentFleet;
  /**
   * True once gxserver has actually read this session's screen. Unlike every
   * other screen-derived field here, it does NOT describe what was found — it
   * says the looking happened, which is the only way a client can tell "the
   * model is still being detected" from "detection ran and this agent's screen
   * names no model". The composer needs that to choose between a loading
   * skeleton and a plain unset pill; a stopped session, which has no screen at
   * all, must never sit under a skeleton forever. Omitted ⇒ not probed yet.
   */
  screenProbed?: boolean;
  /**
   * The session's Ghostex-owned prompt queue, head first. PRESENT (even as an
   * empty array) is the daemon capability probe: a daemon that predates this
   * feature omits it, and a client that sees it omitted hides every queue
   * control instead of calling endpoints that will 404.
   * When present, it is authoritative and replaces the client's list.
   * See CDXC:SessionChatQueueCarriage in ./session-chat-queue.
   */
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Unlike `prompt`/`terminalNotice`, an OMITTED
   * draft means "unchanged / none on the server", NOT cleared — clearing a
   * local draft because an old daemon never sends the field would destroy
   * text the user typed. Clear a draft by writing an empty `content` through
   * /api/setSessionChatDraft instead.
   */
  draft?: SessionChatDraft;
  /**
   * CDXC:DraftSessions 2026-08-28:
   * The agents this session may still be switched to, resolved by the daemon
   * that owns the project. PRESENT ONLY while the session is a draft: once the
   * first user prompt reaches the agent the session's agent is fixed, so an
   * omitted field is the client's signal to hide the composer's "Agents"
   * section entirely (it is also what a daemon predating drafts sends).
   *
   * The list is the five chat-supported base families plus the project's custom
   * agents whose base family is chat-supported — never every launchable agent,
   * because chat cannot read a transcript it has no decoder for.
   */
  availableAgents?: SessionChatAvailableAgent[];
  /**
   * CDXC:DraftSessions 2026-08-28:
   * The session's own launch agent id, which `agent` above is NOT: that one is
   * the transcript family, so a project custom agent built on Claude reports
   * `claude` there and cannot be told apart from Claude itself. This is the id
   * `/api/switchDraftAgent` takes and the one that matches an `availableAgents`
   * row, so the composer can tick the current agent and — after a switch —
   * follow the new agent without a reload instead of trusting its boot-time URL
   * parameter. Absent on plain terminals and on daemons that predate drafts.
   */
  sessionAgentId?: string;
  error?: string;
}

/**
 * CDXC:DraftSessions 2026-08-28:
 * One row of the composer's "Agents" section. `agentId` is what
 * `/api/switchDraftAgent` takes; `baseAgentId` is the chat-supported family the
 * agent belongs to (`agentId` itself for a built-in, the custom agent's declared
 * base otherwise) and is what brand-logo and transcript-decoder lookups use.
 */
export interface SessionChatAvailableAgent {
  agentId: string;
  name: string;
  icon: string;
  baseAgentId: string;
}

export type SessionChatSkillSourceKind = 'global' | 'pluginCache' | 'repository';

export interface SessionChatSkill {
  /** Display/mention name, matching the skill folder shown by Agents Hub. */
  name: string;
  /** Absolute skill folder path on the machine that owns this session. */
  directoryPath: string;
  /** Absolute SKILL.md path on the machine that owns this session. */
  skillFilePath: string;
  sourceKind: SessionChatSkillSourceKind;
}

export interface GxserverReadSessionChatSkillsResult {
  /** gxserver-resolved agent identity; clients do not choose the provider. */
  agentId: string;
  generatedAt: string;
  skills: SessionChatSkill[];
}

/**
 * Composer "@" file mentions. gxserver walks the session's project on its own
 * machine and answers with project-relative paths, so the composer can insert
 * the same "@path" the agent resolves against its working directory.
 */
export interface GxserverReadSessionChatFilesResult {
  /** Absolute project root the paths are relative to. */
  rootPath: string;
  generatedAt: string;
  /** Project-relative paths, always forward-slash separated. */
  files: string[];
  /** True when the walk hit its entry cap, so the list is partial. */
  truncated: boolean;
}

// ---------------------------------------------------------------------------
// /api/sendSessionChatMessage · /api/answerSessionChatPrompt · /api/interruptSessionChat
// ---------------------------------------------------------------------------

/**
 * Raw keystrokes the chat surface can inject into the agent TUI that are not
 * expressible as text. `shift-tab` is Claude Code's permission-mode cycle;
 * shifted arrows adjust Codex reasoning effort.
 */
export type SessionChatSendKey = 'enter' | 'shift-tab' | 'shift-up' | 'shift-down';

export interface GxserverSendSessionChatMessageParams {
  projectId: string;
  sessionId: string;
  /** Message body. Omitted (or empty) when `key` carries the request. */
  text?: string;
  imagePaths?: string[];
  /**
   * Mutually exclusive with `text`/`imagePaths`: writes the key's raw byte
   * sequence into the pty with no bracketed paste, no clear burst and no
   * trailing Enter.
   */
  key?: SessionChatSendKey;
}

export interface GxserverSendSessionChatMessageResult {
  queued: boolean;
  textBytes: number;
}

/*
CDXC:SessionChatImagePaste 2026-08-01:
saveSessionChatImage writes composer-pasted image bytes into the Ghostex image directory on
the machine the session runs on (clients call it over their per-machine RPC,
so a remote session's image lands on the remote machine). The returned
absolute path is what the composer interpolates into "[Image #N](path)" —
the same reference format the terminal paste path produces.
*/
export interface GxserverSaveSessionChatImageParams {
  projectId: string;
  sessionId: string;
  /** Raw base64 or a full data URL (the data: prefix is tolerated). */
  base64Data: string;
  /** Mined only for its extension; the stored name is always generated. */
  suggestedName?: string;
}

export interface GxserverSaveSessionChatImageResult {
  path: string;
  bytes: number;
}

/*
CDXC:SessionChatAttachments 2026-08-02:
saveSessionChatAttachment is the non-image sibling of saveSessionChatImage:
any file's bytes land in the Ghostex attachment directory on the session's machine and the
returned absolute path is what the composer interpolates into
"[File #N](path)". The sanitized original file name is kept in the stored
name (after a generated epoch prefix) so agents see a meaningful extension.
*/
export interface GxserverSaveSessionChatAttachmentParams {
  projectId: string;
  sessionId: string;
  /** Raw base64 or a full data URL (the data: prefix is tolerated). */
  base64Data: string;
  /** Creates a dropped directory instead of writing file bytes. */
  directory?: boolean;
  /** Stable, sanitized identity shared by one recursively uploaded folder. */
  uploadId?: string;
  /** Slash-separated path below the recursively uploaded folder root. */
  relativePath?: string;
  /** Sanitized into the stored file name; path segments are stripped. */
  suggestedName?: string;
}

export interface GxserverSaveSessionChatAttachmentResult {
  path: string;
  bytes: number;
}

/*
readSessionChatImage returns the bytes of an image file on the session's
machine (chat-log thumbnails and image links render through it, since the
paths inside "[Image #N](path)" references are machine paths the client
cannot open directly).
*/
export interface GxserverReadSessionChatImageParams {
  /** Absolute path on the machine that serves the RPC. */
  path: string;
}

export interface GxserverReadSessionChatImageResult {
  base64Data: string;
  /** image/* media type inferred from the file's magic bytes / extension. */
  mediaType: string;
  bytes: number;
}

export interface GxserverAnswerSessionChatPromptParams {
  projectId: string;
  sessionId: string;
  kind: 'question' | 'approval' | 'terminalChoice';
  /** For questions: one entry per question. */
  selections?: SessionChatQuestionSelection[];
  /** For approvals: the raw byte string of the chosen option ("1" allow, "" deny). */
  approvalSend?: string;
  /**
   * For terminalChoice: the `index` of the `SessionChatTerminalNoticeChoice`
   * the user picked. gxserver re-reads the live screen and walks the highlight
   * onto that row, so a picker that was answered in the terminal meanwhile
   * fails loudly instead of confirming whatever replaced it.
   */
  choiceIndex?: number;
}

export interface GxserverAnswerSessionChatPromptResult {
  queued: boolean;
}

export interface GxserverInterruptSessionChatParams {
  projectId: string;
  sessionId: string;
}

export interface GxserverInterruptSessionChatResult {
  interrupted: boolean;
}

export interface GxserverHandoffSessionChatDraftParams {
  projectId: string;
  sessionId: string;
}

/**
 * Result of moving the agent CLI's composer draft out of the terminal so the
 * chat composer can own it. `content` is empty (and `transferred` false) when
 * the CLI composer held nothing — a successful capture of nothing, not an
 * error. The draft is cleared from the terminal before this resolves.
 */
export interface GxserverHandoffSessionChatDraftResult {
  content: string;
  transferred: boolean;
}

// ---------------------------------------------------------------------------
// /api/events frames
// ---------------------------------------------------------------------------

export interface GxserverSubscribeSessionChatMessage {
  type: 'subscribeSessionChat';
  projectId: string;
  sessionId: string;
  /**
   * Follower tail window for snapshot/replaced frames. Hosts pass the size of
   * the list they already display so a re-subscribe (reconnect, duplicate
   * subscribe) cannot answer with fewer rows than are on screen. The server
   * only ever raises a live follower's window, never lowers it; daemons that
   * predate the field ignore it and keep the 300-row default.
   */
  limit?: number;
}

export interface GxserverUnsubscribeSessionChatMessage {
  type: 'unsubscribeSessionChat';
  projectId: string;
  sessionId: string;
}

interface SessionChatFrameBase {
  projectId: string;
  sessionId: string;
  /** Follower generation; bumps on start/replace/re-resolve. */
  epoch: number;
  /** Monotonic within an epoch, starting at 1. */
  seq: number;
  protocolVersion: number;
  serverId: string;
  /**
   * The session's live agent-hook activity at frame time (true = working).
   * Carried by snapshot/replaced/state frames; omitted on appended frames,
   * which never change it.
   */
  working?: boolean;
}

export interface GxserverSessionChatSnapshotEvent extends SessionChatFrameBase {
  type: 'sessionChatSnapshot';
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  /** Present on daemons whose `hasMore` is computed after transient rows are filtered. */
  hasMoreExact?: boolean;
  beforeOffset: number;
  status: SessionChatStatus;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  /** Live on-screen progress (compaction). Omitted ⇒ cleared. */
  terminalActivity?: SessionChatTerminalActivity;
  /**
   * Commands Ghostex itself typed into this session. NOT prompt semantics:
   * an omitted field leaves whatever the client already has.
   */
  appCommands?: SessionChatAppCommand[];
  /** Sub-agents the screen is painting. Omitted ⇒ cleared. */
  agentFleet?: SessionChatAgentFleet;
  /**
   * True once gxserver has actually read this session's screen. Unlike every
   * other screen-derived field here, it does NOT describe what was found — it
   * says the looking happened, which is the only way a client can tell "the
   * model is still being detected" from "detection ran and this agent's screen
   * names no model". The composer needs that to choose between a loading
   * skeleton and a plain unset pill; a stopped session, which has no screen at
   * all, must never sit under a skeleton forever. Omitted ⇒ not probed yet.
   */
  screenProbed?: boolean;
  /**
   * Ghostex's prompt queue, head first. PRESENT (even empty) is the daemon
   * capability probe; omitted ⇒ this daemon has no queue and the client hides
   * every queue control. When present it is authoritative and replaces the
   * client's list. Never carried by `sessionChatAppended`.
   */
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Omitted ⇒ UNCHANGED, not cleared — the
   * opposite of the `prompt`/`terminalNotice` rule above, because this is text
   * the user typed and an old daemon that never sends it must not erase it.
   * Clear it by writing an empty `content` through /api/setSessionChatDraft.
   */
  draft?: SessionChatDraft;
  /**
   * Mobile's SSH transport synthesizes this frame from a read and carries the
   * read-only draft-agent state as own-properties. Daemon event frames omit
   * both fields, so clients must fold them only when either property exists.
   */
  availableAgents?: SessionChatAvailableAgent[];
  sessionAgentId?: string;
  agentSessionId?: string;
}

export interface GxserverSessionChatAppendedEvent extends SessionChatFrameBase {
  type: 'sessionChatAppended';
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  /**
   * Ids of messages an earlier frame published that the transcript has since
   * proven abandoned — a prompt that was re-sent or revised before the agent
   * answered leaves the first submission behind as a dead branch, and the
   * terminal never showed it. Applied BEFORE `messages`. Omitted (not empty)
   * in the common case, so older daemons simply never retract anything.
   */
  supersededMessageIds?: string[];
}

export interface GxserverSessionChatReplacedEvent extends SessionChatFrameBase {
  type: 'sessionChatReplaced';
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  /** Present on daemons whose `hasMore` is computed after transient rows are filtered. */
  hasMoreExact?: boolean;
  beforeOffset: number;
  status: SessionChatStatus;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  /** Live on-screen progress (compaction). Omitted ⇒ cleared. */
  terminalActivity?: SessionChatTerminalActivity;
  /**
   * Commands Ghostex itself typed into this session. NOT prompt semantics:
   * an omitted field leaves whatever the client already has.
   */
  appCommands?: SessionChatAppCommand[];
  /** Sub-agents the screen is painting. Omitted ⇒ cleared. */
  agentFleet?: SessionChatAgentFleet;
  /**
   * True once gxserver has actually read this session's screen. Unlike every
   * other screen-derived field here, it does NOT describe what was found — it
   * says the looking happened, which is the only way a client can tell "the
   * model is still being detected" from "detection ran and this agent's screen
   * names no model". The composer needs that to choose between a loading
   * skeleton and a plain unset pill; a stopped session, which has no screen at
   * all, must never sit under a skeleton forever. Omitted ⇒ not probed yet.
   */
  screenProbed?: boolean;
  /**
   * Ghostex's prompt queue, head first. PRESENT (even empty) is the daemon
   * capability probe; omitted ⇒ this daemon has no queue and the client hides
   * every queue control. When present it is authoritative and replaces the
   * client's list. Never carried by `sessionChatAppended`.
   */
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Omitted ⇒ UNCHANGED, not cleared — the
   * opposite of the `prompt`/`terminalNotice` rule above, because this is text
   * the user typed and an old daemon that never sends it must not erase it.
   * Clear it by writing an empty `content` through /api/setSessionChatDraft.
   */
  draft?: SessionChatDraft;
  agentSessionId?: string;
}

export interface GxserverSessionChatStateEvent extends SessionChatFrameBase {
  type: 'sessionChatState';
  status: SessionChatStatus;
  lifecycle?: SessionChatTurnLifecycle;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  /** Live on-screen progress (compaction). Omitted ⇒ cleared. */
  terminalActivity?: SessionChatTerminalActivity;
  /**
   * Commands Ghostex itself typed into this session. NOT prompt semantics:
   * an omitted field leaves whatever the client already has.
   */
  appCommands?: SessionChatAppCommand[];
  /** Sub-agents the screen is painting. Omitted ⇒ cleared. */
  agentFleet?: SessionChatAgentFleet;
  /**
   * True once gxserver has actually read this session's screen. Unlike every
   * other screen-derived field here, it does NOT describe what was found — it
   * says the looking happened, which is the only way a client can tell "the
   * model is still being detected" from "detection ran and this agent's screen
   * names no model". The composer needs that to choose between a loading
   * skeleton and a plain unset pill; a stopped session, which has no screen at
   * all, must never sit under a skeleton forever. Omitted ⇒ not probed yet.
   */
  screenProbed?: boolean;
  /**
   * Ghostex's prompt queue, head first. PRESENT (even empty) is the daemon
   * capability probe; omitted ⇒ this daemon has no queue and the client hides
   * every queue control. When present it is authoritative and replaces the
   * client's list. Never carried by `sessionChatAppended`.
   */
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Omitted ⇒ UNCHANGED, not cleared — the
   * opposite of the `prompt`/`terminalNotice` rule above, because this is text
   * the user typed and an old daemon that never sends it must not erase it.
   * Clear it by writing an empty `content` through /api/setSessionChatDraft.
   */
  draft?: SessionChatDraft;
  agentSessionId?: string;
}

export type GxserverSessionChatEvent =
  | GxserverSessionChatSnapshotEvent
  | GxserverSessionChatAppendedEvent
  | GxserverSessionChatReplacedEvent
  | GxserverSessionChatStateEvent;

export function isSessionChatEventType(type: string): type is GxserverSessionChatEvent['type'] {
  return (
    type === 'sessionChatSnapshot' ||
    type === 'sessionChatAppended' ||
    type === 'sessionChatReplaced' ||
    type === 'sessionChatState'
  );
}

// ---------------------------------------------------------------------------
// View mode ("viewMode" is taken by the sidebar layout mode — do not reuse it)
// ---------------------------------------------------------------------------

/*
CDXC:AgentHistorySearch 2026-08-20:
"find" is the Find surface — the GUI for `gx f` — which swaps a session's pane
body on exactly the same terms as chat: the terminal parks rather than closing,
and only one surface can own the pane at a time.
*/
export type SessionSurfaceMode = 'terminal' | 'chat';
