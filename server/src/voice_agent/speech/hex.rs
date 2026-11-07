//! The Hex engine: Dictate's recording transcribed by Hex 3's authenticated local API on this
//! computer (Hex's `src/local_api.rs`, spec `docs/specs/local-transcription-service.md`).
//!
//! Hex writes `local-api.json` (`{port, token, apiVersion, pid}`, owner-only) in its data folder
//! while it runs (`managed_tools::hex::support_dir`). Every request carries
//! `Authorization: Bearer <token>`. `GET /health` answers `{version, apiVersion}`, `GET /models`
//! the catalog with each model's readiness, and `POST /transcriptions?model=…&language=…` takes a
//! PCM WAV body (`Content-Type: audio/wav`, with a length) and answers `{transcript, durationMs}`.
//! A transcription never downloads a model: an unprepared one answers 409 `model-not-ready`.
//! Errors are `{"code": "…"}`.
//!
//! CDXC:Dictate 2026-11-07 DECISION:
//! User: a fourth Dictate engine, Hex, beside Local, OpenAI and xAI, on every system Hex 3 ships for (macOS on Apple silicon, Windows and Linux). The settings row says where the user stands in plain words (Hex 3 not found with the link to install it, Hex not running, or ready with a model picker of the models Hex has ready); a 409 or a dead port gives a clear error that says what to do in Hex, never a silent failure or a hang. The token is never logged or shown, and an `apiVersion` Ghostex doesn't know is refused with a clear message. Our local engine stays until Hex is proven.
//! SEE-ALSO: server/src/managed_tools/hex.rs (finding and installing Hex), apps/desktop/src/app/window/settings_modal/tabs/voice_dictate.rs and apps/mobile/app/src/screens/settings/VoicePage.rntl.tsx (the settings rows).

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::managed_tools::hex::{self as install, Found};

/// The local API version this Ghostex speaks (Hex's `API_VERSION`).
pub(crate) const API_VERSION: &str = "2";
const DISCOVERY_FILE: &str = "local-api.json";
/// Hex answers its own probes within 500 ms; a busy machine gets more.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// `/health` and `/models` answer at once.
const QUICK_TIMEOUT: Duration = Duration::from_secs(5);
/// Hex refuses uploads over 64 MiB (`MAX_AUDIO_BYTES`).
pub(crate) const MAX_UPLOAD_BYTES: u64 = 64 * 1024 * 1024;

/// `local-api.json`. Its `Debug` leaves the token out.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Discovery {
    pub(crate) port: u16,
    pub(crate) token: String,
    pub(crate) api_version: String,
    #[serde(default)]
    pub(crate) pid: u32,
}

impl fmt::Debug for Discovery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Discovery")
            .field("port", &self.port)
            .field("token", &"<hidden>")
            .field("api_version", &self.api_version)
            .field("pid", &self.pid)
            .finish()
    }
}

/// Why `local-api.json` can't be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiscoveryError {
    /// No file: Hex 3 isn't running (or isn't installed).
    Missing,
    /// A file that isn't Hex's discovery document, or a token of the wrong shape.
    Invalid,
    /// A local API version this Ghostex doesn't speak.
    UnknownVersion(String),
}

pub(crate) fn discovery_file(home: &Path) -> PathBuf {
    install::support_dir(home).join(DISCOVERY_FILE)
}

/// Reads and checks `local-api.json` at `path`.
pub(crate) fn read_discovery(path: &Path) -> Result<Discovery, DiscoveryError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(DiscoveryError::Missing)
        }
        Err(_) => return Err(DiscoveryError::Invalid),
    };
    parse_discovery(&bytes)
}

pub(crate) fn parse_discovery(bytes: &[u8]) -> Result<Discovery, DiscoveryError> {
    let discovery: Discovery =
        serde_json::from_slice(bytes).map_err(|_| DiscoveryError::Invalid)?;
    if discovery.api_version != API_VERSION {
        return Err(DiscoveryError::UnknownVersion(discovery.api_version));
    }
    if discovery.port == 0 || !valid_token(&discovery.token) {
        return Err(DiscoveryError::Invalid);
    }
    Ok(discovery)
}

