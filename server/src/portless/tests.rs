
use std::fs;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection};

use super::*;
use crate::{
    paths::get_gxserver_paths,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};

#[test]
fn project_slug_create_read_update_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "P1main", "First Display Name");
    let repository = PortlessRepository::new(&db);

    let created = repository
        .upsert_project_slug("P1main", "ghostex")
        .expect("create project slug");
    assert_eq!(created.project_id, "P1main");
    assert_eq!(created.slug, "ghostex");
    assert_eq!(
        repository
            .read_project_slug("P1main")
            .expect("read project slug")
            .map(|record| record.slug),
        Some("ghostex".to_string())
    );

    db.execute(
        "UPDATE projects SET name = ?2, updatedAt = ?3 WHERE projectId = ?1",
        params!["P1main", "Renamed Display Name", "2026-06-22T18:41:00.000Z"],
    )
    .expect("rename display name");
    assert_eq!(
        repository
            .read_project_slug("P1main")
            .expect("read after display rename")
            .map(|record| record.slug),
        Some("ghostex".to_string())
    );

    let updated = repository
        .upsert_project_slug("P1main", "ghostex-app")
        .expect("update project slug");
    assert_eq!(updated.slug, "ghostex-app");
    assert_eq!(
        repository
            .read_project_slug("P1main")
            .expect("read updated project slug")
            .map(|record| record.slug),
        Some("ghostex-app".to_string())
    );
}

#[test]
fn worktree_slug_create_read_update_is_separate_from_display_names() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "P2main", "Main Display Name");
    let repository = PortlessRepository::new(&db);

    let created = repository
        .upsert_worktree_slug("P2main", "P2wtfix", "fix-ui")
        .expect("create worktree slug");
    assert_eq!(created.project_id, "P2main");
    assert_eq!(created.worktree_key, "P2wtfix");
    assert_eq!(created.slug, "fix-ui");

    db.execute(
        "UPDATE projects SET name = ?2, updatedAt = ?3 WHERE projectId = ?1",
        params![
            "P2main",
            "Completely Different Display Name",
            "2026-06-22T18:42:00.000Z"
        ],
    )
    .expect("rename project display name");
    assert_eq!(
        repository
            .read_worktree_slug("P2main", "P2wtfix")
            .expect("read worktree slug")
            .map(|record| record.slug),
        Some("fix-ui".to_string())
    );

    let updated = repository
        .upsert_worktree_slug("P2main", "P2wtfix", "fix-ui-2")
        .expect("update worktree slug");
    assert_eq!(updated.slug, "fix-ui-2");
    assert_eq!(
        repository
            .read_worktree_slug("P2main", "P2wtfix")
            .expect("read updated worktree slug")
            .map(|record| record.slug),
        Some("fix-ui-2".to_string())
    );
}

#[test]
fn ensure_project_slug_persists_first_assignment_across_project_renames() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "P1main",
        "First Display Name",
        Some("/tmp/first-display-name"),
        "2026-06-22T18:41:00.000Z",
    );
    let repository = PortlessRepository::new(&db);

    let created = repository
        .ensure_project_slug("P1main")
        .expect("ensure project slug");
    assert_eq!(created.slug, "first-display-name");

    db.execute(
        "UPDATE projects SET name = ?2, path = ?3, updatedAt = ?4 WHERE projectId = ?1",
        params![
            "P1main",
            "Renamed Project",
            "/tmp/renamed-project",
            "2026-06-22T18:45:00.000Z"
        ],
    )
    .expect("rename project");
    assert_eq!(
        repository
            .ensure_project_slug("P1main")
            .expect("ensure after rename")
            .slug,
        "first-display-name"
    );
    assert_eq!(
        repository
            .backfill_domain_identities()
            .expect("repeat backfill")
            .projects
            .into_iter()
            .find(|project| project.project_id == "P1main")
            .map(|project| project.slug),
        Some("first-display-name".to_string())
    );
}

#[test]
fn project_slug_uses_path_basename_when_visible_identity_has_no_label() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Ppath",
        "!!!",
        Some("/Users/person/dev/Path Fallback App"),
        "2026-06-22T18:41:00.000Z",
    );
    insert_project_with_path(&db, "Pempty", "!!!", None, "2026-06-22T18:42:00.000Z");
    let repository = PortlessRepository::new(&db);

    assert_eq!(
        repository
            .ensure_project_slug("Ppath")
            .expect("ensure project slug")
            .slug,
        "path-fallback-app"
    );
    let fallback = repository
        .ensure_project_slug("Pempty")
        .expect("ensure project fallback slug")
        .slug;
    assert!(fallback.starts_with("project-"));
    assert_eq!(
        repository
            .ensure_project_slug("Pempty")
            .expect("ensure project fallback slug again")
            .slug,
        fallback
    );
}

#[test]
fn project_slug_collisions_keep_first_clean_slug_and_stable_later_suffixes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Pfirst",
        "Ghostex",
        Some("/tmp/first"),
        "2026-06-22T18:41:00.000Z",
    );
    insert_project_with_path(
        &db,
        "Psecond",
        "Ghostex",
        Some("/tmp/second"),
        "2026-06-22T18:42:00.000Z",
    );
    let repository = PortlessRepository::new(&db);

    let first_backfill = repository
        .backfill_domain_identities()
        .expect("first backfill");
    let first_slug = project_slug(&first_backfill, "Pfirst");
    let second_slug = project_slug(&first_backfill, "Psecond");
    assert_eq!(first_slug, "ghostex");
    assert!(second_slug.starts_with("ghostex-"));
    assert_ne!(first_slug, second_slug);

    db.execute(
        "UPDATE projects SET name = ?2, updatedAt = ?3 WHERE projectId = ?1",
        params!["Pfirst", "Renamed Away", "2026-06-22T18:45:00.000Z"],
    )
    .expect("rename first project");
    let second_backfill = repository
        .backfill_domain_identities()
        .expect("second backfill");
    assert_eq!(project_slug(&second_backfill, "Pfirst"), "ghostex");
    assert_eq!(project_slug(&second_backfill, "Psecond"), second_slug);
    assert_eq!(
        sorted_project_slug_pairs(&first_backfill),
        sorted_project_slug_pairs(&second_backfill)
    );
}

#[test]
fn worktree_suffix_persists_across_worktree_name_and_branch_changes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Pparent",
        "Ghostex",
        Some("/tmp/ghostex"),
        "2026-06-22T18:41:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtfix",
        "Pparent",
        "Worktree Display",
        "Fix UI",
        "feature/fix-ui",
        "2026-06-22T18:42:00.000Z",
    );
    let repository = PortlessRepository::new(&db);

    let created = repository
        .ensure_worktree_slug("Pwtfix")
        .expect("ensure worktree slug");
    assert_eq!(created.parent_project_id, "Pparent");
    assert_eq!(created.project_slug, "ghostex");
    assert_eq!(created.worktree_key, "Pwtfix");
    assert_eq!(created.worktree_slug, "fix-ui");

    update_worktree_metadata(
        &db,
        "Pwtfix",
        "Pparent",
        "Renamed Worktree",
        "feature/renamed-worktree",
    );
    let after_rename = repository
        .ensure_worktree_slug("Pwtfix")
        .expect("ensure after rename");
    assert_eq!(after_rename.worktree_slug, "fix-ui");
    assert_eq!(after_rename.project_slug, "ghostex");
}

#[test]
fn worktree_suffix_uses_name_first_then_branch_last_segment() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Pparent",
        "Ghostex",
        Some("/tmp/ghostex"),
        "2026-06-22T18:41:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtname",
        "Pparent",
        "Name Project",
        "Release Prep",
        "feature/ignored-branch",
        "2026-06-22T18:42:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtbranch",
        "Pparent",
        "Branch Project",
        "!!!",
        "refs/heads/feature/fix/login",
        "2026-06-22T18:43:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtfallback",
        "Pparent",
        "Fallback Project",
        "!!!",
        "///",
        "2026-06-22T18:44:00.000Z",
    );
    let repository = PortlessRepository::new(&db);

    let identities = repository
        .backfill_domain_identities()
        .expect("backfill identities");
    assert_eq!(worktree_slug(&identities, "Pwtname"), "release-prep");
    assert_eq!(worktree_slug(&identities, "Pwtbranch"), "login");
    assert!(worktree_slug(&identities, "Pwtfallback").starts_with("wt-"));
}

