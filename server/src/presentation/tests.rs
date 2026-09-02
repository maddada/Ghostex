use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::{
    domain::DomainRepository,
    paths::get_gxserver_paths,
    storage::{initialize_gxserver_storage, open_gxserver_database},
};

#[test]
fn snapshot_sorts_projects_and_sessions_for_sidebar_projection() {
    let projects = vec![
        project("P100", "Zulu", false, false),
        project("P101", "Alpha", true, false),
    ];
    let sessions = vec![
        session("P101", "G100", "Later", "running", 2000.0),
        session("P101", "G101", "Earlier", "running", 1000.0),
        session("P101", "G102", "Hidden stopped", "stopped", 0.0),
    ];
    let snapshot = project_snapshot(projects, sessions, 7, true);
    let projects = snapshot
        .get("projects")
        .and_then(Value::as_array)
        .expect("projects");
    assert_eq!(
        projects[0].get("projectId").and_then(Value::as_str),
        Some("P101")
    );
    let groups = snapshot
        .get("groups")
        .and_then(Value::as_array)
        .expect("groups");
    assert_eq!(groups[0].get("sessionIds"), Some(&json!(["G101", "G100"])));
}

#[test]
fn snapshot_publishes_one_cached_git_status_to_every_session_sharing_a_cwd() {
    /*
    CDXC:SidebarV2GitStatus 2026-07-29-00:00:
    The probe is per unique cwd, so a project's terminal and its agent — two
    rows, one checkout — must render the identical card row from the single
    cached answer. Sessions whose cwd was never probed publish no key at all.
    */
    let cwd = "/tmp/ghostex-presentation-git-status/shared-checkout";
    crate::session_git_status::set_cached_session_git_status_for_test(
        cwd,
        Some(crate::session_git_status::SessionGitStatus {
            branch: Some("ghostex/9f8e7d6c".to_string()),
            additions: 41,
            deletions: 3,
            pull_request: Some(crate::session_git_status::SessionPullRequest {
                number: 118,
                state: crate::session_git_status::PullRequestState::Open,
                url: Some("https://github.com/o/r/pull/118".to_string()),
            }),
            updated_at: "2026-07-29T12:00:00.000Z".to_string(),
        }),
    );

    let mut first = session("P400", "G400", "Terminal", "running", 1.0);
    first
        .as_object_mut()
        .expect("session object")
        .insert("cwd".to_string(), json!(cwd));
    let mut second = session("P400", "G401", "Agent", "running", 2.0);
    second
        .as_object_mut()
        .expect("session object")
        .insert("cwd".to_string(), json!(cwd));
    let mut elsewhere = session("P400", "G402", "Elsewhere", "running", 3.0);
    elsewhere.as_object_mut().expect("session object").insert(
        "cwd".to_string(),
        json!("/tmp/ghostex-presentation-git-status/never-probed"),
    );

    let snapshot = project_snapshot(
        vec![project("P400", "Git", false, false)],
        vec![first, second, elsewhere],
        3,
        true,
    );
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions");
    let expected = json!({
        "additions": 41,
        "branch": "ghostex/9f8e7d6c",
        "deletions": 3,
        "prNumber": 118,
        "prState": "open",
        "prUrl": "https://github.com/o/r/pull/118",
        "updatedAt": "2026-07-29T12:00:00.000Z",
    });
    assert_eq!(sessions[0].get("gitStatus"), Some(&expected));
    assert_eq!(sessions[1].get("gitStatus"), Some(&expected));
    assert!(
        sessions[2].get("gitStatus").is_none(),
        "an unprobed cwd publishes no gitStatus key"
    );
}

#[test]
fn snapshot_publishes_git_status_for_sessions_running_in_the_project_root() {
    /*
    CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
    Agent sessions are created WITHOUT a cwd — they run in the project's path,
    which is why every launcher resolves `session.cwd` else `project.path`.
    Presentation must resolve the git-status key the same way or no agent card
    on the machine ever shows a branch. The published `cwd` field stays raw
    (absent here), because V2's worktree logic reads it to tell a managed
    worktree checkout apart from a project-root session.
    */
    let project_path = "/tmp/ghostex-presentation-git-status/project-root";
    crate::session_git_status::set_cached_session_git_status_for_test(
        project_path,
        Some(crate::session_git_status::SessionGitStatus {
            branch: Some("main".to_string()),
            additions: 7,
            deletions: 2,
            pull_request: None,
            updated_at: "2026-07-30T12:00:00.000Z".to_string(),
        }),
    );

    let mut project = project("P401", "Root", false, false);
    project
        .as_object_mut()
        .expect("project object")
        .insert("path".to_string(), json!(project_path));
    // No `cwd` key at all: exactly how createAgentSession stores an agent row.
    let agent = session("P401", "G410", "Agent", "running", 1.0);

    let snapshot = project_snapshot(vec![project], vec![agent], 4, true);
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions");
    assert_eq!(
        sessions[0].get("gitStatus"),
        Some(&json!({
            "additions": 7,
            "branch": "main",
            "deletions": 2,
            "updatedAt": "2026-07-30T12:00:00.000Z",
        }))
    );
    assert!(
        sessions[0].get("cwd").is_none(),
        "the fallback must not invent a published cwd"
    );
}

