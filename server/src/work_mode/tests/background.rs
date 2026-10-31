//! What the background reads ask for and remember: the Work list's Linear feed requests and when
//! the feeds count as stale, what the card pass remembers per project with a Linear key present or
//! missing and `gh` signed in or out, which credentials a project's reads use, and the errors a
//! source answers when its credentials or checkout are missing. Nothing here reaches Linear or
//! GitHub: the caches are seeded fresh, so a pass finds nothing due, or a test answers Linear's
//! calls itself to see what the pass asks for.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::paths::get_gxserver_paths;

use super::*;

fn linear_session(project_id: &str, session_id: &str, identifiers: &[&str]) -> Value {
    session(
        project_id,
        session_id,
        json!({ "linearIssues": identifiers }),
    )
}

/// The Linear feed requests by key fingerprint, each with the team keys it asks for.
fn requests_by_key(plan: &WorkFeedPlan) -> BTreeMap<u64, Vec<String>> {
    plan.linear
        .iter()
        .map(|request| {
            (
                linear_key_fingerprint(&request.api_key),
                request.team_keys.iter().cloned().collect(),
            )
        })
        .collect()
}

#[test]
fn the_work_list_asks_linear_once_per_key_for_the_teams_its_sessions_name() {
    let key_one = "lin-key-bg-plan-one";
    let key_two = "lin-key-bg-plan-two";
    let input = |id: &str, sessions, key, repo: &str, tracker| {
        work_project(
            project(id, &format!("/gx-char/bg/plan/{id}")),
            sessions,
            key,
            repo,
            tracker,
        )
    };
    let projects = vec![
        input(
            "PBgPlanA",
            vec![
                linear_session("PBgPlanA", "S1", &["acm-1"]),
                linear_session("PBgPlanA", "S2", &["BUG-2"]),
            ],
            Some(key_one),
            "acme/plan-a",
            WorkSourceId::Linear,
        ),
        input(
            "PBgPlanB",
            vec![linear_session("PBgPlanB", "S3", &["OPS-3"])],
            Some(key_one),
            "acme/plan-b",
            WorkSourceId::Linear,
        ),
        input(
            "PBgPlanC",
            Vec::new(),
            Some(key_two),
            "acme/plan-c",
            WorkSourceId::Linear,
        ),
        // A GitHub workspace's project lists no Linear tickets, whatever key applies to it.
        input(
            "PBgPlanD",
            vec![linear_session("PBgPlanD", "S4", &["ZZZ-9"])],
            Some("lin-key-bg-plan-github"),
            "acme/plan-d",
            WorkSourceId::Github,
        ),
        input(
            "PBgPlanE",
            vec![linear_session("PBgPlanE", "S5", &["NOK-5"])],
            None,
            "acme/plan-e",
            WorkSourceId::Linear,
        ),
        // A worktree project shares the main checkout's repo, which is listed once.
        input(
            "PBgPlanAWorktree",
            Vec::new(),
            Some(key_one),
            "acme/plan-a",
            WorkSourceId::Linear,
        ),
    ];
    let plan = work_feed_plan(&projects);
    // One request per key: the map below would fold two requests with the same key into one.
    assert_eq!(plan.linear.len(), 2);
    assert_eq!(
        requests_by_key(&plan),
        BTreeMap::from([
            (
                linear_key_fingerprint(key_one),
                vec!["ACM".to_string(), "BUG".to_string(), "OPS".to_string()],
            ),
            (linear_key_fingerprint(key_two), Vec::new()),
        ])
    );
    assert_eq!(
        plan.github,
        ["PBgPlanA", "PBgPlanB", "PBgPlanC", "PBgPlanD", "PBgPlanE"]
            .map(|id| (format!("/gx-char/bg/plan/{id}"), id.to_string()))
            .to_vec()
    );
}

