//! Panel 1, Welcome: the three feature tabs and their live demos (panels/welcome.tsx (deleted 2026-10-01),
//! previews/welcome-demos.tsx (deleted 2026-10-01), styles/welcome.css).
use super::GpuiOnboardingWindow;
use super::fonts::PLEX_MONO;
use super::interact;
use super::primitives::*;
use super::stage::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Styled as _, canvas, div, linear_color_stop, linear_gradient, point, px,
    size,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WelcomeTab {
    Together,
    Chat,
    Mobile,
}

pub(crate) struct WelcomeState {
    pub(crate) tab: WelcomeTab,
    /// When the current demo mounted (its cycle clock).
    pub(crate) tab_since: Instant,
}

impl WelcomeState {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            tab: WelcomeTab::Together,
            tab_since: now,
        }
    }
}

const TABS: [(WelcomeTab, &str, &str, &str); 3] = [
    (
        WelcomeTab::Together,
        "org",
        "Agents that work together",
        "One agent can launch and hand work to another.",
    ),
    (
        WelcomeTab::Chat,
        "chat",
        "Chat + terminal",
        "Read it as a chat, the real CLI runs underneath.",
    ),
    (
        WelcomeTab::Mobile,
        "phone",
        "Desktop + mobile",
        "Always-on sessions, pick up anywhere.",
    ),
];

/// `<FootActions panel>`: the panel's forward action on the footer row, right-aligned to the copy column.
/// CDXC:Onboarding 2026-09-12 DECISION:
/// User: "please make there only 1 next button on each page, i dont like how we have a cta then a next button". Each panel's single forward action (and an optional ghost secondary before it) sits in this slot.
/// CDXC:Onboarding 2026-09-15 DECISION:
/// User: "please move the Next button to be bottom right of the left half of the modal for all pages". The slot is on the footer row at the copy column's right edge, opposite the Back button.
pub(crate) fn foot_actions(s: S, panel: usize) -> Div {
    div()
        .absolute()
        .right(s.px(STAGE_WIDTH - foot_right_x(panel)))
        .top(s.px(FOOT_TOP))
        .h(s.px(FOOT_HEIGHT))
        .flex()
        .items_center()
        .gap(s.px(24.0))
}

/// `<DemoLink>`: a horizontal link between two cards with a travelling packet and a label.
/// `send` is the packet's direction and when it left; `label` the text and when it last changed.
pub(crate) fn demo_link(
    s: S,
    x: f32,
    y: f32,
    width: f32,
    send: Option<(bool, Instant)>,
    label: Option<(&str, Instant)>,
    now: Instant,
) -> Vec<AnyElement> {
    let end = |left: bool| {
        div()
            .absolute()
            .top(s.px(-3.5))
            .when(left, |this| this.left(s.px(-4.0)))
            .when(!left, |this| this.right(s.px(-4.0)))
            .size(s.px(9.0))
            .rounded_full()
            .bg(hex(0x5b85ff))
            .shadow(vec![shadow(hex(0x4a78ff), 0.0, 0.0, 6.0, 0.0, s)])
    };
    let mut link = abs(s, x, y - 1.0, Some(width), Some(2.0))
        .bg(rgba(70, 110, 240, 0.5))
        .shadow(vec![shadow(
            rgba(60, 100, 255, 0.35),
            0.0,
            0.0,
            8.0,
            0.0,
            s,
        )])
        .child(end(true))
        .child(end(false));
    if let Some((right, started)) = send {
        let t = progress(
            started,
            now,
            Duration::from_millis(1000),
            Ease::Bezier(0.5, 0.0, 0.3, 1.0),
        );
        let travel = width - 24.0;
        let left = if right {
            travel * t
        } else {
            travel * (1.0 - t)
        };
        link = link.child(
            div()
                .absolute()
                .top(s.px(-5.0))
                .left(s.px(left))
                .w(s.px(24.0))
                .h(s.px(12.0))
                .rounded(s.px(6.0))
                .bg(hex(0xcddaff))
                .shadow(vec![shadow(hex(0x7a9dff), 0.0, 0.0, 14.0, 0.0, s)])
                .opacity(1.0 - 0.85 * t),
        );
    }
    let mut out = vec![link.into_any_element()];
    if let Some((text, changed)) = label {
        let fade = progress(changed, now, Duration::from_millis(300), Ease::Ease);
        out.push(
            abs(
                s,
                x + width / 2.0 - 300.0,
                y + 1.0 - 12.0 - 100.0,
                Some(600.0),
                Some(100.0),
            )
            .flex()
            .flex_col()
            .justify_end()
            .items_center()
            .opacity(fade)
            .child(
                sans(s, 12.0, 400.0, hex(0xdfe4ee))
                    .py(s.px(5.0))
                    .px(s.px(10.0))
                    .rounded(s.px(7.0))
                    .bg(rgba(14, 18, 30, 0.95))
                    .border_1()
                    .border_color(rgba(100, 140, 255, 0.35))
                    .whitespace_nowrap()
                    .child(text.to_string()),
            )
            .into_any_element(),
        );
    }
    out
}

