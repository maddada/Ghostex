//! A call with a GBot on the desktop: Call on the GBot's row and, on one of its chats, the header
//! arrow's "Call <GBot>" row. The call is the Voice agent's panel and link (`conversation.rs`)
//! opened with `gbot=<botId>`; gxserver speaks as the GBot in its voice and hands what you say to
//! its chat (server/src/voice_agent/gbot_call/). Protocol: the contract's "Calls with a GBot"
//! section (docs/2026-10-20/voice-mode/voice-agent-client-contract.md).
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (M14 brief): "start a call with a GBot (from its row's menu, its chat, and the phone)", and "GBots off means no call buttons anywhere". Call shows on a GBot's row and the arrow offers it on a GBot's chat only while GBots and the Voice agent are both on; a call with a GBot stays with it (the panel has no All sessions), and the panel's voice list sets that GBot's voice instead of the Voice agent's.

use ghostex_gx_core::MachineId;
use serde_json::{Value, json};

use super::conversation::voice_agent_on;
use super::focus::{THIS_COMPUTER, VoiceCallTarget};
use crate::*;

/// The GBot a call is with, as gxserver's `focus.gbot` names it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CallGbot {
    pub(crate) bot_id: String,
    pub(crate) name: String,
    pub(crate) icon: String,
    pub(crate) color_hex: String,
}

impl CallGbot {
    pub(crate) fn parse(value: &Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().unwrap_or_default().trim().to_string();
        let bot_id = text("botId");
        if bot_id.is_empty() {
            return None;
        }
        Some(Self {
            bot_id,
            name: text("name"),
            icon: Some(text("icon"))
                .filter(|icon| !icon.is_empty())
                .unwrap_or_else(|| "robot".to_string()),
            color_hex: text("colorHex"),
        })
    }

    /// The panel strip's title.
    pub(crate) fn strip_title(&self) -> String {
        format!("Call with {}", self.name)
    }
}

/// A call with a GBot to start: on the computer the GBot lives on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GbotCallRequest {
    pub(crate) machine: MachineId,
    pub(crate) machine_name: Option<String>,
    pub(crate) bot_id: String,
    pub(crate) name: String,
}

/// Whether Call shows anywhere: GBots and the Voice agent both on.
pub(crate) fn gbot_calls_on() -> bool {
    voice_agent_on() && crate::app::native_sidebar::gbots_rows::gbots_on()
}

/// The header arrow's row for a GBot's chat, when calls are on.
pub(crate) fn gbot_call_menu_label(target: &VoiceCallTarget) -> Option<String> {
    let (_, name) = target.gbot.as_ref()?;
    gbot_calls_on().then(|| format!("Call {name}"))
}

/// The machine a sidebar row's `machineId` names.
pub(crate) fn machine_of(machine_id: &str) -> MachineId {
    if machine_id.is_empty() || machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
        MachineId::Local
    } else {
        MachineId::Remote(machine_id.to_string())
    }
}

impl GhostexGpuiApp {
    /// Call on a GBot's row: a call with it about your standing chat with it.
    pub(crate) fn call_gbot(
        &mut self,
        machine_id: &str,
        bot_id: String,
        name: String,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = machine_of(machine_id);
        let machine_name = machine.remote_id().map(|id| {
            self.native_sidebar
                .snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot.groups.iter().find_map(|group| {
                        let context = group.remote_machine_context.as_ref()?;
                        (context.get("machineId").and_then(Value::as_str) == Some(id))
                            .then(|| context.get("machineName").and_then(Value::as_str))
                            .flatten()
                            .map(str::to_string)
                    })
                })
                .unwrap_or_else(|| id.to_string())
        });
        let request = GbotCallRequest {
            machine,
            machine_name,
            bot_id,
            name,
        };
        self.start_or_queue_gbot_call(None, request, window, cx);
    }

    /// The header arrow's "Call <GBot>" on one of a GBot's chats: a call with it about that chat.
    pub(crate) fn call_gbot_about(
        &mut self,
        target: VoiceCallTarget,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some((bot_id, name)) = target.gbot.clone() else {
            return;
        };
        let request = GbotCallRequest {
            machine: target.key.machine.clone(),
            machine_name: target.machine_name.clone(),
            bot_id,
            name,
        };
        self.start_or_queue_gbot_call(Some(target), request, window, cx);
    }

    /// Starts the call, or, while another call is open, asks to hang it up first: a call can't
    /// turn into a call with a GBot.
    fn start_or_queue_gbot_call(
        &mut self,
        about: Option<VoiceCallTarget>,
        request: GbotCallRequest,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !gbot_calls_on() {
            return;
        }
        if !self.voice_agent.active() {
            self.start_voice_call(about, Some(request), cx);
            return;
        }
        let already = self
            .voice_agent
            .focus
            .as_ref()
            .and_then(|focus| focus.gbot.as_ref());
        if already.is_some_and(|gbot| gbot.bot_id == request.bot_id) {
            return;
        }
        let here = self
            .voice_agent
            .machine_name
            .clone()
            .unwrap_or_else(|| THIS_COMPUTER.to_string());
        let answer = window.prompt(
            gpui::PromptLevel::Info,
            &format!("A voice call is open on {here}"),
            Some(&format!("Hang up and call {}?", request.name)),
            &[
                gpui::PromptButton::cancel("Keep the call"),
                gpui::PromptButton::new("Hang up and call"),
            ],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await != Ok(1) {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if !this.voice_agent.active() {
                    this.start_voice_call(about, Some(request), cx);
                    return;
                }
                this.voice_agent.gbot_after_hang_up = Some(request);
                if this.voice_agent.phase != super::conversation::VoicePhase::Ending {
                    this.hang_up_voice_agent(cx);
                }
            });
        })
        .detach();
    }

    /// The panel's voice list in a call with a GBot: the GBot's voice, saved on its computer and
    /// switched in the call. False when the call isn't with a GBot.
    pub(crate) fn pick_gbot_call_voice(
        &mut self,
        voice: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(gbot) = self
            .voice_agent
            .focus
            .as_ref()
            .and_then(|focus| focus.gbot.clone())
        else {
            return false;
        };
        self.send_voice_agent_frame(&json!({ "type": "voice", "voice": voice }));
        let machine = self
            .voice_agent
            .machine
            .remote_id()
            .unwrap_or(ghostex_gx_core::LOCAL_MACHINE_ID)
            .to_string();
        self.call_native_sidebar_gbots(
            &machine,
            json!({ "op": "voiceSet", "bot": gbot.bot_id, "voice": voice }),
            "Voice not saved",
            |_, _, _| {},
            cx,
        );
        self.sync_voice_agent_panel(cx);
        true
    }
}