/// Hex's token shape (`valid_token` in Hex): `hex_` and 64 lowercase hex digits.
fn valid_token(token: &str) -> bool {
    token.len() == 68
        && token.starts_with("hex_")
        && token[4..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// What to tell the user about an API version Ghostex doesn't speak.
pub(crate) fn unknown_version_message(version: &str) -> String {
    let theirs = version.trim().parse::<u64>().ok();
    let ours: u64 = API_VERSION.parse().unwrap_or(0);
    match theirs {
        Some(theirs) if theirs < ours => format!(
            "This Hex speaks local API version {version}, and Ghostex needs version {API_VERSION}. Update Hex, then try again."
        ),
        Some(_) => format!(
            "This Hex speaks local API version {version}, which this Ghostex doesn't know yet. Update Ghostex, then try again."
        ),
        None => format!(
            "This Hex speaks a local API version Ghostex doesn't know (\"{version}\"). Update Ghostex and Hex, then try again."
        ),
    }
}

/// One model of `GET /models`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HexModel {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) installed: bool,
    #[serde(default)]
    pub(crate) verified: bool,
    #[serde(default)]
    pub(crate) managed: bool,
    #[serde(default)]
    pub(crate) languages: Vec<String>,
    #[serde(default)]
    pub(crate) supports_language_detection: bool,
}

impl HexModel {
    /// What Hex's `POST /transcriptions` accepts without preparing: a verified download, or a
    /// system-managed model that is installed.
    pub(crate) fn ready(&self) -> bool {
        self.verified || (self.managed && self.installed)
    }

    /// The `language` to send: Ghostex's choice when the model takes it, `auto` for Auto when it
    /// detects languages, else none (Hex then uses English). `Err` says the model can't do it.
    pub(crate) fn language_for(&self, wanted: Option<&str>) -> Result<Option<String>, String> {
        match wanted {
            None => Ok(self.supports_language_detection.then(|| "auto".to_string())),
            Some(code) => {
                if self
                    .languages
                    .iter()
                    .any(|known| known == code || known == "*")
                {
                    Ok(Some(code.to_string()))
                } else {
                    Err(format!(
                        "{} in Hex doesn't transcribe {}. Pick another Hex model, or set Dictate's language to Auto, then transcribe again.",
                        self.name,
                        super::catalog::LANGUAGE_LABELS
                            .iter()
                            .find(|(known, _)| *known == code)
                            .map_or(code, |(_, label)| label)
                    ))
                }
            }
        }
    }
}

/// The path and query of a transcription request.
pub(crate) fn transcription_path(model: &str, language: Option<&str>) -> String {
    let encode = |text: &str| {
        percent_encoding::utf8_percent_encode(text, percent_encoding::NON_ALPHANUMERIC).to_string()
    };
    let mut path = format!("/transcriptions?model={}", encode(model));
    if let Some(language) = language {
        path.push_str("&language=");
        path.push_str(&encode(language));
    }
    path
}

/// The `Authorization` header value. Kept to request building: never logged or shown.
fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// A failed call: what the user reads, a detail without secrets, and whether waiting may fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
    pub(crate) transient: bool,
}

impl Failure {
    fn final_(code: &'static str, message: impl Into<String>) -> Self {
        Failure {
            code,
            message: message.into(),
            detail: None,
            transient: false,
        }
    }

