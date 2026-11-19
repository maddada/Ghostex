//! The table controls over the live editor, ported from the former React Docs page: while the
//! caret is in a table, a six-button toolbar at the table's
//! top left (Insert row above / below, Insert column left / right, Delete row / column) and a sort
//! button on each header cell. Sorting rewrites the table's rows (one undo step); the Docs page
//! sorted the view first and wrote it with "Apply Sort". While the pointer is over a table, its
//! top right shows the chat's table actions (Open in window, copy as Markdown, copy as CSV).

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Entity, Focusable as _, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, px, svg,
};
use zorite_editor::EditorState;

use super::palette::DocsPalette;

/// The active sort: (header row, column, descending).
pub(crate) type TableSort = Rc<Cell<Option<(usize, usize, bool)>>>;

/// What the hover actions do with a table's Markdown: copy it (with the app's copy feedback) and,
/// when the host has one, open it in the Markdown table window.
#[derive(Clone)]
pub(crate) struct TableActionHost {
    pub(crate) copy: Rc<dyn Fn(String, &mut App)>,
    pub(crate) open: Option<Rc<dyn Fn(String, &mut App)>>,
}

thread_local! {
    /// The header row of the table whose actions the pointer is on, which keeps them up after the
    /// pointer leaves the table for them (one Markdown body draws at a time).
    static ACTIONS_HOVERED: Cell<Option<usize>> = const { Cell::new(None) };
}

const ACTION_HEIGHT: f32 = 22.0;

const TOOLBAR_BUTTON: f32 = 24.0;

fn tooltip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    move |window, cx| gpui_component::tooltip::Tooltip::new(text).build(window, cx)
}

/// The controls for the caret's table, in the editor's own coordinates (the rows the gutter
/// uses); `None` when the caret is not in a rendered table or the editor has no focus.
pub(crate) fn render_table_tools(
    live: &Entity<EditorState>,
    rows: &[(Pixels, Pixels)],
    sort: &TableSort,
    p: &DocsPalette,
    window: &Window,
    cx: &App,
) -> Option<AnyElement> {
    let editor = live.read(cx);
    if !editor.focus_handle(cx).is_focused(window) {
        return None;
    }
    let (header, _end, columns, body_rows, on_body) = editor.caret_table()?;
    let (header_top, _) = *rows.get(header)?;
    let cells = editor.table_header_cells(header);
    let text = super::editor_style::body_color(p);
    let danger = if p.light {
        gpui::rgb(0xd96c56).into()
    } else {
        gpui::rgb(0xe07d6a).into()
    };
    let hover = p.control_hover;
    let button = |id: &'static str,
                  icon: &'static str,
                  label: &'static str,
                  color: gpui::Hsla,
                  enabled: bool,
                  action: fn(&mut EditorState, &mut gpui::Context<EditorState>)| {
        let live = live.clone();
        div()
            .id(id)
            .size(px(TOOLBAR_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        live.update(cx, action);
                    })
            })
            .when(!enabled, |this| this.opacity(0.45))
            .tooltip(tooltip(label))
            .child(svg().path(icon).size(px(18.0)).text_color(color))
    };
    let toolbar = div()
        .absolute()
        .top(header_top - px(TOOLBAR_BUTTON))
        .left_0()
        .flex()
        .items_center()
        .gap(px(2.0))
        .child(button(
            "docs-table-row-above",
            "files-view/t-row-insert-top-2.svg",
            "Insert row above",
            text,
            true,
            |editor, cx| editor.insert_table_row(false, cx),
        ))
        .child(button(
            "docs-table-row-below",
            "files-view/t-row-insert-bottom-2.svg",
            "Insert row below",
            text,
            true,
            |editor, cx| editor.insert_table_row(true, cx),
        ))
        .child(button(
            "docs-table-column-left",
            "files-view/t-column-insert-left-2.svg",
            "Insert column left",
            text,
            true,
            |editor, cx| editor.insert_table_column(false, cx),
        ))
        .child(button(
            "docs-table-column-right",
            "files-view/t-column-insert-right-2.svg",
            "Insert column right",
            text,
            true,
            |editor, cx| editor.insert_table_column(true, cx),
        ))
        .child(button(
            "docs-table-delete-row",
            "files-view/t-row-remove-2.svg",
            "Delete row",
            danger,
            on_body && body_rows > 1,
            |editor, cx| editor.delete_table_row(cx),
        ))
        .child(button(
            "docs-table-delete-column",
            "files-view/t-column-remove-2.svg",
            "Delete column",
            danger,
            columns > 1,
            |editor, cx| editor.delete_table_column(cx),
        ));
    // A sort button at each header cell's right end.
    let active = sort.get().filter(|(row, ..)| *row == header);
    let sort_buttons: Vec<AnyElement> = cells
        .iter()
        .enumerate()
        .map(|(col, cell)| {
            let state = active.filter(|(_, c, _)| *c == col).map(|(.., desc)| desc);
            let (icon, label): (&'static str, SharedString) = match state {
                Some(true) => (
                    "files-view/t-arrow-down-2.svg",
                    "Sorted descending; click to toggle".into(),
                ),
                Some(false) => (
                    "files-view/t-arrow-up-2.svg",
                    "Sorted ascending; click to toggle".into(),
                ),
                None => (
                    "files-view/t-arrows-sort-2.svg",
                    "Sort column descending".into(),
                ),
            };
            let live = live.clone();
            let sort = sort.clone();
            div()
                .id(("docs-table-sort", col))
                .absolute()
                .top(cell.origin.y + (cell.size.height - px(18.0)) / 2.0)
                .left(cell.origin.x + cell.size.width - px(22.0))
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .when(state.is_none(), |this| this.opacity(0.55))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                })
                .child(svg().path(icon).size(px(14.0)).text_color(text))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    // The first click sorts descending, the next ones toggle.
                    let descending = match sort.get() {
                        Some((row, c, desc)) if row == header && c == col => !desc,
                        _ => true,
                    };
                    sort.set(Some((header, col, descending)));
                    live.update(cx, |editor, cx| {
                        editor.sort_table(header, col, descending, cx)
                    });
                })
                .into_any_element()
        })
        .collect();
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(toolbar)
            .children(sort_buttons)
            .into_any_element(),
    )
}

