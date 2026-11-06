//! The stock `SegmentedControl` (packages/components/ui/segmented-control.tsx (deleted 2026-10-01) with its canonical
//! rules in packages/core-ui/styles.css (deleted 2026-11-06)) outside a setting row: one 32px bordered box with an 8px
//! radius around flat segments sized to their labels (or stretched to fill), split by the same
//! hairline, 14px labels at 78% foreground, a 6% wash on hover and a 14% one on the pressed
//! segment. The view scope editor, the extension detail page and the account policy use it.
//! (The setting-row variant with its inset pill is segmented.rs.)
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, px,
};
use gpui_component::h_flex;

/// The control. `options` are `(value, label)`; `stretch` splits the width evenly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stock_segmented<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    options: &[(String, String)],
    selected: Option<&str>,
    disabled: bool,
    stretch: bool,
    on_select: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let id: SharedString = id.into();
    let foreground = p.foreground;
    let border = p.hairline;
    let segments = options.iter().enumerate().map(|(index, (value, label))| {
        let pressed = selected == Some(value.as_str());
        let value = value.clone();
        let on_select = on_select.clone();
        h_flex()
            .id((id.clone(), index))
            .role(gpui::Role::RadioButton)
            .aria_label(label.clone())
            .aria_toggled(a11y_toggled(pressed))
            .when(stretch, |this| this.flex_1().flex_basis(px(0.0)).min_w_0())
            .when(!stretch, |this| this.flex_shrink_0())
            .h_full()
            .px(px(12.0))
            .gap(px(6.0))
            .items_center()
            .justify_center()
            .when(index > 0, |this| {
                this.border_l_1().border_color(hsla(border))
            })
            .text_size(px(14.0))
            .line_height(px(18.67))
            .whitespace_nowrap()
            .text_color(hsla(if pressed {
                foreground
            } else {
                css_fade(foreground, 0.78)
            }))
            .when(pressed, |this| this.bg(hsla(css_fade(foreground, 0.14))))
            .when(!pressed && !disabled, |this| {
                this.cursor_pointer().hover(move |this| {
                    this.bg(hsla(css_fade(foreground, 0.06)))
                        .text_color(hsla(foreground))
                })
            })
            .when(!disabled, |this| {
                this.on_press(cx, move |page, window, cx| {
                    if !pressed {
                        on_select(page, value.clone(), window, cx);
                    }
                })
            })
            .child(label.clone())
    });
    h_flex()
        .id(id.clone())
        .when(stretch, |this| this.w_full())
        .when(!stretch, |this| this.flex_shrink_0())
        .h(px(32.0))
        .items_stretch()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .overflow_hidden()
        .when(disabled, |this| this.opacity(0.5))
        .children(segments)
        .into_any_element()
}

/// `(value, label)` pairs from string slices.
pub(crate) fn segment_options(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(value, label)| (value.to_string(), label.to_string()))
        .collect()
}
