//! The floating window a chat page card opens: an agent's published HTML page (`ghostex show`)
//! over the chat, closed by two clicks away, Escape or its close button. The page's own script
//! (`page.rs`) opens its pictures in a viewer inside the page and remembers where the reader left
//! it, and a refresh button appears when the page's file changed on disk (`source.rs`).
//!
//! CDXC:SessionChat 2026-10-23 DECISION:
//! User: "fam when i click on a html inline page to open it it takes ages to show the window. please make it show the window instantly from first time (even if content inside it needs a second, but we should react to the click instantly)". The window opens on the click with its header and a loading state, and its page is created afterwards: on the window's next frame, or, on the first page of an app run, once the web runtime that click starts is up (`web_runtime_changed`). Before, the window waited for both: the runtime's start, and creating the page's browser, which blocks until Chromium has made it. `open_timing.rs` logs where an open's time goes.
//!
//! CDXC:SessionChat 2026-10-09 DECISION:
//! User: an agent's HTML page marked to open floating opens "in a pop up above the chat basically that when i click away goes away". A card whose block carries `"open": "popup"` opens its page here instead of in the browser.
//! CDXC:SessionChat 2026-10-22 DECISION:
//! User: the page window is harder to lose: clicking outside it no longer closes it on the first click, "have to click twice". The first click away arms it for `CLICK_AWAY_ARMED_FOR` (2 seconds, while the header says "Click outside again to close") and a second click away within that time closes it; Escape and the close button still close it at once. This supersedes the single click away of the 2026-10-09 decision above.
//! CDXC:SessionChat 2026-10-09 WHY:
//! The page is loaded with `#gx-theme=light|dark`, which the bootstrap every published page starts with (server/src/visual_pages/theme.rs) reads before first paint, so the page wears the app's appearance rather than the system's. Links the page opens go to the reader's browser, so the window only ever shows the page itself.
//! SEE-ALSO: apps/desktop/src/app/visual_page_modal_lifecycle.rs (open and close), apps/desktop/src/app/native_chat/visual.rs (the card), packages/gx-visual (the `open` mark).
mod glass;
pub(crate) mod open_timing;
mod page;
mod source;

pub(crate) use page::page_id;

use super::native_modal_kit::*;
use crate::app::helpers::web_runtime::{WebRuntimeState, web_runtime_state};
use crate::app::helpers::*;
use crate::*;
use gpui::{
    App, Context, Entity, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, Styled as _, Subscription, Task, Window, div, point,
    px, size,
};
use gpui_component::{h_flex, v_flex};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

const HEADER_HEIGHT: f32 = 44.0;
const ICON_CLOSE: &str = "titlebar/x.svg";
const ICON_OPEN_IN_BROWSER: &str = "titlebar/external-link.svg";
const ICON_REFRESH: &str = "titlebar/refresh.svg";
/// How long a first click away keeps the window armed for the second one that closes it.
const CLICK_AWAY_ARMED_FOR: Duration = Duration::from_secs(2);
/// A focus loss this soon after a press in another window is that press, already counted.
const SAME_CLICK: Duration = Duration::from_millis(400);
/// Its own browser profile, so a page's storage never mixes with the Browser's or an
/// extension's.
pub(crate) const VISUAL_PAGE_CEF_PROFILE_ID: &str = "visual-pages";

pub(crate) enum VisualPageModalCommand {
    /// A click away, Escape or the close button.
    Close,
    /// Closes the window and opens the page in the reader's browser.
    OpenInBrowser { url: String },
    /// A link the page opened (a new window or `target=_blank`), for the app to route by the
    /// machine the page came from.
    OpenLink { url: String },
    /// Publishing the page's changed file again failed (`source.rs`); the app says why.
    RefreshFailed { message: String },
}

pub(crate) type VisualPageModalHost = Rc<dyn Fn(VisualPageModalCommand, &mut App)>;

pub(crate) struct GpuiVisualPageModalWindow {
    host: VisualPageModalHost,
    palette: ModalPalette,
    url: String,
    title: String,
    detail: String,
    /// What the page is loaded from: `url` with the app's theme and `gx-floating=1`.
    page_url: String,
    /// The card's open message, for opening it again behind the web runtime's install prompt.
    open_message: serde_json::Value,
    surface: Option<Entity<CefSurface>>,
    /// The page has loaded once; until then the header shows a spinner.
    loaded: bool,
    error: Option<String>,
    focus_handle: FocusHandle,
    /// The page's id on the gxserver that serves it, for Refresh.
    page_id: Option<String>,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    /// The page's file changed on disk since it was published (`source.rs`).
    source_changed: bool,
    refreshing: bool,
    /// When a first click away armed the window, while it is armed.
    armed_at: Option<Instant>,
    /// The press in another window counted last, so the same press is never counted twice.
    counted_press: Option<Instant>,
    _disarm: Option<Task<()>>,
    _source_watch: Option<Task<()>>,
    _click_away: Vec<Subscription>,
}

