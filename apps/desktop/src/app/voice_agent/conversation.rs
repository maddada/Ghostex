//! The conversation the work area header's Voice button starts and ends: one `/api/voice` socket,
//! the echo-cancelled microphone streaming into it, the agent's audio playing back, and the state,
//! captions and tool lines the voice panel draws.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt as _;
use ghostex_gx_chat_client::voice::{VoiceInbound, VoiceSink, VoiceSocket};
use gpui::{Pixels, Task, WindowHandle};
use serde_json::{Value, json};

use super::audio::{AudioStartError, MICROPHONE_KEPT_STOPPING, SILENT_MICROPHONE, VoiceAudio};
use super::focus::{
    CallMachine, ReadyFocus, VoiceCallTarget, VoiceFocus, focus_ref, outdated_sentence,
    parse_focus, ready_focus, remote_key_sentence,
};
use super::memory::{VoiceMemory, VoiceMemoryState};
use super::panel::VoicePanelWindow;
use crate::*;

/// Whether this build has the Voice button.
///
/// CDXC:VoiceMode 2026-10-20 DECISION: User: "Please make the voice agent feature and the dictating feature and all of those features related to voice mode basically there on Linux and on Windows. They're not Mac only, you know what I mean?" The Voice button, the panel, Dictate, the voice memory and the settings are on macOS, Windows and Linux alike; this supersedes the macOS-only gate.
/// CDXC:VoiceMode 2026-10-20 WHY: hands-free voice needs echo cancellation, or the agent's own voice from the speakers interrupts it (gxserver's provider runs voice activity detection on everything the microphone hears). macOS has it in the voice-processing I/O unit (`native/macos/GpuiVoiceAudio.m`); Windows and Linux run Sonora's AEC3 in-process on the audio this app plays (`audio_cpal.rs`, `aec.rs`), so it works without headphones and without system setup. The GPUI web build has its own stand-in that hides it.
pub(crate) const VOICE_AGENT_AVAILABLE: bool = cfg!(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "linux"
));

/// Whether this build has the voice agent and its built-in extension is on (Settings >
/// Extensions, on by default). The header's Voice button shows and a call starts only then; the
/// settings hook ends a call that is open when it is turned off (`apply_voice_agent_settings`).
/// Dictate does not ask: it is a separate feature.
///
/// CDXC:VoiceMode 2026-10-25 SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs (the user decision), server/src/voice_agent/feature.rs (gxserver refuses calls while it is off).
pub(crate) fn voice_agent_on() -> bool {
    voice_agent_on_in(shared_settings::shared_sidebar_settings_snapshot().object())
}

/// [`voice_agent_on`] over a settings object.
fn voice_agent_on_in(settings: &serde_json::Map<String, Value>) -> bool {
    VOICE_AGENT_AVAILABLE
        && ghostex_settings_catalog::built_in_extensions::on_with(
            ghostex_settings_catalog::built_in_extensions::VOICE_AGENT,
            |key| settings.get(key).and_then(Value::as_bool),
        )
}

/// What the settings hook does with the call once the voice agent is turned off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnedOffStep {
    /// It is on, nothing is open, or a hang-up is already waiting for `ended`.
    Nothing,
    /// A call is opening or open: hang up, and the panel closes with gxserver's `ended`.
    HangUp,
    /// The panel shows a call that is over: close it.
    ClosePanel,
}

fn turned_off_step(on: bool, phase: VoicePhase) -> TurnedOffStep {
    match phase {
        _ if on => TurnedOffStep::Nothing,
        VoicePhase::Idle | VoicePhase::Ending => TurnedOffStep::Nothing,
        VoicePhase::Starting | VoicePhase::Live => TurnedOffStep::HangUp,
        VoicePhase::Ended => TurnedOffStep::ClosePanel,
    }
}

/// How long Hang up waits for gxserver's `ended` before letting go of the conversation itself.
const HANG_UP_GRACE: std::time::Duration = std::time::Duration::from_secs(3);
/// The panel keeps this many rows; older ones scroll away for good.
const MAX_ROWS: usize = 60;
/// A device change restarts the audio at most this many times per conversation.
pub(super) const MAX_AUDIO_RESTARTS: u32 = 5;
/// The speeds the panel's buttons and Settings › Extensions › Voice agent offer (`voiceAgentSpeed`).
pub(crate) const VOICE_AGENT_SPEEDS: [f32; 6] = [1.0, 1.2, 1.4, 1.6, 1.8, 2.0];
const SPEED_SETTING: &str = "voiceAgentSpeed";
const CALL_MODE_SETTING: &str = "voiceAgentCallMode";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum VoicePhase {
    /// No conversation, and the panel is closed.
    #[default]
    Idle,
    /// The socket is open and gxserver is opening the provider session.
    Starting,
    /// The provider accepted the session.
    Live,
    /// Hang up was pressed; waiting for gxserver's `ended`.
    Ending,
    /// The conversation is over and the panel says why (an error, or the idle hang-up).
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceRole {
    User,
    Agent,
}

/// One line of the panel's running transcript.
#[derive(Clone, Debug)]
pub(crate) enum VoiceRow {
    Caption {
        role: VoiceRole,
        item_id: String,
        text: String,
        /// gxserver's paragraphs of a long agent reply (`rows.rs`); empty: `text` is one.
        paragraphs: Vec<String>,
        /// The agent was talked over: the caption ends where its audio was cut.
        cut: bool,
    },
    Tool {
        /// The call's id, or its run's (the run's first call) when gxserver groups the calls.
        call_id: String,
        label: String,
        /// `None` while it runs.
        ok: Option<bool>,
    },
    Announcement {
        text: String,
    },
    /// gxserver asks before a stop it is unsure about (`rows.rs`); gone once answered.
    Confirm(super::rows::ConfirmCard),
}

/// Why the conversation stopped or could not start, for the panel.
#[derive(Clone, Debug)]
pub(crate) struct VoiceProblem {
    /// gxserver's error code (`noKey`, `badKey`, `providerUnavailable`, ...), or `microphone`,
    /// `connection`, `idle`, `outdated` for what the app itself noticed.
    pub(crate) code: String,
    pub(crate) message: String,
    /// It happened on another computer the call ran on, whose own Settings hold its key.
    pub(crate) elsewhere: bool,
}

impl VoiceProblem {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            elsewhere: false,
        }
    }

    fn is_key(&self) -> bool {
        matches!(self.code.as_str(), "noKey" | "badKey")
    }

    /// Whether this computer's Settings › Extensions › Voice agent is where the user fixes it.
    pub(crate) fn wants_settings(&self) -> bool {
        self.is_key() && !self.elsewhere
    }

    /// Whether starting the same call again can work: not on a computer too old for it, nor on
    /// another computer that has no usable key.
    pub(crate) fn can_start_again(&self) -> bool {
        self.code != "outdated" && !(self.elsewhere && self.is_key())
    }
}

