use super::*;
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, ClickEvent, Context, Div, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Rgba, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Transformation, Window, div, px, radians,
};
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// Dialog title (16px/400, line-height 1.3) and description (13px muted, line-height 1.55), 6px apart.
pub(crate) fn modal_header(
    p: &ModalPalette,
    title: impl Into<SharedString>,
    description: Option<impl Into<SharedString>>,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(20.8))
                .child(title.into()),
        )
        .children(description.map(|description| {
            div()
                .text_size(px(13.0))
                .line_height(px(20.15))
                .text_color(hsla(p.muted))
                .child(description.into())
        }))
        .into_any_element()
}

/// `[data-slot='field-label']` and the export dialog's section titles: 12px/500 muted.
pub(crate) fn modal_section_title(p: &ModalPalette, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.0))
        .line_height(px(17.14))
        .font_weight(FontWeight::MEDIUM)
        .text_color(hsla(p.muted))
        .child(text.into())
}

/// 12px muted helper copy with the 1.45 line height the modals use for hints.
pub(crate) fn modal_hint(p: &ModalPalette, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.0))
        .line_height(px(17.4))
        .text_color(hsla(p.muted))
        .child(text.into())
}

/// `.export-transcript-error` and friends: 12px destructive copy, line-height 1.5.
pub(crate) fn modal_error(p: &ModalPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(hsla(p.destructive))
        .child(text.into())
        .into_any_element()
}

/// A section panel (`--gx-modal-panel`, hairline border, 12px radius), contents supplied by the caller.
pub(crate) fn modal_panel(p: &ModalPalette) -> Div {
    v_flex()
        .w_full()
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.panel))
        .overflow_hidden()
}

/// A raised control box (`--gx-modal-raised`, hairline border, 8px radius).
pub(crate) fn modal_raised_box(p: &ModalPalette) -> Div {
    div()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.raised))
}

/// The row inside a panel list: 9px 12px padding, label 13px/500 and 12px muted description, control on the right.
pub(crate) fn modal_panel_row(
    p: &ModalPalette,
    label: impl Into<SharedString>,
    description: Option<impl Into<SharedString>>,
    control: AnyElement,
    divider: bool,
) -> Div {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .px(px(12.0))
        .py(px(9.0))
        .when(divider, |this| {
            this.border_t_1().border_color(hsla(p.hairline))
        })
        .child(
            v_flex()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.into()),
                )
                .children(description.map(|description| {
                    div()
                        .text_size(px(12.0))
                        .line_height(px(17.14))
                        .text_color(hsla(p.muted))
                        .child(description.into())
                })),
        )
        .child(control)
}

/// The app-wide toggle: 32x20 track with a 2px border and 6px radius, 16px thumb with 4px radius.
/// It only draws; the clickable row that holds it carries `.role(Role::Switch)`,
/// `.aria_toggled(a11y_toggled(..))` and `.aria_label(<the row's visible label>)`, so a screen
/// reader names it by that label and never by the row's element id.
pub(crate) fn modal_switch(p: &ModalPalette, checked: bool, disabled: bool) -> AnyElement {
    div()
        .flex_shrink_0()
        .w(px(32.0))
        .h(px(20.0))
        .rounded(px(6.0))
        .border_2()
        .border_color(if checked {
            hsla(p.primary)
        } else {
            transparent()
        })
        .bg(hsla(if checked { p.primary } else { p.switch_off }))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div()
                .size(px(16.0))
                .ml(px(if checked { 12.0 } else { 0.0 }))
                .rounded(px(4.0))
                .bg(hsla(p.background))
                .shadow_sm(),
        )
        .into_any_element()
}