#[test]
fn worktree_suffix_collisions_are_stable_per_parent_without_reshuffling() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Pparent",
        "Ghostex",
        Some("/tmp/ghostex"),
        "2026-06-22T18:41:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtfirst",
        "Pparent",
        "First Worktree",
        "Fix UI",
        "feature/fix-ui-a",
        "2026-06-22T18:42:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtsecond",
        "Pparent",
        "Second Worktree",
        "Fix UI",
        "feature/fix-ui-b",
        "2026-06-22T18:43:00.000Z",
    );
    let repository = PortlessRepository::new(&db);

    let first_backfill = repository
        .backfill_domain_identities()
        .expect("first backfill");
    let first_suffix = worktree_slug(&first_backfill, "Pwtfirst");
    let second_suffix = worktree_slug(&first_backfill, "Pwtsecond");
    assert_eq!(first_suffix, "fix-ui");
    assert!(second_suffix.starts_with("fix-ui-"));
    assert_ne!(first_suffix, second_suffix);

    update_worktree_metadata(&db, "Pwtfirst", "Pparent", "Other Name", "feature/other");
    let second_backfill = repository
        .backfill_domain_identities()
        .expect("second backfill");
    assert_eq!(worktree_slug(&second_backfill, "Pwtfirst"), "fix-ui");
    assert_eq!(worktree_slug(&second_backfill, "Pwtsecond"), second_suffix);
    assert_eq!(
        sorted_worktree_slug_pairs(&first_backfill),
        sorted_worktree_slug_pairs(&second_backfill)
    );
}

#[test]
fn setup_runtime_state_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    let repository = PortlessRepository::new(&db);

    assert_eq!(repository.read_state().expect("empty state"), None);

    let created = repository
        .upsert_state(PortlessState {
            enabled: true,
            protocol: PortlessProtocol::Https,
            setup_ownership: PortlessSetupOwnership::Missing,
            setup_status: PortlessSetupStatus::Needed,
            runtime_status: PortlessRuntimeStatus::Inactive,
        })
        .expect("create state");
    assert_eq!(
        created.state,
        PortlessState {
            enabled: true,
            protocol: PortlessProtocol::Https,
            setup_ownership: PortlessSetupOwnership::Missing,
            setup_status: PortlessSetupStatus::Needed,
            runtime_status: PortlessRuntimeStatus::Inactive,
        }
    );

    let updated = repository
        .upsert_state(PortlessState {
            enabled: false,
            protocol: PortlessProtocol::Http,
            setup_ownership: PortlessSetupOwnership::Ghostex,
            setup_status: PortlessSetupStatus::Disabled,
            runtime_status: PortlessRuntimeStatus::Active,
        })
        .expect("update state");
    assert_eq!(
        updated.state,
        PortlessState {
            enabled: false,
            protocol: PortlessProtocol::Http,
            setup_ownership: PortlessSetupOwnership::Ghostex,
            setup_status: PortlessSetupStatus::Disabled,
            runtime_status: PortlessRuntimeStatus::Active,
        }
    );
    assert_eq!(
        repository
            .read_state()
            .expect("read state")
            .map(|record| record.state),
        Some(updated.state)
    );
}

#[test]
fn state_update_protocol_change_marks_installed_service_for_reconfigure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    let repository = PortlessRepository::new(&db);
    repository
        .upsert_state(portless_state(
            true,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Active,
            PortlessRuntimeStatus::Active,
        ))
        .expect("active state");

    let updated = apply_portless_state_update(
        &paths,
        &db,
        PortlessStateUpdate::SetProtocol {
            protocol: PortlessProtocol::Http,
        },
    )
    .expect("protocol update");

    assert_eq!(updated.state.protocol, PortlessProtocol::Http);
    assert_eq!(
        updated.state.setup_ownership,
        PortlessSetupOwnership::Ghostex
    );
    assert_eq!(updated.state.setup_status, PortlessSetupStatus::Needed);
    assert_eq!(
        updated.state.runtime_status,
        PortlessRuntimeStatus::Inactive
    );
    assert!(updated.state.enabled);
}

#[test]
fn state_update_admin_failure_keeps_portless_enabled_and_failed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");

    let updated = apply_portless_state_update(
        &paths,
        &db,
        PortlessStateUpdate::RecordAdminResult {
            action: PortlessAdminResultAction::Install,
            ok: false,
            protocol: Some(PortlessProtocol::Https),
        },
    )
    .expect("failed admin result");

    assert!(updated.state.enabled);
    assert_eq!(updated.state.protocol, PortlessProtocol::Https);
    assert_eq!(
        updated.state.setup_ownership,
        PortlessSetupOwnership::Ghostex
    );
    assert_eq!(updated.state.setup_status, PortlessSetupStatus::Failed);
    assert_eq!(updated.state.runtime_status, PortlessRuntimeStatus::Failed);
    assert_eq!(
        recommended_portless_admin_action(&updated.state),
        Some(PortlessAdminActionKind::Retry)
    );
}

#[test]
fn state_update_retry_success_recovers_failed_setup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    let repository = PortlessRepository::new(&db);
    repository
        .upsert_state(portless_state(
            true,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Failed,
            PortlessRuntimeStatus::Failed,
        ))
        .expect("failed state");

    let updated = apply_portless_state_update(
        &paths,
        &db,
        PortlessStateUpdate::RecordAdminResult {
            action: PortlessAdminResultAction::Retry,
            ok: true,
            protocol: Some(PortlessProtocol::Http),
        },
    )
    .expect("retry admin result");

    assert!(updated.state.enabled);
    assert_eq!(updated.state.protocol, PortlessProtocol::Http);
    assert_eq!(
        updated.state.setup_ownership,
        PortlessSetupOwnership::Ghostex
    );
    assert_eq!(updated.state.setup_status, PortlessSetupStatus::Active);
    assert_eq!(updated.state.runtime_status, PortlessRuntimeStatus::Active);
}

#[test]
fn state_update_disable_clears_routes_without_removing_service() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    let repository = PortlessRepository::new(&db);
    repository
        .upsert_state(portless_state(
            true,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Active,
            PortlessRuntimeStatus::Active,
        ))
        .expect("active state");
    sync_portless_routes(&paths, &[route("clear-on-disable.localhost", 3000, 42)])
        .expect("seed route");

    let updated = apply_portless_state_update(
        &paths,
        &db,
        PortlessStateUpdate::SetEnabled { enabled: false },
    )
    .expect("disable update");

    assert!(!updated.state.enabled);
    assert_eq!(
        updated.state.setup_ownership,
        PortlessSetupOwnership::Ghostex
    );
    assert_eq!(updated.state.setup_status, PortlessSetupStatus::Disabled);
    assert_routes_file(&paths, &[]);
}

#[test]
fn state_update_explicit_remove_service_is_separate_from_disable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    let repository = PortlessRepository::new(&db);
    repository
        .upsert_state(portless_state(
            true,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Active,
            PortlessRuntimeStatus::Active,
        ))
        .expect("active state");
    sync_portless_routes(&paths, &[route("clear-on-remove.localhost", 5173, 77)])
        .expect("seed route");

    let updated = apply_portless_state_update(
        &paths,
        &db,
        PortlessStateUpdate::RecordAdminResult {
            action: PortlessAdminResultAction::Remove,
            ok: true,
            protocol: None,
        },
    )
    .expect("remove admin result");

    assert!(updated.state.enabled);
    assert_eq!(
        updated.state.setup_ownership,
        PortlessSetupOwnership::Missing
    );
    assert_eq!(updated.state.setup_status, PortlessSetupStatus::Needed);
    assert_eq!(
        updated.state.runtime_status,
        PortlessRuntimeStatus::Inactive
    );
    assert_routes_file(&paths, &[]);
}

#[test]
fn persistence_apis_do_not_create_or_require_portless_state_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let portless_state_dir = paths.portless_state_dir.clone();
    assert!(!portless_state_dir.exists());

    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "P3main", "Display Name");
    insert_worktree_project(
        &db,
        "P3wt",
        "P3main",
        "Worktree Project",
        "Feature A",
        "feature/a",
        "2026-06-22T18:42:00.000Z",
    );
    let repository = PortlessRepository::new(&db);
    repository
        .upsert_project_slug("P3main", "metadata-only")
        .expect("project slug");
    repository
        .upsert_worktree_slug("P3main", "P3wt", "feature-a")
        .expect("worktree slug");
    repository
        .upsert_state(PortlessState {
            enabled: true,
            protocol: PortlessProtocol::Https,
            setup_ownership: PortlessSetupOwnership::Unknown,
            setup_status: PortlessSetupStatus::Unknown,
            runtime_status: PortlessRuntimeStatus::Unknown,
        })
        .expect("state");
    repository
        .backfill_domain_identities()
        .expect("backfill identities");
    repository
        .ensure_project_slug("P3main")
        .expect("ensure project slug");
    repository
        .ensure_worktree_slug("P3wt")
        .expect("ensure worktree slug");

    assert!(!portless_state_dir.exists());
    assert!(repository
        .read_project_slug("P3main")
        .expect("read project slug")
        .is_some());
    assert!(repository
        .read_worktree_slug("P3main", "P3wt")
        .expect("read worktree slug")
        .is_some());
    assert!(repository.read_state().expect("read state").is_some());
    assert!(!portless_state_dir.exists());
}

#[test]
fn path_computation_includes_ghostex_managed_portless_state_dir() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

    assert_eq!(
        paths.portless_state_dir,
        temp.path()
            .join(".ghostex")
            .join("gxserver")
            .join("portless")
    );
    assert!(paths.portless_state_dir.starts_with(&paths.root_dir));
}

