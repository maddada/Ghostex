# Security: the door, plain approvals, tokens (GitHub #495, option B since 10 Nov)

Read this before you touch a route that lets someone in from outside, or build on pairing, Collab
or the security log. The user's decisions are in `~/.local/share/agent-work/Ghostex/issue-495/decisions.md`
(11.0.1) and the option B brief `~/.local/share/agent-work/Ghostex/security-b/brief.md`; this file
is the contract in code.

Code:

- gxserver: `server/src/security/` (principals and capabilities in `capability.rs`, the one rule of
  what asks in `policy.rs`, the request store, keys, session and owner tokens, persistence),
  `server/src/server/security_http.rs` (the approval API, `require_owner_presence`, the live
  events), `server/src/server/security_gates.rs` (where a route asks or refuses before its
  handler), the `ghostex` CLI's wait-and-retry (`ghostex_cli/rpc.rs`).
- Shared: `packages/gx-protocol/src/approval.rs` (wire types; the signed payloads stay for older
  apps), the `approvalRequestsChanged` event frame in `event.rs`.
- Desktop: `apps/desktop/src/app/owner_security/` (`plain.rs` for a gxserver that approves plainly,
  `mod.rs` for an older one, `ui.rs` for the card), `packages/owner-keys` (the keys and the system
  prompt).
- Phone (phone repo): `src/security/approvals.ts`, `src/screens/ApprovalsScreen.tsx`,
  `src/machines/sshPairing.ts`.

## The rule (option B, 10 Nov)

The user (Q2140 = B): "thinking of removing all these dumb blocks because they're adding friction
and a lot of bugs … i dont know why we want to ban agents from doing everything with ghostex", and
"add a toggle to require touch id/pin etc for access but disabled by default, add it to security".

1. **The door stays.** Nothing from the network gets in without a token it got from pairing; an
   empty Easy Connect allow-list lets nobody in; a Collab link needs its code and, in Approve mode,
   each guest message waits for your click.
2. **Only what lets someone new in asks**, with one plain Approve / Deny card: no Touch ID, no
   signature, no owner or approver key.
3. **Everything else just works** for you, your devices and your agents: settings, connected
   computers, GBots, Autopilot, Cloud Boxes, teams, extensions, secret providers, full access and
   steering other sessions never ask and are never refused because the caller is an agent.
4. **One switch, off by default:** Settings > Security, "Require Touch ID or your password to
   approve" (`approvalsRequirePresence`, per platform; the phone's "Require Face ID to approve" on
   the computer's page under Machines). It is a check the approving device makes itself before it
   sends the same plain approval; gxserver keeps it per computer and reports it as
   `requirePresence` in `/api/securityStatus`, so every device approving for that computer follows it.

## Who is calling: `Principal`

The auth gate (`route_http/dispatch.rs`) settles one `Principal` per request and hands it to
every route as `RouteHttpRequest::caller`.

| Principal | Token | Notes |
| --- | --- | --- |
| `Owner { device_id }` | `gxo_…`, in the desktop app's memory only | Opened with the owner key (`/api/openOwnerSession`). gxserver keeps only its hash (newest four per device), so it survives a gxserver restart. |
| `Device { device_id }` | `gxd_…`, bound to the phone's device key | A paired phone; `device_id` is its row in Paired devices (pairing v2, see below). |
| `Session { project_id, session_id, role }` | `gxs_…`, in the session's `GHOSTEX_SESSION_TOKEN` | Issued when an agent or terminal starts (zmx and wmx), the same token again when that session starts again, revoked when the session is deleted (and by a 5-minute sweep for sessions deleted another way). `role` from the coordinator markers: `agent`, `coordinator`, `thread`; `head` is never stored (see "Chief's head role" below). The `ghostex` CLI inside a session sends it. |
| `LocalAgent` | the token file `gxserver/auth/token` | You: the desktop app (it opens no owner session on a gxserver that approves plainly), the GPUI web build, the CLI in your own terminal, a phone on its SSH login, and another computer's app through its SSH tunnel. Hooks, plugins and older sessions use it too. |
| `Peer { server_id, name }` | a connected computer's grant | Keeps `PEER_ALLOWED_ENDPOINTS`. |
| `Guest { share_id }` | `gxshare_…` | Keeps `GUEST_ENDPOINTS`. |
| `Extension { extension_id }` | an extension's API token | Keeps the extension registry's scope. |
| `Anonymous` | none | Public routes only. |

