use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;

use super::ProgramStatusRecord;

/// The record state that decides the session row, and the record behind it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DerivedProgramStatus {
    /// idle | working | blocked | done | error
    pub state: &'static str,
    pub record: Option<ProgramStatusRecord>,
}

fn state_rank(state: &str) -> u8 {
    match state {
        "blocked" => 5,
        "working" => 4,
        "error" => 3,
        "done" => 2,
        "idle" => 1,
        _ => 0,
    }
}

fn state_name(state: &str) -> &'static str {
    match state {
        "blocked" => "blocked",
        "working" => "working",
        "error" => "error",
        "done" => "done",
        _ => "idle",
    }
}

/// CDXC:SessionStatus 2026-10-14 WHY:
/// A program may hold several records (a root and its workers). The session row shows the one that matters most: waiting on the user beats working, working beats a failure or a finished result (the program is not done while something still runs), and among equals the root record, then the most recently updated one, speaks for the session.
pub(crate) fn derive_program_status(records: &[ProgramStatusRecord]) -> DerivedProgramStatus {
    let best = records
        .iter()
        .filter(|record| state_rank(&record.state) > 0 && !is_omp_wait_report(record))
        .max_by_key(|record| (state_rank(&record.state), record.id.is_empty(), record.seq));
    DerivedProgramStatus {
        state: best
            .map(|record| state_name(&record.state))
            .unwrap_or("idle"),
        record: best.cloned(),
    }
}

/// CDXC:SessionStatus 2026-11-03 WHY:
/// OMP reports `blocked` from the start of a tool that asks or needs approval until that tool ends, also after the user answered, and repeats the record while it waits. Its hooks carry those waits the way Claude's do (a PermissionRequest for the dialog OMP shows, the `ask` call's pre-tool event), so OMP's own `blocked` record counts for nothing: counted, "blocked wins" kept an approved tool's run in attention and each repeat could raise the wait again. Its other states count as before.
/// SEE-ALSO: build_omp_extension_source in server/src/agent_hooks/plugin_sources.rs.
fn is_omp_wait_report(record: &ProgramStatusRecord) -> bool {
    record.id.is_empty() && record.app.as_deref() == Some("omp") && record.state == "blocked"
}

/// The app of a record, inherited from its nearest ancestor when it has none.
pub(crate) fn effective_program_app(
    records: &[ProgramStatusRecord],
    record: &ProgramStatusRecord,
) -> Option<String> {
    if let Some(app) = record.app.as_ref() {
        return Some(app.clone());
    }
    let mut id = record.id.as_str();
    loop {
        let parent = match id.rfind('/') {
            Some(index) => &id[..index],
            None if id.is_empty() => return None,
            None => "",
        };
        if let Some(app) = records
            .iter()
            .find(|candidate| candidate.id == parent)
            .and_then(|candidate| candidate.app.clone())
        {
            return Some(app);
        }
        if parent.is_empty() {
            return None;
        }
        id = parent;
    }
}

/// CDXC:SessionStatus 2026-10-14 DECISION:
/// User brief: working maps to working, blocked to needs attention (with its kind), done to finished-unseen, error to failed, idle to idle. Ghostex has no separate failed activity, so error is a finished turn (attention with the turn-complete source, which the chat queue may deliver into) and the sidebar paints it red from the published program status.
pub(crate) fn program_status_activity(state: &str) -> (&'static str, Option<&'static str>) {
    match state {
        "working" => ("working", None),
        "blocked" => ("attention", None),
        // `finished` is an agent's working → idle step (ingest.rs `finished_turn_step`).
        "done" | "error" | "finished" => ("attention", Some(TURN_COMPLETE_ATTENTION_SOURCE)),
        _ => ("idle", None),
    }
}

/// Identity of a derived state: the same record at the same report never
/// re-raises a state the user already dealt with.
pub(crate) fn program_status_applied_key(derived: &DerivedProgramStatus) -> String {
    match derived.record.as_ref() {
        Some(record) => format!("{}|{}|{}", derived.state, record.id, record.seq),
        None => derived.state.to_string(),
    }
}

/// Text from a record shown outside the terminal grid. zmx already refused
/// control characters; this disarms direction overrides and invisible
/// formatting characters as the spec asks, so a message cannot reorder or
/// hide text around it.
pub(crate) fn program_status_display_text(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .filter(|ch| {
            !matches!(
                *ch as u32,
                0x00ad | 0x061c | 0x180e | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x2064
                    | 0x2066..=0x2069 | 0xfeff
            ) && !ch.is_control()
        })
        .collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
