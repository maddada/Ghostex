//! Native GPUI New bot dialog: a GBot's name, its instructions and the agent it runs on.
//!
//! CDXC:GBots 2026-11-06 DECISION:
//! User (via coordinator): "creating a bot (name, instructions, agent)", the agent being the user's own Claude Code or Codex. The agent is a segmented pick (the shared control), Claude Code first.
//! SEE-ALSO: apps/desktop/src/app/new_gbot_modal_lifecycle.rs (open), apps/desktop/src/app/gx_store/create/bot.rs (`gx_store_create_gbot`, the gxserver call), server/src/gbots/ (what a new bot gets).
use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, Styled as _,
    Subscription, Window, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use gpui_component::v_flex;
use std::rc::Rc;

pub(crate) const NEW_GBOT_MODAL_WIDTH: f32 = 540.0;
pub(crate) const NEW_GBOT_MODAL_INITIAL_HEIGHT: f32 = 520.0;

const TITLE: &str = "New bot";
const DESCRIPTION: &str = "A bot that runs on your own agent. It can start and steer agent sessions on any of your computers.";
const FIELD_NAME: &str = "Name";
const NAME_PLACEHOLDER: &str = "e.g. Release Captain";
const FIELD_INSTRUCTIONS: &str = "Instructions";
const INSTRUCTIONS_PLACEHOLDER: &str =
    "What this bot does, e.g. Keep an eye on my release threads and nudge any that get stuck.";
const FIELD_AGENT: &str = "Runs on";
const CANCEL: &str = "Cancel";
const CREATE: &str = "Create bot";

/// The agents a bot runs on: gxserver's id and the label.
const AGENTS: [(&str, &str); 2] = [("claude", "Claude Code"), ("codex", "Codex")];

pub(crate) enum NewGbotModalCommand {
    Create {
        name: String,
        instructions: String,
        agent: String,
    },
    Cancel,
}

pub(crate) type NewGbotModalHost = Rc<dyn Fn(NewGbotModalCommand, &mut App)>;

pub(crate) struct NewGbotModalConfig {
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiNewGbotModalWindow {
    host: NewGbotModalHost,
    palette: ModalPalette,
    name_input: Entity<InputState>,
    instructions_input: Entity<TextareaState>,
    name: String,
    agent: usize,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiNewGbotModalWindow {
    pub(crate) fn new(
        config: NewGbotModalConfig,
        host: NewGbotModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(NAME_PLACEHOLDER));
        let instructions_input =
            cx.new(|cx| TextareaState::new(window, cx).placeholder(INSTRUCTIONS_PLACEHOLDER));
        let name_subscription = cx.subscribe_in(
            &name_input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.name = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        );
        name_input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            host,
            palette: config.palette,
            name_input,
            instructions_input,
            name: String::new(),
            agent: 0,
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![name_subscription],
        }
    }

    fn close_window_and_send(
        &mut self,
        command: NewGbotModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(NewGbotModalCommand::Cancel, window, cx);
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let instructions = self.instructions_input.read(cx).value().trim().to_string();
        let agent = AGENTS[self.agent.min(AGENTS.len() - 1)].0.to_string();
        self.close_window_and_send(
            NewGbotModalCommand::Create {
                name,
                instructions,
                agent,
            },
            window,
            cx,
        );
    }

    /// Enter in the name field creates; in the instructions it is a new line, and Cmd/Ctrl+Enter
    /// creates.
    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        let in_name = self.name_input.read(cx).focus_handle(cx).is_focused(window);
        if !action.secondary && !in_name {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.create(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if event.keystroke.modifiers.secondary(cx) && !event.is_held => {
                self.create(window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let items: Vec<ModalSegmentedItem> = AGENTS
            .iter()
            .map(|(_, label)| ModalSegmentedItem { icon: None, label })
            .collect();
        v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap(px(16.0))
            .child(
                v_flex()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_NAME))
                    .child(modal_text_input(&p, &self.name_input, false, window, cx)),
            )
            .child(
                v_flex()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_AGENT))
                    .child(modal_segmented_control(
                        &p,
                        "new-gbot-agent",
                        &items,
                        self.agent,
                        |this: &mut Self, index, _window, cx| {
                            this.agent = index;
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(8.0))
                    .line_height(px(20.3))
                    .child(modal_section_title(&p, FIELD_INSTRUCTIONS))
                    .child(modal_text_area(
                        &p,
                        &self.instructions_input,
                        None,
                        false,
                        window,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let cancel = modal_action_button(
            &p,
            "new-gbot-cancel",
            CANCEL,
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let create = modal_action_button(
            &p,
            "new-gbot-create",
            CREATE,
            None,
            ModalButtonTone::Primary,
            self.name.trim().is_empty(),
            |this, window, cx| this.create(window, cx),
            cx,
        );
        modal_footer(vec![cancel, create])
    }
}

impl Render for GpuiNewGbotModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = vec![
            modal_header(&p, TITLE, Some(DESCRIPTION)),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-new-gbot-modal",
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
    }
}

impl ModalCornerClose for GpuiNewGbotModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
