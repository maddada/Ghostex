use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
use gpui::{
    AnyElement, Div, ElementId, Hsla, IntoElement, ParentElement as _, Styled as _, div, px,
    relative,
};

/// The layout a view's skeleton sketches while the view is loading.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewSkeletonKind {
    /// An editor: activity bar, file tree, tab strip, indented code lines.
    Code,
    /// Docs: a document list beside a titled page of paragraphs.
    Docs,
    /// A board of columns holding cards.
    Kanban,
    /// A list of automation rows with a toolbar.
    Automate,
    /// Storybook: a story tree beside a canvas with its toolbar.
    Storybook,
    /// Any other web page: a header, a hero block, a grid of cards.
    Web,
    /// A terminal: a prompt and lines of output.
    Terminal,
}

struct SkeletonPaint {
    fill: Hsla,
    /// A faint wash for chrome that sits on a panel (menu bar, sidebars, status bar).
    panel: Hsla,
    /// Dividers between chrome regions.
    hairline: Hsla,
    background: Hsla,
}

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-19 DECISION:
    /// User: every titlebar view shows a skeleton that looks nice while it is loading, instead of a "Loading …" sentence; Storybook gets whatever fits it best.
    /// Each view sketches its own layout in the shared skeleton tint and pulse, on the same background its real page paints on, so the switch from skeleton to page reads as the page filling in rather than a flash.
    pub(crate) fn view_skeleton_kind_for_mode(&self, mode: TitlebarMode) -> ViewSkeletonKind {
        match mode {
            TitlebarMode::Source => ViewSkeletonKind::Code,
            TitlebarMode::Manage => ViewSkeletonKind::Docs,
            TitlebarMode::Kanban => ViewSkeletonKind::Kanban,
            TitlebarMode::Automate
            | TitlebarMode::BotFeed
            | TitlebarMode::Changes
            | TitlebarMode::SideChat
            | TitlebarMode::Work => ViewSkeletonKind::Automate,
            TitlebarMode::Extension(id) if id.as_str() == "storybook" => {
                ViewSkeletonKind::Storybook
            }
            TitlebarMode::Terminal => ViewSkeletonKind::Terminal,
            TitlebarMode::Extension(_) | TitlebarMode::Agents | TitlebarMode::Browser => {
                ViewSkeletonKind::Web
            }
        }
    }

    /// The skeleton for a workarea view, on that view's own background.
    pub(crate) fn render_view_skeleton(&self, mode: TitlebarMode) -> AnyElement {
        let light = CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed);
        let background: Hsla = if mode == TitlebarMode::Source {
            source_view_background_color()
        } else if light {
            gpui::rgb(0xffffff).into()
        } else {
            workspace_background_color()
        };
        render_view_skeleton(
            self.view_skeleton_kind_for_mode(mode),
            format!("view-skeleton-{}", mode.element_slug()),
            background,
        )
    }
}

