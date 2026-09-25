//! A click on a row that lives on another machine: the store performs all of it.
//!
//! CDXC:RemoteMachines 2026-09-21 WHY:
//! A remote row's click used to leave Rust for QuickJS, be routed there, and come straight back
//! over the fixed native project-path bridge as `openRemoteSessionTerminal`. Everything that round
//! trip resolved (which machine, which project, the agent's Default Agent View, whether the focus
//! changes project) is in the store, so the store builds the same payload and performs it, in the
//! click's own frame, through the same function the bridge message reaches.
//!
//! **The command no longer reaches the old runtime.** What its remote branch did besides the open
//! is performed here, each at its end:
//! - the attention acknowledgement goes to the store's attention intent (gx_store/attention/),
//!   BEFORE the open, as `focusSession` acknowledged first; the store keeps the minimum visible
//!   window, the optimistic clear and the machine's `/api/updateAgentActivity` call for every door,
//!   so the timer stays one implementation;
//! - the remote focus (`setRemotePresentationSessionFocus`) and its publish are what the open's own
//!   tab-selected callback runs (`set_sidebar_gxserver_remote_attach_focus_state`, reached
//!   synchronously from `begin_gpui_remote_attach_terminal_open`), once per click;
//! - the page's half of a click (clear the multi-selection, close an open app modal) is the
//!   store's selection intent mirrored to the page, and the same close the bridge performs.
//!
//! So there is no runtime copy of the open any more, and nothing to drop: the echo marker, the
//! page-entry duplicate check and the three counters that measured the copy are gone together.
//!
//! **The focus is the store's own** (remote focus part 2 step 2, and the runtime's focus gone
//! since 2026-09-25). The open's tab selection reaches `dispatch_gpui_workspace_tab_session_selected`,
//! whose remote branch hands the row to the core's focus (`gx_store_select_remote_session` in
//! local_focus.rs) in the click's frame, so the row draws focused with the pane, and the workspace
//! is published from it.
//!
//! **`keepView` is planned from the group the runtime's `activeGroupId` would hold**, which the
//! store's focus is now (`gx_store_remote_focus_group` in focus_perform.rs): a focused remote
//! session answers its project's own group or its machine's Chats, never the user-made group its
//! row sits in, because that is the group `setRemotePresentationSessionFocus` named and the string
//! rule in gx-core `plan_remote_focus` was written against it. This supersedes the
//! `RuntimeActiveGroup` tracking of what was sent to and published by the runtime.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/remote_focus.rs,
//! apps/desktop/src/app/native_sidebar/actions.rs, apps/desktop/src/app/remote_conn/native_action.rs
//! (`handle_gpui_remote_session_native_action`, which ends in `begin_gpui_remote_attach_terminal_open`),
//! packages/gx-core/src/attention.rs.

use ghostex_gx_core::{
    PreferredInterfaceSettings, RemoteFocusPlan, SessionKey, plan_remote_focus,
};
use serde_json::{Value, json};

use super::diagnostics::{record, routine_logging_enabled};
use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_preferred_agent_interface_from_settings;
use crate::app::model::GpuiPreferredAgentInterface;
use crate::shared_settings;

/// Per-click lines one app run may write.
const MAX_REMOTE_FOCUS_RECORDS: u32 = 200;

/// What this app run did with remote row clicks. Memory only; the log lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarRemoteFocusCounters {
    /// Clicks the store answered, and each kind of them.
    pub(crate) opens: u64,
    pub(crate) splits: u64,
    /// Of those, the ones that carried each option.
    pub(crate) keep_view: u64,
    pub(crate) chat_interface: u64,
    /// Attention acknowledgements asked of the store, one per answered click on a streamed machine.
    pub(crate) acknowledgements: u64,
    /// Remote tab selections, from any sender (the store's opens and a slow attach landing). Each
    /// one moves the store's focus: once per click on a row whose tab exists, and for a row that
    /// needs an attach once in the click and once when it lands.
    pub(crate) tab_selections: u64,
    /// An answered click whose open made no tab selection: the open was refused (no tunnel, no SSH
    /// settings, a toast said so), and the focus did not move.
    pub(crate) marks_missed: u64,
    /// A remote row's click the store did not answer because the renderer is not drawing its list.
    /// Local rows are not counted: they were never this path's.
    pub(crate) declined_source: u64,
    /// A remote row's click the planner refused. Nothing takes such a click any more (a machine
    /// that is offline or has not streamed yet is answered too), so this stays zero on a healthy
    /// run. Local and browser rows, and ids that do not parse as remote, are not counted.
    pub(crate) handed_back: u64,
    /// Remote selections the store's core focus took (`gx_store_select_remote_session`), from any
    /// sender: one per tab selection sent, so it follows `tab_selections`.
    pub(crate) core_focus: u64,
    /// Of those, the ones naming a row the machine does not list, which the core refused and
    /// holds until the row arrives (focus_publish.rs).
    pub(crate) core_unplaced: u64,
}