/// What the panel and the Voice button draw, and the live link behind them.
#[derive(Default)]
pub(crate) struct VoiceAgent {
    pub(crate) phase: VoicePhase,
    /// gxserver's last `state`: `connecting`, `listening`, `userSpeaking`, `thinking`,
    /// `speaking` or `toolRunning`.
    pub(crate) state: String,
    /// The running tool's label ("Checking your sessions") while `state` is `toolRunning`.
    pub(crate) tool_label: Option<String>,
    pub(crate) rows: Vec<VoiceRow>,
    pub(crate) muted: bool,
    pub(crate) problem: Option<VoiceProblem>,
    /// `xai` or `openai`, once `ready` named it.
    pub(crate) provider: Option<String>,
    /// The voice the agent speaks with now, and the provider's voices (from `ready`).
    pub(crate) voice: Option<String>,
    pub(crate) voice_options: Vec<String>,
    /// A voice sent to gxserver and not answered yet.
    voice_requested: Option<String>,
    /// A picked voice this conversation could not switch to: it speaks from the next one.
    pub(crate) next_voice: Option<String>,
    /// The panel shows the voice list instead of the transcript.
    pub(crate) voice_picker_open: bool,
    /// How fast the agent's audio plays (`voiceAgentSpeed`).
    pub(crate) speed: f32,
    /// `voiceAgentCallMode` is `pushToTalk`: the next call is push-to-talk (the gear menu's tick).
    pub(crate) push_to_talk_setting: bool,
    /// This call is push-to-talk (`ready` said so): the microphone streams only while Talk is held.
    pub(crate) push_to_talk: bool,
    /// Talk is held (gxserver was sent `talkStart`).
    pub(crate) talking: bool,
    /// What holds it: the panel's button, Space or the Hold to Talk hotkey (`hold_to_talk.rs`).
    pub(crate) hold: Option<super::hold_to_talk::HoldSource>,
    /// Space would hold Talk now (the main window's keyboard is free), for the panel's keycap.
    pub(crate) space_free: bool,
    /// A short note in the strip above the panel's buttons, and the timer that clears it.
    pub(crate) hint: Option<String>,
    pub(super) hint_timer: Option<Task<()>>,
    /// Where the user dragged the panel: its gap to the window's trailing edge and its top, kept
    /// for the rest of the app's run.
    pub(crate) panel_place: Option<gpui::Point<Pixels>>,
    /// Where the panel was when the current drag started, in the main window's content.
    pub(crate) panel_drag_from: Option<gpui::Point<Pixels>>,
    /// The remembered last conversation gxserver named in `ready`, and what the panel shows of it.
    pub(crate) memory: Option<VoiceMemory>,
    pub(crate) memory_state: VoiceMemoryState,
    /// The "Remembering" line shows the whole summary.
    pub(crate) memory_details_open: bool,
    /// The computer the call runs on, and its name when it isn't this one.
    pub(crate) machine: CallMachine,
    pub(crate) machine_name: Option<String>,
    /// The session the call is about now (`None`: all sessions), as gxserver last said.
    pub(crate) focus: Option<VoiceFocus>,
    /// The call's gxserver knows focused calls (its `ready` carried `focus`).
    pub(crate) focus_supported: bool,
    /// The session the call was started about, which Start again repeats.
    pub(crate) requested: Option<VoiceCallTarget>,
    /// "Hang up and call" (`focus.rs`): the call to start once the open one is over.
    pub(crate) call_after_hang_up: Option<VoiceCallTarget>,
    /// Where the header's Voice button last painted, for its arrow's menu.
    pub(crate) menu_anchor: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>>,
    link: Option<VoiceLink>,
    /// Bumped per conversation, so a late message of an earlier one is ignored.
    generation: u64,
    pub(crate) panel: Option<WindowHandle<VoicePanelWindow>>,
    /// The main window size the panel was placed for.
    pub(crate) panel_placed_for: Option<gpui::Size<Pixels>>,
    pub(crate) panel_reposition: Option<Task<()>>,
    hang_up_timer: Option<Task<()>>,
}

impl VoiceAgent {
    /// Whether a conversation is open or opening (the Voice button reads as on).
    pub(crate) fn active(&self) -> bool {
        matches!(
            self.phase,
            VoicePhase::Starting | VoicePhase::Live | VoicePhase::Ending
        )
    }
}

/// The open socket and audio of one conversation.
struct VoiceLink {
    socket: Arc<VoiceSocket>,
    shared: Arc<LinkShared>,
    audio_restarts: u32,
    _events: Task<()>,
}

/// What the socket thread, the audio thread and the app share.
struct LinkShared {
    /// Microphone audio flows only after `ready`.
    streaming: AtomicBool,
    muted: AtomicBool,
    /// A push-to-talk call: the microphone streams only while `talking`.
    push_to_talk: AtomicBool,
    talking: AtomicBool,
    /// The playback speed as `f32` bits, for audio that (re)starts.
    rate: AtomicU32,
    playback: Mutex<Playback>,
    sender: futures::channel::mpsc::UnboundedSender<VoiceMessage>,
}

