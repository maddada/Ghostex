//! The New Orchestrator dialog's Orchestrator playbook: the row that says which playbook
//! orchestrators in the project follow, and the editor page its Edit button opens (the playbook
//! for all projects or this project's own, Reset to Default with a confirming second click).
//! See CDXC:Coordinators 2026-10-19 in server/src/coordinators/playbook.rs.
use super::*;
use gpui::StatefulInteractiveElement as _;
use std::time::Duration;

const FIELD_PLAYBOOK: &str = "Coordinator playbook";
const PLAYBOOK_TITLE: &str = "Coordinator playbook";
const EDIT: &str = "Edit";
const BACK: &str = "Back";
const SAVE: &str = "Save";
const SAVING: &str = "Saving…";
const RESET: &str = "Reset to Default";
const RESET_CONFIRM: &str = "Click again to reset";
/// How long Reset to Default stays armed for its confirming click.
const RESET_ARM_TIMEOUT: Duration = Duration::from_secs(4);

/// One scope of the playbook as `/api/readCoordinatorPlaybook` answers it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlaybookScopeInfo {
    /// An edited copy is stored for this scope.
    pub(crate) edited: bool,
    pub(crate) text: String,
    pub(crate) enabled: bool,
    /// What Reset to Default goes back to.
    pub(crate) default_text: String,
    /// The default changed since the copy was saved.
    pub(crate) default_changed: bool,
}

impl PlaybookScopeInfo {
    fn from_value(scope: &Value) -> Self {
        let text = |key: &str| {
            scope
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            edited: scope["edited"] == true,
            text: text("text"),
            enabled: scope["enabled"] != false,
            default_text: text("defaultText"),
            default_changed: scope["defaultChanged"] == true,
        }
    }

    fn in_use(&self) -> bool {
        self.edited && self.enabled
    }

    /// The text the editor opens with: the edited copy, else what the scope uses now.
    fn editor_text(&self) -> String {
        if self.edited {
            self.text.clone()
        } else {
            self.default_text.clone()
        }
    }
}

/// The dialog project's playbook: the copy for all projects and the project's own (for a
/// worktree, its project's).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrchestratorPlaybookInfo {
    pub(crate) all_projects: PlaybookScopeInfo,
    pub(crate) project: PlaybookScopeInfo,
    /// The project whose copy covers this one (the parent of a worktree).
    pub(crate) project_id: String,
    pub(crate) project_name: String,
}

impl OrchestratorPlaybookInfo {
    pub(crate) fn from_view(view: &Value) -> Self {
        let project = view.get("project").cloned().unwrap_or(Value::Null);
        Self {
            all_projects: PlaybookScopeInfo::from_value(&view["allProjects"]),
            project: PlaybookScopeInfo::from_value(&project),
            project_id: project
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            project_name: project
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }
    }

    fn scope(&self, project: bool) -> &PlaybookScopeInfo {
        if project {
            &self.project
        } else {
            &self.all_projects
        }
    }

    fn project_label(&self) -> String {
        if self.project_name.trim().is_empty() {
            "this project".to_string()
        } else {
            self.project_name.clone()
        }
    }
}

/// What a save changes.
pub(crate) enum PlaybookChange {
    Text(String),
    Reset,
}

/// Where a save's outcome goes: the dialog that sent it, in its own window.
pub(crate) struct NewCoordinatorPlaybookReply {
    view: WeakEntity<GpuiNewCoordinatorModalWindow>,
    window: AnyWindowHandle,
    /// A reset keeps the editor open on the default text; a save goes back to the form.
    stay: bool,
}

impl NewCoordinatorPlaybookReply {
    /// Hands the outcome (gxserver's new view, or why it refused) to the dialog.
    pub(crate) fn finish(self, result: Result<Value, String>, cx: &mut App) {
        let (view, stay) = (self.view, self.stay);
        let _ = self.window.update(cx, |_root, window, cx| {
            view.update(cx, |dialog, cx| {
                dialog.finish_playbook_change(result, stay, window, cx)
            })
        });
    }
}

/// The editor page the Edit button opens in place of the form.
pub(super) struct PlaybookEditor {
    /// Editing the project's copy (else the one for all projects).
    pub(super) project_scope: bool,
    /// The text typed for each scope: `[all projects, project]`.
    pub(super) drafts: [String; 2],
    saving: bool,
    error: Option<String>,
    /// Reset to Default was clicked once and waits for its confirming click.
    reset_armed: bool,
    /// Bumped on every arming, so an older timeout cannot disarm a newer one.
    reset_generation: u64,
}

impl GpuiNewCoordinatorModalWindow {
    /// The project's playbook, once the host has read it.
    pub(crate) fn set_playbook(
        &mut self,
        info: Option<OrchestratorPlaybookInfo>,
        cx: &mut Context<Self>,
    ) {
        self.playbook = info;
        cx.notify();
    }

