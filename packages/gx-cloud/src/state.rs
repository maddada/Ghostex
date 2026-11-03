//! The box state machine: which state an event moves a box to, and which moves are refused.

use ghostex_gx_protocol::cloud_box::CloudBoxState;

/// Something that happened to a box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoxEvent {
    /// The provider accepted the create call.
    CreateRequested,
    /// The provider reports the machine running; boot starts.
    MachineUp,
    /// `ghostex box boot` (and session preparation) finished.
    Booted,
    /// The box reported no running turn and no open terminal.
    WentIdle,
    /// The box reported activity again.
    BecameBusy,
    /// The provider paused it (Instabox idle suspend).
    Paused,
    /// The provider stopped or archived it (Boat self-stop, a Stop, the archive scheduler).
    Stopped,
    /// A resume was requested.
    ResumeRequested,
    DeleteRequested,
    Deleted,
    /// A step failed.
    Failed,
}

/// The state `event` moves a box in `current` to, or `None` when that move is not allowed (a
/// stale or out-of-order report the registry ignores).
pub fn next_state(current: &CloudBoxState, event: &BoxEvent) -> Option<CloudBoxState> {
    use BoxEvent as E;
    use CloudBoxState as S;
    let next = match (current, event) {
        (S::Deleted, _) => return None,
        (_, E::Deleted) => S::Deleted,
        (S::Deleting, E::Failed) => S::Failed,
        (S::Deleting, _) => return None,
        (_, E::DeleteRequested) => S::Deleting,
        (_, E::Failed) => S::Failed,
        (S::Pending | S::Failed, E::CreateRequested) => S::Creating,
        (S::Creating | S::Paused | S::Stopped | S::Failed, E::MachineUp) => S::Booting,
        (S::Booting, E::Booted) => S::Running,
        (S::Running, E::WentIdle) => S::Idle,
        (S::Idle, E::BecameBusy) => S::Running,
        (S::Running | S::Idle | S::Booting, E::Paused) => S::Paused,
        (S::Running | S::Idle | S::Booting | S::Paused | S::Failed, E::Stopped) => S::Stopped,
        (S::Paused | S::Stopped | S::Failed, E::ResumeRequested) => S::Creating,
        _ => return None,
    };
    Some(next)
}
