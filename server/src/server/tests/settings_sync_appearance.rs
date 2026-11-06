//! Appearance sync carries the custom colours' depths, which have no row of their own in Settings
//! but which Colourfulness and the theme presets save (gx-core `settings_sync/settings_keys.rs`).
//! Both directions go through the real `settingsSyncExchange` route: what a computer answers is its
//! export, and what it takes in reaches the app as an `updateSettingsPatch`.

use std::fs;

use super::*;
use tokio::time::{timeout, Duration};

const DARK_DEPTH: &str = "customSidebarTitlebarBackgroundDarknessPercent";
const LIGHT_DEPTH: &str = "customSidebarTitlebarLightBackgroundLightnessPercent";

fn write_settings(state: &AppState, settings: Value) {
    fs::create_dir_all(&state.paths.app_config_dir).expect("config dir");
    fs::write(
        state
            .paths
            .app_config_dir
            .join("native-sidebar-settings.json"),
        settings.to_string(),
    )
    .expect("settings file");
}

fn computer(temp: &tempfile::TempDir, server_id: &str, settings: Value) -> Arc<AppState> {
    let state = test_app_state_for(
        get_gxserver_paths(Some(temp.path().to_path_buf())),
        server_id,
    );
    write_settings(&state, settings);
    state
}

/// `caller` sends `entries` to `state`, which answers with all of its own.
async fn exchange(state: &Arc<AppState>, caller: &str, entries: Value) -> Value {
    let routed = route_http(
        state.clone(),
        rpc_request(
            crate::settings_sync::EXCHANGE_ENDPOINT,
            &state.auth_token,
            json!({ "serverId": caller, "name": caller, "entries": entries }),
        ),
        "exchange".to_string(),
    )
    .await;
    assert_eq!(routed.response.status(), StatusCode::OK);
    response_json(routed.response).await["result"].clone()
}

fn sent(answer: &Value, setting: &str) -> Option<Value> {
    answer["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["key"] == json!(format!("appearance/setting/{setting}")))
        .map(|entry| entry["value"].clone())
}

#[tokio::test]
async fn a_presets_custom_colour_depths_go_out_with_appearance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let studio = computer(
        &temp,
        "S7a",
        json!({
            "syncSpaces": false,
            "syncAppearance": true,
            "darkThemePreset": "custom",
            "customSidebarTitlebarBackgroundTintColor": "#1c1c2c",
            "themeSidebarContrast": -24,
            "themeWorkAreaContrast": -24,
            DARK_DEPTH: 85,
            LIGHT_DEPTH: 84,
        }),
    );
    let answer = exchange(&studio, "S8b", json!([])).await;
    assert_eq!(sent(&answer, DARK_DEPTH), Some(json!(85)), "{answer}");
    assert_eq!(sent(&answer, LIGHT_DEPTH), Some(json!(84)), "{answer}");
}

#[tokio::test]
async fn a_preset_picked_on_one_computer_saves_its_depths_on_the_other() {
    let studio_home = tempfile::tempdir().expect("tempdir");
    let laptop_home = tempfile::tempdir().expect("tempdir");
    let synced = json!({ "syncSpaces": false, "syncAppearance": true });
    let studio = computer(&studio_home, "S7a", synced.clone());
    // The first exchange takes the studio's values in; picking the preset afterwards is an edit.
    exchange(&studio, "S8b", json!([])).await;
    let mut preset = synced.clone();
    preset[DARK_DEPTH] = json!(85);
    preset[LIGHT_DEPTH] = json!(84);
    write_settings(&studio, preset);
    let studio_entries = exchange(&studio, "S8b", json!([])).await["entries"].clone();

    let mut laptop_settings = synced;
    laptop_settings[DARK_DEPTH] = json!(90);
    laptop_settings[LIGHT_DEPTH] = json!(90);
    let laptop = computer(&laptop_home, "S8b", laptop_settings);
    let (renderer_tx, mut renderer_rx) = laptop.event_hub.client_channel();
    laptop
        .event_hub
        .register_renderer_client("renderer-sync-test", renderer_tx)
        .await;
    exchange(&laptop, "S7a", studio_entries).await;

    let event = timeout(Duration::from_secs(5), renderer_rx.recv())
        .await
        .expect("the laptop's app is asked to save")
        .expect("renderer event");
    assert_eq!(event["command"]["action"], json!("updateSettingsPatch"));
    assert_eq!(event["command"]["payload"]["source"], json!("settingsSync"));
    assert_eq!(
        event["command"]["payload"]["patch"],
        json!({ DARK_DEPTH: 85, LIGHT_DEPTH: 84 })
    );
}
