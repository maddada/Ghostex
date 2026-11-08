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
        .border_color(transparent())
        .bg(hsla(if checked {
            switch_on_track(p.foreground, p.light)
        } else {
            p.switch_off
        }))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div()
                .size(px(16.0))
                .ml(px(if checked { 12.0 } else { 0.0 }))
                .rounded(px(4.0))
                .bg(hsla(if checked {
                    SWITCH_ON_THUMB
                } else {
                    p.background
                }))
                .shadow_sm(),
        )
        .into_any_element()
}

/// The track of an on switch: the theme's ink at partial strength, a mid grey that carries the
/// theme's hue (the dark themes' foregrounds are tinted), so the white [`SWITCH_ON_THUMB`] stands
/// out on it in every look and the track never turns black on glass.
///
/// CDXC:Switches 2026-11-08 DECISION:
/// User, of an on Settings switch under dark glass (a near-white track with a light grey handle): "i dont like how this looks when enabled fam please make it look better (the color of the handle blends with the white of the toggle)". Every on switch (Settings rows, Settings cards, the modal kit's app-wide switch) now draws a mid-tone track (the foreground at 42% in dark looks, 62% in light ones) under a white handle with a soft shadow, the same in glass and solid. The off state keeps its faint track and its foreground or surface handle at the left, so on reads as a brighter track with a white handle at the right. The light on track was the near-black foreground, which the user does not want as a fill on glass.
pub(crate) fn switch_on_track(foreground: Rgba, light: bool) -> Rgba {
    rgba_of(foreground, if light { 0.62 } else { 0.42 })
}

/// The handle of an on switch: white, over [`switch_on_track`].
pub(crate) const SWITCH_ON_THUMB: Rgba = Rgba {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};

/// The faint wash a switch row shows while the pointer is over it: the foreground at 4%, so it is
/// a light veil on a dark surface and a dark one on a light surface, never a solid fill.
pub(crate) fn switch_row_wash(foreground: Rgba) -> Rgba {
    rgba_of(foreground, 0.04)
}

/// Makes a row that holds a switch toggle from a click anywhere on it, not only on the switch.
///
/// CDXC:SwitchRows 2026-11-06 DECISION:
/// User: "allow clicking in the center of any toggle bar or on it's title to toggle it (not just the toggle button itself)". Every row of the app that pairs a title with a switch is built through this one helper (`modal_panel_row` callers, onboarding's `switch_row`, Settings' `toggle_field_with`/`setting_toggle_row`): the whole row is the click target, it shows a pointer cursor and a hover wash, and a disabled row does neither. The switch's own `switch_control` and a row's other controls (reset asterisk, text field, dropdown, button) call `App::stop_propagation` when pressed, so a click on them never reaches the row and a click on the switch itself toggles once, not twice.
pub(crate) trait SwitchRowExt:
    gpui::StatefulInteractiveElement + gpui::Styled + Sized
{
    fn switch_row_press(self, wash: Rgba, disabled: bool, press: BoundPress) -> Self {
        if disabled {
            return self;
        }
        self.cursor_pointer()
            .hover(move |this| this.bg(hsla(wash)))
            .on_bound_press(press)
    }
}

impl<E: gpui::StatefulInteractiveElement + gpui::Styled> SwitchRowExt for E {}

/// The app's vertical scrollbar, the look of the chat transcript's: gpui-component's own geometry
/// (a 16px strip, a 6px thumb 4px in from the edge that grows to 8px on hover or drag, the theme's
/// radius and colours) and shown only while its content scrolls or the pointer is on it. Add
/// nothing that changes its geometry (`thickness`, a thumb inset) at a call site, or that bar
/// stops looking like this one.
///
/// CDXC:AppModal 2026-10-14 DECISION:
/// User: "give the scrollbar in Settings margin from the right and make it look just like the one we have in the chat view instead of what we have now." The modal scroll areas along a window's edge (`modal_edge_scrollbar`, the Settings rail) and the chat transcript (`transcript_scrollbar` in native_chat/scrollbar.rs) build their bar here, so they cannot drift apart.
#[track_caller]
pub(crate) fn app_vertical_scrollbar<H: gpui_component::scroll::ScrollbarHandle + Clone>(
    handle: &H,
) -> gpui_component::scroll::Scrollbar {
    gpui_component::scroll::Scrollbar::vertical(handle)
        .mode(gpui_component::scroll::ScrollbarMode::Scrolling)
}

/// The vertical scrollbar of a scroll area that runs along the modal window's right edge: mount it
/// beside the scroll area (never inside it) and let that area reach the window's edge, keeping a
/// text gutter of at least the bar's 16px strip as padding inside it. It is the chat transcript's
/// bar (CDXC:AppModal 2026-10-14 on `app_vertical_scrollbar`, which supersedes the 2026-10-08
/// decision that put the thumb 1px from the border), so its thumb sits 4px in from that border.
#[track_caller]
pub(crate) fn modal_edge_scrollbar<H: gpui_component::scroll::ScrollbarHandle + Clone>(
    handle: &H,
) -> gpui_component::scroll::Scrollbar {
    app_vertical_scrollbar(handle)
}

