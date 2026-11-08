//! The Settings look the GBot Settings window is drawn in: Settings' palette, its page title, its
//! grouped sections of rows and a notice card. The window only borrows Settings' palette
//! (settings_modal/palette.rs), not its field library, which is tied to the Settings store and is
//! not part of the web build.
use super::super::native_modal_kit::*;
use super::super::settings_modal::palette::SettingsPalette;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, FontWeight, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
    svg,
};
use gpui_component::{h_flex, v_flex};

/// `.settings-list-row` geometry (settings_modal/fields/row.rs).
const ROW_MIN_HEIGHT: f32 = 52.0;
const ROW_PADDING_X: f32 = 20.0;
const ROW_PADDING_Y: f32 = 10.0;
/// The width of the controls on the right of a row.
pub(super) const CONTROL_WIDTH: f32 = 300.0;

/// The kit controls (inputs, selects, buttons) in Settings' tones: Settings' input fill, hairline
/// and focus edge over its surface.
pub(super) fn kit_palette(p: &SettingsPalette) -> ModalPalette {
    let mut kit = p.modal;
    kit.surface = p.surface;
    kit.raised = p.input_background();
    kit.raised_hover = p.raised_hover;
    kit.hairline = p.hairline;
    kit.focus_border = p.focus_border;
    kit.foreground = p.foreground;
    kit.muted = p.muted;
    kit
}

/// The page title and its one line, as every Settings page opens.
pub(super) fn page_header(p: &SettingsPalette, title: &str, intro: &str) -> AnyElement {
    v_flex()
        .w_full()
        .px(px(2.0))
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(20.0))
                .line_height(px(26.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(13.5))
                .line_height(px(19.0))
                .text_color(hsla(p.muted))
                .child(intro.to_string()),
        )
        .into_any_element()
}

/// A Settings section: its 16px heading over a raised card whose rows divide with hairlines.
pub(super) fn section(p: &SettingsPalette, title: &str, rows: Vec<AnyElement>) -> AnyElement {
    let hairline = hsla(p.hairline);
    let rows = rows.into_iter().enumerate().map(|(index, row)| {
        div()
            .w_full()
            .when(index > 0, |this| this.border_t_1().border_color(hairline))
            .child(row)
    });
    v_flex()
        .w_full()
        .gap(px(12.0))
        .child(
            div()
                .px(px(2.0))
                .text_size(px(16.0))
                .line_height(px(20.8))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child(title.to_string()),
        )
        .child(
            v_flex()
                .w_full()
                .rounded(px(MODAL_RADIUS_SECTION))
                .border_1()
                .border_color(hairline)
                .bg(hsla(p.raised))
                .overflow_hidden()
                .children(rows),
        )
        .into_any_element()
}

fn row_label(p: &SettingsPalette, label: &str) -> gpui::Div {
    div()
        .min_w_0()
        .text_size(px(14.0))
        .line_height(px(18.9))
        .text_color(hsla(p.foreground))
        .child(label.to_string())
}

/// A row with its label on the left and its control on the right.
pub(super) fn row(p: &SettingsPalette, label: &str, control: impl IntoElement) -> AnyElement {
    h_flex()
        .w_full()
        .min_h(px(ROW_MIN_HEIGHT))
        .px(px(ROW_PADDING_X))
        .py(px(ROW_PADDING_Y))
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .child(row_label(p, label).flex_1())
        .child(
            div()
                .flex_shrink_0()
                .w(px(CONTROL_WIDTH))
                .flex()
                .justify_end()
                .child(control),
        )
        .into_any_element()
}

/// A row whose control takes its own full-width line under the label, with an optional note
/// under the control.
pub(super) fn wide_row(
    p: &SettingsPalette,
    label: &str,
    control: impl IntoElement,
    note: Option<SharedString>,
) -> AnyElement {
    v_flex()
        .w_full()
        .min_h(px(ROW_MIN_HEIGHT))
        .px(px(ROW_PADDING_X))
        .py(px(12.0))
        .gap(px(10.0))
        .child(row_label(p, label))
        .child(div().w_full().min_w_0().child(control))
        .children(note.map(|note| {
            div()
                .text_size(px(12.5))
                .line_height(px(17.5))
                .text_color(hsla(p.muted))
                .child(note)
        }))
        .into_any_element()
}

/// Muted read-only text for the right of a row.
pub(super) fn read_only(p: &SettingsPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(14.0))
        .text_color(hsla(p.muted))
        .child(text.into())
        .into_any_element()
}

/// A notice card: a warning icon, its short title and gxserver's own sentence under it.
pub(super) fn notice(p: &SettingsPalette, title: &str, message: &str) -> AnyElement {
    let warning = if p.light { 0xb45309 } else { 0xfbbf24 };
    h_flex()
        .w_full()
        .items_start()
        .gap(px(10.0))
        .px(px(14.0))
        .py(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(modal_rgba(warning, 0.35)))
        .bg(hsla(modal_rgba(warning, 0.08)))
        .child(
            svg()
                .path("titlebar/alert-triangle.svg")
                .flex_shrink_0()
                .mt(px(1.0))
                .size(px(16.0))
                .text_color(hsla(modal_rgba(warning, 1.0))),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(13.5))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.foreground))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.5))
                        .text_color(hsla(p.muted))
                        .child(message.to_string()),
                ),
        )
        .into_any_element()
}