/// A Linear workspace's project with a session naming ACM-1 and a GitHub workspace's project, in
/// folders and with a key of their own.
fn stale_projects(name: &str) -> Vec<WorkProjectInput> {
    let linear_id = format!("PBgStale{name}Linear");
    let github_id = format!("PBgStale{name}Github");
    vec![
        work_project(
            project(&linear_id, &format!("/gx-char/bg/stale/{name}/linear")),
            vec![linear_session(&linear_id, "S1", &["ACM-1"])],
            Some(&format!("lin-key-bg-stale-{name}")),
            &format!("acme/stale-{name}-linear"),
            WorkSourceId::Linear,
        ),
        work_project(
            project(&github_id, &format!("/gx-char/bg/stale/{name}/github")),
            Vec::new(),
            None,
            &format!("acme/stale-{name}-github"),
            WorkSourceId::Github,
        ),
    ]
}

fn empty_github_feed(repo: &str) -> GithubFeed {
    GithubFeed {
        repo: Some(repo.to_string()),
        pull_requests: Vec::new(),
        issues: Vec::new(),
    }
}

#[test]
fn the_work_list_feeds_are_stale_until_each_one_is_cached() {
    // Linear's feed is missing while every GitHub feed is cached.
    let projects = stale_projects("linear-missing");
    let plan = work_feed_plan(&projects);
    assert!(work_feeds_stale(&plan));
    seed_work_feeds(
        &[],
        &[
            (
                "/gx-char/bg/stale/linear-missing/linear",
                empty_github_feed("acme/stale-linear-missing-linear"),
            ),
            (
                "/gx-char/bg/stale/linear-missing/github",
                empty_github_feed("acme/stale-linear-missing-github"),
            ),
        ],
        Some(VIEWER_LOGIN),
    );
    assert!(work_feeds_stale(&plan));
    seed_work_feeds(
        &[(&plan.linear[0], LinearFeed { issues: Vec::new() })],
        &[],
        Some(VIEWER_LOGIN),
    );
    assert!(!work_feeds_stale(&plan));

    // One GitHub feed is missing while Linear's is cached.
    let projects = stale_projects("github-missing");
    let plan = work_feed_plan(&projects);
    seed_work_feeds(
        &[(&plan.linear[0], LinearFeed { issues: Vec::new() })],
        &[(
            "/gx-char/bg/stale/github-missing/linear",
            empty_github_feed("acme/stale-github-missing-linear"),
        )],
        Some(VIEWER_LOGIN),
    );
    assert!(work_feeds_stale(&plan));
    seed_work_feeds(
        &[],
        &[(
            "/gx-char/bg/stale/github-missing/github",
            empty_github_feed("acme/stale-github-missing-github"),
        )],
        Some(VIEWER_LOGIN),
    );
    assert!(!work_feeds_stale(&plan));
}

/// A work-mode project in a checkout with no GitHub remote, so the pass finds no GitHub reads due.
fn pass_project(project_id: &str) -> Value {
    json!({ "projectId": project_id, "name": project_id, "path": format!("/gx-char/bg/pass/{project_id}") })
}

#[test]
fn the_card_pass_remembers_each_projects_key_and_tracker_and_reads_nothing_fresh() {
    for (project_id, signed_in) in [("PBgPassSignedIn", true), ("PBgPassSignedOut", false)] {
        let _gh = if signed_in {
            crate::session_git_status::assume_gh_cli_available_for_test()
        } else {
            crate::session_git_status::assume_gh_cli_signed_out_for_test()
        };
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let api_key = format!("lin-key-bg-pass-{project_id}");
        store_linear_api_key(&paths, LinearKeyScope::Shared, Some(&api_key)).unwrap();
        let fingerprint = linear_key_fingerprint(&api_key);
        seed_linear_team_keys(fingerprint, &["ACM"]);
        seed_linear_issue(
            fingerprint,
            "ACM-1",
            Some(linear_issue_info("ACM-1", ("started", "In Progress"))),
        );
        let project = pass_project(project_id);
        let sessions = vec![linear_session(project_id, "S1", &["ACM-1"])];
        // What an earlier pass remembered under another key and tracker, so only this pass can
        // make the assertions below hold (a remembered key also keeps work mode on with `gh`
        // signed out).
        remember_project_linear_key(project_id, Some(linear_key_fingerprint("lin-key-bg-old")));
        remember_project_work_tracker(project_id, WorkSourceId::Github);

        refresh_work_caches(&paths, std::slice::from_ref(&project), &sessions);

        assert_eq!(
            project_linear_key(project_id),
            Some(fingerprint),
            "{project_id}"
        );
        assert_eq!(
            cached_project_work_tracker(project_id),
            WorkSourceId::Linear,
            "{project_id}"
        );
        assert_eq!(
            cached_linear_issue(fingerprint, "ACM-1").and_then(|issue| issue.state_name),
            Some("In Progress".to_string()),
            "{project_id}"
        );
    }
}

