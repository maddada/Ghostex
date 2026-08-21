# 016 — Chat prompt queue + cross-client draft sync

Status: approved design, 2026-08-21. Epic: `ghostex-q1` (see `bd show ghostex-q1`).

Queue prompts from the shared chat view on gpui, web and mobile. gxserver owns
the queue, delivers one prompt each time the agent stops, and syncs both the
queue and the unsent composer draft between every client.

---

## 1. Agreed behaviour (do not redesign these)

| Decision | Outcome |
|---|---|
| Ownership | gxserver owns the queue. A server-side scheduler delivers, so it works with every client closed, the phone locked, or the desktop app quit. |
| Drain | One prompt per idle window. Delivering #1 makes the agent work again; #2 waits for the next stop. |
| Queued while idle | Idle counts as "the agent stopped", so a queue loaded up while idle drains itself, one turn at a time. |
| Queue key | `Tab` in the composer (desktop/web). Long-press the Send button (~500 ms) everywhere, including mobile. |
| Enter mid-turn | **Unchanged.** Enter still sends immediately into the agent's own queue. `Tab` / long-press are the only ways to make a Ghostex queued row. |
| Stop button | While the agent is working, the footer button becomes **Send** whenever the composer holds non-whitespace text (trim before checking). Empty composer keeps **Stop**. |
| Edit | Pulls the row's text into the composer; whatever the composer already held is queued as a new row **at the end**. |
| Row controls | Edit · Send now · Delete · drag-to-reorder. |
| Failure | Hold the row, mark it failed with the reason, stop draining until the user retries or deletes it. Never discard the text. |
| Auto Sleep | A non-empty queue declines **automatic** sleeps. An explicit user Sleep still wins. |
| Draft sync | Pushed on blur / leaving the session / backgrounding — not per keystroke. |
| Draft conflict | Never clobber. Show a one-line "Newer draft from another device — Use / Dismiss" bar above the composer. |
| Sidebar | Small filled `#f6c945` circle with the count, CSS-anchored over the agent icon, no layout shift. |
| Terminal view | Top-left `Queued: N` button; clicking it switches that session to chat view. |
| Scope | Chat-capable agents only (`SESSION_CHAT_SUPPORTED_AGENTS`). Non-chat agents are out of scope for now. |
| Rollout | On by default. No feature flag. |

Derived rules, also settled:

- **"Stopped" means** `presentation_activity(session) == "idle"` **and** the chat
  transcript lifecycle is not `working`, held for a short stability window.
  `attention` (permission/approval prompts) never releases the queue — late
  delivery is harmless, early delivery corrupts a turn.
- Queue rows are **plain text**. The composer already interpolates attachments
  into the text as `[Image #N](path)` / `[File #N](path)` before send, so the
  queue needs no attachment storage.
- **Send now** on a row delivers immediately regardless of agent state, exactly
  like pressing Enter.
- `Tab` keeps completing slash / `$skill` / `@file` while a picker is open
  (`session-chat-composer.tsx` handleSlashKeyDown / handleSkillKeyDown /
  handleFileKeyDown). It only queues when no picker is open and the draft is
  non-empty. Taking `Tab` from the Monaco composer means losing tab-indent
  there; that is accepted.
- Delivery goes through the existing `/api/sendSessionChatMessage` internals so
  it shares the per-session send mutex and cannot collide with a Delayed Send.
- **Naming collision, read this twice:** `SessionChatMessage.queued` already
  exists and means *Claude Code's own* internal queue (`queue-operation` rows).
  Never reuse that field, that flag, or the `.ghostex-chat-queued-label` class
  for this feature. This feature's rows live above the composer, not in the
  transcript.

---

## 2. Wire contract (bead `ghostex-q1.1` owns every type here)

New file `shared/session-chat-queue.ts`, re-exported from `shared/session-chat.ts`.

```ts
export type SessionChatQueuedPromptState = "queued" | "sending" | "failed";

export interface SessionChatQueuedPrompt {
  id: string;                 // server-generated, stable across edits
  text: string;
  state: SessionChatQueuedPromptState;
  /** Set only when state === "failed": why the delivery attempt failed. */
  errorMessage?: string;
  createdAt: string;          // ISO-8601 millis
  updatedAt: string;
}

export interface SessionChatDraft {
  content: string;
  updatedAt: string;          // ISO-8601 millis
  /** Opaque per-client id of the writer, so a client ignores its own echo. */
  originClientId: string;
}
```

Carried on `GxserverReadSessionChatResult` and on snapshot / replaced / state
frames (never on `appended`):

```ts
  /**
   * The session's Ghostex-owned prompt queue, head first. PRESENT (even as an
   * empty array) is the daemon capability probe: a daemon that predates this
   * feature omits it, and a client that sees it omitted hides every queue
   * control instead of calling endpoints that will 404.
   * When present, it is authoritative and replaces the client's list.
   */
  queue?: SessionChatQueuedPrompt[];
  /**
   * Latest synced composer draft. Unlike `prompt`/`terminalNotice`, an OMITTED
   * draft means "unchanged / none on the server", NOT cleared — clearing a
   * local draft because an old daemon never sends the field would destroy
   * text the user typed.
   */
  draft?: SessionChatDraft;
```

