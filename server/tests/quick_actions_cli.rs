//! The phone's two Actions calls as it makes them over SSH: `ghostex sessions --json
//! --mobile-summary` and `ghostex run-action`. Each test runs the real `ghostex` binary and reads
//! its stdout and exit status. The CLI talks to a loopback stand-in for gxserver whose sidebar HUD
//! is built by gxserver's own HUD projection (`read_sidebar_hud`, `commandsByProject`,
//! `globalCommands`), and which records every request, so a test can tell that nothing was launched.
#![cfg(unix)]

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::process::CommandExt,
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    thread,
};

use gxserver::presentation::snapshot::presentation_capabilities;
use gxserver::sidebar_hud::{
    read_sidebar_hud, read_sidebar_hud_commands_by_project, read_sidebar_hud_global_commands,
};
use serde_json::{json, Value};

const PROJECT_ID: &str = "P1abc";
const WORKTREE_PROJECT_ID: &str = "P2wtx";
const CREATED_SESSION_ID: &str = "S1new";

/// What the stand-in serves: project rows as `listProjects` returns them, the stored Global
/// Action definitions in their saved order, rows appended to `globalCommands` as-is, and the
/// daemon's presentation capabilities (`None` for a daemon too old to send any).
struct Daemon {
    projects: Vec<Value>,
    global_definitions: Vec<Value>,
    raw_global_rows: Vec<Value>,
    capabilities: Option<Value>,
    hud_fails: bool,
}

impl Default for Daemon {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            global_definitions: Vec::new(),
            raw_global_rows: Vec::new(),
            capabilities: Some(presentation_capabilities(true)),
            hud_fails: false,
        }
    }
}

struct Running {
    url: String,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
}

impl Running {
    fn requests_to(&self, path: &str) -> Vec<Value> {
        self.requests
            .lock()
            .expect("requests")
            .iter()
            .filter(|(request_path, _)| request_path == path)
            .map(|(_, params)| params.clone())
            .collect()
    }

    fn request_count(&self) -> usize {
        self.requests.lock().expect("requests").len()
    }
}

fn start(daemon: Daemon) -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            serve(stream, &daemon, &recorded);
        }
    });
    Running { url, requests }
}

fn serve(mut stream: TcpStream, daemon: &Daemon, recorded: &Mutex<Vec<(String, Value)>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    let mut content_length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).expect("request body");
    let params = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|body| body.get("params").cloned())
        .unwrap_or(Value::Null);
    recorded
        .lock()
        .expect("requests")
        .push((path.clone(), params.clone()));
    let (status, answer) = match answer(daemon, &path, &params) {
        Some(result) => (
            "200 OK",
            json!({ "ok": true, "requestId": "test", "result": result }),
        ),
        None => (
            "500 Internal Server Error",
            json!({ "error": "unavailable", "message": "unavailable", "ok": false }),
        ),
    };
    let text = answer.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
}

/// Answers the routes the two commands call, the HUD the way `/api/readSidebarHud` composes it.
fn answer(daemon: &Daemon, path: &str, params: &Value) -> Option<Value> {
    match path {
        "/api/listProjects" => Some(json!({ "projects": daemon.projects })),
        "/api/listRecentProjects" => Some(json!({ "projects": [] })),
        "/api/listSessions" => Some(json!({ "sessions": [] })),
        "/api/readPresentationSnapshot" => {
            let mut snapshot = json!({ "sessions": [] });
            if let Some(capabilities) = &daemon.capabilities {
                snapshot["capabilities"] = capabilities.clone();
            }
            Some(json!({ "snapshot": snapshot }))
        }
        "/api/readSidebarHud" if !daemon.hud_fails => {
            let active_project_id = params.get("activeProjectId").and_then(Value::as_str);
            let mut hud = read_sidebar_hud(&daemon.projects, active_project_id);
            if params.get("includeAllProjectCommands") == Some(&json!(true)) {
                hud["commandsByProject"] = read_sidebar_hud_commands_by_project(&daemon.projects);
            }
            let mut globals = read_sidebar_hud_global_commands(&daemon.global_definitions);
            globals
                .as_array_mut()
                .expect("global rows")
                .extend(daemon.raw_global_rows.iter().cloned());
            hud["globalCommands"] = globals;
            Some(hud)
        }
        "/api/createSession" => Some(json!({
            "session": {
                "projectId": params.get("projectId"),
                "sessionId": CREATED_SESSION_ID,
                "title": params.get("title"),
            }
        })),
        "/api/startSessionProvider" => Some(json!({
            "session": {
                "lifecycleState": "running",
                "projectId": params.get("projectId"),
                "sessionId": params.get("sessionId"),
            }
        })),
        _ => None,
    }
}

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Output {
    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {}", self.stdout))
    }
}

