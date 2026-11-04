//! The "You're set" screen: the summary of every choice, the mock window of the first session and
//! the after-onboarding Install guide prompt (panels/finished.tsx (deleted 2026-10-01), styles/get-started.css).
use super::FinishTarget;
use super::GpuiOnboardingWindow;
use super::fonts::PLEX_MONO;
use super::install_guide::{modal_actions, modal_p, popup};
use super::interact;
use super::model::{
    ComputerUseState, VIEW_KEYS, ViewKey, agent_display_name, all_installed_have_hooks,
    default_agent_id,
};
use super::primitives::*;
use super::stage::*;
use super::welcome::run_dot;
use super::workspace_views::{chat_bubble, message_input, phone_pill};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, div,
};
use std::time::{Duration, Instant};

/// Delay before the after-onboarding popup appears, so the summary lands first.
const FOLLOW_UP_DELAY: Duration = Duration::from_millis(900);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FollowUp {
    Ask,
    Guide,
}

pub(crate) struct FinishedState {
    started: Instant,
    shown_lines: usize,
    next_line_at: Instant,
    pub(crate) follow_up: Option<FollowUp>,
    pub(crate) follow_up_at: Instant,
    follow_up_due: Option<Instant>,
}

impl FinishedState {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            started: now,
            shown_lines: 1,
            next_line_at: now + Duration::from_millis(1100),
            follow_up: None,
            follow_up_at: now,
            follow_up_due: Some(now + FOLLOW_UP_DELAY),
        }
    }
}

fn demo_log(agent: &str) -> &'static [(&'static str, &'static str)] {
    match agent {
        "codex" => &[
            (
                "me",
                "Get familiar with this repo and suggest a first task.",
            ),
            (
                "bot",
                "Scanned the repo: TypeScript, Bun, 42 files, 17 tests.",
            ),
            (
                "bot",
                "The legacy test harness has 12 call sites. Porting those is a good first task.",
            ),
        ],
        "cursor" => &[
            (
                "me",
                "Get familiar with this repo and suggest a first task.",
            ),
            (
                "bot",
                "42 files, 48 design tokens still hard-coded in SCSS.",
            ),
            (
                "bot",
                "Moving those tokens to CSS variables is a clean first task. Want me to start?",
            ),
        ],
        "terminal" => &[
            ("cmd", "$ ls"),
            ("out", "README.md  package.json  src  tests"),
            ("cmd", "$ git status"),
            (
                "out",
                "On branch main · nothing to commit, working tree clean",
            ),
        ],
        _ => &[
            (
                "me",
                "Get familiar with this repo and suggest a first task.",
            ),
            (
                "bot",
                "Read package.json, src/ and tests/. 42 files, one flaky test in tests/auth.",
            ),
            (
                "bot",
                "Suggestion: fix the flaky refresh test first; it blocks CI. Want me to start?",
            ),
        ],
    }
}

impl GpuiOnboardingWindow {
    fn finished_start_with(&self) -> String {
        self.flow
            .start_with
            .clone()
            .or_else(|| default_agent_id(&self.settings, &self.agents))
            .unwrap_or_else(|| "terminal".to_string())
    }

    pub(super) fn tick_finished(&mut self, now: Instant, _cx: &mut Context<Self>) {
        let log_len = demo_log(&self.finished_start_with()).len();
        if self.finished.shown_lines < log_len && now >= self.finished.next_line_at {
            self.finished.shown_lines += 1;
            self.finished.next_line_at = now + Duration::from_millis(1100);
        }
        if let Some(due) = self.finished.follow_up_due
            && now >= due
        {
            self.finished.follow_up_due = None;
            if self.flow.install_queued && self.finished.follow_up.is_none() {
                self.finished.follow_up = Some(FollowUp::Ask);
                self.finished.follow_up_at = now;
            }
        }
    }

