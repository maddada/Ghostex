//! The files list: its header buttons, Search, Open Files and the project tree, drawn
//! natively with the former React Docs page's metrics.

use std::cell::Cell;
use std::collections::HashSet;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _,
    Styled as _, Transformation, Window, div, px, radians, svg,
};
use gpui_component::input::Input;

use super::files::parent_path;
use super::palette::DocsPalette;
use super::render::SIDEBAR_WIDTH;
use super::sidebar::DocsSidebarLayout;
use super::state::{DocsEntryKind, DocsFileKind, DocsLoadState, DocsTransient};
use super::tree_drag::{DocsTreeDrag, can_move, drop_folder};
use crate::GhostexGpuiApp;
use crate::app::consts::{
    TITLEBAR_BUTTON_RADIUS, TITLEBAR_CONTROL_HEIGHT, TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
    WORKAREA_HEADER_EDGE_PADDING,
};
use crate::app::helpers::titlebar_svg_icon;
// Every Files sidebar tooltip is the sidebar's bubble (CDXC:Tooltips 2026-10-25 DECISION in
// project_search_view.rs).
use crate::app::native_sidebar::link_chips::{sidebar_style_card_tooltip, sidebar_style_tooltip};

/// The header row, the search row and the document header share this height.
pub(crate) const ROW_STRIP_HEIGHT: f32 = 35.0;
const HEADER_BUTTON_WIDTH: f32 = 32.0;
const HEADER_BUTTON_GAP: f32 = 2.0;
const OPEN_FILE_ROW_HEIGHT: f32 = 28.0;
const TREE_ROW_HEIGHT: f32 = 34.0;
const TREE_INDENT: f32 = 18.0;
const NAME_SIZE: f32 = crate::ui_fonts::FILES_NAME_SIZE;

