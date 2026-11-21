//! Add Project preview. The host answers every gxserver round trip from a fixture tree of three
//! computers (this Mac, a Linux box and a Windows PC with its drive list), with each folder's Git
//! details and each computer's sidebar places, after `latency`.
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (the picker on ~/), `single` (one machine),
//! `browse` (~/dev open), `highlight` (two rows down), `create` (an unknown leaf: Create & Add),
//! `newfolder` (naming a new folder), `newfolder-made` (it made and selected), `inside-repo` (a
//! folder inside a repository), `repository` (Clone a repo on GitHub), `clone` (a URL typed),
//! `lookupfail`, `blocked` (a refused destination and a bad branch), `cloning` (a running clone),
//! `error` (the add fails), `slow` (the still-working notice), `remote` (the Linux box),
//! `windows` (the Windows PC's drive list), `windows-folder` (a folder on it), `suggested`,
//! `machines` (the computer menu), `noproviders`, `oldserver` (a machine without places),
//! `projectsfolder` (a Default projects folder of ~/dev), `projectsfolder-add` (New project named
//! `widgets`, then Create), `newproject`, `newproject-start` (its Start list), `ambiguous`
//! (owner/repo typed) and `pasted-url`. `script:<step>;<step>` runs any `preview_step` steps.
use super::add_project_modal::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

const HOME: &str = "/Users/story";
const REMOTE_HOME: &str = "/home/story";
const WINDOWS_HOME: &str = "C:\\Users\\story";

/// One fixture folder: its name and what it is (`git` branch, language, already a project).
#[derive(Clone, Copy)]
struct Folder {
    name: &'static str,
    branch: Option<&'static str>,
    language: Option<&'static str>,
    project: bool,
}

const fn folder(name: &'static str) -> Folder {
    Folder {
        name,
        branch: None,
        language: None,
        project: false,
    }
}

const fn repo(name: &'static str, branch: &'static str, language: &'static str) -> Folder {
    Folder {
        name,
        branch: Some(branch),
        language: Some(language),
        project: false,
    }
}

const fn project(name: &'static str, branch: &'static str, language: &'static str) -> Folder {
    Folder {
        name,
        branch: Some(branch),
        language: Some(language),
        project: true,
    }
}

fn tree() -> HashMap<&'static str, Vec<Folder>> {
    HashMap::from([
        (
            "/",
            vec![
                folder("Applications"),
                folder("Library"),
                folder("Users"),
                folder("Volumes"),
                folder("opt"),
                folder("home"),
                folder("srv"),
            ],
        ),
        ("/Users/", vec![folder("Shared"), folder("story")]),
        ("/Volumes/", vec![folder("Backup SSD"), folder("Scratch")]),
        (
            "/Volumes/Scratch/",
            vec![folder("archive"), folder("old-projects")],
        ),
        (
            "/Volumes/Scratch/old-projects/",
            vec![
                repo("game-jam", "main", "C#"),
                repo("ray-tracer", "main", "C++"),
            ],
        ),
        (
            "/Users/story/",
            vec![
                folder(".config"),
                folder("Desktop"),
                folder("Documents"),
                folder("Downloads"),
                folder("code"),
                folder("dev"),
                folder("work"),
            ],
        ),
        (
            "/Users/story/Documents/",
            vec![folder("experiments"), folder("invoices")],
        ),
        (
            "/Users/story/Documents/experiments/",
            vec![repo("llm-eval", "eval-v2", "Python")],
        ),
        (
            "/Users/story/dev/",
            vec![
                folder(".cache"),
                project("ghostex", "main", "Rust"),
                repo("ghostex-web", "feat/hero", "TypeScript"),
                folder("playground"),
                repo("rust-notes", "main", "Rust"),
                folder("scratch"),
            ],
        ),
        (
            "/Users/story/dev/ghostex/",
            vec![folder("gpui"), folder("shared"), folder("sidebar")],
        ),
        (
            "/Users/story/dev/playground/",
            vec![folder("alpha"), folder("beta")],
        ),
        (
            "/Users/story/code/",
            vec![
                repo("dotfiles", "main", "Shell"),
                repo("tinyhttp", "main", "Rust"),
            ],
        ),
        (
            "/Users/story/work/",
            vec![
                project("client-portal", "release/2.4", "TypeScript"),
                repo("infra", "main", "HCL"),
                folder("handbook"),
            ],
        ),
        ("/home/", vec![folder("story")]),
        (
            "/home/story/",
            vec![folder("projects"), folder("src"), folder("Downloads")],
        ),
        (
            "/home/story/projects/",
            vec![
                project("api", "main", "Go"),
                repo("worker", "main", "Go"),
                folder("deploy-scripts"),
            ],
        ),
        ("/srv/", vec![folder("deploy"), folder("projects")]),
        (
            "C:\\",
            vec![
                folder("dev"),
                folder("Program Files"),
                folder("Users"),
                folder("Windows"),
            ],
        ),
        ("C:\\Users\\", vec![folder("Public"), folder("story")]),
        (
            "C:\\Users\\story\\",
            vec![
                folder("Desktop"),
                folder("Documents"),
                folder("Downloads"),
                folder("source"),
            ],
        ),
        ("C:\\Users\\story\\source\\", vec![folder("repos")]),
        (
            "C:\\Users\\story\\source\\repos\\",
            vec![
                project("StoreApp", "main", "C#"),
                repo("pdf-export", "dev", "C#"),
            ],
        ),
        (
            "D:\\",
            vec![folder("code"), folder("work"), folder("Media")],
        ),
        ("D:\\work\\", vec![repo("portal", "main", "TypeScript")]),
    ])
}

