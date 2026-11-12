//! `ghostex settings set` on a computer without the desktop app (issue #475).

use super::*;

async fn set(state: &Arc<AppState>, patch: Value) -> (StatusCode, Value) {
    let routed = route_http(
        state.clone(),
        rpc_request(
            "/api/dispatchRendererCommand",
            &state.auth_token.clone(),
            json!({ "params": {
                "action": "updateSettingsPatch",
                "payload": { "patch": patch, "source": "ghostexCli" },
            }}),
        ),
        "settings".to_string(),
    )
    .await;
    let status = routed.response.status();
    (status, response_json(routed.response).await)
}

#[tokio::test]
async fn a_settings_save_with_no_app_is_gxservers_where_no_app_ever_saved_settings() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let (status, body) = set(&state, json!({ "useAgentHooks": true })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["savedBy"], "gxserver");
    assert_eq!(
        crate::shared_settings_file::read_shared_settings(&paths).expect("settings")
            ["useAgentHooks"],
        true
    );

    // The desktop app saves the file (its first start): from then on it is the one writer, and a
    // save while it is closed fails as before.
    std::fs::write(
        paths.app_config_dir.join("native-sidebar-settings.json"),
        r#"{"useAgentHooks":true,"theme":"dark"}"#,
    )
    .expect("app save");
    let (status, body) = set(&state, json!({ "useAgentHooks": false })).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"], "dependencyUnavailable", "{body}");
}
