//! A dictation goes to the engine Dictate's settings name and nowhere else: Hex that isn't ready
//! says so (also for someone who used our retired local engine), and a cloud
//! engine with no key says which key, never trying another.

use serde_json::json;

use crate::voice_agent::focus_tests::test_state;
use crate::voice_agent::recordings::{self, Finished, Piece, Status};
use crate::voice_agent::transcribe;

/// One second of silence as the 24 kHz mono WAV clients upload.
fn silent_wav() -> Vec<u8> {
    let data = 24_000u32 * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&24_000u32.to_le_bytes());
    wav.extend_from_slice(&48_000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    wav.resize(44 + data as usize, 0);
    wav
}

fn record(paths: &crate::paths::GxserverPaths, id: &str) {
    recordings::save_piece(
        paths,
        Piece {
            id,
            offset: 0,
            bytes: &silent_wav(),
            done: Some(Finished {
                created_at: 1,
                duration_ms: 1_000,
                client: "desktop".to_string(),
                dismissed: false,
                language: None,
            }),
        },
    )
    .expect("saved");
}

fn choose(paths: &crate::paths::GxserverPaths, engine: &str) {
    let patch = json!({"engine": engine});
    super::settings::update(paths, patch.as_object().expect("object"), || {
        (false, false, false)
    })
    .expect("settings");
}

#[tokio::test]
async fn our_retired_local_engine_moves_to_hex() {
    let temp = tempfile::tempdir().expect("temp");
    let state = test_state(&temp);
    let file = super::settings::file(&state.paths);
    std::fs::create_dir_all(file.parent().expect("folder")).expect("folder");
    std::fs::write(
        &file,
        r#"{"engine":"local","localModel":"parakeet-tdt-0.6b-v3","language":"auto","polish":{"enabled":false,"model":"claude-sonnet-5-5"}}"#,
    )
    .expect("old settings");
    record(&state.paths, "hex-1");
    let first = transcribe::transcribe(&state, "hex-1")
        .await
        .expect("answered");
    assert_eq!(first.status, Status::Failed);
    let error = first.error.expect("a reason");
    assert_eq!(
        error.provider.as_deref(),
        Some("hex"),
        "never a paid engine"
    );
    assert!(error.code.starts_with("hex"), "{}", error.code);
}

#[tokio::test]
async fn a_cloud_engine_with_no_key_names_that_key() {
    let temp = tempfile::tempdir().expect("temp");
    let state = test_state(&temp);
    choose(&state.paths, "openai");
    record(&state.paths, "openai-1");
    let recording = transcribe::transcribe(&state, "openai-1")
        .await
        .expect("answered");
    let error = recording.error.expect("a reason");
    assert_eq!(error.code, "noKey");
    assert_eq!(error.provider.as_deref(), Some("openai"));
    assert!(
        error.message.contains("set to use OpenAI"),
        "{}",
        error.message
    );
}

#[test]
fn the_tooltip_names_the_engine_and_model_and_what_is_missing() {
    let temp = tempfile::tempdir().expect("temp");
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let mut settings = super::settings::DictateSettings::first_run(false, false, false);
    assert_eq!(super::dictate::engine_label(&settings), "Hex");
    let not_running = super::hex::State::NotRunning;
    // A Mac with Hex 3 installed opens it for the transcription, so the mic is ready.
    let expected = (!super::hex::opens_by_itself(&paths.home_dir)).then(|| {
        (
            "hexNotRunning",
            "Hex isn't running on this computer. Open Hex.".to_string(),
        )
    });
    assert_eq!(
        super::dictate::engine_problem(&paths, &settings, Some(&not_running)),
        expected
    );
    settings.engine = super::settings::Engine::Xai;
    assert_eq!(
        super::dictate::engine_label(&settings),
        "xAI speech to text"
    );
    assert_eq!(
        super::dictate::engine_problem(&paths, &settings, None),
        Some(("noKey", "Add your xAI API key in Settings.".to_string()))
    );
    settings.engine = super::settings::Engine::OpenAi;
    assert_eq!(
        super::dictate::engine_label(&settings),
        "OpenAI gpt-4o-transcribe"
    );
}
