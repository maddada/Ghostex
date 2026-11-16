//! Panel 4, Your devices: what can reach this computer (your phone, your other computers, Cloud
//! Boxes), how they connect (Easy Connect, Tailscale), and the two switches that start pairing a
//! phone or adding a computer after setup (panels/mobile.tsx (deleted 2026-10-01), styles/mobile.css).
//!
//! CDXC:Onboarding 2026-11-13 DECISION:
//! User, on the first-run showcase page (Part 3: Q3052 to Q3055), picked "One page, as drawn",
//! "Easy Connect as the default, Tailscale beside it", "Cloud Boxes: one row with a line of
//! explanation" and "Two switches: pair my phone, add another computer, both off unless picked".
//! The left side names the three kinds of device and holds the two switches; the right side shows
//! this computer with what connects to it and the two ways they meet, Easy Connect lit as the
//! default. This replaces the looping phone pairing demo (CDXC:Onboarding 2026-09-28 WHY, whose
//! captured phone pictures stay in assets/onboarding/phone/ unused) and the single phone switch of
//! the 2026-11-11 DECISION below.
use super::GpuiOnboardingWindow;
use super::interact;
use super::primitives::*;
use super::stage::*;
use super::welcome::foot_actions;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, canvas, div, linear_color_stop, linear_gradient,
};
use std::time::Instant;

pub(crate) struct MobileState {
    /// When the page mounted: the diagram's links flow from here.
    started: Instant,
}

impl MobileState {
    pub(crate) fn new(now: Instant) -> Self {
        Self { started: now }
    }
}

/// The diagram's box on the stage.
const DIAGRAM: (f32, f32, f32, f32) = (820.0, 96.0, 790.0, 330.0);
/// Its links, in the box's own px, from this computer's edge to each card's edge (the cards are
/// translucent, so a link must not run under them): two curves up to the phone and the other
/// computer, one line down to the Cloud Box.
const LINKS: [[(f32, f32); 4]; 3] = [
    [
        (285.0, 178.0),
        (255.0, 178.0),
        (250.0, 108.0),
        (220.0, 108.0),
    ],
    [
        (505.0, 178.0),
        (535.0, 178.0),
        (540.0, 108.0),
        (570.0, 108.0),
    ],
    [
        (395.0, 216.0),
        (395.0, 231.0),
        (395.0, 247.0),
        (395.0, 262.0),
    ],
];

/// What can reach this computer: icon, name, what it is.
const DEVICES: [(&str, &str, &str); 3] = [
    (
        "phone",
        "Your phone",
        "Scan one code with the Ghostex app for iPhone or Android. Reply, approve and get an alert when an agent needs you.",
    ),
    (
        "laptop",
        "Your other computers",
        "Add a Mac, Windows or Linux machine over SSH, or one that already runs Ghostex. Its projects join this sidebar.",
    ),
    (
        "cloud",
        "Cloud Boxes",
        "Start a session on a machine in the cloud that keeps working while this computer sleeps.",
    ),
];

impl GpuiOnboardingWindow {
    /// CDXC:Onboarding 2026-09-15 DECISION:
    /// User: "in the take ghostex with you section, we need to not download the app. We should just have a toggle saying 'Configure my phone with ghostex after this flow' and all that does is it opens the sidebar menu -> remote screen for the user after the flow". There are no install or pair steps and no APK link; the toggle sets `phone_queued`, which opens the Connect a Device window once the flow closes.
    fn toggle_phone(&mut self, cx: &mut Context<Self>) {
        self.flow.phone_queued = !self.flow.phone_queued;
        cx.notify();
    }

    /// "Add another computer after setup": Settings > Remote opens on its Add a computer dialog.
    fn toggle_computer(&mut self, cx: &mut Context<Self>) {
        self.flow.computer_queued = !self.flow.computer_queued;
        cx.notify();
    }

