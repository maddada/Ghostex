//! The document side of Docs: its header and the open file, as an editor, formatted, or a picture.

use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::input::Editor;
use gpui_component::scroll::ScrollbarMode;

use super::files_list::{ROW_STRIP_HEIGHT, header_icon, header_tile};
use super::palette::DocsPalette;
use super::picture_zoom::DocsPictureHost as _;
use super::sidebar::DocsSidebarLayout;
use super::state::{DocsDocumentLoad, DocsFileKind, DocsMarkdownMode};
use crate::GhostexGpuiApp;
use crate::app::consts::{WORKAREA_HEADER_EDGE_PADDING, WORKAREA_HEADER_TITLE_TEXT_SIZE};
use crate::app::helpers::{titlebar_svg_icon, titlebar_tooltip};

thread_local! {
    /// The document header's bounds, so the selection toolbar flips below the text instead of
    /// covering the header.
    pub(crate) static HEADER_BOUNDS: std::cell::Cell<gpui::Bounds<gpui::Pixels>> =
        std::cell::Cell::new(gpui::Bounds::default());
}

/// How long "Saved" and "Copied!" stay up.
const FLASH: Duration = Duration::from_millis(1600);

impl GhostexGpuiApp {
    pub(crate) fn render_native_docs_document(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self.render_native_docs_header(p, layout, cx);
        let notice = self.render_native_docs_disk_notice(p, cx);
        let body = match self.render_native_docs_disk_compare(cx) {
            Some(compare) => compare,
            None => self.render_native_docs_body(p, window, cx),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(header)
            .children(notice)
            // The body fills exactly the space under the header. A percentage height inside the
            // flexed row resolved against the whole column instead, which pushed the body's last
            // 37px (the formatting bar and the document's last lines) below the view.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(div().absolute().inset_0().child(body)),
            )
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
        let title = document.display_path.clone();
        let dirty = document.dirty;
        let copied = document.title_copied_until.is_some_and(|until| until > now);
        let saved = document.saved_flash_until.is_some_and(|until| until > now);
        let kind = document.kind;
        let path = document.path.clone();
        let review = document.review_session_title.clone();
        let deleted = document.disk == super::state::DocsDiskState::Deleted;
        let wraps =
            document.editor.is_some() && kind != DocsFileKind::Markdown && !document.data_view_open;
        let word_wrap = self.native_docs.word_wrap;
        let html_annotate = document.html_annotate;
        let svg = kind == DocsFileKind::Image && DocsFileKind::is_svg(&path);
        let svg_source = document.svg_source;
        let media = kind.is_viewer();
        let published_page = kind == DocsFileKind::Page;
        let page_url = published_page
            .then(|| document.page.as_ref().map(|page| page.url.clone()))
            .flatten();
        // A Ghostex page file shows its page, which has no annotation tool.
        let page_file = kind == DocsFileKind::Html && document.page.is_some();
        // CDXC:Docs 2026-10-01 WHY: the Show files button is the header row's last control, not a corner overlay drawn over the row's right end; a guessed reserve for the overlay let it cover the actions (the Open With arrow and Reload) whenever the row and the overlay disagreed.
        // CDXC:Docs 2026-10-09 DECISION: User: "when i hover the button to show files list pls keep the bar behind to not change". The button stays in the row while the list floats over it (a peek or a drawer) and goes only once the list is docked, so opening the floating list never re-lays out the bar: the title, the meta strip and the other buttons stay exactly where they were.
        let restore = (!layout.docked).then(|| self.render_native_docs_restore_button(p, cx));
        // CDXC:Docs 2026-10-08 DECISION:
        // User: "when i click here please lets copy the whole path not the file name". Clicking the top bar's file name copies the file's full path on its computer (the project folder joined with a project-relative path), supersedes the 2026-09-07 rule of copying the displayed name.
        let copy_title: SharedString = if let Some(page_url) = page_url {
            // A published page has no file; its title copies the page's address.
            page_url
        } else {
            let file = std::path::Path::new(path.as_str());
            if file.is_absolute() {
                path.to_string()
            } else {
                self.native_docs
                    .project
                    .as_ref()
                    .map(|project| {
                        project
                            .project_path
                            .join(file)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_else(|| path.to_string())
            }
        }
        .into();
        let tooltip: SharedString = if copied {
            "Copied!".into()
        } else if dirty {
            if cfg!(target_os = "macos") {
                "Unsaved changes. Press ⌘S to save. Click to copy the full path"
            } else {
                "Unsaved changes. Press Ctrl+S to save. Click to copy the full path"
            }
            .into()
        } else if published_page {
            "Copy page address".into()
        } else {
            "Copy full path".into()
        };
        let note_actions = if kind == DocsFileKind::Markdown {
            let compact = super::render::view_width() < 560.0;
            self.render_native_docs_note_actions(p, compact, cx)
        } else {
            Vec::new()
        };
        let data_tiles = self.render_native_docs_data_tiles(p, cx);
        let picture_tiles = self.render_native_docs_picture_tiles(p, cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(9.0))
            .h(px(ROW_STRIP_HEIGHT))
            .pl(px(13.0))
            .pr(px(WORKAREA_HEADER_EDGE_PADDING))
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
                                "files-view/t-edit-175.svg"
                            } else if published_page {
                                "files-view/t-file-type-html-175.svg"
                            } else if media {
                                super::files_list::file_icon(&path)
                            } else {
                                "files-view/t-file-text-175.svg"
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
                            .text_size(px(WORKAREA_HEADER_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.text)
                            // VS Code strikes through a file deleted on disk.
                            .when(deleted, |title| title.line_through().text_color(p.muted))
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
                // CDXC:Docs 2026-10-15 DECISION: User: "we're showing the file name twice here. and no need for file size or TOML there". The header row is the only place the file's path shows (the code view no longer repeats it above the text), and the meta strip carries no file type or size, only an agent reply's label and the Edited/Saved state.
                // The meta strip gives way before the actions do, so a narrow view never pushes
                // a button out of the row.
                div()
                    .flex()
                    .flex_shrink(1.0)
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .items_center()
                    .gap(px(9.0))
                    .text_size(px(WORKAREA_HEADER_TITLE_TEXT_SIZE))
                    .text_color(p.subtle)
                    .when(review.is_some(), |this| this.child("Agent reply"))
                    .when(published_page, |this| this.child("Ghostex page"))
                    .children(review.clone().filter(|title| !title.is_empty()))
                    .when(deleted, |this| this.child("Deleted"))
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
                    .children(data_tiles)
                    .children(picture_tiles)
                    .when(kind.opens_externally() && review.is_none(), |this| {
                        let (open_path, menu_path) = (path.clone(), path.clone());
                        this.child(
                            header_tile(
                                "native-docs-open-externally",
                                header_icon("titlebar/external-link.svg", false, p),
                                false,
                                false,
                                p,
                            )
                            .tooltip(|window, cx| titlebar_tooltip("Open in browser", window, cx))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.native_docs_open_externally(&open_path, None, cx);
                                },
                            )),
                        )
                        .child(
                            header_tile(
                                "native-docs-open-with",
                                titlebar_svg_icon(
                                    "titlebar/chevron-down.svg",
                                    12.0,
                                    p.toolbar_icon,
                                ),
                                false,
                                false,
                                p,
                            )
                            .w(px(16.0))
                            .child(
                                gpui::canvas(
                                    |bounds, _, _| {
                                        super::open_externally::OPEN_WITH_MENU_ANCHOR
                                            .with(|cell| cell.set(bounds))
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .tooltip(|window, cx| titlebar_tooltip("Open with", window, cx))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.show_native_docs_open_with_menu(&menu_path, window, cx);
                                },
                            )),
                        )
                    })
                    .when(kind == DocsFileKind::Html && !page_file, |this| {
                        let annotate = html_annotate;
                        let toggle_path = path.clone();
                        this.child(
                            header_tile(
                                "native-docs-html-annotate",
                                header_icon("files-view/t-message-plus-2.svg", false, p),
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
                    .when(svg, |this| {
                        let toggle_path = path.clone();
                        this.child(
                            header_tile(
                                "native-docs-svg-source",
                                header_icon(
                                    if svg_source {
                                        "titlebar/photo.svg"
                                    } else {
                                        "titlebar/code.svg"
                                    },
                                    false,
                                    p,
                                ),
                                false,
                                false,
                                p,
                            )
                            .tooltip(move |window, cx| {
                                titlebar_tooltip(
                                    if svg_source {
                                        "Show picture"
                                    } else {
                                        "Edit source"
                                    },
                                    window,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.native_docs_toggle_svg_source(&toggle_path, cx);
                                },
                            )),
                        )
                    })
                    .when(media && review.is_none(), |this| {
                        let open_path = path.clone();
                        this.child(
                            header_tile(
                                "native-docs-open-system-app",
                                header_icon("titlebar/external-link.svg", false, p),
                                false,
                                false,
                                p,
                            )
                            .tooltip(|window, cx| {
                                titlebar_tooltip("Open in system app", window, cx)
                            })
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.native_docs_open_with_system_app(&open_path, cx);
                                },
                            )),
                        )
                    })
                    // CDXC:SessionChat 2026-10-24 WHY: a page an agent published (Settings > Chat > Open Ghostex pages in: Files) has no file, so its header offers what the floating page window's does: open it in the browser, and close it.
                    .when(published_page, |this| {
                        let close_path = path.clone();
                        this.child(
                            header_tile(
                                "native-docs-page-open-in-browser",
                                header_icon("titlebar/external-link.svg", false, p),
                                false,
                                false,
                                p,
                            )
                            .tooltip(|window, cx| titlebar_tooltip("Open in browser", window, cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.native_docs_open_page_in_browser(cx);
                            })),
                        )
                        .child(
                            header_tile(
                                "native-docs-close-page",
                                header_icon("titlebar/x.svg", false, p),
                                false,
                                false,
                                p,
                            )
                            .tooltip(|window, cx| titlebar_tooltip("Close page", window, cx))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.native_docs_close(&close_path, cx);
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
                    // CDXC:Docs 2026-10-15 DECISION: User: "add button to toggle wrapping on the file editor please (should be to the right of the refresh button that we have now)". The Reload button is gone (disk_sync.rs), so the wrap toggle takes its place for text and code files; the choice is remembered for every file.
                    .when(wraps, |this| {
                        this.child(
                            header_tile(
                                "native-docs-word-wrap",
                                header_icon("modals/git-commit/text-wrap.svg", false, p),
                                word_wrap,
                                false,
                                p,
                            )
                            .tooltip(move |window, cx| {
                                titlebar_tooltip(
                                    if word_wrap {
                                        "Turn off line wrapping"
                                    } else {
                                        "Wrap long lines"
                                    },
                                    window,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.native_docs_toggle_word_wrap(window, cx);
                                },
                            )),
                        )
                    })
                    .children(restore),
            )
            .into_any_element()
    }

    fn render_native_docs_body(
        &mut self,
        p: &DocsPalette,
        window: &mut Window,
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
                .text_size(px(WORKAREA_HEADER_TITLE_TEXT_SIZE))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.muted)
                .child(titlebar_svg_icon(icon, 16.0, p.muted))
                .child(text)
                .into_any_element()
        };
        let format_bar = self.render_native_docs_format_bar(p, cx);
        // CSV's table and JSON's tree stand in for the text while their switch is on.
        if let Some(view) = self.render_native_docs_data_view(p, cx) {
            return view;
        }
        let Some(document) = self.native_docs.active_document() else {
            return notice("files-view/t-file-175.svg", "Select a file".to_string());
        };
        let action_button = |id: &'static str, icon: &'static str, label: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(28.0))
                .px(px(12.0))
                .rounded(px(7.0))
                .border_1()
                .border_color(p.border_strong)
                .cursor_pointer()
                .text_color(p.text)
                .hover(|style| style.bg(p.control_hover))
                .child(titlebar_svg_icon(icon, 14.0, p.text))
                .child(label)
        };
        let open_externally = |path: String, reason: String, cx: &mut Context<Self>| {
            div()
                .size_full()
                .min_h(px(140.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.0))
                .text_size(px(WORKAREA_HEADER_TITLE_TEXT_SIZE))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.muted)
                .child(reason)
                .child(
                    action_button(
                        "native-docs-open-system-app-button",
                        "titlebar/external-link.svg",
                        "Open in system app",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_open_with_system_app(&path, cx);
                    })),
                )
                .into_any_element()
        };
        match &document.load {
            DocsDocumentLoad::Loading => {
                return notice("files-view/t-refresh-2.svg", "Loading file".to_string());
            }
            DocsDocumentLoad::Error(error) => {
                return notice("titlebar/alert-triangle.svg", error.clone());
            }
            DocsDocumentLoad::Unsupported(reason) if document.kind == DocsFileKind::Markdown => {
                // Only Markdown has a size limit (CDXC:Docs 2026-09-28 in
                // helpers/os_cli/process_and_constants.rs); past it the file opens in the Code view.
                let path = document.path.clone();
                return div()
                    .size_full()
                    .min_h(px(140.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.0))
                    .text_size(px(WORKAREA_HEADER_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.muted)
                    .child(reason.clone())
                    .child(
                        action_button(
                            "native-docs-open-code-view-button",
                            crate::app::consts::TITLEBAR_ICON_CODE,
                            "Open in VS Code view",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.native_docs_open_in_code_view(&path, cx);
                        })),
                    )
                    .into_any_element();
            }
            DocsDocumentLoad::Unsupported(reason) => {
                let (path, reason) = (document.path.to_string(), reason.clone());
                return open_externally(path, reason, cx);
            }
            DocsDocumentLoad::Ready => {}
        }
        match document.kind {
            DocsFileKind::SystemApp => {
                let path = document.path.to_string();
                return open_externally(path, "This file opens in its own app.".to_string(), cx);
            }
            DocsFileKind::Image if document.svg_source => {}
            DocsFileKind::Image => {
                let Some(image) = document.image.clone() else {
                    return notice("files-view/t-refresh-2.svg", "Loading file".to_string());
                };
                let path = document.path.clone();
                return self.render_native_docs_picture(&path, image, cx);
            }
            DocsFileKind::Html
            | DocsFileKind::Excalidraw
            | DocsFileKind::Video
            | DocsFileKind::Audio
            | DocsFileKind::Pdf
            | DocsFileKind::BrowserImage
            | DocsFileKind::Font
            | DocsFileKind::Page => {
                // The web runtime prompt (app/render/web_runtime_prompt.rs) with this file's Open
                // in system app beside Install (a published page has no file to open).
                let published_page = document.kind == DocsFileKind::Page;
                if let Some(prompt) = crate::app::helpers::web_runtime::web_runtime_install_prompt(
                    if published_page {
                        "This page"
                    } else {
                        "This file"
                    },
                ) {
                    let path = (!published_page).then(|| document.path.to_string());
                    return self.render_web_runtime_prompt_card(
                        "native-docs-web-runtime-prompt",
                        prompt,
                        path,
                        cx,
                    );
                }
                if self.native_docs_browser_area_covered() {
                    return div().size_full().into_any_element();
                }
                return self.render_native_docs_browser_area(cx).unwrap_or_else(|| {
                    notice("files-view/t-refresh-2.svg", "Loading file".to_string())
                });
            }
            DocsFileKind::Markdown => {
                let Some(live) = document.live.clone() else {
                    return notice("files-view/t-refresh-2.svg", "Loading file".to_string());
                };
                let table_actions = {
                    let app = cx.weak_entity();
                    super::table_tools::TableActionHost {
                        copy: std::rc::Rc::new(|text, cx| {
                            crate::app::helpers::gpui_copy_to_clipboard(
                                gpui::ClipboardItem::new_string(text),
                                cx,
                            );
                        }),
                        open: Some(std::rc::Rc::new(move |source, cx| {
                            let message = serde_json::json!({ "source": source });
                            let _ = app.update(cx, |app, cx| {
                                app.open_gpui_markdown_table_modal(&message, cx);
                            });
                        })),
                    }
                };
                let body = super::markdown_body::render_markdown_body(
                    super::markdown_body::DocsMarkdownBody {
                        id: SharedString::from(format!("native-docs-scroll-{}", document.path)),
                        live: &live,
                        scroll: &document.scroll,
                        source: document.mode == DocsMarkdownMode::Source,
                        line_numbers: self.native_docs.line_numbers,
                        constrain: self.native_docs.constrain_width,
                        changes: self
                            .native_docs
                            .git_changes
                            .then_some(document.changes.as_ref())
                            .flatten(),
                        table_actions: &table_actions,
                        sliding: crate::terminal_element::grid_resize_held(),
                    },
                    p,
                    window,
                    cx,
                )
                .capture_key_down(cx.listener(Self::native_docs_selection_key));
                let ruler = super::markdown_body::render_overview_ruler(
                    &live,
                    &document.scroll,
                    self.native_docs
                        .git_changes
                        .then_some(document.changes.as_ref())
                        .flatten(),
                    p,
                    cx,
                );
                return div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(body)
                    .children(ruler)
                    .child(super::markdown_body::render_body_scrollbar(
                        &document.scroll,
                    ))
                    .children(format_bar)
                    .into_any_element();
            }
            DocsFileKind::Sqlite => {
                let path = document.path.clone();
                return self.render_native_docs_sqlite(&path, p, window, cx);
            }
            DocsFileKind::Text => {}
        }
        let Some(editor) = document.editor.clone() else {
            return notice("files-view/t-refresh-2.svg", "Loading file".to_string());
        };
        let read_only = document.read_only;
        let find = self.render_native_docs_code_find(p, cx);
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(
                // CDXC:Docs 2026-10-15 DECISION: User: "need to adjust it so that the number lines are touching the top of the view and the left side of the view". The code editor fills the body with no padding around it: the gutter meets the view's left edge, line 1 starts right under the header and the current-line band runs edge to edge. Markdown keeps its reading column.
                div()
                    .flex_1()
                    .min_h_0()
                    .font_family(p.mono_font.clone())
                    .text_size(px(12.0))
                    .child(
                        // `Editor` brings the theme's code font, size and row height; the text
                        // view keeps the Docs mono font at the input text size and rows.
                        Editor::new(&editor)
                            .readonly(read_only)
                            .scrollbar_show(ScrollbarMode::Always)
                            .appearance(false)
                            // Under glass the Kit theme's opaque gutter and current-line
                            // fills would be solid blocks on the see-through page.
                            .when(p.glass, |editor| {
                                editor
                                    .gutter_background(gpui::transparent_black())
                                    .active_line_background(p.row_hover)
                            })
                            .h_full()
                            // The Kit pads the editor's top by its input size; the line-numbers
                            // gutter keeps its own few pixels on the left.
                            .pt(px(0.))
                            .font_family(p.mono_font.clone())
                            // The input text size and rows (14px on 20px), scaled by the Files
                            // zoom (`text_zoom.rs`).
                            .text_size(px(crate::ui_fonts::files_text_size()))
                            .line_height(px(20.0 * crate::ui_fonts::files_zoom_scale())),
                    ),
            )
            .children(find)
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