/// A computer whose settings file has Actions on or off, and nothing else.
fn computer(actions_on: bool) -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("home");
    let ghostex_home = home.path().join("ghostex");
    fs::create_dir_all(&ghostex_home).expect("ghostex home");
    fs::write(
        ghostex_home.join("native-sidebar-settings.json"),
        json!({ "actionsHidden": !actions_on }).to_string(),
    )
    .expect("settings");
    home
}

fn ghostex(home: &Path, daemon: &Running, args: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_gxserver"))
        .arg0("ghostex")
        .args(args)
        .args(["--server", daemon.url.as_str(), "--token", "test-token"])
        .env_clear()
        .env("HOME", home)
        .env("GHOSTEX_HOME", home.join("ghostex"))
        .env("TMPDIR", home)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run ghostex");
    Output {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn mobile_summary(home: &Path, daemon: &Running) -> Value {
    let output = ghostex(home, daemon, &["sessions", "--json", "--mobile-summary"]);
    assert_eq!(output.code, 0, "stderr: {}", output.stderr);
    output.json()
}

fn project(project_id: &str, path: &str) -> Value {
    json!({ "name": project_id, "path": path, "projectId": project_id })
}

/// A project with a stored Dev override (a configured default), two Actions of its own in a saved
/// order, Setup deleted and Build/Test left as unconfigured defaults, plus its worktree project.
fn projects_with_actions() -> Vec<Value> {
    let mut alpha = project(PROJECT_ID, "/work/alpha");
    alpha["customCommands"] = json!([
        { "command": "bun run dev", "commandId": "dev", "icon": "code", "name": "Dev" },
        {
            "command": "make deploy",
            "commandId": "deploy",
            "icon": "cloud",
            "links": [{ "url": "http://localhost:4000" }],
            "name": "Deploy",
            "showOnProjectRow": true,
        },
        { "actionType": "browser", "commandId": "site", "name": "Site", "url": "https://example.test" },
    ]);
    alpha["customCommandOrder"] = json!(["site", "deploy"]);
    alpha["deletedDefaultCommandIds"] = json!(["setup"]);
    let mut worktree = project(WORKTREE_PROJECT_ID, "/work/alpha-wt");
    worktree["worktree"] = json!({ "parentProjectId": PROJECT_ID });
    vec![alpha, worktree, project("P3emp", "/work/empty")]
}

fn global_definitions() -> Vec<Value> {
    vec![
        json!({
            "actionType": "terminal",
            "command": "echo global-marker",
            "commandId": "g-term",
            "icon": "bolt",
            "links": [{ "url": "http://localhost:5000" }],
            "name": "Global Term",
            "showOnProjectRow": true,
        }),
        json!({
            "actionType": "browser",
            "commandId": "g-web",
            "name": "Docs",
            "url": "http://localhost:3000/docs",
        }),
        json!({
            "actionType": "terminal",
            "command": "echo unpinned",
            "commandId": "g-unpinned",
            "name": "Unpinned",
            "showOnProjectRow": false,
        }),
    ]
}

/// Global rows with nothing to run, which the summary and `run-action` must both skip.
fn unconfigured_global_rows() -> Vec<Value> {
    vec![
        json!({ "actionType": "terminal", "command": "  ", "commandId": "g-empty", "name": "Empty" }),
        json!({ "actionType": "browser", "commandId": "g-no-url", "name": "No URL", "url": " " }),
    ]
}

fn expected_project_actions() -> Value {
    json!([
        { "actionType": "browser", "commandId": "site", "name": "Site", "url": "https://example.test" },
        { "actionType": "terminal", "commandId": "deploy", "icon": "cloud", "name": "Deploy" },
        { "actionType": "terminal", "commandId": "dev", "icon": "code", "name": "Dev" },
    ])
}

#[test]
fn mobile_summary_lists_configured_global_actions() {
    let home = computer(true);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        global_definitions: global_definitions(),
        raw_global_rows: unconfigured_global_rows(),
        ..Daemon::default()
    });
    let summary = mobile_summary(home.path(), &daemon);
    assert_eq!(
        summary["globalQuickActions"],
        json!([
            { "actionType": "terminal", "commandId": "g-term", "icon": "bolt", "name": "Global Term" },
            { "actionType": "browser", "commandId": "g-web", "name": "Docs", "url": "http://localhost:3000/docs" },
            { "actionType": "terminal", "commandId": "g-unpinned", "name": "Unpinned" },
        ])
    );
    assert!(!summary.to_string().contains("echo global-marker"));
}