/// The counters and the log budget.
#[derive(Default)]
pub(crate) struct SidebarRemoteFocusHost {
    pub(crate) counters: SidebarRemoteFocusCounters,
    records: u32,
}

impl SidebarRemoteFocusHost {
    /// The core's focus took a remote selection, or refused a row it does not hold.
    pub(super) fn note_core_selection(&mut self, placed: bool) {
        self.counters.core_focus += 1;
        if !placed {
            self.counters.core_unplaced += 1;
        }
    }

}

impl GhostexGpuiApp {
    /// The store's answer to a sidebar command that selects a remote row, or `None` when it names
    /// no remote row (a local row's click goes on to `sidebar_focus_route.rs`). Performs nothing:
    /// the caller hands the plan to [`Self::gx_store_focus_remote_row`].
    ///
    /// Two shapes arrive. `selectSession` with `mode: focus` is the RENDERER command a row click
    /// sends, which `selectNativeSidebarSession` turns into `{ type: 'focusSession', sessionId }`
    /// before it posts it, and that translation is reproduced here rather than a second reading of
    /// the click. `splitSessionRight` is a gxserver message and arrives WRAPPED.
    pub(crate) fn gx_store_plan_remote_row_focus(
        &mut self,
        command: &Value,
    ) -> Option<RemoteFocusPlan> {
        let message = remote_focus_message(command)?;
        // Only a remote row is this path's, so only a remote row is counted when it is not
        // answered; a local click here is the store's own focus path and says nothing.
        let remote_row = message
            .get("sessionId")
            .and_then(Value::as_str)
            .and_then(SessionKey::parse_remote_scoped_session_id)
            .is_some();
        // The store's answer is only the right one while the store's list is the one on screen:
        // with the old projection drawn, its own state is what the row was built from.
        if !self.gx_store_sidebar_list_ready() {
            if remote_row {
                self.gx_store.sidebar_remote_focus.counters.declined_source += 1;
            }
            return None;
        }
        let settings = self.gx_store_preferred_interface_settings();
        let group = self.gx_store_remote_focus_group();
        let plan = plan_remote_focus(&self.gx_store.core, &message, &settings, group.as_deref());
        if plan.is_none() && remote_row {
            self.gx_store.sidebar_remote_focus.counters.handed_back += 1;
        }
        plan
    }

    /// The whole click on a remote row, in the old path's order: the page's half (the
    /// multi-selection cleared, an open app modal closed), then the runtime's (the attention
    /// acknowledged, then the open, whose tab-selected callback moves the remote focus marks and
    /// publishes them). `command` is the sidebar command the plan was made from; it is NOT sent on.
    pub(crate) fn gx_store_focus_remote_row(
        &mut self,
        command: &Value,
        plan: &RemoteFocusPlan,
        cx: &mut gpui::Context<Self>,
    ) {
        // `selectNativeSidebarSession` cleared the multi-selection and closed an open app modal
        // before it posted the click. The store's own selection intent already follows the command.
        self.gx_store_note_sidebar_command(command, cx);
        if !plan.split_right {
            // Closed BEFORE the open, so a keep-view open that leaves the keyboard where it is
            // cannot have it taken back by the modal's return focus a moment later.
            self.close_app_modal_from_bridge(cx);
        }
        // The open's selection follows the store's newest local one, whose follow-up runs first.
        self.gx_store_flush_local_selection(cx);
        // A machine this run has not streamed holds no live row to acknowledge (`plan.live`).
        if plan.live {
            self.gx_store_acknowledge_attention(plan.session.clone(), cx);
            self.gx_store.sidebar_remote_focus.counters.acknowledgements += 1;
        }
        let tab_selections = self.gx_store.sidebar_remote_focus.counters.tab_selections;
        {
            let counters = &mut self.gx_store.sidebar_remote_focus.counters;
            match plan.split_right {
                true => counters.splits += 1,
                false => counters.opens += 1,
            }
            if plan.keep_view {
                counters.keep_view += 1;
            }
            if plan.preferred_interface.as_deref() == Some("chat") {
                counters.chat_interface += 1;
            }
        }
        // The same function the bridge message reaches, with the same payload the old runtime
        // would have posted, so the machine lookup, the SSH configuration and the tunnel target are
        // the one implementation that already exists.
        self.receive_sidebar_native_project_path_action_payload(
            &plan.native_action.to_string(),
            cx,
        );
        let counters = &mut self.gx_store.sidebar_remote_focus.counters;
        if counters.tab_selections == tab_selections {
            counters.marks_missed += 1;
        }
        self.gx_store_record_remote_focus(plan);
    }