/// A skeleton of the given kind filling its container.
pub(crate) fn render_view_skeleton(
    kind: ViewSkeletonKind,
    id: impl Into<ElementId>,
    background: Hsla,
) -> AnyElement {
    let fill: Hsla = chrome_color(0xe5e8ec, 0x111111)
        .opacity(crate::app::session_chat_skeleton::skeleton_tint())
        .into();
    let paint = SkeletonPaint {
        fill,
        panel: fill.opacity(0.3),
        hairline: fill.opacity(0.6),
        background,
    };
    let body = match kind {
        ViewSkeletonKind::Code => code_skeleton(&paint),
        ViewSkeletonKind::Docs => docs_skeleton(&paint),
        ViewSkeletonKind::Kanban => kanban_skeleton(&paint),
        ViewSkeletonKind::Automate => automate_skeleton(&paint),
        ViewSkeletonKind::Storybook => storybook_skeleton(&paint),
        ViewSkeletonKind::Web => web_skeleton(&paint),
        ViewSkeletonKind::Terminal => terminal_skeleton(&paint),
    };
    let body = body
        .id(id)
        .role(gpui::Role::Status)
        .aria_label("Loading…")
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .bg(paint.background);
    if gpui_macos_reduce_motion_enabled() {
        return body.into_any_element();
    }
    let (period, min) = crate::app::session_chat_skeleton::skeleton_pulse();
    body.with_throttled_animation("view-skeleton-pulse", period, move |body, frame| {
        let dip = ease_in_out(if frame < 0.5 {
            frame * 2.0
        } else {
            (1.0 - frame) * 2.0
        });
        body.opacity(1.0 - (1.0 - min) * dip)
    })
    .into_any_element()
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// A text line: a rounded bar of the given relative width.
fn line(paint: &SkeletonPaint, width: f32, height: f32) -> Div {
    div()
        .w(relative(width))
        .h(px(height))
        .flex_shrink_0()
        .rounded_full()
        .bg(paint.fill)
}

/// A fixed-width bar, for toolbars and tabs.
fn pill(paint: &SkeletonPaint, width: f32, height: f32) -> Div {
    div()
        .w(px(width))
        .h(px(height))
        .flex_shrink_0()
        .rounded_full()
        .bg(paint.fill)
}

/// A square glyph slot.
fn glyph(paint: &SkeletonPaint, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(size * 0.28))
        .bg(paint.fill)
}

/// A card or panel block.
fn block(paint: &SkeletonPaint, height: f32, radius: f32) -> Div {
    div()
        .w_full()
        .h(px(height))
        .flex_shrink_0()
        .rounded(px(radius))
        .bg(paint.fill)
}

/// A column of tree rows with nesting, like a file or story tree.
fn tree(paint: &SkeletonPaint, rows: &[(u8, f32)]) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .children(rows.iter().map(|(depth, width)| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .pl(px(f32::from(*depth) * 16.0))
                .child(glyph(paint, 12.0))
                .child(line(paint, *width, 10.0))
        }))
}

/// Rough glyph advance of the editor font, so code bars read as lines of text.
const CODE_CHAR: f32 = 7.5;
/// One editor or tree row.
const ROW_HEIGHT: f32 = 22.0;
/// Code-server's Explorer width.
const EXPLORER_WIDTH: f32 = 236.0;
/// The Docs files list's default width (`MANAGE_SIDEBAR_DEFAULT_WIDTH`).
const DOCS_LIST_WIDTH: f32 = 292.0;

/// Code lines as (indent level, length in characters, starts a fold).
const CODE_LINES: [(u8, u8, bool); 62] = [
    (0, 1, true),
    (1, 150, false),
    (1, 18, false),
    (1, 24, false),
    (1, 17, true),
    (2, 15, false),
    (2, 20, false),
    (2, 14, false),
    (2, 20, false),
    (2, 16, false),
    (2, 22, false),
    (2, 14, false),
    (2, 18, false),
    (2, 23, false),
    (1, 2, false),
    (1, 11, true),
    (2, 11, true),
    (3, 22, false),
    (3, 24, false),
    (3, 32, false),
    (3, 34, false),
    (3, 68, false),
    (3, 22, false),
    (3, 14, true),
    (4, 18, false),
    (4, 17, false),
    (4, 16, false),
    (4, 62, false),
    (3, 2, false),
    (3, 11, true),
    (4, 1, true),
    (5, 17, false),
    (5, 20, false),
    (5, 19, false),
    (5, 70, false),
    (5, 17, false),
    (4, 2, false),
    (4, 1, true),
    (5, 19, false),
    (5, 23, false),
    (5, 32, false),
    (5, 90, false),
    (5, 15, false),
    (5, 16, false),
    (4, 2, false),
    (4, 1, true),
    (5, 17, false),
    (5, 16, false),
    (5, 19, false),
    (5, 48, false),
    (5, 16, false),
    (4, 2, false),
    (4, 1, true),
    (5, 18, false),
    (5, 17, false),
    (5, 21, false),
    (5, 36, false),
    (5, 17, false),
    (4, 2, false),
    (4, 1, true),
    (5, 17, false),
    (5, 19, false),
];

