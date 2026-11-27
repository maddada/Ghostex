//! Rename and Delete for a drawing, from its ⋯ in the switcher: the Files view's rename dialog
//! look (native_docs/rename_dialog.rs) over the Canvas view. The drawing's page hides while a
//! dialog is up, because a browser page cannot sit under GPUI content.

use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseButton, ParentElement as _, Styled as _, Subscription, Window, deferred,
    div, px,
};
use gpui_component::input::{InputEvent, InputState};

use super::{CANVAS_DELETE_GOES_TO_TRASH, drawing_name};
use crate::GhostexGpuiApp;
use crate::app::window::native_modal_kit::*;

const DIALOG_WIDTH: f32 = 420.0;

pub(crate) enum CanvasDialogKind {
    Rename { input: Entity<InputState> },
    Delete,
}

pub(crate) struct CanvasDialog {
    pub(crate) file: String,
    pub(crate) kind: CanvasDialogKind,
    pub(crate) error: Option<String>,
    _subscription: Option<Subscription>,
}

impl GhostexGpuiApp {
    pub(crate) fn canvas_open_rename(
        &mut self,
        file: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(drawing_name(file)));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.canvas_submit_dialog(window, cx),
                InputEvent::Change => {
                    if let Some(dialog) = this.canvas.dialog.as_mut() {
                        dialog.error = None;
                    }
                    cx.notify();
                }
                _ => {}
            },
        );
        crate::app::native_docs::actions::focus_and_select_all(&input, window);
        self.canvas.dialog = Some(CanvasDialog {
            file: file.to_string(),
            kind: CanvasDialogKind::Rename { input },
            error: None,
            _subscription: Some(subscription),
        });
        self.canvas_sync_page(cx);
    }

    pub(crate) fn canvas_open_delete(&mut self, file: &str, cx: &mut Context<Self>) {
        self.canvas.dialog = Some(CanvasDialog {
            file: file.to_string(),
            kind: CanvasDialogKind::Delete,
            error: None,
            _subscription: None,
        });
        self.canvas_sync_page(cx);
    }

    pub(crate) fn canvas_close_dialog(&mut self, cx: &mut Context<Self>) {
        if self.canvas.dialog.take().is_some() {
            self.canvas_sync_page(cx);
        }
    }

    fn canvas_submit_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.canvas.dialog.as_ref() else {
            return;
        };
        let file = dialog.file.clone();
        let result = match &dialog.kind {
            CanvasDialogKind::Rename { input } => {
                let name = input.read(cx).value().to_string();
                self.canvas_rename_drawing(&file, &name, cx)
            }
            CanvasDialogKind::Delete => self.canvas_delete_drawing(&file, cx),
        };
        match result {
            Ok(()) => self.canvas_close_dialog(cx),
            Err(error) => {
                if let Some(dialog) = self.canvas.dialog.as_mut() {
                    dialog.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    /// The dialog over the Canvas view, or `None` when none is open.
    pub(crate) fn render_canvas_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.canvas.dialog.as_ref()?;
        let p = self.gpui_native_notice_palette();
        let name = drawing_name(&dialog.file);
        let error = dialog.error.clone();
        let (title, description, action, body): (String, Option<String>, &str, Option<AnyElement>) =
            match &dialog.kind {
                CanvasDialogKind::Rename { input } => {
                    let field = modal_text_input(&p, input, false, window, cx);
                    (
                        "Rename drawing".to_string(),
                        None,
                        "Rename",
                        Some(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(modal_section_title(&p, "Name"))
                                .child(field)
                                .into_any_element(),
                        ),
                    )
                }
                CanvasDialogKind::Delete => (
                    format!("Delete “{name}”?"),
                    Some(
                        if CANVAS_DELETE_GOES_TO_TRASH {
                            "It moves to the Trash."
                        } else {
                            "This can't be undone."
                        }
                        .to_string(),
                    ),
                    "Delete",
                    None,
                ),
            };
        let empty = match &dialog.kind {
            CanvasDialogKind::Rename { input } => input.read(cx).value().trim().is_empty(),
            CanvasDialogKind::Delete => false,
        };
        let cancel = modal_action_button(
            &p,
            "canvas-dialog-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |this: &mut Self, _window, cx| this.canvas_close_dialog(cx),
            cx,
        );
        let submit = modal_action_button(
            &p,
            "canvas-dialog-submit",
            action,
            None,
            ModalButtonTone::Neutral,
            empty,
            |this: &mut Self, window, cx| this.canvas_submit_dialog(window, cx),
            cx,
        );
        let card = div()
            .id("canvas-dialog")
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
            .child(modal_header(&p, title, description))
            .children(body)
            .children(error.map(|error| modal_error(&p, error)))
            .child(modal_footer(vec![cancel, submit]));
        Some(
            deferred(
                div()
                    .id("canvas-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(16.0))
                    .bg(crate::app::helpers::glass_veil(gpui::black().opacity(0.3)))
                    .occlude()
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        match event.keystroke.key.as_str() {
                            "escape" => this.canvas_close_dialog(cx),
                            "enter"
                                if matches!(
                                    this.canvas.dialog.as_ref().map(|dialog| &dialog.kind),
                                    Some(CanvasDialogKind::Delete)
                                ) =>
                            {
                                this.canvas_submit_dialog(window, cx)
                            }
                            _ => return,
                        }
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _window, cx| {
                            this.canvas_close_dialog(cx);
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
