//! The floating window a chat page card opens: an agent's published HTML page (`ghostex show`)
//! over the chat, hidden by a click away or its Hide button, closed by Escape or its close button. The page's own script
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
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User's picks on the hide-page mockup (page-links/hide-page.html): a page hides instead of closing (option C of "When the page hides") when a link in it opens in the app's browser, from the Hide button beside its close x, and on any click outside it, which replaces the two clicks that closed it above; the x, Escape and sending its answers still close it for good. A hidden page is kept running (option A of "Bringing it back"): its window is only ordered out, so its pill in the chat's pills bar (app/visual_page_hidden.rs) brings it back at once in the same place and size, scrolled where it was, with every pick and note as it was. Hidden, it has no keyboard and takes no clicks.
//! CDXC:SessionChat 2026-10-09 WHY:
//! The page is loaded with `#gx-theme=light|dark`, which the bootstrap every published page starts with (server/src/visual_pages/theme.rs) reads before first paint, so the page wears the app's appearance rather than the system's. Links the page opens go to the reader's browser, so the window only ever shows the page itself.
//! CDXC:SessionChat 2026-11-04 DECISION:
//! User picked "A. No header" on Q1215 (issues-1027/page-window-glass/header-options.html): the page runs to the top edge, round glass buttons (Refresh, Open in browser, Close) float in its top-right corner, the page's own title is the only title, and the window is dragged from its top. The window has no header any more: a 36px band (`TOP_BAND_HEIGHT`) paints the page's own backdrop (`glass.rs`), so it reads as the page's top edge, holds the round controls at its right and the loading spinner or the "Click outside again to close" note at its left, and moves the window everywhere else (double-click zooms). A floating page drops the template's top padding by as much (server/src/visual_pages/theme.rs), so its content starts where it would at the top of a browser tab and never sits under the controls. Supersedes the title and file line of the 2026-10-09 header.
//! CDXC:SessionChat 2026-11-04 WHY:
//! The controls can't float over the page itself: the page is a windowed CEF browser, a native view nothing drawn in the window can sit above, and a press the page passed back through the page bridge would arrive after its mouse-down, too late to start a window move (`start_window_move` drags with the event being handled). So the band is the window's own, and the page starts under it.
//! SEE-ALSO: apps/desktop/src/app/visual_page_modal_lifecycle.rs (open and close), apps/desktop/src/app/native_chat/visual.rs (the card), packages/gx-visual (the `open` mark).
mod band_geometry;
pub(crate) mod frame;
mod glass;
mod more_menu;
mod more_menu_panel;
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
    KeyDownEvent, ParentElement as _, Render, Styled as _, StyledImage as _, Subscription, Task,
    Window, div, point, px, size,
};
use gpui_component::h_flex;
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) use band_geometry::TOP_BAND_HEIGHT;
use band_geometry::{CONTROL_INSET, CONTROL_SIZE};
const ICON_CLOSE: &str = "titlebar/x.svg";
const ICON_MORE: &str = "titlebar/dots.svg";
const ICON_REFRESH: &str = "titlebar/refresh.svg";
const ICON_HIDE: &str = "titlebar/window-minimize.svg";
/// A focus loss this soon after a press in another window is that press, already counted.
const SAME_CLICK: Duration = Duration::from_millis(400);
/// Its own browser profile, so a page's storage never mixes with the Browser's or an
/// extension's.
pub(crate) const VISUAL_PAGE_CEF_PROFILE_ID: &str = "visual-pages";
/// Saves what the reader typed that the page has not saved yet, synchronously (the page API's
/// `__flush`, server/src/visual_pages/theme.rs).
const FLUSH_DRAFT: &str = "window.ghostex&&ghostex.__flush&&ghostex.__flush();";
/// How long the app's quit waits for the page's flush, inside GPUI's 200 ms `SHUTDOWN_TIMEOUT`.
const FLUSH_ON_QUIT_WAIT: Duration = Duration::from_millis(150);

