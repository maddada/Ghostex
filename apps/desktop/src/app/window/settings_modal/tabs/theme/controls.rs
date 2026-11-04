//! The Theme page's own controls (`.theme-*` in packages/core-ui/styles/settings-theme.css): each
//! group's More row, the subheads, the Appearance preview cards, the colour squares and the "+"
//! Custom square, the Colourfulness slider and its sidebar | work area preview.
//! Everything that sits directly in a card takes the card's 14/20 inset
//! (`.settings-list-card > :not(.settings-list-row)`).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ICON_TILE_GAP, ICON_TILE_SIZE, SliderBinding, SliderSaver, icon_tile, settings_icon,
    step_slider, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::ThemeTab;
use super::colours::{COLOURFULNESS_LAST_POSITION, Rgb, colourfulness_name, colourfulness_points};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

pub(super) const ICON_CHEVRON_RIGHT: &str = "modals/settings/chevron-right.svg";
pub(super) const ICON_CHEVRON_DOWN: &str = "modals/settings/chevron-down.svg";
/// The Theme rows' icon tiles (Tabler outline icons in assets/modals/settings/).
pub(super) const ICON_APPEARANCE: &str = "modals/settings/device-desktop.svg";
pub(super) const ICON_DARK: &str = "modals/settings/moon.svg";
pub(super) const ICON_LIGHT: &str = "modals/settings/sun.svg";
pub(super) const ICON_COLOURFULNESS: &str = "modals/settings/palette.svg";
pub(super) const ICON_MORE: &str = "modals/settings/adjustments-horizontal.svg";
pub(super) const ICON_TRANSPARENCY: &str = "modals/settings/stack-2.svg";
pub(super) const ICON_STRENGTH: &str = "modals/settings/droplet.svg";
pub(super) const ICON_BLUR: &str = "modals/settings/blur.svg";
pub(super) const ICON_BEHIND_GLASS: &str = "modals/settings/photo.svg";
pub(super) const ICON_CHAT: &str = "modals/settings/message-circle.svg";
pub(super) const ICON_TERMINAL: &str = "modals/settings/terminal-2.svg";
pub(super) const ICON_FONT: &str = "modals/settings/typography.svg";
pub(super) const ICON_SIDEBAR: &str = "modals/settings/layout-sidebar.svg";
const ICON_PLUS: &str = "modals/settings/plus.svg";

/// A colour square's side, and the room between squares.
const SWATCH_SIZE: f32 = 20.0;
pub(super) const SWATCH_GAP: f32 = 6.0;

/// `.settings-list-card > :not(.settings-list-row)`: 14px above and below, 20px at the sides.
pub(super) const CARD_INSET_X: f32 = 20.0;
pub(super) const CARD_INSET_Y: f32 = 14.0;

/// One group's More row: an icon tile, the label over a muted summary of what is inside, and a
/// chevron at the end.
#[allow(clippy::too_many_arguments)]
pub(super) fn more_options_button(
    p: &SettingsPalette,
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    open: bool,
    on_toggle: impl Fn(&mut ThemeTab, &mut Window, &mut Context<ThemeTab>) + 'static,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let hover = p.foreground_alpha(0.04);
    h_flex()
        .id(id)
        .w_full()
        .px(px(CARD_INSET_X))
        .py(px(10.0))
        .gap(px(ICON_TILE_GAP))
        .items_center()
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(hover)))
        .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| on_toggle(page, window, cx)))
        .child(icon_tile(p, ICON_MORE))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.9))
                        .text_color(hsla(p.foreground))
                        .child(label),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .text_color(hsla(p.muted))
                        .child(summary),
                ),
        )
        .child(
            settings_icon(
                if open {
                    ICON_CHEVRON_DOWN
                } else {
                    ICON_CHEVRON_RIGHT
                },
                16.0,
                p.muted,
            )
            .flex_shrink_0(),
        )
        .into_any_element()
}

/// `ThemeSubhead`: a muted heading inside a group's More options, numbered when it is a step of
/// the glass picture flow.
pub(super) fn subhead(
    p: &SettingsPalette,
    step: Option<u32>,
    text: impl Into<SharedString>,
) -> AnyElement {
    h_flex()
        .w_full()
        .px(px(CARD_INSET_X))
        .py(px(CARD_INSET_Y))
        .gap(px(8.0))
        .items_center()
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(p.muted))
        .children(step.map(|step| {
            div()
                .flex_shrink_0()
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(hsla(p.foreground_alpha(0.1)))
                .text_size(px(11.0))
                .line_height(px(15.71))
                .text_color(hsla(p.foreground))
                .child(step.to_string())
        }))
        .child(text.into())
        .into_any_element()
}

/// A card child lined up with the text of the icon rows above it (the picture slots and the Live
/// gallery under Behind the glass).
pub(super) fn indented_row(content: impl IntoElement) -> AnyElement {
    div()
        .w_full()
        .pl(px(CARD_INSET_X + ICON_TILE_SIZE + ICON_TILE_GAP))
        .pr(px(CARD_INSET_X))
        .pb(px(CARD_INSET_Y))
        .child(content)
        .into_any_element()
}

