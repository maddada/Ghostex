//! The Hex engine: `local-api.json` is read and checked (API version, token shape), requests are
//! built the way Hex's local API takes them, every refusal reads as a sentence that says what to
//! do in Hex, and a fake Hex that follows the spec (`docs/specs/local-transcription-service.md`)
//! transcribes, refuses an unprepared model with 409 and never makes Dictate hang.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::hex::{
    self, chosen_model, parse_discovery, read_discovery, status_failure, transcription_path,
    unknown_version_message, Client, DiscoveryError, HexModel, State,
};

const TOKEN: &str = "hex_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn discovery_json(port: u16, api: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"port": port, "token": TOKEN, "apiVersion": api, "pid": 4242}))
        .expect("json")
}

fn model(id: &str, verified: bool, languages: &[&str], detects: bool) -> HexModel {
    HexModel {
        id: id.to_string(),
        name: format!("Model {id}"),
        installed: verified,
        verified,
        managed: false,
        languages: languages.iter().map(|code| code.to_string()).collect(),
        supports_language_detection: detects,
    }
}

#[test]
fn local_api_json_is_read_with_its_port_and_token() {
    let discovery = parse_discovery(&discovery_json(49731, "2")).expect("valid");
    assert_eq!(discovery.port, 49731);
    assert_eq!(discovery.token, TOKEN);
    assert_eq!(discovery.api_version, "2");
    assert_eq!(discovery.pid, 4242);
}

#[test]
fn a_missing_local_api_json_means_hex_is_not_running() {
    let temp = tempfile::tempdir().expect("temp");
    assert_eq!(
        read_discovery(&temp.path().join("local-api.json")),
        Err(DiscoveryError::Missing)
    );
}

#[test]
fn a_damaged_file_or_a_token_of_the_wrong_shape_is_refused() {
    assert_eq!(parse_discovery(b"{not json"), Err(DiscoveryError::Invalid));
    for token in [
        "hex_short",
        "nothex_0123",
        &TOKEN.to_uppercase().replace("HEX_", "hex_"),
    ] {
        let bytes = serde_json::to_vec(&json!({"port": 1, "token": token, "apiVersion": "2"}))
            .expect("json");
        assert_eq!(
            parse_discovery(&bytes),
            Err(DiscoveryError::Invalid),
            "{token}"
        );
    }
    let no_port =
        serde_json::to_vec(&json!({"port": 0, "token": TOKEN, "apiVersion": "2"})).expect("json");
    assert_eq!(parse_discovery(&no_port), Err(DiscoveryError::Invalid));
}

#[test]
fn an_api_version_ghostex_does_not_speak_is_refused_with_what_to_update() {
    assert_eq!(
        parse_discovery(&discovery_json(1, "3")),
        Err(DiscoveryError::UnknownVersion("3".to_string()))
    );
    assert!(unknown_version_message("3").contains("Update Ghostex"));
    assert!(unknown_version_message("1").contains("Update Hex"));
    assert!(unknown_version_message("next").contains("doesn't know"));
}

#[test]
fn the_token_never_shows_in_debug_output() {
    let discovery = parse_discovery(&discovery_json(1, "2")).expect("valid");
    let printed = format!("{discovery:?}");
    assert!(!printed.contains(TOKEN), "{printed}");
    assert!(printed.contains("<hidden>"));
}

#[test]
fn hex_keeps_its_file_in_its_own_data_folder_on_each_system() {
    let home = Path::new("/home/someone");
    let none = |_: &str| None;
    let dir = crate::managed_tools::hex::support_dir_from(home, none);
    match crate::managed_tools::platform::os() {
        crate::managed_tools::platform::Os::Mac => {
            assert_eq!(dir, home.join("Library/Application Support/hex"))
        }
        crate::managed_tools::platform::Os::Linux => assert_eq!(dir, home.join(".local/share/hex")),
        crate::managed_tools::platform::Os::Windows => {
            assert_eq!(dir, home.join("AppData").join("Roaming").join("hex"))
        }
    }
    let overridden = crate::managed_tools::hex::support_dir_from(home, |name| {
        (name == "HEX_APPLICATION_SUPPORT_DIR").then(|| "/elsewhere/hex".into())
    });
    assert_eq!(overridden, Path::new("/elsewhere/hex"));
}