pub(crate) enum VisualPageModalCommand {
    /// Escape, the close button, or the page closing itself once its answers were sent.
    Close,
    /// A click away or the Hide button: the window is put aside, still running
    /// (app/visual_page_hidden.rs).
    Hide,
    /// What its pill shows while it is hidden changed: whether a question is unanswered and how
    /// many are answered (`source.rs`), or the page's own title arrived for a page opened without
    /// one.
    TabChanged,
    /// Closes the window and opens the page in the reader's browser.
    OpenInBrowser { url: String },
    /// Downloads the page's exported file in the reader's browser and keeps the window open
    /// (server/src/visual_pages/exported.rs).
    ExportPage { url: String },
    /// A link the page opened (a new window or `target=_blank`), for the app to route by the
    /// machine the page came from.
    OpenLink { url: String },
    /// Publishing the page's changed file again failed (`source.rs`); the app says why.
    RefreshFailed { message: String },
    /// A notice for the app to show: how Import answers went (`more_menu.rs`).
    Notice {
        level: &'static str,
        title: String,
        message: String,
    },
}

pub(crate) type VisualPageModalHost = Rc<dyn Fn(VisualPageModalCommand, &mut App)>;

pub(crate) struct GpuiVisualPageModalWindow {
    host: VisualPageModalHost,
    /// Which page window this is, for the app to tell a hidden one's commands from the shown one's.
    key: u64,
    /// The chat whose card opened the page, whose pills bar shows its pill while it is hidden.
    source_session: Option<TerminalSessionId>,
    /// gxserver says a question on the page has no answer yet (`source.rs`).
    questions_left: bool,
    /// How many of the page's questions are answered, and how many it asks (`source.rs`).
    question_counts: Option<(u64, u64)>,
    /// The window is put aside (`set_hidden`).
    hidden: bool,
    /// When the window was last shown; presses elsewhere from before do not count.
    shown_at: Instant,
    palette: ModalPalette,
    url: String,
    /// The page's title, for screen readers and its pill while hidden: the page shows its own.
    title: String,
    /// `title` came with the open (a page card); a page opened from a plain link to its address
    /// has none, so it takes the page's own title once the page reports it.
    title_known: bool,
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
    /// The ⋯ menu is open (`more_menu.rs`), and where its button was last drawn.
    more_menu_open: bool,
    more_button: Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The press in another window counted last, so the same press is never counted twice.
    counted_press: Option<Instant>,
    _source_watch: Option<Task<()>>,
    /// What the page was last handed of the app's glass and theme (`glass.rs`).
    glass: glass::PageGlass,
    _glass_watch: Option<Task<()>>,
    /// Moves and resizes of the window, which move the page's picture with it (`glass.rs`).
    _glass_bounds: Option<Subscription>,
    /// Moves and resizes of the window, kept for the next page window (`frame.rs`).
    _frame_watch: Subscription,
    _frame_save: Option<Task<()>>,
    _click_away: Vec<Subscription>,
    /// The page's draft is flushed when the app quits and when the window goes away.
    _flush_draft: [Subscription; 2],
}