    /// Turns the dialog into the editor, on the scope whose playbook orchestrators here follow.
    fn open_playbook_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(info) = self.playbook.clone() else {
            return;
        };
        self.close_menus();
        let project_scope = info.project.in_use() || info.project.edited;
        let drafts = [info.all_projects.editor_text(), info.project.editor_text()];
        let text = drafts[usize::from(project_scope)].clone();
        self.editor = Some(PlaybookEditor {
            project_scope,
            drafts,
            saving: false,
            error: None,
            reset_armed: false,
            reset_generation: 0,
        });
        self.playbook_input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn close_playbook_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor = None;
        self.name_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Shows the other scope's text; what was typed in each scope stays until Back.
    fn choose_playbook_scope(
        &mut self,
        project_scope: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.project_scope == project_scope || editor.saving {
            return;
        }
        editor.project_scope = project_scope;
        editor.error = None;
        editor.reset_armed = false;
        let text = editor.drafts[usize::from(project_scope)].clone();
        self.playbook_input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        cx.notify();
    }

    fn send_playbook_change(
        &mut self,
        change: PlaybookChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project_id = self
            .playbook
            .as_ref()
            .map(|info| info.project_id.clone())
            .filter(|id| !id.is_empty());
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.saving {
            return;
        }
        let project_id = match (editor.project_scope, project_id) {
            (false, _) => None,
            (true, Some(project_id)) => Some(project_id),
            (true, None) => return,
        };
        editor.saving = true;
        editor.error = None;
        let reply = NewCoordinatorPlaybookReply {
            view: cx.entity().downgrade(),
            window: window.window_handle(),
            stay: matches!(change, PlaybookChange::Reset),
        };
        cx.notify();
        (self.host)(
            NewCoordinatorModalCommand::SavePlaybook {
                project_id,
                change,
                reply,
            },
            cx,
        );
    }

