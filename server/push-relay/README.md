# ghostex-push-relay

Ghostex's push relay, at `https://share.ghostex.dev/live-activity/` and `https://share.ghostex.dev/push/`. It
holds the Apple push key (`.p8`); a user's computer (gxserver) asks it to send a push, and the relay sends it to
Apple's push service. The `/push/` routes also hold two small things in memory: a mailbox for a Live Activity's
new update token, and one widget snapshot per id.

It lives beside `server/share-relay`: the same box (relay-eu.ghostex.dev), behind the same Caddy, built into one
static binary (`build-linux.sh`). It is its own crate (own `Cargo.lock`), so gxserver's build never compiles it.

This README states what the relay does. What Apple, iOS or WidgetKit do with a push is in Apple's documentation;
the relay's tests talk to a stand-in for Apple.

## Live Activity: `POST /live-activity/v1/send`

`Content-Type: application/json`, at most 8 KB. An update:

```json
{
  "token": "<the activity's push token; hex>",
  "event": "update",
  "environment": "production",
  "timestamp": 1794355519,
  "contentState": { "updatedAt": "…", "counts": { "working": 1, "waiting": 0 }, "agents": [], "moreCount": 0 },
  "staleDate": 1794356419,
  "priority": 10
}
```

The times are examples: the relay checks them against its clock when the request arrives, so these numbers
are refused once they are past. An `end` may add `"dismissalDate"`.

- `token`: hex, 64 to 512 characters (32 to 256 bytes); for `start`, the push-to-start token. Sent lowercased.
- `event`: `update`, `end` or `start`.
- `environment`: `production` (default, `api.push.apple.com`) or `sandbox` (`api.sandbox.push.apple.com`).
- `timestamp`: unix seconds, within the last hour or the next 5 minutes.
- `contentState`: a JSON object, required for `update` and `start`, optional for `end`. The relay checks that it
  is an object, not its fields, and passes it on as `content-state`.
- `staleDate`: optional, unix seconds from one minute ago to 12 hours ahead.
- `dismissalDate`: `end` only, unix seconds from one minute ago to 4 hours ahead. The relay refuses it on
  `update` and `start` with 400 (`dismissalDate is for end only.`).
- `priority`: 5 or 10 (default 10).

Any other field is refused. Nothing in a request chooses the topic
(`com.maddada.ghostex.ios.push-type.liveactivity`), the push type (`liveactivity`) or the host. A start also
carries `attributes-type` `GhostexActivityAttributes`, empty `attributes` and the fixed alert "Ghostex" / "Your
agents are working.", all set by the relay. Every Live Activity push goes with `apns-expiration` one hour after
the relay received it.

## Notifications and background pushes: `POST /push/v1/alert`

`Content-Type: application/json`, at most 16 KB:

```json
{
  "token": "<the app's device token; hex>",
  "environment": "production",
  "kind": "finished",
  "envelope": "<standard base64>",
  "threadId": "<machineId>:<sessionId>",
  "data": { "kind": "finished", "machineId": "<id>", "sessionId": "<id>", "notificationId": "<id>", "worker": true },
  "sound": true,
  "timestamp": 1794355519
}
```

- `token`, `environment`: as for a Live Activity push. `timestamp`: the event's time, in the same window (the
  number above is an example).
- `kind`: an event of the table below, or `background`.
- `envelope`: optional. Standard base64 (with padding), at most 3072 characters (413 `too-large` above that),
  decoding to the version byte `1`, a 12-byte nonce and at least 16 more bytes. It goes on unchanged as the
  payload's top-level `envelope`. The relay never decrypts it and cannot tell whether it is a ciphertext;
  encrypting it is the sending computer's job.
- `threadId`: optional, `<machineId>:<sessionId>`, split at the first `:`, each part 1 to 64 printable ASCII
  characters without spaces. Sent as `aps.thread-id`.
