//! Draws the Files view's Search sidebar: VS Code's Search view laid out in the Files sidebar's own
//! fonts, colours, rows and hover. Its state and keyboard live in `project_search.rs`.
//!
//! CDXC:Docs 2026-10-25 DECISION:
//! User: "same sidebar look and behavior" as VS Code's search. A title row with Refresh, Clear Search Results, Collapse/Expand All and View as Tree/List; the query box with Match Case, Match Whole Word and Use Regular Expression inside it; Toggle Search Details (…) revealing "files to include" and "files to exclude", the latter with Use Exclude Settings and Ignore Files (on by default); the "N results in M files" line without Open in editor; and the results grouped by file (icon, name, dimmed folder, match count) with each match line highlighted, files folding on their chevrons, and a × on a hovered row that drops it from the list. No Replace ("no replace at all").

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, Entity, FontWeight, HighlightStyle, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    Styled as _, StyledText, Transformation, Window, div, px, radians, svg, uniform_list,
};
use gpui_component::Sizable as _;
use gpui_component::input::{Input, InputState};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

use super::files_list::{ROW_STRIP_HEIGHT, file_icon, header_icon, header_tile};
use super::palette::DocsPalette;
use super::project_search::{DocsProjectSearch, SearchRow};
use super::sidebar::DocsSidebarLayout;
use crate::GhostexGpuiApp;
use crate::app::consts::{TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE, WORKAREA_HEADER_EDGE_PADDING};
use crate::app::helpers::titlebar_svg_icon;
use crate::app::native_sidebar::link_chips::{sidebar_style_card_tooltip, sidebar_style_tooltip};

/// The Open Files rows' height, for every result row (the list is virtualized, one height).
const RESULT_ROW_HEIGHT: f32 = 28.0;
const INDENT: f32 = 14.0;
const NAME_SIZE: f32 = 14.5;
const LINE_SIZE: f32 = 13.0;
const FIELD_HEIGHT: f32 = 30.0;
const TOGGLE_SIZE: f32 = 22.0;

/// VS Code's find-match highlight, orange at a third, readable on both appearances.
fn match_highlight() -> Hsla {
    gpui::rgba(0xea5c0055).into()
}

/// How far below the pointer a row's card starts, clear of the pointer's arrow (the gap the
/// files list's row tooltips keep, CDXC:Tooltips 2026-10-09 in `list_row_tooltip`).
const ROW_TOOLTIP_GAP: f32 = 20.0;

/// A button's or toggle's tooltip: the sidebar's frosted bubble.
///
/// CDXC:Tooltips 2026-10-25 DECISION:
/// User, of the frosted link tooltip: "amazing tooltip bg/style pls use it for the sidebar and find in files extension more", and the Search summary's (i) tooltip ran off the window. Every Search view tooltip is the sidebar's bubble (`native_sidebar/tooltips.rs`), frosted when the tooltip host draws it; a row's or note's text uses its card, which sizes itself to its measured text (wrapping at 340px), so the tooltip's placement keeps it inside the window. Plain wrapping tooltips were measured at their narrowest and drawn wider, so they ran past the window's edge.
fn search_tooltip(text: &str, window: &mut Window, cx: &mut App) -> gpui::AnyView {
    sidebar_style_tooltip(text.to_string(), window, cx)
}

/// A row's path or a note: the sidebar's card, `top_gap` below the pointer.
fn search_card_tooltip(
    text: &str,
    top_gap: f32,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyView {
    sidebar_style_card_tooltip(text.to_string(), top_gap, 0.0, window, cx)
}

/// `1,234 results`, `1 file`.
fn plural(count: usize, one: &str, many: &str) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{grouped} {}", if count == 1 { one } else { many })
}

fn count_badge(count: usize, p: &DocsPalette) -> gpui::Div {
    div()
        .flex_none()
        .h(px(17.0))
        .min_w(px(17.0))
        .px(px(5.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.5))
        .bg(p.raised)
        .border_1()
        .border_color(p.border_strong)
        .text_size(px(10.5))
        .text_color(p.muted)
        .child(count.to_string())
}