    pub(super) fn save_playbook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        let text = editor.drafts[usize::from(editor.project_scope)]
            .trim()
            .to_string();
        self.send_playbook_change(PlaybookChange::Text(text), window, cx);
    }

    /// The first click arms Reset to Default; the confirming one removes the scope's edited copy.
    /// Leaving the button or waiting disarms it.
    fn press_playbook_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.reset_armed {
            editor.reset_armed = false;
            self.send_playbook_change(PlaybookChange::Reset, window, cx);
            return;
        }
        editor.reset_armed = true;
        editor.reset_generation += 1;
        let generation = editor.reset_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RESET_ARM_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                this.disarm_playbook_reset(Some(generation), cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn disarm_playbook_reset(&mut self, generation: Option<u64>, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut()
            && editor.reset_armed
            && generation.is_none_or(|armed_at| armed_at == editor.reset_generation)
        {
            editor.reset_armed = false;
            cx.notify();
        }
    }

    /// A saved text goes back to the form, which now shows it; a reset stays on the editor with
    /// the default text; a refusal keeps the editor open with gxserver's message.
    fn finish_playbook_change(
        &mut self,
        result: Result<Value, String>,
        stay: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(view) => {
                let info = OrchestratorPlaybookInfo::from_view(&view);
                self.playbook = Some(info.clone());
                if !stay {
                    self.close_playbook_editor(window, cx);
                    return;
                }
                if let Some(editor) = self.editor.as_mut() {
                    editor.saving = false;
                    editor.drafts = [info.all_projects.editor_text(), info.project.editor_text()];
                    let text = editor.drafts[usize::from(editor.project_scope)].clone();
                    self.playbook_input
                        .update(cx, |input, cx| input.set_value(text, window, cx));
                }
                cx.notify();
            }
            Err(message) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.saving = false;
                    editor.error = Some(message);
                }
                cx.notify();
            }
        }
    }

    /// Which playbook orchestrators here follow, the switches that are off, and the Edit button.
    pub(super) fn render_playbook_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let info = self.playbook.as_ref()?;
        let p = self.palette;
        let project = info.project_label();
        let (source, sentence) = if info.project.in_use() {
            (
                format!("{project}'s"),
                format!(
                    "It follows {project}'s edited playbook, which covers all of its worktrees."
                ),
            )
        } else if info.all_projects.in_use() {
            (
                "All projects".to_string(),
                "It follows the edited playbook for all projects.".to_string(),
            )
        } else {
            (
                "Ghostex's".to_string(),
                "It follows the playbook Ghostex ships.".to_string(),
            )
        };
        let mut off = Vec::new();
        if info.project.edited && !info.project.enabled {
            off.push(format!("{project}'s edited playbook is off"));
        }
        if info.all_projects.edited && !info.all_projects.enabled {
            off.push("the one for all projects is off".to_string());
        }
        let label = if off.is_empty() {
            source
        } else {
            format!("{source} · {} off", off.len())
        };
        let mut hint = sentence;
        if !off.is_empty() {
            let mut text = off.join(", and ");
            if let Some(first) = text.get(..1) {
                text = first.to_uppercase() + &text[1..];
            }
            hint.push_str(&format!(" {text} (Settings)."));
        }
        let edit = modal_action_button(
            &p,
            "new-coordinator-playbook-edit",
            EDIT,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.open_playbook_editor(window, cx),
            cx,
        );
        Some(
            v_flex()
                .w_full()
                .gap(px(8.0))
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .child(modal_section_title(&p, FIELD_PLAYBOOK))
                        .child(
                            div()
                                .id("new-coordinator-playbook-source")
                                .text_size(px(12.0))
                                .text_color(p.muted)
                                .child(label),
                        ),
                )
                .child(
                    h_flex()
                        .id("new-coordinator-playbook")
                        .w_full()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(p.hairline)
                        .bg(p.raised)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(13.0))
                                .line_height(px(18.0))
                                .text_color(p.foreground)
                                .child(hint),
                        )
                        .child(div().flex_shrink_0().w(px(72.0)).child(edit)),
                )
                .into_any_element(),
        )
    }

    /// The editor page: its header, which playbook (all projects or this project), the text, and
    /// notes on an edited copy that is off or whose default changed.
    pub(super) fn render_playbook_page(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = self.palette;
        let Some(editor) = self.editor.as_ref() else {
            return Vec::new();
        };
        let info = self.playbook.clone().unwrap_or_default();
        let project = info.project_label();
        let scope = info.scope(editor.project_scope);
        let items = [
            ModalSegmentedItem {
                icon: None,
                label: "All projects",
            },
            ModalSegmentedItem {
                icon: None,
                label: "This project",
            },
        ];
        let scope_control = modal_segmented_control(
            &p,
            "new-coordinator-playbook-scope",
            &items,
            usize::from(editor.project_scope),
            |this: &mut Self, index, window, cx| this.choose_playbook_scope(index == 1, window, cx),
            cx,
        );
        let default_name = if editor.project_scope {
            "the playbook for all projects"
        } else {
            "Ghostex's playbook"
        };
        let mut hint = if editor.project_scope {
            format!(
                "Coordinators in {project} and all of its worktrees, existing ones too, follow this from their next guide read."
            )
        } else {
            "Every coordinator, existing ones too, follows this from its next guide read, unless its project has its own.".to_string()
        };
        if scope.edited && !scope.enabled {
            hint.push_str(&format!(
                " This edited copy is off, so they follow {default_name}; turn it on in Settings."
            ));
        }
        if scope.default_changed {
            hint.push_str(&format!(
                " {} changed since this copy was saved: Reset to Default takes the new one.",
                default_name[..1].to_uppercase() + &default_name[1..]
            ));
        }
        let mut column = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(12.0))
            .child(scope_control)
            .child(modal_text_area(
                &p,
                &self.playbook_input,
                None,
                editor.saving,
                window,
                cx,
            ))
            .child(modal_hint(&p, hint));
        if let Some(error) = &editor.error {
            column = column.child(modal_error(&p, error.clone()));
        }
        vec![
            modal_header(
                &p,
                PLAYBOOK_TITLE,
                Some(format!(
                    "How every coordinator works, for all projects or just for {project}."
                )),
            ),
            column.into_any_element(),
        ]
    }

    /// Reset to Default on the left, Back and Save on the right.
    pub(super) fn render_playbook_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let Some(editor) = self.editor.as_ref() else {
            return div().into_any_element();
        };
        let default = self
            .playbook
            .as_ref()
            .map(|info| info.scope(editor.project_scope).default_text.clone())
            .unwrap_or_default();
        let matches_default =
            editor.drafts[usize::from(editor.project_scope)].trim() == default.trim();
        let reset = modal_action_button(
            &p,
            "new-coordinator-playbook-reset",
            if editor.reset_armed {
                RESET_CONFIRM
            } else {
                RESET
            },
            None,
            if editor.reset_armed {
                ModalButtonTone::Primary
            } else {
                ModalButtonTone::Neutral
            },
            editor.saving || matches_default,
            |this, window, cx| this.press_playbook_reset(window, cx),
            cx,
        );
        let back = modal_action_button(
            &p,
            "new-coordinator-playbook-back",
            BACK,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.close_playbook_editor(window, cx),
            cx,
        );
        let save = modal_action_button(
            &p,
            "new-coordinator-playbook-save",
            if editor.saving { SAVING } else { SAVE },
            None,
            ModalButtonTone::Primary,
            editor.saving,
            |this, window, cx| this.save_playbook(window, cx),
            cx,
        );
        h_flex()
            .w_full()
            .gap(px(8.0))
            .child(
                h_flex()
                    .id("new-coordinator-playbook-reset-wrap")
                    .w(px(176.0))
                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            this.disarm_playbook_reset(None, cx);
                        }
                    }))
                    .child(reset),
            )
            .child(div().flex_1())
            .child(h_flex().w(px(96.0)).child(back))
            .child(h_flex().w(px(96.0)).child(save))
            .into_any_element()
    }
}
