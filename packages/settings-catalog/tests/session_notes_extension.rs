use ghostex_settings_catalog::{built_in_extensions, default_value, J};

use built_in_extensions::SESSION_NOTES;

fn saved(session_notes_hidden: Option<bool>) -> impl Fn(&str) -> Option<bool> {
    move |key| match key {
        "sessionNotesHidden" => session_notes_hidden,
        _ => None,
    }
}

#[test]
fn session_notes_is_a_built_in_feature_off_by_default() {
    assert_eq!(
        built_in_extensions::switch_key(SESSION_NOTES),
        Some(("sessionNotesHidden", false))
    );
    assert_eq!(default_value("sessionNotesHidden"), Some(&J::Bool(true)));
    assert_eq!(built_in_extensions::title(SESSION_NOTES), "Session notes");
    assert_eq!(built_in_extensions::gate_key(SESSION_NOTES), None);
    // A settings file from before the switch has no `sessionNotesHidden`: off for existing users too.
    assert!(!built_in_extensions::on_with(SESSION_NOTES, saved(None)));
    assert!(!built_in_extensions::enabled_in_value(
        Some(&serde_json::json!({})),
        SESSION_NOTES
    ));
}

#[test]
fn the_phone_hears_it_off_until_it_is_turned_on() {
    assert!(built_in_extensions::disabled_features_with(saved(None)).contains(&SESSION_NOTES));
    assert!(!built_in_extensions::enabled_features_with(saved(None)).contains(&SESSION_NOTES));
    assert!(
        built_in_extensions::enabled_features_with(saved(Some(false))).contains(&SESSION_NOTES)
    );
    assert!(
        !built_in_extensions::disabled_features_with(saved(Some(false))).contains(&SESSION_NOTES)
    );
}

#[test]
fn its_hotkey_shows_and_fires_only_while_it_is_on() {
    assert_eq!(
        built_in_extensions::feature_owning_hotkey("sessionNote"),
        Some(SESSION_NOTES)
    );
    assert!(!built_in_extensions::hotkey_shown_with(
        "sessionNote",
        saved(None)
    ));
    assert!(!built_in_extensions::hotkey_shown_with(
        "sessionNote",
        saved(Some(true))
    ));
    assert!(built_in_extensions::hotkey_shown_with(
        "sessionNote",
        saved(Some(false))
    ));
    // The terminal bar's other rows are not part of it.
    assert!(built_in_extensions::hotkey_shown_with(
        "stashPrompt",
        saved(None)
    ));
}

#[test]
fn it_owns_no_settings_page() {
    assert_eq!(
        built_in_extensions::feature_owning_page("sessionNotes"),
        None
    );
    assert!(built_in_extensions::available_on(
        SESSION_NOTES,
        ghostex_settings_catalog::Platform::current()
    ));
}