- `data`: optional, an object with exactly the keys `kind`, `machineId` and `sessionId` (each 1 to 64 printable
  ASCII characters without spaces), optional `notificationId` (1 to 128 such characters) and optional `worker`
  (`true` or `false`). Any other key makes the whole request 400: gxserver's Expo push `data` can also carry
  `openSessionId` and `url` (`server/src/push/message.rs`), and the relay has no field for either, so a sender
  coming from Expo leaves them out. `data.kind` is checked only for its form, not against the table. Sent as the
  payload's top-level `data`.
- `sound`: optional; `true` adds `"sound":"default"`, `false` adds nothing.

The alert's title is always `Ghostex` and its body the event's fixed words; no field puts other text into them.

| `kind` | Words |
| --- | --- |
| `finished` | Finished |
| `approval` | Waiting for approval |
| `waiting` | Waiting for you |
| `question` | Has a question |
| `failed` | Ended with an error |
| `security` | Security: check this was you |
| `stuck` | Not moving: it may be stuck |
| `blocked` | Blocked: out of usage on every account |
| `threadReport` | Reported to its coordinator |
| `outOfMemory` | Stopped to free memory |

An alert goes to Apple as push type `alert`, topic `com.maddada.ghostex.ios`, priority 10, `apns-expiration`
the event's time plus one hour, payload
`{"aps":{"alert":{"title":"Ghostex","body":"Finished"},"mutable-content":1,"thread-id":"…","sound":"default"},"envelope":"…","data":{…}}`
(`thread-id`, `envelope` and `data` only when given, `sound` only when `true`).

`kind: "background"` sends push type `background`, the same topic, priority 5, `apns-expiration` the event's
time plus 10 minutes, payload `{"aps":{"content-available":1}}`. It takes no `envelope`, `threadId`, `data` or
`sound`.

## Widget pushes: `POST /push/v1/widgets`

`{"token":"<the widget extension's push token; hex>","environment":"production"}` (`environment` optional, no
other field). It goes to Apple as push type `widgets`, topic `com.maddada.ghostex.ios.push-type.widgets`,
priority 5, `apns-expiration` one hour after the relay received it, payload `{"aps":{"content-changed":true}}`.
No snapshot content goes into the push.

## Answers of the three push routes

JSON, `ok` true or false. The relay checks in this order and stops at the first refusal: the caller's address
quota, the key, the body and its fields, a token remembered as gone, the token's quota, the relay-wide quota, a
free slot to Apple; then it asks Apple.

