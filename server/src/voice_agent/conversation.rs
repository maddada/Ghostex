//! One voice conversation: the provider socket, the attached client (if any), the tool calls in
//! flight, playback bookkeeping for interruptions, announcements and the idle hang-up. It runs as
//! one task that owns all of that state; clients and tool tasks talk to it through `Input`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use bytes::Bytes;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::control::{self, OpenConfirm};
use super::focus::{self, FocusScope};
use super::memory;
use super::paragraphs;
use super::prompt;
use super::provider::{Provider, ProviderSocket, BYTES_PER_MS};
use super::registry;
use super::sessions::SessionInfo;
use super::talk::{Cancel, Press, Release, Talk};
use super::tool_runs::ToolRuns;
use super::tools::{self, ToolOutcome};
use crate::server::AppState;

mod card;

/// CDXC:VoiceMode 2026-10-20 DECISION:
/// User: the voice agent "hangs up by itself after 5 minutes with nobody talking" because realtime is billed while connected. Talking is user speech the provider detected, typed text, or the agent's audio answering the user; the timer keeps running while no client is attached.
/// CDXC:VoiceMode 2026-10-20 WHY:
/// Announcements do not count as talking: with busy sessions they would otherwise keep a forgotten conversation (and its bill) open for ever.
pub(super) const IDLE_HANGUP: Duration = Duration::from_secs(5 * 60);
/// CDXC:VoiceMode 2026-10-20 DECISION:
/// User: the phone keeps the conversation going with the screen locked. A socket that closes without `end` leaves the conversation (and its provider session) open this long so the phone can reattach after a network change or a backgrounded app; captions in between are replayed.
pub(super) const DETACH_GRACE: Duration = Duration::from_secs(120);
/// Captions and announcements kept for a client that reattaches.
const REPLAY_LIMIT: usize = 50;
/// How long gxserver waits for the client's own playback position after an interruption before it
/// cuts the reply at its own estimate.
const TRUNCATE_WAIT: Duration = Duration::from_millis(400);
const TICK: Duration = Duration::from_millis(100);
/// How long gxserver waits for the provider's transcript of a cut reply before it sends its own
/// estimate.
const CUT_CAPTION_WAIT: Duration = Duration::from_millis(1_200);
/// How long gxserver waits for the provider to accept a voice change before it keeps the voice
/// for the next conversation.
const VOICE_CHANGE_WAIT: Duration = Duration::from_secs(5);
/// The words of `focusNotFound`, at the start and for a move.
pub(super) const FOCUS_NOT_FOUND: &str = "That session isn't open on this computer anymore.";
/// The client playback rates gxserver accepts (the desktop offers 1x to 2x).
const PLAYBACK_RATE_RANGE: std::ops::RangeInclusive<f64> = 0.5..=3.0;

pub(super) enum ClientFrame {
    Json(Value),
    Audio(Bytes),
    Close,
}

/// A session changed state while the conversation was open.
#[derive(Clone, Debug)]
pub(super) struct Announcement {
    pub(super) text: String,
    pub(super) kind: &'static str,
    pub(super) project_id: String,
    pub(super) session_id: String,
    /// The connected computer the session runs on, when it is not this one.
    pub(super) server_id: Option<String>,
    /// What the model is told instead of `text`, when it needs more (a focused call's finished
    /// turn carries the reply).
    pub(super) model_text: Option<String>,
}

pub(super) enum Input {
    /// The token the client about to attach opened its socket with (`loopback::ToolCaller`).
    Caller(String),
    Attach {
        attach_id: u64,
        out: mpsc::UnboundedSender<ClientFrame>,
        resumed: bool,
    },
    Detach {
        attach_id: u64,
    },
    Audio(Bytes),
    Text(String),
    Interrupt,
    Playback {
        item_id: String,
        played_ms: u64,
    },
    End,
    /// The voice agent was turned off on Settings > Extensions (`feature.rs`): the call ends.
    TurnedOff,
    /// GBots were turned off during a call with a GBot (`gbot_call/watch.rs`): the call ends.
    GbotsOff(String),
    /// The user picked another voice for the current provider.
    Voice(String),
    /// How fast the attached client plays the agent's audio (1.0 is as the provider sent it).
    PlaybackRate(f64),
    /// The user pressed the hold button (or Space, or the Hold to Talk hotkey): a push-to-talk
    /// turn starts, or a muted Regular call listens until `TalkEnd` (`talk.rs`).
    TalkStart,
    /// The user let go: a push-to-talk turn is sent, a Regular call's hold ends.
    TalkEnd,
    /// The user slid off the button: the hold is dropped and nothing is sent.
    TalkCancel,
    /// The client's Mute; gxserver drops the microphone's audio while it is on.
    Mute(bool),
    /// Hand the remembered last conversation to the agent (Continue).
    ContinueMemory,
    /// The user forgot the remembered conversation.
    MemoryForgotten,
    Announce(Vec<Announcement>),
    /// A focused session's finished turn, with the reply the model retells (`model_text`).
    AnnounceReady(Announcement),
    /// The announcement watcher's fresh session list (a focused call follows its session in it).
    Sessions(Arc<Vec<SessionInfo>>),
    /// The client asked to move the call to a session (`Some(ref)`) or to all sessions (`None`).
    Focus(Option<String>),
    /// A move the client asked for, looked up: the new scope and its briefing, or the ref that
    /// names no open session.
    FocusResolved(Result<(FocusScope, String), String>),
    /// The briefing of the session a focused call started about, read once a client attached.
    Briefing(String),
    /// A client frame gxserver could not read; answered with a non-fatal `badRequest`.
    BadRequest(String),
    ToolDone {
        call_id: String,
        name: String,
        label: String,
        outcome: ToolOutcome,
    },
    /// The user pressed a button on the confirm card `id`: Stop on `Some(session)`, else Cancel.
    Confirm {
        id: String,
        session: Option<String>,
    },
    /// The stop the user's Stop on the card ran (`control.rs`).
    CardStopDone {
        call_id: String,
        title: String,
        label: String,
        outcome: ToolOutcome,
    },
}

/// The agent reply whose audio is being relayed (or was, last).
struct ItemAudio {
    item_id: String,
    audio_ms: u64,
    /// The provider sent all of this reply's audio.
    audio_done: bool,
}

/// An interrupted reply waiting for its `cut` caption: the provider's transcript of what was
/// played (xAI sends it with `conversation.item.truncated`), else gxserver's estimate.
struct CutPending {
    item_id: String,
    estimate: String,
    deadline: Instant,
}

/// A voice change sent to the provider and not answered yet.
struct VoiceChange {
    voice: String,
    event_id: String,
    deadline: Instant,
}

