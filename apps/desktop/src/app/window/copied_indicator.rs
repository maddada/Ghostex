//! The "Copied!" feedback every clipboard write in the app shows: the clicked control's tooltip,
//! or a bubble at the pointer.

use gpui::{
    AnyView, AnyWindowHandle, App, AppContext as _, Bounds, Context, Global, IntoElement,
    ParentElement as _, Pixels, Point, Render, Styled as _, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, point, px, size,
};
use gpui_component::{ActiveTheme as _, Root, tooltip::Tooltip};
use std::time::Duration;
use web_time::Instant;

/// How long a clicked control's tooltip reads "Copied!" before its own text comes back.
const TOOLTIP_FLASH: Duration = Duration::from_millis(1000);
/// How far GPUI's pointer may be from the OS pointer and still count as the same place.
const POINTER_TOLERANCE: f32 = 2.0;
/// How long the bubble stays fully visible.
const HOLD: Duration = Duration::from_millis(900);
/// How long it takes to fade out after that.
const FADE: Duration = Duration::from_millis(200);
/// The bubble's own window: wide enough for the label in any font, with room around the bubble for
/// its shadow.
const WINDOW_WIDTH: f32 = 140.0;
/// The bubble's window for a failed copy, wide enough for "Couldn't copy the image".
const FAILED_WINDOW_WIDTH: f32 = 240.0;
const WINDOW_HEIGHT: f32 = 48.0;
/// Room under the bubble inside its window, where the shadow falls.
const SHADOW_INSET: f32 = 10.0;
/// From the pointer's tip up to the bubble's bottom edge.
const POINTER_GAP: f32 = 14.0;

#[derive(Default)]
struct CopiedIndicator {
    window: Option<AnyWindowHandle>,
    generation: u64,
}

impl Global for CopiedIndicator {}

struct CopiedIndicatorView {
    shown_at: Instant,
    label: &'static str,
}

impl Render for CopiedIndicatorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let elapsed = self.shown_at.elapsed();
        let opacity = if elapsed <= HOLD {
            1.0
        } else {
            (1.0 - (elapsed - HOLD).as_secs_f32() / FADE.as_secs_f32()).clamp(0.0, 1.0)
        };
        if elapsed > HOLD && opacity > 0.0 {
            window.request_animation_frame();
        }
        // The same card gpui-component's tooltips draw, so it reads as one of the app's tooltips.
        let theme = cx.theme();
        let font_family = theme.font_family.clone();
        let (background, foreground, border) =
            (theme.tokens.popover, theme.popover_foreground, theme.border);
        div()
            .size_full()
            .flex()
            .items_end()
            .justify_center()
            .pb(px(SHADOW_INSET))
            .child(
                div()
                    .font_family(font_family)
                    .bg(background)
                    .text_color(foreground)
                    .border_1()
                    .border_color(border)
                    .shadow_md()
                    .rounded(px(6.0))
                    .py_0p5()
                    .px_2()
                    .text_sm()
                    .opacity(opacity)
                    .child(self.label),
            )
    }
}

/// CDXC:Clipboard 2026-09-24 DECISION:
/// User: "We need to add copy indicators for every place that you copy in the app. We need to show
/// a small tooltip whenever they copy that appears on screen that says 'Copied!'". Then, because
/// the bubble clashed with the copy button's own tooltip: "make the tooltip say 'Copied!' for a bit
/// instead of having a different element do this". So a copy from a control that has a tooltip
/// turns that tooltip into "Copied!" for a moment and shows no bubble; every other copy (menus,
/// keyboard, terminal selection, toasts, React pages) shows the bubble. The bubble is
/// its own non-activating popup window above the pointer, not an element in the window that copied:
/// copies happen from the main window, from chat menus and pickers, from app modals and from toasts,
/// and from React pages inside CEF views (through the copy-sound bridge message), and one popup
/// serves them all without each root drawing an overlay. It never takes key status, ignores the
/// mouse, and lives in every Space, so it shows over a fullscreen window as well.
pub(crate) fn show_copied_indicator(cx: &mut App) {
    show_indicator("Copied!", WINDOW_WIDTH, cx);
}

/// CDXC:Clipboard 2026-11-11 WHY: a copy that did not land (Copy image on Windows did nothing at
/// all) says so where "Copied!" would have shown, in the same tooltip or bubble, with the
/// failure's own plain words, so the user never takes silence for success.
pub(crate) fn show_copy_failed_indicator(message: &'static str, cx: &mut App) {
    show_indicator(message, FAILED_WINDOW_WIDTH, cx);
}

