# Security: capability tokens, the owner key, signed approvals (GitHub #495, 11.0.1)

Read this before you touch a route that lets someone in from outside, or build on the pairing,
Collab or audit-log work of 11.0.1. The user's decisions are in
`~/.local/share/agent-work/Ghostex/issue-495/decisions.md`; this file is the contract in code.

Code: `server/src/security/` (gxserver), `server/src/server/security_http.rs` (the approval API
and the `require_owner_presence` helper), `packages/gx-protocol/src/approval.rs` (wire types and
the signed payload, shared by gxserver, the desktop and, as a TypeScript port, the phone).

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
| `Owner { device_id }` | `gxo_…`, in the desktop app's memory only | Minted after the app proves the owner key (Phase 2). |
| `Device { device_id }` | `gxd_…` today | A paired phone. Pairing v2 replaces the token with signed requests; until then `device_id` is `token:<first 12 hex of the hash>`. |
| `Session { project_id, session_id, role }` | `gxs_…`, in the session's `GHOSTEX_SESSION_TOKEN` | Minted at session start, revoked when it ends (Phase 2). `role`: `agent`, `coordinator`, `thread`. |
| `LocalAgent` | the token file `gxserver/auth/token` | Downgraded to agent level. The CLI in the user's own terminal uses it. |
| `Peer { server_id, name }` | a connected computer's grant | Keeps `PEER_ALLOWED_ENDPOINTS`. |
| `Guest { share_id }` | `gxshare_…` | Keeps `GUEST_ENDPOINTS`. |
| `Extension { extension_id }` | an extension's API token | Keeps the extension registry's scope. |
| `Anonymous` | none | Public routes only. |

Until Phase 2 mints session and owner tokens, the main token resolves to `LocalAgent`.

## What they may do: `Capability`

`Principal::has(capability)`. Owner and Device have every capability. Peers, guests, extensions
and anonymous callers have none of these (they keep their own endpoint allow-lists).