/// The vertical scrollbar of a scroll area that runs along the modal window's right edge: mount it
/// beside the scroll area (never inside it) and let that area reach the window's edge, keeping the
/// text gutter as padding inside it.
///
/// CDXC:AppModal 2026-10-08 DECISION:
/// User: "give the scroll bar 1px gap from the right border of the window. apply this in all modals that have the same kind of scroll bar pls to be consistent" (superseding the same day's flush "without gap" placement). The thumb sits 1px in from the modal window's right border: a 1px inset inside its track (gpui-component's default is 4px) and no window padding beside it; the content keeps its own gutter so text never sits under the thumb. Every modal scroll area that runs along the window's right border uses this one bar; scrollbars inside a bordered inner panel keep the default.
#[track_caller]
pub(crate) fn modal_edge_scrollbar<H: gpui_component::scroll::ScrollbarHandle + Clone>(
    handle: &H,
) -> gpui_component::scroll::Scrollbar {
    with_modal_edge_gap(gpui_component::scroll::Scrollbar::vertical(handle))
}

/// `modal_edge_scrollbar`'s 1px gap from the window border, for an edge bar with its own
/// thickness. Apply it after `.thickness()`, which resets the inset.
pub(crate) fn with_modal_edge_gap(
    scrollbar: gpui_component::scroll::Scrollbar,
) -> gpui_component::scroll::Scrollbar {
    scrollbar.styles(|styles| styles.thumb(|thumb| thumb.inset(px(1.0))))
}

/// A rotating loader icon for busy primary buttons.
pub(crate) fn modal_spinner(color: Rgba) -> AnyElement {
    modal_icon(ICON_LOADER, 15.0, color)
        .with_animation(
            "native-modal-spinner",
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalButtonTone {
    Neutral,
    Primary,
    Danger,
}

/// A footer pill button (`.gx-app-modal-action-button`): 32px tall, 8px radius,
/// outline by default, filled for the primary tone, destructive-tinted outline
/// for the danger tone, half opacity when disabled.
pub(crate) fn modal_action_button<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    label: impl Into<SharedString>,
    leading: Option<AnyElement>,
    tone: ModalButtonTone,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let (background, border, text, hover) = match tone {
        ModalButtonTone::Neutral => (transparent(), p.hairline, p.foreground, p.raised_hover),
        ModalButtonTone::Primary => (
            hsla(p.primary),
            p.primary,
            p.primary_foreground,
            p.primary_hover(),
        ),
        // `.gx-app-modal-action-danger`: an outline tint, never a fill.
        ModalButtonTone::Danger => (
            transparent(),
            rgba_of(p.destructive, 0.45),
            p.destructive,
            rgba_of(p.destructive, 0.12),
        ),
    };
    let label: SharedString = label.into();
    h_flex()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .accessibility_id(id)
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .h(px(MODAL_FOOTER_BUTTON_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .bg(background)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_press(cx, move |this, window, cx| {
                    on_click(this, window, cx);
                })
        })
        .children(leading)
        .child(label)
        .into_any_element()
}

/// `.gx-app-modal-footer`: equal-width buttons filling the modal width, 8px apart.
pub(crate) fn modal_footer(buttons: Vec<AnyElement>) -> AnyElement {
    h_flex()
        .w_full()
        .gap(px(8.0))
        .children(buttons)
        .into_any_element()
}

/// A 32px ghost icon button (shadcn `variant='ghost' size='icon'` inside the modal skin).
pub(crate) fn modal_icon_button<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    icon_path: &'static str,
    icon_size: f32,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let accent = p.accent;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(id)
        .accessibility_id(id)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(32.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(accent)))
        .on_press(cx, move |this, window, cx| {
            on_click(this, window, cx);
        })
        .child(modal_icon(icon_path, icon_size, p.foreground))
}

/// The accessibility tree's on/off state for a switch, checkbox or pressed segment.
///
/// CDXC:Accessibility 2026-09-30 WHY: GPUI leaves an element without a role out of the accessibility tree, so every clickable native modal control carries a role and a label; without them VoiceOver cannot name it and a background accessibility press (cua-driver) lands on the window instead of the control. Tab strips and the Settings rail use `Role::Button` with `aria_selected`, not `Role::Tab`: a Tab node without a TabList parent left the whole window's macOS accessibility tree empty (verified live in Settings and Commands).
pub(crate) fn a11y_toggled(on: bool) -> gpui::Toggled {
    if on {
        gpui::Toggled::True
    } else {
        gpui::Toggled::False
    }
}