    fn transient(code: &'static str, message: impl Into<String>) -> Self {
        Failure {
            transient: true,
            ..Failure::final_(code, message)
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

pub(crate) const NOT_RUNNING: &str =
    "Hex isn't running on this computer. Open Hex, then transcribe again.";
const REFUSED_TOKEN: &str =
    "Hex refused Ghostex's request. Quit and reopen Hex, then transcribe again.";

/// What a `POST /transcriptions` answer other than 200 means. `model` is the model's name.
pub(crate) fn status_failure(status: u16, body: &str, model: &str) -> Failure {
    let code = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value["code"].as_str().map(str::to_string))
        .unwrap_or_default();
    let detail = if code.is_empty() {
        format!("Hex answered HTTP {status}.")
    } else {
        format!("Hex answered HTTP {status} ({code}).")
    };
    let failure = match (status, code.as_str()) {
        (409, "cancelled") => Failure::transient(
            "hexBusy",
            "Hex stopped this transcription. Transcribe again.",
        ),
        (409, _) => Failure::final_(
            "hexModelNotReady",
            format!("{model} isn't ready in Hex. Open Hex and download it there (or pick another model in Ghostex), then transcribe again."),
        ),
        (401, _) => Failure::final_("hexRefused", REFUSED_TOKEN),
        (400, "unsupported-model" | "unknown-model") => Failure::final_(
            "hexModelNotReady",
            format!("Hex can't transcribe with {model} in this language. Pick another Hex model, or set Dictate's language to Auto, then transcribe again."),
        ),
        (413, _) => Failure::final_(
            "badAudio",
            "The recording is too long for Hex to transcribe in one go. Dictate it in shorter parts.",
        ),
        (400 | 411 | 415, _) => Failure::final_(
            "badAudio",
            "Hex couldn't read the recording. Transcribe again; if it keeps failing, record it again.",
        ),
        (408, _) => Failure::transient(
            "hexUnavailable",
            "Hex didn't receive the recording in time. Transcribe again.",
        ),
        (429, _) => Failure::transient(
            "hexBusy",
            "Hex is busy with another transcription. Wait a moment, then transcribe again.",
        ),
        (503, _) => Failure::transient(
            "hexUnavailable",
            "Hex stopped while transcribing. Open Hex, then transcribe again.",
        ),
        (500, "model-load-failed") => Failure::final_(
            "hexFailed",
            format!("Hex couldn't load {model}. Open Hex and check the model, or pick another one, then transcribe again."),
        ),
        (500, _) => Failure::final_(
            "hexFailed",
            "Hex couldn't transcribe this recording. Transcribe again; if it keeps failing, pick another engine.",
        ),
        _ => Failure::transient(
            "hexUnavailable",
            "Hex isn't answering right now. Wait a moment, then transcribe again.",
        ),
    };
    failure.detail(detail)
}

/// What a request that got no answer means.
pub(crate) fn transport_failure(error: &ureq::Error, timeout: Duration) -> Failure {
    if crate::http_agent::never_connected(error) {
        return Failure::final_("hexNotRunning", NOT_RUNNING)
            .detail("Nothing answered on Hex's port.");
    }
    if matches!(error, ureq::Error::Timeout(_)) {
        return Failure::final_(
            "hexTimeout",
            format!(
                "Hex didn't answer within {} seconds. Check that Hex is working (try dictating in Hex itself), then transcribe again.",
                timeout.as_secs()
            ),
        );
    }
    Failure::transient(
        "hexUnavailable",
        "The connection to Hex dropped. Transcribe again.",
    )
    .detail(error.to_string())
}

/// A client for one running Hex.
pub(crate) struct Client {
    base: String,
    token: String,
}

impl Client {
    pub(crate) fn new(discovery: &Discovery) -> Self {
        Client {
            base: format!("http://127.0.0.1:{}", discovery.port),
            token: discovery.token.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn at(base: String, token: String) -> Self {
        Client { base, token }
    }

    fn get(&self, path: &str) -> Result<(u16, String), Failure> {
        let response = crate::http_agent::answering_agent()
            .get(format!("{}{path}", self.base))
            .header("authorization", bearer(&self.token))
            .config()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(QUICK_TIMEOUT))
            .build()
            .call();
        match response {
            Ok(mut response) => {
                let status = response.status().as_u16();
                let body = response.body_mut().read_to_string().unwrap_or_default();
                Ok((status, body))
            }
            Err(error) => Err(transport_failure(&error, QUICK_TIMEOUT)),
        }
    }

    /// `GET /health`: Hex's version, after checking it speaks [`API_VERSION`].
    pub(crate) fn health(&self) -> Result<String, Failure> {
        let (status, body) = self.get("/health")?;
        if status == 401 {
            return Err(Failure::final_("hexRefused", REFUSED_TOKEN));
        }
        if status != 200 {
            return Err(status_failure(status, &body, "the model"));
        }
        let answer: Value = serde_json::from_str(&body).map_err(|_| {
            Failure::final_(
                "hexFailed",
                "Hex's answer couldn't be read. Update Hex, then try again.",
            )
        })?;
        let version = answer["apiVersion"].as_str().unwrap_or_default();
        if version != API_VERSION {
            return Err(Failure::final_(
                "hexVersion",
                unknown_version_message(version),
            ));
        }
        Ok(answer["version"].as_str().unwrap_or_default().to_string())
    }

    /// `GET /models`: Hex's whole catalog.
    pub(crate) fn models(&self) -> Result<Vec<HexModel>, Failure> {
        let (status, body) = self.get("/models")?;
        if status == 401 {
            return Err(Failure::final_("hexRefused", REFUSED_TOKEN));
        }
        if status != 200 {
            return Err(status_failure(status, &body, "the model"));
        }
        serde_json::from_str(&body).map_err(|_| {
            Failure::final_(
                "hexFailed",
                "Hex's list of models couldn't be read. Update Hex, then try again.",
            )
        })
    }

    /// `POST /transcriptions` with a WAV body: the raw transcript.
    pub(crate) fn transcribe(
        &self,
        model: &HexModel,
        language: Option<&str>,
        wav: &[u8],
        timeout: Duration,
    ) -> Result<String, Failure> {
        if wav.len() as u64 > MAX_UPLOAD_BYTES {
            return Err(status_failure(413, "", &model.name));
        }
        let response = crate::http_agent::answering_agent()
            .post(format!(
                "{}{}",
                self.base,
                transcription_path(&model.id, language)
            ))
            .header("authorization", bearer(&self.token))
            .header("content-type", "audio/wav")
            .config()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(timeout))
            .build()
            .send(wav);
        match response {
            Ok(mut response) => {
                let status = response.status().as_u16();
                let body = response.body_mut().read_to_string().unwrap_or_default();
                if status != 200 {
                    return Err(status_failure(status, &body, &model.name));
                }
                serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|answer| {
                        answer["transcript"]
                            .as_str()
                            .map(|text| text.trim().to_string())
                    })
                    .ok_or_else(|| {
                        Failure::transient(
                            "hexFailed",
                            "Hex's answer couldn't be read. Transcribe again.",
                        )
                    })
            }
            Err(error) => Err(transport_failure(&error, timeout)),
        }
    }
}

/// Where the user stands with Hex on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// Hex 3 has no build for this computer.
    Unsupported(String),
    /// No Hex 3; `older` is the version of an older Hex without the local API.
    NotInstalled {
        older: Option<String>,
    },
    NotRunning,
    /// Hex answered, but Ghostex can't use it (a version it doesn't speak, a refused token).
    Problem {
        code: &'static str,
        message: String,
    },
    Ready {
        version: String,
        models: Vec<HexModel>,
    },
}

