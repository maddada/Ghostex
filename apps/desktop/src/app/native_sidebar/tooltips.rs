use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyView, App, AppContext as _, Context, FontWeight, Hsla, IntoElement, ParentElement, Render,
    Styled, Window, div, px,
};
use gpui_component::{
    tooltip::{ManagedTooltipPlacement, Tooltip},
    v_flex,
};

use super::session_list::SESSION_INSET_X;
use crate::app::helpers::{
    titlebar_popup_menu_background, titlebar_popup_menu_border_color,
    titlebar_popup_menu_foreground, tooltip_background, window_glass_active_in,
};

/**
CDXC:Sidebar 2026-09-21 DECISION: A session or project tooltip "needs to not go out of the sidebar. It has to remain inside it with some padding from the right and left", its width matches the width of the actual card, and it sits right below the card rather than floating lower. Cards sit at different insets (collections, the scroll gutter), so a session's span is its card's own painted bounds, recorded while it is hovered; `sidebar_tooltip` drops the stock bubble margins so the two line up.
*/
#[derive(Clone, Copy)]
pub(super) struct SidebarTooltipSpan {
    pub(super) left: f32,
    pub(super) right: f32,
}

impl SidebarTooltipSpan {
    /// The span of a row that fills the sidebar at the standard row inset.
    pub(super) fn sidebar(sidebar_width: f32, scale: f32) -> Self {
        let inset = SESSION_INSET_X * scale;
        Self {
            left: inset,
            right: (sidebar_width - inset).max(inset),
        }
    }

    pub(super) fn placement(self) -> ManagedTooltipPlacement {
        ManagedTooltipPlacement::BelowWithin {
            left: px(self.left),
            right: px(self.right),
        }
    }
}

/**
CDXC:Sidebar 2026-09-21 DECISION: "Please make the tooltip content not have spaces between each of the sections." The shared tooltip text separates its sections with blank lines for the React renderer; here every non-blank line is one tight row.
*/
pub(super) fn sidebar_tooltip(
    text: String,
    span: SidebarTooltipSpan,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    sidebar_tooltip_sized(text, span, true, scale, window, cx)
}

/// CDXC:Spaces 2026-09-21 DECISION: The Space tooltips up top "match the look and appear below, but no need to make the width match anything, give them free width": same bubble, sized to its text and only capped so it stays inside the sidebar.
pub(super) fn sidebar_free_width_tooltip(
    text: String,
    span: SidebarTooltipSpan,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    sidebar_tooltip_sized(text, span, false, scale, window, cx)
}

/**
CDXC:Sidebar 2026-09-23 DECISION: User: the session tooltip, a flat near-black box on the tinted glass sidebar, should "look more fitting". It takes the sidebar menus' shape (8px corners, the menus' soft ink outline and tinted menu colour), and under window glass it is frosted: on macOS and Windows it draws in the frosted tooltip host with the frosted menus' fill and corners (2026-09-25, Windows since 2026-09-27, app/window/frosted_host.rs); where it stays in the window it is only slightly see-through, because an in-window tooltip cannot blur the rows beneath it. The first line (the title) reads as the heading; the state and id lines beneath it are smaller, muted and cut short on one line each.
*/
fn sidebar_tooltip_sized(
    text: String,
    span: SidebarTooltipSpan,
    fill_span: bool,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let card_width = (span.right - span.left).max(0.0);
    let content_width = px((card_width - 16.0 * scale - 2.0).max(0.0));
    sidebar_tooltip_bubble(
        text,
        fill_span.then_some(card_width),
        content_width,
        false,
        scale,
        window,
        cx,
    )
}

