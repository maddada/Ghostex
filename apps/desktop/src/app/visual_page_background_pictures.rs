//! Pictures for page cards whose page nobody opened yet: when a card is drawn in a chat that is on
//! screen and gxserver says its page may be pictured unopened (`backgroundPicture`, only pages
//! published since 2026-11-02), the app loads the page in a hidden browser of the main window,
//! waits until it has loaded with its web fonts and pictures, takes the same picture the floating
//! page window takes (`visual_page_thumbnails.rs`) and closes the browser again. One page at a
//! time, only while the web runtime is installed, and never in Low Power Mode.
//!
//! CDXC:SessionChat 2026-11-02 SEE-ALSO:
//! The user's decision is on server/src/visual_pages/thumbnail.rs (which also marks new pages, with store.rs). The card asks from apps/desktop/src/app/native_chat/page_card.rs (`pictureUnopenedPage`), the core passes gxserver's mark on in packages/gx-chat-core/src/transcript/page_thumbnails.rs, and the screenshot is apps/desktop/src/cef/shell/page_screenshot.rs.
//! CDXC:CefRuntime 2026-11-02 WHY:
//! The page's browser is an ordinary windowed CEF browser of the main window, created hidden and given a frame the size the floating page window would have (`set_background_bounds`, as background Browser pages get), so it never shows, takes no input and needs no windowless rendering. DevTools pictures a hidden page the way it pictures a background Browser tab.

use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    sync::atomic::Ordering,
    time::Duration,
};

use futures::FutureExt as _;

use crate::app::helpers::web_runtime::{WebRuntimeState, web_runtime_state};
use crate::app::helpers::*;
use crate::app::visual_page_thumbnails::{
    CAPTURING, read_picture_state, screenshot_page_top, store_picture,
};
use crate::app::window::native_modal_kit::hsla;
use crate::app::window::visual_page_modal::VISUAL_PAGE_CEF_PROFILE_ID;
use crate::*;

/// The floating page window's header, which the page sits under.
const PAGE_WINDOW_HEADER_HEIGHT: f32 = 44.0;
/// How long a page gets to load before it is given up on.
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a page gets after it loaded for its scripts to build it.
const SETTLE: Duration = Duration::from_millis(800);
/// The wait before trying again while the web runtime starts or another picture is being taken.
const BUSY_RETRY: Duration = Duration::from_secs(2);
/// How many times a page waits for the runtime or another picture before it is let go; its card
/// offers it again when it is drawn again.
const BUSY_RETRIES: u32 = 30;

/// A drawn card's page that has no picture yet.
#[derive(Clone)]
pub(crate) struct UnopenedPage {
    pub(crate) page_id: String,
    /// The page's address as this computer reaches it.
    pub(crate) url: String,
    /// The chat the card is drawn in.
    pub(crate) session_id: TerminalSessionId,
    pub(crate) remote: Option<GpuiRemoteGxserverRequestTarget>,
}

/// Which pages are pictured, in what order: one at a time, each page once per app run, and only
/// while the chat its card was drawn in is still on screen.
#[derive(Default)]
pub(crate) struct UnopenedPageQueue {
    waiting: VecDeque<UnopenedPage>,
    running: Option<String>,
    tried: HashSet<String>,
    /// Busy retries left for the page at the head of the queue.
    busy_retries: u32,
}

impl UnopenedPageQueue {
    /// A card on screen offers its page; `false` when the page is already waiting, being pictured
    /// or was tried in this run.
    pub(crate) fn offer(&mut self, page: UnopenedPage) -> bool {
        if self.tried.contains(&page.page_id)
            || self.running.as_deref() == Some(page.page_id.as_str())
            || self.waiting.iter().any(|w| w.page_id == page.page_id)
        {
            return false;
        }
        self.waiting.push_back(page);
        true
    }

    /// The next page to picture, while none is: the first waiting page whose chat is still on
    /// screen. Pages whose chat left the screen are dropped (not counted as tried), so their cards
    /// offer them again once they are drawn again.
    pub(crate) fn start_next(
        &mut self,
        on_screen: impl Fn(&UnopenedPage) -> bool,
    ) -> Option<UnopenedPage> {
        if self.running.is_some() {
            return None;
        }
        while let Some(page) = self.waiting.pop_front() {
            if on_screen(&page) {
                self.tried.insert(page.page_id.clone());
                self.running = Some(page.page_id.clone());
                return Some(page);
            }
        }
        None
    }

    /// The page being pictured is done, pictured or not.
    pub(crate) fn finish(&mut self, page_id: &str) {
        if self.running.as_deref() == Some(page_id) {
            self.running = None;
            self.busy_retries = 0;
        }
    }

