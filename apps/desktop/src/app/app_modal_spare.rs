// A warm spare for the React app-modal window: one hidden, fully loaded window in the Settings frame.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::WindowBounds;
use gpui::WindowHandle;
use gpui::WindowOptions;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

/// How long after CEF is ready, or after an app modal closes, the next spare is created, so it never competes with launch or with the modal that is closing.
const APP_MODAL_SPARE_PRELOAD_DELAY: Duration = Duration::from_millis(500);

/// A promoted spare is shown when its page reports the modal presented; this is how long it may stay hidden if that report never arrives.
const APP_MODAL_SPARE_REVEAL_DEADLINE: Duration = Duration::from_millis(750);

/// CDXC:AppModal 2026-09-26 WHY:
/// Opening Settings created a new native window, a new CEF browser and renderer process, parsed the page, loaded its scripts and started React before the first frame, every time: about 0.4 s on macOS and around 6 s on Windows, where process creation and file loads are far slower.
/// A spare is that window made ahead of time and kept hidden with its page loaded and Settings' code already fetched; an open that fits its frame only fills it in and shows it, and a new spare is prepared after each close, the same way the new-thread picker keeps its next window ready.
/// The spare is discarded instead of used when the main window has moved or the app appearance has changed since it was made, because GPUI cannot move a window and the page's first paint colour is fixed when it loads.
pub(crate) struct GpuiAppModalSpare {
    handle: WindowHandle<GpuiAppModalHostWindow>,
    promoted: Rc<Cell<bool>>,
    center: gpui::Point<Pixels>,
    window_size: Size<Pixels>,
    light_appearance: bool,
}

impl GhostexGpuiApp {
    pub(crate) fn schedule_gpui_app_modal_spare_preload(&mut self, cx: &mut gpui::Context<Self>) {
        self.app_modal_spare_preload_generation =
            self.app_modal_spare_preload_generation.wrapping_add(1);
        let generation = self.app_modal_spare_preload_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(APP_MODAL_SPARE_PRELOAD_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.app_modal_spare_preload_generation == generation {
                    this.ensure_gpui_app_modal_spare_preloaded(cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn ensure_gpui_app_modal_spare_preloaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.app_modal_spare.is_some()
            || self.app_modal_window.is_some()
            || !cef::context_initialized()
        {
            return;
        }
        // The spare's page is hydrated when it reports ready, so it waits for the first background read instead of making one on the UI thread.
        let Some(hydrate) = self.app_modal_gxserver_hydrate.clone() else {
            self.refresh_gpui_app_modal_gxserver_hydrate(cx);
            return;
        };
        let Ok(url) = app_modal_host_url() else {
            return;
        };
        let modal = GpuiAppModalKind::Settings;
        let window_size = modal.window_size();
        let center = self.main_window_bounds.center();
        let options = WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            #[cfg(target_os = "linux")]
            x11_parent: self.main_window_handle,
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered_at(
                center,
                window_size,
            ))),
            app_id: gpui_platform_window_app_id(),
            focus: false,
            icon: gpui_platform_window_icon(),
            show: false,
            is_resizable: modal.is_resizable(),
            window_min_size: Some(window_size),
            display_id: crate::app::window::popup_frame::display_at(center, cx)
                .or(self.main_window_display_id),
            titlebar: None,
            ..Default::default()
        };
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_hydrate(&hydrate);
        let promoted = Rc::new(Cell::new(false));
        let own_window = Rc::new(Cell::new(None::<WindowHandle<GpuiAppModalHostWindow>>));
        let event_handler =
            self.app_modal_spare_bridge_event_handler(promoted.clone(), own_window.clone(), cx);
        let gxserver_bootstrap = self.sidebar_gxserver_bootstrap.clone();
        let window_border = self.gpui_native_modal_palette().window_border();
        let light_appearance = CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
        let handle = cx
            .open_window(options, |modal_window, cx| {
                crate::app::window::popup_frame::frame_app_modal_window(
                    modal_window,
                    window_border,
                );
                let host = GpuiAppModalHostWindow::new(
                    modal_window,
                    url,
                    modal,
                    modal.open_message(),
                    sidebar_state_message,
                    gxserver_bootstrap,
                    event_handler,
                    None,
                    None,
                    cx,
                );
                host.update(cx, |host, _cx| host.prepare_as_spare());
                host
            })
            .ok();
        let Some(handle) = handle else {
            return;
        };
        own_window.set(Some(handle));
        self.app_modal_spare = Some(GpuiAppModalSpare {
            handle,
            promoted,
            center,
            window_size,
            light_appearance,
        });
    }

