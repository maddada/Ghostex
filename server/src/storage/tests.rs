use super::*;
use crate::paths::get_gxserver_paths;

#[test]
fn initializes_sqlite_with_current_migrations_and_schema_layout() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let result = initialize_gxserver_storage(&paths).expect("storage init");
    let second = initialize_gxserver_storage(&paths).expect("second storage init");
    assert_eq!(
        result.applied_migrations,
        GXSERVER_MIGRATION_IDS
            .iter()
            .map(|id| (*id).to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(second.applied_migrations, Vec::<String>::new());
    assert_eq!(
        result.state_db_file,
        paths.state_db_file.to_string_lossy().to_string()
    );

    let db = open_gxserver_database(&paths).expect("open db");
    let user_version: i64 = db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version");
    let foreign_keys: i64 = db
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("foreign_keys");
    let journal_mode: String = db
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("journal_mode");
    assert_eq!(user_version, 48);
    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode, "wal");
    assert_eq!(schema_migration_count(&db), 48);
    assert_eq!(
        explicit_index_names(&db),
        vec![
            "agent_message_deliveries_waiting".to_string(),
            "idx_app_user_data_kind_updated".to_string(),
            "idx_automation_runs_active".to_string(),
            "idx_automation_runs_project_created".to_string(),
            "idx_automations_due".to_string(),
            "idx_automations_project_updated".to_string(),
            "idx_coordinator_threads_coordinator".to_string(),
            "idx_delayed_sends_due".to_string(),
            "idx_global_sidebar_commands_order".to_string(),
            "idx_id_allocations_kind_parent".to_string(),
            "idx_notification_feed_read".to_string(),
            "idx_notification_feed_session".to_string(),
            "idx_portless_domain_project_identity".to_string(),
            "idx_portless_domain_project_slug".to_string(),
            "idx_portless_domain_worktree_identity".to_string(),
            "idx_portless_domain_worktree_slug".to_string(),
            "idx_projects_recent_closed".to_string(),
            "idx_projects_visibility".to_string(),
            "idx_session_chat_queued_prompts_session".to_string(),
            "idx_sessions_lifecycle_updated".to_string(),
            "idx_sessions_presentation_kept".to_string(),
            "idx_sessions_project_sidebar_order".to_string(),
            "idx_sessions_project_updated".to_string(),
            "idx_stashed_prompt_tag_links_tag".to_string(),
            "idx_stashed_prompts_agent_session".to_string(),
            "idx_stashed_prompts_updated".to_string(),
            "session_chat_draft_handoffs_session".to_string(),
        ]
    );
    assert_eq!(
        table_columns(&db, "projects"),
        vec![
            "projectId",
            "name",
            "path",
            "identityIconJson",
            "isPinned",
            "isFavorite",
            "defaultCommand",
            "worktreeJson",
            "customAgentsJson",
            "customAgentOrderJson",
            "customCommandsJson",
            "customCommandOrderJson",
            "deletedDefaultCommandIdsJson",
            "launchSettingsJson",
            "runtimeSettingsJson",
            "completionRulesJson",
            "attentionRulesJson",
            "notificationRulesJson",
            "gitConfigJson",
            "projectBoardConfigJson",
            "previousSessionHistoryJson",
            "createdAt",
            "updatedAt",
            "isRecentProject",
            "recentClosedAt",
            "visibility",
            "systemKind",
        ]
    );
    assert_eq!(
        table_columns(&db, "sessions"),
        vec![
            "projectId",
            "sessionId",
            "kind",
            "title",
            "lifecycleState",
            "providerStateJson",
            "zmxName",
            "cwd",
            "agentId",
            "commandId",
            "isPinned",
            "isFavorite",
            "restoredFromSessionId",
            "restoredFromHistoryId",
            "launchSettingsJson",
            "runtimeSettingsJson",
            "completionRulesJson",
            "attentionRulesJson",
            "notificationRulesJson",
            "worktreeJson",
            "createdAt",
            "updatedAt",
            "lastActiveAt",
            "sidebarOrder",
            "settledAt",
            "settledOverride",
            "settledOverrideAt",
            "snoozedAt",
            "snoozedUntil",
            "isParked",
            "sessionTag",
        ]
    );
    assert_eq!(
        table_columns(&db, "delayed_sends"),
        vec![
            "projectId",
            "sessionId",
            "trigger",
            "deadlineAt",
            "nonWorkingSinceAt",
            "state",
            "errorMessage",
            "createdAt",
            "updatedAt",
            "watchedProjectId",
            "watchedSessionId",
        ]
    );
    assert_eq!(
        table_columns(&db, "portless_domain_identities"),
        vec![
            "identityId",
            "identityScope",
            "projectId",
            "worktreeKey",
            "projectSlug",
            "worktreeSlug",
            "createdAt",
            "updatedAt",
        ]
    );
    assert_eq!(
        table_columns(&db, "portless_state"),
        vec![
            "stateId",
            "enabled",
            "protocol",
            "setupOwnership",
            "setupStatus",
            "runtimeStatus",
            "createdAt",
            "updatedAt",
        ]
    );
    assert_eq!(
        table_columns(&db, "automations"),
        vec![
            "automationId",
            "projectId",
            "agentId",
            "name",
            "prompt",
            "enabled",
            "scheduleJson",
            "executionModeJson",
            "nextRunAt",
            "createdAt",
            "updatedAt",
        ]
    );
    assert_eq!(
        table_columns(&db, "automation_runs"),
        vec![
            "runId",
            "automationId",
            "projectId",
            "status",
            "sessionId",
            "worktreeJson",
            "errorMessage",
            "findingsSummary",
            "isArchived",
            "isUnread",
            "createdAt",
            "completedAt",
            "updatedAt",
        ]
    );
    assert_eq!(
        table_columns(&db, "stashed_prompts"),
        vec![
            "promptId",
            "content",
            "projectId",
            "sessionId",
            "cwd",
            "createdAt",
            "updatedAt",
            "agentSessionId",
        ]
    );
    let foreign_key: (String, String, String, String) = db
        .query_row("PRAGMA foreign_key_list(sessions)", [], |row| {
            Ok((row.get(2)?, row.get(3)?, row.get(4)?, row.get(6)?))
        })
        .expect("sessions foreign key");
    assert_eq!(
        foreign_key,
        (
            "projects".to_string(),
            "projectId".to_string(),
            "projectId".to_string(),
            "CASCADE".to_string()
        )
    );
}

