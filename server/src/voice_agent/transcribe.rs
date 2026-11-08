//! The second half of Dictate: a saved recording (`recordings.rs`) turned into text by the engine
//! Dictate's settings name (`speech/settings.rs`): Hex 3's local API on this computer
//! (`speech/hex.rs`), or a batch speech-to-text, xAI's (`POST /v1/stt`) or OpenAI's
//! (`POST /v1/audio/transcriptions`), both of which take the `.m4a` or `.wav` as it is. The text
//! then goes through the clean-up (`speech/cleanup.rs`: aliases, fillers, AI polish). A failure is kept on the recording with a sentence that says
//! what went wrong and what to do; the audio stays whatever happens, so Transcribe again works once
//! the cause is gone.
//!
//! CDXC:VoiceMode 2026-10-20 WHY:
//! The streaming dictation socket (`dictation.rs`) tied the recording to the provider connection: a refused key, an empty account or a dropped connection ended the dictation and lost what was said. Transcribing a finished file decouples the two (the user's decision in `recordings.rs`). Time-fixable failures (rate limits, xAI unavailable, no connection) are retried here a few times before the recording is marked failed.

use std::time::Duration;

use serde_json::{json, Value};

use super::keys::{self, KeyState};
use super::provider::{self, Provider};
use super::recordings::{self, Problem, Recording, Status};
use super::speech::cleanup::{self, PolishOutcome};
use super::speech::dictate;
use super::speech::settings::{DictateSettings, Engine};
use crate::server::AppState;

const XAI_BATCH_STT_URL: &str = "https://api.x.ai/v1/stt";
const OPENAI_TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";
/// The model the voice agent already transcribes the user's speech with (`provider.rs`).
const OPENAI_TRANSCRIPTION_MODEL: &str = "gpt-4o-transcribe";
/// Waits before each retry of a failure time can fix.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(5)];

/// What a transcription found.
pub(super) enum Outcome {
    Text(String),
    /// Worth trying again in a moment (rate limited, xAI unavailable, no connection).
    Transient(Problem),
    Final(Problem),
}

fn problem(code: &str, provider: Option<Provider>, message: impl Into<String>) -> Problem {
    Problem {
        code: code.to_string(),
        message: message.into(),
        provider: provider.map(|provider| provider.id().to_string()),
        detail: None,
    }
}

impl Problem {
    fn with_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail.filter(|detail| !detail.trim().is_empty());
        self
    }
}

