import type { AccountSwitchProgress } from '@/packages/shared/agent-accounts';
import type { SessionChatPendingModelSelection } from '@/packages/shared/session-chat';
import type { SessionChatDraftVersion } from '@/packages/shared/session-chat-queue';
// useSessionChat — host-agnostic session-chat state machine.
// Consumes an injected SessionChatTransport; implements the seed read, frame
// folding with epoch/seq rules (drop dup seq, resnapshot on gap/epoch
// change), the 60s not-found/starting retry patience (upstream chat spec
// §5.13), load-earlier pagination, optimistic sends, and status derivation.
//
// Anti-drop law: the live list only ever grows. Reads window the history they
// seed; appends are never trimmed, because a trim removes the OLDEST rows and
// the pagination cursor cannot reach them again.

import type {
  GxserverAnswerSessionChatPromptParams,
  SessionChatAgentFleet,
  SessionChatAgentTasks,
  SessionChatAvailableAgent,
  SessionChatDetectedOptions,
  SessionChatDraft,
  SessionChatInteractivePrompt,
  SessionChatMessage,
  SessionChatQueuedPrompt,
  SessionChatReturnedPrompt,
  SessionChatSendKey,
  SessionChatStatus,
  SessionChatTerminalActivity,
  SessionChatTerminalNotice,
  SessionChatTurnLifecycle,
} from '../../../shared/session-chat';
import { type SessionChatQueueCapabilities } from '../session-chat-queue';
import type { SessionChatTransport } from '../session-chat-transport';
import { type SessionChatViewState } from '../session-chat-view-state';

// Client-side not-found/starting retry patience (upstream chat spec §5.13).
export const NOTFOUND_RETRY_DELAYS_MS = [1_000, 2_000, 4_000, 8_000] as const;
export const NOTFOUND_RETRY_FIXED_DELAY_MS = 10_000;
export const NOTFOUND_RETRY_WINDOW_MS = 60_000;

// A resync read answers from a stream position the server captured BEFORE it
// read the file, so frames landing while the read is in flight can outrun its
// result. One paced follow-up read covers those bytes; the cap stops a
// continuously streaming turn from turning follow-ups into a read loop.
export const RESYNC_FOLLOW_UP_DELAY_MS = 250;
export const MAX_RESYNC_FOLLOW_UPS = 4;

// Hook-level read deadline. The transport exposes no abort handle, so this
// does not cancel the underlying request — it settles the STATE MACHINE. A
// read that never resolves would otherwise pin `resyncInFlightRef` true, and
// every later gap verdict would early-return out of requestResync: the frozen
// chat view.
export const READ_TIMEOUT_MS = 30_000;

// A resync read that fails (including by timeout) must keep retrying, because
// nothing else re-reads: the gap that asked for it has already been consumed.
export const RESYNC_RETRY_DELAYS_MS = [1_000, 2_000, 4_000, 8_000] as const;
export const RESYNC_RETRY_MAX_DELAY_MS = 15_000;

// Liveness floor. A silently dead follower or WebSocket delivers no frames at
// all, so no gap is ever observed and the fold rules never fire. While the
// view expects frames, this long without one means re-read rather than wait.
export const STALL_THRESHOLD_MS = 20_000;
export const STALL_CHECK_INTERVAL_MS = 5_000;

// Initial-window liveness floor. Before the subscription has delivered its
// first authoritative frame the pane is holding blank, and a resync read
// cannot recover a socket whose subscribe snapshot was lost — only a fresh
// socket can. Shorter than STALL_THRESHOLD_MS because nothing is on screen yet.
export const INITIAL_STALL_THRESHOLD_MS = 15_000;
// Bound on automatic socket recycles per session mount. A transport that is
// simply down would otherwise reconnect forever; past this the manual Retry
// button (surfaced by the view's loading indicator) is the only recovery.
export const MAX_AUTOMATIC_RECONNECTS = 2;

export interface SessionChatStreamPosition {
  epoch: number;
  seq: number;
}

export function isAheadOf(candidate: SessionChatStreamPosition, reference: SessionChatStreamPosition): boolean {
  return candidate.epoch > reference.epoch || (candidate.epoch === reference.epoch && candidate.seq > reference.seq);
}

export function notFoundRetryDelayMs(attempt: number): number {
  return NOTFOUND_RETRY_DELAYS_MS[attempt] ?? NOTFOUND_RETRY_FIXED_DELAY_MS;
}

