//! Native GPUI Rename Worktree dialog, the desktop twin of the React
//! `WorktreeRenameModal` in packages/core-ui/worktree-rename-modal.tsx (deleted 2026-10-01), with
//! the name rules of packages/shared/worktree-rename-name.ts (deleted 2026-10-01) ported verbatim.
//!
//! CDXC:Worktrees 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt as native GPUI windows and each one must match its React twin 1 to 1 (layout, copy, colors, spacing, states and keys) in both appearances. This dialog keeps the legacy `.ghostex-settings-shadcn command-config-modal-shadcn` skin the React one wears rather than the newer `.gx-app-modal` shell: a 640px dialog centered at the top of the 760px window, a square checkbox, 650/700 weights and hairline dividers, and the same live folder/branch preview and refusals.
//! SEE-ALSO: packages/core-ui/worktree-rename-modal.tsx (deleted 2026-10-01), packages/shared/worktree-rename-name.ts (deleted 2026-10-01) and the `.worktree-rename-*` rules in packages/core-ui/styles/modals.css (deleted 2026-11-06) (the React twin mirrored below), apps/desktop/src/app/window/native_modal_kit/ (`ModalLegacyPalette` and the legacy shell, checkbox, input and buttons), apps/desktop/src/app/rename_worktree_modal_lifecycle.rs (open, close, sidebar bridge), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{Escape, InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

/// `APP_MODAL_HOST_COMPACT_WINDOW_WIDTH`: the child window the React dialog opened in.
pub(crate) const RENAME_WORKTREE_MODAL_WIDTH: f32 = 760.0;
/// `APP_MODAL_HOST_DELETE_WORKTREE_WINDOW_HEIGHT` (Rename shares Delete's frame), first frame only.
pub(crate) const RENAME_WORKTREE_MODAL_INITIAL_HEIGHT: f32 = 600.0;
/// `.worktree-rename-modal-shadcn { width: min(640px, calc(100vw - 2rem)) }`.
const DIALOG_WIDTH: f32 = 640.0;
/// `.worktree-rename-modal-body { max-height: min(380px, ...) }`.
const BODY_MAX_HEIGHT: f32 = 380.0;

const WORKTREE_RENAME_NAME_MAX_CHARS: usize = 200;
const WORKTREE_RENAME_FOLDER_SLUG_MAX_CHARS: usize = 48;

const TITLE: &str = "Rename Worktree";
const NAME_LABEL: &str = "Name";
const FOLDER_PREVIEW: &str = "Folder: ";
const BRANCH_PREVIEW: &str = "Branch: ";
const NOT_SET: &str = "Not set";
const RENAME_BRANCH_LABEL: &str = "Also rename the git branch";
const RENAME_BRANCH_HELP: &str =
    "The branch takes the typed name exactly, without the folder’s slug.";
const NOTHING_TO_RENAME: &str = "Nothing to rename.";
const COLLIDES_WITH_MAIN: &str = "That name would collide with the main checkout.";
const REGISTERED_ELSEWHERE: &str = "Another project is already registered at that folder.";
const CANCEL: &str = "Cancel";
const RENAME: &str = "Rename";

pub(crate) const WORKTREE_RENAME_NAME_CHARACTER_ERROR: &str =
    "Use letters, numbers, and . _ / - only, starting with a letter or number.";
pub(crate) const WORKTREE_RENAME_NAME_SEPARATOR_ERROR: &str =
    "Names cannot contain \"..\", \"//\", or end with \"/\".";
pub(crate) const WORKTREE_RENAME_NAME_TOO_LONG_ERROR: &str =
    "Name is too long (200 characters max).";

/// `normalizeWorktreeRenameName`.
pub(crate) fn normalize_worktree_rename_name(value: &str) -> &str {
    value.trim()
}

/// `worktreeRenameNameError`: the same refusals gxserver's `is_allowed_git_ref` applies.
pub(crate) fn worktree_rename_name_error(value: &str) -> Option<&'static str> {
    let name = normalize_worktree_rename_name(value);
    if name.encode_utf16().count() > WORKTREE_RENAME_NAME_MAX_CHARS {
        return Some(WORKTREE_RENAME_NAME_TOO_LONG_ERROR);
    }
    if !name.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return Some(WORKTREE_RENAME_NAME_CHARACTER_ERROR);
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
    {
        return Some(WORKTREE_RENAME_NAME_CHARACTER_ERROR);
    }
    if name.ends_with('.') {
        return Some(WORKTREE_RENAME_NAME_CHARACTER_ERROR);
    }
    if name
        .split('/')
        .any(|component| component.starts_with('.') || component.ends_with(".lock"))
    {
        return Some(WORKTREE_RENAME_NAME_CHARACTER_ERROR);
    }
    if name.contains("..") || name.contains("//") || name.ends_with('/') {
        return Some(WORKTREE_RENAME_NAME_SEPARATOR_ERROR);
    }
    None
}