/// Reads `local-api.json` and asks Hex. Blocking (a few local requests): run it off the runtime.
pub(crate) fn state(home: &Path) -> State {
    if let Err(reason) = install::supported() {
        return State::Unsupported(reason);
    }
    let discovery = match read_discovery(&discovery_file(home)) {
        Ok(discovery) => discovery,
        Err(DiscoveryError::Missing) => {
            return match install::find(home) {
                Some(Found::Current { .. }) => State::NotRunning,
                Some(Found::Older { version, .. }) => State::NotInstalled {
                    older: Some(version.unwrap_or_default()),
                },
                None => State::NotInstalled { older: None },
            }
        }
        Err(DiscoveryError::UnknownVersion(version)) => {
            return State::Problem {
                code: "hexVersion",
                message: unknown_version_message(&version),
            }
        }
        // Hex rewrites a damaged file at its next start.
        Err(DiscoveryError::Invalid) => return State::NotRunning,
    };
    state_of(&Client::new(&discovery))
}

pub(crate) fn state_of(client: &Client) -> State {
    let version = match client.health() {
        Ok(version) => version,
        // A file left by a Hex that crashed points at a dead port.
        Err(failure) if failure.code == "hexNotRunning" => return State::NotRunning,
        Err(failure) => {
            return State::Problem {
                code: failure.code,
                message: failure.message,
            }
        }
    };
    match client.models() {
        Ok(models) => State::Ready { version, models },
        Err(failure) if failure.code == "hexNotRunning" => State::NotRunning,
        Err(failure) => State::Problem {
            code: failure.code,
            message: failure.message,
        },
    }
}

/// The models Hex has ready, in Hex's order.
pub(crate) fn ready_models(models: &[HexModel]) -> Vec<&HexModel> {
    models.iter().filter(|model| model.ready()).collect()
}

/// The model Dictate uses: the saved one, or Hex's first ready model when none is saved.
/// `Err((code, sentence))` when it isn't ready.
pub(crate) fn chosen_model<'a>(
    models: &'a [HexModel],
    saved: Option<&str>,
) -> Result<&'a HexModel, (&'static str, String)> {
    match saved {
        Some(id) => match models.iter().find(|model| model.id == id) {
            Some(model) if model.ready() => Ok(model),
            Some(model) => Err((
                "hexModelNotReady",
                format!(
                    "{} isn't ready in Hex. Open Hex and download it there, or pick another model.",
                    model.name
                ),
            )),
            None => Err((
                "hexModelNotReady",
                "The Hex model Dictate was set to is gone. Pick another model.".to_string(),
            )),
        },
        None => ready_models(models).into_iter().next().ok_or((
            "hexNoModels",
            "Hex has no model ready. Open Hex and download a model there.".to_string(),
        )),
    }
}

