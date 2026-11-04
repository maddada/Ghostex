//! A Markdown document's scrolling body: the gutter beside the live editor in the Docs text
//! column, the merge-conflict actions over it, and the git overview ruler beside it. Free of the
//! app so the native Docs preview (`src/bin/native_docs_demo.rs`) draws the same body the Files
//! view does.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Div, Entity, InteractiveElement as _, ParentElement as _, ScrollHandle,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, div, px,
};
use zorite_editor::EditorState;

use super::gutter::LineChange;
use super::palette::DocsPalette;

/// `MANAGE_MEO_CONTENT_MAX_WIDTH`: the document's text column.
pub(crate) const CONTENT_MAX_WIDTH: f32 = 800.0;
/// The column's padding above the first line.
pub(crate) const BODY_TOP_PAD: f32 = 14.0;
/// The column's padding below the last line: the Docs page's 72px (room under the formatting
/// bar) plus scroll past the end.
pub(crate) const BODY_BOTTOM_PAD: f32 = 72.0 + 60.0;

thread_local! {
    /// The open document's scroll offset at its last paint (one Markdown body draws at a time).
    static LAST_SCROLL: std::cell::Cell<f32> = const { std::cell::Cell::new(f32::NAN) };
    /// The table sort the table controls last applied (one Markdown body draws at a time).
    static TABLE_SORT: super::table_tools::TableSort = std::rc::Rc::default();
    /// The text column's width on the last frame no panel was sliding (one Markdown body draws at
    /// a time).
    static SETTLED_COLUMN: std::cell::Cell<f32> = const { std::cell::Cell::new(f32::NAN) };
}

/// The width the text column keeps this frame, when a panel slides and the body's width would
/// change it.
///
/// CDXC:Docs 2026-10-10 WHY: the editor lays its document out again whenever its column's width changes, so a long file (AGENTS.md) beside a sliding panel, the side panel itself or the sidebar, was laid out on every frame of the slide and made it stutter. The column holds the width it had before the slide, centred, and takes the new one once on the settling frame, as the chat's rows and the terminals do.
fn held_column_width(available: f32, natural: f32, sliding: bool) -> Option<f32> {
    if sliding {
        let settled = SETTLED_COLUMN.with(std::cell::Cell::get);
        return (settled.is_finite() && (settled - natural).abs() > 0.5).then_some(settled);
    }
    if available > 0.0 {
        SETTLED_COLUMN.with(|cell| cell.set(natural));
    }
    None
}

pub(crate) struct DocsMarkdownBody<'a> {
    pub(crate) id: SharedString,
    pub(crate) live: &'a Entity<EditorState>,
    pub(crate) scroll: &'a ScrollHandle,
    pub(crate) source: bool,
    pub(crate) line_numbers: bool,
    pub(crate) constrain: bool,
    /// The git stripe's per-line changes, when Git Changes is on and the file has a HEAD version.
    pub(crate) changes: Option<&'a (Vec<LineChange>, Vec<usize>)>,
    /// What a hovered table's actions do (`table_tools::render_table_actions`).
    pub(crate) table_actions: &'a super::table_tools::TableActionHost,
    /// Whether a window panel is sliding (the app's `terminal_element::grid_resize_held`): the
    /// text column then keeps the width it had before the slide.
    pub(crate) sliding: bool,
}

