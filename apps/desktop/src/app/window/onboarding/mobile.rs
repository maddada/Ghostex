//! Panel 4, Mobile: the phone and notification switches and the looping Easy Connect pairing demo
//! (panels/mobile.tsx (deleted 2026-10-01), previews/phone-pairing.tsx (deleted 2026-10-01), styles/mobile.css).
//!
//! CDXC:Onboarding 2026-09-28 WHY:
//! The React phone mockup leans back in 3D (`perspective(1400px) rotateY(-6deg) rotateX(2deg)
//! rotateZ(1deg)`) and its camera frame is rotated -4deg. GPUI cannot transform a subtree of
//! elements, so each moment of the phone is the page's own rendering, captured from the React story
//! at 2x with a transparent background (apps/desktop/assets/onboarding/phone/), and only what moves
//! (spinner, scan line, pop, breathing dots, banner fade, hover, the card's click) is drawn on top at
//! the measured positions. The page also tilted the phone a few degrees toward the pointer; the
//! captured images hold its resting pose.
use super::GpuiOnboardingWindow;
use super::agents::switch_row;
use super::fonts::PLEX_MONO;
use super::interact;
use super::primitives::*;
use super::stage::*;
use super::welcome::{demo_link, foot_actions};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, SharedString, Styled as _, StyledImage as _, canvas, div, img,
};
use serde_json::Value;
use std::time::{Duration, Instant};

pub(crate) struct MobileState {
    started: Instant,
}

impl MobileState {
    pub(crate) fn new(now: Instant) -> Self {
        Self { started: now }
    }
}

/// Timeline of the looping pairing demo, in ms.
const EASY: u64 = 2000;
const TOGGLE: u64 = 2400;
const PROMPT: u64 = 2700;
const ALLOW: u64 = 4300;
const QR: u64 = 4500;
const SCAN: u64 = 5300;
const PAIRED: u64 = 6800;
const END: u64 = 8400;
const LOOP: u64 = 12400;

impl GpuiOnboardingWindow {
    /// CDXC:Onboarding 2026-09-15 DECISION:
    /// User: "in the take ghostex with you section, we need to not download the app. We should just have a toggle saying 'Configure my phone with ghostex after this flow' and all that does is it opens the sidebar menu -> remote screen for the user after the flow". There are no install or pair steps and no APK link; the toggle sets `phone_queued`, which opens Settings -> Remote (Easy Connect) once the flow closes.
    fn toggle_phone(&mut self, cx: &mut Context<Self>) {
        self.flow.phone_queued = !self.flow.phone_queued;
        cx.notify();
    }

    fn toggle_notify(&mut self, cx: &mut Context<Self>) {
        let mut patch = serde_json::Map::new();
        patch.insert(
            "showMacOSAttentionNotifications".into(),
            Value::Bool(!self.settings.notify),
        );
        self.update_settings(patch, cx);
    }

    fn toggle_floating_capture(&mut self, cx: &mut Context<Self>) {
        let mut patch = serde_json::Map::new();
        patch.insert(
            "ghostexCaptureEnabled".into(),
            Value::Bool(!self.settings.floating_capture),
        );
        self.update_settings(patch, cx);
    }

