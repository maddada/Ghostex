//! The native Automate view's state: the loaded automations, the selected tab and rows, the busy
//! row, and the loads and mutations that go through the automation bridge logic.

use super::dialog::AutomationDialog;
use super::model::{AutomationSessionOption, AutomationState, parse_session_options};
use super::requests::{AutomateRequest, AutomateScope, run_automate_request};
use crate::GhostexGpuiApp;
use crate::app::helpers::{GpuiAutomationBoardNavigation, ProjectBoardBridgeRuntimeContext};
use crate::app::model::TitlebarMode;
use crate::app::window::ModalPalette;
use gpui::{Bounds, Context, Entity, Pixels, ScrollHandle, Task, WeakEntity, WindowHandle};
use gpui_component::Root;
use std::time::Duration;

/// `PROJECT_BOARD_AUTO_REFRESH_INTERVAL_MS`: the React page's refresh cadence while visible.
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_millis(8_000);

/// Prefix of the sidebar-runtime `getState` request ids this view sends for Thread mode's
/// session list; the app hands matching responses back through `receive_sessions_response`.
pub(crate) const NATIVE_AUTOMATE_SESSIONS_REQUEST_PREFIX: &str = "native-automate-sessions:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomateTab {
    Automations,
    Runs,
    Triage,
}

impl AutomateTab {
    pub(crate) const ALL: [Self; 3] = [Self::Automations, Self::Runs, Self::Triage];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Automations => "Automations",
            Self::Runs => "Runs",
            Self::Triage => "Triage",
        }
    }
}