/// A card child with the card's inset (`.theme-stacked-row` and the like).
pub(super) fn stacked_row(content: impl IntoElement) -> AnyElement {
    div()
        .w_full()
        .px(px(CARD_INSET_X))
        .py(px(CARD_INSET_Y))
        .child(content)
        .into_any_element()
}

/// One colour square: its flat colour, a hairline, today's ring (thinner) when selected, and the
/// colour's name as its tooltip.
pub(super) fn swatch(
    p: &SettingsPalette,
    id: SharedString,
    label: String,
    colour: Rgb,
    selected: bool,
    on_select: impl Fn(&mut ThemeTab, &mut Context<ThemeTab>) + 'static,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let rim = p.foreground_alpha(0.1);
    let hover = p.foreground_alpha(0.45);
    swatch_frame(
        p,
        id,
        label,
        selected,
        move |page, _window, cx| on_select(page, cx),
        cx,
    )
    .child(
        div()
            .size(px(SWATCH_SIZE))
            .rounded(px(5.0))
            .border_1()
            .border_color(hsla(rim))
            .bg(hsla(colour.gpui()))
            .hover(move |this| this.border_color(hsla(hover))),
    )
    .into_any_element()
}

/// The "+" square after the colour squares: picks a custom colour for that appearance and opens
/// its Pick Color dialog, which `picker` draws under the square while it is open.
pub(super) fn custom_swatch(
    p: &SettingsPalette,
    id: SharedString,
    selected: bool,
    picker: Option<AnyElement>,
    on_select: impl Fn(&mut ThemeTab, &mut Window, &mut Context<ThemeTab>) + 'static,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let edge = p.foreground_alpha(0.4);
    let hover = p.foreground_alpha(0.7);
    swatch_frame(p, id, "Custom colour".to_string(), selected, on_select, cx)
        .child(
            div()
                .size(px(SWATCH_SIZE))
                .rounded(px(5.0))
                .border_1()
                .border_dashed()
                .border_color(hsla(edge))
                .flex()
                .items_center()
                .justify_center()
                .hover(move |this| this.border_color(hsla(hover)))
                .child(settings_icon(ICON_PLUS, 12.0, p.muted)),
        )
        .children(picker)
        .into_any_element()
}

/// The clickable frame around a square, with the selection ring 2px outside it.
fn swatch_frame(
    p: &SettingsPalette,
    id: SharedString,
    label: String,
    selected: bool,
    on_select: impl Fn(&mut ThemeTab, &mut Window, &mut Context<ThemeTab>) + 'static,
    cx: &mut Context<ThemeTab>,
) -> gpui::Stateful<gpui::Div> {
    let ring = p.foreground_alpha(0.85);
    div()
        .id(id)
        .flex_shrink_0()
        .relative()
        .cursor_pointer()
        .tooltip(tooltip_text(label))
        .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| on_select(page, window, cx)))
        .when(selected, |this| {
            this.child(
                div()
                    .absolute()
                    .left(px(-3.5))
                    .top(px(-3.5))
                    .right(px(-3.5))
                    .bottom(px(-3.5))
                    .rounded(px(7.5))
                    .border(px(1.5))
                    .border_color(hsla(ring)),
            )
        })
}

/// One preview card of the Appearance row: a small Ghostex window in that appearance's colours
/// (System shows light and dark side by side), and its name under it.
#[allow(clippy::too_many_arguments)]
pub(super) fn appearance_card(
    p: &SettingsPalette,
    id: SharedString,
    label: &str,
    windows: Vec<(Rgb, Rgb, Rgb)>,
    selected: bool,
    on_select: impl Fn(&mut ThemeTab, &mut Context<ThemeTab>) + 'static,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let edge = if selected {
        p.foreground_alpha(0.7)
    } else {
        p.hairline
    };
    let hover = p.foreground_alpha(0.35);
    // GPUI's `overflow_hidden` clips to the rectangle, not the rounded corners, so the card's
    // outermost windows round their own outer corners to the radius inside the border.
    let border = if selected { 2.0 } else { 1.0 };
    let inner_radius = MODAL_RADIUS_SECTION - border;
    let count = windows.len();
    v_flex()
        .id(id)
        .flex_1()
        .min_w_0()
        .gap(px(8.0))
        .items_center()
        .cursor_pointer()
        .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| on_select(page, cx)))
        .child(
            h_flex()
                .w_full()
                .h(px(112.0))
                .rounded(px(MODAL_RADIUS_SECTION))
                .overflow_hidden()
                .when(selected, |this| this.border_2())
                .when(!selected, |this| {
                    this.border_1()
                        .hover(move |this| this.border_color(hsla(hover)))
                })
                .border_color(hsla(edge))
                .children(windows.into_iter().enumerate().map(|(index, colours)| {
                    mini_window(colours, index == 0, index + 1 == count, inner_radius)
                })),
        )
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(if selected { p.foreground } else { p.muted }))
                .when(selected, |this| this.font_weight(gpui::FontWeight::MEDIUM))
                .child(label.to_string()),
        )
        .into_any_element()
}

