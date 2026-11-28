# ghostex-push-relay

Ghostex's push relay, at `https://share.ghostex.dev/live-activity/` and `https://share.ghostex.dev/push/`. It holds
the Apple push key (`.p8`); a user's computer (gxserver) asks it to send a push, and the relay sends it to Apple.
The `/push/` routes also hold, in memory only, a mailbox for a Live Activity's update token and one widget
snapshot per id. It lives beside `server/share-relay` (same box, relay-eu.ghostex.dev, same Caddy), builds into
one static binary (`build-linux.sh`) and is its own crate (own `Cargo.lock`), so gxserver's build never compiles
it. This README states what the relay does; what Apple, iOS or WidgetKit then do is in Apple's documentation.

## Push routes

| Route | Body (JSON) | Goes to Apple as |
| --- | --- | --- |
| `POST /live-activity/v1/send` | ≤ 8 KB: `token`, `event`, `environment`, `timestamp`, `contentState`, `staleDate`, `dismissalDate`, `priority` | type `liveactivity`, topic `com.maddada.ghostex.ios.push-type.liveactivity`, `priority` (default 10), expiry now + 1 h |
| `POST /push/v1/alert` | ≤ 16 KB: `token`, `environment`, `kind`, `timestamp`, `envelope`, `threadId`, `data`, `sound` | type `alert`, topic `com.maddada.ghostex.ios`, priority 10, expiry `timestamp` + 1 h |
| `POST /push/v1/alert`, `kind: "background"` | `token`, `environment`, `kind`, `timestamp` only | type `background`, same topic, priority 5, expiry `timestamp` + 10 min, `{"aps":{"content-available":1}}` |
| `POST /push/v1/widgets` | `token`, `environment` only | type `widgets`, topic `com.maddada.ghostex.ios.push-type.widgets`, priority 5, expiry now + 1 h, `{"aps":{"content-changed":true}}` |

A Live Activity update (the times are examples; the relay checks them against its clock on arrival):

```json
{ "token": "<the activity's push token; hex>", "event": "update", "environment": "production",
  "timestamp": 1794355519, "staleDate": 1794356419, "priority": 10,
  "contentState": { "updatedAt": "…", "counts": { "working": 1, "waiting": 0 }, "agents": [], "moreCount": 0 } }
```

| Field | Accepted (anything else, and any field not listed for the route: 400) |
| --- | --- |
| `token` | hex, 64 to 512 characters, whole bytes; sent lowercased. For `start`, the push-to-start token. |
| `environment` | `production` (default, `api.push.apple.com`) or `sandbox` (`api.sandbox.push.apple.com`) |
| `timestamp` | unix seconds, from one hour ago to 5 minutes ahead |
| `event` | `update`, `end` or `start`. A start also gets `attributes-type` `GhostexActivityAttributes`, empty `attributes` and the alert "Ghostex" / "Your agents are working." |
| `contentState` | a JSON object (its fields are not checked), required for `update` and `start`, optional for `end`; sent as `content-state` |
| `staleDate` | optional, unix seconds from one minute ago to 12 hours ahead |
| `dismissalDate` | `end` only, from one minute ago to 4 hours ahead; on `update` or `start` the relay answers 400 `dismissalDate is for end only.` |
| `priority` | 5 or 10 |
| `kind` | an event of the table below, or `background` |
| `envelope` | optional; standard base64 with padding, at most 3072 characters (413 above), decoding to the byte `1`, a 12-byte nonce and at least 16 more bytes. Sent unchanged as top-level `envelope`; the relay never decrypts it and cannot tell a ciphertext from other bytes. |
| `threadId` | optional; `<machineId>:<sessionId>`, split at the first `:`, each part 1 to 64 printable ASCII characters without spaces; sent as `aps.thread-id` |
| `data` | optional; exactly `kind`, `machineId`, `sessionId` (each 1 to 64 printable ASCII characters without spaces), optional `notificationId` (1 to 128 such characters), optional `worker` (`true`/`false`). Any other key fails the request with 400, including the `openSessionId` and `url` that gxserver's Expo push can carry (`server/src/push/message.rs`). `data.kind` is checked only for form. Sent as top-level `data`. |
| `sound` | optional; `true` adds `"sound":"default"` |

An alert's payload is `{"aps":{"alert":{"title":"Ghostex","body":"<words>"},"mutable-content":1,"thread-id":…,"sound":…},"envelope":…,"data":…}`
(the last four only when given). The title and words are always the relay's:

| `kind` | Words | `kind` | Words |
| --- | --- | --- | --- |
| `finished` | Finished | `security` | Security: check this was you |
| `approval` | Waiting for approval | `stuck` | Not moving: it may be stuck |
| `waiting` | Waiting for you | `blocked` | Blocked: out of usage on every account |
| `question` | Has a question | `threadReport` | Reported to its coordinator |
| `failed` | Ended with an error | `outOfMemory` | Stopped to free memory |

Answers of the push routes (JSON with `ok`):

| Status | `error` | Meaning |
| --- | --- | --- |
| 200 | | Apple answered 200 (`apnsId`). Whether a phone shows the push is checked on the phone. |
| 400 | `bad-request` | Not JSON, or a field missing, unknown or out of range (`detail` says which). |
| 410 | `token-gone` | Apple answered 410, `BadDeviceToken`, `Unregistered` or `DeviceTokenNotForTopic` (`appleStatus`, `appleReason`), or, without those two fields, the relay still remembers such an answer (see Lifetimes). Drop the token. |
| 413 | `too-large` | The payload for Apple over 4096 bytes, or an envelope over 3072 characters. |
| 413 | (not JSON) | Body over Caddy's `max_size` (8KB / 16KB) or the relay's (8 KiB / 16 KiB), before the route's code. |
| 429 | `rate-limited` | A relay limit or no slot to Apple (see Checks); `retryAfter` seconds and `Retry-After`. |
| 429 | `apple` | Apple answered 429 (`appleStatus`, `appleReason`; Apple's documentation lists the reasons). No `retryAfter`. |
| 502 | `apple` / `apple-unreachable` | Apple answered another status / no answer within 15 seconds, no connection or another request failure. |
| 503 | `not-configured` | The relay has no key. |

## Store routes: `/push/v1/mailbox` and `/push/v1/snapshot`

The id goes in `Authorization: Bearer <id>`: 64 hex characters, either case. The relay reads it only from that
header, never from the URL, files the entry under the SHA-256 of the lowercased id and keeps no id. The store
routes need no Apple key.

| Request | Answer |
| --- | --- |
| mailbox `PUT`: a hex token, 64 to 512 characters, whitespace around it trimmed, body ≤ 1024 bytes | 200 `{"ok":true}`; stored lowercased, replaces what the id held, its 10 minutes start again |
| mailbox `GET` | 200 `{"ok":true,"token":"<lowercase hex>"}` with `Cache-Control: no-store`, and the entry is removed; 404 `empty` |
| mailbox `HEAD` | 405 with `Allow: GET, PUT`; the entry stays |
| snapshot `PUT`: any bytes, 1 to 8192 | 200 `{"ok":true}`; stored unchanged (never decrypted or checked), replaces what the id held, its 24 hours start again |
| snapshot `GET` | 200 with the bytes as stored, `application/octet-stream`, `Cache-Control: no-store`; it stays. 404 `empty` |

Refusals (JSON, nothing stored): 400 `bad-request` (no valid Bearer id, a mailbox body that is not a token, an
empty snapshot); 404 `empty` (never stored, taken, expired or lost in a restart); 413 `too-large` (over 1024 /
8192 bytes) or, not JSON, over Caddy's 16KB or the relay's 16 KiB; 429 `rate-limited` or `too-many-new-ids`
with `retryAfter` and `Retry-After`; 503 `store-full` with `retryAfter` 60.

## Health

`GET /live-activity/healthz`: `{"ok":true,"configured":…,"sent":N,"refused":N,"rateLimited":N,"appleErrors":N,"goneTokens":N}`.
`GET /push/healthz`: the same for the `/push/` routes plus `stored`, `storeFull`, `mailboxEntries`, `snapshots`,
`storeBytes`. Counts start at zero with the process. `configured` means the key file and `apns.conf` were read
and parsed and the HTTP client was built, not that Apple accepts the key.

## Checks and limits

Every limit is a token bucket: up to *burst* requests, refilled at *refill*; an empty bucket answers 429 with the
wait. The checks run in this order; the first refusal ends the request, and a bucket is spent when its step runs.

| Step | Push routes | Store routes |
| --- | --- | --- |
| 1 | caller address: 60, refill 60/min (Live Activity and `/push/` separately) → 429 | caller address: 60, refill 60/min (not the mailbox `HEAD`) → 429 |
| 2 | key loaded → 503 | Bearer id → 400 |
| 3 | body and fields → 400 / 413 | body size and form → 413 / 400 |
| 4 | token remembered as gone → 410 | id the shelf does not hold: new ids per address, 20, refill 20/hour → 429 `too-many-new-ids` |
| 5 | token: Live Activity 10, refill 10/min; push-to-start 2, refill 1 per 10 min; alert 10, refill 10/min; background 3, refill 3/hour; widget 4, refill 6/hour → 429 | new id: store caps (5,000 entries, 5 MiB of values, both shelves; expired entries dropped first) → 503 `store-full` |
| 6 | whole relay: 300, refill 3000/min, one bucket per family → 429 | id: 20, refill 20/min → 429 (a held id's `PUT` takes steps 6 then 5; a `GET` of an id not held answers 404 here, uncharged) |
| 7 | free slot of 64 per family within 5 seconds → 429 `retryAfter` 5 | stored, taken or read |
| 8 | Apple, 15-second timeout; on 403 `ExpiredProviderToken`/`InvalidProviderToken` with a provider token ≥ 20 minutes old, a new one and one retry | |

So the whole-relay bucket is spent before the slot wait and before Apple answers: well-formed requests with
tokens Apple refuses can empty it, and real pushes then get 429 until it refills. The new-id quota is spent per
attempt, also when the store then answers `store-full`; an id held in the mailbox is new to the snapshot store.
The caller address is the last parsable `X-Forwarded-For` entry with `--behind-proxy` (as the unit runs it), else
the connection's; IPv6 counts by its /64, IPv4-mapped IPv6 as IPv4. Bodies refused for size before the route's
code, and health checks, spend nothing. Nothing held is pushed out for something new; a smaller entry can fit
where a larger does not, and space a mailbox `GET`, an expiry or a smaller replacement frees is free at once.
Tables: 200,000 keys each on the Live Activity route, 7,000 for each of the eight per-address, per-token and
per-id `/push/` tables. A full table drops its idle keys, then refuses a new key with 429 (`retryAfter` 60).

## Lifetimes

| What | Lives | Ends sooner |
| --- | --- | --- |
| mailbox entry | 10 minutes from its last `PUT` | the first `GET`; a restart |
| snapshot | 24 hours from its last `PUT` | a restart |
| token Apple called gone (per environment; on `/push/` also per topic) | until the cleanup once a minute finds it 24 hours old | when full (100,000 Live Activity, 7,000 `/push/`), expired then oldest go; a restart |
| idle bucket | Live Activity 10 minutes; `/push/` until it would be full again, at least 10 minutes (10 for addresses, alert tokens and ids, 40 for widget tokens, 60 for background tokens and new ids) | a restart |
| provider token (signed JWT) | 40 minutes | the 403 renewal in step 8 |

An expired entry is never handed out (each access checks it); its memory is freed then, at the cleanup once a
minute, or when a `PUT` needs space. A restart empties everything above and the health counts; nothing is
written to disk.

## Who can call it, and what that allows

The relay does not authenticate its callers.

- Whoever has a device token can make the relay push to it within the limits. The route fixes topic and push
  type; the sender picks a Live Activity's content and event, an alert's event (not its words), envelope,
  `threadId` and `data`, and when background and widget pushes go.
- The mailbox and snapshot store are open within limits: the Bearer id is the only capability, any 64 hex
  characters are taken, and the relay cannot tell a paired phone or computer from anyone else. The shared caps,
  the per-address new-id quota, the address and id quotas and the short lifetimes bound them.
- Whoever holds a mailbox id can take the entry, a token in plain hex (meant to be the activity's update token,
  not encrypted), and then update or end that Live Activity through the Live Activity route; or replace it.
- Whoever holds a snapshot id can read and replace it. The relay stores the bytes unchanged and does not check
  that they are encrypted; their confidentiality rests on the computer encrypting them before the upload.
- Strangers can fill the stores up to the caps: new ids then get `store-full` for everyone until space frees,
  while held ids keep working. Many addresses can likewise fill a limit table (new keys get 429).
- The key is read at start from `AuthKey.p8` and `apns.conf`, which the deploy steps make readable only to root
  and the service's group; no answer or log line carries it or its two ids. Apple receives per push the token
  (in the path), push type, topic, priority, expiry, the signed provider token and the payload; a widget push
  carries no snapshot content.

### Memory

The unit sets `MemoryMax=128M`; the limits are fixed in the code, and the `/push/` caps were sized against a
local macOS measurement with every table and the store full, without HTTP. Memory under load is an open
availability risk: a `POST` or `PUT` body is read in full (up to 8 KiB / 16 KiB) before the address quotas and
id checks, with no overall budget for bodies being read, so many parallel requests add memory on top of full
tables and a full store. Whether that reaches the limit on Linux is not measured. If the kernel kills the relay
for exceeding `MemoryMax`, the unit restarts it (`Restart=always`, `RestartSec=2`, within systemd's start
limits) and everything under Lifetimes is lost.

## Logs

One line per event on stdout (the journal): at start `configured: the key loaded; target=apple` or
`not configured: <reason>; …` (what is missing or wrong, never a value from the key's files) and
`listening on <address>`; each push Apple answered (`sent` / `push sent`: event or kind, environment, token label,
Apple's status and reason, ms) or did not (`failed` / `push failed`); each refusal in a route's own code
(`reason=` `rate-limited` with `scope=` `address`/`token`/`global`/`id`, `busy`, `not-configured`,
`bad-request`, `too-large`, `token-gone`, `too-many-new-ids`, `store-full`); each store and mailbox take. No line
for health checks, snapshot reads, reads of an empty id, the mailbox `HEAD` or size-refused bodies. A label is
the first 8 hex of the SHA-256 of the token or lowercased id; no line carries a token, an id, a content state,
an envelope, `threadId`, `data`, a snapshot or anything from the key's files. Whether Caddy's access log keeps
the `Authorization` header depends on its logging configuration.

## Run, build, deploy

Locally: `cd server/push-relay && cargo run -- --listen 127.0.0.1:58810 --key /path/AuthKey.p8 --config /path/apns.conf`;
`--apple-stand-in http://127.0.0.1:<port>` sends to a stand-in for Apple over HTTP/2 without TLS
(`<origin>/production/3/device/<token>` or `/sandbox/…`); any address other than loopback is refused.

Build: `server/push-relay/build-linux.sh [x64|arm64|all]` builds static musl binaries from macOS or Linux (Zig
compiles `ring`'s C code; `ZIG` picks the binary) into `dist/ghostex-push-relay-<version>-linux-<arch>.tar.gz`
(binary, systemd unit, Caddy block, `apns.conf.example`, this README) with a `.sha256` beside it.

Deploy (relay-eu.ghostex.dev, beside the share relay):

1. `useradd --system --user-group --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin ghostex-push-relay`
2. `install -d -o root -g root -m 0755 /opt/ghostex-push-relay`; the binary to `/opt/ghostex-push-relay/ghostex-push-relay` (root, 0755).
3. `install -d -o root -g ghostex-push-relay -m 0750 /etc/ghostex-push-relay`.
4. `cp ghostex-push-relay.service /etc/systemd/system/ && systemctl daemon-reload && systemctl enable --now ghostex-push-relay`
   (the unit runs it on `127.0.0.1:58810` with `--behind-proxy` as `ghostex-push-relay`).
5. Add both `handle` blocks of `Caddyfile` (`/live-activity/*` with `max_size 8KB`, `/push/*` with `max_size 16KB`)
   inside the `share.ghostex.dev` site of `/etc/caddy/Caddyfile`, above its `reverse_proxy` line; run
   `caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile`, then `systemctl reload caddy` while no
   shared chat is connected (`ss -tn state established '( sport = :58800 )'`).
6. `curl -s https://share.ghostex.dev/live-activity/healthz` and `curl -s https://share.ghostex.dev/push/healthz`.

The key: `AuthKey.p8` to `/etc/ghostex-push-relay/AuthKey.p8`, `KEY_ID=` and `TEAM_ID=` to
`/etc/ghostex-push-relay/apns.conf`, both `root:ghostex-push-relay` 0640, then `systemctl restart
ghostex-push-relay`. Without them the relay starts with `"configured":false`, pushes get 503 and the store works.
Check in this order: (1) the journal says `configured: the key loaded; target=apple` and both health checks
`"configured":true` (the files were read); (2) a push through each route and environment shows whether Apple
accepts the key for that topic (200 `apnsId`, or `appleStatus`/`appleReason`); (3) the phone shows it.

Updating: replace the binary (and the unit, then `systemctl daemon-reload`), `systemctl restart
ghostex-push-relay`, check both health routes. Between stop and start the relay accepts no connections, so
requests then fail (Caddy's answer is not checked here); after the start everything under Lifetimes is empty.
To add `/push/` to a Live-Activity-only relay, also add the `handle /push/*` block (step 5) and reload Caddy.

Raising a limit: limits are fixed in the code, not read from a setting: rates and `/push/` caps in
`LimitConfig::default()` (`src/limits.rs`), the Live Activity route's 200,000 keys in `MAX_KEYS` beside it, body
sizes in `src/lib.rs` and `src/push_routes.rs` (plus the Caddy `max_size`). Change it there, measure the memory
again with every table and the store full, raise `MemoryMax` in `deploy/ghostex-push-relay.service` above that,
rebuild with `build-linux.sh`, deploy binary and unit (`systemctl daemon-reload`, then restart).
