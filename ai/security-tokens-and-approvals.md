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
| `Device { device_id }` | `gxd_…`, bound to the phone's device key | A paired phone; `device_id` is its row in Paired devices (pairing v2, see below). |
| `Session { project_id, session_id, role }` | `gxs_…`, in the session's `GHOSTEX_SESSION_TOKEN` | Issued when an agent or terminal starts (zmx and wmx), the same token again when that session starts again, revoked when the session is deleted (and by a 5-minute sweep for sessions deleted another way). `role` from the coordinator markers: `agent`, `coordinator`, `thread`; `head` is never stored (see "Chief's head role" below). The `ghostex` CLI inside a session sends it. |
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

| Capability | Session agent/thread | Coordinator | Chief (head) | LocalAgent |
| --- | --- | --- | --- | --- |
| `OwnSession`, `ProjectSessions`, `AgentWritableSettings` | yes | yes | yes | yes |
| `ManageOwnThreads` | no | yes | yes | yes |
| `ManageAnyThreads` | no | no | no | yes |
| `OwnerSettings` | no | no | no | no |
| `RequestSensitiveAction` (may ask) | yes | yes | yes | yes |
| `DecideApprovals` (list, approve, deny, approver keys) | no | no | no | no |
| `OwnerSecrets` (answer a secret request, have gxserver read a secret provider reference) | no | no | no | no |
| `OwnerSetup` (install extensions, secret providers, teams; see "Settings only") | no | no | no | no |

Secret requests are answered only by the owner: `/api/answerSecretRequest` and
`/api/checkSecretReference` need `OwnerSecrets`; `/api/setVoiceAgentKey` needs it when the key
is an `op://` reference, and `/api/cloudBoxes` `cloudKit` (the provider keys and box logins the
phone keeps) always (`security_gates.rs`). The answer handler checks it again before it reads
a reference. Agents, the token file (so also the GPUI web build), connected computers, guests and
extensions are refused; there is no approval to ask for. The other way round, only the asking
session itself (its own `gxs_` token) may create a secret request for its chat
(`/api/createSecretRequest`) and wait for the answer (`/api/waitSecretRequest`).

`SecurityState::skips_approval(caller, action)` (`server/src/security/policy.rs`) is the one rule
for which sensitive actions a caller skips; both gates read it (the route gate in
`security_gates.rs` and `require_owner_presence` for handlers that ask themselves). A skipped call
gets a `PresenceGrant` with `by_policy: true` and a stand-in approval (id `allowed`, status
`consumed`) that names the change and was never a request. The user's answers of 2026-11-10
(Q2098 to Q2105):

| Action(s) | Skipped for | Still asks |
| --- | --- | --- |
| `answerOtherSessionPrompt`, `typeIntoOtherSession`, `interruptOtherSession` | everyone (11.1.2; only Chief's are logged, as `allowed`) | nobody |
| `computerGrant`, `computerRegister`, `computerChangeAccess` | the owner's devices and the token file (your app's owner session, or the token file its SSH login reads on the other computer) | a session's own token, once per computer for 30 days |
| `easyConnectAllowList`, `easyConnectPorts`, `easyConnectRelay` | everyone | nobody |
| `easyConnectEnable` | everyone once it was approved (remembered for good, `rememberedApprovals.easyConnectEnable`), or when the allow-list already holds a device (an earlier setup counts) | the first time |
| `forceFullAccess`, `autopilotAnswerApprovals`, `autopilotLiftLimits`, `cloudBoxShareLogins`, `teamSyncConnect` | everyone; written to the security log as `allowed` | nobody |
| `gbotsEnable`, `gbotsAddBot`, `gbotsApiKey`, `gbotsSetComputer` | the owner's devices | the token file and agents (where `/api/gbots` doesn't refuse them) |
| `collabEnableSendMode`, `collabChangeShare`, `collabShareChat` showing an existing share's link again | the owner's devices | agents; a new share always asks |
| `pairingApproveDevice`, `addApproverKey`, `settingsWeakenSecurity` (narrowed, see Settings below), `installExtensionComponent`, `collabApproveGuestMessage` | nobody | as before |