| Capability | Session agent/thread | Coordinator | LocalAgent |
| --- | --- | --- | --- |
| `OwnSession`, `ProjectSessions`, `AgentWritableSettings` | yes | yes | yes |
| `ManageOwnThreads` | no | yes | yes |
| `ManageAnyThreads` | no | no | yes |
| `OwnerSettings`, `AnswerAnyPrompt` (answer another session's permission prompt without asking) | no | no | no |
| `RequestSensitiveAction` (may ask) | yes | yes | yes |
| `DecideApprovals` (list, approve, deny, approver keys) | no | no | no |

`Principal::may_manage_thread_of(coordinator_session_id)` is the check for "coordinators manage
their own threads".

## How a route declares what it needs

`security::route_access(path) -> RouteAccess`:

- `Public`: no token.
- `Needs(Capability)`: the caller must hold it.
- `Presence(SensitiveAction)`: the whole call is one sensitive action; the gate asks for approval
  with the call's params before the handler runs (for example `/api/issueComputerGrant`).

A route that is only partly sensitive (`/api/autopilot` `op: "set"` turning on
`answerApprovals`, a settings patch with one security-relaxing key) keeps an ordinary access and
calls the gate from its handler for that part. Phase 1 lists only the security routes and lets
the rest default to `Needs(ProjectSessions)`; Phase 2 classifies every endpoint and adds a test
that fails on an unclassified one, and enforces `route_access` in the gate.

## Sensitive actions: `SensitiveAction`

Defined once in `ghostex_gx_protocol::approval::SensitiveAction` (wire names in camelCase):

| Group | Actions | Wired by |
| --- | --- | --- |
| Collab | `collabEnable`, `collabShareChat`, `collabChangeShare`, `collabEnableSendMode`, `collabApproveGuestMessage`, `collabChangeRelay` | Collab thread |
| Pairing | `pairingShowCode`, `pairingApproveDevice` | Pairing thread |
| Easy Connect | `easyConnectEnable`, `easyConnectAllowList`, `easyConnectPorts`, `easyConnectRelay` | core thread |
| Connected computers | `computerRegister`, `computerGrant`, `computerChangeAccess` | core thread |
| Autopilot | `autopilotAnswerApprovals`, `autopilotLiftLimits` | core thread |
| GBots | `gbotsEnable`, `gbotsAddBot`, `gbotsChangeSenders` | core thread |
| Settings | `settingsWeakenSecurity` (target: the setting key) | core thread |
| Prompts | `answerOtherSessionPrompt` (any agent answering another session's permission prompt, a coordinator answering its own threads included; user decision 2026-11-08. The owner's own devices answer without it; Autopilot keeps its own actions) | core thread |
| Approvers | `addApproverKey` | core thread |

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
  agent must not write what the owner approves.
- `params` must be the exact parameters the handler acts on. The approval is bound to their
  digest (SHA-256 of canonical JSON, keys sorted), the action and the target, so it can't be
  spent on another change.
- The error body is the usual RPC error plus `approval` (an `ApprovalRequest`):
  `{ ok: false, error: "approvalRequired", message: "Waiting for your approval…", approval: {…} }`.
- The caller repeats the same call with `params.approvalId = approval.id` once it is approved.
  Only the caller that asked (or the owner's devices) can spend it, once, within 2 minutes of
  the approval. A pending request lapses after 10 minutes. An identical pending request from the
  same caller is reused, not duplicated.
- Clients: the desktop app, getting `approvalRequired` for a click of its own, prompts for Touch
  ID / Windows Hello / polkit at once, approves and repeats the call. The `ghostex` CLI prints the
  message, waits on `/api/readApprovalRequest` and repeats the call when it's approved (Phase 2).

## The approval API

| Endpoint | Params | Who | Result |
| --- | --- | --- | --- |
| `/api/listApprovalRequests` | none | owner's devices | `{ requests }`, pending, oldest first |
| `/api/readApprovalRequest` | `{ id }` | its requester, owner's devices | `{ request }` |
| `/api/approveApprovalRequest` | `{ id, keyId, signature }` | owner's devices | `{ request }` (status `approved`) |
| `/api/denyApprovalRequest` | `{ id }` | owner's devices | `{ request }` (status `denied`; no signature needed) |
| `/api/listApproverKeys` | none | owner's devices | `{ keys }` |
| `/api/enrollApproverKey` | Phase 2 | first key, then `addApproverKey` approval | `{ key }` |
| `/api/removeApproverKey` | `{ keyId }` | owner's devices | `{ key }` |

Statuses: `pending`, `approved`, `denied`, `expired`, `consumed`. Requests live in memory; a
gxserver restart drops them and the agent asks again.

## What is signed

`ghostex_gx_protocol::approval::approval_signing_payload(server_id, &request)`: UTF-8, one field
per line, `\n` line ends, no trailing newline; a field containing a line break is refused.

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

The server id stops an approval being replayed on another computer; the nonce and id stop it
being replayed on another request. The pinned example is the test
`signing_payload_is_line_based_and_stable` in `approval.rs`; the TypeScript port must produce
the same bytes.

## Approver keys: one per approving device

| Device | Key | Unlocked by | `algorithm` |
| --- | --- | --- | --- |
| macOS desktop | Secure Enclave P-256, access control `.biometryCurrentSet` or `.userPresence` (password fallback), in the Keychain limited to Ghostex's signed app | Touch ID (LocalAuthentication) or the Mac password | `ecdsaP256Sha256` |
| Windows desktop | Windows Hello `KeyCredentialManager` key | Windows Hello (`UserConsentVerifier`) | `rsaPkcs1Sha256` |
| Linux desktop | P-256 key in the Secret Service | polkit check (system password/PIN) before each signature; weaker, any program running as the user can read the Secret Service | `ecdsaP256Sha256` |
| iPhone | Secure Enclave P-256 | Face ID / Touch ID | `ecdsaP256Sha256` |
| Android | Android Keystore (StrongBox when present) P-256, user authentication required per use, `BiometricPrompt` with a `CryptoObject` | fingerprint / face / device PIN | `ecdsaP256Sha256` |

Encodings: P-256 public key = 65-byte uncompressed point (X9.63; Android exports SPKI, strip
the 26-byte header), signature = ASN.1 DER. RSA public key = PKCS#1 `RSAPublicKey` DER
(`CryptographicPublicKeyBlobType.Pkcs1RsaPublicKey`), signature = PKCS#1 v1.5 SHA-256. Both in
base64 standard. `keyId` = hex of the first 16 bytes of SHA-256 over the public key bytes;
gxserver computes it, a client can't pick one.

Enrollment (Phase 2): the first desktop key is enrolled at first start of 11.0.1 (macOS: proved
through a Keychain item only Ghostex's signed gxserver and app can read; Windows and Linux: trust
on first use, said plainly in Help); every later key, including a phone's, needs an approved
`addApproverKey` signed by a key already enrolled. Pairing v2 folds the phone's key into the
device approval: approving `pairingApproveDevice` also enrolls the phone's approver key that came
with its pairing request, so the owner approves once.

## The owner key (desktop only)

Separate from the approver key: a non-biometric key in the OS key store (Keychain item limited to
Ghostex's signed app, Windows Credential Manager, Linux Secret Service) that the desktop app uses
to open its `gxo_` owner session at start. It is never written to a file or a session's
environment. An owner session holds the owner's capabilities but still needs a signed approval for
every sensitive action.

## Security events (for the audit log)

`state.security.subscribe()` returns a `tokio::sync::broadcast::Receiver<SecurityEvent>`; a slow
subscriber loses the oldest events (`Lagged`) and never blocks gxserver. Events (`type` tag,
camelCase): `approvalRequested`, `approvalApproved { keyId }`, `approvalDenied { by }`,
`approvalExpired`, `sensitiveActionAuthorized { action, target, approvalId, caller }` (the
approval was spent; the action is about to run), `sensitiveActionPerformed { action, target,
caller, ok }` (from `record_performed`, including always-allowed changes such as turning Easy
Connect off), `sessionTokenMinted`, `sessionTokenRevoked`, `approverKeyAdded`,
`approverKeyRemoved { by }`. The audit-log thread owns the hash-chained log and the notification
per event; the desktop and phone use `approvalRequested` to show the request at once.

## Phase 1 vs Phase 2

Phase 1 (committed): the types, the store, signature checking (real, through `ring`), the gate,
the approval API, the caller on every route, session/owner token registries (in memory, not yet
minted), tests. Nothing calls the gate yet, `route_access` is declared but not enforced, and the
main token still opens everything it opened before.

Phase 2: mint `gxs_` tokens at session start and revoke them at session end (persisted hashes),
move the CLI, hooks, coordinators and title jobs to them, downgrade the token file, enforce
`route_access`, the owner key and owner session, approver key enrollment and persistence, the
desktop approval UI with Touch ID / Windows Hello / polkit, the CLI's wait-and-retry, and the gate
in the Easy Connect, connected-computer, Autopilot, GBots and security-relaxing settings routes.
