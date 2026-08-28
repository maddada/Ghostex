// useSessionChat — host-agnostic session-chat state machine.
// Consumes an injected SessionChatTransport; implements the seed read, frame
// folding with epoch/seq rules (drop dup seq, resnapshot on gap/epoch
// change), the 60s not-found/starting retry patience (upstream chat spec
// §5.13), load-earlier pagination, optimistic sends, and status derivation.
//
// Anti-drop law: the live list only ever grows. Reads window the history they
// seed; appends are never trimmed, because a trim removes the OLDEST rows and
// the pagination cursor cannot reach them again.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type {
  GxserverAnswerSessionChatPromptParams,
  GxserverReadSessionChatResult,
  GxserverSessionChatEvent,
  SessionChatAgentFleet,
  SessionChatAppCommand,
  SessionChatAvailableAgent,
  SessionChatDetectedOptions,
  SessionChatDraft,
  SessionChatInteractivePrompt,
  SessionChatMessage,
  SessionChatQueuedPrompt,
  SessionChatSendKey,
  SessionChatStatus,
  SessionChatTerminalActivity,
  SessionChatTerminalNotice,
  SessionChatTurnLifecycle,
} from '../../shared/session-chat';
import {
  applySessionChatAppends,
  createIncrementalSessionChatAssembler,
  resetIncrementalSessionChatAssembler,
  sessionChatIdCollides,
  sessionChatSharesPrefix,
  stampSessionChatArrivalOrder,
} from './session-chat-assembler';
import {
  applySessionChatMergerAppend,
  createSessionChatMerger,
  removeSessionChatMergerIds,
  replaceSessionChatMergerList,
  type SessionChatMerger,
} from './session-chat-merge';
import { countSessionChatCompactionRecords } from './session-chat-noise';
import {
  appendSessionChatCommandMarker,
  applySessionChatCommandMarkerBoundaries,
  assignSessionChatPendingOccurrence,
  nextSessionChatPendingSendId,
  pruneSessionChatPendingSends,
  SESSION_CHAT_PENDING_SEND_LIMIT,
  sessionChatAppCommandsAsMessages,
  sessionChatCommandMarkersAsMessages,
  sessionChatPendingSendsAsMessages,
  visibleSessionChatPendingSends,
  type SessionChatCommandMarker,
  type SessionChatPendingSend,
} from './session-chat-pending';
import {
  sessionChatPageHasMore,
  SESSION_CHAT_INITIAL_LIMIT,
  SESSION_CHAT_MAX_LIMIT,
  SESSION_CHAT_PAGE,
} from './session-chat-pagination';
import { classifySessionChatSend, SESSION_CHAT_DEFAULT_COMMAND_CATALOG } from './session-chat-send-classification';
import { deriveSessionChatStreamingText, sessionChatStreamingMessage } from './session-chat-streaming';
import { surfaceSkillInvocationUserTurns } from './session-chat-command-envelope';
import {
  moveSessionChatQueueRow,
  sessionChatDraftClientId,
  sessionChatQueueCapabilities,
  type SessionChatQueueCapabilities,
} from './session-chat-queue';
import type { SessionChatTransport } from './session-chat-transport';
import { selectSessionChatViewState, type SessionChatViewState } from './session-chat-view-state';
import { deriveSessionChatWorkingOverride } from './session-chat-working-status';

// Client-side not-found/starting retry patience (upstream chat spec §5.13).
const NOTFOUND_RETRY_DELAYS_MS = [1_000, 2_000, 4_000, 8_000] as const;
const NOTFOUND_RETRY_FIXED_DELAY_MS = 10_000;
const NOTFOUND_RETRY_WINDOW_MS = 60_000;

// A resync read answers from a stream position the server captured BEFORE it
// read the file, so frames landing while the read is in flight can outrun its
// result. One paced follow-up read covers those bytes; the cap stops a
// continuously streaming turn from turning follow-ups into a read loop.
const RESYNC_FOLLOW_UP_DELAY_MS = 250;
const MAX_RESYNC_FOLLOW_UPS = 4;

// Hook-level read deadline. The transport exposes no abort handle, so this
// does not cancel the underlying request — it settles the STATE MACHINE. A
// read that never resolves would otherwise pin `resyncInFlightRef` true, and
// every later gap verdict would early-return out of requestResync: the frozen
// chat view.
const READ_TIMEOUT_MS = 30_000;

// A resync read that fails (including by timeout) must keep retrying, because
// nothing else re-reads: the gap that asked for it has already been consumed.
const RESYNC_RETRY_DELAYS_MS = [1_000, 2_000, 4_000, 8_000] as const;
const RESYNC_RETRY_MAX_DELAY_MS = 15_000;

// Liveness floor. A silently dead follower or WebSocket delivers no frames at
// all, so no gap is ever observed and the fold rules never fire. While the
// view expects frames, this long without one means re-read rather than wait.
const STALL_THRESHOLD_MS = 20_000;
const STALL_CHECK_INTERVAL_MS = 5_000;

// Initial-window liveness floor. Before the subscription has delivered its
// first authoritative frame the pane is holding blank, and a resync read
// cannot recover a socket whose subscribe snapshot was lost — only a fresh
// socket can. Shorter than STALL_THRESHOLD_MS because nothing is on screen yet.
const INITIAL_STALL_THRESHOLD_MS = 15_000;
// Bound on automatic socket recycles per session mount. A transport that is
// simply down would otherwise reconnect forever; past this the manual Retry
// button (surfaced by the view's loading indicator) is the only recovery.
const MAX_AUTOMATIC_RECONNECTS = 2;

const CLAUDE_TERMINAL_STATUS_KIND = 'claude-status';

interface SessionChatStreamPosition {
  epoch: number;
  seq: number;
}

function isAheadOf(candidate: SessionChatStreamPosition, reference: SessionChatStreamPosition): boolean {
  return candidate.epoch > reference.epoch || (candidate.epoch === reference.epoch && candidate.seq > reference.seq);
}

function notFoundRetryDelayMs(attempt: number): number {
  return NOTFOUND_RETRY_DELAYS_MS[attempt] ?? NOTFOUND_RETRY_FIXED_DELAY_MS;
}

function resyncRetryDelayMs(attempt: number): number {
  return RESYNC_RETRY_DELAYS_MS[attempt] ?? RESYNC_RETRY_MAX_DELAY_MS;
}

