//! The first page of the automatic first run, before Welcome: the Ghostex intro video in the
//! operating system's own web view, with Continue into setup.
//!
//! CDXC:Onboarding 2026-10-01 DECISION:
//! User: "I don't want to force CEF install, so let's use the built-in web view on each OS just for this YouTube embed." The intro video (the Ghostex v10 launch video, unlisted on YouTube) is the first screen of the first run, before the setup panels, once per install (`introVideoSeen` in gpui-first-run-onboarding-state.json). It plays in the system web view (intro_web_view.rs), never CEF, and that web view exists only on this page. Linux links no system web view (WebKitGTK would be a new library every Linux install must have), and there the user chose to keep a link: "a still from the video" filling the frame "with a YouTube play button inside it and the text 'Watch the intro on YouTube'", opening the video in the browser from anywhere on it. The still is bundled (assets/onboarding/intro-video.webp), never fetched from YouTube.
//! CDXC:Onboarding 2026-10-01 WHY:
//! Nothing here waits on the network: YouTube is asked first, a frame that cannot reach it says so over the same still, and Continue always works.
//! SEE-ALSO: apps/desktop/native/macos/GpuiIntroVideoWebView.m, apps/desktop/src/app/os_integration/first_run_onboarding.rs (the once-per-install gate), apps/desktop/src/app/onboarding_modal_lifecycle.rs (`IntroVideoSeen`).
use super::intro_web_view::IntroWebView;
use super::primitives::*;
use super::stage::*;
use super::{GpuiOnboardingWindow, OnboardingCommand, interact};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _,
    Styled as _, StyledImage as _, Window, canvas, div, img,
};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The Ghostex v10 launch video (https://youtu.be/QzjFB4J6-8E, unlisted, embedding allowed).
const INTRO_VIDEO_ID: &str = "QzjFB4J6-8E";

const VIDEO_WIDTH: f32 = 1024.0;
const VIDEO_HEIGHT: f32 = 576.0;
const VIDEO_LEFT: f32 = (STAGE_WIDTH - VIDEO_WIDTH) / 2.0;
const VIDEO_TOP: f32 = 212.0;
/// The glass rim around the player.
const BEZEL: f32 = 7.0;

/// The player starts at once: the web view is only made when the user presses Play.
fn embed_url() -> String {
    format!(
        "https://www.youtube-nocookie.com/embed/{INTRO_VIDEO_ID}?rel=0&playsinline=1&fs=0&autoplay=1"
    )
}

/// CDXC:Onboarding 2026-10-07 WHY:
/// YouTube's player shows "Video player configuration error" (error 153) unless the embed request names the embedding site in its Referer. The web view used to open the embed URL as its own top-level page with a Referer header added to the navigation, but WebView2 drops that header (reproduced in WebView2 on Windows 11: error 153 on the first run), so the player now sits in an iframe inside this small page, which the web view serves from a real https origin (intro_web_view.rs: `https://ghostex.localhost/` on Windows, `https://ghostex.dev/` as the WKWebView base URL on macOS). The browser then sends that origin as the iframe's Referer itself, the way any website's embed works. Never load the page from `data:`, `about:blank` or NavigateToString: those origins are opaque and send no Referer.
fn embed_page_html() -> String {
    format!(
        concat!(
            "<!doctype html><html><head><meta charset=\"utf-8\">",
            "<meta name=\"referrer\" content=\"strict-origin-when-cross-origin\"></head>",
            "<body style=\"margin:0;background:#000;overflow:hidden\">",
            "<iframe src=\"{}\" title=\"Ghostex intro video\" ",
            "referrerpolicy=\"strict-origin-when-cross-origin\" allow=\"autoplay; encrypted-media; picture-in-picture\" ",
            "style=\"position:fixed;inset:0;width:100%;height:100%;border:0\"></iframe>",
            "</body></html>"
        ),
        embed_url().replace('&', "&amp;")
    )
}

fn watch_url() -> String {
    format!("https://www.youtube.com/watch?v={INTRO_VIDEO_ID}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    /// The still with a play button; nothing is loaded until Play is pressed.
    Idle,
    /// Asking YouTube whether it answers, before the web view is made.
    Checking,
    /// The web view holds the player.
    Ready,
    /// YouTube did not answer.
    Offline,
    /// No system web view: a Linux build, or one that did not start.
    NoWebView,
}

pub(crate) struct IntroVideo {
    status: Status,
    web_view: Option<Rc<IntroWebView>>,
    /// Bumped by every check, so an older, slower answer cannot overwrite a newer one.
    attempt: u64,
    changed_at: Instant,
}

impl GpuiOnboardingWindow {
    /// CDXC:Onboarding 2026-10-16 WHY:
    /// Issue #237 (Windows 11, a 1366x768 screen): on the page with the player loaded, neither Continue nor anything else took a click. Every element is the same GPUI layout that takes clicks on macOS, and Windows Program Files installs only got a working WebView2 here in v10.14 (before that its creation failed and the page showed the still), so the system web view is the one Windows-only piece on the page. The page now opens on the bundled still with a play button and makes no web view until Play is pressed: Continue (a click, Enter, the right arrow, or Tab to it) never depends on the web view having started, and anyone who skips the video never starts WebView2 or WKWebView at all.
    pub(super) fn start_intro_video(&mut self) {
        self.intro = Some(IntroVideo {
            status: if IntroWebView::SUPPORTED {
                Status::Idle
            } else {
                Status::NoWebView
            },
            web_view: None,
            attempt: 0,
            changed_at: Instant::now(),
        });
    }

    fn check_intro_video(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(intro) = self.intro.as_mut() else {
            return;
        };
        intro.attempt += 1;
        intro.status = Status::Checking;
        intro.web_view = None;
        intro.changed_at = Instant::now();
        let attempt = intro.attempt;
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let reachable = background.spawn(async move { youtube_answers() }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.intro_video_checked(attempt, reachable, window, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn intro_video_checked(
        &mut self,
        attempt: u64,
        reachable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(intro) = self.intro.as_mut() else {
            return;
        };
        if intro.attempt != attempt || intro.status != Status::Checking {
            return;
        }
        intro.status = if !reachable {
            Status::Offline
        } else {
            match IntroWebView::new(window, &embed_page_html()) {
                Ok(web_view) => {
                    intro.web_view = Some(Rc::new(web_view));
                    Status::Ready
                }
                Err(_) => Status::NoWebView,
            }
        };
        intro.changed_at = Instant::now();
        cx.notify();
    }

    pub(super) fn intro_video_active(&self) -> bool {
        self.intro.is_some()
    }

    /// Continue: the web view leaves with the page, the page counts as seen, and Welcome fades in.
    pub(crate) fn leave_intro_video(&mut self, cx: &mut Context<Self>) {
        if self.intro.take().is_none() {
            return;
        }
        self.send(OnboardingCommand::IntroVideoSeen, cx);
        self.scene_started = Instant::now();
        self.mount_panel(self.scene_started, cx);
        self.focus.anchor.set(interact::TabAnchor::PanelStart);
        cx.notify();
    }

    /// Hands the links the player opened in a new window to the default browser.
    pub(super) fn tick_intro_video(&mut self, cx: &mut Context<Self>) {
        let links = self
            .intro
            .as_ref()
            .and_then(|intro| intro.web_view.as_ref())
            .map(|web_view| web_view.take_opened_links())
            .unwrap_or_default();
        for url in links {
            self.send(OnboardingCommand::OpenExternalUrl(url), cx);
        }
    }

    /// The page's lockup, where Welcome draws it, so nothing moves when the page changes.
    pub(super) fn render_intro_video_chrome(&self, s: S, now: Instant) -> Vec<AnyElement> {
        // The Welcome divider and veil slide in from the right edge once the page is left.
        let _ = self
            .transitions
            .value("divider", STAGE_WIDTH, STAGE_MOVE, now);
        vec![
            abs(s, PANEL_LOCKUP_X[0] - 9.0, 19.0, None, None)
                .flex()
                .items_center()
                .gap(s.px(12.0))
                .child(ghostex_logo(s, 34.0, false))
                .child(
                    sans(s, 17.0, 500.0, hex(0xe5e9f1))
                        .line_height(s.px(17.0))
                        .child("Ghostex"),
                )
                .into_any_element(),
        ]
    }

    pub(super) fn render_intro_video(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(status) = self.intro.as_ref().map(|intro| intro.status) else {
            return Vec::new();
        };
        let mut out = vec![
            eyebrow(s, 0.0, 74.0, Some(STAGE_WIDTH), "Welcome to Ghostex").into_any_element(),
            heading(
                s,
                0.0,
                100.0,
                STAGE_WIDTH,
                44.0,
                "See Ghostex in action",
                None,
                true,
            )
            .into_any_element(),
            sub(s, (STAGE_WIDTH - 760.0) / 2.0, 162.0, 760.0, 15.5, true)
                .child(
                    "A quick tour of what Ghostex can do. Watch it now, or continue to setup whenever you're ready.",
                )
                .into_any_element(),
            self.render_intro_video_frame(s, now, cx),
        ];
        out.push(
            abs(
                s,
                VIDEO_LEFT,
                FOOT_TOP,
                Some(VIDEO_WIDTH),
                Some(FOOT_HEIGHT),
            )
            .flex()
            .items_center()
            .justify_between()
            .child(div().when(status != Status::NoWebView, |this| {
                this.child(
                    self.control(
                        s,
                        ghost(s, "intro-video-youtube", "Watch on YouTube", 15.0, false)
                            .gap(s.px(8.0))
                            .child(icon(s, "external", 15.0, 1.6, hex(0x9aa3b4))),
                        "intro-video-youtube",
                        interact::Ring::new(8.0, 0.0),
                        interact::Keys::EnterSpace,
                        cx,
                        |this, _, cx| {
                            this.send(OnboardingCommand::OpenExternalUrl(watch_url()), cx)
                        },
                    ),
                )
            }))
            .child(self.control(
                s,
                cta(
                    s,
                    "intro-video-continue",
                    "Continue",
                    true,
                    true,
                    false,
                    CtaSize::Foot,
                ),
                "intro-video-continue",
                interact::Ring::new(10.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| this.leave_intro_video(cx),
            ))
            .into_any_element(),
        );
        out
    }

    fn render_intro_video_frame(&self, s: S, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let Some(intro) = self.intro.as_ref() else {
            return div().into_any_element();
        };
        let message = |title: &str, body: &str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(s.px(8.0))
                .child(sans(s, 18.0, 500.0, hex(0xeef1f7)).child(title.to_string()))
                .child(
                    sans(s, 14.5, 400.0, hex(0xc3c9d4))
                        .max_w(s.px(520.0))
                        .text_center()
                        .child(body.to_string()),
                )
        };
        // Every state draws over the same still from the video, so the frame never shows an empty box.
        let still = || {
            img("onboarding/intro-video.webp")
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .object_fit(ObjectFit::Cover)
        };
        let veil = |alpha: f32| {
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .bg(black(alpha))
        };
        let centered = || {
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
        };
        let inner: AnyElement = match intro.status {
            // The still shows until the player paints its own thumbnail over it.
            Status::Ready => {
                let web_view = intro.web_view.clone();
                div()
                    .size_full()
                    .relative()
                    .child(still())
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                if let Some(web_view) = &web_view {
                                    web_view.place(bounds);
                                }
                                window.occlude_native_region(bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .left_0()
                        .top_0()
                        .size_full(),
                    )
                    .into_any_element()
            }
            Status::Checking => div()
                .size_full()
                .relative()
                .child(still())
                .child(veil(0.62))
                .child(
                    centered()
                        .gap(s.px(14.0))
                        .child(spinner(s, 22.0, 2.0, intro.changed_at, now))
                        .child(sans(s, 14.5, 400.0, hex(0xc3c9d4)).child("Loading the video…")),
                )
                .into_any_element(),
            Status::Offline => div()
                .size_full()
                .relative()
                .child(still())
                .child(veil(0.84))
                .child(
                    centered()
                        .gap(s.px(18.0))
                        .child(icon(s, "wifi", 28.0, 1.6, hex(0x8c98c2)))
                        .child(message(
                            "The video can't load right now",
                            "Check your internet connection, or continue to setup and watch it later on YouTube.",
                        ))
                        .child(self.control(
                            s,
                            cta(
                                s,
                                "intro-video-retry",
                                "Try again",
                                false,
                                false,
                                false,
                                CtaSize::Small,
                            ),
                            "intro-video-retry",
                            interact::Ring::new(10.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, window, cx| this.check_intro_video(window, cx),
                        )),
                )
                .into_any_element(),
            // The still with a play button: Play loads the player here, or opens YouTube in the
            // browser where there is no system web view.
            Status::Idle | Status::NoWebView => {
                let plays_here = intro.status == Status::Idle;
                let key = "intro-video-watch";
                let dim = interact::tween_value(
                    key,
                    "dim",
                    if interact::hovered(key) { 0.16 } else { 0.3 },
                    200,
                );
                let red = interact::hover_color(key, "red", hex(0xe8002d), hex(0xff0033), 200);
                self.control(
                    s,
                    div()
                        .id(key)
                        .size_full()
                        .relative()
                        .cursor_pointer()
                        .child(still())
                        .child(veil(dim))
                        .child(
                            centered()
                                .gap(s.px(18.0))
                                .child(
                                    div()
                                        .w(s.px(76.0))
                                        .h(s.px(54.0))
                                        .rounded(s.px(14.0))
                                        .bg(red)
                                        .shadow(vec![shadow(black(0.35), 0.0, 6.0, 18.0, 0.0, s)])
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(icon(s, "play", 26.0, 1.6, gpui::white())),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(s.px(8.0))
                                        .py(s.px(8.0))
                                        .px(s.px(16.0))
                                        .rounded_full()
                                        .bg(rgba(10, 12, 18, 0.74))
                                        .child(sans(s, 15.5, 500.0, gpui::white()).child(
                                            if plays_here {
                                                "Play the intro"
                                            } else {
                                                "Watch the intro on YouTube"
                                            },
                                        ))
                                        .when(!plays_here, |this| {
                                            this.child(icon(s, "external", 14.0, 1.6, gpui::white()))
                                        }),
                                )
                                // CDXC:Onboarding 2026-10-28 WHY: without a system web view the still opens YouTube in the browser, which left the app with no warning; it now says so under the decided label.
                                .when(!plays_here, |this| {
                                    this.child(
                                        div()
                                            .py(s.px(4.0))
                                            .px(s.px(12.0))
                                            .rounded_full()
                                            .bg(rgba(10, 12, 18, 0.6))
                                            .child(
                                                sans(s, 13.0, 400.0, hex(0xd6dbe5))
                                                    .child("Opens in your web browser"),
                                            ),
                                    )
                                }),
                        ),
                    key,
                    interact::Ring::new(8.0, 0.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, window, cx| {
                        if plays_here {
                            this.check_intro_video(window, cx);
                        } else {
                            this.send(OnboardingCommand::OpenExternalUrl(watch_url()), cx);
                        }
                    },
                )
                .into_any_element()
            }
        };
        glass(s)
            .absolute()
            .left(s.px(VIDEO_LEFT - BEZEL))
            .top(s.px(VIDEO_TOP - BEZEL))
            .w(s.px(VIDEO_WIDTH + 2.0 * BEZEL))
            .h(s.px(VIDEO_HEIGHT + 2.0 * BEZEL))
            .p(s.px(BEZEL - 1.0))
            .child(
                div()
                    .size_full()
                    .rounded(s.px(8.0))
                    .overflow_hidden()
                    .bg(hex(0x000000))
                    .child(inner),
            )
            .into_any_element()
    }
}

/// Entry points the preview binary uses to show the page's other states.
#[allow(dead_code)]
impl GpuiOnboardingWindow {
    pub(crate) fn preview_intro_video_offline(&mut self, cx: &mut Context<Self>) {
        self.preview_intro_video_status(Status::Offline, cx);
    }

    pub(crate) fn preview_intro_video_no_web_view(&mut self, cx: &mut Context<Self>) {
        self.preview_intro_video_status(Status::NoWebView, cx);
    }

    /// Presses Play on the still.
    pub(crate) fn preview_intro_video_play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.check_intro_video(window, cx);
    }

    fn preview_intro_video_status(&mut self, status: Status, cx: &mut Context<Self>) {
        if let Some(intro) = self.intro.as_mut() {
            intro.attempt += 1;
            intro.status = status;
            intro.web_view = None;
            intro.changed_at = Instant::now();
            cx.notify();
        }
    }
}

/// Whether YouTube answers at all; any HTTP reply counts. Blocking: call it off the main thread.
fn youtube_answers() -> bool {
    let tls_config = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .tls_config(tls_config)
        .build();
    ureq::Agent::new_with_config(config)
        .get(&embed_url())
        .call()
        .is_ok()
}
