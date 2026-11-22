//! Start work on a ticket command's ticket, as `/api/startWorkOnTicket` plans it and links the
//! session it creates: the ticket's branch, the session's `tickets` link, a worktree already on the
//! ticket reused without asking the command, refusals that show no stored value and create
//! nothing, and the cloud start's draft. The session itself needs zmx, so these stop at the plan
//! and the link, which Start runs before and after it.

use super::*;
use crate::work_mode::{plan_ticket_work, WorkTicket};

/// A project of `workspace_id` in a git checkout of its own, and a session row in it (never
/// started).
fn project_in(routes: &Routes, workspace_id: &str, name: &str) -> (String, String, PathBuf) {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let repository = crate::domain::DomainRepository::new(&db, "test");
    let folder = routes.paths.app_data_dir.join(name);
    create_git_repository_for_server_test(&folder);
    let project = repository
        .create_project(
            json!({ "name": name, "path": folder, "launchSettings": { "workspaceId": workspace_id } })
                .as_object()
                .unwrap(),
        )
        .expect("project");
    let project_id = project["projectId"].as_str().unwrap().to_string();
    let session = repository
        .create_session(
            json!({ "projectId": project_id, "kind": "agent", "agentId": "codex", "title": name, "lifecycleState": "running" })
                .as_object()
                .unwrap(),
            false,
        )
        .expect("session");
    (
        project_id,
        session["sessionId"].as_str().unwrap().to_string(),
        folder,
    )
}

fn start_params(project_id: &str, ticket_id: &str) -> Map<String, Value> {
    json!({ "projectId": project_id, "source": "fixture", "ticketId": ticket_id })
        .as_object()
        .unwrap()
        .clone()
}

fn plan(
    routes: &Routes,
    params: &Map<String, Value>,
) -> Result<(Option<String>, Option<String>, Value), String> {
    let ticket = WorkTicket::from_params(params).map_err(|error| error.message)?;
    plan_ticket_work(&routes.state, params, &ticket)
        .map(|plan| {
            (
                plan.existing_worktree,
                plan.branch,
                Value::Object(plan.links),
            )
        })
        .map_err(|error| error.message)
}

fn work_links(routes: &Routes, project_id: &str, session_id: &str) -> Value {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    crate::domain::DomainRepository::new(&db, "test")
        .get_session(project_id, session_id)
        .expect("read")
        .expect("session")["runtimeSettings"]["workLinks"]
        .clone()
}

fn link_started(
    routes: &Routes,
    project_id: &str,
    session_id: &str,
    links: &Value,
) -> Result<(), String> {
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    crate::work_mode::link_started_session(
        &crate::domain::DomainRepository::new(&db, "test"),
        &db,
        &routes.paths,
        project_id,
        session_id,
        links.as_object().unwrap(),
    )
    .map_err(|error| error.message)
}