#[test]
fn mobile_summary_preserves_project_actions() {
    let home = computer(true);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        ..Daemon::default()
    });
    let summary = mobile_summary(home.path(), &daemon);
    assert_eq!(
        summary["quickActionsByProject"],
        json!({
            PROJECT_ID: expected_project_actions(),
            WORKTREE_PROJECT_ID: expected_project_actions(),
        })
    );
}

#[test]
fn mobile_summary_hides_actions_when_disabled() {
    let home = computer(false);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        global_definitions: global_definitions(),
        ..Daemon::default()
    });
    let summary = mobile_summary(home.path(), &daemon);
    assert_eq!(summary["globalQuickActions"], json!([]));
    assert_eq!(summary["quickActionsByProject"], json!({}));
    assert_eq!(summary["capabilities"], presentation_capabilities(true));
}

#[test]
fn mobile_summary_survives_hud_failure() {
    let home = computer(true);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        global_definitions: global_definitions(),
        hud_fails: true,
        ..Daemon::default()
    });
    let summary = mobile_summary(home.path(), &daemon);
    assert_eq!(summary["ok"], json!(true));
    assert_eq!(summary["sessions"], json!([]));
    assert!(summary.get("globalQuickActions").is_none());
    assert!(summary.get("quickActionsByProject").is_none());
    assert_eq!(daemon.requests_to("/api/readSidebarHud").len(), 1);
}

#[test]
fn global_action_capability_matches_cli_support() {
    // The daemon never promises the CLI's flag, so a CLI without it cannot advertise it.
    assert!(presentation_capabilities(true)
        .get("globalQuickActions")
        .is_none());

    let on = computer(true);
    let daemon = start(Daemon::default());
    let mut expected = presentation_capabilities(true);
    expected["globalQuickActions"] = json!(true);
    assert_eq!(mobile_summary(on.path(), &daemon)["capabilities"], expected);

    let off = computer(false);
    assert_eq!(
        mobile_summary(off.path(), &daemon)["capabilities"],
        presentation_capabilities(true)
    );

    let old_daemon = start(Daemon {
        capabilities: None,
        ..Daemon::default()
    });
    assert_eq!(
        mobile_summary(on.path(), &old_daemon)["capabilities"],
        json!({ "globalQuickActions": true })
    );
    assert!(mobile_summary(off.path(), &old_daemon)
        .get("capabilities")
        .is_none());
}

