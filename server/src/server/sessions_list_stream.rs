//! The phone's pushed sessions list: `/api/events?stream=sessionsList`.
//!
//! CDXC:StateSync 2026-11-12 DECISION:
//! The user chose to push the phone's sessions list over its link instead of the 5 s SSH poll ("push the sessions list over the existing socket"), adding "let's please not break things on older versions at all, so we might need to keep both". Measured on a test computer with 32 sessions: the poll moved about 460 KiB a minute while nothing changed, 99% of an idle phone's traffic. So a socket opened on this stream gets the whole mobile summary once (the value `ghostex sessions --json --mobile-summary` prints, built by the same code), then only what changed: top-level fields set or removed, sessions added, changed or removed, and the order when it moved. A phone opens it only when `ghostex server endpoint` lists `sessionsListStream`; every other phone, and this phone against an older computer, keeps the SSH poll unchanged.
//!
//! CDXC:StateSync 2026-11-12 WHY:
//! While an agent works the summary changes every one to two seconds (`gitStatus` of every session in its project, `lastInteractionAt`), so pushing the whole list on each change cost more than the poll (564 KB in 40 s against 47 KB as patches). Patches leave at most every [`MIN_PUSH_GAP_MS`]. The summary is rebuilt when gxserver's database changed (the CLI memo's stamp, summary_memo.rs) and at least every [`REBUILD_MS`] for what is worked out at read time without being stored (time-based activity, cached git status), so the list is never staler than the poll it replaces. A quiet list still sends a small `sessionsListAlive` every [`ALIVE_MS`], so the phone can tell a quiet list from a socket iOS suspended.
//! SEE-ALSO: `GXSERVER_EVENTS_STREAM_SESSIONS_LIST` in packages/gx-protocol/src/lib.rs, `ENDPOINT_CAPABILITIES` in server/src/cli.rs, apps/mobile/app/src/inventory/summaryStream.ts (the phone's twin, which applies the patches).

use std::time::Duration;

use serde_json::{json, Map, Value};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::events::EventClientSender;
use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::sessions::{database_stamp, mobile_summary_value};
use crate::server::AppState;
use std::sync::Arc;

const CHECK_MS: u64 = 1_000;
const REBUILD_MS: u64 = 5_000;
const MIN_PUSH_GAP_MS: u64 = 2_000;
const ALIVE_MS: u64 = 20_000;

pub(crate) const SUMMARY_FRAME: &str = "sessionsListSummary";
pub(crate) const PATCH_FRAME: &str = "sessionsListPatch";
pub(crate) const ALIVE_FRAME: &str = "sessionsListAlive";
/// CDXC:StateSync 2026-11-12 DECISION:
/// The user asked to fold the phone's Cloud Boxes list into the same push ("ride the same push, same capability rules"): the phone asked every computer `POST /api/cloudBoxes {action: list}` every 5 s, which became its most frequent call once the sessions list was pushed. The stream sends that action's answer whenever it differs from the last one sent (at the summary's cadence), and a phone takes it only from a computer that lists `cloudBoxesPush`; others keep the poll.
pub(crate) const CLOUD_BOXES_FRAME: &str = "cloudBoxesList";
/// The one client message: build the list now and answer with its patch (or an alive frame) carrying
/// the same `requestId`, for a phone that needs a list read after something it just did.
pub(crate) const READ_MESSAGE: &str = "readSessionsList";

/// One socket's list stream; dropping it stops the task.
pub(crate) struct SessionsListStream {
    task: JoinHandle<()>,
    reads: mpsc::UnboundedSender<Value>,
}

impl SessionsListStream {
    pub(crate) fn start(state: Arc<AppState>, outbound: EventClientSender) -> Self {
        let (reads, read_rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(state, outbound, read_rx));
        Self { task, reads }
    }

    /// Handles a client message on this stream; anything but [`READ_MESSAGE`] is ignored.
    pub(crate) fn client_message(&self, message: &Value) {
        if message.get("type").and_then(Value::as_str) != Some(READ_MESSAGE) {
            return;
        }
        let request_id = message.get("requestId").cloned().unwrap_or(Value::Null);
        let _ = self.reads.send(request_id);
    }
}