/// `worktreeRenameFolderSlug`: `feat/kanban-assignee` becomes `feat-kanban-assignee`,
/// case preserved, cut at the last dash inside 48 characters.
pub(crate) fn worktree_rename_folder_slug(value: &str) -> String {
    let mut collapsed = String::new();
    let mut in_run = false;
    for c in normalize_worktree_rename_name(value).chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            collapsed.push(c);
            in_run = false;
        } else if !in_run {
            collapsed.push('-');
            in_run = true;
        }
    }
    let collapsed = collapsed.trim_matches('-');
    if collapsed.len() <= WORKTREE_RENAME_FOLDER_SLUG_MAX_CHARS {
        return collapsed.to_string();
    }
    let cut = &collapsed[..WORKTREE_RENAME_FOLDER_SLUG_MAX_CHARS];
    let kept = match cut.rfind('-') {
        Some(boundary) if boundary > 0 => &cut[..boundary],
        _ => cut,
    };
    kept.trim_end_matches('-').to_string()
}

/// `resolveWorktreeRenameInitialName`: prefill the branch when it slugs down to
/// the current folder suffix, so reopening and pressing Rename is a no-op.
pub(crate) fn resolve_worktree_rename_initial_name(draft: &RenameWorktreeDraft) -> String {
    let branch = draft.branch.as_deref().map(str::trim).unwrap_or("");
    if !branch.is_empty() && worktree_rename_folder_slug(branch) == draft.current_name {
        branch.to_string()
    } else {
        draft.current_name.clone()
    }
}

/// `joinRenameParentDirectory`: keeps whichever separator the registered path uses.
pub(crate) fn join_rename_parent_directory(parent_project_path: &str, folder_name: &str) -> String {
    let trimmed = parent_project_path.trim_end_matches(['/', '\\']);
    let separator_index = trimmed.rfind(['/', '\\']);
    let separator = separator_index
        .and_then(|index| trimmed[index..].chars().next())
        .unwrap_or('/');
    let family_root = match separator_index {
        Some(index) if index > 0 => &trimmed[..index],
        _ => "",
    };
    format!("{family_root}{separator}{folder_name}")
}

/// The `worktreeRenameDraft` the sidebar sends with the open message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenameWorktreeDraft {
    pub(crate) blocking_reason: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) current_name: String,
    pub(crate) current_path: String,
    pub(crate) parent_folder_name: String,
    pub(crate) parent_project_path: String,
    pub(crate) project_id: String,
    pub(crate) registered_project_paths: Vec<String>,
    pub(crate) rename_branch_default: bool,
    pub(crate) warnings: Vec<String>,
    pub(crate) worktree_name: String,
}

/// What the dialog asks its host to do. The dialog removes its own window before sending either.
pub(crate) enum RenameWorktreeModalCommand {
    Cancel,
    Rename {
        project_id: String,
        name: String,
        rename_branch: bool,
    },
}

pub(crate) type RenameWorktreeModalHost = Rc<dyn Fn(RenameWorktreeModalCommand, &mut App)>;