#[test]
fn snapshot_publishes_session_lifecycle_state_and_capability_flags() {
    /*
    CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
    Sidebar V2 reads settle/snooze from presentation and hides its
    affordances for machines whose snapshot carries no capability object.
    Sessions with no lifecycle state must publish absent keys, not nulls.
    */
    let mut settled = session("P100", "G100", "Settled", "running", 1.0);
    let settled_object = settled.as_object_mut().expect("settled session object");
    settled_object.insert("settledAt".to_string(), json!("2026-07-20T09:00:00.000Z"));
    settled_object.insert("settledOverride".to_string(), json!("settled"));
    settled_object.insert(
        "settledOverrideAt".to_string(),
        json!("2026-07-20T09:00:00.000Z"),
    );
    settled_object.insert("snoozedAt".to_string(), json!("2026-07-21T09:00:00.000Z"));
    settled_object.insert(
        "snoozedUntil".to_string(),
        json!("2026-07-22T09:00:00.000Z"),
    );

    let snapshot = project_snapshot(
        vec![project("P100", "Active", false, false)],
        vec![settled, session("P100", "G101", "Plain", "running", 2.0)],
        7,
        true,
    );

    assert_eq!(
        snapshot.get("capabilities"),
        Some(&json!({
            "sessionGitStatus": true,
            "sessionSettlement": true,
            "sessionSnooze": true,
            "spaces": true,
            "worktreeSessions": true,
        }))
    );
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions");
    let published = &sessions[0];
    assert_eq!(
        published.get("settledAt"),
        Some(&json!("2026-07-20T09:00:00.000Z"))
    );
    assert_eq!(published.get("settledOverride"), Some(&json!("settled")));
    assert_eq!(
        published.get("snoozedAt"),
        Some(&json!("2026-07-21T09:00:00.000Z"))
    );
    assert_eq!(
        published.get("snoozedUntil"),
        Some(&json!("2026-07-22T09:00:00.000Z"))
    );
    assert!(
        published.get("settledOverrideAt").is_none(),
        "the override stamp stays server-internal"
    );
    for key in ["settledAt", "settledOverride", "snoozedAt", "snoozedUntil"] {
        assert!(
            sessions[1].get(key).is_none(),
            "{key} must be absent on a session with no lifecycle state"
        );
    }
}

#[test]
fn snapshot_publishes_the_cached_origin_remote_for_a_project_and_its_worktree_family() {
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
    Sidebar V2 merges the same repository across machines by its `origin`
    remote. A registered worktree project must carry its FAMILY ROOT's
    remote, so the parent and its worktrees land in one logical project; a
    repository with no origin publishes an explicit null; a non-git or
    unprobed path publishes no key at all.
    */
    let root_path = "/tmp/ghostex-presentation-git-remote/repo";
    let plain_path = "/tmp/ghostex-presentation-git-remote/plain";
    crate::project_git_remote::set_cached_project_git_remote_for_test(
        root_path,
        Some(crate::project_git_remote::ProjectGitRemote {
            origin_url: Some("git@github.com:Owner/Repo.git".to_string()),
            repository_root_path: Some(root_path.to_string()),
        }),
    );
    crate::project_git_remote::set_cached_project_git_remote_for_test(
        plain_path,
        Some(crate::project_git_remote::ProjectGitRemote {
            origin_url: None,
            repository_root_path: None,
        }),
    );

    let mut root = project("P500", "Repo", false, false);
    root.as_object_mut()
        .expect("project object")
        .insert("path".to_string(), json!(root_path));
    let mut worktree = project("P501", "Repo worktree", false, false);
    let worktree_object = worktree.as_object_mut().expect("project object");
    worktree_object.insert(
        "path".to_string(),
        json!("/tmp/ghostex-presentation-git-remote/repo-a1b2c3d4"),
    );
    worktree_object.insert(
        "worktree".to_string(),
        json!({
            "branch": "ghostex/a1b2c3d4",
            "parentProjectId": "P500",
            "parentProjectPath": root_path,
        }),
    );
    let mut plain = project("P502", "Notes", false, false);
    plain
        .as_object_mut()
        .expect("project object")
        .insert("path".to_string(), json!(plain_path));
    let mut unprobed = project("P503", "Fresh", false, false);
    unprobed.as_object_mut().expect("project object").insert(
        "path".to_string(),
        json!("/tmp/ghostex-presentation-git-remote/never-probed"),
    );

    let snapshot = project_snapshot(vec![root, worktree, plain, unprobed], Vec::new(), 3, true);
    let projects = snapshot
        .get("projects")
        .and_then(Value::as_array)
        .expect("projects");
    let published = |project_id: &str, key: &str| -> Option<Value> {
        projects
            .iter()
            .find(|project| project.get("projectId").and_then(Value::as_str) == Some(project_id))
            .expect("published project")
            .get(key)
            .cloned()
    };
    assert_eq!(
        published("P500", "gitRemoteOriginUrl"),
        Some(json!("git@github.com:Owner/Repo.git"))
    );
    assert_eq!(
        published("P501", "gitRemoteOriginUrl"),
        Some(json!("git@github.com:Owner/Repo.git")),
        "a registered worktree publishes its family root's remote"
    );
    assert_eq!(
        published("P502", "gitRemoteOriginUrl"),
        Some(Value::Null),
        "a repository with no origin publishes an explicit null"
    );
    assert_eq!(
        published("P503", "gitRemoteOriginUrl"),
        None,
        "an unprobed path publishes no gitRemoteOriginUrl key"
    );

    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29 (P5 fix round):
    The repository root rides the same cache entry, so it must follow the
    same family rule as the remote and be omitted — never null — wherever
    the probe has no answer.
    */
    assert_eq!(
        published("P500", "gitRepositoryRootPath"),
        Some(json!(root_path))
    );
    assert_eq!(
        published("P501", "gitRepositoryRootPath"),
        Some(json!(root_path)),
        "a registered worktree publishes its family root's repository root"
    );
    assert_eq!(
        published("P502", "gitRepositoryRootPath"),
        None,
        "a probe with no resolved root publishes no key, not a null"
    );
    assert_eq!(published("P503", "gitRepositoryRootPath"), None);
}

