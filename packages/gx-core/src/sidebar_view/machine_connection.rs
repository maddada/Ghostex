//! What a remote machine's connection state looks like in the sidebar: whether its tab spins or
//! turns red, and the notice its list draws while it is not connected.
//!
//! CDXC:RemoteMachines 2026-10-25 DECISION:
//! User: a remote machine that cannot connect must never show an empty tab with no message. Every state that is not connected, connecting or plain `disconnected` is a failure, and the machine's list says why with Reconnect and Remote Settings. Supersedes the 2026-10-02 notice that sat in the list as a red title over two stacked buttons ("these messages at the top are so ugly"). The user's picks for the look: the failure is a card "docked at top, not bottom" of the list with the reason and labelled Reconnect and Remote Settings buttons, plus a "tinted icon (cloud icon becomes redish when fails)"; "tint just the icon not the whole tab"; connecting is the tab's "spinner and the one line connecting to macbook... pill floating near the top", naming the machine. The rows never move for it: "i dont want the projects and sessions to shift down while we connect or have errors etc. these sessions should stay in same spot (floating above" — the card and the pill float over the top of the list with no inset. The card closes with an ×: "Yes, × shrinks it to a one-line pill" that keeps the red mark and the title, and a press on the pill opens the card again. The shrink holds for that failure (retries that fail the same way stay a pill) and is forgotten when the machine connects or fails for another reason (`SidebarUiStore::retain_machine_notice_collapses`).
//!
//! SEE-ALSO: apps/desktop/src/app/helpers/remote/types.rs (`GpuiRemoteGxserverConnectState`, the
//! wire words, and the terminal overlay that reads the same copy),
//! apps/desktop/src/app/native_sidebar/machines.rs and machine_notice.rs (the renderers).

use super::inputs::{MachineTabInput, SidebarUiState, MACHINE_STATE_CONNECTED};
use super::view::MachineNotice;

/// A connect attempt is running for the machine.
pub fn machine_state_is_busy(state: &str) -> bool {
    matches!(
        state,
        "connecting" | "installing" | "downloadingRemoteServerPackage"
    )
}

/// The machine's last connect attempt failed (`sshFailed`, `authFailed`, `tunnelFailed`, …).
pub fn machine_state_is_failure(state: &str) -> bool {
    !machine_state_is_busy(state) && !matches!(state, MACHINE_STATE_CONNECTED | "disconnected")
}

/// The title and the default explanation of a connection state, shared by the sidebar notice and
/// the terminal overlay of a remote session so the two never disagree about one machine.
pub fn machine_state_copy(state: &str) -> (&'static str, Option<&'static str>) {
    match state {
        "installing" => ("Installing gxserver…", None),
        "downloadingRemoteServerPackage" => ("Downloading server package…", None),
        "disconnected" => ("Not connected", None),
        "installApprovalRequired" => (
            "Remote setup needed",
            Some("Approve the gxserver install for this machine."),
        ),
        "installFailed" => (
            "Remote setup failed",
            Some("Reconnect the machine to try again."),
        ),
        "authFailed" => (
            "Cannot sign in to machine",
            Some("The machine rejected the saved SSH username, password or key."),
        ),
        "sshFailed" => ("Cannot reach machine", Some("SSH connection failed.")),
        "tunnelFailed" => ("Cannot reach machine", Some("SSH tunnel failed.")),
        "keychainFailed" => (
            "Cannot reach machine",
            Some("Saved credentials are unavailable."),
        ),
        "tokenUnavailable" => (
            "Cannot reach machine",
            Some("The remote gxserver token is unavailable."),
        ),
        "presentationSubscribeFailed" | "presentationStreamFailed" => (
            "Reconnecting to machine…",
            Some("The remote session stream dropped."),
        ),
        "unsupported" | "unsupportedRemotePlatform" => (
            "Machine unsupported",
            Some("This remote platform cannot host gxserver."),
        ),
        "invalid" => (
            "Machine unavailable",
            Some("The saved machine settings are incomplete."),
        ),
        "failed" => (
            "Cannot reach machine",
            Some("Reconnect the machine to try again."),
        ),
        _ => ("Connecting to machine…", None),
    }
}

