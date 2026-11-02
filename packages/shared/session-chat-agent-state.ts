import type { SessionChatDetectedChoice } from './session-chat-transcript';
import type { SessionChatTerminalDialog } from './session-chat-events';

/**
 * CDXC:AgentScreenDetection 2026-09-03 WHY: how full Claude's context window is, from
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

/**
 * CDXC:SessionChatDetectedOptions 2026-09-04 DECISION:
 * User: the context meter popover shows a "More details" section and starred
 * rows become a text status line under the chat box. This is the slice of
 * Claude's statusLine payload the chat can show, camelCase, every field
 * absent when Claude did not report it (see `claude_statusline_status_value`
 * in server/src/session_chat_options/selection_sources.rs).
 */
export interface SessionChatClaudeStatus {
  cost?: {
    totalUsd?: number;
    durationMs?: number;
    apiDurationMs?: number;
    linesAdded?: number;
    linesRemoved?: number;
  };
  rateLimits?: {
    fiveHour?: SessionChatClaudeRateLimitWindow;
    sevenDay?: SessionChatClaudeRateLimitWindow;
  };
  promptCache?: {
    warm?: boolean;
    ttl?: string;
    /** Epoch seconds. */
    expiresAt?: number;
    hitRatio?: number;
    requests?: number;
    misses?: number;
    lastMissCause?: string;
    cacheWriteTokens?: number;
    recacheTokensIfCold?: number;
  };
  lastRequest?: {
    inputTokens?: number;
    outputTokens?: number;
    cacheReadTokens?: number;
    cacheWriteTokens?: number;
  };
  totalOutputTokens?: number;
  remainingPercentage?: number;
  exceeds200kTokens?: boolean;
  thinkingEnabled?: boolean;
  outputStyle?: string;
  sessionName?: string;
  /** Claude's own session id, the one `claude --resume` takes. */
  sessionId?: string;
  version?: string;
  repo?: { host?: string; owner?: string; name?: string };
  addedDirs?: string[];
  projectDir?: string;
  currentDir?: string;
  pr?: { number?: number; url?: string; reviewState?: string };
}

export interface SessionChatClaudeRateLimitWindow {
  usedPercentage?: number;
  /** Epoch seconds. */
  resetsAt?: number;
}

/** Codex-reported values from persisted rollout events. Missing fields remain unavailable. */
export interface SessionChatCodexTokens {
  inputTokens?: number;
  cachedInputTokens?: number;
  cacheWriteInputTokens?: number;
  outputTokens?: number;
  reasoningOutputTokens?: number;
  totalTokens?: number;
}

export interface SessionChatCodexRateLimitWindow {
  usedPercentage?: number;
  windowMinutes?: number;
  resetsAt?: number;
}

export interface SessionChatCodexStatus {
  totalTokens?: SessionChatCodexTokens;
  turnTokens?: SessionChatCodexTokens;
  lastRequest?: SessionChatCodexTokens;
  primary?: SessionChatCodexRateLimitWindow;
  secondary?: SessionChatCodexRateLimitWindow;
  credits?: { hasCredits?: boolean; unlimited?: boolean; balance?: string };
  plan?: string;
  limitName?: string;
  model?: string;
  effort?: string;
  version?: string;
  provider?: string;
  currentDir?: string;
  sessionId?: string;
  parentThreadId?: string;
  forkedFromId?: string;
  startedAt?: string;
  approvalPolicy?: string;
  sandbox?: string;
  lastTurnDurationMs?: number;
  timeToFirstTokenMs?: number;
}

/** Cursor's statusline payload and the session checkout's git state, every field absent when unknown. */
export interface SessionChatCursorStatus {
  version?: string;
  currentDir?: string;
  projectDir?: string;
  worktree?: string;
  outputStyle?: string;
  totalOutputTokens?: number;
  autorun?: boolean;
  maxMode?: boolean;
  branch?: string;
  linesAdded?: number;
  linesRemoved?: number;
  prNumber?: number;
  prState?: string;
}

export interface SessionChatDetectedOptions {
  model?: SessionChatDetectedChoice;
  effort?: SessionChatDetectedChoice;
  /**
   * Claude's current Shift+Tab permission/input mode, or Codex's Plan
   * collaboration mode (`plan`; absent while Codex is in its default mode),
   * read from the agent's footer.
   */
  mode?: SessionChatDetectedChoice;
  /** Cursor's terminal-reported model context window, for example `272K` or `1M`. */
  contextWindow?: string;
  /** The complete normalized terminal line that supplied the detected values. */
  terminalStatusLine?: string;
  /** Cursor or Codex's terminal-reported Fast mode, or Claude's statusline-reported fast mode. */
  fast?: boolean;
  /** Context snapshot reported by Claude's statusline or Codex's transcript. */
  contextUsage?: SessionChatContextUsage;
  /** The rest of Claude's statusline payload the chat can show. */
  claudeStatus?: SessionChatClaudeStatus;
  codexStatus?: SessionChatCodexStatus;
  /** What Cursor handed its statusline command plus the checkout's git state (`session_chat_cursor_status.rs`). */
  cursorStatus?: SessionChatCursorStatus;
  /** ISO-8601 millis; compared against a pending dispatch's own timestamp. */
  detectedAt: string;
}

