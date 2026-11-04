//! The notes UI: the header's note buttons, the toolbar over selected text, the note composer and
//! the Annotations list, drawn natively over the Docs view.

use std::cell::Cell;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, Window, anchored, deferred, div, point, px,
};
use gpui_component::input::Textarea;

use super::annotations::{DocsAnnotationType, DocsQuickLabelId, annotation_review_counts};
use super::files_list::{header_icon, header_tile};
use super::notes::{DocsSendStatus, SEND_STATUS_DURATION, hex};
use super::notes_windows::{COMPOSER_RADIUS, notes_frosted};
use super::palette::DocsPalette;
use crate::GhostexGpuiApp;
use crate::app::helpers::{
    frosted_menu_fill, titlebar_popup_menu_background, titlebar_popup_menu_border_color,
    titlebar_svg_icon, titlebar_tooltip,
};
use crate::app::window::frosted_host::DOCS_SELECTION_TOOLBAR_RADIUS;

thread_local! {
    static NOTES_LIST_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
    static GLOBAL_COMMENT_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

const TOOLBAR_BUTTON: f32 = 32.0;
/// The toolbar's padding and the gap between its buttons.
const TOOLBAR_SPACING: f32 = 5.0;
const TOOLBAR_EDGE_MARGIN: f32 = 18.0;
const TOOLBAR_GAP: f32 = 8.0;

/// The composer's height: the close button's 34px row, the 116px field, the 10px gap, the 28px
/// button row, the 12px bottom padding and the 1px border on each side.
const COMPOSER_HEIGHT: f32 = 202.0;

/// How many buttons the toolbar shows: Comment, Formatting, the three quick labels and Remove,
/// or Annotations and the seven formatting marks.
fn selection_toolbar_buttons(formatting: bool) -> usize {
    if formatting { 8 } else { 6 }
}

/// The toolbar's size: its buttons, the gaps between them, its padding and its 1px border.
fn selection_toolbar_size(formatting: bool) -> gpui::Size<Pixels> {
    let buttons = selection_toolbar_buttons(formatting) as f32;
    let height = TOOLBAR_BUTTON + 2.0 * TOOLBAR_SPACING + 2.0;
    let width = buttons * TOOLBAR_BUTTON + (buttons + 1.0) * TOOLBAR_SPACING + 2.0;
    gpui::size(px(width), px(height))
}

fn probe(cell: &'static std::thread::LocalKey<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    gpui::canvas(
        move |bounds, _, _| cell.with(|c| c.set(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// Where a finished Send landed, in the words the Send button shows.
fn sent_label(delivery: &str) -> &'static str {
    match delivery {
        "chat" => "Added to chat",
        "terminal" => "Added to terminal",
        _ => "Copied to clipboard",
    }
}

impl GhostexGpuiApp {
    /// The note buttons in a Markdown document's header, left to right: Annotations list, Add
    /// global comment, Send, Copy feedback and Clear.
    ///
    /// CDXC:Docs 2026-09-14 DECISION:
    /// User: keep the actions ordered from right to left as files-list toggle, Reload, Clear, Copy, Add global comment, and Annotations list; use a trash icon for Clear and label the annotations tooltip "Annotations list". On 2026-10-15 the user removed Reload ("can we please remove the refresh button and do something smarter?", disk_sync.rs), so the order is files-list toggle, Clear, Copy, Add global comment, Annotations list.
    ///
    /// CDXC:Docs 2026-09-28 DECISION:
    /// User: Send is a plain header icon in the same color as the others, with no "Send 5"/"Copy 5" label (supersedes the 2026-09-16 label). The number of new notes sits next to the icon, and with no notes the button looks disabled. Once every note has been sent the number goes away and a click sends them all again (the 2026-09-15 Send decision). The destination stays in the tooltip. There is no Review menu (Resend all and Send new across all files were removed the same day), and no Finish review, Undo finish, or Archive: Docs is a side pane, not a review session.
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
        // Beside the icon: the count of new notes, or for a few seconds after a click, where they
        // went (dropped on a narrow pane).
        let send_label: Option<(String, gpui::Hsla)> = match &status {
            Some(DocsSendStatus::Sending) => Some(("Sending".into(), p.muted)),
            Some(DocsSendStatus::Sent { delivery, .. }) => {
                Some((sent_label(delivery).into(), p.green))
            }
            Some(DocsSendStatus::Error(_)) => Some(("Couldn't send".into(), p.danger)),
            Some(DocsSendStatus::Notice(message)) => Some((message.clone(), p.muted)),
            None => (!resending).then(|| (counts.pending.to_string(), p.toolbar_icon)),
        }
        .filter(|_| status.is_none() || !compact);
        let send_tooltip: SharedString = match (&status, &target) {
            (Some(DocsSendStatus::Sent {
                count,
                files,
                delivery,
            }), _) => format!(
                "{}: {count} annotations across {files} files",
                sent_label(delivery)
            )
            .into(),
            (Some(DocsSendStatus::Error(error)), _) => error.clone().into(),
            (_, _) if total == 0 => "No annotations to send".into(),
            (_, Some(target)) => format!(
                "Send {send_count} {}annotations{} to the {} of {} in {} ({})",
                if resending { "" } else { "new " },
                if resending { " again" } else { "" },
                match target.surface {
                    crate::app::docs_annotation_feedback::DocsAnnotationFeedbackSurface::Chat => "chat",
                    crate::app::docs_annotation_feedback::DocsAnnotationFeedbackSurface::Terminal => "terminal",
                },
                target.agent_label,
                target.session_title,
                if cfg!(target_os = "macos") {
                    "⌘↩"
                } else {
                    "Ctrl+↩"
                },
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
                header_icon("files-view/t-messages-2.svg", false, p),
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
                header_icon("files-view/t-message-plus-2.svg", false, p),
                false,
                false,
                p,
            )
            .child(probe(&GLOBAL_COMMENT_ANCHOR))
            .tooltip(|window, cx| titlebar_tooltip("Add global comment", window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                let anchor = GLOBAL_COMMENT_ANCHOR.with(|cell| cell.get());
                this.native_docs_open_composer(Some(anchor), None, "", cx);
            }))
            .into_any_element(),
            header_tile(
                "native-docs-send",
                header_icon("files-view/t-send-2.svg", total == 0, p),
                false,
                total == 0,
                p,
            )
            .when_some(send_label, |this, (label, color)| {
                this.w_auto()
                    .gap(px(5.0))
                    .px(px(7.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color)
                    .child(label)
            })
            .tooltip(move |window, cx| titlebar_tooltip(send_tooltip.clone(), window, cx))
            .when(total > 0, |this| {
                this.on_click(cx.listener(|this, _, _, cx| this.native_docs_send_notes(cx)))
            })
            .into_any_element(),
            header_tile(
                "native-docs-copy-feedback",
                header_icon("files-view/t-copy-2.svg", total == 0, p),
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
                // Armed, the trash icon turns the destructive red with its tint, like "Confirm
                // delete" in the files list (CDXC:ContextMenus 2026-10-10 in context_menu.rs).
                if clear_armed {
                    titlebar_svg_icon(
                        "files-view/t-trash-2.svg",
                        crate::TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
                        danger,
                    )
                    .into_any_element()
                } else {
                    header_icon("files-view/t-trash-2.svg", total == 0, p)
                },
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
        let frame = self.native_docs_selection_toolbar_frame(header_bottom, window, cx);
        // Under glass the toolbar draws in its frosted host (`notes_windows.rs`), which would float
        // over the Annotations list, so it steps aside while that is open.
        let hosted = notes_frosted();
        let host_frame = frame.filter(|_| hosted && !self.native_docs.notes_list_open);
        self.native_docs_sync_toolbar_host(host_frame, window, cx);
        let frame = frame.filter(|_| !hosted)?;
        let toolbar = self.native_docs_selection_toolbar_panel(p, false, cx);
        Some(
            deferred(anchored().position(frame.origin).child(toolbar))
                .with_priority(1)
                .into_any_element(),
        )
    }

    /// Where the toolbar goes, in window coordinates, while text is selected and no composer is
    /// open.
    fn native_docs_selection_toolbar_frame(
        &self,
        header_bottom: Pixels,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.native_docs.composer.is_some() {
            return None;
        }
        let (_, anchor) = self.native_docs_selection_quote(cx)?;
        let viewport = window.viewport_size();
        let toolbar = selection_toolbar_size(self.native_docs.toolbar_formatting);
        let half = f32::from(toolbar.width) / 2.0;
        let center = f32::from(anchor.center().x).clamp(
            half + TOOLBAR_EDGE_MARGIN,
            (f32::from(viewport.width) - half - TOOLBAR_EDGE_MARGIN)
                .max(half + TOOLBAR_EDGE_MARGIN),
        );
        let above = anchor.top() - px(TOOLBAR_GAP) - toolbar.height;
        let top = if above < header_bottom + px(TOOLBAR_GAP) {
            anchor.bottom() + px(TOOLBAR_GAP)
        } else {
            above
        };
        Some(Bounds::new(point(px(center - half), top), toolbar))
    }

    /// The toolbar's bar. `frosted` draws it for its frosted host, which it fills.
    pub(crate) fn native_docs_selection_toolbar_panel(
        &mut self,
        p: &DocsPalette,
        frosted: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
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
                .size(px(TOOLBAR_BUTTON))
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
                    "files-view/t-messages-2.svg",
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
                    "files-view/t-message-plus-2.svg",
                    "#e2b340",
                    "#926b0e",
                    "Comment (C)",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.native_docs_open_composer(None, None, "", cx);
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
        debug_assert_eq!(
            buttons.len(),
            selection_toolbar_buttons(self.native_docs.toolbar_formatting)
        );
        div()
            .id("native-docs-selection-toolbar")
            .flex()
            .items_center()
            .gap(px(TOOLBAR_SPACING))
            .p(px(TOOLBAR_SPACING))
            .rounded(px(DOCS_SELECTION_TOOLBAR_RADIUS))
            .border_1()
            .map(|this| {
                if frosted {
                    this.size_full()
                        .bg(frosted_menu_fill(titlebar_popup_menu_background()))
                        .border_color(titlebar_popup_menu_border_color())
                } else {
                    this.bg(p.raised).border_color(p.border_strong).shadow_lg()
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(buttons)
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

    /// Escape over a selection drops it (and with it the selection toolbar).
    ///
    /// CDXC:Docs 2026-10-09 DECISION:
    /// User: "in the gpui file view selecting some text then typing doesn't work to replace the text i selected like it usually does in any other text file pls fix". Typing, pasting, IME input, Backspace and Delete over a selection edit the text as in any editor. The single-key annotation shortcuts the React Docs page had over a selection (D, Backspace or Delete for Remove this, C and any other letter for the comment box, 1 to 3 for the quick labels) are gone; the selection toolbar's buttons add those notes.
    pub(crate) fn native_docs_selection_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.native_docs.composer.is_some() || event.keystroke.key != "escape" {
            return;
        }
        if event.keystroke.modifiers.modified() || self.native_docs_selection_quote(cx).is_none() {
            return;
        }
        // The selection lives in the Markdown document's live editor.
        if let Some(editor) = self
            .native_docs
            .active_document()
            .and_then(|document| document.live.clone())
        {
            editor.update(cx, |editor, cx| {
                let caret = editor.cursor();
                editor.set_cursor(caret, cx);
            });
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
        let Some(anchor) = self
            .native_docs
            .composer
            .as_ref()
            .map(|composer| composer.anchor)
        else {
            self.native_docs_sync_composer_window(None, cx);
            return None;
        };
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 24.0).clamp(280.0, 360.0);
        let frame = match self.native_docs_composer_text_frame(width, cx) {
            Some(frame) => frame,
            None => {
                // A global comment, or a note whose text is not laid out: by its button.
                let left = (f32::from(anchor.center().x) - width / 2.0)
                    .clamp(12.0, (f32::from(viewport.width) - width - 12.0).max(12.0));
                let top = (f32::from(anchor.top()) + 12.0)
                    .min(f32::from(viewport.height) - 260.0)
                    .max(12.0);
                Bounds::new(
                    point(px(left), px(top)),
                    gpui::size(px(width), px(COMPOSER_HEIGHT)),
                )
            }
        };
        // Under glass the composer draws in its frosted window (`notes_windows.rs`).
        let hosted = notes_frosted();
        self.native_docs_sync_composer_window(hosted.then_some(frame), cx);
        if hosted {
            return None;
        }
        let panel = self
            .native_docs_composer_panel(p, false, window, cx)?
            .w(px(width));
        Some(
            deferred(anchored().position(frame.origin).child(panel))
                .with_priority(2)
                .into_any_element(),
        )
    }

    /// Where the composer goes beside the text it is about (`composer_placement.rs`), from the
    /// editor's layout as last painted, so it follows the text as the document scrolls and
    /// respects the Files zoom's line heights. Its first placement may scroll the document once to
    /// show the text and the composer together. `None` when the composer is not about laid-out text.
    fn native_docs_composer_text_frame(
        &mut self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        use super::composer_placement::place_composer;
        let range = self.native_docs.composer.as_ref()?.range.clone()?;
        let document = self.native_docs.active_document()?;
        let scroll = document.scroll.clone();
        let editor = document.live.as_ref()?.read(cx);
        let len = editor.text().len();
        let line_count = super::gutter::line_count(&editor.text());
        let first = editor.bounds_for_offset(range.start.min(len))?;
        let last = editor
            .bounds_for_offset(range.end.min(len))
            .unwrap_or(first);
        let area = scroll.bounds();
        if area.size.height <= px(0.0) {
            return None;
        }
        // Below the document's header and above the formatting bar that floats over its bottom.
        let header_bottom = super::document_view::HEADER_BOUNDS
            .with(|cell| cell.get())
            .bottom();
        let top = area.top().max(header_bottom);
        let bottom = (area.bottom() - px(super::markdown_body::BODY_BOTTOM_PAD - 60.0)).max(top);
        let visible = Bounds::from_corners(point(area.left(), top), point(area.right(), bottom));
        // A selection over several lines spans the text column.
        let (left, right) = if (last.top() - first.top()).abs() < px(1.0) {
            (first.left().min(last.left()), first.left().max(last.left()))
        } else {
            let full = f32::from(area.size.width);
            let column = if self.native_docs.constrain_width {
                full.min(
                    super::markdown_body::CONTENT_MAX_WIDTH * crate::ui_fonts::files_zoom_scale()
                        + super::gutter::gutter_width(self.native_docs.line_numbers, line_count),
                )
            } else {
                full
            };
            let center = f32::from(area.center().x);
            (px(center - column / 2.0), px(center + column / 2.0))
        };
        let text = Bounds::from_corners(point(left, first.top()), point(right, last.bottom()));
        let composer = self.native_docs.composer.as_mut()?;
        let may_scroll = !std::mem::replace(&mut composer.revealed, true);
        let placed = place_composer(
            text,
            visible,
            gpui::size(px(width), px(COMPOSER_HEIGHT)),
            may_scroll,
        );
        if placed.scroll_by != 0.0 {
            let max = scroll.max_offset().y;
            let mut offset = scroll.offset();
            offset.y = (offset.y - px(placed.scroll_by)).clamp(-max, px(0.0));
            scroll.set_offset(offset);
            cx.notify();
        }
        Some(placed.frame)
    }

    /// The composer's card, with its text field made for `window`. `frosted` draws it for its
    /// frosted window, which it fills.
    pub(crate) fn native_docs_composer_panel(
        &mut self,
        p: &DocsPalette,
        frosted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let input = self.native_docs_composer_input(window, cx)?;
        let editing = self.native_docs.composer.as_ref()?.editing.is_some();
        let chord = if cfg!(target_os = "macos") {
            "⌘↩"
        } else {
            "Ctrl+↩"
        };
        let panel = div()
            .id("native-docs-composer")
            .relative()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .pt(px(34.0))
            .px(px(12.0))
            .pb(px(12.0))
            .rounded(px(COMPOSER_RADIUS))
            .border_1()
            .map(|this| {
                if frosted {
                    // Its own window inherits no type from the Docs view.
                    this.size_full()
                        .font_family(p.font.clone())
                        .text_size(px(13.0))
                        .text_color(p.text)
                        .bg(frosted_menu_fill(titlebar_popup_menu_background()))
                        .border_color(titlebar_popup_menu_border_color())
                } else {
                    this.bg(p.raised).border_color(p.border_strong).shadow_lg()
                }
            })
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
                    .child(Textarea::new(&input).appearance(false).h_full()),
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
        Some(panel)
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
                                        .id(("native-docs-note-sent", index))
                                        .flex()
                                        .items_center()
                                        .gap(px(3.0))
                                        .text_color(p.subtle)
                                        .tooltip(|window, cx| {
                                            titlebar_tooltip(
                                                "Already sent to the agent; editing sends it again",
                                                window,
                                                cx,
                                            )
                                        })
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
                    .when(!note.display_note().is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(p.text)
                                .child(note.display_note().to_string()),
                        )
                    })
                    // The note's images, each a thumbnail and its name that opens the picture.
                    .when(!note.attachments.is_empty(), |this| {
                        this.child(
                            div().flex().flex_col().gap(px(6.0)).children(
                                note.attachments
                                    .iter()
                                    .enumerate()
                                    .map(|(slot, attachment)| {
                                        let image = super::notes::attachment_image(attachment);
                                        let open = attachment.clone();
                                        div()
                                            .id(("native-docs-note-attachment", index * 8 + slot))
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .size(px(34.0))
                                                    .rounded(px(4.0))
                                                    .overflow_hidden()
                                                    .bg(p.text.opacity(0.06))
                                                    .children(image.map(|image| {
                                                        gpui::img(image)
                                                            .size_full()
                                                            .object_fit(gpui::ObjectFit::Cover)
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_size(px(10.0))
                                                    .text_color(p.muted)
                                                    .child(attachment.name.clone()),
                                            )
                                            .on_click(move |_, _, cx| {
                                                super::notes::open_attachment(&open, cx)
                                            })
                                    }),
                            ),
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
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let anchor = NOTES_LIST_ANCHOR.with(|cell| cell.get());
                                    this.native_docs_open_composer(
                                        Some(anchor),
                                        Some(edit_id.clone()),
                                        "",
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
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                // A press on the list's own button is left to its click, which toggles it shut.
                if NOTES_LIST_ANCHOR
                    .with(|cell| cell.get())
                    .contains(&event.position)
                {
                    return;
                }
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

impl GhostexGpuiApp {
    /// The card over the note the caret sits in (nothing selected, no composer open): its type,
    /// its image count, a two-line preview and a remove button (`ManageAnnotationPreviewCard`).
    pub(crate) fn render_native_docs_note_preview(
        &mut self,
        p: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.native_docs.composer.is_some() || self.native_docs.notes_list_open {
            return None;
        }
        let notes = self.native_docs_active_notes();
        if notes.is_empty() {
            return None;
        }
        let document = self.native_docs.active_document()?;
        let live = document.live.as_ref()?;
        let editor = live.read(cx);
        if !gpui::Focusable::focus_handle(editor, cx).is_focused(window)
            || !editor.selected_range().is_empty()
        {
            return None;
        }
        let text = editor.text();
        let found = super::annotations::annotation_range_at(
            text,
            &self.native_docs.note_ranges,
            editor.cursor().min(text.len()),
        )?;
        let note = notes.get(found.annotation_index)?.clone();
        let start = editor.bounds_for_offset(found.range.start)?;
        let end = editor.bounds_for_offset(found.range.end).unwrap_or(start);
        let anchor_x = if (end.top() - start.top()).abs() < px(1.0) {
            (f32::from(start.left()) + f32::from(end.left())) / 2.0
        } else {
            f32::from(start.left())
        };
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 24.0).clamp(240.0, 320.0);
        let half = width / 2.0;
        let center = anchor_x.clamp(
            12.0 + half,
            (f32::from(viewport.width) - 12.0 - half).max(12.0 + half),
        );
        let top = (f32::from(start.top()) - 96.0).max(12.0);
        let color = hex(note.color(), 1.0);
        let images = note.attachments.len();
        let remove_id = note.id.clone();
        let card = div()
            .id("native-docs-note-preview")
            .relative()
            .w(px(width))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .pt(px(10.0))
            .pb(px(10.0))
            .pl(px(12.0))
            .pr(px(36.0))
            .rounded(px(8.0))
            .bg(p.raised)
            .border_1()
            .border_color(color.opacity(0.28))
            .shadow_lg()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(8.0))
                    .bg(color.opacity(0.04)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .line_height(px(11.0))
                    .child(
                        div()
                            .text_color(color)
                            .child(note.type_label().to_uppercase()),
                    )
                    .when(images > 0, |this| {
                        this.child(div().text_color(p.muted).child(format!(
                            "{images} {}",
                            if images == 1 { "image" } else { "images" }
                        )))
                    }),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(12.0 * 1.4))
                    .max_h(px(12.0 * 1.4 * 2.0))
                    .overflow_hidden()
                    .text_color(p.text.opacity(0.9))
                    .child(note.preview_text()),
            )
            .child(
                div()
                    .id("native-docs-note-preview-remove")
                    .absolute()
                    .top(px(7.0))
                    .right(px(7.0))
                    .size(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0))
                    .cursor_pointer()
                    .child(titlebar_svg_icon(
                        "titlebar/x.svg",
                        14.0,
                        color.opacity(0.7),
                    ))
                    .tooltip(|window, cx| titlebar_tooltip("Remove annotation", window, cx))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_remove_note(&remove_id, cx);
                    })),
            );
        Some(
            deferred(
                anchored()
                    .position(point(px(center - half), px(top)))
                    .child(card),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
