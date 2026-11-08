//! A pasted picture the chat has not received yet (`composer_attach.rs` holds a send for it).
//!
//! The page's paste event carries a picture as a file, and GPUI's web platform reads the file's
//! bytes before it hands the paste to the focused field, so Enter pressed meanwhile reached the
//! chat before the paste did and the text left alone. A capturing listener on the document notes
//! the paste at the event itself; the chat's paste hook clears it (`paste_arrived`).

use std::cell::{Cell, RefCell};
use wasm_bindgen::{JsCast as _, closure::Closure};

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    static ON_ITS_WAY: RefCell<Option<web_time::Instant>> = const { RefCell::new(None) };
}

/// Starts watching the page's pastes, once.
pub(super) fn watch() {
    if INSTALLED.with(|installed| installed.replace(true)) {
        return;
    }
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let listener = Closure::<dyn FnMut(web_sys::Event)>::new(|event: web_sys::Event| {
        let Some(data) = event
            .dyn_ref::<web_sys::ClipboardEvent>()
            .and_then(web_sys::ClipboardEvent::clipboard_data)
        else {
            return;
        };
        let items = data.items();
        let picture = (0..items.length())
            .filter_map(|index| items.get(index))
            .any(|item| item.kind() == "file" && item.type_().starts_with("image/"));
        if picture {
            ON_ITS_WAY.with(|since| *since.borrow_mut() = Some(web_time::Instant::now()));
        }
    });
    let _ = document.add_event_listener_with_callback_and_bool(
        "paste",
        listener.as_ref().unchecked_ref(),
        true,
    );
    listener.forget();
}

/// When an image paste the chat has not received yet began, if one is on its way.
pub(super) fn image_paste_on_its_way() -> Option<web_time::Instant> {
    watch();
    ON_ITS_WAY.with(|since| *since.borrow())
}

/// A paste reached the chat's paste hook.
pub(super) fn paste_arrived() {
    watch();
    ON_ITS_WAY.with(|since| *since.borrow_mut() = None);
}
