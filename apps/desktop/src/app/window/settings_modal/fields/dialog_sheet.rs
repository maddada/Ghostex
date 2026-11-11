//! The nested dialog with a close button (`<DialogContent nested showCloseButton>`: Arrange views,
//! Connect your accounts, the Marked sections preview, Playbook overrides) and the bare
//! backdrop other nested sheets reuse (the extension install consent). dialog.rs draws the variant
//! without a close button; this one puts the Settings window's own round X on its top-right
//! corner, caps the sheet at a `max-height` with its body (or the whole sheet) scrolling, and keeps
//! a key typed on the sheet from reaching the Settings search behind it.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, Bounds, Context, DispatchPhase, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseExitEvent, MouseMoveEvent, ParentElement as _, Pixels, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, canvas, deferred, div, point,
    px, size,
};
use gpui_component::v_flex;

/// `[data-slot='dialog-overlay'] { background: rgb(0 0 0 / 65%) }`.
const BACKDROP_ALPHA: f32 = 0.65;

/// The window-wide backdrop with `content` centered on it; a press on the backdrop runs
/// `on_dismiss` (Base UI closes a dialog on an outside press).
pub(crate) fn settings_dialog_overlay<V: 'static>(
    id: impl Into<SharedString>,
    content: AnyElement,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_dialog_layer(id, content, BACKDROP_ALPHA, on_dismiss, window, cx)
}

/// [`settings_dialog_overlay`] with its own backdrop tint (`backdrop_alpha` 0 dims nothing but still
/// catches the outside press). Since 2026-11-11 every dialog in Settings, the extension consents
/// included, takes the standard backdrop (CDXC:Settings 2026-11-11 on
/// `SettingsPalette::dialog_surface`).
pub(crate) fn settings_dialog_layer<V: 'static>(
    id: impl Into<SharedString>,
    content: AnyElement,
    backdrop_alpha: f32,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let id: SharedString = id.into();
    let viewport = window.viewport_size();
    deferred(
        anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(point(px(0.0), px(0.0)))
            .child(
                div()
                    .id(SharedString::from(format!("{id}-backdrop")))
                    .occlude()
                    .w(viewport.width)
                    .h(viewport.height)
                    .bg(hsla(super::nested_glass::nested_backdrop(
                        window,
                        backdrop_alpha,
                    )))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |page, _: &MouseDownEvent, window, cx| {
                            on_dismiss(page, window, cx);
                        }),
                    )
                    .child(content),
            ),
    )
    .with_priority(2)
    .into_any_element()
}

/// The nested dialog. `focus` is tracked on the sheet (focus it when the dialog opens) so Escape
/// closes it. `scroll_all` scrolls the whole sheet (the account guide); otherwise the body is a
/// flexible column whose own children scroll (the view order list).
///
/// CDXC:AppModal 2026-10-19 DECISION:
/// User: "please make the x button on the top right of this appear just like the x button we have for the settings modal itself", then "the x button for the modals in settings that appear inside the settings (like orchestrator instructions for example) need to be only shown when i hover near the top of that modal please (for all similar modals too)". Every sheet nested in Settings closes with the Settings window's own corner X (`modal_corner_close_button`: same size, icon, colours, hover, centred on the top-right corner) instead of the filled square, and like it the X shows only while the pointer is in the sheet's top 80px (`MODAL_CORNER_CLOSE_REVEAL_HEIGHT`) or on the X itself.
/// SEE-ALSO: window/modal_window_frame.rs (the app modals' X and its top band).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_dialog_sheet<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    description: Option<AnyElement>,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    scroll_all: bool,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    dialog_sheet(
        p,
        id,
        focus,
        title,
        description,
        width,
        max_height,
        padding,
        gap,
        scroll_all,
        body,
        footer,
        on_close,
        false,
        window,
        cx,
    )
}

/// [`settings_dialog_sheet`] whose X shows at rest: the preview binary's screenshots, which cannot
/// hover.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_dialog_sheet_close_shown<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    description: Option<AnyElement>,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    dialog_sheet(
        p,
        id,
        focus,
        title,
        description,
        width,
        max_height,
        padding,
        gap,
        false,
        body,
        footer,
        on_close,
        true,
        window,
        cx,
    )
}

/// Whether the sheet's X shows: the pointer is in the sheet's top band or on the X.
#[derive(Default)]
struct CloseReveal {
    shown: bool,
}