/// The moment a frame-derived value last changed, walking back from the current frame.
fn changed_at<T: PartialEq>(
    cycle: &Cycle,
    interval_ms: u64,
    value: impl Fn(usize) -> T,
) -> Instant {
    let current = value(cycle.frame);
    let mut frame = cycle.frame;
    while frame > 0 && value(frame - 1) == current {
        frame -= 1;
    }
    cycle.frame_at(frame, interval_ms)
}

type LogLine = (usize, &'static str, &'static str);

const LEAD_LOG: [LogLine; 4] = [
    (0, "me", "Refactor the auth flow."),
    (1, "", "Edited src/auth/refresh.ts"),
    (2, "acc", "Handing the tests to Codex"),
    (6, "ok", "Codex's tests pass. Ready to merge."),
];
const SUB_LOG: [LogLine; 3] = [
    (3, "", "Got it: tests for refresh.ts"),
    (4, "", "Wrote 6 tests"),
    (5, "ok", "6 of 6 pass"),
];

/// A transcript row: what the terminal shows, and the chat bubble it becomes when there is one.
type TranscriptLine = (
    (&'static str, &'static str),
    Option<(&'static str, &'static str)>,
);

const TRANSCRIPT: [TranscriptLine; 6] = [
    (("cmd", "$ claude"), None),
    (
        ("me", "> Fix the flaky refresh test"),
        Some(("me", "Fix the flaky refresh test")),
    ),
    (
        ("tool", "● Read src/auth/refresh.ts"),
        Some(("tool", "Read refresh.ts")),
    ),
    (
        ("tool", "● Update src/auth/refresh.ts  +12 −4"),
        Some(("tool", "Edited refresh.ts · +12 −4")),
    ),
    (
        ("tool", "● Bash bun test  17 passed"),
        Some(("tool", "Ran the tests · 17 passed")),
    ),
    (
        ("out", "Fixed: the refresh now waits for the lock."),
        Some((
            "bot",
            "Fixed. The refresh now waits for the lock, and all 17 tests pass.",
        )),
    ),
];

const MONITOR_SESSIONS: [(&str, &str); 3] = [
    ("claude", "refactor-auth"),
    ("codex", "port-legacy-tests"),
    ("cursor", "tokens-to-css-vars"),
];

/// `wirePath`: a vertical-horizontal-vertical connector with rounded corners, as polyline points.
fn wire_points(x1: f32, y1: f32, x2: f32, y2: f32) -> Vec<(f32, f32)> {
    let mid_y = (y1 + y2) / 2.0;
    let dx = (x2 - x1).signum();
    let dy = (y2 - y1).signum();
    let r = 16.0f32.min((x2 - x1).abs() / 2.0).min((mid_y - y1).abs());
    let mut points = vec![(x1, y1), (x1, mid_y - dy * r)];
    let quad = |p0: (f32, f32), c: (f32, f32), p1: (f32, f32), points: &mut Vec<(f32, f32)>| {
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            let mt = 1.0 - t;
            points.push((
                mt * mt * p0.0 + 2.0 * mt * t * c.0 + t * t * p1.0,
                mt * mt * p0.1 + 2.0 * mt * t * c.1 + t * t * p1.1,
            ));
        }
    };
    quad(
        (x1, mid_y - dy * r),
        (x1, mid_y),
        (x1 + dx * r, mid_y),
        &mut points,
    );
    points.push((x2 - dx * r, mid_y));
    quad(
        (x2 - dx * r, mid_y),
        (x2, mid_y),
        (x2, mid_y + dy * r),
        &mut points,
    );
    points.push((x2, y2));
    points
}

/// A point `t` (0..1) of the way along a polyline, at constant speed (SVG `animateMotion` is paced).
fn along(points: &[(f32, f32)], t: f32) -> (f32, f32) {
    let lengths: Vec<f32> = points
        .windows(2)
        .map(|pair| ((pair[1].0 - pair[0].0).powi(2) + (pair[1].1 - pair[0].1).powi(2)).sqrt())
        .collect();
    let total: f32 = lengths.iter().sum();
    let mut remaining = t.clamp(0.0, 1.0) * total;
    for (index, length) in lengths.iter().enumerate() {
        if remaining <= *length && *length > 0.0 {
            let k = remaining / length;
            let (a, b) = (points[index], points[index + 1]);
            return (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k);
        }
        remaining -= length;
    }
    *points.last().unwrap_or(&(0.0, 0.0))
}

