//! The regions of the window's top band that move the window when dragged.

use std::sync::Mutex;

use gpui::InteractiveElement;
use gpui::MouseButton;
use gpui::WindowControlArea;

/// Where a left press landed on a drag region that has not moved the window yet. One slot serves
/// every region: there is one pointer.
static WINDOW_DRAG_PRESS: Mutex<Option<gpui::Point<gpui::Pixels>>> = Mutex::new(None);

/// How far the pointer travels before a press becomes a window move, so a click that wobbles on a
/// control inside a region stays a click.
const WINDOW_DRAG_THRESHOLD: f32 = 3.0;

fn set_window_drag_press(press: Option<gpui::Point<gpui::Pixels>>) {
    if let Ok(mut slot) = WINDOW_DRAG_PRESS.lock() {
        *slot = press;
    }
}

#[cfg(all(target_os = "macos", not(ghostex_web)))]
fn window_drag_is_app_owned(_window: &gpui::Window) -> bool {
    true
}

#[cfg(all(target_os = "linux", not(ghostex_web)))]
fn window_drag_is_app_owned(window: &gpui::Window) -> bool {
    matches!(
        window.window_decorations(),
        gpui::Decorations::Client { .. }
    )
}

// The browser build (apps/gpui-web) compiles this file too; a page has no window to drag.
#[cfg(any(all(target_os = "windows", not(ghostex_web)), ghostex_web))]
fn window_drag_is_app_owned(_window: &gpui::Window) -> bool {
    false
}

/// Whether the second press of a double click landed on a drag region itself rather than on a
/// control inside it, so the release that completes it zooms the window.
static WINDOW_DRAG_DOUBLE_PRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/*
CDXC:Titlebar 2026-08-23:
GPUI paints the whole top band itself, so AppKit's own titlebar view never sees a double click there
and the standard macOS zoom gesture silently did nothing. Forward it to the platform window, which
honours the user's NSGlobalDomain AppleActionOnDoubleClick preference (Maximize/Fill/Minimize/
Do Nothing). Linux compositors leave the same gesture to the client, so zoom directly there;
Windows already resolves it from the WindowControlArea::Drag hit test in the platform layer.
*/
#[cfg(all(target_os = "macos", not(ghostex_web)))]
fn window_drag_double_click_action(window: &gpui::Window) {
    window.titlebar_double_click();
}

#[cfg(all(target_os = "linux", not(ghostex_web)))]
fn window_drag_double_click_action(window: &gpui::Window) {
    window.zoom_window();
}

#[cfg(any(all(target_os = "windows", not(ghostex_web)), ghostex_web))]
fn window_drag_double_click_action(_window: &gpui::Window) {}

/// CDXC:Titlebar 2026-09-21 WHY:
/// The view tab strip sits in the window's top band, and on macOS AppKit moved the window from any
/// drag that started in that band, so dragging a tab to reorder it dragged the window instead.
/// Reporting only the tabs' rectangle to AppKit as app-owned was tried and did not stop it, so the
/// main window owns its titlebar drag outright (`app_owns_titlebar_drag` in main.rs) and the band's
/// empty regions hand the move to the platform themselves, the way the header already had to on
/// X11, which never reads GPUI's `WindowControlArea` hit boxes. The move starts once the
/// pointer has travelled a few pixels so clicks and the double-click zoom keep working. Controls inside a region
/// stop their own presses from reaching it, which is what keeps a tab drag a tab drag. Windows
/// resolves the same regions from the `Drag` hit test in the platform layer.
/// CDXC:Titlebar 2026-09-23 DECISION:
/// User: double-clicking the view tab strip, or an empty spot at the top of the sidebar, maximizes
/// the window the way double-clicking the header beside them does. So the double-click zoom
/// belongs to every drag region, not to the header alone. It needs both halves of the second click
/// on the region: the tabs stop only their presses and the sidebar's buttons only their clicks, and
/// double-clicking either must stay theirs.
pub(crate) fn window_drag_region<E: InteractiveElement>(element: E) -> E {
    element
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            let app_owned = window_drag_is_app_owned(window);
            log_window_drag(
                "press",
                event.position,
                serde_json::json!({ "appOwned": app_owned }),
            );
            set_window_drag_press(app_owned.then_some(event.position));
            WINDOW_DRAG_DOUBLE_PRESS.store(
                event.click_count == 2,
                std::sync::atomic::Ordering::Relaxed,
            );
            WINDOW_DRAG_MOVE_LOGGED.store(false, std::sync::atomic::Ordering::Relaxed);
        })
        .on_mouse_up(MouseButton::Left, |event, window, _| {
            set_window_drag_press(None);
            if WINDOW_DRAG_DOUBLE_PRESS.swap(false, std::sync::atomic::Ordering::Relaxed)
                && event.click_count == 2
            {
                window_drag_double_click_action(window);
            }
        })
        .on_mouse_move(|event, window, _| {
            let Some(press) = WINDOW_DRAG_PRESS.lock().ok().and_then(|slot| *slot) else {
                return;
            };
            if !WINDOW_DRAG_MOVE_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log_window_drag(
                    "firstMove",
                    event.position,
                    serde_json::json!({ "leftHeld": event.pressed_button == Some(MouseButton::Left) }),
                );
            }
            // A release that some control swallowed leaves the press behind; without the button
            // still held there is no drag to hand over.
            if event.pressed_button != Some(MouseButton::Left) {
                log_window_drag(
                    "pressDroppedNoButton",
                    event.position,
                    serde_json::json!({}),
                );
                set_window_drag_press(None);
                WINDOW_DRAG_DOUBLE_PRESS.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            if (event.position - press).magnitude() > f64::from(WINDOW_DRAG_THRESHOLD) {
                log_window_drag("startWindowMove", event.position, serde_json::json!({}));
                set_window_drag_press(None);
                WINDOW_DRAG_DOUBLE_PRESS.store(false, std::sync::atomic::Ordering::Relaxed);
                window.start_window_move();
            }
        })
}

/// Diagnostics only: whether this press's first move has been logged.
static WINDOW_DRAG_MOVE_LOGGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// A line in the Terminal focus diagnostic log (scenario `native.terminal.focus`) for the window
/// move a drag region hands to the platform; written only while that scenario is on.
pub(crate) fn log_window_drag(
    event: &str,
    position: gpui::Point<gpui::Pixels>,
    mut details: serde_json::Value,
) {
    if let Some(object) = details.as_object_mut() {
        object.insert("x".into(), serde_json::json!(position.x.as_f32().round()));
        object.insert("y".into(), serde_json::json!(position.y.as_f32().round()));
    }
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        &format!("gpui.windowDrag.{event}"),
        details,
    );
}
