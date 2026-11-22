use super::*;
use crate::ids::is_gxserver_session_id;
use serde_json::{json, Map, Value};

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
            json!({ "name": "Restore IDs", "path": std::env::temp_dir() })
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
            json!({ "name": "Provider Metadata", "path": std::env::temp_dir() })
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
            json!({ "name": "Session Tags", "path": std::env::temp_dir() })
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
            json!({ "name": "Invalid Stored Tags", "path": std::env::temp_dir() })
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
            json!({ "name": "Sidebar Order", "path": std::env::temp_dir() })
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
            json!({ "name": "Sidebar Rollback", "path": std::env::temp_dir() })
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