impl GpuiVisualPageModalWindow {
    /// `web_runtime_ready`: the web runtime is up, so the page is created on the window's next
    /// frame; otherwise the app creates it once the runtime it is starting is up
    /// (`web_runtime_changed`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        url: String,
        title: Option<String>,
        open_message: serde_json::Value,
        palette: ModalPalette,
        host: VisualPageModalHost,
        key: u64,
        source_session: Option<TerminalSessionId>,
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
        let frame_watch =
            cx.observe_window_bounds(window, |this, window, cx| this.keep_frame(window, cx));
        let mut click_away = super::popup_dismissal::close_app_modal_on_click_away(window, cx);
        click_away.push(observe_presses_elsewhere(window, cx));
        // CDXC:SessionChat 2026-11-05 WHY: an app quit (`cargo xtask rebuild` relaunches the app)
        // shuts Chromium down with the page still open, so the page never sees `pagehide` and
        // what the reader typed since its last save was lost (issues-1027/page-draft-loss). The
        // quit observer runs before GPUI drops the windows, and the page's flush is a synchronous
        // request the renderer sends to the network process itself, so it lands while the quit
        // waits. Closing the window also flushes, though `pagehide` saves then too.
        let flush_draft = [
            cx.on_app_quit(|this, cx| {
                this.flush_draft(cx);
                let wait = cx.background_executor().timer(FLUSH_ON_QUIT_WAIT);
                async move {
                    wait.await;
                }
            }),
            cx.on_release(|this, cx| this.flush_draft(cx)),
        ];
        let mut modal = Self {
            host,
            key,
            source_session,
            questions_left: false,
            question_counts: None,
            hidden: false,
            shown_at: Instant::now(),
            palette,
            url,
            title_known: title.is_some(),
            title: title.unwrap_or_else(|| "Ghostex page".to_string()),
            page_url,
            open_message,
            surface: None,
            loaded: false,
            error: None,
            focus_handle,
            page_id,
            remote,
            source_changed: false,
            more_menu_open: false,
            more_button: Rc::default(),
            refreshing: false,
            counted_press: None,
            _source_watch: None,
            glass: glass::PageGlass::new(palette.light),
            _glass_watch: None,
            _glass_bounds: None,
            _frame_watch: frame_watch,
            _frame_save: None,
            _click_away: click_away,
            _flush_draft: flush_draft,
        };
        modal.watch_page_source(cx);
        modal
    }

    /// Asks the page to save its draft now (`FLUSH_DRAFT`).
    fn flush_draft(&self, cx: &mut App) {
        if let Some(surface) = &self.surface {
            surface.update(cx, |surface, _| {
                surface.execute_app_owned_script(FLUSH_DRAFT)
            });
        }
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
        // CDXC:SessionChat 2026-11-12 DECISION:
        // User: as soon as the reply is sent, close the question form popup and the Ghostex page
        // popup with a short fade; keep it open with the answers if the send fails. The page's
        // own close request (sent only after gxserver took its answers) fades this window out
        // first (`popup_frame::fade_out_popup_window`), then closes it.
        let own_window = gpui::Window::window_handle(window);
        let page_closed: cef::BrowserPageMetadataHandler = {
            let modal = cx.weak_entity();
            let async_cx = cx.to_async();
            let foreground = cx.foreground_executor().clone();
            Rc::new(move |event| {
                let page_title = match event {
                    cef::BrowserPageMetadataEvent::CloseRequested => None,
                    cef::BrowserPageMetadataEvent::TitleChanged(title) => Some(title),
                    _ => return,
                };
                let modal = modal.clone();
                let mut async_cx = async_cx.clone();
                foreground
                    .spawn(async move {
                        let Some(title) = page_title else {
                            let wait = own_window
                                .update(&mut async_cx, |_, window, cx| {
                                    crate::app::window::popup_frame::fade_out_popup_window(
                                        window,
                                        cx.reduce_motion(),
                                    )
                                })
                                .ok()
                                .flatten();
                            if let Some(wait) = wait {
                                async_cx.background_executor().timer(wait).await;
                            }
                            let _ = modal.update(&mut async_cx, |this, cx| {
                                (this.host)(VisualPageModalCommand::Close, cx)
                            });
                            // A window the close left standing (it was put aside meanwhile)
                            // shows again when it comes back.
                            if wait.is_some() {
                                let _ = own_window.update(&mut async_cx, |_, window, _| {
                                    crate::app::window::popup_frame::fade_in_popup_window(
                                        window, true,
                                    )
                                });
                            }
                            return;
                        };
                        let _ =
                            modal.update(&mut async_cx, |this, cx| this.take_page_title(title, cx));
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
                (f32::from(viewport.height) - TOP_BAND_HEIGHT).max(1.0),
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
        let page_top = if self.glass.light {
            glass::PAGE_TOP_LIGHT
        } else {
            glass::PAGE_TOP_DARK
        };
        // CDXC:SessionChat 2026-11-11 WHY: the page is made at the area under the band, with the
        // window's scale, instead of 1x1 (cef/shell/browser.rs `initial_frame`), and hidden until a
        // paint has placed it there (element/cef_surface.rs `show_when_placed`), so no frame from
        // before its real size or scale is ever seen: the "collapsed to the top left" flash.
        let viewport = window.viewport_size();
        let initial_frame = (
            gpui::Bounds::new(
                point(px(0.0), px(TOP_BAND_HEIGHT)),
                size(
                    viewport.width,
                    (viewport.height - px(TOP_BAND_HEIGHT)).max(px(1.0)),
                ),
            ),
            window.scale_factor(),
        );
        let created = cef_parent_native_view(window)
            .map_err(|error| error.to_string())
            .and_then(|parent| {
                CefSurface::try_new(
                    "ghostex-gpui-visual-page".to_string(),
                    parent,
                    self.page_url.clone(),
                    VISUAL_PAGE_CEF_PROFILE_ID.to_string(),
                    // Until the page paints, the top colour of its dusk or dawn, which the header
                    // above it paints too.
                    0xff00_0000 | page_top,
                    false,
                    gpui::rgb(page_top).into(),
                    None,
                    false,
                    Some(popup_open_handler),
                    None,
                    Some(page_closed),
                    None,
                    None,
                    None,
                    Some(page_loaded),
                    None,
                    Some(initial_frame),
                    cx,
                )
            });
        open_timing::mark("pageCreated");
        match created {
            Ok(surface) => {
                *shown_surface.borrow_mut() = Some(surface.downgrade());
                surface.read(cx).add_page_script(page::PAGE_SCRIPT);
                surface.update(cx, |surface, _| {
                    surface.show_when_placed();
                    surface.set_visible(true);
                });
                self.surface = Some(surface);
                self.follow_page_glass(window, cx);
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

    /// Keeps where the reader moved the window and the size they gave it for the next page window,
    /// saved once the window has been still for a moment. A drag that ended where the window cannot
    /// stay reachable (another display it cannot be shown on, off the top of the screen) puts it
    /// back first (`frame::settle`, GitHub issue #560).
    fn keep_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let frame = frame::shared_frame(window);
        let Some(main) = main_window_frame(cx) else {
            return;
        };
        if !frame::keep(main, frame) {
            return;
        }
        self._frame_save = Some(cx.spawn_in(window, async move |this, cx| {
            // Still while the button is held is a pause in the drag, not its end.
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if !crate::app::ghostex_capture::platform::primary_mouse_button_down() {
                    break;
                }
            }
            let back = this
                .update_in(cx, |_, window, cx| {
                    let main = main_window_frame(cx)?;
                    let to = frame::settle(
                        main,
                        frame::shared_frame(window),
                        &frame::Screens::now(main, cx),
                    )?;
                    let native = crate::app::helpers::cef_parent_native_view(window).ok()?;
                    Some((native as usize, to, window.scale_factor()))
                })
                .ok()
                .flatten();
            // Moved outside any update: AppKit reports the new frame to GPUI synchronously, and
            // GPUI drops that report while the app is borrowed. The move is seen and kept as any
            // other move is.
            if let Some((native, to, scale)) = back {
                crate::app::ghostex_capture::platform::set_window_frame(native, to, scale);
            }
            frame::save();
        }));
    }

    /// A click away hides the window (CDXC:SessionChat 2026-11-11 DECISION above).
    fn click_away(&mut self, cx: &mut Context<Self>) {
        if !self.hidden {
            (self.host)(VisualPageModalCommand::Hide, cx);
        }
    }

    /// A page opened without a title (a plain link to its address) is named by its own title,
    /// which its pill shows while it is hidden.
    fn take_page_title(&mut self, title: String, cx: &mut Context<Self>) {
        let title = title.trim();
        if self.title_known || title.is_empty() || title.starts_with("http") {
            return;
        }
        if self.title != title {
            self.title = title.to_string();
            (self.host)(VisualPageModalCommand::TabChanged, cx);
        }
    }

    pub(crate) fn key(&self) -> u64 {
        self.key
    }

    pub(crate) fn source_session(&self) -> Option<TerminalSessionId> {
        self.source_session
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn questions_left(&self) -> bool {
        self.questions_left
    }

    pub(crate) fn question_counts(&self) -> Option<(u64, u64)> {
        self.question_counts
    }

    /// The app put the window aside or brought it back. Hiding saves the page's draft at once and
    /// asks gxserver again whether questions are left, so the pill's count counts the last pick.
    pub(crate) fn set_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        self.hidden = hidden;
        if hidden {
            self.close_more_menu(cx);
            self.flush_draft(cx);
            self.check_questions_left_soon(cx);
        } else {
            self.shown_at = Instant::now();
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            window.prevent_default();
            cx.stop_propagation();
            // Escape closes the ⋯ menu first, then the window.
            if self.more_menu_open {
                self.close_more_menu(cx);
                return;
            }
            (self.host)(VisualPageModalCommand::Close, cx);
        }
    }
}

/// The glass of the floating controls in one theme (`ControlGlass::for_theme`).
#[derive(Clone, Copy)]
struct ControlGlass {
    fill: gpui::Rgba,
    hover: gpui::Rgba,
    border: gpui::Rgba,
    icon: gpui::Rgba,
    text: gpui::Rgba,
}

impl ControlGlass {
    /// The page's own glass buttons (server/src/visual_pages/theme.rs `--gx-raised`,
    /// `--gx-card-border`, `--gx-text`, `--gx-fg`), so they read as part of the page.
    fn for_theme(light: bool) -> Self {
        if light {
            Self {
                fill: gpui::rgba(0xffffffb8),
                hover: gpui::rgba(0xfffffff2),
                border: gpui::rgba(0x43434324),
                icon: gpui::rgba(0x3f3f46ff),
                text: gpui::rgba(0x18181bff),
            }
        } else {
            Self {
                fill: gpui::rgba(0x26262699),
                hover: gpui::rgba(0x404040b8),
                border: gpui::rgba(0xe9e9e924),
                icon: gpui::rgba(0xc8cdd5ff),
                text: gpui::rgba(0xfcfcfcff),
            }
        }
    }
}

/// A round glass button of the floating controls; its presses never reach the drag band.
fn control_button(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    glass: ControlGlass,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CONTROL_SIZE))
        .rounded_full()
        .bg(glass.fill)
        .border_1()
        .border_color(glass.border)
        .cursor_pointer()
        .hover(move |style| style.bg(glass.hover))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(label).build(window, cx))
        .child(modal_icon(icon, 14.0, glass.icon))
}

impl GpuiVisualPageModalWindow {
    /// Shown while the page's file has changed on disk since the page was published.
    fn refresh_button(
        &self,
        glass: ControlGlass,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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
            .gap(px(5.0))
            .h(px(CONTROL_SIZE))
            .px(px(10.0))
            .rounded_full()
            .bg(glass.fill)
            .border_1()
            .border_color(glass.border)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(glass.text)
            .cursor_pointer()
            .hover(move |style| style.bg(glass.hover))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("The page's file changed. Reload it")
                    .build(window, cx)
            })
            .on_click(cx.listener(|this, _, _, cx| this.refresh_page(cx)))
            .child(modal_icon(ICON_REFRESH, 13.0, glass.text))
            .child(label)
    }

    /// The band over the page's top edge: nothing but the page's own backdrop, the round glass
    /// controls in its corners, and on its left the loading spinner. The rest of the band moves
    /// the window.
    fn top_band(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let glass = ControlGlass::for_theme(self.glass.light);
        let status = div()
            .flex()
            .items_center()
            .h_full()
            .pl(px(12.0))
            .when(!self.loaded && self.error.is_none(), |this| {
                this.child(modal_spinner(glass.icon))
            });
        let close = control_button("visual-page-close", ICON_CLOSE, "Close", glass).on_click(
            cx.listener(|this, _, _, cx| {
                this.close_more_menu(cx);
                (this.host)(VisualPageModalCommand::Close, cx)
            }),
        );
        // Hide sits beside the X (CDXC:SessionChat 2026-11-11 DECISION above): after it in the
        // left corner, before it at the right.
        let hide = control_button("visual-page-hide", ICON_HIDE, "Hide", glass).on_click(
            cx.listener(|this, _, _, cx| {
                this.close_more_menu(cx);
                (this.host)(VisualPageModalCommand::Hide, cx)
            }),
        );
        // The ⋯ menu (Export page, Import answers, Open in browser) sits beside Hide and Close
        // (CDXC:SessionChat 2026-11-13 DECISION in more_menu.rs); its button's bounds place the menu.
        let more_bounds = self.more_button.clone();
        let more = control_button("visual-page-more", ICON_MORE, "More", glass)
            .relative()
            .on_click(cx.listener(|this, _, window, cx| this.toggle_more_menu(window, cx)))
            .child(
                gpui::canvas(
                    move |bounds, _, _| more_bounds.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        // The X sits in the band's left corner on macOS, before the status, and ends the right
        // corner's controls elsewhere (CDXC:AppModal 2026-11-10 in window/round_close_button.rs).
        let (leading_close, trailing_close) = if super::round_close_button::close_buttons_on_left()
        {
            (
                Some(h_flex().gap(px(6.0)).child(close).child(hide).child(more)),
                None,
            )
        } else {
            (
                None,
                Some(h_flex().gap(px(6.0)).child(more).child(hide).child(close)),
            )
        };
        h_flex()
            .flex_shrink_0()
            .h(px(TOP_BAND_HEIGHT))
            .w_full()
            .items_center()
            .when_some(leading_close, |band, close| {
                band.child(
                    div()
                        .flex_shrink_0()
                        .self_start()
                        .pt(px(CONTROL_INSET))
                        .pl(px(CONTROL_INSET))
                        .child(close),
                )
            })
            .child(
                crate::app::render::window_drag_region::window_drag_region(
                    div().id("visual-page-drag-band").on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.close_more_menu(cx)),
                    ),
                )
                .flex_1()
                .min_w_0()
                .h_full()
                .child(status),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .self_start()
                    .gap(px(6.0))
                    .pt(px(CONTROL_INSET))
                    .pr(px(CONTROL_INSET))
                    .items_center()
                    .when(self.source_changed || self.refreshing, |this| {
                        this.child(self.refresh_button(glass, cx))
                    })
                    .children(trailing_close),
            )
    }
}

