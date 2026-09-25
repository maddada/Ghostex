/// CDXC:FocusRouting 2026-09-17 WHY:
/// GPUI focus handles do not transfer native first responder from a Chromium child.
/// The chat's own pointer and editor focus edges reclaim its native keyboard owner so transcript Copy and editor shortcuts reach the selected native control.
pub(super) fn reclaim_keyboard_focus(window: &gpui::Window) {
    if let Ok(root) = crate::app::helpers::cef_parent_native_view(window) {
        crate::cef::focus_gpui_root_view(root);
    }
}
