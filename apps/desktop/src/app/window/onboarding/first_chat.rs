//! Panel 3, Start chatting: open or add a project cleanly and preview the Native Chat + Prompt Queue
//! loop so getting to the first 5 prompts takes seconds.
//!
//! CDXC:Onboarding 2026-11-24 DECISION:
//! User: "pls make native chat default for all new useres and dont even offerthem anything about
//! terminal based chat which we're taking out soon". Step 3 of the streamlined Day-0 flow shows the
//! user's chosen agent ready in Native Chat, previews how Prompt Queueing lets them keep typing
//! follow-ups while the agent works, and transitions directly into Add Project (or the workspace
//! when a project already exists).
use super::FinishTarget;
use super::GpuiOnboardingWindow;
use super::interact;
use super::model::{catalog_agent_name, default_agent_id};
use super::primitives::*;
use super::stage::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement as _, Styled as _, div,
    linear_color_stop, linear_gradient,
};
use std::time::Instant;

pub(crate) struct FirstChatState {
    started: Instant,
}

impl FirstChatState {
    pub(crate) fn new(now: Instant) -> Self {
        Self { started: now }
    }
}

/// Frames of the live Native Chat + Prompt Queue demo on the right half of Step 3.
/// 0: Prompt 1 sent
/// 1: Agent starts inspecting files
/// 2: User queues Prompt 2 while the agent works
/// 3: Agent finishes Prompt 1 and immediately picks up Prompt 2
const QUEUE_DEMO_FRAMES: usize = 4;
const QUEUE_DEMO_INTERVAL_MS: u64 = 2400;

