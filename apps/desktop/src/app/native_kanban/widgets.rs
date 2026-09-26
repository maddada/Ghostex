//! Small controls the Kanban view draws in several places: buttons, choice pills, text fields and
//! section titles, all on the board's palette.

use std::cell::Cell;
use std::rc::Rc;

use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, Stateful, Styled as _, Window, div, px,
};
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_component::{Sizable as _, Size as ComponentSize};

use super::palette::KanbanPalette;
use crate::app::helpers::titlebar_svg_icon;

pub(crate) const CONTROL_HEIGHT: f32 = 30.0;
pub(crate) const CONTROL_RADIUS: f32 = 8.0;
pub(crate) const LANE_RADIUS: f32 = 12.0;
pub(crate) const CARD_RADIUS: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum KanbanButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

/// A text and/or icon button. The caller adds `on_click`.
pub(crate) fn kanban_button(
    id: impl Into<ElementId>,
    icon: Option<&'static str>,
    label: Option<SharedString>,
    kind: KanbanButtonKind,
    disabled: bool,
    p: &KanbanPalette,
) -> Stateful<gpui::Div> {
    let (background, hover, text) = match kind {
        KanbanButtonKind::Primary => (p.button, p.button.opacity(0.88), p.button_text),
        KanbanButtonKind::Secondary => (p.control, p.control_hover, p.foreground),
        KanbanButtonKind::Ghost => (gpui::transparent_black(), p.control_hover, p.muted),
        KanbanButtonKind::Danger => (p.danger.opacity(0.12), p.danger.opacity(0.2), p.danger),
    };
    let icon_only = label.is_none();
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .h(px(CONTROL_HEIGHT))
        .when(icon_only, |this| this.w(px(CONTROL_HEIGHT)))
        .when(!icon_only, |this| this.px(px(12.0)))
        .rounded(px(CONTROL_RADIUS))
        .bg(background)
        .when(kind == KanbanButtonKind::Secondary, |this| {
            this.border_1().border_color(p.border)
        })
        .text_size(px(13.0))
        .text_color(text)
        .when(disabled, |this| this.opacity(0.45))
        .when(!disabled, |this| {
            this.cursor_pointer().hover(move |style| style.bg(hover))
        })
        .when_some(icon, |this, icon| {
            this.child(titlebar_svg_icon(icon, 15.0, text))
        })
        .when_some(label, |this, label| {
            this.child(div().whitespace_nowrap().child(label))
        })
}

/// One option in a pick-one row (status, priority, estimate).
pub(crate) fn choice_pill(
    id: impl Into<ElementId>,
    label: SharedString,
    selected: bool,
    p: &KanbanPalette,
) -> Stateful<gpui::Div> {
    let hover = p.control_hover;
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(26.0))
        .px(px(10.0))
        .rounded(px(7.0))
        .border_1()
        .text_size(px(12.0))
        .cursor_pointer()
        .when(selected, |this| {
            this.bg(p.control_hover)
                .border_color(p.border_strong)
                .text_color(p.foreground)
        })
        .when(!selected, |this| {
            this.border_color(p.border)
                .text_color(p.muted)
                .hover(move |style| style.bg(hover))
        })
        .child(label)
}

/// A bordered single-line text input on the board's control fill.
pub(crate) fn text_field(
    state: &Entity<InputState>,
    p: &KanbanPalette,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    field_frame(
        focused,
        None,
        p,
        Input::new(state)
            .with_size(ComponentSize::Small)
            .appearance(false)
            .bordered(false)
            .focus_bordered(false)
            .w_full()
            .px(px(0.0))
            .py(px(0.0))
            .text_size(px(13.0))
            .text_color(p.foreground),
    )
}

/// A bordered text area `height` tall on the board's control fill.
pub(crate) fn text_area(
    state: &Entity<TextareaState>,
    height: f32,
    p: &KanbanPalette,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    field_frame(
        focused,
        Some(height),
        p,
        Textarea::new(state)
            .with_size(ComponentSize::Small)
            .appearance(false)
            .bordered(false)
            .focus_bordered(false)
            .w_full()
            .h_full()
            .px(px(0.0))
            .py(px(0.0))
            .text_size(px(13.0))
            .text_color(p.foreground),
    )
}

/// The frame [`text_field`] and [`text_area`] share: control height, or `height` for a text area.
fn field_frame(
    focused: bool,
    height: Option<f32>,
    p: &KanbanPalette,
    field: impl IntoElement,
) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .when_some(height, |this, height| {
            this.h(px(height)).items_start().py(px(8.0))
        })
        .when(height.is_none(), |this| this.h(px(CONTROL_HEIGHT)))
        .px(px(10.0))
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(if focused { p.border_strong } else { p.border })
        .bg(p.control)
        .child(div().flex_1().min_w_0().h_full().child(field))
        .into_any_element()
}

/// A quiet uppercase-free section heading, like the React dialogs' section titles.
pub(crate) fn section_title(text: impl Into<SharedString>, p: &KanbanPalette) -> gpui::Div {
    div()
        .text_size(px(11.0))
        .text_color(p.faint)
        .child(text.into())
}

/// Records where an element was laid out, so a menu can open below it.
pub(crate) fn anchor_probe(cell: Rc<Cell<Bounds<Pixels>>>) -> impl IntoElement {
    gpui::canvas(move |bounds, _, _| cell.set(bounds), |_, _, _, _| {})
        .absolute()
        .size_full()
}