/// The hovered table's actions at its top right, in the editor's own coordinates: Open in window,
/// copy as Markdown and copy as CSV, the chat's table toolbar (`table_actions` in
/// native_chat/rich_markdown.rs) with the same icons and labels.
///
/// CDXC:Docs 2026-10-09 DECISION:
/// User: "add ability to copy as md/csv and button to show in a pop up window". Hovering a table in the Files editor shows the chat's table actions over its top right corner: the external-link button opens the table in the Markdown table window, and the MD and CSV buttons copy it as Markdown or as CSV (the chat's CSV conversion, `helpers/markdown_table_csv.rs`). Source mode shows no table, so no actions.
pub(crate) fn render_table_actions(
    live: &Entity<EditorState>,
    host: &TableActionHost,
    p: &DocsPalette,
    cx: &App,
) -> Option<AnyElement> {
    let editor = live.read(cx);
    let header = editor
        .hovered_table()
        .map(|(header, _)| header)
        .or_else(|| ACTIONS_HOVERED.with(Cell::get))?;
    let source = editor.table_markdown(header)?;
    let cells = editor.table_header_cells(header);
    let (first, last) = (cells.first()?, cells.last()?);
    // The table's visible right edge: a wide table scrolled sideways still ends at the editor's.
    let right = editor
        .hovered_table()
        .filter(|(row, _)| *row == header)
        .map_or(last.right(), |(_, zone)| last.right().min(zone.right()));
    let text = p.muted;
    let hover = p.control_hover;
    let action = |id: &'static str, label: &'static str, format: Option<&'static str>| {
        div()
            .id(id)
            .h(px(ACTION_HEIGHT))
            .flex()
            .items_center()
            .gap(px(3.0))
            .px(px(if format.is_some() { 5.0 } else { 4.0 }))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .tooltip(tooltip(label))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(
                svg()
                    .path(if format.is_some() {
                        "titlebar/copy.svg"
                    } else {
                        "titlebar/external-link.svg"
                    })
                    .size(px(14.0))
                    .text_color(text)
                    .flex_shrink_0(),
            )
            .when_some(format, |this, format| {
                this.child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(text)
                        .child(format),
                )
            })
    };
    let open = host.open.clone().map(|open| {
        let source = source.clone();
        action("docs-table-open", "Open in window", None)
            .on_click(move |_, _, cx| open(source.clone(), cx))
    });
    let markdown = {
        let (copy, source) = (host.copy.clone(), source.clone());
        action("docs-table-copy-md", "Copy as Markdown", Some("MD"))
            .on_click(move |_, _, cx| copy(source.clone(), cx))
    };
    let csv = {
        let copy = host.copy.clone();
        action("docs-table-copy-csv", "Copy as CSV", Some("CSV")).on_click(move |_, _, cx| {
            copy(
                crate::app::helpers::markdown_table_csv::table_csv(&source),
                cx,
            )
        })
    };
    Some(
        div()
            .absolute()
            // Clear of the column pill zorite draws on the header's top border.
            .top(first.origin.y - px(ACTION_HEIGHT + 12.0))
            .left_0()
            .w(right)
            .flex()
            .justify_end()
            .child(
                div()
                    .id("docs-table-actions")
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .p(px(1.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(p.border)
                    // CDXC:Theming 2026-11-20 (decision in helpers/chrome_palette.rs): lifted
                    // under glass, not the near-black #161616.
                    .bg(p.glass_lifted(p.raised))
                    .on_hover(move |hovered, window, _| {
                        ACTIONS_HOVERED.with(|cell| cell.set(hovered.then_some(header)));
                        window.refresh();
                    })
                    .children(open)
                    .child(markdown)
                    .child(csv),
            )
            .into_any_element(),
    )
}