#[test]
fn snapshot_publishes_the_auto_settle_window_this_daemon_sweeps_with() {
    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29-00:00:
    One sidebar renders rows from several daemons, so each snapshot states
    the window THAT daemon applies. The published value comes from
    `read_sweep_auto_settle_after_days` — the same function the auto-settle
    sweep calls — so the advertised window and the applied window are one
    rule, not two.
    */
    let (temp, db) = open_test_database();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let settings_dir = paths.app_config_dir.clone();
    let settings_file = settings_dir.join("native-sidebar-settings.json");

    let published_window = |db: &Connection| -> Value {
        let sessions = DomainRepository::new(db, "S7k")
            .list_sessions(None)
            .expect("sessions");
        let snapshot = read_presentation_snapshot(
            db,
            "S7k",
            crate::session_lifecycle::read_sweep_auto_settle_after_days(&paths),
            crate::session_lifecycle::read_sidebar_v2_selected(&paths),
            sessions,
        )
        .expect("snapshot");
        snapshot
            .get("autoSettleAfterDays")
            .cloned()
            .expect("autoSettleAfterDays is always published")
    };

    assert_eq!(
        published_window(&db),
        Value::Null,
        "a machine with no settings file is a V1 machine and settles nothing"
    );

    std::fs::create_dir_all(&settings_dir).expect("settings dir");
    for (settings, expected) in [
        (json!({ "sidebarVersion": "v1" }), Value::Null),
        (
            json!({ "sidebarVersion": "v1", "sidebarAutoSettleAfterDays": 7 }),
            Value::Null,
        ),
        (json!({ "sidebarVersion": "v2" }), json!(3)),
        (
            json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 7 }),
            json!(7),
        ),
        (
            json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 1.5 }),
            json!(1.5),
        ),
        (
            json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": Value::Null }),
            Value::Null,
        ),
        (
            json!({ "sidebarVersion": "v2", "sidebarAutoSettleAfterDays": 0 }),
            Value::Null,
        ),
    ] {
        std::fs::write(&settings_file, settings.to_string()).expect("settings file");
        assert_eq!(
            published_window(&db),
            expected,
            "settings {settings} must publish {expected}"
        );
    }
}

/*
CDXC:SidebarV2DataGate 2026-07-29:
The capability tells the truth about what this daemon will actually produce.
With Sidebar V1 selected the git/`gh` probes do not run, so `sessionGitStatus`
is false and a V2 client (local or remote) renders those cards exactly as it
does for a daemon too old to probe — instead of waiting forever on data that
is not coming. Settle/snooze and the worktree endpoints are unaffected: those
RPCs are served regardless of which sidebar this machine renders.
*/
#[test]
fn the_git_status_capability_follows_the_sidebar_version_gate() {
    let (temp, db) = open_test_database();
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let settings_dir = paths.app_config_dir.clone();
    let settings_file = settings_dir.join("native-sidebar-settings.json");
    std::fs::create_dir_all(&settings_dir).expect("settings dir");

    let published_capabilities = |db: &Connection| -> Value {
        let sessions = DomainRepository::new(db, "S7m")
            .list_sessions(None)
            .expect("sessions");
        read_presentation_snapshot(
            db,
            "S7m",
            crate::session_lifecycle::read_sweep_auto_settle_after_days(&paths),
            crate::session_lifecycle::read_sidebar_v2_selected(&paths),
            sessions,
        )
        .expect("snapshot")
        .get("capabilities")
        .cloned()
        .expect("capabilities are always published")
    };

    for (settings, expected_git_status) in [
        (json!({ "sidebarVersion": "v1" }), false),
        (json!({ "sidebarVersion": "v2" }), true),
    ] {
        std::fs::write(&settings_file, settings.to_string()).expect("settings file");
        assert_eq!(
            published_capabilities(&db),
            json!({
                "sessionGitStatus": expected_git_status,
                "sessionSettlement": true,
                "sessionSnooze": true,
                "spaces": true,
                "worktreeSessions": true,
            }),
            "settings {settings} must advertise sessionGitStatus {expected_git_status} \
             and leave the lifecycle/worktree capabilities alone"
        );
    }
}

#[test]
fn snapshot_omits_recent_and_hidden_system_projects() {
    /*
    CDXC:ProjectVisibility 2026-06-30-21:23:
    Project presentation is the shared active inventory contract. Recent Projects and Remote Attach carrier projects must stay out of presentation snapshots so React Native Android does not show closed workspaces or remote-attach containers as selectable projects.
    */
    let mut recent = project("P200", "Closed", false, false);
    recent
        .as_object_mut()
        .expect("recent project object")
        .insert("isRecentProject".to_string(), Value::Bool(true));
    let mut carrier = project("P201", "Remote Attach", false, false);
    let carrier_object = carrier.as_object_mut().expect("carrier project object");
    carrier_object.insert("systemKind".to_string(), json!("remoteAttachCarrier"));
    carrier_object.insert("visibility".to_string(), json!("hidden"));
    let snapshot = project_snapshot(
        vec![project("P100", "Active", false, false), recent, carrier],
        vec![
            session("P100", "G100", "Visible", "running", 1.0),
            session("P200", "G200", "Recent hidden", "running", 1.0),
            session("P201", "G201", "Carrier hidden", "running", 1.0),
        ],
        7,
        true,
    );
    let projects = snapshot
        .get("projects")
        .and_then(Value::as_array)
        .expect("projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(
        projects[0].get("projectId").and_then(Value::as_str),
        Some("P100")
    );
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].get("sessionId").and_then(Value::as_str),
        Some("G100")
    );
}