#[derive(Default)]
struct Playback {
    audio: Option<Arc<VoiceAudio>>,
    /// The conversation is over: audio that finishes starting now is stopped at once.
    closed: bool,
    /// Talked over: agent audio is dropped until the next reply starts.
    dropping: bool,
    /// Samples of the current reply handed to the speaker.
    relayed: u64,
}

enum VoiceMessage {
    Socket(VoiceInbound),
    AudioStarted,
    AudioFailed(AudioStartError),
    AudioDeviceChanged,
    MicrophoneSilent,
}

impl GhostexGpuiApp {
    /// The Voice button: starts a conversation, or hangs up the open one.
    pub(crate) fn toggle_voice_agent_conversation(&mut self, cx: &mut gpui::Context<Self>) {
        if self.voice_agent.active() {
            self.hang_up_voice_agent(cx);
        } else {
            self.start_voice_agent_conversation(cx);
        }
    }

    /// The headset: a call about all sessions, on this computer.
    pub(crate) fn start_voice_agent_conversation(&mut self, cx: &mut gpui::Context<Self>) {
        self.start_voice_agent_call(None, cx);
    }

    /// Starts a call about all sessions on this computer, or about `about`, on that session's own
    /// computer (`focus=<projectId>:<sessionId>`).
    ///
    /// The call about another computer's session runs on that computer (CDXC:VoiceMode 2026-10-23
    /// DECISION in `endpoint.rs`); `clientRemote=1` asks it to leave its own screen alone, and this
    /// app shows what the agent shows (`focus.rs`).
    pub(crate) fn start_voice_agent_call(
        &mut self,
        about: Option<VoiceCallTarget>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !voice_agent_on() || self.voice_agent.active() {
            return;
        }
        let machine = about
            .as_ref()
            .map_or(ghostex_gx_core::MachineId::Local, |about| {
                about.key.machine.clone()
            });
        let endpoint = self.voice_agent_endpoint(&machine);
        let agent = &mut self.voice_agent;
        agent.machine = CallMachine(machine);
        agent.machine_name = about.as_ref().and_then(|about| about.machine_name.clone());
        agent.focus = None;
        agent.focus_supported = false;
        agent.requested = about.clone();
        agent.generation = agent.generation.wrapping_add(1);
        agent.rows.clear();
        agent.problem = None;
        agent.tool_label = None;
        agent.provider = None;
        agent.voice = None;
        agent.voice_options.clear();
        agent.voice_requested = None;
        agent.next_voice = None;
        agent.voice_picker_open = false;
        agent.memory = None;
        agent.memory_state = VoiceMemoryState::Hidden;
        agent.memory_details_open = false;
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        agent.speed = voice_agent_speed_setting(settings.object());
        agent.push_to_talk_setting = push_to_talk_setting(settings.object());
        agent.push_to_talk = false;
        agent.talking = false;
        agent.hold = None;
        agent.hint = None;
        agent.hint_timer = None;
        agent.muted = false;
        agent.state = "connecting".to_string();
        agent.hang_up_timer = None;
        let endpoint = match endpoint {
            Ok(endpoint) => endpoint,
            Err(message) => {
                agent.phase = VoicePhase::Ended;
                agent.problem = Some(VoiceProblem::new("connection", message));
                self.sync_voice_agent_panel(cx);
                return;
            }
        };
        agent.phase = VoicePhase::Starting;
        let generation = agent.generation;
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<VoiceMessage>();
        let shared = Arc::new(LinkShared {
            streaming: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            push_to_talk: AtomicBool::new(false),
            talking: AtomicBool::new(false),
            rate: AtomicU32::new(self.voice_agent.speed.to_bits()),
            playback: Mutex::new(Playback::default()),
            sender,
        });
        let focus = about.as_ref().map(|about| focus_ref(&about.key));
        let mut query = vec![("mode", "conversation"), ("client", "desktop")];
        if let Some(focus) = focus.as_deref() {
            query.push(("focus", focus));
        }
        if !self.voice_agent.machine.is_local() {
            query.push(("clientRemote", "1"));
        }
        let socket = Arc::new(VoiceSocket::open(
            &endpoint,
            &query,
            socket_sink(shared.clone()),
        ));
        start_audio(socket.clone(), shared.clone());
        let events = cx.spawn(async move |this, cx| {
            while let Some(message) = receiver.next().await {
                let alive = this.update(cx, |this, cx| {
                    this.receive_voice_agent_message(generation, message, cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        self.voice_agent.link = Some(VoiceLink {
            socket,
            shared,
            audio_restarts: 0,
            _events: events,
        });
        self.sync_voice_agent_panel(cx);
    }

    /// Hang up: gxserver answers `ended` and closes; the panel closes with it.
    pub(crate) fn hang_up_voice_agent(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(link) = self.voice_agent.link.as_ref() else {
            self.close_voice_agent_panel(cx);
            return;
        };
        link.shared.streaming.store(false, Ordering::SeqCst);
        link.socket.send_json(&json!({ "type": "end" }));
        stop_playback(&link.shared);
        self.voice_agent.phase = VoicePhase::Ending;
        let generation = self.voice_agent.generation;
        self.voice_agent.hang_up_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HANG_UP_GRACE).await;
            let _ = this.update(cx, |this, cx| {
                if this.voice_agent.generation == generation
                    && this.voice_agent.phase == VoicePhase::Ending
                {
                    this.end_voice_agent_link();
                    this.close_voice_agent_panel(cx);
                }
            });
        }));
        self.sync_voice_agent_panel(cx);
    }

    /// Mute, told to gxserver too (it drops the microphone's audio and refuses a push-to-talk hold
    /// while muted, `talk.rs`). Muting a push-to-talk call mid-hold ends the hold with nothing sent.
    pub(crate) fn toggle_voice_agent_mute(&mut self, cx: &mut gpui::Context<Self>) {
        let muted = !self.voice_agent.muted;
        self.voice_agent.muted = muted;
        let cancel_hold = muted && self.voice_agent.push_to_talk && self.voice_agent.talking;
        if cancel_hold {
            self.voice_agent.talking = false;
            self.voice_agent.hold = None;
        }
        if let Some(link) = &self.voice_agent.link {
            link.shared.muted.store(muted, Ordering::SeqCst);
            if cancel_hold {
                link.shared.talking.store(false, Ordering::SeqCst);
            }
            if self.voice_agent.phase == VoicePhase::Live {
                link.socket
                    .send_json(&json!({ "type": "mute", "muted": muted }));
            }
        }
        self.sync_voice_agent_panel(cx);
    }

    /// "Stop talking": gxserver cancels the reply and answers `interrupted`.
    pub(crate) fn interrupt_voice_agent(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(link) = &self.voice_agent.link {
            link.socket.send_json(&json!({ "type": "interrupt" }));
            stop_playback(&link.shared);
        }
        cx.notify();
    }

    /// A button on a confirm card: Stop on `session`, or Cancel. The card leaves at once; gxserver
    /// says what happened.
    pub(crate) fn answer_voice_agent_confirm(
        &mut self,
        id: &str,
        session: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.send_voice_agent_frame(&super::rows::confirm_answer(id, session)) {
            self.voice_agent
                .rows
                .retain(|row| !matches!(row, VoiceRow::Confirm(card) if card.id == id));
            cx.notify();
        }
    }

    /// Sends a control frame on the live conversation's socket; false when none is live.
    pub(super) fn send_voice_agent_frame(&self, frame: &Value) -> bool {
        match &self.voice_agent.link {
            Some(link) if self.voice_agent.phase == VoicePhase::Live => {
                link.socket.send_json(frame);
                true
            }
            _ => false,
        }
    }

    /// Lets go of the socket and the audio, without telling gxserver.
    fn end_voice_agent_link(&mut self) {
        if let Some(link) = self.voice_agent.link.take() {
            link.shared.streaming.store(false, Ordering::SeqCst);
            if let Ok(mut playback) = link.shared.playback.lock() {
                playback.closed = true;
                playback.audio = None;
            }
            link.socket.close();
        }
        self.voice_agent.hang_up_timer = None;
    }

    fn receive_voice_agent_message(
        &mut self,
        generation: u64,
        message: VoiceMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        if generation != self.voice_agent.generation {
            return;
        }
        match message {
            VoiceMessage::Socket(VoiceInbound::Event(event)) => {
                self.apply_voice_agent_event(&event, cx)
            }
            VoiceMessage::Socket(VoiceInbound::Audio(_)) => {}
            VoiceMessage::Socket(VoiceInbound::Closed(reason)) => {
                let was_ending = self.voice_agent.phase == VoicePhase::Ending;
                let open = matches!(
                    self.voice_agent.phase,
                    VoicePhase::Starting | VoicePhase::Live
                );
                self.end_voice_agent_link();
                if was_ending {
                    self.close_voice_agent_panel(cx);
                    return;
                }
                if open {
                    self.voice_agent.phase = VoicePhase::Ended;
                    self.voice_agent.problem.get_or_insert(VoiceProblem::new(
                        "connection",
                        reason.unwrap_or_else(|| {
                            "The voice conversation closed unexpectedly.".to_string()
                        }),
                    ));
                }
            }
            VoiceMessage::AudioStarted => {}
            VoiceMessage::AudioFailed(error) => {
                if let Some(link) = &self.voice_agent.link {
                    link.socket.send_json(&json!({ "type": "end" }));
                }
                self.end_voice_agent_link();
                self.voice_agent.phase = VoicePhase::Ended;
                self.voice_agent.problem = Some(VoiceProblem::new("microphone", error.message()));
            }
            VoiceMessage::AudioDeviceChanged => {
                let Some(link) = self.voice_agent.link.as_mut() else {
                    return;
                };
                if link.audio_restarts < MAX_AUDIO_RESTARTS {
                    link.audio_restarts += 1;
                    if let Ok(mut playback) = link.shared.playback.lock() {
                        playback.audio = None;
                    }
                    start_audio(link.socket.clone(), link.shared.clone());
                } else {
                    // The audio has stopped and is not started again: say so instead of going
                    // quiet.
                    link.socket.send_json(&json!({ "type": "end" }));
                    self.end_voice_agent_link();
                    self.voice_agent.phase = VoicePhase::Ended;
                    self.voice_agent.problem =
                        Some(VoiceProblem::new("microphone", MICROPHONE_KEPT_STOPPING));
                }
            }
            VoiceMessage::MicrophoneSilent => {
                self.push_voice_agent_row(VoiceRow::Announcement {
                    text: SILENT_MICROPHONE.to_string(),
                });
            }
        }
        self.sync_voice_agent_panel(cx);
    }

    fn apply_voice_agent_event(&mut self, event: &Value, cx: &mut gpui::Context<Self>) {
        let text = |key: &str| event[key].as_str().unwrap_or_default().to_string();
        match event["type"].as_str().unwrap_or_default() {
            "state" => {
                let state = text("state");
                if state != "toolRunning" {
                    self.voice_agent.tool_label = None;
                }
                self.voice_agent.state = state;
            }
            "ready" => {
                if self.voice_agent.phase == VoicePhase::Starting {
                    self.voice_agent.phase = VoicePhase::Live;
                }
                // A call asked about one session must hear back which one: a `ready` without
                // `focus` comes from a gxserver older than focused calls, which would have opened
                // a call about all sessions instead.
                match ready_focus(event, &self.voice_agent.machine) {
                    ReadyFocus::Unsupported if self.voice_agent.requested.is_some() => {
                        if let Some(link) = &self.voice_agent.link {
                            link.socket.send_json(&json!({ "type": "end" }));
                        }
                        self.end_voice_agent_link();
                        self.voice_agent.phase = VoicePhase::Ended;
                        self.voice_agent.problem = Some(VoiceProblem::new(
                            "outdated",
                            outdated_sentence(self.voice_agent.machine_name.as_deref()),
                        ));
                        return;
                    }
                    ReadyFocus::Unsupported => self.voice_agent.focus_supported = false,
                    ReadyFocus::All => {
                        self.voice_agent.focus_supported = true;
                        self.apply_voice_agent_focus(None);
                    }
                    ReadyFocus::Session(focus) => {
                        self.voice_agent.focus_supported = true;
                        self.apply_voice_agent_focus(Some(focus));
                    }
                }
                self.voice_agent.provider = Some(text("provider"));
                self.voice_agent.voice = Some(text("voice"));
                self.voice_agent.voice_options = event["voiceOptions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|voice| voice.as_str().map(str::to_string))
                    .collect();
                self.apply_voice_agent_memory_ready(event);
                if let Some(link) = &self.voice_agent.link {
                    link.shared.streaming.store(true, Ordering::SeqCst);
                    link.socket.send_json(&json!({
                        "type": "playbackRate",
                        "rate": self.voice_agent.speed,
                    }));
                    // Muted while connecting: gxserver starts every client unmuted.
                    if self.voice_agent.muted {
                        link.socket
                            .send_json(&json!({ "type": "mute", "muted": true }));
                    }
                }
                let push_to_talk = event["callMode"].as_str() == Some("pushToTalk");
                self.voice_agent.push_to_talk = push_to_talk;
                if let Some(link) = &self.voice_agent.link {
                    link.shared
                        .push_to_talk
                        .store(push_to_talk, Ordering::SeqCst);
                }
                // A voice picked while connecting.
                self.apply_voice_agent_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                    cx,
                );
            }
            "caption" => {
                let role = if event["role"].as_str() == Some("user") {
                    VoiceRole::User
                } else {
                    VoiceRole::Agent
                };
                let item_id = text("itemId");
                let caption = text("text");
                let paragraphs = super::rows::caption_paragraphs(event);
                let cut = event["cut"].as_bool().unwrap_or(false);
                if role == VoiceRole::User && !caption.trim().is_empty() {
                    self.voice_agent_memory_user_spoke();
                }
                let position = self.voice_agent.rows.iter().rposition(
                    |row| matches!(row, VoiceRow::Caption { item_id: id, .. } if *id == item_id),
                );
                // CDXC:VoiceMode 2026-10-20 WHY: an interrupted reply ends with one `cut` caption holding only the words actually played, which can come after that reply's `final` caption; it replaces the reply's caption whenever it lands, and a cut before any word was heard removes the caption.
                if cut && caption.trim().is_empty() {
                    if let Some(position) = position {
                        self.voice_agent.rows.remove(position);
                    }
                } else {
                    let existing =
                        position.and_then(|position| match &mut self.voice_agent.rows[position] {
                            VoiceRow::Caption {
                                text,
                                paragraphs,
                                cut: was_cut,
                                ..
                            } => Some((text, paragraphs, was_cut)),
                            _ => None,
                        });
                    match existing {
                        // A reply that was cut keeps the cut caption.
                        Some((_, _, was_cut)) if *was_cut && !cut => {}
                        Some((existing_text, existing_paragraphs, was_cut)) => {
                            *existing_text = caption;
                            *existing_paragraphs = paragraphs;
                            *was_cut = cut;
                        }
                        None if !caption.trim().is_empty() => {
                            self.push_voice_agent_row(VoiceRow::Caption {
                                role,
                                item_id,
                                text: caption,
                                paragraphs,
                                cut,
                            })
                        }
                        None => {}
                    }
                }
            }
            "voice" => {
                let voice = text("voice");
                let agent = &mut self.voice_agent;
                if agent.voice_requested.as_deref() == Some(voice.as_str()) {
                    agent.voice_requested = None;
                }
                if event["applied"].as_str() == Some("now") {
                    agent.voice = Some(voice);
                    agent.next_voice = None;
                } else if agent.next_voice.as_deref() != Some(voice.as_str()) {
                    agent.next_voice = Some(voice.clone());
                    let provider = provider_name(agent.provider.as_deref().unwrap_or_default());
                    let name = voice_name(&voice);
                    let text = if event["reason"].as_str() == Some("providerLocksVoice") {
                        format!(
                            "{provider} keeps a call's voice once it has spoken, so {name} speaks from your next conversation."
                        )
                    } else {
                        format!(
                            "{provider} didn't switch voices mid-call, so {name} speaks from your next conversation."
                        )
                    };
                    self.push_voice_agent_row(VoiceRow::Announcement { text });
                }
            }
            "memory" => self.apply_voice_agent_memory_event(event),
            // gxserver's word on a hold (server/src/voice_agent/talk.rs).
            "talk" => match event["event"].as_str().unwrap_or_default() {
                "tooShort" => self.flash_voice_agent_hint(super::hold_to_talk::HINT_TOO_SHORT, cx),
                "refused" => {
                    self.voice_agent.talking = false;
                    self.voice_agent.hold = None;
                    if let Some(link) = &self.voice_agent.link {
                        link.shared.talking.store(false, Ordering::SeqCst);
                    }
                    self.flash_voice_agent_hint(super::hold_to_talk::HINT_UNMUTE, cx);
                }
                "cancelled" => self.flash_voice_agent_hint(super::hold_to_talk::HINT_CANCELLED, cx),
                _ => {}
            },
            "focus" => {
                let focus = parse_focus(&event["focus"], &self.voice_agent.machine);
                self.apply_voice_agent_focus(focus);
            }
            "interrupted" => {
                if let Some(link) = &self.voice_agent.link
                    && let Some(played_ms) = event["playedMs"].as_u64()
                {
                    link.socket.send_json(&json!({
                        "type": "playback",
                        "itemId": text("itemId"),
                        "playedMs": played_ms,
                    }));
                }
            }
            "tool" => {
                let frame = super::rows::tool_row_frame(event);
                if frame.started {
                    self.voice_agent.tool_label = Some(text("label"));
                }
                let existing = self.voice_agent.rows.iter_mut().rev().find(
                    |row| matches!(row, VoiceRow::Tool { call_id: id, .. } if *id == frame.key),
                );
                if let Some(VoiceRow::Tool { label, ok, .. }) = existing {
                    *label = frame.label;
                    *ok = frame.ok;
                } else if frame.started {
                    self.push_voice_agent_row(VoiceRow::Tool {
                        call_id: frame.key,
                        label: frame.label,
                        ok: frame.ok,
                    });
                }
                if event["phase"].as_str() == Some("finished") {
                    self.follow_remote_voice_call_tool(event, cx);
                }
            }
            "confirm" => {
                if let Some(card) = super::rows::confirm_card(event) {
                    self.push_voice_agent_row(VoiceRow::Confirm(card));
                }
            }
            "confirmDone" => {
                let id = text("id");
                self.voice_agent
                    .rows
                    .retain(|row| !matches!(row, VoiceRow::Confirm(card) if card.id == id));
            }
            "announcement" => {
                let announcement = text("text");
                if !announcement.trim().is_empty() {
                    self.push_voice_agent_row(VoiceRow::Announcement { text: announcement });
                }
            }
            "error" => {
                let mut problem = VoiceProblem::new(&text("code"), error_sentence(event));
                if !self.voice_agent.machine.is_local() {
                    problem.elsewhere = true;
                    if problem.is_key() && event["message"].as_str().is_none_or(str::is_empty) {
                        let name = self.voice_agent.machine_name.as_deref();
                        problem.message = remote_key_sentence(&problem.code, name);
                    }
                }
                if event["fatal"].as_bool() == Some(false) {
                    self.push_voice_agent_row(VoiceRow::Announcement {
                        text: problem.message,
                    });
                } else {
                    self.voice_agent.problem = Some(problem);
                }
            }
            "ended" => {
                let reason = text("reason");
                let was_ending = self.voice_agent.phase == VoicePhase::Ending;
                self.end_voice_agent_link();
                if reason == "user" || was_ending {
                    self.close_voice_agent_panel(cx);
                    return;
                }
                self.voice_agent.phase = VoicePhase::Ended;
                if self.voice_agent.problem.is_none() {
                    self.voice_agent.problem = Some(VoiceProblem::new(
                        if reason == "idle" { "idle" } else { "ended" },
                        ended_sentence(&reason, event["message"].as_str()),
                    ));
                }
            }
            _ => {}
        }
    }

    fn push_voice_agent_row(&mut self, row: VoiceRow) {
        let rows = &mut self.voice_agent.rows;
        rows.push(row);
        if rows.len() > MAX_ROWS {
            let excess = rows.len() - MAX_ROWS;
            rows.drain(..excess);
        }
    }

    /// Closes the panel and forgets the finished conversation.
    pub(crate) fn close_voice_agent_panel(&mut self, cx: &mut gpui::Context<Self>) {
        self.end_voice_agent_link();
        self.voice_agent.phase = VoicePhase::Idle;
        self.voice_agent.problem = None;
        self.voice_agent.focus = None;
        // Every way a hang-up ends comes here (`ended`, the socket closing, `HANG_UP_GRACE`).
        if let Some(next) = self.voice_agent.call_after_hang_up.take() {
            self.start_voice_agent_call(Some(next), cx);
            return;
        }
        self.sync_voice_agent_panel(cx);
    }

    /// The panel's speed buttons: saves `voiceAgentSpeed`, which the settings hook applies.
    pub(crate) fn set_voice_agent_speed(&mut self, speed: f32, cx: &mut gpui::Context<Self>) {
        self.save_voice_agent_setting(SPEED_SETTING, json!(speed_setting_value(speed)), cx);
    }

    /// The gear menu: saves `voiceAgentCallMode`; gxserver reads it when the next call starts.
    pub(crate) fn set_voice_agent_call_mode(
        &mut self,
        push_to_talk: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let value = if push_to_talk {
            "pushToTalk"
        } else {
            "regular"
        };
        self.save_voice_agent_setting(CALL_MODE_SETTING, json!(value), cx);
    }

    /// Tells gxserver a hold started (`down`) or ended (`hold_to_talk.rs` decides when). In a
    /// push-to-talk call pressing cuts a reply the agent is speaking (gxserver answers
    /// `interrupted`, as for Stop) and releasing sends what was said; in a muted Regular call the
    /// microphone streams for as long as the hold lasts.
    pub(super) fn set_voice_agent_talking(&mut self, down: bool, cx: &mut gpui::Context<Self>) {
        let agent = &mut self.voice_agent;
        if agent.phase != VoicePhase::Live || agent.talking == down {
            return;
        }
        agent.talking = down;
        if let Some(link) = &agent.link {
            link.shared.talking.store(down, Ordering::SeqCst);
            link.socket.send_json(&json!({
                "type": if down { "talkStart" } else { "talkEnd" },
            }));
        }
        self.sync_voice_agent_panel(cx);
    }

    /// The panel's voice list: saves the voice for the conversation's provider, which the
    /// settings hook applies.
    pub(crate) fn pick_voice_agent_voice(&mut self, voice: String, cx: &mut gpui::Context<Self>) {
        self.voice_agent.voice_picker_open = false;
        let key = voice_setting_key(self.voice_agent.provider.as_deref().unwrap_or_default());
        self.save_voice_agent_setting(key, json!(voice), cx);
        self.sync_voice_agent_panel(cx);
    }

    pub(crate) fn toggle_voice_agent_voice_picker(&mut self, cx: &mut gpui::Context<Self>) {
        let agent = &mut self.voice_agent;
        agent.voice_picker_open = !agent.voice_picker_open && !agent.voice_options.is_empty();
        self.sync_voice_agent_panel(cx);
    }

    fn save_voice_agent_setting(&mut self, key: &str, value: Value, cx: &mut gpui::Context<Self>) {
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({ "patch": { key: value }, "source": "voicePanel" }),
            cx,
        );
    }

    /// Brings the open conversation in line with the saved speed and voice, after any settings
    /// save (the panel's buttons, Settings › Extensions › Voice agent, `ghostex settings set`).
    ///
    /// CDXC:VoiceMode 2026-10-20 WHY: the speed is applied on this computer (the audio is stretched with its pitch kept: `native/macos/GpuiVoiceAudio.m` on macOS, `stretch.rs` on Windows and Linux), at once and mid-sentence, for both providers; gxserver only learns the rate (`playbackRate`) so its playback clock stays right. A voice is sent to gxserver, which switches when the provider allows and otherwise answers that it speaks from the next conversation.
    pub(crate) fn apply_voice_agent_settings(
        &mut self,
        settings: &serde_json::Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        // Turned off on Settings > Extensions: the open call hangs up and its panel closes (gxserver
        // ends it too, `feature.rs`), and a call waiting for a hang-up never starts.
        let on = voice_agent_on_in(settings);
        if !on {
            self.voice_agent.call_after_hang_up = None;
        }
        match turned_off_step(on, self.voice_agent.phase) {
            TurnedOffStep::Nothing => {}
            TurnedOffStep::HangUp => return self.hang_up_voice_agent(cx),
            TurnedOffStep::ClosePanel => return self.close_voice_agent_panel(cx),
        }
        let speed = voice_agent_speed_setting(settings);
        let push_to_talk = push_to_talk_setting(settings);
        let agent = &mut self.voice_agent;
        // The call mode applies to the next call; only the gear menu's tick follows it now.
        let mode_changed = push_to_talk != agent.push_to_talk_setting;
        agent.push_to_talk_setting = push_to_talk;
        let Some(link) = agent.link.as_ref() else {
            agent.speed = speed;
            return;
        };
        let mut changed = mode_changed;
        if speed != agent.speed {
            agent.speed = speed;
            link.shared.rate.store(speed.to_bits(), Ordering::SeqCst);
            if let Ok(playback) = link.shared.playback.lock()
                && let Some(audio) = &playback.audio
            {
                audio.set_rate(speed);
            }
            if agent.phase == VoicePhase::Live {
                link.socket
                    .send_json(&json!({ "type": "playbackRate", "rate": speed }));
            }
            changed = true;
        }
        if agent.phase == VoicePhase::Live
            && let Some(provider) = agent.provider.as_deref()
            && let Some(voice) = settings
                .get(voice_setting_key(provider))
                .and_then(Value::as_str)
                .map(str::to_ascii_lowercase)
            && agent.voice_options.contains(&voice)
        {
            if agent.voice.as_deref() == Some(voice.as_str()) {
                if agent.next_voice.take().is_some() {
                    changed = true;
                }
            } else if agent.voice_requested.as_deref() != Some(voice.as_str())
                && agent.next_voice.as_deref() != Some(voice.as_str())
            {
                link.socket
                    .send_json(&json!({ "type": "voice", "voice": voice }));
                agent.voice_requested = Some(voice);
                changed = true;
            }
        }
        if changed {
            self.sync_voice_agent_panel(cx);
        }
    }

    /// Settings › Extensions › Voice agent, from the panel's "Open Settings".
    pub(crate) fn open_gpui_settings_voice_agent_page(&mut self, cx: &mut gpui::Context<Self>) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = json!({
            "initialTab": "voiceAgent",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }
}

/// Where the socket thread sends what it reads. Agent audio goes straight to the speaker here,
/// on the socket thread, and an interruption silences it here too, without waiting for the main
/// thread.
fn socket_sink(shared: Arc<LinkShared>) -> VoiceSink {
    Arc::new(move |inbound| {
        let inbound = match inbound {
            VoiceInbound::Audio(pcm) => {
                if let Ok(mut playback) = shared.playback.lock()
                    && !playback.dropping
                {
                    if let Some(audio) = &playback.audio {
                        audio.enqueue(&pcm);
                    }
                    playback.relayed += (pcm.len() / 2) as u64;
                }
                return;
            }
            VoiceInbound::Event(mut event) => {
                match event["type"].as_str() {
                    Some("audioStart") => {
                        if let Ok(mut playback) = shared.playback.lock() {
                            playback.dropping = false;
                            playback.relayed = 0;
                        }
                    }
                    Some("interrupted") => {
                        if let Ok(mut playback) = shared.playback.lock() {
                            let dropped = playback
                                .audio
                                .as_ref()
                                .map(|audio| audio.flush())
                                .unwrap_or(0);
                            playback.dropping = true;
                            // 24 samples per millisecond at 24 kHz.
                            event["playedMs"] =
                                json!(playback.relayed.saturating_sub(dropped) / 24);
                        }
                    }
                    _ => {}
                }
                VoiceInbound::Event(event)
            }
            closed => closed,
        };
        let _ = shared.sender.unbounded_send(VoiceMessage::Socket(inbound));
    })
}

/// Silences the agent at once (Hang up, Stop talking).
fn stop_playback(shared: &LinkShared) {
    if let Ok(mut playback) = shared.playback.lock() {
        if let Some(audio) = &playback.audio {
            audio.flush();
        }
        playback.dropping = true;
    }
}

/// Opens the echo-cancelled microphone and the speaker on a thread of their own (macOS may ask for
/// microphone access first), streaming the microphone into `socket` once `ready` arrived.
fn start_audio(socket: Arc<VoiceSocket>, shared: Arc<LinkShared>) {
    let _ = std::thread::Builder::new()
        .name("ghostex-voice-audio-start".into())
        .spawn(move || {
            let capture_shared = shared.clone();
            let changed_shared = shared.clone();
            let silent_shared = shared.clone();
            let started = VoiceAudio::start(
                true,
                true,
                Box::new(move |samples| {
                    // A hold unmutes a muted Regular call; a push-to-talk call streams only while held.
                    let talking = capture_shared.talking.load(Ordering::Relaxed);
                    if capture_shared.streaming.load(Ordering::Relaxed)
                        && (!capture_shared.muted.load(Ordering::Relaxed) || talking)
                        && (!capture_shared.push_to_talk.load(Ordering::Relaxed) || talking)
                    {
                        let bytes = samples
                            .iter()
                            .flat_map(|sample| sample.to_le_bytes())
                            .collect();
                        socket.send_audio(bytes);
                    }
                }),
                Box::new(move || {
                    let _ = changed_shared
                        .sender
                        .unbounded_send(VoiceMessage::AudioDeviceChanged);
                }),
                Box::new(move || {
                    let _ = silent_shared
                        .sender
                        .unbounded_send(VoiceMessage::MicrophoneSilent);
                }),
            );
            let message = match started {
                Ok(audio) => {
                    audio.set_rate(f32::from_bits(shared.rate.load(Ordering::SeqCst)));
                    if let Ok(mut playback) = shared.playback.lock()
                        && !playback.closed
                    {
                        playback.audio = Some(Arc::new(audio));
                    }
                    VoiceMessage::AudioStarted
                }
                Err(error) => VoiceMessage::AudioFailed(error),
            };
            let _ = shared.sender.unbounded_send(message);
        });
}

/// "Grok" / "GPT Realtime", for "Grok · Eve".
pub(crate) fn provider_name(provider: &str) -> &'static str {
    match provider {
        "openai" => "GPT Realtime",
        _ => "Grok",
    }
}

/// "eve" → "Eve".
pub(crate) fn voice_name(voice: &str) -> String {
    let mut chars = voice.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

fn voice_setting_key(provider: &str) -> &'static str {
    match provider {
        "openai" => "voiceAgentOpenaiVoice",
        _ => "voiceAgentXaiVoice",
    }
}

/// `voiceAgentCallMode` is `pushToTalk` (the default is a regular, hands-free call).
fn push_to_talk_setting(settings: &serde_json::Map<String, Value>) -> bool {
    settings.get(CALL_MODE_SETTING).and_then(Value::as_str) == Some("pushToTalk")
}

/// `voiceAgentSpeed` as one of the offered speeds (1 when unset or unknown).
pub(crate) fn voice_agent_speed_setting(settings: &serde_json::Map<String, Value>) -> f32 {
    let saved = match settings.get(SPEED_SETTING) {
        Some(Value::String(text)) => text.trim().parse::<f32>().ok(),
        Some(Value::Number(number)) => number.as_f64().map(|speed| speed as f32),
        _ => None,
    };
    saved
        .and_then(|saved| {
            VOICE_AGENT_SPEEDS
                .into_iter()
                .find(|speed| (speed - saved).abs() < 0.01)
        })
        .unwrap_or(1.0)
}

/// The setting's stored value for a speed: "1", "1.2", … "2".
fn speed_setting_value(speed: f32) -> String {
    let text = format!("{speed:.1}");
    text.strip_suffix(".0").map(str::to_string).unwrap_or(text)
}

/// gxserver's sentence for an error frame (a missing key's names the selected provider), with the
/// panel's wording when it sent none.
fn error_sentence(event: &Value) -> String {
    let message = event["message"].as_str().map(str::trim).unwrap_or_default();
    match event["code"].as_str() {
        Some("noKey") if message.is_empty() => {
            "Add your API key in Settings › Extensions › Voice agent to talk to the voice agent."
                .to_string()
        }
        Some("badKey") if message.is_empty() => {
            "Your API key was refused. Check it in Settings › Extensions › Voice agent.".to_string()
        }
        Some("badKey") => format!("{message} Check it in Settings › Extensions › Voice agent."),
        Some("providerUnavailable") if message.is_empty() => {
            "The voice provider can't be reached right now. Try again in a moment.".to_string()
        }
        _ if !message.is_empty() => message.to_string(),
        _ => "The voice agent ran into a problem.".to_string(),
    }
}

/// Why a conversation ended, for the panel.
fn ended_sentence(reason: &str, message: Option<&str>) -> String {
    match reason {
        "idle" => "Hung up after 5 minutes with nobody talking.".to_string(),
        "providerClosed" => "The voice provider ended the conversation.".to_string(),
        "replaced" => "This conversation continued on another device.".to_string(),
        "serverShutdown" => {
            "Ghostex's service restarted, which ended the conversation.".to_string()
        }
        "detachedTooLong" => "The conversation ended while this window was away.".to_string(),
        _ => message
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .unwrap_or("The conversation ended.")
            .to_string(),
    }
}

#[cfg(test)]
mod turned_off_tests {
    use super::*;

