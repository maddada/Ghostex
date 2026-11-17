//! Plain approvals (option B): a gxserver that says `plainApprovals` takes an Approve as one click
//! from this app (its token file is one of your devices there), with no owner key, approver key or
//! signature. When that computer's "Require Touch ID or your password to approve" switch is on
//! (`requirePresence`), this app asks for Touch ID, Windows Hello or the password first, then sends
//! the same plain approval.
//!
//! CDXC:Security 2026-11-10 DECISION:
//! User (Q2140 = B): "thinking of removing all these dumb blocks because they're adding friction and a lot of bugs", and "add a toggle to require touch id/pin etc for access but disabled by default, add it to security". The rule is gxserver's (server/src/security/policy.rs); this app reads `/api/securityStatus` per computer and approves plainly where it says `plainApprovals`, and with the signed approval of `mod.rs` on a gxserver from before it (11.2.2 and earlier).
//! SEE-ALSO: server/src/server/security_http.rs (`plainApprovals`, `requirePresence`), apps/mobile/app/src/security/approvals.ts (the phone's same choice).

use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::{Duration, Instant},
};

use ghostex_gx_protocol::approval::ApprovalRequest;
use serde_json::{Value, json};

use super::{Transport, in_security_flow, rpc};

/// How long a computer's answer counts before it is read again.
const STATUS_FRESH: Duration = Duration::from_secs(30);

/// What a computer's gxserver says about approving.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ServerApprovals {
    /// One click approves; no signature (`plainApprovals`).
    pub(crate) plain: bool,
    /// Settings > Security there: ask for Touch ID or the password first (`requirePresence`).
    pub(crate) require_presence: bool,
    /// What this app sent may make codes and approve devices there (`mayApproveDevices`; a
    /// gxserver from before the owner-only door doesn't say, and lets it).
    pub(crate) may_approve_devices: bool,
}

static STATUS: Mutex<Option<HashMap<String, (Instant, ServerApprovals)>>> = Mutex::new(None);

fn with_status<R>(run: impl FnOnce(&mut HashMap<String, (Instant, ServerApprovals)>) -> R) -> R {
    let mut status = STATUS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    run(status.get_or_insert_with(HashMap::new))
}

/// What `transport`'s gxserver says, read now (or kept from the last 30 seconds). Blocks: never on
/// the main thread. `None` when it doesn't answer.
pub(crate) fn server_approvals(transport: &Transport) -> Option<ServerApprovals> {
    let key = transport.key();
    if let Some(fresh) = with_status(|status| {
        status
            .get(&key)
            .filter(|(at, _)| at.elapsed() < STATUS_FRESH)
            .map(|(_, answer)| *answer)
    }) {
        return Some(fresh);
    }
    let answer = in_security_flow(|| rpc(transport, "/api/securityStatus", json!({}))).ok()?;
    let read = ServerApprovals {
        plain: answer.get("plainApprovals").and_then(Value::as_bool) == Some(true),
        require_presence: answer.get("requirePresence").and_then(Value::as_bool) == Some(true),
        may_approve_devices: answer.get("mayApproveDevices").and_then(Value::as_bool)
            != Some(false),
    };
    with_status(|status| status.insert(key, (Instant::now(), read)));
    Some(read)
}

/// The last answer read for `transport`, without a call (for the main thread).
pub(crate) fn known_approvals(transport: &Transport) -> Option<ServerApprovals> {
    with_status(|status| status.get(&transport.key()).map(|(_, answer)| *answer))
}

/// Whether `transport`'s gxserver takes plain approvals (reads it when not known yet).
pub(crate) fn is_plain(transport: &Transport) -> bool {
    server_approvals(transport).is_some_and(|answer| answer.plain)
}

/// Whether Approve on a card for `transport` asks for Touch ID or the password first: where the
/// switch is on, and on a gxserver from before plain approvals (whose signature needs it).
pub(crate) fn asks_presence_first(transport: &Transport) -> bool {
    match known_approvals(transport) {
        Some(answer) => !answer.plain || answer.require_presence,
        None => match transport {
            // This computer's gxserver is this app's own version: its switch is in our settings.
            Transport::Local => this_computer_requires_presence(),
            Transport::Remote(_) => true,
        },
    }
}