    /// The page being pictured could not start yet (the runtime is starting, another picture is
    /// being taken): it goes back to the head of the queue, until it has waited `BUSY_RETRIES`
    /// times. Returns whether it went back.
    pub(crate) fn put_back(&mut self, page: UnopenedPage) -> bool {
        if self.running.as_deref() == Some(page.page_id.as_str()) {
            self.running = None;
        }
        self.tried.remove(&page.page_id);
        self.busy_retries += 1;
        if self.busy_retries > BUSY_RETRIES {
            self.busy_retries = 0;
            return false;
        }
        self.waiting.push_front(page);
        true
    }

    /// Lets go of a page without counting it as tried (Low Power Mode, no runtime), so a later
    /// draw of its card offers it again.
    pub(crate) fn forget(&mut self, page_id: &str) {
        self.finish(page_id);
        self.tried.remove(page_id);
    }

    /// Drops every waiting page, which their cards offer again when drawn.
    pub(crate) fn clear_waiting(&mut self) {
        self.waiting.clear();
    }

    #[cfg(test)]
    fn has_waiting(&self) -> bool {
        !self.waiting.is_empty()
    }
}

/// Whether gxserver's answer about a page's picture (`readVisualPageThumbnail`) lets it be pictured
/// unopened: the page is still there, has no picture, and was published since background pictures
/// (pages published before carry no mark and wait until they are opened).
pub(crate) fn may_picture_unopened(state: &serde_json::Value) -> bool {
    state["backgroundPicture"] == true && state["gone"] != true && state["capturedAt"].is_null()
}

thread_local! {
    static QUEUE: RefCell<UnopenedPageQueue> = RefCell::default();
}

/// Low Power Mode is on (macOS `pmset -g`); never on other systems.
fn low_power_mode() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::app::helpers::agents_hub::keep_awake::gpui_keep_awake_fixed_command_stdout(
            "/usr/bin/pmset",
            &["-g"],
        )
        .ok()
        .as_deref()
        .and_then(crate::app::helpers::agents_hub::keep_awake::gpui_parse_pmset_low_power_mode)
            == Some(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

enum Outcome {
    /// Pictured, or found not to need or allow it.
    Done,
    /// Could not start now; try again shortly.
    Busy,
    /// Not now (Low Power Mode); a later draw of the card offers it again.
    NotNow,
}

impl GhostexGpuiApp {
    /// A page card's `pictureUnopenedPage` (native_chat/page_card.rs): its page has no picture and
    /// may be pictured without being opened.
    pub(crate) fn picture_unopened_page(
        &mut self,
        session_id: TerminalSessionId,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message[key]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(page_id), Some(url)) = (text("pageId"), text("url")) else {
            return;
        };
        // A remote session's page is reached through this computer's tunnel to its machine.
        let (url, remote) = match self
            .remote_project_for_terminal_target(GpuiEngineTerminalEventTarget::Agents(session_id))
        {
            None => (url, None),
            Some(remote_project) => {
                let Some(target) =
                    self.gpui_remote_gxserver_request_target(&remote_project.remote_machine_id)
                else {
                    return;
                };
                match gpui_remote_machine_url(&url, &target) {
                    GpuiRemoteMachineUrl::Gxserver(url) => (url, Some(target)),
                    _ => return,
                }
            }
        };
        let port = remote
            .as_ref()
            .map_or_else(gpui_local_gxserver_api_port, |remote| remote.local_port);
        if crate::app::visual_page_thumbnails::served_page_id(&url, port).as_deref()
            != Some(page_id.to_ascii_lowercase().as_str())
        {
            return;
        }
        let offered = QUEUE.with(|queue| {
            queue.borrow_mut().offer(UnopenedPage {
                page_id: page_id.to_ascii_lowercase(),
                url,
                session_id,
                remote,
            })
        });
        if offered {
            self.picture_next_unopened_page(cx);
        }
    }

    /// The chat `session_id` is on screen: an Agents pane shows it as a chat, or the side panel shows
    /// it.
    fn session_chat_on_screen(&self, session_id: TerminalSessionId) -> bool {
        self.agents_terminal_chat_is_visible(session_id)
            || self.side_panel_chat_on_screen_shell_session() == Some(session_id)
    }

    fn picture_next_unopened_page(&mut self, cx: &mut gpui::Context<Self>) {
        // Only an installed runtime is used: nothing is downloaded for a picture.
        let runtime_up = cef::context_initialized();
        if !runtime_up {
            match web_runtime_state() {
                WebRuntimeState::Installed | WebRuntimeState::Running => {}
                _ => {
                    QUEUE.with(|queue| queue.borrow_mut().clear_waiting());
                    return;
                }
            }
        }
        let next = QUEUE.with(|queue| {
            queue
                .borrow_mut()
                .start_next(|page| self.session_chat_on_screen(page.session_id))
        });
        let Some(page) = next else {
            return;
        };
        if !runtime_up {
            self.request_cef_runtime(cx);
            self.retry_unopened_page_later(page, cx);
            return;
        }
        let Some(window) = self.main_window_handle else {
            QUEUE.with(|queue| queue.borrow_mut().forget(&page.page_id));
            return;
        };
        let light = self.gpui_native_modal_palette().light;
        let background = hsla(self.gpui_native_modal_palette().surface);
        let (width, height) = self.chat_preview_modal_size(cx);
        let viewport = (width, (height - PAGE_WINDOW_HEADER_HEIGHT).max(1.0));
        cx.spawn(async move |this, cx| {
            let outcome =
                picture_page_unopened(&page, window, light, background, viewport, cx).await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Outcome::Done => QUEUE.with(|queue| queue.borrow_mut().finish(&page.page_id)),
                    Outcome::NotNow => QUEUE.with(|queue| queue.borrow_mut().forget(&page.page_id)),
                    Outcome::Busy => {
                        this.retry_unopened_page_later(page, cx);
                        return;
                    }
                }
                this.picture_next_unopened_page(cx);
            });
        })
        .detach();
    }

    fn retry_unopened_page_later(&mut self, page: UnopenedPage, cx: &mut gpui::Context<Self>) {
        if !QUEUE.with(|queue| queue.borrow_mut().put_back(page)) {
            return;
        }
        let timer = cx.background_executor().timer(BUSY_RETRY);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| this.picture_next_unopened_page(cx));
        })
        .detach();
    }
}

