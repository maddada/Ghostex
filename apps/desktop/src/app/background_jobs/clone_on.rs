//! The sidebar project menu's Clone on… (packages/gx-core/src/sidebar_menu/clone_on.rs holds the
//! decision record): clones the project's git remote into the picked machine's usual projects
//! folder and follows it as a background clone (clone.rs).
//!
//! The folder is the one Add Project's clone starts in on that machine: Settings > Projects >
//! Default projects folder on this computer, else the home folder; the home folder on another
//! machine. The new folder is named after the repository, or `<name>-2` … `<name>-9` when that
//! name is taken there.

use serde_json::Value;

use crate::*;

/// How many numbered names are tried after the repository's own.
const FREE_NAME_TRIES: u32 = 9;

/// `owner/name` from a clone URL (`https://host/owner/name.git`, `git@host:owner/name.git`).
fn repository_label(remote_url: &str) -> String {
    let trimmed = remote_url.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let mut parts = trimmed.rsplit(['/', ':']).filter(|part| !part.is_empty());
    match (parts.next(), parts.next()) {
        (Some(name), Some(owner)) if !owner.contains('.') && !owner.contains('@') => {
            format!("{owner}/{name}")
        }
        (Some(name), _) => name.to_string(),
        _ => remote_url.to_string(),
    }
}

/// The folder a clone of `remote_url` is named after.
fn repository_folder(remote_url: &str) -> String {
    repository_label(remote_url)
        .rsplit('/')
        .next()
        .unwrap_or("repository")
        .to_string()
}

impl GhostexGpuiApp {
    /// A Clone on… row was picked: `{ type: "cloneProjectOnMachine", machineId, machineName,
    /// remoteUrl, projectName }`.
    pub(crate) fn clone_project_on_machine(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            command[key]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let (Some(machine_id), Some(remote_url)) = (text("machineId"), text("remoteUrl")) else {
            return;
        };
        let machine_name = text("machineName").unwrap_or_else(|| "That computer".to_string());
        let local = machine_id == GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID;
        // The menu can be older than the connection: check again now.
        if !local
            && self
                .gpui_remote_gxserver_request_target(&machine_id)
                .is_none()
        {
            self.dispatch_gpui_workspace_action_toast(
                "error",
                &format!("{machine_name} isn't connected"),
                "Connect it, then pick Clone on again.",
                cx,
            );
            return;
        }
        let base = if local {
            shared_settings::shared_sidebar_settings_snapshot()
                .object()
                .get("defaultProjectsFolder")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|folder| !folder.is_empty())
                .unwrap_or("~")
                .trim_end_matches(['/', '\\'])
                .to_string()
        } else {
            "~".to_string()
        };
        let base = if base.is_empty() {
            "/".to_string()
        } else {
            base
        };
        let name = repository_folder(&remote_url);
        // The folders already there whose names start with the repository's.
        let mut params = serde_json::Map::new();
        params.insert(
            "partialPath".to_string(),
            serde_json::json!(format!("{base}/{name}")),
        );
        let browse_machine_id = machine_id.clone();
        self.run_gpui_add_project_dialog_operation(
            GpuiAddProjectDialogOperation::Browse,
            Some(browse_machine_id.as_str()),
            &params,
            move |app, result, cx| {
                // A projects folder that isn't there yet is made by the clone.
                let taken: Vec<String> = result
                    .ok()
                    .and_then(|value| value["entries"].as_array().cloned())
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|entry| entry["name"].as_str().map(str::to_lowercase))
                    .collect();
                let free = std::iter::once(name.clone())
                    .chain((2..=FREE_NAME_TRIES + 1).map(|index| format!("{name}-{index}")))
                    .find(|candidate| !taken.contains(&candidate.to_lowercase()));
                let Some(folder) = free else {
                    app.dispatch_gpui_workspace_action_toast(
                        "error",
                        "No free folder name",
                        &format!(
                            "{base}/{name} to {name}-{} already exist on {machine_name}. Clone it from Add Project instead and pick a folder.",
                            FREE_NAME_TRIES + 1
                        ),
                        cx,
                    );
                    return;
                };
                let destination = format!("{base}/{folder}");
                app.start_background_clone(
                    super::CloneSpec {
                        machine_id,
                        params: serde_json::json!({
                            "destinationPath": destination,
                            "remoteUrl": remote_url,
                        }),
                        repository: repository_label(&remote_url),
                        destination,
                    },
                    cx,
                );
            },
            cx,
        );
    }
}
