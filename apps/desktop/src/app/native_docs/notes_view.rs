//! The notes UI: the header's note buttons, the toolbar over selected text, the note composer and
//! the Annotations list, drawn natively over the Docs view.

use std::cell::Cell;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, anchored, deferred, div, point, px,
};
use gpui_component::input::Input;

use super::annotations::{DocsAnnotationType, DocsQuickLabelId, annotation_review_counts};
use super::files_list::{header_icon, header_tile};
use super::notes::{DocsSendStatus, SEND_STATUS_DURATION, hex};
use super::palette::DocsPalette;
use crate::GhostexGpuiApp;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::{titlebar_svg_icon, titlebar_tooltip};

thread_local! {
    static NOTES_LIST_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
    static GLOBAL_COMMENT_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
    static REVIEW_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

const TOOLBAR_HEIGHT: f32 = 42.0;
const TOOLBAR_WIDTH_ESTIMATE: f32 = 228.0;
const TOOLBAR_EDGE_MARGIN: f32 = 18.0;
const TOOLBAR_GAP: f32 = 8.0;

fn probe(cell: &'static std::thread::LocalKey<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    gpui::canvas(
        move |bounds, _, _| cell.with(|c| c.set(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// A markdown wrap the formatting toolbar applies, removed again when the selection already has it.
fn wrap_selection(selected: &str, before: &str, after: &str) -> String {
    if selected.len() >= before.len() + after.len()
        && selected.starts_with(before)
        && selected.ends_with(after)
    {
        selected[before.len()..selected.len() - after.len()].to_string()
    } else {
        format!("{before}{selected}{after}")
    }
}

impl GhostexGpuiApp {
    /// The note buttons in a Markdown document's header, left to right: Annotations list, Add
    /// global comment, Send, Review, Copy feedback and Clear.
    ///
    /// CDXC:Docs 2026-09-14 DECISION:
    /// User: keep the actions ordered from right to left as files-list toggle, Reload, Clear, Copy, Add global comment, and Annotations list; use a trash icon for Clear and label the annotations tooltip "Annotations list".
    ///
    /// CDXC:Docs 2026-09-16 DECISION:
    /// User: do not show the long destination on the Send button; keep that in the tooltip. The button reads "Send 5" or "Copy 5" when there is room, and is icon-only below 560px. The Review menu beside it carries Resend all and, with notes in several files, Send new across all files. There is no Finish review, Undo finish, or Archive: Docs is a side pane, not a review session.
    pub(crate) fn render_native_docs_note_actions(
        &mut self,
        p: &DocsPalette,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let notes = self.native_docs_active_notes();
        let total = notes.len();
        let counts = annotation_review_counts(notes);
        let origin = self
            .native_docs
            .active_document()
            .and_then(|document| document.origin_session);
        let target = self.docs_annotation_feedback_target_or_origin(origin);
        let now = Instant::now();
        let status = self
            .native_docs
            .send_status
            .clone()
            .filter(|(status, at)| {
                matches!(status, DocsSendStatus::Sending)
                    || now.duration_since(*at) < SEND_STATUS_DURATION
            })
            .map(|(status, _)| status);
        let resending = counts.pending == 0;
        let send_count = if resending {
            counts.sent
        } else {
            counts.pending
        };
        let verb = if target.is_some() { "Send" } else { "Copy" };
        let (send_label, send_color): (String, gpui::Hsla) = match &status {
            Some(DocsSendStatus::Sending) => ("Sending".into(), p.muted),
            Some(DocsSendStatus::Sent { delivery, .. }) => (
                match *delivery {
                    "chat" => "Added to chat",
                    "terminal" => "Added to terminal",
                    _ => "Copied to clipboard",
                }
                .into(),
                p.green,
            ),
            Some(DocsSendStatus::Error(_)) => ("Couldn't send".into(), p.danger),
            Some(DocsSendStatus::Notice(message)) => (message.clone(), p.muted),
            None => (format!("{verb} {send_count}"), p.send),
        };
        let send_tooltip: SharedString = match (&status, &target) {
            (Some(DocsSendStatus::Sent { count, files, .. }), _) => {
                format!("{send_label}: {count} annotations across {files} files").into()
            }
            (Some(DocsSendStatus::Error(error)), _) => error.clone().into(),
            (_, _) if total == 0 => "No annotations to send".into(),
            (_, Some(target)) => format!(
                "Send {send_count} {}annotations{} to the {} of {} in {} (⌘↩)",
                if resending { "" } else { "new " },
                if resending { " again" } else { "" },
                match target.surface {
                    crate::app::docs_annotation_feedback::DocsAnnotationFeedbackSurface::Chat => "chat",
                    crate::app::docs_annotation_feedback::DocsAnnotationFeedbackSurface::Terminal => "terminal",
                },
                target.agent_label,
                target.session_title,
            )
            .into(),
            (_, None) => format!(
                "Copy {send_count} {}annotations to the clipboard. No agent session is selected in the sidebar",
                if resending { "" } else { "new " },
            )
            .into(),
        };
        let clear_armed = self
            .native_docs
            .clear_armed_until
            .is_some_and(|until| until > now);
        let list_open = self.native_docs.notes_list_open;
        let danger = p.danger;
        vec![
            header_tile(
                "native-docs-notes-list",
                header_icon("docs/t-messages-2.svg", false, p),
                list_open,
                false,
                p,
            )
            .child(probe(&NOTES_LIST_ANCHOR))
            .when(total > 0, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(1.0))
                        .right(px(1.0))
                        .min_w(px(14.0))
                        .h(px(14.0))
                        .px(px(3.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(p.raised)
                        .border_1()
                        .border_color(p.border_strong)
                        .text_size(px(9.0))
                        .text_color(p.text)
                        .child(total.to_string()),
                )
            })
            .tooltip(|window, cx| titlebar_tooltip("Annotations list", window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                this.native_docs.notes_list_open = !this.native_docs.notes_list_open;
                this.native_docs_notify(cx);
            }))
            .into_any_element(),
            header_tile(
                "native-docs-global-comment",
                header_icon("docs/t-message-plus-2.svg", false, p),
                false,
                false,
                p,
            )
            .child(probe(&GLOBAL_COMMENT_ANCHOR))
            .tooltip(|window, cx| titlebar_tooltip("Add global comment", window, cx))
            .on_click(cx.listener(|this, _, window, cx| {
                let anchor = GLOBAL_COMMENT_ANCHOR.with(|cell| cell.get());
                this.native_docs_open_composer(Some(anchor), None, "", window, cx);
            }))
            .into_any_element(),
            div()
                .id("native-docs-send")
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .h(px(27.0))
                .px(px(8.0))
                .rounded(px(7.0))
                .cursor_pointer()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(send_color)
                .hover(|style| style.bg(p.control_hover))
                .child(titlebar_svg_icon("docs/t-send-2.svg", 16.0, send_color))
                .when(!compact, |this| this.child(send_label))
                .tooltip(move |window, cx| titlebar_tooltip(send_tooltip.clone(), window, cx))
                .on_click(cx.listener(|this, _, _, cx| this.native_docs_send_notes(false, cx)))
                .into_any_element(),
            header_tile(
                "native-docs-review",
                header_icon("docs/t-checklist-2.svg", false, p),
                false,
                false,
                p,
            )
            .child(probe(&REVIEW_MENU_ANCHOR))
            .tooltip(|window, cx| titlebar_tooltip("Review actions", window, cx))
            .on_click(
                cx.listener(|this, _, window, cx| this.show_native_docs_review_menu(window, cx)),
            )
            .into_any_element(),
            header_tile(
                "native-docs-copy-feedback",
                header_icon("docs/t-copy-2.svg", total == 0, p),
                false,
                total == 0,
                p,
            )
            .tooltip(|window, cx| titlebar_tooltip("Copy feedback", window, cx))
            .when(total > 0, |this| {
                this.on_click(cx.listener(|this, _, _, cx| this.native_docs_copy_feedback(cx)))
            })
            .into_any_element(),
            header_tile(
                "native-docs-clear",
                header_icon("docs/t-trash-2.svg", total == 0, p),
                false,
                total == 0,
                p,
            )
            .when(clear_armed, |this| this.bg(danger.opacity(0.13)))
            .tooltip(move |window, cx| {
                titlebar_tooltip(
                    if clear_armed {
                        "Confirm"
                    } else {
                        "Clear annotations"
                    },
                    window,
                    cx,
                )
            })
            .when(total > 0, |this| {
                this.on_click(cx.listener(|this, _, _, cx| this.native_docs_clear_notes(cx)))
            })
            .into_any_element(),
        ]
    }

    /// The Review menu: Resend all, and Send new across all files when notes wait in several.
    fn show_native_docs_review_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let counts = annotation_review_counts(self.native_docs_active_notes());
        let pending_paths = self.native_docs.notes.paths_with_notes(true);
        let pending_total: usize = pending_paths
            .iter()
            .map(|path| {
                annotation_review_counts(self.native_docs.notes.get(path).unwrap_or(&[])).pending
            })
            .sum();
        let mut menu = GpuiContextMenu::new();
        if pending_paths.len() > 1 {
            menu = menu.menu_with_icon(
                format!(
                    "Send new across all files ({pending_total} new in {} files)",
                    pending_paths.len()
                ),
                "titlebar/folders.svg",
                false,
                Box::new(super::actions::NativeDocsAction {
                    command: serde_json::json!({ "type": "sendAcrossFiles" }),
                }),
            );
        }
        menu.menu_with_icon(
            format!("Resend all ({} sent, {} new)", counts.sent, counts.pending),
            "docs/t-send-2.svg",
            counts.sent + counts.pending == 0,
            Box::new(super::actions::NativeDocsAction {
                command: serde_json::json!({ "type": "resendAll" }),
            }),
        )
        .toggle_below(REVIEW_MENU_ANCHOR.with(|cell| cell.get()), window, cx);
    }

    /// The toolbar over selected text: note buttons, or formatting buttons.
    ///
    /// CDXC:Docs 2026-09-16 DECISION:
    /// User: the annotation toolbar sits above the selected text, and moves below it when the selection is near the top of the editor, where "above" would land on the document header. The X button adds a "Remove this" note for the selected text (the same redline the D key adds), not dismiss the toolbar. Unselecting the text is how the toolbar goes away.
    pub(crate) fn render_native_docs_selection_toolbar(
        &mut self,
        p: &DocsPalette,
        header_bottom: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.native_docs.composer.is_some() {
            return None;
        }
        let (_, anchor) = self.native_docs_selection_quote(cx)?;
        let viewport = window.viewport_size();
        let half = TOOLBAR_WIDTH_ESTIMATE / 2.0;
        let center = f32::from(anchor.center().x).clamp(
            half + TOOLBAR_EDGE_MARGIN,
            (f32::from(viewport.width) - half - TOOLBAR_EDGE_MARGIN)
                .max(half + TOOLBAR_EDGE_MARGIN),
        );
        let above = anchor.top() - px(TOOLBAR_GAP + TOOLBAR_HEIGHT);
        let top = if above < header_bottom + px(TOOLBAR_GAP) {
            anchor.bottom() + px(TOOLBAR_GAP)
        } else {
            above
        };
        let light = p.light;
        let button = move |id: &'static str,
                           icon: &'static str,
                           color: &str,
                           light_color: &str,
                           tooltip: &'static str| {
            let tint = hex(if light { light_color } else { color }, 1.0);
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .size(px(32.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(move |style| style.bg(tint.opacity(0.16)))
                .child(titlebar_svg_icon(icon, 17.0, tint))
                .tooltip(move |window, cx| titlebar_tooltip(tooltip, window, cx))
        };
        let buttons: Vec<AnyElement> = if self.native_docs.toolbar_formatting {
            let format = |id: &'static str,
                          icon: &'static str,
                          tooltip: &'static str,
                          before: &'static str,
                          after: &'static str| {
                button(id, icon, "#ededed", "#3f3f46", tooltip)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.native_docs_wrap_selection(before, after, window, cx);
                    }))
                    .into_any_element()
            };
            vec![
                button(
                    "docs-sel-annotations",
                    "docs/t-messages-2.svg",
                    "#ededed",
                    "#3f3f46",
                    "Annotations",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.native_docs.toolbar_formatting = false;
                    this.native_docs_notify(cx);
                }))
                .into_any_element(),
                format("docs-sel-bold", "titlebar/bold.svg", "Bold", "**", "**"),
                format("docs-sel-italic", "titlebar/italic.svg", "Italic", "*", "*"),
                format(
                    "docs-sel-strike",
                    "titlebar/strikethrough.svg",
                    "Lineover",
                    "~~",
                    "~~",
                ),
                format(
                    "docs-sel-code",
                    "titlebar/code.svg",
                    "Inline Code",
                    "`",
                    "`",
                ),
                format("docs-sel-link", "titlebar/link.svg", "Link", "[", "]()"),
                format(
                    "docs-sel-wiki",
                    "titlebar/brackets.svg",
                    "Wiki Link",
                    "[[",
                    "]]",
                ),
                format(
                    "docs-sel-kbd",
                    "titlebar/keyboard.svg",
                    "Kbd",
                    "<kbd>",
                    "</kbd>",
                ),
            ]
        } else {
            let label = |id: &'static str,
                         icon: &'static str,
                         label: DocsQuickLabelId,
                         dark: &str,
                         light_color: &str,
                         tooltip: &'static str| {
                button(id, icon, dark, light_color, tooltip)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_quick_note(DocsAnnotationType::Comment, Some(label), cx);
                    }))
                    .into_any_element()
            };
            vec![
                button(
                    "docs-sel-comment",
                    "docs/t-message-plus-2.svg",
                    "#e2b340",
                    "#926b0e",
                    "Comment (C)",
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.native_docs_open_composer(None, None, "", window, cx);
                }))
                .into_any_element(),
                button(
                    "docs-sel-formatting",
                    "titlebar/typography.svg",
                    "#ededed",
                    "#3f3f46",
                    "Formatting",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.native_docs.toolbar_formatting = true;
                    this.native_docs_notify(cx);
                }))
                .into_any_element(),
                label(
                    "docs-sel-clarify",
                    "titlebar/help-circle.svg",
                    DocsQuickLabelId::Clarify,
                    "#a78bfa",
                    "#7c3aed",
                    "Clarify (1)",
                ),
                label(
                    "docs-sel-tests",
                    "titlebar/test-pipe.svg",
                    DocsQuickLabelId::NeedsTests,
                    "#f59e0b",
                    "#b45309",
                    "Needs tests (2)",
                ),
                label(
                    "docs-sel-good",
                    "titlebar/circle-check.svg",
                    DocsQuickLabelId::LooksGood,
                    "#86efac",
                    "#15803d",
                    "Looks good (3)",
                ),
                button(
                    "docs-sel-remove",
                    "titlebar/x.svg",
                    "#f87171",
                    "#dc2626",
                    "Remove this (D)",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.native_docs_quick_note(DocsAnnotationType::Redline, None, cx);
                }))
                .into_any_element(),
            ]
        };
        let toolbar = div()
            .id("native-docs-selection-toolbar")
            .flex()
            .items_center()
            .gap(px(5.0))
            .p(px(5.0))
            .rounded(px(8.0))
            .bg(p.raised)
            .border_1()
            .border_color(p.border_strong)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(buttons);
        Some(
            deferred(
                anchored()
                    .position(point(px(center - half), top))
                    .child(toolbar),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// Wraps the selection in markdown markers (or unwraps it) in the open editor.
    fn native_docs_wrap_selection(
        &mut self,
        before: &str,
        after: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self
            .native_docs
            .active_document()
            .and_then(|document| document.live.clone())
        else {
            return;
        };
        super::format_bar::wrap_inline(&editor, before, after, cx);
    }

    /// The single-key shortcuts over a selection: D, Backspace or Delete add "Remove this", C
    /// opens the composer, 1 to 3 add the quick labels, Escape drops the selection, and any other
    /// letter opens the composer with that letter typed.
    pub(crate) fn native_docs_selection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.native_docs.composer.is_some() || self.native_docs.toolbar_formatting {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
            return;
        }
        if self.native_docs_selection_quote(cx).is_none() {
            return;
        }
        let key = event.keystroke.key.as_str();
        match key {
            "d" | "backspace" | "delete" => {
                self.native_docs_quick_note(DocsAnnotationType::Redline, None, cx)
            }
            "c" => self.native_docs_open_composer(None, None, "", window, cx),
            "1" => self.native_docs_quick_note(
                DocsAnnotationType::Comment,
                Some(DocsQuickLabelId::Clarify),
                cx,
            ),
            "2" => self.native_docs_quick_note(
                DocsAnnotationType::Comment,
                Some(DocsQuickLabelId::NeedsTests),
                cx,
            ),
            "3" => self.native_docs_quick_note(
                DocsAnnotationType::Comment,
                Some(DocsQuickLabelId::LooksGood),
                cx,
            ),
            "escape" => {
                if let Some(editor) = self
                    .native_docs
                    .active_document()
                    .and_then(|document| document.editor.clone())
                {
                    editor.update(cx, |editor, cx| editor.unselect(window, cx));
                }
            }
            _ => {
                let Some(typed) = event
                    .keystroke
                    .key_char
                    .clone()
                    .filter(|text| !text.trim().is_empty())
                else {
                    return;
                };
                self.native_docs_open_composer(None, None, &typed, window, cx);
            }
        }
        cx.stop_propagation();
    }

    /// The note composer, anchored at its selection or button.
    pub(crate) fn render_native_docs_composer(
        &mut self,
        p: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let composer = self.native_docs.composer.as_ref()?;
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 24.0).clamp(280.0, 360.0);
        let left = (f32::from(composer.anchor.center().x) - width / 2.0)
            .clamp(12.0, (f32::from(viewport.width) - width - 12.0).max(12.0));
        let top = (f32::from(composer.anchor.top()) + 12.0)
            .min(f32::from(viewport.height) - 260.0)
            .max(12.0);
        let editing = composer.editing.is_some();
        let chord = if cfg!(target_os = "macos") {
            "⌘↩"
        } else {
            "Ctrl+↩"
        };
        let panel = div()
            .id("native-docs-composer")
            .relative()
            .w(px(width))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .pt(px(34.0))
            .px(px(12.0))
            .pb(px(12.0))
            .rounded(px(10.0))
            .bg(p.raised)
            .border_1()
            .border_color(p.border_strong)
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.native_docs_close_composer(cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .id("native-docs-composer-close")
                    .absolute()
                    .top(px(8.0))
                    .right(px(8.0))
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(p.control_hover))
                    .child(titlebar_svg_icon("titlebar/x.svg", 14.0, p.muted))
                    .on_click(cx.listener(|this, _, _, cx| this.native_docs_close_composer(cx))),
            )
            .child(
                div()
                    .h(px(116.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border_strong)
                    .text_size(px(12.0))
                    .p(px(8.0))
                    .child(Input::new(&composer.input).appearance(false).h_full()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        div()
                            .px(px(5.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(p.border)
                            .text_size(px(11.0))
                            .text_color(p.subtle)
                            .child(chord),
                    )
                    .child(
                        div()
                            .id("native-docs-composer-add")
                            .h(px(28.0))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .rounded(px(7.0))
                            .border_1()
                            .border_color(p.border_strong)
                            .cursor_pointer()
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .hover(|style| style.bg(p.control_hover))
                            .child(if editing { "Save" } else { "Add" })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.native_docs_commit_composer(window, cx);
                            })),
                    ),
            );
        Some(
            deferred(anchored().position(point(px(left), px(top))).child(panel))
                .with_priority(2)
                .into_any_element(),
        )
    }

    /// The Annotations list, under its header button.
    pub(crate) fn render_native_docs_notes_list(
        &mut self,
        p: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.native_docs.notes_list_open {
            return None;
        }
        let anchor = NOTES_LIST_ANCHOR.with(|cell| cell.get());
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 28.0).min(360.0);
        let left = (f32::from(anchor.right()) - width).max(14.0);
        let top = anchor.bottom() + px(8.0);
        let max_height = (f32::from(viewport.height) - 76.0).min(520.0);
        let notes = self.native_docs_active_notes().to_vec();
        let cards: Vec<AnyElement> = notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                let color = hex(note.color(), 1.0);
                let sent = !note.is_pending();
                let redline = note.kind == DocsAnnotationType::Redline;
                let remove_id = note.id.clone();
                let edit_id = note.id.clone();
                div()
                    .id(("native-docs-note", index))
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .pt(px(9.0))
                    .pl(px(9.0))
                    .pb(px(9.0))
                    .pr(px(33.0))
                    .rounded(px(4.0))
                    .bg(color.opacity(0.04))
                    .border_1()
                    .border_color(color.opacity(if redline { 0.28 } else { 0.24 }))
                    .when(sent, |this| this.opacity(0.78))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(11.0))
                            .child(div().text_color(color).child(note.type_label()))
                            .when(sent, |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(3.0))
                                        .text_color(p.subtle)
                                        .child(titlebar_svg_icon(
                                            "titlebar/check.svg",
                                            11.0,
                                            p.subtle,
                                        ))
                                        .child("Sent"),
                                )
                            }),
                    )
                    .when(!note.quote.is_empty(), |this| {
                        this.child(
                            div()
                                .pl(px(8.0))
                                .border_l_2()
                                .border_color(color.opacity(0.6))
                                .text_size(px(12.0))
                                .text_color(p.muted)
                                .max_h(px(96.0))
                                .overflow_hidden()
                                .when(redline, |this| this.line_through())
                                .child(note.quote.clone()),
                        )
                    })
                    .when(!note.note.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(p.text)
                                .child(note.note.clone()),
                        )
                    })
                    .when(note.kind == DocsAnnotationType::Comment, |this| {
                        this.child(
                            div()
                                .id(("native-docs-note-edit", index))
                                .absolute()
                                .top(px(7.0))
                                .right(px(31.0))
                                .size(px(22.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.0))
                                .cursor_pointer()
                                .hover(|style| style.bg(p.control_hover))
                                .child(titlebar_svg_icon("titlebar/pencil.svg", 13.0, p.muted))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let anchor = NOTES_LIST_ANCHOR.with(|cell| cell.get());
                                    this.native_docs_open_composer(
                                        Some(anchor),
                                        Some(edit_id.clone()),
                                        "",
                                        window,
                                        cx,
                                    );
                                })),
                        )
                    })
                    .child(
                        div()
                            .id(("native-docs-note-remove", index))
                            .absolute()
                            .top(px(7.0))
                            .right(px(7.0))
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(p.control_hover))
                            .child(titlebar_svg_icon("titlebar/x.svg", 13.0, p.muted))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.native_docs_remove_note(&remove_id, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();
        let panel = div()
            .id("native-docs-notes-list")
            .w(px(width))
            .max_h(px(max_height))
            .flex()
            .flex_col()
            .rounded(px(5.0))
            .bg(p.raised)
            .border_1()
            .border_color(p.border_strong)
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.native_docs.notes_list_open = false;
                this.native_docs_notify(cx);
            }))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(40.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(p.border)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.text)
                    .child("Annotations"),
            )
            .child(
                div()
                    .id("native-docs-notes-list-body")
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(10.0))
                    .overflow_y_scroll()
                    .when(cards.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(p.subtle)
                                .child("No annotations"),
                        )
                    })
                    .children(cards),
            );
        Some(
            deferred(anchored().position(point(px(left), top)).child(panel))
                .with_priority(2)
                .into_any_element(),
        )
    }
}