/// What the app hands the view each frame so it can open its dialog window centred on the main
/// window in the app modals' palette.
#[derive(Clone)]
pub(crate) struct AutomateHostInfo {
    #[cfg(target_os = "linux")]
    pub(crate) window: gpui::AnyWindowHandle,
    pub(crate) main_window_bounds: Bounds<Pixels>,
    pub(crate) display_id: Option<gpui::DisplayId>,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct OpenAutomationDialog {
    pub(crate) window: WindowHandle<Root>,
    pub(crate) dialog: Entity<AutomationDialog>,
}

pub(crate) struct NativeAutomateView {
    pub(crate) app: WeakEntity<GhostexGpuiApp>,
    pub(crate) scope: Option<AutomateScope>,
    pub(crate) context: Option<ProjectBoardBridgeRuntimeContext>,
    pub(crate) host: Option<AutomateHostInfo>,
    /// Window glass and light appearance at the last sync, so a change redraws the cached page.
    appearance: Option<(bool, bool)>,
    /// `None` until the first load for the current scope answers.
    pub(crate) state: Option<AutomationState>,
    pub(crate) loading: bool,
    /// The first load failed, so there is nothing to show but the error and Retry.
    pub(crate) load_error: Option<String>,
    /// The notice under the header (`errorMessage` on the React page).
    pub(crate) error_message: Option<String>,
    /// `automationActionId`: the automation or run a request is in flight for.
    pub(crate) busy_id: Option<String>,
    pub(crate) tab: AutomateTab,
    pub(crate) selected_automation_id: Option<String>,
    pub(crate) selected_run_id: Option<String>,
    pub(crate) sessions: Vec<AutomationSessionOption>,
    pub(crate) dialog: Option<OpenAutomationDialog>,
    pub(crate) list_scroll: ScrollHandle,
    pub(crate) detail_scroll: ScrollHandle,
    load_generation: u64,
    sessions_generation: u64,
    _refresh_task: Task<()>,
}

impl NativeAutomateView {
    pub(crate) fn new(app: WeakEntity<GhostexGpuiApp>, cx: &mut Context<Self>) -> Self {
        let refresh_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(AUTO_REFRESH_INTERVAL).await;
                let alive = this.update(cx, |this, cx| {
                    if this.shown(cx) && !this.loading {
                        this.load(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            app,
            scope: None,
            context: None,
            host: None,
            appearance: None,
            state: None,
            loading: false,
            load_error: None,
            error_message: None,
            busy_id: None,
            tab: AutomateTab::Automations,
            selected_automation_id: None,
            selected_run_id: None,
            sessions: Vec::new(),
            dialog: None,
            list_scroll: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            load_generation: 0,
            sessions_generation: 0,
            _refresh_task: refresh_task,
        }
    }

    /// Whether the Automate view is the awake, active view panel page right now.
    fn shown(&self, cx: &Context<Self>) -> bool {
        self.app.upgrade().is_some_and(|app| {
            let app = app.read(cx);
            app.active_mode == TitlebarMode::Automate
                && app
                    .project_editor_shell
                    .is_mode_awake(TitlebarMode::Automate)
        })
    }

    /// Called by the app on every frame it hosts the view. A new scope (the user switched
    /// project) drops the old project's data and loads the new one. True when the page must be
    /// redrawn: the scope or the (glass, light) `appearance` changed.
    pub(crate) fn sync(
        &mut self,
        scope: AutomateScope,
        context: Option<ProjectBoardBridgeRuntimeContext>,
        host: AutomateHostInfo,
        appearance: (bool, bool),
        cx: &mut Context<Self>,
    ) -> bool {
        self.context = context;
        self.host = Some(host);
        let appearance_changed = self.appearance.replace(appearance) != Some(appearance);
        if self.scope.as_ref() == Some(&scope) {
            return appearance_changed;
        }
        let load = !scope.coming_soon;
        self.scope = Some(scope);
        self.state = None;
        self.load_error = None;
        self.error_message = None;
        self.busy_id = None;
        self.selected_automation_id = None;
        self.selected_run_id = None;
        self.sessions.clear();
        self.loading = false;
        if load {
            self.load(cx);
        }
        true
    }

    /// True while the view has nothing to show yet; the host draws the Automate skeleton.
    pub(crate) fn is_initial_loading(&self) -> bool {
        self.state.is_none()
            && self.load_error.is_none()
            && self.scope.as_ref().is_some_and(|scope| !scope.coming_soon)
    }

    pub(crate) fn is_all_projects(&self) -> bool {
        self.scope.as_ref().is_some_and(|scope| scope.all_projects)
    }

    /// `automationProjectPathForId`.
    pub(crate) fn project_path_for(&self, project_id: &str) -> Option<String> {
        let scope = self.scope.as_ref()?;
        self.state
            .as_ref()
            .and_then(|state| state.project(project_id))
            .map(|project| project.path.clone())
            .or_else(|| (project_id == scope.project_id).then(|| scope.project_path.clone()))
            .or_else(|| self.state.as_ref().map(|state| state.project_path.clone()))
    }

    /// The project an automation or run belongs to, with the page's own project as fallback.
    pub(crate) fn target_project_id(&self, candidate: Option<&str>) -> String {
        candidate
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .or_else(|| {
                self.state
                    .as_ref()
                    .map(|state| state.project_id.clone())
                    .filter(|id| !id.is_empty())
            })
            .or_else(|| self.scope.as_ref().map(|scope| scope.project_id.clone()))
            .unwrap_or_default()
    }

    pub(crate) fn load(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        if scope.coming_soon {
            return;
        }
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loading = true;
        let request =
            AutomateRequest::Load.to_bridge_request(&scope, |id| self.project_path_for(id));
        let context = self.context.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { run_automate_request(&request, context.as_ref()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.load_generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok((state, _)) => {
                        this.apply_state(AutomationState::from_json(&state));
                        this.load_error = None;
                    }
                    // A failed refresh keeps what is on screen, as the React page did.
                    Err(error) if this.state.is_none() => this.load_error = Some(error),
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_state(&mut self, state: AutomationState) {
        self.state = Some(state);
    }

    /// Runs a mutation and applies the state it answers with. `on_success` runs after the state
    /// is applied (closing the dialog, switching to Runs); `on_failure` gets the error.
    pub(crate) fn mutate(
        &mut self,
        request: AutomateRequest,
        on_success: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        on_failure: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let failure_message = request.failure_message();
        self.busy_id = request.busy_id();
        let bridge_request = request.to_bridge_request(&scope, |id| self.project_path_for(id));
        let context = self.context.clone();
        let background = cx.background_executor().clone();
        let app = self.app.clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { run_automate_request(&bridge_request, context.as_ref()) })
                .await;
            let (result, navigation) = match result {
                Ok((state, navigation)) => (Ok(state), navigation),
                Err(error) => (Err(error), None),
            };
            let _ = this.update(cx, |this, cx| {
                this.busy_id = None;
                match result {
                    Ok(state) => {
                        this.error_message = None;
                        // All Automations reloads every project after a change, like the React page.
                        if this.is_all_projects() {
                            this.load(cx);
                        } else {
                            this.apply_state(AutomationState::from_json(&state));
                        }
                        on_success(this, cx);
                    }
                    Err(error) => {
                        let message = if error.trim().is_empty() {
                            failure_message.to_string()
                        } else {
                            error
                        };
                        on_failure(this, message, cx);
                    }
                }
                cx.notify();
            });
            if let Some(navigation) = navigation {
                let _ = app.update(cx, |app, cx| match navigation {
                    GpuiAutomationBoardNavigation::FocusSession(focus_id) => {
                        let _ = app.dispatch_gpui_command_palette_session_focus(&focus_id, cx);
                    }
                    GpuiAutomationBoardNavigation::FocusProject(project_id) => {
                        let _ = app.dispatch_gpui_menu_bar_project_activation(&project_id, cx);
                    }
                    GpuiAutomationBoardNavigation::RevealWorktreePath(path) => {
                        let _ =
                            crate::app::helpers::gpui_spawn_os_open(std::ffi::OsStr::new(&path));
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    /// A mutation whose failure shows in the page notice.
    pub(crate) fn mutate_with_notice(&mut self, request: AutomateRequest, cx: &mut Context<Self>) {
        self.mutate(
            request,
            |_, _| {},
            |this, error, _| this.error_message = Some(error),
            cx,
        );
    }

    /// Asks the store (gx_store/create/board.rs) for the project's agent sessions (Thread mode's
    /// picker), the `getState` board request the React dialog sent when it opened.
    pub(crate) fn request_sessions(&mut self, project_id: &str, cx: &mut Context<Self>) {
        let Some(scope) = self.scope.as_ref() else {
            return;
        };
        self.sessions_generation += 1;
        self.sessions.clear();
        let mut request = serde_json::json!({
            "action": "getState",
            "projectEditorId": scope.editor_id,
            "projectId": project_id,
            "requestId": format!(
                "{NATIVE_AUTOMATE_SESSIONS_REQUEST_PREFIX}{}",
                self.sessions_generation
            ),
        });
        if let Some(path) = self.project_path_for(project_id).filter(|p| !p.is_empty()) {
            request["projectPath"] = serde_json::Value::String(path);
        }
        if let Some(remote) = self
            .context
            .as_ref()
            .and_then(|context| context.remote_machine_id.clone())
        {
            request["remoteMachineId"] = serde_json::Value::String(remote);
        }
        let app = self.app.clone();
        cx.spawn(async move |_, cx| {
            let _ = app.update(cx, |app, cx| {
                app.dispatch_gpui_project_board_conversation_request(&request, cx)
            });
        })
        .detach();
    }

    /// The store's answer to `request_sessions`; stale answers are dropped.
    pub(crate) fn receive_sessions_response(
        &mut self,
        response: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let current = format!(
            "{NATIVE_AUTOMATE_SESSIONS_REQUEST_PREFIX}{}",
            self.sessions_generation
        );
        if response["requestId"].as_str() != Some(current.as_str()) {
            return;
        }
        self.sessions = if response["ok"].as_bool() == Some(true) {
            parse_session_options(&response["payload"])
        } else {
            Vec::new()
        };
        if let Some(open) = self.dialog.as_ref() {
            let sessions = self.sessions.clone();
            open.dialog
                .update(cx, |dialog, cx| dialog.set_sessions(sessions, cx));
        }
    }

    pub(crate) fn set_tab(&mut self, tab: AutomateTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    pub(crate) fn copy_text(&self, text: &str, cx: &mut Context<Self>) {
        crate::app::helpers::gpui_play_copy_sound();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_string()));
    }
}
