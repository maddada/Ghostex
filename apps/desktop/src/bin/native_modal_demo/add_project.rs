//! Add Project preview. The host answers every gxserver round trip from the fixture tree of
//! packages/core-ui/add-project-modal/add-project-modal-mocks.ts (deleted 2026-10-01), after `latency`.
//! States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (Sources, one machine), `machines`,
//! `remote` (preselected remote machine), `noproviders`, `browse`, `highlight` (two rows down),
//! `create` (an unknown leaf: Create & Add), `newfolder`, `repository` (GitHub), `destination`,
//! `review`, `blocked` (review with a refused destination and a bad branch), `error` (the add
//! fails), `slow` (the still-working notice), `cloning` (a running clone past the notice),
//! `lookupfail`, `drives` (a PowerShell machine's drive list), `ambiguous` (owner/repo typed on
//! the Sources step).
use super::add_project_modal::*;
use gpui::{App, AppContext as _, Entity, WindowHandle};
use gpui_component::Root;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

const HOME: &str = "/Users/story";
const REMOTE_HOME: &str = "/srv";

fn tree() -> HashMap<&'static str, Vec<&'static str>> {
    HashMap::from([
        (
            "/",
            vec![
                "Applications",
                "Library",
                "Users",
                "Volumes",
                "opt",
                "srv",
                "tmp",
            ],
        ),
        ("/Volumes/", vec!["Backup SSD", "Macintosh HD", "Scratch"]),
        (
            "/Users/story/",
            vec![".config", "Desktop", "dev", "Documents", "Downloads"],
        ),
        (
            "/Users/story/dev/",
            vec![".cache", "ghostex", "ghostex-web", "playground", "scratch"],
        ),
        (
            "/Users/story/dev/ghostex/",
            vec!["gpui", "sidebar", "shared"],
        ),
        ("/Users/story/dev/playground/", vec!["alpha", "beta"]),
        ("/srv/", vec!["deploy", "projects"]),
        ("/srv/projects/", vec!["api", "worker"]),
    ])
}

#[derive(Default)]
struct Mock {
    created: HashMap<String, Vec<String>>,
    polls: HashMap<String, u32>,
    destinations: HashMap<String, String>,
    canceled: Vec<String>,
}

struct Options {
    add_error: Option<&'static str>,
    lookup_error: Option<&'static str>,
    discovery_unavailable: bool,
    running_polls: u32,
    blocked: bool,
    drives: bool,
}

fn expand(value: &str, home: &str) -> String {
    if value == "~" {
        return format!("{home}/");
    }
    match value.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => value.to_string(),
    }
}

