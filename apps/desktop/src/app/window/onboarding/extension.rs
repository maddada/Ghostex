//! The Agents panel's right-hand explainers: what the Ghostex integration adds and what Computer Use
//! can do (previews/agent-extension-panels.tsx (deleted 2026-10-01), `.xpanel` and `.pv-*` in styles/agents.css).
use super::GpuiOnboardingWindow;
use super::OnboardingCommand;
use super::fonts::PLEX_MONO;
use super::interact;
use super::model::{
    ComputerUseDesktop, ComputerUseGrantSetting, ComputerUseState, computer_use_copy,
};
use super::primitives::*;
use super::stage::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, div, img, linear_color_stop, linear_gradient,
};
use std::time::{Duration, Instant};

const INTEGRATION_LEAD: &str = "Ghostex adds a small helper to each agent's own settings, so it can show what every agent is doing.";
const INTEGRATION_TERMS: [(&str, &str); 4] = [
    (
        "What's added",
        "A small Ghostex helper in each agent's own settings.",
    ),
    (
        "What you get",
        "Live status, alerts, session names, the chat view and resume.",
    ),
    (
        "Your agent",
        "Still runs as its normal CLI, and nothing goes through a Ghostex cloud.",
    ),
    (
        "To remove it",
        "Settings → Agents, one agent or all at once.",
    ),
];
const COMPUTER_USE_LEAD: &str = "This hands over the whole machine: supported agents can see your screen and drive apps outside Ghostex.";
const COMPUTER_USE_TERMS: [(&str, &str); 3] = [
    (
        "What's added",
        "A Computer Use skill for your agents and a small helper app.",
    ),
    (
        "What agents can do",
        "Click, type and read other apps when a task needs it.",
    ),
    ("To turn it off", "Settings → Integrations, any time."),
];
const INTEGRATION_SESSIONS: [(&str, &str, &str); 3] = [
    ("claude", "claude", "refactor-auth"),
    ("codex", "codex", "port-tests"),
    ("cursor", "cursor-agent", "css-tokens"),
];
const HELPER_FILES: [(&str, &str); 3] = [
    ("claude", "~/.claude/settings.json"),
    ("codex", "~/.codex/hooks.json"),
    ("cursor", "~/.cursor/hooks.json"),
];
const CURSOR_PATH: [(f32, f32); 6] = [
    (500.0, 230.0),
    (70.0, 76.0),
    (70.0, 76.0),
    (46.0, 190.0),
    (46.0, 190.0),
    (500.0, 230.0),
];
const CALENDAR_DAYS: [(&str, &[&str]); 3] = [
    ("WED", &["Design sync"]),
    ("THU", &["Standup"]),
    ("FRI", &[]),
];

/// `<ExtensionPanel>`: the lead line, a preview and the terms grid.
fn extension_panel(s: S, lead: &str, terms: &[(&str, &str)], preview: AnyElement) -> AnyElement {
    let mut grid_rows = Vec::new();
    for pair in terms.chunks(2) {
        grid_rows.push(
            div()
                .flex()
                .gap(s.px(24.0))
                .children(pair.iter().map(|(term, text)| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(s.px(2.0))
                        .child(tracked_text(
                            s,
                            term.to_uppercase(),
                            PLEX_MONO,
                            500.0,
                            10.0,
                            17.0,
                            0.1,
                            hex(0x7f8aa0),
                        ))
                        .child(
                            sans(s, 12.5, 400.0, hex(0xc9d0dc))
                                .line_height(s.px(12.5 * 1.4))
                                .child(text.to_string()),
                        )
                }))
                .when(pair.len() == 1, |this| this.child(div().flex_1())),
        );
    }
    glass(s)
        .absolute()
        .left(s.px(885.0))
        .top(s.px(174.0))
        .w(s.px(700.0))
        .h(s.px(586.0))
        .flex()
        .flex_col()
        .gap(s.px(14.0))
        .py(s.px(18.0))
        .px(s.px(20.0))
        .border_color(rgba(90, 125, 235, 0.25))
        .bg(linear_gradient(
            180.0,
            linear_color_stop(rgba(10, 14, 28, 0.86), 0.0),
            linear_color_stop(rgba(6, 8, 16, 0.9), 1.0),
        ))
        .child(
            sans(s, 14.0, 400.0, hex(0xdfe4ee))
                .flex_none()
                .line_height(s.px(14.0 * 1.5))
                .child(lead.to_string()),
        )
        .child(div().flex_1().min_h_0().relative().child(preview))
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(s.px(11.0))
                .children(grid_rows),
        )
        .into_any_element()
}