export function resyncRetryDelayMs(attempt: number): number {
  return RESYNC_RETRY_DELAYS_MS[attempt] ?? RESYNC_RETRY_MAX_DELAY_MS;
}

export function withReadTimeout<T>(read: Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error('Session chat read timed out.'));
    }, READ_TIMEOUT_MS);
    read.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (readError: unknown) => {
        clearTimeout(timer);
        reject(readError instanceof Error ? readError : new Error(String(readError)));
      }
    );
  });
}

export function normalizedSessionChatText(message: SessionChatMessage): string {
  return message.blocks
    .filter((block) => block.type === 'text')
    .map((block) => (block.type === 'text' ? block.text : ''))
    .join('\n\n')
    .replace(/\s+/g, ' ')
    .trim();
}

export interface FrameState {
  epoch: number | null;
  seq: number;
  frameArrived: boolean;
}

export interface UseSessionChatOptions {
  transport: SessionChatTransport;
  /** Live assistant preview text from the host's hook status, if available. */
  previewText?: string | null;
  /** Optional external live-work signal merged with the server status. */
  working?: boolean;
  /** Verified command catalog for local "Ran /x" markers. */
  commandCatalog?: readonly string[];
  initialLimit?: number;
  /**
   * Host diagnostic breadcrumb sink; the host gates it behind its own
   * scenario, so calls are cheap and carry only enums, counts, and booleans.
   */
  diagnosticLog?: (event: string, details?: Record<string, unknown>) => void;
}

/*
CDXC:SessionChat 2026-08-21:
Ghostex's own prompt queue (plan 016) and the cross-client composer draft. The
hook owns both because both arrive on the same frames the transcript does.

`capabilities.supported` is false until a read result or a snapshot/replaced/
state frame CARRIES a `queue` field — that presence, even as an empty array, is
the daemon capability probe. Every mutation below is a no-op while its
capability is false, and the UI hides the matching control instead of calling
an endpoint that would 404.
*/
export interface SessionChatQueueController {
  capabilities: SessionChatQueueCapabilities;
  /** Authoritative queue, head first. Empty while supported but nothing waits. */
  prompts: readonly SessionChatQueuedPrompt[];
  /** Appends text at the end of the queue (Tab / long-press on Send). */
  queuePrompt: (text: string, draftVersion?: SessionChatDraftVersion) => Promise<void>;
  /** Moves a failed row back to queued and clears its error. */
  retryPrompt: (promptId: string) => Promise<void>;
  /** Deletes a row and resolves with it, so Edit can reuse the text. */
  removePrompt: (promptId: string) => Promise<SessionChatQueuedPrompt | null>;
  /** Commits a drag with the full id list, head first (applied optimistically). */
  reorder: (promptIds: string[]) => Promise<void>;
  /** Delivers one row immediately, exactly like pressing Enter. */
  sendNow: (promptId: string) => Promise<void>;
}

export interface SessionChatDraftController {
  /** This client's opaque id, echoed back as a draft's originClientId. */
  clientId: string;
  /** Whether this host can push at all; false means local-only drafts. */
  canSync: boolean;
  /** Latest draft gxserver reported, from any device. Null ⇒ none seen. */
  synced: SessionChatDraft | null;
  /**
   * Pushes the unsent composer text. Called on blur / session switch /
   * unmount / backgrounding — never per keystroke. An empty string clears.
   */
  push: (content: string, draftVersion?: SessionChatDraftVersion) => Promise<void>;
}