    pub(super) fn render_mobile(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        // CDXC:Onboarding 2026-11-11 DECISION:
        // User, on the first-run page (step 9, Mobile), picked "Only the phone question" and wrote "make this page about explaining how ghostex can connect to other devices and can be controlled by your phone by just scanning a qr". The panel explains Connect a Device (the phone app for iPhone and Android, other computers) and asks one thing, whether to open Connect a Device after setup. Agent alerts stay on by default (Settings > Notifications), and Floating Capture leaves setup for Settings, superseding CDXC:GhostexCapture 2026-09-30's "add Floating Capture to the setup toggles": its first capture is what asks for Screen Recording, so it waits until someone turns it on there.
        // 2026-11-13: the second switch (another computer) joins it, per the DECISION at the top.
        let mut out = Vec::new();
        out.push(eyebrow(s, 46.0, 112.0, None, "Optional · Your devices").into_any_element());
        out.push(
            heading(
                s,
                46.0,
                138.0,
                700.0,
                52.0,
                "Your agents, on every device.",
                None,
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 46.0, 214.0, 670.0, 16.0, false)
                .child("Sessions run on the computer that holds them. Your phone and your other computers connect to it and see the same sidebar.")
                .into_any_element(),
        );
        // Alerts stay on without a switch here, so a computer that cannot show them says why
        // under the phone (Linux without notify-send, Windows with them turned off).
        let alert_problem = self.notification_problem.clone();
        out.push(
            abs(s, 46.0, 296.0, Some(670.0), None)
                .flex()
                .flex_col()
                .gap(s.px(20.0))
                .children(DEVICES.into_iter().map(|(icon_name, title, detail)| {
                    let problem = alert_problem.clone().filter(|_| icon_name == "phone");
                    div()
                        .flex()
                        .items_center()
                        .gap(s.px(20.0))
                        .child(ibox(s, 44.0, 11.0).child(icon(
                            s,
                            icon_name,
                            20.0,
                            1.6,
                            hex(0xdfe4ee),
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(nm(s, 17.0, 500.0).child(title))
                                .child(ss(s, 13.0).mt(s.px(4.0)).child(detail))
                                .children(problem.map(|problem| {
                                    ss(s, 13.0)
                                        .mt(s.px(2.0))
                                        .text_color(hex(0xffc46b))
                                        .child(problem)
                                })),
                        )
                }))
                .into_any_element(),
        );
        let phone_queued = self.flow.phone_queued;
        let computer_queued = self.flow.computer_queued;
        let phone_row = self.after_setup_row(
            s,
            now,
            "mobile-phone",
            "phone",
            "Pair my phone after setup",
            "Opens Connect a Phone after your first agent reply.",
            phone_queued,
            false,
            cx,
            |this, cx| this.toggle_phone(cx),
        );
        let computer_row = self.after_setup_row(
            s,
            now,
            "mobile-computer",
            "laptop",
            "Add another computer after setup",
            "Opens Add a computer after your first reply: SSH or an Easy Connect code.",
            computer_queued,
            true,
            cx,
            |this, cx| this.toggle_computer(cx),
        );
        out.push(
            glass(s)
                .absolute()
                .left(s.px(46.0))
                .top(s.px(578.0))
                .w(s.px(670.0))
                .h(s.px(150.0))
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(phone_row)
                .child(computer_row)
                .into_any_element(),
        );
        out.push(
            abs(s, 46.0, 756.0, None, None)
                .flex()
                .child(sans(s, 15.5, 400.0, hex(0xaeb6c4)).child("Anytime later: "))
                .child(sans(s, 15.5, 600.0, hex(0xeef1f7)).child("Ghostex menu → Connect a Phone"))
                .child(sans(s, 15.5, 400.0, hex(0xaeb6c4)).child(", or "))
                .child(sans(s, 15.5, 600.0, hex(0xeef1f7)).child("Settings → Remote"))
                .child(sans(s, 15.5, 400.0, hex(0xaeb6c4)).child("."))
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
        out.push(self.render_device_diagram(s, now));
        out.extend(render_connect_ways(s));
        out
    }

    /// One switch row inside the after-setup card; the whole row toggles it.
    #[allow(clippy::too_many_arguments)]
    fn after_setup_row(
        &mut self,
        s: S,
        now: Instant,
        id: &'static str,
        icon_name: &'static str,
        title: &'static str,
        detail: &'static str,
        on: bool,
        divided: bool,
        cx: &mut Context<Self>,
        flip: impl Fn(&mut Self, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let row_key = format!("{id}-row");
        let toggle_key = format!("{id}-toggle");
        let thumb = self.thumb(id, on, now);
        let toggle_flip = flip.clone();
        let switch = self.control(
            s,
            toggle(s, toggle_key.clone(), on, thumb, ToggleSize::Lg, false),
            toggle_key,
            interact::Ring::new(17.0, 1.0),
            interact::Keys::EnterSpace,
            cx,
            move |this, _, cx| {
                cx.stop_propagation();
                toggle_flip(this, cx);
            },
        );
        let row = div()
            .id(SharedString::from(row_key.clone()))
            .relative()
            .flex()
            .items_center()
            .gap(s.px(20.0))
            .h(s.px(75.0))
            .pl(s.px(26.0))
            .pr(s.px(26.0))
            .cursor_pointer()
            .when(divided, |this| this.border_t_1().border_color(white(0.06)))
            .child(icon(s, icon_name, 22.0, 1.6, hex(0xd6dbe5)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(nm(s, 17.0, 500.0).child(title))
                    .child(ss(s, 12.5).mt(s.px(2.0)).child(detail)),
            )
            .child(switch);
        self.control(
            s,
            row,
            row_key,
            interact::Ring::new(0.0, 0.0),
            interact::Keys::EnterSpace,
            cx,
            move |this, _, cx| flip(this, cx),
        )
        .into_any_element()
    }

    /// This computer in the middle, with your phone, another computer and a Cloud Box joined to
    /// it by links whose dashes flow toward it.
    fn render_device_diagram(&self, s: S, now: Instant) -> AnyElement {
        let (left, top, width, height) = DIAGRAM;
        let flow = now
            .saturating_duration_since(self.mobile.started)
            .as_secs_f32()
            * 14.0;
        let node = |x: f32, y: f32, w: f32, h: f32, icon_name: &str, name: &str, via: &str| {
            glass(s)
                .absolute()
                .left(s.px(x))
                .top(s.px(y))
                .w(s.px(w))
                .h(s.px(h))
                .flex()
                .items_center()
                .gap(s.px(14.0))
                .px(s.px(16.0))
                .child(ibox(s, 40.0, 10.0).child(icon(s, icon_name, 19.0, 1.6, hex(0xdfe4ee))))
                .child(
                    div()
                        .min_w_0()
                        .child(nm(s, 16.0, 500.0).child(name.to_string()))
                        .child(ss(s, 12.5).child(via.to_string())),
                )
        };
        abs(s, left, top, Some(width), Some(height))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        for link in LINKS {
                            paint_dashed_curve(window, bounds.origin, s, link, flow);
                        }
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .size_full(),
            )
            .child(node(
                20.0,
                70.0,
                200.0,
                76.0,
                "phone",
                "Your phone",
                "Easy Connect",
            ))
            .child(node(
                570.0,
                70.0,
                200.0,
                76.0,
                "laptop",
                "Work laptop",
                "SSH",
            ))
            .child(node(
                295.0,
                262.0,
                200.0,
                68.0,
                "cloud",
                "Cloud Box",
                "Instabox · Boat",
            ))
            .child(
                lit_card(node(
                    285.0,
                    140.0,
                    220.0,
                    76.0,
                    "monitor",
                    "This computer",
                    "Holds your sessions",
                ))
                .shadow(vec![shadow(
                    rgba(60, 100, 255, 0.22),
                    0.0,
                    0.0,
                    30.0,
                    0.0,
                    s,
                )]),
            )
            .into_any_element()
    }
}

/// The selected look the page uses for what is on by default: a blue wash and border.
fn lit_card(card: Div) -> Div {
    card.bg(linear_gradient(
        90.0,
        linear_color_stop(rgba(60, 90, 200, 0.26), 0.0),
        linear_color_stop(rgba(60, 90, 200, 0.1), 1.0),
    ))
    .border_color(rgba(110, 140, 240, 0.45))
}

/// "How they connect": Easy Connect, lit as the default, and Tailscale beside it.
fn render_connect_ways(s: S) -> Vec<AnyElement> {
    let way = |icon_name: &str,
               name: &str,
               pill: &str,
               lead: &str,
               points: [(bool, &str); 3],
               lit: bool| {
        let card = glass(s)
            .flex_1()
            .min_w_0()
            .h(s.px(232.0))
            .p(s.px(22.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s.px(12.0))
                    .child(icon(s, icon_name, 22.0, 1.6, hex(0xdfe4ee)))
                    .child(sans(s, 18.0, 600.0, hex(0xeef1f7)).child(name.to_string()))
                    .child(div().flex_1())
                    .child(
                        sans(
                            s,
                            11.5,
                            500.0,
                            if lit { hex(0x9db4ff) } else { hex(0xaeb6c4) },
                        )
                        .h(s.px(24.0))
                        .px(s.px(9.0))
                        .flex()
                        .items_center()
                        .rounded(s.px(7.0))
                        .border_1()
                        .border_color(if lit {
                            rgba(110, 150, 255, 0.4)
                        } else {
                            white(0.12)
                        })
                        .child(pill.to_string()),
                    ),
            )
            .child(
                sans(s, 14.5, 400.0, hex(0xc9d0dc))
                    .mt(s.px(14.0))
                    .line_height(s.px(14.5 * 1.5))
                    .child(lead.to_string()),
            )
            .child(
                div()
                    .mt(s.px(18.0))
                    .flex()
                    .flex_col()
                    .gap(s.px(10.0))
                    .children(points.into_iter().map(|(good, text)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(s.px(9.0))
                            .child(if good {
                                icon(s, "check", 15.0, 2.0, hex(0x3be3a2))
                            } else {
                                icon(s, "plus", 15.0, 1.6, hex(0x8b93a6))
                            })
                            .child(
                                sans(
                                    s,
                                    13.5,
                                    400.0,
                                    if good { hex(0xc9d0dc) } else { hex(0x9ea7b6) },
                                )
                                .child(text.to_string()),
                            )
                    })),
            );
        if lit { lit_card(card) } else { card }
    };
    vec![
        abs(s, 830.0, 470.0, None, None)
            .child(label(s, "How they connect"))
            .into_any_element(),
        abs(s, 830.0, 500.0, Some(770.0), None)
            .flex()
            .gap(s.px(18.0))
            .child(way(
                "qr",
                "Easy Connect",
                "Recommended",
                "One code, nothing else to install. Works from any network.",
                [
                    (true, "Phones and computers"),
                    (true, "Behind any router"),
                    (false, "Sets up a small helper here once"),
                ],
                true,
            ))
            .child(way(
                "globe",
                "Tailscale",
                "Already on it?",
                "Your devices reach this computer inside your own tailnet.",
                [
                    (true, "Your private network"),
                    (true, "No code to expire"),
                    (false, "Tailscale app on every device"),
                ],
                false,
            ))
            .into_any_element(),
    ]
}

/// A cubic link drawn as 6px dashes with 6px gaps, `flow` px along so the dashes travel toward
/// this computer.
fn paint_dashed_curve(
    window: &mut gpui::Window,
    origin: gpui::Point<gpui::Pixels>,
    s: S,
    [p0, p1, p2, p3]: [(f32, f32); 4],
    flow: f32,
) {
    const STEPS: usize = 160;
    const DASH: f32 = 6.0;
    let at = |t: f32| {
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        (
            a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
        )
    };
    let to_px = |(x, y): (f32, f32)| gpui::point(origin.x + s.px(x), origin.y + s.px(y));
    let color = rgba(109, 147, 255, 0.55);
    let mut travelled = flow % (DASH * 2.0);
    let mut previous = at(0.0);
    for step in 1..=STEPS {
        let point = at(step as f32 / STEPS as f32);
        let (dx, dy) = (point.0 - previous.0, point.1 - previous.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length > 0.0 && travelled % (DASH * 2.0) < DASH {
            let (nx, ny) = (-dy / length, dx / length);
            let mut path = gpui::Path::new(to_px((previous.0 + nx, previous.1 + ny)));
            path.line_to(to_px((point.0 + nx, point.1 + ny)));
            path.line_to(to_px((point.0 - nx, point.1 - ny)));
            path.line_to(to_px((previous.0 - nx, previous.1 - ny)));
            window.paint_path(path, color);
        }
        travelled += length;
        previous = point;
    }
}