#[test]
fn snapshot_sorts_sessions_with_sidebar_order_before_absent_order() {
    let projects = vec![project("P100", "Manual Order", false, false)];
    let ordered_later = session("P100", "G200", "Saved later", "running", 1000.0);
    let ordered_new = session("P100", "G100", "New default", "running", 0.0);
    let mut absent = session("P100", "G300", "No saved order", "running", 500.0);
    absent
        .as_object_mut()
        .expect("session object")
        .remove("sidebarOrder");

    let snapshot = project_snapshot(projects, vec![ordered_later, absent, ordered_new], 7, true);
    let groups = snapshot
        .get("groups")
        .and_then(Value::as_array)
        .expect("groups");
    assert_eq!(
        groups[0].get("sessionIds"),
        Some(&json!(["G100", "G200", "G300"]))
    );
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions");
    assert_eq!(sessions[0].get("sidebarOrder"), Some(&json!(0.0)));
    assert_eq!(sessions[1].get("sidebarOrder"), Some(&json!(1000.0)));
    assert!(sessions[2].get("sidebarOrder").is_none());
}

#[test]
fn snapshot_projects_provider_actions_observation_and_tooltip_like_typescript() {
    let mut project = project("P100", "Projection", false, false);
    project
        .as_object_mut()
        .expect("project object")
        .insert("path".to_string(), json!("/workspace/projection"));
    let mut provider_missing = session("P100", "G100", "Missing Provider", "running", 0.0);
    provider_missing
        .as_object_mut()
        .expect("session object")
        .insert("agentId".to_string(), json!("codex"));
    provider_missing
        .as_object_mut()
        .expect("session object")
        .insert("commandId".to_string(), json!("build"));
    provider_missing
        .as_object_mut()
        .expect("session object")
        .insert("cwd".to_string(), json!("/workspace/projection"));
    provider_missing
        .as_object_mut()
        .expect("session object")
        .insert(
            "runtimeSettings".to_string(),
            json!({
                "zmxTitleObservation": {
                    "failureCount": 2.9,
                    "lastFailedAt": "2026-06-07T00:29:59.000Z",
                    "lastObservedAt": "2026-06-07T00:29:40.000Z",
                    "lastStartedAt": "2026-06-07T00:29:58.000Z",
                    "nextRetryAt": "2026-06-07T00:30:00.000Z",
                    "rawTitle": "private terminal title",
                    "status": "retrying"
                }
            }),
        );
    let mut provider_unknown = session("P100", "G101", "Unknown Provider", "running", 1000.0);
    provider_unknown
        .as_object_mut()
        .expect("session object")
        .remove("providerState");
    let mut provider_off = session("P100", "G102", "Provider Off", "running", 2000.0);
    provider_off
        .as_object_mut()
        .expect("session object")
        .insert(
            "runtimeSettings".to_string(),
            json!({ "sessionPersistenceProvider": "off" }),
        );

    let snapshot = project_snapshot(
        vec![project],
        vec![provider_missing, provider_unknown, provider_off],
        7,
        true,
    );
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions");
    let missing = sessions
        .iter()
        .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("G100"))
        .expect("missing provider row");
    assert_eq!(
        missing.get("providerSessionState").and_then(Value::as_str),
        Some("missing")
    );
    assert_eq!(
        missing.get("commandId").and_then(Value::as_str),
        Some("build")
    );
    assert_eq!(
        missing
            .get("sessionPersistenceProvider")
            .and_then(Value::as_str),
        Some("zmx")
    );
    assert_eq!(
        missing
            .get("actions")
            .and_then(Value::as_object)
            .and_then(|actions| actions.get("attach"))
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        missing.get("titleObservation"),
        Some(&json!({
            "failureCount": 2,
            "lastFailedAt": "2026-06-07T00:29:59.000Z",
            "lastObservedAt": "2026-06-07T00:29:40.000Z",
            "lastStartedAt": "2026-06-07T00:29:58.000Z",
            "nextRetryAt": "2026-06-07T00:30:00.000Z",
            "status": "retrying"
        }))
    );
    assert_eq!(
        missing.get("tooltip").and_then(Value::as_str),
        Some("Missing Provider - Projection - /workspace/projection - codex - build")
    );
    assert!(!serde_json::to_string(missing)
        .expect("serialize row")
        .contains("private terminal title"));

    let unknown = sessions
        .iter()
        .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("G101"))
        .expect("unknown provider row");
    assert_eq!(
        unknown.get("providerSessionState").and_then(Value::as_str),
        Some("unknown")
    );
    assert_eq!(
        unknown
            .get("actions")
            .and_then(Value::as_object)
            .and_then(|actions| actions.get("attach"))
            .and_then(Value::as_bool),
        Some(true)
    );

    let off = sessions
        .iter()
        .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("G102"))
        .expect("provider off row");
    assert_eq!(
        off.get("providerSessionState").and_then(Value::as_str),
        Some("persistence-disabled")
    );
    assert_eq!(
        off.get("sessionPersistenceProvider")
            .and_then(Value::as_str),
        None
    );
    assert_eq!(
        off.get("actions")
            .and_then(Value::as_object)
            .and_then(|actions| actions.get("attach"))
            .and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn snapshot_caps_stopped_kept_sessions_and_uses_truthy_session_tags() {
    let projects = vec![project("P100", "Stopped Cap", false, false)];
    let active = session("P100", "Gactive", "Active", "running", 1000.0);
    let mut sessions = vec![active];
    for index in 0..25 {
        let mut stopped = session(
            "P100",
            &format!("G{index:02}"),
            &format!("Stopped {index:02}"),
            "stopped",
            index as f64,
        );
        stopped
            .as_object_mut()
            .expect("stopped session")
            .insert("isPinned".to_string(), Value::Bool(true));
        sessions.push(stopped);
    }
    let mut null_tag = session("P100", "Gnull", "Null Tag", "stopped", 40.0);
    null_tag
        .as_object_mut()
        .expect("null tag")
        .insert("sessionTag".to_string(), Value::Null);
    sessions.push(null_tag);
    let mut empty_tag = session("P100", "Gempty", "Empty Tag", "running", 2000.0);
    empty_tag
        .as_object_mut()
        .expect("empty tag")
        .insert("sessionTag".to_string(), Value::String(String::new()));
    empty_tag
        .as_object_mut()
        .expect("empty tag")
        .insert("agentId".to_string(), Value::String(String::new()));
    empty_tag
        .as_object_mut()
        .expect("empty tag")
        .insert("cwd".to_string(), Value::String(String::new()));
    empty_tag
        .as_object_mut()
        .expect("empty tag")
        .insert("sidebarOrder".to_string(), Value::Null);
    sessions.push(empty_tag);

    let snapshot = project_snapshot(projects, sessions, 7, true);
    let projected = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .expect("sessions");
    let ids = projected
        .iter()
        .filter_map(|session| session.get("sessionId").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 22);
    assert!(ids.contains(&"Gactive"));
    assert!(ids.contains(&"Gempty"));
    assert!(ids.contains(&"G00"));
    assert!(ids.contains(&"G19"));
    assert!(!ids.contains(&"G20"));
    assert!(!ids.contains(&"Gnull"));
    let empty_projected = projected
        .iter()
        .find(|session| session.get("sessionId").and_then(Value::as_str) == Some("Gempty"))
        .expect("empty tag active row");
    assert!(empty_projected.get("sessionTag").is_none());
    assert!(empty_projected.get("agentId").is_none());
    assert!(empty_projected.get("agentIcon").is_none());
    assert!(empty_projected.get("cwd").is_none());
    assert!(empty_projected.get("subtitle").is_none());
    assert_eq!(empty_projected.get("sidebarOrder"), Some(&Value::Null));
}

#[test]
fn search_matches_case_insensitive_project_text_and_paginates() {
    let projects = vec![project("P100", "Search Project", false, false)];
    let sessions = vec![
        session("P100", "G100", "First", "running", 1000.0),
        session("P100", "G101", "Second", "running", 2000.0),
    ];
    let params = json!({
        "limit": 1,
        "query": "search project",
    });
    let result = search_sessions(
        projects,
        sessions,
        params.as_object().expect("params object"),
    )
    .expect("search sessions");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(result.get("cursor").and_then(Value::as_str), Some("1"));
    assert_eq!(
        results[0]
            .get("match")
            .and_then(Value::as_object)
            .and_then(|matched| matched.get("field"))
            .and_then(Value::as_str),
        Some("project")
    );
}

#[test]
fn list_previous_sessions_reads_domain_rows_with_closed_at() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "S7k");
    let project = repository
        .create_project(
            json!({
                "name": "History",
                "path": std::env::temp_dir(),
            })
            .as_object()
            .expect("project params"),
        )
        .expect("project created");
    let project_id = string_field(&project, "projectId").expect("project id");
    let session = repository
        .create_session(
            json!({
                "agentId": "codex",
                "kind": "agent",
                "lifecycleState": "stopped",
                "projectId": project_id,
                "providerState": {
                    "lifecycleState": "missing",
                    "probedAt": "2026-06-06T12:00:00.000Z",
                    "provider": "zmx",
                },
                "runtimeSettings": {
                    "titleSource": "terminal-auto",
                },
                "title": "Restorable session",
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("session created");

    let result = list_previous_sessions(&db, "S7k", &Map::new()).expect("previous sessions");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("sessionId"), session.get("sessionId"));
    assert_eq!(
        results[0].get("closedAt").and_then(Value::as_str),
        Some("2026-06-06T12:00:00.000Z")
    );
}

#[test]
fn previous_sessions_filter_candidates_and_return_closed_at() {
    let projects = vec![project("P100", "History", false, false)];
    let trusted = previous_session(
        "G100",
        "Trusted title",
        "stopped",
        "workspace",
        Some("2026-06-06T12:00:00.000Z"),
        "2026-06-06T12:30:00.000Z",
        "2026-06-01T09:00:00.000Z",
    );
    let placeholder = previous_session(
        "G101",
        "Search by Text",
        "stopped",
        "workspace",
        Some("2026-06-07T12:00:00.000Z"),
        "2026-06-07T12:30:00.000Z",
        "2026-06-07T09:00:00.000Z",
    );
    let mut favorite_placeholder = previous_session(
        "G102",
        "Search by Text",
        "stopped",
        "workspace",
        None,
        "2026-06-05T12:30:00.000Z",
        "2026-06-05T09:00:00.000Z",
    );
    favorite_placeholder
        .as_object_mut()
        .expect("favorite object")
        .insert(
            "sessionTag".to_string(),
            Value::String("favorite".to_string()),
        );
    let mut command_pinned = previous_session(
        "G103",
        "Pinned command",
        "stopped",
        "commands",
        Some("2026-06-08T12:00:00.000Z"),
        "2026-06-08T12:30:00.000Z",
        "2026-06-08T09:00:00.000Z",
    );
    command_pinned
        .as_object_mut()
        .expect("command object")
        .insert("isPinned".to_string(), Value::Bool(true));
    let running = previous_session(
        "G104",
        "Running",
        "running",
        "workspace",
        Some("2026-06-09T12:00:00.000Z"),
        "2026-06-09T12:30:00.000Z",
        "2026-06-09T09:00:00.000Z",
    );

    let result = search_previous_sessions(
        projects,
        vec![
            trusted,
            placeholder,
            favorite_placeholder,
            command_pinned,
            running,
        ],
        &Map::new(),
    )
    .expect("previous sessions");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");

    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec!["G100", "G102"]
    );
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.get("closedAt").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec!["2026-06-06T12:00:00.000Z", "2026-06-05T12:30:00.000Z"]
    );

    let query_params = json!({ "query": "09:00:00.000Z" });
    let query_result = search_previous_sessions(
        vec![project("P100", "History", false, false)],
        vec![
            previous_session(
                "G100",
                "Trusted title",
                "stopped",
                "workspace",
                Some("2026-06-06T12:00:00.000Z"),
                "2026-06-06T12:30:00.000Z",
                "2026-06-01T09:00:00.000Z",
            ),
            previous_session(
                "G102",
                "Favorite title",
                "stopped",
                "workspace",
                None,
                "2026-06-05T12:30:00.000Z",
                "2026-06-05T10:00:00.000Z",
            ),
        ],
        query_params.as_object().expect("query params"),
    )
    .expect("previous session query");
    let query_results = query_result
        .get("results")
        .and_then(Value::as_array)
        .expect("query results");
    assert_eq!(query_results.len(), 1);
    assert_eq!(
        query_results[0].get("sessionId").and_then(Value::as_str),
        Some("G100")
    );
    assert_eq!(
        query_results[0]
            .get("match")
            .and_then(Value::as_object)
            .and_then(|matched| matched.get("field"))
            .and_then(Value::as_str),
        Some("timestamp")
    );
}