#[test]
fn the_request_names_the_model_and_language_in_its_query() {
    assert_eq!(
        transcription_path("parakeet_v3", Some("de")),
        "/transcriptions?model=parakeet%5Fv3&language=de"
    );
    assert_eq!(
        transcription_path("custom-a b&c", None),
        "/transcriptions?model=custom%2Da%20b%26c"
    );
}

#[test]
fn auto_is_sent_only_to_a_model_that_detects_languages() {
    let whisper = model("whisper_large_v3_turbo", true, &["en", "de", "auto"], true);
    let parakeet = model("parakeet_v2", true, &["en"], false);
    assert_eq!(whisper.language_for(None), Ok(Some("auto".to_string())));
    assert_eq!(parakeet.language_for(None), Ok(None));
    assert_eq!(
        parakeet.language_for(Some("en")),
        Ok(Some("en".to_string()))
    );
    let refused = parakeet.language_for(Some("de")).expect_err("no German");
    assert!(refused.contains("doesn't transcribe German"), "{refused}");
    let any = model("cohere", true, &["*"], false);
    assert_eq!(any.language_for(Some("ja")), Ok(Some("ja".to_string())));
}

#[test]
fn the_saved_model_is_used_and_hex_first_ready_model_when_none_is_saved() {
    let models = vec![
        model("a", false, &["en"], false),
        model("b", true, &["en"], false),
        model("c", true, &["en"], false),
    ];
    assert_eq!(chosen_model(&models, None).expect("ready").id, "b");
    assert_eq!(chosen_model(&models, Some("c")).expect("ready").id, "c");
    let (code, message) = chosen_model(&models, Some("a")).expect_err("not ready");
    assert_eq!(code, "hexModelNotReady");
    assert!(message.contains("Open Hex and download it"), "{message}");
    let (code, _) = chosen_model(&models[..1], None).expect_err("none ready");
    assert_eq!(code, "hexNoModels");
}