    pub(super) fn finished_popup_later(&mut self, cx: &mut Context<Self>) {
        match self.finished.follow_up {
            Some(FollowUp::Ask) => {
                self.finished.follow_up = None;
                self.show_toast("You can do this anytime from Settings", cx);
            }
            Some(FollowUp::Guide) => {
                self.finished.follow_up = None;
                self.flow.install_queued = false;
            }
            None => {}
        }
        cx.notify();
    }

    pub(super) fn render_finished(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let folder = self
            .flow
            .finished_path
            .clone()
            .or_else(|| self.picked_folder.clone())
            .unwrap_or_default();
        let project = if folder.is_empty() {
            "Your project".to_string()
        } else {
            folder_basename(&folder)
        };
        let default_agent = default_agent_id(&self.settings, &self.agents);
        let start_with = self.finished_start_with();
        let session_view = self.settings.preferred_interface.clone();
        let notify = self.settings.notify;
        let views_on: Vec<ViewKey> = VIEW_KEYS
            .iter()
            .copied()
            .filter(|key| self.settings.is_view_on(*key))
            .collect();
        let views_summary = if views_on.is_empty() {
            "None, agents only".to_string()
        } else {
            views_on
                .iter()
                .map(|key| {
                    if *key == ViewKey::Browser && self.browser_skill_shown_on() {
                        "Browser (browser skill on)".to_string()
                    } else {
                        key.title().to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let connected = self.flow.integration_on && all_installed_have_hooks(&self.agents);
        let mut rows: Vec<(&str, String, usize)> = vec![
            (
                "Default agent",
                agent_display_name(&self.agents, default_agent.as_deref()),
                2,
            ),
            (
                "Ghostex integration",
                if !self.flow.integration_on {
                    "Off".into()
                } else if connected {
                    "Every detected agent connected".into()
                } else {
                    "Skipped for now".into()
                },
                2,
            ),
            (
                "Computer Use",
                match self.computer_use_state() {
                    ComputerUseState::On => "On".into(),
                    ComputerUseState::Permissions => {
                        super::model::computer_use_copy(super::model::ComputerUseDesktop::current())
                            .pending_summary
                            .into()
                    }
                    ComputerUseState::Installing => "Installing…".into(),
                    ComputerUseState::Off => "Off".into(),
                },
                2,
            ),
            ("Workspace views", views_summary, 3),
            (
                "Mobile",
                format!(
                    "{}{}",
                    if self.flow.phone_queued {
                        "Remote settings open after this screen"
                    } else {
                        "Not now"
                    },
                    if notify { " · agent alerts on" } else { "" }
                ),
                4,
            ),
            (
                "Project",
                if folder.is_empty() {
                    "No folder chosen".into()
                } else {
                    folder.clone()
                },
                5,
            ),
            (
                "First session",
                if start_with == "terminal" {
                    "Terminal, no agent".into()
                } else {
                    agent_display_name(&self.agents, Some(&start_with))
                },
                5,
            ),
            (
                "Session view",
                if session_view == "chat" {
                    "Chat".into()
                } else {
                    "Terminal".into()
                },
                5,
            ),
        ];
        if self.flow.install_queued {
            rows.push(("Install guide", "Opens after this screen".into(), 2));
        }
        let row_count = rows.len();
        let mut out = Vec::new();
        out.push(eyebrow(s, 44.0, 118.0, None, "You're set").into_any_element());
        out.push(
            heading(
                s,
                44.0,
                145.0,
                720.0,
                52.0,
                "Ghostex is open.",
                Some(&format!("{project} is ready.")),
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 44.0, 264.0, 680.0, 16.0, false)
                .child("Here is everything you chose. Click a row to change it; all of it stays changeable in Settings.")
                .into_any_element(),
        );
        out.push(
            glass(s)
                .absolute()
                .left(s.px(44.0))
                .top(s.px(320.0))
                .w(s.px(699.0))
                .h(s.px(row_count as f32 * 44.0 + 18.0))
                .py(s.px(9.0))
                .px(s.px(22.0))
                .children(
                    rows.into_iter()
                        .enumerate()
                        .map(|(index, (key, value, panel))| {
                            let id = SharedString::from(format!("sum-row-{index}"));
                            // `.sum-row .edit { transition: opacity 0.2s }`, shown on the row's hover.
                            let edit = interact::tween_value(
                                &id,
                                "edit",
                                if interact::hovered(&id) { 1.0 } else { 0.0 },
                                200,
                            );
                            let row = div()
                                .id(id.clone())
                                .relative()
                                .flex()
                                .items_center()
                                .w_full()
                                .h(s.px(44.0))
                                .when(index + 1 < row_count, |this| {
                                    this.border_b_1().border_color(white(0.05))
                                })
                                .cursor_pointer()
                                .child(div().w(s.px(190.0)).flex_none().child(tracked_text(
                                    s,
                                    key.to_uppercase(),
                                    PLEX_MONO,
                                    500.0,
                                    11.0,
                                    11.0 * LH_MONO,
                                    0.16,
                                    hex(0x8e97a8),
                                )))
                                .child(
                                    sans(s, 14.5, 400.0, hex(0xeef1f7))
                                        .flex_1()
                                        .min_w_0()
                                        .child(value),
                                )
                                .child(
                                    sans(s, 12.0, 400.0, hex(0x6d93ff))
                                        .opacity(edit)
                                        .child("Edit"),
                                );
                            self.control(
                                s,
                                row,
                                id,
                                interact::Ring::new(0.0, 0.0),
                                interact::Keys::EnterSpace,
                                cx,
                                move |this, _, cx| this.go(panel, cx),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .into_any_element(),
        );
        out.push(
            abs(s, 44.0, 340.0 + row_count as f32 * 44.0 + 30.0, None, None)
                .flex()
                .items_center()
                .child(self.control(
                    s,
                    cta(
                        s,
                        "finished-start",
                        "Start working",
                        true,
                        true,
                        false,
                        CtaSize::Large,
                    ),
                    "finished-start",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.finish(FinishTarget::None, cx),
                ))
                .child(self.control(
                    s,
                    ghost(s, "finished-back", "Back to setup", 15.5, false).ml(s.px(22.0)),
                    "finished-back",
                    interact::Ring::new(0.0, 0.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.go(5, cx),
                ))
                .into_any_element(),
        );
        out.push(self.finished_window(
            s,
            now,
            &project,
            &folder,
            &start_with,
            &session_view,
            &views_on,
            connected,
        ));
        out.push(
            glass_n(s)
                .absolute()
                .left(s.px(1008.0))
                .top(s.px(814.0))
                .w(s.px(420.0))
                .h(s.px(70.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .id("finished-ready")
                .map(|this| interact::watch_hover(this, "finished-ready"))
                // `.ready { transition: border-color 0.25s }` and its `:hover` border.
                .border_color(interact::hover_color(
                    "finished-ready",
                    "border",
                    rgba(88, 124, 235, 0.3),
                    rgba(125, 162, 255, 0.7),
                    250,
                ))
                .cursor_pointer()
                .child(
                    sans(s, 18.0, 600.0, hex(0xeef1f7))
                        .flex()
                        .items_center()
                        .gap(s.px(12.0))
                        .child(
                            div()
                                .size(s.px(24.0))
                                .rounded_full()
                                .bg(hex(0x16b877))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icon(s, "check", 15.0, 2.6, gpui::white())),
                        )
                        .child("Project ready"),
                )
                .child(
                    sans(s, 15.0, 400.0, hex(0xc5ccd8))
                        .mt(s.px(6.0))
                        .child("The session keeps running if you close this window."),
                )
                .into_any_element(),
        );
        match self.finished.follow_up {
            Some(FollowUp::Ask) => {
                let this = cx.entity().downgrade();
                let close = self.popup_close(s, cx);
                let body = vec![
                    modal_p(s)
                        .child("It lists agents you can add; install one and Ghostex picks it up.")
                        .into_any_element(),
                    modal_actions(s)
                        .child(self.control(
                            s,
                            ghost(s, "follow-up-later", "Later", 15.5, false),
                            "follow-up-later",
                            interact::Ring::new(0.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| this.finished_popup_later(cx),
                        ))
                        .child(self.control(
                            s,
                            cta(
                                s,
                                "follow-up-open",
                                "Open guide",
                                true,
                                false,
                                false,
                                CtaSize::Small,
                            ),
                            "follow-up-open",
                            interact::Ring::new(10.0, 1.0),
                            interact::Keys::EnterSpace,
                            cx,
                            |this, _, cx| {
                                this.finished.follow_up = Some(FollowUp::Guide);
                                this.finished.follow_up_at = Instant::now();
                                cx.notify();
                            },
                        ))
                        .into_any_element(),
                ];
                out.push(popup(
                    s,
                    "Open the install guide now?",
                    480.0,
                    self.finished.follow_up_at,
                    now,
                    close,
                    body,
                    move |_, cx| {
                        let _ = this.update(cx, |this, cx| this.finished_popup_later(cx));
                    },
                ));
            }
            Some(FollowUp::Guide) => out.push(self.render_install_guide(s, now, false, cx)),
            None => {}
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn finished_window(
        &self,
        s: S,
        now: Instant,
        project: &str,
        folder: &str,
        start_with: &str,
        session_view: &str,
        views_on: &[ViewKey],
        connected: bool,
    ) -> AnyElement {
        let log = demo_log(start_with);
        let terminal_mode = start_with == "terminal" || session_view == "terminal";
        let agent_name = agent_display_name(&self.agents, Some(start_with));
        let mut tabs = vec!["Agents".to_string()];
        tabs.extend(views_on.iter().map(|key| key.title().to_string()));
        let shown = self.finished.shown_lines.min(log.len());
        let started = self.finished.started;
        let line_appeared = |index: usize| started + Duration::from_millis(1100 * index as u64);
        let lines = log[..shown]
            .iter()
            .enumerate()
            .map(|(index, (kind, text))| {
                if terminal_mode {
                    let t = entrance(line_appeared(index), now, 250);
                    let color = match *kind {
                        "cmd" => hex(0xeef1f7),
                        "out" | "bot" => hex(0x9aa3b3),
                        _ => hex(0xc9d0dc),
                    };
                    mono(s, 13.0, 400.0, color)
                        .relative()
                        .left(s.px(-4.0 * (1.0 - t)))
                        .opacity(t)
                        .flex()
                        .when(*kind == "me", |this| {
                            this.child(div().text_color(hex(0x6d93ff)).child("> "))
                        })
                        .child(text.to_string())
                        .into_any_element()
                } else {
                    chat_bubble(s, kind, text, &agent_name, Some(line_appeared(index)), now)
                }
            });
        let typing = (shown < log.len()).then(|| {
            div()
                .flex()
                .gap(s.px(4.0))
                .py(s.px(4.0))
                .children((0..3).map(|index| {
                    div()
                        .size(s.px(6.0))
                        .rounded_full()
                        .bg(hex(0x6d93ff))
                        .opacity(breathe(self.opened_at, now, 1.0, index as f32 * 0.15))
                }))
        });
        let fs_pop = progress(
            started,
            now,
            Duration::from_millis(400),
            Ease::Bezier(0.3, 1.6, 0.5, 1.0),
        );
        glass_n(s)
            .absolute()
            .left(s.px(812.0))
            .top(s.px(92.0))
            .w(s.px(812.0))
            .h(s.px(704.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .h(s.px(52.0))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(s.px(12.0))
                    .px(s.px(18.0))
                    .border_b_1()
                    .border_color(white(0.05))
                    .child(lights(s, false).mr(s.px(26.0)))
                    .child(ghostex_logo(s, 26.0, false))
                    .child(sans(s, 14.0, 600.0, hex(0xeef1f7)).child(project.to_string()))
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .gap(s.px(4.0))
                            .children(tabs.into_iter().enumerate().map(|(index, tab)| {
                                sans(s, 12.0, 400.0, if index == 0 { gpui::white() } else { hex(0x9aa3b3) })
                                    .py(s.px(5.0))
                                    .px(s.px(11.0))
                                    .rounded(s.px(7.0))
                                    .when(index == 0, |this| this.bg(white(0.07)))
                                    .child(tab)
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .child(
                        div()
                            .w(s.px(210.0))
                            .flex_none()
                            .py(s.px(16.0))
                            .px(s.px(14.0))
                            .border_r_1()
                            .border_color(white(0.05))
                            .child(div().mb(s.px(10.0)).child(label_sized(s, "Projects", 10.0)))
                            .child(
                                mono(s, 12.5, 400.0, hex(0xeef1f7))
                                    .py(s.px(7.0))
                                    .px(s.px(10.0))
                                    .rounded(s.px(7.0))
                                    .bg(white(0.06))
                                    .child(project.to_string()),
                            )
                            .child(div().mt(s.px(18.0)).mb(s.px(10.0)).child(label_sized(s, "Sessions", 10.0)))
                            .child(
                                div()
                                    .opacity(fs_pop.clamp(0.0, 1.0))
                                    .flex()
                                    .flex_col()
                                    .py(s.px(8.0))
                                    .px(s.px(10.0))
                                    .rounded(s.px(8.0))
                                    .bg(rgba(80, 110, 220, 0.1))
                                    .border_1()
                                    .border_color(rgba(90, 125, 235, 0.3))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(s.px(8.0))
                                            .child(run_dot(s))
                                            .child(mono(s, 12.0, 400.0, hex(0xeef1f7)).child("first-session")),
                                    )
                                    .child(
                                        sans(s, 10.5, 400.0, hex(0x9aa3b3)).ml(s.px(15.0)).child(format!(
                                            "{} · live",
                                            if start_with == "terminal" { "shell" } else { start_with }
                                        )),
                                    ),
                            )
                            .when(connected, |this| {
                                this.child(
                                    sans(s, 11.0, 400.0, hex(0x7d8697))
                                        .mt(s.px(14.0))
                                        .line_height(s.px(11.0 * 1.45))
                                        .child("Connected: status and titles come from the agent."),
                                )
                            }),
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
                                    .py(s.px(14.0))
                                    .px(s.px(20.0))
                                    .border_b_1()
                                    .border_color(white(0.05))
                                    .child(agent_logo(s, &self.catalog, start_with, 18.0))
                                    .child(mono(s, 12.5, 400.0, hex(0xeef1f7)).child("first-session"))
                                    .child(phone_pill(s, "run", "running", self.opened_at, now, false))
                                    .child(
                                        mono(s, 12.5, 400.0, hex(0x8e97a8))
                                            .ml_auto()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .child(folder.to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .py(s.px(18.0))
                                    .px(s.px(22.0))
                                    .flex()
                                    .flex_col()
                                    .gap(s.px(if terminal_mode { 6.0 } else { 12.0 }))
                                    .when(terminal_mode, |this| this.bg(black(0.18)))
                                    .children(lines)
                                    .children(typing),
                            )
                            .child(
                                message_input(s)
                                    .mx(s.px(20.0))
                                    .mb(s.px(18.0))
                                    .child(
                                        sans(s, 13.5, 400.0, hex(0x8e97a8)).flex_1().child(if start_with == "terminal" {
                                            "Type a command…".to_string()
                                        } else {
                                            format!("Message {agent_name}")
                                        }),
                                    )
                                    .child(
                                        mono(s, 12.0, 400.0, hex(0xc9d0dc))
                                            .py(s.px(3.0))
                                            .px(s.px(7.0))
                                            .rounded(s.px(6.0))
                                            .border_1()
                                            .border_color(white(0.12))
                                            .child(crate::hotkey_label::terminal_overlay_hotkey_chord_label("ctrl+g")),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