impl GpuiOnboardingWindow {
    pub(super) fn render_first_chat(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let agent_id = default_agent_id(&self.settings, &self.agents)
            .unwrap_or_else(|| self.settings.default_prompt_agent_id.clone());
        let agent_name = catalog_agent_name(&self.agents, &self.catalog, &agent_id);
        let agent_installed = self
            .agents
            .iter()
            .any(|agent| agent.agent_id == agent_id && agent.installed);
        let has_projects = self.has_projects;

        let mut out = Vec::new();
        out.push(eyebrow(s, 46.0, 114.0, None, "Step 3 · Start chatting").into_any_element());
        out.push(
            heading(
                s,
                46.0,
                140.0,
                676.0,
                48.0,
                if has_projects {
                    "Ready for your"
                } else {
                    "Open a project."
                },
                Some(if has_projects {
                    "first Native Chat."
                } else {
                    "Send your first prompt."
                }),
                false,
            )
            .into_any_element(),
        );
        out.push(
            sub(s, 46.0, 260.0, 650.0, 16.0, false)
                .child(
                    "Every agent opens straight into Native Chat. Pick a starter prompt or type your own, and queue follow-ups while your agent works.",
                )
                .into_any_element(),
        );

        let agent_pill = if agent_installed {
            detpill(s, "Native Chat ready", DetTone::On, DETPILL_DEFAULT)
        } else {
            detpill(s, "Install anytime", DetTone::Wait, DETPILL_DEFAULT)
        };
        let step_row = |done: bool,
                        step_num: &str,
                        title: &str,
                        detail: String,
                        trailing: Option<AnyElement>,
                        divided: bool| {
            div()
                .flex()
                .items_center()
                .gap(s.px(16.0))
                .py(s.px(18.0))
                .px(s.px(22.0))
                .when(divided, |this| this.border_t_1().border_color(white(0.06)))
                .child(
                    div()
                        .size(s.px(28.0))
                        .flex_none()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(if done {
                            rgba(59, 227, 162, 0.45)
                        } else {
                            rgba(110, 150, 255, 0.4)
                        })
                        .bg(if done {
                            rgba(59, 227, 162, 0.14)
                        } else {
                            rgba(90, 125, 235, 0.12)
                        })
                        .child(if done {
                            icon(s, "check", 14.0, 2.0, hex(0x3be3a2)).into_any_element()
                        } else {
                            mono(s, 12.0, 500.0, hex(0x9db4ff))
                                .child(step_num.to_string())
                                .into_any_element()
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(nm(s, 16.5, 500.0).child(title.to_string()))
                        .child(ss(s, 13.0).mt(s.px(3.0)).child(detail)),
                )
                .children(trailing)
        };

        let checklist = glass(s)
            .absolute()
            .left(s.px(46.0))
            .top(s.px(344.0))
            .w(s.px(670.0))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(step_row(
                agent_installed,
                "1",
                &format!("Default agent: {agent_name}"),
                if agent_installed {
                    "Configured for Native Chat with your preferences saved.".to_string()
                } else {
                    "You can install an agent CLI anytime in Settings → Agents.".to_string()
                },
                Some(agent_pill.into_any_element()),
                false,
            ))
            .child(step_row(
                has_projects,
                "2",
                if has_projects {
                    "Project folder connected"
                } else {
                    "Open or add a project"
                },
                if has_projects {
                    "Your workspace is ready to start a new Native Chat session.".to_string()
                } else {
                    "Pick any local folder or clone a repository to start your first chat."
                        .to_string()
                },
                None,
                true,
            ))
            .child(step_row(
                false,
                "3",
                "Send a starter prompt and queue the next",
                "Press Enter while your agent is working to queue follow-up prompts in order."
                    .to_string(),
                None,
                true,
            ));
        out.push(checklist.into_any_element());

        // CDXC:Onboarding 2026-11-11 DECISION: mention anonymous usage analytics once in setup.
        out.push(
            abs(s, 46.0, 744.0, Some(670.0), None)
                .child(
                    sans(s, 13.0, 400.0, hex(0x8e97a8))
                        .child("Ghostex sends anonymous usage analytics to get better."),
                )
                .into_any_element(),
        );

        let cta_label = if has_projects {
            "Open Ghostex"
        } else {
            "Add a project"
        };
        out.push(
            foot_actions(s, 3)
                .child(self.control(
                    s,
                    cta(
                        s,
                        "first-chat-finish",
                        cta_label,
                        true,
                        true,
                        false,
                        CtaSize::Foot,
                    ),
                    "first-chat-finish",
                    interact::Ring::new(10.0, 1.0),
                    interact::Keys::EnterSpace,
                    cx,
                    |this, _, cx| this.finish(FinishTarget::None, cx),
                ))
                .into_any_element(),
        );

        out.push(self.render_first_chat_preview(s, now, &agent_id, &agent_name));
        out
    }

    /// Right-column animated preview of a Native Chat session with starter prompts and Prompt Queue.
    fn render_first_chat_preview(
        &self,
        s: S,
        now: Instant,
        agent_id: &str,
        agent_name: &str,
    ) -> AnyElement {
        let state = cycle(
            self.first_chat.started,
            now,
            QUEUE_DEMO_FRAMES,
            QUEUE_DEMO_INTERVAL_MS,
            1,
        );
        let frame = state.frame;
        let working = frame == 1 || frame == 2;
        let status_label = if working { "Working" } else { "Ready" };

        let mut transcript = div()
            .flex_1()
            .py(s.px(22.0))
            .px(s.px(24.0))
            .flex()
            .flex_col()
            .gap(s.px(14.0));

        transcript = transcript.child(chat_bubble(
            s,
            "me",
            "Give me a quick architecture overview of this project.",
            agent_name,
            None,
            now,
        ));

        if frame >= 1 {
            let reply = if frame >= 3 {
                "Mapped 3 core modules: API routes in src/server, shared state in src/store, and UI views in src/app. Starting your queued prompt next."
            } else {
                "Reading src/server and src/store to map the main entry points…"
            };
            transcript = transcript.child(chat_bubble(s, "bot", reply, agent_name, None, now));
        }

        if frame >= 3 {
            let t = entrance(state.frame_started, now, 320);
            transcript = transcript.child(div().opacity(t).child(chat_bubble(
                s,
                "me",
                "Review recent git changes and flag any edge cases.",
                agent_name,
                None,
                now,
            )));
        }

        let queued_banner = (frame == 2).then(|| {
            let t = entrance(state.frame_started, now, 280);
            div()
                .mx(s.px(20.0))
                .mb(s.px(10.0))
                .py(s.px(10.0))
                .px(s.px(14.0))
                .rounded(s.px(10.0))
                .border_1()
                .border_color(rgba(110, 150, 255, 0.38))
                .bg(rgba(60, 90, 200, 0.14))
                .opacity(t)
                .flex()
                .items_center()
                .gap(s.px(10.0))
                .child(detpill(
                    s,
                    "Queued next",
                    DetTone::Detected,
                    DETPILL_DEFAULT,
                ))
                .child(
                    sans(s, 13.5, 400.0, hex(0xeef1f7))
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child("Review recent git changes and flag any edge cases."),
                )
        });

        let starter_row = div()
            .mx(s.px(20.0))
            .mb(s.px(12.0))
            .flex()
            .flex_wrap()
            .gap(s.px(8.0))
            .children(
                [
                    "Summarize this codebase",
                    "Review recent changes",
                    "Find and fix a bug",
                ]
                .into_iter()
                .map(|chip| {
                    sans(s, 12.0, 500.0, hex(0xc9d0dc))
                        .py(s.px(6.0))
                        .px(s.px(11.0))
                        .rounded(s.px(999.0))
                        .border_1()
                        .border_color(white(0.09))
                        .bg(white(0.03))
                        .child(chip)
                }),
            );

        let composer_hint = if working {
            "Type your next prompt and press Enter to queue…".to_string()
        } else {
            format!("Message {agent_name}…")
        };

        glass(s)
            .absolute()
            .left(s.px(806.0))
            .top(s.px(118.0))
            .w(s.px(814.0))
            .h(s.px(716.0))
            .border_color(rgba(90, 125, 235, 0.25))
            .bg(linear_gradient(
                180.0,
                linear_color_stop(rgba(10, 13, 24, 0.88), 0.0),
                linear_color_stop(rgba(6, 8, 16, 0.92), 1.0),
            ))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                pane_header(s)
                    .child(agent_logo(s, &self.catalog, agent_id, 20.0))
                    .child(
                        sans(s, 15.0, 500.0, hex(0xeef1f7))
                            .flex_1()
                            .child(format!("{agent_name} · Native Chat")),
                    )
                    .child(phone_pill(
                        s,
                        "run",
                        status_label,
                        self.opened_at,
                        now,
                        false,
                    )),
            )
            .child(transcript)
            .children(queued_banner)
            .child(starter_row)
            .child(
                message_input(s)
                    .mx(s.px(20.0))
                    .mb(s.px(20.0))
                    .child(sans(s, 15.0, 400.0, hex(0x8e97a8)).child(composer_hint)),
            )
            .into_any_element()
    }
}

fn pane_header(s: S) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(s.px(12.0))
        .h(s.px(48.0))
        .px(s.px(16.0))
}

fn message_input(s: S) -> Div {
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

fn chat_bubble(
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

fn phone_pill(
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