impl Drop for SessionsListStream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn build() -> Option<Map<String, Value>> {
    tokio::task::spawn_blocking(|| {
        let flags = parse_args(&[]).flags;
        match mobile_summary_value(&flags, false) {
            Ok(Value::Object(mut map)) => {
                keep_read_git_fields(&mut map);
                Some(map)
            }
            _ => None,
        }
    })
    .await
    .ok()
    .flatten()
}

/// CDXC:StateSync 2026-11-12 WHY:
/// A session's `gitStatus` on the stream keeps only `branch`, the one field the phone reads from it (parseSession.ts). Its line counts and `updatedAt` change with every file an agent writes and once a minute for every session of a project, which was most of the list's changes (a 27 KB patch of every row each minute). The SSH command still prints the whole `gitStatus`; a phone that wants more asks for a new capability.
fn keep_read_git_fields(summary: &mut Map<String, Value>) {
    let Some(Value::Array(sessions)) = summary.get_mut("sessions") else {
        return;
    };
    for session in sessions {
        let Some(Value::Object(git)) = session.get_mut("gitStatus") else {
            continue;
        };
        git.retain(|key, _| key == "branch");
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// The Cloud Boxes list as `POST /api/cloudBoxes {action: list}` answers it, or None when it fails.
async fn cloud_boxes(state: &Arc<AppState>) -> Option<Value> {
    let mut params = Map::new();
    params.insert("action".into(), json!("list"));
    crate::cloud_boxes::endpoint::dispatch(state.clone(), &params)
        .await
        .ok()
}

async fn run(
    state: Arc<AppState>,
    outbound: EventClientSender,
    mut reads: mpsc::UnboundedReceiver<Value>,
) {
    let mut sent: Option<Map<String, Value>> = None;
    let mut sent_boxes: Option<Value> = None;
    let mut seq: u64 = 0;
    let mut built_stamp = String::new();
    let mut built_at = 0u64;
    let mut pushed_at = 0u64;
    let mut sent_at = 0u64;
    let mut ticker = tokio::time::interval(Duration::from_millis(CHECK_MS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let request_id = tokio::select! {
            _ = ticker.tick() => None,
            read = reads.recv() => match read {
                Some(id) => Some(id),
                None => return,
            },
        };
        let now = now_ms();
        let stamp = database_stamp();
        let due =
            sent.is_none() || stamp != built_stamp || now.saturating_sub(built_at) >= REBUILD_MS;
        let may_push = request_id.is_some() || now.saturating_sub(pushed_at) >= MIN_PUSH_GAP_MS;
        let mut frame: Option<Value> = None;
        if (due && may_push) || request_id.is_some() {
            if let Some(boxes) = cloud_boxes(&state).await {
                if sent_boxes.as_ref() != Some(&boxes) {
                    let frame = json!({ "type": CLOUD_BOXES_FRAME, "result": boxes.clone() });
                    if outbound.try_send(frame).is_err() {
                        return;
                    }
                    sent_boxes = Some(boxes);
                }
            }
            if let Some(next) = build().await {
                built_stamp = stamp;
                built_at = now;
                frame = match &sent {
                    None => Some(summary_frame(seq + 1, &next)),
                    Some(previous) => match list_patch(previous, &next) {
                        PatchOutcome::Unchanged => None,
                        PatchOutcome::Patch(mut patch) => {
                            patch.insert("type".into(), json!(PATCH_FRAME));
                            patch.insert("seq".into(), json!(seq + 1));
                            Some(Value::Object(patch))
                        }
                        PatchOutcome::Whole => Some(summary_frame(seq + 1, &next)),
                    },
                };
                if frame.is_some() {
                    seq += 1;
                    pushed_at = now;
                    sent = Some(next);
                }
            }
        }
        if sent.is_none() {
            // Nothing to say until the first list is built; a read waits for the next tick.
            continue;
        }
        let quiet_too_long = now.saturating_sub(sent_at) >= ALIVE_MS;
        let frame = match frame {
            Some(frame) => Some(frame),
            None if request_id.is_some() || quiet_too_long => {
                Some(json!({ "type": ALIVE_FRAME, "seq": seq }))
            }
            None => None,
        };
        let Some(mut frame) = frame else { continue };
        if let (Some(id), Some(map)) = (request_id, frame.as_object_mut()) {
            if !id.is_null() {
                map.insert("requestId".into(), id);
            }
        }
        if outbound.try_send(frame).is_err() {
            return;
        }
        sent_at = now;
    }
}

fn summary_frame(seq: u64, summary: &Map<String, Value>) -> Value {
    json!({ "type": SUMMARY_FRAME, "seq": seq, "summary": Value::Object(summary.clone()) })
}

#[derive(Debug, PartialEq)]
pub(crate) enum PatchOutcome {
    Unchanged,
    Patch(Map<String, Value>),
    /// The sessions cannot be keyed (a row without ids, or two rows with the same ids): send it whole.
    Whole,
}

/// A session's key on the wire: `[projectId, sessionId]`.
fn session_key(session: &Value) -> Option<(String, String)> {
    let project = session.get("projectId")?.as_str()?.to_string();
    let id = session.get("sessionId")?.as_str()?.to_string();
    Some((project, id))
}

fn keyed(sessions: Option<&Value>) -> Option<Vec<((String, String), &Value)>> {
    let list = match sessions {
        None => return Some(Vec::new()),
        Some(Value::Array(list)) => list,
        Some(_) => return None,
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(list.len());
    for session in list {
        let key = session_key(session)?;
        if !seen.insert(key.clone()) {
            return None;
        }
        out.push((key, session));
    }
    Some(out)
}

/// What changed from `previous` to `next`, ignoring `revision` (a new id on every build).
pub(crate) fn list_patch(previous: &Map<String, Value>, next: &Map<String, Value>) -> PatchOutcome {
    let (Some(old_sessions), Some(new_sessions)) =
        (keyed(previous.get("sessions")), keyed(next.get("sessions")))
    else {
        return PatchOutcome::Whole;
    };
    let mut patch = Map::new();
    let mut set = Map::new();
    for (key, value) in next {
        if key == "sessions" || key == "revision" {
            continue;
        }
        if previous.get(key) != Some(value) {
            set.insert(key.clone(), value.clone());
        }
    }
    let unset: Vec<Value> = previous
        .keys()
        .filter(|key| *key != "sessions" && *key != "revision" && !next.contains_key(*key))
        .map(|key| json!(key))
        .collect();
    let old_by_key: std::collections::HashMap<&(String, String), &Value> = old_sessions
        .iter()
        .map(|(key, value)| (key, *value))
        .collect();
    let new_keys: std::collections::HashSet<&(String, String)> =
        new_sessions.iter().map(|(key, _)| key).collect();
    let upsert: Vec<Value> = new_sessions
        .iter()
        .filter(|(key, value)| old_by_key.get(key) != Some(value))
        .map(|(_, value)| (*value).clone())
        .collect();
    let remove: Vec<Value> = old_sessions
        .iter()
        .filter(|(key, _)| !new_keys.contains(key))
        .map(|((project, id), _)| json!([project, id]))
        .collect();
    let old_order: Vec<&(String, String)> = old_sessions
        .iter()
        .map(|(key, _)| key)
        .filter(|key| new_keys.contains(key))
        .collect();
    let new_order: Vec<&(String, String)> = new_sessions.iter().map(|(key, _)| key).collect();
    // Added rows go where `order` says; without `order` the phone appends them, which is the
    // server's order only when every added row comes after the rows that stayed.
    let appended_in_place = {
        let kept: Vec<&(String, String)> = new_order
            .iter()
            .copied()
            .filter(|key| old_by_key.contains_key(key))
            .collect();
        kept == old_order && new_order.starts_with(&kept)
    };
    if !set.is_empty() {
        patch.insert("set".into(), Value::Object(set));
    }
    if !unset.is_empty() {
        patch.insert("unset".into(), Value::Array(unset));
    }
    if !upsert.is_empty() {
        patch.insert("upsert".into(), Value::Array(upsert));
    }
    if !remove.is_empty() {
        patch.insert("remove".into(), Value::Array(remove));
    }
    if !appended_in_place {
        let order = new_order
            .iter()
            .map(|(project, id)| json!([project, id]))
            .collect();
        patch.insert("order".into(), Value::Array(order));
    }
    if patch.is_empty() {
        PatchOutcome::Unchanged
    } else {
        PatchOutcome::Patch(patch)
    }
}