#[derive(Default)]
struct Mock {
    created: HashMap<String, Vec<String>>,
    polls: HashMap<String, u32>,
    destinations: HashMap<String, String>,
    canceled: Vec<String>,
    pinned: Vec<String>,
}

struct Options {
    add_error: Option<&'static str>,
    lookup_error: Option<&'static str>,
    discovery_unavailable: bool,
    running_polls: u32,
    blocked: bool,
    /// Answers `places` like a gxserver from before the picker: unknown endpoint.
    old_server: bool,
}

fn home_of(machine_id: &str) -> &'static str {
    match machine_id {
        "machine-bigbox" => REMOTE_HOME,
        "machine-studio" => WINDOWS_HOME,
        _ => HOME,
    }
}

fn separator_of(path: &str) -> char {
    if path.contains('\\') { '\\' } else { '/' }
}

fn expand(value: &str, home: &str) -> String {
    let separator = separator_of(home);
    if value == "~" || value == "~/" || value == "~\\" {
        return format!("{home}{separator}");
    }
    match value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        Some(rest) => format!("{home}{separator}{rest}"),
        None => value.to_string(),
    }
}

fn trim_separator(value: &str) -> String {
    let trimmed = value.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        return value.to_string();
    }
    trimmed.to_string()
}

impl Mock {
    fn folders(&self, parent: &str) -> Vec<Folder> {
        let mut folders = tree().get(parent).cloned().unwrap_or_default();
        let key = trim_separator(parent);
        // Created folders are layered over the fixture and the listing re-sorted.
        if let Some(created) = self.created.get(&key) {
            for name in created {
                if !folders.iter().any(|folder| folder.name == name) {
                    folders.push(folder(Box::leak(name.clone().into_boxed_str())));
                }
            }
        }
        folders.sort_by_key(|folder| (folder.name.starts_with('.'), folder.name.to_lowercase()));
        folders
    }

    fn find(&self, path: &str) -> Option<Folder> {
        let trimmed = trim_separator(path);
        let index = trimmed.rfind(['/', '\\'])?;
        let parent = &trimmed[..=index];
        let name = &trimmed[index + 1..];
        self.folders(parent)
            .into_iter()
            .find(|folder| folder.name == name)
    }

    fn exists(&self, path: &str) -> bool {
        let directory = format!("{}{}", trim_separator(path), separator_of(path));
        tree().contains_key(directory.as_str())
            || self.created.contains_key(&trim_separator(path))
            || self.find(path).is_some()
    }

