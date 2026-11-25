//! What the Automate view's buttons do: the same bridge actions the React page sent, plus opening
//! the create/edit dialog.

use super::dialog::{AutomationDialog, AutomationDialogConfig};
use super::drafts::{AutomationDraft, ExecutionKind};
use super::model::{AutomationDefinition, AutomationRun};
use super::requests::AutomateRequest;
use super::view::{AutomateTab, NativeAutomateView, OpenAutomationDialog};
use gpui::{AppContext as _, Context, Styled as _, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;
use std::cell::RefCell;
use std::rc::Rc;

/// The dialog window's width and its first-frame height before it fits its content.
const DIALOG_WIDTH: f32 = 560.0;
const DIALOG_INITIAL_HEIGHT: f32 = 640.0;

impl NativeAutomateView {
    pub(crate) fn set_enabled(
        &mut self,
        automation: &AutomationDefinition,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let project_id = self.target_project_id(automation.project_id());
        self.mutate_with_notice(
            AutomateRequest::SetEnabled {
                automation_id: automation.id.clone(),
                project_id,
                enabled,
            },
            cx,
        );
    }

    pub(crate) fn run_now(&mut self, automation: &AutomationDefinition, cx: &mut Context<Self>) {
        let project_id = self.target_project_id(automation.project_id());
        self.mutate(
            AutomateRequest::RunNow {
                automation_id: automation.id.clone(),
                project_id,
            },
            |this, cx| this.set_tab(AutomateTab::Runs, cx),
            |this, error, _| this.error_message = Some(error),
            cx,
        );
    }

    /// The React page deletes without asking, and so does this view.
    pub(crate) fn delete(&mut self, automation: &AutomationDefinition, cx: &mut Context<Self>) {
        let project_id = self.target_project_id(automation.project_id());
        self.mutate_with_notice(
            AutomateRequest::Delete {
                automation_id: automation.id.clone(),
                project_id,
            },
            cx,
        );
    }

    pub(crate) fn archive_run(&mut self, run: &AutomationRun, cx: &mut Context<Self>) {
        let project_id = self.target_project_id(Some(&run.project_id));
        self.mutate_with_notice(
            AutomateRequest::ArchiveRun {
                run_id: run.id.clone(),
                project_id,
            },
            cx,
        );
    }

    pub(crate) fn mark_run_read(&mut self, run: &AutomationRun, cx: &mut Context<Self>) {
        let project_id = self.target_project_id(Some(&run.project_id));
        self.mutate_with_notice(
            AutomateRequest::MarkRunRead {
                run_id: run.id.clone(),
                project_id,
            },
            cx,
        );
    }

    pub(crate) fn mark_all_runs_read(&mut self, automation_id: &str, cx: &mut Context<Self>) {
        let (Some(scope), Some(state)) = (self.scope.clone(), self.state.as_ref()) else {
            return;
        };
        let requests: Vec<serde_json::Value> = state
            .visible_runs()
            .filter(|run| run.automation_id == automation_id && run.is_unread)
            .map(|run| {
                let project_id = self.target_project_id(Some(&run.project_id));
                AutomateRequest::MarkRunRead {
                    run_id: run.id.clone(),
                    project_id,
                }
                .to_bridge_request(&scope, |id| self.project_path_for(id))
            })
            .collect();
        if requests.is_empty() {
            return;
        }
        self.busy_id = Some(automation_id.to_string());
        let context = self.context.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    let mut last_state = None;
                    for request in &requests {
                        let (state, _) =
                            super::requests::run_automate_request(request, context.as_ref())?;
                        last_state = Some(state);
                    }
                    Ok::<Option<serde_json::Value>, String>(last_state)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy_id = None;
                match result {
                    Ok(last_state) => {
                        this.error_message = None;
                        if this.is_all_projects() {
                            this.load(cx);
                        } else if let Some(state) = last_state {
                            this.state = Some(super::model::AutomationState::from_json(&state));
                        }
                    }
                    Err(error) => {
                        let message = if error.trim().is_empty() {
                            "Could not mark automation runs read.".to_string()
                        } else {
                            error
                        };
                        this.error_message = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// CDXC:Automations 2026-11-25 WHY: GitHub #719: opening a run's session is how you check
    /// it, but only the bell, Mark all read and Archive cleared unread, so the "N unread" badge
    /// stayed up for runs the user had already read. Opening the session now marks it read too,
    /// once the session has opened. Selecting a run in the list still does not, so the
    /// unread-only list the badge opens keeps its rows while the user looks through them.
    pub(crate) fn open_run_session(&mut self, run: &AutomationRun, cx: &mut Context<Self>) {
        let project_id = self.target_project_id(Some(&run.project_id));
        let unread = run.is_unread.then(|| run.clone());
        self.mutate(
            AutomateRequest::OpenRunSession {
                run_id: run.id.clone(),
                project_id,
            },
            move |this, cx| {
                if let Some(run) = unread {
                    this.mark_run_read(&run, cx);
                }
            },
            |this, error, _| this.error_message = Some(error),
            cx,
        );
    }

    pub(crate) fn open_run_worktree(&mut self, run: &AutomationRun, cx: &mut Context<Self>) {
        let project_id = self.target_project_id(Some(&run.project_id));
        self.mutate_with_notice(
            AutomateRequest::OpenRunWorktree {
                run_id: run.id.clone(),
                project_id,
            },
            cx,
        );
    }

    /// `openNewAutomationDialog`.
    pub(crate) fn open_create_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let project_id = if self.is_all_projects() {
            state
                .projects
                .first()
                .map(|project| project.project_id.clone())
                .unwrap_or_else(|| state.project_id.clone())
        } else {
            state.project_id.clone()
        };
        let can_use_worktrees = if self.is_all_projects() {
            state
                .project(&project_id)
                .is_some_and(|project| project.can_use_worktrees)
        } else {
            state.project_can_use_worktrees
        };
        let draft = AutomationDraft::new(self.default_agent_id(), project_id, can_use_worktrees);
        self.open_dialog(draft, cx);
    }

    /// `openEditAutomationDialog`.
    pub(crate) fn open_edit_dialog(
        &mut self,
        automation: &AutomationDefinition,
        cx: &mut Context<Self>,
    ) {
        let project_id = self.target_project_id(automation.project_id());
        let draft = AutomationDraft::from_definition(automation, project_id);
        self.open_dialog(draft, cx);
    }

    /// `resolveAutomationDraftAgentId`: the Default Prompt Agent when it is launchable here, else
    /// the first agent.
    pub(crate) fn default_agent_id(&self) -> String {
        let Some(state) = self.state.as_ref() else {
            return String::new();
        };
        let default = state.default_agent_id.as_deref().map(str::trim);
        state
            .agents
            .iter()
            .find(|agent| Some(agent.agent_id.as_str()) == default)
            .or_else(|| state.agents.first())
            .map(|agent| agent.agent_id.clone())
            .unwrap_or_default()
    }

    /// Opens the create/edit form in its own borderless child window centred on the main window,
    /// the way the native app modals open (native_app_modal_lifecycle.rs). Reopening replaces it.
    fn open_dialog(&mut self, draft: AutomationDraft, cx: &mut Context<Self>) {
        let (Some(state), Some(host), Some(scope)) =
            (self.state.clone(), self.host.clone(), self.scope.clone())
        else {
            return;
        };
        self.close_dialog(cx);
        self.request_sessions(&draft.project_id, cx);
        let palette = host.palette;
        let config = AutomationDialogConfig {
            palette,
            all_projects: scope.all_projects,
            project_name: scope.project_name.clone(),
            state,
            sessions: self.sessions.clone(),
            draft,
        };
        let view = cx.weak_entity();
        let window_bounds = gpui::Bounds::centered_at(
            host.main_window_bounds.center(),
            size(px(DIALOG_WIDTH), px(DIALOG_INITIAL_HEIGHT)),
        );
        // A child window like every app modal: on Windows that makes it a popup owned by the main
        // window, so it moves along when the main window moves (CDXC:AppModal 2026-10-04 in
        // workspace_windows/owned_windows.rs) and stays off the taskbar and out of FancyZones.
        let options = WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            window_decorations: crate::app::window::popup_frame::child_window_decorations(),
            #[cfg(target_os = "linux")]
            x11_parent: Some(host.window),
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            app_id: crate::gpui_platform_window_app_id(),
            focus: true,
            icon: crate::gpui_platform_window_icon(),
            show: true,
            is_resizable: false,
            is_minimizable: false,
            display_id: crate::app::window::popup_frame::PopupOwner::new(
                host.main_window_bounds,
                host.display_id,
            )
            .display_for(window_bounds, cx),
            titlebar: None,
            // The dialog draws the app modals' palette, frosted under window glass.
            window_background: crate::app::helpers::modal_window_glass_background_appearance(),
            ..Default::default()
        };
        let dialog_slot = Rc::new(RefCell::new(None));
        let dialog_out = dialog_slot.clone();
        let window_border = palette.window_border();
        #[cfg(target_os = "macos")]
        let main_window_native_view = host.main_window_native_view;
        #[cfg(not(target_os = "macos"))]
        let main_window_native_view = std::ptr::null_mut();
        let window = cx
            .open_window(options, move |window, cx| {
                crate::app::window::popup_frame::frame_app_modal_window(window, window_border);
                window.set_window_title("");
                crate::app::helpers::apply_frosted_menu_blur(window);
                window.activate_window();
                crate::app::window::attach_gpui_app_modal_window_to_main_window(
                    window,
                    main_window_native_view,
                );
                let dialog = cx.new(|cx| AutomationDialog::new(config, view, window, cx));
                *dialog_out.borrow_mut() = Some(dialog.clone());
                let frame = cx.new(|_| crate::app::window::ModalWindowFrame::new(dialog, palette));
                cx.new(|cx| {
                    Root::new(frame, window, cx)
                        .bordered(false)
                        .bg(gpui::transparent_black())
                })
            })
            .ok();
        let dialog = dialog_slot.borrow_mut().take();
        if let (Some(window), Some(dialog)) = (window, dialog) {
            self.dialog = Some(OpenAutomationDialog { window, dialog });
        }
    }

    pub(crate) fn close_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = self.dialog.take() {
            let _ = open
                .window
                .update(cx, |_, window, _| window.remove_window());
        }
    }

    /// The dialog's Save (`saveAutomation`). A draft that cannot be saved answers synchronously so
    /// the dialog shows why; a gxserver failure reaches the dialog once the request returns.
    pub(crate) fn save_draft(
        &mut self,
        draft: &AutomationDraft,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| "Automations are still loading.".to_string())?;
        let project_id = if self.is_all_projects() {
            state
                .project(&draft.project_id)
                .map(|project| project.project_id.clone())
                .or_else(|| state.projects.first().map(|p| p.project_id.clone()))
                .unwrap_or_default()
        } else {
            self.target_project_id(Some(&draft.project_id))
        };
        if project_id.is_empty() {
            return Err("Choose a project before saving automation.".to_string());
        }
        let definition = draft
            .to_definition_json(&self.default_agent_id(), &project_id)
            .ok_or_else(|| "Name, agent, prompt, and schedule are required.".to_string())?;
        if draft.execution_kind == ExecutionKind::Worktree {
            let (can_use, reason) = if self.is_all_projects() {
                state
                    .project(&project_id)
                    .map(|p| (p.can_use_worktrees, p.worktree_unavailable_reason.clone()))
                    .unwrap_or((false, None))
            } else {
                (
                    state.project_can_use_worktrees,
                    state.worktree_unavailable_reason.clone(),
                )
            };
            if !can_use {
                return Err(reason
                    .unwrap_or_else(|| "Worktree mode is unavailable for this project.".into()));
            }
        }
        self.mutate(
            AutomateRequest::Save {
                definition,
                project_id,
            },
            |this, cx| this.close_dialog(cx),
            |this, error, cx| {
                if let Some(open) = this.dialog.as_ref() {
                    open.dialog
                        .update(cx, |dialog, cx| dialog.save_failed(error, cx));
                } else {
                    this.error_message = Some(error);
                }
            },
            cx,
        );
        Ok(())
    }
}