thread_local! {
    /// Where the header's menu buttons are, so their menus open below them.
    pub(crate) static CREATE_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
    pub(crate) static OVERFLOW_MENU_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

/// The icon a file row shows (`manageFileIconForPath`).
pub(crate) fn file_icon(path: &str) -> &'static str {
    match super::state::DocsMediaKind::for_path(path) {
        Some(super::state::DocsMediaKind::Image) => return "titlebar/photo.svg",
        Some(super::state::DocsMediaKind::Video) => return "titlebar/movie.svg",
        Some(super::state::DocsMediaKind::Audio) => return "titlebar/music.svg",
        None => {}
    }
    match DocsFileKind::for_path(path) {
        DocsFileKind::Markdown => "files-view/t-markdown-175.svg",
        DocsFileKind::Html => "files-view/t-file-type-html-175.svg",
        DocsFileKind::Excalidraw => "files-view/t-edit-175.svg",
        DocsFileKind::Pdf => "files-view/t-file-type-pdf-175.svg",
        DocsFileKind::Font => "titlebar/letter-case.svg",
        DocsFileKind::Sqlite => "titlebar/database.svg",
        DocsFileKind::Text if DocsFileKind::editor_language(path) != "text" => {
            "titlebar/file-code.svg"
        }
        _ => "files-view/t-file-175.svg",
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
        // The Search view (Cmd+Shift+F) takes the list's place, inside the same frame.
        let body: Vec<AnyElement> = if self.native_docs.search_panel {
            vec![self.render_native_docs_project_search(p, layout, window, cx)]
        } else {
            let header = self.render_native_docs_files_header(p, layout, cx);
            let search = self.render_native_docs_search_row(p, window, cx);
            // With the open files as tabs, the row above the view lists them (open_file_tabs.rs).
            let open_files = if self.docs_open_files_as_tabs {
                None
            } else {
                self.render_native_docs_open_files(p, cx)
            };
            let tree = self.render_native_docs_tree(p, window, cx);
            [header, search]
                .into_iter()
                .chain(open_files)
                .chain([tree])
                .collect()
        };
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
            .children(body)
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
        // `isCreatingItem`: the "+" waits while something is being created.
        let creating = self
            .native_docs
            .file_operation
            .as_ref()
            .is_some_and(|(action, _)| action.starts_with("create:"));
        let active_listed = self.native_docs.active.as_deref().is_some_and(|active| {
            self.native_docs
                .entries
                .iter()
                .any(|entry| entry.path == active && entry.kind == DocsEntryKind::File)
        });
        let collapse_icon = svg()
            .size(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE))
            .path(if any_open {
                "files-view/t-arrows-diagonal-minimize-2.svg"
            } else {
                "files-view/t-arrows-diagonal-2-2.svg"
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
        let edge = self.render_native_docs_edge_tile(p, layout, cx);
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
                    "native-docs-show-search",
                    header_icon("files-view/t-search-18.svg", false, p),
                    false,
                    false,
                    p,
                )
                .tooltip(|window, cx| {
                    sidebar_style_tooltip(
                        crate::app::helpers::titlebar_tooltip_label(
                            "Search in files",
                            "searchProjectFiles",
                        )
                        .to_string(),
                        window,
                        cx,
                    )
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.native_docs_show_project_search(window, cx);
                })),
            )
            // CDXC:Docs 2026-10-09 DECISION: User: "remove this button in files list top bar" (the Open file button, which only focused the "Search or paste a path" box under it). The box is still reached by clicking it, by Cmd/Ctrl+F in Files and by the Open File hotkey.
            .child(
                header_tile(
                    "native-docs-toggle-all",
                    collapse_icon,
                    false,
                    !can_toggle_all,
                    p,
                )
                .tooltip(move |window, cx| {
                    sidebar_style_tooltip(collapse_label.to_string(), window, cx)
                })
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
                    header_icon("files-view/t-current-location-2.svg", !active_listed, p),
                    false,
                    !active_listed,
                    p,
                )
                .tooltip(|window, cx| {
                    sidebar_style_tooltip("Reveal open file in sidebar".to_string(), window, cx)
                })
                .when(active_listed, |this| {
                    this.on_click(cx.listener(|this, _, window, cx| {
                        this.native_docs_reveal_open_file(window, cx);
                    }))
                }),
            )
            .child(
                header_tile(
                    "native-docs-create",
                    header_icon("files-view/t-plus-2.svg", creating, p),
                    false,
                    creating,
                    p,
                )
                .child(anchor(&CREATE_MENU_ANCHOR))
                .tooltip(|window, cx| {
                    sidebar_style_tooltip("Create docs item".to_string(), window, cx)
                })
                .when(!creating, |this| {
                    this.on_click(cx.listener(|this, _, window, cx| {
                        this.show_native_docs_create_menu("docs", window, cx);
                    }))
                }),
            )
            .child(
                header_tile(
                    "native-docs-overflow",
                    header_icon("files-view/t-menu-2-2.svg", false, p),
                    false,
                    false,
                    p,
                )
                .child(anchor(&OVERFLOW_MENU_ANCHOR))
                .tooltip(|window, cx| {
                    sidebar_style_tooltip("Files sidebar menu".to_string(), window, cx)
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_native_docs_overflow_menu(window, cx);
                })),
            )
            .children(edge)
            .into_any_element()
    }

    /// The header's rightmost button: hide the docked list, pin a peek, or close a drawer. The
    /// files header and the Search view's header both end with it.
    pub(crate) fn render_native_docs_edge_tile(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let peek = self.native_docs.transient == Some(DocsTransient::Peek);
        // CDXC:Docs 2026-09-16 DECISION:
        // User: show the pin icon only when the list can actually be pinned. A narrow pane cannot dock the list, so a peek there offers the close control instead of a pin that appears to do nothing.
        let edge = (!layout.forced).then_some({
            if layout.docked {
                (
                    "files-view/t-layout-sidebar-right-collapse-2.svg",
                    "Hide files",
                    "hide",
                )
            } else if peek && !layout.narrow {
                ("files-view/t-pin-2.svg", "Pin files", "pin")
            } else {
                (
                    "files-view/t-layout-sidebar-right-collapse-2.svg",
                    "Close files",
                    "close",
                )
            }
        });
        edge.map(|(icon, label, kind)| {
            header_tile(
                "native-docs-edge",
                header_icon(icon, false, p),
                false,
                false,
                p,
            )
            .tooltip(move |window, cx| sidebar_style_tooltip(label.to_string(), window, cx))
            .on_click(cx.listener(move |this, _, _, cx| match kind {
                "hide" => this.native_docs_hide_sidebar(cx),
                "pin" => this.native_docs_pin_peek(cx),
                _ => this.native_docs_close_transient(cx),
            }))
            .into_any_element()
        })
    }

    /// The search box's leading button: the type filter (`type_filter.rs`), accent-coloured with a
    /// dot while types are picked.
    fn render_native_docs_type_filter_button(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.native_docs_type_filter_active();
        let tooltip = self.native_docs_type_filter_tooltip();
        let hover = p.control_hover;
        div()
            .id("native-docs-type-filter")
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(22.0))
            .rounded(px(5.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(super::type_filter::filter_anchor())
            .child(titlebar_svg_icon(
                "titlebar/filter.svg",
                15.0,
                if active { p.accent } else { p.muted },
            ))
            .when(active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(2.0))
                        .right(px(2.0))
                        .size(px(6.0))
                        .rounded_full()
                        .bg(p.accent),
                )
            })
            .tooltip(move |window, cx| sidebar_style_tooltip(tooltip.clone(), window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                this.show_native_docs_type_filter_menu(window, cx);
            }))
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
            // CDXC:Docs 2026-10-10 DECISION: User, of the files search field: "reduce gap between icon and text to half" (11px to 5.5px).
            .gap(px(5.5))
            .h(px(ROW_STRIP_HEIGHT))
            .px(px(10.0))
            .mb(px(4.0))
            .border_b_1()
            .border_color(p.rule)
            .when(focused, |this| this.bg(p.text.opacity(0.08)))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_target.update(cx, |input, cx| input.focus(window, cx));
            })
            .child(self.render_native_docs_type_filter_button(p, cx))
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
    ///
    /// CDXC:Docs 2026-10-28 DECISION:
    /// User: "remove this in the sidebar of the files", pointing at the "Project Docs" label, so the tree has no label above it.
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
                let display_path = document.display_path.clone();
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
                    .tooltip(move |window, cx| {
                        sidebar_style_card_tooltip(display_path.to_string(), 20.0, 0.0, window, cx)
                    })
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
                            .tooltip(move |window, cx| {
                                sidebar_style_tooltip(close_label.to_string(), window, cx)
                            })
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.native_docs_open_files_focus_handle(cx)
                            .focus(window, cx);
                        this.native_docs_select(&path, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        let list_focus = self.native_docs_open_files_focus_handle(cx);
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
                        .track_focus(&list_focus)
                        .on_key_down(cx.listener(Self::native_docs_open_files_key_down))
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

    /// The "Open Files" label, with the close-all button at the row's right edge, in line with the
    /// rows' own close buttons.
    ///
    /// CDXC:Docs 2026-10-01 DECISION:
    /// User: the close-all-open-files button sits right-aligned in the "Open Files" row, not beside the label, vertically centred on the label, inside a circle, with the same x icon as the open files' own close buttons, and shows only while the pointer is over that row. The label keeps equal space above and below it (2026-09-25). This supersedes the 2026-09-30 decision, which had no circle.
    fn render_native_docs_open_files_header(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = p.row_hover;
        let group: SharedString = "docs-open-files-header".into();
        div()
            .group(group.clone())
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(26.0))
            .pl(px(14.0))
            .pr(px(7.0))
            .text_size(px(11.0))
            .line_height(px(14.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.subtle)
            .child("Open Files")
            .child(
                div()
                    .id("native-docs-close-all")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(18.0))
                    .rounded_full()
                    .border_1()
                    .border_color(p.border_strong)
                    .cursor_pointer()
                    .invisible()
                    .group_hover(group, |style| style.visible())
                    .hover(move |style| style.bg(hover))
                    .child(titlebar_svg_icon("titlebar/x.svg", 11.0, p.row_text))
                    .tooltip(|window, cx| {
                        sidebar_style_tooltip("Close all open files".to_string(), window, cx)
                    })
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
        let search_key = self.native_docs_search_key();
        let filter_tree = self.native_docs.search_query.trim().is_empty()
            && self.native_docs_type_filter_active();
        let state = &self.native_docs;
        let query = state.search_query.trim();
        let status = if !query.is_empty() || filter_tree {
            match &state.search_results {
                _ if state.search_results_query != search_key => Some("Searching…".to_string()),
                Some(results) if results.is_empty() && state.search_incomplete => Some(
                    "No matching files. This project is large, so search covers its first 200,000 files and folders."
                        .to_string(),
                ),
                Some(results) if results.is_empty() => Some("No matching files".to_string()),
                _ if state.search_truncated && filter_tree => Some(
                    "Showing the first 3,000 files of these types. Type a name to narrow them."
                        .to_string(),
                ),
                _ if state.search_truncated => {
                    Some("Showing the best matches. Type more to narrow them.".to_string())
                }
                _ => None,
            }
        } else {
            match state.load_state {
                Some(DocsLoadState::Loading) => Some("Updating files…".to_string()),
                Some(DocsLoadState::Error) => state.error.clone(),
                _ if state.entries.is_empty() => Some("No files found".to_string()),
                _ => None,
            }
        };
        let query_is_empty = query.is_empty();
        let active = state.active.clone().unwrap_or_default();
        let expandable = self.native_docs_expandable_folders();
        let tree_focused = self
            .native_docs
            .tree_focus_handle
            .as_ref()
            .is_some_and(|handle| handle.is_focused(window));
        let focused_row = self.native_docs.tree_focus.clone();
        // While a row is dragged: what the drop targets check a move against.
        let drag_entries = cx
            .has_active_drag()
            .then(|| std::rc::Rc::new(self.native_docs_drag_entries()));
        let surface = p.row_surface;
        let rows: Vec<AnyElement> =
            self.native_docs_tree_rows()
                .into_iter()
                .enumerate()
                .map(|(index, row)| {
                    let is_directory = row.kind == DocsEntryKind::Directory;
                    let selected = !is_directory && active == row.path;
                    let ancestor = is_directory && active.starts_with(&format!("{}/", row.path));
                    // A folder not listed yet may have children; a listed one shows the chevron only
                    // when it does. Search results open their folder in the tree instead.
                    // The type filter's folders always hold a matching file.
                    let has_children = row.folder.is_none()
                        && (filter_tree
                            || expandable.contains(row.path.as_str())
                            || !self.native_docs.loaded_folders.contains(&row.path));
                    let in_search = row.folder.is_some() || !query_is_empty;
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
                        .path("files-view/t-chevron-right-19.svg")
                        .text_color(if ancestor { p.ancestor_text } else { p.subtle })
                        .when(row.expanded, |this| {
                            this.with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2,
                            )))
                        });
                    let icon = if is_directory {
                        if row.expanded {
                            "files-view/t-folder-open-175.svg"
                        } else {
                            "files-view/t-folder-175.svg"
                        }
                    } else {
                        file_icon(&row.path)
                    };
                    let keyboard_focused =
                        tree_focused && focused_row.as_deref() == Some(row.path.as_str());
                    let load_badge =
                        if let Some(error) = self.native_docs.folder_errors.get(&row.path) {
                            Some(("!", SharedString::from(error.clone())))
                        } else if self.native_docs.folders_loading.contains(&row.path) {
                            Some(("…", SharedString::from("Folder not loaded yet")))
                        } else {
                            None
                        };
                    let focus_path = row.path.clone();
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
                        // The row keyboard focus is on (`:focus-visible`, the hover look).
                        .when(keyboard_focused && !selected, |this| {
                            this.bg(hover_bg).text_color(strong)
                        })
                        // Drag to move (`tree_drag.rs`).
                        .map(|this| {
                            let drag = DocsTreeDrag {
                                path: row.path.clone(),
                                kind,
                                name: row.name.clone().into(),
                                icon,
                                text: p.text,
                                background: p.raised,
                                border: p.border_strong,
                            };
                            let folder = drop_folder(&row.path, kind);
                            let over_folder = folder.clone();
                            let entries = drag_entries.clone();
                            this.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                                .drag_over::<DocsTreeDrag>(move |style, drag, _, _| {
                                    if entries.as_ref().is_some_and(|entries| {
                                        can_move(entries, drag, &over_folder)
                                    }) {
                                        style.bg(surface)
                                    } else {
                                        style
                                    }
                                })
                                .on_drop(cx.listener(move |this, drag: &DocsTreeDrag, _, cx| {
                                    cx.stop_propagation();
                                    this.native_docs_drop_into(drag, &folder, cx);
                                }))
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
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_baseline()
                                .gap(px(7.0))
                                .child(
                                    div()
                                        .flex_none()
                                        .max_w_full()
                                        .truncate()
                                        .text_size(px(NAME_SIZE))
                                        .font_weight(FontWeight::LIGHT)
                                        .line_height(px(20.0))
                                        .child(row.name),
                                )
                                .when_some(row.folder.clone(), |this, folder| {
                                    this.child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis_start()
                                            .text_size(px(11.5))
                                            .text_color(p.subtle)
                                            .child(folder),
                                    )
                                }),
                        )
                        .when_some(load_badge, |this, (badge, tooltip)| {
                            this.child(
                                div()
                                    .id(("native-docs-tree-badge", index))
                                    .flex_none()
                                    .text_size(px(12.0))
                                    .text_color(p.muted)
                                    .child(badge)
                                    .tooltip(move |window, cx| {
                                        sidebar_style_card_tooltip(
                                            tooltip.to_string(),
                                            20.0,
                                            0.0,
                                            window,
                                            cx,
                                        )
                                    }),
                            )
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
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.native_docs_focus_tree_row(&focus_path, window, cx);
                            }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if is_directory && in_search {
                                // A folder found by search: show it, open, in the tree.
                                this.native_docs_clear_search(window, cx);
                                this.native_docs.expanded.insert(click_path.clone());
                                this.native_docs_list_folder(&click_path, cx);
                                this.native_docs_reveal_in_tree(&click_path, cx);
                                this.native_docs.reveal_request = Some(click_path.clone());
                                this.native_docs_notify(cx);
                            } else if is_directory {
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
        self.native_docs.tree_status_rows = usize::from(status.is_some());
        let tree_focus = self.native_docs_tree_focus_handle(cx);
        div()
            .id("native-docs-tree")
            .track_focus(&tree_focus)
            .on_key_down(cx.listener(Self::native_docs_tree_key_down))
            .track_scroll(&self.native_docs.tree_scroll)
            // Dragging a row near the list's top or bottom edge scrolls it, 12px per move within
            // 30px of the edge, like the former React Docs page.
            .on_drag_move(
                cx.listener(|this, event: &gpui::DragMoveEvent<DocsTreeDrag>, _, cx| {
                    let (y, bounds) = (event.event.position.y, event.bounds);
                    if !bounds.contains(&event.event.position) {
                        return;
                    }
                    let step = if y < bounds.top() + px(30.0) {
                        px(12.0)
                    } else if y > bounds.bottom() - px(30.0) {
                        px(-12.0)
                    } else {
                        return;
                    };
                    let scroll = &this.native_docs.tree_scroll;
                    let max = scroll.max_offset().y;
                    let mut offset = scroll.offset();
                    offset.y = (offset.y + step).clamp(-max, px(0.0));
                    scroll.set_offset(offset);
                    cx.notify();
                }),
            )
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
            .child(self.render_native_docs_root_drop(drag_entries, p, cx))
            .into_any_element()
    }

    /// The space under the last row: dropping there moves the dragged item into the `docs` folder.
    /// While a movable item is over it, it shows the Docs page's insertion line at its top.
    fn render_native_docs_root_drop(
        &mut self,
        drag_entries: Option<std::rc::Rc<Vec<(String, DocsEntryKind)>>>,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let line = if p.light {
            gpui::rgb(0x52525b)
        } else {
            gpui::rgb(0xc8cdd5)
        };
        div()
            .id("native-docs-tree-root-drop")
            .flex_1()
            .min_h(px(TREE_ROW_HEIGHT))
            .border_t(px(3.0))
            .border_color(gpui::transparent_black())
            .drag_over::<DocsTreeDrag>(move |style, drag, _, _| {
                if drag_entries
                    .as_ref()
                    .is_some_and(|entries| can_move(entries, drag, super::tree_drag::DOCS_ROOT))
                {
                    style.border_color(line)
                } else {
                    style
                }
            })
            .on_drop(cx.listener(|this, drag: &DocsTreeDrag, _, cx| {
                this.native_docs_drop_into(drag, super::tree_drag::DOCS_ROOT, cx);
            }))
            .into_any_element()
    }
}
