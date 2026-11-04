//! Whether this host can float an agent's HTML page over the chat.
//!
//! SEE-ALSO: apps/gpui-web/src/app/native_chat/visual_popup.rs (the browser build) is the other copy of this file; it has no web runtime to host the page, so its cards open it as a link.

/// The desktop opens a page marked `"open": "popup"` in its visual page window
/// (apps/desktop/src/app/window/visual_page_modal.rs).
pub(super) const FLOATING_PAGES: bool = true;