// ---------------------------------------------------------------------------
// Terminal-state notices
// ---------------------------------------------------------------------------

/*
CDXC:AgentScreenDetection 2026-08-19:
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
  /**
   * `trustAndRemember` is the folder-trust card's second button: gxserver
   * remembers the session's folders and answers this and every later trust
   * prompt on them itself, whichever agent asks. `restartAgent` sleeps and
   * wakes a session whose agent exited to the shell.
   */
  kind: 'switchToTerminal' | 'sendKeys' | 'recoverCodexConversation' | 'trustAndRemember' | 'restartAgent';
  /** Raw bytes for `sendKeys`, written verbatim through answerSessionChatPrompt. */
  send?: string;
}

/*
CDXC:SessionChat 2026-08-21:
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

export interface SessionChatConversationLock {
  conversationId: string | null;
  sessions: { projectId: string; sessionId: string }[];
}

export interface SessionChatTerminalNotice {
  conversationLock?: SessionChatConversationLock;
  /** Live Codex-owned menu or form, validated again before each input. */
  dialog?: SessionChatTerminalDialog;
  /**
   * Open set (`loginExpired`, `trustPrompt`, `permissionsWarning`,
   * `onboarding`, `usageLimit`, `streamError`, `updatePrompt`, `agentExited`, `agentError`,
   * `queuedInput`, `deliveryFailed`, `resumePrompt`, `switchConfirmPrompt`,
   * `sessionPausedPrompt`, `permissionPrompt`, `codexInputBlocked`, `claudeInputBlocked`, `cursorInputBlocked`, `grokInputBlocked`,
   * `hermesInputBlocked`, `ompInputBlocked`, `piInputBlocked`). Clients MUST
   * render an unknown kind generically; title/detail/severity are self-sufficient.
   */
  kind: string;
  severity: 'error' | 'warning' | 'info';
  /** Short human line, e.g. "Codex login expired". */
  title: string;
  /** One or two plain sentences: what happened, what to do. */
  detail?: string;
  /** The terminal's own line the card is about, drawn behind the card's Details toggle. */
  details?: string;
  /** SGR-stripped last visible lines (trimmed, capped ~2000 chars). */
  screenTail?: string;
  source: 'screen' | 'watchdog';
  /**
   * ISO-8601 millis; also the key a client's local dismissal remembers.
   * gxserver keeps it stable while the same notice is re-detected, including
   * across short gaps where a banner missed a probe.
   */
  detectedAt: string;
  actions?: SessionChatTerminalNoticeAction[];
  /**
   * Answerable picker rows, in screen order. Absent for every notice that only
   * describes a state; present means the card shows an answer picker.
   */
  choices?: SessionChatTerminalNoticeChoice[];
}

/*
CDXC:AgentScreenDetection 2026-08-22:
Live work the agent CLI reports on its terminal before transcript JSONL catches
up. `agent-stream` is the `⏺ …` message Claude is writing right now: `text`
carries the whole block as painted so far (stitched across probes once the
bullet scrolls off the full-screen grid; `label` is its first paragraph), and
the client shows it as the streaming assistant bubble until the transcript
carries the same text (packages/gx-chat-core/src/session/terminal.rs). `claude-status` is an
allowlisted star-marker line and becomes transient reasoning history in the
client; `claude-tool` is the row above a `⎿` output gutter, i.e. a tool call,
shown as a pending tool row at the bottom of the transcript and never in the
working strip; `shells-running` remains one bottom activity
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
  /** Open set (`compacting`, `agent-stream`, `claude-status`, `claude-tool`, `shells-running`). */
  kind: string;
  /** Agent-facing wording, without the spinner glyph or the clock. */
  label: string;
  /** 0-100, only when the screen actually painted a percentage. */
  percent?: number;
  /** Seconds the CLI reported, only when it painted them. */
  elapsedSeconds?: number;
  /** `compacting` only: the token counter painted after the clock (`↓ 901 tokens`), as shown. */
  tokens?: string;
  /** ISO-8601 millis; stable for the whole run, so a local clock can tick. */
  detectedAt: string;
  /** `claude-tool` only: the tool block painted under the row, as shown on the terminal. */
  detail?: string;
  /** `agent-stream` only: the message text painted so far; starts with `…` when its first rows scrolled off before the first probe stitched them. */
  text?: string;
  /**
   * `shells-running` only: the footer's count (the authority on how much runs) and the newest
   * launches Claude's transcript has not seen end, never more than `count`
   * (server/src/session_chat_claude_background.rs).
   */
  background?: {
    count: number;
    tasks: { id: string; kind: 'shell' | 'monitor'; title: string; command?: string; startedAt: string }[];
  };
}