/// Tracks the pointer over the sheet (`bounds`) the way the app modals' frame does
/// (`ModalWindowFrame::top_band_tracker`): window-wide listeners, so the sheet's own controls
/// cannot swallow the moves and nothing under it loses input.
fn close_reveal_tracker(reveal: gpui::Entity<CloseReveal>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let half = MODAL_CORNER_CLOSE_SIZE / 2.0;
            let corner = bounds.top_right();
            let button = Bounds::new(
                point(corner.x - px(half), corner.y - px(half)),
                size(px(MODAL_CORNER_CLOSE_SIZE), px(MODAL_CORNER_CLOSE_SIZE)),
            );
            let set = |reveal: &gpui::Entity<CloseReveal>, shown: bool, cx: &mut gpui::App| {
                reveal.update(cx, |reveal, cx| {
                    if reveal.shown != shown {
                        reveal.shown = shown;
                        cx.notify();
                    }
                });
            };
            let move_reveal = reveal.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let position = event.position;
                let in_band = bounds.contains(&position)
                    && f32::from(position.y - bounds.top()) <= MODAL_CORNER_CLOSE_REVEAL_HEIGHT;
                set(&move_reveal, in_band || button.contains(&position), cx);
            });
            let exit_reveal = reveal.clone();
            window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    set(&exit_reveal, false, cx);
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

#[allow(clippy::too_many_arguments)]
fn dialog_sheet<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    description: Option<AnyElement>,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    scroll_all: bool,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: impl Fn(&mut V, &mut Window, &mut Context<V>) + Clone + 'static,
    close_always_shown: bool,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let id: SharedString = id.into();
    let reveal = window.use_keyed_state(
        SharedString::from(format!("{id}-close-reveal")),
        cx,
        |_, _| CloseReveal::default(),
    );
    let shown = close_always_shown || reveal.read(cx).shown;
    let close = {
        let on_close = on_close.clone();
        let half = MODAL_CORNER_CLOSE_SIZE / 2.0;
        modal_corner_close_button(
            &p.modal,
            SharedString::from(format!("{id}-close")),
            if shown {
                ModalCornerCloseReveal::Shown
            } else {
                ModalCornerCloseReveal::Hidden
            },
            move |page: &mut V, window, cx| on_close(page, window, cx),
            cx,
        )
        .absolute()
        .top(px(-half))
        .map(|close| {
            // The top-left corner on macOS (CDXC:AppModal 2026-11-10 in
            // window/round_close_button.rs).
            if crate::app::window::round_close_button::close_buttons_on_left() {
                close.left(px(-half))
            } else {
                close.right(px(-half))
            }
        })
        .size(px(MODAL_CORNER_CLOSE_SIZE))
        // The half outside the sheet sits over the backdrop, whose press would close the sheet
        // on mouse-down instead of on the button's own click.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    };
    let header = v_flex()
        .w_full()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(p.foreground))
                .child(title.into()),
        )
        .children(description.map(|description| {
            div()
                .w_full()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(p.muted))
                .child(description)
        }));
    let focus_for_keys = focus.clone();
    let on_escape = on_close.clone();
    let viewport = window.viewport_size();
    let content = v_flex()
        .w_full()
        .gap(px(gap))
        .when(!scroll_all, |this| this.flex_1().min_h_0())
        .child(header)
        .child(
            v_flex()
                .w_full()
                .gap(px(gap))
                .when(!scroll_all, |this| this.flex_1().min_h_0())
                .children(body),
        )
        .children(footer);
    let sheet = super::dialog::settings_dialog_surface(p, div())
        .id(id.clone())
        .track_focus(focus)
        .occlude()
        .relative()
        .flex()
        .flex_col()
        .w(px(width))
        .max_w(viewport.width - px(32.0))
        .max_h(px(max_height.unwrap_or(f32::MAX)).min(viewport.height - px(32.0)))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(cx.listener(move |page, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                on_escape(page, window, cx);
                return;
            }
            if focus_for_keys.is_focused(window) {
                cx.stop_propagation();
            }
        }))
        .child(if scroll_all {
            div()
                .id(SharedString::from(format!("{id}-scroll")))
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(px(padding))
                .child(content)
                .into_any_element()
        } else {
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .p(px(padding))
                .child(content)
                .into_any_element()
        })
        .child(close_reveal_tracker(reveal))
        .child(close)
        .into_any_element();
    settings_dialog_overlay(
        id,
        sheet,
        move |page, window, cx| on_close(page, window, cx),
        window,
        cx,
    )
}
