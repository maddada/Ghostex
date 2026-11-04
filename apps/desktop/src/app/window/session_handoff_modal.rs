//! The message a session dropped on another project opens: where it goes, which agent continues,
//! and whether the source closes.
//!
//! CDXC:Sidebar 2026-10-31 DECISION:
//! User: "I can handover when you drag", "after a message to confirm". User (Q862): it "should say 'Handoff to <Project name>' not handoff to agent name", as the short message with Continue with kept. User (Q864): "put a toggle that's enabled by default that says 'close source session'". User (Q866): "stop-first": a working session's message says its turn will be stopped, and its button says so too.
//! The host answers Confirm, not this window: an agent that started working after the message opened gets the warning first instead of being stopped by a click that never saw it.
//! SEE-ALSO: apps/desktop/src/app/session_handoff_modal_lifecycle.rs (open and answers), apps/desktop/src/app/gx_store/git/session_handoff.rs (the handoff), apps/desktop/src/bin/native_modal_demo.rs (standalone preview).
use super::export_transcript_modal::ExportTranscriptAgent;
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window,
    px,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const SESSION_HANDOFF_MODAL_WIDTH: f32 = 440.0;
/// First-frame height only; the window is resized to the measured layout on the first prepaint.
pub(crate) const SESSION_HANDOFF_MODAL_INITIAL_HEIGHT: f32 = 300.0;

const CONTINUE_WITH: &str = "Continue with";
const SELECT_AGENT_PLACEHOLDER: &str = "Select agent";
const CLOSE_SOURCE: &str = "Close source session";
const CLOSE_SOURCE_DESCRIPTION: &str = "Closes once the new session has started.";

/// `Handoff to <project>`: the message's title and its button.
pub(crate) fn session_handoff_title(project: &str) -> String {
    format!("Handoff to {project}")
}

fn session_handoff_description(project: &str) -> String {
    format!(
        "A new session starts in {project} with this conversation attached as a file, ready for your first message."
    )
}

fn session_handoff_stop_warning(agent: &str) -> String {
    format!(
        "{agent} is still working. Handing off stops this turn first, and anything it was in the middle of stays unfinished."
    )
}

pub(crate) enum SessionHandoffModalCommand {
    /// The button or Enter. The window stays open: the host closes it, or warns first.
    Confirm {
        agent_id: String,
        close_source: bool,
        /// The message said the turn will be stopped.
        warned: bool,
    },
    /// Cancel, Escape or the corner close. The dialog removes its own window before sending this.
    Cancel,
}

pub(crate) type SessionHandoffModalHost = Rc<dyn Fn(SessionHandoffModalCommand, &mut App)>;

