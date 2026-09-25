# Debugging a chat message that did not send

Read this when the user reports that pressing Enter in the chat did nothing, the message "came back", or a send failed. The logs below already hold the answer in most cases; read them before reproducing anything.

## 1. gxserver's send log (always on)

`~/.local/state/ghostex/logs/session-chat-send-failures.jsonl` (5 MB, rotated to `.1`–`.3`). gxserver writes it on every machine, with no debug switch:

- `sessionChatSendFailure`: every refused `/api/sendSessionChatMessage`, `setSessionChatDraft`, `queueSessionChatPrompt` and `sendSessionChatQueuedPrompt`, with `code`, `message`, `projectId`, `sessionId` and a text fingerprint (never the text).
- `sessionChatSendFailureTerminal`: the same refusal followed by the session's **visible terminal screen** (`terminal.text`) captured right after the failure, or `captureError` when the zmx daemon could not be reached (`No such file or directory` means the daemon was gone, so the session was asleep).
- Recoveries, where the send cleared something before typing (`terminal.tail` holds the screen it acted on):
  - `sessionChatSendClosingClaudePanel`: Escape closed Claude's Settings, an offer from `server/src/session_chat_claude_popups.rs`, or its focused background-agents list.
  - `sessionChatSendWokeSession` / `sessionChatSendWakeFailed`: the send found no zmx daemon and woke the session.
  - `sessionChatSendHeldForStartingAgent`: the agent was still printing "Restoring session...", so the message went to the queue as a startup send.

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

## 2. The desktop chat's own log (debug scenario)

`~/.local/state/ghostex/logs/gpui-session-chat-debug.jsonl`, written only while Show debug UI controls and the session-chat Diagnostic disk logging scenario are on. `sessionChat.nativeRpcResult` lines carry `method`, `sessionKey` (`projectId:sessionId`), `errorCode` and `errorMessage` for every chat RPC, so a refusal the user saw can be matched to the gxserver line above.

## 3. The live screen

`/Applications/Ghostex.app/Contents/Resources/Web/gxserver/bin/zmx history <zmxName>` prints the session's scrollback now (the zmx name is `S90-<projectId>-<sessionId>`, also in Copy Details as "Persistence").

## What the codes mean

- `composerNotReady`: gxserver could not see the agent's input box. The logged screen shows what was there instead: a dialog Ghostex does not know yet (add it to `session_chat_claude_popups.rs` only if Claude's own source shows that Escape declines it), a booting CLI, or a damaged repaint.
- `invalidState` "… Answer it in chat before sending": a question, approval, trust or model-switch card is waiting; this refusal is deliberate.
- `dependencyUnavailable` "The terminal did not accept the pasted message": the paste never appeared in the input box; the screen shows where the keyboard was.
- `composerNotCleared`: text left in the terminal's input box could not be cleared; with `captureError` the daemon was gone.