#[test]
fn previous_sessions_rank_by_close_time_then_session_id() {
    let projects = vec![project("P100", "History", false, false)];
    let closed_recent = previous_session(
        "G1close",
        "Closed recently",
        "stopped",
        "workspace",
        Some("2026-06-06T12:00:00.000Z"),
        "2026-06-06T12:00:00.000Z",
        "2026-06-01T09:00:00.000Z",
    );
    let active_before_close = previous_session(
        "G2active",
        "Active before close",
        "stopped",
        "workspace",
        Some("2026-06-05T12:00:00.000Z"),
        "2026-06-05T12:00:00.000Z",
        "2026-06-07T09:00:00.000Z",
    );
    let metadata_edited = previous_session(
        "G3meta",
        "Metadata edited after close",
        "stopped",
        "workspace",
        Some("2026-06-04T12:00:00.000Z"),
        "2026-06-08T12:00:00.000Z",
        "2026-06-04T09:00:00.000Z",
    );
    let same_time_later_id = previous_session(
        "G9same",
        "Same close later id",
        "stopped",
        "workspace",
        Some("2026-06-06T12:00:00.000Z"),
        "2026-06-06T12:00:00.000Z",
        "2026-06-06T09:00:00.000Z",
    );
    let same_time_earlier_id = previous_session(
        "G0same",
        "Same close earlier id",
        "stopped",
        "workspace",
        Some("2026-06-06T12:00:00.000Z"),
        "2026-06-06T12:00:00.000Z",
        "2026-06-06T09:00:00.000Z",
    );

    let result = search_previous_sessions(
        projects,
        vec![
            active_before_close,
            metadata_edited,
            same_time_later_id,
            closed_recent,
            same_time_earlier_id,
        ],
        &Map::new(),
    )
    .expect("previous sessions");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");

    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec!["G0same", "G1close", "G9same", "G2active", "G3meta"]
    );
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.get("closedAt").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec![
            "2026-06-06T12:00:00.000Z",
            "2026-06-06T12:00:00.000Z",
            "2026-06-06T12:00:00.000Z",
            "2026-06-05T12:00:00.000Z",
            "2026-06-04T12:00:00.000Z"
        ]
    );
}