pub(crate) struct SessionHandoffModalConfig {
    pub(crate) project_title: String,
    pub(crate) agents: Vec<ExportTranscriptAgent>,
    /// The source session's own agent, preselected.
    pub(crate) default_agent_id: Option<String>,
    /// The name of the agent that is working in the source session, when it is.
    pub(crate) working_agent: Option<String>,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiSessionHandoffModalWindow {
    host: SessionHandoffModalHost,
    palette: ModalPalette,
    project_title: String,
    agents: Vec<ExportTranscriptAgent>,
    default_agent_id: Option<String>,
    selected_agent_id: Option<String>,
    working_agent: Option<String>,
    close_source: bool,
    agent_select: ModalSelect,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiSessionHandoffModalWindow {
    pub(crate) fn new(
        config: SessionHandoffModalConfig,
        host: SessionHandoffModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            project_title: config.project_title,
            agents: config.agents,
            default_agent_id: config.default_agent_id,
            selected_agent_id: None,
            working_agent: config.working_agent,
            close_source: true,
            agent_select: ModalSelect::new(),
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    /// A refreshed agent list; the pick stays when it is still offered.
    pub(crate) fn set_agents(
        &mut self,
        agents: Vec<ExportTranscriptAgent>,
        cx: &mut Context<Self>,
    ) {
        self.agents = agents;
        cx.notify();
    }

    /// The agent started working after this message opened.
    pub(crate) fn set_working_agent(&mut self, agent: String, cx: &mut Context<Self>) {
        self.working_agent = Some(agent);
        self.fit.refit();
        cx.notify();
    }

    fn effective_agent(&self) -> Option<&ExportTranscriptAgent> {
        let find = |id: &str| self.agents.iter().find(|agent| agent.agent_id == id);
        self.selected_agent_id
            .as_deref()
            .and_then(find)
            .or_else(|| self.default_agent_id.as_deref().and_then(find))
            .or_else(|| self.agents.first())
    }

    fn effective_agent_index(&self) -> Option<usize> {
        let agent = self.effective_agent()?;
        self.agents
            .iter()
            .position(|candidate| candidate.agent_id == agent.agent_id)
    }

    fn toggle_agent_menu(&mut self, cx: &mut Context<Self>) {
        if self.agents.is_empty() {
            return;
        }
        self.agent_select.toggle(self.effective_agent_index());
        cx.notify();
    }

    fn choose_agent(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(agent) = self.agents.get(index) {
            self.selected_agent_id = Some(agent.agent_id.clone());
        }
        self.agent_select.close();
        cx.notify();
    }

    fn toggle_close_source(&mut self, cx: &mut Context<Self>) {
        self.close_source = !self.close_source;
        self.agent_select.close();
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(SessionHandoffModalCommand::Cancel, cx);
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(agent) = self.effective_agent() else {
            return;
        };
        let command = SessionHandoffModalCommand::Confirm {
            agent_id: agent.agent_id.clone(),
            close_source: self.close_source,
            warned: self.working_agent.is_some(),
        };
        self.agent_select.close();
        (self.host)(command, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        match self.agent_select.handle_key(key, self.agents.len()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Choose(index) => {
                self.choose_agent(index, cx);
                cx.stop_propagation();
                return;
            }
            ModalSelectKey::Ignored => {}
        }
        match key {
            "enter" => self.confirm(cx),
            "escape" => self.cancel(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let value = self.effective_agent().map(|agent| agent.name.clone());
        let close_source = self.close_source;
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(
                h_flex()
                    .min_h(px(MODAL_CONTROL_HEIGHT))
                    .items_center()
                    .gap(px(10.0))
                    .on_children_prepainted(capture_child_bounds(
                        self.agent_select.trigger_bounds.clone(),
                        1,
                    ))
                    .child(modal_section_title(&p, CONTINUE_WITH).flex_shrink_0())
                    .child(modal_select_trigger(
                        &p,
                        &self.agent_select,
                        "session-handoff-agent-select",
                        value,
                        SELECT_AGENT_PLACEHOLDER,
                        false,
                        |this: &mut Self, _window, cx| this.toggle_agent_menu(cx),
                        cx,
                    )),
            )
            .child(
                modal_panel(&p).child(
                    modal_panel_row(
                        &p,
                        CLOSE_SOURCE,
                        Some(CLOSE_SOURCE_DESCRIPTION),
                        modal_switch(&p, close_source, false),
                        false,
                    )
                    .id("session-handoff-close-source")
                    .cursor_pointer()
                    .on_click(cx.listener(
                        |this, _: &ClickEvent, _window, cx| {
                            this.toggle_close_source(cx);
                        },
                    )),
                ),
            )
            .when_some(self.working_agent.as_deref(), |body, agent| {
                body.child(
                    modal_panel(&p)
                        .px(px(12.0))
                        .py(px(10.0))
                        .child(modal_hint(&p, session_handoff_stop_warning(agent))),
                )
            })
            .into_any_element()
    }
}

impl Render for GpuiSessionHandoffModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let names: Vec<String> = self.agents.iter().map(|agent| agent.name.clone()).collect();
        let menu = modal_select_menu(
            &p,
            &self.agent_select,
            "session-handoff-agent-menu",
            &names,
            self.effective_agent_index(),
            |this: &mut Self, index, _window, cx| this.choose_agent(index, cx),
            |this: &mut Self, _window, cx| {
                this.agent_select.close();
                cx.notify();
            },
            window,
            cx,
        );
        let title = session_handoff_title(&self.project_title);
        let description = session_handoff_description(&self.project_title);
        let (label, tone) = match self.working_agent {
            Some(_) => ("Stop and handoff".to_string(), ModalButtonTone::Danger),
            None => (title.clone(), ModalButtonTone::Primary),
        };
        let footer = modal_footer(vec![
            modal_action_button(
                &p,
                "session-handoff-cancel",
                "Cancel",
                None,
                ModalButtonTone::Neutral,
                false,
                |this: &mut Self, window, cx| this.cancel(window, cx),
                cx,
            ),
            modal_action_button(
                &p,
                "session-handoff-confirm",
                label,
                None,
                tone,
                self.effective_agent().is_none(),
                |this: &mut Self, _window, cx| this.confirm(cx),
                cx,
            ),
        ]);
        let body = self.render_body(cx);
        modal_shell(
            &p,
            "ghostex-gpui-session-handoff-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            vec![modal_header(&p, title, Some(description)), body],
            footer,
            menu,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiSessionHandoffModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_and_button_name_the_project_not_the_agent() {
        assert_eq!(session_handoff_title("acme-web"), "Handoff to acme-web");
        assert!(session_handoff_description("acme-web").contains("in acme-web"));
    }

    #[test]
    fn a_working_session_says_its_turn_will_be_stopped() {
        let warning = session_handoff_stop_warning("Claude");
        assert!(warning.starts_with("Claude is still working."));
        assert!(warning.contains("stops this turn first"));
    }
}
