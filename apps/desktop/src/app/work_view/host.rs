//! Where the Work view lives in the window: when it is available, its page's URL and surface,
//! and the ways it opens (the sidebar's briefcase, "Open a view", a session card's chip).

use gpui::AnyElement;
use gpui::Window;
use serde_json::Value;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// What the window remembers for its Work page between the page's requests.
#[derive(Default)]
pub(crate) struct WorkViewState {
    /// A ticket a chip asked to open, held until the page says it opened it (`work.ackOpen`), so a
    /// page that is still loading gets it with its `work.ready` answer.
    pub(crate) pending_open: Option<Value>,
    /// The current session the page was last told about (current_session.rs).
    pub(crate) current_session_sent: Option<Value>,
}

impl GhostexGpuiApp {
    /// The work-mode projects of this computer that the window's sidebar shows.
    pub(crate) fn work_view_project_ids(&self) -> Vec<String> {
        self.native_sidebar
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.work_mode_project_ids())
            .unwrap_or_default()
    }

    /// CDXC:WorkMode 2026-10-09 DECISION:
    /// User: Personal work-mode projects get the Work list too, so the view is there whenever the
    /// window shows any project with Work mode on, in any workspace.
    pub(crate) fn work_view_available(&self) -> bool {
        !self.work_view_project_ids().is_empty()
    }

    /// The sidebar list changed and the window no longer shows any work-mode project (Workspaces
    /// switched off, the last project's Work mode turned off or moved away): an open Work view
    /// leaves like any view whose project context went away, and its page is released.
    ///
    /// CDXC:WorkMode 2026-10-10 WHY: The settings-save refresh that moves off a switched-off view
    /// runs before gxserver republishes the projects without work mode, so it still saw the Work
    /// view as available; the tab then stayed open with no label over a "Turn on Work mode" note.
    pub(crate) fn leave_work_view_if_unavailable(
        &mut self,
        was_available: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if was_available && !self.work_view_available() {
            self.coerce_active_mode_to_available_project_context(cx);
            self.prune_project_workarea_runtime_cef_surfaces_for_current_gates(cx);
        }
    }

    pub(crate) fn work_view_showing(&self) -> bool {
        self.active_mode == TitlebarMode::Work
    }

    /// The bundled page, or nothing when this build has no `work.html`.
    pub(crate) fn work_view_page_url(&self) -> Option<ProjectWorkareaRealRuntimeUrl> {
        let url = gpui_cef_html_entry_url("GHOSTEX_GPUI_WORK_URL", "work.html").ok()?;
        ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(url)
    }

    /// The sidebar's briefcase: opens Work as a view tab, and a second click closes it like its
    /// tab's close button does.
    pub(crate) fn toggle_work_view(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.work_view_showing() {
            self.close_view_tab(TitlebarMode::Work, window, cx);
        } else if self.titlebar_mode_available(TitlebarMode::Work) {
            self.open_view_tab(TitlebarMode::Work, window, cx);
        }
    }

    /// Opens one ticket's details in the Work view (a session card's chip). `item` names it the
    /// way `/api/readWorkItem` takes it. Returns false when the window cannot show the Work view,
    /// and the caller opens the chip's link instead.
    pub(crate) fn open_work_item(&mut self, item: Value, cx: &mut gpui::Context<Self>) -> bool {
        if !self.titlebar_mode_available(TitlebarMode::Work) || self.work_view_page_url().is_none()
        {
            return false;
        }
        self.work_view.pending_open = Some(item.clone());
        // A page that is already loaded (showing, or behind another tab) takes the ticket now; one
        // still loading asks for it with `work.ready`.
        if self
            .project_workarea_runtime_cef_surfaces
            .contains_key(&ProjectWorkareaCefSurfaceSlotKey::Work)
        {
            self.dispatch_project_workarea_json_event(
                ProjectWorkareaCefSurfaceSlotKey::Work,
                "ghostex-work-open",
                &item.to_string(),
                cx,
            );
        }
        if !self.work_view_showing() {
            self.defer_in_main_window(cx, |this, window, cx| {
                this.open_view_tab(TitlebarMode::Work, window, cx);
            });
        }
        true
    }

    /// The Create Linear Ticket dialog made a ticket (from the Work page's New ticket or a
    /// project's "…" menu): a loaded Work page reads its list again so the ticket shows.
    pub(crate) fn work_view_ticket_created(&mut self, cx: &mut gpui::Context<Self>) {
        if self
            .project_workarea_runtime_cef_surfaces
            .contains_key(&ProjectWorkareaCefSurfaceSlotKey::Work)
        {
            self.dispatch_project_workarea_json_event(
                ProjectWorkareaCefSurfaceSlotKey::Work,
                "ghostex-work-refresh",
                "{}",
                cx,
            );
        }
    }

    /// The work-mode projects the window shows, as `{ projectId, name }` for the Create Linear
    /// Ticket dialog's Project picker.
    pub(crate) fn work_view_projects(&self) -> Vec<Value> {
        let mut projects: Vec<Value> = Vec::new();
        for group in self
            .native_sidebar
            .snapshot
            .iter()
            .flat_map(|snapshot| snapshot.groups.iter())
        {
            if let Some(project_id) = group.work_mode_project_id()
                && !projects
                    .iter()
                    .any(|project| project["projectId"] == project_id)
            {
                projects.push(serde_json::json!({ "projectId": project_id, "name": group.title }));
            }
        }
        projects
    }

    pub(crate) fn render_work_view_surface(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Work;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let message = if !self.work_view_available() {
            "Turn on Work mode for a project (right-click it → Work Mode) to see its tickets here."
        } else if self.work_view_page_url().is_none() {
            "This build of Ghostex has no Work page."
        } else {
            return self.render_view_skeleton(TitlebarMode::Work);
        };
        self.render_project_editor_placeholder(
            ProjectEditorPlaceholderSignature {
                mode: TitlebarMode::Work,
                title: Some("Work".to_string()),
                message: message.to_string(),
                actions: Vec::new(),
            },
            cx,
        )
    }
}

/// The agents a work-mode project's launcher offers, from the sidebar's own launcher menu: the
/// Start chat menu lists the same agents with the same default.
pub(crate) fn work_view_launcher_agents(header_actions: &[Value]) -> Vec<Value> {
    fn walk(value: &Value, agents: &mut Vec<Value>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| walk(item, agents)),
            Value::Object(object) => {
                let command = object.get("command");
                let launch = command
                    .and_then(|command| command.get("type"))
                    .and_then(Value::as_str)
                    == Some("agentAccounts")
                    && command
                        .and_then(|command| command.get("action"))
                        .and_then(Value::as_str)
                        == Some("launch");
                let agent_id = command
                    .and_then(|command| command.get("agentId"))
                    .and_then(Value::as_str);
                match (launch, agent_id) {
                    (true, Some(agent_id))
                        if !agents.iter().any(|agent| agent["id"] == agent_id) =>
                    {
                        agents.push(serde_json::json!({
                            "id": agent_id,
                            "name": object.get("label").cloned().unwrap_or(Value::Null),
                            "primary": object.get("primary").and_then(Value::as_bool).unwrap_or(false),
                            "iconDataUrl": object.get("imageDataUrl").cloned().unwrap_or(Value::Null),
                        }));
                    }
                    _ => object.values().for_each(|value| walk(value, agents)),
                }
            }
            _ => {}
        }
    }
    let mut agents = Vec::new();
    header_actions
        .iter()
        .for_each(|action| walk(action, &mut agents));
    agents
}