    /// A remote tab was selected (the store's own open, or an attach that completes later).
    pub(crate) fn gx_store_note_remote_tab_selection(&mut self) {
        self.gx_store.sidebar_remote_focus.counters.tab_selections += 1;
    }

    /// `resolveEffectivePreferredAgentInterface`'s inputs, read off the shared settings document
    /// the same way every other reader of the Default Agent View reads them.
    pub(super) fn gx_store_preferred_interface_settings(&self) -> PreferredInterfaceSettings {
        let snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings = snapshot.object();
        // The whole override map rather than one lookup: which agent the row has is the planner's
        // to read, and it reads it from the machine's own presentation. The value filter is the one
        // `gpui_preferred_agent_interface_override_from_settings` applies, so an unknown string is
        // an absent override on both paths.
        let overrides = settings
            .get("preferredAgentInterfaceOverrides")
            .and_then(serde_json::Value::as_object)
            .map(|overrides| {
                overrides
                    .iter()
                    .filter_map(|(agent_id, value)| {
                        let value = value.as_str()?;
                        GpuiPreferredAgentInterface::from_str(value)?;
                        Some((agent_id.clone(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        PreferredInterfaceSettings {
            default_interface: match gpui_preferred_agent_interface_from_settings(settings) {
                GpuiPreferredAgentInterface::Chat => "chat".to_string(),
                GpuiPreferredAgentInterface::Terminal => "terminal".to_string(),
            },
            overrides,
        }
    }

    /// One line per remote row click: which kind, and the two options that decide what the user
    /// sees. Nothing about the row itself, so no machine, project or session id is written.
    fn gx_store_record_remote_focus(&mut self, plan: &RemoteFocusPlan) {
        let host = &mut self.gx_store.sidebar_remote_focus;
        if host.records >= MAX_REMOTE_FOCUS_RECORDS || !routine_logging_enabled() {
            return;
        }
        host.records += 1;
        let counters = host.counters;
        record(
            "gxStore.sidebarRemoteFocus",
            json!({
                "splitRight": plan.split_right,
                "keepView": plan.keep_view,
                "chatInterface": plan.preferred_interface.as_deref() == Some("chat"),
                "totals": remote_focus_counters_json(&counters),
            }),
        );
    }

    /// The remote focus counters, for the periodic summary in `sidebar_remote.rs`.
    pub(super) fn gx_store_remote_focus_counters(&mut self) -> SidebarRemoteFocusCounters {
        self.gx_store.sidebar_remote_focus.counters
    }
}

/// The message a remote row's click means, in the shape the planner answers. `selectSession` is a
/// renderer command at the TOP level; `splitSessionRight` is a gxserver message and is wrapped.
fn remote_focus_message(command: &Value) -> Option<Value> {
    let kind = command.get("type").and_then(Value::as_str)?;
    if kind == "selectSession" {
        if command.get("mode").and_then(Value::as_str) != Some("focus") {
            return None;
        }
        let session_id = command.get("sessionId").and_then(Value::as_str)?;
        return Some(json!({ "type": "focusSession", "sessionId": session_id }));
    }
    if kind != "command" {
        return None;
    }
    let message = command.get("message")?;
    match message.get("type").and_then(Value::as_str) {
        Some("splitSessionRight") => Some(message.clone()),
        _ => None,
    }
}

/// The twelve keys both records carry, well under the sanitizer's 32-entry cap at depth 2. Counts
/// only: no id, title or path.
pub(super) fn remote_focus_counters_json(counters: &SidebarRemoteFocusCounters) -> Value {
    json!({
        "opens": counters.opens,
        "splits": counters.splits,
        "keepView": counters.keep_view,
        "chatInterface": counters.chat_interface,
        "acknowledgements": counters.acknowledgements,
        "tabSelections": counters.tab_selections,
        "marksMissed": counters.marks_missed,
        "declinedSource": counters.declined_source,
        "handedBack": counters.handed_back,
        "coreFocus": counters.core_focus,
        "coreUnplaced": counters.core_unplaced,
    })
}
