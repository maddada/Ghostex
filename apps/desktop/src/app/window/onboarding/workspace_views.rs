//! The mock workspace window's views (Agents session, Browser, Files, Code, Kanban, Automate) on
//! the Workspace panel (previews/workspace-window.tsx (deleted 2026-10-01), styles/workspace.css).
use super::GpuiOnboardingWindow;
use super::fonts::{MANROPE, PLEX_MONO};
use super::interact;
use super::primitives::*;
use super::stage::*;
use super::workspace::DEMO_AGENTS;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, div, img, linear_color_stop, linear_gradient,
};
use std::time::{Duration, Instant};

const DIFF_LINES: [(&str, &str); 11] = [
    ("ctx", "import { lock } from './lock';"),
    ("ctx", ""),
    ("ctx", "export async function refresh(session: Session) {"),
    ("del", "  const token = await fetchToken(session);"),
    ("del", "  session.token = token;"),
    ("add", "  return lock.run(session.id, async () => {"),
    ("add", "    const token = await fetchToken(session);"),
    ("add", "    session.token = token;"),
    ("add", "    return token;"),
    ("add", "  });"),
    ("ctx", "}"),
];

const KANBAN_CARDS: [(&str, &str); 4] = [
    ("Label the yearly toggle", "codex"),
    ("Fix the flaky refresh test", "claude"),
    ("Move tokens to CSS variables", "cursor"),
    ("Write release notes", "claude"),
];

const AUTOMATIONS: [(&str, &str, &str); 3] = [
    ("Nightly dependency bump", "Every night at 02:00", "codex"),
    ("Weekly issue triage", "Mondays at 09:00", "claude"),
    ("Release notes", "Once, Friday at 17:00", "cursor"),
];

fn demo_chat(agent: &str) -> &'static [(&'static str, &'static str)] {
    match agent {
        "claude" => &[
            ("me", "Build the pricing page."),
            (
                "bot",
                "Done: /pricing has three tiers and a yearly toggle. Tests pass.",
            ),
        ],
        "codex" => &[
            ("me", "Review the pricing page."),
            (
                "bot",
                "Looks good. One nit: the yearly toggle has no label.",
            ),
        ],
        "cursor" => &[("bot", "Ready when you are.")],
        _ => &[("bot", "Copilot and OpenCode are ready in this project.")],
    }
}