/// Loads `page` in a hidden browser of `window`, pictures its top and closes the browser.
async fn picture_page_unopened(
    page: &UnopenedPage,
    window: gpui::AnyWindowHandle,
    light: bool,
    background: Hsla,
    viewport: (f32, f32),
    cx: &mut gpui::AsyncApp,
) -> Outcome {
    let Some(state) = read_picture_state(page.remote.clone(), &page.page_id).await else {
        return Outcome::Done;
    };
    if !may_picture_unopened(&state) {
        return Outcome::Done;
    }
    if cx
        .background_executor()
        .spawn(async { low_power_mode() })
        .await
    {
        return Outcome::NotNow;
    }
    if CAPTURING.swap(true, Ordering::AcqRel) {
        return Outcome::Busy;
    }
    let outcome = load_and_picture(page, window, light, background, viewport, cx).await;
    CAPTURING.store(false, Ordering::Release);
    outcome
}

async fn load_and_picture(
    page: &UnopenedPage,
    window: gpui::AnyWindowHandle,
    light: bool,
    background: Hsla,
    viewport: (f32, f32),
    cx: &mut gpui::AsyncApp,
) -> Outcome {
    let theme = if light { "light" } else { "dark" };
    let page_url = format!(
        "{}#gx-theme={theme}",
        page.url.split('#').next().unwrap_or(&page.url)
    );
    let (loaded_tx, mut loaded) = futures::channel::mpsc::unbounded::<()>();
    // Called from inside CEF's message pump: it only fills a channel.
    let page_loaded: cef::PageLoadEndHandler = std::rc::Rc::new(move || {
        let _ = loaded_tx.unbounded_send(());
    });
    let created = window.update(cx, |_, window, cx| {
        let parent = cef_parent_native_view(window).map_err(|error| error.to_string())?;
        let surface = CefSurface::try_new(
            "ghostex-gpui-visual-page-picture".to_string(),
            parent,
            page_url,
            VISUAL_PAGE_CEF_PROFILE_ID.to_string(),
            pane_prepaint_background_color(),
            false,
            background,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(page_loaded),
            None,
            cx,
        )?;
        surface.read(cx).set_background_bounds(
            gpui::Bounds::new(
                gpui::point(px(0.0), px(0.0)),
                gpui::size(px(viewport.0), px(viewport.1)),
            ),
            window.scale_factor(),
        );
        Ok::<_, String>(surface)
    });
    let Ok(Ok(surface)) = created else {
        return Outcome::Done;
    };
    let timeout = cx.background_executor().timer(LOAD_TIMEOUT);
    let loaded = futures::select! {
        loaded = futures::StreamExt::next(&mut loaded).fuse() => loaded.is_some(),
        _ = timeout.fuse() => false,
    };
    let mut pictured = None;
    if loaded {
        cx.background_executor().timer(SETTLE).await;
        pictured = screenshot_page_top(&surface.downgrade(), viewport, cx).await;
    }
    // Closes the page's browser.
    let _ = cx.update(|_| drop(surface));
    if let Some(base64) = pictured {
        let _ = store_picture(base64, &page.page_id, page.remote.clone(), cx).await;
    }
    Outcome::Done
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(id: &str, session: u64) -> UnopenedPage {
        UnopenedPage {
            page_id: id.to_string(),
            url: format!("http://127.0.0.1:1/visual/{id}"),
            session_id: TerminalSessionId(session),
            remote: None,
        }
    }

    #[test]
    fn only_new_pages_without_a_picture_are_pictured_unopened() {
        // Published since background pictures, no picture yet.
        assert!(may_picture_unopened(&json!({
            "backgroundPicture": true, "gone": false, "capturedAt": null, "stale": true
        })));
        // Published before: gxserver gives no mark (an older gxserver says nothing at all).
        assert!(!may_picture_unopened(&json!({
            "backgroundPicture": false, "gone": false, "capturedAt": null, "stale": true
        })));
        assert!(!may_picture_unopened(&json!({
            "gone": false, "capturedAt": null, "stale": true
        })));
        // Already pictured, or gone.
        assert!(!may_picture_unopened(&json!({
            "backgroundPicture": true, "gone": false, "capturedAt": "2026-11-02T00:00:00.000Z"
        })));
        assert!(!may_picture_unopened(&json!({
            "backgroundPicture": true, "gone": true, "capturedAt": null
        })));
    }

    #[test]
    fn one_page_at_a_time_in_the_order_offered() {
        let mut queue = UnopenedPageQueue::default();
        assert!(queue.offer(page("a", 1)));
        assert!(queue.offer(page("b", 1)));
        assert_eq!(
            queue.start_next(|_| true).map(|p| p.page_id),
            Some("a".into())
        );
        // A picture is being taken: nothing else starts.
        assert!(queue.start_next(|_| true).is_none());
        queue.finish("a");
        assert_eq!(
            queue.start_next(|_| true).map(|p| p.page_id),
            Some("b".into())
        );
    }

    #[test]
    fn a_page_is_offered_once_per_run() {
        let mut queue = UnopenedPageQueue::default();
        assert!(queue.offer(page("a", 1)));
        // Waiting already.
        assert!(!queue.offer(page("a", 1)));
        queue.start_next(|_| true);
        // Being pictured.
        assert!(!queue.offer(page("a", 1)));
        queue.finish("a");
        // Tried in this run.
        assert!(!queue.offer(page("a", 1)));
    }

    #[test]
    fn only_pages_whose_chat_is_on_screen_start() {
        let mut queue = UnopenedPageQueue::default();
        queue.offer(page("hidden", 1));
        queue.offer(page("shown", 2));
        let on_screen = |p: &UnopenedPage| p.session_id == TerminalSessionId(2);
        assert_eq!(
            queue.start_next(on_screen).map(|p| p.page_id),
            Some("shown".into())
        );
        queue.finish("shown");
        // The page whose chat left the screen was dropped, not tried: drawn again, it is offered
        // again and starts once its chat is on screen.
        assert!(!queue.has_waiting());
        assert!(queue.offer(page("hidden", 1)));
        assert_eq!(
            queue.start_next(|_| true).map(|p| p.page_id),
            Some("hidden".into())
        );
    }

    #[test]
    fn a_busy_page_goes_back_to_the_head_until_it_has_waited_enough() {
        let mut queue = UnopenedPageQueue::default();
        queue.offer(page("a", 1));
        queue.offer(page("b", 1));
        let first = queue.start_next(|_| true).unwrap();
        assert!(queue.put_back(first));
        assert_eq!(
            queue.start_next(|_| true).map(|p| p.page_id),
            Some("a".into())
        );
        for _ in 1..BUSY_RETRIES {
            assert!(queue.put_back(page("a", 1)));
            queue.start_next(|_| true);
        }
        // Let go after waiting too long; the next page goes on.
        assert!(!queue.put_back(page("a", 1)));
        assert_eq!(
            queue.start_next(|_| true).map(|p| p.page_id),
            Some("b".into())
        );
    }

    #[test]
    fn a_page_let_go_for_now_is_offered_again() {
        let mut queue = UnopenedPageQueue::default();
        queue.offer(page("a", 1));
        queue.start_next(|_| true);
        queue.forget("a");
        assert!(queue.offer(page("a", 1)));
    }
}
