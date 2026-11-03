//! Clicks outside the sidebar that land on a session or a group: the status item, a
//! menu bar session row, a Quick Access or command palette session row, a palette Action run, and
//! a Back or Forward stop.
//!
//! CDXC:StatusPet 2026-09-25 WHY:
//! Each of these used to reach the app runtime through a callback of its own
//! (`onStatusPetActivation`, `onMenuBarSessionActivation`, `onCommandPaletteSessionFocus`,
//! `onCommandPaletteRunSidebarCommand`), and every one of those only re-entered the runtime's
//! `focusSession` or `runSidebarCommand`. The ids are shaped here and handed to the store's one
//! focus and Action run (`focus_perform.rs`, `sidebar_command_run.rs`), the same ones a sidebar
//! row click and a project row's Actions button end in. The bounded-id rules the callbacks
//! enforced (StatusPet 2026-06-26) are enforced before this point by `status_activation.rs` and here.
//!
//! SEE-ALSO: apps/desktop/src/app/status_activation.rs (the dispatchers),
//! apps/desktop/src/app/gx_store/focus_perform.rs.

use ghostex_gx_core::{ProjectKey, SessionKey, SidebarUiIntent};
use serde_json::json;

use super::focus_perform::RowFocusOptions;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Focuses one session the way a sidebar row click does. Returns whether the id named
    /// something a focus can take.
    pub(crate) fn gx_store_focus_activated_session(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if session_id.is_empty() {
            return false;
        }
        // A focus starts from the store's newest selection (gx_store/burst.rs).
        self.gx_store_flush_local_selection(cx);
        self.gx_store_focus_session_row(session_id, RowFocusOptions::default(), cx)
    }

    /// Shows one session the way a click in the notification feed does: the sidebar switches to
    /// the session's computer with the same intent its machine picker sends, then the session is
    /// focused (a remote one opens on its computer) and its row revealed. The feed, an OS banner
    /// and a session link all end here, so they cannot drift apart again.
    ///
    /// The explicit switch comes first because the reveal only switches when it can place the
    /// row, which it cannot for a computer whose rows the store does not hold (offline).
    pub(crate) fn gx_store_show_session_on_its_machine(
        &mut self,
        session: &SessionKey,
        options: RowFocusOptions,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_apply_sidebar_ui_intent(
            SidebarUiIntent::SelectMachine {
                machine_id: session_machine_tab(session),
            },
            cx,
        );
        let row_id = session.to_sidebar_session_id();
        // A focus starts from the store's newest selection (gx_store/burst.rs).
        self.gx_store_flush_local_selection(cx);
        self.gx_store_focus_session_row(&row_id, options, cx);
        let request_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_micros() as u64);
        self.gx_store_note_local_sidebar_reveal(&row_id, request_id);
        self.gx_store_note_sidebar_reveal(&row_id, request_id, cx);
        self.gx_store_sidebar_state_changed(cx);
    }

    /// Whether this window has the session behind `row_id` focused or on screen, so a
    /// notification or menu bar click on it can go to this window (app/workspace_windows/).
    pub(crate) fn gx_store_shows_session_row(&self, row_id: &str) -> bool {
        let row_id = row_id.trim();
        let Some(session) = SessionKey::parse_remote_scoped_session_id(row_id)
            .or_else(|| SessionKey::parse_sidebar_session_id(row_id))
        else {
            return false;
        };
        let focus = self.gx_store.core.focus();
        focus.focused_session.as_ref() == Some(&session)
            || focus.visible_sessions.contains(&session)
    }

    /// Focuses one sidebar group (a project, or a user-made group in one) the way a header click
    /// does. Returns whether the id named a group a focus can take.
    pub(crate) fn gx_store_focus_activated_group(
        &mut self,
        group_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if group_id.is_empty() {
            return false;
        }
        self.gx_store_flush_local_selection(cx);
        self.gx_store_focus_group_row(group_id, cx)
    }

    /// The Back/Forward stop the sidebar is on (navigation_history/controller.rs).
    pub(crate) fn gx_store_navigation_entry(
        &self,
    ) -> Option<ghostex_gx_core::navigation_history::NavigationHistoryEntry> {
        ghostex_gx_core::navigation_history::sidebar_navigation_entry(
            self.gx_store.sidebar_list.model(),
            self.gx_store.core.focus(),
        )
    }

    /// Runs one saved Action by id, with the scope that picks its list. Returns `true`: the run is
    /// answered here whatever it finds.
    pub(crate) fn gx_store_run_activated_sidebar_command(
        &mut self,
        command_id: &str,
        run_mode: Option<&str>,
        scope: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut message = json!({ "type": "runSidebarCommand", "commandId": command_id.trim() });
        if let Some(run_mode) = run_mode {
            message["runMode"] = json!(run_mode);
        }
        if let Some(scope) = scope {
            message["scope"] = json!(scope);
        }
        // The run can move the project, which starts from the store's newest selection.
        self.gx_store_flush_local_selection(cx);
        self.gx_store_run_sidebar_command(&message, cx);
        true
    }
}