/// The cut caption's text when the provider gives none: the share of the reply's text that its
/// played audio covers, ended at a word. While audio was still arriving the reply's length is
/// unknown, so a speaking rate of 15 characters a second stands in.
fn played_text(text: &str, played_ms: u64, item: Option<&ItemAudio>) -> String {
    let text = text.trim();
    let total = text.chars().count();
    let chars = match item {
        Some(item) if item.audio_done && item.audio_ms > 0 => {
            (total as u64 * played_ms.min(item.audio_ms) / item.audio_ms) as usize
        }
        _ => (played_ms as usize * 15 / 1000).min(total),
    };
    if chars >= total {
        return text.to_string();
    }
    let cut: String = text.chars().take(chars).collect();
    let next_is_space = text.chars().nth(chars).is_some_and(char::is_whitespace);
    let cut = if next_is_space {
        cut
    } else {
        match cut.rfind(char::is_whitespace) {
            Some(index) => cut[..index].to_string(),
            None => String::new(),
        }
    };
    cut.trim_end_matches(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ':')
        .to_string()
}

pub(super) struct Conversation {
    id: String,
    state: Arc<AppState>,
    provider: Provider,
    voice: String,
    /// The `session.update` the conversation was configured with, resent with another voice.
    session_update: Value,
    voice_change: Option<VoiceChange>,
    voice_changes: u64,
    /// The provider has sent agent audio (OpenAI's voice is fixed from then on).
    audio_sent: bool,
    /// A push-to-talk call (no turn detection), fixed when the call started.
    push_to_talk: bool,
    /// The hold to talk and the client's Mute (`talk.rs`).
    talk: Talk,
    /// CDXC:VoiceMode 2026-10-20 WHY:
    /// The speed buttons stretch the agent's audio on the client (pitch kept), so a reply plays in less time than the provider's audio lasts. gxserver's playback clock (when the reply ends, how much was heard at an interruption, when a tool follow-up may start) runs at this rate; `playedMs` and `audio_end_ms` stay in the provider's audio time.
    playback_rate: f64,
    sink: SplitSink<ProviderSocket, WsMessage>,
    stream: SplitStream<ProviderSocket>,
    rx: mpsc::UnboundedReceiver<Input>,
    tx: mpsc::UnboundedSender<Input>,
    client: Option<(u64, mpsc::UnboundedSender<ClientFrame>)>,
    detached_since: Option<Instant>,
    replay: VecDeque<Value>,
    /// Which tool calls clients draw as one row (`tool_runs.rs`).
    tool_runs: ToolRuns,
    last_talk: Instant,
    client_state: &'static str,
    response_active: bool,
    seen_calls: HashSet<String>,
    pending_calls: HashSet<String>,
    outputs_waiting: bool,
    create_at: Option<Instant>,
    playback_end: Instant,
    item: Option<ItemAudio>,
    agent_captions: HashMap<String, String>,
    user_captions: HashMap<String, String>,
    truncate_pending: Option<(String, u64, Instant)>,
    /// The full caption text of recent agent replies, kept after `final` so a cut can be trimmed.
    agent_texts: HashMap<String, String>,
    /// Replies the user talked over: their later caption events are not passed on.
    cut_items: HashSet<String>,
    cut_pending: Option<CutPending>,
    user_speaking: bool,
    announcements: Vec<Announcement>,
    typed_turns: u64,
    memory: Arc<tools::ToolMemory>,
    /// The reply in progress was asked for by an announcement, not by the user.
    announcing: bool,
    /// What was said and done, for the summary made when the call ends.
    call_log: memory::CallLog,
    /// The session the call is about now (`focus.rs`), `None` for all sessions.
    focus: Option<FocusScope>,
    /// The call started about one session: it never offers the remembered conversation.
    started_focused: bool,
    /// The start's briefing is still to be read (on the first attach).
    briefing_pending: bool,
    scope_changes: u64,
    /// The confirm card on screen, if any (`control.rs`).
    confirm: Option<OpenConfirm>,
}

enum Ended {
    Reason(&'static str, String),
}

impl Conversation {
    pub(super) fn new(
        id: String,
        state: Arc<AppState>,
        provider: Provider,
        voice: String,
        session_update: Value,
        push_to_talk: bool,
        focus: Option<FocusScope>,
        client_remote: bool,
        socket: ProviderSocket,
        tx: mpsc::UnboundedSender<Input>,
        rx: mpsc::UnboundedReceiver<Input>,
    ) -> Self {
        let (sink, stream) = socket.split();
        let now = Instant::now();
        let call_log = memory::CallLog::new(&state.paths, &id, provider);
        let memory = tools::ToolMemory::new(client_remote);
        memory.set_scope(focus.clone());
        registry::set_focus(&id, focus::client_view(focus.as_ref()));
        Self {
            id,
            state,
            provider,
            voice,
            session_update,
            voice_change: None,
            voice_changes: 0,
            audio_sent: false,
            push_to_talk,
            talk: Talk::new(push_to_talk),
            playback_rate: 1.0,
            sink,
            stream,
            rx,
            tx,
            client: None,
            detached_since: Some(now),
            replay: VecDeque::new(),
            tool_runs: ToolRuns::default(),
            last_talk: now,
            client_state: "listening",
            response_active: false,
            seen_calls: HashSet::new(),
            pending_calls: HashSet::new(),
            outputs_waiting: false,
            create_at: None,
            playback_end: now,
            item: None,
            agent_captions: HashMap::new(),
            user_captions: HashMap::new(),
            truncate_pending: None,
            agent_texts: HashMap::new(),
            cut_items: HashSet::new(),
            cut_pending: None,
            user_speaking: false,
            announcements: Vec::new(),
            typed_turns: 0,
            memory: Arc::new(memory),
            announcing: false,
            call_log,
            started_focused: focus.is_some(),
            confirm: None,
            briefing_pending: focus.is_some(),
            focus,
            scope_changes: 0,
        }
    }

    pub(super) async fn run(mut self) {
        let mut shutdown = self.state.shutdown_tx.subscribe();
        let mut tick = crate::ticks::steady_interval(TICK);
        let ended = loop {
            let step = tokio::select! {
                frame = self.stream.next() => match frame {
                    Some(Ok(WsMessage::Text(text))) => self.on_provider_event(text.as_str()).await,
                    Some(Ok(WsMessage::Close(frame))) => Err(Ended::Reason(
                        "providerClosed",
                        frame.map(|frame| frame.reason.to_string()).filter(|reason| !reason.is_empty())
                            .unwrap_or_else(|| "The provider ended the conversation.".to_string()),
                    )),
                    Some(Ok(_)) => Ok(()),
                    Some(Err(error)) => Err(Ended::Reason("providerClosed", format!("The provider connection failed: {error}"))),
                    None => Err(Ended::Reason("providerClosed", "The provider ended the conversation.".to_string())),
                },
                input = self.rx.recv() => match input {
                    Some(input) => self.on_input(input).await,
                    None => Err(Ended::Reason("error", "The conversation lost its controller.".to_string())),
                },
                _ = tick.tick() => self.on_tick().await,
                _ = shutdown.recv() => Err(Ended::Reason("serverShutdown", "Ghostex's server is restarting.".to_string())),
            };
            if let Err(ended) = step {
                break ended;
            }
        };
        let Ended::Reason(reason, message) = ended;
        self.finish(reason, &message).await;
    }

