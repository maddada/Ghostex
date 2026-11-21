//! The Settings list-row `SegmentedControl` (packages/components/ui/segmented-control.tsx (deleted 2026-10-01) as
//! `.settings-list-row-control [data-slot='segmented-control']` restyles it: a transparent tray
//! with a hairline edge, 9px radius and 3px inset, 2px between segments, and the pressed segment
//! as its own 6px-rounded 13% foreground fill), and the fields built on it.
use super::super::super::native_modal_kit::*;
use super::super::catalog::SettingOption;
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use super::row::{PageAction, RowSpec, setting_row};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;

/// The segmented control; `selected` is `None` when no option is active (the Preset row's Custom).
pub(crate) fn settings_segmented<V: 'static>(
    p: &SettingsPalette,
    id: &str,
    options: &[SettingOption],
    selected: Option<&str>,
    on_select: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    segmented(p, id, options, selected, false, on_select, cx)
}

/// [`settings_segmented`] whose pressed segment still answers a click: Experience's Custom
/// segment opens and closes its area's switches.
pub(crate) fn settings_segmented_repress<V: 'static>(
    p: &SettingsPalette,
    id: &str,
    options: &[SettingOption],
    selected: Option<&str>,
    on_select: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    segmented(p, id, options, selected, true, on_select, cx)
}

fn segmented<V: 'static>(
    p: &SettingsPalette,
    id: &str,
    options: &[SettingOption],
    selected: Option<&str>,
    repress: bool,
    on_select: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let pressed_bg = p.foreground_alpha(0.13);
    let hover_bg = p.foreground_alpha(0.06);
    let resting_text = p.foreground_alpha(0.78);
    let text = p.foreground;
    let segments = options.iter().enumerate().map(|(index, option)| {
        let pressed = selected == Some(option.value.as_str());
        let value = option.value.clone();
        let on_select = on_select.clone();
        div()
            .id(SharedString::from(format!("{id}-segment-{index}")))
            .role(gpui::Role::RadioButton)
            .aria_label(option.label.clone())
            .aria_toggled(a11y_toggled(pressed))
            .flex_shrink_0()
            .h_full()
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .whitespace_nowrap()
            .cursor_pointer()
            .text_color(hsla(if pressed { text } else { resting_text }))
            .when(pressed, |this| this.bg(hsla(pressed_bg)))
            .when(!pressed, |this| {
                this.hover(move |this| this.bg(hsla(hover_bg)).text_color(hsla(text)))
            })
            .on_press(cx, move |this, window, cx| {
                if !pressed || repress {
                    on_select(this, value.clone(), window, cx);
                }
            })
            .child(option.label.clone())
    });
    h_flex()
        .id(SharedString::from(format!("{id}-segmented")))
        .flex_shrink_0()
        .h(px(32.0))
        .p(px(3.0))
        .gap(px(2.0))
        .items_stretch()
        .rounded(px(9.0))
        .border_1()
        .border_color(hsla(p.hairline))
        .children(segments)
        .into_any_element()
}

/// A setting row whose control is a segmented control (`SidebarSpacesField`,
/// `PanelAnimationSpeedField`, `PreferredAgentInterfaceField`, `TerminalViewWidthModeField`,
/// `ChatFileOpenViewSetting`, `MediaFileOpenTargetSetting`, `SessionChatThemeField`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn segmented_field<V: SettingsPage>(
    p: &SettingsPalette,
    id: &'static str,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    options: &[SettingOption],
    selected: Option<&str>,
    leading: Option<AnyElement>,
    on_select: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let control = settings_segmented(p, id, options, selected, on_select, cx);
    let control = match leading {
        Some(leading) => h_flex()
            .items_center()
            .gap(px(12.0))
            .child(leading)
            .child(control)
            .into_any_element(),
        None => control,
    };
    setting_row(p, id, spec, on_reset, control, cx)
}

/// The Preset row's `Custom` note before the segments when no preset matches.
pub(crate) fn custom_note(p: &SettingsPalette) -> AnyElement {
    div()
        .text_size(px(13.0))
        .text_color(hsla(p.muted))
        .child("Custom")
        .into_any_element()
}