#[test]
fn existing_state_db_rows_survive_rust_storage_initialization() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");

    {
        let db = open_gxserver_database(&paths).expect("open db");
        insert_project(&db, "P1ts", "TypeScript Project", "/tmp/typescript-project");
        insert_session(&db, "P1ts", "G1ts", "TypeScript Session");
    }

    let result = initialize_gxserver_storage(&paths).expect("storage re-init");
    assert_eq!(result.applied_migrations, Vec::<String>::new());

    let db = open_gxserver_database(&paths).expect("open db");
    let project_name: String = db
        .query_row(
            "SELECT name FROM projects WHERE projectId = ?1",
            ["P1ts"],
            |row| row.get(0),
        )
        .expect("project row");
    let session_title: String = db
        .query_row(
            "SELECT title FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
            ("P1ts", "G1ts"),
            |row| row.get(0),
        )
        .expect("session row");
    assert_eq!(project_name, "TypeScript Project");
    assert_eq!(session_title, "TypeScript Session");
}

#[test]
fn migration_status_reads_typescript_legacy_import_metadata_from_state_db() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let result = initialize_gxserver_storage(&paths).expect("storage init");
    {
        let db = open_gxserver_database(&paths).expect("open db");
        db.execute(
            r#"
                INSERT INTO metadata (key, value, updatedAt)
                VALUES (?1, ?2, ?3)
                "#,
            rusqlite::params![
                LEGACY_IMPORT_METADATA_KEY,
                serde_json::json!({
                    "completedAt": "2026-05-30T17:27:00.000Z",
                    "id": LEGACY_MACOS_STATE_IMPORT_ID,
                    "logsImported": {
                        "filesRead": 2,
                        "malformedLineCount": 1,
                        "migratedLineCount": 6,
                    },
                    "projectsImported": 3,
                    "sessionsImported": 4,
                    "sourceFilesRead": ["native-sidebar-projects.json"],
                    "status": "completed",
                })
                .to_string(),
                "2026-05-30T17:27:00.000Z",
            ],
        )
        .expect("insert legacy import metadata");
    }

    let status = create_gxserver_migration_status(&result);
    let legacy_status = status
        .state_imports
        .expect("state imports")
        .legacy_macos_state;
    assert_eq!(
        legacy_status.completed_at.as_deref(),
        Some("2026-05-30T17:27:00.000Z")
    );
    assert_eq!(legacy_status.id, LEGACY_MACOS_STATE_IMPORT_ID);
    assert_eq!(legacy_status.projects_imported, Some(3));
    assert_eq!(legacy_status.sessions_imported, Some(4));
    assert_eq!(
        legacy_status
            .logs_imported
            .as_ref()
            .map(|logs| logs.migrated_line_count),
        Some(6)
    );
    assert_eq!(
        legacy_status.source_files_read,
        Some(vec!["native-sidebar-projects.json".to_string()])
    );
    assert_eq!(
        legacy_status.skipped_reason.as_deref(),
        Some("alreadyCompleted")
    );
    assert_eq!(legacy_status.status, "skipped");
}