/// The scrolling body. The caller adds its key handling and puts the formatting bar and
/// `render_overview_ruler` over it.
pub(crate) fn render_markdown_body(
    body: DocsMarkdownBody<'_>,
    p: &DocsPalette,
    window: &gpui::Window,
    cx: &App,
) -> Stateful<Div> {
    let text_color = super::editor_style::body_color(p);
    let state = body.live.read(cx);
    let rows = state.row_layout();
    let text = state.text();
    let caret = state.cursor().min(text.len());
    let caret_line = text[..caret].bytes().filter(|byte| *byte == b'\n').count();
    // Source mode shows the fold lane empty, like the Docs page.
    let folds: Vec<(usize, bool)> = if body.source {
        Vec::new()
    } else {
        state
            .heading_folds()
            .into_iter()
            .map(|(row, _, folded)| (row, folded))
            .collect()
    };
    // The viewport in the gutter's y (it starts below the column's top pad), one screen of margin
    // each side.
    let band = {
        let viewport = body.scroll.bounds().size.height;
        let top = -body.scroll.offset().y - px(BODY_TOP_PAD);
        (viewport > px(0.0)).then(|| (top - viewport, top + viewport * 2.0))
    };
    super::gutter::measure_number_digits(window);
    let gutter = super::gutter::render(
        super::gutter::GutterModel {
            rows: &rows,
            caret_line,
            numbers: body.line_numbers,
            changes: body.changes,
            band,
            folds: &folds,
            live: Some(body.live),
        },
        p,
        text_color,
    );
    let conflicts = super::conflicts::render_conflict_actions(body.live, &rows, p, cx);
    let table_tools = (!body.source)
        .then(|| {
            let sort = TABLE_SORT.with(Clone::clone);
            super::table_tools::render_table_tools(body.live, &rows, &sort, p, window, cx)
        })
        .flatten();
    let table_actions = (!body.source)
        .then(|| super::table_tools::render_table_actions(body.live, body.table_actions, p, cx))
        .flatten();
    // CDXC:Docs 2026-09-28 WHY: the gutter is drawn from the editor's row layout as of its last
    // paint, so when a diagram, formula or image finishes rendering (or a fold changes a row's
    // height) this frame's gutter still has the old rows. The editor also shapes only the rows near
    // the viewport it saw one frame earlier, so a scroll settles its row heights a frame late. After
    // the editor paints, a changed layout or a scroll asks for one more frame, so the numbers, git
    // stripes and conflict buttons move with the rows.
    let settle = {
        let live = body.live.clone();
        let drawn = rows.clone();
        let scroll = body.scroll.clone();
        gpui::canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                let current = live.update(cx, |editor, _| {
                    // The Docs page has no block-drag grip, and draws the heading chevrons in
                    // its own fold lane (the gutter's).
                    editor.set_block_grip(false);
                    editor.set_heading_chevrons(false);
                    // A wide table's scroll bar stays above the formatting bar.
                    editor.set_table_scrollbar_inset(px(BODY_BOTTOM_PAD - 60.0));
                    editor.row_layout()
                });
                let offset = f32::from(scroll.offset().y);
                let scrolled = LAST_SCROLL.with(|last| last.replace(offset)) != offset;
                if current != drawn || scrolled {
                    window.refresh();
                }
            },
        )
        .absolute()
        .size_full()
    };
    let focus_target = body.live.clone();
    let gutter_width = super::gutter::gutter_width(body.line_numbers, rows.len());
    let available = f32::from(body.scroll.bounds().size.width);
    // The reading column grows with the Files zoom, so a zoomed document keeps its line length.
    let content_max_width = CONTENT_MAX_WIDTH * crate::ui_fonts::files_zoom_scale();
    let natural = if body.constrain {
        available.min(content_max_width + gutter_width)
    } else {
        available
    };
    let held = held_column_width(available, natural, body.sliding);
    div()
        .id(body.id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(body.scroll)
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            focus_target.update(cx, |editor, cx| editor.focus(window, cx));
        })
        .child(
            div().w_full().flex().justify_center().child(
                div()
                    .w_full()
                    .when(body.constrain, |row| {
                        row.max_w(px(content_max_width + gutter_width))
                    })
                    .when_some(held, |row, width| {
                        row.flex_shrink_0().w(px(width)).max_w(px(width))
                    })
                    .pt(px(BODY_TOP_PAD))
                    .pb(px(BODY_BOTTOM_PAD))
                    .pr(px(12.0))
                    .flex()
                    .items_start()
                    .child(gutter)
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(crate::ui_fonts::files_text_size()))
                            .text_color(text_color)
                            .font_family(if body.source {
                                super::fonts::DOCS_MONO
                            } else {
                                super::fonts::DOCS_FONT
                            })
                            .child(body.live.clone())
                            .children(conflicts)
                            .children(table_tools)
                            .children(table_actions)
                            .child(settle),
                    ),
            ),
        )
}

/// The git overview ruler for the open document, laid over the body's viewport (the caller's
/// relative container), when git changes show.
pub(crate) fn render_overview_ruler(
    live: &Entity<EditorState>,
    scroll: &ScrollHandle,
    changes: Option<&(Vec<LineChange>, Vec<usize>)>,
    p: &DocsPalette,
    cx: &App,
) -> Option<AnyElement> {
    let changes = changes?;
    let rows = live.read(cx).row_layout();
    let last = rows.last().map_or(px(0.), |(top, height)| *top + *height);
    let content_bottom = last + px(BODY_TOP_PAD);
    let track = scroll.bounds().size.height;
    let scroll_height = (content_bottom + px(BODY_BOTTOM_PAD)).max(track);
    super::gutter::render_overview_ruler(
        changes,
        content_bottom,
        scroll_height,
        track,
        p,
        super::editor_style::body_color(p),
    )
}

/// The body's vertical scrollbar, for the caller's relative container: beside the scroll area
/// (never inside it, where the track would move with the content and a drag would lose the thumb).
///
/// CDXC:Docs 2026-10-15 DECISION:
/// User chose to keep the scrollbar always shown when the file scrolls, with no track or line, and nothing drawn when the whole file fits. This refines the 2026-10-09 decision ("please always show the scrollbar in the gpui file editor"), which kept the Files editor's vertical scrollbar visible for Markdown, text and code files instead of only on hover or while scrolling. gpui-component's Always mode already hides the bar when the content fits and the app theme's track is transparent; the text and code editor sets the same mode on its own scrollbar.
///
/// CDXC:Docs 2026-10-11 SEE-ALSO:
/// The chat transcript's scrollbar (`transcript_scrollbar` in native_chat/scrollbar.rs) must match this one exactly, so both leave gpui-component's geometry and the app theme's thumb colours alone; a width, inset or colour set here has to be set there too.
pub(crate) fn render_body_scrollbar(scroll: &ScrollHandle) -> gpui_component::scroll::Scrollbar {
    gpui_component::scroll::Scrollbar::vertical(scroll)
        .mode(gpui_component::scroll::ScrollbarMode::Always)
}