/// Transcribes `id`, saving its text or why it failed. A recording dismissed while this ran stays
/// dismissed and its text is not saved. Answers the recording as it now is.
pub(super) async fn transcribe(state: &AppState, id: &str) -> Result<Recording, String> {
    let paths = state.paths.clone();
    let recording = recordings::read(&paths, id)
        .ok_or_else(|| "That recording no longer exists.".to_string())?;
    if recording.status == Status::Transcribed {
        return Ok(recording);
    }
    if recording.text.is_some() {
        // Dismissed after xAI had already answered: Recover needs no second call.
        let paths = paths.clone();
        let id = id.to_string();
        return tokio::task::spawn_blocking(move || {
            recordings::update(&paths, &id, |recording| {
                recording.status = Status::Transcribed
            })
        })
        .await
        .map_err(|error| error.to_string())?;
    }
    let generation = recordings::generation(id);
    let recording = {
        let paths = paths.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            recordings::update(&paths, &id, |recording| {
                recording.status = Status::Transcribing;
                recording.error = None;
            })
        })
        .await
        .map_err(|error| error.to_string())??
    };
    let settings = {
        let paths = paths.clone();
        tokio::task::spawn_blocking(move || dictate::current_settings(&paths))
            .await
            .map_err(|error| error.to_string())?
    };
    let words = dictate::hint_words(&settings, &dictate::project_words(state));
    let engine_used =
        json!({"engine": settings.engine.id(), "label": dictate::engine_label(&settings)});
    let mut outcome = call_once(&paths, &recording, &settings, &words).await;
    for delay in RETRY_DELAYS {
        if !matches!(outcome, Outcome::Transient(_)) || recordings::generation(id) != generation {
            break;
        }
        tokio::time::sleep(delay).await;
        outcome = call_once(&paths, &recording, &settings, &words).await;
    }
    if let Outcome::Transient(problem) | Outcome::Final(problem) = &outcome {
        super::warn(
            state,
            "voiceAgent.dictationTranscribeFailed",
            json!({"code": problem.code, "provider": problem.provider, "durationMs": recording.duration_ms}),
        );
    }
    let raw = match outcome {
        Outcome::Text(raw) => raw,
        Outcome::Transient(problem) | Outcome::Final(problem) => {
            return save(&paths, id, generation, move |recording| {
                recording.status = Status::Failed;
                recording.error = Some(problem);
            })
            .await;
        }
    };
    let cleaned = cleanup::clean(&raw, &settings);
    if !settings.polish.enabled || cleaned.is_empty() {
        return save(&paths, id, generation, move |recording| {
            recording.status = Status::Transcribed;
            recording.text = Some(cleaned);
            recording.raw_text = Some(raw);
            recording.engine = Some(engine_used);
            recording.polish = None;
            recording.error = None;
        })
        .await;
    }
    // Saved before polishing, so a restart or a failed polish still has the cleaned text.
    {
        let cleaned = cleaned.clone();
        let raw = raw.clone();
        let engine_used = engine_used.clone();
        save(&paths, id, generation, move |recording| {
            recording.status = Status::Polishing;
            recording.text = Some(cleaned);
            recording.raw_text = Some(raw);
            recording.engine = Some(engine_used);
            recording.error = None;
        })
        .await?;
    }
    let (text, report) = match cleanup::polish(state, id, &settings, &words, &cleaned).await {
        PolishOutcome::Polished(text) => (text, json!({"status": "polished"})),
        PolishOutcome::Skipped => (cleaned, json!({"status": "skipped"})),
        PolishOutcome::Unavailable => (
            cleaned,
            json!({"status": "unavailable", "message": "AI polish needs Claude Code on this computer."}),
        ),
        PolishOutcome::Failed(message) => {
            super::warn_error(
                state,
                "voiceAgent.dictationPolishFailed",
                &message,
                json!({"durationMs": recording.duration_ms}),
            );
            (cleaned, json!({"status": "failed", "message": message}))
        }
    };
    save(&paths, id, generation, move |recording| {
        recording.status = Status::Transcribed;
        recording.text = Some(text);
        recording.polish = Some(report);
    })
    .await
}

