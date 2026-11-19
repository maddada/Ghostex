//! The files list's "Rename item" dialog (ported from the former React Docs page's
//! `ManageRenameDialog`): the app-modal look over the Files view,
//! the whole current name selected, Cancel and Rename, the refusal under the field, Escape or a
//! click outside to close.

use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, ParentElement as _, Styled as _, Subscription, Window, deferred,
    div, px,
};
use gpui_component::input::{InputEvent, InputState};
use serde_json::json;

use super::files::parent_path;
use crate::GhostexGpuiApp;
use crate::app::window::native_modal_kit::*;

/// `.gx-app-modal`'s width (`--gx-modal-width` default).
const DIALOG_WIDTH: f32 = 460.0;

pub(crate) struct DocsRenameDialog {
    pub(crate) path: String,
    pub(crate) input: Entity<InputState>,
    pub(crate) error: Option<String>,
    pub(crate) renaming: bool,
    _subscription: Subscription,
}

impl GhostexGpuiApp {
    /// Opens the dialog for the tree item at `path`, its whole name selected.
    pub(crate) fn native_docs_open_rename(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.native_docs_submit_rename(window, cx),
                InputEvent::Change => {
                    if let Some(dialog) = this.native_docs.rename_dialog.as_mut() {
                        dialog.error = None;
                    }
                    this.native_docs_notify(cx);
                }
                _ => {}
            },
        );
        super::actions::focus_and_select_all(&input, window);
        self.native_docs.rename_dialog = Some(DocsRenameDialog {
            path: path.to_string(),
            input,
            error: None,
            renaming: false,
            _subscription: subscription,
        });
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_close_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.native_docs.rename_dialog.take().is_some() {
            if let Some(focus) = self.native_docs.focus.clone() {
                focus.focus(window, cx);
            }
            self.native_docs_notify(cx);
        }
    }

    fn native_docs_rename_error(&mut self, error: &str, cx: &mut Context<Self>) {
        if let Some(dialog) = self.native_docs.rename_dialog.as_mut() {
            dialog.error = Some(error.to_string());
            dialog.renaming = false;
        }
        self.native_docs_notify(cx);
    }

    /// Rename: `validateManageRenameFileName`, the collision check, the unsaved-file rules, then
    /// the file bridge's `rename`; open files, drafts, notes and folders follow the item.
    pub(crate) fn native_docs_submit_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.native_docs.rename_dialog.as_ref() else {
            return;
        };
        if dialog.renaming {
            return;
        }
        let path = dialog.path.clone();
        let name = dialog.input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let refusal = if name == "." || name == ".." {
            Some("Use a normal file name.")
        } else if name.contains(['/', '\\', '\0']) {
            Some("File names cannot contain path separators.")
        } else {
            None
        };
        if let Some(refusal) = refusal {
            self.native_docs_rename_error(refusal, cx);
            return;
        }
        if !self
            .native_docs
            .entries
            .iter()
            .any(|entry| entry.path == path)
        {
            self.native_docs_rename_error("This item is no longer available.", cx);
            return;
        }
        let parent = parent_path(&path);
        let new_path = if parent.is_empty() {
            name.clone()
        } else {
            format!("{parent}/{name}")
        };
        if new_path == path {
            self.native_docs_close_rename(window, cx);
            return;
        }
        if new_path.to_lowercase() != path.to_lowercase()
            && self
                .native_docs
                .entries
                .iter()
                .any(|entry| entry.path.to_lowercase() == new_path.to_lowercase())
        {
            self.native_docs_rename_error("A file or folder with that name already exists.", cx);
            return;
        }
        if self
            .native_docs
            .document(&path)
            .is_some_and(|document| document.saving)
        {
            self.native_docs_rename_error(
                "Wait for the current save to finish before renaming.",
                cx,
            );
            return;
        }
        let folder_prefix = format!("{path}/");
        if self
            .native_docs
            .documents
            .iter()
            .any(|document| document.dirty && document.path.starts_with(&folder_prefix))
        {
            self.native_docs_rename_error("Save the current file before renaming its folder.", cx);
            return;
        }
        if let Some(dialog) = self.native_docs.rename_dialog.as_mut() {
            dialog.renaming = true;
        }
        self.native_docs_notify(cx);
        let request =
            self.native_docs_request("rename", json!({ "path": path, "newPath": new_path }));
        let generation = self.native_docs.generation;
        self.native_docs_begin_operation("rename", &path);
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            this.native_docs_end_operation();
            if this.native_docs.generation != generation {
                return;
            }
            if response.get("error").is_some() {
                let error = response["error"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                    .unwrap_or("Could not rename item.")
                    .to_string();
                this.native_docs_rename_error(&error, cx);
                return;
            }
            this.native_docs.rename_dialog = None;
            this.native_docs_remap_paths(&path, &new_path, cx);
            this.native_docs_refresh(cx);
        });
    }

    /// The dialog over the Files view with its dimmed backdrop, or `None` when it is closed.
    pub(crate) fn render_native_docs_rename_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.native_docs.rename_dialog.as_ref()?;
        // CDXC:Theming 2026-11-20 (decision in helpers/chrome_palette.rs): under glass the dialog
        // takes the lifted fill of the dialogs inside Settings (`ModalPalette::lifted_dialog_fill`)
        // with the frosted dialogs' washes for its field, which is the notice palette, instead of
        // the near-black `frosted_modal_fill` that nothing blurs here. Glass off, both palettes
        // are the solid one.
        let p = self.gpui_native_notice_palette();
        let (input, error, renaming) =
            (dialog.input.clone(), dialog.error.clone(), dialog.renaming);
        let empty = input.read(cx).value().trim().is_empty();
        let field = modal_text_input(&p, &input, renaming, window, cx);
        let cancel = modal_action_button(
            &p,
            "docs-rename-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            renaming,
            |this: &mut Self, window, cx| this.native_docs_close_rename(window, cx),
            cx,
        );
        let submit = modal_action_button(
            &p,
            "docs-rename-submit",
            if renaming { "Renaming" } else { "Rename" },
            None,
            ModalButtonTone::Neutral,
            renaming || empty,
            |this: &mut Self, window, cx| this.native_docs_submit_rename(window, cx),
            cx,
        );
        let card = div()
            .id("docs-rename-dialog")
            .w(px(DIALOG_WIDTH))
            .max_w_full()
            .flex()
            .flex_col()
            .gap(px(MODAL_SECTION_GAP))
            .p(px(MODAL_WINDOW_PADDING))
            .rounded(px(MODAL_RADIUS_SECTION))
            .bg(hsla(p.surface))
            .border_1()
            .border_color(hsla(p.hairline))
            .shadow_xl()
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(modal_header(
                &p,
                "Rename item",
                Some("Choose a new name for the selected file or folder."),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, "Name"))
                    .child(field)
                    .children(error.map(|error| modal_error(&p, error))),
            )
            .child(modal_footer(vec![cancel, submit]));
        Some(
            deferred(
                div()
                    .id("docs-rename-backdrop")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(16.0))
                    // CDXC:Theming 2026-11-20: under glass a veil of the work area's own colour
                    // (`glass_veil`), not black.
                    .bg(crate::app::helpers::glass_veil(gpui::black().opacity(0.3)))
                    .occlude()
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "escape" {
                            this.native_docs_close_rename(window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            if !this
                                .native_docs
                                .rename_dialog
                                .as_ref()
                                .is_some_and(|dialog| dialog.renaming)
                            {
                                this.native_docs_close_rename(window, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .child(card),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