#[test]
fn every_refusal_says_what_to_do_and_which_ones_are_retried() {
    let cases: [(u16, &str, &str, bool); 11] = [
        (409, "model-not-ready", "hexModelNotReady", false),
        (409, "cancelled", "hexBusy", true),
        (401, "", "hexRefused", false),
        (400, "unsupported-model", "hexModelNotReady", false),
        (413, "resource-exhausted", "badAudio", false),
        (415, "unsupported-audio", "badAudio", false),
        (408, "upload-timeout", "hexUnavailable", true),
        (429, "queue-full", "hexBusy", true),
        (503, "service-gone", "hexUnavailable", true),
        (500, "model-load-failed", "hexFailed", false),
        (500, "transcription-failed", "hexFailed", false),
    ];
    for (status, code, expected, transient) in cases {
        let body = if code.is_empty() {
            String::new()
        } else {
            json!({"code": code}).to_string()
        };
        let failure = status_failure(status, &body, "Parakeet v3");
        assert_eq!(failure.code, expected, "{status} {code}");
        assert_eq!(failure.transient, transient, "{status} {code}");
        assert!(failure.message.ends_with('.'), "{}", failure.message);
        assert!(failure
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains(&status.to_string()));
    }
    let not_ready = status_failure(409, r#"{"code":"model-not-ready"}"#, "Parakeet v3");
    assert!(not_ready
        .message
        .contains("Parakeet v3 isn't ready in Hex. Open Hex and download it"));
}

#[test]
fn the_row_says_where_the_user_stands() {
    let view = hex::view(&State::NotInstalled { older: None }, None);
    assert_eq!(view["state"], "notInstalled");
    assert_eq!(view["available"], true);
    assert_eq!(view["ready"], false);
    assert!(view["problem"]["message"]
        .as_str()
        .unwrap()
        .contains("Hex 3 isn't installed"));
    assert_eq!(
        view["downloadUrl"],
        "https://github.com/anomalyco/hex#downloads"
    );

    let older = hex::view(
        &State::NotInstalled {
            older: Some("0.6.2".into()),
        },
        None,
    );
    assert!(older["problem"]["message"]
        .as_str()
        .unwrap()
        .contains("Hex 0.6.2, which has no local API"));
    assert_eq!(older["olderVersion"], "0.6.2");

    let stopped = hex::view(&State::NotRunning, None);
    assert_eq!(stopped["state"], "notRunning");
    assert!(stopped["problem"]["message"]
        .as_str()
        .unwrap()
        .contains("Open Hex"));

    let unsupported = hex::view(
        &State::Unsupported("Hex 3 runs only on Macs with Apple silicon.".into()),
        None,
    );
    assert_eq!(unsupported["available"], false);

    let ready = hex::view(
        &State::Ready {
            version: "3.3.0".into(),
            models: vec![
                model("a", false, &["en"], false),
                model("b", true, &["en"], false),
            ],
        },
        None,
    );
    assert_eq!(ready["state"], "ready");
    assert_eq!(ready["ready"], true);
    assert_eq!(ready["version"], "3.3.0");
    assert_eq!(ready["model"], "b");
    let ids: Vec<&str> = ready["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["b"], "only models Hex has ready are offered");
    assert!(!ready.to_string().contains("hex_"), "no token in the view");
}

// A fake Hex 3 local API, following the spec: bearer token on every request (an empty 401
// otherwise), `/health`, `/models`, and `POST /transcriptions` that needs `audio/wav`, a
// `Content-Length` and a `model`, never downloads (409 `model-not-ready`), and answers
// `{transcript, durationMs}`.

#[derive(Default)]
struct Seen {
    paths: Vec<String>,
    content_type: Option<String>,
    body_len: usize,
}

struct FakeHex {
    port: u16,
    seen: Arc<Mutex<Seen>>,
}

fn fake_hex(api_version: &'static str, models: Value) -> FakeHex {
    let listener = TcpListener::bind("127.0.0.1:0").expect("free port");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            serve(stream, api_version, &models, &record);
        }
    });
    FakeHex { port, seen }
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        409 => "Conflict",
        _ => "Error",
    };
    let content_type = if body.is_empty() {
        ""
    } else {
        "Content-Type: application/json\r\n"
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn serve(mut stream: TcpStream, api_version: &str, models: &Value, seen: &Mutex<Seen>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v.clone())
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    seen.lock()
        .unwrap()
        .paths
        .push(format!("{method} {target}"));
    if header("authorization").as_deref() != Some(&format!("Bearer {TOKEN}")) {
        return respond(&mut stream, 401, "");
    }
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    match (method.as_str(), path) {
        ("GET", "/health") => respond(
            &mut stream,
            200,
            &json!({"version": "3.3.0", "apiVersion": api_version}).to_string(),
        ),
        ("GET", "/models") => respond(&mut stream, 200, &models.to_string()),
        ("POST", "/transcriptions") => {
            if header("content-type").as_deref() != Some("audio/wav") {
                return respond(&mut stream, 415, r#"{"code":"unsupported-audio"}"#);
            }
            let Some(length) = header("content-length").and_then(|v| v.parse::<usize>().ok())
            else {
                return respond(&mut stream, 411, r#"{"code":"length-required"}"#);
            };
            let mut body = vec![0u8; length];
            if reader.read_exact(&mut body).is_err() || !body.starts_with(b"RIFF") {
                return respond(&mut stream, 400, r#"{"code":"invalid-audio"}"#);
            }
            {
                let mut seen = seen.lock().unwrap();
                seen.content_type = header("content-type");
                seen.body_len = length;
            }
            let model_id = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("model="))
                .map(|id| {
                    percent_encoding::percent_decode_str(id)
                        .decode_utf8_lossy()
                        .into_owned()
                });
            let entry = models.as_array().and_then(|models| {
                models
                    .iter()
                    .find(|m| Some(m["id"].as_str().unwrap_or_default().to_string()) == model_id)
            });
            match entry {
                None => respond(&mut stream, 400, r#"{"code":"unknown-model"}"#),
                Some(entry) if entry["verified"] != true => {
                    respond(&mut stream, 409, r#"{"code":"model-not-ready"}"#)
                }
                Some(_) if model_id.as_deref() == Some("slow") => {
                    std::thread::sleep(Duration::from_secs(5));
                }
                Some(_) => respond(
                    &mut stream,
                    200,
                    &json!({"transcript": "  hello from hex  ", "durationMs": 1500}).to_string(),
                ),
            }
        }
        _ => respond(&mut stream, 404, r#"{"code":"not_found"}"#),
    }
}

fn catalog() -> Value {
    json!([
        {"id": "parakeet_v3", "name": "Parakeet v3", "installed": true, "verified": true, "managed": false,
         "downloadBytes": 1, "languages": ["en", "de"], "supportsLanguageDetection": false, "source": "hex"},
        {"id": "whisper_large_v3_turbo", "name": "Whisper Turbo", "installed": false, "verified": false,
         "managed": false, "downloadBytes": 1, "languages": ["en", "auto"], "supportsLanguageDetection": true},
        {"id": "slow", "name": "Slow", "installed": true, "verified": true, "managed": false,
         "languages": ["en"], "supportsLanguageDetection": false}
    ])
}

/// A short 16 kHz mono PCM16 WAV (half a second of silence).
fn wav_file(dir: &Path) -> std::path::PathBuf {
    let samples = 8_000u32;
    let data_len = samples * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&16_000u32.to_le_bytes());
    wav.extend_from_slice(&32_000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.resize(wav.len() + data_len as usize, 0);
    let path = dir.join("recording.wav");
    std::fs::write(&path, wav).expect("wav");
    path
}

fn client(port: u16) -> Client {
    Client::at(format!("http://127.0.0.1:{port}"), TOKEN.to_string())
}

#[test]
fn a_fake_hex_transcribes_the_recording_with_the_token() {
    let hex = fake_hex("2", catalog());
    let temp = tempfile::tempdir().expect("temp");
    let audio = wav_file(temp.path());
    let text = hex::transcribe_with(
        &client(hex.port),
        None,
        &audio,
        Some("de"),
        Duration::from_secs(10),
    )
    .expect("transcribed");
    assert_eq!(text, "hello from hex");
    let seen = hex.seen.lock().unwrap();
    assert_eq!(seen.content_type.as_deref(), Some("audio/wav"));
    assert_eq!(
        seen.body_len as u64,
        std::fs::metadata(&audio).unwrap().len()
    );
    assert!(
        seen.paths
            .contains(&"POST /transcriptions?model=parakeet%5Fv3&language=de".to_string()),
        "{:?}",
        seen.paths
    );
}

#[test]
fn the_fake_hex_state_lists_only_ready_models() {
    let hex = fake_hex("2", catalog());
    let State::Ready { version, models } = hex::state_of(&client(hex.port)) else {
        panic!("ready");
    };
    assert_eq!(version, "3.3.0");
    let view = hex::view(&State::Ready { version, models }, None);
    let ids: Vec<&str> = view["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["parakeet_v3", "slow"]);
}

#[test]
fn an_unprepared_model_gets_409_and_a_sentence_about_hex() {
    let hex = fake_hex("2", catalog());
    let models: Vec<HexModel> = serde_json::from_value(catalog()).unwrap();
    let whisper = models
        .iter()
        .find(|m| m.id == "whisper_large_v3_turbo")
        .unwrap();
    let temp = tempfile::tempdir().expect("temp");
    let wav = std::fs::read(wav_file(temp.path())).unwrap();
    // Straight to the POST, as if the model went away between /models and the upload.
    let failure = client(hex.port)
        .transcribe(whisper, Some("auto"), &wav, Duration::from_secs(10))
        .expect_err("not ready");
    assert_eq!(failure.code, "hexModelNotReady");
    assert!(!failure.transient);
    assert!(failure
        .message
        .contains("Whisper Turbo isn't ready in Hex. Open Hex and download it"));
    // The saved model is checked against /models first.
    let audio = wav_file(temp.path());
    let failure = hex::transcribe_with(
        &client(hex.port),
        Some("whisper_large_v3_turbo"),
        &audio,
        None,
        Duration::from_secs(10),
    )
    .expect_err("not ready");
    assert_eq!(failure.code, "hexModelNotReady");
}

#[test]
fn a_wrong_token_is_refused_with_what_to_do() {
    let hex = fake_hex("2", catalog());
    let wrong = Client::at(format!("http://127.0.0.1:{}", hex.port), "hex_wrong".into());
    let failure = wrong.health().expect_err("refused");
    assert_eq!(failure.code, "hexRefused");
    assert!(failure.message.contains("Quit and reopen Hex"));
    assert!(!failure.message.contains("hex_wrong"));
}

#[test]
fn a_hex_with_another_api_version_is_refused_before_any_upload() {
    let hex = fake_hex("3", catalog());
    let temp = tempfile::tempdir().expect("temp");
    let audio = wav_file(temp.path());
    let failure = hex::transcribe_with(
        &client(hex.port),
        None,
        &audio,
        None,
        Duration::from_secs(10),
    )
    .expect_err("refused");
    assert_eq!(failure.code, "hexVersion");
    assert!(failure.message.contains("Update Ghostex"));
    assert!(!hex
        .seen
        .lock()
        .unwrap()
        .paths
        .iter()
        .any(|path| path.starts_with("POST")));
}

#[test]
fn a_dead_port_fails_at_once_with_open_hex() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("free port");
        listener.local_addr().unwrap().port()
    };
    let temp = tempfile::tempdir().expect("temp");
    let audio = wav_file(temp.path());
    let started = Instant::now();
    let failure = hex::transcribe_with(&client(port), None, &audio, None, Duration::from_secs(10))
        .expect_err("nothing there");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(failure.code, "hexNotRunning");
    assert!(!failure.transient);
    assert_eq!(failure.message, hex::NOT_RUNNING);
    assert_eq!(hex::state_of(&client(port)), State::NotRunning);
}

#[test]
fn a_hex_that_never_answers_times_out_instead_of_hanging() {
    let hex = fake_hex("2", catalog());
    let temp = tempfile::tempdir().expect("temp");
    let audio = wav_file(temp.path());
    let started = Instant::now();
    let failure = hex::transcribe_with(
        &client(hex.port),
        Some("slow"),
        &audio,
        None,
        Duration::from_secs(1),
    )
    .expect_err("timed out");
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(failure.code, "hexTimeout");
    assert!(failure.message.contains("didn't answer within 1 seconds"));
}

#[test]
fn local_api_json_in_hex_data_folder_leads_to_the_running_hex() {
    let hex = fake_hex("2", catalog());
    let home = tempfile::tempdir().expect("home");
    let support = crate::managed_tools::hex::support_dir(home.path());
    if crate::managed_tools::hex::supported().is_err() || !support.starts_with(home.path()) {
        // No Hex 3 for this computer, or HEX_APPLICATION_SUPPORT_DIR points elsewhere.
        return;
    }
    std::fs::create_dir_all(&support).unwrap();
    std::fs::write(
        support.join("local-api.json"),
        discovery_json(hex.port, "2"),
    )
    .unwrap();
    assert!(matches!(hex::state(home.path()), State::Ready { .. }));
    let audio = wav_file(home.path());
    let text =
        hex::transcribe(home.path(), None, &audio, None, Duration::from_secs(10)).expect("text");
    assert_eq!(text, "hello from hex");

    std::fs::write(
        support.join("local-api.json"),
        discovery_json(hex.port, "9"),
    )
    .unwrap();
    let failure = hex::transcribe(home.path(), None, &audio, None, Duration::from_secs(10))
        .expect_err("version");
    assert_eq!(failure.code, "hexVersion");
}

/// Against a real Hex 3 (`hex service --embedded` from a scratch folder): `HEX_REAL_URL`,
/// `HEX_REAL_TOKEN`, `HEX_REAL_AUDIO` (a .wav or .m4a), `HEX_REAL_MODEL`, and `HEX_REAL_EXPECT`
/// (`text` or `notReady`). Run by hand with `--ignored`; the token comes from the environment
/// and is never printed.
#[test]
#[ignore]
fn a_real_hex_transcribes_or_refuses_an_unprepared_model() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let client = Client::at(var("HEX_REAL_URL"), var("HEX_REAL_TOKEN"));
    let audio = std::path::PathBuf::from(var("HEX_REAL_AUDIO"));
    let result = hex::transcribe_with(
        &client,
        Some(&var("HEX_REAL_MODEL")),
        &audio,
        None,
        Duration::from_secs(120),
    );
    match var("HEX_REAL_EXPECT").as_str() {
        "text" => {
            let text = result.expect("transcribed");
            println!("Hex transcribed: {text:?}");
            assert!(!text.is_empty());
        }
        _ => {
            let failure = result.expect_err("not ready");
            println!("Hex refused: {} ({:?})", failure.message, failure.detail);
            assert_eq!(failure.code, "hexModelNotReady");
        }
    }
}