#[test]
fn search_normalizes_unicode_query_project_id_and_bad_tags_like_typescript() {
    let projects = vec![
        project("P100", "Unicode", false, false),
        project("P200", "Other", false, false),
    ];
    let sessions = vec![
        session("P100", "G100", "\u{00dc}ber Build", "running", 1000.0),
        session("P200", "G200", "Other Build", "running", 2000.0),
    ];

    let params = json!({
        "projectId": "",
        "query": "\u{00fc}ber",
    });
    let result = search_sessions(
        projects.clone(),
        sessions.clone(),
        params.as_object().expect("params object"),
    )
    .expect("unicode search");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("sessionId").and_then(Value::as_str),
        Some("G100")
    );

    let truthy_project_id = json!({ "projectId": true });
    let result = search_sessions(
        projects.clone(),
        sessions.clone(),
        truthy_project_id.as_object().expect("params object"),
    )
    .expect("truthy project id search");
    assert_eq!(
        result
            .get("results")
            .and_then(Value::as_array)
            .expect("results")
            .len(),
        0
    );

    let bad_tags = json!({ "sessionTags": "favorite" });
    let error = search_sessions(
        projects,
        sessions,
        bad_tags.as_object().expect("params object"),
    )
    .expect_err("bad sessionTags should match TypeScript internal error");
    assert_eq!(error.code, "internalError");
    assert_eq!(error.message, "values?.filter is not a function");
}