#[test]
fn run_action_global_uses_selected_project() {
    let home = computer(true);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        global_definitions: global_definitions(),
        ..Daemon::default()
    });
    let output = ghostex(
        home.path(),
        &daemon,
        &[
            "run-action",
            "g-term",
            "--project-id",
            PROJECT_ID,
            "--scope",
            "global",
        ],
    );
    assert_eq!(output.code, 0, "stderr: {}", output.stderr);
    let result = output.json();
    assert_eq!(result["ok"], json!(true));
    assert_eq!(result["actionType"], json!("terminal"));
    assert_eq!(result["commandId"], json!("g-term"));
    assert_eq!(result["session"]["sessionId"], json!(CREATED_SESSION_ID));
    assert_eq!(result["session"]["projectId"], json!(PROJECT_ID));
    let created = daemon.requests_to("/api/createSession");
    assert_eq!(created.len(), 1);
    // No cwd: gxserver starts a project's terminal in the project's own folder.
    assert_eq!(
        created[0],
        json!({
            "kind": "terminal",
            "launchSettings": { "startupCommand": "echo global-marker" },
            "projectId": PROJECT_ID,
            "title": "Global Term",
        })
    );
    assert_eq!(
        daemon.requests_to("/api/startSessionProvider"),
        vec![json!({ "projectId": PROJECT_ID, "sessionId": CREATED_SESSION_ID })]
    );
}

#[test]
fn run_action_scope_disambiguates_equal_ids() {
    let home = computer(true);
    let mut projects = projects_with_actions();
    projects[0]["customCommands"]
        .as_array_mut()
        .expect("project actions")
        .push(json!({ "command": "echo project-same", "commandId": "same", "name": "Same" }));
    let daemon = start(Daemon {
        projects,
        global_definitions: vec![
            json!({ "command": "echo global-same", "commandId": "same", "name": "Same" }),
        ],
        ..Daemon::default()
    });
    let runs: [&[&str]; 3] = [&[], &["--scope", "project"], &["--scope", "global"]];
    for scope in runs {
        let mut args = vec!["run-action", "same", "--project-id", PROJECT_ID];
        args.extend_from_slice(scope);
        let output = ghostex(home.path(), &daemon, &args);
        assert_eq!(output.code, 0, "{scope:?} stderr: {}", output.stderr);
    }
    let started: Vec<Value> = daemon
        .requests_to("/api/createSession")
        .iter()
        .map(|params| params["launchSettings"]["startupCommand"].clone())
        .collect();
    assert_eq!(
        started,
        vec![
            json!("echo project-same"),
            json!("echo project-same"),
            json!("echo global-same"),
        ]
    );
}

#[test]
fn run_action_rejects_invalid_scope_before_launch() {
    let home = computer(true);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        global_definitions: global_definitions(),
        ..Daemon::default()
    });
    let invalid: [&[&str]; 5] = [
        &["--scope", "bogus"],
        &["--scope", "Global"],
        &["--scope", ""],
        &["--scope="],
        &["--scope"],
    ];
    for scope in invalid {
        let mut args = vec!["run-action", "g-term", "--project-id", PROJECT_ID];
        args.extend_from_slice(scope);
        let output = ghostex(home.path(), &daemon, &args);
        assert_eq!(output.code, 1, "{scope:?} stdout: {}", output.stdout);
        assert!(
            output.stderr.contains("--scope"),
            "{scope:?} stderr: {}",
            output.stderr
        );
    }
    assert_eq!(daemon.request_count(), 0);
}