    async fn finish(&mut self, reason: &'static str, message: &str) {
        registry::remove(&self.id);
        if matches!(reason, "providerClosed" | "error") {
            super::warn(
                &self.state,
                "voiceAgent.ended",
                json!({"provider": self.provider.id(), "reason": reason, "message": message}),
            );
        }
        self.to_client(json!({"type": "ended", "reason": reason, "message": message}));
        if let Some((_, out)) = &self.client {
            let _ = out.send(ClientFrame::Close);
        }
        let _ = self.sink.send(WsMessage::Close(None)).await;
        memory::call_ended(self.state.clone(), std::mem::take(&mut self.call_log));
    }

    /// Sends a JSON frame to the attached client; captions and announcements are kept for a
    /// client that reattaches.
    fn to_client(&mut self, mut frame: Value) {
        self.tool_runs.frame(&mut frame);
        if frame.get("type").and_then(Value::as_str) == Some("caption")
            && frame.get("role").and_then(Value::as_str) == Some("agent")
        {
            let parts = paragraphs::paragraphs(frame["text"].as_str().unwrap_or_default());
            if parts.len() > 1 {
                frame["paragraphs"] = json!(parts);
            }
        }
        let kind = frame
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let keep = matches!(kind, "caption" | "announcement" | "tool")
            && (kind != "caption" || frame.get("final").and_then(Value::as_bool) == Some(true));
        if keep {
            // CDXC:VoiceMode 2026-10-23 DECISION: User, asked "What should a call about one session do with the voice agent's memory of your last call?", picked "Leave it alone: no Continue card, and it does not replace the remembered call". Nothing said while the call is about one session goes into the summary.
            // CDXC:VoiceMode 2026-10-23 WHY: a call widened to all sessions partway is an all-sessions call from then on, so what is said after widening is recorded and may replace the remembered call; a call about one session the whole time records nothing and leaves it as it was. The session's own chat is the memory of the focused part.
            if self.focus.is_none() {
                self.call_log.record(&frame);
            }
            self.replay.push_back(frame.clone());
            while self.replay.len() > REPLAY_LIMIT {
                self.replay.pop_front();
            }
        }
        if let Some((_, out)) = &self.client {
            let _ = out.send(ClientFrame::Json(frame));
        }
    }

    fn set_state(&mut self, state: &'static str) {
        if self.client_state != state {
            self.client_state = state;
            self.to_client(json!({"type": "state", "state": state}));
        }
    }

    async fn provider_send(&mut self, event: Value) -> Result<(), Ended> {
        self.sink
            .send(WsMessage::Text(event.to_string().into()))
            .await
            .map_err(|error| {
                Ended::Reason(
                    "providerClosed",
                    format!("The provider connection failed: {error}"),
                )
            })
    }

    fn playing(&self) -> bool {
        self.playback_end > Instant::now()
    }

    /// Nothing is being said, generated or run: the moment for an announcement or a hang-up.
    fn quiet(&self) -> bool {
        !self.user_speaking
            && !self.response_active
            && self.pending_calls.is_empty()
            && self.create_at.is_none()
            && !self.playing()
    }