#[test]
fn the_card_pass_reads_a_due_linear_ticket_and_leaves_a_fresh_one() {
    let _gh = crate::session_git_status::assume_gh_cli_signed_out_for_test();
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let _linear = answer_linear_for_test({
        let asked = asked.clone();
        move |_query, variables| {
            asked.borrow_mut().push(variables["ids"].clone());
            let nodes: Vec<Value> = variables["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| {
                    linear_node(
                        id.as_str().unwrap(),
                        ("completed", "Done"),
                        "2026-10-29T09:00:00.000Z",
                    )
                })
                .collect();
            Ok(json!({ "data": { "issues": { "nodes": nodes } } }))
        }
    });
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let api_key = "lin-key-bg-pass-due";
    store_linear_api_key(&paths, LinearKeyScope::Shared, Some(api_key)).unwrap();
    let fingerprint = linear_key_fingerprint(api_key);
    seed_linear_team_keys(fingerprint, &["ACM"]);
    // ACM-1 was read a moment ago; ACM-2 was never read, so it is due.
    seed_linear_issue(
        fingerprint,
        "ACM-1",
        Some(linear_issue_info("ACM-1", ("started", "In Progress"))),
    );
    let project = pass_project("PBgPassDue");
    let sessions = vec![linear_session("PBgPassDue", "S1", &["ACM-1", "ACM-2"])];
    // The key an earlier pass remembered keeps work mode on with `gh` signed out.
    remember_project_linear_key("PBgPassDue", Some(fingerprint));

    refresh_work_caches(&paths, std::slice::from_ref(&project), &sessions);

    assert_eq!(*asked.borrow(), vec![json!(["ACM-2"])]);
    assert_eq!(
        cached_linear_issue(fingerprint, "ACM-2").and_then(|issue| issue.state_name),
        Some("Done".to_string())
    );
    assert_eq!(
        cached_linear_issue(fingerprint, "ACM-1").and_then(|issue| issue.state_name),
        Some("In Progress".to_string())
    );
}

#[test]
fn the_card_pass_leaves_a_project_with_no_key_and_gh_signed_out_alone() {
    let _gh = crate::session_git_status::assume_gh_cli_signed_out_for_test();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let project = pass_project("PBgPassNothing");
    let sessions = vec![linear_session("PBgPassNothing", "S1", &["ACM-1"])];

    refresh_issues_workflow_setup(&paths, std::slice::from_ref(&project));
    refresh_work_caches(&paths, std::slice::from_ref(&project), &sessions);

    assert_eq!(project_linear_key("PBgPassNothing"), None);
    assert!(!project_work_mode(&project));
}

#[test]
fn a_projects_linear_key_is_its_own_then_its_workspaces_then_the_shared_one() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let key = |project_id, workspace_id| linear_api_key(&paths, project_id, workspace_id);
    assert_eq!(key(Some("PBgKey"), Some("ws-bg-key")), None);
    store_linear_api_key(&paths, LinearKeyScope::Shared, Some("lin-key-bg-shared")).unwrap();
    assert_eq!(
        key(Some("PBgKey"), Some("ws-bg-key")).as_deref(),
        Some("lin-key-bg-shared")
    );
    store_linear_api_key(
        &paths,
        LinearKeyScope::Workspace("ws-bg-key"),
        Some("lin-key-bg-workspace"),
    )
    .unwrap();
    assert_eq!(
        key(Some("PBgKey"), Some("ws-bg-key")).as_deref(),
        Some("lin-key-bg-workspace")
    );
    store_linear_api_key(
        &paths,
        LinearKeyScope::Project("PBgKey"),
        Some("lin-key-bg-project"),
    )
    .unwrap();
    assert_eq!(
        key(Some("PBgKey"), Some("ws-bg-key")).as_deref(),
        Some("lin-key-bg-project")
    );
    // The default workspace's key is the shared one.
    store_linear_api_key(
        &paths,
        LinearKeyScope::Workspace(crate::workspaces::DEFAULT_WORKSPACE_ID),
        Some("lin-key-bg-default"),
    )
    .unwrap();
    assert_eq!(
        key(None, Some(crate::workspaces::DEFAULT_WORKSPACE_ID)).as_deref(),
        Some("lin-key-bg-default")
    );
    assert_eq!(
        key(None, Some("ws-bg-other")).as_deref(),
        Some("lin-key-bg-default")
    );
}