#[test]
fn ensure_portless_state_dir_creates_writable_directory_under_gxserver_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

    ensure_portless_state_dir(&paths).expect("ensure Portless state dir");

    assert!(paths.portless_state_dir.starts_with(&paths.root_dir));
    assert!(fs::metadata(&paths.portless_state_dir)
        .expect("Portless state dir metadata")
        .is_dir());
    let probe = paths.portless_state_dir.join("probe");
    fs::write(&probe, b"ok").expect("write probe");
    assert_eq!(fs::read(&probe).expect("read probe"), b"ok");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&paths.portless_state_dir)
                .expect("Portless state dir metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
}

#[test]
fn route_sync_uses_routes_lock_and_blocks_when_lock_is_active() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
    fs::create_dir(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK)).expect("lock dir");

    let result = sync_portless_routes_with_options(
        &paths,
        &[route("blocked.localhost", 5173, 42)],
        PortlessRouteSyncOptions {
            lock_timeout: Duration::from_millis(25),
            lock_retry_delay: Duration::from_millis(5),
            stale_lock_age: Duration::from_secs(3600),
        },
    );

    assert!(result.is_err());
    assert!(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK).exists());
    assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
    fs::remove_dir_all(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK))
        .expect("remove test lock");
}

#[test]
fn route_sync_removes_deterministically_stale_routes_lock() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
    fs::create_dir(paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK)).expect("lock dir");

    sync_portless_routes_with_options(
        &paths,
        &[route("fresh.localhost", 3000, 77)],
        PortlessRouteSyncOptions {
            lock_timeout: Duration::from_millis(100),
            lock_retry_delay: Duration::from_millis(1),
            stale_lock_age: Duration::ZERO,
        },
    )
    .expect("sync after stale lock removal");

    assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_LOCK).exists());
    assert_routes_file(&paths, &[route("fresh.localhost", 3000, 77)]);
}

#[test]
fn route_sync_replaces_stale_routes_with_exact_desired_set() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
    fs::write(
        paths.portless_state_dir.join(PORTLESS_ROUTES_FILE),
        serde_json::to_string_pretty(&vec![
            route("old.localhost", 3000, 11),
            route("keep.localhost", 5173, 12),
        ])
        .expect("serialize old routes"),
    )
    .expect("write old routes");

    sync_portless_routes(
        &paths,
        &[
            route("keep.localhost", 5173, 12),
            route("new.keep.localhost", 8080, 13),
        ],
    )
    .expect("sync routes");

    assert_routes_file(
        &paths,
        &[
            route("keep.localhost", 5173, 12),
            route("new.keep.localhost", 8080, 13),
        ],
    );
    assert_no_portless_temp_artifacts(&paths);
}

#[test]
fn route_sync_writes_valid_json_atomically_and_cleans_temp_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    ensure_portless_state_dir(&paths).expect("ensure Portless state dir");
    fs::write(
        paths.portless_state_dir.join(PORTLESS_ROUTES_FILE),
        r#"[{"hostname":"previous.localhost","port":3000,"pid":21}]"#,
    )
    .expect("write previous routes");

    sync_portless_routes(&paths, &[route("atomic.localhost", 5174, 22)]).expect("sync routes");

    let text = fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
        .expect("read routes file");
    let parsed: Vec<PortlessRoute> = serde_json::from_str(&text).expect("valid routes json");
    assert_eq!(parsed, vec![route("atomic.localhost", 5174, 22)]);
    assert!(!text.contains("previous.localhost"));
    assert_no_portless_temp_artifacts(&paths);
}

#[test]
fn route_sync_empty_desired_routes_leaves_empty_valid_routes_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

    sync_portless_routes(&paths, &[route("clear-me.localhost", 3000, 31)]).expect("initial sync");
    sync_portless_routes(&paths, &[]).expect("empty sync");

    let routes_path = paths.portless_state_dir.join(PORTLESS_ROUTES_FILE);
    assert!(routes_path.exists());
    assert_eq!(
        serde_json::from_str::<Vec<PortlessRoute>>(
            &fs::read_to_string(routes_path).expect("read empty routes")
        )
        .expect("parse empty routes"),
        Vec::<PortlessRoute>::new()
    );
    assert_no_portless_temp_artifacts(&paths);
}

#[test]
fn background_sync_policy_clears_routes_when_disabled_regardless_of_setup_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    sync_portless_routes(&paths, &[route("stale.localhost", 3000, 61)])
        .expect("initial stale route");
    let disabled_active_state = portless_state(
        false,
        PortlessSetupOwnership::Ghostex,
        PortlessSetupStatus::Active,
        PortlessRuntimeStatus::Active,
    );

    let outcome = apply_portless_background_sync_policy(
        &paths,
        Some(&disabled_active_state),
        &[route("desired.localhost", 5173, 62)],
        1,
    )
    .expect("apply disabled policy");

    assert_eq!(
        outcome.action,
        PortlessBackgroundRouteAction::ClearMirroredRoutes
    );
    assert_eq!(outcome.status, PortlessBackgroundStatus::Disabled);
    assert_eq!(outcome.desired_route_count, 1);
    assert_routes_file(&paths, &[]);
}

#[test]
fn background_sync_policy_skips_setup_missing_without_writing_desired_routes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let setup_missing_state = portless_state(
        true,
        PortlessSetupOwnership::Missing,
        PortlessSetupStatus::Needed,
        PortlessRuntimeStatus::Inactive,
    );

    let outcome = apply_portless_background_sync_policy(
        &paths,
        Some(&setup_missing_state),
        &[route("missing.localhost", 5173, 71)],
        1,
    )
    .expect("apply missing policy");

    assert_eq!(
        outcome.action,
        PortlessBackgroundRouteAction::SkipRouteFileWrite
    );
    assert_eq!(outcome.status, PortlessBackgroundStatus::SetupNeeded);
    assert_eq!(outcome.desired_route_count, 1);
    assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
}

#[test]
fn background_sync_policy_skips_failed_setup_without_writing_desired_routes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let failed_state = portless_state(
        true,
        PortlessSetupOwnership::Ghostex,
        PortlessSetupStatus::Failed,
        PortlessRuntimeStatus::Failed,
    );

    let outcome = apply_portless_background_sync_policy(
        &paths,
        Some(&failed_state),
        &[route("failed.localhost", 8080, 81)],
        1,
    )
    .expect("apply failed policy");

    assert_eq!(
        outcome.action,
        PortlessBackgroundRouteAction::SkipRouteFileWrite
    );
    assert_eq!(outcome.status, PortlessBackgroundStatus::SetupFailed);
    assert_eq!(outcome.desired_route_count, 1);
    assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
}

#[test]
fn background_sync_policy_skips_non_ghostex_setup_without_writing_desired_routes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let standalone_state = portless_state(
        true,
        PortlessSetupOwnership::Standalone,
        PortlessSetupStatus::Needed,
        PortlessRuntimeStatus::Inactive,
    );

    let outcome = apply_portless_background_sync_policy(
        &paths,
        Some(&standalone_state),
        &[route("standalone.localhost", 3000, 91)],
        1,
    )
    .expect("apply standalone policy");

    assert_eq!(
        outcome.action,
        PortlessBackgroundRouteAction::SkipRouteFileWrite
    );
    assert_eq!(outcome.status, PortlessBackgroundStatus::SetupNeeded);
    assert!(!paths.portless_state_dir.join(PORTLESS_ROUTES_FILE).exists());
}

#[test]
fn background_sync_policy_mirrors_active_ghostex_routes_and_removes_stale_empty_set() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    sync_portless_routes(&paths, &[route("old.localhost", 3000, 101)])
        .expect("initial stale route");
    let active_state = portless_state(
        true,
        PortlessSetupOwnership::Ghostex,
        PortlessSetupStatus::Active,
        PortlessRuntimeStatus::Active,
    );

    let mirrored = apply_portless_background_sync_policy(
        &paths,
        Some(&active_state),
        &[
            route("current.localhost", 5173, 102),
            route("p8080.current.localhost", 8080, 103),
        ],
        2,
    )
    .expect("mirror active routes");

    assert_eq!(
        mirrored.action,
        PortlessBackgroundRouteAction::MirrorDesiredRoutes
    );
    assert_eq!(mirrored.status, PortlessBackgroundStatus::SetupActive);
    assert_routes_file(
        &paths,
        &[
            route("current.localhost", 5173, 102),
            route("p8080.current.localhost", 8080, 103),
        ],
    );

    let emptied = apply_portless_background_sync_policy(&paths, Some(&active_state), &[], 0)
        .expect("mirror empty live route set");

    assert_eq!(
        emptied.action,
        PortlessBackgroundRouteAction::MirrorDesiredRoutes
    );
    assert_eq!(emptied.desired_route_count, 0);
    assert_routes_file(&paths, &[]);
}

#[test]
fn background_sync_once_clears_disabled_state_without_live_listener_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    PortlessRepository::new(&db)
        .upsert_state(portless_state(
            false,
            PortlessSetupOwnership::Missing,
            PortlessSetupStatus::Needed,
            PortlessRuntimeStatus::Inactive,
        ))
        .expect("disabled state");
    sync_portless_routes(&paths, &[route("disabled-stale.localhost", 3000, 111)])
        .expect("initial stale route");

    let outcome = run_portless_background_sync_once(&paths).expect("one-shot sync");

    assert_eq!(
        outcome.action,
        PortlessBackgroundRouteAction::ClearMirroredRoutes
    );
    assert_eq!(outcome.status, PortlessBackgroundStatus::Disabled);
    assert_eq!(outcome.live_listener_count, 0);
    assert_eq!(outcome.desired_route_count, 0);
    assert_routes_file(&paths, &[]);
}

