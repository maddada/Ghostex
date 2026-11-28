# ghostex-push-relay

Ghostex's push relay, at `https://share.ghostex.dev/live-activity/` and `https://share.ghostex.dev/push/`. While
the iPhone is locked, iOS keeps the Ghostex app suspended, so the app cannot update its Lock Screen banner (the
Live Activity) itself; only a push to the activity can. Those pushes go through Apple's push service with the
Apple push key (`.p8`). Expo's push service, which carries Ghostex's notifications, does not carry Live
Activity pushes, and the key cannot go on users' computers. So a user's computer (gxserver) asks this
relay, which holds the key, to send the push.

The `/push/` routes let a computer send the app's notifications, background pushes and widget pushes the same
way, straight to Apple instead of through Expo, and hold two small things in memory for the phone: a mailbox
for a Live Activity's new update token, and one encrypted widget snapshot per id.

It lives beside `server/share-relay` because it is the same kind of thing: a small standalone service on
the same box (relay-eu.ghostex.dev), behind the same Caddy, built the same way into one static binary. It
is its own crate (own `Cargo.lock`), so gxserver's build never compiles it.

## Live Activity request

`POST /live-activity/v1/send`, `Content-Type: application/json`, at most 8 KB:

```json
{
  "token": "<the activity's push token, or the app's push-to-start token for a start; hex>",
  "event": "update",
  "environment": "production",
  "timestamp": 1794355519,
  "contentState": { "updatedAt": "…", "counts": { "working": 1, "waiting": 0 }, "agents": [], "moreCount": 0 },
  "staleDate": 1794356419,
  "dismissalDate": 1794356119,
  "priority": 10
}
```

