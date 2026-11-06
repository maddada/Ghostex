import type { SessionChatSource } from './session-chat-agents';

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

export interface SessionChatAsyncQuestion {
  title: string;
  options?: string[];
}

export interface SessionChatMessage {
  /** Older completed work is fetched only when its disclosure is opened. */
  deferredWork?: SessionChatDeferredWork;
  /** Stable across re-reads: record uuid/payload id, else `${filePath}:${byteOffset16}`. */
  id: string;
  role: SessionChatRole;
  blocks: SessionChatBlock[];
  /** Codex questions answered as ordinary messages while work continues. */
  asyncQuestions?: SessionChatAsyncQuestion[];
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
  /** The agent's own record that its API call failed, with the code it recorded (`server_error`, `rate_limit`, ...). */
  apiError?: string;
  /** On a row OMP wrote as a `custom_message`: its type and, for the types the chat draws as cards or rows, the details OMP recorded. */
  customMessage?: {
    customType: string;
    details?: Record<string, unknown>;
  };
  /** Client presentation of an accepted send waiting for the terminal, separate from the agent CLI queue. */
  startupDelivery?: {
    promptId: string;
    state: 'queued' | 'sending' | 'failed';
    errorMessage?: string;
  };
}

export interface SessionChatDeferredWork {
  completedAt: number | null;
  beforeOffset: number;
  startId: string;
  endId: string;
  messageCount: number;
  filePaths: string[];
  /** Picture files the held-back rows opened from disk (gx-chat-core `read_images.rs`); absent when none. */
  imagePaths?: string[];
}

export interface SessionChatHistoryReadParams {
  beforeOffset: number;
  limit?: number;
  /** Raw pages used only by an explicitly expanded work section. */
  detail?: boolean;
  /** The leading fragment belongs to a response that is still running. */
  preserveNewest?: boolean;
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
  /** True when Claude draws this question beside option previews, where typed text is only a note on a picked option. */
  previewLayout?: boolean;
  options: SessionChatQuestionOption[];
}

export type SessionChatInteractivePrompt =
  | {
      kind: 'question';
      questions: SessionChatQuestion[];
      /**
       * The hook's tool_use_id of the asking call, when the hook payload
       * carried one. gxserver retires the card on that call's own post-tool
       * event only, so a subagent's tool traffic in the same session cannot
       * retire it. Informational for clients.
       */
      toolUseId?: string;
    }
  | { kind: 'approval'; tool: string; summary?: string; toolUseId?: string };

/** One answer per question, by 0-based option indices plus optional free text. */
export interface SessionChatQuestionSelection {
  indices: number[];
  other?: string;
}

// ---------------------------------------------------------------------------
// Detected session options (model / reasoning effort)
// ---------------------------------------------------------------------------

/*
CDXC:AgentScreenDetection 2026-08-01:
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
   * stored by the Ghostex-installed script (CDXC:AgentScreenDetection 2026-09-03).
   * `launch` is the flags the agent command was started with, sent only until
   * the agent reports through its transcript or statusline.
   */
  source?: 'terminal' | 'transcript' | 'statusline' | 'launch';
}
