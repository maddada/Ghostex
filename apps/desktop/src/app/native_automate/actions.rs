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

    pub(crate) fn open_run_session(&mut self, run: &AutomationRun, cx: &mut Context<Self>) {
        let project_id = self.target_project_id(Some(&run.project_id));
        self.mutate_with_notice(
            AutomateRequest::OpenRunSession {
                run_id: run.id.clone(),
                project_id,
            },
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
        let config = AutomationDialogConfig {
            palette: host.palette,
            all_projects: scope.all_projects,
            project_name: scope.project_name.clone(),
            state,
            sessions: self.sessions.clone(),
            draft,
        };
        let view = cx.weak_entity();
        let options = WindowOptions {
            #[cfg(target_os = "linux")]
            kind: crate::app::window::popup_frame::child_window_kind(),
            #[cfg(target_os = "linux")]
            x11_parent: Some(host.window),
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered_at(
                host.main_window_bounds.center(),
                size(px(DIALOG_WIDTH), px(DIALOG_INITIAL_HEIGHT)),
            ))),
            app_id: crate::gpui_platform_window_app_id(),
            focus: true,
            icon: crate::gpui_platform_window_icon(),
            show: true,
            is_resizable: false,
            is_minimizable: false,
            display_id: crate::app::window::popup_frame::display_at(
                host.main_window_bounds.center(),
                cx,
            )
            .or(host.display_id),
            titlebar: None,
            // The dialog draws the app modals' palette, frosted under window glass.
            window_background: crate::app::helpers::window_glass_background_appearance(),
            ..Default::default()
        };
        let dialog_slot = Rc::new(RefCell::new(None));
        let dialog_out = dialog_slot.clone();
        let window = cx
            .open_window(options, move |window, cx| {
                window.set_window_title("");
                crate::app::helpers::apply_frosted_menu_blur(window);
                window.activate_window();
                let dialog = cx.new(|cx| AutomationDialog::new(config, view, window, cx));
                *dialog_out.borrow_mut() = Some(dialog.clone());
                cx.new(|cx| Root::new(dialog, window, cx).bg(gpui::transparent_black()))
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
