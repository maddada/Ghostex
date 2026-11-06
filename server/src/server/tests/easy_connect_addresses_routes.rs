//! `/api/easyConnectAddresses`: what a paired phone is told about this computer's addresses.

use super::*;

fn temp_state() -> (tempfile::TempDir, Arc<AppState>) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    std::fs::create_dir_all(paths.config_file.parent().expect("config directory"))
        .expect("create config directory");
    (temp, test_app_state(paths))
}

async fn ask(state: &Arc<AppState>, token: &str) -> (StatusCode, Value) {
    let routed = route_http(
        state.clone(),
        rpc_request("/api/easyConnectAddresses", token, json!({ "params": {} })),
        "addresses".to_string(),
    )
    .await;
    let status = routed.response.status();
    (status, response_json(routed.response).await)
}

#[tokio::test]
async fn a_paired_phone_reads_the_addresses_with_its_own_token() {
    let (_temp, state) = temp_state();
    let device_token = crate::auth::mint_device_api_token();
    state
        .device_api_tokens
        .replace([crate::auth::hash_device_api_token(&device_token)]);

    let (status, body) = ask(&state, &device_token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Easy Connect is off in a fresh state, so there is nothing to dial yet.
    assert_eq!(body["result"]["addresses"], json!([]));

    let (status, _) = ask(&state, &state.auth_token.clone()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_caller_without_a_token_is_refused() {
    let (_temp, state) = temp_state();
    let (status, _) = ask(&state, "gxdev_not-a-real-token").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