export interface UseSessionChatResult {
  view: SessionChatViewState;
  status: SessionChatStatus;
  /** Composed list: transcript + markers + streaming bubble + pending echoes. */
  messages: SessionChatMessage[];
  lifecycle: SessionChatTurnLifecycle | null;
  prompt: SessionChatInteractivePrompt | null;
  working: boolean;
  /**
   * The raw live signal — server status/working frames plus the host's hook —
   * BEFORE the lifecycle settle folded into `working`. A terminal turn
   * lifecycle (or trailing-prose recovery) settles `working` so Stop-vs-Send
   * and the typing indicator cannot get stuck, but the session process may
   * still be running then (hooks, background tasks, an immediate follow-up
   * turn) and the session status the user sees still says "working". The
   * transcript keys off THIS so it never settles a turn — folding it into
   * "Worked for Xs" — while any live signal still reports the session busy.
   */
  workingSignal: boolean;
  /**
   * CDXC:SessionStatus 2026-09-04 DECISION:
   * User: the working strip above the composer and the sidebar's working
   * spinner must derive from the same source so they always match and never
   * desync. This is that source: the session activity gxserver presents (the
   * sidebar's `activity === 'working'`), untouched by the transcript lifecycle
   * settle, the local Stop suppression, the settle hold, and the optimistic
   * send echo that shape `working` for Stop-vs-Send and the transcript fold.
   */
  sessionWorking: boolean;
  /**
   * Model/effort gxserver read out of the agent's own terminal, when it could
   * detect them. Null while nothing has been detected — the option pills then
   * keep their local truth.
   */
  selectedOptions: SessionChatDetectedOptions | null;
  /*
  CDXC:AgentScreenDetection 2026-08-19:
  Blocking/failed terminal state gxserver classified off the agent's screen (or
  the send watchdog). Follows `prompt` semantics: a frame that can carry it and
  does not means CLEARED, so this drops back to null on its own.
  */
  terminalNotice: SessionChatTerminalNotice | null;
  /**
   * The prompt Claude Code handed back to its composer after an Escape, for
   * the view to put back into the chat composer (once per id). Null until a
   * read or frame carries one; a later omission leaves it as is.
   */
  returnedPrompt: SessionChatReturnedPrompt | null;
  /**
   * Live on-screen progress (compaction). Follows `terminalNotice` semantics:
   * a frame that can carry it and does not has CLEARED it.
   */
  terminalActivity: SessionChatTerminalActivity | null;
  /**
   * Sub-agents Claude is running, from the same screen and with the same
   * cleared-on-omission rule. Never gated on `working`: these outlive the turn
   * that spawned them, so an idle agent is exactly when this still has rows.
   */
  agentFleet: SessionChatAgentFleet | null;
  /**
   * Claude's task list, read from its on-disk task store, with the same
   * cleared-on-omission rule. Never gated on `working`.
   */
  agentTasks: SessionChatAgentTasks | null;
  /**
   * True once gxserver has actually read this session's screen. Latched: a
   * later frame that omits it does NOT unset it, because "we have looked" does
   * not stop being true. The option pills use it to decide between a loading
   * skeleton and a plain unset pill.
   */
  screenProbed: boolean;
  agent: string | null;
  agentSessionId: string | null;
  /*
  CDXC:Drafts 2026-08-28:
  The draft-only agent switcher's two inputs, and the only chat state that is
  carried by READ RESULTS ALONE: no frame type has a field for either, so they
  are folded in `refresh`/seed reads and left untouched by every frame. Null
  means "this session is not a draft" — including a draft that was just
  promoted by its first Send, whose next read simply stops carrying them.
  */
  availableAgents: readonly SessionChatAvailableAgent[] | null;
  /**
   * CDXC:AgentProviders 2026-09-03:
   * The same-family accounts a PROMPTED session can be resumed under, carried
   * by reads exactly like `availableAgents`. Null when nothing is compatible.
   */
  switchableAgents: readonly SessionChatAvailableAgent[] | null;
  /** The session's own launch agent id (never the transcript family). */
  sessionAgentId: string | null;
  /**
   * Re-reads the authoritative state now. Callers use it after an action whose
   * result lives only in a read result — switching a draft's agent, or a send
   * that may have promoted the draft — because no frame carries those fields.
   */
  refresh: () => void;
  error: string | null;
  hasMore: boolean;
  /** Authoritative byte cursor after the most recently accepted history window. */
  earlierPageCursor: number;
  loadingEarlier: boolean;
  loadEarlier: () => void;
  send: (text: string, imagePaths?: string[], draftVersion?: SessionChatDraftVersion) => Promise<void>;
  /**
   * Raw keystroke injection for agent-owned TUI controls. Undefined when the
   * host transport cannot deliver keys, so callers hide the control instead
   * of pretending it works.
   */
  sendKey?: (key: SessionChatSendKey, marker: string) => Promise<void>;
  answerPrompt: (params: Omit<GxserverAnswerSessionChatPromptParams, 'projectId' | 'sessionId'>) => Promise<void>;
  interrupt: () => Promise<void>;
  /**
   * Tear the subscription down and rebuild it: a fresh `transport.subscribe`
   * plus a fresh seed read. The only recovery for a socket that came up but
   * never delivered its subscribe snapshot, which no re-read can repair.
   */
  retry: () => void;
  /** Ghostex prompt queue: rows the agent has never seen (plan 016). */
  queue: SessionChatQueueController;
  accountSwitch: AccountSwitchProgress | null;
  pendingModelSelection: SessionChatPendingModelSelection | null | undefined;
  /** Cross-client composer draft sync. */
  draft: SessionChatDraftController;
}