#[test]
fn legacy_macos_recent_project_backfill_reads_the_resolved_state_directory() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    paths.app_state_dir = temp.path().join(".local/state/ghostex");
    ensure_gxserver_storage_layout(&paths).expect("storage layout");
    let mut db = open_gxserver_database(&paths).expect("open db");
    run_gxserver_migrations(&mut db).expect("migrations");
    insert_project(&db, "P1rec", "Recent", "/repo/recent");
    insert_project(&db, "P2vis", "Visible", "/repo/visible");
    let state_dir = paths.app_state_dir.clone();
    fs::create_dir_all(&state_dir).expect("state dir");
    fs::write(
        state_dir.join(LEGACY_NATIVE_PROJECTS_STATE_FILE),
        serde_json::json!({
            "projects": [
                {
                    "isRecentProject": true,
                    "projectId": "P1rec",
                    "recentClosedAt": "2026-06-27T15:36:00.000Z",
                },
                {
                    "isRecentProject": false,
                    "projectId": "P2vis",
                },
                {
                    "isRecentProject": true,
                    "projectId": "P9miss",
                    "recentClosedAt": "2026-06-27T16:00:00.000Z",
                },
            ],
        })
        .to_string(),
    )
    .expect("legacy projects file");

    backfill_legacy_macos_recent_projects(&mut db, &paths).expect("backfill");

    let recent: (i64, Option<String>) = db
        .query_row(
            "SELECT isRecentProject, recentClosedAt FROM projects WHERE projectId = 'P1rec'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("recent row");
    assert_eq!(recent, (1, Some("2026-06-27T15:36:00.000Z".to_string())));
    let visible: i64 = db
        .query_row(
            "SELECT isRecentProject FROM projects WHERE projectId = 'P2vis'",
            [],
            |row| row.get(0),
        )
        .expect("visible row");
    assert_eq!(visible, 0);
    let marker: Value = serde_json::from_str(
        &db.query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [LEGACY_RECENT_PROJECTS_BACKFILL_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .expect("marker"),
    )
    .expect("marker json");
    assert_eq!(marker["status"], "completed");
    assert_eq!(marker["legacyRecentProjects"], 2);
    assert_eq!(marker["matchedProjects"], 1);
    assert_eq!(marker["updatedProjects"], 1);
}

#[test]
fn previous_session_quality_migration_matches_typescript_cleanup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_gxserver_storage_layout(&paths).expect("storage layout");
    let mut db = open_gxserver_database(&paths).expect("open db");
    apply_migration_range(&mut db, 0..3);
    insert_project(&db, "P1cle", "Ghostex", "/repo/ghostex");

    /*
    CDXC:ServerDaemon 2026-06-22-05:10:
    Rust storage migrations must preserve TypeScript-created state.db behavior for existing users. Migration 0004 removes only low-signal inactive placeholder rows and backfills retained inactive rows with updatedAt, matching the TypeScript cleanup semantics.
    */
    insert_pre_tag_session(&db, "P1cle", "G1noi", "Terminal Session", "stopped", 0);
    insert_pre_tag_session(&db, "P1cle", "G2kee", "Useful restore row", "stopped", 0);
    insert_pre_tag_session(&db, "P1cle", "G3fav", "Codex Session", "stopped", 1);
    insert_pre_tag_session(&db, "P1cle", "G4unk", "Unknown stale row", "unknown", 0);
    insert_pre_tag_session(&db, "P1cle", "G5run", "Running row", "running", 0);

    run_gxserver_migrations(&mut db).expect("remaining migrations");

    let rows = query_session_activity(&db);
    assert_eq!(
        rows.iter()
            .map(|(session_id, _, _)| session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["G2kee", "G3fav", "G5run"]
    );
    assert_eq!(
        rows.iter()
            .find(|(session_id, _, _)| session_id == "G2kee")
            .and_then(|(_, _, last_active_at)| last_active_at.as_deref()),
        Some("2026-06-04T16:21:00.000Z")
    );
    assert_eq!(
        rows.iter()
            .find(|(session_id, _, _)| session_id == "G3fav")
            .and_then(|(_, _, last_active_at)| last_active_at.as_deref()),
        Some("2026-06-04T16:21:00.000Z")
    );
    assert_eq!(
        rows.iter()
            .find(|(session_id, _, _)| session_id == "G5run")
            .and_then(|(_, _, last_active_at)| last_active_at.as_deref()),
        None
    );
}

#[test]
fn session_lifecycle_migration_leaves_pre_upgrade_rows_without_lifecycle_state() {
    /*
    CDXC:StateSync 2026-07-29-00:00:
    Every state.db written before migration 0016 must keep working: the new
    settle/snooze columns are added as NULL, which is exactly the "never
    settled, never snoozed" state the Sidebar V2 predicates already expect,
    and no existing row is rewritten.
    */
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_gxserver_storage_layout(&paths).expect("storage layout");
    let mut db = open_gxserver_database(&paths).expect("open db");
    apply_migration_range(&mut db, 0..15);
    insert_project(&db, "P1life", "Ghostex", "/repo/ghostex");
    insert_session(&db, "P1life", "G1life", "Pre-upgrade session");

    run_gxserver_migrations(&mut db).expect("remaining migrations");

    for column in [
        "settledAt",
        "settledOverride",
        "settledOverrideAt",
        "snoozedAt",
        "snoozedUntil",
    ] {
        let value: Option<String> = db
            .query_row(
                &format!("SELECT {column} FROM sessions WHERE sessionId = ?1"),
                ["G1life"],
                |row| row.get(0),
            )
            .expect("session lifecycle column");
        assert_eq!(value, None, "{column} must default to NULL");
    }
    let title: String = db
        .query_row(
            "SELECT title FROM sessions WHERE sessionId = ?1",
            ["G1life"],
            |row| row.get(0),
        )
        .expect("session title");
    assert_eq!(title, "Pre-upgrade session");
}

#[test]
fn unsupported_session_kind_migration_removes_legacy_rows_before_tightening_schema() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_gxserver_storage_layout(&paths).expect("storage layout");
    let mut db = open_gxserver_database(&paths).expect("open db");
    apply_migration_range(&mut db, 0..18);
    insert_project(&db, "P1kind", "Ghostex", "/repo/ghostex");
    insert_session(&db, "P1kind", "G1keep", "Supported session");
    insert_session(&db, "P1kind", "G2drop", "Retired session");
    db.execute(
        "UPDATE sessions SET kind = 't3' WHERE sessionId = 'G2drop'",
        [],
    )
    .expect("mark retired session kind");
    db.execute(
        r#"
            UPDATE sessions
            SET settledAt = '2026-08-09T12:00:00.000Z',
                settledOverride = 'settled',
                settledOverrideAt = '2026-08-09T12:00:00.000Z',
                snoozedAt = '2026-08-09T12:01:00.000Z',
                snoozedUntil = '2026-08-10T12:01:00.000Z'
            WHERE sessionId = 'G1keep'
            "#,
        [],
    )
    .expect("set supported lifecycle state");

    apply_migration_range(&mut db, 18..19);

    let rows = db
        .prepare("SELECT sessionId, kind FROM sessions ORDER BY sessionId")
        .expect("prepare session query")
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("query sessions")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect sessions");
    assert_eq!(rows, vec![("G1keep".to_string(), "terminal".to_string())]);

    let lifecycle: (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = db
        .query_row(
            r#"
                SELECT settledAt, settledOverride, settledOverrideAt, snoozedAt, snoozedUntil
                FROM sessions
                WHERE sessionId = 'G1keep'
                "#,
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("supported lifecycle state");
    assert_eq!(
        lifecycle,
        (
            Some("2026-08-09T12:00:00.000Z".to_string()),
            Some("settled".to_string()),
            Some("2026-08-09T12:00:00.000Z".to_string()),
            Some("2026-08-09T12:01:00.000Z".to_string()),
            Some("2026-08-10T12:01:00.000Z".to_string()),
        )
    );

    let insert_retired = db.execute(
        r#"
            INSERT INTO sessions (
              projectId, sessionId, kind, title, lifecycleState,
              providerStateJson, zmxName, createdAt, updatedAt
            ) VALUES (
              'P1kind', 'G3reject', 't3', 'Rejected session', 'stopped',
              '{}', 'G3reject', '2026-08-09T12:00:00.000Z', '2026-08-09T12:00:00.000Z'
            )
            "#,
        [],
    );
    assert!(
        insert_retired.is_err(),
        "the rebuilt schema must reject t3 rows"
    );
}

#[test]
fn session_tag_expansion_migrations_match_typescript_allowed_values() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_gxserver_storage_layout(&paths).expect("storage layout");
    let mut db = open_gxserver_database(&paths).expect("open db");
    apply_migration_range(&mut db, 0..5);
    insert_project(&db, "P1tag", "Ghostex", "/repo/ghostex");
    insert_session(&db, "P1tag", "G1old", "Old allowed tag");
    update_session_tag(&db, "G1old", Some("todo"));

    /*
    CDXC:Sessions 2026-06-22-05:58:
    Rust storage migrations must keep the TypeScript sessionTag schema contract: supported tag values survive each constraint rebuild, legacy/retired values are cleared by migration 0008, and existing state.db files can continue through the expanded tag model.
    */
    apply_migration_range(&mut db, 5..6);
    update_session_tag(&db, "G1old", Some("testing"));
    insert_session(&db, "P1tag", "G2new", "Blocked tag");
    update_session_tag(&db, "G2new", Some("blocked"));

    apply_migration_range(&mut db, 6..7);
    insert_session(&db, "P1tag", "G3wip", "In Progress tag");
    update_session_tag(&db, "G3wip", Some("in-progress"));
    insert_session(&db, "P1tag", "G4typ", "Bug tag");
    update_session_tag(&db, "G4typ", Some("bug"));
    insert_session(&db, "P1tag", "G5des", "Design tag");
    update_session_tag(&db, "G5des", Some("design"));

    db.execute_batch("PRAGMA ignore_check_constraints = ON;")
        .expect("disable tag check");
    update_session_tag(&db, "G4typ", Some("retired-type"));
    db.execute_batch("PRAGMA ignore_check_constraints = OFF;")
        .expect("restore tag check");
    apply_migration_range(&mut db, 7..8);

    let rows = query_session_tags(&db);
    assert_eq!(
        rows,
        vec![
            ("G1old".to_string(), Some("testing".to_string())),
            ("G2new".to_string(), Some("blocked".to_string())),
            ("G3wip".to_string(), Some("in-progress".to_string())),
            ("G4typ".to_string(), None),
            ("G5des".to_string(), Some("design".to_string())),
        ]
    );
}

#[test]
fn legacy_zmux_chat_project_migration_removes_only_typescript_legacy_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_gxserver_storage_layout(&paths).expect("storage layout");
    let mut db = open_gxserver_database(&paths).expect("open db");
    apply_migration_range(&mut db, 0..8);

    let old_chat_path = temp
        .path()
        .join("zmux/chats/2026-05-08-140732018-chat")
        .to_string_lossy()
        .to_string();
    let old_plugins_path = temp
        .path()
        .join("zmux/chats/2026-05-08-110833862-plugins")
        .to_string_lossy()
        .to_string();
    let current_chat_path = temp
        .path()
        .join("ghostex/chats/2026-06-05-200700000-chat")
        .to_string_lossy()
        .to_string();
    let repo_path = temp
        .path()
        .join("dev/zmux/chats/repo")
        .to_string_lossy()
        .to_string();

    /*
    CDXC:ServerDaemon 2026-06-22-05:10:
    Migration 0009 is intentionally narrow for TypeScript-created state.db compatibility: delete only legacy `~/zmux/chats` Chat/Browser/Plugins quick-project rows, leaving current `~/ghostex/chats` projects and normal repositories whose paths happen to include `/zmux/chats/`.
    */
    insert_project(&db, "P4rpp", "Chat 2026-05-08 14:07", &old_chat_path);
    insert_session(&db, "P4rpp", "G1old", "Terminal Session");
    insert_project(&db, "P5rpk", "Plugins", &old_plugins_path);
    insert_project(&db, "P6new", "Chat 2026-06-05 20:07", &current_chat_path);
    insert_project(&db, "P7rep", "Repo", &repo_path);

    run_gxserver_migrations(&mut db).expect("remaining migrations");

    let projects = query_project_names_and_paths(&db);
    assert_eq!(
        projects,
        vec![
            ("Chat 2026-06-05 20:07".to_string(), current_chat_path),
            ("Repo".to_string(), repo_path),
        ]
    );
    let old_session_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE projectId = ?1",
            ["P4rpp"],
            |row| row.get(0),
        )
        .expect("old session count");
    assert_eq!(old_session_count, 0);
}

