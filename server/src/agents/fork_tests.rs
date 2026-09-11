use super::*;
use crate::paths::get_gxserver_paths;
use crate::storage::{initialize_gxserver_storage, open_gxserver_database};

#[test]
fn fork_title_survives_identity_and_inherited_metadata_until_child_is_named() {
    let home = tempfile::tempdir().unwrap();
    let paths = get_gxserver_paths(Some(home.path().join("ghostex")));
    initialize_gxserver_storage(&paths).unwrap();
    let db = open_gxserver_database(&paths).unwrap();
    let repository = DomainRepository::new(&db, "fork-test");
    let project = repository
        .create_project(
            json!({"path":home.path(), "name":"Fork Test"})
                .as_object()
                .unwrap(),
        )
        .unwrap();
    let source = repository.create_session(
        json!({"projectId":project["projectId"], "agentId":"codex", "kind":"agent", "title":"Parent conversation"}).as_object().unwrap(), false,
    ).unwrap();
    let params = create_agent_fork_session_params(&project, &source, &json!({"agentId":"codex"}));
    let created = repository
        .create_session(params.as_object().unwrap(), false)
        .unwrap();
    let lifecycle = LifecycleParams {
        project_id: created["projectId"].as_str().unwrap().to_string(),
        session_id: created["sessionId"].as_str().unwrap().to_string(),
    };
    let id = "33333333-3333-4333-8333-333333333333";
    let (_, identified) = apply_session_state_update(
        &repository,
        &lifecycle,
        json!({"agentName":"codex", "agentSessionId":id})
            .as_object()
            .unwrap(),
        SessionIdentityUpdateSource::LiveProcess,
    )
    .unwrap();
    assert_eq!(identified["title"], "Fork: Parent conversation");
    assert_eq!(
        project_session_title_projection(&identified)["displayTitle"],
        "∗ Fork: Parent conversation"
    );

    let codex_dir = home.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let index = codex_dir.join("session_index.jsonl");
    std::fs::write(
        &index,
        format!("{}\n", json!({"id":id,"thread_name":"Parent conversation"})),
    )
    .unwrap();
    let inherited =
        reconcile_agent_metadata_title(&repository, &lifecycle, home.path(), "pending").unwrap();
    assert!(!inherited.changed);
    assert_eq!(
        inherited.session.unwrap()["title"],
        "Fork: Parent conversation"
    );

    std::fs::write(
        &index,
        format!("{}\n", json!({"id":id,"thread_name":"Child conversation"})),
    )
    .unwrap();
    let renamed =
        reconcile_agent_metadata_title(&repository, &lifecycle, home.path(), "pending").unwrap();
    assert!(renamed.changed);
    let renamed = renamed.session.unwrap();
    assert_eq!(renamed["title"], "Child conversation");
    assert!(renamed["runtimeSettings"]
        .get("forkFirstPromptAutoTitlePending")
        .is_none());
}