impl GpuiOnboardingWindow {
    pub(super) fn render_welcome(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        out.push(eyebrow(s, 46.0, 114.0, None, "Welcome to Ghostex").into_any_element());
        out.push(
            heading(
                s,
                46.0,
                139.0,
                700.0,
                50.0,
                "Your coding agents.",
                Some("One serious workspace."),
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 46.0, 264.0, 640.0, 15.0, false)
                .child("Use your own subscriptions or API keys with 20+ supported agents, Ghostex is the workspace around them.")
                .into_any_element(),
        );
        let tab = self.welcome.tab;
        out.push(
            glass(s)
                .absolute()
                .left(s.px(46.0))
                .top(s.px(348.0))
                .w(s.px(676.0))
                .h(s.px(312.0))
                .py(s.px(8.0))
                .px(s.px(14.0))
                .flex()
                .flex_col()
                .justify_center()
                .children(TABS.iter().map(|(id, icon_name, title, detail)| {
                    let selected = *id == tab;
                    let id = *id;
                    let key = format!("welcome-tab-{title}");
                    // `.tabrow.hl` (hovered) and `.tabrow.sel`: `transition: background 0.2s,
                    // box-shadow 0.2s`; the selected gradient is an image and swaps at once.
                    let hl = interact::hovered(&key);
                    let flat = interact::tween_color(
                        &key,
                        "bg",
                        if hl && !selected {
                            rgba(80, 110, 220, 0.09)
                        } else {
                            rgba(80, 110, 220, 0.0)
                        },
                        200,
                    );
                    let ring = interact::tween_color(
                        &key,
                        "shadow",
                        rgba(110, 150, 255, if selected { 0.35 } else { 0.0 }),
                        200,
                    );
                    let chevron =
                        interact::hover_color(&key, "chevron", hex(0x4a5263), hex(0x6d93ff), 200);
                    let nudge =
                        interact::tween_value(&key, "nudge", if hl { 3.0 } else { 0.0 }, 200);
                    let row = div()
                        .id(gpui::SharedString::from(key.clone()))
                        .relative()
                        .flex()
                        .items_center()
                        .gap(s.px(20.0))
                        .h(s.px(96.0))
                        .px(s.px(14.0))
                        .rounded(s.px(11.0))
                        .cursor_pointer()
                        .map(|this| {
                            if selected {
                                this.bg(linear_gradient(
                                    90.0,
                                    linear_color_stop(rgba(60, 90, 200, 0.2), 0.0),
                                    linear_color_stop(rgba(60, 90, 200, 0.03), 1.0),
                                ))
                            } else {
                                this.bg(flat)
                            }
                        })
                        .shadow(vec![inset_shadow(ring, 0.0, 0.0, 0.0, 1.0, s)])
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
                                .child(nm(s, 18.0, 500.0).child(title.to_string()))
                                .child(ss(s, 14.0).mt(s.px(7.0)).child(detail.to_string())),
                        )
                        .child(
                            div()
                                .relative()
                                .left(s.px(nudge))
                                .child(icon(s, "chevR", 16.0, 1.6, chevron)),
                        );
                    self.control(
                        s,
                        row,
                        key,
                        interact::Ring::new(11.0, 0.0),
                        interact::Keys::EnterSpace,
                        cx,
                        move |this, _, cx| {
                            if this.welcome.tab != id {
                                this.welcome.tab = id;
                                this.welcome.tab_since = std::time::Instant::now();
                            }
                            cx.notify();
                        },
                    )
                    .into_any_element()
                }))
                .into_any_element(),
        );
        // CDXC:Onboarding 2026-09-15 DECISION: User: "remove the I already know ghostex button"; Next is the only action.
        out.push(
            foot_actions(s, 1)
                .child(self.control(
                    s,
                    cta(s, "welcome-next", "Next", true, true, false, CtaSize::Foot),
                    "welcome-next",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.go(2, cx),
                ))
                .into_any_element(),
        );
        let mounted = self.welcome.tab_since;
        match tab {
            WelcomeTab::Together => out.extend(self.agents_together_demo(s, mounted, now)),
            WelcomeTab::Chat => out.extend(self.chat_terminal_demo(s, mounted, now)),
            WelcomeTab::Mobile => out.extend(self.desktop_mobile_demo(s, mounted, now)),
        }
        out
    }

    fn demo_card(
        &self,
        s: S,
        (left, top, width, height): (f32, f32, f32, f32),
        (agent_id, name, role): (&str, &str, String),
        status: (PillKind, &str),
        log: &[LogLine],
        cycle: &Cycle,
        lit: bool,
        mounted: Instant,
        now: Instant,
    ) -> AnyElement {
        let fade = progress(mounted, now, Duration::from_millis(350), Ease::Ease);
        glass_card(s)
            .absolute()
            .left(s.px(left))
            .top(s.px(top))
            .w(s.px(width))
            .h(s.px(height))
            .flex()
            .flex_col()
            .opacity(fade)
            .when(lit, |this| {
                this.border_color(rgba(125, 162, 255, 0.75)).shadow(vec![
                    inset_shadow(white(0.07), 0.0, 1.0, 0.0, 0.0, s),
                    shadow(rgba(60, 100, 255, 0.25), 0.0, 0.0, 40.0, 0.0, s),
                ])
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .pt(s.px(18.0))
                    .px(s.px(18.0))
                    .pb(s.px(16.0))
                    .border_b_1()
                    .border_color(white(0.06))
                    .child(abox(s, 40.0, 10.0, 12.0).child(agent_logo(
                        s,
                        &self.catalog,
                        agent_id,
                        24.0,
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(sans(s, 16.0, 600.0, hex(0xeef1f7)).child(name.to_string()))
                            .child(
                                sans(s, 12.0, 400.0, hex(0x8e97a8))
                                    .mt(s.px(2.0))
                                    .whitespace_nowrap()
                                    .child(role),
                            ),
                    )
                    .child(status_pill(
                        s,
                        status.0,
                        status.1,
                        self.opened_at,
                        now,
                        PILL_DEFAULT,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .py(s.px(16.0))
                    .px(s.px(18.0))
                    .flex()
                    .flex_col()
                    .gap(s.px(10.0))
                    .children(log.iter().filter(|(at, _, _)| *at <= cycle.frame).map(
                        |(at, class, text)| {
                            let appeared = cycle.frame_at(*at, 1150).max(mounted);
                            let t = entrance(appeared, now, 300);
                            let fresh = *at == cycle.frame;
                            let (color, background, border) = match *class {
                                "me" => (
                                    hex(0xeef1f7),
                                    rgba(60, 90, 200, 0.22),
                                    rgba(100, 140, 255, 0.3),
                                ),
                                "acc" => (hex(0xaec2ff), white(0.03), rgba(110, 150, 255, 0.3)),
                                "ok" => (hex(0x3be3a2), white(0.03), rgba(59, 227, 162, 0.25)),
                                _ => (hex(0xc9d0dc), white(0.03), white(0.05)),
                            };
                            div()
                                .relative()
                                .left(s.px(-4.0 * (1.0 - t)))
                                .opacity(t)
                                .when(*class == "me", |this| this.self_end())
                                .font_family(super::fonts::dm_sans())
                                .text_size(s.px(13.5))
                                .line_height(s.px(13.5 * 1.4))
                                .text_color(color)
                                .py(s.px(9.0))
                                .px(s.px(12.0))
                                .rounded(s.px(9.0))
                                .bg(background)
                                .border_1()
                                .border_color(border)
                                .when(fresh, |this| {
                                    let glow = glow_once(appeared, now);
                                    this.shadow(vec![shadow(
                                        rgba(120, 160, 255, 0.9 * glow),
                                        0.0,
                                        0.0,
                                        7.0,
                                        0.0,
                                        s,
                                    )])
                                })
                                .child(text.to_string())
                        },
                    )),
            )
            .into_any_element()
    }

    fn agents_together_demo(&self, s: S, mounted: Instant, now: Instant) -> Vec<AnyElement> {
        let cycle = cycle(mounted, now, 7, 1150, 3);
        let frame = cycle.frame;
        let handing = (2..5).contains(&frame);
        let mut out = vec![self.demo_card(
            s,
            (812.0, 300.0, 300.0, 320.0),
            ("claude", "Claude Code", "Lead agent".to_string()),
            if frame >= 6 {
                (PillKind::Done, "Done")
            } else {
                (PillKind::Run, "Working")
            },
            &LEAD_LOG,
            &cycle,
            !(3..6).contains(&frame),
            mounted,
            now,
        )];
        let send = match frame {
            2 => Some((true, cycle.frame_started)),
            5 => Some((false, cycle.frame_started)),
            _ => None,
        };
        let label_for = |frame: usize| {
            if (2..5).contains(&frame) {
                Some("Write tests for refresh.ts")
            } else if frame >= 5 {
                Some("Tests pass")
            } else {
                None
            }
        };
        let label = if handing {
            Some("Write tests for refresh.ts")
        } else {
            label_for(frame)
        };
        let label_changed = changed_at(&cycle, 1150, label_for).max(mounted);
        out.extend(demo_link(
            s,
            1112.0,
            460.0,
            206.0,
            send,
            label.map(|text| (text, label_changed)),
            now,
        ));
        out.push(self.demo_card(
            s,
            (1318.0, 300.0, 300.0, 320.0),
            (
                "codex",
                "Codex",
                if frame >= 3 {
                    "Started by Claude".to_string()
                } else {
                    "Sub-agent".to_string()
                },
            ),
            if frame >= 5 {
                (PillKind::Done, "Done")
            } else if frame >= 3 {
                (PillKind::Run, "Working")
            } else {
                (PillKind::Idle, "Waiting")
            },
            &SUB_LOG,
            &cycle,
            (3..6).contains(&frame),
            mounted,
            now,
        ));
        out
    }

    fn chat_terminal_demo(&self, s: S, mounted: Instant, now: Instant) -> Vec<AnyElement> {
        let cycle = cycle(mounted, now, TRANSCRIPT.len(), 1150, 3);
        let frame = cycle.frame;
        let elapsed = now.saturating_duration_since(mounted).as_secs_f32();
        let wires = [
            (
                wire_points(1215.0, 196.0, 1000.0, 250.0),
                (1000.0, 250.0),
                0.0f32,
            ),
            (
                wire_points(1215.0, 196.0, 1430.0, 250.0),
                (1430.0, 250.0),
                0.5f32,
            ),
        ];
        let scale = s.0;
        let wire_canvas = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let at = |(x, y): (f32, f32)| {
                    point(
                        bounds.origin.x + px(x * scale),
                        bounds.origin.y + px(y * scale),
                    )
                };
                for (points, end, delay) in &wires {
                    for (width, color) in [
                        (6.0, rgba(80, 130, 255, 0.5 * 0.45)),
                        (1.5, rgba(150, 182, 255, 1.0)),
                    ] {
                        let mut path = gpui::PathBuilder::stroke(px(width * scale));
                        path.move_to(at(points[0]));
                        for point in &points[1..] {
                            path.line_to(at(*point));
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color);
                        }
                    }
                    let dot = |center: (f32, f32),
                               radius: f32,
                               color: gpui::Hsla,
                               glow: gpui::Hsla,
                               window: &mut gpui::Window| {
                        let c = at(center);
                        for (extra, glow_alpha) in [(3.0, 0.25), (1.5, 0.45)] {
                            let r = (radius + extra) * scale;
                            window.paint_quad(
                                gpui::fill(
                                    Bounds::new(
                                        point(c.x - px(r), c.y - px(r)),
                                        size(px(r * 2.0), px(r * 2.0)),
                                    ),
                                    glow.opacity(glow_alpha),
                                )
                                .corner_radii(px(r)),
                            );
                        }
                        let r = radius * scale;
                        window.paint_quad(
                            gpui::fill(
                                Bounds::new(
                                    point(c.x - px(r), c.y - px(r)),
                                    size(px(r * 2.0), px(r * 2.0)),
                                ),
                                color,
                            )
                            .corner_radii(px(r)),
                        );
                    };
                    dot(*end, 3.4, hex(0x5b85ff), hex(0x4a78ff), window);
                    for (radius, begin) in [(3.2, *delay), (2.6, 0.57)] {
                        let local = elapsed - begin;
                        if local < 0.0 {
                            continue;
                        }
                        let t = (local / 1.15).fract();
                        dot(
                            along(points, t),
                            radius,
                            hex(0xcddaff),
                            hex(0x7a9dff),
                            window,
                        );
                    }
                }
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(s.px(STAGE_WIDTH))
        .h(s.px(STAGE_HEIGHT));
        let mut out = vec![wire_canvas.into_any_element()];
        out.push(
            abs(s, 1095.0, 158.0, Some(240.0), Some(38.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(s.px(9.0))
                .rounded(s.px(19.0))
                .bg(rgba(14, 18, 30, 0.95))
                .border_1()
                .border_color(rgba(100, 140, 255, 0.4))
                .shadow(vec![shadow(
                    rgba(60, 100, 255, 0.2),
                    0.0,
                    0.0,
                    22.0,
                    0.0,
                    s,
                )])
                .child(run_dot(s))
                .child(mono(s, 12.5, 500.0, hex(0xdfe4ee)).child("refactor-auth · one session"))
                .into_any_element(),
        );
        let caret_on = blink(self.opened_at, now);
        let terminal_lines =
            TRANSCRIPT[..=frame]
                .iter()
                .enumerate()
                .map(|(index, ((class, text), _))| {
                    let appeared = cycle.frame_at(index, 1150).max(mounted);
                    let t = entrance(appeared, now, 250);
                    let color = match *class {
                        "cmd" => hex(0xeef1f7),
                        "me" => hex(0xaec2ff),
                        "tool" => hex(0x9aa3b3),
                        "out" => hex(0x3be3a2),
                        _ => hex(0xc9d0dc),
                    };
                    mono(s, 12.5, 400.0, color)
                        .relative()
                        .left(s.px(-4.0 * (1.0 - t)))
                        .opacity(t)
                        .whitespace_nowrap()
                        .child(text.to_string())
                        .into_any_element()
                });
        out.push(
            demo_pane(s, 810.0, 250.0, "terminal", "Terminal")
                .child(
                    pane_body(s).bg(black(0.22)).children(terminal_lines).child(
                        div()
                            .w(s.px(8.0))
                            .h(s.px(14.0))
                            .flex_none()
                            .bg(hex(0xaab2c1))
                            .opacity(if caret_on { 1.0 } else { 0.0 }),
                    ),
                )
                .into_any_element(),
        );
        let chat_lines = TRANSCRIPT[..=frame]
            .iter()
            .enumerate()
            .filter_map(|(index, (_, line))| line.map(|line| (index, line)))
            .map(|(index, (class, text))| {
                let appeared = cycle.frame_at(index, 1150).max(mounted);
                let t = entrance(appeared, now, 300);
                let fresh = index == frame;
                let base = div()
                    .relative()
                    .left(s.px(-4.0 * (1.0 - t)))
                    .opacity(t)
                    .font_family(super::fonts::dm_sans());
                match class {
                    "me" => base
                        .self_end()
                        .max_w(relative(0.84))
                        .text_size(s.px(13.0))
                        .line_height(s.px(13.0 * 1.45))
                        .bg(rgba(60, 90, 200, 0.25))
                        .border_1()
                        .border_color(rgba(100, 140, 255, 0.3))
                        .py(s.px(8.0))
                        .px(s.px(12.0))
                        .rounded(s.px(12.0))
                        .when(fresh, |this| {
                            this.shadow(vec![shadow(
                                rgba(120, 160, 255, 0.9 * glow_once(appeared, now)),
                                0.0,
                                0.0,
                                7.0,
                                0.0,
                                s,
                            )])
                        })
                        .child(text.to_string())
                        .into_any_element(),
                    "tool" => base
                        .self_start()
                        .flex()
                        .items_center()
                        .gap(s.px(7.0))
                        .text_size(s.px(12.0))
                        .line_height(s.px(12.0 * 1.45))
                        .text_color(hex(0xaab2c1))
                        .py(s.px(5.0))
                        .px(s.px(10.0))
                        .rounded(s.px(8.0))
                        .bg(white(0.04))
                        .border_1()
                        .border_color(white(0.07))
                        .child(icon(s, "check", 13.0, 2.2, hex(0x3be3a2)))
                        .child(text.to_string())
                        .into_any_element(),
                    _ => base
                        .max_w(relative(0.94))
                        .text_size(s.px(13.0))
                        .line_height(s.px(13.0 * 1.45))
                        .text_color(hex(0xe8ecf4))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(s.px(6.0))
                                .mb(s.px(4.0))
                                .text_size(s.px(11.0))
                                .line_height(s.px(11.0 * 1.45))
                                .text_color(hex(0x8e97a8))
                                .child(agent_logo(s, &self.catalog, "claude", 13.0))
                                .child("Claude Code"),
                        )
                        .child(text.to_string())
                        .into_any_element(),
                }
            });
        out.push(
            demo_pane(s, 1240.0, 250.0, "chat", "Chat view")
                .child(pane_body(s).children(chat_lines))
                .into_any_element(),
        );
        out
    }

    fn desktop_mobile_demo(&self, s: S, mounted: Instant, now: Instant) -> Vec<AnyElement> {
        let cycle = cycle(mounted, now, 6, 1150, 3);
        let frame = cycle.frame;
        let codex_status = if frame >= 5 {
            (PillKind::Done, "Done")
        } else if frame >= 4 {
            (PillKind::Run, "Working")
        } else if frame >= 1 {
            (PillKind::Need, "Needs you")
        } else {
            (PillKind::Run, "Working")
        };
        let status_for = |id: &str| {
            if id == "codex" {
                codex_status
            } else {
                (PillKind::Run, "Working")
            }
        };
        let detail_key_changed = cycle
            .frame_at(if frame >= 4 { 4 } else { 0 }, 1150)
            .max(mounted);
        let detail_fade = progress(
            detail_key_changed,
            now,
            Duration::from_millis(350),
            Ease::Ease,
        );
        let mut out = Vec::new();
        out.push(
            div()
                .absolute()
                .left(s.px(800.0))
                .top(s.px(262.0))
                .w(s.px(450.0))
                .h(s.px(312.0))
                .flex()
                .flex_col()
                .rounded(s.px(12.0))
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(rgba(12, 16, 31, 0.95), 0.0),
                    linear_color_stop(rgba(7, 9, 18, 0.97), 1.0),
                ))
                .border(s.px(6.0))
                .border_color(hex(0x1b1e25))
                .shadow(vec![
                    shadow(rgba(90, 125, 235, 0.35), 0.0, 0.0, 0.0, 1.0, s),
                    shadow(black(0.6), 0.0, 20.0, 60.0, 0.0, s),
                    shadow(rgba(40, 80, 220, 0.14), 0.0, 0.0, 40.0, 0.0, s),
                ])
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(s.px(10.0))
                        .h(s.px(38.0))
                        .px(s.px(14.0))
                        .border_b_1()
                        .border_color(white(0.06))
                        .child(lights(s, true).mr(s.px(8.0)))
                        .child(ghostex_logo(s, 18.0, false))
                        .child(sans(s, 13.0, 700.0, hex(0xeef1f7)).child("orbit-api")),
                )
                .child(
                    div()
                        .flex_1()
                        .p(s.px(14.0))
                        .flex()
                        .flex_col()
                        .gap(s.px(8.0))
                        .children(MONITOR_SESSIONS.iter().map(|(id, name)| {
                            let hot = *id == "codex" && frame >= 1;
                            let (kind, text) = status_for(id);
                            div()
                                .flex()
                                .items_center()
                                .gap(s.px(10.0))
                                .h(s.px(40.0))
                                .px(s.px(12.0))
                                .rounded(s.px(9.0))
                                .border_1()
                                .map(|this| {
                                    if hot {
                                        this.border_color(rgba(255, 180, 70, 0.4))
                                            .bg(rgba(255, 180, 70, 0.05))
                                    } else {
                                        this.border_color(white(0.05)).bg(white(0.03))
                                    }
                                })
                                .child(agent_logo(s, &self.catalog, id, 16.0))
                                .child(
                                    mono(s, 12.5, 400.0, hex(0xeef1f7))
                                        .flex_1()
                                        .child(name.to_string()),
                                )
                                .child(status_pill(
                                    s,
                                    kind,
                                    text,
                                    self.opened_at,
                                    now,
                                    PILL_DEFAULT,
                                ))
                        }))
                        .child(
                            sans(s, 13.0, 400.0, hex(0xdfe4ee))
                                .mt(s.px(4.0))
                                .min_h(s.px(40.0))
                                .flex()
                                .items_center()
                                .gap(s.px(8.0))
                                .px(s.px(12.0))
                                .opacity(detail_fade)
                                .when((1..4).contains(&frame), |this| {
                                    this.child(agent_logo(s, &self.catalog, "codex", 14.0))
                                        .child("Keep the global fixtures, or inline them?")
                                })
                                .when(frame >= 4, |this| {
                                    this.child(
                                        div()
                                            .text_color(hex(0x3be3a2))
                                            .child("Replied from your phone:"),
                                    )
                                    .child("Inline them")
                                }),
                        ),
                )
                .into_any_element(),
        );
        out.push(
            abs(s, 990.0, 574.0, Some(70.0), Some(30.0))
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(hex(0x1b1e25), 0.0),
                    linear_color_stop(hex(0x12141a), 1.0),
                ))
                .into_any_element(),
        );
        out.push(
            abs(s, 945.0, 603.0, Some(160.0), Some(7.0))
                .rounded(s.px(4.0))
                .bg(hex(0x1b1e25))
                .into_any_element(),
        );
        out.push(demo_caption(
            s,
            800.0,
            628.0,
            450.0,
            "Your computer keeps the work",
        ));
        let send = match frame {
            1 => Some((true, cycle.frame_started)),
            4 => Some((false, cycle.frame_started)),
            _ => None,
        };
        let label_for = |frame: usize| match frame {
            1 | 2 => "Codex needs you",
            3 | 4 => "Inline them",
            _ => "Live",
        };
        let label_changed = changed_at(&cycle, 1150, label_for).max(mounted);
        out.extend(demo_link(
            s,
            1250.0,
            440.0,
            170.0,
            send,
            Some((label_for(frame), label_changed)),
            now,
        ));
        out.push(self.demo_phone(s, &cycle, frame, mounted, now, &status_for));
        out.push(demo_caption(
            s,
            1420.0,
            656.0,
            196.0,
            "Your phone steers it",
        ));
        out
    }

    fn demo_phone(
        &self,
        s: S,
        cycle: &Cycle,
        frame: usize,
        mounted: Instant,
        now: Instant,
        status_for: &dyn Fn(&str) -> (PillKind, &'static str),
    ) -> AnyElement {
        let banner = (frame == 1 || frame == 2).then(|| {
            let started = cycle.frame_at(1, 1150).max(mounted);
            let t = progress(
                started,
                now,
                Duration::from_millis(350),
                Ease::Bezier(0.2, 0.8, 0.2, 1.0),
            );
            div()
                .absolute()
                .left(s.px(8.0))
                .right(s.px(8.0))
                .top(s.px(30.0 - 30.0 * (1.0 - t)))
                .opacity(t)
                .flex()
                .items_center()
                .gap(s.px(8.0))
                .py(s.px(8.0))
                .px(s.px(10.0))
                .rounded(s.px(12.0))
                .bg(rgba(30, 36, 52, 0.97))
                .border_1()
                .border_color(rgba(255, 180, 70, 0.35))
                .child(agent_logo(s, &self.catalog, "codex", 13.0))
                .child(
                    sans(s, 10.0, 400.0, hex(0xc9d0dc))
                        .child(sans(s, 11.0, 700.0, gpui::white()).child("Codex needs you"))
                        .child("Keep global, or inline?"),
                )
        });
        let body: AnyElement = if frame < 2 {
            div()
                .flex()
                .flex_col()
                .gap(s.px(6.0))
                .px(s.px(10.0))
                .children(MONITOR_SESSIONS.iter().map(|(id, name)| {
                    let (kind, text) = status_for(id);
                    div()
                        .flex()
                        .items_center()
                        .gap(s.px(7.0))
                        .h(s.px(34.0))
                        .px(s.px(8.0))
                        .rounded(s.px(8.0))
                        .bg(white(0.035))
                        .child(agent_logo(s, &self.catalog, id, 13.0))
                        .child(
                            mono(s, 9.5, 400.0, hex(0xeef1f7))
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(name.to_string()),
                        )
                        .child(status_pill(
                            s,
                            kind,
                            text,
                            self.opened_at,
                            now,
                            (18.0, 6.0, 9.0, 4.0),
                        ))
                }))
                .into_any_element()
        } else {
            let bubble = |text: &str, me: bool, appeared: Instant| {
                let t = entrance(appeared, now, 300);
                sans(
                    s,
                    11.0,
                    400.0,
                    if me { hex(0xeef1f7) } else { hex(0xdfe4ee) },
                )
                .line_height(s.px(11.0 * 1.4))
                .relative()
                .left(s.px(-4.0 * (1.0 - t)))
                .opacity(t)
                .py(s.px(7.0))
                .px(s.px(9.0))
                .rounded(s.px(10.0))
                .max_w(relative(0.92))
                .map(|this| {
                    if me {
                        this.self_end()
                            .bg(rgba(60, 90, 200, 0.3))
                            .border_1()
                            .border_color(rgba(100, 140, 255, 0.3))
                    } else {
                        this.bg(white(0.05))
                    }
                })
                .child(text.to_string())
            };
            let quick = |text: &str, class: &str| {
                sans(s, 10.0, 400.0, hex(0xc9d6ff))
                    .py(s.px(5.0))
                    .px(s.px(8.0))
                    .rounded(s.px(8.0))
                    .border_1()
                    .border_color(rgba(110, 150, 255, 0.4))
                    .when(class == "tap", |this| this.bg(rgba(60, 90, 200, 0.55)))
                    .when(class == "picked", |this| this.bg(rgba(60, 90, 200, 0.3)))
                    .child(text.to_string())
            };
            div()
                .flex()
                .flex_col()
                .gap(s.px(8.0))
                .py(s.px(6.0))
                .px(s.px(10.0))
                .child(bubble(
                    "Keep the global fixtures, or inline them?",
                    false,
                    cycle.frame_at(2, 1150).max(mounted),
                ))
                .child(
                    div()
                        .flex()
                        .gap(s.px(6.0))
                        .child(quick("Keep global", ""))
                        .child(quick(
                            "Inline them",
                            if frame == 3 {
                                "tap"
                            } else if frame > 3 {
                                "picked"
                            } else {
                                ""
                            },
                        )),
                )
                .when(frame >= 3, |this| {
                    this.child(bubble(
                        "Inline them",
                        true,
                        cycle.frame_at(3, 1150).max(mounted),
                    ))
                })
                .when(frame >= 5, |this| {
                    this.child(bubble(
                        "Inlined 12 fixtures. Tests pass.",
                        false,
                        cycle.frame_at(5, 1150).max(mounted),
                    ))
                })
                .into_any_element()
        };
        div()
            .absolute()
            .left(s.px(1420.0))
            .top(s.px(236.0))
            .w(s.px(196.0))
            .h(s.px(404.0))
            .flex()
            .flex_col()
            .pt(s.px(34.0))
            .rounded(s.px(30.0))
            .border(s.px(6.0))
            .border_color(hex(0x1b1e25))
            .bg(hex(0x05070c))
            .shadow(vec![
                shadow(black(0.6), 0.0, 20.0, 60.0, 0.0, s),
                shadow(rgba(40, 80, 220, 0.16), 0.0, 0.0, 40.0, 0.0, s),
            ])
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .top(s.px(8.0))
                    .left(s.px((196.0 - 12.0 - 60.0) / 2.0))
                    .w(s.px(60.0))
                    .h(s.px(18.0))
                    .rounded(s.px(9.0))
                    .bg(hex(0x000000)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s.px(8.0))
                    .pt(s.px(4.0))
                    .px(s.px(14.0))
                    .pb(s.px(10.0))
                    .child(ghostex_logo(s, 16.0, false))
                    .child(sans(s, 13.0, 600.0, hex(0xeef1f7)).child("Ghostex")),
            )
            .child(body)
            .children(banner)
            .into_any_element()
    }
}

