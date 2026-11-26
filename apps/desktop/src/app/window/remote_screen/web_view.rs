//! The system web view that shows a computer's screen in the remote screen window: WKWebView
//! through native/macos/GpuiRemoteScreenWebView.m, WebView2 through wry on Windows. It is a child
//! view of the window placed at the bounds the window lays out, and it is removed when this value
//! drops. Linux has none yet (`SUPPORTED` is false).
//!
//! CDXC:RemoteScreen 2026-11-27 WHY: not CEF. The viewer decodes H.264 with WebCodecs and
//! Ghostex's CEF has no H.264 (VideoDecoder.isConfigSupported false for every avc1 profile in a
//! test copy, 2026-11-27); WKWebView and WebView2 both decode it in hardware. The view stays on the
//! viewer's own address and opens no other windows.
use gpui::{Bounds, Pixels, Window};
use std::cell::Cell;

pub(crate) struct RemoteScreenWebView {
    backend: backend::Backend,
    frame: Cell<Option<Bounds<Pixels>>>,
}

impl RemoteScreenWebView {
    pub(crate) const SUPPORTED: bool = backend::SUPPORTED;

    /// Starts loading `url` (gxserver's viewer address), hidden until the first `place`.
    pub(crate) fn new(window: &Window, url: &str) -> Result<Self, String> {
        Ok(Self {
            backend: backend::Backend::new(window, url)?,
            frame: Cell::new(None),
        })
    }

    /// Moves the view onto `bounds` (window coordinates, logical pixels); unchanged bounds cost nothing.
    pub(crate) fn place(&self, bounds: Bounds<Pixels>) {
        if self.frame.get() == Some(bounds) {
            return;
        }
        self.frame.set(Some(bounds));
        self.backend.set_frame(
            f64::from(bounds.origin.x.as_f32()),
            f64::from(bounds.origin.y.as_f32()),
            f64::from(bounds.size.width.as_f32()),
            f64::from(bounds.size.height.as_f32()),
        );
    }
}

#[cfg(target_os = "macos")]
mod backend {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::{CString, c_char, c_void};

    pub(super) const SUPPORTED: bool = true;

    unsafe extern "C" {
        fn GhostexGpuiRemoteScreenWebViewCreate(
            parent_view: *mut c_void,
            url: *const c_char,
        ) -> *mut c_void;
        fn GhostexGpuiRemoteScreenWebViewSetFrame(
            handle: *mut c_void,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        );
        fn GhostexGpuiRemoteScreenWebViewDestroy(handle: *mut c_void);
    }

    pub(super) struct Backend {
        handle: *mut c_void,
    }

    impl Backend {
        pub(super) fn new(window: &gpui::Window, url: &str) -> Result<Self, String> {
            // gpui's own `Window::window_handle` is its app-level handle, so the trait is named.
            let parent = match HasWindowHandle::window_handle(window)
                .map_err(|error| error.to_string())?
                .as_raw()
            {
                RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
                _ => return Err("the window has no AppKit view".to_string()),
            };
            let url = CString::new(url).map_err(|error| error.to_string())?;
            let handle = unsafe { GhostexGpuiRemoteScreenWebViewCreate(parent, url.as_ptr()) };
            if handle.is_null() {
                return Err("WKWebView could not be created".to_string());
            }
            Ok(Self { handle })
        }

        pub(super) fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
            unsafe { GhostexGpuiRemoteScreenWebViewSetFrame(self.handle, x, y, width, height) };
        }
    }

    impl Drop for Backend {
        fn drop(&mut self) {
            unsafe { GhostexGpuiRemoteScreenWebViewDestroy(self.handle) };
        }
    }
}

#[cfg(target_os = "windows")]
mod backend {
    use wry::dpi::{LogicalPosition, LogicalSize};

    pub(super) const SUPPORTED: bool = true;

    /// WebView2's profile, a cache beside the intro video's (CDXC:Onboarding 2026-10-07 WHY on
    /// window/onboarding/intro_web_view.rs: next to the executable it is not writable).
    fn data_directory() -> std::path::PathBuf {
        ghostex_paths::GhostexPaths::resolve()
            .cache_dir
            .join("remote-screen-webview2")
    }

    pub(super) struct Backend {
        web_view: wry::WebView,
        /// Kept for the web view's lifetime; WebView2 reads it while it creates its environment.
        _web_context: wry::WebContext,
    }

    impl Backend {
        pub(super) fn new(window: &gpui::Window, url: &str) -> Result<Self, String> {
            let allowed = url.to_string();
            let mut web_context = wry::WebContext::new(Some(data_directory()));
            let web_view = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                .with_url(url)
                .with_autoplay(true)
                .with_visible(false)
                .with_focused(true)
                .with_background_color((0, 0, 0, 255))
                // The viewer's own address only; nothing opens a window.
                .with_navigation_handler(move |next| next.starts_with(&allowed))
                .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
                .build_as_child(window)
                .map_err(|error| error.to_string())?;
            Ok(Self {
                web_view,
                _web_context: web_context,
            })
        }

        pub(super) fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
            let _ = self.web_view.set_bounds(wry::Rect {
                position: LogicalPosition::new(x, y).into(),
                size: LogicalSize::new(width.max(0.0), height.max(0.0)).into(),
            });
            let _ = self.web_view.set_visible(true);
            let _ = self.web_view.focus();
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod backend {
    pub(super) const SUPPORTED: bool = false;

    pub(super) struct Backend;

    impl Backend {
        pub(super) fn new(_window: &gpui::Window, _url: &str) -> Result<Self, String> {
            Err("this platform has no system web view for the remote screen yet".to_string())
        }

        pub(super) fn set_frame(&self, _x: f64, _y: f64, _width: f64, _height: f64) {}
    }
}
