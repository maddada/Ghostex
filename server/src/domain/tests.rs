
use super::*;
use crate::{
    ids::is_gxserver_session_id,
    paths::get_gxserver_paths,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

fn open_test_database() -> (tempfile::TempDir, Connection) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    (temp, db)
}

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_git_for_test(cwd: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("test path is valid UTF-8")
}

fn value_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{key} string missing"))
}

fn object_field<'a>(value: &'a Value, key: &str) -> &'a Map<String, Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{key} object missing"))
}

fn number_field(value: &Value, key: &str) -> f64 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("{key} number missing"))
}

#[test]
fn optional_project_id_rejects_whitespace_filter_ids() {
    let mut params = Map::new();
    assert_eq!(read_optional_project_id(&params).expect("missing id"), None);

    params.insert("projectId".to_string(), json!(""));
    assert_eq!(read_optional_project_id(&params).expect("empty id"), None);

    params.insert("projectId".to_string(), json!("   "));
    let error = read_optional_project_id(&params).expect_err("whitespace id rejected");
    assert_eq!(error.code, "badRequest");
    assert_eq!(error.message, "Invalid gxserver project ID:    .");

    params.insert("projectId".to_string(), json!("P3a91"));
    assert_eq!(
        read_optional_project_id(&params).expect("valid id"),
        Some("P3a91".to_string())
    );
}