    fn browse(&self, machine_id: &str, partial: &str, inspect: Option<&str>) -> Value {
        if machine_id == "machine-studio" && (partial == "/" || partial == "\\") {
            return json!({
                "entries": [
                    { "fullPath": "C:\\Users\\story", "name": "Home (C:\\Users\\story)" },
                    { "fullPath": "C:\\", "name": "C:/" },
                    { "fullPath": "D:\\", "name": "D:/" },
                ],
                "isDriveList": true,
                "parentPath": "/",
            });
        }
        let home = home_of(machine_id);
        let expanded = expand(partial, home);
        let parent = if expanded.ends_with(['/', '\\']) {
            expanded.clone()
        } else {
            expanded[..expanded
                .rfind(['/', '\\'])
                .map(|index| index + 1)
                .unwrap_or(0)]
                .to_string()
        };
        let separator = separator_of(&parent);
        let entries: Vec<Value> = self
            .folders(&parent)
            .iter()
            .map(|folder| {
                let mut entry =
                    json!({ "fullPath": format!("{parent}{}", folder.name), "name": folder.name });
                if let Some(branch) = folder.branch {
                    entry["git"] = json!(true);
                    entry["branch"] = json!(branch);
                }
                if let Some(language) = folder.language {
                    entry["language"] = json!(language);
                }
                if folder.project {
                    entry["projectId"] = json!(format!("project-{}", folder.name));
                }
                entry
            })
            .collect();
        let parent_path = trim_separator(&parent);
        let mut result = json!({ "entries": entries, "parentPath": parent_path });
        if let Some(inspect) = inspect {
            let path = trim_separator(&expand(inspect, home));
            let exists = self.exists(&path);
            let mut inspection =
                json!({ "path": path, "kind": if exists { "directory" } else { "missing" } });
            if let Some(found) = self.find(&path) {
                if found.project {
                    inspection["projectId"] = json!(format!("project-{}", found.name));
                    inspection["projectPath"] = json!(path);
                }
                if let Some(branch) = found.branch {
                    inspection["branch"] = json!(branch);
                }
                if let Some(language) = found.language {
                    inspection["language"] = json!(language);
                }
            }
            // The nearest folder above with a branch is the repository this one sits inside.
            let mut current = path.clone();
            while let Some(index) = current.rfind(['/', '\\']) {
                current.truncate(index);
                if current.is_empty() {
                    break;
                }
                if self
                    .find(&current)
                    .is_some_and(|folder| folder.branch.is_some())
                {
                    inspection["repoRoot"] = json!(current);
                    break;
                }
            }
            result["inspection"] = inspection;
        }
        let _ = separator;
        result
    }