#[test]
fn migration_status_can_serialize_typescript_not_run_shape() {
    let status = MigrationStatus {
        applied_migrations: Vec::new(),
        current_version: GXSERVER_MIGRATION_IDS.len(),
        state_db_file: "/tmp/state.db".to_string(),
        state_imports: Some(MigrationStateImports {
            legacy_macos_state: LegacyMacosStateImportStatus {
                completed_at: None,
                id: "legacy_macos_sidebar_state_v1".to_string(),
                logs_imported: None,
                projects_imported: None,
                sessions_imported: None,
                skipped_reason: None,
                source_files_read: None,
                status: "notRun".to_string(),
            },
        }),
    };

    let value = serde_json::to_value(status).expect("migration status json");
    assert_eq!(
        value["stateImports"]["legacyMacosState"],
        serde_json::json!({
            "id": "legacy_macos_sidebar_state_v1",
            "status": "notRun",
        })
    );
}

#[cfg(unix)]
#[test]
fn storage_initialization_creates_auth_and_config_with_strict_modes() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

    initialize_gxserver_storage(&paths).expect("storage init");

    assert_eq!(
        fs::metadata(&paths.auth_dir)
            .expect("auth dir metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&paths.config_file)
            .expect("config metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

fn apply_migration_range(db: &mut Connection, range: std::ops::Range<usize>) {
    db.execute_batch(
        r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
              id TEXT PRIMARY KEY,
              appliedAt TEXT NOT NULL
            );
            "#,
    )
    .expect("create schema_migrations");

    for migration in &GXSERVER_STORAGE_MIGRATIONS[range] {
        let transaction = db.transaction().expect("migration transaction");
        transaction
            .execute_batch(migration.sql)
            .expect("migration sql");
        transaction
            .execute(
                "INSERT INTO schema_migrations (id, appliedAt) VALUES (?1, ?2)",
                (migration.id, "2026-06-22T01:10:00.000Z"),
            )
            .expect("record migration");
        transaction.commit().expect("commit migration");
    }
}

fn schema_migration_count(db: &Connection) -> i64 {
    db.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
        row.get(0)
    })
    .expect("schema migration count")
}

