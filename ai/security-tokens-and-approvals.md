# Security: capability tokens, the owner key, signed approvals (GitHub #495, 11.0.1)

Read this before you touch a route that lets someone in from outside, or build on the pairing,
Collab or audit-log work of 11.0.1. The user's decisions are in
`~/.local/share/agent-work/Ghostex/issue-495/decisions.md`; this file is the contract in code.

Code:

- gxserver: `server/src/security/` (principals, capabilities, approvals, keys, session and owner
  tokens, persistence), `server/src/server/security_http.rs` (the approval and owner-key API, the
  `require_owner_presence` helper, the live events), `server/src/server/security_gates.rs` (where
  each sensitive route asks), the `ghostex` CLI's wait-and-retry (`ghostex_cli/rpc.rs`).
- Shared: `packages/gx-protocol/src/approval.rs` (wire types, the two signed payloads), the
  `approvalRequestsChanged` event frame in `event.rs`.
- Desktop: `packages/owner-keys` (the keys in the OS key store and the presence prompt, per
  platform), `apps/desktop/src/app/owner_security/` (owner sessions, approving, the approval card).
- Phone (phone repo branch `approvals-495`): `src/security/`, `src/screens/ApprovalsScreen.tsx`,
  the native approver key in `modules/ghostex-native` (`GhostexApproverKey.swift` / `.kt`).

## The rule

An agent can only **ask** for a sensitive action. The call turns into an approval request that
shows on the computer and the phone, and the action happens only after the owner approves it
with a signature from a key that Touch ID, Windows Hello, polkit or the phone's biometric prompt
unlocks. A plain Allow button is never enough (agents can drive the GUI). The owner's own desktop
session needs the signature too. Turning things off, stopping a share, removing a viewer and
revoking a device never need one.

## Who is calling: `Principal`

The auth gate (`route_http/dispatch.rs`) settles one `Principal` per request and hands it to
every route as `RouteHttpRequest::caller`.

| Principal | Token | Notes |
| --- | --- | --- |
| `Owner { device_id }` | `gxo_…`, in the desktop app's memory only | Opened with the owner key (`/api/openOwnerSession`). gxserver keeps only its hash (newest four per device), so it survives a gxserver restart. |
| `Device { device_id }` | `gxd_…` today | A paired phone. Pairing v2 replaces the token; until then `device_id` is `token:<first 12 hex of the hash>`. |
| `Session { project_id, session_id, role }` | `gxs_…`, in the session's `GHOSTEX_SESSION_TOKEN` | Issued when an agent or terminal starts (zmx and wmx), the same token again when that session starts again, revoked when the session is deleted (and by a 5-minute sweep for sessions deleted another way). `role` from the coordinator markers: `agent`, `coordinator`, `thread`. The `ghostex` CLI inside a session sends it. |
| `LocalAgent` | the token file `gxserver/auth/token` | Agent level plus thread management (user decision 3A). Hooks, plugins, older sessions and the CLI in the user's own terminal use it. |
| `Peer { server_id, name }` | a connected computer's grant | Keeps `PEER_ALLOWED_ENDPOINTS`. May ask (answering a prompt or typing into a session here go through the same gate as an agent here). |
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

`Principal::has(capability)`. Owner and Device have every capability. Peers have only
`RequestSensitiveAction`; guests, extensions and anonymous callers have none of these (all of them
keep their own endpoint allow-lists).

