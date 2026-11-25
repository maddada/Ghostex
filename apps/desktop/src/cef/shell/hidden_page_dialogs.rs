//! Native panels a page that is not on screen may not raise: the Open panel of a file chooser is
//! cancelled, and the Save panel of a download waits until the page is shown.
//!
//! CDXC:Browser 2026-11-26 DECISION:
//! Coordinator, on the user's behalf (security review of #762, findings B4 and B12): a page the user cannot see (an agent's background page, a tab behind another, a parked project's Browser) must not bring a native panel up in front of them, because a file they pick in an Open panel goes to that page, and a Save panel they did not ask for reads as their own download. So a hidden page's file chooser is cancelled (the page gets the same answer as a person pressing Cancel; an agent fills upload fields with `ghostex_set_input_files` instead), and its download is held, without a panel, until the page is shown, when the Save panel opens as usual. One download waits per page; another one while it waits is cancelled. A sign-in popup from a hidden page opens as a background tab instead of its own window (browser_handlers.rs).
//! SEE-ALSO: apps/desktop/src/cef/shell/devtools_relay.rs (file choosers are also intercepted on every page an agent's DevTools session drives), apps/desktop/src/cef/shell/browser.rs (`set_visible`, which marks a page hidden and lets its held download go).

use super::*;
use cef::{
    DialogHandler, FileDialogCallback, FileDialogMode, ImplDialogHandler,
    ImplFileDialogCallback as _, WrapDialogHandler, wrap_dialog_handler,
};

thread_local! {
    /// The download a hidden page asked for, by the page's native view: where it would be saved
    /// and the callback that opens its Save panel.
    static HELD_DOWNLOADS: RefCell<HashMap<usize, (BeforeDownloadCallback, PathBuf)>> =
        RefCell::new(HashMap::new());
}

/// Whether `browser`'s page is not on screen (its view is hidden).
pub(crate) fn browser_is_off_screen(browser: &cef::Browser) -> bool {
    browser.host().is_some_and(|host| {
        cef_native_view_is_hidden(platform::native_view_ptr(host.window_handle()))
    })
}

/// Holds a hidden page's download until the page is shown. Returns false when the page is on
/// screen (the Save panel may open now) or already has one waiting (this one is cancelled).
pub(crate) fn hold_hidden_page_download(
    browser: &cef::Browser,
    callback: &BeforeDownloadCallback,
    download_path: &std::path::Path,
) -> HeldDownload {
    let Some(host) = browser.host() else {
        return HeldDownload::NotHidden;
    };
    let native_view = platform::native_view_ptr(host.window_handle());
    if !cef_native_view_is_hidden(native_view) {
        return HeldDownload::NotHidden;
    }
    HELD_DOWNLOADS.with(|held| {
        let mut held = held.borrow_mut();
        if held.contains_key(&(native_view as usize)) {
            return HeldDownload::Refused;
        }
        held.insert(
            native_view as usize,
            (callback.clone(), download_path.to_path_buf()),
        );
        HeldDownload::Held
    })
}

/// What `hold_hidden_page_download` did with a download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeldDownload {
    /// The page is on screen: ask where to save now.
    NotHidden,
    /// It waits for the page to be shown.
    Held,
    /// Another download of the page already waits: this one is cancelled.
    Refused,
}

/// The page of `native_view` is on screen again: its held download asks where to save.
pub(crate) fn release_held_download(native_view: *mut c_void) {
    let Some((callback, download_path)) =
        HELD_DOWNLOADS.with(|held| held.borrow_mut().remove(&(native_view as usize)))
    else {
        return;
    };
    callback.cont(
        Some(&CefString::from(download_path.to_string_lossy().as_ref())),
        1,
    );
}

/// The page of `native_view` closed: its held download goes with it.
pub(crate) fn forget_held_download(native_view: *mut c_void) {
    HELD_DOWNLOADS.with(|held| held.borrow_mut().remove(&(native_view as usize)));
}

wrap_dialog_handler! {
    pub(crate) struct GhostexHiddenPageDialogHandler {}

    impl DialogHandler {
        fn on_file_dialog(
            &self,
            browser: Option<&mut cef::Browser>,
            _mode: FileDialogMode,
            _title: Option<&CefString>,
            _default_file_path: Option<&CefString>,
            _accept_filters: Option<&mut cef::CefStringList>,
            _accept_extensions: Option<&mut cef::CefStringList>,
            _accept_descriptions: Option<&mut cef::CefStringList>,
            callback: Option<&mut FileDialogCallback>,
        ) -> c_int {
            let hidden = browser.is_some_and(|browser| browser_is_off_screen(browser));
            if !hidden {
                // On screen: the usual Open panel.
                return 0;
            }
            if let Some(callback) = callback {
                callback.cancel();
            }
            1
        }
    }
}
