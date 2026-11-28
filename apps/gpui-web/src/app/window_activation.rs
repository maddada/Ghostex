//! The browser build's answer to the desktop's `app/window_activation.rs`: a page never brings
//! itself in front of another app, so the shared files' activation calls go through as they are.

use gpui::{App, Window};

pub(crate) fn may_take_keyboard(_cx: &App) -> bool {
    true
}

pub(crate) fn activate_window_if_frontmost(
    window: &mut Window,
    _cx: &App,
    _reason: &'static str,
) -> bool {
    window.activate_window();
    true
}

pub(crate) fn log_held(_reason: &str) {}