/// `.run` dot: 7px green with a glow.
pub(crate) fn run_dot(s: S) -> Div {
    div()
        .size(s.px(7.0))
        .flex_none()
        .rounded_full()
        .bg(hex(0x3be3a2))
        .shadow(vec![shadow(hex(0x3be3a2), 0.0, 0.0, 6.0, 0.0, s)])
}

/// `.dcard` surface.
fn glass_card(s: S) -> Div {
    div()
        .rounded(s.px(14.0))
        .bg(linear_gradient(
            180.0,
            linear_color_stop(rgba(13, 19, 38, 0.86), 0.0),
            linear_color_stop(rgba(7, 10, 21, 0.88), 1.0),
        ))
        .border_1()
        .border_color(rgba(88, 124, 235, 0.26))
        .shadow(vec![inset_shadow(white(0.05), 0.0, 1.0, 0.0, 0.0, s)])
}

/// `.dpane` with its header.
fn demo_pane(s: S, left: f32, top: f32, icon_name: &str, title: &str) -> Div {
    div()
        .absolute()
        .left(s.px(left))
        .top(s.px(top))
        .w(s.px(380.0))
        .h(s.px(440.0))
        .flex()
        .flex_col()
        .rounded(s.px(14.0))
        .bg(linear_gradient(
            180.0,
            linear_color_stop(rgba(12, 16, 31, 0.9), 0.0),
            linear_color_stop(rgba(7, 9, 18, 0.92), 1.0),
        ))
        .border_1()
        .border_color(rgba(90, 125, 235, 0.34))
        .shadow(vec![
            inset_shadow(white(0.05), 0.0, 1.0, 0.0, 0.0, s),
            shadow(rgba(40, 80, 220, 0.12), 0.0, 0.0, 34.0, 0.0, s),
        ])
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(s.px(10.0))
                .h(s.px(46.0))
                .px(s.px(18.0))
                .border_b_1()
                .border_color(white(0.06))
                .font_family(super::fonts::dm_sans())
                .text_size(s.px(14.0))
                .font_weight(FontWeight(500.0))
                .text_color(hex(0xe8ecf4))
                .child(icon(s, icon_name, 16.0, 1.6, hex(0x6d93ff)))
                .child(title.to_string()),
        )
}

fn pane_body(s: S) -> Div {
    div()
        .flex_1()
        .py(s.px(16.0))
        .px(s.px(18.0))
        .flex()
        .flex_col()
        .gap(s.px(10.0))
        .overflow_hidden()
}

/// `.dcap`: the uppercase caption under a demo device.
pub(crate) fn demo_caption(s: S, left: f32, top: f32, width: f32, text: &str) -> AnyElement {
    abs(s, left, top, Some(width), Some(20.0))
        .flex()
        .justify_center()
        .child(tracked_text(
            s,
            text.to_uppercase(),
            PLEX_MONO,
            500.0,
            11.0,
            11.0 * LH_MONO,
            0.16,
            hex(0x7f8aa0),
        ))
        .into_any_element()
}

fn relative(value: f32) -> gpui::DefiniteLength {
    gpui::relative(value)
}
