//! One-click New Session draft launching in the browser build (matching the desktop's
//! `new_agent_session.rs`, with no New Thread picker).
use gpui::Context;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

fn pick_primary_agent_id(hud_agents: &[Value], primary: Option<&str>) -> Option<String> {
    let mut ids = hud_agents.iter().filter_map(|row| {
        let id = row.get("agentId")?.as_str()?.trim();
        let name = row.get("name")?.as_str()?.trim();
        (!id.is_empty() && !name.is_empty()).then(|| id.to_string())
    });
    if let Some(primary) = primary
        && hud_agents.iter().any(|row| {
            row.get("agentId")
                .and_then(Value::as_str)
                .map(str::trim)
                == Some(primary)
        })
    {
        return Some(primary.to_string());
    }
    ids.next()
}

impl GhostexGpuiApp {
    /// One-click New session: immediately creates a Native draft in the open session's project
    /// (or Main Hub when none is open), with no picker.
    pub(crate) fn start_new_agent_session(&mut self, cx: &mut Context<Self>) {
        let target_group_id = self.gx_store_new_session_default_group_id();
        self.start_new_agent_session_target(target_group_id, cx);
    }

    /// Starts a new agent draft aimed at `group_id`.
    pub(crate) fn start_new_agent_session_in_group(
        &mut self,
        group_id: String,
        cx: &mut Context<Self>,
    ) {
        self.start_new_agent_session_target(Some(group_id), cx);
    }

    fn start_new_agent_session_target(
        &mut self,
        target_group_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let hud_agents = self
            .new_thread_picker_agents
            .clone()
            .or_else(|| {
                self.native_sidebar
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.hud["agents"].as_array().cloned())
            })
            .or_else(|| {
                self.gx_store_launch_hud()
                    .and_then(|hud| hud["agents"].as_array().cloned())
            })
            .unwrap_or_default();
        let primary = crate::app::gx_store::read_primary_agent_launcher_id();
        let Some(agent_id) = pick_primary_agent_id(&hud_agents, primary.as_deref()) else {
            return;
        };
        // If already viewing an untouched draft in the target project, focus its composer instead
        // of recreating it (and apply pending coordinator mode if requested).
        if let Some(open) = self.open_session.clone()
            && target_group_id
                .as_deref()
                .and_then(ghostex_gx_core::ProjectKey::parse_sidebar_group_id)
                .is_some_and(|project| open.project_key() == project)
            && self.gx_store_is_tracked_untouched_draft(&open)
            && self.gx_store_draft_chat_view_untouched(&open, cx)
            && let Some((_, view)) = self.native_chats.get(&open).cloned()
        {
            let want_coordinator = self
                .gx_store
                .unified_sidebar
                .carry_coordinator_draft
                .replace(false);
            view.update(cx, |view, cx| {
                if want_coordinator {
                    view.draft_coordinator_mode = true;
                    view.draft_setup_changed = true;
                }
                view.focus_requested = true;
                cx.notify();
            });
            return;
        }
        let mut message = json!({ "type": "runSidebarAgent", "agentId": agent_id });
        if let Some(group_id) = target_group_id {
            message["groupId"] = json!(group_id);
        }
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }
}
