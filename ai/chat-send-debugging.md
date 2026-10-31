# Debugging a chat message that did not send

Read this when the user reports that pressing Enter in the chat did nothing, the message "came back", or a send failed. The logs below already hold the answer in most cases; read them before reproducing anything.

## 1. gxserver's send log (always on)

`~/.local/state/ghostex/logs/session-chat-send-failures.jsonl` on macOS and Linux, `%LOCALAPPDATA%\Ghostex\Logs\session-chat-send-failures.jsonl` on native Windows (5 MB, rotated to `.1`–`.3`). gxserver writes it on every machine, with no debug switch:

- `sessionChatSendFailure`: every refused `/api/sendSessionChatMessage`, `setSessionChatDraft`, `queueSessionChatPrompt` and `sendSessionChatQueuedPrompt`, with `code`, `message`, `projectId`, `sessionId` and a text fingerprint (never the text).
- `sessionChatSendFailureTerminal`: the same refusal followed by the session's **visible terminal screen** (`terminal.text`) captured right after the failure, or `captureError` when the zmx daemon could not be reached (`No such file or directory` means the daemon was gone, so the session was asleep).
- Recoveries, where the send cleared something before typing (`terminal.tail` holds the screen it acted on):
  - `sessionChatSendClosingClaudePanel`: Escape closed Claude's Settings, an offer from `server/src/session_chat_claude_popups.rs`, or its focused background-agents list; or `0` dismissed Claude's "How is Claude doing this session?" rating survey, which had pushed the input box off a short screen (`server/src/session_chat_claude_rating_survey.rs`).
  - `sessionChatSendWokeSession` / `sessionChatSendWakeFailed`: the send found no zmx daemon and woke the session.
  - `sessionChatSendHeldForStartingAgent`: the agent was still printing "Restoring session...", so the message went to the queue as a startup send.
  - `sessionChatSendPressedEnterAgain` (Codex): the message was still in Codex's input box after Return, so the send pressed Return once more. Each one is a cause to fix at its source (`server/src/session_chat_send_submit.rs`); `sessionChatSendNotSubmitted` means Codex kept it even then and the send failed.
  - `sessionChatSendWaitedForBusyAgent`: the agent was mid-turn and its screen was slow to show the send's clear, paste or Return, so the send kept the input line and waited, typing nothing more (`server/src/session_chat_send/busy_input_wait.rs`). A send that waited five minutes without the agent catching up fails with "The agent stayed in the middle of a turn for five minutes…" and its `sessionChatSendFailedStep` names `AgentStalled`; nothing is retyped then.
  - `sessionChatSendDroppedStaleEscape`: a queued prompt's play button (desktop) or Send now (phone) was clicked, and before its Escape reached the terminal a message was submitted, the agent's transcript recorded a prompt it took, or no prompt was waiting in the agent's queue (Claude's "Press up to edit queued messages" in its empty input box, Codex's queued-message preview), so the Escape was dropped instead of cancelling or interrupting that prompt (`server/src/session_chat_send/queued_prompt_escape.rs`).
- The queue's self-heal ladder, for a queued row (page answers, a Tab-queued message, a thread report, `ghostex agents send`) waiting on an idle agent whose input box gxserver cannot read (thresholds in `server/src/session_chat_queue_runtime/wait_reason.rs`): `sessionChatQueueRedrewClaude` (Claude's Ctrl+L, from 20 s, once a minute), `sessionChatQueueResizedForRepaint` (the terminal one row shorter and back, from 45 s, every two minutes, any agent), `sessionChatQueueHeldForInputBox` (the row has waited a minute; it now shows why in the chat), `sessionChatQueueLastResort` (at three minutes the row goes to the send path, which checks the screen itself), `sessionChatQueueNotSent` (the send path still found no box for two minutes, so the row is Not sent with Retry), `sessionChatQueueWatchdog` (a row held five minutes by any other gate on an agent idle by every signal), and `sessionChatQueueStuckOnInputBox` (the row has waited five minutes on the input box: it now says what the terminal shows, and one notification goes to the bell; the retries go on). A row waiting with none of these is held by the agent's activity, its transcript, a compaction or a notice, and its chat row says which.
- `sessionChatDeliveryNotice`: the delivery watchdog showed a card such as "Your message might not have reached the agent", with the screen it saw. Compare its time with the agent's own transcript (Codex: `~/.codex/sessions/**/rollout-*<agent session id>.jsonl`, `user_message` rows) to see when, or whether, the message was recorded.

