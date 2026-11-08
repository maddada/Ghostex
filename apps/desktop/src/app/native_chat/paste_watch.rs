//! A paste whose picture the chat has not received yet (`composer_attach.rs` holds a send for it).
//! The desktop sees every paste as the `Paste` action, before the clipboard read
//! (`note_paste_started`), so it has nothing to watch here. The web build's own copy watches the
//! page's paste events, whose pictures reach the chat only after the browser reads their files.

/// Starts watching the page's pastes (the web build); nothing on the desktop.
pub(super) fn watch() {}

/// When an image paste the chat has not received yet began, if one is on its way.
pub(super) fn image_paste_on_its_way() -> Option<web_time::Instant> {
    None
}

/// A paste reached the chat's paste hook.
pub(super) fn paste_arrived() {}
