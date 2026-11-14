//! The Automate view's colours and small shared pieces (icon buttons, the switch, the empty
//! state), taken from the native chat's `ChatAppearance` so the page reads like the chat beside it.

use crate::app::native_chat::appearance::ChatAppearance;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, Div, ElementId, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, Window, div, px, rgb, svg,
};
use gpui_component::{h_flex, v_flex};

pub(crate) const ICON_PLAY: &str = "titlebar/player-play.svg";
pub(crate) const ICON_PENCIL: &str = "titlebar/pencil.svg";
pub(crate) const ICON_TRASH: &str = "titlebar/trash.svg";
pub(crate) const ICON_REFRESH: &str = "titlebar/refresh.svg";
pub(crate) const ICON_PLUS: &str = "titlebar/plus.svg";
pub(crate) const ICON_ARCHIVE: &str = "titlebar/archive.svg";
pub(crate) const ICON_COPY: &str = "titlebar/copy.svg";
pub(crate) const ICON_BELL: &str = "titlebar/bell.svg";
pub(crate) const ICON_EXTERNAL: &str = "titlebar/external-link.svg";
pub(crate) const ICON_FOLDER_OPEN: &str = "titlebar/folder-open.svg";
pub(crate) const ICON_CLOCK: &str = "titlebar/clock.svg";
pub(crate) const ICON_ALERT: &str = "titlebar/alert-circle.svg";

/// Rounding of cards and list rows, the chat bubbles' radius.
pub(crate) const CARD_RADIUS: f32 = 12.0;
pub(crate) const ROW_RADIUS: f32 = 8.0;

#[derive(Clone)]
pub(crate) struct AutomatePalette {
    pub(crate) font: String,
    /// The page fill; clear under window glass so the frosted work area shows through.
    pub(crate) page: Hsla,
    pub(crate) card: Hsla,
    pub(crate) border: Hsla,
    pub(crate) hover: Hsla,
    pub(crate) selected: Hsla,
    pub(crate) foreground: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) accent: Hsla,
    pub(crate) success: Hsla,
    pub(crate) danger: Hsla,
    /// The neutral filled button (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
    pub(crate) button: crate::app::native_chat::appearance::ChatButtonFills,
    pub(crate) switch_off: Hsla,
    pub(crate) switch_thumb: Hsla,
    /// The chat's appearance is light: the on switch takes the light look.
    pub(crate) light: bool,
}

impl AutomatePalette {
    /// Under window glass the page paints nothing and its cards and rows are thin washes of the ink colour with hairline ink borders, the same treatment `ChatAppearance::on_window_glass` gives the chat; opaque, they take the chat's own solid tones.
    pub(crate) fn resolve(glass: bool) -> Self {
        let chat = ChatAppearance::current(&serde_json::Value::Null).on_window_glass(glass);
        let ink: Hsla = rgb(if chat.light { 0x000000 } else { 0xffffff }).into();
        let (page, card) = if glass {
            (
                gpui::transparent_black(),
                ink.opacity(if chat.light { 0.04 } else { 0.05 }),
            )
        } else {
            (chat.background, chat.card_panel)
        };
        Self {
            font: chat.font.clone(),
            page,
            card,
            // The Borders theme role, when set (helpers/theme_roles.rs).
            border: crate::app::helpers::theme_role_or(
                "border",
                chat.light,
                if glass { ink.opacity(0.08) } else { chat.border },
            ),
            hover: crate::app::helpers::theme_role_or("hover", chat.light, ink.opacity(0.04)),
            selected: crate::app::helpers::theme_role_or(
                "selection",
                chat.light,
                ink.opacity(if glass { 0.08 } else { 0.06 }),
            ),
            foreground: chat.foreground,
            muted: chat.muted,
            accent: chat.accent,
            success: chat.success(),
            danger: chat.error(),
            button: chat.neutral_button_fills(),
            switch_off: ink.opacity(if chat.light { 0.15 } else { 0.18 }),
            switch_thumb: chat.background,
            light: chat.light,
        }
    }

    /// `automationRunStatusTone`.
    pub(crate) fn run_status_color(&self, status: &str) -> Hsla {
        match status {
            "findings" => self.success,
            "failed" | "needs_attention" => self.danger,
            "running" | "queued" => self.accent,
            _ => self.muted,
        }
    }
}

pub(crate) fn icon(path: &'static str, size: f32, color: Hsla) -> gpui::Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
}