fn calls(routes: &Routes) -> Vec<String> {
    let dir = command_work_dir(&routes.paths, Some("fixture")).expect("work dir");
    fs::read_to_string(dir.join("calls.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Start plans a worktree on the ticket's own branch and a `tickets` link, which it writes on the
/// session through the link check; once the workspace cleared its command, that link is refused.
#[tokio::test]
async fn starting_work_on_a_command_ticket_uses_its_branch_and_links_it() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Pinned", "tok-start-plan").await;
    let (project_id, session_id, _) = project_in(&routes, &workspace_id, "planned");
    let params = start_params(&project_id, "acm-12");
    let (existing, branch, links) = plan(&routes, &params).expect("planned");
    assert_eq!(existing, None);
    assert_eq!(branch.as_deref(), Some("leandro/acm-12-spec"));
    assert_eq!(links, json!({ "tickets": { "fixture": ["ACM-12"] } }));
    link_started(&routes, &project_id, &session_id, &links).expect("linked");
    assert_eq!(
        work_links(&routes, &project_id, &session_id),
        json!({ "tickets": { "fixture": ["ACM-12"] } })
    );

    let (_, other_session, _) = project_in(&routes, &workspace_id, "cleared");
    let other_project = {
        let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
        crate::domain::DomainRepository::new(&db, "test")
            .list_projects()
            .expect("projects")
            .into_iter()
            .find(|project| project["name"] == "cleared")
            .expect("project")["projectId"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        link_started(&routes, &other_project, &other_session, &links),
        Err(crate::work_mode::TICKET_LINKS_REFUSED.to_string())
    );
    assert_eq!(
        work_links(&routes, &other_project, &other_session),
        Value::Null
    );
}

/// A Work page row names the workspace its ticket was read in: Start and Start in cloud work on it
/// only in a project of that workspace, where another workspace pinning the same id would answer
/// its own ACM-12. A request that names no row reads the ticket in its project's workspace.
#[tokio::test]
async fn a_row_read_in_another_workspace_never_starts_in_this_one() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let read_in = with_token(&routes, "Read in", "tok-start-row-a").await;
    let picked = with_token(&routes, "Picked", "tok-start-row-b").await;
    let (project_id, _, folder) = project_in(&routes, &picked, "picked");
    let remote = routes.paths.app_data_dir.join("github.com/acme/picked.git");
    fs::create_dir_all(&remote).expect("remote");
    run_git_for_server_test(&remote, &["init", "--bare", "--quiet"]);
    let url = format!("file://{}", path_to_string(&remote));
    run_git_for_server_test(&folder, &["remote", "add", "origin", &url]);
    let row = |workspace_id: &str| {
        let mut params = start_params(&project_id, "ACM-12");
        params.insert(
            "sourceKey".to_string(),
            json!(crate::work_mode::source_key(workspace_id, "fixture")),
        );
        params
    };
    let refused = crate::work_mode::START_IN_THE_ROWS_WORKSPACE.to_string();
    assert_eq!(plan(&routes, &row(&read_in)).unwrap_err(), refused);
    assert_eq!(
        crate::work_mode::draft_cloud_work(&routes.state, &row(&read_in))
            .unwrap_err()
            .message,
        refused
    );
    assert!(
        !calls(&routes).contains(&"detail".to_string()),
        "{:?}",
        calls(&routes)
    );
    let (_, branch, _) = plan(&routes, &row(&picked)).expect("planned");
    assert_eq!(branch.as_deref(), Some("leandro/acm-12-spec"));
    let (_, branch, _) = plan(&routes, &start_params(&project_id, "ACM-12")).expect("planned");
    assert_eq!(branch.as_deref(), Some("leandro/acm-12-spec"));
}

/// Start in cloud on a ticket the cards read a moment ago still shows its details when the
/// details page keeps a failure: it reads them again, as Start does.
#[tokio::test]
async fn cloud_start_reads_past_a_failure_the_details_page_keeps() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Pinned", "tok-start-kept").await;
    let (project_id, _, folder) = project_in(&routes, &workspace_id, "kept");
    let remote = routes.paths.app_data_dir.join("github.com/acme/kept.git");
    fs::create_dir_all(&remote).expect("remote");
    run_git_for_server_test(&remote, &["init", "--bare", "--quiet"]);
    let url = format!("file://{}", path_to_string(&remote));
    run_git_for_server_test(&folder, &["remote", "add", "origin", &url]);
    // The details page reads the ticket while the command fails, and keeps that.
    routes.answer("error-json");
    let credentials = crate::work_mode::WorkCredentials::new(&routes.paths);
    let scope = crate::work_mode::TicketScope {
        project_id: &project_id,
        effective_workspace_id: Some(&workspace_id),
        credentials: Some(&credentials),
        ..crate::work_mode::TicketScope::default()
    };
    let source = crate::work_mode::WorkSourceId::parse("fixture").expect("fixture");
    assert!(crate::work_mode::work_source(source)
        .read_detail(&scope, "ACM-12", false)
        .is_err());
    // Then the cards read it.
    routes.answer("good");
    let crate::work_mode::WorkSourceId::Command(command) = source else {
        unreachable!("a command id")
    };
    let plan = crate::work_mode::JobPlan {
        workspace_id: workspace_id.clone(),
        source: command,
        generation: crate::work_mode::work_source_generation(&workspace_id),
    };
    crate::work_mode::queue_exact_read(crate::work_mode::ExactRead {
        plan: plan.clone(),
        ids: vec!["ACM-12".to_string()],
        projects: Vec::new(),
        paths: routes.paths.clone(),
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while crate::work_mode::cached_command_ticket(&plan, "ACM-12").is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the card read never landed"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let draft =
        crate::work_mode::draft_cloud_work(&routes.state, &start_params(&project_id, "ACM-12"))
            .map_err(|error| error.message)
            .expect("drafted");
    assert_eq!(draft["branch"], "leandro/acm-12-spec");
    assert!(
        draft["prompt"]
            .as_str()
            .unwrap()
            .contains("Spec the contract"),
        "{draft}"
    );
    assert_eq!(
        calls(&routes)
            .iter()
            .filter(|call| *call == "detail")
            .count(),
        2,
        "{:?}",
        calls(&routes)
    );
}

/// A second Start on the ticket finds the worktree a session linking it works in, and asks the
/// command nothing, even with its program gone.
#[tokio::test]
async fn a_second_start_reuses_the_worktree_without_asking_the_command() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Pinned", "tok-start-reuse").await;
    let (project_id, _, folder) = project_in(&routes, &workspace_id, "reused");
    let worktree = folder.with_file_name("reused-acm-12");
    fs::create_dir_all(&worktree).expect("worktree folder");
    let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
    let mut runtime = Map::new();
    runtime.insert(
        crate::worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY.to_string(),
        crate::worktree_sessions::worktree_session_marker_value(
            "leandro/acm-12-spec",
            &path_to_string(&worktree),
            "First",
            "2026-11-05T00:00:00.000Z",
        ),
    );
    runtime.insert(
        "workLinks".to_string(),
        json!({ "tickets": { "fixture": ["ACM-12"] } }),
    );
    crate::domain::DomainRepository::new(&db, "test")
        .create_session(
            json!({ "projectId": project_id, "kind": "agent", "agentId": "codex", "title": "First", "lifecycleState": "running", "runtimeSettings": runtime })
                .as_object()
                .unwrap(),
            false,
        )
        .expect("first session");
    fs::rename(&routes.program, format!("{}.moved", routes.program)).expect("moved away");
    let (existing, branch, links) =
        plan(&routes, &start_params(&project_id, "ACM-12")).expect("planned");
    assert_eq!(
        existing.as_deref(),
        Some(path_to_string(&worktree).as_str())
    );
    assert_eq!(branch, None);
    assert_eq!(links, json!({ "tickets": { "fixture": ["ACM-12"] } }));
    assert!(
        !calls(&routes).contains(&"detail".to_string()),
        "{:?}",
        calls(&routes)
    );
}

/// A refusal reaches the answer and the request log through the workspace's mask, and nothing is
/// created: no session, no worktree.
#[tokio::test]
async fn a_start_refusal_never_shows_a_stored_value_and_creates_nothing() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let token = "tok-start-refused";
    let workspace_id = with_token(&routes, "Pinned", token).await;
    // Its `detail` fails with a message that quotes the token.
    routes.answer("error-token");
    let (project_id, _, folder) = project_in(&routes, &workspace_id, "refused");
    let sessions = || {
        let db = crate::storage::open_gxserver_database(&routes.paths).expect("db");
        crate::domain::DomainRepository::new(&db, "test")
            .list_sessions(Some(&project_id))
            .expect("sessions")
            .len()
    };
    let before = sessions();
    let worktrees = || {
        fs::read_dir(folder.parent().unwrap())
            .expect("folder")
            .count()
    };
    let worktrees_before = worktrees();
    let (status, body) = routes
        .logged_call(
            "/api/startWorkOnTicket",
            json!({ "projectId": project_id, "source": "fixture", "ticketId": "ACM-12" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        error_text(&body).ends_with("Ghostex needs to read the ticket to start work on it."),
        "{body}"
    );
    assert!(!body.to_string().contains(token), "{body}");
    let log = fs::read_to_string(&routes.paths.log_file).unwrap_or_default();
    assert!(!log.contains(token), "{log}");
    assert_eq!(sessions(), before);
    assert_eq!(worktrees(), worktrees_before);
    // Sent without a source once the command is the tracker, as `ghostex issues start ACM-12`
    // sends it: the command's ticket, refused the same way.
    pick_tracker(&routes, &workspace_id, "fixture").await;
    let (status, body) = routes
        .logged_call(
            "/api/startWorkOnTicket",
            json!({ "projectId": project_id, "ticketId": "ACM-12" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        error_text(&body).ends_with("Ghostex needs to read the ticket to start work on it."),
        "{body}"
    );
    assert!(!body.to_string().contains(token), "{body}");
    assert_eq!(sessions(), before);
}

/// Start in cloud cuts the same branch from the same read, and names the ticket in its task.
#[tokio::test]
async fn starting_a_command_ticket_in_the_cloud_uses_its_branch() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Pinned", "tok-start-cloud").await;
    let (project_id, _, folder) = project_in(&routes, &workspace_id, "clouded");
    // A GitHub remote by its name, served from a folder, so nothing leaves this computer.
    let remote = routes.paths.app_data_dir.join("github.com/acme/app.git");
    fs::create_dir_all(&remote).expect("remote");
    run_git_for_server_test(&remote, &["init", "--bare", "--quiet"]);
    let url = format!("file://{}", path_to_string(&remote));
    run_git_for_server_test(&folder, &["remote", "add", "origin", &url]);
    let draft =
        crate::work_mode::draft_cloud_work(&routes.state, &start_params(&project_id, "acm-12"))
            .map_err(|error| error.message)
            .expect("drafted");
    assert_eq!(draft["branch"], "leandro/acm-12-spec");
    assert_eq!(draft["branchOnRemote"], false);
    assert_eq!(draft["ticket"], "fixture:ACM-12");
    assert_eq!(draft["label"], "ACM-12");
    let prompt = draft["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("open a pull request from it that names ACM-12 in its title."),
        "{prompt}"
    );
    assert_eq!(
        calls(&routes)
            .iter()
            .filter(|call| *call == "detail")
            .count(),
        1
    );
    // With the command as the tracker, an ID sent without a source is its.
    pick_tracker(&routes, &workspace_id, "fixture").await;
    let params = json!({ "projectId": project_id, "ticketId": "ACM-12" });
    let draft = crate::work_mode::draft_cloud_work(&routes.state, params.as_object().unwrap())
        .map_err(|error| error.message)
        .expect("drafted without a source");
    assert_eq!(draft["ticket"], "fixture:ACM-12");
}

/// The ticket id as typed is checked too: `acm-12` holds the stored `acm`, though the command
/// reads it as `ACM-12`. Nothing runs.
#[tokio::test]
async fn a_typed_id_holding_a_stored_value_is_refused_before_anything_runs() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Pinned", "tok-start-typed").await;
    let (status, body) = routes
        .call(
            "/api/setWorkSourceCredentials",
            json!({ "workspaceId": workspace_id, "values": { "mode": "acm" } }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (project_id, _, _) = project_in(&routes, &workspace_id, "typed");
    let before = calls(&routes);
    assert_eq!(
        plan(&routes, &start_params(&project_id, "acm-12")),
        Err(crate::work_mode::START_ID_SPELLS_CREDENTIAL.to_string())
    );
    assert_eq!(calls(&routes), before);
}

/// The source gxserver reads a `ticketId` sent without one as.
fn source_of(routes: &Routes, project_id: &str, ticket_id: &str, sent: Option<&str>) -> Value {
    let mut params = json!({ "projectId": project_id, "ticketId": ticket_id });
    if let Some(source) = sent {
        params["source"] = json!(source);
    }
    crate::work_mode::with_ticket_source(&routes.state, params.as_object().unwrap())
        .map_err(|error| error.message)
        .expect("read")["source"]
        .clone()
}

async fn pick_tracker(routes: &Routes, workspace_id: &str, tracker: &str) {
    let (status, body) = routes
        .call(
            "/api/setWorkTracker",
            json!({ "workspaceId": workspace_id, "tracker": tracker }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// An ID sent without a source is the ticket command's only while the command is the tracker
/// and the ID has one of its keys; otherwise a number is GitHub's and anything else Linear's, as
/// before. A pinned command that isn't the tracker is never picked by its keys, and a source sent
/// is kept.
#[tokio::test]
async fn an_id_sent_without_a_source_follows_the_tracker() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("good");
    let workspace_id = with_token(&routes, "Pinned", "tok-start-source").await;
    let (project_id, _, _) = project_in(&routes, &workspace_id, "sourced");
    let table = |rows: &[(&str, Option<&str>, &str)]| {
        for (ticket_id, sent, source) in rows {
            assert_eq!(
                source_of(&routes, &project_id, ticket_id, *sent),
                json!(source),
                "{ticket_id} {sent:?}"
            );
        }
    };
    // No tracker picked and no Linear key: GitHub, the rule for a workspace never picked.
    table(&[
        ("ACM-12", None, "linear"),
        ("218", None, "github"),
        ("ACM-12", Some("fixture"), "fixture"),
    ]);
    pick_tracker(&routes, &workspace_id, "fixture").await;
    table(&[
        ("ACM-12", None, "fixture"),
        ("acm-12", None, "fixture"),
        ("SPX-1", None, "linear"),
        ("218", None, "github"),
        ("#218", None, "github"),
        ("ACM-12", Some("linear"), "linear"),
    ]);
    // Linear as the tracker, with the command still pinned and declaring `ACM`.
    pick_tracker(&routes, &workspace_id, "linear").await;
    table(&[
        ("ACM-12", None, "linear"),
        ("ACM-12", Some("fixture"), "fixture"),
    ]);
    // A team stops the command, so its pick no longer applies.
    pick_tracker(&routes, &workspace_id, "fixture").await;
    routes.join_team(&workspace_id);
    table(&[("ACM-12", None, "linear"), ("218", None, "github")]);
}

/// A number command as the tracker takes every number. It answers under an id of its own: an id's
/// format is the same in every workspace, and `fixture` is a key-number id.
#[tokio::test]
async fn a_number_command_tracker_takes_a_number_sent_without_a_source() {
    let _slots = crate::work_mode::holding_slots();
    let routes = Routes::new();
    routes.answer("hourly start-numbered number");
    let workspace_id = routes.workspace("Numbers").await;
    let (status, body) = routes
        .call(
            "/api/setWorkSource",
            json!({ "workspaceId": workspace_id, "path": routes.program }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    pick_tracker(&routes, &workspace_id, "start-numbered").await;
    let (project_id, _, _) = project_in(&routes, &workspace_id, "numbered");
    assert_eq!(
        source_of(&routes, &project_id, "218", None),
        json!("start-numbered")
    );
    assert_eq!(
        source_of(&routes, &project_id, "#218", None),
        json!("start-numbered")
    );
    assert_eq!(
        source_of(&routes, &project_id, "SPX-1", None),
        json!("linear")
    );
}