    fn settings(voice_agent_hidden: Option<bool>) -> serde_json::Map<String, Value> {
        let mut settings = serde_json::Map::new();
        if let Some(hidden) = voice_agent_hidden {
            settings.insert("voiceAgentHidden".to_string(), json!(hidden));
        }
        settings
    }

    #[test]
    fn the_voice_button_and_calls_follow_the_switch() {
        assert_eq!(voice_agent_on_in(&settings(None)), VOICE_AGENT_AVAILABLE);
        assert_eq!(
            voice_agent_on_in(&settings(Some(false))),
            VOICE_AGENT_AVAILABLE
        );
        assert!(!voice_agent_on_in(&settings(Some(true))));
    }

    #[test]
    fn turning_it_off_hangs_up_an_open_call_and_closes_a_finished_panel() {
        use TurnedOffStep::*;
        for phase in [VoicePhase::Starting, VoicePhase::Live] {
            assert_eq!(turned_off_step(false, phase), HangUp, "{phase:?}");
            assert_eq!(turned_off_step(true, phase), Nothing, "{phase:?}");
        }
        assert_eq!(turned_off_step(false, VoicePhase::Ended), ClosePanel);
        assert_eq!(turned_off_step(false, VoicePhase::Ending), Nothing);
        assert_eq!(turned_off_step(false, VoicePhase::Idle), Nothing);
    }
}