The sockets (`/api/events`, `/api/terminal`, `/api/voice`, `/api/browserTcp`, the CEF DevTools
host and pages) don't go through that gate: they take the token file, a phone's `gxd_` and the
desktop's `gxo_` owner session (`auth::is_authorized_client_token`). The desktop sends `gxo_` on
every call once it is open, its sidebar store's `/api/events` socket included, so a socket that
refused it would empty the sidebar of this computer's projects and sessions. A live `gxs_`
session token opens only `/api/voice` and the DevTools pages (`/api/cefDevtools/json`,
`/api/cefDevtools/page/<id>`, `auth::is_authorized_session_client_token`): the `ghostex` CLI in a
session sends it for `ghostex voice` and `ghostex browser mcp`. No client in a session uses the
others, so they keep refusing it. A cloud box attach (`ghostex_cli/cloud_attach.rs`) sends the
box's own token file, peers and extensions open no socket. Test:
`server/src/server/tests/socket_tokens.rs`.

## What they may do: `Capability`

`Principal::has(capability)`: the owner session, the token file and a paired phone (`is_owner_device`,
"you") hold every capability. An agent's session token holds every one except the two that keep
the door: `DecideApprovals` (list, approve and deny requests) and `OwnerSecrets` (answer a secret
request, have gxserver read a secret provider reference, the phone's Cloud Box kit). Coordinators
manage only their own threads (`ManageOwnThreads`; `ManageAnyThreads` is the token file's), which
is how threads are organised, not a security rule. Peers hold only `RequestSensitiveAction` and
keep `PEER_ALLOWED_ENDPOINTS`; guests, extensions and anonymous callers hold none and keep their
own allow-lists.

Refusals that stay because they keep the door: an agent's session token can't approve or deny,
list requests, answer or read a secret, enroll an owner key, get a pairing code (it gets the
connection details with an empty code), or ask to pair over SSH (`/api/pairOverSsh` takes only the
token file); a request is never approved by the caller that asked (`ApprovalStore::approve_plain`).
A GBot API chat takes input only from you (`api_chat_refused`): its text comes from outside Ghostex.

### Chief's head role

Chief (`server/src/chief/`) is one coordinator session per computer that the user lets steer every
other session (CDXC:Chief). No token is stored with `SessionRole::Head`: Chief's token holds
`coordinator` like any coordinator's, and the auth gate makes it `Head` per request
(`chief::settle_principal`) only when its `(project, session)` is the one in gxserver's Chief
record (`metadata.chiefSession`, written only when gxserver itself creates that session) and the
`chiefHidden` setting (user-only, not agent-writable) is off. Turning Chief off, New Chat and
Change Agent therefore take the role away on the next request. `/api/chief` `open`, `newChat` and
`setAgent` accept only the owner's devices and the token file.

`Principal::may_manage_thread_of(coordinator_session_id)` is the check for "coordinators manage
their own threads"; the gate refuses a session that links threads to, edits or promotes a
coordinator other than itself.

## How a route declares what it needs

`security::route_access(path) -> RouteAccess`, enforced by `security_gates::gate` for owner,
device, session and token-file callers before any route handler runs:

- `Public`: no token.
- `Needs(Capability)`: the caller must hold it (the default for unlisted routes is
  `ProjectSessions`, which every agent holds).
- `Presence(SensitiveAction)`: the whole call is one sensitive action; the gate asks with the
  call's params.

A route that is only partly sensitive is gated in `security_gates.rs` for that part (Easy Connect), or
from its handler with `require_owner_presence` (pairing and Collab).

Routes answered before the token gate (`route_http/dispatch.rs`), each trusting only its own
check: the Composio sign-in redirect (its OAuth `state`), visual pages, file links, and the GBot
API, `/hooks/gbots/…` (`server/src/gbots/api/inbound.rs`). The GBot API is opened only by that
GBot's own `gxb_` key (32 random bytes, stored as its SHA-256 in `gbots_bot_keys`, one live key per
GBot), which is checked there and accepted nowhere else, so it never passes the gate as another
caller. It answers on the local listener only (127.0.0.1 and `::1`; devices reach it only over an
SSH path they already have), with 10 calls a minute per key, 200 messages and 20 new chats a day,
at most 3 open chats and one new chat a minute. Its chats (`runtimeSettings.gbot.openedBy:
"api:<keyId>"`) start in their own folder (`<app data>/gbots/api/<botId>/`, outside the GBot's
folder) with a brief that leaves out memory and preferences unless the owner shares them, are
always held to the secret folders plus gxserver's own files (`with_api_chat_sandbox`: Codex in a
read-only profile writing only its folder, Claude's deny rules), never write memory, and take input
(answers, typing, messages, queue edits, interrupts) from the owner's devices only
(`api_chat_refused`, checked before every other gate). `/api/updateSession` keeps the `gbot` and
`gbotHome` marks and an API chat's folder as stored (`gbots/markers.rs`).

## What asks: `SecurityState::skips_approval` (`security/policy.rs`)

The list is `ghostex_gx_protocol::approval::STILL_ASKS` (action, when it asks, and the line Settings >
Security's "What still asks" shows): `skips_approval` reads it, so the page and the rule can't drift.

| Action | Asks when | Who can approve |
| --- | --- | --- |
| `pairingApproveDevice` | a phone or computer asks to pair (three matching words) | you |
| `collabApproveGuestMessage` | each guest message in Approve mode | you |
| `easyConnectEnable` | an agent's session token turns Easy Connect on, the first time (remembered for good in `security.json` `rememberedApprovals`; an allow-list that already holds a device counts) | you |
| `easyConnectAllowList` | an agent's session token adds a device to the allow-list (emptying it closes it, so never asks) | you |
| `collabShareChat`, `collabEnableSendMode` | an agent's session token makes a share, asks for its link again, or turns on Send mode | you |
| every other action | never | |

Your own click in Ghostex is the approval for your own action, so it never makes a card. A
skipped call gets a `PresenceGrant` with `by_policy: true` and a stand-in approval (id `allowed`,
status `consumed`) that names the change and was never a request. Chief's steering of other
sessions is still written to the security log as `allowed`.

Settings: no setting asks or is refused for agents any more. The API and `ghostex settings set`
write the ten formerly owner-only keys (`help::api_may_write`); the committed Help keeps marking
them "not agent-writable" until the release that ships option B empties `USER_ONLY_KEYS` and
regenerates it (`~/.local/share/agent-work/Ghostex/security-b/release-steps.md`). Keys on the
Accounts and Remote pages, and keys holding a secret, stay out of the API as before: they are
account or pairing state, not preferences.

## The gate: `require_owner_presence`

In a route handler (`crate::server::require_owner_presence`):

```rust
let action = ActionRequest::new(SensitiveAction::CollabShareChat, "Share “Fix login” through a link")
    .target(session_id.clone())
    .params(Value::Object(params.clone()));      // the call's exact params; approvalId is dropped
let grant = match require_owner_presence(&state, &caller, action, &body_json, &path, &request_id) {
    Ok(grant) => grant,                         // asked nobody (`by_policy`) or spent an approval
    Err(response) => return response,           // 403 approvalRequired / approvalDenied / forbidden
};
// …make the change…
```

- The summary and detail are written by gxserver (or the Collab component), never copied from the
  caller: an agent must not write what you approve. Never put a secret in them.
- The approval is bound to the action, target and the params' digest (SHA-256 of canonical JSON).
- The error body is the usual RPC error plus `approval` (an `ApprovalRequest`), and its message
  names the id. The caller repeats the same call with `params.approvalId` once it is approved; only
  the caller that asked (or you) can spend it, once, within 2 minutes. A pending request lapses
  after 10 minutes; an identical pending request from the same caller is reused, and a newer one
  for the same thing (the same action and target, or for pairing the same device key) replaces
  the caller's older one, which becomes `expired` with `replacedBy` (a Collab guest's messages
  never replace each other).
- Clients: the `ghostex` CLI prints what it waits for on stderr, polls `/api/readApprovalRequest`
  and repeats the call (`--no-wait` returns at once). The desktop and the phone show the card.

## The approval API

| Endpoint | Params | Who | Result |
| --- | --- | --- | --- |
| `/api/listApprovalRequests` | none | you | `{ requests }`, pending, oldest first |
| `/api/readApprovalRequest` | `{ id }` | its requester, you | `{ request, current? }`: `current` is the newest request that replaced this one; an unknown id is `notFound` "This request expired." |
| `/api/approveApprovalRequest` | `{ id }` (older apps add `keyId` and `signature`, accepted and not checked) | you, never the requester | `{ request }` (status `approved`) |
| `/api/denyApprovalRequest` | `{ id }` | you | `{ request }` (`denied`) |
| `/api/securityStatus` | none | any but guests | `{ serverId, caller, plainApprovals: true, requirePresence, ownerKeys, approverKeys, pendingApprovals }` |
| `/api/listApproverKeys`, `/api/enrollApproverKey`, `/api/removeApproverKey` | as before | you | kept for older apps: an approver key enrolls at once and is listed, but approving checks no signature |
| `/api/enrollOwnerKey`, `/api/ownerChallenge`, `/api/openOwnerSession` | as before (`askOwner`, `approverKey` accepted) | the token file or an owner session (an agent's session token can't add an owner key) | kept for desktops from before plain approvals: a key enrolls at once, no "act as the owner" card (#532) |

Statuses: `pending`, `approved`, `denied`, `expired`, `consumed` (a replaced request is `expired`
with `replacedBy`; no new status, so older apps read it). Approving or denying a lapsed one answers
`expired` with plain words (`expired_message` in packages/gx-protocol/src/approval.rs: "This request
expired. Ask again from the phone."), a replaced one `replaced`. Requests are saved in
`gxserver/auth/approval-requests.json` (see Persistence), so after a restart they keep their id,
words and expiry. `/api/events` carries
`{ type: "approvalRequestsChanged", change: "requested"|"approved"|"denied"|"expired", request }`;
a replaced request goes out as `expired` with `replacedBy`, so its card closes and the newer
request's card shows. The desktop reads a card's request back before Approve and moves to
`current`, shown to be read (its words may differ), never approved for the user.

## Older apps and older gxservers

- gxserver says `plainApprovals` in `/api/securityStatus` and in `ghostex server endpoint`'s
  capabilities. A desktop (`owner_security/plain.rs`) or phone (`security/approvals.ts`) that sees
  it approves with `{ id }` (the system prompt first only with `requirePresence`); one that doesn't
  (a gxserver from 11.2.2 or earlier) keeps the signed approval, the owner session and the approver
  key. That client code is marked with a dated note: delete it once computers older than this
  release no longer matter.
- Phones and desktops from 11.1.0 to 11.2.2 against a new gxserver: their owner and approver keys
  enroll at once, their signed approvals count as plain ones, their pairing flow is unchanged, and
  calls that used to wait for an approval now just go through. An 11.2.2 desktop still hides a
  request its owner session made by device id; under option B your own devices make no requests,
  so nothing it hides goes unanswered.
- The desktop hides a card only for a request it got back from its own calls
  (`owner_security::plain::note_own_request`), never by device id: two Macs share an owner key
  after Migration Assistant (Gregg's report, 10 Nov).
- The payload an older app signs (`approval_signing_payload`) and the owner session's
  (`owner_session_signing_payload`) are unchanged in `approval.rs`; owner sessions still verify
  their signature, because an owner session is one of your devices.

## Persistence

`gxserver/auth/security.json` (0600, written whole through a rename): `ownerKeys`,
`approverKeys`, `sessionTokens` (whole tokens, so a restarted session gets the same one; the file
is as readable to a same-user program as the token file), `ownerSessions` (hashes only),
`approvedComputers` (server id → when; read by nothing since option B, kept so the file
round-trips), `rememberedApprovals` (key → when: approvals asked once and kept for good, today
`easyConnectEnable`; older gxservers ignore it), `approvedProjectReach` (`{ caller, target }`: an agent session and a project it may reach). An unreadable file is moved aside and gxserver starts with
none; the desktop then enrolls again on first use.

`gxserver/auth/approval-requests.json` (0600, written whole through a rename on every change):
each request with its asker (`requester`, a `Principal`), `shared`, `replaceKey`, and `resume`:
what the part of gxserver that asked keeps to finish it once approved after a restart (`pairing`:
the waiting phone or computer, restored by `RemotePairingRuntime::restore_waiting`; `collab`: the
call Collab repeats, restored by `collab_shim::http::approvals::start`). Decided requests are kept
30 minutes, so a client hears how its request ended. An unreadable file is moved aside and the
askers ask again. Older gxservers ignore the file.

## The security log (`crate::audit`)

The gate writes to the hash-chained security log: a new request as `pending` (which also sends
the phone a push), a refusal at the gate, a denial at `/api/denyApprovalRequest`, owner and
approver keys added or removed. When the gate spends an approval on a request, it is noted for
that request (`audit::note_approval`): an entry the route's handler records with `Outcome::Done`
carries the approval and reads as `approved`. Routes whose handlers keep no entry of their own
(prompt answers, terminal input, settings, GBots on) get one `approved` entry from the gate. The
log names `gxs_` callers as the session and `gxo_` callers as the owner.

## Security events (in-process stream)

`state.security.subscribe()` returns a `tokio::sync::broadcast::Receiver<SecurityEvent>`; a slow
subscriber loses the oldest events (`Lagged`) and never blocks gxserver. Events (`type` tag,
camelCase): `approvalRequested`, `approvalApproved { keyId }`, `approvalDenied { by }`,
`approvalExpired`, `sensitiveActionAuthorized { action, target, approvalId, caller }` (an approval
was spent; the action is about to run), `sensitiveActionPerformed { action, target, caller, ok }`
(from `record_performed`), `sessionTokenMinted`, `sessionTokenRevoked`, `approverKeyAdded`,
`approverKeyRemoved { by }`. The audit-log thread owns the hash-chained log and the notification
per event.

## The voice agent's tools

The voice agent's tools call gxserver's own routes on the loopback listener
(`voice_agent/loopback.rs`) with the token the attached client opened `/api/voice` with
(`Input::Caller`, the last client to attach wins), not the token file: the desktop's `gxo_`, a
phone's `gxd_`, a session's `gxs_`, or the token file (the web build, `ghostex voice` outside a
session). So a call can do exactly what its client may. The owner's devices answer prompts, type
into sessions and stop turns without asking, as their own clicks do. For any other client a gated
call answers `approvalRequired`: the tool tells the model at once that it waits for the user's
approval on their computer or phone (the request is an agent's, so it shows as a card on both),
`voice_agent/approvals.rs` reads the request every two seconds as that client, and once approved
runs the tool again with the approval (calls that already went through are not repeated) and
announces the result; a denial or a lapse is announced too. Test:
`server/src/server/tests/voice_caller_token.rs`.

## Pairing v2 (devices)

Code: `server/src/remote_access/pair_device.rs`, `packages/gx-protocol/src/pairing.rs`.

- `/api/remotePairingCode` gives a code (the one-time nonce) only to `Principal::Owner` with
  `openTunnel: true` (the desktop's Show QR sheet is open); everyone else gets the Easy Connect
  details with empty `payload`/`text` and no `secret`. A code lives 5 minutes and is single-use.
- `/api/pairDevice` (public) takes `deviceKey` (P-256, `ecdsaP256Sha256`) and optionally the
  phone's `approverKey`, consumes the code, and answers `{ status: "pending", pendingId, words }`.
  It creates a `pairingApproveDevice` approval request (target: the pending id; the detail names
  the three words) with `Principal::Anonymous` as requester. Nothing is granted yet.
- Words: SHA-256 of `ghostex-pairing-words-v1\n<nonce>\n<deviceKey.publicKey>`, first three bytes,
  each an index into `PAIRING_WORDS` (256 words). The phone computes them itself.
- `/api/pairingStatus` (public) `{ pendingId, timestampMs, signature }`, signed by the device key
  over `ghostex-pairing-status-v1\n<pendingId>\n<timestampMs>`. The ask that finds the approval
  signed spends it and writes the pairing: the restricted SSH line, the device row with its key, a
  `gxd_` token (30 days) and, if sent, the phone's approver key (enrolled with the same approval).
- `/api/renewDeviceToken` (public) `{ deviceId, timestampMs, signature }` over
  `ghostex-device-token-v1\n<deviceId>\n<timestampMs>`: a fresh token, the old one stops working.
- `/api/pairedDevices` lists `pending` (with words) beside `devices` (`status`: `paired` or
  `legacy`). `/api/removePairedDevice` revokes a device (key line, token, push, allow-list entry,
  approver key) or denies a pending one; it never needs approval.
- Phones that sign in over SSH (Tailscale, the local network) pair through that login:
  `/api/pairOverSsh` (token file only, `LocalAgent`; a session token, peer or paired phone is
  refused) takes `/api/pairDevice`'s body without the code, SSH key and tailcat key, and answers
  `{ status: "pending", pendingId, nonce, words }` (words from that fresh nonce and the device key).
  The same `pairingApproveDevice` request (its params carry `overSsh: true`) and `/api/pairingStatus`
  finish it without the SSH line and allow-list entry: the device row, the `gxd_` token and the
  approver key. Such rows answer `overSsh: true` in `/api/pairedDevices`, as do their Pending
  entries. `ghostex server endpoint` lists the `pairOverSsh` capability.
- Computers pair the same way (`kind: "computer"` on `/api/pairDevice`, sent by the connecting
  computer's gxserver from `/api/pairComputer`, `remote_access/computer_pairing.rs`): Pending, three
  words, the signed approval; what it grants is only the computer's tailcat client key on the
  allow-list (no SSH line, no token: a computer signs in with its own SSH login and reaches
  gxserver through connected-computer grants). An empty Easy Connect allow-list is closed.
- Rows from before 11.0.1 are `legacy`: no token, SSH line removed at start where no Windows
  administrator prompt is needed. `remote_access::has_active_easy_connect_devices(&db)` and
  `easy_connect_devices_to_pair_again(&db)` are for the one-time re-pair notice.

## Bundled skills are signed

gxserver refreshes Ghostex's bundled skills from GitHub between releases
(`server/src/agent_skills_remote.rs`), but installs a downloaded file only when it matches a
statement signed with the component signing key: the `agent-skills` release's
`ghostex-skills.json`, `{ statement, signature }`, where the statement is
`ghostex-skills-v1\n<commit>\n<issuedAtMs>\n<skill>/<path> <sha256> <size> <mode>…` (sorted).
The files are fetched at that commit and checked by SHA-256; a manifest older than the newest one
accepted is refused. `.github/workflows/publish-agent-skills.yml` signs the skills at the
dispatched commit (`tooling/release-gpui/sign-agent-skills.mjs`) and replaces the asset. Without a
manifest that verifies, nothing is downloaded and the app's bundled copy stays the source. Debug
builds may read `GHOSTEX_AGENT_SKILLS_MANIFEST_URL` and `GHOSTEX_AGENT_SKILLS_FILES_URL` (and the
test key in `GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY`) to test against a local copy.

## Not covered yet

- The macOS approver key is a Keychain key behind Touch ID, not a Secure Enclave key (user
  decision 1A of the follow-up: revisit with a `keychain-access-groups` entitlement later).
- Settings > Security lists the security log, not the approver keys or pending requests; pending
  requests show as cards on the desktop and in the phone's Approvals screen.
- `/api/terminal` still opens for the token file, so a token-file caller can attach to any
  session's terminal and type there without the approval `/api/sendSessionText` asks for. This is
  part of the same-user limit, not a separate hole: any program running as the user can already
  type into any session without gxserver, by running `ghostex attach <session>` (it execs the
  session's stored `zmx attach` command, `ghostex_cli/attach.rs`) or by attaching to the zmx
  socket, which is the user's own file. Shutting the token file out of `/api/terminal` alone
  would break its only two clients, both of which hold just a token file today: the GPUI web
  build (`/api/webBootstrap` hands it this computer's token file) and Cloud Box attach
  (`ghostex cloud attach` sends the box's token file). Tighten it together with their own tokens:
  an approval-gated owner session for `ghostex web`, and a token the box gives home at boot that
  agents on the box can't read. The desktop (local and over SSH), the phone and `ghostex attach`
  don't use `/api/terminal`.

Retired after 11.1.2 (user: "let's please stop limiting agents from talking to other projects this
is dumb", and "please let agents manage other sessions like before"): the `reachOtherProject`
question (11.1.1 and 11.1.2: an agent's first message to, or new session in, another project) with
its remembered approvals (`approvedProjectReach` in `security.json` is read and ignored, and the
next save drops it), and the `answerOtherSessionPrompt`, `typeIntoOtherSession` and
`interruptOtherSession` questions, which nobody is asked any more.
