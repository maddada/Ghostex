//! Which tracker a workspace uses when the team, this computer or nobody picked one.

use serde_json::json;

use crate::paths::get_gxserver_paths;
use crate::workspaces::DEFAULT_WORKSPACE_ID;

use super::*;

#[test]
fn a_team_choice_wins_then_this_computers_then_linear_when_a_key_applies() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let workspaces = json!({ "workspaces": {
        DEFAULT_WORKSPACE_ID: {},
        "ws-char-picked": { "tracker": "github" },
        "ws-char-team": { "tracker": "github" },
        "ws-char-keyed": {},
        "ws-char-bad": { "tracker": "jira" },
    } });
    let tracker = |workspace_id: &str| {
        let (tracker, source) = workspace_work_tracker(&paths, &workspaces, workspace_id);
        (tracker.as_wire(), source.as_wire())
    };

    assert_eq!(tracker(DEFAULT_WORKSPACE_ID), ("github", "default"));
    assert_eq!(tracker("ws-char-picked"), ("github", "workspace"));
    assert_eq!(tracker("ws-char-bad"), ("github", "default"));

    store_linear_api_key(
        &paths,
        LinearKeyScope::Workspace("ws-char-keyed"),
        Some("lin-key-ws"),
    )
    .unwrap();
    assert_eq!(tracker("ws-char-keyed"), ("linear", "default"));
    assert_eq!(tracker(DEFAULT_WORKSPACE_ID), ("github", "default"));
    store_linear_api_key(&paths, LinearKeyScope::Shared, Some("lin-key-shared")).unwrap();
    assert_eq!(tracker(DEFAULT_WORKSPACE_ID), ("linear", "default"));
    // A workspace that does not exist is the default one.
    assert_eq!(tracker("ws-char-missing"), ("linear", "default"));
    // It takes the default workspace's own pick over the key, which the line above cannot tell.
    let default_picked = json!({ "workspaces": { DEFAULT_WORKSPACE_ID: { "tracker": "github" } } });
    let (picked, source) = workspace_work_tracker(&paths, &default_picked, "ws-char-missing");
    assert_eq!(
        (picked.as_wire(), source.as_wire()),
        ("github", "workspace")
    );
    // The workspace's own pick still beats the key.
    assert_eq!(tracker("ws-char-picked"), ("github", "workspace"));

    remember_team_work_tracker("ws-char-team", Some(WorkSourceId::Linear));
    assert_eq!(tracker("ws-char-team"), ("linear", "team"));
    remember_team_work_tracker("ws-char-team", None);
    assert_eq!(tracker("ws-char-team"), ("github", "workspace"));
}

#[test]
fn trackers_read_and_write_their_wire_names() {
    assert_eq!(WorkSourceId::Linear.as_wire(), "linear");
    assert_eq!(WorkSourceId::Github.as_wire(), "github");
    for tracker in [WorkSourceId::Linear, WorkSourceId::Github] {
        assert_eq!(
            WorkSourceId::from_wire(Some(&json!(tracker.as_wire()))),
            Some(tracker)
        );
    }
    assert_eq!(
        WorkSourceId::from_wire(Some(&json!(" linear "))),
        Some(WorkSourceId::Linear)
    );
    assert_eq!(WorkSourceId::from_wire(Some(&json!("jira"))), None);
    assert_eq!(WorkSourceId::from_wire(None), None);
}
