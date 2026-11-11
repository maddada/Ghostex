//! The chat's view across devices: each view pick made here is handed to the computer that owns
//! the session, and a newer pick made on another device, which every chat read carries
//! (`chatView`), is taken in place of this device's.
//!
//! The four view records stay this device's own, read at boot and written when they change
//! (`composer/storage.rs`), so a chat opens in its view at once and keeps working while the
//! computer cannot be reached. Beside them the `viewSync` record keeps, per record, when its value
//! was picked and whether the computer has it ([`Stamp`]). A pick is stamped and handed over at
//! once; one the computer did not get is handed over again after the next read that reaches it.
//! A read whose pick of a record is newer than this device's replaces the value here. The newest
//! pick of each record wins, by the picking device's clock.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User: "btw we need to track the chat view the user set on each chat accross all devices so it's the same (doesn't need to sync instantly, some slight delay is ok)". The user's decision and the computer's half are in server/src/session_chat_view.rs.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! A computer older than the pick (its reads carry no `chatView`) is never asked to keep one, so
//! the chat works against it as before, with each device keeping its own view. A record chosen
//! before this existed has no stamp and is never handed over: it stays this device's until a pick
//! on any device replaces it.

use std::collections::BTreeMap;

pub use ghostex_gx_protocol::{ChatViewField, ChatViewPick};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::composer::storage::{
    encode_summary, encode_verbose, quiet_view_key, simple_key, summary_key, verbose_key,
    QUIET_VIEW_STORE, SIMPLE_STORE, SUMMARY_STORE, VERBOSE_STORE,
};
use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The records kept across devices: the chat's view level and its Quiet View choice.
pub const RECORDS: [&str; 4] = [SUMMARY_STORE, SIMPLE_STORE, VERBOSE_STORE, QUIET_VIEW_STORE];

/// When one record's value here was picked, and whether the computer has that pick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub kept: bool,
}

/// What the cross-device view remembers between events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewSyncState {
    /// The `viewSync` record: a stamp per record picked here or taken from the computer.
    pub stamps: BTreeMap<String, Stamp>,
    /// The session's computer keeps view picks (a read from it carried `chatView`).
    pub computer_keeps: bool,
    /// The pick being handed over: its request id and the time of each record it carries.
    pub push: Option<(u64, BTreeMap<String, i64>)>,
    /// The computer's pick from the newest read, taken on this event's settle.
    pub incoming: Option<ChatViewPick>,
}

/// The boot read's `viewSync` record.
pub fn adopt_boot(state: &mut ChatState, raw: &Value) {
    state.transcript_view.view_sync.stamps =
        serde_json::from_value(raw.clone()).unwrap_or_default();
}

/// A read (or a snapshot synthesized from one) carried the computer's pick.
pub fn read_carried(state: &mut ChatState, pick: Option<&ChatViewPick>) {
    if let Some(pick) = pick {
        let sync = &mut state.transcript_view.view_sync;
        sync.computer_keeps = true;
        sync.incoming = Some(pick.clone());
    }
}

/// The value a record holds here, the write in flight included.
fn current(state: &ChatState, record: &str) -> Option<bool> {
    let view = &state.transcript_view;
    match record {
        SUMMARY_STORE => Some(view.pending_summary_mode.unwrap_or(view.summary_mode)),
        SIMPLE_STORE => view.pending_simple_override.unwrap_or(view.simple_override),
        VERBOSE_STORE => view
            .pending_verbose_override
            .unwrap_or(view.verbose_override),
        QUIET_VIEW_STORE => view
            .pending_quiet_view_override
            .unwrap_or(view.quiet_view_override),
        _ => None,
    }
}

fn write_stamps(state: &ChatState) -> Effect {
    Effect::WriteStorage {
        key: crate::composer::storage::view_sync_key(&state.identity.session_key),
        value: Some(json!(state.transcript_view.view_sync.stamps).to_string()),
        durable: false,
    }
}

/// The view actions' writes are the user's picks: each record they write is stamped now and
/// handed to the computer.
pub fn picked(state: &mut ChatState, context: &ChatContext, effects: &[Effect]) -> Vec<Effect> {
    let records: Vec<&str> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::WriteStorage { key, .. } => {
                RECORDS.iter().copied().find(|record| *record == key.store)
            }
            _ => None,
        })
        .collect();
    if records.is_empty() {
        return Vec::new();
    }
    let at = context.now_millis();
    for record in records {
        state
            .transcript_view
            .view_sync
            .stamps
            .insert(record.to_string(), Stamp { at, kept: false });
    }
    let mut out = vec![write_stamps(state)];
    out.extend(push(state));
    out
}