#[test]
fn restored_session_ids_reject_invalid_provided_values() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Restore IDs" })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();

    for restored in [json!("   "), json!(42), json!({ "sessionId": "G8v20" })] {
        let error = repository
            .create_session(
                json!({
                    "projectId": project_id,
                    "restoredFromSessionId": restored,
                    "title": "Invalid restore",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect_err("invalid restore id rejected");
        assert_eq!(error.code, "badRequest");
        assert!(error.message.starts_with("Invalid restoredFromSessionId: "));
    }

    let source = repository
        .create_session(
            json!({
                "projectId": project_id,
                "title": "Source",
            })
            .as_object()
            .expect("source params"),
            false,
        )
        .expect("source session created");
    let source_session_id = value_str(&source, "sessionId").to_string();
    let restored = repository
        .create_session(
            json!({
                "projectId": project_id,
                "restoredFromSessionId": source_session_id,
                "title": "Restored",
            })
            .as_object()
            .expect("restored params"),
            false,
        )
        .expect("restored session created");
    assert_eq!(
        object_field(&restored, "hiddenMetadata")
            .get("restoredFromSessionId")
            .and_then(Value::as_str),
        Some(source_session_id.as_str())
    );

    let restored_session_id = value_str(&restored, "sessionId").to_string();
    let cleared = repository
        .update_session(
            json!({
                "projectId": project_id,
                "restoredFromSessionId": "",
                "sessionId": restored_session_id,
            })
            .as_object()
            .expect("clear params"),
        )
        .expect("restore id cleared");
    assert!(object_field(&cleared, "hiddenMetadata")
        .get("restoredFromSessionId")
        .is_none());

    let error = repository
        .update_session(
            json!({
                "projectId": project_id,
                "restoredFromSessionId": "   ",
                "sessionId": restored_session_id,
            })
            .as_object()
            .expect("invalid update params"),
        )
        .expect_err("invalid update restore id rejected");
    assert_eq!(error.code, "badRequest");
    assert_eq!(error.message, "Invalid restoredFromSessionId:    .");
}

#[test]
fn session_provider_state_carries_canonical_zmx_provider() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Provider Metadata" })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();
    let session = repository
        .create_session(
            json!({
                "projectId": project_id.as_str(),
                "providerState": { "lifecycleState": "exists", "provider": "tmux" },
                "title": "Remote provider metadata",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("session created");
    let session_id = value_str(&session, "sessionId").to_string();
    let zmx_name = format!("S7k-{project_id}-{session_id}");
    let provider_state = object_field(&session, "providerState");
    assert_eq!(provider_state.get("provider"), Some(&json!("zmx")));
    assert_eq!(provider_state.get("zmxName"), Some(&json!(zmx_name)));
    assert_eq!(provider_state.get("lifecycleState"), Some(&json!("exists")));

    let updated = repository
        .update_session(
            json!({
                "projectId": project_id.as_str(),
                "providerState": { "lifecycleState": "missing" },
                "sessionId": session_id.as_str(),
            })
            .as_object()
            .expect("update params"),
        )
        .expect("session updated");
    let updated_provider_state = object_field(&updated, "providerState");
    assert_eq!(updated_provider_state.get("provider"), Some(&json!("zmx")));
    assert_eq!(
        updated_provider_state.get("zmxName"),
        Some(&json!(zmx_name))
    );
    assert_eq!(
        updated_provider_state.get("lifecycleState"),
        Some(&json!("missing"))
    );

    let reloaded = repository
        .get_session(&project_id, &session_id)
        .expect("session reloaded")
        .expect("session exists");
    let reloaded_provider_state = object_field(&reloaded, "providerState");
    assert_eq!(reloaded_provider_state.get("provider"), Some(&json!("zmx")));
    assert_eq!(
        reloaded_provider_state.get("zmxName"),
        Some(&json!(zmx_name))
    );
    assert_eq!(
        reloaded_provider_state.get("lifecycleState"),
        Some(&json!("missing"))
    );
}

#[test]
fn session_tags_normalize_persist_and_clear_like_typescript() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Session Tags" })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();
    let supported_tags = [
        "favorite",
        "high-priority",
        "research",
        "todo",
        "in-progress",
        "testing",
        "blocked",
        "low-priority",
        "on-hold",
        "done",
        "bug",
        "feature",
        "design",
    ];

    for tag in supported_tags {
        let session = repository
            .create_session(
                json!({
                    "projectId": project_id.as_str(),
                    "sessionTag": tag,
                    "title": format!("Tagged {tag}"),
                })
                .as_object()
                .expect("tagged session params"),
                false,
            )
            .expect("tagged session created");
        assert_eq!(session.get("sessionTag"), Some(&json!(tag)));
        assert_eq!(
            session.get("isFavorite").and_then(Value::as_bool),
            Some(tag == "favorite")
        );

        let reloaded = repository
            .get_session(&project_id, value_str(&session, "sessionId"))
            .expect("tagged session reloaded")
            .expect("tagged session exists");
        assert_eq!(reloaded.get("sessionTag"), Some(&json!(tag)));
    }

    let legacy_favorite = repository
        .create_session(
            json!({
                "isFavorite": true,
                "projectId": project_id.as_str(),
                "title": "Legacy favorite",
            })
            .as_object()
            .expect("legacy favorite params"),
            false,
        )
        .expect("legacy favorite created");
    assert_eq!(
        legacy_favorite.get("isFavorite").and_then(Value::as_bool),
        Some(true)
    );
    assert!(legacy_favorite.get("sessionTag").is_none());

    let legacy_favorite_id = value_str(&legacy_favorite, "sessionId").to_string();
    let reloaded_legacy_favorite = repository
        .get_session(&project_id, &legacy_favorite_id)
        .expect("legacy favorite reloaded")
        .expect("legacy favorite exists");
    assert_eq!(
        reloaded_legacy_favorite.get("sessionTag"),
        Some(&json!("favorite"))
    );
    assert_eq!(
        reloaded_legacy_favorite
            .get("isFavorite")
            .and_then(Value::as_bool),
        Some(true)
    );

    let mutable = repository
        .create_session(
            json!({
                "projectId": project_id.as_str(),
                "title": "Mutable tag",
            })
            .as_object()
            .expect("mutable session params"),
            false,
        )
        .expect("mutable session created");
    let mutable_id = value_str(&mutable, "sessionId").to_string();

    let research = repository
        .update_session(
            json!({
                "projectId": project_id.as_str(),
                "sessionId": mutable_id.as_str(),
                "sessionTag": "research",
            })
            .as_object()
            .expect("research tag params"),
        )
        .expect("research tag update");
    assert_eq!(research.get("sessionTag"), Some(&json!("research")));
    assert_eq!(
        research.get("isFavorite").and_then(Value::as_bool),
        Some(false)
    );

    let favorite = repository
        .update_session(
            json!({
                "isFavorite": true,
                "projectId": project_id.as_str(),
                "sessionId": mutable_id.as_str(),
            })
            .as_object()
            .expect("favorite params"),
        )
        .expect("favorite update");
    assert_eq!(favorite.get("sessionTag"), Some(&json!("favorite")));
    assert_eq!(
        favorite.get("isFavorite").and_then(Value::as_bool),
        Some(true)
    );

    for cleared_value in [Value::Null, json!("")] {
        let cleared = repository
            .update_session(
                json!({
                    "projectId": project_id.as_str(),
                    "sessionId": mutable_id.as_str(),
                    "sessionTag": cleared_value,
                })
                .as_object()
                .expect("clear tag params"),
            )
            .expect("tag cleared");
        assert!(cleared.get("sessionTag").is_none());
        assert_eq!(
            cleared.get("isFavorite").and_then(Value::as_bool),
            Some(false)
        );
    }

    for invalid_tag in [json!("retired-type"), json!("   "), json!(42)] {
        let error = repository
            .create_session(
                json!({
                    "projectId": project_id.as_str(),
                    "sessionTag": invalid_tag,
                    "title": "Invalid tag",
                })
                .as_object()
                .expect("invalid tag params"),
                false,
            )
            .expect_err("invalid tag rejected");
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, "sessionTag must be a supported session tag.");
    }
}

#[test]
fn persisted_invalid_session_tag_is_rejected_on_hydration() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Invalid Stored Tags" })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();
    let session = repository
        .create_session(
            json!({
                "projectId": project_id.as_str(),
                "title": "Stored invalid tag",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("session created");
    let session_id = value_str(&session, "sessionId").to_string();

    db.execute_batch("PRAGMA ignore_check_constraints = ON;")
        .expect("disable tag check");
    db.execute(
        "UPDATE sessions SET sessionTag = ?3 WHERE projectId = ?1 AND sessionId = ?2",
        rusqlite::params![project_id.as_str(), session_id.as_str(), "retired-type"],
    )
    .expect("write invalid stored tag");
    db.execute_batch("PRAGMA ignore_check_constraints = OFF;")
        .expect("restore tag check");

    let error = repository
        .get_session(&project_id, &session_id)
        .expect_err("invalid stored tag rejected");
    assert_eq!(error.code, "badRequest");
    assert_eq!(error.message, "sessionTag must be a supported session tag.");
}

#[test]
fn update_session_order_defaults_returns_touched_rows_and_list_remains_updated_at_ordered() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Sidebar Order" })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();
    let first = repository
        .create_session(
            json!({ "projectId": project_id.as_str(), "title": "First" })
                .as_object()
                .expect("first params"),
            false,
        )
        .expect("first session");
    let second = repository
        .create_session(
            json!({ "projectId": project_id.as_str(), "title": "Second" })
                .as_object()
                .expect("second params"),
            false,
        )
        .expect("second session");
    let third = repository
        .create_session(
            json!({ "projectId": project_id.as_str(), "title": "Third" })
                .as_object()
                .expect("third params"),
            false,
        )
        .expect("third session");
    let first_id = value_str(&first, "sessionId").to_string();
    let second_id = value_str(&second, "sessionId").to_string();
    let third_id = value_str(&third, "sessionId").to_string();

    assert_eq!(number_field(&first, "sidebarOrder"), 0.0);
    assert_eq!(number_field(&second, "sidebarOrder"), 0.0);

    let ordered = repository
        .update_session_order(
            json!({
                "projectId": project_id.as_str(),
                "sessionIds": [second_id.as_str(), first_id.as_str()],
            })
            .as_object()
            .expect("order params"),
        )
        .expect("order updated");
    assert_eq!(ordered.len(), 2);
    assert_eq!(
        ordered
            .iter()
            .filter_map(|session| session.get("sessionId").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec![second_id.as_str(), first_id.as_str()]
    );
    assert_eq!(
        ordered
            .iter()
            .map(|session| number_field(session, "sidebarOrder"))
            .collect::<Vec<_>>(),
        vec![1000.0, 2000.0]
    );
    let untouched = repository
        .get_session(&project_id, &third_id)
        .expect("get untouched")
        .expect("untouched exists");
    assert_eq!(number_field(&untouched, "sidebarOrder"), 0.0);

    for (session_id, updated_at) in [
        (second_id.as_str(), "2026-06-02T12:00:00.000Z"),
        (first_id.as_str(), "2026-06-01T12:00:00.000Z"),
        (third_id.as_str(), "2026-05-31T12:00:00.000Z"),
    ] {
        db.execute(
            "UPDATE sessions SET updatedAt = ?3 WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![project_id, session_id, updated_at],
        )
        .expect("updatedAt patched");
    }
    let listed = repository
        .list_sessions(Some(&project_id))
        .expect("sessions listed");
    assert_eq!(
        listed
            .iter()
            .filter_map(|session| session.get("sessionId").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec![second_id.as_str(), first_id.as_str(), third_id.as_str()]
    );
}

#[test]
fn update_session_order_rejects_invalid_duplicate_and_unknown_ids_without_partial_writes() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "Sidebar Rollback" })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();
    let first = repository
        .create_session(
            json!({ "projectId": project_id.as_str(), "title": "First" })
                .as_object()
                .expect("first params"),
            false,
        )
        .expect("first session");
    let second = repository
        .create_session(
            json!({ "projectId": project_id.as_str(), "title": "Second" })
                .as_object()
                .expect("second params"),
            false,
        )
        .expect("second session");
    let first_id = value_str(&first, "sessionId").to_string();
    let second_id = value_str(&second, "sessionId").to_string();
    let original_updated_at = value_str(&first, "updatedAt").to_string();

    let empty_error = repository
        .update_session_order(
            json!({ "projectId": project_id.as_str(), "sessionIds": [] })
                .as_object()
                .expect("empty params"),
        )
        .expect_err("empty order rejected");
    assert_eq!(empty_error.code, "badRequest");
    assert_eq!(
        empty_error.message,
        "sessionIds must contain at least one session ID."
    );

    let invalid_error = repository
        .update_session_order(
            json!({ "projectId": project_id.as_str(), "sessionIds": ["session-local"] })
                .as_object()
                .expect("invalid params"),
        )
        .expect_err("invalid order rejected");
    assert_eq!(invalid_error.code, "badRequest");
    assert_eq!(invalid_error.message, "Invalid sessionId: session-local.");

    let duplicate_error = repository
            .update_session_order(
                json!({ "projectId": project_id.as_str(), "sessionIds": [first_id.as_str(), first_id.as_str()] })
                    .as_object()
                    .expect("duplicate params"),
            )
            .expect_err("duplicate order rejected");
    assert_eq!(duplicate_error.code, "badRequest");
    assert_eq!(
        duplicate_error.message,
        format!("Duplicate sessionId: {first_id}.")
    );

    let mut missing_id = "G9zzz".to_string();
    if missing_id == first_id || missing_id == second_id {
        missing_id = "G9zzy".to_string();
    }
    assert!(is_gxserver_session_id(&missing_id));
    let missing_error = repository
            .update_session_order(
                json!({ "projectId": project_id.as_str(), "sessionIds": [first_id.as_str(), missing_id.as_str()] })
                    .as_object()
                    .expect("missing params"),
            )
            .expect_err("missing order rejected");
    assert_eq!(missing_error.code, "notFound");
    assert_eq!(
        missing_error.message,
        format!("Session {project_id}/{missing_id} does not exist.")
    );
    let first_after = repository
        .get_session(&project_id, &first_id)
        .expect("get first after rollback")
        .expect("first exists after rollback");
    assert_eq!(number_field(&first_after, "sidebarOrder"), 0.0);
    assert_eq!(
        first_after.get("updatedAt").and_then(Value::as_str),
        Some(original_updated_at.as_str())
    );
}

#[test]
fn update_and_remove_crud_paths_report_not_found_for_unvalidated_ids() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({ "name": "CRUD IDs" })
                .as_object()
                .expect("project params"),
        )
        .expect("project created");
    let project_id = value_str(&project, "projectId").to_string();

    let missing_project = repository
        .update_project(
            json!({ "projectId": "project-local" })
                .as_object()
                .expect("update project params"),
        )
        .expect_err("invalid project lookup is not found");
    assert_eq!(missing_project.code, "notFound");
    assert_eq!(
        missing_project.message,
        "Project project-local does not exist."
    );

    let missing_session = repository
        .update_session(
            json!({ "projectId": project_id, "sessionId": "session-local" })
                .as_object()
                .expect("update session params"),
        )
        .expect_err("invalid session lookup is not found");
    assert_eq!(missing_session.code, "notFound");
    assert!(missing_session
        .message
        .contains("/session-local does not exist."));

    let missing_remove = repository
        .remove_session(json!({}).as_object().expect("remove session params"))
        .expect_err("missing session lookup is not found");
    assert_eq!(missing_remove.code, "notFound");
    assert_eq!(
        missing_remove.message,
        "Session undefined/undefined does not exist."
    );
}

