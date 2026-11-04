//! The two realtime providers behind one interface. xAI's Grok Voice API follows OpenAI's Realtime
//! event protocol (same client and server event names), so one conversation loop serves both and
//! only the URL, the `session.update` shape and a few event spellings differ.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: both providers from the start, xAI Grok Voice and OpenAI GPT Realtime, picked in Settings (`voiceAgentProvider`).
//! CDXC:VoiceMode 2026-10-20 WHY:
//! gxserver relays the provider socket instead of minting ephemeral tokens for clients: one implementation of both protocols, the tools run where the sessions are, and the phone never holds a key. The relay adds one hop between the computer and the provider, which is small next to the provider's own turn latency.

use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub(super) type ProviderSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 16-bit mono PCM at 24 kHz: what both providers speak natively and what every client sends.
pub(super) const SAMPLE_RATE: u32 = 24_000;
pub(super) const BYTES_PER_MS: usize = (SAMPLE_RATE as usize * 2) / 1000;

const XAI_REALTIME_URL: &str = "wss://api.x.ai/v1/realtime?model=grok-voice-latest";
const OPENAI_REALTIME_URL: &str = "wss://api.openai.com/v1/realtime?model=gpt-realtime-2.1";
const XAI_STT_URL: &str = "wss://api.x.ai/v1/stt";

/// OpenAI Realtime: `session.audio.input.transcription.prompt` takes at most 1024 characters (the API
/// answers "string too long. Expected a string with maximum length 1024"). The session is refused whole
/// when it is longer, so the call never starts.
pub(super) const OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS: usize = 1024;
/// xAI Realtime: `session.audio.input.transcription.keyterms` takes at most 100 terms of 50 characters.
pub(super) const XAI_KEYTERMS_MAX_COUNT: usize = 100;
pub(super) const XAI_KEYTERM_MAX_CHARS: usize = 50;

const OPENAI_TRANSCRIPTION_PROMPT_LEAD: &str = "Names you may hear: ";
const OPENAI_TRANSCRIPTION_PROMPT_TRAIL: &str = ".";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Provider {
    Xai,
    OpenAi,
}

/// Grok voices (`GET /v1/tts/voices`, 2026-10-20). The catalog row lists the same ids.
pub(super) const XAI_VOICES: &[&str] = &[
    "eve", "ara", "rex", "sal", "leo", "altair", "atlas", "aurora", "carina", "castor", "celeste",
    "cosmo", "helios", "helix", "iris", "kepler", "liora", "lumen", "luna", "lux", "naksh",
    "orion", "perseus", "rigel", "sirius", "ursa", "zagan", "zenith",
];
/// OpenAI Realtime voices; `marin` and `cedar` are the ones OpenAI recommends.
pub(super) const OPENAI_VOICES: &[&str] = &[
    "marin", "cedar", "alloy", "ash", "ballad", "coral", "echo", "sage", "shimmer", "verse",
];

impl Provider {
    pub(super) fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "xai" | "grok" => Some(Self::Xai),
            "openai" | "gpt" => Some(Self::OpenAi),
            _ => None,
        }
    }

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Xai => "xai",
            Self::OpenAi => "openai",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Xai => "xAI",
            Self::OpenAi => "OpenAI",
        }
    }

    pub(super) fn default_voice(self) -> &'static str {
        match self {
            Self::Xai => "eve",
            Self::OpenAi => "marin",
        }
    }

    pub(super) fn voices(self) -> &'static [&'static str] {
        match self {
            Self::Xai => XAI_VOICES,
            Self::OpenAi => OPENAI_VOICES,
        }
    }

    pub(super) fn key_check_url(self) -> &'static str {
        match self {
            Self::Xai => "https://api.x.ai/v1/api-key",
            Self::OpenAi => "https://api.openai.com/v1/models",
        }
    }

    fn realtime_url(self) -> &'static str {
        match self {
            Self::Xai => XAI_REALTIME_URL,
            Self::OpenAi => OPENAI_REALTIME_URL,
        }
    }

    /// The `session.update` that configures a conversation: server-side voice activity detection
    /// (which also interrupts the agent when the user talks over it), 24 kHz PCM both ways, the
    /// user's speech transcribed for captions, the voice, the system prompt and the tools.
    /// `keyterms` are names the recognizer should expect (Ghostex, project names), which it otherwise
    /// mishears ("Gostex"), most useful first: each provider keeps as many as its limit allows (`xai_keyterms`,
    /// `openai_transcription_prompt`). A push-to-talk call has no turn detection: the user holds Talk, and
    /// gxserver commits the audio and asks for the reply when Talk is released.
    ///
    /// CDXC:VoiceMode 2026-10-21 WHY: xAI documents "Sessions using the default `server_vad` turn detection are billed for session duration" and "Push-to-talk sessions are billed only for audio sent and received"; its realtime schema gives `turn_detection.type` as "`server_vad` for automatic detection, `null` for manual turns". Measured 2026-10-21 (`usage.billable_audio_seconds`): a session that starts with `{"type": null}` billed 2 s over a 15 s quiet gap, but turning server_vad off mid-call kept billing the whole duration (17 s), so the mode is fixed when the call starts and never switched mid-call.
    pub(super) fn session_update(
        self,
        voice: &str,
        instructions: &str,
        tools: &Value,
        keyterms: &[String],
        push_to_talk: bool,
    ) -> Value {
        let pcm = json!({"type": "audio/pcm", "rate": SAMPLE_RATE});
        match self {
            Self::Xai => json!({
                "type": "session.update",
                "session": {
                    "instructions": instructions,
                    "voice": voice,
                    "turn_detection": if push_to_talk { json!({"type": null}) } else { json!({"type": "server_vad"}) },
                    "audio": {
                        "input": {"format": pcm, "transcription": {"keyterms": xai_keyterms(keyterms)}},
                        "output": {"format": pcm},
                    },
                    "tools": tools,
                },
            }),
            Self::OpenAi => {
                let mut transcription = json!({"model": "gpt-4o-transcribe"});
                let prompt = openai_transcription_prompt(keyterms);
                if !prompt.is_empty() {
                    transcription["prompt"] = json!(prompt);
                }
                json!({
                    "type": "session.update",
                    "session": {
                        "type": "realtime",
                        "instructions": instructions,
                        "output_modalities": ["audio"],
                        "audio": {
                            "input": {
                                "format": pcm,
                                "transcription": transcription,
                                "turn_detection": if push_to_talk { Value::Null } else { json!({
                                    "type": "semantic_vad",
                                    "create_response": true,
                                    "interrupt_response": true,
                                }) },
                            },
                            "output": {"format": pcm, "voice": voice},
                        },
                        "tools": tools,
                        "tool_choice": "auto",
                    },
                })
            }
        }
    }
}