#[test]
fn search_includes_untagged_sessions_when_untagged_filter_is_selected() {
    let projects = vec![project("P100", "Tags", false, false)];
    let mut tagged = session("P100", "G-tagged", "Tagged", "running", 1000.0);
    tagged
        .as_object_mut()
        .expect("session object")
        .insert("sessionTag".to_string(), json!("in-progress"));
    let untagged = session("P100", "G-untagged", "Untagged", "running", 2000.0);
    let mut favorite = session("P100", "G-favorite", "Favorite", "running", 3000.0);
    favorite
        .as_object_mut()
        .expect("session object")
        .insert("isFavorite".to_string(), json!(true));
    let sessions = vec![tagged, untagged, favorite];

    let result = search_sessions(
        projects.clone(),
        sessions.clone(),
        json!({ "sessionTags": ["untagged"] })
            .as_object()
            .expect("params object"),
    )
    .expect("untagged search");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].get("sessionId").and_then(Value::as_str),
        Some("G-untagged")
    );

    let mixed_result = search_sessions(
        projects,
        sessions,
        json!({ "sessionTags": ["untagged", "in-progress"] })
            .as_object()
            .expect("params object"),
    )
    .expect("mixed tag search");
    let mixed_ids: Vec<&str> = mixed_result
        .get("results")
        .and_then(Value::as_array)
        .expect("results")
        .iter()
        .filter_map(|row| row.get("sessionId").and_then(Value::as_str))
        .collect();
    assert_eq!(mixed_ids.len(), 2);
    assert!(mixed_ids.contains(&"G-untagged"));
    assert!(mixed_ids.contains(&"G-tagged"));
    assert!(!mixed_ids.contains(&"G-favorite"));
}

#[test]
fn search_does_not_treat_provider_off_rows_as_active() {
    let projects = vec![project("P100", "Provider Off", false, false)];
    let mut provider_off = session("P100", "G100", "Provider off", "unknown", 1000.0);
    provider_off
        .as_object_mut()
        .expect("session object")
        .insert(
            "providerState".to_string(),
            json!({ "lifecycleState": "exists", "provider": "zmx" }),
        );
    provider_off
        .as_object_mut()
        .expect("session object")
        .insert(
            "runtimeSettings".to_string(),
            json!({ "sessionPersistenceProvider": "off" }),
        );
    let params = json!({
        "includeActive": true,
        "includePrevious": false,
    });

    let result = search_sessions(
        projects,
        vec![provider_off],
        params.as_object().expect("params object"),
    )
    .expect("provider off search");
    assert_eq!(
        result
            .get("results")
            .and_then(Value::as_array)
            .expect("results")
            .len(),
        0
    );
}

#[test]
fn previous_sessions_reject_non_restorable_title_noise() {
    let projects = vec![project("P100", "History", false, false)];
    let trusted = previous_session(
        "G100",
        "Trusted title",
        "stopped",
        "workspace",
        Some("2026-06-06T12:00:00.000Z"),
        "2026-06-06T12:30:00.000Z",
        "2026-06-01T09:00:00.000Z",
    );
    let path_title = previous_session(
        "G101",
        "/Users/madda/private",
        "stopped",
        "workspace",
        Some("2026-06-07T12:00:00.000Z"),
        "2026-06-07T12:30:00.000Z",
        "2026-06-07T09:00:00.000Z",
    );
    let command_title = previous_session(
        "G102",
        "codex resume 019e7f01-8243-7aa1-88db-dd84ebcf6aa4",
        "stopped",
        "workspace",
        Some("2026-06-08T12:00:00.000Z"),
        "2026-06-08T12:30:00.000Z",
        "2026-06-08T09:00:00.000Z",
    );
    let generic_title = previous_session(
        "G103",
        "Terminal Session",
        "stopped",
        "workspace",
        Some("2026-06-09T12:00:00.000Z"),
        "2026-06-09T12:30:00.000Z",
        "2026-06-09T09:00:00.000Z",
    );
    let gx_id_title = previous_session(
        "G104",
        "G1abc",
        "stopped",
        "workspace",
        Some("2026-06-10T12:00:00.000Z"),
        "2026-06-10T12:30:00.000Z",
        "2026-06-10T09:00:00.000Z",
    );

    let result = search_previous_sessions(
        projects,
        vec![
            trusted,
            path_title,
            command_title,
            generic_title,
            gx_id_title,
        ],
        &Map::new(),
    )
    .expect("previous sessions");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
            .collect::<Vec<_>>(),
        vec!["G100"]
    );
}

