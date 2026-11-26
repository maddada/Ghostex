//! What a caller of the DevTools relay (cef_devtools.rs) may drive: which pages, which commands,
//! and which files it may put in a page's upload field.
//!
//! CDXC:Browser 2026-11-26 DECISION:
//! Coordinator, on the user's behalf (security review of #762, findings B1 and B2): the relay is the one place every DevTools caller passes, so the limits live here and not in `ghostex browser mcp`, which an agent can skip by opening the page socket itself. A session's own token (`gxs_`) reaches only its own project's Browser tabs (the app's `listBrowserPages` answer, never Ghostex's own pages, Live pages or another project's tabs) and only the commands the browser tools send (`SESSION_METHODS`); `Browser.*`, `Target.*`, cookie reads, download behaviour and file drags are refused, a navigation must be http, https or about:blank, and an upload may only name files in the session's project folder, its own folder (worktree) or Ghostex's attachment folders, resolved after symlinks and checked with gxserver's own paths, never the CLI's `HOME` or `GHOSTEX_HOME`. The token file, the owner session and a paired phone keep the full protocol (the same user, by design). Every upload goes into the security log with its files, the page's host and who asked.
//! Coordinator, on the user's behalf (re-check of #762, N2): `ghostex_click` decides its mouse press through DevTools' DOM domain instead of the page's script (server/src/ghostex_cli/browser_mcp/click_hit_test.rs), so a session may also read the DOM: find an element (`DOM.getDocument` without piercing, `DOM.querySelector`), describe a node and its subtree (`DOM.describeNode`, which may pierce shadow roots and the documents of frames the page holds in its own process, that is same-site frames: cross-site frames run in other processes and stay out of reach), its boxes (`DOM.getContentQuads`), the node at a point (`DOM.getNodeForLocation`), and the page's frames and view size (`Page.getFrameTree`, `Page.getLayoutMetrics`). All of them only read.
//! Coordinator, on the user's behalf (re-check 8 of #762, S5): the click's `:active` and `:disabled` checks list the matching elements with `DOM.querySelectorAll` (which only reads, like `DOM.querySelector`) instead of splicing the agent's selector into another one.
//! SEE-ALSO: server/src/ghostex_cli/browser_mcp/mcp_tools.rs (the tools these commands serve), apps/desktop/src/cef/shell/devtools_relay.rs (the host, which attaches a scoped session only by the page's DevTools target id).

use super::*;

/// The most files one upload may put in a field.
const MAX_UPLOAD_FILES: usize = 20;
/// How long the relay waits for the app's list of a project's pages.
const PROJECT_PAGES_TIMEOUT_MS: u64 = 5_000;

/// The commands a session's own token may send: the ones `ghostex browser mcp` uses.
const SESSION_METHODS: &[&str] = &[
    "DOM.describeNode",
    "DOM.getContentQuads",
    "DOM.getDocument",
    "DOM.getNodeForLocation",
    "DOM.querySelector",
    "DOM.querySelectorAll",
    "DOM.setFileInputFiles",
    "Input.dispatchKeyEvent",
    "Input.dispatchMouseEvent",
    "Log.enable",
    "Page.captureScreenshot",
    "Page.enable",
    "Page.getFrameTree",
    "Page.getLayoutMetrics",
    "Page.navigate",
    "Runtime.enable",
    "Runtime.evaluate",
    "Runtime.releaseObject",
];

/// Who opened a relay socket, as far as the relay's limits go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RelayCaller {
    /// The token file, the owner session or a paired phone: every page, every command.
    Full,
    /// A session's own token: its project's Browser tabs and the browser tools' commands.
    Session {
        project_id: String,
        session_id: String,
    },
}

