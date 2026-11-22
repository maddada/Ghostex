use super::*;
use rusqlite::OptionalExtension;
use serde_json::{json, Value};

#[test]
fn add_project_path_repairs_visibility_metadata_for_existing_path() {
    /*
    CDXC:Projects 2026-06-30-21:23:
    Remote Attach carrier registration may reuse an existing gxserver path row. Repair hidden/system project metadata on `/api/addProjectPath` so every inventory client hides that carrier through daemon-owned state instead of macOS-only project filters.
    */
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let carrier_path = temp.path().join("remote-attach-carriers");
    std::fs::create_dir_all(&carrier_path).expect("carrier dir");

    let initial = repository
        .add_project_path(
            json!({
                "name": "Remote Attach",
                "path": path_str(&carrier_path),
            })
            .as_object()
            .expect("initial project params"),
        )
        .expect("initial project");
    let project_id = value_str(&initial, "projectId").to_string();
    assert_eq!(
        initial.get("visibility").and_then(Value::as_str),
        Some("visible")
    );
    assert!(initial.get("systemKind").is_none());

    let repaired = repository
        .add_project_path(
            json!({
                "name": "Remote Attach",
                "path": path_str(&carrier_path),
                "systemKind": "remoteAttachCarrier",
                "visibility": "hidden",
            })
            .as_object()
            .expect("repair project params"),
        )
        .expect("repaired project");
    assert_eq!(value_str(&repaired, "projectId"), project_id);
    assert_eq!(
        repaired.get("visibility").and_then(Value::as_str),
        Some("hidden")
    );
    assert_eq!(
        repaired.get("systemKind").and_then(Value::as_str),
        Some("remoteAttachCarrier")
    );

    let invalid = repository
        .add_project_path(
            json!({
                "name": "Remote Attach",
                "path": path_str(&carrier_path),
                "visibility": "archived",
            })
            .as_object()
            .expect("invalid project params"),
        )
        .expect_err("invalid visibility rejected");
    assert_eq!(invalid.code, "badRequest");
}

#[test]
fn records_project_and_session_id_allocations() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Allocated", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();
    let session = repository
        .create_session(
            json!({
                "projectId": project_id,
                "title": "Allocated session",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("session created");
    let session_id = value_str(&session, "sessionId").to_string();

    let project_allocation: Option<String> = db
        .query_row(
            "SELECT id FROM id_allocations WHERE kind = 'project' AND parentId = '' AND id = ?1",
            [&project_id],
            |row| row.get(0),
        )
        .optional()
        .expect("project allocation query");
    assert_eq!(project_allocation, Some(project_id.clone()));

    let session_allocation: Option<String> = db
        .query_row(
            "SELECT id FROM id_allocations WHERE kind = 'session' AND parentId = ?1 AND id = ?2",
            rusqlite::params![project_id, session_id],
            |row| row.get(0),
        )
        .optional()
        .expect("session allocation query");
    assert_eq!(session_allocation, Some(session_id));
}

#[test]
fn rejects_domain_json_deeper_than_contract_limit() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let mut nested = json!("leaf");
    for _ in 0..12 {
        nested = json!({ "child": nested });
    }
    let params = json!({
        "name": "Deep JSON",
        "runtimeSettings": nested,
    });
    let error = repository
        .create_project(params.as_object().expect("params object"))
        .expect_err("deep JSON rejected");
    assert_eq!(error.code, "badRequest");
    assert!(error.message.contains("depth limit"));
}

#[test]
fn domain_json_size_limit_counts_utf16_code_units_like_typescript() {
    let value = json!({ "emoji": "👻".repeat(260_000) });
    assert!(stringify_domain_json_field("runtimeSettings", &value).is_ok());
}

#[test]
fn maps_corrupt_project_json_to_corrupt_state() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let params = json!({ "name": "Corrupt JSON" });
    let project = repository
        .create_project(params.as_object().expect("params object"))
        .expect("project created");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id");
    db.execute(
        "UPDATE projects SET runtimeSettingsJson = ?1 WHERE projectId = ?2",
        rusqlite::params!["{not-json", project_id],
    )
    .expect("corrupt project row");
    let error = repository
        .list_projects()
        .expect_err("corrupt row rejected");
    assert_eq!(error.code, "corruptState");
    assert!(error.message.contains("runtimeSettingsJson"));
}

#[test]
fn title_runtime_settings_only_default_search_by_text_to_placeholder() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Title Defaults", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId");

    let terminal = repository
        .create_session(
            json!({
                "projectId": project_id,
                "title": "Terminal Session",
            })
            .as_object()
            .expect("terminal session params"),
            false,
        )
        .expect("terminal session created");
    let terminal_runtime = object_field(&terminal, "runtimeSettings");
    assert_eq!(terminal_runtime.get("titleSource"), None);

    let search = repository
        .create_session(
            json!({
                "projectId": project_id,
                "runtimeSettings": { "titleSource": null },
                "title": "Search   by\nText",
            })
            .as_object()
            .expect("search session params"),
            false,
        )
        .expect("search session created");
    let search_runtime = object_field(&search, "runtimeSettings");
    assert_eq!(
        search_runtime.get("titleSource"),
        Some(&json!("placeholder"))
    );
}

#[test]
fn invalid_surface_values_do_not_create_persisted_launch_surface_defaults() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Surface Defaults", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId");

    let explicit_invalid = repository
        .create_session(
            json!({
                "projectId": project_id,
                "surface": "invalid",
                "title": "Invalid Explicit Surface",
            })
            .as_object()
            .expect("explicit invalid params"),
            false,
        )
        .expect("explicit invalid session created");
    assert_eq!(explicit_invalid.get("surface"), Some(&json!("workspace")));
    assert_eq!(
        object_field(&explicit_invalid, "launchSettings").get("surface"),
        None
    );

    let launch_invalid = repository
        .create_session(
            json!({
                "launchSettings": { "surface": "invalid" },
                "projectId": project_id,
                "title": "Invalid Launch Surface",
            })
            .as_object()
            .expect("launch invalid params"),
            false,
        )
        .expect("launch invalid session created");
    assert_eq!(launch_invalid.get("surface"), Some(&json!("workspace")));
    assert_eq!(
        object_field(&launch_invalid, "launchSettings").get("surface"),
        Some(&json!("invalid"))
    );

    let updated = repository
        .update_session(
            json!({
                "launchSettings": { "surface": "invalid" },
                "projectId": project_id,
                "sessionId": value_str(&explicit_invalid, "sessionId"),
                "surface": "invalid",
            })
            .as_object()
            .expect("update invalid params"),
        )
        .expect("invalid surface update");
    assert_eq!(updated.get("surface"), Some(&json!("workspace")));
    assert_eq!(
        object_field(&updated, "launchSettings").get("surface"),
        Some(&json!("invalid"))
    );
}