impl GpuiVisualPageModalWindow {
    /// `web_runtime_ready`: the web runtime is up, so the page is created on the window's next
    /// frame; otherwise the app creates it once the runtime it is starting is up
    /// (`web_runtime_changed`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        url: String,
        title: String,
        detail: String,
        open_message: serde_json::Value,
        palette: ModalPalette,
        host: VisualPageModalHost,
        remote: Option<GpuiRemoteGxserverRequestTarget>,
        web_runtime_ready: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let page_id = page::page_id(&url);
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let theme = if palette.light { "light" } else { "dark" };
        let page_url = format!(
            "{}#gx-theme={theme}&gx-floating=1",
            url.split('#').next().unwrap_or(&url)
        );
        // Creating the page's browser holds the app until Chromium has made it, so it waits for
        // the frame after the window's first one: the window is on screen first.
        let modal = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            open_timing::mark("windowShown");
            if web_runtime_ready {
                let _ = modal.update(cx, |this, cx| this.create_surface(window, cx));
            }
        });
        let mut click_away = super::popup_dismissal::close_app_modal_on_click_away(window, cx);
        click_away.push(observe_presses_elsewhere(window, cx));
        let mut modal = Self {
            host,
            palette,
            url,
            title,
            detail,
            page_url,
            open_message,
            surface: None,
            loaded: false,
            error: None,
            focus_handle,
            page_id,
            remote,
            source_changed: false,
            refreshing: false,
            armed_at: None,
            counted_press: None,
            _disarm: None,
            _source_watch: None,
            _click_away: click_away,
        };
        modal.watch_page_source(cx);
        modal
    }

    /// The web runtime the window is waiting for changed (`refresh_web_runtime_views`): once it
    /// is up the page is created; when it failed the window says why. Returns the card's open
    /// message when the runtime turned out not to be installed, for the app to show its install
    /// prompt instead of this window.
    pub(crate) fn web_runtime_changed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<serde_json::Value> {
        if self.surface.is_some() || self.error.is_some() {
            return None;
        }
        if cef::context_initialized() {
            open_timing::mark("webRuntimeReady");
            self.create_surface(window, cx);
            return None;
        }
        match web_runtime_state() {
            WebRuntimeState::Failed(message) => {
                open_timing::finish("webRuntimeFailed");
                self.error = Some(message);
                cx.notify();
                None
            }
            WebRuntimeState::NotInstalled | WebRuntimeState::Installing(_) => {
                Some(self.open_message.clone())
            }
            WebRuntimeState::Installed | WebRuntimeState::Running => None,
        }
    }

    /// Creates the page's browser under the header.
    fn create_surface(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.surface.is_some() || self.error.is_some() {
            return;
        }
        open_timing::mark("pageCreateStarted");
        let url = self.url.clone();
        let remote = self.remote.clone();
        // A remote machine's page can link to that machine's own localhost, which only the app
        // knows how to reach (`gpui_remote_machine_url`), so every link goes through it.
        let popup_open_handler: cef::BrowserPopupOpenHandler = {
            let modal = cx.weak_entity();
            let async_cx = cx.to_async();
            let foreground = cx.foreground_executor().clone();
            Rc::new(move |requested_url, _| {
                let modal = modal.clone();
                let mut async_cx = async_cx.clone();
                // Run on the next turn: this is called from inside CEF's message pump.
                foreground
                    .spawn(async move {
                        let _ = modal.update(&mut async_cx, |this, cx| {
                            (this.host)(
                                VisualPageModalCommand::OpenLink { url: requested_url },
                                cx,
                            );
                        });
                    })
                    .detach();
            })
        };
        // CDXC:SessionChat 2026-10-11 DECISION: User: when the reader clicks Send on a page in this window, the window closes once the answers are sent, and stays open showing the error when sending fails. The page knows it floats (`gx-floating=1`, server/src/visual_pages/theme.rs) and closes itself only after gxserver accepted its answers; CEF reports that as a close request, which closes this window.
        let page_closed: cef::BrowserPageMetadataHandler = {
            let modal = cx.weak_entity();
            let async_cx = cx.to_async();
            let foreground = cx.foreground_executor().clone();
            Rc::new(move |event| {
                if !matches!(event, cef::BrowserPageMetadataEvent::CloseRequested) {
                    return;
                }
                let modal = modal.clone();
                let mut async_cx = async_cx.clone();
                foreground
                    .spawn(async move {
                        let _ = modal.update(&mut async_cx, |this, cx| {
                            (this.host)(VisualPageModalCommand::Close, cx);
                        });
                    })
                    .detach();
            })
        };
        // The page's card picture comes from this window once the page has loaded
        // (app/visual_page_thumbnails.rs); the surface is filled in once it exists.
        let shown_surface: Rc<std::cell::RefCell<Option<gpui::WeakEntity<CefSurface>>>> =
            Rc::default();
        let page_loaded: cef::PageLoadEndHandler = {
            let shown_surface = shown_surface.clone();
            let modal = cx.weak_entity();
            let page = url.clone();
            let viewport = window.viewport_size();
            let viewport = (
                f32::from(viewport.width),
                (f32::from(viewport.height) - HEADER_HEIGHT).max(1.0),
            );
            let async_cx = cx.to_async();
            let foreground = cx.foreground_executor().clone();
            Rc::new(move || {
                let Some(surface) = shown_surface.borrow().clone() else {
                    return;
                };
                let modal = modal.clone();
                let page = page.clone();
                let remote = remote.clone();
                let mut async_cx = async_cx.clone();
                // Run on the next turn: this is called from inside CEF's message pump.
                foreground
                    .spawn(async move {
                        let _ = modal.update(&mut async_cx, |this, cx| {
                            if !this.loaded {
                                open_timing::mark("pageLoaded");
                                open_timing::finish("loaded");
                                this.loaded = true;
                                cx.notify();
                            }
                        });
                        async_cx.update(|cx| {
                            crate::app::visual_page_thumbnails::picture_opened_page(
                                surface, &page, remote, viewport, cx,
                            )
                        });
                    })
                    .detach();
            })
        };
        let created = cef_parent_native_view(window)
            .map_err(|error| error.to_string())
            .and_then(|parent| {
                CefSurface::try_new(
                    "ghostex-gpui-visual-page".to_string(),
                    parent,
                    self.page_url.clone(),
                    VISUAL_PAGE_CEF_PROFILE_ID.to_string(),
                    pane_prepaint_background_color(),
                    false,
                    hsla(self.palette.surface),
                    None,
                    true,
                    Some(popup_open_handler),
                    None,
                    Some(page_closed),
                    None,
                    None,
                    None,
                    Some(page_loaded),
                    None,
                    cx,
                )
            });
        open_timing::mark("pageCreated");
        match created {
            Ok(surface) => {
                *shown_surface.borrow_mut() = Some(surface.downgrade());
                surface.read(cx).add_page_script(page::PAGE_SCRIPT);
                let mut page_bounds = window.bounds();
                page_bounds.origin.y += px(HEADER_HEIGHT);
                page_bounds.size.height =
                    (page_bounds.size.height - px(HEADER_HEIGHT)).max(px(1.0));
                if let Some(script) = glass::page_glass_script(page_bounds, cx) {
                    surface.read(cx).add_page_script(&script);
                }
                // A CEF page starts 1×1 and only gets its frame when the window paints; give it
                // the area under the header from the start so it lays out at its real width.
                let viewport = window.viewport_size();
                surface.read(cx).set_initial_bounds(
                    gpui::Bounds::new(
                        point(px(0.0), px(HEADER_HEIGHT)),
                        size(
                            viewport.width,
                            (viewport.height - px(HEADER_HEIGHT)).max(px(1.0)),
                        ),
                    ),
                    window.scale_factor(),
                );
                self.surface = Some(surface);
            }
            Err(error) => {
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({ "surface": "visualPage", "error": error }),
                );
                open_timing::finish("pageCreateFailed");
                self.error = Some("This page could not be opened here.".to_string());
            }
        }
        cx.notify();
    }

    /// One click away: the first arms the window, a second within `CLICK_AWAY_ARMED_FOR` closes it.
    fn click_away(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self
            .armed_at
            .is_some_and(|armed| now.duration_since(armed) < CLICK_AWAY_ARMED_FOR)
        {
            (self.host)(VisualPageModalCommand::Close, cx);
            return;
        }
        self.armed_at = Some(now);
        cx.notify();
        self._disarm = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CLICK_AWAY_ARMED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.armed_at = None;
                cx.notify();
            });
        }));
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            window.prevent_default();
            cx.stop_propagation();
            (self.host)(VisualPageModalCommand::Close, cx);
        }
    }
}

