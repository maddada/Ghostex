//! Native GPUI Delete Worktree confirmation, the desktop twin of the React
//! `WorktreeDeleteModal` in packages/core-ui/worktree-delete-modal.tsx (deleted 2026-10-01).
//!
//! CDXC:Worktrees 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt as native GPUI windows and each one must match its React twin 1 to 1 (layout, copy, colors, spacing, states and keys) in both appearances. This dialog keeps the legacy `.ghostex-settings-shadcn command-config-modal-shadcn` skin the React one wears rather than the newer `.gx-app-modal` shell: a 720px dialog centered at the top of the 760px window, square checkboxes, 650/700 weights and hairline dividers.
//! SEE-ALSO: packages/core-ui/worktree-delete-modal.tsx (deleted 2026-10-01) and the `.worktree-delete-*` rules in packages/core-ui/styles/modals.css (deleted 2026-11-06) (the React twin mirrored below), apps/desktop/src/app/window/native_modal_kit/ (`ModalLegacyPalette` and the legacy shell, checkbox and buttons), apps/desktop/src/app/delete_worktree_modal_lifecycle.rs (open, close, sidebar bridge), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Window, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

/// `APP_MODAL_HOST_COMPACT_WINDOW_WIDTH`: the child window the React dialog opened in.
pub(crate) const DELETE_WORKTREE_MODAL_WIDTH: f32 = 760.0;
/// `APP_MODAL_HOST_DELETE_WORKTREE_WINDOW_HEIGHT`, first frame only; the window then fits the dialog.
pub(crate) const DELETE_WORKTREE_MODAL_INITIAL_HEIGHT: f32 = 600.0;
/// `.worktree-delete-modal-shadcn { width: min(720px, calc(100vw - 2rem)) }`.
const DIALOG_WIDTH: f32 = 720.0;
/// `.worktree-delete-modal-body { max-height: min(380px, ...) }`.
const BODY_MAX_HEIGHT: f32 = 380.0;
/// `.worktree-delete-status-summary { max-height: min(220px, ...) }`.
const STATUS_MAX_HEIGHT: f32 = 220.0;

const ICON_CHECK: &str = "modals/delete-worktree/check.svg";

const TITLE: &str = "Delete worktree";
const STATUS_HEADING: &str = "This worktree has uncommitted changes:";
const CLEAN_ROW: &str = "The worktree has no local changes";
const LOCAL_BRANCH_LABEL: &str = "Delete local branch ";
const LOCAL_BRANCH_HELP: &str = "No local branch is checked out for this worktree.";
const REMOTE_BRANCH_LABEL: &str = "Delete remote branch ";
const REMOTE_BRANCH_HELP: &str = "No matching remote branch exists.";
const NOTE_PREFIX: &str = "This action will remove the worktree directory";
const CANCEL: &str = "Cancel";
const COMMIT: &str = "Commit";
const DELETE_WORKTREE: &str = "Delete Worktree";

/// The `worktreeDeleteDraft` the sidebar sends with the open message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeleteWorktreeDraft {
    pub(crate) branch: Option<String>,
    pub(crate) can_delete_local_branch: bool,
    pub(crate) group_id: String,
    pub(crate) has_changes: bool,
    pub(crate) local_branch_name: Option<String>,
    pub(crate) project_id: String,
    pub(crate) remote_branch_disabled_reason: Option<String>,
    pub(crate) remote_branch_exists: bool,
    pub(crate) remote_branch_name: Option<String>,
    pub(crate) remote_name: Option<String>,
    pub(crate) status_summary: String,
    pub(crate) worktree_name: String,
}

/// What the dialog asks its host to do. The dialog removes its own window before sending any of these.
pub(crate) enum DeleteWorktreeModalCommand {
    Cancel,
    Commit {
        group_id: String,
    },
    Delete {
        project_id: String,
        delete_local_branch: bool,
        delete_remote_branch: bool,
    },
}

pub(crate) type DeleteWorktreeModalHost = Rc<dyn Fn(DeleteWorktreeModalCommand, &mut App)>;

pub(crate) struct DeleteWorktreeModalConfig {
    pub(crate) draft: DeleteWorktreeDraft,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiDeleteWorktreeModalWindow {
    host: DeleteWorktreeModalHost,
    palette: ModalLegacyPalette,
    draft: DeleteWorktreeDraft,
    /// Both start unchecked on every open (the React `useEffect` reset).
    delete_local_branch: bool,
    delete_remote_branch: bool,
    fit: ModalFit,
    focus_handle: FocusHandle,
    /// Set once the window is gone, so a key that reaches both the shell's key handler and the
    /// input's action (Escape and Enter arrive on both paths) sends its command only once.
    closed: bool,
}

impl GpuiDeleteWorktreeModalWindow {
    pub(crate) fn new(
        config: DeleteWorktreeModalConfig,
        host: DeleteWorktreeModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: ModalLegacyPalette::resolve(&config.palette),
            draft: config.draft,
            delete_local_branch: false,
            delete_remote_branch: false,
            fit: ModalFit::new(),
            focus_handle,
            closed: false,
        }
    }