/// The session tooltip's look for any short text: the links lists' tooltips (icon states, Link,
/// Restore, the pin) and the header's link chips use it, so they are as readable over busy
/// content as the sidebar's.
///
/// CDXC:SessionLinks 2026-10-24 DECISION:
/// User: "please make tooltips that show there more blurred bg so text in tooltip is more readable (like we did for tooltips for session in sidebar)". The links' tooltips are the sidebar session tooltip's bubble (`sidebar_tooltip_bubble`), not the plain one.
///
/// CDXC:Tooltips 2026-10-25 SEE-ALSO: every sidebar button, icon and menu tooltip uses this bubble now, and so does the Files Search view (the DECISION is in apps/desktop/src/app/native_docs/project_search_view.rs).
///
/// CDXC:Tooltips 2026-10-29 DECISION: a one-line text that fits the bubble is the app's plain tooltip (`titlebar_tooltip`, the user's "please apply 2nd one everywhere that makes sense"), so a sidebar button reads exactly like a header button, its shortcut in key caps; it is frosted by the same tooltip host. Several lines, or a line too long for 320px, keep this bubble, which cuts each line short instead of wrapping.
pub(crate) fn sidebar_style_tooltip(text: String, window: &mut Window, cx: &mut App) -> AnyView {
    if !text.trim().contains('\n') && text_width(window, text.trim(), 12.5) <= SINGLE_LINE_MAX {
        return crate::app::helpers::titlebar_tooltip(text.trim().to_owned(), window, cx);
    }
    // One line each, cut short at 320px: a bubble sized to its text with nothing to fill would
    // otherwise wrap after every letter.
    sidebar_tooltip_bubble(text, None, px(SINGLE_LINE_MAX), true, 1.0, window, cx)
}

/// The widest line `sidebar_style_tooltip` draws before it cuts the line short.
const SINGLE_LINE_MAX: f32 = 320.0;

/// The session tooltip's card for a link: the sidebar's bubble with the link's title in bold,
/// wrapping, and its muted lines under it (`SessionLink::hover_card_text`), up to 340px wide.
///
/// CDXC:SessionLinks 2026-10-24 DECISION:
/// User: "when i hover over the title of a link in the floating thingy pls make it show the full title of that link (issue/pr/etc)", "also please make the tooltip look like this one which we have in the sidebar". A link's row (in the floating card and the header's list) and the header's chips show this card, reusing the sidebar session card's bubble rather than a lookalike.
pub(crate) fn sidebar_style_card_tooltip(
    text: String,
    top_gap: f32,
    bottom_gap: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    // A bubble sized to its text collapses to its narrowest wrap, so the card is as wide as its
    // longest line (up to 340px, where the title wraps).
    let widest = text
        .lines()
        .enumerate()
        .map(|(index, line)| text_width(window, line, if index == 0 { 12.5 } else { 11.0 }))
        .fold(0.0_f32, f32::max);
    let content = (widest + 2.0).clamp(120.0, 340.0);
    let card = sidebar_tooltip_bubble(
        text,
        Some(content + 2.0 * 9.0 + 2.0),
        px(content),
        false,
        1.0,
        window,
        cx,
    );
    tooltip_with_gap(card, top_gap, bottom_gap, cx)
}

/// `card` with clear space above or below it (see `TooltipGap`); the card itself when both are 0.
pub(super) fn tooltip_with_gap(card: AnyView, top: f32, bottom: f32, cx: &mut App) -> AnyView {
    if top <= 0.0 && bottom <= 0.0 {
        return card;
    }
    cx.new(|_| TooltipGap { card, top, bottom }).into()
}

/// A line's width in the UI font at `size`.
pub(super) fn text_width(window: &Window, text: &str, size: f32) -> f32 {
    let run = gpui::TextRun {
        len: text.len(),
        font: gpui::font(crate::ui_fonts::UI_FONT),
        ..Default::default()
    };
    f32::from(
        window
            .text_system()
            .shape_line(text.to_owned().into(), px(size), &[run], None)
            .width,
    )
}

/// A card with clear space above or below it, so a tooltip that opens at the pointer starts
/// beyond the row's edge and leaves the row's own buttons uncovered.
struct TooltipGap {
    card: AnyView,
    top: f32,
    bottom: f32,
}

impl Render for TooltipGap {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pt(px(self.top))
            .pb(px(self.bottom))
            .child(self.card.clone())
    }
}

