//! Which computers' sessions the Unified list shows, and what a card's third line says.

use crate::keys::MachineId;
use crate::presentation_store::PresentationStore;
use crate::ConnectionPhase;

use super::build::UnifiedRow;

/// Whether a computer's sessions are cards: this computer always, another computer only while its
/// stream is live.
///
/// CDXC:Sidebar 2026-11-05 DECISION:
/// User: "when we're using unified sidebar, the sessions from the other machine still show in sidebar when that machine goes disconnected". The Project sidebar keeps a disconnected computer's last-seen rows, faded, under that computer's own tab with its offline notice (CDXC:RemoteMachines 2026-09-21 in presentation_store/apply.rs), because there you have picked that computer. The Unified list mixes every computer into one list with no tab to say it is offline, so a computer that is not live (lost, reconnecting, or only its stored copy) gives no cards, no filter choice and no counts until its stream is live again, when its cards come back on their own. It is the same "live" `is_stale` tests (sidebar_view/model.rs).
pub fn unified_shows_machine(store: &PresentationStore, machine: &MachineId) -> bool {
    machine.is_local()
        || store.machine(machine).is_some_and(|entry| {
            entry.loaded().is_some_and(|loaded| !loaded.last_seen)
                && entry.connection().phase == ConnectionPhase::Live
        })
}

/// A card's third line, which every card draws so every card is three lines high.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnifiedBranchLine {
    /// The branch its folder is on.
    Branch(String),
    /// Its folder is in no git work tree ([`super::build::no_source_control`]): "Untracked".
    Untracked,
    /// The branch it was last seen on, while its folder's branch is not known now.
    LastKnown(String),
    /// No branch is known: a detached checkout, or one not probed yet ("No branch").
    Unknown,
}

/// What a card's third line says, from its row and the branch the card showed last, if any.
///
/// CDXC:Sidebar 2026-11-05 DECISION:
/// User: "also we don't show the branch row in that case which is bad, i want rows to always remain 3 rows in unified sidebar". The third line is never left out: the branch, else "Untracked" for a folder outside git, else the branch the card last showed (gxserver probes a session's branch only while it is live, and a project not probed yet has none), else a quiet "No branch" beside the branch icon.
pub fn unified_branch_line(row: &UnifiedRow, last_known: Option<&str>) -> UnifiedBranchLine {
    if let Some(branch) = &row.session.row.menu_facts.branch {
        return UnifiedBranchLine::Branch(branch.clone());
    }
    if row.no_source_control {
        return UnifiedBranchLine::Untracked;
    }
    match last_known.filter(|branch| !branch.trim().is_empty()) {
        Some(branch) => UnifiedBranchLine::LastKnown(branch.to_string()),
        None => UnifiedBranchLine::Unknown,
    }
}