impl GpuiVisualPageModalWindow {
    /// Shown while the page's file has changed on disk since the page was published.
    fn refresh_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let label = if self.refreshing {
            "Refreshing…"
        } else {
            "Refresh"
        };
        div()
            .id("visual-page-refresh")
            .role(gpui::Role::Button)
            .aria_label("Refresh the page: its file changed")
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .bg(hsla(p.accent))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(p.foreground))
            .cursor_pointer()
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("The page's file changed. Reload it")
                    .build(window, cx)
            })
            .on_click(cx.listener(|this, _, _, cx| this.refresh_page(cx)))
            .child(modal_icon(ICON_REFRESH, 14.0, p.foreground))
            .child(label)
    }
}

impl Render for GpuiVisualPageModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let header = h_flex()
            .flex_shrink_0()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .pl(px(14.0))
            .pr(px(6.0))
            .gap(px(4.0))
            .items_center()
            .border_b_1()
            .border_color(hsla(p.hairline))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child(self.title.clone()),
                    )
                    .map(|this| match self.armed_at {
                        Some(_) => this.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(hsla(p.foreground))
                                .child("Click outside again to close"),
                        ),
                        None if !self.detail.is_empty() => this.child(
                            div()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(hsla(p.muted))
                                .child(self.detail.clone()),
                        ),
                        None => this,
                    }),
            )
            .when(!self.loaded && self.error.is_none(), |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .px(px(6.0))
                        .child(modal_spinner(p.muted)),
                )
            })
            .when(self.source_changed || self.refreshing, |this| {
                this.child(self.refresh_button(cx))
            })
            .child(modal_icon_button(
                &p,
                "visual-page-open-in-browser",
                ICON_OPEN_IN_BROWSER,
                16.0,
                |this: &mut Self, _, cx| {
                    let url = this.url.clone();
                    (this.host)(VisualPageModalCommand::OpenInBrowser { url }, cx);
                },
                cx,
            ))
            .child(modal_icon_button(
                &p,
                "visual-page-close",
                ICON_CLOSE,
                16.0,
                |this: &mut Self, _, cx| (this.host)(VisualPageModalCommand::Close, cx),
                cx,
            ));
        let body = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .children(self.surface.clone())
            .when(self.surface.is_none() && self.error.is_none(), |this| {
                this.flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(hsla(p.muted))
                    .child("Opening the page…")
            })
            .when_some(self.error.clone(), |this, error| {
                this.flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(hsla(p.muted))
                    .child(error)
            });
        div()
            .id("ghostex-gpui-visual-page-modal")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(hsla(p.surface))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(header)
            .child(body)
    }
}