#[test]
fn add_project_path_repairs_visibility_metadata_for_existing_path() {
    /*
    CDXC:ProjectVisibility 2026-06-30-21:23:
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
            json!({ "name": "Allocated" })
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
            json!({ "name": "Title Defaults" })
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
            json!({ "name": "Surface Defaults" })
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

#[test]
fn add_project_path_normalizes_nullish_fallback_and_deduplicates_path_syntax() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let parent = temp.path().join("paths");
    let repo_path = parent.join("repo");
    let fallback_path = parent.join("fallback");
    std::fs::create_dir_all(&repo_path).expect("repo dir");
    std::fs::create_dir_all(&fallback_path).expect("fallback dir");

    let first_input = repo_path.join("..").join("repo").join(".");
    let first = repository
        .add_project_path(
            json!({ "path": path_str(&first_input) })
                .as_object()
                .expect("first params"),
        )
        .expect("first add");
    assert_eq!(value_str(&first, "path"), path_str(&repo_path));
    assert_eq!(value_str(&first, "name"), "repo");

    let trailing_input = format!("{}/", path_str(&repo_path));
    let second = repository
        .add_project_path(
            json!({ "path": trailing_input })
                .as_object()
                .expect("second params"),
        )
        .expect("second add");
    assert_eq!(
        value_str(&second, "projectId"),
        value_str(&first, "projectId")
    );
    assert_eq!(repository.list_projects().expect("projects").len(), 1);

    let fallback = repository
        .add_project_path(
            json!({ "path": null, "projectPath": path_str(&fallback_path) })
                .as_object()
                .expect("fallback params"),
        )
        .expect("null path falls back to projectPath");
    assert_eq!(value_str(&fallback, "path"), path_str(&fallback_path));

    let empty_error = repository
        .add_project_path(
            json!({ "path": "", "projectPath": path_str(&repo_path) })
                .as_object()
                .expect("empty params"),
        )
        .expect_err("blank path does not fall back");
    assert_eq!(empty_error.code, "badRequest");
    assert_eq!(empty_error.message, "path must be a non-empty path.");
}

#[test]
fn add_project_path_creates_workspace_root_when_create_if_missing_is_requested() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let missing = temp.path().join("created-parent").join("created-project");

    let without_flag = repository
        .add_project_path(
            json!({ "path": path_str(&missing) })
                .as_object()
                .expect("params"),
        )
        .expect_err("missing path is rejected without the flag");
    assert_eq!(without_flag.code, "notFound");
    assert!(!missing.exists());

    let project = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&missing) })
                .as_object()
                .expect("params"),
        )
        .expect("missing path created and registered");
    assert_eq!(value_str(&project, "path"), path_str(&missing));
    assert_eq!(value_str(&project, "name"), "created-project");
    assert!(missing.is_dir());

    let repeated = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&missing) })
                .as_object()
                .expect("params"),
        )
        .expect("second add is idempotent");
    assert_eq!(
        value_str(&repeated, "projectId"),
        value_str(&project, "projectId")
    );

    let file_path = temp.path().join("create-if-missing-file");
    std::fs::write(&file_path, "file\n").expect("file");
    let file_error = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&file_path) })
                .as_object()
                .expect("params"),
        )
        .expect_err("existing file is still rejected");
    assert_eq!(file_error.code, "badRequest");
    assert_eq!(
        file_error.message,
        format!("path is not a directory: {}", path_str(&file_path))
    );

    let unwritable = file_path.join("child");
    let create_error = repository
        .add_project_path(
            json!({ "createIfMissing": true, "path": path_str(&unwritable) })
                .as_object()
                .expect("params"),
        )
        .expect_err("mkdir failure surfaces the workspace-root message");
    assert_eq!(create_error.code, "badRequest");
    assert_eq!(
        create_error.message,
        format!("Failed to create workspace root: {}", path_str(&unwritable))
    );
}

#[test]
fn add_project_path_rejects_invalid_path_inputs_with_typescript_messages() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let missing_input = temp
        .path()
        .join("missing-parent")
        .join("..")
        .join("missing");
    let missing_normalized = temp.path().join("missing");
    let file_path = temp.path().join("not-a-directory");
    std::fs::write(&file_path, "file\n").expect("file");

    let cases = vec![
        (
            json!({ "path": null }),
            "badRequest",
            "path must be a non-empty path.".to_string(),
        ),
        (
            json!({ "path": 42 }),
            "badRequest",
            "path must be a non-empty path.".to_string(),
        ),
        (
            json!({ "path": "   " }),
            "badRequest",
            "path must be a non-empty path.".to_string(),
        ),
        (
            json!({ "path": "relative/repo" }),
            "badRequest",
            "path must be an absolute path or start with ~/".to_string(),
        ),
        (
            json!({ "path": path_str(&missing_input) }),
            "notFound",
            format!("path does not exist: {}", path_str(&missing_normalized)),
        ),
        (
            json!({ "path": path_str(&file_path) }),
            "badRequest",
            format!("path is not a directory: {}", path_str(&file_path)),
        ),
    ];

    for (params, code, message) in cases {
        let error = repository
            .add_project_path(params.as_object().expect("params"))
            .expect_err("invalid add path rejected");
        assert_eq!(error.code, code);
        assert_eq!(error.message, message);
    }
}

#[test]
fn normalize_existing_directory_path_expands_home_shortcut() {
    let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    else {
        return;
    };
    let normalized = normalize_existing_directory_path(Some(&json!("~")), "path")
        .expect("home shortcut normalized");
    assert_eq!(normalized, path_to_string(&resolve_path_syntax(home)));
}

#[test]
fn create_session_project_resolution_uses_nullish_path_fallback() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let repo_path = temp.path().join("session-project");
    std::fs::create_dir_all(&repo_path).expect("repo dir");
    let project = repository
        .add_project_path(
            json!({ "path": path_str(&repo_path) })
                .as_object()
                .expect("project params"),
        )
        .expect("project added");
    let project_id = value_str(&project, "projectId").to_string();

    let cwd_input = repo_path.join("..").join("session-project").join(".");
    let session = repository
        .create_session(
            json!({
                "cwd": path_str(&cwd_input),
                "projectId": "project-local",
                "projectPath": null,
                "title": "Terminal",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("session created");
    assert_eq!(value_str(&session, "projectId"), project_id);
    assert_eq!(value_str(&session, "cwd"), path_str(&cwd_input));

    let empty_error = repository
        .create_session(
            json!({
                "cwd": path_str(&repo_path),
                "projectId": "project-local",
                "projectPath": "",
                "title": "Terminal",
            })
            .as_object()
            .expect("empty session params"),
            false,
        )
        .expect_err("blank projectPath does not fall back to cwd");
    assert_eq!(empty_error.code, "badRequest");
    assert_eq!(
        empty_error.message,
        "Invalid gxserver project ID: project-local."
    );
}

#[test]
fn missing_project_folder_blocks_session_insertion_until_relocated() {
    let (temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let original_path = temp.path().join("original-project");
    let relocated_path = temp.path().join("relocated-project");
    std::fs::create_dir_all(&original_path).expect("original project dir");
    let project = repository
        .add_project_path(
            json!({ "name": "Moved Project", "path": path_str(&original_path) })
                .as_object()
                .expect("project params"),
        )
        .expect("project added");
    let project_id = value_str(&project, "projectId").to_string();
    std::fs::rename(&original_path, &relocated_path).expect("move project dir");

    assert_eq!(project_path_state(&project), ProjectPathState::Missing);
    let error = repository
        .create_session(
            json!({ "projectId": project_id, "title": "Terminal" })
                .as_object()
                .expect("session params"),
            false,
        )
        .expect_err("missing project folder blocks session creation");
    assert_eq!(error.code, "projectPathUnavailable");
    assert!(repository
        .list_sessions(Some(&project_id))
        .expect("sessions")
        .is_empty());

    let relocated = repository
        .relocate_project(
            json!({ "path": path_str(&relocated_path), "projectId": project_id })
                .as_object()
                .expect("relocate params"),
        )
        .expect("project relocated");
    assert_eq!(value_str(&relocated, "projectId"), project_id);
    assert_eq!(value_str(&relocated, "name"), "Moved Project");
    assert_eq!(value_str(&relocated, "path"), path_str(&relocated_path));
    assert_eq!(project_path_state(&relocated), ProjectPathState::Available);

    let session = repository
        .create_session(
            json!({ "projectId": project_id, "title": "Terminal" })
                .as_object()
                .expect("session params"),
            false,
        )
        .expect("session created after relocation");
    assert_eq!(value_str(&session, "projectId"), project_id);
}

#[test]
fn add_project_path_attaches_and_repairs_linked_worktree_metadata() {
    if !git_available() {
        return;
    }

    let (temp, db) = open_test_database();
    let root = temp.path();
    let repo_path = root.join("registered-main");
    let worktree_path = root.join("registered-main-feature");
    let orphan_worktree_path = root.join("registered-main-orphan");
    std::fs::create_dir_all(&repo_path).expect("repo dir");
    run_git_for_test(&repo_path, &["init"]);
    run_git_for_test(
        &repo_path,
        &["config", "user.email", "ghostex@example.invalid"],
    );
    run_git_for_test(&repo_path, &["config", "user.name", "Ghostex Test"]);
    std::fs::write(repo_path.join("README.md"), "main\n").expect("write readme");
    run_git_for_test(&repo_path, &["add", "README.md"]);
    run_git_for_test(&repo_path, &["commit", "-m", "Initial commit"]);
    run_git_for_test(
        &repo_path,
        &[
            "worktree",
            "add",
            "-b",
            "feature/existing-worktree",
            path_str(&worktree_path),
        ],
    );
    run_git_for_test(
        &repo_path,
        &[
            "worktree",
            "add",
            "-b",
            "feature/orphan-worktree",
            path_str(&orphan_worktree_path),
        ],
    );

    let repository = DomainRepository::new(&db, "S7k");
    let main_params = json!({ "name": "Registered Main", "path": path_str(&repo_path) });
    let main_project = repository
        .add_project_path(main_params.as_object().expect("main params"))
        .expect("main project");
    let main_project_id = value_str(&main_project, "projectId").to_string();

    let worktree_params = json!({ "path": path_str(&worktree_path) });
    let worktree_project = repository
        .add_project_path(worktree_params.as_object().expect("worktree params"))
        .expect("worktree project");
    assert_eq!(
        value_str(&worktree_project, "path"),
        path_str(&worktree_path)
    );
    let metadata = object_field(&worktree_project, "worktree");
    assert_eq!(
        metadata.get("parentProjectId").and_then(Value::as_str),
        Some(main_project_id.as_str())
    );
    assert_eq!(
        metadata.get("parentProjectName").and_then(Value::as_str),
        Some("Registered Main")
    );
    assert_eq!(
        metadata.get("parentProjectPath").and_then(Value::as_str),
        Some(path_str(&repo_path))
    );
    assert_eq!(
        metadata.get("branch").and_then(Value::as_str),
        Some("feature/existing-worktree")
    );
    assert!(metadata.get("createdAt").and_then(Value::as_str).is_some());

    let second_add = repository
        .add_project_path(worktree_params.as_object().expect("worktree params"))
        .expect("second worktree add");
    assert_eq!(
        value_str(&second_add, "projectId"),
        value_str(&worktree_project, "projectId")
    );

    let orphan_params =
        json!({ "name": "Orphan Worktree", "path": path_str(&orphan_worktree_path) });
    let orphan_project = repository
        .create_project(orphan_params.as_object().expect("orphan params"))
        .expect("orphan project");
    assert!(orphan_project.get("worktree").is_none());
    let repaired = repository
        .add_project_path(orphan_params.as_object().expect("orphan params"))
        .expect("repaired worktree project");
    assert_eq!(
        value_str(&repaired, "projectId"),
        value_str(&orphan_project, "projectId")
    );
    let repaired_metadata = object_field(&repaired, "worktree");
    assert_eq!(
        repaired_metadata
            .get("parentProjectId")
            .and_then(Value::as_str),
        Some(main_project_id.as_str())
    );
    assert_eq!(
        repaired_metadata.get("branch").and_then(Value::as_str),
        Some("feature/orphan-worktree")
    );
}

fn stash_params(content: &str, project_id: Option<&str>) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("content".to_string(), json!(content));
    if let Some(project_id) = project_id {
        params.insert("projectId".to_string(), json!(project_id));
    }
    params.insert("sessionId".to_string(), json!("G1abc"));
    params.insert("cwd".to_string(), json!("/tmp/example"));
    params
}

fn listed_stash_contents(repository: &DomainRepository, project_id: Option<&str>) -> Vec<String> {
    let mut params = Map::new();
    if let Some(project_id) = project_id {
        params.insert("projectId".to_string(), json!(project_id));
    }
    repository
        .list_stashed_prompts(&params)
        .expect("list stashed prompts")
        .get("prompts")
        .and_then(Value::as_array)
        .expect("prompts array")
        .iter()
        .map(|prompt| value_str(prompt, "content").to_string())
        .collect()
}

#[test]
fn stashed_prompts_save_dedupes_same_project_content() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    let first = repository
        .save_stashed_prompt(&stash_params("fix the login bug", Some("P1aaa")))
        .expect("first save");
    let second = repository
        .save_stashed_prompt(&stash_params("fix the login bug", Some("P1aaa")))
        .expect("duplicate save");
    assert_eq!(
        value_str(first.get("prompt").expect("prompt"), "promptId"),
        value_str(second.get("prompt").expect("prompt"), "promptId"),
    );
    assert_eq!(listed_stash_contents(&repository, None).len(), 1);

    // Same content in another project stays a separate stash.
    repository
        .save_stashed_prompt(&stash_params("fix the login bug", Some("P2bbb")))
        .expect("other-project save");
    assert_eq!(listed_stash_contents(&repository, None).len(), 2);

    let error = repository
        .save_stashed_prompt(&stash_params("   \n  ", Some("P1aaa")))
        .expect_err("blank content rejected");
    assert_eq!(error.code, "badRequest");
}

#[test]
fn stashed_prompts_edit_updates_existing_row_in_place() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    let saved = repository
        .save_stashed_prompt(&stash_params("original prompt", Some("P1aaa")))
        .expect("save prompt");
    let saved_prompt = saved.get("prompt").expect("saved prompt");
    let prompt_id = value_str(saved_prompt, "promptId").to_string();

    let mut edit_params = stash_params("edited prompt", Some("P2bbb"));
    edit_params.insert("promptId".to_string(), json!(prompt_id));
    let edited = repository
        .save_stashed_prompt(&edit_params)
        .expect("edit prompt");
    let edited_prompt = edited.get("prompt").expect("edited prompt");

    assert_eq!(value_str(edited_prompt, "promptId"), prompt_id);
    assert_eq!(value_str(edited_prompt, "content"), "edited prompt");
    assert_eq!(value_str(edited_prompt, "projectId"), "P1aaa");
    assert_eq!(
        listed_stash_contents(&repository, None),
        vec!["edited prompt"]
    );
}

#[test]
fn stashed_prompts_list_scopes_to_project_and_delete_removes() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    repository
        .save_stashed_prompt(&stash_params("prompt in project A", Some("P1aaa")))
        .expect("save A");
    repository
        .save_stashed_prompt(&stash_params("prompt in project B", Some("P2bbb")))
        .expect("save B");
    repository
        .save_stashed_prompt(&stash_params("projectless prompt", None))
        .expect("save projectless");

    assert_eq!(
        listed_stash_contents(&repository, Some("P1aaa")),
        vec!["prompt in project A".to_string()]
    );
    let all = listed_stash_contents(&repository, None);
    assert_eq!(all.len(), 3);
    // Newest first.
    assert_eq!(all[0], "projectless prompt");

    let saved = repository
        .save_stashed_prompt(&stash_params("prompt in project B", Some("P2bbb")))
        .expect("re-save B");
    let prompt_id = value_str(saved.get("prompt").expect("prompt"), "promptId").to_string();
    let mut delete_params = Map::new();
    delete_params.insert("promptId".to_string(), json!(prompt_id));
    let deleted = repository
        .delete_stashed_prompt(&delete_params)
        .expect("delete");
    assert_eq!(deleted.get("deleted"), Some(&json!(true)));
    assert_eq!(listed_stash_contents(&repository, None).len(), 2);
    let deleted_again = repository
        .delete_stashed_prompt(&delete_params)
        .expect("delete again");
    assert_eq!(deleted_again.get("deleted"), Some(&json!(false)));
}

#[test]
fn stashed_prompts_project_scope_includes_legacy_worktree_family() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");

    // Register a parent project and a legacy worktree-as-project child by
    // writing the rows directly; detect_registered_git_worktree_metadata
    // needs a real git checkout, which this scoping test does not.
    let parent = repository
        .create_project(
            json!({ "name": "Main", "path": "/tmp/stash-main" })
                .as_object()
                .expect("parent params"),
        )
        .expect("parent project");
    let parent_id = value_str(&parent, "projectId").to_string();
    let child = repository
        .create_project(
            json!({ "name": "Main Worktree", "path": "/tmp/stash-worktree" })
                .as_object()
                .expect("child params"),
        )
        .expect("child project");
    let child_id = value_str(&child, "projectId").to_string();
    db.execute(
        "UPDATE projects SET worktreeJson = ?1 WHERE projectId = ?2",
        params![
            json!({ "parentProjectId": parent_id }).to_string(),
            child_id
        ],
    )
    .expect("mark worktree project");

    repository
        .save_stashed_prompt(&stash_params("parent prompt", Some(&parent_id)))
        .expect("save parent");
    repository
        .save_stashed_prompt(&stash_params("worktree prompt", Some(&child_id)))
        .expect("save worktree");
    repository
        .save_stashed_prompt(&stash_params("unrelated prompt", Some("P9zzz")))
        .expect("save unrelated");

    let mut from_parent = listed_stash_contents(&repository, Some(&parent_id));
    let mut from_child = listed_stash_contents(&repository, Some(&child_id));
    from_parent.sort();
    from_child.sort();
    let expected = vec!["parent prompt".to_string(), "worktree prompt".to_string()];
    assert_eq!(from_parent, expected);
    assert_eq!(from_child, expected);
}