/// A 28px ghost icon button; a disabled one is dimmed and ignores clicks.
pub(crate) fn icon_button<V: 'static>(
    p: &AutomatePalette,
    id: impl Into<ElementId>,
    icon_path: &'static str,
    label: &'static str,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let hover = p.hover;
    div()
        .id(id)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded(px(ROW_RADIUS))
        .role(gpui::Role::Button)
        .aria_label(label)
        .when(disabled, |this| this.opacity(0.4))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hover))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    on_click(this, window, cx);
                }))
        })
        .child(icon(icon_path, 16.0, p.foreground.opacity(0.85)))
}

/// A secondary text button in the neutral fill, optionally with a leading icon.
pub(crate) fn secondary_button<V: 'static>(
    p: &AutomatePalette,
    id: impl Into<ElementId>,
    leading: Option<&'static str>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let fills = p.button;
    h_flex()
        .id(id)
        .flex_shrink_0()
        .h(px(32.0))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(gpui::transparent_black())
        .bg(fills.rest)
        .text_size(px(13.0))
        .text_color(p.foreground)
        .cursor_pointer()
        .hover(move |this| this.bg(fills.hover))
        .active(move |this| this.bg(fills.pressed))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            on_click(this, window, cx);
        }))
        .children(leading.map(|path| icon(path, 15.0, p.foreground)))
        .child(label.into())
}

/// The app-wide switch ([`draw_switch`]): 32x20 track, 6px track radius, 16px thumb with a 4px
/// radius; off is Automate's own quiet track.
pub(crate) fn switch(p: &AutomatePalette, checked: bool, disabled: bool) -> Div {
    crate::app::window::native_modal_kit::draw_switch(
        crate::app::window::native_modal_kit::SwitchShape::REGULAR,
        crate::app::window::native_modal_kit::SwitchOff {
            track: p.switch_off.into(),
            edge: None,
            thumb: p.switch_thumb.into(),
        },
        p.light,
        checked,
        disabled,
    )
}

/// `AutomationGroupCard`: a rounded card whose rows are split by hairlines.
pub(crate) fn group_card(p: &AutomatePalette, rows: Vec<AnyElement>) -> Div {
    let count = rows.len();
    v_flex()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(CARD_RADIUS))
        .border_1()
        .border_color(p.border)
        .bg(p.card)
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            div()
                .w_full()
                .when(index + 1 < count, |this| {
                    this.border_b_1().border_color(p.border)
                })
                .child(row)
        }))
}

/// `AutomationDetailRow`: a label on the left, the value (and any trailing control) on the right.
pub(crate) fn detail_row(
    p: &AutomatePalette,
    label: &'static str,
    value: Vec<AnyElement>,
) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .min_h(px(44.0))
        .px(px(16.0))
        .py(px(10.0))
        .gap(px(16.0))
        .items_center()
        .justify_between()
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(14.0))
                .text_color(p.foreground.opacity(0.9))
                .child(label),
        )
        .child(
            h_flex()
                .min_w_0()
                .gap(px(6.0))
                .items_center()
                .text_size(px(14.0))
                .text_color(p.muted)
                .children(value),
        )
        .into_any_element()
}

/// Truncating single-line text.
pub(crate) fn truncated(text: impl Into<SharedString>) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text.into())
}

pub(crate) fn section_label(p: &AutomatePalette, text: &'static str) -> Div {
    div().text_size(px(13.0)).text_color(p.muted).child(text)
}

/// `AutomationEmptyState`: an icon tile, a title, a sentence, and an optional action.
pub(crate) fn empty_state(
    p: &AutomatePalette,
    icon_path: &'static str,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    action: Option<AnyElement>,
) -> AnyElement {
    v_flex()
        .size_full()
        .min_h_0()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .p(px(32.0))
        .child(
            div()
                .mb(px(4.0))
                .size(px(48.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(CARD_RADIUS))
                .border_1()
                .border_color(p.border)
                .bg(p.card)
                .child(icon(icon_path, 24.0, p.muted)),
        )
        .child(
            div()
                .text_size(px(14.0))
                .text_color(p.foreground)
                .child(title.into()),
        )
        .child(
            div()
                .max_w(px(320.0))
                .text_center()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(p.muted)
                .child(description.into()),
        )
        .children(action.map(|action| div().mt(px(8.0)).child(action)))
        .into_any_element()
}

pub(crate) fn status_dot(color: Hsla) -> Div {
    div().flex_shrink_0().size(px(6.0)).rounded_full().bg(color)
}

pub(crate) fn heading(
    p: &AutomatePalette,
    eyebrow: impl Into<SharedString>,
    eyebrow_color: Hsla,
    title: impl Into<SharedString>,
) -> Div {
    v_flex()
        .min_w_0()
        .child(
            div()
                .text_size(px(13.0))
                .text_color(eyebrow_color)
                .child(eyebrow.into()),
        )
        .child(
            truncated(title)
                .mt(px(4.0))
                .text_size(px(18.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(p.foreground),
        )
}