/// Applies `change` to the recording unless it was dismissed (or deleted and re-made) since
/// `generation`: the dismissal stands.
async fn save(
    paths: &crate::paths::GxserverPaths,
    id: &str,
    generation: u64,
    change: impl FnOnce(&mut Recording) + Send + 'static,
) -> Result<Recording, String> {
    let paths = paths.clone();
    let id = id.to_string();
    tokio::task::spawn_blocking(move || {
        recordings::update(&paths, &id, |recording| {
            if recordings::generation(&recording.id) == generation {
                change(recording);
            }
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn call_once(
    paths: &crate::paths::GxserverPaths,
    recording: &Recording,
    settings: &DictateSettings,
    words: &[String],
) -> Outcome {
    let audio_path = recordings::folder(paths).join(&recording.audio_file);
    // The setting wins; a client's hint ("en") applies when the setting is Auto.
    let language = settings
        .language_hint()
        .map(str::to_string)
        .or_else(|| recording.language.clone());
    let provider = match settings.engine {
        Engine::Hex => {
            return transcribe_with_hex(paths, recording, settings, &audio_path, language).await
        }
        Engine::Xai => Provider::Xai,
        Engine::OpenAi => Provider::OpenAi,
    };
    match keys::key_state(paths, provider) {
        KeyState::Saved => {}
        KeyState::Unreadable => {
            return Outcome::Final(problem(
                "noKey",
                Some(provider),
                format!(
                    "The saved {} API key can't be read anymore. Paste it again in Settings, then transcribe again.",
                    provider.label()
                ),
            ))
        }
        KeyState::Missing => return Outcome::Final(problem("noKey", Some(provider), no_key(provider))),
    }
    let key = match keys::read_key_async(paths, provider).await {
        Ok(Some(key)) => key,
        Ok(None) => return Outcome::Final(problem("noKey", Some(provider), no_key(provider))),
        Err(message) => return Outcome::Final(problem("noKey", Some(provider), message)),
    };
    let audio = match tokio::fs::read(&audio_path).await {
        Ok(audio) if !audio.is_empty() => audio,
        _ => {
            return Outcome::Final(problem(
                "audioMissing",
                None,
                "The recording's audio file is gone, so it can't be transcribed.",
            ))
        }
    };
    let file_name = recording.audio_file.clone();
    let keyterms = words.to_vec();
    // A minute of audio takes the provider a few seconds; long recordings get longer.
    let timeout = Duration::from_secs(60 + recording.duration_ms / 2_000);
    tokio::task::spawn_blocking(move || {
        post_batch(
            provider,
            &key,
            &file_name,
            &audio,
            language.as_deref(),
            &keyterms,
            timeout,
        )
    })
    .await
    .unwrap_or_else(|error| {
        Outcome::Transient(offline(provider).with_detail(Some(error.to_string())))
    })
}

/// Dictate's engine is this provider and it has no key.
fn no_key(provider: Provider) -> String {
    format!(
        "Dictate is set to use {label}, and no {label} API key is set. Add one in Settings, or pick another engine for Dictate, then transcribe again.",
        label = provider.label()
    )
}

/// Transcribes with Hex 3's local API (`speech/hex.rs`). Never falls back to another engine.
async fn transcribe_with_hex(
    paths: &crate::paths::GxserverPaths,
    recording: &Recording,
    settings: &DictateSettings,
    audio_path: &std::path::Path,
    language: Option<String>,
) -> Outcome {
    let home = paths.home_dir.clone();
    let model = settings.hex_model.clone();
    let audio = audio_path.to_path_buf();
    // Hex runs on this computer; a minute of audio takes it seconds, long ones get longer.
    let timeout = Duration::from_secs(60 + recording.duration_ms / 2_000);
    let result = tokio::task::spawn_blocking(move || {
        super::speech::hex::transcribe(
            &home,
            model.as_deref(),
            &audio,
            language.as_deref(),
            timeout,
        )
    })
    .await;
    let failure = match result {
        Ok(Ok(text)) => return Outcome::Text(text),
        Ok(Err(failure)) => failure,
        Err(error) => {
            return Outcome::Transient(
                problem(
                    "hexUnavailable",
                    None,
                    "Ghostex couldn't reach Hex. Transcribe again.",
                )
                .with_detail(Some(error.to_string())),
            )
        }
    };
    // Someone whose engine was our retired local model hears why it is Hex now, once.
    let message = if settings.local_retired_dictation_notice && !failure.transient {
        let paths = paths.clone();
        let _ = tokio::task::spawn_blocking(move || {
            super::speech::settings::dictation_notice_said(&paths)
        })
        .await;
        format!("{} {}", super::speech::retired::NOTICE, failure.message)
    } else {
        failure.message
    };
    let problem = Problem {
        code: failure.code.to_string(),
        message,
        provider: Some("hex".to_string()),
        detail: None,
    }
    .with_detail(failure.detail);
    if failure.transient {
        Outcome::Transient(problem)
    } else {
        Outcome::Final(problem)
    }
}

/// The provider's endpoint and the form fields it takes besides the file.
pub(super) fn request_fields(
    provider: Provider,
    language: Option<&str>,
    keyterms: &[String],
) -> (&'static str, Vec<(&'static str, String)>) {
    let mut fields = Vec::new();
    match provider {
        Provider::Xai => {
            if let Some(language) = language {
                fields.push(("language", language.to_string()));
            }
            for keyterm in provider::xai_keyterms(keyterms) {
                fields.push(("keyterm", keyterm.to_string()));
            }
            (XAI_BATCH_STT_URL, fields)
        }
        Provider::OpenAi => {
            fields.push(("model", OPENAI_TRANSCRIPTION_MODEL.to_string()));
            fields.push(("response_format", "json".to_string()));
            if let Some(language) = language {
                fields.push(("language", language.to_string()));
            }
            let prompt = provider::openai_transcription_prompt(keyterms);
            if !prompt.is_empty() {
                fields.push(("prompt", prompt));
            }
            (OPENAI_TRANSCRIPTIONS_URL, fields)
        }
    }
}

/// The multipart request both providers document: options first, `file` last.
pub(super) fn multipart_body(
    boundary: &str,
    fields: &[(&str, String)],
    file_name: &str,
    audio: &[u8],
) -> Vec<u8> {
    let mut body = Vec::with_capacity(audio.len() + 1024);
    for (name, value) in fields {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    }
    let mime = if file_name.ends_with(".m4a") {
        "audio/mp4"
    } else {
        "audio/wav"
    };
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(audio);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn post_batch(
    provider: Provider,
    key: &str,
    file_name: &str,
    audio: &[u8],
    language: Option<&str>,
    keyterms: &[String],
    timeout: Duration,
) -> Outcome {
    let boundary = format!("ghostex-{}", uuid::Uuid::new_v4().simple());
    let (url, fields) = request_fields(provider, language, keyterms);
    let body = multipart_body(&boundary, &fields, file_name, audio);
    let response = crate::http_agent::answering_agent()
        .post(url)
        .header("authorization", format!("Bearer {key}"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .config()
        .timeout_global(Some(timeout))
        .build()
        .send(&body[..]);
    match response {
        Ok(mut response) if response.status().as_u16() >= 400 => {
            let detail = response.body_mut().read_to_string().unwrap_or_default();
            status_problem(provider, response.status().as_u16(), &detail)
        }
        Ok(response) => match crate::http_agent::read_json::<Value>(response) {
            Ok(answer) => Outcome::Text(
                answer
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            ),
            Err(error) => Outcome::Transient(
                problem(
                    "providerError",
                    Some(provider),
                    format!(
                        "{}'s answer couldn't be read. Transcribe again.",
                        provider.label()
                    ),
                )
                .with_detail(Some(error.to_string())),
            ),
        },
        Err(error) => Outcome::Transient(offline(provider).with_detail(Some(error.to_string()))),
    }
}

fn offline(provider: Provider) -> Problem {
    problem(
        "providerUnavailable",
        Some(provider),
        format!(
            "Couldn't reach {}. Check this computer's internet connection, then transcribe again.",
            provider.label()
        ),
    )
}

/// What a refusal means for the recording: a plain sentence saying what to do, with the
/// provider's own words (`provider::error_body_text`) kept as the detail.
///
/// CDXC:VoiceMode 2026-10-20 WHY: xAI documents no HTTP 402 (its rate limits are 429), yet answered 402 for a few minutes on 2026-10-20 and then served again; an account on The X Subscription draws API usage from the plan's weekly included pool and needs prepaid credit beyond it. So an xAI 402 is retried like a rate limit, keeps what xAI said as the detail, and never claims the account is out of credit.
/// CDXC:VoiceMode 2026-10-30 WHY: OpenAI answers an account without credit with 429 and `insufficient_quota` (not 402), which waiting does not fix, so that one reads as payment required and is not retried.
pub(super) fn status_problem(provider: Provider, status: u16, body: &str) -> Outcome {
    let label = provider.label();
    let detail = provider::error_body_text((!body.trim().is_empty()).then_some(body.as_bytes()))
        .or_else(|| Some(format!("{label} answered HTTP {status}.")));
    let said = |code: &str, message: String| {
        problem(code, Some(provider), message).with_detail(detail.clone())
    };
    let no_quota =
        provider == Provider::OpenAi && status == 429 && body.contains("insufficient_quota");
    match status {
        401 | 403 => Outcome::Final(said(
            "badKey",
            format!("{label} refused the API key. Check it in Settings > Voice, then transcribe again."),
        )),
        413 => Outcome::Final(said(
            "badAudio",
            format!("The recording is too long for {label} to transcribe in one go. Dictate it in shorter parts."),
        )),
        400 | 415 | 422 => Outcome::Final(said(
            "badAudio",
            format!("{label} couldn't read the recording. Transcribe again; if it keeps failing, record it again."),
        )),
        402 | 429 if provider == Provider::OpenAi && (status == 402 || no_quota) => Outcome::Final(said(
            "paymentRequired",
            "OpenAI didn't accept the request: the account's quota or credit is used up. Check billing at platform.openai.com, then transcribe again.".to_string(),
        )),
        402 => Outcome::Transient(said(
            "paymentRequired",
            "xAI didn't accept the request (payment required). Check your credit or plan usage at console.x.ai, then transcribe again.".to_string(),
        )),
        429 => Outcome::Transient(said(
            "rateLimited",
            format!("{label} is limiting requests right now. Wait a moment, then transcribe again."),
        )),
        _ => Outcome::Transient(said(
            "providerUnavailable",
            format!("{label} isn't answering right now. Wait a moment, then transcribe again."),
        )),
    }
}