/// A 1px gap from the window border, for an edge bar with its own thickness (the account usage
/// popup's). Apply it after `.thickness()`, which resets the inset.
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

/// A footer pill button (`.gx-app-modal-action-button`): 32px tall, 8px radius, the neutral fill
/// by default, the primary fill for the primary tone, the destructive fill for the danger tone
/// (CDXC:AppModal 2026-10-14 on [`neutral_button_fills`]), half opacity when disabled.
///
/// CDXC:Release 2026-10-22 WHY: the body lives in the non-generic `modal_action_button_bound`; generic over the click handler it was copied into the release binary once per call site (52 copies, about 0.18 MB).
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
    modal_action_button_bound(
        p,
        id,
        label.into(),
        leading,
        tone,
        disabled,
        bind_press(cx, on_click),
    )
}

fn modal_action_button_bound(
    p: &ModalPalette,
    id: &'static str,
    label: SharedString,
    leading: Option<AnyElement>,
    tone: ModalButtonTone,
    disabled: bool,
    on_click: BoundPress,
) -> AnyElement {
    let p = *p;
    let (fills, text) = match tone {
        ModalButtonTone::Neutral => (neutral_button_fills(p.light), p.foreground),
        ModalButtonTone::Primary => (
            ButtonFills {
                rest: p.primary,
                hover: p.primary_hover(),
                pressed: css_mix(p.primary, 0.8, gpui::rgb(0xffffff)),
            },
            p.primary_foreground,
        ),
        ModalButtonTone::Danger => (
            destructive_button_fills(p.destructive, p.light),
            p.destructive,
        ),
    };
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
        .border_color(transparent())
        .bg(hsla(fills.rest))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(fills.hover)))
                .active(move |this| this.bg(hsla(fills.pressed)))
                .on_bound_press(on_click)
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

/// A view's click handler already bound to that view, so the element code that attaches it is
/// not generic over the view or the handler.
pub(crate) type BoundPress = Rc<dyn Fn(&mut Window, &mut App)>;

/// Binds `handler` to the view `cx` belongs to (what `cx.listener` does for one click).
pub(crate) fn bind_press<V: 'static>(
    cx: &Context<V>,
    handler: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> BoundPress {
    bind_press_rc(cx, Rc::new(handler))
}

/// [`bind_press`] for a handler that is already shared.
#[inline(never)]
pub(crate) fn bind_press_rc<V: 'static>(
    cx: &Context<V>,
    handler: Rc<dyn Fn(&mut V, &mut Window, &mut Context<V>)>,
) -> BoundPress {
    let view = cx.weak_entity();
    Rc::new(move |window, app| {
        let _ = view.update(app, |this, cx| handler(this, window, cx));
    })
}

/// `on_click` for a native modal control that also answers an accessibility press (VoiceOver,
/// cua-driver) by running the same handler.
///
/// CDXC:Accessibility 2026-10-01 WHY: GPUI answers a press on a node with no press listener by synthesising a mouse click at the node's last bounds, which lands on whatever is there now: nothing when the control is scrolled out of its list, the wrong row after a re-layout. The handler is called directly instead.
///
/// CDXC:Release 2026-10-22 WHY: `on_press` only binds the handler and hands it to the non-generic `on_bound_press`. With the body generic over the handler, the release binary carried one full copy per call site (485 copies, about 0.88 MB with their closures and drop glue).
pub(crate) trait PressExt: gpui::StatefulInteractiveElement + Sized {
    fn on_press<V: 'static>(
        self,
        cx: &Context<V>,
        handler: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        self.on_bound_press(bind_press(cx, handler))
    }

    /// [`PressExt::on_press`] with a handler bound by [`bind_press`].
    #[inline(never)]
    fn on_bound_press(self, press: BoundPress) -> Self {
        let click = Rc::clone(&press);
        self.on_click(move |_: &ClickEvent, window, app| click(window, app))
            .on_a11y_action(gpui::AccessibleAction::Click, move |_, window, app| {
                press(window, app)
            })
    }
}

impl<E: gpui::StatefulInteractiveElement> PressExt for E {}

/// `press` that also stops the click reaching the elements behind it: a control inside a switch
/// row (the switch itself, the reset asterisk, a button) runs its own handler and never the row's
/// toggle, so it cannot flip the switch twice or flip it by accident. See [`SwitchRowExt`].
pub(crate) fn stopping_press(press: BoundPress) -> BoundPress {
    Rc::new(move |window, app| {
        app.stop_propagation();
        press(window, app)
    })
}

/// Wraps a control that sits inside a switch row (a colour swatch, a button, a field) so a click on
/// it never reaches the row's toggle. See [`SwitchRowExt`].
pub(crate) fn click_shield(child: impl IntoElement) -> AnyElement {
    div()
        .id("click-shield")
        .on_click(|_, _, app| app.stop_propagation())
        .child(child)
        .into_any_element()
}

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
