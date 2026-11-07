//! Dictate's settings: the first-run engine is picked once from the saved keys and then kept, AI
//! polish starts on, and a client's patch is checked before it is saved.

use serde_json::{json, Map, Value};

use super::settings::{self, DictateSettings, Engine};

fn patch(value: Value) -> Map<String, Value> {
    value.as_object().expect("an object").clone()
}

#[test]
fn a_new_install_takes_the_xai_key_then_the_openai_key_then_local() {
    assert_eq!(DictateSettings::first_run(true, true).engine, Engine::Xai);
    assert_eq!(
        DictateSettings::first_run(false, true).engine,
        Engine::OpenAi
    );
    assert_eq!(
        DictateSettings::first_run(false, false).engine,
        Engine::Local
    );
}

#[test]
fn a_new_install_has_parakeet_auto_language_fillers_off_the_text_and_polish_on() {
    let settings = DictateSettings::first_run(false, false);
    assert_eq!(settings.local_model, "parakeet-tdt-0.6b-v3");
    assert_eq!(settings.language, "auto");
    assert_eq!(settings.language_hint(), None);
    assert!(settings.remove_fillers);
    assert!(settings.add_project_names);
    assert!(settings.polish.enabled);
    assert_eq!(settings.polish.model, "claude-sonnet-5-5");
    assert_eq!(settings.polish.prompt, None);
}

#[test]
fn the_first_choice_is_kept_when_a_key_is_added_later() {
    let temp = tempfile::tempdir().expect("temp");
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let first = settings::read(&paths, || (false, false));
    assert_eq!(first.engine, Engine::Local);
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
        Engine::Local,
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
            "localModel": "whisper-base",
            "language": "DE",
            "removeFillers": false,
            "polish": {"enabled": false},
            "dictionary": [{"word": " gxserver ", "aliases": ["GX server", "gx  server"]}],
        })),
        || (false, false),
    )
    .expect("saved");
    assert_eq!(saved.engine, Engine::OpenAi);
    assert_eq!(saved.local_model, "whisper-base");
    assert_eq!(saved.language, "de");
    assert!(!saved.remove_fillers);
    assert!(!saved.polish.enabled);
    assert_eq!(
        saved.polish.model, "claude-sonnet-5-5",
        "polish merges its own keys"
    );
    assert_eq!(saved.dictionary.len(), 1);
    assert_eq!(saved.dictionary[0].word, "gxserver");
    assert_eq!(
        saved.dictionary[0].aliases,
        vec!["GX server".to_string()],
        "trimmed and deduplicated"
    );
    assert_eq!(settings::read(&paths, || (true, true)), saved);
}

#[test]
fn bad_values_are_refused_with_a_sentence() {
    let base = DictateSettings::first_run(false, false);
    for (bad, says) in [
        (json!({"engine": "google"}), "engine must be"),
        (json!({"localModel": "whisper-tiny"}), "not a local model"),
        (json!({"language": "xx"}), "not a language"),
        (json!({"removeFillers": "yes"}), "true or false"),
        (json!({"polish": {"model": "gpt-5"}}), "claude-sonnet-5-5"),
        (
            json!({"dictionary": [{"word": "kubectl", "aliases": ["kc"]}]}),
            "3 to 64",
        ),
        (json!({"colour": "red"}), "no setting named colour"),
    ] {
        let error = base
            .patched(&patch(bad.clone()))
            .expect_err(&bad.to_string());
        assert!(error.contains(says), "{bad}: {error}");
    }
}

#[test]
fn the_dictionary_is_limited_and_repeats_are_dropped() {
    let base = DictateSettings::first_run(false, false);
    let words: Vec<Value> = (0..201)
        .map(|i| json!({"word": format!("word{i}")}))
        .collect();
    let error = base
        .patched(&patch(json!({ "dictionary": words })))
        .expect_err("too many");
    assert!(error.contains("200"), "{error}");
    let repeated = base
        .patched(&patch(
            json!({"dictionary": [{"word": "SpaceO"}, {"word": "spaceo"}, {"word": "  "}]}),
        ))
        .expect("saved");
    assert_eq!(repeated.dictionary.len(), 1);
}

#[test]
fn an_empty_prompt_means_the_built_in_one() {
    let base = DictateSettings::first_run(false, false);
    let custom = base
        .patched(&patch(
            json!({"polish": {"prompt": "  Fix commas only.  "}}),
        ))
        .expect("saved");
    assert_eq!(custom.polish.prompt.as_deref(), Some("Fix commas only."));
    let reset = custom
        .patched(&patch(json!({"polish": {"prompt": ""}})))
        .expect("saved");
    assert_eq!(reset.polish.prompt, None);
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
    // Settings saved before Hex existed still read.
    let old = r#"{"engine":"local","localModel":"parakeet-tdt-0.6b-v3","language":"auto","polish":{"enabled":true,"model":"claude-sonnet-5-5"}}"#;
    let read: DictateSettings = serde_json::from_str(old).expect("old settings");
    assert_eq!(read.hex_model, None);
}
