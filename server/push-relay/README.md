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
  "envelope": "<standard base64: the title and body, encrypted on the computer for the phone>",
  "threadId": "<machine id>:<session id>",
  "sound": true,
  "timestamp": 1794355519
}
```

- `kind`: the event, one of the table below, or `background`.
- `environment`, `timestamp`: as for a Live Activity push (`timestamp` is the event's time).
- `envelope`: optional, at most 3072 characters of standard base64 (with padding). The relay checks only its
  length and that it is base64, and passes it on as the payload's top-level `envelope` key; the phone's
  notification service extension decrypts it and replaces the title and body. Without it, or when the phone
  cannot decrypt it, the phone shows the fixed words.
- `threadId`: optional, 1 to 128 letters, digits or `. _ : -`. iOS stacks the alerts of one thread together.
  It reaches Apple readable, so it must be an id, never a name.
- `sound`: optional; `true` plays the default sound. No caller picks a sound file.

What the phone shows in the clear is the relay's own: the title `Ghostex` and the event's fixed words, the
ones gxserver's alerts use (`server/src/push/message.rs`). No request can put other words on a lock screen.

| `kind` | Words |
| --- | --- |
| `finished` | Finished |
| `waiting` | Waiting for you |
| `question` | Has a question |
| `failed` | Ended with an error |
| `security` | Security: check this was you |
| `stuck` | Not moving: it may be stuck |
| `blocked` | Blocked: out of usage on every account |
| `threadReport` | Reported to its coordinator |
| `outOfMemory` | Stopped to free memory |

An alert goes to Apple as push type `alert`, topic `com.maddada.ghostex.ios`, priority 10, with
`"mutable-content": 1` (so the notification service extension runs) and an expiry of the event's time plus
one hour:
`{"aps":{"alert":{"title":"Ghostex","body":"Finished"},"mutable-content":1,"thread-id":"…","sound":"default"},"envelope":"…"}`.

`kind: "background"` sends a background push for the app's opportunistic refresh instead: push type
`background`, same topic, priority 5 (the only one Apple takes for it), `{"aps":{"content-available":1}}`, an
expiry of the event's time plus 10 minutes. It takes no `envelope`, `threadId` or `sound`.

Any other field, or any other `kind`, is refused.

## Widget pushes: `POST /push/v1/widgets`

`{"token":"<the widget extension's push token; hex>","environment":"production"}` (environment optional). It
goes to Apple as push type `widgets`, topic `com.maddada.ghostex.ios.push-type.widgets`, priority 5, expiry one
hour, payload `{"aps":{"content-changed":true}}`: WidgetKit reloads the widgets when its budget allows, and
they fetch the snapshot. Any other field is refused.

Answers of both push routes are those of the Live Activity route (table above): 200 with `apnsId`, 400
`bad-request`, 410 `token-gone`, 413 `too-large` (over 16 KB, an envelope over 3072 characters, or Apple's
payload over 4 KB), 429 `rate-limited`, 502 `apple` or `apple-unreachable`, 503 `not-configured`.

## Token mailbox: `PUT` and `GET /push/v1/mailbox/<id>`

When a push starts a Live Activity, only the woken phone learns the activity's update token. It leaves the token
here and the computer collects it.

- `<id>`: 256 bits in hex (64 characters), which the phone and the computer derive from their pairing key.
  The relay files the entry under the id's SHA-256 and never holds or logs the id itself.
- `PUT`: the body is the update token in hex (64 to 512 characters, at most 1 KB with whitespace around
  it). It replaces what the id holds. 200 `{"ok":true}`.
- `GET`: 200 `{"ok":true,"token":"<hex>"}`, and the entry is gone; 404 `empty` when the id holds nothing.
- An entry lives at most 10 minutes.

## Widget snapshot: `PUT` and `GET /push/v1/snapshot/<id>`

While the app is closed, the computer leaves the widgets' data here, encrypted, and the widget extension
fetches it when it reloads.

- `<id>`: as for the mailbox.
- `PUT`: the body is the encrypted snapshot, any bytes, 1 byte to 8 KiB. It replaces what the id holds and
  starts its 24 hours again. 200 `{"ok":true}`.
- `GET`: 200 with the bytes as stored (`application/octet-stream`); 404 `empty` when the id holds nothing.
- A snapshot is kept until it is replaced or for 24 hours.

Answers of both store routes (JSON unless a snapshot comes back):

| Status | `error` | Meaning |
| --- | --- | --- |
| 200 | | Stored, or here it is. |
| 400 | `bad-request` | The id is not 64 hex characters, the mailbox body is not a token in hex, or the snapshot is empty. |
| 404 | `empty` | Nothing is held under this id (never stored, taken, or expired). |
| 413 | `too-large` | A mailbox body over 1 KB, a snapshot over 8 KiB, or a body over 16 KB. |
| 429 | `rate-limited`, `too-many-new-ids` | `retryAfter` seconds (also `Retry-After`); `too-many-new-ids` when this address created its quota of new ids. Nothing was stored. |
| 503 | `store-full` | The relay holds as much as it may; `retryAfter` 60. Nothing was stored. |

They need no Apple key; they answer while the relay is not configured.

`GET /push/healthz` answers `{"ok":true,"configured":true|false,"sent":N,"refused":N,"rateLimited":N,"appleErrors":N,"stored":N,"storeFull":N,"mailboxEntries":N,"snapshots":N,"storeBytes":N,"goneTokens":N}`
(counts of the `/push/` routes; `goneTokens` is the one memory both routes share).

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
  minutes after a start and a widget fetches its snapshot when it reloads; neither needs more.
- New ids per caller address: 20, refilled 20 an hour, for the mailbox and the snapshot store together,
  checked before anything is stored. Replacing what an id already holds, and reading, do not count. A phone
  leaves a mailbox entry after a pushed start and a computer keeps one snapshot per paired phone, so a home
  with several phones and computers needs a few an hour.
- What the mailbox and the snapshot store hold together: at most 20,000 entries and 16 MiB of tokens and
  snapshots. Before a new id is refused as `store-full`, expired entries are dropped; an id that already
  holds something can still be read and replaced (a replacement only when it does not grow the store past
  16 MiB). Nothing old is pushed out for something new, so a flood cannot evict real snapshots.
- Memory: the service runs with `MemoryMax=128M`. The store takes at most about 21 MB (16 MiB of contents and
  about 5 MB for 20,000 entries). Each of the `/push/` routes' eight limit tables holds at most 25,000 keys,
  about 14 MB together; the Live Activity route's tables hold at most 200,000 keys each and the gone tokens
  100,000, about 50 MB together (estimates of the hash tables at their caps). Full to every cap at once,
  which only a flood from very many addresses could reach, that is about 85 MB, under the limit with room
  for the program itself. A table full of busy keys refuses a new key with 429 rather than growing.

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
  of pushes and not of anything else on the box.
- With one id (from that phone or computer), an attacker can read and replace that one mailbox entry or
  snapshot. They read nothing of the snapshot, which the computer encrypts for the phone, and an update token
  lets them do no more than the Live Activity token above.
- Nobody can get the key out through the relay: it is read from a file only the service's user can read,
  is never logged, and no answer carries it.

What the relay can read: of an alert, the event, the thread id and whether it sounds; nothing of the
envelope or the snapshot, which it never parses. What Apple sees of an alert: the fixed title and event
words, the thread id, the push type, the token and the envelope as ciphertext; of a widget push, the push type
and the token. The mailbox and the snapshots stay in the relay's memory only, never on disk, and are gone on
a restart.

The log has counts, Apple's status and reason, and an 8-hex hash of a token or id; never a token, an id, a
content state, an envelope, a snapshot or anything from the key's files.

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
   16 KB) inside the `share.ghostex.dev` site in `/etc/caddy/Caddyfile`, check it with
   `caddy validate`, and `systemctl reload caddy` while no shared chat is connected (a reload closes
   proxied WebSockets; check `ss -tn state established '( sport = :58800 )'`).
6. Check: `curl -s https://share.ghostex.dev/live-activity/healthz` and
   `curl -s https://share.ghostex.dev/push/healthz`.

The key: copy `AuthKey.p8` to `/etc/ghostex-push-relay/AuthKey.p8` and write `KEY_ID=` and `TEAM_ID=` to
`/etc/ghostex-push-relay/apns.conf`, both `root:ghostex-push-relay` mode 0640, then
`systemctl restart ghostex-push-relay`; `journalctl -u ghostex-push-relay` says `configured key_id=…`
and the health check says `"configured":true`. The same key signs Live Activity, notification, background
and widget pushes of the app; a key limited to other topics gets Apple's refusal on the routes it does not
cover (502 `apple`).

Updating: replace the binary and `systemctl restart ghostex-push-relay`; a computer retries a push the
restart refused. A restart empties the mailbox and the snapshots; the phone leaves a token again on its next
start and the computer uploads the snapshot again on the next change.

Adding the `/push/` routes to a relay that already runs: build with `build-linux.sh`, replace the binary,
add the `handle /push/*` block next to `handle /live-activity/*` (step 5), `systemctl restart
ghostex-push-relay`, reload Caddy, and check both health routes (step 6). The Live Activity route answers as
before throughout.