/// Presses in the app's other windows after the page window opened, each one a click away. On
/// macOS and Windows the first one also takes focus from the window, which the shared click-away
/// rule reports too; `close_from_corner` drops that report so one press counts once.
fn observe_presses_elsewhere(
    window: &mut Window,
    cx: &mut Context<GpuiVisualPageModalWindow>,
) -> Subscription {
    let opened_at = Instant::now();
    let own_window = gpui::Window::window_handle(window).window_id();
    cx.observe_global_in::<super::popup_dismissal::MainWindowPointerPress>(
        window,
        move |this, _, cx| {
            let press = *cx.global::<super::popup_dismissal::MainWindowPointerPress>();
            if press.at <= opened_at
                || press.window == own_window
                || this.counted_press == Some(press.at)
            {
                return;
            }
            this.counted_press = Some(press.at);
            this.click_away(cx);
        },
    )
}

impl ModalCornerClose for GpuiVisualPageModalWindow {
    /// The window draws its own close button and Escape closes it directly, so the shared
    /// click-away rule (`close_app_modal_on_click_away`) is the only caller: a click away, which
    /// only arms the window the first time.
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let pressed_just_now = super::popup_dismissal::latest_main_window_press(cx)
            .is_some_and(|press| press.at.elapsed() < SAME_CLICK);
        if !pressed_just_now {
            self.click_away(cx);
        }
    }

    /// It draws its own close button in its header.
    fn shows_corner_close(&self, _cx: &App) -> bool {
        false
    }
}