impl Provider {
    /// The conversation's `session.update` again with another voice, for a voice change mid-call.
    /// The whole configuration is resent rather than only the voice, so a provider that replaces
    /// the session on update (xAI does not say whether it merges) keeps its tools and prompt.
    pub(super) fn with_voice(self, session_update: &Value, voice: &str, event_id: &str) -> Value {
        let mut update = session_update.clone();
        update["event_id"] = json!(event_id);
        match self {
            Self::Xai => update["session"]["voice"] = json!(voice),
            Self::OpenAi => update["session"]["audio"]["output"]["voice"] = json!(voice),
        }
        update
    }

    /// The voice a `session.updated` event says the session now speaks with.
    pub(super) fn session_voice(self, event: &Value) -> Option<String> {
        let pointer = match self {
            Self::Xai => "/session/voice",
            Self::OpenAi => "/session/audio/output/voice",
        };
        event
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
    }

    /// CDXC:VoiceMode 2026-10-20 WHY:
    /// OpenAI documents that the voice "cannot be changed during the session once the model has responded with audio at least once" (`session.audio.output.voice`), so a voice picked after the first reply is kept for the next conversation instead of being sent and refused. xAI documents no such limit, so gxserver asks Grok and reports what it answered.
    pub(super) fn voice_locked_after_audio(self) -> bool {
        matches!(self, Self::OpenAi)
    }
}

/// The terms xAI accepts: the first `XAI_KEYTERMS_MAX_COUNT` that are at most `XAI_KEYTERM_MAX_CHARS` long.
/// A longer term is dropped whole, never cut.
pub(super) fn xai_keyterms(keyterms: &[String]) -> Vec<&str> {
    keyterms
        .iter()
        .map(String::as_str)
        .filter(|term| term.chars().count() <= XAI_KEYTERM_MAX_CHARS)
        .take(XAI_KEYTERMS_MAX_COUNT)
        .collect()
}

/// The recognizer hint OpenAI takes: "Names you may hear: A, B, C." within
/// `OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS`. `keyterms` come most useful first, so the list stops at the
/// first name that no longer fits and every name before it is kept whole. With no names that fit, the
/// hint is empty (`session_update` then leaves the prompt out) rather than a lead-in with nothing after it.
pub(super) fn openai_transcription_prompt(keyterms: &[String]) -> String {
    let mut prompt = String::from(OPENAI_TRANSCRIPTION_PROMPT_LEAD);
    let mut length = prompt.chars().count() + OPENAI_TRANSCRIPTION_PROMPT_TRAIL.chars().count();
    let mut kept = 0;
    for term in keyterms {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }
        let separator = if kept == 0 { 0 } else { 2 };
        let needed = separator + term.chars().count();
        if length + needed > OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS {
            break;
        }
        if kept > 0 {
            prompt.push_str(", ");
        }
        prompt.push_str(term);
        length += needed;
        kept += 1;
    }
    if kept == 0 {
        return String::new();
    }
    prompt.push_str(OPENAI_TRANSCRIPTION_PROMPT_TRAIL);
    prompt
}

/// What a Realtime `error` event says, for the log and the panel.
pub(super) struct ProviderError {
    /// The provider's own sentence.
    pub message: String,
    /// The log details: provider, event type, code and the offending parameter. Never the key or audio.
    pub details: Value,
}