    fn places(&self, machine_id: &str) -> Value {
        let pinned: Vec<Value> = self
            .pinned
            .iter()
            .map(|path| json!({ "name": path.rsplit(['/', '\\']).next().unwrap_or(path), "path": path }))
            .collect();
        match machine_id {
            "machine-bigbox" => json!({
                "home": REMOTE_HOME,
                "platform": "linux",
                "volumes": [
                    { "name": "/", "path": "/", "kind": "startup" },
                    { "name": "data", "path": "/mnt/data", "kind": "external" },
                ],
                "standard": [{ "name": "Downloads", "path": "/home/story/Downloads" }],
                "projectFolders": [
                    { "name": "projects", "path": "/home/story/projects" },
                    { "name": "src", "path": "/home/story/src" },
                ],
                "otherApps": [],
                "recent": [],
                "pinned": pinned,
                "suggested": [
                    { "name": "worker", "path": "/home/story/projects/worker", "branch": "main", "language": "Go", "source": "projectFolder", "from": "projects" },
                ],
            }),
            "machine-studio" => json!({
                "home": WINDOWS_HOME,
                "platform": "win32",
                "volumes": [
                    { "name": "Local Disk (C:)", "path": "C:/", "kind": "drive" },
                    { "name": "Data (D:)", "path": "D:/", "kind": "drive" },
                    { "name": "Ubuntu (WSL)", "path": "\\\\wsl.localhost\\Ubuntu\\home", "kind": "wsl" },
                ],
                "standard": [
                    { "name": "Desktop", "path": "C:\\Users\\story\\Desktop" },
                    { "name": "Documents", "path": "C:\\Users\\story\\Documents" },
                    { "name": "Downloads", "path": "C:\\Users\\story\\Downloads" },
                ],
                "projectFolders": [
                    { "name": "repos", "path": "C:\\Users\\story\\source\\repos" },
                    { "name": "dev", "path": "C:\\dev" },
                ],
                "otherApps": [{ "name": "portal", "path": "D:\\work\\portal" }],
                "recent": [{ "name": "work", "path": "D:\\work" }],
                "pinned": pinned,
                "suggested": [
                    { "name": "portal", "path": "D:\\work\\portal", "branch": "main", "language": "TypeScript", "source": "otherApps" },
                    { "name": "pdf-export", "path": "C:\\Users\\story\\source\\repos\\pdf-export", "branch": "dev", "language": "C#", "source": "projectFolder", "from": "repos" },
                ],
            }),
            _ => json!({
                "home": HOME,
                "platform": "darwin",
                "volumes": [
                    { "name": "Macintosh HD", "path": "/", "kind": "startup" },
                    { "name": "Backup SSD", "path": "/Volumes/Backup SSD", "kind": "external" },
                    { "name": "Scratch", "path": "/Volumes/Scratch", "kind": "external" },
                ],
                "standard": [
                    { "name": "Desktop", "path": "/Users/story/Desktop" },
                    { "name": "Documents", "path": "/Users/story/Documents" },
                    { "name": "Downloads", "path": "/Users/story/Downloads" },
                ],
                "projectFolders": [
                    { "name": "dev", "path": "/Users/story/dev" },
                    { "name": "code", "path": "/Users/story/code" },
                    { "name": "work", "path": "/Users/story/work" },
                ],
                "otherApps": [
                    { "name": "llm-eval", "path": "/Users/story/Documents/experiments/llm-eval" },
                    { "name": "game-jam", "path": "/Volumes/Scratch/old-projects/game-jam" },
                ],
                "recent": [{ "name": "playground", "path": "/Users/story/dev/playground" }],
                "pinned": pinned,
                "suggested": [
                    { "name": "llm-eval", "path": "/Users/story/Documents/experiments/llm-eval", "branch": "eval-v2", "language": "Python", "source": "otherApps" },
                    { "name": "game-jam", "path": "/Volumes/Scratch/old-projects/game-jam", "branch": "main", "language": "C#", "source": "otherApps" },
                    { "name": "ghostex-web", "path": "/Users/story/dev/ghostex-web", "branch": "feat/hero", "language": "TypeScript", "source": "projectFolder", "from": "dev" },
                    { "name": "rust-notes", "path": "/Users/story/dev/rust-notes", "branch": "main", "language": "Rust", "source": "projectFolder", "from": "dev" },
                    { "name": "tinyhttp", "path": "/Users/story/code/tinyhttp", "branch": "main", "language": "Rust", "source": "projectFolder", "from": "code" },
                    { "name": "infra", "path": "/Users/story/work/infra", "branch": "main", "language": "HCL", "source": "projectFolder", "from": "work" },
                ],
            }),
        }
    }