    async fn on_input(&mut self, input: Input) -> Result<(), Ended> {
        match input {
            Input::Caller(token) => self.memory.set_caller_token(token),
            Input::Attach {
                attach_id,
                out,
                resumed,
            } => {
                if let Some((_, previous)) = self.client.take() {
                    let _ = previous.send(ClientFrame::Json(json!({
                        "type": "ended",
                        "reason": "replaced",
                        "message": "Another device took over this conversation.",
                    })));
                    let _ = previous.send(ClientFrame::Close);
                }
                let _ = out.send(ClientFrame::Json(json!({
                    "type": "ready",
                    "conversationId": self.id,
                    "provider": self.provider.id(),
                    "voice": self.voice,
                    "voiceOptions": self.provider.voices(),
                    "callMode": if self.push_to_talk { "pushToTalk" } else { "regular" },
                    "resumed": resumed,
                    "memory": if self.started_focused { Value::Null } else { memory::client_view(&self.state.paths) },
                    "memoryContinued": !self.started_focused && self.call_log.continued(),
                    "focus": focus::client_view(self.focus.as_ref()),
                    "idleHangupMs": IDLE_HANGUP.as_millis() as u64,
                    "audio": {"encoding": "pcm16", "sampleRate": super::provider::SAMPLE_RATE, "channels": 1},
                })));
                if resumed {
                    for frame in &self.replay {
                        let _ = out.send(ClientFrame::Json(frame.clone()));
                    }
                }
                let _ = out.send(ClientFrame::Json(
                    json!({"type": "state", "state": self.client_state}),
                ));
                self.client = Some((attach_id, out));
                self.detached_since = None;
                self.talk.attached();
                if std::mem::take(&mut self.briefing_pending) {
                    self.read_briefing();
                }
                // Each client reports its own playback rate after `ready`.
                self.playback_rate = 1.0;
                registry::set_attached(&self.id, true);
            }
            Input::Detach { attach_id } => {
                if self.client.as_ref().is_some_and(|(id, _)| *id == attach_id) {
                    self.client = None;
                    self.detached_since = Some(Instant::now());
                    registry::set_attached(&self.id, false);
                    // Nobody can hear the rest of the reply.
                    self.playback_end = Instant::now();
                    if let Some(release) = self.talk.detached(Instant::now()) {
                        self.apply_talk_release(release).await?;
                    }
                }
            }
            Input::Audio(bytes) => {
                // Push-to-talk listens only while Talk is held, and nobody while muted.
                if bytes.len() >= 2 && self.talk.passes_audio((bytes.len() / BYTES_PER_MS) as u64) {
                    if self.talk.held() {
                        self.last_talk = Instant::now();
                    }
                    let even = &bytes[..bytes.len() - bytes.len() % 2];
                    let audio = base64::engine::general_purpose::STANDARD.encode(even);
                    self.provider_send(
                        json!({"type": "input_audio_buffer.append", "audio": audio}),
                    )
                    .await?;
                }
            }
            Input::Text(text) => {
                let text = text.trim().to_string();
                if !text.is_empty() {
                    self.last_talk = Instant::now();
                    self.typed_turns += 1;
                    let item_id = format!("typed-{}", self.typed_turns);
                    self.to_client(json!({"type": "caption", "role": "user", "itemId": item_id, "text": text, "final": true}));
                    if self.response_active {
                        self.provider_send(json!({"type": "response.cancel"}))
                            .await?;
                    }
                    self.stop_playback(false).await?;
                    self.provider_send(json!({
                        "type": "conversation.item.create",
                        "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]},
                    }))
                    .await?;
                    self.create_response_now().await?;
                }
            }
            Input::Interrupt => {
                if self.response_active {
                    self.provider_send(json!({"type": "response.cancel"}))
                        .await?;
                }
                self.stop_playback(true).await?;
                self.set_state("listening");
            }
            Input::Playback { item_id, played_ms } => {
                if self
                    .truncate_pending
                    .as_ref()
                    .is_some_and(|(pending, _, _)| *pending == item_id)
                {
                    let total = self
                        .item
                        .as_ref()
                        .filter(|item| item.item_id == item_id)
                        .map_or(played_ms, |item| item.audio_ms);
                    self.truncate(item_id, played_ms.min(total)).await?;
                }
            }
            Input::Voice(voice) => self.change_voice(voice).await?,
            Input::TalkStart => self.talk_start().await?,
            Input::TalkEnd => self.talk_end().await?,
            Input::TalkCancel => self.talk_cancel().await?,
            Input::Mute(muted) => {
                if self.talk.set_muted(muted) == Cancel::Cleared {
                    self.drop_held_turn().await?;
                }
            }
            Input::PlaybackRate(rate) => {
                let rate = rate.clamp(*PLAYBACK_RATE_RANGE.start(), *PLAYBACK_RATE_RANGE.end());
                let now = Instant::now();
                if self.playback_end > now {
                    // What is still queued now plays at the new rate.
                    let left = self.playback_end.duration_since(now).as_secs_f64();
                    self.playback_end =
                        now + Duration::from_secs_f64(left * self.playback_rate / rate);
                }
                self.playback_rate = rate;
            }
            Input::ContinueMemory if self.started_focused || self.focus.is_some() => {
                self.to_client(json!({"type": "memory", "state": "none"}));
            }
            Input::ContinueMemory => self.continue_memory().await?,
            Input::MemoryForgotten => {
                self.call_log.set_continued(false);
                self.to_client(json!({"type": "memory", "state": "forgotten"}));
            }
            Input::End => {
                return Err(Ended::Reason(
                    "user",
                    "You ended the conversation.".to_string(),
                ));
            }
            Input::GbotsOff(message) => return Err(Ended::Reason("gbotsOff", message)),
            Input::TurnedOff => {
                return Err(Ended::Reason(
                    super::feature::TURNED_OFF,
                    super::feature::voice_agent_off_message(),
                ));
            }
            Input::Announce(announcements) => {
                for announcement in announcements {
                    if let Some(scope) = &self.focus {
                        if !scope.wants(&announcement) {
                            continue;
                        }
                    }
                    let finished_focus = announcement.kind == "turnFinished"
                        && announcement.server_id.is_none()
                        && self.focus.as_ref().is_some_and(|scope| {
                            scope.project_id == announcement.project_id
                                && scope.session_id == announcement.session_id
                        });
                    let mut frame = json!({
                        "type": "announcement",
                        "text": announcement.text,
                        "kind": announcement.kind,
                        "projectId": announcement.project_id,
                        "sessionId": announcement.session_id,
                    });
                    if let Some(server_id) = &announcement.server_id {
                        frame["serverId"] = json!(server_id);
                    }
                    self.to_client(frame);
                    if finished_focus {
                        self.read_finished_reply(announcement);
                    } else {
                        self.announcements.push(announcement);
                    }
                }
            }
            Input::AnnounceReady(announcement) => self.announcements.push(announcement),
            Input::Sessions(sessions) => self.follow_focus(&sessions),
            // A call with a GBot stays with it (gbot_call/): no widening, no moving.
            Input::Focus(_) if self.focus.as_ref().is_some_and(|scope| scope.gbot.is_some()) => {
                self.to_client(json!({
                    "type": "error",
                    "code": "badRequest",
                    "message": super::gbot_call::STAYS_MESSAGE,
                    "fatal": false,
                }));
                self.to_client(
                    json!({"type": "focus", "focus": focus::client_view(self.focus.as_ref())}),
                );
            }
            Input::Focus(None) if self.focus.is_some() => {
                let previous = self.focus.as_ref().map(|scope| scope.title.clone());
                self.last_talk = Instant::now();
                self.apply_scope(None, focus::widened_message(previous.as_deref()), true)
                    .await?;
            }
            Input::Focus(request) => self.request_focus(request),
            Input::FocusResolved(Ok((scope, briefing))) => {
                self.last_talk = Instant::now();
                self.apply_scope(Some(scope), briefing, true).await?;
            }
            Input::FocusResolved(Err(reference)) => {
                self.to_client(json!({
                    "type": "error",
                    "code": "focusNotFound",
                    "message": FOCUS_NOT_FOUND,
                    "fatal": false,
                    "session": reference,
                }));
            }
            Input::Briefing(briefing) => self.add_scope_message(briefing, true).await?,
            Input::BadRequest(message) => {
                self.to_client(json!({"type": "error", "code": "badRequest", "message": message, "fatal": false}));
            }
            Input::ToolDone {
                call_id,
                name,
                label,
                outcome,
            } => {
                let mut finished = json!({
                    "type": "tool",
                    "phase": "finished",
                    "callId": call_id,
                    "name": name,
                    "label": label,
                    "ok": outcome.ok,
                });
                if outcome.ok {
                    if let Some(session) = super::show::shown_session(&name, &outcome.output) {
                        // A session on another computer goes in a field of its own, so a client
                        // that would open it by its ids on this computer (the phone's chip, an
                        // older desktop) leaves it alone; the desktop shows it on that computer.
                        if session.get("serverId").is_some() {
                            finished["computerSession"] = session;
                            if name == "show_session" {
                                finished["show"] = json!("now");
                            } else if self.focus.is_none() {
                                finished["show"] = json!("follow");
                            }
                        } else {
                            finished["session"] = session;
                            // A client on another computer shows the session itself.
                            if self.memory.client_remote() {
                                if name == "show_session" {
                                    finished["show"] = json!("now");
                                } else if self.focus.is_none() {
                                    finished["show"] = json!("follow");
                                }
                            }
                        }
                    }
                }
                let mut confirm = None;
                if name == "stop_session" {
                    confirm = control::confirm_frame(&call_id, &outcome.output);
                    if confirm.is_some() {
                        finished["label"] = json!(control::UNSURE_LABEL);
                        self.close_confirm("replaced");
                    } else if let Some(stopped) = control::stopped_ref(&outcome.output) {
                        if self
                            .confirm
                            .as_ref()
                            .is_some_and(|open| open.title_of(stopped).is_some())
                        {
                            self.close_confirm("stopped");
                        }
                    }
                }
                if name == "widen_call"
                    && outcome.output.get("widened").and_then(Value::as_bool) == Some(true)
                {
                    let previous = self.focus.as_ref().map(|scope| scope.title.clone());
                    self.apply_scope(None, focus::widened_message(previous.as_deref()), false)
                        .await?;
                }
                self.to_client(finished);
                if let Some(frame) = confirm {
                    self.confirm = OpenConfirm::from_frame(&frame);
                    self.to_client(frame);
                }
                if name == "recall_last_conversation" && outcome.ok {
                    self.memory_recalled();
                }
                self.provider_send(json!({
                    "type": "conversation.item.create",
                    "item": {"type": "function_call_output", "call_id": call_id, "output": outcome.output.to_string()},
                }))
                .await?;
                self.pending_calls.remove(&call_id);
                self.outputs_waiting = true;
                self.schedule_response_after_tools();
            }
            Input::Confirm { id, session } => self.answer_confirm(id, session).await?,
            Input::CardStopDone {
                call_id,
                title,
                label,
                outcome,
            } => {
                self.to_client(json!({"type": "tool", "phase": "finished", "callId": call_id, "name": "stop_session", "label": label, "ok": outcome.ok}));
                self.add_user_item(control::card_stop_message(&title, &outcome.output))
                    .await?;
                self.outputs_waiting = true;
                self.schedule_response_after_tools();
            }
        }
        Ok(())
    }