/*
CDXC:SessionChat 2026-08-23:
Slash commands GHOSTEX typed into the agent without the composer:
provider-specific first-prompt auto-title jobs and the rename modal stage
`/rename <title>` (Pi `/name`, Hermes Agent `/title`), while forks
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
/*
CDXC:SessionChat 2026-09-04 DECISION:
User: a prompt Claude Code pulls back into its composer after an Escape must
come back into the chat composer too, and its bubble must leave the transcript,
so the user never writes a follow-up to a message the agent never took.
SEE-ALSO: server/src/session_chat_returned_prompt.rs,
packages/gx-chat-core/src/state/composer.rs.
*/
export interface SessionChatReturnedPrompt {
  /** Stable per detection; a client applies each id once. */
  id: string;
  /** Exactly what was sent from Chat. */
  text: string;
  /** Image attachments the send carried, if any. */
  imagePaths?: string[];
  /** ISO-8601 millis. */
  at: string;
}

/** The goal cell Codex printed for a `/goal` command. */
export interface SessionChatCodexGoal {
  /** Codex's own label: `active`, `paused`, `stalled`, `usage limited`, `limited by budget`, `complete`, `cleared`. */
  status: string;
  /** The objective as Codex echoed it, without the trailing usage summary. */
  objective: string;
  /** `Time: 2m · Tokens: 63.9K/50K`, when Codex appended usage. */
  usage?: string;
}

export interface SessionChatAppCommand {
  /** Codex's local command result, which its conversation transcript omits. */
  output?: string;
  /** Parsed from `output` for `/goal`, so the chat shows a goal card instead of raw screen text. */
  goal?: SessionChatCodexGoal;
  /** Stable within a session; two sends can carry identical text. */
  id: string;
  /** Verbatim command as written to the terminal, e.g. `/rename Fix parser`. */
  command: string;
  /** Assigned session title; arrives after agent metadata resolves a bare `/rename`. */
  title?: string;
  /**
   * The live half of a slash command the USER sent from chat, which gxserver
   * also archives and replays on the next read. Rendered as the same rows that
   * archive produces, so the look does not change under the reader when the
   * replay takes over.
   */
  localCommand?: boolean;
  /** Stable identity shared with the archived command and its output. */
  archiveId?: string;
  /** ISO-8601 millis. */
  sentAt: string;
}

/**
 * CDXC:SessionStatus 2026-09-10 WHY:
 * Both providers report persisted child lifecycle evidence, independently of the lead turn.
 * Omission clears the roster. Failed reads mark it stale; validUntil bounds animations when a connection stops delivering fresh observations.
 * SEE-ALSO: server/src/session_chat_agent_fleet.rs.
 */
export interface SessionChatSubAgent {
  /** Exact provider child identity; names need not be unique. */
  id?: string;
  /** Idle Claude children may remain in the terminal roster between turns. */
  status?: 'working' | 'idle';
  /** Current turn start, in Unix milliseconds; changes when a child resumes. */
  startedAt?: number;
  /** Agent type as the CLI names it (`general-purpose`). */
  name: string;
  /** Latest child-specific model and effort, supplied with the roster. */
  model?: string;
  effort?: string;
  /** Persisted task description. */
  task?: string;
  /** Provider elapsed time sampled at detectedAt; held still while idle. */
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
  /** Stable provider order, never empty. */
  agents: SessionChatSubAgent[];
  /** ISO-8601 millis, paired with this sample's elapsedSeconds. */
  detectedAt: string;
  /** The provider could not verify the last roster. */
  stale?: boolean;
  /** Clock/pulse lease, renewed only by successful provider observations. */
  validUntil?: string;
}

/*
CDXC:SessionChat 2026-09-03:
The task list Claude Code keeps for a session through its TaskCreate /
TaskUpdate tools, the block the CLI pins under its transcript and folds with
ctrl+t. The CURRENT list lives only in the CLI's on-disk task store
(`~/.claude/tasks/<session id>/<n>.json`), which gxserver reads
(server/src/session_chat_agent_tasks.rs) so the chat shows what the terminal
shows. Carried by read results and by snapshot/replaced/state frames with
`prompt` semantics: omitted ⇒ CLEARED. Never gated on the agent working: a
finished turn leaves its list behind, and that is exactly when the user reads
it to see what is left.
*/
export type SessionChatAgentTaskStatus = 'pending' | 'in_progress' | 'completed';

export interface SessionChatAgentTask {
  /** The CLI's own task number, also its file name. */
  id: string;
  subject: string;
  /** Present-continuous label the CLI paints while the task runs. */
  activeForm?: string;
  /** Verbatim from the store; anything unknown renders as pending. */
  status: SessionChatAgentTaskStatus | string;
  /** Ids of tasks that must finish before this one can start. */
  blockedBy?: string[];
}

export interface SessionChatAgentTasks {
  /** CLI numbering order, never empty: no tasks means no list at all. */
  tasks: SessionChatAgentTask[];
}