    fn answer(
        &mut self,
        operation: &str,
        machine_id: &str,
        params: &Value,
        options: &Options,
    ) -> Result<Value, String> {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        match operation {
            "browse" => Ok(self.browse(
                machine_id,
                &text("partialPath"),
                params.get("inspectPath").and_then(Value::as_str),
            )),
            "places" => {
                if options.old_server {
                    return Err("Unknown endpoint.".to_string());
                }
                if let Some(pin) = params.get("pin").and_then(Value::as_str) {
                    self.pinned.retain(|existing| existing != pin);
                    self.pinned.push(pin.to_string());
                }
                if let Some(unpin) = params.get("unpin").and_then(Value::as_str) {
                    self.pinned.retain(|existing| existing != unpin);
                }
                Ok(self.places(machine_id))
            }
            "add" => match options.add_error {
                Some(error) => Err(error.to_string()),
                None => {
                    Ok(json!({ "project": { "path": text("path"), "projectId": "project-demo" } }))
                }
            },
            "discoverSourceControl" => {
                if options.discovery_unavailable {
                    return Err("gxserver is not reachable.".to_string());
                }
                Ok(json!({ "discovery": { "providers": [
                    { "auth": { "detail": null, "status": "authenticated" }, "installHint": null, "provider": "github", "status": "available", "version": "2.62.0" },
                    { "installHint": "GitLab support needs the glab CLI.", "installTool": "glab", "provider": "gitlab", "status": "missing" },
                    { "installHint": "Bitbucket support needs a CLI Ghostex does not ship yet.", "provider": "bitbucket", "status": "missing" },
                    { "installHint": "Azure DevOps support needs a CLI Ghostex does not ship yet.", "provider": "azure-devops", "status": "missing" },
                ] } }))
            }
            "lookupRepository" => match options.lookup_error {
                Some(error) => Err(error.to_string()),
                None => {
                    let repository = text("repository");
                    let provider = text("provider");
                    Ok(json!({ "repository": {
                        "nameWithOwner": repository,
                        "provider": provider,
                        "sshUrl": format!("git@{provider}.com:{repository}.git"),
                        "url": format!("https://{provider}.com/{repository}"),
                    } }))
                }
            },
            "previewClone" => {
                let destination = trim_separator(&text("destinationPath"));
                let folder = destination
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or("")
                    .to_string();
                let blocked = options.blocked || self.exists(&destination);
                let mut preview = json!({
                    "cloneMainOnly": false,
                    "cloneUrl": text("remoteUrl"),
                    "destinationBlocked": blocked,
                    "destinationExists": blocked,
                    "destinationFolderName": folder,
                    "destinationPath": destination,
                    "parentPath": "/Users/story/dev",
                    "repositoryName": folder,
                    "shallowClone": false,
                });
                if blocked {
                    preview["destinationExistsKind"] = json!("directory");
                    preview["warning"] =
                        json!("The destination folder already exists and is not empty.");
                }
                Ok(json!({ "preview": preview }))
            }
            "startClone" => {
                let job_id = format!("clone-job-{}", self.destinations.len() + 1);
                self.destinations
                    .insert(job_id.clone(), text("destinationPath"));
                Ok(json!({ "job": { "jobId": job_id } }))
            }
            "readCloneJob" => {
                let job_id = text("jobId");
                if self.canceled.contains(&job_id) {
                    return Ok(
                        json!({ "job": { "jobId": job_id, "message": "Clone canceled", "state": "canceled" } }),
                    );
                }
                let polls = self.polls.entry(job_id.clone()).or_insert(0);
                *polls += 1;
                if *polls <= options.running_polls {
                    return Ok(json!({ "job": {
                            "jobId": job_id,
                            "message": "Cloning repository.",
                            "progress": "Receiving objects:  45% (12345/27434), 1.21 GiB | 11.30 MiB/s",
                            "state": "running",
                        } }));
                }
                let path = self.destinations.get(&job_id).cloned().unwrap_or_default();
                Ok(
                    json!({ "job": { "jobId": job_id, "message": "Clone completed", "projectPath": path, "state": "completed" } }),
                )
            }
            "cancelCloneJob" => {
                self.canceled.push(text("jobId"));
                Ok(json!({}))
            }
            "createDirectory" => {
                let parent = trim_separator(&text("parentPath"));
                let name = text("name");
                let separator = separator_of(&parent);
                let path = if parent.ends_with(['/', '\\']) {
                    format!("{parent}{name}")
                } else {
                    format!("{parent}{separator}{name}")
                };
                self.created
                    .entry(parent.clone())
                    .or_default()
                    .push(name.clone());
                self.created.entry(path.clone()).or_default();
                Ok(json!({ "name": name, "parentPath": parent, "path": path }))
            }
            _ => Err("The add-project request was invalid.".to_string()),
        }
    }
}

/// The form's starting point: the Work Space open, `ghostex` (in Client apps) selected, two agents
/// and Claude picked last.
fn demo_new_project() -> AddProjectNewProjectConfig {
    let space = |id: &str, label: &str| AddProjectPlaceChoice {
        id: id.to_string(),
        label: label.to_string(),
        space_ids: Vec::new(),
    };
    let placement = AddProjectPlacementOptions {
        spaces: Some(vec![space("home", "Home"), space("work", "Work")]),
        groups: vec![
            AddProjectPlaceChoice {
                id: "client-apps".to_string(),
                label: "Client apps".to_string(),
                space_ids: vec!["work".to_string()],
            },
            AddProjectPlaceChoice {
                id: "experiments".to_string(),
                label: "Experiments".to_string(),
                space_ids: vec!["home".to_string()],
            },
        ],
        space_id: Some("work".to_string()),
        group_id: Some("client-apps".to_string()),
        selected_project_parent: Some("~/dev".to_string()),
    };
    AddProjectNewProjectConfig {
        placements: std::collections::HashMap::from([
            ("local".to_string(), placement.clone()),
            (
                "machine-bigbox".to_string(),
                AddProjectPlacementOptions {
                    groups: Vec::new(),
                    group_id: None,
                    selected_project_parent: None,
                    ..placement
                },
            ),
        ]),
        agents: vec![
            AddProjectStartAgent {
                agent_id: "claude".to_string(),
                name: "Claude".to_string(),
            },
            AddProjectStartAgent {
                agent_id: "codex".to_string(),
                name: "Codex".to_string(),
            },
        ],
        start: Some(AddProjectStart::Agent("claude".to_string())),
    }
}