impl GpuiOnboardingWindow {
    pub(super) fn render_integration_panel(&self, s: S, now: Instant) -> AnyElement {
        let mounted = self.agents_panel.right_tab_since;
        let cycle = cycle(mounted, now, 4, 1300, 3);
        let frame = cycle.frame;
        let on = self.flow.integration_on;
        let status = |id: &str| match id {
            "claude" if frame >= 2 => (PillKind::Done, "Done"),
            "codex" if frame == 1 || frame == 2 => (PillKind::Need, "Needs you"),
            _ => (PillKind::Run, "Working"),
        };
        let column = |with: bool| {
            let lit = with == on;
            let alert: AnyElement = if !with {
                div()
                    .h(s.px(44.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(s.px(10.0))
                    .rounded(s.px(10.0))
                    .border_1()
                    .border_dashed()
                    .border_color(white(0.14))
                    .child(
                        sans(s, 11.5, 400.0, hex(0x7c8598))
                            .child("No alerts: you only find out when you look."),
                    )
                    .into_any_element()
            } else if frame == 1 || frame == 2 {
                let started = cycle.frame_at(1, 1300).max(mounted);
                let t = progress(
                    started,
                    now,
                    Duration::from_millis(350),
                    Ease::Bezier(0.2, 0.8, 0.2, 1.0),
                );
                div()
                    .relative()
                    .top(s.px(-30.0 * (1.0 - t)))
                    .opacity(t)
                    .h(s.px(44.0))
                    .flex()
                    .items_center()
                    .gap(s.px(9.0))
                    .px(s.px(10.0))
                    .rounded(s.px(10.0))
                    .bg(rgba(30, 36, 52, 0.97))
                    .border_1()
                    .border_color(rgba(255, 180, 70, 0.35))
                    .child(agent_logo(s, &self.catalog, "codex", 14.0))
                    .child(
                        sans(s, 10.5, 400.0, hex(0xc9d0dc))
                            .child(sans(s, 11.5, 700.0, gpui::white()).child("Codex needs you"))
                            .child("Keep the global fixtures, or inline them?"),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            };
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(s.px(7.0))
                .p(s.px(13.0))
                .rounded(s.px(12.0))
                .border_1()
                .map(|this| {
                    if lit {
                        this.border_color(rgba(110, 150, 255, 0.55))
                            .shadow(vec![shadow(
                                rgba(60, 100, 255, 0.14),
                                0.0,
                                0.0,
                                26.0,
                                0.0,
                                s,
                            )])
                    } else {
                        this.border_color(white(0.07)).opacity(0.55)
                    }
                })
                .bg(rgba(10, 12, 20, 0.75))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(sans(s, 13.5, 600.0, hex(0xeef1f7)).child(if with {
                            "With the integration"
                        } else {
                            "Without it"
                        }))
                        .when(lit, |this| {
                            this.child(tracked_text(
                                s,
                                "YOUR CHOICE",
                                PLEX_MONO,
                                500.0,
                                10.0,
                                13.0,
                                0.1,
                                hex(0x8fabff),
                            ))
                        }),
                )
                .child(div().h(s.px(44.0)).child(alert))
                .children(INTEGRATION_SESSIONS.iter().map(|(id, command, name)| {
                    let (kind, text) = if with {
                        status(id)
                    } else {
                        (PillKind::Idle, "No status")
                    };
                    div()
                        .flex()
                        .items_center()
                        .gap(s.px(9.0))
                        .h(s.px(36.0))
                        .px(s.px(10.0))
                        .rounded(s.px(9.0))
                        .bg(white(0.035))
                        .border_1()
                        .border_color(white(0.05))
                        .child(if with {
                            agent_logo(s, &self.catalog, id, 16.0)
                        } else {
                            icon(s, "terminal", 16.0, 1.6, hex(0x9aa3b3)).into_any_element()
                        })
                        .child(
                            mono(s, 12.0, 400.0, hex(0xeef1f7))
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(if with {
                                    name.to_string()
                                } else {
                                    command.to_string()
                                }),
                        )
                        .child(status_pill(
                            s,
                            kind,
                            text,
                            self.opened_at,
                            now,
                            (20.0, 7.0, 10.5, 6.0),
                        ))
                }))
                .child(
                    sans(s, 12.0, 400.0, hex(0x9ea7b6))
                        .mt_auto()
                        .line_height(s.px(12.0 * 1.4))
                        .child(if with {
                            "Named after the task, live, and it tells you when an agent is waiting."
                        } else {
                            "Plain terminals. You check each one yourself."
                        }),
                )
        };
        let files = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(s.px(18.0))
            .gap_y(s.px(7.0))
            .py(s.px(11.0))
            .px(s.px(14.0))
            .rounded(s.px(12.0))
            .border_1()
            .border_color(white(0.07))
            .bg(black(0.28))
            .child(
                div()
                    .w_full()
                    .child(label_sized(s, "Where the helper goes", 10.0)),
            )
            .children(HELPER_FILES.iter().map(|(id, file)| {
                mono(s, 12.0, 400.0, hex(0xc9d0dc))
                    .flex()
                    .items_center()
                    .gap(s.px(8.0))
                    .child(agent_logo(s, &self.catalog, id, 14.0))
                    .child(file.to_string())
            }))
            .child(
                sans(s, 12.0, 400.0, hex(0x7f8aa0))
                    .w_full()
                    .child("…and the matching settings file of each other agent you connect."),
            );
        let preview = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(s.px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(s.px(10.0))
                    .child(column(false))
                    .child(column(true)),
            )
            .child(files)
            .into_any_element();
        extension_panel(s, INTEGRATION_LEAD, &INTEGRATION_TERMS, preview)
    }

    pub(super) fn render_computer_use_panel(
        &self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.computer_use_state();
        let mounted = self.agents_panel.right_tab_since;
        let cycle = cycle(mounted, now, CURSOR_PATH.len(), 1100, 2);
        let frame = cycle.frame;
        let asking = state == ComputerUseState::Permissions;
        let copy = computer_use_copy(ComputerUseDesktop::current());
        let caption = match state {
            ComputerUseState::On => "Claude Code is using Calendar",
            ComputerUseState::Installing => "Installing Computer Use…",
            ComputerUseState::Permissions => "Waiting for your permission",
            ComputerUseState::Off => "What Claude Code could do in Calendar",
        };
        let (target_x, target_y) = CURSOR_PATH[frame];
        let move_curve = (
            Duration::from_millis(800),
            Ease::Bezier(0.45, 0.0, 0.2, 1.0),
        );
        let cursor_x = self
            .transitions
            .value("cu-cursor-x", target_x, move_curve, now);
        let cursor_y = self
            .transitions
            .value("cu-cursor-y", target_y, move_curve, now);
        let field = |top: f32, focus: bool| {
            div()
                .absolute()
                .left(s.px(20.0))
                .top(s.px(top))
                .w(s.px(190.0))
                .h(s.px(34.0))
                .flex()
                .items_center()
                .px(s.px(10.0))
                .rounded(s.px(8.0))
                .border_1()
                .bg(white(0.03))
                .font_family(super::fonts::dm_sans())
                .text_size(s.px(13.0))
                .text_color(hex(0xeef1f7))
                .map(|this| {
                    if focus {
                        // The 3px focus ring, drawn outside the field (a spread shadow would also
                        // fill it, since GPUI paints shadows under the whole box).
                        this.border_color(rgba(110, 150, 255, 0.7)).child(
                            div()
                                .absolute()
                                .left(s.px(-4.0))
                                .top(s.px(-4.0))
                                .right(s.px(-4.0))
                                .bottom(s.px(-4.0))
                                .rounded(s.px(11.0))
                                .border(s.px(3.0))
                                .border_color(rgba(110, 150, 255, 0.15)),
                        )
                    } else {
                        this.border_color(white(0.12))
                    }
                })
        };
        let small_label = |left: f32, top: f32, text: &str| {
            div()
                .absolute()
                .left(s.px(left))
                .top(s.px(top))
                .child(tracked_text(
                    s,
                    text.to_uppercase(),
                    PLEX_MONO,
                    500.0,
                    10.0,
                    13.0,
                    0.12,
                    hex(0x7f8aa0),
                ))
        };
        // `gxob-typeIn`: "Release review" typed one character per step over 0.9s.
        let typed = if frame >= 2 {
            let started = cycle.frame_at(2, 1100).max(mounted);
            let steps = (now.saturating_duration_since(started).as_secs_f32() / 0.9 * 14.0).floor()
                as usize;
            Some(
                "Release review"
                    .chars()
                    .take(steps.min(14))
                    .collect::<String>(),
            )
        } else {
            None
        };
        let click_ring = (frame == 1 || frame == 4).then(|| {
            let started = cycle.frame_started.max(mounted);
            let t = progress(started, now, Duration::from_millis(600), Ease::EaseOut);
            let ring = 18.0 * (0.3 + 1.1 * t);
            div()
                .absolute()
                .left(s.px(2.0 - ring / 2.0))
                .top(s.px(1.0 - ring / 2.0))
                .size(s.px(ring))
                .rounded_full()
                .border(s.px(2.0))
                .border_color(hex(0x8fabff))
                .opacity(1.0 - t)
        });
        let save_tapped = frame == 4;
        let app = div()
            .relative()
            .flex_1()
            .min_h_0()
            .rounded(s.px(12.0))
            .bg(hex(0x0d1018))
            .border_1()
            .border_color(white(0.1))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s.px(12.0))
                    .h(s.px(32.0))
                    .px(s.px(12.0))
                    .border_b_1()
                    .border_color(white(0.06))
                    .font_family(super::fonts::dm_sans())
                    .text_size(s.px(12.0))
                    .text_color(hex(0xc9d0dc))
                    .child(lights(s, true))
                    .child("Calendar"),
            )
            .child(small_label(20.0, 48.0, "Title"))
            .child(
                field(64.0, frame == 1 || frame == 2)
                    .when_some(typed, |this, typed| this.child(typed)),
            )
            .child(small_label(20.0, 112.0, "When"))
            .child(field(128.0, false).child("Friday · 15:00"))
            .child(
                div()
                    .absolute()
                    .left(s.px(20.0))
                    .top(s.px(180.0))
                    .h(s.px(32.0))
                    .px(s.px(18.0))
                    .flex()
                    .items_center()
                    .rounded(s.px(8.0))
                    .bg(if save_tapped {
                        hex(0x2e5af5)
                    } else {
                        hex(0x2548c4)
                    })
                    .font_family(super::fonts::dm_sans())
                    .font_weight(FontWeight(500.0))
                    .text_size(s.px(13.0))
                    .text_color(hex(0xeef1f7))
                    .child("Save"),
            )
            .child(
                div()
                    .absolute()
                    .left(s.px(232.0))
                    .right(s.px(14.0))
                    .top(s.px(46.0))
                    .bottom(s.px(14.0))
                    .flex()
                    .gap(s.px(6.0))
                    .children(CALENDAR_DAYS.iter().map(|(day, events)| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(s.px(6.0))
                            .py(s.px(8.0))
                            .px(s.px(7.0))
                            .rounded(s.px(8.0))
                            .bg(white(0.025))
                            .border_1()
                            .border_color(white(0.05))
                            .child(tracked_text(
                                s,
                                day.to_string(),
                                PLEX_MONO,
                                500.0,
                                9.5,
                                12.0,
                                0.08,
                                hex(0x7f8aa0),
                            ))
                            .children(events.iter().map(|event| calendar_event(s, event, false)))
                            .when(*day == "FRI" && frame >= 4, |this| {
                                let started = cycle.frame_at(4, 1100).max(mounted);
                                let t = progress(
                                    started,
                                    now,
                                    Duration::from_millis(400),
                                    Ease::Bezier(0.3, 1.6, 0.5, 1.0),
                                );
                                this.child(
                                    calendar_event(s, "Release review · 15:00", true)
                                        .opacity(t.clamp(0.0, 1.0)),
                                )
                            })
                    })),
            )
            .child(
                div()
                    .absolute()
                    .left(s.px(cursor_x))
                    .top(s.px(cursor_y))
                    .size(s.px(20.0))
                    .children(click_ring)
                    .child(img("onboarding/cursor.svg").size(s.px(20.0))),
            );
        let mut permission = |id: &'static str,
                              icon_name: &str,
                              name: &str,
                              why: &str,
                              open: Option<OnboardingCommand>| {
            let active = asking && open.is_some();
            // `button.pv-perm.act { transition: border-color 0.2s, background 0.2s }` and its hover.
            let hover = active && interact::hovered(id);
            let border = interact::tween_color(
                id,
                "border",
                if !active {
                    white(0.07)
                } else if hover {
                    rgba(150, 180, 255, 0.7)
                } else {
                    rgba(110, 150, 255, 0.45)
                },
                if active { 200 } else { 0 },
            );
            let bg = interact::tween_color(
                id,
                "bg",
                if !active {
                    white(0.025)
                } else if hover {
                    rgba(60, 90, 200, 0.2)
                } else {
                    rgba(60, 90, 200, 0.12)
                },
                if active { 200 } else { 0 },
            );
            let row = div()
                .id(id)
                .relative()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(s.px(10.0))
                .h(s.px(40.0))
                .px(s.px(12.0))
                .rounded(s.px(10.0))
                .border_1()
                .font_family(super::fonts::dm_sans())
                .text_size(s.px(13.0))
                .text_color(hex(0xeef1f7))
                .border_color(border)
                .bg(bg)
                .when(active, |this| this.cursor_pointer())
                .child(icon(s, icon_name, 16.0, 1.6, hex(0x8fabff)))
                .child(
                    div()
                        .font_weight(FontWeight(700.0))
                        .whitespace_nowrap()
                        .child(name.to_string()),
                )
                .child(
                    sans(s, 12.0, 400.0, hex(0x9ea7b6))
                        .min_w_0()
                        .child(why.to_string()),
                )
                .when(active, |this| {
                    this.child(
                        sans(s, 12.0, 400.0, hex(0x8fabff))
                            .ml_auto()
                            .whitespace_nowrap()
                            .child("Open"),
                    )
                });
            match open.filter(|_| active) {
                Some(command) => self.control(
                    s,
                    row,
                    id,
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    move |this, _, cx| this.send(command.clone(), cx),
                ),
                None => row,
            }
        };
        let grants: Vec<_> = copy
            .grants
            .iter()
            .zip(["cu-grant-0", "cu-grant-1"])
            .map(|(grant, id)| {
                let opens = grant.opens.map(|setting| match setting {
                    ComputerUseGrantSetting::Accessibility => {
                        OnboardingCommand::OpenAccessibilityPreferences
                    }
                    ComputerUseGrantSetting::ScreenRecording => {
                        OnboardingCommand::OpenScreenRecordingPreferences
                    }
                });
                permission(id, grant.icon, grant.name, grant.why, opens)
            })
            .collect();
        let preview = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(s.px(11.0))
            .child(
                div()
                    .self_start()
                    .flex()
                    .items_center()
                    .gap(s.px(8.0))
                    .h(s.px(28.0))
                    .px(s.px(12.0))
                    .rounded(s.px(14.0))
                    .bg(rgba(14, 18, 30, 0.95))
                    .border_1()
                    .border_color(rgba(100, 140, 255, 0.4))
                    .font_family(super::fonts::dm_sans())
                    .text_size(s.px(12.0))
                    .text_color(hex(0xdfe4ee))
                    .child(agent_logo(s, &self.catalog, "claude", 14.0))
                    .child(caption),
            )
            .child(app)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(s.px(12.0))
                    .gap_y(s.px(8.0))
                    .child(div().w_full().child(label_sized(
                        s,
                        if asking {
                            copy.asking_label
                        } else {
                            copy.label
                        },
                        10.0,
                    )))
                    .child(div().w_full().flex().gap(s.px(12.0)).children(grants)),
            )
            .into_any_element();
        extension_panel(s, COMPUTER_USE_LEAD, &COMPUTER_USE_TERMS, preview)
    }
}

fn calendar_event(s: S, text: &str, new: bool) -> Div {
    sans(
        s,
        11.0,
        400.0,
        if new { gpui::white() } else { hex(0xc9d0dc) },
    )
    .line_height(s.px(11.0 * 1.3))
    .py(s.px(6.0))
    .px(s.px(7.0))
    .rounded(s.px(6.0))
    .map(|this| {
        if new {
            this.bg(rgba(60, 90, 200, 0.55))
                .border_1()
                .border_color(rgba(130, 165, 255, 0.6))
        } else {
            this.bg(white(0.07))
        }
    })
    .child(text.to_string())
}