    /// Once every tool of a reply has answered and the reply itself is done, ask for the next
    /// reply after the current audio has played, so the follow-up does not talk over it.
    fn schedule_response_after_tools(&mut self) {
        if self.outputs_waiting && self.pending_calls.is_empty() && !self.response_active {
            self.create_at = Some(self.playback_end.max(Instant::now()));
        }
    }

    async fn create_response_now(&mut self) -> Result<(), Ended> {
        self.create_at = None;
        self.outputs_waiting = false;
        self.announcing = false;
        self.set_state("thinking");
        self.provider_send(json!({"type": "response.create"})).await
    }

    /// Stops relaying the current reply's audio: tells the client to drop what it queued and cuts
    /// the unplayed part from the conversation (now, or once the client reports where it stopped).
    async fn stop_playback(&mut self, notify: bool) -> Result<(), Ended> {
        let now = Instant::now();
        if self.playback_end > now {
            if let Some(item) = &self.item {
                let unplayed = (self.playback_end.duration_since(now).as_secs_f64()
                    * 1000.0
                    * self.playback_rate) as u64;
                let estimate = item.audio_ms.saturating_sub(unplayed);
                let item_id = item.item_id.clone();
                if notify || self.client.is_some() {
                    self.to_client(json!({"type": "interrupted", "itemId": item_id}));
                }
                // Its caption stops here; the `cut` caption follows once the cut point is known.
                self.agent_captions.remove(&item_id);
                self.cut_items.insert(item_id.clone());
                self.truncate_pending = Some((item_id, estimate, now + TRUNCATE_WAIT));
            }
            self.playback_end = now;
        }
        Ok(())
    }

    async fn truncate(&mut self, item_id: String, audio_end_ms: u64) -> Result<(), Ended> {
        self.truncate_pending = None;
        let text = self.agent_texts.get(&item_id).cloned().unwrap_or_default();
        let item = self.item.as_ref().filter(|item| item.item_id == item_id);
        self.cut_pending = Some(CutPending {
            estimate: played_text(&text, audio_end_ms, item),
            item_id: item_id.clone(),
            deadline: Instant::now() + CUT_CAPTION_WAIT,
        });
        self.provider_send(json!({
            "type": "conversation.item.truncate",
            "item_id": item_id,
            "content_index": 0,
            "audio_end_ms": audio_end_ms,
        }))
        .await
    }

    /// CDXC:VoiceMode 2026-10-20 WHY:
    /// Every interruption ends with a `cut` caption of what was actually heard. Grok finishes a reply's text well before its audio has played, so closing the growing caption at the interruption (the first version) usually sent nothing and clients showed the whole reply as heard (found by the phone thread).
    fn send_cut_caption(&mut self, provider_text: Option<String>) {
        let Some(cut) = self.cut_pending.take() else {
            return;
        };
        let text = provider_text
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or(cut.estimate);
        self.to_client(json!({
            "type": "caption",
            "role": "agent",
            "itemId": cut.item_id,
            "text": text,
            "final": true,
            "cut": true,
        }));
    }