/// Explorer entries as (is folder, label width, has changes, is the open file).
const EXPLORER_ROWS: [(bool, f32, bool, bool); 38] = [
    (true, 42.0, false, false),
    (true, 36.0, false, false),
    (true, 38.0, false, false),
    (true, 40.0, false, false),
    (true, 50.0, false, false),
    (true, 82.0, true, false),
    (true, 50.0, false, false),
    (true, 56.0, false, false),
    (true, 12.0, false, false),
    (true, 28.0, true, false),
    (true, 52.0, false, false),
    (true, 30.0, false, false),
    (true, 28.0, false, false),
    (true, 36.0, false, false),
    (true, 56.0, true, false),
    (true, 40.0, false, false),
    (true, 30.0, true, false),
    (true, 104.0, false, false),
    (true, 22.0, false, false),
    (true, 40.0, false, false),
    (false, 82.0, false, false),
    (false, 64.0, false, false),
    (false, 74.0, false, false),
    (false, 58.0, false, false),
    (false, 70.0, false, false),
    (false, 148.0, false, true),
    (false, 64.0, false, false),
    (false, 70.0, false, false),
    (false, 50.0, false, false),
    (false, 88.0, false, false),
    (false, 70.0, false, false),
    (false, 96.0, false, false),
    (false, 100.0, false, false),
    (false, 60.0, false, false),
    (false, 50.0, false, false),
    (false, 72.0, false, false),
    (false, 106.0, false, false),
    (false, 64.0, false, false),
];

/// A short text bar, the height of small UI type.
fn text(paint: &SkeletonPaint, width: f32) -> Div {
    pill(paint, width, 7.0)
}

/// A dense file-tree row: a chevron for folders or an icon for files, then the name.
fn tree_row(paint: &SkeletonPaint, depth: u8, folder: bool, width: f32) -> Div {
    div()
        .h(px(ROW_HEIGHT))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(7.0))
        .pl(px(14.0 + f32::from(depth) * 12.0))
        .pr(px(14.0))
        .child(glyph(paint, if folder { 8.0 } else { 11.0 }))
        .child(text(paint, width))
}

/// A fixed-height chrome strip (menu bar, tab strip, header, status bar).
fn strip(paint: &SkeletonPaint, height: f32) -> Div {
    div()
        .h(px(height))
        .flex_shrink_0()
        .flex()
        .items_center()
        .bg(paint.panel)
}

/// The window's menu bar: menus, history arrows, the centered command box, layout toggles.
fn code_menu_bar(paint: &SkeletonPaint) -> Div {
    let menus = div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(17.0))
        .pl(px(12.0))
        .pr(px(18.0))
        .overflow_hidden()
        .children(
            [20.0, 22.0, 50.0, 28.0, 16.0, 22.0, 50.0, 26.0]
                .into_iter()
                .map(|width| text(paint, width)),
        )
        .child(div().flex_1())
        .child(glyph(paint, 11.0))
        .child(glyph(paint, 11.0));
    let command_center = div()
        .w(relative(0.37))
        .max_w(px(600.0))
        .h(px(21.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(10.0))
        .rounded(px(5.0))
        .border_1()
        .border_color(paint.fill)
        .child(text(paint, 44.0))
        .child(div().flex_1())
        .child(glyph(paint, 12.0));
    let toggles = div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(13.0))
        .pr(px(12.0))
        .overflow_hidden()
        .children((0..6).map(|_| glyph(paint, 13.0)));
    strip(paint, 31.0)
        .border_b_1()
        .border_color(paint.hairline)
        .child(menus)
        .child(command_center)
        .child(toggles)
}

/// The status bar: branch and problems on the left, cursor, indentation, encoding and language on the right.
fn code_status_bar(paint: &SkeletonPaint) -> Div {
    strip(paint, 22.0)
        .gap(px(14.0))
        .px(px(12.0))
        .border_t_1()
        .border_color(paint.hairline)
        .overflow_hidden()
        .child(glyph(paint, 10.0))
        .child(text(paint, 42.0))
        .child(text(paint, 36.0))
        .child(text(paint, 22.0))
        .child(div().flex_1())
        .children(
            [186.0, 64.0, 58.0, 38.0, 14.0, 112.0, 72.0]
                .into_iter()
                .map(|width| text(paint, width)),
        )
        .child(glyph(paint, 10.0))
}