/// The bubble: `fill_width` sizes it to a card, else it fits its text up to `content_width`.
fn sidebar_tooltip_bubble(
    text: String,
    fill_width: Option<f32>,
    content_width: gpui::Pixels,
    single_lines: bool,
    scale: f32,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let fill_span = fill_width.is_some();
    let menu = titlebar_popup_menu_background();
    // Under glass the tooltip draws in the frosted tooltip host, so it takes the frosted menus' fill
    // and the host's bubble corners (see app/window/frosted_host.rs).
    // CDXC:SessionLinks 2026-10-25 WHY: Frosted whenever the bubble is handed to the frosted host, whichever window built it: the links card beside the sidebar is not a glass window itself, so its link cards came out "too dark" (the user) while the sidebar's session card above it was glass.
    let frosted = crate::app::window::frosted_host::frosted_hosting_active()
        && window.tooltip_presenter_active();
    // Without glass dark mode darkens it (`tooltip_background`); under glass it is the frosted
    // menus' material (CDXC:Tooltips 2026-10-15).
    let in_window = in_window_bubble(window_glass_active_in(window));
    let background = if frosted {
        crate::app::helpers::frosted_tooltip_fill(menu)
    } else {
        in_window.fill
    };
    let radius = px(gpui_component::tooltip::TOOLTIP_RADIUS);
    let secondary = in_window.secondary;
    Tooltip::element(move |_, _| {
        let lines = text.lines().filter(|line| !line.trim().is_empty());
        v_flex()
            .map(|lines| {
                if fill_span {
                    lines.w(content_width)
                } else {
                    lines.max_w(content_width)
                }
            })
            .gap(px(2.0 * scale))
            .children(lines.enumerate().map(|(index, line)| {
                let row = div()
                    .min_w_0()
                    .when(single_lines, |row| row.whitespace_nowrap().truncate());
                if index == 0 {
                    row.pb(px(1.0 * scale))
                        .text_size(px(12.5 * scale))
                        .line_height(px(17.0 * scale))
                        .font_weight(FontWeight::MEDIUM)
                        .child(line.to_owned())
                } else {
                    row.truncate()
                        .text_size(px(11.0 * scale))
                        .line_height(px(15.0 * scale))
                        .text_color(secondary)
                        .child(line.to_owned())
                }
            }))
    })
    .when_some(fill_width, |bubble, width| bubble.w(px(width)))
    .mx_0()
    .mt(px(2.0 * scale))
    .mb_0()
    .py(px(7.0 * scale))
    .px(px(9.0 * scale))
    .rounded(radius)
    .bg(background)
    .border_color(in_window.border)
    .build(window, cx)
}

/// The bubble's colours where it is drawn inside the window it floats over.
#[derive(Clone, Copy)]
pub(crate) struct InWindowBubble {
    pub(crate) fill: Hsla,
    pub(crate) border: Hsla,
    /// Icons and second lines on the fill.
    pub(crate) secondary: Hsla,
}

/// The session tooltip bubble's look for a control that floats over content inside its own window,
/// where nothing can blur what is under it (GPUI blurs only behind a whole window): its opaque
/// tooltip fill, nearly opaque under window glass so it still sits on the glass.
///
/// CDXC:Tooltips 2026-10-25 DECISION: User, of the chat's floating Copy / Paste in terminal bar over a reply: "this floating terminal/copy thing is too clear bg, please make the bg super blurred like we do in the sidebar tool tips!!" Anything that floats over content takes this bubble's fill (the frosted host's when it draws there), never a faint wash the text underneath reads through.
/// SEE-ALSO: apps/desktop/src/app/native_chat/run_command.rs (the inline command bar), native_chat/image_viewer/ and native_docs/picture_viewer.rs (the picture toolbars and arrows).
pub(crate) fn in_window_bubble(glass: bool) -> InWindowBubble {
    let fill = tooltip_background(titlebar_popup_menu_background());
    InWindowBubble {
        fill: if glass { fill.opacity(0.9) } else { fill },
        border: titlebar_popup_menu_border_color(),
        secondary: titlebar_popup_menu_foreground().opacity(0.6),
    }
}