fn section_caption(text: &'static str, p: &DocsPalette) -> impl IntoElement {
    div()
        .px(px(12.0))
        .pt(px(6.0))
        .pb(px(3.0))
        .text_size(px(11.0))
        .line_height(px(14.0))
        .text_color(p.subtle)
        .child(text)
}

impl GhostexGpuiApp {
    /// The Search view, in the sidebar frame `render_native_docs_files_list` draws.
    pub(crate) fn render_native_docs_project_search(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.native_docs_ensure_project_search(window, cx);
        self.native_docs_refresh_stale_project_search(cx);
        let header = self.render_native_docs_search_header(p, layout, cx);
        let Some(search) = self.native_docs.project_search.as_mut() else {
            return div().into_any_element();
        };
        search.rebuild_rows();
        let fields = self.render_native_docs_search_fields(p, window, cx);
        let summary = self.render_native_docs_search_summary(p, cx);
        let Some(search) = self.native_docs.project_search.as_ref() else {
            return div().into_any_element();
        };
        let results_focus = search.results_focus.clone();
        let results = uniform_list(
            "native-docs-search-results",
            search.rows.len(),
            cx.processor(Self::render_native_docs_search_rows),
        )
        .track_scroll(&search.scroll)
        .size_full();
        div()
            .id("native-docs-project-search")
            .key_context("NativeDocsSearch")
            .on_key_down(cx.listener(Self::native_docs_project_search_key_down))
            // The floating list's own window has no Files root to claim Cmd+Shift+F; the panel does.
            .on_action(cx.listener(
                |this, action: &crate::app::hotkeys::RunConfiguredGhostexHotkey, window, cx| {
                    if super::project_search::claims_hotkey(&action.action_id) {
                        this.native_docs_show_project_search(window, cx);
                    } else {
                        cx.propagate();
                    }
                },
            ))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(fields)
            .children(summary)
            .child(
                div()
                    .id("native-docs-search-results-frame")
                    .track_focus(&results_focus)
                    .flex_1()
                    .min_h_0()
                    // VS Code leaves a gap between the summary and the first result row.
                    .pt(px(4.0))
                    .pb(px(6.0))
                    .child(results),
            )
            .into_any_element()
    }