fn trim_separator(value: &str) -> String {
    if value.len() > 1 && value.ends_with('/') {
        value[..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

impl Mock {
    fn browse(&self, machine_id: &str, partial: &str, options: &Options) -> Value {
        if options.drives && partial == "/" {
            return json!({
                "entries": [
                    { "fullPath": "C:\\Users\\story", "name": "story (home)" },
                    { "fullPath": "C:\\", "name": "C:" },
                    { "fullPath": "D:\\", "name": "D: (Data)" },
                ],
                "isDriveList": true,
                "parentPath": "/",
            });
        }
        let home = if machine_id == "machine-bigbox" {
            REMOTE_HOME
        } else {
            HOME
        };
        let expanded = expand(partial, home);
        let parent = if expanded.ends_with('/') {
            expanded.clone()
        } else {
            expanded[..expanded.rfind('/').map(|index| index + 1).unwrap_or(0)].to_string()
        };
        let prefix = if expanded.ends_with('/') {
            String::new()
        } else {
            expanded[expanded.rfind('/').map(|index| index + 1).unwrap_or(0)..].to_string()
        };
        let show_hidden = expanded.ends_with('/') || prefix.starts_with('.');
        let tree = tree();
        let mut names: Vec<String> = tree
            .get(parent.as_str())
            .map(|names| names.iter().map(|name| name.to_string()).collect())
            .unwrap_or_default();
        let parent_path = trim_separator(&parent);
        // Created folders are layered over the fixture and the listing re-sorted, as in the story mock.
        if let Some(created) = self.created.get(&parent_path) {
            for name in created {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            names.sort();
        }
        let entries: Vec<Value> = names
            .iter()
            .filter(|name| name.to_lowercase().starts_with(&prefix.to_lowercase()))
            .filter(|name| show_hidden || !name.starts_with('.'))
            .map(|name| json!({ "fullPath": format!("{parent}{name}"), "name": name }))
            .collect();
        json!({ "entries": entries, "parentPath": parent_path })
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
            "browse" => Ok(self.browse(machine_id, &text("partialPath"), options)),
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
                    { "auth": { "detail": "GitLab CLI is not authenticated. Run glab auth login.", "status": "unauthenticated" }, "installHint": null, "provider": "gitlab", "status": "available", "version": "1.48.0" },
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
                let folder = destination.rsplit('/').next().unwrap_or("").to_string();
                let mut preview = json!({
                    "cloneMainOnly": false,
                    "cloneUrl": text("remoteUrl"),
                    "destinationBlocked": options.blocked,
                    "destinationExists": options.blocked,
                    "destinationFolderName": folder,
                    "destinationPath": destination,
                    "parentPath": "/Users/story/dev",
                    "repositoryName": folder,
                    "shallowClone": false,
                });
                if options.blocked {
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
                let path = format!("{parent}/{name}");
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

fn local_machine() -> AddProjectMachineOption {
    AddProjectMachineOption {
        description: Some("This computer".to_string()),
        label: "Local".to_string(),
        machine_id: "local".to_string(),
        platform: Some("MacIntel".to_string()),
        ..Default::default()
    }
}

fn remote_machine() -> AddProjectMachineOption {
    AddProjectMachineOption {
        add_project_base_directory: Some("~/projects/".to_string()),
        description: Some("Connected remote machine".to_string()),
        label: "Bigbox".to_string(),
        machine_id: "machine-bigbox".to_string(),
        platform: Some("Linux".to_string()),
        ..Default::default()
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
        drives: state == "drives",
    });
    let latency = Duration::from_millis(match state {
        "slow" => 60_000,
        _ => 30,
    });
    let machines = match state {
        "machines" | "remote" => vec![local_machine(), remote_machine()],
        _ if state.starts_with("script:") && state.contains("machine:") => {
            vec![local_machine(), remote_machine()]
        }
        "drives" => vec![AddProjectMachineOption {
            label: "This PC".to_string(),
            machine_id: "local".to_string(),
            platform: Some("Win32".to_string()),
            starts_at_drive_list: true,
            ..Default::default()
        }],
        _ => vec![local_machine()],
    };
    let target: Target = Rc::new(RefCell::new(None));
    let mock = Rc::new(RefCell::new(Mock::default()));
    let host_target = target.clone();
    let host: AddProjectModalHost = Rc::new(move |command, cx: &mut App| match command {
        AddProjectModalCommand::Request {
            request_id,
            operation,
            machine_id,
            params,
        } => {
            eprintln!("request {request_id}: {operation} on {machine_id} {params}");
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
        AddProjectModalCommand::Close => {
            eprintln!("close");
            cx.quit();
        }
    });
    let config = AddProjectModalConfig {
        machines,
        initial_machine_id: (state == "remote").then(|| "machine-bigbox".to_string()),
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
        // GHOSTEX_ADD_PROJECT_DEMO_CLONE_URL opens on the clone destination step, as a handoff's destination does.
        initial_clone_url: std::env::var("GHOSTEX_ADD_PROJECT_DEMO_CLONE_URL")
            .ok()
            .filter(|url| !url.trim().is_empty()),
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
    let to_review = vec![
        "click:source:url",
        "type:https://github.com/acme/widgets.git",
        "key:enter",
        "type:~/dev/widgets",
        "key:enter",
    ];
    let steps: Vec<&'static str> = match state {
        "browse" => vec!["click:source:local"],
        "highlight" => vec!["click:source:local", "key:down", "key:down"],
        "create" => vec!["click:source:local", "type:~/dev/brand-new"],
        "newfolder" => vec!["click:source:local", "new-folder", "folder-name:my-app"],
        "repository" => vec!["click:source:github"],
        "lookupfail" => vec!["click:source:github", "type:acme/missing", "key:enter"],
        "destination" => to_review[..4].to_vec(),
        "review" => to_review,
        "blocked" => [to_review, vec!["branch:feature..bad"]].concat(),
        "cloning" => [to_review, vec!["clone"]].concat(),
        "error" => vec!["click:source:local", "type:~/dev/ghostex/", "key:enter"],
        "slow" => vec!["click:source:local", "key:enter"],
        "drives" => vec!["click:source:local"],
        "ambiguous" => vec!["type:acme/widgets"],
        _ => Vec::new(),
    };
    // `script:<step>;<step>...` runs any sequence of `preview_step` steps.
    let steps: Vec<String> = match state.strip_prefix("script:") {
        Some(script) => script.split(';').map(str::to_string).collect(),
        None => steps.into_iter().map(str::to_string).collect(),
    };
    if !steps.is_empty() {
        run_steps(target, steps, cx);
    }
}