#[test]
fn a_source_without_its_credentials_or_checkout_says_what_is_missing() {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let no_key = work_project(
        project("PBgNoKey", "/gx-char/bg/no-key"),
        Vec::new(),
        None,
        "acme/no-key",
        WorkSourceId::Linear,
    );
    let mut no_folder = work_project(
        project("PBgNoFolder", "/gx-char/bg/no-folder"),
        Vec::new(),
        None,
        "acme/no-folder",
        WorkSourceId::Github,
    );
    no_folder.path = None;
    let extra = Map::new();
    let draft = TicketDraft {
        title: "A ticket",
        description: None,
        assign_to_me: true,
        extra: &extra,
    };
    let linear = work_source(WorkSourceId::Linear);
    let github = work_source(WorkSourceId::Github);
    let no_key_message = "Set a Linear API key first (Settings, or ghostex issues linear-key).";
    assert!(!linear.can_read(&no_key.ticket_scope()));
    assert_eq!(
        linear
            .read_detail(&no_key.ticket_scope(), "ACM-1", false)
            .unwrap_err(),
        no_key_message
    );
    assert_eq!(
        linear.create(&no_key.ticket_scope(), &draft).err().unwrap(),
        no_key_message
    );
    assert_eq!(
        linear
            .branch_for(&no_key.ticket_scope(), "ACM-1")
            .unwrap_err(),
        no_key_message
    );
    assert!(!github.can_read(&no_folder.ticket_scope()));
    assert_eq!(
        github
            .read_detail(&no_folder.ticket_scope(), "12", false)
            .unwrap_err(),
        "This project has no folder."
    );
    assert_eq!(
        github
            .create(&no_folder.ticket_scope(), &draft)
            .err()
            .unwrap(),
        "This project has no folder."
    );
}

#[test]
fn the_details_of_a_linear_ticket_use_another_projects_key_when_its_own_project_has_none() {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let api_key = "lin-key-bg-details-fallback";
    let projects = vec![
        work_project(
            project("PBgDetailsNoKey", "/gx-char/bg/details/no-key"),
            Vec::new(),
            None,
            "acme/details-no-key",
            WorkSourceId::Linear,
        ),
        work_project(
            project("PBgDetailsKey", "/gx-char/bg/details/key"),
            Vec::new(),
            Some(api_key),
            "acme/details-key",
            WorkSourceId::Linear,
        ),
    ];
    // What the page read a moment ago with the other project's key, so nothing asks Linear.
    cached_detail(
        format!("linear:{}:ACM-6", linear_key_fingerprint(api_key)),
        true,
        || Ok(json!({ "source": "linear", "id": "ACM-6", "title": "Read with the other key" })),
    )
    .unwrap();
    let read = read_work_item(
        &paths,
        &projects,
        &WorkFeedPlan::default(),
        Some("PBgDetailsNoKey"),
        &WorkItemRef::Ticket(TicketRef::new(WorkSourceId::Linear, "ACM-6")),
        false,
    );
    assert_eq!(read["ticket"]["title"], json!("Read with the other key"));
    assert_eq!(read["errors"], json!([]));
}