#[test]
fn portless_routine_operational_logs_are_debug_gated_while_warnings_persist() {
    let disabled_temp = tempfile::tempdir().expect("disabled tempdir");
    let disabled_paths = get_gxserver_paths(Some(disabled_temp.path().to_path_buf()));
    let disabled_logger = crate::logging::GxserverLogger::new(disabled_paths.clone());
    let outcome = PortlessBackgroundSyncOutcome {
        action: PortlessBackgroundRouteAction::MirrorDesiredRoutes,
        desired_route_count: 2,
        live_listener_count: 1,
        status: PortlessBackgroundStatus::SetupActive,
    };

    log_portless_background_sync_outcome(&disabled_logger, &outcome, 11);
    assert!(!disabled_paths.log_file.exists());

    log_portless_background_sync_failure(
        &disabled_logger,
        PortlessLogErrorCode::BackgroundSyncFailed,
        12,
    );
    let warning_text = fs::read_to_string(&disabled_paths.log_file).expect("read warning log");
    assert!(warning_text.contains("portless.backgroundSyncFailed"));
    assert!(warning_text.contains("backgroundSyncFailed"));
    assert!(!warning_text.contains("portless.backgroundSync\""));

    let enabled_temp = tempfile::tempdir().expect("enabled tempdir");
    let enabled_paths = get_gxserver_paths(Some(enabled_temp.path().to_path_buf()));
    enable_debugging_mode_for_test(&enabled_paths);
    let enabled_logger = crate::logging::GxserverLogger::new(enabled_paths.clone());
    log_portless_background_sync_outcome(&enabled_logger, &outcome, 13);

    let debug_text = fs::read_to_string(&enabled_paths.log_file).expect("read debug log");
    assert!(debug_text.contains("portless.backgroundSync"));
    assert!(debug_text.contains("\"routeCount\":2"));
    assert!(debug_text.contains("\"liveListenerCount\":1"));
    assert_portless_log_text_has_no_forbidden_raw_values(&debug_text);
}

#[test]
fn portless_state_update_logs_do_not_persist_forbidden_raw_values() {
    /*
    CDXC:PortlessLogging 2026-06-23-04:45:
    Phase 17 tests must prove Portless persisted diagnostics do not carry raw project/worktree names, paths, full URLs, hostnames, command text, env values, tokens, secrets, stdout, or stderr. The log helper accepts only enum/count/boolean/protocol state, so the test scans both success and warning entries for those forbidden values and field names.
    */
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    enable_debugging_mode_for_test(&paths);
    let logger = crate::logging::GxserverLogger::new(paths.clone());
    let update = PortlessStateUpdate::RecordAdminResult {
        action: PortlessAdminResultAction::Reconfigure,
        ok: false,
        protocol: Some(PortlessProtocol::Http),
    };
    let record = PortlessStateRecord {
        state: portless_state(
            true,
            PortlessSetupOwnership::Ghostex,
            PortlessSetupStatus::Failed,
            PortlessRuntimeStatus::Failed,
        ),
        created_at: "2026-06-23T00:45:00.000Z".to_string(),
        updated_at: "2026-06-23T00:45:01.000Z".to_string(),
    };

    log_portless_state_update_success(&logger, &update, &record, 21);
    log_portless_state_update_failure(
        &logger,
        &update,
        PortlessLogErrorCode::StateUpdateFailed,
        22,
    );

    let text = fs::read_to_string(&paths.log_file).expect("read Portless log");
    assert!(text.contains("portless.stateUpdate"));
    assert!(text.contains("portless.stateUpdateFailed"));
    assert!(text.contains("\"protocol\":\"http\""));
    assert!(text.contains("\"setupStatus\":\"failed\""));
    assert!(text.contains("\"errorCode\":\"stateUpdateFailed\""));
    assert_portless_log_text_has_no_forbidden_raw_values(&text);
    for forbidden_field in [
        "hostname",
        "path",
        "url",
        "command",
        "env",
        "token",
        "secret",
        "stdout",
        "stderr",
        "projectName",
        "worktreeName",
    ] {
        assert!(
            !text.contains(forbidden_field),
            "Portless log included forbidden field {forbidden_field}: {text}"
        );
    }
}

#[test]
fn service_inspection_classifies_missing_as_setup_needed_install_state() {
    let expectation =
        service_expectation(Path::new("/Users/ghostex-user"), PortlessProtocol::Https);

    let inspection = inspect_portless_service_from_plist_text(
        None,
        &expectation,
        PortlessServiceReachability::default(),
    )
    .expect("inspect missing service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::Missing
    );
    assert!(state.enabled);
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Missing);
    assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
}

#[test]
fn service_inspection_accepts_escaped_ghostex_plist_as_active() {
    let home = Path::new("/Users/ghostex-user");
    let expectation = service_expectation(home, PortlessProtocol::Https);
    let plist = service_plist(
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
        "~/.ghostex/gxserver/portless",
        443,
        true,
        false,
        false,
        None,
        &["--foreground", "--port", "443", "--https", "--skip-trust"],
    );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(true),
        },
    )
    .expect("inspect Ghostex service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection,
        PortlessServiceInspection {
            classification: PortlessServiceClassification::GhostexActive,
            mismatch_count: 0,
        }
    );
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
    assert_eq!(state.setup_status, PortlessSetupStatus::Active);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Active);
}

#[test]
fn service_inspection_accepts_http_config_when_protocol_setting_is_http() {
    let home = Path::new("/Users/ghostex-user");
    let expectation = service_expectation(home, PortlessProtocol::Http);
    let plist = service_plist(
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
        "/Users/ghostex-user/.ghostex/gxserver/portless",
        80,
        false,
        false,
        false,
        None,
        &["--foreground", "--port", "80", "--no-tls", "--skip-trust"],
    );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(true),
        },
    )
    .expect("inspect HTTP Ghostex service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::GhostexActive
    );
    assert_eq!(inspection.mismatch_count, 0);
    assert_eq!(state.protocol, PortlessProtocol::Http);
    assert_eq!(state.setup_status, PortlessSetupStatus::Active);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Active);
}

#[test]
fn service_inspection_classifies_standalone_service_separately_from_reconfigure() {
    let expectation =
        service_expectation(Path::new("/Users/ghostex-user"), PortlessProtocol::Https);
    let plist = service_plist(
        "/usr/local/bin/node",
        "/usr/local/lib/node_modules/portless/dist/cli.js",
        "/Users/ghostex-user/.portless",
        443,
        true,
        false,
        false,
        None,
        &["--foreground", "--port", "443", "--https", "--skip-trust"],
    );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(true),
        },
    )
    .expect("inspect standalone service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::Standalone
    );
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Standalone);
    assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
}

#[test]
fn service_inspection_classifies_ghostex_config_mismatch_as_reconfigure_needed() {
    let home = Path::new("/Users/ghostex-user");
    let expectation = service_expectation(home, PortlessProtocol::Https);
    let plist = service_plist_with_lan_ip(
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
        "/Users/ghostex-user/.ghostex/gxserver/portless",
        80,
        false,
        true,
        true,
        Some("local"),
        Some("192.168.1.42"),
        &[
            "--foreground",
            "--port",
            "80",
            "--no-tls",
            "--lan",
            "--wildcard",
        ],
    );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(true),
        },
    )
    .expect("inspect mismatch service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::GhostexConfigMismatch
    );
    assert!(inspection.mismatch_count >= 6);
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
    assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
}

#[test]
fn service_inspection_requires_sync_hosts_disabled_for_ghostex_service() {
    let home = Path::new("/Users/ghostex-user");
    let expectation = service_expectation(home, PortlessProtocol::Https);
    let plist = service_plist(
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
        "/Users/ghostex-user/.ghostex/gxserver/portless",
        443,
        true,
        false,
        false,
        None,
        &["--foreground", "--port", "443", "--https", "--skip-trust"],
    )
    .replace(
        "    <key>PORTLESS_SYNC_HOSTS</key>\n    <string>0</string>\n",
        "",
    );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(true),
        },
    )
    .expect("inspect sync-hosts mismatch service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::GhostexConfigMismatch
    );
    assert_eq!(inspection.mismatch_count, 1);
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
    assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
}

