//! The document side of Docs: its header and the open file, as an editor, formatted, or a picture.

use std::time::{Duration, Instant};

use gpui::StyledImage as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::input::Input;

use super::files_list::{ROW_STRIP_HEIGHT, header_icon, header_tile};
use super::palette::DocsPalette;
use super::sidebar::DocsSidebarLayout;
use super::state::{DocsDocumentLoad, DocsFileKind, DocsMarkdownMode};
use crate::GhostexGpuiApp;
use crate::app::consts::WORKAREA_HEADER_EDGE_PADDING;
use crate::app::helpers::{titlebar_svg_icon, titlebar_tooltip};

/// `MANAGE_MEO_CONTENT_MAX_WIDTH`: the document's text column.
const CONTENT_MAX_WIDTH: f32 = 800.0;
thread_local! {
    /// The document header's bounds, so the selection toolbar flips below the text instead of
    /// covering the header.
    pub(crate) static HEADER_BOUNDS: std::cell::Cell<gpui::Bounds<gpui::Pixels>> =
        std::cell::Cell::new(gpui::Bounds::default());
}

/// How long "Saved" and "Copied!" stay up.
const FLASH: Duration = Duration::from_millis(1600);

/// `formatFileSize`.
fn format_file_size(size: u64) -> String {
    if size < 1024 {
        return format!("{size} B");
    }
    let units = ["KB", "MB", "GB"];
    let mut value = size as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 10.0 {
        format!("{value:.0} {}", units[unit])
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

/// `languageLabelForPath`.
fn language_label(path: &str) -> String {
    let Some((_, extension)) = path.rsplit_once('.') else {
        return "Text".to_string();
    };
    let extension = extension.to_lowercase();
    match extension.as_str() {
        "css" => "CSS",
        "excalidraw" => "Excalidraw",
        "go" => "Go",
        "h" => "C/C++",
        "html" => "HTML",
        "js" | "mjs" => "JavaScript",
        "json" => "JSON",
        "jsx" | "tsx" => "React",
        "md" => "Markdown",
        "py" => "Python",
        "rs" => "Rust",
        "sh" => "Shell",
        "swift" => "Swift",
        "ts" => "TypeScript",
        "txt" => "Text",
        "yaml" | "yml" => "YAML",
        "zig" => "Zig",
        _ => return extension.to_uppercase(),
    }
    .to_string()
}

impl GhostexGpuiApp {
    pub(crate) fn render_native_docs_document(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self.render_native_docs_header(p, layout, cx);
        let body = self.render_native_docs_body(p, window, cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(header)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The document header: the title (which copies itself), the meta strip and the actions.
    ///
    /// CDXC:Docs 2026-09-21 DECISION:
    /// User: make the buttons on the top right of the Docs view match the look, gap, and right-edge alignment of the native view tab strip buttons above them: 32px by 27px tiles, 7px radius, 2px gap, 18px stroke-2 icons, and a 9px right inset.
    fn render_native_docs_header(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(document) = self.native_docs.active_document() else {
            return div()
                .flex_none()
                .h(px(ROW_STRIP_HEIGHT))
                .border_b_1()
                .border_color(p.border)
                .bg(p.chrome)
                .into_any_element();
        };
        let now = Instant::now();
        let title: SharedString = document.display_path.clone().into();
        let dirty = document.dirty;
        let copied = document.title_copied_until.is_some_and(|until| until > now);
        let saved = document.saved_flash_until.is_some_and(|until| until > now);
        let kind = document.kind;
        let path = document.path.clone();
        let review = document.review_session_title.clone();
        let language = if review.is_some() {
            "Agent reply".to_string()
        } else {
            language_label(&document.path)
        };
        let external_change = document.external_change;
        let size = document.size.map(format_file_size);
        let html_annotate = document.html_annotate;
        // The corner restore button sits over the header's right end while the list is not
        // docked, so the actions keep clear of it.
        let reserve = if layout.docked {
            WORKAREA_HEADER_EDGE_PADDING
        } else {
            WORKAREA_HEADER_EDGE_PADDING + 32.0 + 13.0
        };
        let copy_title = title.clone();
        let tooltip: SharedString = if copied {
            "Copied!".into()
        } else if dirty {
            "Unsaved changes. Press ⌘S to save. Click to copy file name".into()
        } else {
            "Copy file name".into()
        };
        let reload_path = path.clone();
        let note_actions = if kind == DocsFileKind::Markdown {
            let compact = super::render::view_width() < 560.0;
            self.render_native_docs_note_actions(p, compact, cx)
        } else {
            Vec::new()
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(9.0))
            .h(px(ROW_STRIP_HEIGHT))
            .pl(px(13.0))
            .pr(px(reserve))
            .border_b_1()
            .border_color(p.border)
            .bg(p.chrome)
            .child(
                gpui::canvas(
                    |bounds, _, _| HEADER_BOUNDS.with(|cell| cell.set(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                // CDXC:Docs 2026-09-07 DECISION:
                // User: clicking the file name in the Docs top bar copies it and shows a tooltip. Copy the displayed name or path, including the readable name of mounted folders.
                //
                // CDXC:Docs 2026-09-15 DECISION:
                // User: unsaved changes are shown by the file icon in the top bar, which becomes a filled dot until the file is saved. Long titles truncate from the start so the buttons on the right stay on screen.
                div()
                    .id("native-docs-title")
                    .flex()
                    .flex_shrink(1.0)
                    .min_w_0()
                    .items_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .child(if dirty {
                        div()
                            .flex_none()
                            .size(px(15.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().size(px(9.0)).rounded_full().bg(p.text))
                            .into_any_element()
                    } else {
                        titlebar_svg_icon(
                            if kind == DocsFileKind::Excalidraw {
                                "docs/t-edit-175.svg"
                            } else {
                                "docs/t-file-text-175.svg"
                            },
                            15.0,
                            p.muted,
                        )
                        .into_any_element()
                    })
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_start()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.text)
                            .child(title),
                    )
                    .tooltip(move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_title.to_string()));
                        crate::app::helpers::gpui_copy_feedback(cx);
                        if let Some(active) = this.native_docs.active.clone()
                            && let Some(document) = this.native_docs.document_mut(&active)
                        {
                            document.title_copied_until = Some(Instant::now() + FLASH);
                        }
                        this.native_docs_notify_after(FLASH, cx);
                        this.native_docs_notify(cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(9.0))
                    .text_size(px(10.5))
                    .text_color(p.subtle)
                    .child(language)
                    .children(review.clone().filter(|title| !title.is_empty()))
                    .children(size)
                    .when(dirty, |this| this.child("Edited"))
                    .when(!dirty && saved, |this| this.child("Saved")),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(2.0))
                    .children(note_actions)
                    .when(kind == DocsFileKind::Html, |this| {
                        let annotate = html_annotate;
                        let toggle_path = path.clone();
                        this.child(
                            header_tile(
                                "native-docs-html-annotate",
                                header_icon("docs/t-message-plus-2.svg", false, p),
                                annotate,
                                false,
                                p,
                            )
                            .tooltip(move |window, cx| {
                                titlebar_tooltip(
                                    if annotate {
                                        "Disable annotations"
                                    } else {
                                        "Enable annotations"
                                    },
                                    window,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(document) =
                                        this.native_docs.document_mut(&toggle_path)
                                    {
                                        document.html_annotate = !document.html_annotate;
                                    }
                                    this.native_docs_notify(cx);
                                },
                            )),
                        )
                    })
                    .when(review.is_some(), |this| {
                        this.child(
                            header_tile(
                                "native-docs-close-review",
                                header_icon("titlebar/x.svg", false, p),
                                false,
                                false,
                                p,
                            )
                            .tooltip(|window, cx| {
                                titlebar_tooltip("Close reply review", window, cx)
                            })
                            .on_click(
                                cx.listener(|this, _, _, cx| this.native_docs_drop_reviews(cx)),
                            ),
                        )
                    })
                    .when(review.is_none() && kind != DocsFileKind::Image, |this| {
                        let amber = p.amber;
                        this.child(
                            header_tile(
                                "native-docs-reload",
                                header_icon("docs/t-refresh-2.svg", false, p),
                                false,
                                false,
                                p,
                            )
                            .when(external_change, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .top(px(6.0))
                                        .right(px(6.0))
                                        .size(px(7.0))
                                        .rounded_full()
                                        .bg(amber),
                                )
                            })
                            .tooltip(move |window, cx| {
                                titlebar_tooltip(
                                    if external_change {
                                        "Reload to show new changes"
                                    } else {
                                        "Reload file"
                                    },
                                    window,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.native_docs_reload(&reload_path, cx);
                                },
                            )),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_native_docs_body(
        &mut self,
        p: &DocsPalette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let notice = |icon: &'static str, text: String| {
            div()
                .size_full()
                .min_h(px(140.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.muted)
                .child(titlebar_svg_icon(icon, 16.0, p.muted))
                .child(text)
                .into_any_element()
        };
        let format_bar = self.render_native_docs_format_bar(p, cx);
        let Some(document) = self.native_docs.active_document() else {
            return notice("docs/t-file-175.svg", "Select a file".to_string());
        };
        match &document.load {
            DocsDocumentLoad::Loading => {
                return notice("docs/t-refresh-2.svg", "Loading file".to_string());
            }
            DocsDocumentLoad::Error(error) => {
                return notice("titlebar/alert-triangle.svg", error.clone());
            }
            DocsDocumentLoad::Ready => {}
        }
        match document.kind {
            DocsFileKind::Image => {
                let Some(image) = document.image.clone() else {
                    return notice("docs/t-refresh-2.svg", "Loading file".to_string());
                };
                return div()
                    .id("native-docs-image")
                    .size_full()
                    .overflow_scroll()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(24.0))
                    .child(
                        gpui::img(image)
                            .max_w_full()
                            .max_h_full()
                            .object_fit(gpui::ObjectFit::Contain),
                    )
                    .into_any_element();
            }
            DocsFileKind::Html | DocsFileKind::Excalidraw => {
                if self.native_docs_browser_area_covered() {
                    return div().size_full().into_any_element();
                }
                return self
                    .render_native_docs_browser_area(cx)
                    .unwrap_or_else(|| notice("docs/t-refresh-2.svg", "Loading file".to_string()));
            }
            DocsFileKind::Markdown => {
                let Some(live) = document.live.clone() else {
                    return notice("docs/t-refresh-2.svg", "Loading file".to_string());
                };
                let body = super::editor_style::body_color(p);
                let state = live.read(cx);
                let rows = state.row_layout();
                let text = state.text();
                let caret = state.cursor().min(text.len());
                let caret_line = text[..caret].bytes().filter(|byte| *byte == b'\n').count();
                let source = document.mode == DocsMarkdownMode::Source;
                // The viewport in the gutter's y (it starts below the column's 14px top pad), one
                // screen of margin each side.
                let band = {
                    let viewport = document.scroll.bounds().size.height;
                    let top = -document.scroll.offset().y - px(14.0);
                    (viewport > px(0.0)).then(|| (top - viewport, top + viewport * 2.0))
                };
                let gutter = super::gutter::render(
                    super::gutter::GutterModel {
                        rows: &rows,
                        caret_line,
                        numbers: self.native_docs.line_numbers,
                        changes: self
                            .native_docs
                            .git_changes
                            .then_some(document.changes.as_ref())
                            .flatten(),
                        band,
                    },
                    p,
                    body,
                );
                let constrain = self.native_docs.constrain_width;
                let focus_target = live.clone();
                let scroll = document.scroll.clone();
                return div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "native-docs-scroll-{}",
                                document.path
                            )))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                focus_target.update(cx, |editor, cx| editor.focus(window, cx));
                            })
                            .capture_key_down(cx.listener(Self::native_docs_selection_key))
                            .child(
                                div().w_full().flex().justify_center().child(
                                    div()
                                        .w_full()
                                        .when(constrain, |row| {
                                            row.max_w(px(CONTENT_MAX_WIDTH + 51.0))
                                        })
                                        .pt(px(14.0))
                                        .pb(px(72.0 + 60.0))
                                        .pr(px(12.0))
                                        .flex()
                                        .items_start()
                                        .child(gutter)
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_size(px(14.0))
                                                .text_color(body)
                                                .font_family(if source {
                                                    super::fonts::DOCS_MONO
                                                } else {
                                                    super::fonts::DOCS_FONT
                                                })
                                                .child(live),
                                        ),
                                ),
                            ),
                    )
                    .children(format_bar)
                    .into_any_element();
            }
            DocsFileKind::Text => {}
        }
        let Some(editor) = document.editor.clone() else {
            return notice("docs/t-refresh-2.svg", "Loading file".to_string());
        };
        let path_row: SharedString = document.display_path.clone().into();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(18.0))
                    .py(px(8.0))
                    .text_size(px(11.0))
                    .font_family(p.mono_font.clone())
                    .text_color(p.subtle)
                    .child(path_row),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .pt(px(16.0))
                    .px(px(18.0))
                    .pb(px(28.0))
                    .font_family(p.mono_font.clone())
                    .text_size(px(12.0))
                    .child(Input::new(&editor).appearance(false).h_full()),
            )
            .into_any_element()
    }

    /// Redraws once `after` has passed, to take down "Saved" and "Copied!".
    pub(crate) fn native_docs_notify_after(&mut self, after: Duration, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(after).await;
            let _ = this.update(cx, |this, cx| this.native_docs_notify(cx));
        })
        .detach();
    }
}
