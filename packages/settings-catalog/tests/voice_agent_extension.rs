use ghostex_settings_catalog::{built_in_extensions, default_value, J};

use built_in_extensions::VOICE_AGENT;

fn saved(voice_agent_hidden: Option<bool>) -> impl Fn(&str) -> Option<bool> {
    move |key| match key {
        "voiceAgentHidden" => voice_agent_hidden,
        _ => None,
    }
}

#[test]
fn voice_agent_is_a_built_in_feature_on_by_default() {
    assert_eq!(
        built_in_extensions::switch_key(VOICE_AGENT),
        Some(("voiceAgentHidden", false))
    );
    assert_eq!(default_value("voiceAgentHidden"), Some(&J::Bool(false)));
    assert_eq!(built_in_extensions::title(VOICE_AGENT), "Voice agent");
    assert!(built_in_extensions::on_with(VOICE_AGENT, saved(None)));
    assert!(built_in_extensions::enabled_features_with(saved(None)).contains(&VOICE_AGENT));
    assert!(!built_in_extensions::disabled_features_with(saved(None)).contains(&VOICE_AGENT));
}

#[test]
fn voice_agent_needs_no_experimental_gate() {
    assert_eq!(built_in_extensions::gate_key(VOICE_AGENT), None);
    let experimental_off = |key: &str| match key {
        "showBetaFeatures" => Some(false),
        _ => None,
    };
    assert!(built_in_extensions::on_with(VOICE_AGENT, experimental_off));
}

#[test]
fn turning_it_off_lists_it_as_off_for_the_phone() {
    assert!(!built_in_extensions::on_with(
        VOICE_AGENT,
        saved(Some(true))
    ));
    assert!(!built_in_extensions::enabled_features_with(saved(Some(true))).contains(&VOICE_AGENT));
    assert!(built_in_extensions::disabled_features_with(saved(Some(true))).contains(&VOICE_AGENT));
}

#[test]
fn turning_it_off_hides_its_hotkeys_but_not_dictate() {
    for hotkey in ["toggleVoiceAgent", "voiceCallAboutSession", "holdToTalk"] {
        assert_eq!(
            built_in_extensions::feature_owning_hotkey(hotkey),
            Some(VOICE_AGENT)
        );
        assert!(built_in_extensions::hotkey_shown_with(hotkey, saved(None)));
        assert!(!built_in_extensions::hotkey_shown_with(
            hotkey,
            saved(Some(true))
        ));
    }
    assert!(built_in_extensions::hotkey_shown_with(
        "toggleDictation",
        saved(Some(true))
    ));
}

#[test]
fn turning_it_off_keeps_the_page_with_dictates_keys_and_history() {
    assert_eq!(built_in_extensions::feature_owning_page("voiceAgent"), None);
    assert!(built_in_extensions::page_shown_with(
        "voiceAgent",
        saved(Some(true))
    ));
    let hidden = built_in_extensions::hidden_setting_keys_with(saved(Some(true)));
    for call_row in [
        "voiceAgentProvider",
        "voiceAgentXaiVoice",
        "voiceAgentOpenaiVoice",
        "voiceAgentSpeed",
        "voiceAgentCallMode",
        "voiceAgentMemory",
    ] {
        assert!(hidden.contains(&call_row), "{call_row} hides");
    }
    // Dictate uses the xAI key, or the OpenAI key when no xAI key is saved.
    assert!(!hidden.contains(&"voiceAgentXaiKey"));
    assert!(!hidden.contains(&"voiceAgentOpenaiKey"));
    assert!(!hidden.contains(&"voiceAgentDictationHistory"));
    let shown = built_in_extensions::hidden_setting_keys_with(saved(None));
    assert!(!shown.contains(&"voiceAgentProvider"));
}