fn show_indicator(label: &'static str, window_width: f32, cx: &mut App) {
    // Opened outside the update that copied, the way every other popup of the app is opened.
    cx.defer(move |cx| {
        let Some(pointer) = pointer_position(cx) else {
            return;
        };
        if flash_clicked_tooltip(pointer, label, cx) {
            return;
        }
        if !cx.has_global::<CopiedIndicator>() {
            cx.set_global(CopiedIndicator::default());
        }
        if let Some(previous) = cx.global_mut::<CopiedIndicator>().window.take() {
            let _ = previous.update(cx, |_, window, _| window.remove_window());
        }
        let generation = {
            let indicator = cx.global_mut::<CopiedIndicator>();
            indicator.generation += 1;
            indicator.generation
        };
        let size = size(px(window_width), px(WINDOW_HEIGHT));
        let mut origin = point(
            pointer.x - size.width / 2.0,
            pointer.y - px(POINTER_GAP) - size.height + px(SHADOW_INSET),
        );
        if let Some(bounds) = cx.displays().into_iter().find_map(|display| {
            let (bounds, _) = crate::app::window::popup_frame::global_display_frames(&*display);
            bounds.contains(&pointer).then_some(bounds)
        }) {
            origin.x = origin.x.max(bounds.left()).min(bounds.right() - size.width);
            origin.y = origin
                .y
                .max(bounds.top())
                .min(bounds.bottom() - size.height);
        }
        let (bounds, display_id) =
            crate::app::window::popup_frame::place_global(Bounds::new(origin, size), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                titlebar: None,
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                app_id: crate::gpui_platform_window_app_id(),
                icon: crate::gpui_platform_window_icon(),
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                prepare_window(window);
                cx.new(|cx| {
                    // The first frame after the hold starts the fade; the frames after that
                    // request themselves.
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(HOLD).await;
                        let _ = this.update(cx, |_, cx| cx.notify());
                    })
                    .detach();
                    CopiedIndicatorView {
                        shown_at: Instant::now(),
                        label,
                    }
                })
            },
        );
        let Ok(handle) = result else {
            return;
        };
        cx.global_mut::<CopiedIndicator>().window = Some(handle.into());
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(HOLD + FADE + Duration::from_millis(50))
                .await;
            cx.update(|cx| {
                let indicator = cx.global_mut::<CopiedIndicator>();
                if indicator.generation != generation {
                    return;
                }
                indicator.window = None;
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        })
        .detach();
    });
}

/// Turns the tooltip of the control under the pointer in the active window into "Copied!", trying
/// gpui-component's managed tooltips (the trigger just pressed) before GPUI's own. False when no
/// such control is there, including when GPUI's idea of the pointer is stale because the click
/// went to a CEF or terminal view that takes the mouse events.
fn flash_clicked_tooltip(pointer: Point<Pixels>, label: &'static str, cx: &mut App) -> bool {
    let Some(handle) = cx.active_window() else {
        return false;
    };
    handle
        .update(cx, |_, window, cx| {
            let gpui_pointer = window_pointer(window, cx);
            if (gpui_pointer.x - pointer.x).abs() > px(POINTER_TOLERANCE)
                || (gpui_pointer.y - pointer.y).abs() > px(POINTER_TOLERANCE)
            {
                return false;
            }
            let tooltip = move |window: &mut Window, cx: &mut App| -> AnyView {
                Tooltip::new(label).build(window, cx)
            };
            Root::flash_pressed_tooltip(window, cx, TOOLTIP_FLASH, tooltip)
                || window.flash_hovered_tooltip(TOOLTIP_FLASH, tooltip, cx)
        })
        .unwrap_or(false)
}

/// GPUI's last pointer position in `window`, in the shared space [`pointer_position`] returns.
fn window_pointer(window: &Window, cx: &App) -> Point<Pixels> {
    let bounds = window.bounds();
    // The window's frame includes a system titlebar where it has one; content coordinates start
    // under it.
    let titlebar = (bounds.size.height - window.viewport_size().height).max(px(0.0));
    crate::app::window::popup_frame::PopupOwner::of(window, cx).to_global(
        bounds.origin + point(px(0.0), titlebar) + window.mouse_position(),
        cx,
    )
}

/// The pointer in the space every display shares (`popup_frame::place_global`): points from the
/// top-left corner of the primary display.
fn pointer_position(cx: &mut App) -> Option<Point<Pixels>> {
    #[cfg(target_os = "macos")]
    {
        // Read from the OS: GPUI's own record is stale while the pointer is over a CEF or terminal
        // view, which take the mouse events, and React pages copy from exactly there.
        let (mut x, mut y) = (0f64, 0f64);
        if unsafe { GhostexGpuiPointerScreenLocation(&mut x, &mut y) } {
            return Some(point(px(x as f32), px(y as f32)));
        }
    }
    let window = cx.active_window()?;
    window
        .update(cx, |_, window, cx| window_pointer(window, cx))
        .ok()
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn GhostexGpuiPointerScreenLocation(x: *mut f64, y: *mut f64) -> bool;
    fn GhostexGpuiPrepareCopiedIndicatorWindow(native_view: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
fn prepare_window(window: &mut Window) {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    if let Ok(handle) = window.window_handle()
        && let RawWindowHandle::AppKit(handle) = handle.as_raw()
    {
        unsafe { GhostexGpuiPrepareCopiedIndicatorWindow(handle.ns_view.as_ptr()) };
    }
}

#[cfg(not(target_os = "macos"))]
fn prepare_window(_: &mut Window) {}
