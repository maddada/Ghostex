//! The wire fold: what a frame or a read does to the retained snapshot.
//!
//! Ported from `apps/desktop/sidebar/session-chat-runtime/fold.ts`. The rules that must survive
//! the port, one per key:
//!
//! - Cleared when a frame that can carry them omits them: `prompt`, `terminalNotice`,
//!   `terminalActivity`, `agentFleet`, `agentTasks`.
//! - Unchanged when omitted: `working`, `agent`, `appCommands`, `returnedPrompt`, `queue`,
//!   `draft`, `retiredAsyncQuestionIds`, and `screenProbed`, which is sticky and resets only on an
//!   agent change.
//! - `selectedOptions` merges by evidence priority and then by `detectedAt`.
//! - `accountSwitch`, `pendingModelSelection` and `asyncQuestionsSince` are three-state: absent,
//!   `null` and a value all mean different things.

use ghostex_gx_protocol::{
    ChatAppendedFrame, ChatSnapshotFrame, ChatStateFrame, ChatStatus, ReadSessionChatResult, Tri,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::jsnum::{js_number_of, js_number_value};

use crate::session::merge::merge_messages;
use crate::session::view_state::transcript_status_after_state;

/// The retained snapshot: the folded read result plus the keys a frame spreads in beside it.
///
/// The TypeScript folds frames and read results into one object, so a fold whose last input was a
/// frame keeps that frame's routing keys (`type`, `projectId`, `sessionId`, `serverId`,
/// `protocolVersion`). They are carried here for the same reason: the stored record must still
/// deserialize as what the TypeScript wrote.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoldedSnapshot {
    #[serde(flatten)]
    pub result: ReadSessionChatResult,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl FoldedSnapshot {
    /// A folded snapshot with no frame keys.
    pub fn new(result: ReadSessionChatResult) -> Self {
        Self {
            result,
            extra: Map::new(),
        }
    }
}

/// Anything that can carry state into the fold.
pub enum StateCarrier<'a> {
    /// A `sessionChatSnapshot` or `sessionChatReplaced` frame.
    Snapshot(&'a ChatSnapshotFrame),
    /// A `sessionChatState` frame: state only, no transcript.
    State(&'a ChatStateFrame),
    /// A `readSessionChat` result.
    Read(&'a ReadSessionChatResult),
}

/// `foldSessionChatState`.
pub fn fold_state(previous: Option<&FoldedSnapshot>, incoming: StateCarrier<'_>) -> FoldedSnapshot {
    let state_only = matches!(incoming, StateCarrier::State(_));
    // `base` is what the object spread starts from: the frame for a snapshot or a read, and the
    // previous fold for a state frame, which carries no transcript of its own.
    let mut folded = match (&incoming, previous) {
        (StateCarrier::State(_), Some(previous)) => previous.clone(),
        (StateCarrier::State(_), None) => FoldedSnapshot::new(empty_result()),
        (StateCarrier::Snapshot(frame), _) => snapshot_as_result(frame, previous),
        (StateCarrier::Read(result), _) => read_as_result(result, previous),
    };
    let side = match &incoming {
        StateCarrier::Snapshot(frame) => &frame.state,
        StateCarrier::State(frame) => &frame.state,
        StateCarrier::Read(result) => &result.state,
    };
    let previous_result = previous.map(|snapshot| &snapshot.result);
    let previous_side = previous_result.map(|result| &result.state);

    let agent_changed = match (
        previous_result.and_then(|result| result.session_agent_id.as_ref()),
        incoming_session_agent_id(&incoming),
    ) {
        (Some(before), Some(after)) => !before.is_empty() && !after.is_empty() && before != after,
        _ => false,
    };

    let (epoch, seq) = position(&incoming);
    folded.result.epoch = epoch;
    folded.result.seq = seq;

    if !state_only {
        // A window is rebuilt through the merger so two rows sharing an API response id both
        // survive and the index never points at the wrong row.
        folded.result.messages = merge_messages(&[], &folded.result.messages);
    }

    folded.result.status = if state_only {
        transcript_status_after_state(
            previous_result
                .map(|result| result.status.clone())
                .unwrap_or(ChatStatus::Loading),
            incoming_status(&incoming),
        )
    } else {
        incoming_status(&incoming)
    };

    folded.result.lifecycle = incoming_lifecycle(&incoming).cloned().or(if state_only {
        previous_result.and_then(|result| result.lifecycle.clone())
    } else {
        None
    });

    folded.result.working =
        incoming_working(&incoming).or(previous_result.and_then(|result| result.working));

    // Prompt semantics: a carrier that can hold it and does not has CLEARED it.
    folded.result.state.prompt = side.prompt.clone();
    folded.result.state.terminal_notice = side.terminal_notice.clone();
    folded.result.state.terminal_activity = side.terminal_activity.clone();
    folded.result.state.agent_fleet = side.agent_fleet.clone();
    folded.result.state.agent_tasks = side.agent_tasks.clone();

    folded.result.state.retired_async_question_ids = side
        .retired_async_question_ids
        .clone()
        .or_else(|| previous_side.and_then(|state| state.retired_async_question_ids.clone()));

    folded.result.state.async_questions_since = match &side.async_questions_since {
        Tri::Absent => previous_side
            .map(|state| state.async_questions_since.clone())
            .unwrap_or(Tri::Absent),
        present => present.clone(),
    };

    folded.result.agent = incoming_agent(&incoming)
        .cloned()
        .or_else(|| previous_result.and_then(|result| result.agent.clone()));

    folded.result.state.agent_session_id = side.agent_session_id.clone().or(if state_only {
        previous_side.and_then(|state| state.agent_session_id.clone())
    } else {
        None
    });

    folded.result.state.selected_options = merge_options(
        if agent_changed {
            None
        } else {
            previous_side.and_then(|state| state.selected_options.as_ref())
        },
        side.selected_options.as_ref(),
    );

    folded.result.state.screen_probed = if agent_changed {
        side.screen_probed
    } else {
        match (
            side.screen_probed,
            previous_side.and_then(|state| state.screen_probed),
        ) {
            (Some(true), _) => Some(true),
            (_, Some(true)) => Some(true),
            (Some(false), previous) => previous.or(Some(false)),
            (None, previous) => previous,
        }
    };

    folded.result.state.app_commands = side
        .app_commands
        .clone()
        .or_else(|| previous_side.and_then(|state| state.app_commands.clone()));

    folded.result.state.returned_prompt = side
        .returned_prompt
        .clone()
        .or_else(|| previous_side.and_then(|state| state.returned_prompt.clone()));

    folded.result.state.account_switch = match &side.account_switch {
        Tri::Absent => previous_side
            .map(|state| state.account_switch.clone())
            .unwrap_or(Tri::Absent),
        present => present.clone(),
    };
    folded.result.state.pending_model_selection = match &side.pending_model_selection {
        Tri::Absent => previous_side
            .map(|state| state.pending_model_selection.clone())
            .unwrap_or(Tri::Absent),
        present => present.clone(),
    };

    folded.result.state.queue = side
        .queue
        .clone()
        .or_else(|| previous_side.and_then(|state| state.queue.clone()));

    folded.result.state.draft = match &side.draft {
        Some(draft) => Some(merge_draft_state(
            previous_side.and_then(|state| state.draft.as_ref()),
            draft,
        )),
        None => previous_side.and_then(|state| state.draft.clone()),
    };

    folded
}

/// `foldSessionChatAppend`: new rows, and nothing else that is not on the frame.
pub fn fold_append(previous: &FoldedSnapshot, event: &ChatAppendedFrame) -> FoldedSnapshot {
    let mut folded = previous.clone();
    folded.result.epoch = event.base.epoch;
    folded.result.seq = event.base.seq;
    if !event.superseded_message_ids.is_empty() {
        folded.result.messages.retain(|message| {
            !event
                .superseded_message_ids
                .iter()
                .any(|id| id == &message.id)
        });
    }
    folded.result.messages = merge_messages(&folded.result.messages, &event.messages);
    if let Some(lifecycle) = event.lifecycle.clone() {
        folded.result.lifecycle = Some(lifecycle);
    }
    if !event.messages.is_empty() {
        folded.result.status = ChatStatus::Ready;
    }
    folded
}

/// `mergeOptions`: the stronger evidence wins, then the later capture, and the three status
/// payloads fill in whatever the winner left out.
///
/// Terminal option captures, Claude statusline payloads and Codex transcript stats arrive
/// independently, so an options-only reply must not clear reported usage.
pub fn merge_options(current: Option<&Value>, incoming: Option<&Value>) -> Option<Value> {
    merge_options_detail(current, incoming).0
}

/// [`merge_options`], plus whether `setSelectedOptions` would have been handed a NEW object.
///
/// React bails out of a state update when `Object.is(next, current)`, so the identity of
/// `applySelectedOptions`'s result decided whether the controller re-rendered and therefore
/// whether `native-options.ts`'s `useLayoutEffect(applyDetected, [.., chat.selectedOptions])` fired
/// again. The TypeScript returned `current` itself only when the older capture won AND none of the
/// three status payloads had to be folded in; every other path builds a fresh object, equal value
/// or not. Callers that need that distinction (family e1's detection dep, and through it the
/// `optionWrite` round trip) take the flag; the rest use [`merge_options`].
pub fn merge_options_detail(
    current: Option<&Value>,
    incoming: Option<&Value>,
) -> (Option<Value>, bool) {
    let Some(incoming) = incoming else {
        return (current.cloned(), false);
    };
    let stronger = ["model", "effort", "mode"].iter().any(|field| {
        evidence_priority(choice_source(incoming, field))
            > evidence_priority(current.and_then(|current| choice_source(current, field)))
    });
    let mut kept_current = false;
    let chosen = match current {
        Some(current)
            if !stronger
                && parse_date(detected_at(current)) > parse_date(detected_at(incoming)) =>
        {
            kept_current = true;
            current
        }
        _ => incoming,
    };
    let mut merged = chosen.clone();
    let mut spread = false;
    for key in [
        "codexStatus",
        "claudeStatus",
        "cursorStatus",
        "contextUsage",
    ] {
        let value = present(chosen.get(key))
            .or_else(|| present(incoming.get(key)))
            .or_else(|| current.and_then(|current| present(current.get(key))));
        if let (Some(value), Some(object)) = (value, merged.as_object_mut()) {
            object.insert(key.to_string(), value.clone());
            spread = true;
        }
    }
    (Some(merged), !kept_current || spread)
}

fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

fn choice_source<'a>(options: &'a Value, field: &str) -> Option<&'a str> {
    options.get(field)?.get("source")?.as_str()
}

fn detected_at(options: &Value) -> Option<&str> {
    options.get("detectedAt")?.as_str()
}

/// `sessionChatOptionEvidencePriority`.
fn evidence_priority(source: Option<&str>) -> u8 {
    match source {
        Some("terminal") => 3,
        Some("statusline") => 2,
        Some("transcript") => 1,
        _ => 0,
    }
}

/// `Date.parse` of an ISO 8601 stamp, reduced to what the comparison needs: a lexicographic
/// compare of the normalized text.
///
/// Every `detectedAt` gxserver writes is `toISOString()`, which is fixed-width UTC, so ordering
/// the strings orders the instants. A value that is not such a stamp compares as absent, which is
/// what `Date.parse` returning `NaN` does in the TypeScript (every comparison against it is
/// false).
fn parse_date(value: Option<&str>) -> Option<&str> {
    value.filter(|value| value.len() >= 20 && value.ends_with('Z') && value.as_bytes()[4] == b'-')
}

/// `mergeSessionChatDraftState`, which the fold needs and family d shares.
///
/// The highest revision of a draft id wins, a consumed draft's body is emptied rather than
/// dropped, and the delivery receipts are kept newest first and bounded.
pub fn merge_draft_state(current: Option<&Value>, incoming: &Value) -> Value {
    let mut consumed: Vec<(String, f64)> = Vec::new();
    for receipt in receipts(current, "consumedDrafts")
        .into_iter()
        .chain(receipts(Some(incoming), "consumedDrafts"))
    {
        let Some(draft_id) = receipt.get("draftId").and_then(Value::as_str) else {
            continue;
        };
        // `Math.max(consumed.get(id) ?? 0, receipt.revision)` over a JSON number, which has no
        // integer form: `Value::as_i64` refused a `5.0` token and wrote 0 back into the
        // document, so every later `revision >= version.revision` saw a retired draft as live.
        let revision = js_number_of(receipt.get("revision")).unwrap_or(0.0);
        match consumed.iter_mut().find(|(id, _)| id == draft_id) {
            Some(entry) => entry.1 = entry.1.max(revision),
            None => consumed.push((draft_id.to_string(), revision)),
        }
    }

    let body = match current {
        Some(current)
            if version_field(current, "draftId") == version_field(incoming, "draftId")
                && version_field(current, "draftId").is_some()
                && version_revision(current) > version_revision(incoming) =>
        {
            current
        }
        _ => incoming,
    };
    let retired = version_field(body, "draftId").is_some_and(|draft_id| {
        consumed
            .iter()
            .find(|(id, _)| id == draft_id)
            .map(|(_, revision)| *revision)
            .unwrap_or(0.0)
            >= version_revision(body)
    });

    let mut delivered: Vec<Value> = Vec::new();
    for receipt in receipts(current, "deliveredDrafts")
        .into_iter()
        .chain(receipts(Some(incoming), "deliveredDrafts"))
    {
        let id = receipt.get("id").cloned().unwrap_or(Value::Null);
        match delivered
            .iter_mut()
            .find(|existing| existing.get("id") == Some(&id))
        {
            Some(existing) => *existing = receipt,
            None => delivered.push(receipt),
        }
    }
    delivered.sort_by(|left, right| {
        let key = |value: &Value| {
            (
                value
                    .get("deliveredAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                value
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        };
        key(right).cmp(&key(left))
    });
    delivered.truncate(50);

    let mut merged = body.clone();
    if let Some(object) = merged.as_object_mut() {
        if retired {
            object.insert("content".to_string(), Value::String(String::new()));
        }
        object.insert(
            "consumedDrafts".to_string(),
            Value::Array(
                consumed
                    .into_iter()
                    .map(|(draft_id, revision)| {
                        let mut entry = Map::new();
                        entry.insert("draftId".to_string(), Value::String(draft_id));
                        // `JSON.stringify(5)` is `5`, not `5.0`: the receipt goes back on the
                        // wire, where an integer stays an integer.
                        entry.insert("revision".to_string(), js_number_value(revision));
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
        object.insert("deliveredDrafts".to_string(), Value::Array(delivered));
    }
    merged
}

fn receipts(value: Option<&Value>, key: &str) -> Vec<Value> {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn version_field<'a>(draft: &'a Value, key: &str) -> Option<&'a str> {
    draft.get("version")?.get(key)?.as_str()
}

/// `draft.version.revision` as JavaScript reads it: one double, whatever token carried it.
fn version_revision(draft: &Value) -> f64 {
    js_number_of(
        draft
            .get("version")
            .and_then(|version| version.get("revision")),
    )
    .unwrap_or(0.0)
}

fn empty_result() -> ReadSessionChatResult {
    ReadSessionChatResult {
        messages: Vec::new(),
        has_more: false,
        has_more_exact: None,
        before_offset: 0,
        epoch: 0,
        seq: 0,
        status: ChatStatus::Loading,
        working: None,
        fingerprint: None,
        agent: None,
        session_agent_id: None,
        available_agents: None,
        switchable_agents: None,
        fork_info: None,
        subagent: None,
        error: None,
        lifecycle: None,
        state: Default::default(),
    }
}

/// A snapshot or replaced frame, spread over whatever the previous fold held.
///
/// The frame owns the transcript window, the status, the working flag, the agent and the
/// lifecycle; everything a frame has no field for (the draft metadata, the fork info, the
/// fingerprint) survives from the previous fold, which is exactly what `{...previous, ...base}`
/// does there.
/// The snapshot frame as the read result the VIEW applies: the frame's own fields and nothing
/// carried from the previous fold.
///
/// `onEvent` in `controller.ts` hands `applyAuthoritative` the EVENT; the store's fold, which
/// carries `selectedOptions`, `appCommands`, the queue and the draft forward when a frame omits
/// them, is only what is retained. Applying the fold instead re-applied the carried options as a
/// fresh detection on every replaced frame (an `optionWrite` the TypeScript brain never made, which
/// then took the acknowledgement a draft write was waiting for), and re-set the carried
/// `appCommands` as a new array.
pub fn snapshot_frame_result(frame: &ChatSnapshotFrame) -> ReadSessionChatResult {
    let mut result = snapshot_as_result(frame, None).result;
    result.epoch = frame.base.epoch;
    result.seq = frame.base.seq;
    result
}

/// Whether a snapshot owns the three read-only draft-agent fields: any of them present, `null`
/// included (`'sessionAgentId' in event`). Every gxserver socket frame omits all three; a host
/// that synthesizes snapshots from reads sends all three, `null` for the ones a promoted session's
/// read no longer carries.
pub fn snapshot_owns_draft_agents(frame: &ChatSnapshotFrame) -> bool {
    !frame.session_agent_id.is_absent()
        || !frame.available_agents.is_absent()
        || !frame.switchable_agents.is_absent()
}

fn snapshot_as_result(
    frame: &ChatSnapshotFrame,
    previous: Option<&FoldedSnapshot>,
) -> FoldedSnapshot {
    let mut folded = previous
        .cloned()
        .unwrap_or_else(|| FoldedSnapshot::new(empty_result()));
    folded.result.messages = frame.messages.clone();
    folded.result.has_more = frame.has_more;
    folded.result.has_more_exact = frame.has_more_exact;
    folded.result.before_offset = frame.before_offset;
    folded.result.status = frame.status.clone();
    folded.result.working = frame.working;
    folded.result.agent = frame.agent.clone();
    // `{...previous, ...base}`: a frame that owns the three read-only draft-agent fields sets
    // them (an owned `null` clears one), and one that does not leaves what the previous fold held.
    if snapshot_owns_draft_agents(frame) {
        folded.result.session_agent_id = frame.session_agent_id.value().cloned();
        folded.result.available_agents = frame.available_agents.value().cloned();
        folded.result.switchable_agents = frame.switchable_agents.value().cloned();
    }
    folded.result.lifecycle = frame.lifecycle.clone();
    folded.result.state = frame.state.clone();
    folded.extra.insert(
        "projectId".to_string(),
        Value::String(frame.base.project_id.clone()),
    );
    folded.extra.insert(
        "sessionId".to_string(),
        Value::String(frame.base.session_id.clone()),
    );
    folded.extra.insert(
        "serverId".to_string(),
        Value::String(frame.base.server_id.clone()),
    );
    folded.extra.insert(
        "protocolVersion".to_string(),
        Value::from(frame.base.protocol_version),
    );
    folded
}

/// A read result, spread the same way.
fn read_as_result(
    result: &ReadSessionChatResult,
    previous: Option<&FoldedSnapshot>,
) -> FoldedSnapshot {
    let mut folded = previous
        .cloned()
        .unwrap_or_else(|| FoldedSnapshot::new(empty_result()));
    folded.result = result.clone();
    folded
}

fn position(incoming: &StateCarrier<'_>) -> (i64, i64) {
    match incoming {
        StateCarrier::Snapshot(frame) => (frame.base.epoch, frame.base.seq),
        StateCarrier::State(frame) => (frame.base.epoch, frame.base.seq),
        StateCarrier::Read(result) => (result.epoch, result.seq),
    }
}

fn incoming_status(incoming: &StateCarrier<'_>) -> ChatStatus {
    match incoming {
        StateCarrier::Snapshot(frame) => frame.status.clone(),
        StateCarrier::State(frame) => frame.status.clone(),
        StateCarrier::Read(result) => result.status.clone(),
    }
}

fn incoming_working(incoming: &StateCarrier<'_>) -> Option<bool> {
    match incoming {
        StateCarrier::Snapshot(frame) => frame.working,
        StateCarrier::State(frame) => frame.working,
        StateCarrier::Read(result) => result.working,
    }
}

fn incoming_agent<'a>(incoming: &'a StateCarrier<'a>) -> Option<&'a String> {
    match incoming {
        StateCarrier::Snapshot(frame) => frame.agent.as_ref(),
        StateCarrier::State(_) => None,
        StateCarrier::Read(result) => result.agent.as_ref(),
    }
}

fn incoming_lifecycle<'a>(
    incoming: &'a StateCarrier<'a>,
) -> Option<&'a ghostex_gx_protocol::TurnLifecycle> {
    match incoming {
        StateCarrier::Snapshot(frame) => frame.lifecycle.as_ref(),
        StateCarrier::State(frame) => frame.lifecycle.as_ref(),
        StateCarrier::Read(result) => result.lifecycle.as_ref(),
    }
}

/// `sessionAgentId`, which is what makes an agent change detectable.
///
/// A read always carries it; an ordinary daemon frame does not, but a snapshot synthesized from a
/// read (the retained store's `snapshotEvent`, the mobile SSH host) owns it deliberately.
fn incoming_session_agent_id<'a>(incoming: &'a StateCarrier<'a>) -> Option<&'a String> {
    match incoming {
        StateCarrier::Read(result) => result.session_agent_id.as_ref(),
        StateCarrier::Snapshot(frame) => frame.session_agent_id.value(),
        _ => None,
    }
}
