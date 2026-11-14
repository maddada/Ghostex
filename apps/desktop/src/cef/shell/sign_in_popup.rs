//! Sign-in popups (Google, Apple, GitHub, any OAuth provider a page opens with `window.open` and
//! popup features): a real native window CEF creates for the page, in the page's own profile,
//! with `window.opener` and `postMessage` intact, which closes when the provider closes it.
//!
//! CDXC:Browser 2026-10-23 WHY:
//! Every popup used to become a Browser tab (CDXC:Browser 2026-06-22 in browser_handlers.rs), which cancels Chromium's popup: `window.open` returns nothing to the page, so Google Identity Services reported "Failed to open popup window" and claude.ai's Continue with Google failed in the Browser (seen 2026-10-23 in a Claude account's profile). A sign-in popup needs its opener to hand the result back, so a `window.open` that asks for a popup (Chromium's NEW_POPUP disposition, from popup features such as a width) or an OAuth page opened with `display=popup` is let through as its own native window, like the DevTools window. Plain `target=_blank` links and `window.open` without popup features still open as tabs.

use super::*;

/// Whether a popup request is a sign-in style popup the page has to keep talking to.
pub(crate) fn is_sign_in_popup(
    disposition: WindowOpenDisposition,
    target_url: Option<&CefString>,
) -> bool {
    if disposition == WindowOpenDisposition::NEW_POPUP {
        return true;
    }
    target_url
        .map(CefString::to_string)
        .is_some_and(|url| url.starts_with("https://") && url.contains("display=popup"))
}

/// Turns CEF's pre-filled popup into a top-level native window sized as the page asked (500x650
/// otherwise), on a client that lets its own close request through.
pub(crate) fn prepare_sign_in_popup(
    window_info: &mut WindowInfo,
    client: &mut Option<Client>,
    popup_features: Option<&PopupFeatures>,
) {
    let feature = |set: c_int, value: c_int, fallback: c_int| {
        if set != 0 && value > 0 {
            value
        } else {
            fallback
        }
    };
    let (width, height, x, y) = match popup_features {
        Some(features) => (
            feature(features.width_set, features.width, 500).clamp(320, 1200),
            feature(features.height_set, features.height, 650).clamp(320, 1000),
            feature(features.x_set, features.x, 160),
            feature(features.y_set, features.y, 120),
        ),
        None => (500, 650, 160, 120),
    };
    let runtime_style = window_info.runtime_style;
    *window_info = WindowInfo {
        window_name: CefString::from("Sign in"),
        bounds: cef::Rect {
            x,
            y,
            width,
            height,
        },
        runtime_style,
        ..Default::default()
    };
    *client = Some(GhostexGpuiCefClient::new(
        Some(GhostexSignInPopupLifeSpanHandler::new()),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(GhostexGpuiCefFocusHandler::new()),
        None,
        None,
    ));
}

wrap_life_span_handler! {
    pub(crate) struct GhostexSignInPopupLifeSpanHandler {}

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut cef::Browser>) {
            // The same focus registration the DevTools window gets.
            let Some(browser) = browser else {
                return;
            };
            let mut task = GhostexRegisterDevToolsNativeView::new(browser.clone());
            post_task(ThreadId::UI, Some(&mut task));
        }

        fn do_close(&self, _browser: Option<&mut cef::Browser>) -> c_int {
            // A top-level window of its own: the provider's `window.close()` (or the window's
            // close button) closes it the normal way.
            0
        }

        fn on_before_close(&self, browser: Option<&mut cef::Browser>) {
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return;
            };
            unregister_native_view_browser(platform::native_view_ptr(host.window_handle()));
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: c_int,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
            popup_features: Option<&PopupFeatures>,
            window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut c_int>,
        ) -> c_int {
            // A provider's own popup (an account chooser) opens the same way.
            if let (Some(window_info), Some(client)) = (window_info, client) {
                prepare_sign_in_popup(window_info, client, popup_features);
            }
            0
        }
    }
}
