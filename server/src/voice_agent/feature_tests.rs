//! The voice agent's switch on Settings > Extensions (`feature.rs`): read fresh from the settings
//! file, and a call that is open when it is turned off ends with a plain message. Runs the real
//! `Conversation` against the fake provider of `focus_tests.rs`.

use std::time::Duration;

use super::conversation::Input;
use super::focus_tests::{test_state, Call};
use super::{feature, registry};

fn write_settings(state: &crate::server::AppState, settings: serde_json::Value) {
    let dir = &state.paths.app_config_dir;
    std::fs::create_dir_all(dir).expect("config dir");
    std::fs::write(
        dir.join("native-sidebar-settings.json"),
        settings.to_string(),
    )
    .expect("settings file");
}

#[test]
fn the_switch_is_read_fresh_from_the_settings_file() {
    let temp = tempfile::tempdir().expect("temp");
    let state = test_state(&temp);
    assert!(
        feature::voice_agent_on(&state.paths),
        "on with no settings file"
    );
    write_settings(&state, serde_json::json!({"voiceAgentHidden": true}));
    assert!(!feature::voice_agent_on(&state.paths), "off once hidden");
    write_settings(&state, serde_json::json!({"voiceAgentHidden": false}));
    assert!(
        feature::voice_agent_on(&state.paths),
        "back on without a restart"
    );
}

#[tokio::test]
async fn turning_it_off_ends_an_open_call_with_a_plain_message() {
    let mut call = Call::start(None, false).await;
    call.client_frame("ready").await;
    assert!(registry::sender(&call.id).is_some());
    call.tx.send(Input::TurnedOff).expect("turn off");
    let ended = call.client_frame("ended").await;
    assert_eq!(ended["reason"], "turnedOff");
    let message = ended["message"].as_str().unwrap_or_default();
    assert!(
        message.starts_with("Voice agent is turned off."),
        "{message}"
    );
    assert!(message.contains("ghostex settings set voiceAgentHidden false"));
    // The call left the registry, so the watcher stops and a reattach finds nothing.
    tokio::time::timeout(Duration::from_secs(5), async {
        while registry::sender(&call.id).is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the call left the registry");
    let _ = &call.state;
}