`readSessionChat`'s long-poll `fingerprint` MUST fold in a queue revision and
the draft `updatedAt`, or the mobile host (which synthesizes frames from
long-polled reads) never learns about queue or draft changes.

### Endpoints

Registered in `shared/gxserver-protocol.ts`, `gxserver-rs/src/protocol.rs`,
`gxserver-rs/src/server.rs`, and as `ghostex` CLI verbs in
`gxserver-rs/src/ghostex_cli/{mod.rs,actions.rs}` — mirror the
`/api/handoffSessionChatDraft` chain exactly.

| Endpoint | Params (plus projectId, sessionId) | Result |
|---|---|---|
| `/api/readSessionChatQueue` | — | `{ queue, draft? }` |
| `/api/queueSessionChatPrompt` | `text` | `{ queue, prompt }` |
| `/api/updateSessionChatQueuedPrompt` | `promptId`, `text?`, `retry?: boolean` | `{ queue }` |
| `/api/removeSessionChatQueuedPrompt` | `promptId` | `{ queue, prompt }` — returns the removed row so Edit can pull its text into the composer |
| `/api/reorderSessionChatQueue` | `promptIds: string[]` | `{ queue }` |
| `/api/sendSessionChatQueuedPrompt` | `promptId` | `{ queue, sent: boolean }` |
| `/api/setSessionChatDraft` | `content`, `clientId` | `{ draft }` |

`retry: true` moves a `failed` row back to `queued` and clears `errorMessage`.
Every mutation broadcasts a `sessionChatState` frame carrying the new `queue`
(and `draft` for the draft endpoint) to that session's followers.

### Transport (`sidebar/chat/session-chat-transport.ts`, also bead `.1`)

All new methods are **optional**, so a host that cannot reach them omits them
and the shared UI hides the corresponding control:

```ts
  queuePrompt?(params: { text: string }): Promise<GxserverSessionChatQueueResult>;
  updateQueuedPrompt?(params: { promptId: string; text?: string; retry?: boolean }): Promise<GxserverSessionChatQueueResult>;
  removeQueuedPrompt?(params: { promptId: string }): Promise<GxserverSessionChatRemoveQueuedPromptResult>;
  reorderQueue?(params: { promptIds: string[] }): Promise<GxserverSessionChatQueueResult>;
  sendQueuedPrompt?(params: { promptId: string }): Promise<GxserverSessionChatQueueResult>;
  setDraft?(params: { content: string; clientId: string }): Promise<void>;
```

---

## 3. Server (beads `.2` storage/endpoints, `.4` scheduler)

### Storage — migration `0021_session_chat_queue`

Append to the migration list in `gxserver-rs/src/constants.rs` and add the DDL in
`gxserver-rs/src/storage.rs` next to `0020_delayed_sends`.

```sql
CREATE TABLE IF NOT EXISTS session_chat_queued_prompts (
  promptId     TEXT PRIMARY KEY,
  projectId    TEXT NOT NULL,
  sessionId    TEXT NOT NULL,
  position     INTEGER NOT NULL,
  text         TEXT NOT NULL,
  state        TEXT NOT NULL DEFAULT 'queued',   -- queued | sending | failed
  errorMessage TEXT,
  createdAt    TEXT NOT NULL,
  updatedAt    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_session_chat_queued_prompts_session
  ON session_chat_queued_prompts(projectId, sessionId, position);

CREATE TABLE IF NOT EXISTS session_chat_drafts (
  projectId      TEXT NOT NULL,
  sessionId      TEXT NOT NULL,
  content        TEXT NOT NULL,
  originClientId TEXT NOT NULL,
  updatedAt      TEXT NOT NULL,
  PRIMARY KEY (projectId, sessionId)
);
```

Storage + endpoint logic lives in a new `gxserver-rs/src/session_chat_queue.rs`.
Keep `server.rs` to dispatch only. Register the index name wherever
`storage.rs` keeps its known-index list (see `idx_stashed_prompts_updated`).

### Scheduler — `gxserver-rs/src/session_chat_queue_runtime.rs`

Model it on `delayed_sends.rs`, which already solves this shape:

- 1 s tick, started from the same place `DelayedSendRuntime::start` is started.
- Only consider sessions that actually have a queued head row — never walk
  transcripts for sessions with an empty queue.
- Ready when `presentation_activity(&session, &now) == "idle"` **and** the chat
  lifecycle is not `working`. Reuse the cheapest existing signal
  `session_chat.rs` exposes (a live follower's cached lifecycle when present,
  otherwise a bounded tail read) rather than adding a per-tick full scan.
- Stability window `SESSION_CHAT_QUEUE_STABILITY_MS = 2_000`, tracked the same
  way `delayed_sends` tracks `nonWorkingSinceAt` (reset the instant the session
  looks busy again). Persist it in memory or on the row; do not re-fire on a
  blip.
- A blocking `terminalNotice` of severity `error` on the session (login expired,
  trust prompt, agent exited) is **not** a delivery opportunity: mark the head
  row `failed` with the notice title as the reason and stop draining.