/// This computer's own Settings > Security switch: Touch ID, Windows Hello or the password before
/// an Approve here, and before this app uses its own keys (`approvalsRequirePresence`).
pub(crate) fn this_computer_requires_presence() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("approvalsRequirePresence")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// What Touch ID, Windows Hello or the password prompt says for a request (macOS shows "Ghostex
/// is trying to <reason>."): its summary without the parts meant for matching on a card, the
/// server id ("(Ghostex server S1u)") and the key words ("(key words: …)"), and starting
/// lower-case (CDXC:Security 2026-11-17 at `super::machine_and_host`).
pub(crate) fn presence_reason(summary: &str) -> String {
    let mut reason = summary.trim().trim_end_matches('.').to_string();
    for marker in [" (Ghostex server ", " (key words: "] {
        while let Some(start) = reason.find(marker) {
            let end = reason[start..]
                .find(')')
                .map_or(reason.len(), |close| start + close + 1);
            reason.replace_range(start..end, "");
        }
    }
    let mut chars = reason.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => "approve a request".to_string(),
    }
}

/// What the system prompt says for `request` on `transport`: the card's own question where the
/// app words it ("let m7-arch connect to M7-Desktop", CDXC:Security 2026-11-17 DECISION in
/// packages/gx-protocol/src/plain_ask.rs), else gxserver's sentence cleaned up ([`presence_reason`]).
pub(crate) fn prompt_reason(transport: &Transport, request: &ApprovalRequest) -> String {
    let place = match transport {
        Transport::Local => crate::app::window::native_modal_kit::this_computer_place().to_string(),
        Transport::Remote(target) => {
            crate::app::helpers::gpui_remote_machine_name_from_settings(&target.machine_id)
                .unwrap_or_else(|| "that computer".to_string())
        }
    };
    ghostex_gx_protocol::plain_ask::plain_ask(request, &place)
        .map(|ask| ask.prompt_reason())
        .unwrap_or_else(|| presence_reason(&request.summary))
}

/// Approves `request` on `transport` with one click: the system prompt first when that computer
/// asks for it (`require_presence`), then `{ id }`.
pub(crate) fn approve_plain(
    transport: &Transport,
    request: &ApprovalRequest,
    require_presence: bool,
) -> Result<(), String> {
    if require_presence {
        ghostex_owner_keys::confirm_presence(&prompt_reason(transport, request))
            .map_err(|error| error.to_string())?;
    }
    let approve = || {
        in_security_flow(|| {
            rpc(
                transport,
                "/api/approveApprovalRequest",
                json!({ "id": request.id }),
            )
        })
        .map(|_| ())
        .map_err(|(_, message)| message)
    };
    // A new device, a terminal's pairing code or another owner: gxserver takes it only from this
    // app's owner session once that computer has an owner device (CDXC:Security 2026-11-16 in
    // `super::is_door_call`), on this computer and on a connected one.
    if opens_door(&request.action) {
        return super::door_call(transport, approve);
    }
    approve()
}

/// The requests gxserver lets only the owner session or a paired phone approve once this computer
/// has one (`opens_door` in server/src/security/policy.rs).
pub(crate) fn opens_door(action: &ghostex_gx_protocol::approval::SensitiveAction) -> bool {
    use ghostex_gx_protocol::approval::SensitiveAction;
    matches!(
        action,
        SensitiveAction::PairingApproveDevice
            | SensitiveAction::PairingShowCode
            | SensitiveAction::AddApproverKey
    )
}

/*
CDXC:Security 2026-11-10 WHY:
Gregg's report (gregg-thread-1110): a Mac restored from another Mac's backup (Migration Assistant copies the login Keychain) holds the same owner key, so "requested by this app's device id" hid the other Mac's requests on both, and nobody saw the card. A card is now hidden only for a request this app process itself got back from one of its own calls (its own click, which it approves where it happens, or its background renewal, which shows its own card), never by a key's device id.
*/
static OWN_REQUESTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// A request this app got back from one of its own calls: its card is the app's to show.
pub(crate) fn note_own_request(id: &str) {
    OWN_REQUESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_or_insert_with(HashSet::new)
        .insert(id.to_string());
}

/// Whether request `id` came back from one of this app's own calls.
pub(crate) fn is_own_request(id: &str) -> bool {
    OWN_REQUESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .is_some_and(|ids| ids.contains(id))
}