    pub(super) fn render_mobile(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let notify = self.settings.notify;
        let floating_capture = self.settings.floating_capture;
        let phone_queued = self.flow.phone_queued;
        let mut out = Vec::new();
        out.push(eyebrow(s, 46.0, 146.0, None, "Optional · Mobile").into_any_element());
        out.push(
            heading(
                s,
                46.0,
                174.0,
                700.0,
                52.0,
                "Take the session with you.",
                None,
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 46.0, 250.0, 660.0, 16.0, false)
                .child("The agents keep running on your computer while you reply, steer and keep working from your phone.")
                .into_any_element(),
        );
        let phone_thumb = self.thumb("mobile-phone", phone_queued, now);
        let phone_toggle = self.control(
            s,
            toggle(
                s,
                "mobile-phone-toggle",
                phone_queued,
                phone_thumb,
                ToggleSize::Lg,
                false,
            ),
            "mobile-phone-toggle",
            interact::Ring::new(17.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                this.toggle_phone(cx);
            },
        );
        let phone_row = switch_row(s, "mobile-phone-row", 326.0, 84.0, (20.0, 30.0), 26.0, None)
                .left(s.px(46.0))
                .w(s.px(660.0))
                .child(icon(s, "phone", 22.0, 1.6, hex(0xd6dbe5)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 17.0, 500.0).child("Configure my phone with Ghostex after this flow"))
                        .child(ss(s, 12.5).child(if phone_queued {
                            "Settings → Remote opens when you finish: turn on Easy Connect and scan the code with Ghostex Mobile."
                        } else {
                            "Pair Ghostex Mobile right after setup, so every session is on your phone too."
                        })),
                )
                .child(phone_toggle);
        out.push(
            self.control(
                s,
                phone_row,
                "mobile-phone-row",
                interact::Ring::new(14.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| this.toggle_phone(cx),
            )
            .into_any_element(),
        );
        let notify_thumb = self.thumb("mobile-notify", notify, now);
        let notify_toggle = self.control(
            s,
            toggle(
                s,
                "mobile-notify-toggle",
                notify,
                notify_thumb,
                ToggleSize::Lg,
                false,
            ),
            "mobile-notify-toggle",
            interact::Ring::new(17.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                this.toggle_notify(cx);
            },
        );
        let notify_row = switch_row(
            s,
            "mobile-notify-row",
            426.0,
            84.0,
            (20.0, 30.0),
            26.0,
            None,
        )
        .left(s.px(46.0))
        .w(s.px(660.0))
        .child(icon(s, "bell", 22.0, 1.6, hex(0xd6dbe5)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(nm(s, 17.0, 500.0).child("Ping me when an agent needs me"))
                .child(match (&self.notification_problem, notify) {
                    (Some(problem), true) => {
                        ss(s, 12.5).text_color(hex(0xffc46b)).child(problem.clone())
                    }
                    (_, true) => ss(s, 12.5).child(
                        "This computer and your phone alert you when an agent needs an answer.",
                    ),
                    (_, false) => ss(s, 12.5).child(
                        "No alerts on this computer or your phone: you find out when you look.",
                    ),
                }),
        )
        .child(notify_toggle);
        out.push(
            self.control(
                s,
                notify_row,
                "mobile-notify-row",
                interact::Ring::new(14.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| this.toggle_notify(cx),
            )
            .into_any_element(),
        );
        // CDXC:GhostexCapture 2026-09-30 DECISION:
        // User: add Floating Capture to the setup toggles "so people know about it when they are onboarding onto the app".
        let capture_thumb = self.thumb("mobile-capture", floating_capture, now);
        let capture_toggle = self.control(
            s,
            toggle(
                s,
                "mobile-capture-toggle",
                floating_capture,
                capture_thumb,
                ToggleSize::Lg,
                false,
            ),
            "mobile-capture-toggle",
            interact::Ring::new(17.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            |this, _, cx| {
                cx.stop_propagation();
                this.toggle_floating_capture(cx);
            },
        );
        let capture_row = switch_row(
            s,
            "mobile-capture-row",
            526.0,
            84.0,
            (20.0, 30.0),
            26.0,
            None,
        )
        .left(s.px(46.0))
        .w(s.px(660.0))
        .child(icon(s, "capture", 22.0, 1.6, hex(0xd6dbe5)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(nm(s, 17.0, 500.0).child("Floating Capture"))
                .child(ss(s, 12.5).child(if floating_capture {
                    "A small button floats over every app with your agents' counts; screenshot anything and prompt without switching to Ghostex."
                } else {
                    "Turn on a floating button to see your agents and send screenshots and prompts from any app."
                })),
        )
        .child(capture_toggle);
        out.push(
            self.control(
                s,
                capture_row,
                "mobile-capture-row",
                interact::Ring::new(14.0, 1.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| this.toggle_floating_capture(cx),
            )
            .into_any_element(),
        );
        out.push(
            abs(s, 46.0, 634.0, None, None)
                .flex()
                .child(
                    sans(s, 16.0, 400.0, hex(0xaeb6c4)).child("You can set this up anytime from "),
                )
                .child(sans(s, 16.0, 600.0, hex(0xeef1f7)).child("Settings → Remote"))
                .child(sans(s, 16.0, 400.0, hex(0xaeb6c4)).child("."))
                .into_any_element(),
        );
        out.push(
            foot_actions(s, 4)
                .child(self.control(
                    s,
                    cta(s, "mobile-next", "Next", true, true, false, CtaSize::Foot),
                    "mobile-next",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.go(5, cx),
                ))
                .into_any_element(),
        );
        out.extend(self.render_pairing_preview(s, now, notify, cx));
        out
    }

    fn render_pairing_preview(
        &self,
        s: S,
        now: Instant,
        notify: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let elapsed = now
            .saturating_duration_since(self.mobile.started)
            .as_millis() as u64;
        let t = elapsed % LOOP;
        let loop_started = self.mobile.started + Duration::from_millis(elapsed - t);
        let at = |ms: u64| loop_started + Duration::from_millis(ms);
        let mut out = vec![self.computer_card(s, now, t, notify, &at)];
        if t >= END && notify {
            let p = progress(
                at(END),
                now,
                Duration::from_millis(300),
                Ease::Bezier(0.2, 0.8, 0.2, 1.0),
            );
            out.push(
                abs(s, 926.0 + 40.0 * (1.0 - p), 178.0, Some(300.0), None)
                    .opacity(p)
                    .flex()
                    .items_center()
                    .gap(s.px(11.0))
                    .py(s.px(10.0))
                    .px(s.px(13.0))
                    .rounded(s.px(12.0))
                    .bg(rgba(30, 36, 52, 0.97))
                    .border_1()
                    .border_color(white(0.12))
                    .shadow(vec![shadow(black(0.55), 0.0, 14.0, 36.0, 0.0, s)])
                    .child(agent_logo(s, &self.catalog, "codex", 20.0))
                    .child(
                        sans(s, 11.0, 400.0, hex(0xc9d0dc))
                            .line_height(s.px(11.0 * 1.35))
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_end()
                                    .justify_between()
                                    .gap(s.px(8.0))
                                    .child(
                                        sans(s, 12.0, 700.0, gpui::white())
                                            .child("Codex needs you"),
                                    )
                                    .child(sans(s, 10.0, 400.0, hex(0x7f8aa0)).child("now")),
                            )
                            .child("Keep the global fixtures, or inline them?"),
                    )
                    .into_any_element(),
            );
        }
        let send = if (SCAN..SCAN + 1000).contains(&t) {
            Some((false, at(SCAN)))
        } else if (PAIRED..PAIRED + 1000).contains(&t) {
            Some((true, at(PAIRED)))
        } else {
            None
        };
        let label = if (SCAN..PAIRED).contains(&t) {
            Some(("Scanning", at(SCAN)))
        } else if t >= PAIRED {
            Some(("Paired", at(PAIRED)))
        } else {
            None
        };
        out.extend(demo_link(s, 1226.0, 470.0, 90.0, send, label, now));
        out.extend(self.phone_mockup(s, now, t, notify, &at, cx));
        out
    }

    fn computer_card(
        &self,
        s: S,
        now: Instant,
        t: u64,
        notify: bool,
        at: &dyn Fn(u64) -> Instant,
    ) -> AnyElement {
        let toggled = t >= TOGGLE;
        let allowed = t >= ALLOW;
        let qr_shown = t >= QR;
        let paired = t >= PAIRED;
        let done = t >= END;
        let typed = ((t as i64 - PROMPT as i64 - 300) / 140).clamp(0, 8) as usize;
        let thumb = self.transitions.value(
            "pairing-easy-connect",
            if toggled { 1.0 } else { 0.0 },
            (Duration::from_millis(220), Ease::Bezier(0.3, 0.8, 0.3, 1.0)),
            now,
        );
        let divider = || div().w_full().h(s.px(1.0)).bg(white(0.06));
        let qr: AnyElement = if qr_shown {
            let p = progress(
                at(QR),
                now,
                Duration::from_millis(400),
                Ease::Bezier(0.3, 1.6, 0.5, 1.0),
            );
            let size_px = (128.0 * p).max(0.0);
            div()
                .size(s.px(128.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(qr_code(s, size_px))
                .into_any_element()
        } else {
            div()
                .size(s.px(128.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(s.px(8.0))
                .border_1()
                .border_dashed()
                .border_color(white(0.15))
                .child(icon(s, "lock", 22.0, 1.6, hex(0x5a6272)))
                .into_any_element()
        };
        let sheet = (PROMPT..ALLOW).contains(&t).then(|| {
            let fade = progress(at(PROMPT), now, Duration::from_millis(200), Ease::Ease);
            let tapped = t > ALLOW - 400;
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(s.px(300.0))
                        .p(s.px(20.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(s.px(8.0))
                        .rounded(s.px(14.0))
                        .bg(hex(0x161a26))
                        .border_1()
                        .border_color(white(0.14))
                        .shadow(vec![shadow(black(0.7), 0.0, 24.0, 60.0, 0.0, s)])
                        .opacity(fade)
                        .child(icon(s, "lock", 22.0, 1.6, hex(0xeef1f7)))
                        .child(
                            sans(s, 14.0, 700.0, hex(0xeef1f7))
                                .text_center()
                                .child("Ghostex wants to turn on remote access."),
                        )
                        .child(
                            sans(s, 12.5, 400.0, hex(0xaab2c1))
                                .text_center()
                                .child("Enter your password to allow this."),
                        )
                        .child(
                            div()
                                .w_full()
                                .h(s.px(36.0))
                                .mt(s.px(6.0))
                                .flex()
                                .items_center()
                                .px(s.px(12.0))
                                .rounded(s.px(8.0))
                                .border_1()
                                .border_color(rgba(110, 150, 255, 0.5))
                                .bg(black(0.3))
                                .child(tracked_text(
                                    s,
                                    "•".repeat(typed),
                                    PLEX_MONO,
                                    400.0,
                                    16.0,
                                    20.0,
                                    0.2,
                                    hex(0xeef1f7),
                                ))
                                .child(
                                    div()
                                        .ml(s.px(2.0))
                                        .w(s.px(1.5))
                                        .h(s.px(16.0))
                                        .bg(hex(0x8fabff))
                                        .opacity(if blink(self.opened_at, now) {
                                            1.0
                                        } else {
                                            0.0
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .self_end()
                                .mt(s.px(6.0))
                                .h(s.px(32.0))
                                .px(s.px(16.0))
                                .flex()
                                .items_center()
                                .rounded(s.px(8.0))
                                .bg(if tapped { hex(0x2e5af5) } else { hex(0x2548c4) })
                                .child(sans(s, 13.0, 500.0, hex(0xeef1f7)).child("Allow")),
                        ),
                )
        });
        glass(s)
            .absolute()
            .left(s.px(806.0))
            .top(s.px(246.0))
            .w(s.px(420.0))
            .h(s.px(448.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(s.px(10.0))
                    .h(s.px(42.0))
                    .px(s.px(16.0))
                    .border_b_1()
                    .border_color(white(0.06))
                    .child(lights(s, true).mr(s.px(6.0)))
                    .child(sans(s, 13.0, 400.0, hex(0x8e97a8)).child("Settings"))
                    .child(sans(s, 13.0, 700.0, hex(0xeef1f7)).child("Remote"))
                    .child(div().flex_1())
                    .when(done && !notify, |this| {
                        this.child(tracked_text(s, "NOTIFICATIONS OFF", PLEX_MONO, 500.0, 8.5, 8.5, 0.12, hex(0x6c7688)))
                    })
                    .child(mono(s, 11.0, 400.0, hex(0x7f8aa0)).child("This computer")),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(s.px(12.0))
                    .py(s.px(18.0))
                    .px(s.px(20.0))
                    .child(sans(s, 17.0, 600.0, hex(0xeef1f7)).child("Connect your phone"))
                    .child(
                        sans(s, 13.0, 400.0, hex(0xaab2c1))
                            .line_height(s.px(13.0 * 1.45))
                            .child("Easy Connect is the simplest way to reach this computer from your phone."),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(divider())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(s.px(10.0))
                                    .pt(s.px(12.0))
                                    .child(sans(s, 14.0, 600.0, hex(0xeef1f7)).child("Easy Connect"))
                                    .child(
                                        sans(s, 10.0, 400.0, hex(0x8fabff))
                                            .h(s.px(20.0))
                                            .px(s.px(7.0))
                                            .flex()
                                            .items_center()
                                            .rounded(s.px(5.0))
                                            .border_1()
                                            .border_color(rgba(90, 130, 255, 0.35))
                                            .child("Recommended"),
                                    )
                                    .child(div().flex_1())
                                    .child(toggle(s, "pairing-toggle", toggled, thumb, ToggleSize::Sm, false)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(sans(s, 13.0, 400.0, hex(0xc9d0dc)).child("Remote access"))
                            .child(
                                mono(s, 13.0, 400.0, if allowed { hex(0x3be3a2) } else { hex(0x8e97a8) })
                                    .child(if allowed { "on" } else { "off" }),
                            ),
                    )
                    .child(
                        div()
                            .mt(s.px(4.0))
                            .flex()
                            .flex_col()
                            .child(divider())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(s.px(18.0))
                                    .pt(s.px(14.0))
                                    .child(qr)
                                    .child(
                                        sans(s, 13.0, 400.0, hex(0xc9d0dc))
                                            .line_height(s.px(13.0 * 1.45))
                                            .flex()
                                            .flex_col()
                                            .gap(s.px(5.0))
                                            .map(|this| {
                                                if paired {
                                                    this.child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(s.px(6.0))
                                                            .text_color(hex(0x3be3a2))
                                                            .font_weight(FontWeight(500.0))
                                                            .child(icon(s, "checkCircle", 16.0, 1.6, hex(0x3be3a2)))
                                                            .child("Paired with your phone"),
                                                    )
                                                } else if qr_shown {
                                                    this.child("Scan it in the Ghostex app.")
                                                } else {
                                                    this.child("Turn on Easy Connect to show the code.")
                                                }
                                            }),
                                    ),
                            ),
                    ),
            )
            .children(sheet)
            .into_any_element()
    }

    /// The tilted phone: the page's own rendering of each moment of the demo (images captured from
    /// the React phone), with the parts that move drawn on top where the page drew them.
    fn phone_mockup(
        &self,
        s: S,
        now: Instant,
        t: u64,
        notify: bool,
        at: &dyn Fn(u64) -> Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (clip_x, clip_y, clip_w, clip_h) = PHONE_CLIP;
        let layer = |name: &str, opacity: f32| {
            img(SharedString::from(format!("onboarding/phone/{name}.webp")))
                .absolute()
                .left(s.px(clip_x))
                .top(s.px(clip_y))
                .w(s.px(clip_w))
                .h(s.px(clip_h))
                .object_fit(ObjectFit::Fill)
                .opacity(opacity)
                .into_any_element()
        };
        // The phone's outer shadows (`0 30px 80px`, `0 0 50px`), under its untilted box.
        let mut out = vec![
            abs(s, 1316.0, 150.0, Some(292.0), Some(612.0))
                .rounded(s.px(46.0))
                .bg(hex(0x05070c))
                .shadow(vec![
                    shadow(black(0.7), 0.0, 30.0, 80.0, 0.0, s),
                    shadow(rgba(40, 80, 220, 0.16), 0.0, 0.0, 50.0, 0.0, s),
                ])
                .into_any_element(),
        ];
        // A point on the captured phone, in stage px.
        let point_at = |(x, y): (f32, f32)| (clip_x + x, clip_y + y);
        if t < END {
            // `.pd` fades in when the demo (re)starts; its later phases reuse the same element.
            let fade = progress(at(0), now, Duration::from_millis(300), Ease::Ease);
            let phase = match t {
                t if t < 900 => "get",
                t if t < 1600 => "loading",
                t if t < EASY => "open",
                t if t < SCAN => "connect",
                t if t < PAIRED => "cam",
                _ => "paired",
            };
            out.push(layer("shell", 1.0));
            out.push(layer(phase, fade));
            // `.pd-btn.tap`: `transition: transform 0.15s, filter 0.15s`.
            if phase == "connect" && t >= QR + 400 {
                let tap = progress(at(QR + 400), now, Duration::from_millis(150), Ease::Ease);
                out.push(layer("connect-tap", tap * fade));
            }
            if phase == "loading" {
                let (x, y) = point_at(PHONE_SPINNER);
                out.push(
                    abs(s, x - 9.0, y - 9.0, Some(18.0), Some(18.0))
                        .child(spinner(s, 18.0, 2.0, at(900), now))
                        .into_any_element(),
                );
            }
            if phase == "cam" {
                out.push(self.phone_scanline(s, now, at(SCAN)));
            }
            if phase == "paired" {
                // `.pd-ok`: `gxob-pop 0.45s cubic-bezier(0.3, 1.6, 0.5, 1)` from `scale(0)`.
                let pop = progress(
                    at(PAIRED),
                    now,
                    Duration::from_millis(450),
                    Ease::Bezier(0.3, 1.6, 0.5, 1.0),
                );
                let size = 64.0 * pop.max(0.0);
                let (x, y) = point_at(PHONE_OK);
                out.push(
                    abs(s, x - size / 2.0, y - size / 2.0, Some(size), Some(size))
                        .rounded_full()
                        .bg(hex(0x16b877))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(s, "check", 30.0 * pop.max(0.0), 2.4, gpui::white()))
                        .into_any_element(),
                );
            }
            return out;
        }
        if notify {
            // `.ph-banner`: `gxob-drop 0.3s` in.
            let drop = progress(
                at(END),
                now,
                Duration::from_millis(300),
                Ease::Bezier(0.2, 0.8, 0.2, 1.0),
            );
            out.push(layer("home-notify-nobanner", 1.0));
            out.push(layer("home-notify", drop));
        } else {
            out.push(layer("home-quiet", 1.0));
        }
        // The breathing status dots (`gxob-breathe`: 1.8s running, 0.9s working).
        for ((x, y), period, color) in PHONE_PILL_DOTS {
            let (x, y) = point_at((x, y));
            out.push(
                abs(s, x - 3.0, y - 3.0, Some(6.0), Some(6.0))
                    .rounded_full()
                    .bg(hex(color))
                    .opacity(breathe(at(END), now, period, 0.0))
                    .into_any_element(),
            );
        }
        let (x, y) = point_at(PHONE_CONNECTED_DOT);
        out.push(
            abs(s, x - 3.0, y - 3.0, Some(6.0), Some(6.0))
                .rounded_full()
                .bg(hex(0x3be3a2))
                .into_any_element(),
        );
        // `.ph-row:hover { background: rgba(255, 255, 255, 0.04) }`, no transition.
        for (index, (x, y, w, h)) in PHONE_ROWS.into_iter().enumerate() {
            let (x, y) = point_at((x, y));
            let key = format!("phone-row-{index}");
            let hovered = interact::hovered(&key);
            out.push(
                interact::watch_hover(
                    abs(s, x, y, Some(w), Some(h))
                        .id(SharedString::from(key.clone()))
                        .rounded(s.px(9.0))
                        .when(hovered, |this| this.bg(white(0.04))),
                    key,
                )
                .into_any_element(),
            );
        }
        // `.ph-card` is the phone's one button.
        let (x, y, w, h) = PHONE_CARD;
        let (x, y) = point_at((x, y));
        out.push(
            self.control(
                s,
                abs(s, x, y, Some(w), Some(h))
                    .id("phone-card")
                    .cursor_pointer(),
                "phone-card",
                interact::Ring::new(12.0, 0.0),
                interact::Keys::EnterSpace,
                cx,
                |this, _, cx| this.show_toast("Your computer · 18 ms · 4 sessions", cx),
            )
            .into_any_element(),
        );
        out
    }

    /// `.pd-scanline`: a 2px #6d93ff bar with a 12px glow, `gxob-scanline 1.1s ease-in-out infinite
    /// alternate` down the tilted camera frame.
    fn phone_scanline(&self, s: S, now: Instant, mounted: Instant) -> AnyElement {
        let cycles = now.saturating_duration_since(mounted).as_secs_f32() / 1.1;
        let phase = cycles.fract();
        let forward = (cycles as u32).is_multiple_of(2);
        let f = Ease::EaseInOut.apply(if forward { phase } else { 1.0 - phase });
        let lerp = |a: (f32, f32), b: (f32, f32)| (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
        let left = lerp(PHONE_SCAN_TOP.0, PHONE_SCAN_BOTTOM.0);
        let right = lerp(PHONE_SCAN_TOP.1, PHONE_SCAN_BOTTOM.1);
        let (clip_x, clip_y, clip_w, clip_h) = PHONE_CLIP;
        abs(s, clip_x, clip_y, Some(clip_w), Some(clip_h))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let to_px = |(x, y): (f32, f32)| {
                            gpui::point(bounds.origin.x + s.px(x), bounds.origin.y + s.px(y))
                        };
                        // The glow as widening, fading bands, then the bar itself.
                        for (half, alpha) in [(7.0, 0.07), (4.5, 0.14), (2.5, 0.3), (1.0, 1.0)] {
                            let mut path = gpui::Path::new(to_px((left.0, left.1 - half)));
                            path.line_to(to_px((right.0, right.1 - half)));
                            path.line_to(to_px((right.0, right.1 + half)));
                            path.line_to(to_px((left.0, left.1 + half)));
                            window.paint_path(path, rgba(109, 147, 255, alpha));
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}

/// Where the captured phone images sit on the stage (stage px): the tilted phone's bounds.
const PHONE_CLIP: (f32, f32, f32, f32) = (1308.0, 149.0, 309.0, 624.0);
/// Points on the captured phone (px from its top-left), measured on the React page.
const PHONE_SPINNER: (f32, f32) = (152.1, 398.8);
const PHONE_OK: (f32, f32) = (154.3, 297.4);
const PHONE_SCAN_TOP: ((f32, f32), (f32, f32)) = ((82.4, 251.1), (219.6, 243.4));
const PHONE_SCAN_BOTTOM: ((f32, f32), (f32, f32)) = ((88.6, 382.7), (226.3, 376.3));
const PHONE_PILL_DOTS: [((f32, f32), f32, u32); 4] = [
    ((203.4, 248.8), 1.8, 0x3be3a2),
    ((201.9, 286.8), 0.9, 0x6d93ff),
    ((201.9, 324.9), 1.8, 0x3be3a2),
    ((194.6, 362.9), 1.8, 0x3be3a2),
];
const PHONE_CONNECTED_DOT: (f32, f32) = (89.7, 174.5);
const PHONE_ROWS: [(f32, f32, f32, f32); 4] = [
    (28.9, 227.7, 255.0, 41.2),
    (28.0, 265.2, 255.2, 42.0),
    (27.1, 302.8, 255.5, 42.8),
    (26.2, 340.4, 255.7, 43.6),
];
const PHONE_CARD: (f32, f32, f32, f32) = (32.6, 126.4, 251.2, 67.1);