#[test]
fn service_inspection_rejects_persistent_launchd_output_paths() {
    let home = Path::new("/Users/ghostex-user");
    let expectation = service_expectation(home, PortlessProtocol::Https);
    let plist = service_plist(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
            "/Users/ghostex-user/.ghostex/gxserver/portless",
            443,
            true,
            false,
            false,
            None,
            &["--foreground", "--port", "443", "--https", "--skip-trust"],
        )
        .replace(
            "  <key>StandardOutPath</key>\n  <string>/dev/null</string>",
            "  <key>StandardOutPath</key>\n  <string>/Users/ghostex-user/.ghostex/gxserver/portless/service.log</string>",
        )
        .replace(
            "  <key>StandardErrorPath</key>\n  <string>/dev/null</string>",
            "  <key>StandardErrorPath</key>\n  <string>/Users/ghostex-user/.ghostex/gxserver/portless/service.log</string>",
        );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(true),
        },
    )
    .expect("inspect persistent output mismatch service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::GhostexConfigMismatch
    );
    assert_eq!(inspection.mismatch_count, 2);
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
    assert_eq!(state.setup_status, PortlessSetupStatus::Needed);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Inactive);
}

#[test]
fn service_inspection_treats_moved_ghostex_binary_as_reconfigure_not_standalone() {
    let home = Path::new("/Users/ghostex-user");
    let expectation = service_expectation(home, PortlessProtocol::Https);
    let plist = service_plist(
        "/Applications/Old Ghostex.app/Contents/Resources/Web/code-server/lib/node",
        "/Applications/Old Ghostex.app/Contents/Resources/Web/portless/dist/cli.js",
        "/Users/ghostex-user/.ghostex/gxserver/portless",
        443,
        true,
        false,
        false,
        None,
        &["--foreground", "--port", "443", "--https", "--skip-trust"],
    );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(true),
        },
    )
    .expect("inspect moved Ghostex service");

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::GhostexConfigMismatch
    );
    assert!(inspection.mismatch_count >= 2);
}

#[test]
fn service_inspection_classifies_unreachable_ghostex_service_as_failed_retry_state() {
    let home = Path::new("/Users/ghostex-user");
    let expectation = service_expectation(home, PortlessProtocol::Https);
    let plist = service_plist(
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
        "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
        "/Users/ghostex-user/.ghostex/gxserver/portless",
        443,
        true,
        false,
        false,
        None,
        &["--foreground", "--port", "443", "--https", "--skip-trust"],
    );

    let inspection = inspect_portless_service_from_plist_text(
        Some(&plist),
        &expectation,
        PortlessServiceReachability {
            manager_running: Some(true),
            proxy_reachable: Some(false),
        },
    )
    .expect("inspect failed service");
    let state = portless_state_for_service_inspection(None, expectation.protocol, &inspection);

    assert_eq!(
        inspection.classification,
        PortlessServiceClassification::GhostexFailed
    );
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
    assert_eq!(state.setup_status, PortlessSetupStatus::Failed);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Failed);
}

#[test]
fn service_detection_preserves_existing_disabled_state_without_reenabling() {
    let expectation =
        service_expectation(Path::new("/Users/ghostex-user"), PortlessProtocol::Https);
    let existing = portless_state(
        false,
        PortlessSetupOwnership::Missing,
        PortlessSetupStatus::Disabled,
        PortlessRuntimeStatus::Inactive,
    );
    let inspection = PortlessServiceInspection {
        classification: PortlessServiceClassification::GhostexActive,
        mismatch_count: 0,
    };

    let state =
        portless_state_for_service_inspection(Some(&existing), expectation.protocol, &inspection);

    assert!(!state.enabled);
    assert_eq!(state.setup_ownership, PortlessSetupOwnership::Ghostex);
    assert_eq!(state.setup_status, PortlessSetupStatus::Disabled);
    assert_eq!(state.runtime_status, PortlessRuntimeStatus::Active);
}

#[test]
fn route_sync_validation_rejects_pid_zero_invalid_hosts_and_invalid_ports() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    sync_portless_routes(&paths, &[route("valid.localhost", 3000, 41)]).expect("initial sync");
    let original = fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
        .expect("read original routes");

    let invalid_routes = [
        route("pid-zero.localhost", 3000, 0),
        route("port-zero.localhost", 0, 42),
        route("", 3000, 42),
        route("https://raw-url.localhost", 3000, 42),
        route("raw-url.localhost/path", 3000, 42),
        route("localhost", 3000, 42),
        route("bad..label.localhost", 3000, 42),
        route("-bad.localhost", 3000, 42),
        route("bad-.localhost", 3000, 42),
        route("BadUpper.localhost", 3000, 42),
        route("wrong.test", 3000, 42),
    ];

    for invalid in invalid_routes {
        assert!(
            sync_portless_routes(&paths, &[invalid]).is_err(),
            "invalid route should be rejected"
        );
        assert_eq!(
            fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
                .expect("read routes after rejected sync"),
            original
        );
    }
}

#[test]
fn desired_routes_single_project_server_uses_project_base_domain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Psingle", "Single App");
    PortlessRepository::new(&db)
        .upsert_project_slug("Psingle", "ghostex")
        .expect("project slug");

    let routes = compute_desired_portless_routes(
        &db,
        &[owned_listener(
            "Psingle",
            "Gdev",
            "S90-Psingle-Gdev",
            None,
            8080,
            81,
        )],
    )
    .expect("desired routes");

    assert_eq!(routes, vec![route("ghostex.localhost", 8080, 81)]);
}

#[test]
fn desired_routes_choose_project_primary_by_port_preference_and_extra_domains() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pmulti", "Multi App");
    PortlessRepository::new(&db)
        .upsert_project_slug("Pmulti", "ghostex")
        .expect("project slug");

    let routes = compute_desired_portless_routes(
        &db,
        &[
            owned_listener("Pmulti", "G8080", "S90-Pmulti-G8080", None, 8080, 80),
            owned_listener("Pmulti", "Glow", "S90-Pmulti-Glow", None, 4000, 40),
            owned_listener("Pmulti", "G5173", "S90-Pmulti-G5173", None, 5173, 51),
        ],
    )
    .expect("desired routes");

    assert_eq!(
        routes,
        vec![
            route("ghostex.localhost", 5173, 51),
            route("p4000.ghostex.localhost", 4000, 40),
            route("p8080.ghostex.localhost", 8080, 80),
        ]
    );
}

#[test]
fn desired_routes_primary_falls_back_to_lowest_port_without_preferred_ports() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pfallback", "Fallback App");
    PortlessRepository::new(&db)
        .upsert_project_slug("Pfallback", "ghostex")
        .expect("project slug");

    let routes = compute_desired_portless_routes(
        &db,
        &[
            owned_listener("Pfallback", "G9000", "S90-Pfallback-G9000", None, 9000, 90),
            owned_listener("Pfallback", "G4242", "S90-Pfallback-G4242", None, 4242, 42),
        ],
    )
    .expect("desired routes");

    assert_eq!(
        routes,
        vec![
            route("ghostex.localhost", 4242, 42),
            route("p9000.ghostex.localhost", 9000, 90),
        ]
    );
}

#[test]
fn desired_routes_worktree_listener_uses_project_and_worktree_base_domain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pparent", "Parent App");
    insert_worktree_project(
        &db,
        "Pwtfix",
        "Pparent",
        "Worktree App",
        "Fix UI",
        "feature/fix-ui",
        "2026-06-22T18:42:00.000Z",
    );
    let repository = PortlessRepository::new(&db);
    repository
        .upsert_project_slug("Pparent", "ghostex")
        .expect("project slug");
    repository
        .upsert_worktree_slug("Pparent", "Pwtfix", "fix-ui")
        .expect("worktree slug");

    let routes = compute_desired_portless_routes(
        &db,
        &[owned_listener(
            "Pwtfix",
            "Gdev",
            "S90-Pwtfix-Gdev",
            Some("Pparent"),
            8080,
            88,
        )],
    )
    .expect("desired routes");

    assert_eq!(routes, vec![route("ghostex.fix-ui.localhost", 8080, 88)]);
}

#[test]
fn desired_routes_worktree_extra_routes_use_port_prefixed_base_domain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pparent", "Parent App");
    insert_worktree_project(
        &db,
        "Pwtfix",
        "Pparent",
        "Worktree App",
        "Fix UI",
        "feature/fix-ui",
        "2026-06-22T18:42:00.000Z",
    );
    let repository = PortlessRepository::new(&db);
    repository
        .upsert_project_slug("Pparent", "ghostex")
        .expect("project slug");
    repository
        .upsert_worktree_slug("Pparent", "Pwtfix", "fix-ui")
        .expect("worktree slug");

    let routes = compute_desired_portless_routes(
        &db,
        &[
            owned_listener(
                "Pwtfix",
                "G8787",
                "S90-Pwtfix-G8787",
                Some("Pparent"),
                8787,
                87,
            ),
            owned_listener(
                "Pwtfix",
                "G5173",
                "S90-Pwtfix-G5173",
                Some("Pparent"),
                5173,
                51,
            ),
            owned_listener(
                "Pwtfix",
                "G3000",
                "S90-Pwtfix-G3000",
                Some("Pparent"),
                3000,
                30,
            ),
        ],
    )
    .expect("desired routes");

    assert_eq!(
        routes,
        vec![
            route("ghostex.fix-ui.localhost", 3000, 30),
            route("p5173.ghostex.fix-ui.localhost", 5173, 51),
            route("p8787.ghostex.fix-ui.localhost", 8787, 87),
        ]
    );
}