    /// Hands the spare over as the app-modal window when `modal` fits its frame, and reports whether it did. The caller's existing reuse path then fills it in; the window itself is shown by the page's `presented` report, because its page is already running and hydrated and would otherwise show one empty frame before Settings renders.
    pub(crate) fn promote_gpui_app_modal_spare(
        &mut self,
        modal: GpuiAppModalKind,
        window_size: Size<Pixels>,
        open_message: &serde_json::Value,
        sidebar_state_message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.app_modal_window.is_some() {
            return false;
        }
        let Some(spare) = self.app_modal_spare.take() else {
            return false;
        };
        let fits = modal.uses_react_modal_host()
            && !modal.has_titlebar()
            && !modal.is_resizable()
            && !matches!(
                modal,
                GpuiAppModalKind::Onboarding | GpuiAppModalKind::FirstLaunchSetup
            )
            && spare.window_size == window_size;
        let current = spare.center == self.main_window_bounds.center()
            && spare.light_appearance == CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed);
        if !fits {
            self.app_modal_spare = Some(spare);
            return false;
        }
        if !current {
            let _ = spare
                .handle
                .update(cx, |_host, window, _cx| window.remove_window());
            return false;
        }
        spare.promoted.set(true);
        self.app_modal_window = Some(spare.handle);
        let ready = spare
            .handle
            .update(cx, |host, _window, _cx| host.is_ready())
            .unwrap_or(false);
        if !ready {
            self.app_modal_open_attempt_id = self.app_modal_open_attempt_id.wrapping_add(1);
            self.schedule_gpui_app_modal_ready_timeout(
                self.app_modal_open_attempt_id,
                modal,
                open_message.clone(),
                sidebar_state_message.clone(),
                cx,
            );
        }
        let handle = spare.handle;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(APP_MODAL_SPARE_REVEAL_DEADLINE)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.app_modal_window.map(|open| open.window_id()) == Some(handle.window_id()) {
                    let _ = handle.update(cx, |host, window, _cx| {
                        if !host.has_presented() {
                            window.activate_window();
                        }
                    });
                }
            });
        })
        .detach();
        true
    }

    /// Until it is promoted, the spare's page may only report ready, and that goes to the spare itself: every other bridge message acts on "the" app-modal window, which is not the spare yet. A ready that raced a promotion still reaches its own window.
    fn app_modal_spare_bridge_event_handler(
        &self,
        promoted: Rc<Cell<bool>>,
        own_window: Rc<Cell<Option<WindowHandle<GpuiAppModalHostWindow>>>>,
        cx: &mut gpui::Context<Self>,
    ) -> cef::AppModalHostBridgeEventHandler {
        let forward = self.app_modal_host_bridge_event_handler(cx);
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        Rc::new(move |event: cef::AppModalHostBridgeEvent| {
            if promoted.get() {
                forward(event);
                return;
            }
            let cef::AppModalHostBridgeEvent::Message(payload) = &event;
            let is_ready = serde_json::from_str::<serde_json::Value>(payload)
                .ok()
                .is_some_and(|message| {
                    message.get("type").and_then(serde_json::Value::as_str) == Some("ready")
                });
            let Some(handle) = own_window.get().filter(|_| is_ready) else {
                return;
            };
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = handle.update(&mut async_cx, |host, window, cx| {
                        host.receive_bridge_message(
                            serde_json::json!({ "type": "ready" }),
                            window,
                            cx,
                        );
                    });
                })
                .detach();
        })
    }
}
