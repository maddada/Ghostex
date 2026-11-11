# ghostex-push-relay

Ghostex's Live Activity push relay, at `https://share.ghostex.dev/live-activity/`. While the iPhone is
locked, iOS keeps the Ghostex app suspended, so the app cannot update its Lock Screen banner (the Live
Activity) itself; only a push to the activity can. Those pushes go through Apple's push service with the
Apple push key (`.p8`). Expo's push service, which carries Ghostex's notifications, does not carry Live
Activity pushes, and the key cannot go on users' computers. So a user's computer (gxserver) asks this
relay, which holds the key, to send the push.

It lives beside `server/share-relay` because it is the same kind of thing: a small standalone service on
the same box (relay-eu.ghostex.dev), behind the same Caddy, built the same way into one static binary. It
is its own crate (own `Cargo.lock`), so gxserver's build never compiles it.

## Request

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

`GET /live-activity/healthz` answers `{"ok":true,"configured":true|false,"sent":N,"refused":N,"rateLimited":N,"appleErrors":N}`.

## Limits, and why these numbers

- Per device token: 10 pushes, refilled 10 a minute. A banner changes when an agent starts, waits,
  finishes or stops; gxserver coalesces those, so a phone needs a few a minute at most. Apple budgets
  pushes per activity and throttles a device that gets too many; 10 a minute keeps a broken computer
  inside that.
- Per push-to-start token: 2, then one every 10 minutes. A start is needed only when no banner runs; Apple
  limits starts tightly.
- Per caller address (the last `X-Forwarded-For` entry, which Caddy sets): 60, refilled 60 a minute, for
  every request, refused ones included. Enough for a home with several computers and phones, and it stops
  one address from probing tokens quickly.
- Whole relay: 300, refilled 3000 a minute (50 a second), and 64 requests to Apple in flight. Apple takes
  far more than that from one key; the cap is what keeps a flood from many addresses from reaching Apple
  as a flood, so Apple never has a reason to throttle the key itself.
- A token Apple called gone (410, `BadDeviceToken`, `Unregistered`, `DeviceTokenNotForTopic`) is refused
  here for 24 hours.
- One provider token (the signed JWT) is reused for 40 minutes: Apple refuses one older than an hour and
  answers `TooManyProviderTokenUpdates` to one renewed more often than every 20 minutes.

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
- Nobody can get the key out through the relay: it is read from a file only the service's user can read,
  is never logged, and no answer carries it.

The log has counts, Apple's status and reason, and an 8-hex hash of a token; never a token, a content
state or anything from the key's files.

## Run it locally

```bash
cd server/push-relay && cargo test          # unit tests and the stand-in for Apple (tests/relay.rs)
cargo test --test relay -- --nocapture      # also prints each request the stand-in received
cargo run -- --listen 127.0.0.1:58810 --key /path/AuthKey.p8 --config /path/apns.conf
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
5. Add the `Caddyfile` block inside the `share.ghostex.dev` site in `/etc/caddy/Caddyfile`, check it with
   `caddy validate`, and `systemctl reload caddy` while no shared chat is connected (a reload closes
   proxied WebSockets; check `ss -tn state established '( sport = :58800 )'`).
6. Check: `curl -s https://share.ghostex.dev/live-activity/healthz`.

The key: copy `AuthKey.p8` to `/etc/ghostex-push-relay/AuthKey.p8` and write `KEY_ID=` and `TEAM_ID=` to
`/etc/ghostex-push-relay/apns.conf`, both `root:ghostex-push-relay` mode 0640, then
`systemctl restart ghostex-push-relay`; `journalctl -u ghostex-push-relay` says `configured key_id=…`
and the health check says `"configured":true`.

Updating: replace the binary and `systemctl restart ghostex-push-relay`; a computer retries a push the
restart refused.