    /// `draft.localBranchName ?? draft.branch`.
    fn local_branch_name(&self) -> Option<&str> {
        self.draft
            .local_branch_name
            .as_deref()
            .or(self.draft.branch.as_deref())
    }

    fn remote_name(&self) -> &str {
        self.draft.remote_name.as_deref().unwrap_or("origin")
    }

    /// `` `${remoteName}/${draft.remoteBranchName}` `` or `` `${remoteName} branch` ``.
    fn remote_branch_label(&self) -> String {
        match self.draft.remote_branch_name.as_deref() {
            Some(name) => format!("{}/{name}", self.remote_name()),
            None => format!("{} branch", self.remote_name()),
        }
    }

    /// The checkbox state the React modal reports: checked only while its guard holds.
    fn local_checked(&self) -> bool {
        self.delete_local_branch && self.draft.can_delete_local_branch
    }

    fn remote_checked(&self) -> bool {
        self.delete_remote_branch && self.draft.remote_branch_exists
    }

    fn note(&self) -> String {
        let mut deletes: Vec<String> = Vec::new();
        if self.local_checked()
            && let Some(name) = self.local_branch_name()
        {
            deletes.push(format!("local branch {name}"));
        }
        if self.remote_checked() && self.draft.remote_branch_name.is_some() {
            deletes.push(format!("remote branch {}", self.remote_branch_label()));
        }
        if deletes.is_empty() {
            format!("{NOTE_PREFIX}.")
        } else {
            format!("{NOTE_PREFIX} and delete {}.", deletes.join(" and "))
        }
    }

    fn toggle_local(&mut self, cx: &mut Context<Self>) {
        if !self.draft.can_delete_local_branch {
            return;
        }
        self.delete_local_branch = !self.delete_local_branch;
        cx.notify();
    }

    fn toggle_remote(&mut self, cx: &mut Context<Self>) {
        if !self.draft.remote_branch_exists {
            return;
        }
        self.delete_remote_branch = !self.delete_remote_branch;
        cx.notify();
    }

    fn close_window_and_send(
        &mut self,
        command: DeleteWorktreeModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closed {
            return;
        }
        self.closed = true;
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(DeleteWorktreeModalCommand::Cancel, window, cx);
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let group_id = self.draft.group_id.clone();
        self.close_window_and_send(DeleteWorktreeModalCommand::Commit { group_id }, window, cx);
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = DeleteWorktreeModalCommand::Delete {
            project_id: self.draft.project_id.clone(),
            delete_local_branch: self.local_checked(),
            delete_remote_branch: self.remote_checked(),
        };
        self.close_window_and_send(command, window, cx);
    }