/// Why Hex can't transcribe now, `(code, sentence)`, or `None` when it can.
pub(crate) fn problem(state: &State, saved: Option<&str>) -> Option<(&'static str, String)> {
    match state {
        State::Unsupported(reason) => Some(("hexUnsupported", reason.clone())),
        State::NotInstalled { older: Some(version) } => Some((
            "hexNotInstalled",
            format!("This computer has Hex {version}, which has no local API. Install Hex 3 to transcribe with it."),
        )),
        State::NotInstalled { older: None } => Some((
            "hexNotInstalled",
            "Hex 3 isn't installed on this computer. Install it to transcribe with Hex.".to_string(),
        )),
        State::NotRunning => Some((
            "hexNotRunning",
            "Hex isn't running on this computer. Open Hex.".to_string(),
        )),
        State::Problem { code, message } => Some((code, message.clone())),
        State::Ready { models, .. } => chosen_model(models, saved).err(),
    }
}

/// `hex` in `/api/readDictateSettings`: the settings row's state.
pub(crate) fn view(state: &State, saved: Option<&str>) -> Value {
    let (code, models, version) = match state {
        State::Unsupported(_) => ("unsupported", Vec::new(), None),
        State::NotInstalled { .. } => ("notInstalled", Vec::new(), None),
        State::NotRunning => ("notRunning", Vec::new(), None),
        State::Problem { .. } => ("problem", Vec::new(), None),
        State::Ready { version, models } => ("ready", ready_models(models), Some(version.as_str())),
    };
    let problem = problem(state, saved);
    let chosen = match state {
        State::Ready { models, .. } => chosen_model(models, saved)
            .ok()
            .map(|model| model.id.clone()),
        _ => None,
    };
    json!({
        "available": !matches!(state, State::Unsupported(_)),
        "state": code,
        "version": version,
        "olderVersion": match state {
            State::NotInstalled { older } => older.clone(),
            _ => None,
        },
        "models": models
            .iter()
            .map(|model| json!({
                "id": model.id,
                "name": model.name,
                "languages": model.languages,
                "supportsLanguageDetection": model.supports_language_detection,
            }))
            .collect::<Vec<_>>(),
        "model": chosen,
        "ready": problem.is_none(),
        "problem": problem.map(|(code, message)| json!({"code": code, "message": message})),
        "downloadUrl": install::DOWNLOAD_PAGE,
    })
}

/// The recording as the PCM WAV Hex takes: a `.wav` as it is (the phone's 24 kHz PCM16), an
/// `.m4a` (macOS keeps recordings as AAC) decoded to 16 kHz mono with `afconvert`.
fn wav_bytes(audio: &Path) -> Result<Vec<u8>, Failure> {
    let gone = || {
        Failure::final_(
            "audioMissing",
            "The recording's audio file is gone, so it can't be transcribed.",
        )
    };
    if !audio.is_file() {
        return Err(gone());
    }
    if audio.extension().is_some_and(|ext| ext == "m4a") {
        let wav = super::engine::decode_m4a(audio).map_err(|detail| {
            Failure::final_(
                "badAudio",
                "Ghostex couldn't prepare the recording for Hex. Transcribe again.",
            )
            .detail(detail)
        })?;
        let bytes = std::fs::read(&wav);
        let _ = std::fs::remove_file(&wav);
        return bytes.map_err(|_| gone());
    }
    std::fs::read(audio).map_err(|_| gone())
}

/// Transcribes `audio` with Hex. Blocking: run it off the async runtime.
pub(crate) fn transcribe(
    home: &Path,
    saved_model: Option<&str>,
    audio: &Path,
    language: Option<&str>,
    timeout: Duration,
) -> Result<String, Failure> {
    let discovery = match read_discovery(&discovery_file(home)) {
        Ok(discovery) => discovery,
        Err(DiscoveryError::UnknownVersion(version)) => {
            return Err(Failure::final_(
                "hexVersion",
                unknown_version_message(&version),
            ))
        }
        Err(DiscoveryError::Missing | DiscoveryError::Invalid) => {
            let state = state(home);
            let (code, message) =
                problem(&state, saved_model).unwrap_or(("hexNotRunning", NOT_RUNNING.to_string()));
            return Err(Failure::final_(code, message));
        }
    };
    transcribe_with(
        &Client::new(&discovery),
        saved_model,
        audio,
        language,
        timeout,
    )
}

pub(crate) fn transcribe_with(
    client: &Client,
    saved_model: Option<&str>,
    audio: &Path,
    language: Option<&str>,
    timeout: Duration,
) -> Result<String, Failure> {
    client.health()?;
    let models = client.models()?;
    let model = chosen_model(&models, saved_model).map_err(|(code, message)| {
        Failure::final_(code, format!("{message} Then transcribe again."))
    })?;
    let language = model
        .language_for(language)
        .map_err(|message| Failure::final_("hexModelNotReady", message))?;
    let wav = wav_bytes(audio)?;
    client.transcribe(model, language.as_deref(), &wav, timeout)
}