#[test]
fn desired_routes_are_temporary_and_tied_to_live_listener_input() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Ptemp", "Temporary App");
    PortlessRepository::new(&db)
        .upsert_project_slug("Ptemp", "ghostex")
        .expect("project slug");
    let primary_listener = owned_listener("Ptemp", "G3000", "S90-Ptemp-G3000", None, 3000, 30);
    let extra_listener = owned_listener("Ptemp", "G5173", "S90-Ptemp-G5173", None, 5173, 51);

    let with_extra =
        compute_desired_portless_routes(&db, &[primary_listener.clone(), extra_listener.clone()])
            .expect("desired routes with extra");
    let without_extra = compute_desired_portless_routes(&db, &[primary_listener])
        .expect("desired routes after removal");

    assert_eq!(
        with_extra,
        vec![
            route("ghostex.localhost", 3000, 30),
            route("p5173.ghostex.localhost", 5173, 51),
        ]
    );
    assert_eq!(without_extra, vec![route("ghostex.localhost", 3000, 30)]);
}

#[test]
fn protocol_status_payload_serializes_only_metadata_and_local_native_action_requirements() {
    /*
    CDXC:PortlessProtocol 2026-06-23-00:25:
    Phase 12 status payloads must tell React which Portless setup action is relevant while keeping all privileged actions unavailable in gxserver metadata. The native sidebar may enable them only for local Mac admin bridge execution.
    */
    let payload = portless_status_payload_from_record(
        Some(PortlessStateRecord {
            state: PortlessState {
                enabled: true,
                protocol: PortlessProtocol::Https,
                setup_ownership: PortlessSetupOwnership::Missing,
                setup_status: PortlessSetupStatus::Needed,
                runtime_status: PortlessRuntimeStatus::Inactive,
            },
            created_at: "2026-06-23T00:25:00.000Z".to_string(),
            updated_at: "2026-06-23T00:25:00.000Z".to_string(),
        }),
        PortlessPayloadSourceStatus::Current,
    );

    let value = serde_json::to_value(&payload).expect("serialize status payload");
    assert_eq!(value["enabled"], true);
    assert_eq!(value["protocol"], "https");
    assert_eq!(value["setupOwnership"], "missing");
    assert_eq!(value["setupStatus"], "needed");
    assert_eq!(value["runtimeStatus"], "inactive");
    assert_eq!(value["sourceStatus"], "current");
    assert_eq!(value["actions"]["install"]["recommended"], true);
    assert_eq!(value["actions"]["install"]["available"], false);
    assert_eq!(value["actions"]["install"]["localMacOnly"], true);
    assert_eq!(
        value["actions"]["install"]["unavailableReason"],
        "nativeAdminBridgeRequired"
    );
    assert_eq!(value["actions"]["reconfigure"]["recommended"], false);
    assert_eq!(value["actions"]["reconfigure"]["available"], false);

    let text = value.to_string();
    for disallowed in [
        "stdout",
        "stderr",
        "token",
        "cookie",
        "env",
        "commandText",
        "filePath",
        "http://",
        "https://",
        "/tmp/",
    ] {
        assert!(
            !text.contains(disallowed),
            "Portless status payload exposed disallowed field/value {disallowed}"
        );
    }
}

#[test]
fn protocol_route_previews_join_desired_routes_to_stable_ids_without_pids_or_urls() {
    /*
    CDXC:PortlessProtocol 2026-06-23-00:25:
    Route previews are presentation metadata, not Portless file contents. Carry protocol, hostname, port, stable project/session ids, and primary/additional kind so UI can render links later without full URLs, pids, raw paths, command text, or process output.
    */
    let listeners = vec![
        owned_listener(
            "Ppreview",
            "Gprimary",
            "S90-Ppreview-Gprimary",
            None,
            3000,
            30,
        ),
        owned_listener(
            "Ppreview",
            "Gadditional",
            "S90-Ppreview-Gadditional",
            None,
            5173,
            51,
        ),
    ];
    let previews = portless_route_previews_for_desired_routes(
        PortlessProtocol::Https,
        &listeners,
        &[
            route("ghostex.localhost", 3000, 30),
            route("p5173.ghostex.localhost", 5173, 51),
        ],
    );

    let value = serde_json::to_value(&previews).expect("serialize route previews");
    assert_eq!(value[0]["hostname"], "ghostex.localhost");
    assert_eq!(value[0]["kind"], "primary");
    assert_eq!(value[0]["port"], 3000);
    assert_eq!(value[0]["projectId"], "Ppreview");
    assert_eq!(value[0]["protocol"], "https");
    assert_eq!(value[0]["sessionId"], "Gprimary");
    assert_eq!(value[1]["hostname"], "p5173.ghostex.localhost");
    assert_eq!(value[1]["kind"], "additional");
    assert_eq!(value[1]["port"], 5173);
    assert_eq!(value[1]["sessionId"], "Gadditional");

    let text = value.to_string();
    for disallowed in [
        "pid",
        "stdout",
        "stderr",
        "token",
        "cookie",
        "env",
        "commandText",
        "http://",
        "https://",
        "/tmp/",
    ] {
        assert!(
            !text.contains(disallowed),
            "Portless route preview exposed disallowed field/value {disallowed}"
        );
    }
}

#[test]
fn presentation_payload_includes_assigned_domains_without_live_listeners() {
    /*
    CDXC:PortlessSettings 2026-06-23-04:02:
    Assigned domains are persisted slug metadata, not a live listener view.
    The Settings UI needs these hostnames for stopped projects/worktrees
    while route previews remain limited to currently detected dev servers.
    */
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Passigned",
        "Assigned Project",
        Some("/tmp/assigned-project"),
        "2026-06-22T18:42:00.000Z",
    );
    insert_worktree_project(
        &db,
        "PassignedWt",
        "Passigned",
        "Assigned Worktree",
        "Feature UI",
        "feature/ui",
        "2026-06-22T18:43:00.000Z",
    );

    let payload = read_portless_presentation_payload(&db);

    assert_eq!(payload.assigned_domains.len(), 2);
    let project = payload
        .assigned_domains
        .iter()
        .find(|domain| domain.project_id == "Passigned")
        .expect("project assigned domain");
    assert_eq!(project.kind, PortlessAssignedDomainKind::Project);
    assert_eq!(project.parent_project_id, None);
    assert!(project.hostname.ends_with(".localhost"));

    let worktree = payload
        .assigned_domains
        .iter()
        .find(|domain| domain.project_id == "PassignedWt")
        .expect("worktree assigned domain");
    assert_eq!(worktree.kind, PortlessAssignedDomainKind::Worktree);
    assert_eq!(worktree.parent_project_id.as_deref(), Some("Passigned"));
    assert!(worktree.hostname.ends_with(".localhost"));
    assert!(payload.route_previews.is_empty());

    let text = serde_json::to_value(&payload.assigned_domains)
        .expect("serialize assigned domains")
        .to_string();
    for disallowed in [
        "stdout",
        "stderr",
        "token",
        "cookie",
        "env",
        "commandText",
        "filePath",
        "http://",
        "https://",
        "/tmp/",
    ] {
        assert!(
            !text.contains(disallowed),
            "Portless assigned domain exposed disallowed field/value {disallowed}"
        );
    }
}

#[test]
fn presentation_payload_marks_disabled_without_running_listener_detection() {
    /*
    CDXC:PortlessProtocol 2026-06-23-00:25:
    Disabled Portless should render as explicit presentation metadata with no route previews and without probing live listeners, because disabled setup is a user-visible state rather than a listener-discovery failure.
    */
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    PortlessRepository::new(&db)
        .upsert_state(PortlessState {
            enabled: false,
            protocol: PortlessProtocol::Https,
            setup_ownership: PortlessSetupOwnership::Ghostex,
            setup_status: PortlessSetupStatus::Disabled,
            runtime_status: PortlessRuntimeStatus::Inactive,
        })
        .expect("disabled state");

    let payload = read_portless_presentation_payload(&db);

    assert_eq!(
        payload.route_preview_status,
        PortlessRoutePreviewStatus::Disabled
    );
    assert_eq!(payload.live_listener_count, 0);
    assert!(payload.route_previews.is_empty());
    assert_eq!(payload.status.setup_status, PortlessSetupStatus::Disabled);
}