fn local_machine() -> AddProjectMachineOption {
    AddProjectMachineOption {
        // GHOSTEX_ADD_PROJECT_DEMO_BASE_DIR: This machine's Default projects folder, for a live run.
        add_project_base_directory: std::env::var("GHOSTEX_ADD_PROJECT_DEMO_BASE_DIR")
            .ok()
            .filter(|folder| !folder.trim().is_empty()),
        description: Some("This computer".to_string()),
        label: "This Mac".to_string(),
        machine_id: "local".to_string(),
        platform: Some("MacIntel".to_string()),
        ..Default::default()
    }
}

fn remote_machine() -> AddProjectMachineOption {
    AddProjectMachineOption {
        add_project_base_directory: Some("~/projects/".to_string()),
        description: Some("Connected remote machine".to_string()),
        label: "build-box".to_string(),
        machine_id: "machine-bigbox".to_string(),
        platform: Some("Linux".to_string()),
        ..Default::default()
    }
}

/// A native Windows (PowerShell) computer: it opens at its drive list.
fn windows_machine() -> AddProjectMachineOption {
    AddProjectMachineOption {
        description: Some("Connected remote machine".to_string()),
        label: "Studio PC".to_string(),
        machine_id: "machine-studio".to_string(),
        platform: Some("Win32".to_string()),
        starts_at_drive_list: true,
        ..Default::default()
    }
}

/// GHOSTEX_ADD_PROJECT_DEMO_GXSERVER=<base url> with GHOSTEX_ADD_PROJECT_DEMO_GXSERVER_TOKEN=<token
/// file> answers every round trip from a real gxserver (a `cargo xtask test-server`), so the
/// listing and a clone run against the daemon's own answers instead of the fixture tree.
pub(super) struct LiveGxserver {
    pub(super) base_url: String,
    pub(super) token: String,
}

impl LiveGxserver {
    pub(super) fn from_env() -> Option<Self> {
        let base_url = std::env::var("GHOSTEX_ADD_PROJECT_DEMO_GXSERVER").ok()?;
        let token_file = std::env::var("GHOSTEX_ADD_PROJECT_DEMO_GXSERVER_TOKEN").ok()?;
        let token = std::fs::read_to_string(token_file.trim()).ok()?;
        Some(Self {
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
        })
    }

    fn endpoint(operation: &str) -> Option<&'static str> {
        Some(match operation {
            "add" => "/api/addProjectPath",
            "browse" => "/api/browseProjectDirectories",
            "cancelCloneJob" => "/api/cancelRepositoryCloneJob",
            "createDirectory" => "/api/createProjectDirectory",
            "discoverSourceControl" => "/api/discoverSourceControl",
            "lookupRepository" => "/api/lookupRepository",
            "places" => "/api/projectPickerPlaces",
            "previewClone" => "/api/previewRepositoryClone",
            "readCloneJob" => "/api/readRepositoryCloneJob",
            "startClone" => "/api/startRepositoryClone",
            _ => return None,
        })
    }

    pub(super) fn call(
        base_url: &str,
        token: &str,
        endpoint: &str,
        params: Value,
    ) -> Result<Value, String> {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .build();
        let mut response = ureq::Agent::new_with_config(config)
            .post(&format!("{base_url}{endpoint}"))
            .header("Authorization", &format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .send(json!({ "protocolVersion": 1, "params": params }).to_string())
            .map_err(|error| error.to_string())?;
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|error| error.to_string())?;
        let body: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if body["ok"] == true {
            Ok(body["result"].clone())
        } else {
            Err(body["message"]
                .as_str()
                .unwrap_or("The machine refused the request.")
                .to_string())
        }
    }
}

type Target = Rc<RefCell<Option<(WindowHandle<Root>, Entity<GpuiAddProjectModalWindow>)>>>;