- Fire: claim the head row (`state = 'sending'`, guarded UPDATE so two ticks
  cannot double-claim), call the same internal send path
  `/api/sendSessionChatMessage` uses, then delete the row on success or set
  `state='failed'` + `errorMessage` on error. Broadcast a state frame either way.
- Restart recovery: any row left in `sending` becomes `failed` with
  "gxserver restarted while this prompt was being delivered." — never silently
  re-send, which is what `delayed_sends::recover_after_restart` does and why.
- Auto Sleep: at the `crate::session_keep_awake::sleep_trigger_is_automatic`
  call site in `zmx.rs:591`, also decline when the session has a non-empty
  queue. Explicit user sleeps are untouched.

---

## 4. Shared chat UI (bead `.3`)

Files: `sidebar/chat/*`, `sidebar/styles/chat.css`.

- New `sidebar/chat/session-chat-queue-rows.tsx` — the strip, rendered directly
  above the composer input inside the composer container. One row per prompt,
  **single line**: the first non-empty line of the text, ellipsized. Controls:
  drag handle, Edit, Send now, Delete. A `failed` row shows its reason and a
  Retry. Cap the strip around 5 rows then scroll.
- New `sidebar/chat/session-chat-queue.ts` — pure client logic (row preview
  text, optimistic reorder, capability check) so it stays testable and the
  component stays dumb.
- `session-chat-composer.tsx`:
  - `Tab` queues when no picker is open and `draft.trim() !== ""`.
  - Long-press (≈500 ms pointerdown) on the Send button queues instead of
    sending; a normal tap still sends. Must work with touch on the mobile
    WebView.
  - While `isWorking`, render **Send** instead of **Stop** when
    `draft.trim() !== ""` (`session-chat-composer.tsx:1478`).
  - Edit: remove the row, queue the current composer text at the end if it is
    non-empty, then load the removed text into the composer.
  - Draft push on blur / session switch / unmount / `visibilitychange` to
    hidden. Keep the existing per-client `localStorage` cache as-is; the server
    draft is a sync channel, not a replacement.
  - "Newer draft from another device — Use / Dismiss" bar when an incoming
    draft has a newer `updatedAt`, different `content`, and a different
    `originClientId`. Never overwrite a non-empty local composer without it.
- `use-session-chat.ts` holds queue + draft state from frames/reads and exposes
  the mutations. Hide every queue control when the read result omits `queue`
  (old daemon) or the transport lacks the method.
- Drag-to-reorder: reuse whatever the repo already uses in the sidebar, and be
  aware of the recorded dnd-kit failure modes (click swallowing, activation
  distance-vs-delay). Touch drag must work on the phone.

## 5. Hosts

- **Web** (bead `.5`) — `ghostex-web/src/chat/session-chat-transport.ts`,
  `ghostex-web/src/app/*`: implement the transport methods over the existing
  gxserver client; add the top-left `Queued: N` button over the terminal view in
  `agents-workspace.tsx` that switches `sessionSurfaceMode` to `chat`.
- **gpui** (bead `.6`) — `gpui/sidebar/chat-main.tsx` plus the bridge ops and
  RPC calls in `gpui/src/main.rs` (mirror how `/api/handoffSessionChatDraft` is
  called there). The terminal-view `Queued: N` control must live in existing
  pane chrome as a normal sibling frame. **Do not** float an interactive overlay
  over the Ghostty/terminal surface — that is an explicit AGENTS.md violation.
  If there is no chrome slot for it, stop and report instead of adding one.
- **Mobile** (bead `.7`) — `mobile-chat/session-chat-main.tsx` bridge ops,
  `mobile/src/chat/session-chat-bridge.ts`, `mobile/src/commands/ghostexCli.ts`
  (new CLI verb wrappers, mirroring `handoffSessionChatDraftCommand`),
  `mobile/src/chat/SessionChatWebView.tsx`, `mobile/src/screens/TerminalScreen.tsx`
  (`Queued: N` button in terminal view) and
  `mobile/src/components/terminal/PromptEditorSheet.tsx` (seed from and write
  back the same synced draft when `destination === 'chat'`). Long-press-to-queue
  is the only queue gesture on the phone, so it has to feel right under touch.

## 6. Sidebar badge (bead `.8`)

Count of queued prompts per session, projected onto the sidebar session item and
rendered as a small filled circle over the agent icon.

- Mirror the `delayedSendDeadlineAt` chain end to end: `gxserver-rs/src/agents.rs`
  / `sidebar_hud.rs` → `shared/session-grid-contract-*.ts` →
  `shared/gxserver-presentation-sidebar-projection.ts` → `sidebar/session-card-content.tsx`
  → `gpui/sidebar/gxserver-runtime.ts` and the web/gpui hosts.
- Badge: filled circle, background `#f6c945` (same yellow as
  `.session-delayed-send-agent-icon` in `sidebar/styles/session-cards.css`),
  small dark number, positioned with **CSS anchor positioning** over the agent
  icon so it cannot change the session card's layout. Hidden at count 0.
- Unlike Delayed Send, this badge does **not** replace the agent icon.