#[test]
fn desired_routes_backfill_missing_slugs_with_stable_allocator() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Pparent",
        "Ghostex",
        Some("/tmp/ghostex"),
        "2026-06-22T18:41:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtfix",
        "Pparent",
        "Worktree App",
        "Fix UI",
        "feature/fix-ui",
        "2026-06-22T18:42:00.000Z",
    );
    let listener = owned_listener(
        "Pwtfix",
        "Gdev",
        "S90-Pwtfix-Gdev",
        Some("Pparent"),
        5173,
        73,
    );

    let first = compute_desired_portless_routes(&db, std::slice::from_ref(&listener))
        .expect("first desired routes");
    let repository = PortlessRepository::new(&db);
    assert_eq!(
        repository
            .read_project_slug("Pparent")
            .expect("read project slug")
            .map(|record| record.slug),
        Some("ghostex".to_string())
    );
    assert_eq!(
        repository
            .read_worktree_slug("Pparent", "Pwtfix")
            .expect("read worktree slug")
            .map(|record| record.slug),
        Some("fix-ui".to_string())
    );

    db.execute(
        "UPDATE projects SET name = ?2, path = ?3, updatedAt = ?4 WHERE projectId = ?1",
        params![
            "Pparent",
            "Renamed Parent",
            "/tmp/renamed-parent",
            "2026-06-22T18:45:00.000Z"
        ],
    )
    .expect("rename parent project");
    update_worktree_metadata(
        &db,
        "Pwtfix",
        "Pparent",
        "Renamed Worktree",
        "feature/renamed-worktree",
    );
    let second = compute_desired_portless_routes(&db, &[listener]).expect("second desired routes");

    assert_eq!(first, vec![route("ghostex.fix-ui.localhost", 5173, 73)]);
    assert_eq!(second, first);
    assert!(!paths.portless_state_dir.exists());
}

#[test]
fn owned_listener_detection_maps_manual_dev_server_to_running_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pmanual", "Manual App");
    let zmx_name = "S90-Pmanual-Gmanual";
    insert_session_row(&db, "Pmanual", "Gmanual", zmx_name, "running", "zmx");

    let listeners = compute_portless_owned_listeners_from_snapshot(
        &db,
        "name=S90-Pmanual-Gmanual\tpid=100\tclients=1\tcreated=1\tstart_dir=/private",
        r#"
100 1 /bundle/zmx run S90-Pmanual-Gmanual
101 100 -zsh
220 101 npm run dev
221 220 node vite
"#
        .trim(),
        r#"
p221
cnode
n*:5173
"#
        .trim(),
    )
    .expect("owned listeners");

    assert_eq!(
        listeners,
        vec![owned_listener(
            "Pmanual", "Gmanual", zmx_name, None, 5173, 221
        )]
    );
}

#[test]
fn owned_listener_detection_ignores_external_project_looking_listener() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pexternal", "External App");
    insert_session_row(
        &db,
        "Pexternal",
        "Gterminal",
        "S90-Pexternal-Gterminal",
        "running",
        "zmx",
    );

    let listeners = compute_portless_owned_listeners_from_snapshot(
        &db,
        "name=S90-Pexternal-Gterminal\tpid=100\tclients=1\tcreated=1\tstart_dir=/private",
        r#"
100 1 /bundle/zmx run S90-Pexternal-Gterminal
101 100 -zsh
700 1 /Applications/Visual Studio Code.app/Contents/MacOS/Electron /tmp/Pexternal
701 700 node /tmp/Pexternal/node_modules/vite/bin/vite.js
"#
        .trim(),
        r#"
p701
cPexternal-dev
n127.0.0.1:5173
"#
        .trim(),
    )
    .expect("owned listeners");

    assert_eq!(listeners, Vec::<PortlessOwnedListener>::new());
}

#[test]
fn owned_listener_detection_ignores_sleeping_stopped_and_missing_sessions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pstale", "Stale Sessions");
    insert_session_row(
        &db,
        "Pstale",
        "Gsleep",
        "S90-Pstale-Gsleep",
        "sleeping",
        "zmx",
    );
    insert_session_row(&db, "Pstale", "Gstop", "S90-Pstale-Gstop", "stopped", "zmx");
    insert_session_row(&db, "Pstale", "Gmiss", "S90-Pstale-Gmiss", "missing", "zmx");

    let listeners = compute_portless_owned_listeners_from_snapshot(
        &db,
        r#"
name=S90-Pstale-Gsleep pid=200 clients=1
name=S90-Pstale-Gstop pid=300 clients=1
name=S90-Pstale-Gmiss pid=400 clients=1
"#
        .trim(),
        r#"
200 1 -zsh
201 200 node stale-sleep
300 1 -zsh
301 300 node stale-stop
400 1 -zsh
401 400 node stale-missing
"#
        .trim(),
        r#"
p201
n*:3000
p301
n*:5173
p401
n*:8080
"#
        .trim(),
    )
    .expect("owned listeners");

    assert_eq!(listeners, Vec::<PortlessOwnedListener>::new());
}

#[test]
fn owned_listener_detection_drops_exited_listener_absent_from_current_tree() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project(&db, "Pexit", "Exit App");
    let zmx_name = "S90-Pexit-Gterm";
    insert_session_row(&db, "Pexit", "Gterm", zmx_name, "running", "zmx");
    let zmx_list = "name=S90-Pexit-Gterm pid=500 clients=1";
    let listener_output = r#"
p521
n*:3000
"#
    .trim();

    let first = compute_portless_owned_listeners_from_snapshot(
        &db,
        zmx_list,
        r#"
500 1 /bundle/zmx run S90-Pexit-Gterm
520 500 npm run dev
521 520 node vite
"#
        .trim(),
        listener_output,
    )
    .expect("first owned listeners");
    assert_eq!(
        first,
        vec![owned_listener("Pexit", "Gterm", zmx_name, None, 3000, 521)]
    );

    let current = compute_portless_owned_listeners_from_snapshot(
        &db,
        zmx_list,
        r#"
500 1 /bundle/zmx run S90-Pexit-Gterm
520 500 npm run dev
"#
        .trim(),
        listener_output,
    )
    .expect("current owned listeners");
    assert_eq!(current, Vec::<PortlessOwnedListener>::new());
}

#[test]
fn owned_listener_detection_preserves_worktree_parent_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    insert_project_with_path(
        &db,
        "Pparent",
        "Parent App",
        Some("/tmp/parent-app"),
        "2026-06-22T18:41:00.000Z",
    );
    insert_worktree_project(
        &db,
        "Pwtfix",
        "Pparent",
        "Worktree App",
        "Fix UI",
        "feature/fix-ui",
        "2026-06-22T18:42:00.000Z",
    );
    let zmx_name = "S90-Pwtfix-Gdev";
    insert_session_row(&db, "Pwtfix", "Gdev", zmx_name, "running", "zmx");

    let listeners = compute_portless_owned_listeners_from_snapshot(
        &db,
        "name=S90-Pwtfix-Gdev pid=600 clients=1",
        r#"
600 1 /bundle/zmx run S90-Pwtfix-Gdev
601 600 -zsh
602 601 npm run dev
"#
        .trim(),
        r#"
p602
n[::1]:8080
"#
        .trim(),
    )
    .expect("owned listeners");

    assert_eq!(
        listeners,
        vec![owned_listener(
            "Pwtfix",
            "Gdev",
            zmx_name,
            Some("Pparent"),
            8080,
            602
        )]
    );
}

#[test]
fn listener_parser_accepts_lsof_field_rows_and_rejects_invalid_pids_and_ports() {
    let rows = parse_portless_tcp_listener_rows(
        r#"
p0
n*:3000
p42
cnode
n*:0
n*:65536
n*:5173
n[::1]:8080 (LISTEN)
p999999999999
n*:9999
p43
nTCP 127.0.0.1:8000 (LISTEN)
LISTEN 0 511 127.0.0.1:5174 0.0.0.0:* users:(("node",pid=44,fd=23))
LISTEN 0 511 [::1]:9000 [::]:* users:(("vite",pid=45,fd=24))
LISTEN 0 511 *:0 *:* users:(("bad",pid=46,fd=24))
LISTEN 0 511 *:4242 *:* users:(("bad",pid=0,fd=24))
"#
        .trim(),
    );

    assert_eq!(
        rows,
        vec![
            PortlessTcpListenerRow {
                pid: 42,
                port: 5173
            },
            PortlessTcpListenerRow {
                pid: 42,
                port: 8080
            },
            PortlessTcpListenerRow {
                pid: 43,
                port: 8000
            },
            PortlessTcpListenerRow {
                pid: 44,
                port: 5174
            },
            PortlessTcpListenerRow {
                pid: 45,
                port: 9000
            },
        ]
    );
}

fn insert_project(db: &Connection, project_id: &str, name: &str) {
    insert_project_with_path(
        db,
        project_id,
        name,
        Some(&format!("/tmp/{project_id}")),
        "2026-06-22T18:41:00.000Z",
    );
}

fn insert_project_with_path(
    db: &Connection,
    project_id: &str,
    name: &str,
    path: Option<&str>,
    created_at: &str,
) {
    db.execute(
        r#"
            INSERT INTO projects (projectId, name, path, createdAt, updatedAt)
            VALUES (?1, ?2, ?3, ?4, ?4)
            "#,
        params![project_id, name, path, created_at],
    )
    .expect("insert project");
}

fn insert_worktree_project(
    db: &Connection,
    project_id: &str,
    parent_project_id: &str,
    display_name: &str,
    worktree_name: &str,
    branch: &str,
    created_at: &str,
) {
    db.execute(
        r#"
            INSERT INTO projects (projectId, name, path, worktreeJson, createdAt, updatedAt)
            VALUES (?1, ?2, ?3, ?4, ?5, ?5)
            "#,
        params![
            project_id,
            display_name,
            format!("/tmp/{project_id}"),
            worktree_json(parent_project_id, worktree_name, branch),
            created_at,
        ],
    )
    .expect("insert worktree project");
}