/// A project in workspace `ws-bg-scope`, whose own key is the workspace's: a shared key exists
/// too, so a scope that lost its workspace id would read with the shared one instead.
fn workspace_key_setup(
    project_id: &str,
) -> (tempfile::TempDir, crate::paths::GxserverPaths, Value) {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    store_linear_api_key(
        &paths,
        LinearKeyScope::Shared,
        Some("lin-key-bg-scope-shared"),
    )
    .unwrap();
    store_linear_api_key(
        &paths,
        LinearKeyScope::Workspace("ws-bg-scope"),
        Some("lin-key-bg-scope-workspace"),
    )
    .unwrap();
    let project = json!({
        "projectId": project_id,
        "name": project_id,
        "path": format!("/gx-char/bg/scope/{project_id}"),
        "launchSettings": { "workspaceId": "ws-bg-scope" },
    });
    (temp, paths, project)
}

#[test]
fn a_projects_ticket_scope_reads_its_workspaces_key() {
    let (_temp, paths, project) = workspace_key_setup("PBgScopeList");
    let input = WorkProjectInput {
        credentials: Arc::new(WorkCredentials::new(&paths)),
        project_id: "PBgScopeList".to_string(),
        name: "PBgScopeList".to_string(),
        path: project["path"].as_str().map(str::to_string),
        project,
        sessions: Vec::new(),
        repo: None,
        workspace_id: "ws-bg-scope".to_string(),
        tracker: WorkSourceId::Linear,
    };
    let mut plan = WorkFeedPlan::default();

    work_source(WorkSourceId::Linear).plan_list(std::slice::from_ref(&input), &mut plan);

    assert_eq!(
        requests_by_key(&plan).into_keys().collect::<Vec<_>>(),
        vec![linear_key_fingerprint("lin-key-bg-scope-workspace")]
    );
}

#[test]
fn the_card_pass_reads_a_project_with_its_workspaces_key() {
    let (_temp, paths, project) = workspace_key_setup("PBgScopePass");
    // Fresh team keys for both keys, so the pass asks Linear nothing whichever key it picks.
    for key in ["lin-key-bg-scope-shared", "lin-key-bg-scope-workspace"] {
        seed_linear_team_keys(linear_key_fingerprint(key), &["ACM"]);
    }
    let credentials = WorkCredentials::new(&paths);
    let projects: HashMap<&str, &Value> = HashMap::from([("PBgScopePass", &project)]);

    work_source(WorkSourceId::Linear).refresh_tickets(&TicketPass {
        credentials: &credentials,
        projects: &projects,
        sessions: &[],
    });

    assert_eq!(
        project_linear_key("PBgScopePass"),
        Some(linear_key_fingerprint("lin-key-bg-scope-workspace"))
    );
}

/// A source that keeps every default and reads only with project `PBgBorrowKey`'s credentials.
struct OneProjectSource;

impl WorkSource for OneProjectSource {
    fn name(&self) -> std::borrow::Cow<'static, str> {
        "One project".into()
    }
    fn link_kind(&self) -> std::borrow::Cow<'static, str> {
        unreachable!()
    }
    fn key(&self, _: &str, _: &str) -> String {
        unreachable!()
    }
    fn label(&self, _: &str) -> String {
        unreachable!()
    }
    fn untitled(&self, _: &str) -> String {
        unreachable!()
    }
    fn keys_name_a_repo(&self) -> bool {
        unreachable!()
    }
    fn invalid_ref(&self, _: &str) -> String {
        unreachable!()
    }
    fn parse_ref(&self, _: &str) -> Option<String> {
        unreachable!()
    }
    fn tickets_in_branch(&self, _: &TicketScope, _: &str) -> Vec<String> {
        unreachable!()
    }
    fn branch_names(&self, _: &str, _: &str) -> bool {
        unreachable!()
    }
    fn tickets_in_pull_request(&self, _: &[String], _: &PullRequestText) -> Vec<String> {
        unreachable!()
    }
    fn plan_list(&self, _: &[WorkProjectInput], _: &mut WorkFeedPlan) {
        unreachable!()
    }
    fn list_stale(&self, _: &WorkFeedPlan) -> bool {
        unreachable!()
    }
    fn refresh_list(&self, _: &WorkFeedPlan, _: bool) {
        unreachable!()
    }
    fn refresh_tickets(&self, _: &TicketPass) {
        unreachable!()
    }
    fn known_keys(&self, _: &TicketScope, _: &WorkFeedPlan) -> Vec<String> {
        unreachable!()
    }
    fn list_holds_every_open_ticket(&self) -> bool {
        unreachable!()
    }
    fn status_follows_pull_request(&self) -> bool {
        unreachable!()
    }
    fn can_read(&self, scope: &TicketScope) -> bool {
        scope.project_id == "PBgBorrowKey"
    }
    fn cached_list(&self, _: &[WorkProjectInput], _: &WorkFeedPlan) -> SourceList {
        unreachable!()
    }
    fn cached_ticket(&self, _: &TicketScope, _: &str) -> TicketRead {
        unreachable!()
    }
    fn read_detail(&self, _: &TicketScope, _: &str, _: bool) -> Result<Value, String> {
        unreachable!()
    }
    fn create(&self, _: &TicketScope, _: &TicketDraft) -> Result<CreatedTicket, String> {
        unreachable!()
    }
    fn branch_for(&self, _: &TicketScope, _: &str) -> Result<String, String> {
        unreachable!()
    }
}

