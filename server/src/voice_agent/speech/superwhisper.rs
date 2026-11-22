//! The Superwhisper engine: Dictate's recording transcribed by Superwhisper on this Linux computer,
//! through its command line (`superwhisper`, which talks to its `superwhisper.service` and starts
//! it when it isn't running).
//!
//! `superwhisper transcribe FILE` takes the recording and answers `{"ok": true, "session_id": N}`
//! at once (`{"error": "…"}` with a non-zero exit when it can't read the file). The text arrives a
//! moment later in `superwhisper read`, the last dictation (`{session_id, createdAt, text,
//! warning}`); without `--insert` Superwhisper neither pastes it nor touches the clipboard.
//! Session ids start again at 1 when the service restarts, and `createdAt` is the recording's
//! time rather than the request's, so a result is ours when its id matches and it isn't the last
//! dictation `read` showed before the request.
//!
//! CDXC:Dictate 2026-11-14 DECISION: User: "Omarchy comes with Superwhisper pre-installed. I think a lot of people are going to use that… Maybe we should offer Superwhisper on Linux to support Omarchy better… Same way we did for Hex. So, we forward the dictation recording to Superwhisper. Then it gets transcribed there". Superwhisper is a Dictate engine on Linux, offered where it is installed; it transcribes with its own processing mode, language, tone and vocabulary, so Dictate's language setting doesn't apply to it.
//! SEE-ALSO: hex.rs (the other local engine), apps/desktop/src/app/window/settings_modal/tabs/voice_dictate.rs (the engine picker).

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::hex::Failure;

/// How long one command may take; the first one after a restart starts the service.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// How often `read` is asked while Superwhisper transcribes.
const RESULT_POLL: Duration = Duration::from_millis(200);

const NOT_INSTALLED: &str =
    "Superwhisper isn't installed on this computer. Install it, or pick another engine for Dictate.";

/// Where the user stands with Superwhisper on this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// Superwhisper's command line exists only on Linux here.
    Unsupported,
    NotInstalled,
    Installed,
}

/// The `superwhisper` command: on `PATH`, else where its packages put it (gxserver may run with a
/// short `PATH` when started by the system).
pub(crate) fn binary(home: &Path) -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend([
        home.join(".local/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]);
    dirs.into_iter()
        .map(|dir| dir.join("superwhisper"))
        .find(|path| path.is_file())
}

/// Whether Superwhisper is installed. Asks nothing of the service, so reading Settings never
/// starts it.
pub(crate) fn state(home: &Path) -> State {
    if !cfg!(target_os = "linux") {
        State::Unsupported
    } else if binary(home).is_some() {
        State::Installed
    } else {
        State::NotInstalled
    }
}

/// Why Superwhisper can't transcribe now, `(code, sentence)`, or `None` when it can.
pub(crate) fn problem(state: &State) -> Option<(&'static str, String)> {
    match state {
        State::Unsupported => Some((
            "superwhisperUnsupported",
            "Superwhisper dictation works with Ghostex on Linux only.".to_string(),
        )),
        State::NotInstalled => Some(("superwhisperNotInstalled", NOT_INSTALLED.to_string())),
        State::Installed => None,
    }
}

/// `superwhisper` in `/api/readDictateSettings`: whether to offer it and where the user stands.
pub(crate) fn view(state: &State) -> Value {
    let problem = problem(state);
    json!({
        "available": *state == State::Installed,
        "state": match state {
            State::Unsupported => "unsupported",
            State::NotInstalled => "notInstalled",
            State::Installed => "installed",
        },
        "ready": problem.is_none(),
        "problem": problem.map(|(code, message)| json!({"code": code, "message": message})),
    })
}

/// Starts Superwhisper's service in the background when it isn't running (the microphone was
/// pressed: it warms up while the user speaks).
pub(crate) fn prepare_in_background(home: &Path) {
    let Some(binary) = binary(home) else { return };
    std::thread::spawn(move || {
        let _ = run(&binary, &["status"], COMMAND_TIMEOUT);
    });
}

/// Runs one `superwhisper` command and reads its JSON answer. `Err` holds the detail to log.
fn run(binary: &Path, args: &[&str], timeout: Duration) -> Result<Value, String> {
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Couldn't run superwhisper: {error}"))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "superwhisper {} didn't answer within {} seconds.",
                    args.first().copied().unwrap_or_default(),
                    timeout.as_secs()
                ));
            }
            Err(error) => return Err(error.to_string()),
        }
    };
    let text = reader.join().unwrap_or_default();
    let answer: Value = serde_json::from_str(text.trim()).unwrap_or(Value::Null);
    // A refused command exits non-zero with `{"error": …}`. `read` exits non-zero too when the last
    // dictation failed, and still answers that dictation (with its `session_id` and `error`).
    if !status.success() && answer.get("session_id").is_none() {
        return Err(answer["error"].as_str().map_or_else(
            || format!("superwhisper exited with {status}."),
            str::to_string,
        ));
    }
    Ok(answer)
}