Find the user's report by session and time (the `ts` fields are UTC):

```bash
python3 - <<'EOF'
import json, glob
for path in sorted(glob.glob('/Users/madda/.local/state/ghostex/logs/session-chat-send-failures.jsonl*')):
    for line in open(path):
        d = json.loads(line)
        if d.get('sessionId') == 'G3jyc':   # the Routing ID's second half from Copy Details
            print(d['ts'], d['event'], d.get('code'), d.get('message') or d.get('reason'))
            screen = (d.get('terminal') or {}).get('text') or (d.get('terminal') or {}).get('tail')
            if screen: print(screen[-1500:])
EOF
```

To see every kind of failure there has been, group the `sessionChatSendFailure` lines by `(endpoint, code, message)`.

## 2. The send ledger and the agent's own records

Successful sends write nothing to the log above. gxserver's database (`~/.local/state/ghostex/gxserver/state.db`, `%LOCALAPPDATA%\Ghostex\State\gxserver\state.db` on Windows) keeps every send RPC for 24 hours in `session_chat_send_requests`: `createdAtMs` is when the request arrived, `updatedAtMs` when its job finished (after the session's earlier jobs), and a row with `textBytes` 0 in its `response` and no `needles` is a raw key (`sendKey`: the play button's Escape, the Mode pill's Shift+Tab). Open it read-only (`sqlite3 'file:…/state.db?mode=ro'`) and filter on `projectId` and `sessionId`.

Claude Code records the moment it takes a prompt in `~/.claude/history.jsonl` (`timestamp` in ms, with `sessionId`). In its transcript, a lone `total_tokens_reminder` attachment row followed by no user row means Claude took a Return and then dropped the prompt back into its input box: an Escape or Ctrl+C reached it before it wrote the prompt.

## 3. The desktop chat's own log (debug scenario)

`~/.local/state/ghostex/logs/gpui-session-chat-debug.jsonl`, written only while Show debug UI controls and the Chat area under Diagnostic logs are on. `sessionChat.nativeRpcResult` lines carry `method`, `sessionKey` (`projectId:sessionId`), `errorCode` and `errorMessage` for every chat RPC, so a refusal the user saw can be matched to the gxserver line above.

## 4. The live screen

`/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/zmx history <zmxName>` prints the session's scrollback now (the zmx name is `S90-<projectId>-<sessionId>`, also in Copy Details as "Persistence").

On every platform, `ghostex read-text <selector> [--lines n]` prints the same screen through gxserver (`send-text`, `send-enter` and `send-key` type into it).

Text in Claude Code's input row is not always a message: when its turn ends, Claude shows a guess at the user's next message there in faint text, and the box is empty. `read-text` marks that row "(faint suggestion from Claude, not typed: the input box is empty)" and `--json` returns it as `inputSuggestion`. Enter on it does nothing, which is correct; never submit a suggestion as the user's message. With an older gxserver, check the style with `zmx history <zmxName> --vt` (`wmx.exe history <zmxName> --scrollback 0 --vt`): a suggestion follows `❯` as `ESC[0;2m…`. On native Windows the daemon is wmx: run `wmx.exe history <zmxName>` from the folder of the running `gxserver.exe` (`Get-Process gxserver | Select-Object Path`, usually `C:\Program Files\Ghostex\resources\native\`), not a copy left by an older per-user install under `%LOCALAPPDATA%\Ghostex\current`, whose `list` can hang. `The system cannot find the file specified. (os error 2)` from any of these means the session has no daemon: it is asleep, or it is a draft created with `create-agent --defer-start` that no client has opened yet.

## What the codes mean

- `composerNotReady`: gxserver could not see the agent's input box. The logged screen shows what was there instead: a dialog Ghostex does not know yet (add it to `session_chat_claude_popups.rs` only if Claude's own source shows that Escape declines it), a booting CLI, or a damaged repaint.
- `invalidState` "… Answer it in chat before sending": a question, approval, trust or model-switch card is waiting; this refusal is deliberate.
- `dependencyUnavailable` "The terminal did not accept the pasted message": the paste never appeared in the input box; the screen shows where the keyboard was.
- `composerNotCleared`: text left in the terminal's input box could not be cleared; with `captureError` the daemon was gone.