#[test]
fn only_linear_reads_the_details_with_another_projects_credentials() {
    let projects = vec![
        work_project(
            project("PBgBorrowNone", "/gx-char/bg/borrow/none"),
            Vec::new(),
            None,
            "acme/borrow-none",
            WorkSourceId::Linear,
        ),
        work_project(
            project("PBgBorrowKey", "/gx-char/bg/borrow/key"),
            Vec::new(),
            Some("lin-key-bg-borrow"),
            "acme/borrow-key",
            WorkSourceId::Linear,
        ),
    ];
    let cwd = Some("/gx-char/bg/borrow/none");
    let scope_of = |rules: &dyn WorkSource| {
        let scope = source_scope(rules, projects.first(), &projects, cwd);
        (scope.project_id.to_string(), scope.cwd.map(str::to_string))
    };

    assert_eq!(
        scope_of(work_source(WorkSourceId::Linear)),
        ("PBgBorrowKey".to_string(), cwd.map(str::to_string))
    );
    assert_eq!(
        scope_of(&OneProjectSource),
        ("PBgBorrowNone".to_string(), cwd.map(str::to_string))
    );
}

#[test]
fn a_linear_feed_request_never_prints_its_key() {
    let request = LinearFeedRequest {
        api_key: "lin-key-bg-debug-secret".to_string(),
        team_keys: ["ACM".to_string()].into(),
    };
    let printed = format!("{request:?}");
    assert!(!printed.contains("lin-key-bg-debug-secret"), "{printed}");
    assert!(printed.contains("ACM"), "{printed}");
}

#[test]
fn the_card_pass_knows_a_sessions_repo_before_linear_is_asked() {
    let _gh = crate::session_git_status::assume_gh_cli_signed_out_for_test();
    let checkout = tempfile::tempdir().expect("tempdir");
    let cwd = checkout.path().to_str().unwrap().to_string();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/order-check.git",
        ],
    ] {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&cwd)
            .status()
            .expect("git");
        assert!(status.success());
    }
    let repo_when_asked = std::rc::Rc::new(std::cell::RefCell::new(None));
    let _linear = answer_linear_for_test({
        let (repo_when_asked, cwd) = (repo_when_asked.clone(), cwd.clone());
        move |_query, _variables| {
            *repo_when_asked.borrow_mut() = Some(cached_work_repo(&cwd));
            Ok(json!({ "data": { "issues": { "nodes": [] } } }))
        }
    });
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let api_key = "lin-key-bg-pass-repo-first";
    store_linear_api_key(&paths, LinearKeyScope::Shared, Some(api_key)).unwrap();
    seed_linear_team_keys(linear_key_fingerprint(api_key), &["ACM"]);
    let project = json!({ "projectId": "PBgPassRepo", "name": "PBgPassRepo", "path": cwd });
    let sessions = vec![linear_session("PBgPassRepo", "S1", &["ACM-3"])];
    remember_project_linear_key("PBgPassRepo", Some(linear_key_fingerprint(api_key)));

    refresh_work_caches(&paths, std::slice::from_ref(&project), &sessions);

    assert_eq!(
        *repo_when_asked.borrow(),
        Some(Some("acme/order-check".to_string()))
    );
}