impl GpuiOnboardingWindow {
    pub(super) fn ws_session_view(&self, s: S, now: Instant) -> AnyElement {
        let agent = self.workspace.agent;
        let (_, name, _) = DEMO_AGENTS
            .iter()
            .find(|(id, _, _)| *id == agent)
            .copied()
            .unwrap_or(DEMO_AGENTS[0]);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                pane_header(s)
                    .child(agent_logo(s, &self.catalog, agent, 20.0))
                    .child(sans(s, 15.0, 500.0, hex(0xeef1f7)).flex_1().child(name))
                    .child(phone_pill(
                        s,
                        "run",
                        if agent == "claude" {
                            "Running"
                        } else {
                            "Ready"
                        },
                        self.opened_at,
                        now,
                        false,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .py(s.px(18.0))
                    .px(s.px(22.0))
                    .flex()
                    .flex_col()
                    .gap(s.px(12.0))
                    .children(
                        demo_chat(agent)
                            .iter()
                            .map(|(kind, text)| chat_bubble(s, kind, text, name, None, now)),
                    ),
            )
            .child(
                message_input(s)
                    .mx(s.px(18.0))
                    .mb(s.px(18.0))
                    .child(sans(s, 16.0, 400.0, hex(0x8e97a8)).child(format!("Message {name}"))),
            )
            .into_any_element()
    }

    pub(super) fn ws_browser_view(
        &self,
        s: S,
        now: Instant,
        drive: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let since = self.workspace.drive_since.max(self.workspace.shown_since);
        let auto_clicked = drive && {
            let elapsed = now.saturating_duration_since(since).as_millis() as u64;
            elapsed >= 2600 && (elapsed - 2600) % 5200 < 2000
        };
        let manual_clicked = self
            .workspace
            .browser_clicked_at
            .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_millis(2000));
        let clicked = auto_clicked || manual_clicked;
        let cursor = drive.then(|| {
            // `gxob-cursorGo`: 5.2s loop from (250, 110) to the button (60, 14), a press, and back.
            let t = (now.saturating_duration_since(since).as_secs_f32() / 5.2).fract();
            let segment = |from: f32, to: f32, a: f32, b: f32| {
                let k = Ease::EaseInOut.apply(((t - from) / (to - from)).clamp(0.0, 1.0));
                a + (b - a) * k
            };
            let (x, y, scale) = if t < 0.42 {
                (
                    segment(0.0, 0.42, 250.0, 60.0),
                    segment(0.0, 0.42, 110.0, 14.0),
                    1.0,
                )
            } else if t < 0.47 {
                (60.0, 14.0, segment(0.42, 0.47, 1.0, 0.78))
            } else if t < 0.52 {
                (60.0, 14.0, segment(0.47, 0.52, 0.78, 1.0))
            } else if t < 0.82 {
                (60.0, 14.0, 1.0)
            } else {
                (
                    segment(0.82, 1.0, 60.0, 250.0),
                    segment(0.82, 1.0, 14.0, 110.0),
                    1.0,
                )
            };
            let opacity = if t < 0.12 {
                t / 0.12
            } else if t < 0.82 {
                1.0
            } else {
                1.0 - (t - 0.82) / 0.18
            };
            let size_px = 20.0 * scale;
            div()
                .absolute()
                .left(s.px(x + (20.0 - size_px) / 2.0))
                .top(s.px(y + (20.0 - size_px) / 2.0))
                .size(s.px(size_px))
                .opacity(opacity)
                .child(img("onboarding/cursor.svg").size_full())
        });
        let heading_line = |text: &str| {
            tracked_text(
                s,
                text.to_string(),
                MANROPE,
                700.0,
                42.0,
                48.0,
                -0.025,
                hex(0xeef1f7),
            )
        };
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .child(
                div()
                    .mt(s.px(12.0))
                    .mx(s.px(12.0))
                    .h(s.px(32.0))
                    .rounded(s.px(7.0))
                    .border_1()
                    .border_color(white(0.07))
                    .flex()
                    .items_center()
                    .gap(s.px(10.0))
                    .px(s.px(12.0))
                    .child(icon(s, "arrowL", 12.0, 1.6, hex(0xaab2c1)))
                    .child(icon(s, "arrowR", 12.0, 1.6, hex(0xaab2c1)))
                    .child(icon(s, "refresh", 12.0, 1.6, hex(0xaab2c1)))
                    .child(mono(s, 12.0, 400.0, hex(0xaab2c1)).ml(s.px(10.0)).child("http://localhost:3000")),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_start()
                    .py(s.px(26.0))
                    .px(s.px(34.0))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(s.px(18.0))
                            .font_family(super::fonts::dm_sans())
                            .text_size(s.px(10.5))
                            .text_color(hex(0xaab2c1))
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .gap(s.px(8.0))
                                    .items_center()
                                    .text_color(hex(0xe8ecf4))
                                    .text_size(s.px(12.0))
                                    .font_weight(FontWeight(700.0))
                                    .child(
                                        div()
                                            .size(s.px(15.0))
                                            .rounded_full()
                                            .bg(hex(0x2c58e6))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(s.px(8.0))
                                            .font_weight(FontWeight(700.0))
                                            .child("A"),
                                    )
                                    .child("Acme"),
                            )
                            .child("Product")
                            .child("Docs")
                            .child("Pricing")
                            .child(
                                div()
                                    .py(s.px(4.0))
                                    .px(s.px(10.0))
                                    .rounded(s.px(6.0))
                                    .border_1()
                                    .border_color(white(0.1))
                                    .child("Sign in"),
                            ),
                    )
                    .child(
                        div()
                            .mt(s.px(84.0))
                            .child(heading_line(if clicked { "You're in." } else { "Build faster" }))
                            .child(heading_line(if clicked { "Welcome to Acme." } else { "with AI agents." })),
                    )
                    .child(sans(s, 15.0, 400.0, hex(0xb3bac7)).mt(s.px(10.0)).child("From idea to production, together."))
                    .child({
                        // `.site-btn:hover { filter: brightness(1.15) }`, with no transition.
                        let bright = if interact::hovered("ws-site-button") { 1.15 } else { 1.0 };
                        self.control(
                            s,
                            div()
                                .id("ws-site-button")
                                .relative()
                                .mt(s.px(18.0))
                                .flex()
                                .items_center()
                                .gap(s.px(8.0))
                                .h(s.px(36.0))
                                .px(s.px(16.0))
                                .rounded(s.px(7.0))
                                .bg(linear_gradient(
                                    180.0,
                                    linear_color_stop(brightness(hex(0x2446b8), bright), 0.0),
                                    linear_color_stop(brightness(hex(0x1b3796), bright), 1.0),
                                ))
                                .border_1()
                                .border_color(brightness(rgba(120, 150, 255, 0.45), bright))
                                .font_family(super::fonts::dm_sans())
                                .text_size(s.px(12.5))
                                .text_color(brightness(hex(0xeef1f7), bright))
                                .cursor_pointer()
                                .child("Get started")
                                .child(icon(s, "arrowR", 13.0, 1.6, brightness(hex(0xeef1f7), bright)))
                                .children(cursor),
                            "ws-site-button",
                            interact::Ring::new(7.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| {
                                this.workspace.browser_clicked_at = Some(Instant::now());
                                this.show_toast("You clicked it", cx);
                            },
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .right(s.px(30.0))
                            .top(s.px(150.0))
                            .w(s.px(170.0))
                            .h(s.px(250.0))
                            .rounded(s.px(10.0))
                            .border_1()
                            .border_color(white(0.06))
                            .bg(white(0.02))
                            .p(s.px(10.0))
                            .flex()
                            .flex_col()
                            .gap(s.px(9.0))
                            .child(lights(s, true).mb(s.px(6.0)))
                            .children((0..3).map(|_| div().h(s.px(4.0)).rounded(s.px(2.0)).bg(white(0.07)))),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(s.px(3.0))
                    .py(s.px(10.0))
                    .px(s.px(16.0))
                    .border_t_1()
                    .border_color(white(0.05))
                    .bg(rgba(14, 18, 32, 0.72))
                    .child(
                        sans(s, 12.0, 600.0, if drive { hex(0x8fabff) } else { hex(0x9ea7b6) }).child(if drive {
                            "Your agent is using the browser skill."
                        } else {
                            "The browser skill is off, so agents can't touch this browser."
                        }),
                    )
                    .child(
                        sans(s, 11.5, 400.0, hex(0x8a93a4))
                            .line_height(s.px(11.5 * 1.45))
                            .child(if drive {
                                "It can open pages, click, type, read the page and take screenshots. Ask your agent to use it; remove it in Settings → Integrations."
                            } else {
                                "Switch it on and agents can open pages, click, type, read the page and take screenshots."
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn ws_docs_view(&self, s: S, cx: &mut Context<Self>) -> AnyElement {
        let saved = self.workspace.docs_saved;
        let node = |width: f32, height: f32| {
            div()
                .w(s.px(width))
                .h(s.px(height))
                .flex_none()
                .rounded(s.px(8.0))
                .border_1()
                .border_color(white(0.1))
                .bg(white(0.025))
                .font_family(super::fonts::dm_sans())
                .text_size(s.px(12.0))
                .text_color(hex(0xd6dbe5))
        };
        let line = || {
            div()
                .w(s.px(36.0))
                .h(s.px(1.0))
                .flex_none()
                .bg(rgba(150, 170, 230, 0.45))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                pane_header(s)
                    .child(sans(s, 15.0, 500.0, hex(0xeef1f7)).flex_1().child("Project plan"))
                    .child({
                        // `.edit-chip { transition: all 0.2s }`.
                        let key = "ws-docs-edit";
                        let color = interact::tween_color(
                            key,
                            "color",
                            if saved { hex(0x3be3a2) } else { hex(0x4a68c4) },
                            200,
                        );
                        let border = interact::tween_color(
                            key,
                            "border",
                            if saved { rgba(59, 227, 162, 0.3) } else { rgba(60, 90, 200, 0.2) },
                            200,
                        );
                        let bg = interact::tween_color(
                            key,
                            "bg",
                            if saved { rgba(59, 227, 162, 0.06) } else { rgba(60, 90, 200, 0.08) },
                            200,
                        );
                        self.control(
                            s,
                            div()
                                .id(key)
                                .relative()
                                .flex()
                                .items_center()
                                .gap(s.px(5.0))
                                .h(s.px(26.0))
                                .px(s.px(12.0))
                                .rounded(s.px(7.0))
                                .border_1()
                                .font_family(super::fonts::dm_sans())
                                .text_size(s.px(11.5))
                                .cursor_pointer()
                                .text_color(color)
                                .border_color(border)
                                .bg(bg)
                                .map(|this| {
                                    if saved {
                                        this.child(icon(s, "check", 12.0, 1.6, color)).child("Saved")
                                    } else {
                                        this.child("Editing...")
                                    }
                                }),
                            key,
                            interact::Ring::new(7.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| {
                                this.workspace.docs_saved = !this.workspace.docs_saved;
                                cx.notify();
                            },
                        )
                    }),
            )
            .child(
                div()
                    .py(s.px(4.0))
                    .px(s.px(26.0))
                    .child(
                        sans(s, 13.0, 400.0, hex(0xaab2c1))
                            .line_height(s.px(13.0 * 1.5))
                            .max_w(s.px(520.0))
                            .child("An overview of the architecture, key components, and next steps for the agent-driven application."),
                    )
                    .child(sans(s, 13.5, 500.0, hex(0xeef1f7)).mt(s.px(20.0)).child("System diagram"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .mt(s.px(14.0))
                            .child(
                                node(78.0, 70.0)
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .gap(s.px(4.0))
                                    .child(icon(s, "users", 16.0, 1.6, hex(0xd6dbe5)))
                                    .child("User"),
                            )
                            .child(line())
                            .child(
                                node(92.0, 52.0)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .border_color(rgba(160, 120, 255, 0.55))
                                    // GPUI paints a shadow under the whole box, so the node takes the
                                    // opaque colour of what is behind it to keep the glow outside.
                                    .bg(hex(0x10121a))
                                    .shadow(vec![shadow(rgba(140, 100, 255, 0.2), 0.0, 0.0, 14.0, 0.0, s)])
                                    .text_size(s.px(13.5))
                                    .font_weight(FontWeight(500.0))
                                    .child("Agent"),
                            )
                            .child(line())
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(s.px(10.0))
                                    .children([("target", "Browser"), ("list", "Files")].into_iter().map(|(icon_name, text)| {
                                        node(132.0, 44.0)
                                            .flex()
                                            .items_center()
                                            .px(s.px(12.0))
                                            .gap(s.px(10.0))
                                            .child(icon(s, icon_name, 15.0, 1.6, hex(0xd6dbe5)))
                                            .child(text)
                                    })),
                            ),
                    )
                    .child(sans(s, 13.5, 500.0, hex(0xeef1f7)).mt(s.px(20.0)).child("Next steps"))
                    .child(
                        div()
                            .mt(s.px(10.0))
                            .ml(s.px(18.0))
                            .children(
                                ["Ship the pricing page", "Label the yearly toggle", "Hand release notes to Automate"]
                                    .into_iter()
                                    .map(|item| {
                                        sans(s, 13.0, 400.0, hex(0xc9d0dc))
                                            .line_height(s.px(13.0 * 1.9))
                                            .relative()
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(s.px(-12.0))
                                                    .top(s.px(13.0 * 0.95 - 2.5))
                                                    .size(s.px(5.0))
                                                    .rounded_full()
                                                    .bg(hex(0xc9d0dc)),
                                            )
                                            .child(item)
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn ws_code_view(&self, s: S, now: Instant) -> AnyElement {
        let mounted = self.workspace.shown_since;
        let step = (now.saturating_duration_since(mounted).as_millis() / 420) as usize;
        let shown = step % (DIFF_LINES.len() + 5);
        let tree_line = |text: &str, dim: bool, on: bool| {
            mono(
                s,
                11.5,
                400.0,
                if on {
                    gpui::white()
                } else if dim {
                    hex(0x8e97a8)
                } else {
                    hex(0xaab2c1)
                },
            )
            .whitespace_nowrap()
            .when(on, |this| {
                this.bg(rgba(60, 90, 200, 0.2))
                    .rounded(s.px(5.0))
                    .mx(s.px(-6.0))
                    .py(s.px(2.0))
                    .px(s.px(6.0))
            })
            .child(text.to_string())
        };
        let cycle_start = mounted + Duration::from_millis(420 * (step - shown) as u64);
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .w(s.px(170.0))
                    .flex_none()
                    .py(s.px(16.0))
                    .px(s.px(14.0))
                    .border_r_1()
                    .border_color(white(0.06))
                    .flex()
                    .flex_col()
                    .gap(s.px(8.0))
                    .child(tree_line("src", true, false))
                    .child(tree_line("  auth", true, false))
                    .child(tree_line("    refresh.ts", false, true))
                    .child(tree_line("    lock.ts", false, false))
                    .child(tree_line("  pricing.tsx", false, false)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(s.px(10.0))
                            .h(s.px(40.0))
                            .px(s.px(14.0))
                            .border_b_1()
                            .border_color(white(0.06))
                            .child(
                                mono(s, 12.0, 400.0, hex(0xeef1f7))
                                    .py(s.px(6.0))
                                    .px(s.px(10.0))
                                    .rounded(s.px(6.0))
                                    .bg(white(0.05))
                                    .child("refresh.ts"),
                            )
                            .child(mono(s, 12.0, 400.0, hex(0xe3b341)).child("M")),
                    )
                    .child(
                        div()
                            .py(s.px(12.0))
                            .flex()
                            .flex_col()
                            .children(
                                DIFF_LINES
                                    .iter()
                                    .take(shown.min(DIFF_LINES.len()))
                                    .enumerate()
                                    .map(|(index, (kind, text))| {
                                        let appeared = cycle_start
                                            + Duration::from_millis(420 * (index as u64 + 1));
                                        let t = entrance(appeared, now, 200);
                                        let (sign, sign_color, background, color) = match *kind {
                                            "add" => (
                                                "+",
                                                hex(0x3be3a2),
                                                Some(rgba(59, 227, 162, 0.08)),
                                                hex(0xc9d0dc),
                                            ),
                                            "del" => (
                                                "−",
                                                hex(0xff6b62),
                                                Some(rgba(255, 95, 87, 0.08)),
                                                hex(0x9aa3b3),
                                            ),
                                            _ => (" ", hex(0x7c8598), None, hex(0xc9d0dc)),
                                        };
                                        div()
                                            .relative()
                                            .left(s.px(-4.0 * (1.0 - t)))
                                            .opacity(t)
                                            .flex()
                                            .items_center()
                                            .h(s.px(25.0))
                                            .pr(s.px(12.0))
                                            .font_family(PLEX_MONO)
                                            .text_size(s.px(12.0))
                                            .text_color(color)
                                            .whitespace_nowrap()
                                            .when_some(background, |this, background| {
                                                this.bg(background)
                                            })
                                            .child(
                                                div()
                                                    .w(s.px(40.0))
                                                    .flex_none()
                                                    .flex()
                                                    .justify_end()
                                                    .pr(s.px(12.0))
                                                    .text_color(hex(0x4d5566))
                                                    .child((index + 1).to_string()),
                                            )
                                            .child(
                                                div()
                                                    .w(s.px(16.0))
                                                    .flex_none()
                                                    .text_color(sign_color)
                                                    .child(sign),
                                            )
                                            .child(text.to_string())
                                    }),
                            )
                            .when(shown < DIFF_LINES.len(), |this| {
                                this.child(
                                    div()
                                        .mt(s.px(6.0))
                                        .ml(s.px(68.0))
                                        .w(s.px(2.0))
                                        .h(s.px(12.0))
                                        .bg(hex(0x6d93ff))
                                        .opacity(if blink(self.opened_at, now) {
                                            1.0
                                        } else {
                                            0.0
                                        }),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn ws_kanban_view(&self, s: S, now: Instant) -> AnyElement {
        let mounted = self.workspace.shown_since;
        let tick = (now.saturating_duration_since(mounted).as_millis() / 1800) as usize;
        let step = tick % 5;
        let column_of = |index: usize, step: usize| {
            if index + 1 < step {
                "done"
            } else if index + 1 == step {
                "prog"
            } else {
                "todo"
            }
        };
        // A card re-mounts (and slides in) when it changes column.
        let mounted_at = |index: usize| {
            let column = column_of(index, step);
            let mut first = step;
            while first > 0 && column_of(index, first - 1) == column {
                first -= 1;
            }
            let loop_start = mounted + Duration::from_millis(1800 * (tick - step) as u64);
            (loop_start + Duration::from_millis(1800 * first as u64)).max(mounted)
        };
        div()
            .size_full()
            .flex()
            .gap(s.px(12.0))
            .p(s.px(16.0))
            .children(
                [("todo", "To do"), ("prog", "In progress"), ("done", "Done")]
                    .into_iter()
                    .map(|(id, title)| {
                        let cards: Vec<usize> = (0..KANBAN_CARDS.len())
                            .filter(|index| column_of(*index, step) == id)
                            .collect();
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(s.px(8.0))
                            .p(s.px(12.0))
                            .rounded(s.px(10.0))
                            .bg(white(0.025))
                            .border_1()
                            .border_color(white(0.05))
                            .child(
                                sans(s, 12.0, 400.0, hex(0xaab2c1))
                                    .flex()
                                    .justify_between()
                                    .mb(s.px(4.0))
                                    .child(title)
                                    .child(
                                        div()
                                            .font_weight(FontWeight(500.0))
                                            .px(s.px(6.0))
                                            .rounded(s.px(4.0))
                                            .bg(white(0.08))
                                            .child(cards.len().to_string()),
                                    ),
                            )
                            .children(cards.into_iter().map(|index| {
                                let (text, agent) = KANBAN_CARDS[index];
                                let t = entrance(mounted_at(index), now, 450);
                                sans(s, 12.5, 400.0, hex(0xeef1f7))
                                    .line_height(s.px(12.5 * 1.35))
                                    .relative()
                                    .left(s.px(-40.0 * (1.0 - t)))
                                    .opacity(t * if id == "done" { 0.7 } else { 1.0 })
                                    .flex()
                                    .items_center()
                                    .gap(s.px(8.0))
                                    .p(s.px(12.0))
                                    .rounded(s.px(9.0))
                                    .bg(white(0.045))
                                    .border_1()
                                    .map(|this| {
                                        if id == "prog" {
                                            this.border_color(rgba(110, 150, 255, 0.45)).shadow(
                                                vec![shadow(
                                                    rgba(60, 100, 255, 0.15),
                                                    0.0,
                                                    0.0,
                                                    16.0,
                                                    0.0,
                                                    s,
                                                )],
                                            )
                                        } else {
                                            this.border_color(white(0.07))
                                        }
                                    })
                                    .child(div().flex_1().min_w_0().child(text))
                                    .when(id != "todo", |this| {
                                        this.child(agent_logo(s, &self.catalog, agent, 14.0))
                                    })
                            }))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn ws_automate_view(&self, s: S, cx: &mut Context<Self>) -> AnyElement {
        let seconds = 59 - (chrono::Local::now().timestamp() % 60);
        div()
            .size_full()
            .flex()
            .flex_col()
            .pb(s.px(14.0))
            .child(
                pane_header(s)
                    .child(
                        sans(s, 15.0, 500.0, hex(0xeef1f7))
                            .flex_1()
                            .child("Automations"),
                    )
                    .child(
                        mono(s, 11.5, 400.0, hex(0x6d93ff))
                            .child(format!("next run in 00:{seconds:02}")),
                    ),
            )
            .children(
                AUTOMATIONS
                    .iter()
                    .enumerate()
                    .map(|(index, (title, schedule, agent))| {
                        div()
                            .flex()
                            .items_center()
                            .gap(s.px(14.0))
                            .mx(s.px(14.0))
                            .mb(s.px(10.0))
                            .py(s.px(14.0))
                            .px(s.px(16.0))
                            .rounded(s.px(10.0))
                            .bg(white(0.03))
                            .border_1()
                            .border_color(white(0.06))
                            .child(
                                div()
                                    .size(s.px(9.0))
                                    .flex_none()
                                    .rounded_full()
                                    .border(s.px(1.5))
                                    .map(|this| {
                                        if index == 0 {
                                            this.bg(gpui::white())
                                                .border_color(gpui::white())
                                                .shadow(vec![shadow(
                                                    gpui::white(),
                                                    0.0,
                                                    0.0,
                                                    8.0,
                                                    0.0,
                                                    s,
                                                )])
                                        } else {
                                            this.border_color(hex(0x4a5570))
                                        }
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        sans(s, 14.0, 500.0, hex(0xeef1f7))
                                            .child(title.to_string()),
                                    )
                                    .child(
                                        sans(s, 12.0, 400.0, hex(0x8e97a8))
                                            .mt(s.px(3.0))
                                            .child(schedule.to_string()),
                                    ),
                            )
                            .child(agent_logo(s, &self.catalog, agent, 18.0))
                    }),
            )
            .child(
                self.control(
                    s,
                    sans(s, 12.5, 400.0, hex(0xc9d0dc))
                        .id("ws-new-automation")
                        .relative()
                        .my(s.px(6.0))
                        .mx(s.px(14.0))
                        .self_start()
                        .flex()
                        .items_center()
                        .gap(s.px(8.0))
                        .h(s.px(34.0))
                        .px(s.px(14.0))
                        .rounded(s.px(8.0))
                        .border_1()
                        .border_dashed()
                        .border_color(white(0.18))
                        .cursor_pointer()
                        .child(icon(s, "plus", 14.0, 1.6, hex(0xc9d0dc)))
                        .child("New automation"),
                    "ws-new-automation",
                    interact::Ring::new(8.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.show_toast("New automation", cx),
                ),
            )
            .into_any_element()
    }
}

/// `.pane-h`.
pub(crate) fn pane_header(s: S) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(12.0))
        .h(s.px(48.0))
        .px(s.px(16.0))
}

/// `.fm-input`: a composer box.
pub(crate) fn message_input(s: S) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .pl(s.px(14.0))
        .pr(s.px(10.0))
        .h(s.px(44.0))
        .rounded(s.px(10.0))
        .border_1()
        .border_color(white(0.1))
        .bg(white(0.02))
}

/// `.fb`: a chat bubble (the user's on the right, the agent's with its name above).
pub(crate) fn chat_bubble(
    s: S,
    kind: &str,
    text: &str,
    agent_name: &str,
    appeared: Option<Instant>,
    now: Instant,
) -> AnyElement {
    let t = appeared.map_or(1.0, |appeared| entrance(appeared, now, 300));
    let base = sans(s, 14.0, 400.0, hex(0xdfe4ee))
        .line_height(s.px(14.0 * 1.5))
        .max_w(gpui::relative(0.84))
        .relative()
        .left(s.px(-4.0 * (1.0 - t)))
        .opacity(t);
    if kind == "me" {
        base.self_end()
            .text_color(hex(0xeef1f7))
            .bg(rgba(60, 90, 200, 0.25))
            .border_1()
            .border_color(rgba(100, 140, 255, 0.3))
            .py(s.px(9.0))
            .px(s.px(13.0))
            .rounded(s.px(12.0))
            .child(text.to_string())
            .into_any_element()
    } else {
        base.child(
            sans(s, 11.0, 400.0, hex(0x8e97a8))
                .mb(s.px(3.0))
                .child(agent_name.to_string()),
        )
        .child(text.to_string())
        .into_any_element()
    }
}

/// `.ph-pill`: the phone and session status chip.
pub(crate) fn phone_pill(
    s: S,
    kind: &str,
    label: &str,
    epoch: Instant,
    now: Instant,
    chevron: bool,
) -> AnyElement {
    let (color, background, border, period) = if kind == "work" {
        (
            hex(0x6d93ff),
            rgba(80, 120, 255, 0.1),
            rgba(80, 120, 255, 0.25),
            0.9,
        )
    } else {
        (
            hex(0x3be3a2),
            rgba(59, 227, 162, 0.1),
            rgba(59, 227, 162, 0.3),
            1.8,
        )
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(5.0))
        .h(s.px(22.0))
        .px(s.px(8.0))
        .rounded(s.px(6.0))
        .bg(background)
        .border_1()
        .border_color(border)
        .font_family(super::fonts::dm_sans())
        .font_weight(FontWeight(500.0))
        .text_size(s.px(10.5))
        .text_color(color)
        .whitespace_nowrap()
        .child(
            div()
                .size(s.px(6.0))
                .rounded_full()
                .bg(color)
                .opacity(breathe(epoch, now, period, 0.0)),
        )
        .child(label.to_string())
        .when(chevron, |this| {
            this.child(icon(s, "chevR", 10.0, 2.4, color))
        })
        .into_any_element()
}