    fn render_native_docs_search_header(
        &mut self,
        p: &DocsPalette,
        layout: DocsSidebarLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let edge = self.render_native_docs_edge_tile(p, layout, cx);
        let (has_query, has_results, any_expanded, view_as_tree) = self
            .native_docs
            .project_search
            .as_ref()
            .map(|search| {
                (
                    !search.query.read(cx).value().is_empty(),
                    !search.results.is_empty(),
                    search.any_expanded(),
                    search.view_as_tree,
                )
            })
            .unwrap_or_default();
        let collapse_icon = svg()
            .size(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE))
            .path(if any_expanded || !has_results {
                "files-view/t-arrows-diagonal-minimize-2.svg"
            } else {
                "files-view/t-arrows-diagonal-2-2.svg"
            })
            .text_color(if has_results {
                p.toolbar_icon
            } else {
                p.toolbar_disabled
            })
            .with_transformation(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2)));
        let collapse_label = if any_expanded || !has_results {
            "Collapse All"
        } else {
            "Expand All"
        };
        let view_label = if view_as_tree {
            "View as List"
        } else {
            "View as Tree"
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(ROW_STRIP_HEIGHT))
            .pl(px(14.0))
            .pr(px(WORKAREA_HEADER_EDGE_PADDING))
            .border_b_1()
            .border_color(p.border)
            // CDXC:Docs 2026-10-25 DECISION: User: "remove 'search' from here" (the header's title). The spacer keeps the buttons at the right.
            .child(div().flex_1().min_w_0())
            .child(
                header_tile(
                    "native-docs-search-refresh",
                    header_icon("files-view/t-refresh-2.svg", !has_query, p),
                    false,
                    !has_query,
                    p,
                )
                .tooltip(|window, cx| search_tooltip("Refresh", window, cx))
                .when(has_query, |this| {
                    this.on_click(cx.listener(|this, _, _, cx| {
                        this.native_docs_schedule_project_search(true, cx);
                    }))
                }),
            )
            .child(
                header_tile(
                    "native-docs-search-clear",
                    header_icon("files-view/l-list-x-2.svg", !has_query && !has_results, p),
                    false,
                    !has_query && !has_results,
                    p,
                )
                .tooltip(|window, cx| search_tooltip("Clear Search Results", window, cx))
                .when(has_query || has_results, |this| {
                    this.on_click(cx.listener(|this, _, window, cx| {
                        this.native_docs_clear_project_search(window, cx);
                    }))
                }),
            )
            .child(
                header_tile(
                    "native-docs-search-collapse",
                    collapse_icon,
                    false,
                    !has_results,
                    p,
                )
                .tooltip(move |window, cx| search_tooltip(collapse_label, window, cx))
                .when(has_results, |this| {
                    this.on_click(cx.listener(|this, _, _, cx| {
                        this.native_docs_toggle_search_collapse_all(cx);
                    }))
                }),
            )
            .child(
                header_tile(
                    "native-docs-search-view",
                    header_icon(
                        if view_as_tree {
                            "files-view/l-list-2.svg"
                        } else {
                            "files-view/l-list-tree-2.svg"
                        },
                        false,
                        p,
                    ),
                    false,
                    false,
                    p,
                )
                .tooltip(move |window, cx| search_tooltip(view_label, window, cx))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(search) = this.native_docs.project_search.as_mut() {
                        search.view_as_tree = !search.view_as_tree;
                        search.mark_rows_stale();
                    }
                    this.native_docs_notify(cx);
                })),
            )
            .child(
                header_tile(
                    "native-docs-search-files",
                    header_icon("files-view/t-folder-175.svg", false, p),
                    false,
                    false,
                    p,
                )
                .tooltip(|window, cx| search_tooltip("Show files", window, cx))
                .on_click(cx.listener(|this, _, _, cx| this.native_docs_show_files_tree(cx))),
            )
            .children(edge)
            .into_any_element()
    }

    /// One bordered field (the query, include or exclude box) with its buttons inside it.
    fn native_docs_search_field(
        id: &'static str,
        input: &Entity<InputState>,
        trailing: Vec<AnyElement>,
        p: &DocsPalette,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let focused = gpui::Focusable::focus_handle(input.read(cx), cx).is_focused(window);
        let target = input.clone();
        div()
            .id(id)
            .mx(px(10.0))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.0))
            .h(px(FIELD_HEIGHT))
            .pl(px(9.0))
            .pr(px(4.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(if focused {
                p.text.opacity(0.32)
            } else {
                p.border_strong
            })
            .bg(if focused {
                p.text.opacity(0.07)
            } else {
                p.text.opacity(0.035)
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                target.update(cx, |input, cx| input.focus(window, cx));
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(LINE_SIZE))
                    .font_weight(FontWeight::LIGHT)
                    .child(Input::new(input).appearance(false).small()),
            )
            .children(trailing)
            .into_any_element()
    }

    fn native_docs_search_toggle(
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        active: bool,
        p: &DocsPalette,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = p.control_hover;
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(TOGGLE_SIZE))
            .rounded(px(4.0))
            .cursor_pointer()
            .border_1()
            .border_color(if active {
                p.accent.opacity(0.7)
            } else {
                gpui::transparent_black()
            })
            .when(active, |this| this.bg(p.accent.opacity(0.18)))
            .when(!active, |this| this.hover(move |style| style.bg(hover)))
            .child(titlebar_svg_icon(
                icon,
                15.0,
                if active { p.text } else { p.muted },
            ))
            .tooltip(move |window, cx| search_tooltip(label, window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    fn render_native_docs_search_fields(
        &mut self,
        p: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(search) = self.native_docs.project_search.as_ref() else {
            return div().into_any_element();
        };
        let (query, include, exclude) = (
            search.query.clone(),
            search.include.clone(),
            search.exclude.clone(),
        );
        let (match_case, whole_word, is_regex, use_ignore_files, details_open) = (
            search.match_case,
            search.whole_word,
            search.is_regex,
            search.use_ignore_files,
            search.details_open,
        );
        let toggles = vec![
            Self::native_docs_search_toggle(
                "native-docs-search-case",
                "files-view/l-case-sensitive-2.svg",
                "Match Case",
                match_case,
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.native_docs_toggle_search_option(
                    |search| search.match_case = !search.match_case,
                    cx,
                );
            }))
            .into_any_element(),
            Self::native_docs_search_toggle(
                "native-docs-search-word",
                "files-view/l-whole-word-2.svg",
                "Match Whole Word",
                whole_word,
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.native_docs_toggle_search_option(
                    |search| search.whole_word = !search.whole_word,
                    cx,
                );
            }))
            .into_any_element(),
            Self::native_docs_search_toggle(
                "native-docs-search-regex",
                "files-view/l-regex-2.svg",
                "Use Regular Expression",
                is_regex,
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.native_docs_toggle_search_option(
                    |search| search.is_regex = !search.is_regex,
                    cx,
                );
            }))
            .into_any_element(),
        ];
        let ignore_toggle = vec![
            Self::native_docs_search_toggle(
                "native-docs-search-ignore",
                "titlebar/filter.svg",
                "Use Exclude Settings and Ignore Files",
                use_ignore_files,
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.native_docs_toggle_search_option(
                    |search| search.use_ignore_files = !search.use_ignore_files,
                    cx,
                );
            }))
            .into_any_element(),
        ];
        let hover = p.control_hover;
        div()
            .flex()
            .flex_col()
            .flex_none()
            .pt(px(10.0))
            .child(Self::native_docs_search_field(
                "native-docs-search-query",
                &query,
                toggles,
                p,
                window,
                cx,
            ))
            .child(
                div().flex().justify_end().px(px(10.0)).pt(px(3.0)).child(
                    div()
                        .id("native-docs-search-details")
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(26.0))
                        .h(px(16.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .when(details_open, |this| this.bg(hover))
                        .hover(move |style| style.bg(hover))
                        .child(titlebar_svg_icon(
                            "files-view/l-ellipsis-2.svg",
                            15.0,
                            p.muted,
                        ))
                        .tooltip(|window, cx| search_tooltip("Toggle Search Details", window, cx))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(search) = this.native_docs.project_search.as_mut() {
                                search.details_open = !search.details_open;
                            }
                            this.native_docs_notify(cx);
                        })),
                ),
            )
            .when(details_open, |this| {
                this.child(section_caption("files to include", p))
                    .child(Self::native_docs_search_field(
                        "native-docs-search-include",
                        &include,
                        Vec::new(),
                        p,
                        window,
                        cx,
                    ))
                    .child(section_caption("files to exclude", p))
                    .child(Self::native_docs_search_field(
                        "native-docs-search-exclude",
                        &exclude,
                        ignore_toggle,
                        p,
                        window,
                        cx,
                    ))
                    .child(div().h(px(4.0)))
            })
            .into_any_element()
    }

    /// "N results in M files", what is going on, or why there is nothing: one short line, with
    /// any longer note behind an (i) icon at its end.
    ///
    /// CDXC:Docs 2026-10-25 DECISION:
    /// User, of the three-line "10,000 results in 311 files. Only the first 10,000 are shown…": "we need this to be in an i icon that when hovered shows this text, and we need padding below the top area". The summary stays one line; the cap (and the no-results explanation) shows in the sidebar's tooltip when the (i) icon is hovered, and the icon shows only when there is such a note.
    fn render_native_docs_search_summary(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let search = self.native_docs.project_search.as_ref()?;
        let line = |text: String, color: Hsla, note: Option<&'static str>| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.0))
                .px(px(14.0))
                .pt(px(6.0))
                .pb(px(8.0))
                .text_size(px(11.5))
                .line_height(px(15.0))
                .text_color(color)
                .child(div().min_w_0().truncate().child(text))
                .when_some(note, |this, note| {
                    this.child(
                        div()
                            .id("native-docs-search-summary-note")
                            .flex_none()
                            .cursor_pointer()
                            .child(titlebar_svg_icon(
                                "titlebar/info-circle.svg",
                                13.0,
                                p.subtle,
                            ))
                            // Below the icon, right edges aligned, opening leftwards (CDXC:Tooltips
                            // 2026-10-25 DECISION in window/frosted_host.rs).
                            .managed_tooltip_with_placement(
                                ManagedTooltipPlacement::BelowLeft,
                                move |window, cx| search_card_tooltip(note, 4.0, window, cx),
                            ),
                    )
                })
                .into_any_element()
        };
        if let Some(error) = &search.error {
            return Some(
                div()
                    .px(px(14.0))
                    .pt(px(6.0))
                    .pb(px(8.0))
                    .text_size(px(11.5))
                    .line_height(px(15.0))
                    .text_color(p.danger)
                    .child(error.clone())
                    .into_any_element(),
            );
        }
        if search.query.read(cx).value().is_empty() {
            return None;
        }
        if search.results.is_empty() {
            return Some(if search.running {
                line("Searching…".to_string(), p.subtle, None)
            } else if search.use_ignore_files {
                line(
                    "No results found".to_string(),
                    p.subtle,
                    Some(
                        "Files in .gitignore, dependency and build folders, vendored code (.dependencies, vendor, third_party), and nested repositories inside dot-folders are skipped while Use Exclude Settings and Ignore Files is on. Name a folder in files to include to search it.",
                    ),
                )
            } else {
                line("No results found".to_string(), p.subtle, None)
            });
        }
        let mut text = format!(
            "{} in {}",
            plural(search.match_count(), "result", "results"),
            plural(search.results.len(), "file", "files")
        );
        if search.running {
            text.push('…');
        }
        let note = (search.capped && !search.running).then_some(
            "Only the first 10,000 results are shown. Narrow the search to see the rest.",
        );
        Some(line(text, p.subtle, note))
    }

    /// The visible slice of the results, for the virtualized list.
    fn render_native_docs_search_rows(
        &mut self,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = self
            .native_docs
            .palette
            .clone()
            .unwrap_or_else(|| DocsPalette::current(window));
        let Some(search) = self.native_docs.project_search.as_ref() else {
            return Vec::new();
        };
        let focused = search.results_focus.is_focused(window);
        range
            .filter_map(|index| Some((index, search.rows.get(index)?.clone())))
            .map(|(index, row)| {
                Self::render_native_docs_search_result_row(search, index, row, focused, &p, cx)
            })
            .collect()
    }

    fn render_native_docs_search_result_row(
        search: &DocsProjectSearch,
        index: usize,
        row: SearchRow,
        list_focused: bool,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = row.key();
        let selected = search.selected.as_ref() == Some(&key);
        let group: SharedString = format!("docs-search-row-{index}").into();
        let (hover_bg, strong) = (p.row_hover, p.row_text_strong);
        let depth = match &row {
            SearchRow::Folder { depth, .. }
            | SearchRow::File { depth, .. }
            | SearchRow::Match { depth, .. } => *depth,
        };
        let chevron = |expanded: bool| {
            div().flex_none().w(px(14.0)).child(
                svg()
                    .size(px(14.0))
                    .path("files-view/t-chevron-right-19.svg")
                    .text_color(p.subtle)
                    .when(expanded, |this| {
                        this.with_transformation(Transformation::rotate(radians(
                            std::f32::consts::FRAC_PI_2,
                        )))
                    }),
            )
        };
        let dismiss_key = key.clone();
        let dismiss = div()
            .id(("native-docs-search-dismiss", index))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(20.0))
            .rounded(px(4.0))
            .invisible()
            .group_hover(group.clone(), |style| style.visible())
            .hover(move |style| style.bg(hover_bg))
            .child(titlebar_svg_icon("titlebar/x.svg", 13.0, p.row_text))
            .tooltip(|window, cx| search_tooltip("Dismiss", window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.native_docs_dismiss_search_row(dismiss_key.clone(), cx);
            }));
        let base = div()
            .id(("native-docs-search-row", index))
            .group(group.clone())
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(RESULT_ROW_HEIGHT))
            .pl(px(8.0 + INDENT * depth as f32))
            .pr(px(6.0))
            .cursor_pointer()
            .text_color(if selected {
                p.row_text_strong
            } else {
                p.row_text
            })
            .when(selected, |this| this.bg(p.row_surface))
            .when(selected && list_focused, |this| {
                this.border_1().border_color(p.accent.opacity(0.45))
            })
            .when(!selected, |this| {
                this.hover(move |style| style.bg(hover_bg).text_color(strong))
            });
        match row {
            SearchRow::Folder {
                path,
                label,
                count,
                expanded,
                ..
            } => {
                let toggle_key = key.clone();
                base.child(chevron(expanded))
                    .child(
                        div()
                            .flex_none()
                            .w(px(16.0))
                            .opacity(0.75)
                            .child(titlebar_svg_icon(
                                if expanded {
                                    "files-view/t-folder-open-175.svg"
                                } else {
                                    "files-view/t-folder-175.svg"
                                },
                                15.0,
                                p.muted,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(NAME_SIZE))
                            .font_weight(FontWeight::LIGHT)
                            .text_color(p.muted)
                            .child(label),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_end()
                            .h(px(20.0))
                            .min_w(px(20.0))
                            .child(
                                count_badge(count, p)
                                    .group_hover(group.clone(), |style| style.invisible()),
                            )
                            .child(dismiss.absolute().top_0().right_0()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.native_docs_select_search_row(toggle_key.clone(), window, cx);
                        this.native_docs_toggle_search_collapsed(toggle_key.clone(), cx);
                    }))
                    .tooltip(move |window, cx| {
                        search_card_tooltip(&path, ROW_TOOLTIP_GAP, window, cx)
                    })
                    .into_any_element()
            }
            SearchRow::File {
                path,
                count,
                expanded,
                ..
            } => {
                let toggle_key = key.clone();
                let (folder, name) = match path.rsplit_once('/') {
                    Some((folder, name)) => (Some(folder.to_string()), name.to_string()),
                    None => (None, path.clone()),
                };
                let folder = folder.filter(|_| !search.view_as_tree);
                base.child(chevron(expanded))
                    .child(
                        div()
                            .flex_none()
                            .w(px(16.0))
                            .opacity(0.75)
                            .child(titlebar_svg_icon(file_icon(&path), 15.0, p.row_text)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_baseline()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex_none()
                                    .max_w_full()
                                    .truncate()
                                    .text_size(px(NAME_SIZE))
                                    .font_weight(FontWeight::LIGHT)
                                    .child(name),
                            )
                            .when_some(folder, |this, folder| {
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
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_end()
                            .h(px(20.0))
                            .min_w(px(20.0))
                            .child(
                                count_badge(count, p)
                                    .group_hover(group.clone(), |style| style.invisible()),
                            )
                            .child(dismiss.absolute().top_0().right_0()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.native_docs_select_search_row(toggle_key.clone(), window, cx);
                        this.native_docs_toggle_search_collapsed(toggle_key.clone(), cx);
                    }))
                    .tooltip(move |window, cx| {
                        search_card_tooltip(&path, ROW_TOOLTIP_GAP, window, cx)
                    })
                    .into_any_element()
            }
            SearchRow::Match {
                path, index: at, ..
            } => {
                let Some(found) = search.file(&path).and_then(|file| file.matches.get(at)) else {
                    return base.into_any_element();
                };
                let text = format!("{}{}{}", found.before, found.matched, found.after);
                let start = found.before.len();
                let end = start + found.matched.len();
                let highlight = HighlightStyle {
                    background_color: Some(match_highlight()),
                    color: Some(p.row_text_strong),
                    ..Default::default()
                };
                let line_label: SharedString = format!("{}:{}", path, found.line + 1).into();
                let open_key = key.clone();
                base.child(div().flex_none().w(px(14.0)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(LINE_SIZE))
                            .child(
                                StyledText::new(text).with_highlights([(start..end, highlight)]),
                            ),
                    )
                    .child(dismiss)
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            this.native_docs_select_search_row(open_key.clone(), window, cx);
                            this.native_docs_open_search_match(
                                &path,
                                at,
                                event.click_count() >= 2,
                                cx,
                            );
                        }),
                    )
                    .tooltip(move |window, cx| {
                        search_card_tooltip(&line_label, ROW_TOOLTIP_GAP, window, cx)
                    })
                    .into_any_element()
            }
        }
    }
}
