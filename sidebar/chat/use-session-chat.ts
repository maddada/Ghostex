// useSessionChat — host-agnostic session-chat state machine.
// Consumes an injected SessionChatTransport; implements the seed read, frame
// folding with epoch/seq rules (drop dup seq, resnapshot on gap/epoch
// change), the 60s not-found/starting retry patience (upstream chat spec
// §5.13), load-earlier pagination, optimistic sends, and status derivation.
//
// Anti-drop law: the live list only ever grows. Reads window the history they
// seed; appends are never trimmed, because a trim removes the OLDEST rows and
// the pagination cursor cannot reach them again.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  GxserverAnswerSessionChatPromptParams,
  GxserverReadSessionChatResult,
  GxserverSessionChatEvent,
  SessionChatDetectedOptions,
  SessionChatInteractivePrompt,
  SessionChatMessage,
  SessionChatSendKey,
  SessionChatStatus,
  SessionChatTurnLifecycle,
} from "../../shared/session-chat";
import {
  applySessionChatAppends,
  createIncrementalSessionChatAssembler,
  resetIncrementalSessionChatAssembler,
  sessionChatIdCollides,
  sessionChatSharesPrefix,
  stampSessionChatArrivalOrder,
} from "./session-chat-assembler";
import {
  applySessionChatMergerAppend,
  createSessionChatMerger,
  replaceSessionChatMergerList,
  type SessionChatMerger,
} from "./session-chat-merge";
import {
  appendSessionChatCommandMarker,
  applySessionChatCommandMarkerBoundaries,
  assignSessionChatPendingOccurrence,
  nextSessionChatPendingSendId,
  pruneSessionChatPendingSends,
  SESSION_CHAT_PENDING_SEND_LIMIT,
  sessionChatCommandMarkersAsMessages,
  sessionChatPendingSendsAsMessages,
  visibleSessionChatPendingSends,
  type SessionChatCommandMarker,
  type SessionChatPendingSend,
} from "./session-chat-pending";
import {
  SESSION_CHAT_INITIAL_LIMIT,
  SESSION_CHAT_MAX_LIMIT,
  SESSION_CHAT_PAGE,
} from "./session-chat-pagination";
import {
  classifySessionChatSend,
  SESSION_CHAT_DEFAULT_COMMAND_CATALOG,
} from "./session-chat-send-classification";
import {
  deriveSessionChatStreamingText,
  sessionChatStreamingMessage,
} from "./session-chat-streaming";
import { surfaceSkillInvocationUserTurns } from "./session-chat-command-envelope";
import type { SessionChatTransport } from "./session-chat-transport";
import {
  selectSessionChatViewState,
  type SessionChatViewState,
} from "./session-chat-view-state";
import { deriveSessionChatWorkingOverride } from "./session-chat-working-status";

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

interface SessionChatStreamPosition {
  epoch: number;
  seq: number;
}

function isAheadOf(
  candidate: SessionChatStreamPosition,
  reference: SessionChatStreamPosition,
): boolean {
  return (
    candidate.epoch > reference.epoch ||
    (candidate.epoch === reference.epoch && candidate.seq > reference.seq)
  );
}

function notFoundRetryDelayMs(attempt: number): number {
  return NOTFOUND_RETRY_DELAYS_MS[attempt] ?? NOTFOUND_RETRY_FIXED_DELAY_MS;
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
  agent: string | null;
  agentSessionId: string | null;
  error: string | null;
  hasMore: boolean;
  loadingEarlier: boolean;
  loadEarlier: () => void;
  send: (text: string, imagePaths?: string[]) => Promise<void>;
  /**
   * Raw keystroke injection (Claude's Shift+Tab mode cycle). Undefined when
   * the host transport cannot deliver keys, so callers hide the control
   * instead of pretending it works.
   */
  sendKey?: (key: SessionChatSendKey, marker: string) => Promise<void>;
  answerPrompt: (
    params: Omit<GxserverAnswerSessionChatPromptParams, "projectId" | "sessionId">,
  ) => Promise<void>;
  interrupt: () => Promise<void>;
}