Q2100 settled as the user's option C (Q2109, Q2110): a phone that signs in over SSH works at once
with everything the token file allows (`LocalAgent`: reading, replying, Stop, typing, starting
sessions), and only its owner powers (approving and denying requests, enrolling its approver key,
answering secret requests, installing extensions, secret providers and the rest of
`DecideApprovals`, `OwnerSecrets`, `OwnerSetup`) wait for the one-time `pairingApproveDevice`
with the three words. `ghostex server endpoint` says `sshPhoneWorksUnpaired`, and a phone that
sees it shows the waiting request as one small line it can hide instead of a card. Pairing gives a
token-file holder nothing until the owner approves, so an agent posing as a phone gains nothing. Q2099 is the desktop's: it asks a
connected computer that has its own owner once, at connect (`owner_security::cached_token`).

### Settings only: `OwnerSetup` (11.1.1, #495 follow-up)

The user's rule: agents explain owner-only things and point to Settings, but can't turn them on.
These calls are the owner's devices' alone; agents, coordinators, threads, Chief, the token file
(so also the ghostex CLI in the user's own terminal and the GPUI web build) and connected
computers are refused with a message naming the Settings page
(`server/src/server/security_owner_only.rs`). There is no approval to ask for.

| Route | Owner-only when | Settings page |
| --- | --- | --- |
| `/api/installExtension` | always: a local folder, a catalog id, a zip URL | Extensions |
| `/api/updateExtensionState` | it turns an extension on, or grants a permission it doesn't hold | Extensions |
| `/api/saveSecretProvider`, `/api/removeSecretProvider` | always | Secrets |
| `/api/joinTeamSync`, `/api/createTeamSyncInvite`, `/api/setTeamLinearKey`, `/api/setOwnLinearKey`, `/api/setSlackFlowSettings` | always (leaving a team, and linking your Slack user with the one-time code Slack sends you, stay open) | Workspaces |
| `/api/updateAgentSettings` | it turns `agentAcceptAllEnabled` on | Agents |
| `updateSettingsPatch` | a key the catalog doesn't mark `agentWritable` (`settings_agent_writable.rs`) | the key's page |

The settings keys that loosen security are off the agent-writable list
(`USER_ONLY_KEYS` in `packages/settings-catalog/src/help/mod.rs`): `agentAcceptAllEnabled`,
`workspacesHidden`, `cloudBoxesPhoneStartEnabled`, `trustProjectFoldersAutomatically`,
`connectorsHidden`, `customSessionTitleGenerationCommand`, `globalWorktreeCommand`,
`customDefaultEditorCommand`, `terminalClipboardPasteProtection`, `botAutomationsHidden`.
GBots and Sync keys stay writable and ask (`settingsWeakenSecurity`, `gbotsEnable`).

The owner installs an extension from a folder with Settings > Extensions > **Install from
folder…** (`tabs/extensions/folder_install.rs`): a folder picker, the extension's name and the
permissions it asks for, each a tick box starting unticked. An approval grants only the
permissions it lists (a left-out one is not granted, not refused); a background process, a
remote website or another version it didn't show is still refused (`check_install_approval`).

Extensions besides the routes: an install without the consent dialog's approval gets no
permissions (`store_entry_for_install`), and a payload that wasn't installed through Ghostex (no
store entry) is never started, served or given state; it lists as off and can only be
uninstalled (`extensions/registry.rs`).

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

A route that is only partly sensitive is gated in `security_gates.rs` for that part, or from its
handler with `require_owner_presence` (that is how the pairing and Collab routes do it).

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

## Sensitive actions: `SensitiveAction`

Defined once in `ghostex_gx_protocol::approval::SensitiveAction` (wire names in camelCase). "Asks
when" says when the request is made; whether the caller is asked at all is the policy above:

