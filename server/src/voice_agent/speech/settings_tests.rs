//! Dictate's settings: the first-run engine is picked once from the saved keys (Hex without one)
//! and then kept, a client's patch is checked before it is saved (settings older clients still
//! send are ignored), and a computer whose engine was our retired local model moves to Hex.

use serde_json::{json, Map, Value};

use super::settings::{self, DictateSettings, Engine};

fn patch(value: Value) -> Map<String, Value> {
    value.as_object().expect("an object").clone()
}

#[test]
fn a_new_install_takes_the_xai_key_then_the_openai_key_then_hex() {
    assert_eq!(DictateSettings::first_run(true, true).engine, Engine::Xai);
    assert_eq!(
        DictateSettings::first_run(false, true).engine,
        Engine::OpenAi
    );
    assert_eq!(DictateSettings::first_run(false, false).engine, Engine::Hex);
}

#[test]
fn a_new_install_has_auto_language() {
    let settings = DictateSettings::first_run(false, false);
    assert_eq!(settings.hex_model, None);
    assert_eq!(settings.language, "auto");
    assert_eq!(settings.language_hint(), None);
}

#[test]
fn the_first_choice_is_kept_when_a_key_is_added_later() {
    let temp = tempfile::tempdir().expect("temp");
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let first = settings::read(&paths, || (false, false));
    assert_eq!(first.engine, Engine::Hex);
    assert!(
        settings::file(&paths).is_file(),
        "written on the first read"
    );
    let mut asked = false;
    let later = settings::read(&paths, || {
        asked = true;
        (true, true)
    });
    assert_eq!(
        later.engine,
        Engine::Hex,
        "a key added later doesn't switch"
    );
    assert!(!asked, "keys are only asked about on the first read");
}

#[test]
fn a_patch_is_saved_and_read_back() {
    let temp = tempfile::tempdir().expect("temp");
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let saved = settings::update(
        &paths,
        &patch(json!({
            "engine": "openai",
            "language": "DE",
            "removeFillers": false,
            "polish": {"enabled": false},
            "dictionary": [{"word": " gxserver ", "aliases": ["GX server", "gx  server"]}],
        })),
        || (false, false),
    )
    .expect("saved");
    assert_eq!(saved.engine, Engine::OpenAi);
    assert_eq!(saved.language, "de");
    assert_eq!(settings::read(&paths, || (true, true)), saved);
}

#[test]
fn bad_values_are_refused_with_a_sentence() {
    let base = DictateSettings::first_run(false, false);
    for (bad, says) in [
        (json!({"engine": "google"}), "engine must be"),
        (
            json!({"engine": "local"}),
            "no longer has its own speech model",
        ),
        (
            json!({"localModel": "whisper-base"}),
            "no longer has its own speech model",
        ),
        (json!({"language": "xx"}), "not a language"),
        (json!({"colour": "red"}), "no setting named colour"),
    ] {
        let error = base
            .patched(&patch(bad.clone()))
            .expect_err(&bad.to_string());
        assert!(error.contains(says), "{bad}: {error}");
    }
}

#[test]
fn hex_is_an_engine_with_its_own_model_choice() {
    let settings = DictateSettings::first_run(false, false);
    assert_eq!(settings.hex_model, None);
    let next = settings
        .patched(&patch(json!({"engine": "hex", "hexModel": "parakeet_v3"})))
        .expect("valid");
    assert_eq!(next.engine, Engine::Hex);
    assert_eq!(next.hex_model.as_deref(), Some("parakeet_v3"));
    let cleared = next
        .patched(&patch(json!({"hexModel": null})))
        .expect("valid");
    assert_eq!(cleared.hex_model, None);
    assert!(settings
        .patched(&patch(json!({"hexModel": "a b&c"})))
        .is_err());
    assert!(settings.patched(&patch(json!({"hexModel": ""}))).is_err());
}