/// `on_click` for a native modal control that also answers an accessibility press (VoiceOver,
/// cua-driver) by running the same handler.
///
/// CDXC:Accessibility 2026-10-01 WHY: GPUI answers a press on a node with no press listener by synthesising a mouse click at the node's last bounds, which lands on whatever is there now: nothing when the control is scrolled out of its list, the wrong row after a re-layout. The handler is called directly instead.
pub(crate) trait PressExt: gpui::StatefulInteractiveElement + Sized {
    fn on_press<V: 'static>(
        self,
        cx: &Context<V>,
        handler: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let handler = Rc::new(handler);
        let click = Rc::clone(&handler);
        let view = cx.entity().downgrade();
        self.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            click(this, window, cx);
        }))
        .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, app| {
            let _ = view.update(app, |this, cx| handler(this, window, cx));
        })
    }
}

impl<E: gpui::StatefulInteractiveElement> PressExt for E {}

/// Captures one child's bounds from a prepaint pass, for anchoring popovers.
pub(crate) fn capture_child_bounds(
    cell: Rc<Cell<Option<Bounds<Pixels>>>>,
    index: usize,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
    move |bounds, _window, _cx| cell.set(bounds.get(index).copied())
}

/// Whether a select list of `list_height` opens above its trigger.
///
/// CDXC:AppModal 2026-09-28 WHY:
/// Base UI flips a select popup above its trigger when it does not fit below and there is more room above (the commit review's footer agent select sits on the window's bottom edge). Snapping the list back into the window instead drew it over the trigger.
pub(crate) fn select_menu_opens_upward(
    trigger: Bounds<Pixels>,
    list_height: Pixels,
    window: &Window,
) -> bool {
    let viewport = window.viewport_size().height;
    let below = viewport - (trigger.origin.y + trigger.size.height + px(4.0)) - px(8.0);
    let above = trigger.origin.y - px(4.0) - px(8.0);
    list_height > below && above > below
}

/// The shadcn input skin the modals use: raised background, hairline border
/// (white or #525252 when focused), 8px radius, 32px tall, 14px text. Wraps a
/// gpui-component `Input`, whose window root must be a gpui-component `Root`.
pub(crate) fn modal_text_input(
    p: &ModalPalette,
    state: &gpui::Entity<InputState>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
        .bg(hsla(p.raised))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .disabled(disabled)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(14.0))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .into_any_element()
}

/// The shadcn textarea skin: the input skin with 12px vertical padding.
/// `min_height` is the React `min-height`; `None` makes the editor fill the
/// remaining column height (the fixed-frame editors).
pub(crate) fn modal_text_area(
    p: &ModalPalette,
    state: &gpui::Entity<TextareaState>,
    min_height: Option<f32>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    modal_text_area_with_paste(p, state, min_height, disabled, None, window, cx)
}

/// The field's paste hook (`Textarea::on_paste`): `true` takes the paste, `false` lets the field
/// insert the clipboard's text.
pub(crate) type ModalPasteHandler =
    Rc<dyn Fn(&gpui::ClipboardItem, &mut Window, &mut App) -> bool + 'static>;

/// [`modal_text_area`] whose pastes are offered to `paste` first (pasted images).
pub(crate) fn modal_text_area_with_paste(
    p: &ModalPalette,
    state: &gpui::Entity<TextareaState>,
    min_height: Option<f32>,
    disabled: bool,
    paste: Option<ModalPasteHandler>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .when_some(min_height, |this, min_height| this.min_h(px(min_height)))
        .when(min_height.is_none(), |this| this.flex_1().min_h_0())
        .px(px(12.0))
        .py(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
        .bg(hsla(p.raised))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div()
                .w_full()
                .min_w_0()
                .when(min_height.is_none(), |this| this.flex_1().min_h_0())
                .child(
                    Textarea::new(state)
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .disabled(disabled)
                        .when_some(paste, |this, paste| {
                            this.on_paste(move |clipboard, window, cx| paste(clipboard, window, cx))
                        })
                        .w_full()
                        .when(min_height.is_none(), |this| this.h_full())
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(14.0))
                        .text_color(hsla(p.foreground)),
                ),
        )
        .into_any_element()
}
