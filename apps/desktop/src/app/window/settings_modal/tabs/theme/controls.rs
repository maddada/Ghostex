//! The Theme page's own controls (`.theme-*` in packages/core-ui/styles/settings-theme.css): each
//! group's More options row, the numbered subheads, the Dark mode / Light mode tabs, the colour
//! squares, the Colourfulness slider and its sidebar | work area preview, and the Related links.
//! Everything that sits directly in a card takes the card's 14/20 inset
//! (`.settings-list-card > :not(.settings-list-row)`).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    SliderBinding, SliderSaver, settings_icon, step_slider, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::ThemeTab;
use super::colours::{COLOURFULNESS_LAST_POSITION, Rgb, colourfulness_name, colourfulness_points};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, BoxShadow, ClickEvent, Context, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, RenderImage, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window, div, img, point, px,
};
use gpui_component::{h_flex, v_flex};
use std::sync::Arc;

pub(super) const ICON_CHEVRON_RIGHT: &str = "modals/settings/chevron-right.svg";
pub(super) const ICON_CHEVRON_DOWN: &str = "modals/settings/chevron-down.svg";

/// `.settings-list-card > :not(.settings-list-row)`: 14px above and below, 20px at the sides.
pub(super) const CARD_INSET_X: f32 = 20.0;
pub(super) const CARD_INSET_Y: f32 = 14.0;

/// One group's More options row: a chevron, the label and a muted summary.
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
        .py(px(CARD_INSET_Y))
        .gap(px(8.0))
        .items_center()
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(p.muted))
        .hover(move |this| this.bg(hsla(hover)))
        .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| on_toggle(page, window, cx)))
        .child(
            settings_icon(
                if open {
                    ICON_CHEVRON_DOWN
                } else {
                    ICON_CHEVRON_RIGHT
                },
                14.0,
                p.muted,
            )
            .flex_shrink_0(),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(hsla(p.foreground))
                .child(label),
        )
        .child(
            div()
                .min_w_0()
                .opacity(0.75)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(summary),
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

/// A card child with the card's inset (`.theme-stacked-row` and the like).
pub(super) fn stacked_row(content: impl IntoElement) -> AnyElement {
    div()
        .w_full()
        .px(px(CARD_INSET_X))
        .py(px(CARD_INSET_Y))
        .child(content)
        .into_any_element()
}

/// `ThemeSchemeTabs`: which appearance's colours the squares show.
pub(super) fn scheme_tabs(
    p: &SettingsPalette,
    dark: bool,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let tab = |scheme_dark: bool, label: &'static str, cx: &mut Context<ThemeTab>| {
        let selected = scheme_dark == dark;
        div()
            .id(SharedString::from(format!("theme-scheme-{label}")))
            .px(px(12.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .text_size(px(13.0))
            .line_height(px(18.57))
            .text_color(hsla(if selected { p.foreground } else { p.muted }))
            .when(selected, |this| this.bg(hsla(p.foreground_alpha(0.1))))
            .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                page.scheme_dark = scheme_dark;
                cx.notify();
            }))
            .child(label)
    };
    h_flex()
        .flex_shrink_0()
        .p(px(2.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.foreground_alpha(0.04)))
        .child(tab(true, "Dark mode", cx))
        .child(tab(false, "Light mode", cx))
        .into_any_element()
}

fn white(alpha: f32) -> gpui::Hsla {
    hsla(modal_rgba(0xffffff, alpha))
}

fn shadow(color: gpui::Hsla, y: f32, blur: f32, spread: f32, inset: bool) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset,
    }
}

/// One colour square of `ThemeSwatchGrid`: the gradient bitmap with its inset rim and drop
/// shadow, the ring when selected, and the colour's name as its tooltip.
#[allow(clippy::too_many_arguments)]
pub(super) fn swatch(
    p: &SettingsPalette,
    id: SharedString,
    label: String,
    image: Option<Arc<RenderImage>>,
    selected: bool,
    on_select: impl Fn(&mut ThemeTab, &mut Context<ThemeTab>) + 'static,
    cx: &mut Context<ThemeTab>,
) -> AnyElement {
    let ring = p.ring;
    let rim = vec![
        shadow(white(0.14), 1.0, 0.0, 0.0, true),
        shadow(white(0.06), 0.0, 0.0, 1.0, true),
    ];
    div()
        .id(id)
        .flex_1()
        .min_w_0()
        .relative()
        .tooltip(tooltip_text(label))
        .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| on_select(page, cx)))
        .child(
            div()
                .relative()
                .w_full()
                .aspect_square()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .shadow(vec![shadow(
                    hsla(modal_rgba(0x000000, 0.3)),
                    4.0,
                    10.0,
                    0.0,
                    false,
                )])
                .when_some(image, |this, image| {
                    this.child(
                        img(image)
                            .absolute()
                            .left_0()
                            .top_0()
                            .size_full()
                            .rounded(px(MODAL_RADIUS_CONTROL))
                            .object_fit(ObjectFit::Fill),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .size_full()
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .shadow(rim),
                )
                .when(selected, |this| {
                    // `outline: 2px solid var(--ring); outline-offset: 2px`.
                    this.child(
                        div()
                            .absolute()
                            .left(px(-4.0))
                            .top(px(-4.0))
                            .right(px(-4.0))
                            .bottom(px(-4.0))
                            .rounded(px(12.0))
                            .border_2()
                            .border_color(hsla(ring)),
                    )
                }),
        )
        .into_any_element()
}

/// `ColourfulnessSlider`: Subtle, the half-point track, Vivid, and the nearest named point.
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
        .child(end("Vivid"))
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
