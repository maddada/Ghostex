use super::images::FEEDBACK_MAX_IMAGES;
use super::state::{FeedbackStep, GpuiFeedbackModalWindow, github_username};
use crate::app::window::native_modal_kit::*;
use gpui::{
    AnyElement, ClickEvent, Context, ExternalPaths, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::v_flex;
use std::rc::Rc;

impl GpuiFeedbackModalWindow {
    fn render_compose(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let paste: ModalPasteHandler = Rc::new(Self::paste_handler(cx.entity().downgrade()));
        let paste_chord = crate::hotkey_label::terminal_overlay_hotkey_chord_label("cmd+v");
        // The web build's GPUI platform delivers no file drops, so the page only offers paste.
        let paste_hint = if cfg!(target_family = "wasm") {
            format!(
                "You can paste screenshots into this box with {paste_chord}: up to \
                 {FEEDBACK_MAX_IMAGES} PNG, JPEG or WebP images."
            )
        } else {
            format!(
                "You can paste screenshots into this box with {paste_chord} or drop them here: up \
                 to {FEEDBACK_MAX_IMAGES} PNG, JPEG or WebP images."
            )
        };
        let message = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(8.0))
            .child(modal_section_title(&p, "Feedback"))
            .child(modal_text_area_with_paste(
                &p,
                &self.message_input,
                None,
                self.busy,
                Some(paste),
                window,
                cx,
            ))
            .children(self.render_images(cx))
            .child(modal_hint(&p, paste_hint))
            .children(self.image_error.clone().map(|error| modal_error(&p, error)))
            .into_any_element();
        let github = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap(px(8.0))
            .child(modal_section_title(&p, "GitHub username"))
            .child(modal_text_input(
                &p,
                &self.github_input,
                self.busy,
                window,
                cx,
            ))
            .child(match github_username(&self.github) {
                Ok(_) => modal_hint(
                    &p,
                    "Required. The issue mentions you, so GitHub lets you know when it's fixed.",
                )
                .into_any_element(),
                Err(()) => modal_error(
                    &p,
                    "That isn't a GitHub username: letters, digits and single hyphens, up to 39 \
                     characters.",
                ),
            })
            .into_any_element();
        let busy = self.busy;
        let agent_hint = match &self.agent_machine {
            _ if busy && self.collect_with_agent => "Starting the agent and handing it your \
                 report. This can take up to a minute and a half; if it can't start, this says \
                 why and you can send the report without it."
                .to_string(),
            Some(machine) => format!(
                "An agent on {}, where this session runs, reads its Ghostex logs and settings, \
                 looks into the problem without changing anything, and shows you the issue before \
                 it posts it.",
                machine.name
            ),
            None => "An agent reads this computer's Ghostex logs and settings, looks into the \
                     problem without changing anything, and shows you the issue before it posts \
                     it."
            .to_string(),
        };
        let agent = modal_panel(&p)
            .child(
                modal_panel_row(
                    &p,
                    "Collect more data with an agent",
                    Some(agent_hint),
                    modal_switch(&p, self.collect_with_agent, busy),
                    false,
                )
                .id("feedback-collect-with-agent")
                .role(gpui::Role::Switch)
                .aria_toggled(a11y_toggled(self.collect_with_agent))
                .aria_label("Collect more data with an agent")
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.toggle_collect_with_agent(cx)
                })),
            )
            .into_any_element();
        let subtitle = if self.collect_with_agent {
            "Tell us what is broken, confusing or missing. Start Agent opens a session where an \
             agent gathers the details and drafts the issue for the Ghostex GitHub."
        } else {
            "Tell us what is broken, confusing or missing. You review the issue before it is \
             posted publicly on the Ghostex GitHub."
        };
        vec![
            modal_header(&p, "Send Bug / Feedback", Some(subtitle)),
            github,
            message,
            agent,
        ]
    }

    fn render_review(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette;
        let screenshots = match self.images.len() {
            0 => String::new(),
            1 => "1 screenshot and ".to_string(),
            count => format!("{count} screenshots and "),
        };
        let added = format!(
            "Added when it is posted: {screenshots}\u{201c}{}\u{201d}.",
            self.footer
        );
        let fields = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(8.0))
            .child(modal_section_title(&p, "Title"))
            .child(modal_text_input(
                &p,
                &self.title_input,
                self.busy,
                window,
                cx,
            ))
            .child(div().h(px(4.0)))
            .child(modal_section_title(&p, "Description"))
            .child(modal_text_area(
                &p,
                &self.body_input,
                None,
                self.busy,
                window,
                cx,
            ))
            .child(modal_hint(&p, added))
            .children(self.error.clone().map(|error| modal_error(&p, error)))
            .children(self.render_offline_offer(cx))
            .into_any_element();
        vec![
            modal_header(
                &p,
                "Review Issue",
                Some(
                    "This is exactly what will be posted publicly on the Ghostex GitHub. Edit \
                     anything before you send it.",
                ),
            ),
            fields,
        ]
    }

    /// The way to report while gxserver does not answer (CDXC:Feedback 2026-10-22 DECISION in
    /// offline_issue.rs), shown under the error it answers.
    fn render_offline_offer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        if let Some(notice) = self.offline_notice.clone() {
            return Some(modal_hint(&p, notice).into_any_element());
        }
        if !self.offline_offer {
            return None;
        }
        Some(
            v_flex()
                .w_full()
                .flex_shrink_0()
                .gap(px(8.0))
                .child(modal_hint(
                    &p,
                    "Ghostex's background service isn't answering, so the report can't go \
                     through it. You can open the issue on GitHub in your browser instead, with \
                     your message and this computer's startup diagnostics filled in. Your user \
                     name, home folder, computer name, addresses and tokens are removed first.",
                ))
                .child(div().flex().child(modal_action_button(
                    &p,
                    "feedback-open-in-browser",
                    "Open a GitHub issue in your browser",
                    self.busy.then(|| modal_spinner(p.primary_foreground)),
                    ModalButtonTone::Primary,
                    self.busy,
                    |this, _window, cx| this.open_offline_issue(cx),
                    cx,
                )))
                .into_any_element(),
        )
    }

    fn render_sent(&self, number: u64, url: &str) -> Vec<AnyElement> {
        let p = self.palette;
        vec![
            modal_header(
                &p,
                "Thanks for the feedback",
                Some(format!(
                    "Issue #{number} is on GitHub now. You can follow it there."
                )),
            ),
            modal_hint(&p, url.to_string()).into_any_element(),
        ]
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let spinner = || self.busy.then(|| modal_spinner(p.primary_foreground));
        let buttons = match &self.step {
            FeedbackStep::Compose => vec![
                modal_action_button(
                    &p,
                    "feedback-cancel",
                    "Cancel",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this, window, cx| this.close(window, cx),
                    cx,
                ),
                if self.collect_with_agent {
                    modal_action_button(
                        &p,
                        "feedback-start-agent",
                        "Start Agent",
                        spinner(),
                        ModalButtonTone::Primary,
                        !self.can_review(),
                        |this, window, cx| this.start_agent(window, cx),
                        cx,
                    )
                } else {
                    modal_action_button(
                        &p,
                        "feedback-review",
                        "Review",
                        spinner(),
                        ModalButtonTone::Primary,
                        !self.can_review(),
                        |this, window, cx| this.review(window, cx),
                        cx,
                    )
                },
            ],
            FeedbackStep::Review => vec![
                modal_action_button(
                    &p,
                    "feedback-back",
                    "Back",
                    None,
                    ModalButtonTone::Neutral,
                    self.busy,
                    |this, window, cx| this.back(window, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "feedback-send",
                    "Send",
                    spinner(),
                    ModalButtonTone::Primary,
                    !self.can_send(),
                    |this, window, cx| this.send(window, cx),
                    cx,
                ),
            ],
            FeedbackStep::Sent { .. } => vec![
                modal_action_button(
                    &p,
                    "feedback-done",
                    "Done",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this, window, cx| this.close(window, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "feedback-open-issue",
                    "Open Issue",
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this, _window, cx| this.open_issue(cx),
                    cx,
                ),
            ],
        };
        modal_footer(buttons)
    }
}

impl Render for GpuiFeedbackModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = match &self.step {
            FeedbackStep::Compose => {
                let mut content = self.render_compose(window, cx);
                if let Some(error) = self.error.clone() {
                    content.push(modal_error(&p, error));
                }
                content.extend(self.render_offline_offer(cx));
                content
            }
            FeedbackStep::Review => self.render_review(window, cx),
            FeedbackStep::Sent { number, url } => self.render_sent(*number, url),
        };
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-feedback-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
        .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
            this.drop_paths(paths, cx);
            cx.stop_propagation();
        }))
    }
}