/// The machine picker tab a session lives under: its remote machine id, or the local tab.
pub(crate) fn session_machine_tab(session: &SessionKey) -> String {
    session
        .machine
        .remote_id()
        .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
        .to_string()
}

/// The session an OS banner's tag names (a macOS banner, a Windows toast or a Linux notification):
/// a bounded sidebar id, local or remote. `None` for anything else, which a focus would refuse.
pub(crate) fn system_notification_session(tag: &str) -> Option<SessionKey> {
    let tag = tag.trim();
    if !crate::app::helpers::gpui_status_bridge_id_allowed(tag) {
        return None;
    }
    SessionKey::parse_sidebar_session_id(tag)
}

/// The sidebar id a menu bar session row focuses. A row carries its project id and either a
/// sidebar id already or the daemon's raw session id, which is scoped to its project here.
pub(crate) fn menu_bar_session_focus_id(project_id: &str, session_id: &str) -> String {
    if SessionKey::parse_sidebar_session_id(session_id).is_some() {
        return session_id.to_string();
    }
    match ProjectKey::parse_workspace_project_id(project_id) {
        Some(ProjectKey {
            machine: ghostex_gx_core::MachineId::Remote(machine_id),
            project_id,
        }) => SessionKey::remote(machine_id, project_id, session_id).to_sidebar_session_id(),
        _ => SessionKey::local(project_id, session_id).to_sidebar_session_id(),
    }
}

/// A palette session row names a projected sidebar id: a local `combined-session:` id or a remote
/// one. A raw daemon id is not routable from the palette and is refused.
pub(crate) fn palette_session_focus_id(session_id: &str) -> Option<&str> {
    let session_id = session_id.trim();
    SessionKey::parse_sidebar_session_id(session_id).map(|_| session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_sessions_banner_names_its_computer() {
        let tag = SessionKey::remote("studio", "P1", "G1").to_sidebar_session_id();
        let session = system_notification_session(&tag).expect("a remote banner names a session");
        assert_eq!(session, SessionKey::remote("studio", "P1", "G1"));
        assert_eq!(session_machine_tab(&session), "studio");
    }

    #[test]
    fn a_local_sessions_banner_names_the_local_tab() {
        let tag = SessionKey::local("P1", "G1").to_sidebar_session_id();
        let session = system_notification_session(&tag).expect("a local banner names a session");
        assert_eq!(session, SessionKey::local("P1", "G1"));
        assert_eq!(
            session_machine_tab(&session),
            ghostex_gx_core::LOCAL_MACHINE_ID
        );
    }

    #[test]
    fn a_banner_tag_that_is_not_a_sidebar_id_names_nothing() {
        assert_eq!(system_notification_session(""), None);
        assert_eq!(system_notification_session("G1"), None);
        assert_eq!(system_notification_session("../etc/passwd"), None);
        assert_eq!(system_notification_session("remote:studio:"), None);
    }

    #[test]
    fn every_notification_click_takes_the_one_machine_switching_step() {
        // Issue #401: the OS banner focused the session without switching the sidebar to its
        // computer, while the feed did both. The banner (macOS, Windows and Linux all end in
        // `activate_system_notification`), the feed and a session link share one step.
        let banner = include_str!("../workspace_reconcile/sidebar_payloads.rs");
        let activation = &banner[banner
            .find("fn activate_system_notification")
            .expect("the banner click handler")..];
        let activation = &activation[..activation
            .find("\n    }\n")
            .expect("the end of the banner click handler")];
        assert!(activation.contains("gx_store_show_session_on_its_machine"));
        assert!(!activation.contains("dispatch_gpui_status_activation"));
        let feed = include_str!("notifications/commands.rs");
        assert!(feed.contains("gx_store_show_session_on_its_machine"));
        assert!(!feed.contains("SidebarUiIntent::SelectMachine"));
        let link = include_str!("session_deep_link.rs");
        assert!(link.contains("gx_store_show_session_on_its_machine"));
    }
}
