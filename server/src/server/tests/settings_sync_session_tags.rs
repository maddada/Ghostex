//! The Session Tags list through an older app's write: a phone or desktop that predates the status
//! entries writes the list back without them, and the computer that held them puts them back where
//! they were as its own newer edit (settings_sync/sidebar_tags.rs `keep_status_entries_merged`).
//! Both exchanges go through the real `settingsSyncExchange` route.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

const LIST_KEY: &str = "spaces/setting/sidebarSessionTagListItems";

fn status(id: &str) -> Value {
    json!({ "id": id, "type": "status", "enabled": true, "visible": true })
}

fn tag(id: &str) -> Value {
    json!({ "id": id, "type": "tag", "tag": id, "enabled": true, "visible": true })
}

fn untagged() -> Value {
    json!({ "id": "untagged", "type": "untagged", "enabled": true, "visible": true })
}

fn computer(temp: &tempfile::TempDir, server_id: &str, settings: Value) -> Arc<AppState> {
    let state = test_app_state_for(
        get_gxserver_paths(Some(temp.path().to_path_buf())),
        server_id,
    );
    fs::create_dir_all(&state.paths.app_config_dir).expect("config dir");
    fs::write(
        state
            .paths
            .app_config_dir
            .join("native-sidebar-settings.json"),
        settings.to_string(),
    )
    .expect("settings file");
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

fn sent_list(answer: &Value) -> Value {
    answer["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["key"] == json!(LIST_KEY))
        .cloned()
        .expect("the Session Tags list goes out")
}

#[tokio::test]
async fn settings_sync_puts_back_the_status_entries_an_older_writer_dropped() {
    let temp = tempfile::tempdir().expect("tempdir");
    // Arranged here: Working first, the other status entries after a hand tag.
    let held = json!([
        status("status-working"),
        tag("bug"),
        status("status-needs-you"),
        status("status-question"),
        status("status-error"),
        status("status-finished"),
        status("status-idle"),
        tag("todo"),
        untagged(),
    ]);
    let laptop = computer(
        &temp,
        "S7a",
        json!({ "syncSpaces": true, "syncAppearance": false, "sidebarSessionTagListItems": held }),
    );
    let first = exchange(&laptop, "S8b", json!([])).await;
    assert_eq!(sent_list(&first)["value"], held);

    // The older app on the other computer drops the status entries and writes back what is left,
    // as an edit newer than anything the laptop holds.
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    let stripped = json!([tag("bug"), tag("todo"), untagged()]);
    let answer = exchange(
        &laptop,
        "S8b",
        json!([{
            "key": LIST_KEY,
            "value": stripped,
            "hlc": format!("{:013}-{:06}-S8b", now_ms + 1_000, 0),
            "origin": "S8b",
        }]),
    )
    .await;
    let repaired = sent_list(&answer);
    assert_eq!(repaired["value"], held, "{answer}");
    assert_eq!(repaired["origin"], json!("S7a"), "{answer}");
}