/// Hands the computer every record it does not have yet, when it keeps picks and no hand-over is
/// in flight.
fn push(state: &mut ChatState) -> Vec<Effect> {
    let sync = &state.transcript_view.view_sync;
    if !sync.computer_keeps || sync.push.is_some() {
        return Vec::new();
    }
    let unkept: ChatViewPick = sync
        .stamps
        .iter()
        .filter(|(_, stamp)| !stamp.kept)
        .map(|(record, stamp)| {
            (
                record.clone(),
                ChatViewField {
                    value: current(state, record),
                    at: stamp.at,
                },
            )
        })
        .collect();
    if unkept.is_empty() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.transcript_view.view_sync.push = Some((
        request_id,
        unkept
            .iter()
            .map(|(record, field)| (record.clone(), field.at))
            .collect(),
    ));
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SetSessionChatView,
        params: Box::new(json!({
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "chatView": unkept,
        })),
    }]
}

/// Puts `value` in `record` here, as a pick on this device would, without stamping it.
fn set_value(state: &mut ChatState, record: &str, value: Option<bool>) -> Option<Effect> {
    let session_key = state.identity.session_key.clone();
    let view = &mut state.transcript_view;
    let (key, raw) = match record {
        SUMMARY_STORE => {
            let on = value.unwrap_or(false);
            if view.summary_mode == on {
                return None;
            }
            view.summary_mode = on;
            view.projection_inputs = None;
            (summary_key(&session_key), Some(encode_summary(on)))
        }
        SIMPLE_STORE => {
            if view.simple_override == value {
                return None;
            }
            view.simple_override = value;
            view.projection_inputs = None;
            (simple_key(&session_key), value.map(encode_verbose))
        }
        VERBOSE_STORE => {
            if view.verbose_override == value {
                return None;
            }
            view.verbose_override = value;
            view.projection_inputs = None;
            (verbose_key(&session_key), value.map(encode_verbose))
        }
        QUIET_VIEW_STORE => {
            if view.quiet_view_override == value {
                return None;
            }
            view.quiet_view_override = value;
            (quiet_view_key(&session_key), value.map(encode_summary))
        }
        _ => return None,
    };
    Some(Effect::WriteStorage {
        key,
        value: raw.map(str::to_string),
        durable: false,
    })
}

/// Takes the records of the computer's `pick` that are newer than this device's, and marks which
/// of this device's picks the computer has.
fn take(state: &mut ChatState, pick: &ChatViewPick) -> Vec<Effect> {
    let mut effects = Vec::new();
    let mut stamps_moved = false;
    let mut view_moved = false;
    for record in RECORDS {
        let stamp = state.transcript_view.view_sync.stamps.get(record).copied();
        let next = match (pick.get(record), stamp) {
            // Newer on the computer: the other device's pick replaces this one.
            (Some(field), _) if stamp.is_none_or(|stamp| field.at > stamp.at) => {
                if let Some(write) = set_value(state, record, field.value) {
                    effects.push(write);
                    view_moved = true;
                }
                Some(Stamp {
                    at: field.at,
                    kept: true,
                })
            }
            (Some(field), Some(stamp)) if field.at == stamp.at => Some(Stamp {
                kept: true,
                ..stamp
            }),
            // Newer here, or a pick the computer no longer has: hand it over again.
            (_, Some(stamp)) => Some(Stamp {
                kept: false,
                ..stamp
            }),
            (_, None) => None,
        };
        if let Some(next) = next.filter(|next| Some(*next) != stamp) {
            state
                .transcript_view
                .view_sync
                .stamps
                .insert(record.to_string(), next);
            stamps_moved = true;
        }
    }
    if view_moved {
        crate::transcript::quiet_view::sync(state);
        state.core.request_publish();
    }
    if stamps_moved {
        effects.push(write_stamps(state));
    }
    effects
}

/// Takes the pick the newest read carried, then hands over what the computer does not have.
pub fn settle(state: &mut ChatState) -> Vec<Effect> {
    let Some(pick) = state.transcript_view.view_sync.incoming.take() else {
        return Vec::new();
    };
    let mut effects = take(state, &pick);
    effects.extend(push(state));
    effects
}

/// The answer to a hand-over, or `None` when `request_id` is not one.
pub fn settle_rpc(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    if state
        .transcript_view
        .view_sync
        .push
        .as_ref()
        .is_none_or(|(id, _)| *id != request_id)
    {
        return None;
    }
    let (_, sent) = state.transcript_view.view_sync.push.take()?;
    // A refusal shows nothing: the pick stays this device's and goes again after the next read.
    state.core.claim_refusal();
    let RpcOutcome::Ok { result } = outcome else {
        return Some(Vec::new());
    };
    let mut effects = Vec::new();
    let mut moved = false;
    for (record, at) in sent {
        if let Some(stamp) = state.transcript_view.view_sync.stamps.get_mut(&record) {
            if stamp.at == at && !stamp.kept {
                stamp.kept = true;
                moved = true;
            }
        }
    }
    if moved {
        effects.push(write_stamps(state));
    }
    // The answer is the pick as kept, which may hold a newer one from another device.
    if let Some(kept) = result
        .get("chatView")
        .and_then(|value| serde_json::from_value::<ChatViewPick>(value.clone()).ok())
    {
        effects.extend(take(state, &kept));
    }
    // Picks made while this one was on its way.
    effects.extend(push(state));
    Some(effects)
}
