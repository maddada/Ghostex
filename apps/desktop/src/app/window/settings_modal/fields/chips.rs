//! A row of chips, each one a switch for one setting (Sidebar & Sessions' Card details, Power &
//! Sleep's Turn on / Turn off automatically): a click flips that chip alone, and the row's reset
//! puts every chip back (CDXC:Settings 2026-10-29 DECISION, cleanup Q13 and Q17, in
//! packages/settings-catalog/src/general/sidebar.rs and system.rs).
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use super::row::{PageAction, RowSpec, setting_row};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;

/// One chip: the setting it stands for, its label, and whether it is on.
pub(crate) struct Chip {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) on: bool,
}

/// The chips row: the chips sit under the label, left-aligned, like Session hover buttons (user,
/// 2026-10-29: "show the pills below the setting title like we did for this one"). `on_toggle` gets
/// the chip's id and the state it was clicked toward.
pub(crate) fn chips_field<V: SettingsPage>(
    p: &SettingsPalette,
    id: &'static str,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    chips: Vec<Chip>,
    on_toggle: impl Fn(&mut V, &'static str, bool, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let on_bg = p.foreground_alpha(0.13);
    let on_edge = p.foreground_alpha(0.3);
    let hover_bg = p.foreground_alpha(0.06);
    let resting_text = p.foreground_alpha(0.78);
    let text = p.foreground;
    let hairline = p.hairline;
    let control = h_flex()
        .flex_wrap()
        .gap(px(6.0))
        .children(chips.into_iter().map(|chip| {
            let on = chip.on;
            let chip_id = chip.id;
            let on_toggle = on_toggle.clone();
            div()
                .id(SharedString::from(format!("{id}-chip-{chip_id}")))
                .role(gpui::Role::Switch)
                .aria_label(chip.label)
                .aria_toggled(a11y_toggled(on))
                .flex_shrink_0()
                .h(px(28.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(14.0))
                .border_1()
                .border_color(hsla(if on { on_edge } else { hairline }))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .whitespace_nowrap()
                .cursor_pointer()
                .text_color(hsla(if on { text } else { resting_text }))
                .when(on, |this| this.bg(hsla(on_bg)))
                .hover(move |this| this.bg(hsla(hover_bg)).text_color(hsla(text)))
                .on_press(cx, move |page, window, cx| {
                    on_toggle(page, chip_id, !on, window, cx)
                })
                .child(chip.label)
        }));
    setting_row(p, id, spec.wide(), on_reset, control.into_any_element(), cx)
}