- `event`: `update`, `end` or `start` (start: iOS 17.2 and later, with the push-to-start token).
- `environment`: `production` (default) or `sandbox` (Apple's development service, for development builds).
- `timestamp`: unix seconds, now (within the last hour or the next 5 minutes); Apple drops an update older
  than one it delivered.
- `contentState`: required for `update` and `start`, optional for `end`. The JSON the app's
  `GhostexActivityContent` decodes (`apps/mobile/app/modules/ghostex-live-activity/ios/Shared`).
- `staleDate`: optional, unix seconds within the next 12 hours. `dismissalDate`: `end` only, within 4 hours.
- `priority`: 5 or 10 (default 10).

Any other field is refused. In particular nothing in a request chooses Apple's topic
(`com.maddada.ghostex.ios.push-type.liveactivity`), the push type (`liveactivity`) or the host
(`api.push.apple.com`, or `api.sandbox.push.apple.com` for `sandbox`). A start carries the attributes type
`GhostexActivityAttributes`, empty attributes and a fixed alert ("Ghostex", "Your agents are working."),
all set here.

Answers (JSON, `ok` true or false):

| Status | `error` | Meaning |
| --- | --- | --- |
| 200 | | Apple took it (`apnsId`). |
| 400 | `bad-request` | Not a Live Activity push this relay sends (`detail` says why). |
| 410 | `token-gone` | Apple no longer takes this token (unregistered, bad, or not for this app); drop it. Refused here for 24 hours without asking Apple. |
| 413 | `too-large` | Over 8 KB, or Apple's payload over 4 KB. |
| 429 | `rate-limited` | `retryAfter` seconds (also `Retry-After`). Also when Apple answers 429 for the device. |
| 502 | `apple`, `apple-unreachable` | Apple refused it otherwise (`appleStatus`, `appleReason`) or did not answer. |
| 503 | `not-configured` | The relay has no key yet. |

`GET /live-activity/healthz` answers `{"ok":true,"configured":true|false,"sent":N,"refused":N,"rateLimited":N,"appleErrors":N,"goneTokens":N}`.

## Notifications: `POST /push/v1/alert`

`Content-Type: application/json`, at most 16 KB:

```json
{
  "token": "<the app's device token; hex>",
  "environment": "production",
  "kind": "finished",
  "envelope": "<standard base64: version 1, nonce, ciphertext; encrypted on the computer for the phone>",
  "threadId": "<machineId>:<sessionId>",
  "data": { "kind": "finished", "machineId": "<id>", "sessionId": "<id>", "notificationId": "<id>", "worker": true },
  "sound": true,
  "timestamp": 1794355519
}
```

- `kind`: the event, one of the table below, or `background`.
- `environment`, `timestamp`: as for a Live Activity push (`timestamp` is the event's time).
- `envelope`: optional. Standard base64 (with padding), at most 3072 characters, that decodes to the format
  version byte `1`, a 12-byte nonce and a ciphertext of at least its 16-byte tag. It goes on as the payload's
  top-level `envelope` key; the phone's notification service extension decrypts it and replaces the title and
  body. Without it, or when the phone cannot decrypt it, the phone shows the fixed words.
- `threadId`: optional, `<machineId>:<sessionId>`, each part 1 to 64 printable ASCII characters without spaces
  (split at the first `:`). iOS stacks the alerts of one thread together.
- `data`: optional, exactly `kind`, `machineId` and `sessionId` (each 1 to 64 printable ASCII characters without
  spaces), optional `notificationId` (up to 128) and optional `worker` (true or false); any other key is refused.
  It goes on as the payload's top-level `data` key, the same object today's Expo push carries, so the phone opens
  the right session on a tap with `pushEventFromData(data)` (`src/notifications/pushMessage.ts` in the phone app)
  whether or not the notification service extension ran.
- `sound`: optional; `true` plays the default sound. No caller picks a sound file.

What the phone shows in the clear is the relay's own: the title `Ghostex` and the event's fixed words, the
ones gxserver's alerts use (`server/src/push/message.rs`). No request can put other words on a lock screen.

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

What the relay enforces and what it cannot: it enforces the fixed title and words, the form and size of
`threadId`, `data` and `envelope` above, and that nothing else goes to Apple. It cannot tell whether the
envelope's bytes really are a ciphertext, or whether an identifier is an id rather than a name written without
spaces: only the computer that sends (encrypting before it sends, putting only opaque ids in `threadId` and
`data`) and the phone that receives can guarantee that.

An alert goes to Apple as push type `alert`, topic `com.maddada.ghostex.ios`, priority 10, with
`"mutable-content": 1` (so the notification service extension runs) and an expiry of the event's time plus
one hour:
`{"aps":{"alert":{"title":"Ghostex","body":"Finished"},"mutable-content":1,"thread-id":"…","sound":"default"},"envelope":"…","data":{…}}`.

`kind: "background"` sends a background push for the app's opportunistic refresh instead: push type
`background`, same topic, priority 5 (the only one Apple takes for it), `{"aps":{"content-available":1}}`, an
expiry of the event's time plus 10 minutes. It takes no `envelope`, `threadId`, `data` or `sound`.

Any other field, or any other `kind`, is refused.

## Widget pushes: `POST /push/v1/widgets`

`{"token":"<the widget extension's push token; hex>","environment":"production"}` (environment optional). It
goes to Apple as push type `widgets`, topic `com.maddada.ghostex.ios.push-type.widgets`, priority 5, expiry one
hour, payload `{"aps":{"content-changed":true}}`: WidgetKit reloads the widgets when its budget allows, and
they fetch the snapshot. Any other field is refused.

Answers of both push routes (JSON, `ok` true or false):

| Status | `error` | Meaning |
| --- | --- | --- |
| 200 | | Apple took it (`apnsId`). |
| 400 | `bad-request` | Not a push this relay sends (`detail` says why). |
| 410 | `token-gone` | Apple no longer takes this token for this topic; drop it. Refused here for 24 hours without asking Apple (remembered apart from the Live Activity route). |
| 413 | `too-large` | An envelope over 3072 characters, or Apple's payload over 4 KB. |
| 413 | (not JSON) | The body is over the size Caddy (16 KB) or the relay (16 KiB) reads at all; the proxy or the server answers before the relay's code runs. |
| 429 | `rate-limited` | The relay's own limit: `retryAfter` seconds and a `Retry-After` header. |
| 429 | `apple` | Apple's limit for this device (`appleStatus` 429, `appleReason`, e.g. `TooManyRequests`). No `retryAfter`: back off on the caller's side. |
| 502 | `apple`, `apple-unreachable` | Apple refused it otherwise (`appleStatus`, `appleReason`, e.g. `TopicDisallowed` when the key is not allowed this topic) or did not answer. |
| 503 | `not-configured` | The relay has no key yet. |

## Token mailbox: `PUT` and `GET /push/v1/mailbox`

When a push starts a Live Activity, only the woken phone learns the activity's update token. It leaves the token
here and the computer collects it.

- The id: `Authorization: Bearer <id>`, 256 bits in hex (64 characters), which the phone and the computer derive
  from their pairing key. It is a capability, so it never goes in the URL, where a proxy's logs would keep it;
  Caddy leaves the `Authorization` header out of its logs. The relay files the entry under the id's SHA-256 and
  never holds or logs the id itself.
- `PUT`: the body is the update token in hex (64 to 512 characters, at most 1 KB with whitespace around
  it). It replaces what the id holds. 200 `{"ok":true}`.
- `GET`: 200 `{"ok":true,"token":"<hex>"}`, and the entry is gone; 404 `empty` when the id holds nothing.
- `HEAD`: 405 with only the status and an `Allow: GET, PUT` header, no body; the entry stays: only a `GET`
  takes it.
- An entry lives at most 10 minutes.

## Widget snapshot: `PUT` and `GET /push/v1/snapshot`

While the app is closed, the computer leaves the widgets' data here, encrypted, and the widget extension
fetches it when it reloads.

- The id: `Authorization: Bearer <id>`, as for the mailbox.
- `PUT`: the body is the encrypted snapshot, any bytes, 1 byte to 8 KiB. It replaces what the id holds and
  starts its 24 hours again. 200 `{"ok":true}`.
- `GET`: 200 with the bytes as stored (`application/octet-stream`); 404 `empty` when the id holds nothing.
- A snapshot is kept until it is replaced or for 24 hours.

Answers of both store routes (JSON unless a snapshot comes back):

| Status | `error` | Meaning |
| --- | --- | --- |
| 200 | | Stored, or here it is. |
| 400 | `bad-request` | No `Authorization: Bearer` with 64 hex characters, the mailbox body is not a token in hex, or the snapshot is empty. |
| 404 | `empty` | Nothing is held under this id (never stored, taken, or expired). |
| 405 | (no body) | `HEAD` on the mailbox: only the status and `Allow: GET, PUT`. |
| 413 | `too-large` | A mailbox body over 1 KB or a snapshot over 8 KiB. |
| 413 | (not JSON) | A body over 16 KB, refused by Caddy or the server before the relay's code runs. |
| 429 | `rate-limited`, `too-many-new-ids` | `retryAfter` seconds (also `Retry-After`); `too-many-new-ids` when this address created its quota of new ids. Nothing was stored. |
| 503 | `store-full` | The relay holds as much as it may; `retryAfter` 60. Nothing was stored. |

They need no Apple key; they answer while the relay is not configured.

`GET /push/healthz` answers `{"ok":true,"configured":true|false,"sent":N,"refused":N,"rateLimited":N,"appleErrors":N,"stored":N,"storeFull":N,"mailboxEntries":N,"snapshots":N,"storeBytes":N,"goneTokens":N}`
(counts of the `/push/` routes only; the Live Activity route's health check counts its own).

## Limits, and why these numbers

- Per device token: 10 pushes, refilled 10 a minute. A banner changes when an agent starts, waits,
  finishes or stops; gxserver coalesces those, so a phone needs a few a minute at most. Apple budgets
  pushes per activity and throttles a device that gets too many; 10 a minute keeps a broken computer
  inside that.
- Per push-to-start token: 2, then one every 10 minutes. A start is needed only when no banner runs; Apple
  limits starts tightly.
- Per caller address (the last `X-Forwarded-For` entry, which Caddy sets; an IPv6 address counts by its /64,
  since anyone given a /64 has more addresses than a limit could count one by one): 60, refilled 60 a minute,
  for every request, refused ones included. Enough for a home with several computers and phones, and it stops
  one address from probing tokens quickly.
- Whole relay: 300, refilled 3000 a minute (50 a second), and 64 requests to Apple in flight. Only a request
  that is about to go to Apple spends it, so junk from many addresses cannot use up what real pushes need. Apple takes
  far more than that from one key; the cap is what keeps a flood from many addresses from reaching Apple
  as a flood, so Apple never has a reason to throttle the key itself.
- A token Apple called gone (410, `BadDeviceToken`, `Unregistered`, `DeviceTokenNotForTopic`) is refused
  here for 24 hours. At most 100,000 are remembered (the oldest goes first), so random tokens cannot grow the
  relay's memory without end; the health check reports the count (`goneTokens`).
- One provider token (the signed JWT) is reused for 40 minutes: Apple refuses one older than an hour and
  answers `TooManyProviderTokenUpdates` to one renewed more often than every 20 minutes.

The `/push/` routes have limits of their own, so their traffic never takes from the Live Activity route's:

- Per caller address: 60, refilled 60 a minute, for every push request; and separately 60, refilled 60 a
  minute, for every mailbox and snapshot request. Counted as above (last `X-Forwarded-For` entry, IPv6 by /64).
- Per device token, alerts: 10, refilled 10 a minute. gxserver sends one phone at most 8 alerts a minute
  (`server/src/push/limiter.rs`); 10 leaves room for that and stops a broken computer.
- Per device token, background pushes: 3, refilled 3 an hour. Apple advises sending no more than two or three
  an hour and holds back the rest itself.
- Per widget push token: 4, refilled 6 an hour. WidgetKit budgets the reloads a push asks for; the computer
  coalesces changes, and more than one push every 10 minutes would only spend the widgets' budget.
- Whole relay: 300, refilled 3000 a minute, and 64 requests to Apple in flight, apart from the Live Activity
  route's same numbers. Even both at once (6000 a minute) stay far below what Apple takes from one key.
- Per mailbox id and per snapshot id: 20, refilled 20 a minute. A computer polls the mailbox for a few
  minutes after a start and a widget fetches its snapshot when it reloads; neither needs more. Only an id the
  store holds, or one it has just admitted, is counted, so reads of unknown ids and refused new ids leave no
  state here.
- New ids per caller address: 20, refilled 20 an hour, for the mailbox and the snapshot store together,
  checked before anything is stored or counted for the id. Replacing what an id already holds, and reading, do
  not count. A phone leaves a mailbox entry after a pushed start and a computer keeps one snapshot per paired
  phone, so a home with several phones and computers needs a few an hour. Like every `/push/` bucket, an idle
  one is forgotten only once it would have refilled completely (here after an hour), so a pause never hands
  out a fresh quota early.
- What the mailbox and the snapshot store hold together: at most 5,000 entries and 5 MiB of tokens and
  snapshots. Assuming a widget snapshot of about 1 to 2 KiB (the phone's format is not fixed yet), 5 MiB holds
  the snapshots of a few thousand phones, and mailbox entries live only minutes. Before a new id is refused as `store-full`,
  expired entries are dropped; an id that already holds something can still be read and replaced (a
  replacement only when it does not grow the store past 5 MiB). Nothing old is pushed out for something new,
  so a flood cannot evict real snapshots. The body of a refused request is never copied into the store.
- A token Apple called gone on a `/push/` route is remembered for 24 hours for that environment and topic, at
  most 7,000 (the oldest goes first), apart from the Live Activity route's 100,000: the two never push each
  other's entries out or show in each other's health check.
- Each of the `/push/` routes' eight limit tables (per address, per token, per id, new ids per address) holds
  at most 7,000 keys. A key stays in its table from its last attempt until its quota would have refilled
  completely, and never less than 10 minutes: 10 minutes for alert tokens, mailbox and snapshot ids and the
  push and store address tables, 40 minutes for widget tokens, and 60 minutes for background tokens and new
  ids per address. So size by the keys seen within those times, not per minute. A table full of such keys
  refuses a new key with 429 rather than growing; when Ghostex has more phones than that active at once,
  these numbers and the memory below have to grow together.
- Memory: the service runs with `MemoryMax=128M`. The `/push/` caps above (5,000 entries, 5 MiB, 7,000 keys
  per table, 7,000 gone tokens) were chosen to fit beside the Live Activity route's tables (200,000 keys each,
  100,000 gone tokens), which take most of the memory. What is measured is local only: a release build on
  macOS, with the real limiter and store types filled to every cap in one process, held about 92 to 97 MiB
  resident over several runs, with no HTTP, TLS or connections. It was not measured on Linux or under the
  service's 128 MiB limit, so the room left there is not known. Every body is capped per request (8 KB on
  `/live-activity/`, 16 KB on `/push/`, through Caddy), but there is no overall budget for bodies still being
  read, so many large requests at once add to that. Only a flood from very many addresses fills every table
  at once. If memory reaches the limit, systemd restarts the relay, which drops every mailbox entry and
  snapshot; phones recover with their next update (the phone leaves a token again on its next start, the
  computer uploads the snapshot again on its next change).
- Raising a limit: these limits are fixed in the code, not read from a setting. The rates and the
  `/push/` caps are in `LimitConfig::default()` in `src/limits.rs`, the Live Activity route's 200,000 keys per
  table in `MAX_KEYS` beside it, and the body sizes in `src/lib.rs` and `src/push_routes.rs` (those also have
  a Caddy limit, step 5 of the deploy). To raise one: change it there, measure the relay's memory again with
  every table and the store full, raise `MemoryMax` in `deploy/ghostex-push-relay.service` with room above
  that measurement, rebuild with `build-linux.sh`, and deploy the new binary and unit (`systemctl
  daemon-reload`, then restart).

## Who can call it, and what an attacker can do

It is open, the way Expo's push service is: the device token is what lets anyone push to a phone. A
Live Activity token belongs to one activity on one phone; a push-to-start token to one app install. They
are 32 bytes or more of Apple's randomness, cannot be guessed, and go only to the computers that phone
paired with (the phone's push registration, over the paired connection).

- Without a token, an attacker reaches no phone. Random tokens get `BadDeviceToken` from Apple and the
  per-address and relay-wide limits stop them from making that a flood. The worst they can do is spend the
  relay-wide budget, so real pushes get 429 for a while and banners go stale until it refills: a denial of
  this feature, not of anything else on the box, and not of the key.
- With one phone's token (from that user's computer, for example), an attacker can update, end or start
  that one phone's Ghostex banner with content of their choosing, at most 10 times a minute (starts 2, then
  one per 10 minutes). The banner shows agent and project names and counts; the alert of a start is fixed.
  They cannot send notifications, reach other apps (the topic is fixed), or touch other phones.
- With one phone's device token, an attacker can send that phone alerts, at most 10 a minute, but only with
  the fixed title and an event's fixed words; an envelope they make cannot be decrypted by the phone, which
  then shows the fixed words. Background pushes are limited to 3 an hour, widget pushes to 6 an hour.
- The relay cannot tell a paired phone or computer from anyone else: it never learns the pairing key, so
  any well-formed id is accepted. Without an id, an attacker cannot read or replace anyone's mailbox entry
  or snapshot (256 bits cannot be guessed). They can fill the store with ids of their own, which the
  per-address quota for new ids, the per-address limits and the shared caps bound; at the caps new ids are
  refused for everyone (`store-full`) until entries expire (10 minutes for the mailbox, 24 hours for
  snapshots), while ids already held keep working: a denial of new widget snapshots and token returns, not
  of pushes and not of anything else on the box. Many addresses can likewise fill a limit table (7,000 keys
  on the `/push/` routes, 200,000 on the Live Activity route); a full table refuses new tokens, ids and
  addresses with 429 until its keys age out, while keys already in it keep their quota.
- With one id (from that phone or computer), an attacker can read and replace that one mailbox entry or
  snapshot. The relay stores the snapshot's bytes unchanged and neither decrypts nor checks them: that they
  are unreadable rests on the computer encrypting them for the phone before it uploads them. An update token
  lets them do no more than the Live Activity token above. The mailbox entry itself is not encrypted: it is
  that activity's update token as the phone left it, so whoever holds the id can take it and then update or
  end that phone's Live Activity as above. Only the widget snapshot is encrypted, by the computer.
- Nobody can get the key out through the relay: it is read from a file only the service's user can read,
  is never logged, and no answer carries it.

What the relay can read: of an alert, the event, the identifiers in `threadId` and `data` and whether it
sounds; of a mailbox entry, the update token itself, held as the phone sent it; nothing of the envelope, which
it never decrypts (it checks only the envelope's form and size). A snapshot it stores as the bytes it was
given, without decrypting or checking them; its confidentiality relies on the computer encrypting it before
the upload. What Apple sees of an alert:
the fixed title and event words, `threadId`, `data`, the push type, the token and the envelope as ciphertext;
of a widget push, the push type and the token. That these hold no
names rests on the computer that sends them (see the alert route). The mailbox and the snapshots stay in the
relay's memory only, never on disk, and are gone on a restart.

The log has counts, Apple's status and reason, and an 8-hex hash of a token or id; never a token, an id, a
content state, an envelope, a snapshot, the key's id or anything from the key's files. A mailbox or snapshot id
travels only in the `Authorization` header, never in a URL, so it stays out of Caddy's logs too (Caddy logs
that header as `REDACTED`).

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
5. Add the `Caddyfile` blocks (`handle /live-activity/*` with an 8 KB body limit and `handle /push/*` with
   16 KB) inside the `share.ghostex.dev` site in `/etc/caddy/Caddyfile`, check the whole file with
   `caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile` (without `--config`, Caddy checks a
   `Caddyfile` in the current folder, which in the release folder is the bare fragment and fails), and
   `systemctl reload caddy` while no shared chat is connected (a reload closes
   proxied WebSockets; check `ss -tn state established '( sport = :58800 )'`).
6. Check: `curl -s https://share.ghostex.dev/live-activity/healthz` and
   `curl -s https://share.ghostex.dev/push/healthz`.

The key: copy `AuthKey.p8` to `/etc/ghostex-push-relay/AuthKey.p8` and write `KEY_ID=` and `TEAM_ID=` to
`/etc/ghostex-push-relay/apns.conf`, both `root:ghostex-push-relay` mode 0640, then
`systemctl restart ghostex-push-relay`; `journalctl -u ghostex-push-relay` says `configured: the key loaded`
and the health checks say `"configured":true`. That only means the key and its two ids were read. Whether
Apple accepts the key for each topic and environment (notifications, Live Activity, widgets; production or
sandbox) shows only on a phone: run the TestFlight checks of the setup guide (a notification on the locked
phone, a Live Activity started while the app is closed, a widget that updates while the app stays closed). A
key Apple does not allow for a topic gets Apple's refusal on that route (502 `apple`, e.g.
`TopicDisallowed`).

Updating: replace the binary and `systemctl restart ghostex-push-relay`; a computer retries a push the
restart refused. A restart empties the mailbox and the snapshots; the phone leaves a token again on its next
start and the computer uploads the snapshot again on the next change.

Adding the `/push/` routes to a relay that already runs: build with `build-linux.sh`, replace the binary,
add the `handle /push/*` block next to `handle /live-activity/*` (step 5), `systemctl restart
ghostex-push-relay`, reload Caddy, and check both health routes (step 6). The Live Activity route answers as
before throughout.