fn table_columns(db: &Connection, table: &str) -> Vec<String> {
    let mut statement = db
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("table info");
    statement
        .query_map([], |row| row.get::<_, String>("name"))
        .expect("table columns")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("table columns rows")
}

fn explicit_index_names(db: &Connection) -> Vec<String> {
    let mut statement = db
        .prepare(
            r#"
                SELECT name
                FROM sqlite_master
                WHERE type = 'index'
                  AND name NOT LIKE 'sqlite_autoindex_%'
                ORDER BY name
                "#,
        )
        .expect("index names");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("index rows")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("index row values")
}

fn insert_project(db: &Connection, project_id: &str, name: &str, path: &str) {
    db.execute(
        r#"
            INSERT INTO projects (projectId, name, path, createdAt, updatedAt)
            VALUES (?1, ?2, ?3, ?4, ?4)
            "#,
        rusqlite::params![project_id, name, path, "2026-06-04T16:21:00.000Z"],
    )
    .expect("insert project");
}

fn insert_session(db: &Connection, project_id: &str, session_id: &str, title: &str) {
    db.execute(
        r#"
            INSERT INTO sessions (
              projectId, sessionId, kind, title, lifecycleState, providerStateJson,
              zmxName, createdAt, updatedAt
            )
            VALUES (?1, ?2, 'terminal', ?3, 'stopped', '{}', ?4, ?5, ?5)
            "#,
        rusqlite::params![
            project_id,
            session_id,
            title,
            format!("S90-{project_id}-{session_id}"),
            "2026-06-04T16:21:00.000Z",
        ],
    )
    .expect("insert session");
}