fn failure(code: &'static str, message: &str, transient: bool, detail: Option<String>) -> Failure {
    Failure {
        code,
        message: message.to_string(),
        detail,
        transient,
    }
}

/// Transcribes `audio` (the WAV gxserver keeps) with Superwhisper. Blocking: run it off the async
/// runtime.
pub(crate) fn transcribe(home: &Path, audio: &Path, timeout: Duration) -> Result<String, Failure> {
    let Some(binary) = binary(home) else {
        return Err(failure(
            "superwhisperNotInstalled",
            NOT_INSTALLED,
            false,
            None,
        ));
    };
    if !audio.is_file() {
        return Err(failure(
            "audioMissing",
            "The recording's audio file is gone, so it can't be transcribed.",
            false,
            None,
        ));
    }
    // Also starts the service when it isn't running, so the ids below come from one run of it.
    let before = run(&binary, &["read"], COMMAND_TIMEOUT).ok();
    let path = audio.to_string_lossy();
    let accepted = run(&binary, &["transcribe", &*path], COMMAND_TIMEOUT).map_err(|detail| {
        if detail.contains("decode") {
            failure(
                "badAudio",
                "Superwhisper couldn't read the recording. Transcribe again; if it keeps failing, record it again.",
                false,
                Some(detail),
            )
        } else {
            failure(
                "superwhisperUnavailable",
                "Superwhisper didn't take the recording. Check that Superwhisper works on this computer, then transcribe again.",
                true,
                Some(detail),
            )
        }
    })?;
    let Some(session) = accepted["session_id"].as_u64() else {
        return Err(failure(
            "superwhisperFailed",
            "Superwhisper's answer couldn't be read. Update Superwhisper, then transcribe again.",
            false,
            Some(accepted.to_string()),
        ));
    };
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        std::thread::sleep(RESULT_POLL);
        // A failed `read` (the service restarting) is asked again until the deadline.
        let Ok(last) = run(&binary, &["read"], COMMAND_TIMEOUT) else {
            continue;
        };
        if last["session_id"].as_u64() != Some(session) || before.as_ref() == Some(&last) {
            continue;
        }
        let text = last["text"].as_str().unwrap_or_default().trim().to_string();
        let warning = last["warning"].as_str().unwrap_or_default();
        let error = last["error"].as_str().unwrap_or_default();
        // "No speech detected" is an empty dictation, as Hex answers one.
        let silent = warning.is_empty() || warning == "No speech detected";
        if !text.is_empty() || (error.is_empty() && silent) {
            return Ok(text);
        }
        let detail = if error.is_empty() { warning } else { error };
        return Err(failure(
            "superwhisperFailed",
            "Superwhisper couldn't transcribe this recording. Transcribe again; if it keeps failing, pick another engine.",
            false,
            Some(detail.to_string()),
        ));
    }
    Err(failure(
        "superwhisperTimeout",
        &format!(
            "Superwhisper didn't finish within {} seconds. Check that dictating in Superwhisper itself works, then transcribe again.",
            timeout.as_secs()
        ),
        false,
        None,
    ))
}