/// The state word a "This is this computer" notice is shrunk against.
pub const THIS_COMPUTER_NOTICE_STATE: &str = "thisComputer";

/// CDXC:RemoteMachines 2026-10-28 WHY: a saved machine that points back at this computer (SSH to
/// localhost, its own address or name) reaches the gxserver this app already runs, so its tab lists
/// the same projects as Local and an install or update there would stop that very gxserver. Found
/// testing a loopback machine on Linux; the host detects it on every platform by comparing the
/// remote and local server ids and blocks Install/Update for it. Its list says so in a quiet card
/// (not the error colour) that × shrinks like any other.
fn this_computer_notice(machine: &MachineTabInput, ui: &SidebarUiState) -> MachineNotice {
    MachineNotice {
        machine_id: machine.machine_id.clone(),
        title: "This is this computer".to_string(),
        detail: Some(
            "It runs the same gxserver as Local, so this tab shows the same projects.".to_string(),
        ),
        busy: false,
        failed: false,
        state: THIS_COMPUTER_NOTICE_STATE.to_string(),
        collapsed: ui
            .collapsed_machine_notices
            .get(&machine.machine_id)
            .map(String::as_str)
            == Some(THIS_COMPUTER_NOTICE_STATE),
    }
}

/// The notice of the selected machine's list: none for this computer or a connected machine,
/// except a saved machine that is this computer, which says so.
/// A failure explains itself with the host's sanitized reason when it has one.
pub(crate) fn machine_notice(
    machine: Option<&MachineTabInput>,
    ui: &SidebarUiState,
) -> Option<MachineNotice> {
    let machine = machine.filter(|machine| !machine.is_local())?;
    if machine.is_connected() {
        return machine
            .this_computer
            .then(|| this_computer_notice(machine, ui));
    }
    let (title, detail) = machine_state_copy(&machine.state);
    let failed = machine_state_is_failure(&machine.state);
    let busy = machine_state_is_busy(&machine.state);
    // The floating pill names the machine; the terminal overlay keeps the shared generic copy.
    let title = if machine.state == "connecting" {
        format!("Connecting to {}…", machine.label)
    } else {
        title.to_string()
    };
    Some(MachineNotice {
        machine_id: machine.machine_id.clone(),
        title,
        detail: if failed {
            machine.message.clone().or(detail.map(str::to_string))
        } else {
            detail.map(str::to_string)
        },
        busy,
        failed,
        state: machine.state.clone(),
        collapsed: !busy
            && ui.collapsed_machine_notices.get(&machine.machine_id) == Some(&machine.state),
    })
}

#[cfg(test)]
mod this_computer_tests {
    use super::*;

    fn machine(state: &str, this_computer: bool) -> MachineTabInput {
        MachineTabInput {
            machine_id: "remote-loop".to_string(),
            label: "Loopback".to_string(),
            state: state.to_string(),
            message: None,
            fed: true,
            this_computer,
        }
    }

    #[test]
    fn a_connected_machine_that_is_this_computer_says_so_quietly() {
        let ui = SidebarUiState::default();
        let notice = machine_notice(Some(&machine(MACHINE_STATE_CONNECTED, true)), &ui)
            .expect("the list explains the mirror");
        assert_eq!(notice.title, "This is this computer");
        assert!(!notice.failed && !notice.busy);
        assert!(machine_notice(Some(&machine(MACHINE_STATE_CONNECTED, false)), &ui).is_none());
        // A failure still explains the failure.
        let failed = machine_notice(Some(&machine("sshFailed", true)), &ui).expect("failure");
        assert!(failed.failed);
    }
}
