//! The files list: its header buttons, Search, Open Files and the Project Docs tree, drawn
//! natively with the Docs page's metrics (`apps/desktop/views/manage/styles.ts`).

use std::cell::Cell;
use std::collections::HashSet;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _,
    Transformation, Window, div, px, radians, svg,
};
use gpui_component::Sizable as _;
use gpui_component::input::Input;

use super::files::parent_path;
use super::palette::DocsPalette;
use super::render::SIDEBAR_WIDTH;
use super::sidebar::DocsSidebarLayout;
use super::state::{DocsEntryKind, DocsFileKind, DocsLoadState, DocsTransient};
use crate::GhostexGpuiApp;
use crate::app::consts::{
    TITLEBAR_BUTTON_RADIUS, TITLEBAR_CONTROL_HEIGHT, TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
    WORKAREA_HEADER_EDGE_PADDING,
};
use crate::app::helpers::{titlebar_svg_icon, titlebar_tooltip};

/// The header row, the search row and the document header share this height.
pub(crate) const ROW_STRIP_HEIGHT: f32 = 35.0;
const HEADER_BUTTON_WIDTH: f32 = 32.0;
const HEADER_BUTTON_GAP: f32 = 2.0;
const OPEN_FILE_ROW_HEIGHT: f32 = 28.0;
const TREE_ROW_HEIGHT: f32 = 34.0;
const TREE_INDENT: f32 = 18.0;
/// `.manage-file-name`: 15.55px at weight 300.
const NAME_SIZE: f32 = 15.55;

thread_local! {
    /// Where the header's menu buttons are, so their menus open below them.
    pub(crate) static CREATE_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
    pub(crate) static OVERFLOW_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

/// The icon a file row shows (`manageFileIconForPath`).
pub(crate) fn file_icon(path: &str) -> &'static str {
    match DocsFileKind::for_path(path) {
        DocsFileKind::Markdown => "docs/t-markdown-175.svg",
        DocsFileKind::Html => "docs/t-file-type-html-175.svg",
        DocsFileKind::Excalidraw => "docs/t-edit-175.svg",
        DocsFileKind::Image => "titlebar/photo.svg",
        DocsFileKind::Text => "docs/t-file-175.svg",
    }
}