/// The editor group: one open tab, the breadcrumb row, then numbered code with fold chevrons and a scrollbar.
fn code_editor(paint: &SkeletonPaint) -> Div {
    let tabs = strip(paint, 35.0)
        .child(
            div()
                .w(px(248.0))
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .bg(paint.background)
                .child(glyph(paint, 10.0))
                .child(text(paint, 120.0)),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .gap(px(14.0))
                .pr(px(14.0))
                .child(glyph(paint, 13.0))
                .child(glyph(paint, 13.0)),
        );
    let breadcrumbs = div()
        .h(px(ROW_HEIGHT))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(9.0))
        .pl(px(18.0))
        .overflow_hidden()
        .children(
            [
                (true, 118.0),
                (true, 38.0),
                (true, 34.0),
                (true, 38.0),
                (false, 8.0),
            ]
            .into_iter()
            .flat_map(|(chevron, width)| {
                [
                    Some(glyph(paint, 9.0)),
                    Some(text(paint, width)),
                    chevron.then(|| glyph(paint, 6.0)),
                ]
            })
            .flatten(),
        );
    let lines = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .pt(px(4.0))
        .overflow_hidden()
        .children(
            CODE_LINES
                .iter()
                .enumerate()
                .map(|(index, (indent, chars, fold))| {
                    let number_width = if index + 1 < 10 { 6.0 } else { 12.0 };
                    div()
                        .h(px(17.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .w(px(46.0))
                                .flex_shrink_0()
                                .flex()
                                .justify_end()
                                .child(text(paint, number_width)),
                        )
                        .child(
                            div()
                                .w(px(20.0))
                                .flex_shrink_0()
                                .flex()
                                .justify_center()
                                .when(*fold, |slot| slot.child(glyph(paint, 7.0))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .pl(px(f32::from(*indent) * CODE_CHAR * 2.0))
                                .overflow_hidden()
                                .child(text(paint, f32::from(*chars) * CODE_CHAR)),
                        )
                }),
        );
    let scrollbar = div()
        .w(px(14.0))
        .h_full()
        .flex_shrink_0()
        .flex()
        .justify_center()
        .pt(px(4.0))
        .child(
            div()
                .w(px(10.0))
                .h(px(104.0))
                .rounded(px(2.0))
                .bg(paint.fill),
        );
    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .child(tabs)
        .child(breadcrumbs)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(lines)
                .child(scrollbar),
        )
}