fn insert_pre_tag_session(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    title: &str,
    lifecycle_state: &str,
    is_favorite: i64,
) {
    db.execute(
        r#"
            INSERT INTO sessions (
              projectId,
              sessionId,
              kind,
              title,
              lifecycleState,
              providerStateJson,
              zmxName,
              isPinned,
              isFavorite,
              launchSettingsJson,
              runtimeSettingsJson,
              completionRulesJson,
              attentionRulesJson,
              notificationRulesJson,
              worktreeJson,
              createdAt,
              updatedAt
            )
            VALUES (
              ?1,
              ?2,
              'terminal',
              ?3,
              ?4,
              '{}',
              ?5,
              0,
              ?6,
              '{}',
              '{}',
              '{}',
              '{}',
              '{}',
              '{}',
              ?7,
              ?7
            )
            "#,
        rusqlite::params![
            project_id,
            session_id,
            title,
            lifecycle_state,
            format!("S90-{project_id}-{session_id}"),
            is_favorite,
            "2026-06-04T16:21:00.000Z",
        ],
    )
    .expect("insert pre-tag session");
}

fn query_session_activity(db: &Connection) -> Vec<(String, String, Option<String>)> {
    let mut statement = db
        .prepare("SELECT sessionId, lifecycleState, lastActiveAt FROM sessions ORDER BY sessionId")
        .expect("session activity statement");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("session activity rows")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("session activity row values")
}

fn update_session_tag(db: &Connection, session_id: &str, session_tag: Option<&str>) {
    db.execute(
        "UPDATE sessions SET sessionTag = ?1 WHERE sessionId = ?2",
        rusqlite::params![session_tag, session_id],
    )
    .expect("update session tag");
}

fn query_session_tags(db: &Connection) -> Vec<(String, Option<String>)> {
    let mut statement = db
        .prepare("SELECT sessionId, sessionTag FROM sessions ORDER BY sessionId")
        .expect("session tag statement");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("session tag rows")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("session tag row values")
}

fn query_project_names_and_paths(db: &Connection) -> Vec<(String, String)> {
    let mut statement = db
        .prepare("SELECT name, path FROM projects ORDER BY projectId")
        .expect("project rows statement");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("project rows")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("project row values")
}