pub(crate) struct RenameWorktreeModalConfig {
    pub(crate) draft: RenameWorktreeDraft,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiRenameWorktreeModalWindow {
    host: RenameWorktreeModalHost,
    palette: ModalLegacyPalette,
    draft: RenameWorktreeDraft,
    initial_name: String,
    name_input: Entity<InputState>,
    rename_branch: bool,
    fit: ModalFit,
    focus_handle: FocusHandle,
    /// Set once the window is gone, so a key that reaches both the shell's key handler and the
    /// input's action (Escape and Enter arrive on both paths) sends its command only once.
    closed: bool,
    _subscriptions: Vec<Subscription>,
}

/// The live derivation the React modal recomputes on every keystroke.
struct RenamePreview {
    trimmed_name: String,
    next_folder_name: String,
    submit_error: Option<String>,
}

impl GpuiRenameWorktreeModalWindow {
    pub(crate) fn new(
        config: RenameWorktreeModalConfig,
        host: RenameWorktreeModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial_name = resolve_worktree_rename_initial_name(&config.draft);
        let name_input =
            cx.new(|cx| InputState::new(window, cx).default_value(initial_name.clone()));
        let subscription = cx.subscribe_in(
            &name_input,
            window,
            |this: &mut Self, _input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        );
        // The React modal focuses the field and selects its whole value on open.
        let length = initial_name.len();
        name_input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(0..length, cx);
        });
        let mut this = Self {
            host,
            palette: ModalLegacyPalette::resolve(&config.palette),
            rename_branch: config.draft.rename_branch_default,
            draft: config.draft,
            initial_name,
            name_input,
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
            closed: false,
            _subscriptions: vec![subscription],
        };
        this._subscriptions
            .extend(super::popup_dismissal::close_app_modal_on_click_away(
                window, cx,
            ));
        this
    }

    fn preview(&self, cx: &App) -> RenamePreview {
        let name = self.name_input.read(cx).value().to_string();
        let trimmed_name = normalize_worktree_rename_name(&name).to_string();
        let folder_slug = worktree_rename_folder_slug(&name);
        let next_folder_name = if folder_slug.is_empty() {
            String::new()
        } else {
            format!("{}-{folder_slug}", self.draft.parent_folder_name)
        };
        let next_folder_path = if next_folder_name.is_empty() {
            String::new()
        } else {
            join_rename_parent_directory(&self.draft.parent_project_path, &next_folder_name)
        };
        let validation_error = worktree_rename_name_error(&name);
        let unchanged = trimmed_name == self.initial_name;
        let collision_error = self.collision_error(&next_folder_name, &next_folder_path, unchanged);
        let submit_error = validation_error
            .map(str::to_string)
            .or_else(|| (unchanged && !self.rename_branch).then(|| NOTHING_TO_RENAME.to_string()))
            .or(collision_error);
        RenamePreview {
            trimmed_name,
            next_folder_name,
            submit_error,
        }
    }

    /// `resolveWorktreeRenameCollisionError`: the two refusals answerable from the draft alone.
    fn collision_error(
        &self,
        next_folder_name: &str,
        next_folder_path: &str,
        unchanged: bool,
    ) -> Option<String> {
        if next_folder_name.is_empty() || unchanged {
            return None;
        }
        if next_folder_path == self.draft.parent_project_path {
            return Some(COLLIDES_WITH_MAIN.to_string());
        }
        if self
            .draft
            .registered_project_paths
            .iter()
            .any(|path| path == next_folder_path)
        {
            return Some(REGISTERED_ELSEWHERE.to_string());
        }
        None
    }

    fn can_submit(&self, preview: &RenamePreview) -> bool {
        preview.submit_error.is_none() && self.draft.blocking_reason.is_none()
    }

    fn toggle_rename_branch(&mut self, cx: &mut Context<Self>) {
        self.rename_branch = !self.rename_branch;
        cx.notify();
    }

    fn close_window_and_send(
        &mut self,
        command: RenameWorktreeModalCommand,
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
        self.close_window_and_send(RenameWorktreeModalCommand::Cancel, window, cx);
    }

    /// The form submit: Enter in the field or the Rename button, refused unless `canSubmit`.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preview = self.preview(cx);
        if !self.can_submit(&preview) {
            return;
        }
        let command = RenameWorktreeModalCommand::Rename {
            project_id: self.draft.project_id.clone(),
            name: preview.trimmed_name,
            rename_branch: self.rename_branch,
        };
        self.close_window_and_send(command, window, cx);
    }

    /// Escape and Enter while the frame itself has focus (the field owns them otherwise).
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" => {
                if event.is_held {
                    return;
                }
                self.submit(window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Escape inside the field: the input's own `Escape` action, captured before it runs.
    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }

    /// `.worktree-rename-preview-line`: 13px muted copy with a 12px mono code (every weight in
    /// this skin is flattened to 400 by `.ghostex-settings-shadcn.ghostex-settings-shadcn *`).
    fn render_preview_line(&self, label: &'static str, code: String) -> AnyElement {
        let lp = self.palette;
        h_flex()
            .min_w_0()
            .flex_wrap()
            .items_baseline()
            .text_size(px(13.0))
            .line_height(px(18.85))
            .text_color(hsla(lp.muted))
            .child(label)
            .child(
                div()
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    .line_height(px(17.4))
                    .text_color(hsla(lp.text(0.94)))
                    .child(code),
            )
            .into_any_element()
    }

    /// `FieldGroup > Field`: the 14px label (`leading-snug`), the input, and the preview description, 12px apart.
    fn render_field(&self, preview: &RenamePreview, window: &Window, cx: &App) -> AnyElement {
        let lp = self.palette;
        let folder = if preview.next_folder_name.is_empty() {
            NOT_SET.to_string()
        } else {
            preview.next_folder_name.clone()
        };
        let branch = if preview.trimmed_name.is_empty() {
            NOT_SET.to_string()
        } else {
            preview.trimmed_name.clone()
        };
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(19.25))
                    .text_color(hsla(lp.foreground))
                    .child(NAME_LABEL),
            )
            .child(modal_legacy_text_input(
                &lp,
                &self.name_input,
                false,
                window,
                cx,
            ))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(3.0))
                    .child(self.render_preview_line(FOLDER_PREVIEW, folder))
                    .when(self.rename_branch, |this| {
                        this.child(self.render_preview_line(BRANCH_PREVIEW, branch))
                    }),
            )
            .into_any_element()
    }

    /// `.worktree-rename-branch-option`: the bordered checkbox row.
    fn render_branch_option(&self, cx: &mut Context<Self>) -> AnyElement {
        let lp = self.palette;
        h_flex()
            .id("rename-worktree-branch-option")
            .w_full()
            .min_w_0()
            .items_start()
            .gap(px(10.0))
            .p(px(12.0))
            .border_1()
            .border_color(hsla(css_fade(lp.input, 0.62)))
            .text_color(hsla(lp.text(0.88)))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.toggle_rename_branch(cx);
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .mt(px(2.0))
                    .child(modal_square_checkbox(&lp, self.rename_branch, false)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.2))
                            .child(RENAME_BRANCH_LABEL),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.2))
                            .text_color(hsla(css_fade(lp.muted, 0.92)))
                            .child(RENAME_BRANCH_HELP),
                    ),
            )
            .into_any_element()
    }

    /// `.worktree-rename-message`: 12px blended copy, destructive when blocking.
    fn render_message(&self, text: String, blocking: bool) -> AnyElement {
        let lp = self.palette;
        div()
            .w_full()
            .text_size(px(12.0))
            .line_height(px(16.8))
            .text_color(hsla(if blocking {
                lp.destructive
            } else {
                lp.text(0.84)
            }))
            .child(text)
            .into_any_element()
    }

    /// `.worktree-rename-modal-body`: a 16px grid inside 20px padding that scrolls past 380px.
    fn render_body(
        &self,
        preview: &RenamePreview,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lp = self.palette;
        let mut body = v_flex()
            .id("rename-worktree-body")
            .w_full()
            .min_h_0()
            .max_h(px(BODY_MAX_HEIGHT))
            .overflow_y_scroll()
            .p(px(20.0))
            .gap(px(16.0))
            .child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(MODAL_MONO_FONT)
                    .text_size(px(12.0))
                    // The dialog's 14px/20px body line height is inherited as the 1.4286 factor.
                    .line_height(px(17.14))
                    .text_color(hsla(css_fade(lp.muted, 0.92)))
                    // The worktree's folder opens in Files (CDXC:PathLinks 2026-11-08).
                    .child(path_link(
                        "rename-worktree-current-path",
                        self.draft.current_path.clone(),
                        self.draft.current_path.clone(),
                    )),
            )
            .child(self.render_field(preview, window, cx))
            .child(self.render_branch_option(cx));
        if let Some(reason) = &self.draft.blocking_reason {
            body = body.child(self.render_message(reason.clone(), true));
        } else if let Some(error) = &preview.submit_error {
            body = body.child(self.render_message(error.clone(), true));
        }
        for warning in &self.draft.warnings {
            body = body.child(self.render_message(warning.clone(), false));
        }
        body.into_any_element()
    }

    fn render_footer(&self, preview: &RenamePreview, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let lp = self.palette;
        vec![
            modal_legacy_action_button(
                &lp,
                "rename-worktree-cancel",
                CANCEL,
                ModalLegacyButtonTone::Outline,
                false,
                |this, window, cx| this.cancel(window, cx),
                cx,
            ),
            modal_legacy_action_button(
                &lp,
                "rename-worktree-submit",
                RENAME,
                ModalLegacyButtonTone::Primary,
                !self.can_submit(preview),
                |this, window, cx| this.submit(window, cx),
                cx,
            ),
        ]
    }
}

impl Render for GpuiRenameWorktreeModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lp = self.palette;
        let preview = self.preview(cx);
        let subject = match self.draft.branch.as_deref() {
            Some(branch) => format!("{} · {branch}", self.draft.worktree_name),
            None => self.draft.worktree_name.clone(),
        };
        // `DialogTitle.text-xl` (20px with Tailwind's 28px line height, since tailwind-merge drops
        // `leading-none` for `text-xl`, at the skin's 500) over `.worktree-rename-modal-subject`, 4px apart.
        let header = v_flex()
            .min_w_0()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(20.0))
                    .line_height(px(28.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(TITLE),
            )
            .child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.0))
                    .line_height(px(20.15))
                    .text_color(hsla(lp.text(0.78)))
                    .child(subject),
            );
        let body = self.render_body(&preview, window, cx);
        let footer = self.render_footer(&preview, cx);
        modal_legacy_shell(
            &lp,
            "ghostex-gpui-rename-worktree-modal",
            DIALOG_WIDTH,
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            header,
            body,
            footer,
            cx,
        )
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiRenameWorktreeModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