| Status | `error` | Meaning |
| --- | --- | --- |
| 200 | | Apple answered 200 (`apnsId`). Whether a phone shows the push is checked on the phone. |
| 400 | `bad-request` | Not JSON, or a field missing, unknown or out of its form or range (`detail` says which). |
| 410 | `token-gone` | Apple answered 410, `BadDeviceToken`, `Unregistered` or `DeviceTokenNotForTopic` (`appleStatus`, `appleReason`); or, without those two fields, the relay still remembers such an answer for this token (see Lifetimes). Drop the token. |
| 413 | `too-large` | The payload for Apple over 4096 bytes, or an envelope over 3072 characters. |
| 413 | (not JSON) | The body is over Caddy's `max_size` (8KB on `/live-activity/`, 16KB on `/push/`) or the relay's (8 KiB, 16 KiB); refused before the route's code runs. |
| 429 | `rate-limited` | One of the relay's limits, or no free slot to Apple within 5 seconds: `retryAfter` seconds and a `Retry-After` header. |
| 429 | `apple` | Apple answered 429 (`appleStatus`, `appleReason`; Apple's documentation lists the reasons). No `retryAfter`. |
| 502 | `apple` | Apple answered another status (`appleStatus`, `appleReason`). |
| 502 | `apple-unreachable` | No answer from Apple within 15 seconds, no connection, or another failure of the request. |
| 503 | `not-configured` | The relay has no key. |

## Token mailbox: `/push/v1/mailbox`

The id goes in `Authorization: Bearer <id>`: 64 hex characters, either case. The relay reads it only from that
header, never from the URL, files the entry under the SHA-256 of the lowercased id and keeps no id itself.

- `PUT`: the body is a token in hex, 64 to 512 characters, whitespace around it trimmed; the body is at most
  1024 bytes. Stored lowercased; it replaces what the id held and starts its 10 minutes again. 200 `{"ok":true}`.
- `GET`: 200 `{"ok":true,"token":"<lowercase hex>"}` with `Cache-Control: no-store`, and the entry is removed;
  404 `empty` when the id holds nothing.
- `HEAD`: 405 with `Allow: GET, PUT`; the entry stays.

## Widget snapshot: `/push/v1/snapshot`

The id as for the mailbox.

- `PUT`: the body is any bytes, 1 to 8192, stored unchanged: the relay neither decrypts nor checks them (the
  computer is meant to encrypt them for the phone). It replaces what the id held and starts its 24 hours again.
  200 `{"ok":true}`.
- `GET`: 200 with the bytes as stored, `Content-Type: application/octet-stream`, `Cache-Control: no-store`; the
  snapshot stays. 404 `empty` when the id holds nothing.

Answers of both store routes (JSON unless a snapshot comes back). They need no Apple key.

| Status | `error` | Meaning |
| --- | --- | --- |
| 200 | | Stored, or here it is. |
| 400 | `bad-request` | No `Authorization: Bearer` with 64 hex characters, a mailbox body that is not a token in hex, or an empty snapshot. |
| 404 | `empty` | Nothing is held under this id (never stored, taken, expired, or lost in a restart). |
| 405 | | `HEAD` on the mailbox. |
| 413 | `too-large` | A mailbox body over 1024 bytes or a snapshot over 8192. |
| 413 | (not JSON) | A body over Caddy's 16KB or the relay's 16 KiB, refused before the route's code runs. |
| 429 | `rate-limited` | The address's or the id's quota, or a full limit table: `retryAfter` and `Retry-After`. |
| 429 | `too-many-new-ids` | This address has spent its quota of new ids: `retryAfter` and `Retry-After`. Nothing was stored. |
| 503 | `store-full` | The entry does not fit the store's caps: `retryAfter` 60. Nothing was stored. |

## Health checks

`GET /live-activity/healthz` answers `{"ok":true,"configured":true|false,"sent":N,"refused":N,"rateLimited":N,"appleErrors":N,"goneTokens":N}`
for the Live Activity route; `GET /push/healthz` the same for the `/push/` routes plus `stored`, `storeFull`,
`mailboxEntries`, `snapshots` and `storeBytes`. The counts start at zero when the process starts. `configured`
means the key file and `apns.conf` were read and parsed and the HTTP client was built, not that Apple accepts
the key.

## Limits

Each limit is a token bucket with a burst and a refill: a request takes one token, and an empty bucket answers
429 with how long until a token is free.

| Bucket | Burst | Refill | Spent by |
| --- | --- | --- | --- |
| Live Activity, per caller address | 60 | 60 a minute | every request to `/live-activity/v1/send` |
| Live Activity, per token (update, end) | 10 | 10 a minute | a request that passed the address, key, form and gone checks |
| Live Activity, per push-to-start token | 2 | 1 per 10 minutes | the same, for `start` |
| Live Activity, whole relay | 300 | 3000 a minute | a request that also passed its token's quota |
| `/push/` alert and widgets, per caller address | 60 | 60 a minute | every request to those two routes |
| `/push/`, per device token, alerts | 10 | 10 a minute | as on the Live Activity route |
| `/push/`, per device token, background | 3 | 3 an hour | as on the Live Activity route |
| `/push/`, per widget push token | 4 | 6 an hour | as on the Live Activity route |
| `/push/`, whole relay | 300 | 3000 a minute | as on the Live Activity route; a bucket of its own |
| Store, per caller address | 60 | 60 a minute | every request to the mailbox and the snapshot, except the mailbox's `HEAD` |
| Store, new ids per caller address | 20 | 20 an hour | a `PUT` for an id its shelf (mailbox or snapshot store) does not hold |
| Store, per mailbox id and per snapshot id | 20 | 20 a minute | a request for an id the store holds or is admitting |

- The caller's address is, with `--behind-proxy` (as the unit starts it), the last `X-Forwarded-For` entry that
  parses as an address, otherwise the connection's address. An IPv6 address counts by its /64, an IPv4 address
  mapped into IPv6 as that IPv4 address. A body refused for its size before the route's code runs, and the
  health checks, spend nothing.
- A relay-wide bucket is spent before the wait for a slot and before Apple answers: a request that then gets
  429 for want of a slot, or that Apple refuses, has spent it. Well-formed requests with tokens Apple refuses
  can empty it, and real pushes then get 429 until it refills. A request the relay refuses earlier does not
  spend it. The two relay-wide buckets are separate, so `/push/` traffic never takes from the Live Activity
  route's.
- Requests to Apple: at most 64 at once for the Live Activity route and 64 for the `/push/` routes; a request
  waits up to 5 seconds for a slot, else 429 `rate-limited` with `retryAfter` 5. Each request to Apple times out
  after 15 seconds. When Apple answers 403 `ExpiredProviderToken` or `InvalidProviderToken` and the relay's
  provider token is at least 20 minutes old, the relay signs a new one and sends the push once more, so one
  request can reach Apple twice.
- New ids: charged for a `PUT` on an id its shelf does not hold, after the address quota and the id, size and
  body checks and before the capacity check, so a `store-full` refusal has spent it. An id held in the mailbox
  is new to the snapshot store, and the other way round. Replacing what an id holds, and reading, do not count.
- Per id: a held id is charged on every read and on a `PUT` before the capacity check; a new id only once its
  address's quota and the capacity admitted it. Reads of an id the store does not hold, and refused new ids,
  are not counted per id.
- Store caps: 5,000 entries and 5 MiB (5,242,880 bytes) of tokens and snapshots, mailbox and snapshots
  together. A `PUT` that would pass either cap, even after expired entries are dropped, gets `store-full`; a
  smaller entry can still fit where a larger one does not. Nothing held is pushed out for something new. Space
  that a mailbox `GET`, an expiry or a smaller replacement frees is available at once.
- Table sizes: each of the Live Activity route's tables holds at most 200,000 keys, each of the `/push/` routes'
  eight per-address, per-token and per-id tables 7,000. The cleanup once a minute forgets a key idle for 10
  minutes on the Live Activity route; on the `/push/` routes, once its bucket would have refilled completely
  and never under 10 minutes: 10 minutes for addresses, alert tokens and ids, 40 for widget tokens, 60 for
  background tokens and new ids. A full table first forgets its idle keys, then refuses a new key with 429
  (`retryAfter` 60); keys already in it keep their buckets.

## Lifetimes

- A mailbox entry lives 10 minutes from its last `PUT`, until the first `GET` takes it.
- A snapshot lives 24 hours from its last `PUT`.
- An expired entry is never handed out (each access checks it); its memory is freed at that access, at the
  cleanup once a minute, or when a `PUT` needs the space.
- After Apple's gone answer (410 row above), the relay refuses that token without asking Apple, per environment
  (and on the `/push/` routes per topic), until the cleanup once a minute finds the answer 24 hours old. At most
  100,000 such tokens are remembered for the Live Activity route and 7,000 for the `/push/` routes, apart from
  each other; when the memory is full, expired ones and then the oldest are forgotten for a new one. So the
  refusal can end sooner: after eviction or a restart.
- The provider token (the signed JWT) is reused for 40 minutes, and renewed sooner only as described under
  Limits.
- A restart, or any end of the process, empties all of it: mailbox entries, snapshots, remembered gone tokens,
  every bucket and the health counts. Nothing is written to disk.

## Who can call it, and what that allows

The relay does not authenticate its callers.

- Push routes: whoever has a device token can make the relay push to it, within the limits above. The route
  fixes the topic and the push type; the sender chooses a Live Activity's content and event, an alert's event
  (the words stay the relay's) with its envelope, `threadId` and `data`, and when background and widget pushes
  go. Without a working token, well-formed requests can still spend a relay-wide bucket (see Limits).
- Mailbox and snapshot store: open within limits. The id in the `Authorization` header is the only
  capability, and the relay cannot tell a paired phone or computer from anyone else: it takes any 64 hex
  characters. What bounds the stores is the shared caps, the per-address quota on new ids, the address and id
  quotas and the short lifetimes.
- Whoever holds a mailbox id can take that entry: a token in plain hex (meant to be the activity's update
  token), not encrypted, and with it they can update or end that Live Activity through the Live Activity route,
  within its limits. They can also replace the entry.
- Whoever holds a snapshot id can read and replace that snapshot. The relay stores the bytes unchanged and does
  not check that they are encrypted; their confidentiality rests on the computer encrypting them before the
  upload.
- Strangers can fill the stores with ids of their own up to the caps. New ids then get `store-full` for
  everyone until space frees, while ids already held keep working. Many addresses can likewise fill a limit
  table: new keys then get 429 until idle ones are forgotten.
- The key: read at start from `AuthKey.p8` and `apns.conf`, which the deploy steps make readable only to root
  and the service's group. No answer and no log line carries the key or its two ids. What goes to Apple for a
  push: the token (in the request path), push type, topic, priority, expiry, the signed provider token and the
  payload.

### Memory

The unit runs the relay with `MemoryMax=128M`. The limits are fixed in the code; the `/push/` caps were sized
against a local macOS measurement with every table and the store full, without HTTP. Memory under load is an
open availability risk: a `POST` or `PUT` body is read in full (up to 8 KiB on `/live-activity/`, 16 KiB on
`/push/`) before the address quotas and the id checks run, and there is no overall budget for bodies being
read, so many parallel requests add memory on top of full tables and a full store. Whether that reaches the
limit on Linux is not measured. If the kernel kills the relay because it cannot stay under `MemoryMax`, the
unit restarts it (`Restart=always`, `RestartSec=2`, within systemd's start limits), and everything listed
under Lifetimes is lost.

## Logs

The relay writes one line per event to stdout (the journal under systemd): at start
`configured: the key loaded; target=apple` or `not configured: <reason>; …` (the reason says what is missing or
wrong, never a value from the key's files), then `listening on <address>`; each push Apple answered (`sent`,
`push sent`: event or kind, environment, token label, Apple's status and reason, milliseconds) and each that
got no answer (`failed`, `push failed`); each refusal in a route's own code, with `reason=` `rate-limited` (and
`scope=` `address`, `token`, `global` or `id`), `busy`, `not-configured`, `bad-request`, `too-large`,
`token-gone`, `too-many-new-ids` or `store-full`; each store (`mailbox stored`, `snapshot stored`, with id label
and bytes) and each mailbox take (`mailbox taken`).

No line is written for a health check, a snapshot read, a read of an id that holds nothing, the mailbox's
`HEAD`, or a body refused for its size before the route's code. A label is the first 8 hex characters of the
SHA-256 of the token, or of the lowercased id. No line carries a token, an id, a content state, an envelope,
`threadId`, `data`, a snapshot or anything from the key's files. Whether Caddy's access log keeps the
`Authorization` header depends on Caddy's logging configuration.

## Run it locally

```bash
cd server/push-relay && cargo run -- --listen 127.0.0.1:58810 --key /path/AuthKey.p8 --config /path/apns.conf
```

`--apple-stand-in http://127.0.0.1:<port>` sends to a stand-in for Apple over HTTP/2 without TLS
(`<origin>/production/3/device/<token>` or `/sandbox/…`); any address other than loopback is refused.

## Build for Linux

`server/push-relay/build-linux.sh [x64|arm64|all]` builds static musl binaries from macOS or Linux (Zig
compiles `ring`'s C code; `ZIG` picks the binary) and writes
`dist/ghostex-push-relay-<version>-linux-<arch>.tar.gz` (binary, systemd unit, Caddy block,
`apns.conf.example`, this README) with a `.sha256` beside it.

## Deploy (relay-eu.ghostex.dev, beside the share relay)

1. `useradd --system --user-group --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin ghostex-push-relay`
2. `install -d -o root -g root -m 0755 /opt/ghostex-push-relay` and the binary to
   `/opt/ghostex-push-relay/ghostex-push-relay` (root, 0755).
3. `install -d -o root -g ghostex-push-relay -m 0750 /etc/ghostex-push-relay`.
4. `cp ghostex-push-relay.service /etc/systemd/system/ && systemctl daemon-reload && systemctl enable --now ghostex-push-relay`.
   The unit starts the relay on `127.0.0.1:58810` with `--behind-proxy` as user `ghostex-push-relay`.
5. Add both `handle` blocks of `Caddyfile` (`/live-activity/*` with `max_size 8KB`, `/push/*` with
   `max_size 16KB`, both to `127.0.0.1:58810`) inside the `share.ghostex.dev` site in `/etc/caddy/Caddyfile`,
   above its `reverse_proxy` line. Check the whole file with
   `caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile`, then `systemctl reload caddy` while no
   shared chat is connected (check `ss -tn state established '( sport = :58800 )'`).
6. Check: `curl -s https://share.ghostex.dev/live-activity/healthz` and
   `curl -s https://share.ghostex.dev/push/healthz`.

The key: copy `AuthKey.p8` to `/etc/ghostex-push-relay/AuthKey.p8` and write `KEY_ID=` and `TEAM_ID=` to
`/etc/ghostex-push-relay/apns.conf`, both `root:ghostex-push-relay` mode 0640, then
`systemctl restart ghostex-push-relay`. Without them the relay starts, its health checks say
`"configured":false`, pushes get 503 and the store routes work. Three checks, in this order:

1. `journalctl -u ghostex-push-relay` says `configured: the key loaded; target=apple` and both health checks say
   `"configured":true`: the files were read.
2. A push through each route, in each environment the phones use, shows whether Apple accepts the key for that
   topic: 200 with `apnsId`, or Apple's refusal with `appleStatus` and `appleReason`.
3. Whether the phone shows it is checked on the phone.

Updating: replace the binary (and the unit, then `systemctl daemon-reload`) and `systemctl restart
ghostex-push-relay`, then check both health routes. Between the stop and the start the relay accepts no
connections, so requests in that gap fail (what Caddy answers them is not checked here); after the start,
everything listed under Lifetimes starts empty. To add the `/push/` routes to a relay that runs only the Live
Activity route, also add the `handle /push/*` block (step 5) and reload Caddy.

Raising a limit: the limits are fixed in the code, not read from a setting. The rates and the `/push/` caps are
in `LimitConfig::default()` in `src/limits.rs`, the Live Activity route's 200,000 keys per table in `MAX_KEYS`
beside it, and the body sizes in `src/lib.rs` and `src/push_routes.rs` (those also have a Caddy `max_size`,
step 5). Change it there, measure the relay's memory again with every table and the store full, raise
`MemoryMax` in `deploy/ghostex-push-relay.service` with room above that measurement, rebuild with
`build-linux.sh`, and deploy the new binary and unit (`systemctl daemon-reload`, then restart).