#[test]
fn run_action_missing_global_never_falls_back() {
    let home = computer(true);
    let mut projects = projects_with_actions();
    projects[0]["customCommands"]
        .as_array_mut()
        .expect("project actions")
        .push(
            json!({ "command": "echo project-only", "commandId": "only-project", "name": "Only" }),
        );
    let daemon = start(Daemon {
        projects,
        global_definitions: global_definitions(),
        raw_global_rows: unconfigured_global_rows(),
        ..Daemon::default()
    });
    for command_id in ["only-project", "g-empty", "g-no-url"] {
        let output = ghostex(
            home.path(),
            &daemon,
            &[
                "run-action",
                command_id,
                "--project-id",
                PROJECT_ID,
                "--scope",
                "global",
            ],
        );
        assert_eq!(output.code, 1, "{command_id}");
        let result = output.json();
        assert_eq!(result["ok"], json!(false), "{command_id}");
        assert_eq!(result["commandId"], json!(command_id));
        assert!(result["error"].is_string());
    }
    assert!(daemon.requests_to("/api/createSession").is_empty());

    let before = daemon.request_count();
    let no_project = ghostex(
        home.path(),
        &daemon,
        &["run-action", "g-term", "--scope", "global"],
    );
    assert_eq!(no_project.code, 1);
    assert!(
        no_project.stderr.contains("--project-id"),
        "{}",
        no_project.stderr
    );
    let no_command = ghostex(
        home.path(),
        &daemon,
        &[
            "run-action",
            "--project-id",
            PROJECT_ID,
            "--scope",
            "global",
        ],
    );
    assert_eq!(no_command.code, 1);
    assert!(
        no_command.stderr.contains("command id"),
        "{}",
        no_command.stderr
    );
    let off = computer(false);
    let disabled = ghostex(
        off.path(),
        &daemon,
        &[
            "run-action",
            "g-term",
            "--project-id",
            PROJECT_ID,
            "--scope",
            "global",
        ],
    );
    assert_eq!(disabled.code, 1);
    assert!(
        disabled.stderr.contains("actionsHidden"),
        "{}",
        disabled.stderr
    );
    assert_eq!(daemon.request_count(), before);
}

#[test]
fn run_action_requires_a_project_id_value() {
    let home = computer(true);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        global_definitions: global_definitions(),
        ..Daemon::default()
    });
    let bare: [&[&str]; 4] = [
        &["run-action", "g-term", "--project-id", "--scope", "global"],
        &["run-action", "g-web", "--project-id", "--scope", "global"],
        &[
            "run-action",
            "g-web",
            "--project-id",
            "--scope",
            "global",
            "--json",
        ],
        &["run-action", "deploy", "--project-id"],
    ];
    for args in bare {
        let output = ghostex(home.path(), &daemon, args);
        assert_eq!(output.code, 1, "{args:?} stdout: {}", output.stdout);
        assert!(
            format!("{}{}", output.stdout, output.stderr).contains("--project-id"),
            "{args:?} stdout: {} stderr: {}",
            output.stdout,
            output.stderr
        );
    }
    assert_eq!(daemon.request_count(), 0);
}

#[test]
fn run_action_global_browser_returns_url_without_session() {
    let home = computer(true);
    let daemon = start(Daemon {
        projects: projects_with_actions(),
        global_definitions: global_definitions(),
        ..Daemon::default()
    });
    let output = ghostex(
        home.path(),
        &daemon,
        &[
            "run-action",
            "g-web",
            "--project-id",
            PROJECT_ID,
            "--scope",
            "global",
        ],
    );
    assert_eq!(output.code, 0, "stderr: {}", output.stderr);
    assert_eq!(
        output.json(),
        json!({
            "actionType": "browser",
            "commandId": "g-web",
            "name": "Docs",
            "ok": true,
            "url": "http://localhost:3000/docs",
        })
    );
    assert!(daemon.requests_to("/api/createSession").is_empty());
}

#[test]
fn quick_actions_help_documents_scope() {
    let home = computer(true);
    let daemon = start(Daemon::default());
    for args in [
        &["run-action", "--help"][..],
        &["quick-actions", "--help"][..],
        &["guide", "features"][..],
    ] {
        let output = ghostex(home.path(), &daemon, args);
        assert_eq!(output.code, 0, "{args:?} stderr: {}", output.stderr);
        assert!(
            output.stdout.contains("--scope global"),
            "{args:?} does not document --scope global"
        );
    }
    assert_eq!(daemon.request_count(), 0);
}