    /// Escape closes the dialog (the Dialog's `onOpenChange`); the React modal has no Enter handling.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" {
            self.cancel(window, cx);
            cx.stop_propagation();
        }
    }

    /// `.worktree-delete-modal-question`, `-note` and `-status-heading`: 14px, line-height 1.5.
    /// Their 650 weight, like every weight in this skin, is flattened to 400 by
    /// `.ghostex-settings-shadcn.ghostex-settings-shadcn * { font-weight: 400 }` in styles.css (deleted 2026-11-06).
    fn render_statement(&self, text: String) -> AnyElement {
        div()
            .w_full()
            .text_size(px(14.0))
            .line_height(px(21.0))
            .text_color(hsla(self.palette.text(0.90)))
            .child(text)
            .into_any_element()
    }

    /// `.worktree-delete-status-block`: the heading over the scrolling `pre` summary.
    fn render_status_block(&self) -> AnyElement {
        let lp = self.palette;
        v_flex()
            .w_full()
            .min_w_0()
            .gap(px(10.0))
            .child(self.render_statement(STATUS_HEADING.to_string()))
            .child(
                div()
                    .id("delete-worktree-status-summary")
                    .w_full()
                    .max_h(px(STATUS_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .p(px(12.0))
                    .border_1()
                    .border_color(hsla(css_fade(lp.input, 0.72)))
                    .bg(hsla(css_fade(lp.surface, 0.68)))
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(17.4))
                    .text_color(hsla(lp.text(0.88)))
                    .whitespace_normal()
                    .child(self.draft.status_summary.clone()),
            )
            .into_any_element()
    }

    /// `.worktree-delete-clean-row`: the `#238636` square with a white check, 14px copy.
    fn render_clean_row(&self) -> AnyElement {
        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(px(10.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(self.palette.text(0.88)))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.0))
                    // `.worktree-delete-clean-icon { background: #238636; color: white }`.
                    .bg(hsla(rgb(0x238636)))
                    .child(modal_icon(ICON_CHECK, 15.0, rgb(0xffffff))),
            )
            .child(CLEAN_ROW)
            .into_any_element()
    }

    /// One `.worktree-delete-branch-option` row: square checkbox, 13px label with a 12px mono code, optional help.
    fn render_branch_option(
        &self,
        index: usize,
        checked: bool,
        enabled: bool,
        label: &'static str,
        code: Option<String>,
        help: Option<String>,
        on_toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lp = self.palette;
        let row_color = if enabled {
            lp.text(0.88)
        } else {
            css_fade(lp.muted, 0.86)
        };
        h_flex()
            .id(("delete-worktree-branch-option", index))
            .w_full()
            .min_w_0()
            .items_start()
            .gap(px(10.0))
            .p(px(12.0))
            .when(index > 0, |this| {
                this.border_t_1()
                    .border_color(hsla(css_fade(lp.input, 0.50)))
            })
            .text_color(hsla(row_color))
            .when(enabled, |this| {
                this.cursor_pointer().on_click(cx.listener(
                    move |this, _: &ClickEvent, _window, cx| {
                        on_toggle(this, cx);
                    },
                ))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .mt(px(2.0))
                    .child(modal_square_checkbox(&lp, checked, !enabled)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .gap(px(3.0))
                    .child(
                        h_flex()
                            .min_w_0()
                            .flex_wrap()
                            .items_baseline()
                            .text_size(px(13.0))
                            .line_height(px(18.2))
                            .child(label)
                            .children(code.map(|code| {
                                div()
                                    .font_family(MODAL_MONO_FONT)
                                    .text_size(px(12.0))
                                    .line_height(px(16.8))
                                    .text_color(hsla(lp.text(0.94)))
                                    .child(code)
                            })),
                    )
                    .children(
                        help.map(|help| {
                            div().text_size(px(12.0)).line_height(px(16.2)).child(help)
                        }),
                    ),
            )
            .into_any_element()
    }

    /// `.worktree-delete-branch-options`: the bordered two-row list.
    fn render_branch_options(&self, cx: &mut Context<Self>) -> AnyElement {
        let lp = self.palette;
        let local_help =
            (!self.draft.can_delete_local_branch).then(|| LOCAL_BRANCH_HELP.to_string());
        let remote_help = (!self.draft.remote_branch_exists).then(|| {
            self.draft
                .remote_branch_disabled_reason
                .clone()
                .unwrap_or_else(|| REMOTE_BRANCH_HELP.to_string())
        });
        v_flex()
            .w_full()
            .min_w_0()
            .border_1()
            .border_color(hsla(css_fade(lp.input, 0.62)))
            .child(self.render_branch_option(
                0,
                self.local_checked(),
                self.draft.can_delete_local_branch,
                LOCAL_BRANCH_LABEL,
                self.local_branch_name().map(str::to_string),
                local_help,
                |this, cx| this.toggle_local(cx),
                cx,
            ))
            .child(
                self.render_branch_option(
                    1,
                    self.remote_checked(),
                    self.draft.remote_branch_exists,
                    REMOTE_BRANCH_LABEL,
                    self.draft
                        .remote_branch_name
                        .is_some()
                        .then(|| self.remote_branch_label()),
                    remote_help,
                    |this, cx| this.toggle_remote(cx),
                    cx,
                ),
            )
            .into_any_element()
    }

    /// `.worktree-delete-modal-body`: an 18px grid inside 20px padding that scrolls past 380px.
    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let question = format!("Delete worktree \"{}\"?", self.draft.worktree_name);
        v_flex()
            .id("delete-worktree-body")
            .w_full()
            .min_h_0()
            .max_h(px(BODY_MAX_HEIGHT))
            .overflow_y_scroll()
            .p(px(20.0))
            .gap(px(18.0))
            .child(self.render_statement(question))
            .child(if self.draft.has_changes {
                self.render_status_block()
            } else {
                self.render_clean_row()
            })
            .child(self.render_branch_options(cx))
            .child(self.render_statement(self.note()))
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let lp = self.palette;
        let mut buttons = vec![modal_legacy_action_button(
            &lp,
            "delete-worktree-cancel",
            CANCEL,
            ModalLegacyButtonTone::Outline,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        )];
        if self.draft.has_changes {
            buttons.push(modal_legacy_action_button(
                &lp,
                "delete-worktree-commit",
                COMMIT,
                ModalLegacyButtonTone::Outline,
                false,
                |this, window, cx| this.commit(window, cx),
                cx,
            ));
        }
        buttons.push(modal_legacy_action_button(
            &lp,
            "delete-worktree-delete",
            DELETE_WORKTREE,
            ModalLegacyButtonTone::Destructive,
            false,
            |this, window, cx| this.delete(window, cx),
            cx,
        ));
        buttons
    }
}

impl Render for GpuiDeleteWorktreeModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lp = self.palette;
        // `DialogTitle.text-xl`: 20px with Tailwind's 28px line height (tailwind-merge drops
        // `leading-none` for `text-xl`), at the 500 the skin gives `[data-slot='dialog-title']`.
        let header = v_flex().gap(px(0.0)).child(
            div()
                .text_size(px(20.0))
                .line_height(px(28.0))
                .font_weight(FontWeight::MEDIUM)
                .child(TITLE),
        );
        let body = self.render_body(cx);
        let footer = self.render_footer(cx);
        modal_legacy_shell(
            &lp,
            "ghostex-gpui-delete-worktree-modal",
            DIALOG_WIDTH,
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            header,
            body,
            footer,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiDeleteWorktreeModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
