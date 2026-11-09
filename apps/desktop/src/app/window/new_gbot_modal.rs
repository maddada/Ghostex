//! Native GPUI New GBot dialog: a GBot's name, its role, its instructions and the agent it runs
//! on. Its look (icon and colour) and the rest are set in GBot Settings afterwards.
//!
//! CDXC:GBots 2026-11-06 DECISION:
//! User (via coordinator): "creating a bot (name, instructions, agent)", the agent being the user's own Claude Code or Codex. The agent is a segmented pick (the shared control), Claude Code first.
//! CDXC:GBots 2026-11-09 DECISION:
//! Plan piece 13: New GBot offers the built-in presets and the ones you kept (server/src/gbots/team_file/presets.rs) under Start from. Picking one fills the name, role and instructions, which you can still change; the preset's look, description and starter notes come with it when you create. Blank clears what a preset filled.
//! SEE-ALSO: apps/desktop/src/app/new_gbot_modal_lifecycle.rs (open), apps/desktop/src/app/gx_store/create/bot.rs (`gx_store_create_gbot`, the gxserver call), server/src/gbots/ (what a new bot gets).
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px, rgb, svg,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;
use std::rc::Rc;

pub(crate) const NEW_GBOT_MODAL_WIDTH: f32 = 540.0;
pub(crate) const NEW_GBOT_MODAL_INITIAL_HEIGHT: f32 = 670.0;

const TITLE: &str = "New GBot";
const DESCRIPTION: &str = "A GBot keeps its name, instructions and memory in every chat. It runs on your own agent and sits under Chief of Staff in Main Hub.";
const FIELD_NAME: &str = "Name";
const NAME_PLACEHOLDER: &str = "e.g. Researcher";
const FIELD_ROLE: &str = "Role";
const ROLE_PLACEHOLDER: &str = "e.g. Finds and compares options";
const FIELD_INSTRUCTIONS: &str = "Instructions";
const INSTRUCTIONS_PLACEHOLDER: &str =
    "What this GBot does, e.g. Compare flights and hotels and bring me a short list with prices.";
const FIELD_AGENT: &str = "Runs on";
const FIELD_START_FROM: &str = "Start from";
const BLANK: &str = "Blank";
const CANCEL: &str = "Cancel";
const CREATE: &str = "Create GBot";

/// The agents a bot runs on: gxserver's id and the label.
const AGENTS: [(&str, &str); 2] = [("claude", "Claude Code"), ("codex", "Codex")];

pub(crate) enum NewGbotModalCommand {
    Create {
        name: String,
        title: String,
        instructions: String,
        agent: String,
        /// The preset it starts from (`/api/gbots` `create` `preset`).
        preset: Option<String>,
    },
    Cancel,
}

pub(crate) type NewGbotModalHost = Rc<dyn Fn(NewGbotModalCommand, &mut App)>;

pub(crate) struct NewGbotModalConfig {
    pub(crate) palette: ModalPalette,
    /// `/api/gbots` `presets`' list; empty when it couldn't be read.
    pub(crate) presets: Vec<Value>,
}

pub(crate) struct GpuiNewGbotModalWindow {
    host: NewGbotModalHost,
    palette: ModalPalette,
    name_input: Entity<InputState>,
    title_input: Entity<InputState>,
    instructions_input: Entity<TextareaState>,
    name: String,
    agent: usize,
    presets: Vec<Value>,
    /// The picked preset's index in `presets`.
    preset: Option<usize>,
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
        let title_input = cx.new(|cx| InputState::new(window, cx).placeholder(ROLE_PLACEHOLDER));
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
            title_input,
            instructions_input,
            name: String::new(),
            agent: 0,
            presets: config.presets,
            preset: None,
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
        let title = self.title_input.read(cx).value().trim().to_string();
        let instructions = self.instructions_input.read(cx).value().trim().to_string();
        let agent = AGENTS[self.agent.min(AGENTS.len() - 1)].0.to_string();
        let preset = self
            .preset
            .and_then(|index| self.presets.get(index))
            .and_then(|preset| preset["presetId"].as_str())
            .map(str::to_string);
        self.close_window_and_send(
            NewGbotModalCommand::Create {
                name,
                title,
                instructions,
                agent,
                preset,
            },
            window,
            cx,
        );
    }

    /// Enter in the name or role field creates; in the instructions it is a new line, and Cmd/Ctrl+Enter
    /// creates.
    fn on_enter_action(&mut self, action: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        let in_name = self.name_input.read(cx).focus_handle(cx).is_focused(window)
            || self
                .title_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
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

    /// Fills the fields from preset `index` (`None`: Blank empties them).
    fn pick_preset(&mut self, index: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let field = |key: &str| {
            index
                .and_then(|index| self.presets.get(index))
                .and_then(|preset| preset[key].as_str())
                .unwrap_or_default()
                .to_string()
        };
        let (name, title, instructions) = (field("name"), field("title"), field("instructions"));
        self.name_input
            .update(cx, |input, cx| input.set_value(name.clone(), window, cx));
        self.title_input
            .update(cx, |input, cx| input.set_value(title, window, cx));
        self.instructions_input
            .update(cx, |input, cx| input.set_value(instructions, window, cx));
        self.name = name;
        self.preset = index;
        cx.notify();
    }

    /// Blank and each preset, as chips that wrap.
    fn render_start_from(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let chip = |id: usize, label: String, icon: Option<(String, u32)>, picked: bool| {
            h_flex()
                .id(("new-gbot-preset", id))
                .gap(px(6.0))
                .px(px(10.0))
                .h(px(28.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(if picked { p.foreground } else { p.hairline })
                .bg(if picked { p.raised_hover } else { p.raised })
                .hover(|chip| chip.bg(p.raised_hover))
                .cursor_pointer()
                .text_size(px(12.5))
                .children(icon.map(|(icon, hex)| {
                    svg()
                        .size(px(14.0))
                        .path(format!("titlebar/{icon}.svg"))
                        .text_color(rgb(hex))
                }))
                .child(label)
        };
        let mut chips = vec![
            chip(0, BLANK.to_string(), None, self.preset.is_none())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.pick_preset(None, window, cx)
                }))
                .into_any_element(),
        ];
        for (index, preset) in self.presets.iter().enumerate() {
            let hex = preset["colorHex"]
                .as_str()
                .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
                .unwrap_or(0x9fb0ff);
            let icon = preset["icon"].as_str().unwrap_or("robot").to_string();
            chips.push(
                chip(
                    index + 1,
                    preset["name"].as_str().unwrap_or_default().to_string(),
                    Some((icon, hex)),
                    self.preset == Some(index),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.pick_preset(Some(index), window, cx)
                }))
                .into_any_element(),
            );
        }
        let description = self
            .preset
            .and_then(|index| self.presets.get(index))
            .and_then(|preset| preset["description"].as_str())
            .filter(|description| !description.is_empty())
            .map(str::to_string);
        v_flex()
            .gap(px(8.0))
            .child(modal_section_title(&p, FIELD_START_FROM))
            .child(div().flex().flex_wrap().gap(px(6.0)).children(chips))
            .when_some(description, |column, description| {
                column.child(modal_hint(&p, description))
            })
            .into_any_element()
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
            .when(!self.presets.is_empty(), |body| {
                body.child(self.render_start_from(cx))
            })
            .child(
                v_flex()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_NAME))
                    .child(modal_text_input(&p, &self.name_input, false, window, cx)),
            )
            .child(
                v_flex()
                    .gap(px(8.0))
                    .child(modal_section_title(&p, FIELD_ROLE))
                    .child(modal_text_input(&p, &self.title_input, false, window, cx)),
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