/// The caller behind the request's token, or `None` when the relay does not take it.
pub(super) fn relay_caller(state: &AppState, headers: &HeaderMap) -> Option<RelayCaller> {
    let token = crate::auth::bearer_token(headers)?;
    if let Some(crate::security::Principal::Session {
        project_id,
        session_id,
        ..
    }) = state.security.session_tokens.resolve(token)
    {
        return Some(RelayCaller::Session {
            project_id,
            session_id,
        });
    }
    crate::auth::is_authorized_client_token(
        token,
        &state.auth_token,
        &state.device_api_tokens,
        &state.security.session_tokens,
    )
    .then_some(RelayCaller::Full)
}

/// The caller for the security log.
pub(super) fn audit_caller(state: &AppState, headers: &HeaderMap) -> crate::audit::Caller {
    crate::audit::Caller::from_headers(
        headers,
        &state.auth_token,
        &state.device_api_tokens,
        &state.security.session_tokens,
    )
}

/// The DevTools target ids of `project_id`'s Browser tabs, live or parked, as the app lists them.
pub(super) async fn project_page_ids(
    state: &AppState,
    project_id: &str,
) -> Result<HashSet<String>, String> {
    let mut payload = Map::new();
    payload.insert("projectId".to_string(), json!(project_id));
    let answer = state
        .event_hub
        .dispatch_renderer_command(
            "listBrowserPages".to_string(),
            payload,
            PROJECT_PAGES_TIMEOUT_MS,
        )
        .await
        .map_err(|error| error.message)?;
    Ok(page_ids_of_answer(&answer))
}