    async fn on_tick(&mut self) -> Result<(), Ended> {
        let now = Instant::now();
        if self.talk.wants_silence(now) {
            // A muted Regular call's hold ended: the provider hears the quiet it needs to end the turn.
            let silence = vec![0u8; TICK.as_millis() as usize * BYTES_PER_MS];
            let audio = base64::engine::general_purpose::STANDARD.encode(silence);
            self.provider_send(json!({"type": "input_audio_buffer.append", "audio": audio}))
                .await?;
        }
        if self
            .cut_pending
            .as_ref()
            .is_some_and(|cut| cut.deadline <= now)
        {
            self.send_cut_caption(None);
        }
        if self
            .voice_change
            .as_ref()
            .is_some_and(|change| change.deadline <= now)
        {
            let change = self.voice_change.take();
            if let Some(change) = change {
                self.voice_answer(&change.voice, "nextConversation", "noAnswer");
            }
        }
        if let Some((item_id, estimate, deadline)) = self.truncate_pending.clone() {
            if deadline <= now {
                self.truncate(item_id, estimate).await?;
            }
        }
        if self.create_at.is_some_and(|at| at <= now) {
            self.create_response_now().await?;
        }
        if self.client_state == "speaking" && !self.playing() && !self.response_active {
            let next = if self.pending_calls.is_empty() {
                "listening"
            } else {
                "toolRunning"
            };
            self.set_state(next);
        }
        if !self.announcements.is_empty() && self.quiet() {
            let lines: Vec<String> = self
                .announcements
                .drain(..)
                .map(|a| a.model_text.unwrap_or(a.text))
                .collect();
            self.provider_send(json!({
                "type": "conversation.item.create",
                "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": prompt::announcement_message(&lines)}]},
            }))
            .await?;
            self.create_response_now().await?;
            self.announcing = true;
        }
        if let Some(since) = self.detached_since {
            if now.duration_since(since) >= DETACH_GRACE {
                return Err(Ended::Reason(
                    "detachedTooLong",
                    "No device reconnected to the conversation, so it was ended.".to_string(),
                ));
            }
        }
        if now.duration_since(self.last_talk) >= IDLE_HANGUP && self.quiet() {
            return Err(Ended::Reason(
                "idle",
                "Nobody talked for 5 minutes, so the conversation was ended.".to_string(),
            ));
        }
        Ok(())
    }

    async fn on_provider_event(&mut self, text: &str) -> Result<(), Ended> {
        let Ok(event) = serde_json::from_str::<Value>(text) else {
            return Ok(());
        };
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let str_field = |key: &str| {
            event
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match kind {
            "input_audio_buffer.speech_started" => {
                self.user_speaking = true;
                self.last_talk = Instant::now();
                self.stop_playback(true).await?;
                // The reply to this speech will see the tool results; a separate one would repeat it.
                if self.create_at.take().is_some() {
                    self.outputs_waiting = false;
                }
                self.set_state("userSpeaking");
            }
            "input_audio_buffer.speech_stopped" => {
                self.talk.speech_stopped();
                self.user_speaking = false;
                self.last_talk = Instant::now();
                self.set_state("thinking");
            }
            "conversation.item.input_audio_transcription.delta" => {
                let item_id = str_field("item_id");
                let caption = self.user_captions.entry(item_id.clone()).or_default();
                caption.push_str(&str_field("delta"));
                let text = caption.clone();
                self.to_client(json!({"type": "caption", "role": "user", "itemId": item_id, "text": text, "final": false}));
            }
            "conversation.item.input_audio_transcription.updated" => {
                let item_id = str_field("item_id");
                let text = str_field("transcript");
                self.to_client(json!({"type": "caption", "role": "user", "itemId": item_id, "text": text, "final": false}));
            }
            "conversation.item.input_audio_transcription.completed" => {
                let item_id = str_field("item_id");
                self.user_captions.remove(&item_id);
                let text = str_field("transcript").trim().to_string();
                if !text.is_empty() {
                    self.to_client(json!({"type": "caption", "role": "user", "itemId": item_id, "text": text, "final": true}));
                }
            }
            "response.created" => {
                self.response_active = true;
                if self.client_state != "speaking" {
                    self.set_state("thinking");
                }
            }
            "response.output_audio.delta" | "response.audio.delta" => {
                let Ok(audio) =
                    base64::engine::general_purpose::STANDARD.decode(str_field("delta"))
                else {
                    return Ok(());
                };
                let item_id = str_field("item_id");
                let now = Instant::now();
                let ms = (audio.len() / BYTES_PER_MS) as u64;
                if !self.announcing {
                    self.last_talk = now;
                }
                if self.item.as_ref().map(|item| item.item_id.as_str()) != Some(item_id.as_str()) {
                    self.item = Some(ItemAudio {
                        item_id: item_id.clone(),
                        audio_ms: 0,
                        audio_done: false,
                    });
                    // Only the reply being relayed can be cut from now on.
                    self.agent_texts.retain(|id, _| *id == item_id);
                    self.to_client(json!({"type": "audioStart", "itemId": item_id}));
                }
                if let Some(item) = &mut self.item {
                    item.audio_ms += ms;
                }
                self.audio_sent = true;
                if self.client.is_some() {
                    self.playback_end = self.playback_end.max(now)
                        + Duration::from_secs_f64(ms as f64 / 1000.0 / self.playback_rate);
                    if let Some((_, out)) = &self.client {
                        let _ = out.send(ClientFrame::Audio(Bytes::from(audio)));
                    }
                }
                self.set_state("speaking");
            }
            "response.output_audio.done" | "response.audio.done" => {
                let item_id = str_field("item_id");
                if let Some(item) = self.item.as_mut().filter(|item| item.item_id == item_id) {
                    item.audio_done = true;
                }
                self.to_client(json!({"type": "audioDone", "itemId": item_id}));
            }
            "response.output_audio_transcript.delta" | "response.audio_transcript.delta" => {
                let item_id = str_field("item_id");
                self.agent_texts
                    .entry(item_id.clone())
                    .or_default()
                    .push_str(&str_field("delta"));
                if self.cut_items.contains(&item_id) {
                    return Ok(());
                }
                let caption = self.agent_captions.entry(item_id.clone()).or_default();
                caption.push_str(&str_field("delta"));
                let text = caption.clone();
                self.to_client(json!({"type": "caption", "role": "agent", "itemId": item_id, "text": text, "final": false}));
            }
            "response.output_audio_transcript.done" | "response.audio_transcript.done" => {
                let item_id = str_field("item_id");
                let streamed = self.agent_captions.remove(&item_id).unwrap_or_default();
                let text = Some(str_field("transcript"))
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or(streamed);
                if !text.trim().is_empty() {
                    self.agent_texts.insert(item_id.clone(), text.clone());
                }
                if self.cut_items.contains(&item_id) {
                    return Ok(());
                }
                self.to_client(json!({"type": "caption", "role": "agent", "itemId": item_id, "text": text.trim(), "final": true}));
            }
            "session.updated" => {
                if let Some(change) = self.voice_change.take() {
                    // xAI's `session.updated` does not echo the voice; only a different one refuses.
                    match self.provider.session_voice(&event) {
                        Some(voice) if voice != change.voice => {
                            super::warn(
                                &self.state,
                                "voiceAgent.voiceChangeIgnored",
                                json!({"provider": self.provider.id(), "voice": change.voice, "sessionVoice": voice}),
                            );
                            self.voice_answer(&change.voice, "nextConversation", "refused");
                        }
                        _ => {
                            self.voice = change.voice.clone();
                            self.voice_answer(&change.voice, "now", "");
                        }
                    }
                }
            }
            "conversation.item.truncated" => {
                if self
                    .cut_pending
                    .as_ref()
                    .is_some_and(|cut| cut.item_id == str_field("item_id"))
                {
                    self.send_cut_caption(Some(str_field("transcript")));
                }
            }
            "response.function_call_arguments.done" => {
                self.start_tool(
                    str_field("call_id"),
                    str_field("name"),
                    str_field("arguments"),
                );
            }
            "response.done" => {
                self.response_active = false;
                self.announcing = false;
                // What the call has cost so far (xAI: seconds billed since it started).
                if let Some(billable) = event.pointer("/usage/billable_audio_seconds") {
                    self.to_client(json!({"type": "usage", "billableAudioSeconds": billable}));
                }
                // OpenAI also lists the reply's function calls here; start any not seen yet.
                let calls: Vec<(String, String, String)> = event
                    .pointer("/response/output")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|item| {
                        item.get("type").and_then(Value::as_str) == Some("function_call")
                    })
                    .map(|item| {
                        let field = |key: &str| {
                            item.get(key)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string()
                        };
                        (field("call_id"), field("name"), field("arguments"))
                    })
                    .collect();
                for (call_id, name, arguments) in calls {
                    self.start_tool(call_id, name, arguments);
                }
                let status = event
                    .pointer("/response/status")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if status == "failed" {
                    let failure = json!({
                        "type": "response.done",
                        "error": event.pointer("/response/status_details/error").cloned().unwrap_or(Value::Null),
                    });
                    let report = self.provider.describe_error(&failure);
                    let message = if report.message == "The provider reported an error." {
                        "The provider could not answer.".to_string()
                    } else {
                        report.message.clone()
                    };
                    super::warn_error(
                        &self.state,
                        "voiceAgent.responseFailed",
                        &message,
                        report.details,
                    );
                    self.to_client(json!({"type": "error", "code": "providerError", "message": message, "fatal": false}));
                }
                self.schedule_response_after_tools();
                if !self.playing() && self.pending_calls.is_empty() && self.create_at.is_none() {
                    self.set_state("listening");
                }
            }
            "error" => {
                let error = event.get("error").cloned().unwrap_or_else(|| event.clone());
                let code = error
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let event_id = error
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(change) = self
                    .voice_change
                    .take_if(|change| !event_id.is_empty() && change.event_id == event_id)
                {
                    let report = self.provider.describe_error(&event);
                    let mut details = report.details;
                    details["voice"] = json!(change.voice);
                    super::warn_error(
                        &self.state,
                        "voiceAgent.voiceChangeRefused",
                        &report.message,
                        details,
                    );
                    self.voice_answer(&change.voice, "nextConversation", "refused");
                    return Ok(());
                }
                // Cancelling a reply that just finished is harmless.
                if code != "response_cancel_not_active"
                    && code != "conversation_already_has_active_response"
                {
                    let message = super::warn_provider_error(
                        &self.state,
                        "voiceAgent.providerError",
                        self.provider,
                        &event,
                    )
                    .message;
                    self.to_client(json!({"type": "error", "code": "providerError", "message": message, "fatal": false}));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Continue: adds the remembered conversation as a `[Ghostex memory]` message (the way
    /// announcements go in) and asks for a reply, which recaps it in a sentence.
    async fn continue_memory(&mut self) -> Result<(), Ended> {
        let Some(remembered) = memory::read(&self.state.paths) else {
            self.to_client(json!({"type": "memory", "state": "none"}));
            return Ok(());
        };
        if !self.call_log.continued() {
            self.call_log.set_continued(true);
            self.last_talk = Instant::now();
            if self.response_active {
                self.provider_send(json!({"type": "response.cancel"}))
                    .await?;
            }
            self.stop_playback(false).await?;
            self.provider_send(json!({
                "type": "conversation.item.create",
                "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": memory::continue_message(&remembered)}]},
            }))
            .await?;
            self.create_response_now().await?;
        }
        self.to_client(
            json!({"type": "memory", "state": "continued", "memory": remembered.to_client()}),
        );
        Ok(())
    }

    /// The agent fetched the remembered conversation itself ("pick up where we left off"): the
    /// clients show it as continued, and this call's summary carries it forward.
    fn memory_recalled(&mut self) {
        let Some(remembered) = memory::read(&self.state.paths) else {
            return;
        };
        self.call_log.set_continued(true);
        self.to_client(
            json!({"type": "memory", "state": "continued", "memory": remembered.to_client()}),
        );
    }

    /// Reads the focused session's briefing in the background; it arrives as `Input::Briefing`.
    fn read_briefing(&self) {
        let Some(scope) = self.focus.clone() else {
            return;
        };
        let (state, tx) = (self.state.clone(), self.tx.clone());
        tokio::spawn(async move {
            let facts = tools::briefing(&state, &scope).await;
            let _ = tx.send(Input::Briefing(focus::briefing_message(
                &scope, &facts, false,
            )));
        });
    }

    /// The focused session finished its turn: reads its reply, which the model retells, and
    /// queues the announcement with it (`Input::AnnounceReady`). A chat that can't be read still
    /// announces the finished turn.
    fn read_finished_reply(&self, mut announcement: Announcement) {
        let Some(scope) = self.focus.clone() else {
            return;
        };
        let (state, tx) = (self.state.clone(), self.tx.clone());
        tokio::spawn(async move {
            let reply = tools::finished_reply(&state, &scope).await;
            announcement.model_text =
                Some(focus::finished_message(&scope, reply.as_deref()));
            let _ = tx.send(Input::AnnounceReady(announcement));
        });
    }

    /// A fresh session list: a focused call follows its session's title and threads, and tells
    /// the client and the model once when the session was closed. The call keeps going.
    fn follow_focus(&mut self, sessions: &[SessionInfo]) {
        let Some(scope) = self.focus.as_mut() else {
            return;
        };
        let was_closed = scope.closed;
        let changed = scope.refresh(sessions);
        let scope = scope.clone();
        self.memory.set_scope(Some(scope.clone()));
        if !changed {
            return;
        }
        registry::set_focus(&self.id, scope.to_client());
        self.to_client(json!({"type": "focus", "focus": scope.to_client()}));
        if scope.closed && !was_closed {
            self.to_client(json!({
                "type": "announcement",
                "text": format!("\"{}\" was closed.", scope.title),
                "kind": "sessionClosed",
                "projectId": scope.project_id,
                "sessionId": scope.session_id,
            }));
            self.announcements.push(Announcement {
                text: format!("\"{}\" was closed.", scope.title),
                kind: "sessionClosed",
                project_id: scope.project_id.clone(),
                session_id: scope.session_id.clone(),
                server_id: None,
                model_text: Some(focus::closed_message(&scope.title)),
            });
        }
    }

    /// The client's `focus` frame for a move (widening is applied in `on_input`): looks the
    /// session up and reads its briefing in the background, then arrives as
    /// `Input::FocusResolved`. Asking for the scope the call already has just answers it again.
    fn request_focus(&mut self, request: Option<String>) {
        let current = self.focus.as_ref().map(FocusScope::reference);
        let Some(reference) = request.filter(|reference| Some(reference) != current.as_ref())
        else {
            self.to_client(
                json!({"type": "focus", "focus": focus::client_view(self.focus.as_ref())}),
            );
            return;
        };
        let (state, tx) = (self.state.clone(), self.tx.clone());
        tokio::spawn(async move {
            let sessions = super::sessions::list_sessions_async(&state)
                .await
                .unwrap_or_default();
            let resolved = match FocusScope::from_sessions(&sessions, &reference) {
                Some(scope) => {
                    let facts = tools::briefing(&state, &scope).await;
                    let briefing = focus::briefing_message(&scope, &facts, true);
                    Ok((scope, briefing))
                }
                None => Err(reference),
            };
            let _ = tx.send(Input::FocusResolved(resolved));
        });
    }

    /// Sets the call's scope: resends the session configuration with the scope's instructions and
    /// tools (the way a voice change resends it), tells the client, and hands the model `message`
    /// (the briefing, or that the call is about all sessions now). `respond` asks for a reply when
    /// nothing else is going on; a tool's own follow-up reply covers it otherwise.
    async fn apply_scope(
        &mut self,
        scope: Option<FocusScope>,
        message: String,
        respond: bool,
    ) -> Result<(), Ended> {
        self.focus = scope;
        self.memory.set_scope(self.focus.clone());
        let view = focus::client_view(self.focus.as_ref());
        registry::set_focus(&self.id, view.clone());
        self.scope_changes += 1;
        let mut update = self.session_update.clone();
        update["session"]["instructions"] = json!(prompt::instructions(self.focus.as_ref()));
        update["session"]["tools"] = tools::definitions(self.focus.as_ref());
        self.session_update = update.clone();
        // A voice change still waiting for its answer keeps its voice.
        if let Some(change) = &self.voice_change {
            update = self
                .provider
                .with_voice(&update, &change.voice, &change.event_id);
        } else {
            update["event_id"] = json!(format!("ghostex-scope-{}", self.scope_changes));
        }
        self.provider_send(update).await?;
        self.to_client(json!({"type": "focus", "focus": view}));
        self.add_scope_message(message, respond).await
    }

    /// Adds a `[Ghostex focus]` message to the conversation, and asks for a reply when `respond`
    /// and nobody is talking.
    async fn add_scope_message(&mut self, message: String, respond: bool) -> Result<(), Ended> {
        self.provider_send(json!({
            "type": "conversation.item.create",
            "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": message}]},
        }))
        .await?;
        if respond && self.quiet() {
            self.create_response_now().await?;
        }
        Ok(())
    }

    /// Switches the conversation to `voice` when the provider can, else keeps it for the next
    /// conversation (the client saved it as the setting). Answers with a `voice` frame.
    async fn change_voice(&mut self, voice: String) -> Result<(), Ended> {
        let voice = voice.trim().to_ascii_lowercase();
        if !self.provider.voices().contains(&voice.as_str()) {
            self.to_client(json!({"type": "error", "code": "badRequest", "message": format!("{voice} is not a {} voice.", self.provider.label()), "fatal": false}));
            return Ok(());
        }
        if voice == self.voice {
            self.voice_change = None;
            self.voice_answer(&voice, "now", "");
            return Ok(());
        }
        if self.provider.voice_locked_after_audio() && self.audio_sent {
            self.voice_answer(&voice, "nextConversation", "providerLocksVoice");
            return Ok(());
        }
        self.voice_changes += 1;
        let event_id = format!("ghostex-voice-{}", self.voice_changes);
        let update = self
            .provider
            .with_voice(&self.session_update, &voice, &event_id);
        self.provider_send(update).await?;
        self.voice_change = Some(VoiceChange {
            voice,
            event_id,
            deadline: Instant::now() + VOICE_CHANGE_WAIT,
        });
        Ok(())
    }

    /// `applied` is `now` (the next words use `voice`) or `nextConversation` (this conversation
    /// keeps `current`); `reason` says why not now.
    fn voice_answer(&mut self, voice: &str, applied: &str, reason: &str) {
        if applied == "now" {
            self.session_update = self.provider.with_voice(&self.session_update, voice, "");
            if let Some(session) = self.session_update.as_object_mut() {
                session.remove("event_id");
            }
        }
        let current = self.voice.clone();
        self.to_client(json!({
            "type": "voice",
            "voice": voice,
            "current": current,
            "applied": applied,
            "reason": reason,
        }));
    }

    /// Talk pressed. In a push-to-talk call, talking over the agent cuts its reply, as Stop does,
    /// then the microphone's audio collects in the provider's buffer until Talk is released. In a
    /// muted Regular call the hold only opens the microphone; the provider's turn detection does
    /// the rest (`talk.rs`).
    ///
    /// CDXC:VoiceMode 2026-10-21 DECISION:
    /// User: "I feel we're overcomplicating things. Let's just have two modes: push-to-talk and regular, and mention the pricing difference between the two in settings for the mobile app and for the desktop app. No need to switch when I'm muted, and only any of that. Let's just keep it simple: two modes in the settings, please." `voiceAgentCallMode` (`regular` default, `pushToTalk`) is read when a call starts and never switched mid-call; mute is a plain microphone mute in both modes. Supersedes the 2026-10-21 "Push-to-talk while muted" switch, which xAI kept billing for connected time. The 2026-10-24 hold to unmute in talk.rs leaves the mode alone.
    async fn talk_start(&mut self) -> Result<(), Ended> {
        match self.talk.press() {
            Press::Ignored => Ok(()),
            Press::RefusedMuted => {
                self.to_client(json!({"type": "talk", "event": "refused", "reason": "muted"}));
                Ok(())
            }
            Press::Unmute => {
                self.last_talk = Instant::now();
                Ok(())
            }
            Press::Talk => {
                if self.response_active {
                    self.provider_send(json!({"type": "response.cancel"}))
                        .await?;
                }
                self.stop_playback(true).await?;
                // The reply to this turn will see any tool results; a separate one would repeat it.
                if self.create_at.take().is_some() {
                    self.outputs_waiting = false;
                }
                self.provider_send(json!({"type": "input_audio_buffer.clear"}))
                    .await?;
                self.user_speaking = true;
                self.last_talk = Instant::now();
                self.set_state("userSpeaking");
                Ok(())
            }
        }
    }

    /// Talk released: a push-to-talk turn is committed and answered; a hold too short to hold any
    /// speech is dropped and the client told (`tooShort`).
    async fn talk_end(&mut self) -> Result<(), Ended> {
        let release = self.talk.release(Instant::now());
        self.apply_talk_release(release).await
    }

    async fn apply_talk_release(&mut self, release: Release) -> Result<(), Ended> {
        match release {
            Release::Ignored => Ok(()),
            Release::TooShort { clear } => {
                self.last_talk = Instant::now();
                if clear {
                    self.drop_held_turn().await?;
                }
                self.to_client(json!({"type": "talk", "event": "tooShort"}));
                Ok(())
            }
            Release::Unmuted => {
                self.last_talk = Instant::now();
                Ok(())
            }
            Release::Commit => {
                self.user_speaking = false;
                self.last_talk = Instant::now();
                self.provider_send(json!({"type": "input_audio_buffer.commit"}))
                    .await?;
                self.create_response_now().await
            }
        }
    }

    /// The user slid off the hold button: nothing they said is sent.
    async fn talk_cancel(&mut self) -> Result<(), Ended> {
        if self.talk.cancel() == Cancel::Cleared {
            self.drop_held_turn().await?;
            self.to_client(json!({"type": "talk", "event": "cancelled"}));
        }
        Ok(())
    }

    /// Forgets the audio of a hold that will not be answered.
    async fn drop_held_turn(&mut self) -> Result<(), Ended> {
        self.provider_send(json!({"type": "input_audio_buffer.clear"}))
            .await?;
        if self.push_to_talk {
            self.user_speaking = false;
            self.set_state("listening");
        }
        Ok(())
    }

    fn start_tool(&mut self, call_id: String, name: String, arguments: String) {
        if call_id.is_empty() || !self.seen_calls.insert(call_id.clone()) {
            return;
        }
        let args: Value = serde_json::from_str(&arguments).unwrap_or_else(|_| json!({}));
        let label = tools::label(
            &name,
            &args,
            self.focus.as_ref().map(|scope| scope.title.as_str()),
        );
        self.pending_calls.insert(call_id.clone());
        self.to_client(json!({"type": "tool", "phase": "started", "callId": call_id, "name": name, "label": label}));
        if !self.playing() {
            self.set_state("toolRunning");
        }
        let state = self.state.clone();
        let tx = self.tx.clone();
        let memory = self.memory.clone();
        tokio::spawn(async move {
            let outcome =
                super::approvals::run_in_call(state, memory, name.clone(), args, tx.clone()).await;
            let _ = tx.send(Input::ToolDone {
                call_id,
                name,
                label,
                outcome,
            });
        });
    }
}