/// The Explorer on the right: view switcher icons, the EXPLORER header, the workspace tree, then collapsed Outline and Timeline.
fn code_explorer(paint: &SkeletonPaint) -> Div {
    let switcher = div()
        .h(px(35.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(14.0))
        .pl(px(12.0))
        .children((0..6).map(|_| glyph(paint, 15.0)));
    let header = div()
        .h(px(30.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .pl(px(20.0))
        .pr(px(16.0))
        .child(text(paint, 58.0))
        .child(div().flex_1())
        .child(pill(paint, 12.0, 4.0));
    let entries = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(tree_row(paint, 0, true, 56.0))
        .children(EXPLORER_ROWS.iter().map(|(folder, width, changed, open)| {
            tree_row(paint, 1, *folder, *width)
                .when(*open, |row| row.bg(paint.fill))
                .when(*changed, |row| {
                    row.child(div().flex_1())
                        .child(glyph(paint, 6.0).rounded_full())
                })
        }));
    let section = |width: f32| {
        div()
            .h(px(ROW_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(7.0))
            .pl(px(12.0))
            .border_t_1()
            .border_color(paint.hairline)
            .child(glyph(paint, 8.0))
            .child(text(paint, width))
    };
    div()
        .w(px(EXPLORER_WIDTH))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .bg(paint.panel)
        .border_l_1()
        .border_color(paint.hairline)
        .child(switcher)
        .child(header)
        .child(entries)
        .child(section(56.0))
        .child(section(62.0))
}

/// CDXC:Workarea 2026-09-19 DECISION:
/// User: the Code and Docs loading skeletons should match the real Code view more closely (their code-server screenshot).
/// Code sketches code-server's own frame: menu bar with the command box, a single tab and breadcrumbs over line-numbered code, the Explorer on the right, and the status bar. Docs uses the same vocabulary for its own frame (35px header strip, files list on the right) so both read as the page filling in.
fn code_skeleton(paint: &SkeletonPaint) -> Div {
    div()
        .flex()
        .flex_col()
        .child(code_menu_bar(paint))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(code_editor(paint))
                .child(code_explorer(paint)),
        )
        .child(code_status_bar(paint))
}

/// Docs: the document under its title header, the files list docked on the right (search, open files, Project Docs tree).
fn docs_skeleton(paint: &SkeletonPaint) -> Div {
    let header = strip(paint, 35.0)
        .gap(px(8.0))
        .px(px(16.0))
        .border_b_1()
        .border_color(paint.hairline)
        .child(glyph(paint, 11.0))
        .child(text(paint, 160.0))
        .child(div().flex_1())
        .children((0..5).map(|_| glyph(paint, 14.0)));
    let paragraphs: [&[f32]; 5] = [
        &[0.96, 0.9, 0.62],
        &[0.88, 0.97, 0.93, 0.4],
        &[0.94, 0.86],
        &[0.9, 0.98, 0.7, 0.55],
        &[0.92, 0.84, 0.48],
    ];
    let body = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .pt(px(32.0))
        .px(px(48.0))
        .gap(px(26.0))
        .overflow_hidden()
        .child(line(paint, 0.38, 20.0))
        .children(paragraphs.iter().enumerate().map(|(index, widths)| {
            div()
                .w_full()
                .max_w(px(760.0))
                .flex()
                .flex_col()
                .gap(px(11.0))
                .when(index == 2, |section| {
                    section.child(div().pb(px(4.0)).child(line(paint, 0.26, 14.0)))
                })
                .children(widths.iter().map(|width| line(paint, *width, 9.0)))
        }));
    let actions = div()
        .h(px(35.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(12.0))
        .children((0..6).map(|_| glyph(paint, 14.0)))
        .child(div().flex_1())
        .child(glyph(paint, 14.0));
    let search = div()
        .mx(px(10.0))
        .mb(px(8.0))
        .h(px(28.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(paint.fill)
        .child(glyph(paint, 11.0))
        .child(text(paint, 40.0));
    let label = |width: f32| {
        div()
            .h(px(28.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .pl(px(14.0))
            .child(text(paint, width))
    };
    let tree_rows: [(u8, bool, f32); 18] = [
        (0, true, 70.0),
        (1, false, 112.0),
        (1, false, 86.0),
        (1, false, 128.0),
        (0, true, 54.0),
        (1, true, 88.0),
        (2, false, 104.0),
        (2, false, 76.0),
        (1, false, 118.0),
        (0, true, 62.0),
        (1, false, 96.0),
        (1, false, 134.0),
        (1, false, 80.0),
        (0, false, 72.0),
        (0, false, 90.0),
        (0, false, 58.0),
        (0, false, 100.0),
        (0, false, 66.0),
    ];
    let list = div()
        .w(px(DOCS_LIST_WIDTH))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .bg(paint.panel)
        .border_l_1()
        .border_color(paint.hairline)
        .overflow_hidden()
        .child(actions)
        .child(search)
        .child(tree_row(paint, 0, false, 118.0).bg(paint.fill))
        .child(tree_row(paint, 0, false, 92.0))
        .child(label(76.0))
        .children(
            tree_rows
                .iter()
                .map(|(depth, folder, width)| tree_row(paint, *depth, *folder, *width)),
        );
    div()
        .flex()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(header)
                .child(body),
        )
        .child(list)
}

fn kanban_skeleton(paint: &SkeletonPaint) -> Div {
    let toolbar = div()
        .h(px(48.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(20.0))
        .child(pill(paint, 130.0, 18.0))
        .child(pill(paint, 72.0, 18.0))
        .child(pill(paint, 72.0, 18.0));
    let columns: [&[f32]; 4] = [
        &[64.0, 88.0, 56.0, 72.0],
        &[80.0, 56.0, 96.0],
        &[56.0, 72.0, 64.0, 88.0, 56.0],
        &[88.0, 64.0],
    ];
    let board = div()
        .flex_1()
        .min_h_0()
        .flex()
        .gap(px(16.0))
        .px(px(20.0))
        .pt(px(8.0))
        .children(columns.iter().map(|cards| {
            div()
                .flex_1()
                .min_w_0()
                .max_w(px(320.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(div().pb(px(4.0)).child(line(paint, 0.55, 12.0)))
                .children(cards.iter().map(|height| block(paint, *height, 8.0)))
        }));
    div().flex().flex_col().child(toolbar).child(board)
}

/// The Automate page (`native_automate/render.rs`): the header with its eyebrow and project title,
/// the Automations / Runs / Triage tabs, refresh and + Automation, then the automation list beside
/// the selected automation's detail, at the real page's padding and split so loading reads as the
/// page filling in.
fn automate_skeleton(paint: &SkeletonPaint) -> Div {
    let tab = |width: f32, active: bool| {
        div()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .when(active, |this| this.bg(paint.panel))
            .child(pill(paint, width, 9.0))
    };
    let header = div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(16.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(pill(paint, 76.0, 7.0))
                .child(pill(paint, 64.0, 11.0)),
        )
        .child(
            div()
                .flex()
                .gap(px(4.0))
                .child(tab(74.0, true))
                .child(tab(36.0, false))
                .child(tab(42.0, false)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(14.0))
                .child(glyph(paint, 16.0))
                .child(
                    div()
                        .w(px(116.0))
                        .h(px(32.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(paint.hairline)
                        .child(glyph(paint, 12.0))
                        .child(pill(paint, 60.0, 9.0)),
                ),
        );
    // (title width, has a status tag, subtitle width)
    let rows: [(f32, bool, f32); 4] = [
        (150.0, true, 230.0),
        (104.0, true, 170.0),
        (128.0, false, 196.0),
        (88.0, true, 150.0),
    ];
    let list = div()
        .w(relative(0.45))
        .min_w(px(280.0))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(1.0))
        .p(px(8.0))
        .border_r_1()
        .border_color(paint.hairline)
        .children(
            rows.into_iter()
                .enumerate()
                .map(|(index, (title, tag, subtitle))| {
                    div()
                        .w_full()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .px(px(12.0))
                        .py(px(12.0))
                        .rounded(px(8.0))
                        .when(index == 0, |this| this.bg(paint.panel))
                        .child(
                            div()
                                .size(px(6.0))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(paint.fill),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(pill(paint, title, 10.0))
                                        .when(tag, |this| this.child(pill(paint, 40.0, 7.0))),
                                )
                                .child(pill(paint, subtitle, 7.0)),
                        )
                        .child(
                            div()
                                .w(px(32.0))
                                .h(px(20.0))
                                .flex_shrink_0()
                                .rounded(px(6.0))
                                .bg(paint.fill),
                        )
                }),
        );
    let card = || {
        div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .rounded(px(12.0))
            .border_1()
            .border_color(paint.panel)
            .bg(paint.panel)
    };
    let prompt = card().gap(px(12.0)).p(px(16.0)).children(
        [0.94, 0.86, 0.9, 0.8, 0.92, 0.55]
            .into_iter()
            .map(|width| line(paint, width, 9.0)),
    );
    let details = card().children(
        [(64.0, 150.0), (60.0, 90.0), (44.0, 70.0), (40.0, 100.0)]
            .into_iter()
            .enumerate()
            .map(|(index, (label, value))| {
                div()
                    .h(px(44.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .when(index > 0, |this| {
                        this.border_t_1().border_color(paint.panel)
                    })
                    .child(pill(paint, label, 9.0))
                    .child(pill(paint, value, 9.0))
            }),
    );
    let detail = div().flex_1().min_w(px(320.0)).h_full().min_h_0().child(
        div()
            .w_full()
            .max_w(px(672.0))
            .mx_auto()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .p(px(24.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(pill(paint, 52.0, 8.0))
                            .child(pill(paint, 180.0, 13.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(18.0))
                            .pt(px(6.0))
                            .child(glyph(paint, 16.0))
                            .child(glyph(paint, 16.0))
                            .child(glyph(paint, 16.0)),
                    ),
            )
            .child(prompt)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(pill(paint, 52.0, 8.0))
                    .child(details),
            ),
    );
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .p(px(20.0))
        .child(header)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .pt(px(4.0))
                .border_t_1()
                .border_color(paint.hairline)
                .child(list)
                .child(detail),
        )
}

fn storybook_skeleton(paint: &SkeletonPaint) -> Div {
    let sidebar = div()
        .w(px(240.0))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .pt(px(14.0))
        .px(px(14.0))
        .gap(px(18.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(glyph(paint, 22.0))
                .child(line(paint, 0.5, 12.0)),
        )
        .child(div().w_full().h(px(28.0)).rounded(px(6.0)).bg(paint.fill))
        .child(tree(
            paint,
            &[
                (0, 0.5),
                (1, 0.62),
                (2, 0.4),
                (2, 0.5),
                (2, 0.34),
                (1, 0.55),
                (2, 0.45),
                (0, 0.44),
                (1, 0.6),
                (2, 0.36),
                (2, 0.52),
                (1, 0.42),
            ],
        ));
    let toolbar = div()
        .h(px(40.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(14.0))
        .children((0..6).map(|_| glyph(paint, 16.0)))
        .child(div().flex_1())
        .child(glyph(paint, 16.0))
        .child(glyph(paint, 16.0));
    let canvas = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(18.0))
        .p(px(32.0))
        .child(
            div()
                .w(relative(0.68))
                .h(relative(0.56))
                .rounded(px(12.0))
                .bg(paint.fill),
        )
        .child(pill(paint, 160.0, 10.0));
    div().flex().child(sidebar).child(
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(canvas),
    )
}

fn web_skeleton(paint: &SkeletonPaint) -> Div {
    let header = div()
        .h(px(52.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(18.0))
        .px(px(28.0))
        .child(glyph(paint, 24.0))
        .child(pill(paint, 70.0, 12.0))
        .child(pill(paint, 54.0, 12.0))
        .child(pill(paint, 62.0, 12.0))
        .child(div().flex_1())
        .child(pill(paint, 84.0, 26.0));
    let body =
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .px(px(28.0))
            .pt(px(12.0))
            .child(block(paint, 128.0, 12.0))
            .child(
                div().flex().gap(px(16.0)).children(
                    (0..3).map(|_| div().flex_1().min_w_0().child(block(paint, 96.0, 10.0))),
                ),
            )
            .child(div().flex().gap(px(16.0)).children(
                (0..3).map(|_| div().flex_1().min_w_0().child(block(paint, 96.0, 10.0))),
            ));
    div().flex().flex_col().child(header).child(body)
}

fn terminal_skeleton(paint: &SkeletonPaint) -> Div {
    let rows: [(bool, f32); 8] = [
        (true, 0.32),
        (false, 0.58),
        (false, 0.41),
        (false, 0.66),
        (true, 0.24),
        (false, 0.5),
        (false, 0.37),
        (true, 0.06),
    ];
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .pt(px(14.0))
        .px(px(16.0))
        .children(rows.iter().map(|(prompt, width)| {
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .when(*prompt, |row| row.child(glyph(paint, 12.0)))
                .child(line(paint, *width, 10.0))
        }))
}

use gpui::prelude::FluentBuilder as _;
