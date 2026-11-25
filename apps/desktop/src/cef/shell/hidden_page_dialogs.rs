//! Native panels a page may not raise: the Open panel of a file chooser that a page not on screen
//! asked for, or that an agent's own input opened, is cancelled, and the Save panel of a download a
//! hidden page started waits until the page is shown.
//!
//! CDXC:Browser 2026-11-26 DECISION:
//! Coordinator, on the user's behalf (security review of #762, findings B4 and B12): a page the user cannot see (an agent's background page, a tab behind another, a parked project's Browser) must not bring a native panel up in front of them, because a file they pick in an Open panel goes to that page, and a Save panel they did not ask for reads as their own download. So a hidden page's file chooser is cancelled (the page gets the same answer as a person pressing Cancel; an agent fills upload fields with `ghostex_set_input_files` instead), and its download is held, without a panel, until the page is shown, when the Save panel opens as usual. One download waits per page; another one while it waits is cancelled. A sign-in popup from a hidden page opens as a background tab instead of its own window (browser_handlers.rs).
//! Coordinator, on the user's behalf (#762 follow-up, replacing the DevTools interception that cancelled every chooser while an agent was attached): on a page on screen, a file chooser opens normally when a person opened it, and is cancelled when the agent's own input did. The page cannot tell them apart (DevTools input is trusted and gives the page user activation), so the app compares when a DevTools session last acted on the page (devtools_relay.rs) with when a real person last pressed a key or mouse button on that same page's view. The agent's input being the newer one, within the 10 seconds a page's activation can last, cancels the chooser, and the agent's tools are told (`Ghostex.fileChooserCancelled`).
//! Coordinator, on the user's behalf (re-check of #762, N3): the agent's time is when the page answered its command, not only when it was sent, and counts as now while the command waits, so a page kept busy until a queued press lands cannot let that press open a chooser; every command that can run page script counts (input, scripts, DOM changes, navigations). A real press counts only on the page's own view, never anywhere else on the computer: macOS reports mouse-downs from the page's view and key-downs from the app's `sendEvent:` observer for the view holding focus, Windows from input hooks on the thread that owns Chromium's windows (mouse moves do not count), and Linux from the XI2 presses on Chromium's window (mouse only; a chooser opened with the keyboard soon after an agent's input is cancelled, as on Wayland, where no press is seen).
//! SEE-ALSO: apps/desktop/src/cef/shell/devtools_relay.rs (when an agent's DevTools session last acted on each page), apps/desktop/src/cef/shell/browser.rs (`set_visible`, which marks a page hidden and lets its held download go).

use super::*;
use cef::{
    DialogHandler, FileDialogCallback, FileDialogMode, ImplDialogHandler,
    ImplFileDialogCallback as _, WrapDialogHandler, wrap_dialog_handler,
};

/// How long after an agent's input a file chooser can still be its doing: a page's user
/// activation lasts about five seconds, so this leaves room.
const AGENT_GESTURE_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

thread_local! {
    /// When a real person last pressed a key or mouse button on each page, by the page's native
    /// view: the presses the app's own views receive, which DevTools input never reaches.
    static REAL_PRESSES: RefCell<HashMap<usize, Instant>> = RefCell::new(HashMap::new());
}

/// A real person pressed a key or mouse button on `native_view` (macOS: native_view.rs and the
/// app's `sendEvent:` observer; Windows: the thread's input hooks in cef/windows.rs; Linux: the XI2
/// presses in linux_x11_pointer_focus.rs). Views that are not a page are ignored, so a caller may
/// report every view from the one pressed up to its window.
pub(crate) fn note_real_user_press(native_view: *mut c_void) {
    if native_view.is_null() {
        return;
    }
    let key = native_view as usize;
    if !CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| browsers.borrow().contains_key(&key)) {
        return;
    }
    REAL_PRESSES.with(|presses| presses.borrow_mut().insert(key, Instant::now()));
}

/// The page of `native_view` closed: its press record goes with it.
pub(crate) fn forget_real_user_presses(native_view: *mut c_void) {
    REAL_PRESSES.with(|presses| presses.borrow_mut().remove(&(native_view as usize)));
}

/// How long ago a real person last pressed a key or mouse button on `browser`'s own page.
fn since_real_press_on(browser: &cef::Browser) -> Option<std::time::Duration> {
    let native_view = platform::native_view_ptr(browser.host()?.window_handle());
    REAL_PRESSES.with(|presses| {
        presses
            .borrow()
            .get(&(native_view as usize))
            .map(Instant::elapsed)
    })
}

/// Whether a file chooser `browser` asks for now came from an agent's input rather than a
/// person's: an agent's DevTools session acted on the page within `AGENT_GESTURE_WINDOW` (or still
/// waits for the page to answer it), and no real press on this page came after it.
fn opened_by_agent_input(browser: &cef::Browser) -> bool {
    let Some(agent) = cef_devtools_relay_agent_input_age(browser.identifier()) else {
        return false;
    };
    agent < AGENT_GESTURE_WINDOW
        && !since_real_press_on(browser).is_some_and(|since_person| since_person < agent)
}

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
            let Some(browser) = browser else {
                return 0;
            };
            let by_agent = opened_by_agent_input(browser);
            if !by_agent && !browser_is_off_screen(browser) {
                // On screen and opened by a person: the usual Open panel.
                return 0;
            }
            if let Some(callback) = callback {
                callback.cancel();
            }
            if by_agent {
                cef_devtools_relay_note_file_chooser_cancelled(browser.identifier());
            }
            1
        }
    }
}