export function useSessionChat(options: UseSessionChatOptions): UseSessionChatResult {
  const {
    commandCatalog = SESSION_CHAT_DEFAULT_COMMAND_CATALOG,
    initialLimit = SESSION_CHAT_INITIAL_LIMIT,
    previewText = null,
    transport,
    working: externalWorking = false,
  } = options;

  const [transcript, setTranscript] = useState<readonly SessionChatMessage[]>([]);
  const [serverStatus, setServerStatus] = useState<SessionChatStatus>("loading");
  const [lifecycle, setLifecycle] = useState<SessionChatTurnLifecycle | null>(null);
  const [prompt, setPrompt] = useState<SessionChatInteractivePrompt | null>(null);
  const [agent, setAgent] = useState<string | null>(null);
  const [agentSessionId, setAgentSessionId] = useState<string | null>(null);
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
  const [selectedOptions, setSelectedOptions] = useState<SessionChatDetectedOptions | null>(
    null,
  );

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
  const loadEarlierEpochRef = useRef<number | null>(null);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const workingRef = useRef(false);
  const workingStartedAtRef = useRef<number | null>(null);

  const applyAuthoritative = useCallback(
    (result: {
      messages: SessionChatMessage[];
      lifecycle?: SessionChatTurnLifecycle;
      hasMore: boolean;
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
    }): void => {
      replaceSessionChatMergerList(mergerRef.current, result.messages);
      setTranscript(mergerRef.current.list);
      setLifecycle(result.lifecycle ?? null);
      setHasMore(result.hasMore);
      beforeOffsetRef.current = result.beforeOffset;
      setServerStatus(result.status);
      setServerWorking(result.working === true || result.status === "working");
      setPrompt(result.prompt ?? null);
      if (result.agent !== undefined) {
        setAgent(result.agent);
      }
      setAgentSessionId(result.agentSessionId ?? null);
      if (result.selectedOptions) {
        setSelectedOptions(result.selectedOptions);
      }
      setError(result.status === "error" ? (result.error ?? "Conversation could not be loaded.") : null);
      // A fresh authoritative generation cancels an in-flight older page.
      loadEarlierEpochRef.current = null;
      setLoadingEarlier(false);
    },
    [],
  );

  const requestResync = useCallback((): void => {
    if (resyncInFlightRef.current || closedRef.current) {
      // Frames arriving from here on are recorded by onEvent and covered by
      // the follow-up read this flight schedules.
      return;
    }
    resyncInFlightRef.current = true;
    resyncSeenInFlightRef.current = null;
    const generation = generationRef.current;
    void transport
      .read({ limit: limitRef.current })
      .then((result) => {
        if (closedRef.current || generationRef.current !== generation) {
          return;
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
        applyAuthoritative(result);
        if (outrun) {
          scheduleResyncFollowUp();
        } else {
          resyncFollowUpsRef.current = 0;
        }
      })
      .catch(() => {
        if (!closedRef.current && generationRef.current === generation) {
          setError("Conversation could not be loaded.");
          setServerStatus("error");
        }
      })
      .finally(() => {
        if (generationRef.current === generation) {
          resyncInFlightRef.current = false;
        }
      });

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
    limitRef.current = initialLimit;
    beforeOffsetRef.current = 0;
    resyncInFlightRef.current = false;
    resyncSeenInFlightRef.current = null;
    resyncFollowUpsRef.current = 0;
    workingStartedAtRef.current = null;
    setServerWorking(false);
    setTranscript([]);
    setServerStatus("loading");
    setLifecycle(null);
    setPrompt(null);
    setAgentSessionId(null);
    setError(null);
    setHasMore(false);
    setLoadingEarlier(false);
    setPending([]);
    setMarkers([]);
    setInterrupted(false);
    // A different session's detection must never leak into this one.
    setSelectedOptions(null);

    const acceptSequencedFrame = (event: {
      epoch: number;
      seq: number;
    }): "apply" | "drop" | "resync" => {
      if (frameState.epoch !== null && event.epoch === frameState.epoch) {
        if (event.seq <= frameState.seq) {
          return "drop";
        }
        if (event.seq === frameState.seq + 1) {
          frameState.seq = event.seq;
          return "apply";
        }
      }
      return "resync";
    };

    const onEvent = (event: GxserverSessionChatEvent): void => {
      if (closedRef.current) {
        return;
      }
      if (resyncInFlightRef.current) {
        // Remember how far the live stream ran while the read was in flight;
        // the read answers from a position captured before it.
        const seen = resyncSeenInFlightRef.current;
        const position = { epoch: event.epoch, seq: event.seq };
        if (seen === null || isAheadOf(position, seen)) {
          resyncSeenInFlightRef.current = position;
        }
      }
      if (event.type === "sessionChatSnapshot" || event.type === "sessionChatReplaced") {
        frameState.epoch = event.epoch;
        frameState.seq = event.seq;
        frameState.frameArrived = true;
        applyAuthoritative(event);
        return;
      }
      const verdict = acceptSequencedFrame(event);
      if (verdict === "drop") {
        return;
      }
      if (verdict === "resync") {
        requestResync();
        return;
      }
      if (event.type === "sessionChatAppended") {
        if (event.messages.length > 0) {
          applySessionChatMergerAppend(mergerRef.current, event.messages);
          // Keep the read window at least as large as what is on screen so a
          // later resync/pagination read cannot answer with less than the
          // live list already holds.
          limitRef.current = Math.min(
            SESSION_CHAT_MAX_LIMIT,
            Math.max(limitRef.current, mergerRef.current.list.length),
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
      setServerStatus(event.status);
      setServerWorking(event.working === true || event.status === "working");
      if (event.lifecycle) {
        setLifecycle(event.lifecycle);
      }
      setPrompt(event.prompt ?? null);
      if (event.selectedOptions) {
        setSelectedOptions(event.selectedOptions);
      }
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
      void transport
        .read({ limit: limitRef.current })
        .then((result: GxserverReadSessionChatResult) => {
          if (
            closedRef.current ||
            generationRef.current !== generation ||
            frameState.frameArrived
          ) {
            return;
          }
          frameState.epoch = result.epoch;
          frameState.seq = result.seq;
          applyAuthoritative(result);
          if (
            result.status === "starting" &&
            Date.now() - startedAt < NOTFOUND_RETRY_WINDOW_MS
          ) {
            scheduleRetry(seedRead);
          }
        })
        .catch(() => {
          if (
            closedRef.current ||
            generationRef.current !== generation ||
            frameState.frameArrived
          ) {
            return;
          }
          if (Date.now() - startedAt < NOTFOUND_RETRY_WINDOW_MS) {
            scheduleRetry(seedRead);
            return;
          }
          setError("Conversation could not be loaded.");
          setServerStatus("error");
        });
    };
    seedRead();

    return () => {
      closedRef.current = true;
      if (retryTimerRef.current !== null) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      if (resyncFollowUpTimerRef.current !== null) {
        clearTimeout(resyncFollowUpTimerRef.current);
        resyncFollowUpTimerRef.current = null;
      }
      unsubscribe();
    };
  }, [applyAuthoritative, initialLimit, requestResync, transport]);

  // --- Assembly (suffix-extension fast path, §6.4) ---------------------------
  const assembled = useMemo(() => {
    const assembler = assemblerRef.current;
    const applied = appliedRef.current;
    // The transport list is in transcript-file order; record it so
    // same-millisecond rows keep that order through the sort.
    stampSessionChatArrivalOrder(transcript);
    const isSuffixExtension =
      transcript.length >= applied.length &&
      sessionChatSharesPrefix(transcript, applied, applied.length);
    if (isSuffixExtension && transcript.length > applied.length) {
      applySessionChatAppends(assembler, transcript.slice(applied.length));
    } else if (!isSuffixExtension) {
      resetIncrementalSessionChatAssembler(assembler, transcript);
    }
    appliedRef.current = transcript;
    return assembler.messages;
  }, [transcript]);

  const catalogSet = useMemo(() => new Set(commandCatalog), [commandCatalog]);

  const surfaced = useMemo(
    () => surfaceSkillInvocationUserTurns(assembled, catalogSet),
    [assembled, catalogSet],
  );

  const boundaried = useMemo(
    () => applySessionChatCommandMarkerBoundaries(surfaced, markers),
    [markers, surfaced],
  );

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
  const workingSignal =
    serverWorking || serverStatus === "working" || externalWorking === true;
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
  const working = workingOverride === "working" && !interrupted;
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
    ? "error"
    : serverStatus === "loading" || serverStatus === "starting"
      ? serverStatus
      : working
        ? "working"
        : serverStatus === "working"
          ? "ready"
          : serverStatus;

  // --- Composition (§11.1 order: markers → streaming → pending) --------------
  const messages = useMemo(() => {
    const markerMessages = sessionChatCommandMarkersAsMessages(markers);
    const pendingMessages = sessionChatPendingSendsAsMessages(
      visibleSessionChatPendingSends(pending, boundaried),
    );
    const tail: SessionChatMessage[] = [...markerMessages];
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
  }, [boundaried, markers, pending, previewText, working]);

  const view = selectSessionChatViewState({
    error,
    hasKnownAgentSession: agentSessionId !== null,
    messageCount: messages.length,
    status,
  });

  // --- Actions ----------------------------------------------------------------
  const loadEarlier = useCallback((): void => {
    if (loadingEarlier || !hasMore || closedRef.current) {
      return;
    }
    setLoadingEarlier(true);
    const requestEpoch = frameStateRef.current.epoch;
    loadEarlierEpochRef.current = requestEpoch;
    void transport
      .read({ beforeOffset: beforeOffsetRef.current, limit: SESSION_CHAT_PAGE })
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
          Math.max(limitRef.current + SESSION_CHAT_PAGE, merger.list.length),
        );
        setTranscript(merger.list);
        setHasMore(result.hasMore);
        beforeOffsetRef.current = result.beforeOffset;
        // Older pages never rewind the live lifecycle or status.
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
      if (
        classification === "chat" &&
        (text.trim().length > 0 || (imagePaths?.length ?? 0) > 0)
      ) {
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
      } else if (classification === "command") {
        setMarkers((current) => appendSessionChatCommandMarker(current, text.trim()));
      }
      try {
        await transport.send(text, imagePaths);
      } catch (sendError) {
        if (pendingId !== null) {
          const dropId = pendingId;
          setPending((current) => current.filter((entry) => entry.id !== dropId));
        }
        throw sendError;
      }
    },
    [commandCatalog, transport],
  );

  /**
   * Keystroke dispatch: the marker is recorded only after the write is
   * accepted, so a failed injection leaves no "Sent Shift+Tab" ghost.
   */
  const transportSendKey = transport.sendKey;
  const sendKey = useCallback(
    async (key: SessionChatSendKey, marker: string): Promise<void> => {
      if (!transportSendKey) {
        return;
      }
      await transportSendKey.call(transport, key);
      setMarkers((current) =>
        appendSessionChatCommandMarker(current, key, Date.now(), marker),
      );
    },
    [transport, transportSendKey],
  );

  const answerPrompt = useCallback(
    async (
      params: Omit<GxserverAnswerSessionChatPromptParams, "projectId" | "sessionId">,
    ): Promise<void> => {
      await transport.answerPrompt(params);
    },
    [transport],
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
    error,
    hasMore,
    interrupt,
    lifecycle,
    loadEarlier,
    loadingEarlier,
    messages,
    prompt,
    selectedOptions,
    send,
    status,
    view,
    working,
    ...(transportSendKey ? { sendKey } : {}),
  };
}