function withReadTimeout<T>(read: Promise<T>): Promise<T> {
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

function normalizedSessionChatText(message: SessionChatMessage): string {
  return message.blocks
    .filter((block) => block.type === 'text')
    .map((block) => (block.type === 'text' ? block.text : ''))
    .join('\n\n')
    .replace(/\s+/g, ' ')
    .trim();
}

function terminalStatusMessage(activity: SessionChatTerminalActivity): SessionChatMessage | null {
  const text = activity.label.trim();
  if (activity.kind !== CLAUDE_TERMINAL_STATUS_KIND || !text) {
    return null;
  }
  const timestamp = Date.parse(activity.detectedAt);
  return {
    id: `terminal-status:${activity.detectedAt}:${text}`,
    role: 'reasoning',
    blocks: [{ type: 'text', text }],
    timestamp: Number.isNaN(timestamp) ? Date.now() : timestamp,
    source: 'hook',
  };
}

interface FrameState {
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
CDXC:SessionChatPromptQueue 2026-08-21:
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
  queuePrompt: (text: string) => Promise<void>;
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
  push: (content: string) => Promise<void>;
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
   * Model/effort gxserver read out of the agent's own terminal, when it could
   * detect them. Null while nothing has been detected — the option pills then
   * keep their local truth.
   */
  selectedOptions: SessionChatDetectedOptions | null;
  /*
  CDXC:SessionChatTerminalNotices 2026-08-19:
  Blocking/failed terminal state gxserver classified off the agent's screen (or
  the send watchdog). Follows `prompt` semantics: a frame that can carry it and
  does not means CLEARED, so this drops back to null on its own.
  */
  terminalNotice: SessionChatTerminalNotice | null;
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
   * True once gxserver has actually read this session's screen. Latched: a
   * later frame that omits it does NOT unset it, because "we have looked" does
   * not stop being true. The option pills use it to decide between a loading
   * skeleton and a plain unset pill.
   */
  screenProbed: boolean;
  agent: string | null;
  agentSessionId: string | null;
  /*
  CDXC:DraftSessions 2026-08-28:
  The draft-only agent switcher's two inputs, and the only chat state that is
  carried by READ RESULTS ALONE: no frame type has a field for either, so they
  are folded in `refresh`/seed reads and left untouched by every frame. Null
  means "this session is not a draft" — including a draft that was just
  promoted by its first Send, whose next read simply stops carrying them.
  */
  availableAgents: readonly SessionChatAvailableAgent[] | null;
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
  loadingEarlier: boolean;
  loadEarlier: () => void;
  send: (text: string, imagePaths?: string[]) => Promise<void>;
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
  /** Cross-client composer draft sync. */
  draft: SessionChatDraftController;
}

export function useSessionChat(options: UseSessionChatOptions): UseSessionChatResult {
  const {
    commandCatalog = SESSION_CHAT_DEFAULT_COMMAND_CATALOG,
    diagnosticLog,
    initialLimit = SESSION_CHAT_INITIAL_LIMIT,
    previewText = null,
    transport,
    working: externalWorking = false,
  } = options;
  const diagnosticLogRef = useRef(diagnosticLog);
  diagnosticLogRef.current = diagnosticLog;

  const [transcript, setTranscript] = useState<readonly SessionChatMessage[]>([]);
  const [serverStatus, setServerStatus] = useState<SessionChatStatus>('loading');
  const [lifecycle, setLifecycle] = useState<SessionChatTurnLifecycle | null>(null);
  const [prompt, setPrompt] = useState<SessionChatInteractivePrompt | null>(null);
  const [agent, setAgent] = useState<string | null>(null);
  const [agentSessionId, setAgentSessionId] = useState<string | null>(null);
  /*
  CDXC:DraftSessions 2026-08-28: read-result-only state (see the doc on
  UseSessionChatResult.availableAgents). Both are set from a read and from
  nothing else, so an omission on a read is authoritative: the session is not
  (or is no longer) a draft.
  */
  const [availableAgents, setAvailableAgents] = useState<readonly SessionChatAvailableAgent[] | null>(null);
  const [sessionAgentId, setSessionAgentId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const [pending, setPending] = useState<readonly SessionChatPendingSend[]>([]);
  const [markers, setMarkers] = useState<readonly SessionChatCommandMarker[]>([]);
  const [interrupted, setInterrupted] = useState(false);
  // Live work as reported by the chat channel itself: the `working` flag on
  // read results/snapshots plus the server's activity-transition state frames.
  const [serverWorking, setServerWorking] = useState(false);
  // Detected model/effort: carried by read results and by
  // snapshot/replaced/state frames. Absent ⇒ unchanged (older daemons omit it).
  const [selectedOptions, setSelectedOptions] = useState<SessionChatDetectedOptions | null>(null);
  // Terminal-state notice: carried by read results and by
  // snapshot/replaced/state frames. Omitted ⇒ CLEARED (prompt semantics, unlike
  // selectedOptions) — the server only stops sending it once the state is gone.
  const [terminalNotice, setTerminalNotice] = useState<SessionChatTerminalNotice | null>(null);
  /*
  CDXC:SessionChatTerminalActivity 2026-08-22: structured on-screen progress
  (compaction), carried and cleared exactly like the notice above. Claude's
  current `⏺` line is split into transient reasoning history below instead.
  */
  const [terminalActivity, setTerminalActivity] = useState<SessionChatTerminalActivity | null>(null);
  // CDXC:SessionChatAgentFleet 2026-08-23: carried and cleared exactly like the
  // activity row above; the strip's clocks tick locally off `detectedAt`.
  const [agentFleet, setAgentFleet] = useState<SessionChatAgentFleet | null>(null);
  /*
  CDXC:SessionChatAppCommands 2026-08-23: commands Ghostex typed into the agent
  itself. NOT prompt semantics — a frame that omits them leaves what we have,
  because the server retires them on its own TTL and an omission is far more
  often "this frame had nothing to add" than "that rename never happened".
  */
  const [appCommands, setAppCommands] = useState<readonly SessionChatAppCommand[]>([]);
  // Claude replaces its current `⏺ …` terminal line in place. Keep each
  // DISTINCT value only for this mounted chat; matching transcript text removes
  // it from composition as soon as JSONL catches up. Distinct rather than
  // merely non-repeating: the line cycles back to an earlier phrase between
  // lines that the transcript later swallows, so a "differs from the previous
  // one" rule leaves the same phrase standing several times in a row.
  const [terminalStatusMessages, setTerminalStatusMessages] = useState<readonly SessionChatMessage[]>([]);
  /*
  CDXC:SessionChatScreenProbed 2026-08-22: "gxserver has read this screen",
  carried by read results and by snapshot/replaced/state frames. Latched rather
  than cleared-on-omission: unlike the notice and the activity row, this is not
  a description of the screen that can stop being true, and an older daemon
  that never sends it simply leaves the pills in their pre-probe state.
  */
  const [screenProbed, setScreenProbed] = useState(false);
  /*
  Ghostex prompt queue. `null` means NO frame has carried a `queue` field yet,
  which is the "old daemon / not supported" state and hides every queue
  control. An empty array means supported-and-empty. Once present it is
  authoritative and replaces the list wholesale.
  */
  const [queuePrompts, setQueuePrompts] = useState<readonly SessionChatQueuedPrompt[] | null>(null);
  /*
  Latest synced composer draft. An OMITTED draft means unchanged, NOT cleared
  (see CDXC:SessionChatQueueCarriage) — so this only ever moves forward, and a
  clear arrives as an explicit empty `content`.
  */
  const [syncedDraft, setSyncedDraft] = useState<SessionChatDraft | null>(null);
  /*
  Bumping this re-runs the subscribe effect, which is the full recycle: the old
  socket is unsubscribed, a new one is opened, and a fresh seed read starts.
  Both the initial-window watchdog and the view's Retry button drive it.
  */
  const [reconnectNonce, setReconnectNonce] = useState(0);

  const mergerRef = useRef<SessionChatMerger>(createSessionChatMerger());
  const assemblerRef = useRef(createIncrementalSessionChatAssembler());
  const appliedRef = useRef<readonly SessionChatMessage[]>([]);
  const frameStateRef = useRef<FrameState>({ epoch: null, frameArrived: false, seq: 0 });
  const limitRef = useRef(initialLimit);
  const beforeOffsetRef = useRef(0);
  const closedRef = useRef(false);
  /**
   * Bumped every time the subscription is rebuilt (session/transport change).
   * A read that was in flight across the swap must not apply its result: it
   * belongs to the previous conversation.
   */
  const generationRef = useRef(0);
  const resyncInFlightRef = useRef(false);
  /** Newest frame position observed while a resync read was in flight. */
  const resyncSeenInFlightRef = useRef<SessionChatStreamPosition | null>(null);
  const resyncFollowUpsRef = useRef(0);
  const resyncFollowUpTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  /**
   * Failure retry, kept strictly apart from the follow-up above: a follow-up
   * chases bytes a SUCCESSFUL read outran, this one re-attempts a read that
   * never landed. Sharing a counter would let one exhaust the other's budget.
   */
  const resyncFailuresRef = useRef(0);
  const resyncRetryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const loadEarlierEpochRef = useRef<number | null>(null);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const workingRef = useRef(false);
  const workingStartedAtRef = useRef<number | null>(null);
  /** Last moment this session got a frame, or a read result that stood in for one. */
  const lastFrameAtRef = useRef(Date.now());
  const lastWatchdogResyncAtRef = useRef(0);
  /**
   * The watchdog's "frames are expected" input. A ref because the subscribe
   * effect must not tear the subscription down every time work starts or ends.
   */
  const workingSignalRef = useRef(false);
  /**
   * The initial-window watchdog's "the pane is still holding blank" input. A
   * ref for the same reason `workingSignalRef` is one.
   */
  const loadingHoldRef = useRef(true);
  /** Automatic recycles spent on this session mount (reset per transport). */
  const autoReconnectsRef = useRef(0);
  /** Transport that last seeded the view state; gates the session-identity wipe. */
  const seededTransportRef = useRef<SessionChatTransport | null>(null);
  /*
  CDXC:DraftAgentSwitch 2026-08-28:
  The launch agent id the last read carried, so a read can tell "this draft is
  now running a different agent CLI" from "this is the first read". A ref, not
  the state above, because the comparison happens inside the fold itself.
  */
  const sessionAgentIdRef = useRef<string | null>(null);

  const reconnect = useCallback((): void => {
    diagnosticLogRef.current?.('sessionChat.reconnect');
    setReconnectNonce((nonce) => nonce + 1);
  }, []);

  // The recycle budget is per session, not per recycle: a new transport is a
  // new conversation and gets its own two automatic attempts.
  useEffect(() => {
    autoReconnectsRef.current = 0;
  }, [transport]);

  const applyTerminalActivity = useCallback((activity: SessionChatTerminalActivity | undefined): void => {
    const transient = activity ? terminalStatusMessage(activity) : null;
    if (!transient) {
      setTerminalActivity(activity ?? null);
      return;
    }
    setTerminalActivity(null);
    setTerminalStatusMessages((current) => {
      const text = normalizedSessionChatText(transient);
      if (current.some((message) => normalizedSessionChatText(message) === text)) {
        return current;
      }
      return [...current, transient];
    });
  }, []);

  /**
   * Folds the two queue-carriage fields with their DIFFERENT omission rules:
   * an absent `queue` leaves the capability (and the list) exactly as it was,
   * an absent `draft` means unchanged. Neither one ever clears anything —
   * clearing a draft is an explicit empty `content` from the server.
   */
  const applyQueueCarriage = useCallback(
    (carrier: { queue?: SessionChatQueuedPrompt[]; draft?: SessionChatDraft }): void => {
      if (carrier.queue !== undefined) {
        setQueuePrompts(carrier.queue);
      }
      if (carrier.draft !== undefined) {
        setSyncedDraft(carrier.draft);
      }
    },
    []
  );

  const applyAuthoritative = useCallback(
    (
      result: {
        messages: SessionChatMessage[];
        lifecycle?: SessionChatTurnLifecycle;
        hasMore: boolean;
        hasMoreExact?: boolean;
        beforeOffset: number;
        status: SessionChatStatus;
        prompt?: SessionChatInteractivePrompt;
        agent?: string;
        agentSessionId?: string;
        error?: string;
        /** Hook-derived live-work flag carried by reads and snapshots. */
        working?: boolean;
        /** Detected model/effort; omitted when the agent's screen said nothing. */
        selectedOptions?: SessionChatDetectedOptions;
        /** Blocking/failed terminal state; omitted ⇒ cleared. */
        terminalNotice?: SessionChatTerminalNotice;
        /** Live on-screen progress; omitted ⇒ cleared. */
        terminalActivity?: SessionChatTerminalActivity;
        /** Sub-agents on screen; omitted ⇒ cleared. */
        agentFleet?: SessionChatAgentFleet;
        /** Commands Ghostex sent; omitted ⇒ unchanged, never cleared. */
        appCommands?: SessionChatAppCommand[];
        /** gxserver has read the screen; latched, never cleared by omission. */
        screenProbed?: boolean;
        /** Ghostex prompt queue; PRESENT (even empty) is the capability probe. */
        queue?: SessionChatQueuedPrompt[];
        /** Synced composer draft; omitted ⇒ unchanged, never cleared. */
        draft?: SessionChatDraft;
      },
      source: string
    ): void => {
      diagnosticLogRef.current?.('sessionChat.authoritative', {
        hasAgentSessionId: result.agentSessionId !== undefined,
        hasMore: result.hasMore,
        messageCount: result.messages.length,
        source,
        status: result.status,
        working: result.working === true,
      });
      replaceSessionChatMergerList(mergerRef.current, result.messages);
      setTranscript(mergerRef.current.list);
      setLifecycle(result.lifecycle ?? null);
      setHasMore(sessionChatPageHasMore(result));
      beforeOffsetRef.current = result.beforeOffset;
      setServerStatus(result.status);
      setServerWorking(result.working === true || result.status === 'working');
      setPrompt(result.prompt ?? null);
      if (result.agent !== undefined) {
        setAgent(result.agent);
      }
      setAgentSessionId(result.agentSessionId ?? null);
      if (result.selectedOptions) {
        setSelectedOptions(result.selectedOptions);
      }
      setTerminalNotice(result.terminalNotice ?? null);
      applyTerminalActivity(result.terminalActivity);
      setAgentFleet(result.agentFleet ?? null);
      if (result.appCommands) {
        setAppCommands(result.appCommands);
      }
      if (result.screenProbed) {
        setScreenProbed(true);
      }
      applyQueueCarriage(result);
      setError(result.status === 'error' ? (result.error ?? 'Conversation could not be loaded.') : null);
      // A fresh authoritative generation cancels an in-flight older page.
      loadEarlierEpochRef.current = null;
      setLoadingEarlier(false);
    },
    [applyQueueCarriage, applyTerminalActivity]
  );

  /*
  CDXC:DraftSessions 2026-08-28:
  Folded from READ RESULTS ONLY — snapshot/replaced/state frames have no field
  for either value, so folding them in `applyAuthoritative` (which frames also
  go through) would clear the switcher on the next frame. An omission on a read
  is the promotion signal and clears both.
  */
  const applyDraftAgentCarriage = useCallback(
    (result: { availableAgents?: SessionChatAvailableAgent[]; sessionAgentId?: string }): void => {
      const nextSessionAgentId = result.sessionAgentId ?? null;
      const previousSessionAgentId = sessionAgentIdRef.current;
      sessionAgentIdRef.current = nextSessionAgentId;
      setAvailableAgents(result.availableAgents ?? null);
      setSessionAgentId(nextSessionAgentId);
      /*
      CDXC:DraftAgentSwitch 2026-08-28:
      A draft that came back running a DIFFERENT agent CLI has no detected
      options any more: the model, effort and mode this chat is holding belong
      to the CLI that was just replaced. Both are dropped here rather than left
      to be overwritten, because the new agent may name none of them — and the
      probe latch has to go with them, or the pill would present the previous
      agent's model as a settled answer instead of showing that the new one has
      not been read yet. Applied only on an id-to-id change: the first read of a
      session (null → id) is this chat learning its own identity, and it runs
      right after `applyAuthoritative` has folded that same read's detection.
      */
      if (
        previousSessionAgentId !== null &&
        nextSessionAgentId !== null &&
        previousSessionAgentId !== nextSessionAgentId
      ) {
        setSelectedOptions(null);
        setScreenProbed(false);
      }
    },
    []
  );

  const requestResync = useCallback((): void => {
    if (resyncInFlightRef.current || closedRef.current) {
      // Frames arriving from here on are recorded by onEvent and covered by
      // the follow-up read this flight schedules.
      return;
    }
    resyncInFlightRef.current = true;
    resyncSeenInFlightRef.current = null;
    diagnosticLogRef.current?.('sessionChat.resyncRequested');
    const generation = generationRef.current;
    void withReadTimeout(transport.read({ limit: limitRef.current }))
      .then((result) => {
        if (closedRef.current || generationRef.current !== generation) {
          return;
        }
        // The read landed, so the stream is answering again: drop the failure
        // backoff and count this as liveness for the watchdog.
        resyncFailuresRef.current = 0;
        lastFrameAtRef.current = Date.now();
        if (resyncRetryTimerRef.current !== null) {
          clearTimeout(resyncRetryTimerRef.current);
          resyncRetryTimerRef.current = null;
        }
        const observed = resyncSeenInFlightRef.current;
        const readPosition: SessionChatStreamPosition = {
          epoch: result.epoch,
          seq: result.seq,
        };
        const outrun = observed !== null && isAheadOf(observed, readPosition);
        if (outrun && observed.epoch > readPosition.epoch) {
          // A newer generation already replaced the tail; this result is from
          // the previous one and must not clobber it.
          scheduleResyncFollowUp();
          return;
        }
        const frameState = frameStateRef.current;
        frameState.epoch = result.epoch;
        // Frames seen during the flight were already accounted for; keeping
        // the cursor at the read's older seq would make every following
        // append look like a gap and resync forever.
        frameState.seq = outrun ? observed.seq : result.seq;
        applyAuthoritative(result, 'resyncRead');
        applyDraftAgentCarriage(result);
        if (outrun) {
          scheduleResyncFollowUp();
        } else {
          resyncFollowUpsRef.current = 0;
        }
      })
      .catch(() => {
        if (!closedRef.current && generationRef.current === generation) {
          setError('Conversation could not be loaded.');
          setServerStatus('error');
          scheduleResyncRetry();
        }
      })
      .finally(() => {
        if (generationRef.current === generation) {
          resyncInFlightRef.current = false;
        }
      });

    // Backoff, never giving up while mounted: the next read is the only thing
    // that can clear the error state (applyAuthoritative does it on success).
    function scheduleResyncRetry(): void {
      if (closedRef.current || generationRef.current !== generation || resyncRetryTimerRef.current !== null) {
        return;
      }
      const delay = resyncRetryDelayMs(resyncFailuresRef.current);
      resyncFailuresRef.current += 1;
      resyncRetryTimerRef.current = setTimeout(() => {
        resyncRetryTimerRef.current = null;
        requestResync();
      }, delay);
    }

    function scheduleResyncFollowUp(): void {
      if (
        closedRef.current ||
        generationRef.current !== generation ||
        resyncFollowUpTimerRef.current !== null ||
        resyncFollowUpsRef.current >= MAX_RESYNC_FOLLOW_UPS
      ) {
        return;
      }
      resyncFollowUpsRef.current += 1;
      resyncFollowUpTimerRef.current = setTimeout(() => {
        resyncFollowUpTimerRef.current = null;
        requestResync();
      }, RESYNC_FOLLOW_UP_DELAY_MS);
    }
  }, [applyAuthoritative, transport]);

  useEffect(() => {
    closedRef.current = false;
    generationRef.current += 1;
    const generation = generationRef.current;
    const frameState: FrameState = { epoch: null, frameArrived: false, seq: 0 };
    frameStateRef.current = frameState;
    mergerRef.current = createSessionChatMerger();
    assemblerRef.current = createIncrementalSessionChatAssembler();
    appliedRef.current = [];
    beforeOffsetRef.current = 0;
    resyncInFlightRef.current = false;
    resyncSeenInFlightRef.current = null;
    resyncFollowUpsRef.current = 0;
    resyncFailuresRef.current = 0;
    lastFrameAtRef.current = Date.now();
    lastWatchdogResyncAtRef.current = 0;
    workingStartedAtRef.current = null;
    setLoadingEarlier(false);
    /*
    The view-state wipe below is about SESSION IDENTITY, not connection
    lifecycle: a different session's transcript, detection, queue, or draft
    must never leak into this one. A same-transport rerun is a reconnect of
    the SAME conversation (the stall watchdog or chat.retry recycling the
    socket), and wiping there destroyed a perfectly good view: status fell
    back to 'loading', whose early return unmounts the whole pane — composer
    included — mid-typing. On reconnect the view keeps showing what it has;
    the sequencing reset above already guarantees no stale frame can fold in
    (nothing applies until a fresh authoritative snapshot re-seeds the epoch,
    and snapshots replace the content wholesale).
    */
    const sessionChanged = seededTransportRef.current !== transport;
    seededTransportRef.current = transport;
    diagnosticLogRef.current?.('sessionChat.subscribeStarted', {
      generation,
      sessionChanged,
    });
    if (sessionChanged) {
      limitRef.current = initialLimit;
      setServerWorking(false);
      setTranscript([]);
      setServerStatus('loading');
      setLifecycle(null);
      setPrompt(null);
      setAgentSessionId(null);
      setError(null);
      setHasMore(false);
      setPending([]);
      setMarkers([]);
      setInterrupted(false);
      setTerminalNotice(null);
      setTerminalActivity(null);
      setTerminalStatusMessages([]);
      setAppCommands([]);
      // A different session's detection must never leak into this one.
      setSelectedOptions(null);
      // Its "we have looked" latch is per-session too: a new session has not
      // been probed yet, whatever the previous one had proven.
      setScreenProbed(false);
      // Nor its queue or its draft: both are per-session, and re-probing the
      // capability from scratch is what keeps a mixed old/new daemon honest.
      setQueuePrompts(null);
      setSyncedDraft(null);
      // CDXC:DraftSessions 2026-08-28: another session's draft agent list must
      // never tick a row (or offer a switch) for this one.
      setAvailableAgents(null);
      setSessionAgentId(null);
      // Another session's agent id must not read as an agent SWITCH on this
      // one's first read (which would wipe the detection that read carried).
      sessionAgentIdRef.current = null;
    }

    const acceptSequencedFrame = (event: { epoch: number; seq: number }): 'apply' | 'drop' | 'resync' => {
      if (frameState.epoch !== null && event.epoch === frameState.epoch) {
        if (event.seq <= frameState.seq) {
          return 'drop';
        }
        if (event.seq === frameState.seq + 1) {
          frameState.seq = event.seq;
          return 'apply';
        }
      }
      return 'resync';
    };

    const onEvent = (event: GxserverSessionChatEvent): void => {
      if (closedRef.current) {
        return;
      }
      // Liveness is "a frame reached us", not "a frame changed something": a
      // dropped or duplicate frame still proves the stream is alive.
      lastFrameAtRef.current = Date.now();
      if (resyncInFlightRef.current) {
        // Remember how far the live stream ran while the read was in flight;
        // the read answers from a position captured before it.
        const seen = resyncSeenInFlightRef.current;
        const position = { epoch: event.epoch, seq: event.seq };
        if (seen === null || isAheadOf(position, seen)) {
          resyncSeenInFlightRef.current = position;
        }
      }
      if (event.type === 'sessionChatSnapshot' || event.type === 'sessionChatReplaced') {
        frameState.epoch = event.epoch;
        frameState.seq = event.seq;
        frameState.frameArrived = true;
        applyAuthoritative(event, event.type);
        return;
      }
      const verdict = acceptSequencedFrame(event);
      if (verdict === 'drop') {
        return;
      }
      if (verdict === 'resync') {
        requestResync();
        return;
      }
      if (event.type === 'sessionChatAppended') {
        // Retract first: the rows that replace an abandoned prompt can ride
        // the very same frame.
        const retracted = removeSessionChatMergerIds(mergerRef.current, event.supersededMessageIds ?? []);
        if (retracted) {
          setTranscript(mergerRef.current.list);
        }
        if (event.messages.length > 0) {
          applySessionChatMergerAppend(mergerRef.current, event.messages);
          // Keep the read window at least as large as what is on screen so a
          // later resync/pagination read cannot answer with less than the
          // live list already holds.
          limitRef.current = Math.min(
            SESSION_CHAT_MAX_LIMIT,
            Math.max(limitRef.current, mergerRef.current.list.length)
          );
          setTranscript(mergerRef.current.list);
        }
        if (event.lifecycle) {
          setLifecycle(event.lifecycle);
        }
        return;
      }
      // sessionChatState — also how hook activity transitions (working ↔ idle)
      // reach every host.
      diagnosticLogRef.current?.('sessionChat.stateFrame', {
        epoch: event.epoch,
        hasAgentSessionId: event.agentSessionId !== undefined,
        seq: event.seq,
        status: event.status,
        working: event.working === true,
      });
      setServerStatus(event.status);
      setServerWorking(event.working === true || event.status === 'working');
      if (event.lifecycle) {
        setLifecycle(event.lifecycle);
      }
      setPrompt(event.prompt ?? null);
      if (event.selectedOptions) {
        setSelectedOptions(event.selectedOptions);
      }
      setTerminalNotice(event.terminalNotice ?? null);
      applyTerminalActivity(event.terminalActivity);
      setAgentFleet(event.agentFleet ?? null);
      if (event.appCommands) {
        setAppCommands(event.appCommands);
      }
      if (event.screenProbed) {
        setScreenProbed(true);
      }
      applyQueueCarriage(event);
      if (event.agentSessionId !== undefined) {
        setAgentSessionId(event.agentSessionId);
      }
    };

    // The window follows what is on screen: limitRef grows with the live list,
    // so a reconnect's fresh snapshot never comes back smaller than the
    // conversation already shown.
    const unsubscribe = transport.subscribe({
      currentLimit: () => limitRef.current,
      onEvent,
    });

    // Seed read: independent of the subscription; permanently outranked by
    // the first snapshot/replacement frame.
    const startedAt = Date.now();
    let attempt = 0;
    const scheduleRetry = (run: () => void): void => {
      retryTimerRef.current = setTimeout(run, notFoundRetryDelayMs(attempt));
      attempt += 1;
    };
    const seedRead = (): void => {
      void withReadTimeout(transport.read({ limit: limitRef.current }))
        .then((result: GxserverReadSessionChatResult) => {
          if (closedRef.current || generationRef.current !== generation || frameState.frameArrived) {
            return;
          }
          lastFrameAtRef.current = Date.now();
          frameState.epoch = result.epoch;
          frameState.seq = result.seq;
          applyAuthoritative(result, 'seedRead');
          applyDraftAgentCarriage(result);
          if (result.status === 'starting' && Date.now() - startedAt < NOTFOUND_RETRY_WINDOW_MS) {
            scheduleRetry(seedRead);
          }
        })
        .catch(() => {
          if (closedRef.current || generationRef.current !== generation || frameState.frameArrived) {
            return;
          }
          const retryScheduled = Date.now() - startedAt < NOTFOUND_RETRY_WINDOW_MS;
          diagnosticLogRef.current?.('sessionChat.seedReadRejected', { retryScheduled });
          if (retryScheduled) {
            scheduleRetry(seedRead);
            return;
          }
          setError('Conversation could not be loaded.');
          setServerStatus('error');
        });
    };
    seedRead();

    // Stall watchdog. Nothing below the hook reports a follower or socket that
    // stopped delivering: without a frame there is no gap, and without a gap
    // the fold rules never ask for a resync. Re-reading is the only recovery.
    const stallTimer = setInterval(() => {
      if (closedRef.current) {
        return;
      }
      const now = Date.now();
      // Initial window: the working gate below cannot protect it, because a
      // session that never resolved its transcript may never report work. A
      // silent socket here leaves the view in its blank loading hold forever,
      // and only a new subscription can re-request the snapshot that was lost.
      // Rebuilding restamps lastFrameAtRef, so the next tick starts a fresh
      // window rather than firing again immediately.
      if (
        !frameState.frameArrived &&
        loadingHoldRef.current &&
        autoReconnectsRef.current < MAX_AUTOMATIC_RECONNECTS &&
        now - lastFrameAtRef.current > INITIAL_STALL_THRESHOLD_MS
      ) {
        autoReconnectsRef.current += 1;
        diagnosticLogRef.current?.('sessionChat.initialStallRecycle', {
          attempt: autoReconnectsRef.current,
        });
        reconnect();
        return;
      }
      if (!workingSignalRef.current) {
        return;
      }
      if (
        now - lastFrameAtRef.current <= STALL_THRESHOLD_MS ||
        now - lastWatchdogResyncAtRef.current <= STALL_THRESHOLD_MS
      ) {
        return;
      }
      // Stamped before the call: a read that succeeds but answers with the same
      // stale tail leaves lastFrameAtRef fresh, but one that is dropped by the
      // in-flight guard must not let the watchdog fire again on the next tick.
      lastWatchdogResyncAtRef.current = now;
      requestResync();
    }, STALL_CHECK_INTERVAL_MS);

    return () => {
      closedRef.current = true;
      clearInterval(stallTimer);
      if (retryTimerRef.current !== null) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      if (resyncFollowUpTimerRef.current !== null) {
        clearTimeout(resyncFollowUpTimerRef.current);
        resyncFollowUpTimerRef.current = null;
      }
      if (resyncRetryTimerRef.current !== null) {
        clearTimeout(resyncRetryTimerRef.current);
        resyncRetryTimerRef.current = null;
      }
      unsubscribe();
    };
  }, [
    applyAuthoritative,
    applyDraftAgentCarriage,
    applyQueueCarriage,
    applyTerminalActivity,
    initialLimit,
    reconnect,
    reconnectNonce,
    requestResync,
    transport,
  ]);

  // --- Assembly (suffix-extension fast path, §6.4) ---------------------------
  const assembled = useMemo(() => {
    const assembler = assemblerRef.current;
    const applied = appliedRef.current;
    // The transport list is in transcript-file order; record it so
    // same-millisecond rows keep that order through the sort.
    stampSessionChatArrivalOrder(transcript);
    const isSuffixExtension =
      transcript.length >= applied.length && sessionChatSharesPrefix(transcript, applied, applied.length);
    if (isSuffixExtension && transcript.length > applied.length) {
      applySessionChatAppends(assembler, transcript.slice(applied.length));
    } else if (!isSuffixExtension) {
      resetIncrementalSessionChatAssembler(assembler, transcript);
    }
    appliedRef.current = transcript;
    return assembler.messages;
  }, [transcript]);

  const catalogSet = useMemo(() => new Set(commandCatalog), [commandCatalog]);

  const surfaced = useMemo(() => surfaceSkillInvocationUserTurns(assembled, catalogSet), [assembled, catalogSet]);

  const boundaried = useMemo(() => applySessionChatCommandMarkerBoundaries(surfaced, markers), [markers, surfaced]);

  /*
   * Counted off the RAW authoritative list, the same one `send` snapshots
   * from: assembly folds turns together and a `/clear` boundary hides them
   * outright, so counting the two ends of the comparison on different lists
   * could leave a `/compact` marker either retired on sight or stranded.
   */
  const compactionRecords = useMemo(() => countSessionChatCompactionRecords(transcript), [transcript]);

  // --- Pending prune against the authoritative list --------------------------
  useEffect(() => {
    setPending((current) => {
      if (current.length === 0) {
        return current;
      }
      const next = pruneSessionChatPendingSends(current, boundaried);
      return next === current ? current : next;
    });
  }, [boundaried]);

  // --- Working / status derivation -------------------------------------------
  // Three independent starts: the `working` flag on read results/snapshots,
  // the server's activity-transition state frames, and the host's own hook
  // signal. Settling is owned by an idle transition, a terminal turn
  // lifecycle, or a local interrupt.
  const optimisticWorking = pending.length > 0;
  const workingSignal = optimisticWorking || serverWorking || serverStatus === 'working' || externalWorking === true;
  workingSignalRef.current = workingSignal;
  if (workingSignal) {
    workingStartedAtRef.current ??= Date.now();
  } else {
    workingStartedAtRef.current = null;
  }
  const workingOverride = deriveSessionChatWorkingOverride({
    lifecycle,
    transcriptMessages: transcript,
    working: workingSignal,
    // Without a start boundary the PREVIOUS turn's completed lifecycle would
    // settle the new turn instantly — the dead-indicator bug.
    workingStartedAt: workingStartedAtRef.current,
  });
  // A locally accepted send owns the working presentation immediately. It
  // remains pending until the authoritative transcript advances past that
  // user turn, bridging the gap before host/server activity arrives.
  const working = (optimisticWorking || workingOverride === 'working') && !interrupted;
  workingRef.current = working;

  // Clear the Stop suppression once the live signal settles (§10.5).
  useEffect(() => {
    if (!workingSignal && interrupted) {
      setInterrupted(false);
    }
  }, [interrupted, workingSignal]);

  // Live work can arrive before the seed read. Keep unresolved transcript
  // states authoritative so they cannot be mistaken for confirmed emptiness.
  const status: SessionChatStatus = error
    ? 'error'
    : serverStatus === 'loading' || serverStatus === 'starting'
      ? serverStatus
      : working
        ? 'working'
        : serverStatus === 'working'
          ? 'ready'
          : serverStatus;

  // --- Composition (§11.1 order: markers → streaming → pending) --------------
  const messages = useMemo(() => {
    const pendingMessages = sessionChatPendingSendsAsMessages(visibleSessionChatPendingSends(pending, boundaried));
    const authoritativeText = new Set(
      boundaried
        .filter((message) => message.source === 'transcript')
        .map(normalizedSessionChatText)
        .filter(Boolean)
    );
    const markerMessages = sessionChatCommandMarkersAsMessages(markers, compactionRecords).filter(
      (message) => message.role !== 'user' || !authoritativeText.has(normalizedSessionChatText(message))
    );
    const visibleTerminalStatuses = terminalStatusMessages.filter(
      (message) => !authoritativeText.has(normalizedSessionChatText(message))
    );
    const tail: SessionChatMessage[] = [
      ...visibleTerminalStatuses,
      ...sessionChatAppCommandsAsMessages(appCommands, boundaried),
      ...markerMessages,
    ];
    const streamingText = deriveSessionChatStreamingText({
      messages: [...boundaried, ...pendingMessages],
      previewText,
      working,
    });
    if (streamingText) {
      tail.push(sessionChatStreamingMessage(streamingText));
    }
    tail.push(...pendingMessages);
    return [...boundaried, ...tail];
  }, [appCommands, boundaried, compactionRecords, markers, pending, previewText, terminalStatusMessages, working]);

  const view = selectSessionChatViewState({
    error,
    hasKnownAgentSession: agentSessionId !== null,
    /*
    CDXC:DraftAgentSwitch 2026-08-28:
    `availableAgents` is the daemon's own draft marker (it is sent for a draft
    and for nothing else), so its presence is what keeps an agent switch from
    unmounting this pane. See selectSessionChatViewState.
    */
    isDraft: availableAgents !== null,
    messageCount: messages.length,
    status,
  });
  // Both of these render as "nothing to show yet": 'loading' is the blank hold
  // (including the working-with-no-messages case), 'starting' the welcome.
  loadingHoldRef.current = view.kind === 'loading' || view.kind === 'starting';

  // --- Actions ----------------------------------------------------------------
  const loadEarlier = useCallback((): void => {
    if (loadingEarlier || !hasMore || closedRef.current) {
      return;
    }
    setLoadingEarlier(true);
    const requestEpoch = frameStateRef.current.epoch;
    const requestedBeforeOffset = beforeOffsetRef.current;
    loadEarlierEpochRef.current = requestEpoch;
    void withReadTimeout(transport.read({ beforeOffset: requestedBeforeOffset, limit: SESSION_CHAT_PAGE }))
      .then((result) => {
        if (closedRef.current || loadEarlierEpochRef.current !== requestEpoch) {
          return;
        }
        if (frameStateRef.current.epoch !== requestEpoch) {
          // A replacement rebuilt the tail while this page was in flight.
          return;
        }
        const merger = mergerRef.current;
        const older = result.messages.filter((message) => {
          const at = merger.indexById.get(message.id);
          if (at === undefined) {
            return true;
          }
          // Same id but a different row (shared response id) is real history,
          // not a duplicate — the merger re-keys it on the way in.
          const existing = merger.list[at];
          return existing !== undefined && sessionChatIdCollides(existing, message);
        });
        replaceSessionChatMergerList(merger, [...older, ...merger.list]);
        // Grow the read window so a later resync answers with at least the
        // history that is already on screen.
        limitRef.current = Math.min(
          SESSION_CHAT_MAX_LIMIT,
          Math.max(limitRef.current + SESSION_CHAT_PAGE, merger.list.length)
        );
        setTranscript(merger.list);
        setHasMore(sessionChatPageHasMore(result, requestedBeforeOffset));
        beforeOffsetRef.current = result.beforeOffset;
        // Older pages never rewind the live lifecycle or status.
      })
      .catch(() => {
        // A failed page must not become the view's error state: the live tail
        // is still valid. `hasMore` stays set, so the control comes back and
        // the user can ask for the same page again.
      })
      .finally(() => {
        if (!closedRef.current && loadEarlierEpochRef.current === requestEpoch) {
          setLoadingEarlier(false);
          loadEarlierEpochRef.current = null;
        }
      });
  }, [hasMore, loadingEarlier, transport]);

  const send = useCallback(
    async (text: string, imagePaths?: string[]): Promise<void> => {
      const classification = classifySessionChatSend(text, commandCatalog);
      let pendingId: string | null = null;
      let commandMarkerSentAt: number | null = null;
      if (classification === 'chat' && (text.trim().length > 0 || (imagePaths?.length ?? 0) > 0)) {
        const last = mergerRef.current.list.at(-1);
        const id = nextSessionChatPendingSendId();
        pendingId = id;
        const baseEntry: SessionChatPendingSend = {
          afterMessageId: last?.id ?? null,
          afterMessageTimestamp: last?.timestamp ?? null,
          id,
          imagePaths,
          sentAt: Date.now(),
          text,
        };
        setPending((current) => {
          const entry = assignSessionChatPendingOccurrence(current, baseEntry);
          const next = [...current, entry];
          return next.length > SESSION_CHAT_PENDING_SEND_LIMIT
            ? next.slice(next.length - SESSION_CHAT_PENDING_SEND_LIMIT)
            : next;
        });
      } else if (classification === 'command') {
        // Snapshot the compactions already on record, so a `/compact` marker
        // retires against ITS OWN compaction rather than an earlier one.
        const compactionRecordsBefore = countSessionChatCompactionRecords(mergerRef.current.list);
        commandMarkerSentAt = Date.now();
        setMarkers((current) =>
          appendSessionChatCommandMarker(
            current,
            text.trim(),
            commandMarkerSentAt ?? Date.now(),
            undefined,
            compactionRecordsBefore
          )
        );
      }
      try {
        await transport.send(text, imagePaths);
      } catch (sendError) {
        if (pendingId !== null) {
          const dropId = pendingId;
          setPending((current) => current.filter((entry) => entry.id !== dropId));
        }
        if (commandMarkerSentAt !== null) {
          const failedCommand = text.trim();
          const failedSentAt = commandMarkerSentAt;
          setMarkers((current) =>
            current.filter((marker) => marker.sentAt !== failedSentAt || marker.command !== failedCommand)
          );
        }
        throw sendError;
      }
    },
    [commandCatalog, transport]
  );

  /**
   * Keystroke dispatch: a non-empty marker is recorded only after the write
   * is accepted. Multi-key setting adjustments pass an empty marker so their
   * implementation keystrokes do not become chat rows.
   */
  const transportSendKey = transport.sendKey;
  const sendKey = useCallback(
    async (key: SessionChatSendKey, marker: string): Promise<void> => {
      if (!transportSendKey) {
        return;
      }
      await transportSendKey.call(transport, key);
      if (marker.trim() !== '') {
        setMarkers((current) => appendSessionChatCommandMarker(current, key, Date.now(), marker));
      }
    },
    [transport, transportSendKey]
  );

  const answerPrompt = useCallback(
    async (params: Omit<GxserverAnswerSessionChatPromptParams, 'projectId' | 'sessionId'>): Promise<void> => {
      await transport.answerPrompt(params);
    },
    [transport]
  );

  // --- Ghostex prompt queue + synced draft ------------------------------------
  const queueCapabilities = useMemo(
    () =>
      sessionChatQueueCapabilities({
        daemonSupportsQueue: queuePrompts !== null,
        transport,
      }),
    [queuePrompts, transport]
  );
  const clientId = useMemo(() => sessionChatDraftClientId(), []);
  // Every mutation answers with the whole authoritative queue, so an optimistic
  // step that lost a race self-corrects on the next line instead of needing a
  // rollback path.
  const queueMutation = useCallback(
    async (run: (() => Promise<{ queue: SessionChatQueuedPrompt[] }>) | undefined): Promise<void> => {
      if (!run) {
        return;
      }
      const result = await run();
      if (!closedRef.current) {
        setQueuePrompts(result.queue);
      }
    },
    []
  );
  const queuePrompt = useCallback(
    async (text: string): Promise<void> => {
      const call = transport.queuePrompt?.bind(transport);
      if (!queueCapabilities.canQueue || !call) {
        return;
      }
      await queueMutation(() => call({ text }));
    },
    [queueCapabilities.canQueue, queueMutation, transport]
  );
  const retryPrompt = useCallback(
    async (promptId: string): Promise<void> => {
      const call = transport.updateQueuedPrompt?.bind(transport);
      if (!queueCapabilities.canRetry || !call) {
        return;
      }
      await queueMutation(() => call({ promptId, retry: true }));
    },
    [queueCapabilities.canRetry, queueMutation, transport]
  );
  const removePrompt = useCallback(
    async (promptId: string): Promise<SessionChatQueuedPrompt | null> => {
      const call = transport.removeQueuedPrompt?.bind(transport);
      if (!queueCapabilities.canRemove || !call) {
        return null;
      }
      const result = await call({ promptId });
      if (!closedRef.current) {
        setQueuePrompts(result.queue);
      }
      // The removed row rides back on the answer so Edit can pull its text into
      // the composer without having cached it across the round trip.
      return result.prompt;
    },
    [queueCapabilities.canRemove, transport]
  );
  const reorder = useCallback(
    async (promptIds: string[]): Promise<void> => {
      const call = transport.reorderQueue?.bind(transport);
      if (!queueCapabilities.canReorder || !call) {
        return;
      }
      // Optimistic: the strip must settle into the dropped order immediately.
      setQueuePrompts((current) => {
        if (current === null) {
          return current;
        }
        let next = [...current];
        promptIds.forEach((id, target) => {
          const from = next.findIndex((prompt) => prompt.id === id);
          if (from >= 0) {
            next = moveSessionChatQueueRow(next, from, target);
          }
        });
        return next;
      });
      await queueMutation(() => call({ promptIds }));
    },
    [queueCapabilities.canReorder, queueMutation, transport]
  );
  const sendNow = useCallback(
    async (promptId: string): Promise<void> => {
      const call = transport.sendQueuedPrompt?.bind(transport);
      if (!queueCapabilities.canSendNow || !call) {
        return;
      }
      await queueMutation(() => call({ promptId }));
    },
    [queueCapabilities.canSendNow, queueMutation, transport]
  );
  const pushDraft = useCallback(
    async (content: string): Promise<void> => {
      const call = transport.setDraft?.bind(transport);
      if (!call) {
        return;
      }
      await call({ clientId, content });
    },
    [clientId, transport]
  );
  const queue = useMemo<SessionChatQueueController>(
    () => ({
      capabilities: queueCapabilities,
      prompts: queuePrompts ?? [],
      queuePrompt,
      removePrompt,
      reorder,
      retryPrompt,
      sendNow,
    }),
    [queueCapabilities, queuePrompt, queuePrompts, removePrompt, reorder, retryPrompt, sendNow]
  );
  const draft = useMemo<SessionChatDraftController>(
    () => ({
      canSync: queueCapabilities.canSyncDraft,
      clientId,
      push: pushDraft,
      synced: syncedDraft,
    }),
    [clientId, pushDraft, queueCapabilities.canSyncDraft, syncedDraft]
  );

  const interrupt = useCallback(async (): Promise<void> => {
    if (workingRef.current) {
      // Stop: suppress the spinner and drop optimistic echoes — the delayed
      // server-side Enter may never fire, so the echo would be a ghost bubble.
      setInterrupted(true);
      setPending([]);
    }
    await transport.interrupt();
  }, [transport]);

  return {
    agent,
    agentSessionId,
    answerPrompt,
    availableAgents,
    draft,
    error,
    hasMore,
    interrupt,
    lifecycle,
    loadEarlier,
    loadingEarlier,
    messages,
    prompt,
    queue,
    refresh: requestResync,
    retry: reconnect,
    sessionAgentId,
    selectedOptions,
    send,
    status,
    terminalNotice,
    terminalActivity,
    agentFleet,
    screenProbed,
    view,
    working,
    ...(transportSendKey ? { sendKey } : {}),
  };
}