impl Render for GpuiVisualPageModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let band = self.top_band(cx);
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
        // The page's own backdrop runs under the band (`glass.rs`): its picture and tint, or the
        // top colour of its painted dusk or dawn.
        let page_top = if self.glass.light {
            glass::PAGE_TOP_LIGHT
        } else {
            glass::PAGE_TOP_DARK
        };
        div()
            .id("ghostex-gpui-visual-page-modal")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(gpui::rgb(page_top))
            .on_key_down(cx.listener(Self::on_key_down))
            .when_some(self.glass.backdrop.as_ref(), |this, backdrop| {
                this.child(
                    div()
                        .absolute()
                        .left(backdrop.rect.origin.x)
                        .top(backdrop.rect.origin.y)
                        .w(backdrop.rect.size.width)
                        .h(backdrop.rect.size.height)
                        .child(
                            gpui::img(backdrop.image.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Fill),
                        ),
                )
                .child(div().absolute().inset_0().bg(backdrop.tint))
            })
            .aria_label(self.title.clone())
            .child(band)
            .child(body)
    }
}

/// The main window's frame in the space all displays share (`frame::shared_frame`).
fn main_window_frame(cx: &mut App) -> Option<gpui::Bounds<gpui::Pixels>> {
    crate::app::helpers::main_window_handle(cx)?
        .update(cx, |_, window, _| frame::shared_frame(window))
        .ok()
}