| Group | Actions | Wired by | Asks when |
| --- | --- | --- | --- |
| Collab | `collabShareChat`, `collabChangeShare`, `collabEnableSendMode`, `collabApproveGuestMessage` | The Collab component asks (server/collab/src/owner.rs), gxserver gates (`collab_shim/http.rs`) | `/api/collab` `op: share` (also to see a link again), `setMode` to Send or from View only, `resolve` with `send`. `collabEnable` and `collabChangeRelay` are no longer asked: Collab is on while installed, and the relay is never settable. |
| Extensions | `installExtensionComponent` (target: the component, `collab` today) | `collab_shim/http.rs` | `/api/extensionComponents` `op: install`: a component that opens a way into this computer. Updating it, turning it off and uninstalling never ask. |
| Pairing | `pairingShowCode`, `pairingApproveDevice` | Pairing thread | |
| Easy Connect | `easyConnectEnable`, `easyConnectAllowList`, `easyConnectPorts`, `easyConnectRelay` | `security_gates.rs` | `/api/updateTailcatState`: turning it on, a new port, new devices on (or emptying) the allow-list (empty lets any device in), a relay change. Turning it off never asks. |
| Connected computers | `computerGrant` (`computerRegister`, `computerChangeAccess` are no longer asked) | `security_gates.rs` | `/api/registerConnectedComputer`, `/api/issueComputerGrant` with Agent access on, for a computer not approved in the last 30 days: both routes ask one shared request per computer (any caller reuses and may spend it; it waits a day), and a call that meets it already waiting is answered `approvalPending` instead of `approvalRequired`, so no app prompts for it again. Renewals of an approved computer pass; either route with `agentAccess: false` never asks, nor does registering a computer with `waitingOn: "here" | "there"` (the desktop's note that the pair still waits: no address, no token; `listConnectedComputers` answers `waitingForApproval` with what to approve and where, and the relay, `ghostex computers` and every `--computer` verb say it). The desktop's renewal never prompts: it shows one card and waits on the request (`remote_conn/connected_computers.rs`). |
| Autopilot | `autopilotAnswerApprovals`, `autopilotLiftLimits` | `security_gates.rs` | `/api/autopilot` `op: "set"` with `answerApprovals: true`, or `limits`. |
| GBots | `gbotsEnable`, `gbotsAddBot` | `security_gates.rs` | `updateSettingsPatch` turning `gbotsEnabled` on; `/api/gbots` `op: "create"`. |
| GBot API | `gbotsApiKey` (target: the GBot's id) | `security_gates.rs` `gbots_api_key_action` | `/api/gbots` `op: "apiOn"`, `"apiNewKey"`, and `"apiShareMemory"` with `on: true`, from the owner's devices and the token file only (every agent, Chief included, is refused by `/api/gbots` without being asked). The detail names what the key reaches: messaging the GBot, its replies, what it knows (its memory only when shared) and its routines by name. The params bind the label and the SHA-256 of the caller's one-time `keyNonce`, so the key goes only to the call that asked: another program reading the pending request can't repeat it. `apiOff` and sharing off never ask. Each change is a security-log entry (`gbots.apiOn`, `gbots.apiNewKey`, `gbots.apiOff`, `gbots.apiShareMemory`). |
| GBot Cloud Box chats | `cloudBoxShareLogins` (target: the box) | `security_gates.rs` `gbots_cloud_box_action` | `/api/gbots` `op: "newChat"` or `"ask"` for a GBot that works on a Cloud Box, by anyone but the owner's devices: the box gets the agent logins and project files set up for boxes, and the GBot's instructions and memory. |
| Settings | `settingsWeakenSecurity` (target: the keys) | `security_gates.rs`, desktop `owner_security/settings.rs` | One rule, `ghostex_settings_catalog::security_relaxing::needs_owner_presence`: a key on the Accounts, Secrets or Connectors page, one of the Remote page's ways in (`sshAccess`, `easyConnectEnabled`, `pairingCode`, `pairedDevices`, `pairedDevicesOnly`, `tailscaleEnabled`, `remoteTailscaleEnabled`), `homeSecretFolders`, or a key holding a token, password, secret, API key or credential (Q2102, 2026-11-10: Sync and the other Remote rows no longer ask). The desktop's Settings window uses `own_save_needs_presence`, the same rule without `gbotsEnabled` (Q2105). Through the API (`updateSettingsPatch`), a key the API may not write at all is refused by `settings_agent_writable.rs` before any approval; the rest (Sync's, GBots') ask. The desktop's own Settings window shows the system prompt (Touch ID / Windows Hello / polkit) before it saves such a change, and shows the saved values again when the prompt is declined. Collab's keys are left to the Collab routes. |
| Approvers | `addApproverKey` | `security_http.rs` | A second owner key, or any approver key after the first. |
| Full access | `forceFullAccess` | `security_full_access.rs` | A create with `launchSettings.acceptAllMode: "enabled"` when neither "Agent approvals: accept all" nor the agent's own launcher setting is on, in any project, whoever but the owner's devices calls (Chief included). Every time. The same for `/api/updateSession` turning a session's `acceptAllMode` on (a resume reads it); leaving it on or turning it off never asks. |
| Teams | `teamSyncConnect` (target: the backend's address) | `security_gates.rs` | `/api/connectTeamSync` by anyone but the owner's devices. It is `ghostex team deploy`'s last step, which Settings > Workspaces sends the owner to a terminal for, so it asks instead of being refused; the rest of team setup is Settings only. |
| Cloud Boxes | `cloudBoxShareLogins` | `security_gates.rs` | `/api/cloudBoxes` `create`, `setAgentLogin` with `allowed: true`, `projectSettings` with `settings`, by anyone but the owner's devices: they put the owner's agent logins and a project's secret files onto a box. Turning a login off and reading never ask. `connectInfo` is unchanged. |

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
- A person's own action in a chat (Stop, answering a prompt, typing into a session) sends
  `params.finishWhenApproved: true` (desktop and web `native_chat/rpc.rs`) for a connected
  computer still on gxserver 11.1.0 or 11.1.1, which asked about these; a later gxserver never
  asks for them and ignores the flag.
- The desktop's Stop, prompt answers and typing on a connected computer's sessions go as its owner
  session there, which skips these approvals. Until it has one (another owner is enrolled there and
  has not approved this app's owner key: one `addApproverKey` request that waits a day), the
  desktop doesn't send them and the chat says what to approve and where
  (`owner_security::steer_needs_owner`, code `ownerApprovalNeeded`): never a request per action.

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
| `/api/enrollOwnerKey` | `{ key, approverKey?, askOwner? }` | any local caller (loopback only); the first key only from the token file (the Ghostex app here, or another computer's app through its SSH tunnel), never a session's token | `{ key }`: the first is trusted on first use (user decision 1A), the same again is a no-op, another needs `addApproverKey`. Any program running as this OS user can read the token file, so it can still enroll first. |
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
| Windows desktop | P-256 in Credential Manager | Windows Hello (`UserConsentVerifier`) before each signature; where Windows Hello isn't set up, the Windows sign-in prompt for the signed-in account, accepted only with that account's correct password (`LogonUserW`, same SID as Ghostex), presence `windowsPassword` (GitHub #520) | Credential Manager |
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
and enrolls its approver key under that session the first time it approves something there.
It sends its approver key with its owner key (`approverKey`): with the first owner key it
goes in at once while no approver key is enrolled; on a computer that already has an owner, the
one `addApproverKey` request ("Let <computer> (Ghostex app) act as the owner of Ghostex on this
computer and answer its requests") enrolls both. A key the owner approves goes in the moment it
is approved (`security::ApprovalEffect`), so the asking call coming back with `approvalId` (the
desktop's, or the 11.2.1 phone's `ensureEnrolled`) finds it enrolled and gets it back; the
request stays `approved` until it lapses, as before.

The desktop shows a connected computer's request with Approve only where it can answer it (an
owner session there and its approver key enrolled, or none enrolled yet,
`owner_security::answerer`); elsewhere it shows a notice naming the devices that can approve it
there, and asks that computer once to let it act as the owner there. A request made by another
computer's app through its owner session (`requestedBy.kind: "owner"` with another `deviceId`)
shows as a card; only the app's own requests don't. A
phone's approver key (and any later key) needs an approved `addApproverKey` from a key already
enrolled; the phone asks, the desktop's card approves it.

## Persistence

`gxserver/auth/security.json` (0600, written whole through a rename): `ownerKeys`,
`approverKeys`, `sessionTokens` (whole tokens, so a restarted session gets the same one; the file
is as readable to a same-user program as the token file), `ownerSessions` (hashes only),
`approvedComputers` (server id → when), `rememberedApprovals` (key → when: approvals asked once
and kept for good, today `easyConnectEnable`; older gxservers ignore it), `approvedProjectReach` (`{ caller, target }`: an agent session and a project it may reach). An unreadable file is moved aside and gxserver starts with
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