impl Provider {
    /// Reads an `error` event (`{"type": "error", "error": {"type", "code", "message", "param"}}`; a
    /// body without the `error` wrapper is read the same way).
    pub(super) fn describe_error(self, event: &Value) -> ProviderError {
        let error = event.get("error").unwrap_or(event);
        let text = |key: &str| {
            error
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
        };
        let message = match text("message") {
            "" => "The provider reported an error.",
            message => message,
        }
        .to_string();
        let mut details = json!({
            "provider": self.id(),
            "eventType": event.get("type").and_then(Value::as_str).unwrap_or("unknown"),
        });
        for (name, key) in [("code", "code"), ("param", "param"), ("errorType", "type")] {
            if !text(key).is_empty() {
                details[name] = json!(text(key));
            }
        }
        ProviderError { message, details }
    }
}

/// Why a provider socket could not be opened.
pub(super) enum ConnectError {
    BadKey,
    Unavailable(String),
    /// The provider answered with an error status; the sentence to show, with the provider's own
    /// explanation when it sent one.
    Refused(String),
}

/// The provider's own explanation in an error answer's body: `error.message`, `error`, `message`
/// or `detail` of a JSON body, else short plain text.
pub(super) fn error_body_text(body: Option<&[u8]>) -> Option<String> {
    let text = String::from_utf8_lossy(body?).trim().to_string();
    if text.is_empty() {
        return None;
    }
    let explained = match serde_json::from_str::<Value>(&text) {
        Ok(json) => ["/error/message", "/error", "/message", "/detail"]
            .iter()
            .find_map(|pointer| json.pointer(pointer).and_then(Value::as_str))
            .map(|text| text.trim().to_string())?,
        Err(_) if text.starts_with('<') => return None,
        Err(_) => text,
    };
    let explained: String = explained.chars().take(300).collect();
    (!explained.is_empty()).then_some(explained)
}

/// CDXC:VoiceMode 2026-10-20 WHY:
/// An xAI account on The X Subscription draws API usage from the plan's included pool, and xAI answered HTTP 402 for a few minutes and then served again (live, 2026-10-20). The panel says what the provider said; without a body it never claims the account is out of credit.
pub(super) fn refused_sentence(label: &str, status: u16, body: Option<&[u8]>) -> String {
    match (status, error_body_text(body)) {
        (402, Some(detail)) => format!("{label} didn't accept this request: {detail}"),
        (402, None) if label == "xAI" => "xAI didn't accept this request right now (payment required). If you use Grok through your X subscription, its included usage may be used up for the moment; check grok.com/dashboard.".to_string(),
        (402, None) => format!("{label} didn't accept this request right now (payment required). Check your {label} billing."),
        (429, Some(detail)) => format!("{label} is limiting requests right now: {detail}"),
        (429, None) => format!("{label} is limiting requests right now. Try again in a moment."),
        (status, Some(detail)) => format!("{label} answered HTTP {status}: {detail}"),
        (status, None) => format!("{label} answered HTTP {status}."),
    }
}

async fn open(url: &str, key: &str, label: &str) -> Result<ProviderSocket, ConnectError> {
    let mut request = url
        .into_client_request()
        .map_err(|error| ConnectError::Unavailable(error.to_string()))?;
    let bearer =
        HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| ConnectError::BadKey)?;
    request.headers_mut().insert("authorization", bearer);
    let connect = tokio_tungstenite::connect_async(request);
    match tokio::time::timeout(std::time::Duration::from_secs(15), connect).await {
        Err(_) => Err(ConnectError::Unavailable(
            "The provider did not answer within 15 seconds.".to_string(),
        )),
        Ok(Ok((socket, _))) => Ok(socket),
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response))) => {
            match response.status().as_u16() {
                401 | 403 => Err(ConnectError::BadKey),
                status => Err(ConnectError::Refused(refused_sentence(
                    label,
                    status,
                    response.body().as_deref(),
                ))),
            }
        }
        Ok(Err(error)) => Err(ConnectError::Unavailable(error.to_string())),
    }
}

pub(super) async fn connect_realtime(
    provider: Provider,
    key: &str,
) -> Result<ProviderSocket, ConnectError> {
    open(provider.realtime_url(), key, provider.label()).await
}

/// xAI's streaming speech-to-text: raw PCM in binary frames, `transcript.partial` events back.
pub(super) async fn connect_dictation(
    key: &str,
    language: Option<&str>,
) -> Result<ProviderSocket, ConnectError> {
    let mut url =
        format!("{XAI_STT_URL}?sample_rate={SAMPLE_RATE}&encoding=pcm&interim_results=true");
    // Only a language hint: `format=true` writes numbers and units out ("30 s" for "thirty seconds"), which reads wrong in a prompt.
    if let Some(language) = language {
        url.push_str(&format!("&language={language}"));
    }
    open(&url, key, Provider::Xai.label()).await
}
