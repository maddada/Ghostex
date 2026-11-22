use super::*;
use std::fs;

#[tokio::test]
async fn browse_project_directories_route_filters_directory_entries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("picker-parent");
    fs::create_dir_all(parent.join("alpha")).expect("alpha");
    fs::create_dir_all(parent.join("alpine")).expect("alpine");
    fs::create_dir_all(parent.join("beta")).expect("beta");
    fs::create_dir_all(parent.join(".hidden")).expect("hidden");
    fs::write(parent.join("alphabet.txt"), "not a directory\n").expect("file");
    let parent_path = path_to_string(&parent);

    let filtered = route_http(
        state.clone(),
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({
                "params": {
                    "limit": 5,
                    "partialPath": format!("{parent_path}/al")
                }
            }),
        ),
        "request-browse-filtered".to_string(),
    )
    .await;
    assert_eq!(filtered.response.status(), StatusCode::OK);
    let body = response_json(filtered.response).await;
    assert_eq!(body["result"]["parentPath"], json!(parent_path));
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["alpha", "alpine"]);

    let hidden = route_http(
        state.clone(),
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({
                "params": {
                    "partialPath": format!("{parent_path}/.h")
                }
            }),
        ),
        "request-browse-hidden".to_string(),
    )
    .await;
    assert_eq!(hidden.response.status(), StatusCode::OK);
    let body = response_json(hidden.response).await;
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec![".hidden"]);

    let relative = route_http(
        state,
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({
                "params": {
                    "cwd": parent_path,
                    "partialPath": "./a"
                }
            }),
        ),
        "request-browse-relative".to_string(),
    )
    .await;
    assert_eq!(relative.response.status(), StatusCode::OK);
    let body = response_json(relative.response).await;
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["alpha", "alpine"]);
}

#[tokio::test]
async fn browse_project_directories_sorts_case_insensitively_and_swallows_permission_errors() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths.clone());
    let token = state.auth_token.clone();
    let parent = paths.root_dir.join("browse-order");
    for name in ["Zebra", "apple", "Banana", "cherry"] {
        fs::create_dir_all(parent.join(name)).expect("dir");
    }
    let parent_path = path_to_string(&parent);

    let sorted = route_http(
        state.clone(),
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({ "params": { "partialPath": format!("{parent_path}/") } }),
        ),
        "request-browse-order".to_string(),
    )
    .await;
    assert_eq!(sorted.response.status(), StatusCode::OK);
    let body = response_json(sorted.response).await;
    let names = body["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["apple", "Banana", "cherry", "Zebra"]);
    assert!(body["result"]["entries"][0]
        .as_object()
        .unwrap()
        .get("sortKey")
        .is_none());

    let unreadable = paths.root_dir.join("browse-unreadable");
    fs::create_dir_all(unreadable.join("child")).expect("child");
    let mut permissions = fs::metadata(&unreadable).expect("metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o000);
    }
    fs::set_permissions(&unreadable, permissions).expect("chmod");
    let denied = route_http(
        state,
        rpc_request(
            "/api/browseProjectDirectories",
            &token,
            json!({ "params": { "partialPath": format!("{}/", path_to_string(&unreadable)) } }),
        ),
        "request-browse-denied".to_string(),
    )
    .await;
    let denied_status = denied.response.status();
    let denied_body = response_json(denied.response).await;
    let mut restored = fs::metadata(&unreadable).expect("metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        restored.set_mode(0o755);
    }
    fs::set_permissions(&unreadable, restored).expect("chmod restore");
    assert_eq!(denied_status, StatusCode::OK);
    assert_eq!(denied_body["result"]["entries"], json!([]));
    assert_eq!(
        denied_body["result"]["parentPath"],
        json!(path_to_string(&unreadable))
    );
}

#[tokio::test]
async fn discover_source_control_reports_every_provider_with_a_hint() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths);
    let token = state.auth_token.clone();

    let response = route_http(
        state,
        rpc_request(
            "/api/discoverSourceControl",
            &token,
            json!({ "params": {} }),
        ),
        "request-discover-source-control".to_string(),
    )
    .await;
    assert_eq!(response.response.status(), StatusCode::OK);
    let body = response_json(response.response).await;
    let providers = body["result"]["discovery"]["providers"]
        .as_array()
        .expect("providers")
        .clone();
    let names = providers
        .iter()
        .map(|entry| entry["provider"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["github", "gitlab", "bitbucket", "azure-devops"]);
    for provider in &providers {
        assert!(provider["installHint"]
            .as_str()
            .is_some_and(|hint| !hint.is_empty()));
        assert!(provider["label"]
            .as_str()
            .is_some_and(|label| !label.is_empty()));
        assert!(provider["auth"]["status"].as_str().is_some());
        assert!(matches!(
            provider["status"].as_str(),
            Some("available") | Some("missing") | Some("unsupported")
        ));
    }
    for provider in providers.iter().filter(|entry| {
        matches!(
            entry["provider"].as_str(),
            Some("bitbucket") | Some("azure-devops")
        )
    }) {
        assert_eq!(provider["status"], json!("unsupported"));
    }
    assert!(body["result"]["discovery"]["checkedAt"]
        .as_str()
        .is_some_and(|value| value.ends_with('Z')));
}

#[tokio::test]
async fn lookup_repository_rejects_unsupported_providers_and_blank_repositories() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let state = test_app_state(paths);
    let token = state.auth_token.clone();

    let unsupported = route_http(
        state.clone(),
        rpc_request(
            "/api/lookupRepository",
            &token,
            json!({ "params": { "provider": "bitbucket", "repository": "team/app" } }),
        ),
        "request-lookup-unsupported".to_string(),
    )
    .await;
    assert_eq!(unsupported.response.status(), StatusCode::BAD_REQUEST);

    let blank = route_http(
        state,
        rpc_request(
            "/api/lookupRepository",
            &token,
            json!({ "params": { "provider": "github", "repository": "  " } }),
        ),
        "request-lookup-blank".to_string(),
    )
    .await;
    assert_eq!(blank.response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(blank.response).await;
    assert_eq!(body["error"], json!("badRequest"));
    assert_eq!(
        body["message"],
        json!("repository must be a non-empty string.")
    );
}