#[test]
fn search_result_title_projection_matches_generic_type_script_rows() {
    let projects = vec![project("P100", "Titles", false, false)];
    let sessions = vec![session(
        "P100",
        "G100",
        "Terminal Session",
        "running",
        1000.0,
    )];

    let result = search_sessions(projects, sessions, &Map::new()).expect("search sessions");
    let results = result
        .get("results")
        .and_then(Value::as_array)
        .expect("results");
    let row = results.first().expect("search row");
    assert_eq!(row.get("titleSource").and_then(Value::as_str), Some("user"));
    assert_eq!(
        row.get("isTemporaryTitle").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(row.get("trustedResumeTitle"), None);
    assert_eq!(
        row.get("displayTitle").and_then(Value::as_str),
        Some("\u{2217} Terminal Session")
    );
    assert_eq!(
        row.get("displayTitleTooltip").and_then(Value::as_str),
        Some("\u{2217} Terminal Session (Unsynced title)")
    );
}

#[test]
fn title_projection_strips_factory_droid_status_marker_from_stored_title() {
    let mut session = session("P100", "G100", "\u{26ec} New Session", "running", 1000.0);
    let session_object = session.as_object_mut().expect("session object");
    session_object.insert("agentId".to_string(), json!("droid"));
    session_object.insert(
        "runtimeSettings".to_string(),
        json!({
            "agentName": "factory droid",
            "titleSource": "terminal-auto"
        }),
    );

    let projection = project_session_title_projection(&session);

    assert_eq!(
        projection.get("displayTitle").and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection
            .get("displayTitleTooltip")
            .and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection.get("primaryTitle").and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection.get("trustedResumeTitle").and_then(Value::as_str),
        Some("New Session")
    );
    assert_eq!(
        projection.get("title").and_then(Value::as_str),
        Some("\u{26ec} New Session")
    );
}

#[test]
fn title_projection_strips_omp_idle_and_spinner_prefixes() {
    for raw_title in [
        "  \u{03c0} >   Delete marketplace skill and inventory skills  ",
        "  \u{03c0} \u{2827}   Delete marketplace skill and inventory skills  ",
    ] {
        let mut session = session("P100", "G100", raw_title, "running", 1000.0);
        let session_object = session.as_object_mut().expect("session object");
        session_object.insert("agentId".to_string(), json!("omp"));
        session_object.insert(
            "runtimeSettings".to_string(),
            json!({ "agentName": "omp", "titleSource": "terminal-auto" }),
        );

        let projection = project_session_title_projection(&session);

        assert_eq!(
            projection.get("displayTitle").and_then(Value::as_str),
            Some("Delete marketplace skill and inventory skills")
        );
        assert_eq!(
            projection
                .get("displayTitleTooltip")
                .and_then(Value::as_str),
            Some("Delete marketplace skill and inventory skills")
        );
        assert_eq!(
            projection.get("primaryTitle").and_then(Value::as_str),
            Some("Delete marketplace skill and inventory skills")
        );
    }
}

#[test]
fn title_projection_replaces_wsl_shell_location_with_terminal_default_title() {
    let mut session = session(
        "P100",
        "G100",
        "madda@M7-Desktop: /mnt/c/dev/Ghostex",
        "running",
        1000.0,
    );
    let session_object = session.as_object_mut().expect("session object");
    session_object.insert(
        "runtimeSettings".to_string(),
        json!({ "titleSource": "terminal-auto" }),
    );

    let projection = project_session_title_projection(&session);

    assert_eq!(
        projection.get("primaryTitle").and_then(Value::as_str),
        Some("Terminal Session")
    );
    assert_eq!(projection.get("trustedResumeTitle"), None);
    assert_eq!(
        projection.get("displayTitle").and_then(Value::as_str),
        Some("\u{2217} Terminal Session")
    );
}

fn project(project_id: &str, name: &str, is_pinned: bool, is_favorite: bool) -> Value {
    json!({
        "createdAt": "2026-06-15T09:55:00.000Z",
        "isFavorite": is_favorite,
        "isPinned": is_pinned,
        "name": name,
        "projectId": project_id,
        "updatedAt": "2026-06-15T09:55:00.000Z",
    })
}

fn session(
    project_id: &str,
    session_id: &str,
    title: &str,
    lifecycle_state: &str,
    sidebar_order: f64,
) -> Value {
    json!({
        "createdAt": "2026-06-15T09:55:00.000Z",
        "isFavorite": false,
        "isPinned": false,
        "kind": "terminal",
        "lifecycleState": lifecycle_state,
        "projectId": project_id,
        "providerState": { "lifecycleState": "missing", "provider": "zmx" },
        "runtimeSettings": {},
        "sessionId": session_id,
        "sidebarOrder": sidebar_order,
        "surface": "workspace",
        "title": title,
        "updatedAt": "2026-06-15T09:55:00.000Z",
        "zmxName": format!("S7k-{project_id}-{session_id}"),
    })
}

fn previous_session(
    session_id: &str,
    title: &str,
    lifecycle_state: &str,
    surface: &str,
    probed_at: Option<&str>,
    updated_at: &str,
    last_active_at: &str,
) -> Value {
    let provider_state = match probed_at {
        Some(probed_at) => json!({
            "lifecycleState": "missing",
            "probedAt": probed_at,
            "provider": "zmx",
            "zmxName": format!("S7k-P100-{session_id}"),
        }),
        None => json!({
            "lifecycleState": "missing",
            "provider": "zmx",
            "zmxName": format!("S7k-P100-{session_id}"),
        }),
    };
    json!({
        "agentId": "codex",
        "createdAt": "2026-06-01T08:00:00.000Z",
        "isFavorite": false,
        "isPinned": false,
        "kind": "agent",
        "lastActiveAt": last_active_at,
        "lifecycleState": lifecycle_state,
        "projectId": "P100",
        "providerState": provider_state,
        "runtimeSettings": {
            "titleSource": if title == "Search by Text" { "placeholder" } else { "terminal-auto" },
        },
        "sessionId": session_id,
        "surface": surface,
        "title": title,
        "updatedAt": updated_at,
        "zmxName": format!("S7k-P100-{session_id}"),
    })
}

fn open_test_database() -> (tempfile::TempDir, Connection) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    initialize_gxserver_storage(&paths).expect("storage init");
    let db = open_gxserver_database(&paths).expect("open db");
    (temp, db)
}