fn anchor(cell: &'static std::thread::LocalKey<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    gpui::canvas(
        move |bounds, _, _| cell.with(|c| c.set(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// A 32x27 header tile with an 18px icon, the view tab strip's panel-toggle metrics.
pub(crate) fn header_tile(
    id: impl Into<gpui::ElementId>,
    icon: impl IntoElement,
    active: bool,
    disabled: bool,
    p: &DocsPalette,
) -> gpui::Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(HEADER_BUTTON_WIDTH))
        .h(px(TITLEBAR_CONTROL_HEIGHT))
        .rounded(px(TITLEBAR_BUTTON_RADIUS))
        .when(active, |this| this.bg(hover))
        .when(!disabled, |this| {
            this.cursor_pointer().hover(move |style| style.bg(hover))
        })
        .child(icon)
}

pub(crate) fn header_icon(path: &'static str, disabled: bool, p: &DocsPalette) -> AnyElement {
    titlebar_svg_icon(
        path,
        TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
        if disabled {
            p.toolbar_disabled
        } else {
            p.toolbar_icon
        },
    )
    .into_any_element()
}

fn section_label(label: &'static str, p: &DocsPalette) -> impl IntoElement {
    div()
        .flex_none()
        .pt(px(8.0))
        .px(px(14.0))
        .pb(px(3.0))
        .text_size(px(11.0))
        .line_height(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.subtle)
        .child(label)
}

impl GhostexGpuiApp {
    /// The files list, docked or floating.
    ///
    /// CDXC:Docs 2026-09-16 DECISION:
    /// User: the Docs files list always sits on the right; the option to switch it to the left was removed from the sidebar menu.
    ///
    /// CDXC:Docs 2026-09-05 DECISION:
    /// User: make the files list match the existing app sidebar, including its lightweight text, neutral row states, and context menu.
    pub(crate) fn render_native_docs_files_list(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        floating: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self.render_native_docs_files_header(p, layout, cx);
        let search = self.render_native_docs_search_row(p, window, cx);
        let open_files = self.render_native_docs_open_files(p, cx);
        let tree = self.render_native_docs_tree(p, window, cx);
        div()
            .id("native-docs-files")
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .min_h_0()
            .when(floating, |this| {
                this.border_1().rounded(px(
                    crate::app::floating_reveal::model::FLOATING_PANEL_CORNER_RADIUS,
                ))
            })
            .when(!floating, |this| this.border_l_1())
            .border_color(if floating { p.border } else { p.divider })
            .bg(match (floating, p.glass) {
                // The floating list's window shows the glass picture; like the floating sessions
                // sidebar it lays the sidebar's own tint over it, then the list's wash.
                (true, true) => crate::app::helpers::sidebar_glass_tint(),
                (true, false) => p.floating,
                (false, _) => p.chrome,
            })
            .when(floating && p.glass, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(
                            crate::app::floating_reveal::model::FLOATING_PANEL_CORNER_RADIUS,
                        ))
                        .bg(p.chrome),
                )
            })
            .child(header)
            .child(search)
            .children(open_files)
            .child(section_label("Project Docs", p))
            .child(tree)
            .into_any_element()
    }

    fn render_native_docs_files_header(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expandable = self.native_docs_expandable_folders();
        let any_open = expandable
            .iter()
            .any(|path| self.native_docs.expanded.contains(*path));
        let can_toggle_all = !expandable.is_empty();
        let active_listed = self.native_docs.active.as_deref().is_some_and(|active| {
            self.native_docs
                .entries
                .iter()
                .any(|entry| entry.path == active && entry.kind == DocsEntryKind::File)
        });
        let collapse_icon = svg()
            .size(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE))
            .path(if any_open {
                "docs/t-arrows-diagonal-minimize-2.svg"
            } else {
                "docs/t-arrows-diagonal-2-2.svg"
            })
            .text_color(if can_toggle_all {
                p.toolbar_icon
            } else {
                p.toolbar_disabled
            })
            .with_transformation(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2)));
        let collapse_label = if any_open {
            "Collapse All"
        } else {
            "Expand All"
        };
        let peek = self.native_docs.transient == Some(DocsTransient::Peek);
        // CDXC:Docs 2026-09-16 DECISION:
        // User: show the pin icon only when the list can actually be pinned. A narrow pane cannot dock the list, so a peek there offers the close control instead of a pin that appears to do nothing.
        let edge = (!layout.forced).then(|| {
            if layout.docked {
                (
                    "docs/t-layout-sidebar-right-collapse-2.svg",
                    "Hide files",
                    "hide",
                )
            } else if peek && !layout.narrow {
                ("docs/t-pin-2.svg", "Pin files", "pin")
            } else {
                (
                    "docs/t-layout-sidebar-right-collapse-2.svg",
                    "Close files",
                    "close",
                )
            }
        });
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .gap(px(HEADER_BUTTON_GAP))
            .h(px(ROW_STRIP_HEIGHT))
            .px(px(WORKAREA_HEADER_EDGE_PADDING))
            .border_b_1()
            .border_color(p.border)
            .child(
                header_tile(
                    "native-docs-toggle-all",
                    collapse_icon,
                    false,
                    !can_toggle_all,
                    p,
                )
                .tooltip(move |window, cx| titlebar_tooltip(collapse_label, window, cx))
                .when(can_toggle_all, |this| {
                    this.on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_toggle_all_folders(any_open, cx);
                    }))
                }),
            )
            .child(
                // CDXC:Docs 2026-09-07 DECISION:
                // User: add a button immediately left of New file that takes the sidebar to the currently open file. Clear the filter and expand its ancestors before scrolling and focusing its row.
                header_tile(
                    "native-docs-reveal-open",
                    header_icon("docs/t-current-location-2.svg", !active_listed, p),
                    false,
                    !active_listed,
                    p,
                )
                .tooltip(|window, cx| titlebar_tooltip("Reveal open file in sidebar", window, cx))
                .when(active_listed, |this| {
                    this.on_click(cx.listener(|this, _, window, cx| {
                        this.native_docs_reveal_open_file(window, cx);
                    }))
                }),
            )
            .child(
                header_tile(
                    "native-docs-create",
                    header_icon("docs/t-plus-2.svg", false, p),
                    false,
                    false,
                    p,
                )
                .child(anchor(&CREATE_MENU_ANCHOR))
                .tooltip(|window, cx| titlebar_tooltip("Create docs item", window, cx))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_native_docs_create_menu("docs", window, cx);
                })),
            )
            .child(
                header_tile(
                    "native-docs-overflow",
                    header_icon("docs/t-menu-2-2.svg", false, p),
                    false,
                    false,
                    p,
                )
                .child(anchor(&OVERFLOW_MENU_ANCHOR))
                .tooltip(|window, cx| titlebar_tooltip("Docs sidebar menu", window, cx))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_native_docs_overflow_menu(window, cx);
                })),
            )
            .when_some(edge, |this, (icon, label, kind)| {
                this.child(
                    header_tile(
                        "native-docs-edge",
                        header_icon(icon, false, p),
                        false,
                        false,
                        p,
                    )
                    .tooltip(move |window, cx| titlebar_tooltip(label, window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| match kind {
                        "hide" => this.native_docs_hide_sidebar(cx),
                        "pin" => this.native_docs_pin_peek(cx),
                        _ => this.native_docs_close_transient(cx),
                    })),
                )
            })
            .into_any_element()
    }

    /// CDXC:Docs 2026-09-06 DECISION:
    /// User: make the Docs file search bar 3px taller.
    fn render_native_docs_search_row(
        &mut self,
        p: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(search) = self.native_docs.search.clone() else {
            return div().into_any_element();
        };
        let focused = gpui::Focusable::focus_handle(search.read(cx), cx).is_focused(window);
        let has_query = !self.native_docs.search_query.is_empty();
        let focus_target = search.clone();
        div()
            .id("native-docs-search-row")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(11.0))
            .h(px(ROW_STRIP_HEIGHT))
            .px(px(10.0))
            .mb(px(4.0))
            .border_b_1()
            .border_color(p.rule)
            .when(focused, |this| this.bg(p.text.opacity(0.08)))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_target.update(cx, |input, cx| input.focus(window, cx));
            })
            .child(titlebar_svg_icon("docs/t-search-18.svg", 15.0, p.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(NAME_SIZE))
                    .font_weight(FontWeight::LIGHT)
                    .child(Input::new(&search).appearance(false)),
            )
            .when(has_query, |this| {
                this.child(
                    div()
                        .id("native-docs-search-clear")
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(20.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .child(titlebar_svg_icon(
                            "titlebar/x.svg",
                            14.0,
                            p.text.opacity(0.58),
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.native_docs_clear_search(window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: a vertical list of the currently open files sits under Search and above the tree, and is exactly as tall as the number of open files. Each row is the file's icon and name; hovering shows a close control, and an unsaved file shows a dot in its place until hovered, so the list doubles as the unsaved indicator. It only scrolls once it would take more than a third of the sidebar.
    ///
    /// CDXC:Docs 2026-09-16 DECISION:
    /// User: label the list under Search "Open Files" and the tree below it "Project Docs" so the two lists read as different things.
    fn render_native_docs_open_files(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.native_docs.documents.is_empty() {
            return None;
        }
        let active = self.native_docs.active.clone();
        let rows: Vec<AnyElement> = self
            .native_docs
            .documents
            .iter()
            .enumerate()
            .map(|(index, document)| {
                let path = document.path.clone();
                let selected = active.as_deref() == Some(document.path.as_str());
                let group: SharedString = format!("docs-open-{index}").into();
                let close_path = path.clone();
                let middle_path = path.clone();
                let dirty = document.dirty;
                let close_label = if dirty {
                    "Unsaved changes. Close file"
                } else {
                    "Close file"
                };
                let display_path: SharedString = document.display_path.clone().into();
                let hover_bg = p.row_hover;
                let strong = p.row_text_strong;
                div()
                    .id(("native-docs-open-file", index))
                    .group(group.clone())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(9.0))
                    .h(px(OPEN_FILE_ROW_HEIGHT))
                    .pl(px(14.0))
                    .pr(px(7.0))
                    .cursor_pointer()
                    .text_color(if selected {
                        p.row_text_strong
                    } else {
                        p.row_text
                    })
                    .when(selected, |this| this.bg(p.row_surface))
                    .when(!selected, |this| {
                        this.hover(move |style| style.bg(hover_bg).text_color(strong))
                    })
                    .tooltip(move |window, cx| titlebar_tooltip(display_path.clone(), window, cx))
                    .child(
                        div()
                            .flex_none()
                            .w(px(16.0))
                            .opacity(0.75)
                            .child(titlebar_svg_icon(
                                file_icon(&document.path),
                                15.0,
                                p.row_text,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(NAME_SIZE))
                            .font_weight(FontWeight::LIGHT)
                            .child(document.name.clone()),
                    )
                    .child(
                        div()
                            .id(("native-docs-open-file-close", index))
                            .relative()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(20.0))
                            .rounded(px(4.0))
                            .hover(move |style| style.bg(hover_bg))
                            .tooltip(move |window, cx| titlebar_tooltip(close_label, window, cx))
                            .child(
                                div()
                                    .absolute()
                                    .size(px(8.0))
                                    .rounded_full()
                                    .bg(p.row_text)
                                    .when(!dirty, |this| this.invisible())
                                    .when(dirty, |this| {
                                        this.group_hover(group.clone(), |style| style.invisible())
                                    }),
                            )
                            .child(
                                div()
                                    .invisible()
                                    .group_hover(group.clone(), |style| style.visible())
                                    .child(titlebar_svg_icon("titlebar/x.svg", 14.0, p.row_text)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.native_docs_request_close(&close_path, window, cx);
                            })),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, window, cx| {
                            this.native_docs_request_close(&middle_path, window, cx);
                        }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_select(&path, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .max_h(gpui::relative(0.34))
                .min_h_0()
                .child(self.render_native_docs_open_files_header(p, cx))
                .child(
                    div()
                        .id("native-docs-open-files")
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .pb(px(4.0))
                        .border_b_1()
                        .border_color(p.rule)
                        .overflow_y_scroll()
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    /// The "Open Files" label with its close-all "x" beside the words, centred in its row.
    ///
    /// CDXC:Docs 2026-09-25 DECISION:
    /// User: "add a new button that just says x that closes all open files (next to the word open files) also please adjust the alignment for open files label there so it has equal gap above and below it".
    fn render_native_docs_open_files_header(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = p.row_hover;
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .py(px(6.0))
            .px(px(14.0))
            .text_size(px(11.0))
            .line_height(px(14.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.subtle)
            .child("Open Files")
            .child(
                div()
                    .id("native-docs-close-all")
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(14.0))
                    .px(px(3.0))
                    .rounded(px(3.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .child("x")
                    .tooltip(|window, cx| titlebar_tooltip("Close all open files", window, cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.native_docs_request_close_all(window, cx);
                    })),
            )
            .into_any_element()
    }

    /// Folders that have listed children, the ones the chevron can open.
    pub(crate) fn native_docs_expandable_folders(&self) -> HashSet<&str> {
        let entries = &self.native_docs.entries;
        let directories: HashSet<&str> = entries
            .iter()
            .filter(|entry| entry.kind == DocsEntryKind::Directory)
            .map(|entry| entry.path.as_str())
            .collect();
        entries
            .iter()
            .filter(|entry| entry.depth > 0)
            .filter_map(|entry| directories.get(parent_path(&entry.path)).copied())
            .collect()
    }

    fn render_native_docs_tree(
        &mut self,
        p: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.native_docs;
        let status = match state.load_state {
            Some(DocsLoadState::Loading) => Some("Updating files…".to_string()),
            Some(DocsLoadState::Error) => state.error.clone(),
            _ if state.entries.is_empty() => Some("No files found".to_string()),
            _ => None,
        };
        let active = state.active.clone().unwrap_or_default();
        let expandable = self.native_docs_expandable_folders();
        let rename = self
            .native_docs
            .rename
            .as_ref()
            .map(|rename| (rename.path.clone(), rename.input.clone()));
        let rows: Vec<AnyElement> = self
            .native_docs_tree_rows()
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let is_directory = row.kind == DocsEntryKind::Directory;
                let selected = !is_directory && active == row.path;
                let ancestor = is_directory && active.starts_with(&format!("{}/", row.path));
                let has_children = expandable.contains(row.path.as_str());
                let text = if ancestor {
                    p.ancestor_text
                } else if selected {
                    p.row_text_strong
                } else if is_directory {
                    p.muted
                } else {
                    p.row_text
                };
                let chevron = svg()
                    .size(px(14.0))
                    .path("docs/t-chevron-right-19.svg")
                    .text_color(if ancestor { p.ancestor_text } else { p.subtle })
                    .when(row.expanded, |this| {
                        this.with_transformation(Transformation::rotate(radians(
                            std::f32::consts::FRAC_PI_2,
                        )))
                    });
                let icon = if is_directory {
                    if row.expanded {
                        "docs/t-folder-open-175.svg"
                    } else {
                        "docs/t-folder-175.svg"
                    }
                } else {
                    file_icon(&row.path)
                };
                let editing = rename
                    .as_ref()
                    .filter(|(path, _)| *path == row.path)
                    .map(|(_, input)| input.clone());
                let click_path = row.path.clone();
                let click_display = row.display_path.clone();
                let menu_path = row.path.clone();
                let kind = row.kind;
                let note_count = self.native_docs.notes.count(&row.path);
                let hover_bg = p.row_hover;
                let strong = p.row_text_strong;
                div()
                    .id(("native-docs-tree-row", index))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(9.0))
                    .min_h(px(TREE_ROW_HEIGHT))
                    .py(px(7.0))
                    .pr(px(7.0))
                    .pl(px(9.0 + TREE_INDENT * row.depth as f32))
                    .cursor_pointer()
                    .text_color(text)
                    .when(selected, |this| this.bg(p.row_surface))
                    .when(!selected, |this| {
                        this.hover(move |style| style.bg(hover_bg).text_color(strong))
                    })
                    .child(
                        div()
                            .flex_none()
                            .w(px(14.0))
                            .when(!(is_directory && has_children), |this| this.opacity(0.0))
                            .child(chevron),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(16.0))
                            .opacity(0.75)
                            .child(titlebar_svg_icon(icon, 15.0, text)),
                    )
                    .child(match editing {
                        Some(input) => div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(NAME_SIZE))
                            .font_weight(FontWeight::LIGHT)
                            .child(Input::new(&input).small())
                            .into_any_element(),
                        None => div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(NAME_SIZE))
                            .font_weight(FontWeight::LIGHT)
                            .line_height(px(20.0))
                            .child(row.name)
                            .into_any_element(),
                    })
                    .when(note_count > 0, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .h(px(17.0))
                                .min_w(px(17.0))
                                .px(px(4.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.0))
                                .bg(p.raised)
                                .border_1()
                                .border_color(p.border_strong)
                                .text_size(px(10.0))
                                .text_color(p.muted)
                                .child(note_count.to_string()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if is_directory {
                            this.native_docs_toggle_folder(&click_path, cx);
                        } else {
                            this.native_docs_open(&click_path, &click_display, cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.show_native_docs_entry_menu(
                                &menu_path,
                                kind,
                                event.position,
                                window,
                                cx,
                            );
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        let _ = window;
        if let Some(reveal) = self.native_docs.reveal_request.take() {
            let offset = usize::from(status.is_some());
            if let Some(index) = self
                .native_docs_tree_rows()
                .iter()
                .position(|row| row.path == reveal)
            {
                self.native_docs.tree_scroll.scroll_to_item(index + offset);
            }
        }
        div()
            .id("native-docs-tree")
            .track_scroll(&self.native_docs.tree_scroll)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .pt(px(4.0))
            .pb(px(10.0))
            .overflow_y_scroll()
            .when_some(status, |this, status| {
                this.child(
                    div()
                        .px(px(12.0))
                        .py(px(8.0))
                        .text_size(px(11.0))
                        .text_color(p.subtle)
                        .child(status),
                )
            })
            .children(rows)
            .into_any_element()
    }
}