fn run_steps(target: Target, steps: Vec<String>, cx: &mut App) {
    cx.spawn(async move |cx| {
        for step in steps {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let current = target.borrow().clone();
            let Some((window, view)) = current else {
                return;
            };
            cx.update(|cx| {
                let _ = window.update(cx, |_root, window, cx| {
                    view.update(cx, |modal, cx| modal.preview_step(&step, window, cx));
                });
            });
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            cx.update(|cx| {
                eprintln!("after {step:?}: {}", view.read(cx).preview_summary());
            });
        }
    })
    .detach();
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.as_str();
    let options = Rc::new(Options {
        add_error: match state {
            "error" => Some("Workspace root is not a directory: /Users/story/dev/notes.md"),
            _ => None,
        },
        lookup_error: (state == "lookupfail")
            .then_some("GitHub repository not found: acme/missing"),
        discovery_unavailable: state == "noproviders",
        running_polls: if state == "cloning" { 1000 } else { 2 },
        blocked: state == "blocked",
        old_server: state == "oldserver",
    });
    let latency = Duration::from_millis(match state {
        "slow" => 60_000,
        _ => 30,
    });
    let machines = match state {
        "single" => vec![local_machine()],
        "projectsfolder" | "projectsfolder-add" => vec![
            AddProjectMachineOption {
                add_project_base_directory: Some("~/dev".to_string()),
                ..local_machine()
            },
            remote_machine(),
            windows_machine(),
        ],
        _ => vec![local_machine(), remote_machine(), windows_machine()],
    };
    let target: Target = Rc::new(RefCell::new(None));
    let mock = Rc::new(RefCell::new(Mock::default()));
    let host_target = target.clone();
    let live = LiveGxserver::from_env().map(Rc::new);
    let host: AddProjectModalHost = Rc::new(move |command, cx: &mut App| match command {
        AddProjectModalCommand::Request {
            request_id,
            operation,
            machine_id,
            params,
        } => {
            eprintln!("request {request_id}: {operation} on {machine_id} {params}");
            if let Some(live) = &live {
                let Some(endpoint) = LiveGxserver::endpoint(operation) else {
                    return;
                };
                let (base_url, token) = (live.base_url.clone(), live.token.clone());
                let target = host_target.clone();
                let background = cx.background_executor().clone();
                cx.spawn(async move |cx| {
                    let answer = background
                        .spawn(
                            async move { LiveGxserver::call(&base_url, &token, endpoint, params) },
                        )
                        .await;
                    eprintln!(
                        "answer {request_id}: {}",
                        match &answer {
                            Ok(value) => value.to_string().chars().take(400).collect::<String>(),
                            Err(error) => format!("error: {error}"),
                        }
                    );
                    let current = target.borrow().clone();
                    if let Some((window, view)) = current {
                        cx.update(|cx| {
                            let _ = window.update(cx, |_root, window, cx| {
                                view.update(cx, |modal, cx| {
                                    modal.receive_response(request_id, answer, window, cx)
                                });
                            });
                        });
                    }
                })
                .detach();
                return;
            }
            // The slow state's add never answers, so the notice stays up for the screenshot.
            let delay = if operation == "add" || operation == "readCloneJob" {
                latency
            } else {
                Duration::from_millis(30)
            };
            let answer = mock
                .borrow_mut()
                .answer(operation, &machine_id, &params, &options);
            let target = host_target.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(delay).await;
                let current = target.borrow().clone();
                if let Some((window, view)) = current {
                    cx.update(|cx| {
                        let _ = window.update(cx, |_root, window, cx| {
                            view.update(cx, |modal, cx| {
                                modal.receive_response(request_id, answer, window, cx)
                            });
                        });
                    });
                }
            })
            .detach();
        }
        AddProjectModalCommand::StartBackgroundClone {
            machine_id,
            params,
            repository,
            destination,
        } => {
            eprintln!("background clone on {machine_id}: {repository} into {destination} {params}");
        }
        AddProjectModalCommand::Close => {
            eprintln!("close");
            cx.quit();
        }
    });
    let config = AddProjectModalConfig {
        machines,
        initial_machine_id: None,
        active_project_cwd: None,
        client_platform: "MacIntel".to_string(),
        // GHOSTEX_ADD_PROJECT_DEMO_GLASS=1 previews the frosted palette the app uses under window glass.
        palette: if std::env::var("GHOSTEX_ADD_PROJECT_DEMO_GLASS").as_deref() == Ok("1") {
            let mut fill = demo.palette.surface;
            fill.a = 0.82;
            demo.palette.frosted(fill)
        } else {
            demo.palette
        },
        clone_job_poll_interval: AddProjectModalConfig::CLONE_JOB_POLL_INTERVAL,
        // GHOSTEX_ADD_PROJECT_DEMO_BACKGROUND=1: Clone & Add hands the clone off and closes, as in the app.
        background_clone: std::env::var("GHOSTEX_ADD_PROJECT_DEMO_BACKGROUND").as_deref()
            == Ok("1"),
        // GHOSTEX_ADD_PROJECT_DEMO_CLONE_URL opens on the clone destination step, as a handoff's destination does.
        initial_clone_url: std::env::var("GHOSTEX_ADD_PROJECT_DEMO_CLONE_URL")
            .ok()
            .filter(|url| !url.trim().is_empty()),
        new_project: demo_new_project(),
        slow_operation_notice: if matches!(state, "slow" | "cloning") {
            Duration::from_millis(400)
        } else {
            AddProjectModalConfig::SLOW_OPERATION_NOTICE
        },
    };
    let (window, view) = super::open_modal_window(
        ADD_PROJECT_MODAL_WIDTH,
        ADD_PROJECT_MODAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiAddProjectModalWindow::new(config, host, window, cx)),
        cx,
    );
    *target.borrow_mut() = Some((window, view));
    let clone_steps = vec!["clone", "repo:https://github.com/acme/widgets.git"];
    let steps: Vec<&'static str> = match state {
        "browse" => vec!["open:/Users/story/dev"],
        "highlight" => vec!["key:down", "key:down"],
        "create" => vec!["type:~/dev/brand-new"],
        "newfolder" => vec!["new-folder", "folder-name:my-app"],
        "newfolder-made" => vec!["new-folder", "folder-name:my-app", "key:enter"],
        "inside-repo" => vec![
            "open:/Users/story/dev/ghostex",
            "open:/Users/story/dev/ghostex/sidebar",
        ],
        "repository" => vec!["clone", "source:github"],
        "lookupfail" => vec![
            "clone",
            "source:github",
            "repo:acme/missing",
            "clone-submit",
        ],
        "clone" => clone_steps.clone(),
        "blocked" => vec![
            "place:/Users/story/code",
            "clone",
            "repo:https://github.com/acme/tinyhttp.git",
            "clone-submit",
        ],
        "badbranch" => [clone_steps.clone(), vec!["branch:feature..bad"]].concat(),
        "cloning" => [clone_steps, vec!["clone-submit"]].concat(),
        "error" => vec!["type:~/dev/ghostex/", "add"],
        "slow" => vec!["add"],
        "remote" => vec!["machine:machine-bigbox"],
        "windows" => vec!["machine:machine-studio"],
        "windows-folder" => vec![
            "machine:machine-studio",
            "open:C:\\Users\\story",
            "open:C:\\Users\\story\\source",
        ],
        "suggested" => vec!["suggested"],
        "machines" => vec!["machine-menu"],
        "projectsfolder-add" => vec!["new-project", "project-name:widgets", "create"],
        "newproject" => vec!["new-project", "project-name:billing-api"],
        "newproject-start" => vec!["new-project", "project-name:billing-api", "pick:start"],
        "ambiguous" => vec!["type:acme/widgets"],
        "pasted-url" => vec!["type:https://github.com/acme/widgets"],
        _ => Vec::new(),
    };
    // `script:<step>;<step>...` runs any sequence of `preview_step` steps.
    // GHOSTEX_ADD_PROJECT_DEMO_STEPS=<step>;<step>: the same, kept as typed (the state is lowercased).
    let raw_steps = std::env::var("GHOSTEX_ADD_PROJECT_DEMO_STEPS").unwrap_or_default();
    let steps: Vec<String> = match state.strip_prefix("script:") {
        _ if !raw_steps.trim().is_empty() => raw_steps.split(';').map(str::to_string).collect(),
        Some(script) => script.split(';').map(str::to_string).collect(),
        None => steps.into_iter().map(str::to_string).collect(),
    };
    if !steps.is_empty() {
        run_steps(target, steps, cx);
    }
}