| Capability | Session agent/thread | Coordinator | LocalAgent |
| --- | --- | --- | --- |
| `OwnSession`, `ProjectSessions`, `AgentWritableSettings` | yes | yes | yes |
| `ManageOwnThreads` | no | yes | yes |
| `ManageAnyThreads` | no | no | yes |
| `OwnerSettings`, `AnswerAnyPrompt` (answer another session's permission prompt without asking) | no | no | no |
| `RequestSensitiveAction` (may ask) | yes | yes | yes |
| `DecideApprovals` (list, approve, deny, approver keys) | no | no | no |

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

A route that is only partly sensitive is gated in `security_gates.rs` for that part, or from its
handler with `require_owner_presence` (that is how the pairing and Collab routes do it).

## Sensitive actions: `SensitiveAction`

Defined once in `ghostex_gx_protocol::approval::SensitiveAction` (wire names in camelCase):

| Group | Actions | Wired by | Asks when |
| --- | --- | --- | --- |
| Collab | `collabEnable`, `collabShareChat`, `collabChangeShare`, `collabEnableSendMode`, `collabApproveGuestMessage`, `collabChangeRelay` | Collab thread | |
| Pairing | `pairingShowCode`, `pairingApproveDevice` | Pairing thread | |
| Easy Connect | `easyConnectEnable`, `easyConnectAllowList`, `easyConnectPorts`, `easyConnectRelay` | `security_gates.rs` | `/api/updateTailcatState`: turning it on, a new port, new devices on (or emptying) the allow-list (empty lets any device in), a relay change. Turning it off never asks. |
| Connected computers | `computerRegister`, `computerGrant`, `computerChangeAccess` | `security_gates.rs` | `/api/registerConnectedComputer`, `/api/issueComputerGrant` for a computer not approved in the last 30 days (renewals of an approved one pass; Agent access off never asks). |
| Autopilot | `autopilotAnswerApprovals`, `autopilotLiftLimits` | `security_gates.rs` | `/api/autopilot` `op: "set"` with `answerApprovals: true`, or `limits`. |
| GBots | `gbotsEnable`, `gbotsAddBot`, `gbotsChangeSenders` | `security_gates.rs` | `updateSettingsPatch` turning `gbotsEnabled` on or changing `gbotsComputer`; `/api/gbots` `action: "create"`. Who may message a bot is set inside the GBots sidecar, not through gxserver. |
| Settings | `settingsWeakenSecurity` (target: the keys) | `security_gates.rs`, desktop `owner_security/settings.rs` | One rule, `ghostex_settings_catalog::security_relaxing::needs_owner_presence`: a key on the Remote, Accounts, Secrets, Sync or Connectors page, or holding a token, password, secret, API key or credential. Through the API (`updateSettingsPatch`), a key the API may not write at all is refused by `settings_agent_writable.rs` before any approval; the rest (Sync's, GBots') ask. The desktop's own Settings window shows the system prompt (Touch ID / Windows Hello / polkit) before it saves such a change, and shows the saved values again when the prompt is declined. Collab's keys are left to the Collab routes. |
| Prompts | `answerOtherSessionPrompt` | `security_gates.rs` | `/api/answerSessionChatPrompt` by anyone but the owner's devices for a session that isn't the caller's own, a coordinator answering its own threads included (user decision 2; Autopilot keeps its own actions). |
| Terminal input | `typeIntoOtherSession` | `security_gates.rs` | `/api/sendSessionText` (send-text, send-key), `/api/sendSessionEnter`, `/api/sendSessionMessage` by anyone but the owner's devices into a session that isn't the caller's own, a coordinator's threads included (user decision). A coordinator's Escape or Ctrl-C to its own thread passes; chat messages through the chat queue (`ghostex agents send`) never ask. |
| Interrupting | `interruptOtherSession` | `security_gates.rs` | `/api/interruptSessionChat` by anyone but the owner's devices for a session that is neither the caller's own nor one of its threads (user decision). |
| Approvers | `addApproverKey` | `security_http.rs` | A second owner key, or any approver key after the first. |

Add a new action here, in the enum, and in the phone's TypeScript port together.

## The gate: `require_owner_presence`

In a route handler (`crate::server::require_owner_presence`):

```rust
let action = ActionRequest::new(SensitiveAction::CollabEnableSendMode, "Let guests send messages to Fix login without your approval")
    .target(session_id.clone())
    .params(Value::Object(params.clone()));      // the call's exact params; approvalId is dropped
let grant = match require_owner_presence(&state, &caller, action, &body_json, &endpoint.path, &request_id) {
    Ok(grant) => grant,
    Err(response) => return response,           // 403 approvalRequired / approvalDenied / forbidden
};
// …make the change…
state.security.record_performed(grant.approval.action, grant.approval.target, &caller, true);
```

- The summary and detail are written by gxserver from the action and params, in the card
  wording rules (what happens, in plain words). Never copy text from the caller into them: an
  agent must not write what the owner approves. Never put a secret in them.
- `params` must be the exact parameters the handler acts on. The approval is bound to their
  digest (SHA-256 of canonical JSON, keys sorted), the action and the target.
- The error body is the usual RPC error plus `approval` (an `ApprovalRequest`):
  `{ ok: false, error: "approvalRequired", message: "Waiting for your approval… repeat this call with approvalId \"apr_…\".", approval: {…} }`.
  The message names the id too, for clients whose transport keeps only code and message (the
  phone's).
- The caller repeats the same call with `params.approvalId = approval.id` once it is approved.
  Only the caller that asked (or the owner's devices) can spend it, once, within 2 minutes of the
  approval. A pending request lapses after 10 minutes. An identical pending request from the same
  caller is reused.
- Clients: the desktop app, getting `approvalRequired` for a call of its own (local, or a remote
  computer's through the tunnel), shows the system prompt at once with the summary, approves and
  sends the call again (never on the main thread). The `ghostex` CLI prints what it waits for on
  stderr, polls `/api/readApprovalRequest` and repeats the call (`--no-wait` returns at once).

## The approval and owner-key API

| Endpoint | Params | Who | Result |
| --- | --- | --- | --- |
| `/api/listApprovalRequests` | none | owner's devices | `{ requests }`, pending, oldest first |
| `/api/readApprovalRequest` | `{ id }` | its requester, owner's devices | `{ request }` |
| `/api/approveApprovalRequest` | `{ id, keyId, signature }` | owner's devices | `{ request }` (status `approved`) |
| `/api/denyApprovalRequest` | `{ id }` | owner's devices | `{ request }` (`denied`; no signature) |
| `/api/listApproverKeys` | none | owner's devices | `{ keys }` |
| `/api/enrollApproverKey` | `{ key }` | owner's devices | `{ key }`: the first from the owner session, later ones after an approved `addApproverKey` |
| `/api/removeApproverKey` | `{ keyId }` | owner's devices | `{ key }` |
| `/api/enrollOwnerKey` | `{ key }` | any local caller (loopback only) | `{ key }`: the first is trusted on first use (user decision 1A), the same again is a no-op, another needs `addApproverKey` |
| `/api/ownerChallenge` | none | any local caller (loopback only) | `{ challenge, expiresAtMs }`, single use, 60 s |
| `/api/openOwnerSession` | `{ keyId, challenge, signature }` | any local caller (loopback only) | `{ token, deviceId }` |
| `/api/securityStatus` | none | any but guests | `{ serverId, caller, ownerKeys, approverKeys, pendingApprovals }` |

Statuses: `pending`, `approved`, `denied`, `expired`, `consumed`. Requests live in memory; a
gxserver restart drops them and the agent asks again. `/api/events` carries
`{ type: "approvalRequestsChanged", change: "requested"|"approved"|"denied"|"expired", request }`.

## What is signed

`approval_signing_payload(server_id, &request)`: UTF-8, one field per line, `\n` line ends, no
trailing newline; a field containing a line break is refused.

```text
ghostex-approval-v1
<server id of the gxserver that made the request>
<approval id>
<nonce: 32 random bytes, base64url>
<action wire name>
<target, or an empty line>
<params digest, lowercase hex SHA-256>
<expiresAtMs of the pending request, decimal>
approve
```

`owner_session_signing_payload(server_id, challenge, key_id)`:
`ghostex-owner-session-v1\n<server id>\n<challenge>\n<key id>`.

The pinned examples are the tests in `approval.rs`; the phone's TypeScript port
(`src/security/approvalPayload.ts`) is tested against the same text.

## Keys

| Device | Approver key | Unlocked by | Owner key |
| --- | --- | --- | --- |
| macOS desktop | P-256 in the login Keychain (another program reads it only with the user's keychain password) | LocalAuthentication: Touch ID or the Mac password, before each signature | P-256 in the login Keychain, no prompt |
| Windows desktop | P-256 in Credential Manager | Windows Hello (`UserConsentVerifier`) before each signature | Credential Manager |
| Linux desktop | P-256 in the Secret Service | polkit password prompt (`pkcheck`, a fresh subject each time so the "keep" cache never applies) | Secret Service |
| iPhone | Secure Enclave P-256, `.userPresence` | Face ID / Touch ID / passcode, per signature | (pairing's device key) |
| Android | Android Keystore P-256 (StrongBox when present), user authentication per use | BiometricPrompt: strong biometric or screen lock | (pairing's device key) |

All `ecdsaP256Sha256`: public key = 65-byte uncompressed point, signature = ASN.1 DER, both base64
standard (`rsaPkcs1Sha256` is accepted too, for a later Windows Hello `KeyCredentialManager` key).
`keyId` = hex of the first 16 bytes of SHA-256 over the public key bytes; gxserver computes it.

The macOS approver key is not in the Secure Enclave: a persistent Secure Enclave key needs a
`keychain-access-groups` entitlement with a provisioning profile that the Developer ID app doesn't
ship. On Windows and Linux any program running as the user can read these key stores (Help says
so, user decision 1A).

Enrollment: the desktop enrolls its owner key on each gxserver it talks to the first time (this
computer's, and each connected remote computer's through its tunnel), opens an owner session,
and enrolls its approver key under that session the first time it approves something there. A
phone's approver key (and any later key) needs an approved `addApproverKey` from a key already
enrolled; the phone asks, the desktop's card approves it.

## Persistence

`gxserver/auth/security.json` (0600, written whole through a rename): `ownerKeys`,
`approverKeys`, `sessionTokens` (whole tokens, so a restarted session gets the same one; the file
is as readable to a same-user program as the token file), `ownerSessions` (hashes only),
`approvedComputers` (server id → when). An unreadable file is moved aside and gxserver starts with
none; the desktop then enrolls again on first use.

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