impl Drop for GpuiVisualPageModalWindow {
    /// A window closed right after it was moved or resized still leaves its frame for the next one.
    fn drop(&mut self) {
        if self._frame_save.take().is_some() {
            frame::save();
        }
    }
}

/// Presses in the app's other windows after the page window opened, each one a click away. On
/// macOS and Windows the first one also takes focus from the window, which the shared click-away
/// rule reports too; `close_from_corner` drops that report so one press counts once.
fn observe_presses_elsewhere(
    window: &mut Window,
    cx: &mut Context<GpuiVisualPageModalWindow>,
) -> Subscription {
    let own_window = gpui::Window::window_handle(window).window_id();
    cx.observe_global_in::<super::popup_dismissal::MainWindowPointerPress>(
        window,
        move |this, _, cx| {
            let press = *cx.global::<super::popup_dismissal::MainWindowPointerPress>();
            if press.at <= this.shown_at
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floating_controls_fit_inside_the_band_over_the_page() {
        assert!(CONTROL_INSET + CONTROL_SIZE <= TOP_BAND_HEIGHT);
    }

    /// A floating page starts under the band, so its template drops the band's height from its
    /// 56px top padding and its content lands where it would at the top of a browser tab.
    #[test]
    fn a_floating_page_drops_the_band_from_its_top_padding() {
        let theme = include_str!("../../../../../server/src/visual_pages/theme.rs");
        let padding = 56.0 - TOP_BAND_HEIGHT;
        assert!(theme.contains(&format!(
            "html[data-gx-floating] gx-page,html[data-gx-floating] gx-questions{{padding-top:{padding}px}}"
        )));
    }
}