/// A Ghostex window drawn small: the sidebar with three cards, and the work area with a message,
/// a few lines of text and the composer.
/// `first` and `last` say which outer edges of the card this window sits on; those corners take
/// `radius`.
fn mini_window(colours: (Rgb, Rgb, Rgb), first: bool, last: bool, radius: f32) -> AnyElement {
    let (sidebar, work, foreground) = colours;
    let ink = |alpha: f32| hsla(css_fade(foreground.gpui(), alpha));
    let line = move |width: f32, alpha: f32| {
        div()
            .h(px(5.0))
            .w(gpui::relative(width))
            .rounded(px(3.0))
            .bg(ink(alpha))
    };
    let card = move |active: bool, first: f32, second: Option<f32>| {
        v_flex()
            .w_full()
            .gap(px(4.0))
            .p(px(5.0))
            .rounded(px(5.0))
            .bg(ink(if active { 0.1 } else { 0.05 }))
            .child(line(first, 0.22))
            .children(second.map(|width| line(width, 0.14)))
    };
    h_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(
            v_flex()
                .w(gpui::relative(0.36))
                .h_full()
                .gap(px(5.0))
                .p(px(8.0))
                .bg(hsla(sidebar.gpui()))
                .when(first, |this| {
                    this.rounded_tl(px(radius)).rounded_bl(px(radius))
                })
                .child(
                    h_flex()
                        .gap(px(3.0))
                        .mb(px(3.0))
                        .children((0..3).map(|_| div().size(px(5.0)).rounded_full().bg(ink(0.2)))),
                )
                .child(card(true, 0.8, Some(0.5)))
                .child(card(false, 0.7, Some(0.4)))
                .child(card(false, 0.6, None)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .gap(px(6.0))
                .p(px(9.0))
                .border_l_1()
                .border_color(ink(0.08))
                .bg(hsla(work.gpui()))
                .when(last, |this| {
                    this.rounded_tr(px(radius)).rounded_br(px(radius))
                })
                .child(
                    h_flex().w_full().justify_end().child(
                        div()
                            .w(gpui::relative(0.5))
                            .h(px(14.0))
                            .rounded(px(6.0))
                            .bg(ink(0.09)),
                    ),
                )
                .child(line(0.9, 0.16))
                .child(line(0.72, 0.16))
                .child(line(0.8, 0.16))
                .child(div().flex_1())
                .child(
                    div()
                        .w_full()
                        .h(px(18.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(ink(0.12))
                        .bg(ink(0.04)),
                ),
        )
        .into_any_element()
}

/// `ColourfulnessSlider`: Subtle, the half-point track, Full, and the nearest named point.
pub(super) fn colourfulness_slider(
    page: &mut ThemeTab,
    p: &SettingsPalette,
    key: &'static str,
    step: usize,
    saver: SliderSaver,
    window: &mut Window,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let end = |text: &'static str| {
        div()
            .flex_shrink_0()
            .text_size(px(13.0))
            .line_height(px(18.57))
            .text_color(hsla(p.muted))
            .whitespace_nowrap()
            .child(text)
    };
    let slider = step_slider(
        page,
        p,
        SliderBinding {
            key,
            min: 0.0,
            max: COLOURFULNESS_LAST_POSITION as f64,
            step: 1.0,
        },
        step as f64,
        saver,
        window,
        cx,
    );
    h_flex()
        .flex_shrink_0()
        .w(px(288.0))
        .items_center()
        .gap(px(10.0))
        .child(end("Subtle"))
        .child(div().flex_1().min_w_0().flex().child(slider))
        .child(end("Full"))
        .child(
            div()
                .flex_shrink_0()
                .w(px(68.0))
                .flex()
                .justify_end()
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(p.muted))
                .child(colourfulness_name(colourfulness_points(step))),
        )
        .into_any_element()
}

/// `ColourfulnessPreview`: sidebar | work area in the chosen appearance's colours.
pub(super) fn colourfulness_preview(p: &SettingsPalette, colours: (Rgb, Rgb, Rgb)) -> AnyElement {
    let (sidebar, work, foreground) = colours;
    h_flex()
        .id("theme-colour-live-preview")
        .flex_shrink_0()
        .w(px(96.0))
        .h(px(32.0))
        .overflow_hidden()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .tooltip(tooltip_text("Sidebar | Work area"))
        .child(
            div()
                .h_full()
                .w(gpui::relative(0.38))
                .bg(hsla(sidebar.gpui())),
        )
        .child(
            div()
                .h_full()
                .flex_1()
                .border_l_1()
                .border_color(hsla(css_fade(foreground.gpui(), 0.12)))
                .bg(hsla(work.gpui())),
        )
        .into_any_element()
}

/// A column that stacks its children with `gap`.
pub(super) fn column(gap: f32, children: Vec<AnyElement>) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(gap))
        .children(children)
        .into_any_element()
}