fn page_ids_of_answer(answer: &Value) -> HashSet<String> {
    answer
        .get("pages")
        .and_then(Value::as_array)
        .map(|pages| {
            pages
                .iter()
                .filter_map(|page| page.get("targetId").and_then(Value::as_str))
                .filter(|target_id| !target_id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Where a session may upload files from, each resolved: its project's folder, its own folder
/// (a worktree), and Ghostex's attachment and image folders. A folder that is the home folder, or
/// holds it, is left out, so a session started in `~` does not open the whole home folder.
pub(super) fn session_upload_roots(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Vec<PathBuf> {
    let mut candidates = vec![
        state.paths.app_data_dir.join("f"),
        state.paths.app_data_dir.join("i"),
    ];
    if let Ok(db) = crate::storage::open_gxserver_database(&state.paths) {
        let repository =
            crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
        let text = |row: &Value, key: &str| {
            row.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        if let Some(project) = repository
            .get_project_without_session_history(project_id)
            .ok()
            .flatten()
        {
            candidates.extend(text(&project, "path"));
        }
        if let Some(session) = repository
            .get_session(project_id, session_id)
            .ok()
            .flatten()
        {
            candidates.extend(text(&session, "cwd"));
        }
    }
    let home = fs::canonicalize(&state.paths.home_dir).unwrap_or(state.paths.home_dir.clone());
    upload_roots_from(candidates, &home)
}

fn upload_roots_from(candidates: Vec<PathBuf>, home: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if !candidate.is_absolute() {
            continue;
        }
        let Ok(root) = fs::canonicalize(&candidate) else {
            continue;
        };
        if !root.is_dir() || home.starts_with(&root) || roots.contains(&root) {
            continue;
        }
        roots.push(root);
    }
    roots
}

/// Folders no upload may come from even inside an allowed folder: Ghostex's own state, settings
/// and cache (its tokens, and the Browser's cookies), and the home folder's credential folders.
pub(super) fn refused_upload_folders(state: &AppState) -> Vec<PathBuf> {
    let paths = &state.paths;
    let mut folders = vec![
        paths.app_state_dir.clone(),
        paths.app_config_dir.clone(),
        paths.app_cache_dir.clone(),
        paths.root_dir.clone(),
        paths.auth_dir.clone(),
    ];
    for entry in [
        ".ssh",
        ".gnupg",
        ".aws",
        ".azure",
        ".kube",
        ".docker",
        ".config/gcloud",
        ".config/gh",
        ".password-store",
        ".netrc",
        ".git-credentials",
        "Library/Keychains",
    ] {
        folders.push(paths.home_dir.join(entry));
    }
    folders
        .into_iter()
        .map(|folder| fs::canonicalize(&folder).unwrap_or(folder))
        .collect()
}

/// The limits one relay socket checks each command against.
#[derive(Clone, Debug, Default)]
pub(super) struct RelayLimits {
    /// A session's own token: only `SESSION_METHODS`, web navigations, and uploads from
    /// `upload_roots`. A full caller may send any command.
    pub(super) scoped: bool,
    /// Where uploads may come from; `None` for a full caller, which may name any file.
    pub(super) upload_roots: Option<Vec<PathBuf>>,
    /// Where no upload may come from, for every caller (`refused_upload_folders`).
    pub(super) refused_folders: Vec<PathBuf>,
}

impl RelayLimits {
    pub(super) fn for_caller(state: &AppState, caller: &RelayCaller) -> Self {
        match caller {
            RelayCaller::Full => RelayLimits {
                scoped: false,
                upload_roots: None,
                refused_folders: refused_upload_folders(state),
            },
            RelayCaller::Session {
                project_id,
                session_id,
            } => RelayLimits {
                scoped: true,
                upload_roots: Some(session_upload_roots(state, project_id, session_id)),
                refused_folders: refused_upload_folders(state),
            },
        }
    }
}

/// What the relay does with one command from a client.
#[derive(Debug, PartialEq)]
pub(super) enum Checked {
    /// Send it on (possibly rewritten: upload paths are replaced with the files they resolve to).
    Forward(String),
    /// An upload to send on: the command and the files it sets, for the security log.
    Upload {
        command: String,
        id: Value,
        files: Vec<String>,
    },
    /// Answer the client with this error and send nothing to the page.
    Refuse {
        id: Value,
        message: String,
        files: Option<Vec<String>>,
    },
}

/// Checks one client frame against `limits`. A full caller's frames go on as they are, except
/// that its uploads are checked against the refused folders and logged. Every frame is parsed
/// before its method is read, so a method name written with JSON escapes is the method the page
/// runs (re-check of #762, N7).
pub(super) fn check_command(text: &str, limits: &RelayLimits) -> Checked {
    let Ok(mut command) = serde_json::from_str::<Value>(text) else {
        if !limits.scoped {
            return Checked::Forward(text.to_string());
        }
        return Checked::Refuse {
            id: Value::Null,
            message: "The DevTools command is not JSON.".to_string(),
            files: None,
        };
    };
    let id = command.get("id").cloned().unwrap_or(Value::Null);
    let refuse = |message: String| Checked::Refuse {
        id: id.clone(),
        message,
        files: None,
    };
    let Some(method) = method_of(&command).map(str::to_string) else {
        if !limits.scoped {
            return Checked::Forward(text.to_string());
        }
        return refuse("The DevTools command has no method.".to_string());
    };
    if !limits.scoped && method != "DOM.setFileInputFiles" {
        return Checked::Forward(text.to_string());
    }
    if limits.scoped && command.get("sessionId").is_some() {
        return refuse(
            "A session's browser tools reach only the page itself, not other targets.".to_string(),
        );
    }
    if limits.scoped && !SESSION_METHODS.contains(&method.as_str()) {
        return refuse(format!(
            "{method} is not available to an agent's browser tools. They may use: {}.",
            SESSION_METHODS.join(", ")
        ));
    }
    match method.as_str() {
        "Page.navigate" => {
            let url = command
                .pointer("/params/url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(message) = crate::browser_agent_limits::agent_navigation_refusal(url) {
                return refuse(message);
            }
        }
        // A key event's editor commands (paste, copy, cut, ...) would reach the system clipboard,
        // which a sandboxed agent cannot otherwise read or replace (re-check of #762, N6).
        "Input.dispatchKeyEvent"
            if command
                .pointer("/params/commands")
                .is_some_and(|commands| !commands.is_null()) =>
        {
            return refuse(
                "An agent's key presses cannot carry editor commands (commands); type the keys instead."
                    .to_string(),
            );
        }
        // The whole document at once, through frames, is more than finding an element needs.
        "DOM.getDocument" if command.pointer("/params/pierce") == Some(&Value::Bool(true)) => {
            return refuse(
                "An agent's browser tools read the document without piercing (pierce); describe one node with DOM.describeNode instead."
                    .to_string(),
            );
        }
        "Runtime.evaluate" => {
            let params = command.get("params");
            if ["contextId", "uniqueContextId"]
                .iter()
                .any(|key| params.and_then(|params| params.get(*key)).is_some())
            {
                return refuse(
                    "An agent's browser tools run scripts in the page's own context only."
                        .to_string(),
                );
            }
        }
        "DOM.setFileInputFiles" => {
            let given = string_list(command.pointer("/params/files"));
            return match checked_upload_files(&given, limits) {
                Ok(files) => {
                    command["params"]["files"] = json!(files);
                    Checked::Upload {
                        command: command.to_string(),
                        id,
                        files,
                    }
                }
                Err(message) => Checked::Refuse {
                    id,
                    message,
                    files: Some(given),
                },
            };
        }
        _ => {}
    }
    Checked::Forward(text.to_string())
}

fn method_of(command: &Value) -> Option<&str> {
    command.get("method").and_then(Value::as_str)
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Each file resolved after symlinks, or why one may not be uploaded.
fn checked_upload_files(given: &[String], limits: &RelayLimits) -> Result<Vec<String>, String> {
    if given.is_empty() {
        return Err("An upload needs at least one file.".to_string());
    }
    if given.len() > MAX_UPLOAD_FILES {
        return Err(format!(
            "An upload takes at most {MAX_UPLOAD_FILES} files at a time."
        ));
    }
    given
        .iter()
        .map(|path| {
            let path = Path::new(path.trim());
            if !path.is_absolute() {
                return Err(format!(
                    "{} is not an absolute path; give the file's full path.",
                    path.display()
                ));
            }
            let resolved =
                fs::canonicalize(path).map_err(|_| format!("{} does not exist.", path.display()))?;
            if !resolved.is_file() {
                return Err(format!("{} is not a file.", path.display()));
            }
            if let Some(folder) = limits
                .refused_folders
                .iter()
                .find(|folder| resolved.starts_with(folder))
            {
                return Err(format!(
                    "{} is inside {}, which holds credentials or Ghostex's own data; the browser tools never upload from there.",
                    path.display(),
                    folder.display()
                ));
            }
            let Some(upload_roots) = &limits.upload_roots else {
                return Ok(resolved.to_string_lossy().to_string());
            };
            if !upload_roots.iter().any(|root| resolved.starts_with(root)) {
                let roots = upload_roots
                    .iter()
                    .map(|root| root.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "{} is outside this session's project folders ({roots}); an agent's browser tools upload only files from there.",
                    path.display()
                ));
            }
            Ok(resolved.to_string_lossy().to_string())
        })
        .collect()
}

/// The CDP error answer to a refused command.
pub(super) fn refusal_reply(id: &Value, message: &str) -> String {
    json!({ "id": id, "error": { "code": -32000, "message": message } }).to_string()
}

/// Writes one upload into the security log.
pub(super) fn log_upload(
    caller: &crate::audit::Caller,
    page_url: Option<&str>,
    files: &[String],
    outcome: crate::audit::Outcome,
) {
    let host = page_url
        .and_then(|url| url::Url::parse(url).ok())
        .and_then(|url| url.host_str().map(str::to_string))
        .or_else(|| page_url.map(str::to_string))
        .unwrap_or_else(|| "an unknown page".to_string());
    let action = crate::audit::Action::new(
        "browser.upload",
        "Put files in a Browser page's upload field",
    )
    .target(format!("{host}: {}", files.join(", ")));
    let actor = crate::audit::actor_of_caller(caller);
    crate::audit::record(action, actor, outcome, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "gxserver-relay-scope-{name}-{}-{}",
                std::process::id(),
                Uuid::new_v4()
            ));
            fs::create_dir_all(&path).expect("test folder");
            Folder(fs::canonicalize(&path).expect("canonical test folder"))
        }

        fn file(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("parent folder");
            fs::write(&path, "x").expect("test file");
            path
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn command(method: &str, params: Value) -> String {
        json!({ "id": 7, "method": method, "params": params }).to_string()
    }

    fn refused(checked: &Checked) -> bool {
        matches!(checked, Checked::Refuse { .. })
    }

    fn session_limits(upload_roots: Vec<PathBuf>, refused_folders: Vec<PathBuf>) -> RelayLimits {
        RelayLimits {
            scoped: true,
            upload_roots: Some(upload_roots),
            refused_folders,
        }
    }

    #[test]
    fn a_session_may_send_only_the_browser_tools_commands() {
        let limits = session_limits(vec![], vec![]);
        for method in [
            "Browser.close",
            "Browser.setDownloadBehavior",
            "Page.setDownloadBehavior",
            "Target.createTarget",
            "Target.attachToTarget",
            "Network.getAllCookies",
            "Network.getCookies",
            "Storage.getCookies",
            "Input.dispatchDragEvent",
            "DOM.setFileInputFiles2",
        ] {
            assert!(
                refused(&check_command(&command(method, json!({})), &limits)),
                "{method} must be refused"
            );
        }
        for method in [
            "Runtime.enable",
            "Page.captureScreenshot",
            "Input.dispatchMouseEvent",
        ] {
            let text = command(method, json!({}));
            assert_eq!(
                check_command(&text, &limits),
                Checked::Forward(text.clone())
            );
        }
        // A command for a child target is refused whatever its method.
        let child =
            json!({ "id": 1, "method": "Runtime.evaluate", "sessionId": "X", "params": {} });
        assert!(refused(&check_command(&child.to_string(), &limits)));
        // Another script context (an isolated world) is out of reach.
        let isolated = command(
            "Runtime.evaluate",
            json!({ "expression": "1", "contextId": 3 }),
        );
        assert!(refused(&check_command(&isolated, &limits)));
    }

    #[test]
    fn a_full_caller_may_send_anything_but_credential_uploads() {
        let full = |refused_folders: Vec<PathBuf>| RelayLimits {
            scoped: false,
            upload_roots: None,
            refused_folders,
        };
        let text = command("Network.getAllCookies", json!({}));
        assert_eq!(
            check_command(&text, &full(vec![])),
            Checked::Forward(text.clone())
        );
        let home = Folder::new("full");
        let anywhere = home.file("Downloads/photo.png");
        let key = home.file(".ssh/id_ed25519");
        let limits = full(vec![home.0.join(".ssh")]);
        let upload = |file: &PathBuf| {
            check_command(
                &command(
                    "DOM.setFileInputFiles",
                    json!({ "files": [file.to_string_lossy()], "objectId": "1" }),
                ),
                &limits,
            )
        };
        assert!(matches!(upload(&anywhere), Checked::Upload { .. }));
        assert!(refused(&upload(&key)));
        // A method name written with JSON escapes is still the upload it decodes to.
        let escaped = format!(
            r#"{{"id":8,"method":"DOM.setFileInputFiles","params":{{"files":[{}],"objectId":"1"}}}}"#,
            json!(key.to_string_lossy())
        );
        assert!(refused(&check_command(&escaped, &limits)));
        let escaped_anywhere = escaped.replace(
            &json!(key.to_string_lossy()).to_string(),
            &json!(anywhere.to_string_lossy()).to_string(),
        );
        assert!(matches!(
            check_command(&escaped_anywhere, &limits),
            Checked::Upload { .. }
        ));
    }

    #[test]
    fn a_sessions_key_presses_carry_no_editor_commands() {
        let limits = session_limits(vec![], vec![]);
        for commands in [json!(["paste"]), json!(["copy"]), json!([])] {
            let text = command(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyDown", "key": "v", "commands": commands }),
            );
            assert!(refused(&check_command(&text, &limits)), "{commands}");
        }
        let plain = command(
            "Input.dispatchKeyEvent",
            json!({ "type": "keyDown", "key": "v", "text": "v" }),
        );
        assert_eq!(
            check_command(&plain, &limits),
            Checked::Forward(plain.clone())
        );
    }

    #[test]
    fn a_session_navigates_only_to_web_pages() {
        let limits = session_limits(vec![], vec![]);
        for url in [
            "file:///etc/passwd",
            "FILE:///Users/me/.ssh/id_ed25519",
            "chrome://settings",
            "devtools://devtools/bundled/inspector.html",
            "javascript:alert(1)",
            "data:text/html,hi",
            "view-source:https://example.com",
            "localhost:3000",
        ] {
            let text = command("Page.navigate", json!({ "url": url }));
            assert!(refused(&check_command(&text, &limits)), "{url}");
        }
        for url in [
            "https://example.com",
            "http://localhost:3000/x",
            "about:blank",
        ] {
            let text = command("Page.navigate", json!({ "url": url }));
            assert_eq!(
                check_command(&text, &limits),
                Checked::Forward(text.clone()),
                "{url}"
            );
        }
    }

    #[test]
    fn uploads_come_only_from_the_sessions_folders() {
        let project = Folder::new("project");
        let outside = Folder::new("outside");
        let inside = project.file("docs/report.pdf");
        let secret = outside.file("id_ed25519");
        let limits = session_limits(vec![project.0.clone()], vec![]);
        let upload = |files: Value| {
            check_command(
                &command(
                    "DOM.setFileInputFiles",
                    json!({ "files": files, "objectId": "1" }),
                ),
                &limits,
            )
        };
        match upload(json!([inside.to_string_lossy()])) {
            Checked::Upload { files, .. } => {
                assert_eq!(files, vec![inside.to_string_lossy().to_string()])
            }
            other => panic!("the project's file must upload: {other:?}"),
        }
        assert!(refused(&upload(json!([secret.to_string_lossy()]))));
        assert!(refused(&upload(json!(["docs/report.pdf"]))));
        assert!(refused(&upload(json!([project.0.to_string_lossy()]))));
        assert!(refused(&upload(json!([]))));
        // A link inside the project to a file outside it resolves to the outside file.
        let link = project.0.join("innocent.txt");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, &link).expect("symlink");
            assert!(refused(&upload(json!([link.to_string_lossy()]))));
        }
        // `..` out of the project resolves outside it too.
        let climb = project.0.join("docs/../..").join(
            outside
                .0
                .file_name()
                .expect("name")
                .to_string_lossy()
                .to_string(),
        );
        assert!(refused(&upload(json!([climb
            .join("id_ed25519")
            .to_string_lossy()]))));
    }

    #[test]
    fn a_refused_folder_wins_over_an_allowed_one() {
        let project = Folder::new("refused");
        let token = project.file("state/auth/token");
        let limits = session_limits(vec![project.0.clone()], vec![project.0.join("state")]);
        let text = command(
            "DOM.setFileInputFiles",
            json!({ "files": [token.to_string_lossy()], "objectId": "1" }),
        );
        assert!(refused(&check_command(&text, &limits)));
    }

    #[test]
    fn the_home_folder_is_never_an_upload_root() {
        let home = Folder::new("home");
        let project = home.0.join("dev/app");
        fs::create_dir_all(&project).expect("project");
        let parent = home.0.parent().expect("parent").to_path_buf();
        let roots = upload_roots_from(
            vec![
                home.0.clone(),
                parent,
                project.clone(),
                PathBuf::from("relative"),
            ],
            &home.0,
        );
        assert_eq!(roots, vec![fs::canonicalize(project).expect("canonical")]);
    }

    #[test]
    fn the_apps_page_list_gives_the_target_ids() {
        let answer = json!({
            "ok": true,
            "projectId": "P1",
            "pages": [
                { "tabId": 1, "targetId": "A" },
                { "tabId": 2, "targetId": null },
                { "tabId": 3, "targetId": "" },
            ],
        });
        assert_eq!(
            page_ids_of_answer(&answer),
            HashSet::from(["A".to_string()])
        );
    }
}
