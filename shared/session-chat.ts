// Session Chat — normalized chat projection of an agent terminal session.
// Canonical wire types shared by gxserver (Rust mirror in gxserver-rs/src/session_chat.rs),
// the shared React chat components (sidebar/chat/), and every client host.
// All values must stay plain JSON: they cross the /api/events websocket, the CEF bridge,
// and the gpui remote-machine proxy.

export const SESSION_CHAT_SUPPORTED_AGENTS = new Set([
  "claude",
  "openclaude",
  "codex",
  "grok",
  "pi",
  "omp",
]);

export type SessionChatTranscriptAgent = "claude" | "codex" | "grok" | "pi";

export function resolveSessionChatTranscriptAgent(
  agentId: string | null | undefined,
): SessionChatTranscriptAgent | null {
  if (agentId === "claude" || agentId === "openclaude") return "claude";
  if (agentId === "codex" || agentId === "grok") return agentId;
  if (agentId === "pi" || agentId === "omp") return "pi";
  return null;
}

export type SessionChatSource = "transcript" | "hook" | "client";

/** Visual palette for the shared chat surface, independent of app chrome. */
export type SessionChatTheme = "light" | "dark";

export function normalizeSessionChatTheme(value: unknown): SessionChatTheme {
  return value === "light" ? "light" : "dark";
}

// Higher wins when the same message id/turn arrives from two sources.
export const SESSION_CHAT_SOURCE_PRIORITY: Record<SessionChatSource, number> = {
  transcript: 3,
  hook: 2,
  client: 1,
};

export type SessionChatRole =
  | "user"
  | "assistant"
  | "reasoning"
  | "tool"
  | "system";

export interface SessionChatTextBlock {
  type: "text";
  text: string;
}

export interface SessionChatToolCallBlock {
  type: "tool-call";
  name: string;
  input: unknown;
}

export interface SessionChatToolResultBlock {
  type: "tool-result";
  output: string;
  isError?: boolean;
}

export interface SessionChatImageRefBlock {
  type: "image-ref";
  path?: string;
  url?: string;
  alt?: string;
}

export type SessionChatBlock =
  | SessionChatTextBlock
  | SessionChatToolCallBlock
  | SessionChatToolResultBlock
  | SessionChatImageRefBlock;

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
   * it. Never set on client-sourced optimistic echoes: those render
   * identically to real turns so the swap is invisible.
   */
  queued?: boolean;
}

export type SessionChatTurnLifecycleState =
  | "working"
  | "completed"
  | "interrupted";

export interface SessionChatTurnLifecycle {
  state: SessionChatTurnLifecycleState;
  turnId: string;
  timestamp: number | null;
}

export type SessionChatStatus =
  | "loading"
  | "ready"
  | "working"
  | "empty"
  | "starting"
  | "error"
  | "unsupported";

export interface SessionChatQuestionOption {
  label: string;
  description?: string;
}

export interface SessionChatQuestion {
  question: string;
  header?: string;
  multiSelect: boolean;
  options: SessionChatQuestionOption[];
}

export type SessionChatInteractivePrompt =
  | { kind: "question"; questions: SessionChatQuestion[] }
  | { kind: "approval"; tool: string; summary?: string };

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
  /** Evidence source; absent only when talking to an older daemon. */
  source?: "terminal" | "transcript";
}

export interface SessionChatDetectedOptions {
  model?: SessionChatDetectedChoice;
  effort?: SessionChatDetectedChoice;
  /** Codex's trailing `fast` modifier. Informational: no pill tracks it. */
  fast?: boolean;
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
  kind: "switchToTerminal" | "sendKeys";
  /** Raw bytes for `sendKeys`, written verbatim through answerSessionChatPrompt. */
  send?: string;
}

export interface SessionChatTerminalNotice {
  /**
   * Open set (`loginExpired`, `trustPrompt`, `permissionsWarning`,
   * `onboarding`, `usageLimit`, `streamError`, `updatePrompt`, `agentExited`,
   * `queuedInput`, `deliveryFailed`). Clients MUST render an unknown kind
   * generically — title/detail/severity are self-sufficient.
   */
  kind: string;
  severity: "error" | "warning" | "info";
  /** Short human line, e.g. "Codex login expired". */
  title: string;
  /** One or two sentences of guidance, including quoted terminal evidence. */
  detail?: string;
  /** SGR-stripped last visible lines (trimmed, capped ~2000 chars). */
  screenTail?: string;
  source: "screen" | "watchdog";
  /** ISO-8601 millis; also the key a client's local dismissal remembers. */
  detectedAt: string;
  actions?: SessionChatTerminalNoticeAction[];
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

export interface GxserverReadSessionChatResult {
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  beforeOffset: number;
  epoch: number;
  seq: number;
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
  error?: string;
}

export type SessionChatSkillSourceKind = "global" | "pluginCache" | "repository";

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
export type SessionChatSendKey = "shift-tab" | "shift-up" | "shift-down";

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
  kind: "question" | "approval";
  /** For questions: one entry per question. */
  selections?: SessionChatQuestionSelection[];
  /** For approvals: the raw byte string of the chosen option ("1" allow, "" deny). */
  approvalSend?: string;
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
  type: "subscribeSessionChat";
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
  type: "unsubscribeSessionChat";
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
  type: "sessionChatSnapshot";
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  beforeOffset: number;
  status: SessionChatStatus;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  agentSessionId?: string;
}

export interface GxserverSessionChatAppendedEvent extends SessionChatFrameBase {
  type: "sessionChatAppended";
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
  type: "sessionChatReplaced";
  messages: SessionChatMessage[];
  lifecycle?: SessionChatTurnLifecycle;
  hasMore: boolean;
  beforeOffset: number;
  status: SessionChatStatus;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  agentSessionId?: string;
}

export interface GxserverSessionChatStateEvent extends SessionChatFrameBase {
  type: "sessionChatState";
  status: SessionChatStatus;
  lifecycle?: SessionChatTurnLifecycle;
  prompt?: SessionChatInteractivePrompt;
  /** Model/effort read out of the session's terminal, when detectable. */
  selectedOptions?: SessionChatDetectedOptions;
  /** Blocking/failed terminal state. Omitted ⇒ cleared (prompt semantics). */
  terminalNotice?: SessionChatTerminalNotice;
  agentSessionId?: string;
}

export type GxserverSessionChatEvent =
  | GxserverSessionChatSnapshotEvent
  | GxserverSessionChatAppendedEvent
  | GxserverSessionChatReplacedEvent
  | GxserverSessionChatStateEvent;

export function isSessionChatEventType(
  type: string,
): type is GxserverSessionChatEvent["type"] {
  return (
    type === "sessionChatSnapshot" ||
    type === "sessionChatAppended" ||
    type === "sessionChatReplaced" ||
    type === "sessionChatState"
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
export type SessionSurfaceMode = "terminal" | "chat" | "find";