fn insert_session_row(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    lifecycle_state: &str,
    persistence_provider: &str,
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
              launchSettingsJson,
              runtimeSettingsJson,
              completionRulesJson,
              attentionRulesJson,
              notificationRulesJson,
              worktreeJson,
              createdAt,
              updatedAt
            )
            VALUES (?1, ?2, 'terminal', ?3, ?4, ?5, ?6, '{}', ?7, '{}', '{}', '{}', '{}', ?8, ?8)
            "#,
        params![
            project_id,
            session_id,
            format!("Session {session_id}"),
            lifecycle_state,
            serde_json::json!({ "lifecycleState": "exists", "provider": "zmx" }).to_string(),
            zmx_name,
            serde_json::json!({ "sessionPersistenceProvider": persistence_provider }).to_string(),
            "2026-06-22T18:43:00.000Z",
        ],
    )
    .expect("insert session");
}

fn update_worktree_metadata(
    db: &Connection,
    project_id: &str,
    parent_project_id: &str,
    worktree_name: &str,
    branch: &str,
) {
    db.execute(
        r#"
            UPDATE projects
            SET name = ?2,
                worktreeJson = ?3,
                updatedAt = ?4
            WHERE projectId = ?1
            "#,
        params![
            project_id,
            worktree_name,
            worktree_json(parent_project_id, worktree_name, branch),
            "2026-06-22T18:45:00.000Z",
        ],
    )
    .expect("update worktree metadata");
}

fn worktree_json(parent_project_id: &str, worktree_name: &str, branch: &str) -> String {
    serde_json::json!({
        "branch": branch,
        "createdAt": "2026-06-22T18:42:00.000Z",
        "name": worktree_name,
        "parentProjectId": parent_project_id,
        "parentProjectName": "Parent Project",
        "parentProjectPath": "/tmp/parent-project"
    })
    .to_string()
}

fn route(hostname: &str, port: u16, pid: u32) -> PortlessRoute {
    PortlessRoute {
        hostname: hostname.to_string(),
        port,
        pid,
    }
}

fn owned_listener(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    worktree_parent_project_id: Option<&str>,
    port: u16,
    pid: u32,
) -> PortlessOwnedListener {
    PortlessOwnedListener {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        zmx_name: zmx_name.to_string(),
        worktree_parent_project_id: worktree_parent_project_id.map(str::to_string),
        port,
        pid,
    }
}

fn enable_debugging_mode_for_test(paths: &crate::paths::GxserverPaths) {
    let settings_path = paths
        .home_dir
        .join(".ghostex")
        .join("state")
        .join("native-sidebar-settings.json");
    fs::create_dir_all(settings_path.parent().expect("settings parent"))
        .expect("create settings dir");
    fs::write(
            settings_path,
            r#"{"debuggingMode":true,"diagnosticLogging":{"scenarios":{"gxserver.portless":{"enabled":true}},"version":1}}"#,
        )
        .expect("write debugging setting");
}

fn assert_portless_log_text_has_no_forbidden_raw_values(text: &str) {
    for forbidden in [
        "Private Project",
        "Feature Worktree",
        "/Users/person/dev/private-project",
        "https://ghostex.localhost/private?token=SECRET",
        "ghostex.localhost",
        "npm run dev",
        "PORTLESS_STATE_DIR=/Users/person/.ghostex/gxserver/portless",
        "SECRET",
        "stdout payload",
        "stderr payload",
    ] {
        assert!(
            !text.contains(forbidden),
            "Portless log leaked forbidden raw value {forbidden}: {text}"
        );
    }
}

fn service_expectation(home_dir: &Path, protocol: PortlessProtocol) -> PortlessServiceExpectation {
    PortlessServiceExpectation {
        home_dir: home_dir.to_path_buf(),
        expected_node_paths: vec![normalize_path_for_comparison(Path::new(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/code-server/lib/node",
        ))],
        expected_cli_paths: vec![normalize_path_for_comparison(Path::new(
            "/Applications/Ghostex & Dev.app/Contents/Resources/Web/portless/dist/cli.js",
        ))],
        expected_state_dir: normalize_path_for_comparison(
            &home_dir.join(".ghostex").join("gxserver").join("portless"),
        ),
        protocol,
        proxy_port: portless_service_port_for_protocol(protocol),
    }
}

#[allow(clippy::too_many_arguments)]
fn service_plist(
    node: &str,
    cli: &str,
    state_dir: &str,
    port: u16,
    https: bool,
    lan: bool,
    wildcard: bool,
    tld: Option<&str>,
    proxy_args: &[&str],
) -> String {
    service_plist_with_lan_ip(
        node, cli, state_dir, port, https, lan, wildcard, tld, None, proxy_args,
    )
}

#[allow(clippy::too_many_arguments)]
fn service_plist_with_lan_ip(
    node: &str,
    cli: &str,
    state_dir: &str,
    port: u16,
    https: bool,
    lan: bool,
    wildcard: bool,
    tld: Option<&str>,
    lan_ip: Option<&str>,
    proxy_args: &[&str],
) -> String {
    let mut args = vec![
        node.to_string(),
        cli.to_string(),
        "proxy".to_string(),
        "start".to_string(),
    ];
    args.extend(proxy_args.iter().map(|arg| (*arg).to_string()));
    let mut env = vec![
        ("PORTLESS_STATE_DIR", state_dir.to_string()),
        ("PORTLESS_PORT", port.to_string()),
        ("PORTLESS_HTTPS", if https { "1" } else { "0" }.to_string()),
        ("PORTLESS_LAN", if lan { "1" } else { "0" }.to_string()),
        (
            "PORTLESS_WILDCARD",
            if wildcard { "1" } else { "0" }.to_string(),
        ),
        ("PORTLESS_SYNC_HOSTS", "0".to_string()),
    ];
    if let Some(tld) = tld {
        env.push(("PORTLESS_TLD", tld.to_string()));
    }
    if let Some(lan_ip) = lan_ip {
        env.push(("PORTLESS_LAN_IP", lan_ip.to_string()));
    }
    let args_xml = args
        .iter()
        .map(|arg| format!("    <string>{}</string>", test_xml_escape(arg)))
        .collect::<Vec<_>>()
        .join("\n");
    let env_xml = env
        .iter()
        .map(|(key, value)| {
            format!(
                "    <key>{}</key>\n    <string>{}</string>",
                test_xml_escape(key),
                test_xml_escape(value)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{}</string>
  <key>ProgramArguments</key>
  <array>
{}
  </array>
  <key>EnvironmentVariables</key>
  <dict>
{}
  </dict>
  <key>StandardOutPath</key>
  <string>/dev/null</string>
  <key>StandardErrorPath</key>
  <string>/dev/null</string>
</dict>
</plist>
"#,
        PORTLESS_SERVICE_LABEL, args_xml, env_xml
    )
}

fn test_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn portless_state(
    enabled: bool,
    setup_ownership: PortlessSetupOwnership,
    setup_status: PortlessSetupStatus,
    runtime_status: PortlessRuntimeStatus,
) -> PortlessState {
    PortlessState {
        enabled,
        protocol: PortlessProtocol::Https,
        setup_ownership,
        setup_status,
        runtime_status,
    }
}

fn assert_routes_file(paths: &crate::paths::GxserverPaths, expected: &[PortlessRoute]) {
    let text = fs::read_to_string(paths.portless_state_dir.join(PORTLESS_ROUTES_FILE))
        .expect("read routes file");
    let routes: Vec<PortlessRoute> = serde_json::from_str(&text).expect("parse routes file");
    assert_eq!(routes, expected);
}

fn assert_no_portless_temp_artifacts(paths: &crate::paths::GxserverPaths) {
    let names = fs::read_dir(&paths.portless_state_dir)
        .expect("read Portless state dir")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        names
            .iter()
            .all(|name| !name.starts_with(".routes.json.tmp.")),
        "temporary Portless route files should be cleaned up: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name == PORTLESS_ROUTES_LOCK),
        "Portless routes lock should be released"
    );
}

fn project_slug(identities: &PortlessDomainIdentities, project_id: &str) -> String {
    identities
        .projects
        .iter()
        .find(|project| project.project_id == project_id)
        .map(|project| project.slug.clone())
        .expect("project slug")
}

fn worktree_slug(identities: &PortlessDomainIdentities, worktree_project_id: &str) -> String {
    identities
        .worktrees
        .iter()
        .find(|worktree| worktree.worktree_project_id == worktree_project_id)
        .map(|worktree| worktree.worktree_slug.clone())
        .expect("worktree slug")
}

fn sorted_project_slug_pairs(identities: &PortlessDomainIdentities) -> Vec<(String, String)> {
    let mut pairs = identities
        .projects
        .iter()
        .map(|project| (project.project_id.clone(), project.slug.clone()))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
}

fn sorted_worktree_slug_pairs(identities: &PortlessDomainIdentities) -> Vec<(String, String)> {
    let mut pairs = identities
        .worktrees
        .iter()
        .map(|worktree| {
            (
                worktree.worktree_project_id.clone(),
                worktree.worktree_slug.clone(),
            )
        })
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
}
