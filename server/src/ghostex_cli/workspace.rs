//! `ghostex workspace`: list, create, rename and delete this computer's workspaces, and move a
//! project into one (crate::workspaces). Workspaces and projects can be named by id or by name;
//! gxserver resolves both.

use serde_json::{json, Map, Value};

use super::args::{parse_args, Flags};
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

/// `ghostex workspace list | create <name> [--kind work|personal] [--color #hex] [--letter L] |
/// rename <workspace> <new name> | move-project <project> <workspace> | delete <workspace>`.
pub(super) fn workspace_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let subcommand = parsed.rest.first().map(String::as_str).unwrap_or("list");
    let rest = parsed.rest.get(1..).unwrap_or_default();
    match subcommand {
        "list" | "ls" => list(flags),
        "create" | "new" => {
            let name = joined(rest, "Pass the name: ghostex workspace create <name>.")?;
            let mut params = Map::new();
            params.insert("name".to_string(), json!(name));
            if let Some(kind) = flags.string_value("kind") {
                let kind = kind.trim().to_ascii_lowercase();
                if kind != "work" && kind != "personal" {
                    return Err(CliError::Other(
                        "--kind is work or personal.".to_string(),
                    ));
                }
                params.insert("kind".to_string(), json!(kind));
            }
            for key in ["color", "letter"] {
                if let Some(value) = flags.string_value(key) {
                    params.insert(key.to_string(), json!(value));
                }
            }
            call_and_print("/api/createWorkspace", params, flags)
        }
        "rename" => {
            let (workspace, name) = match rest {
                [workspace, name @ ..] if !name.is_empty() => (workspace, name.join(" ")),
                _ => {
                    return Err(CliError::Other(
                        "Pass the workspace and its new name: ghostex workspace rename <workspace> <new name>."
                            .to_string(),
                    ))
                }
            };
            let mut params = Map::new();
            params.insert("workspaceId".to_string(), json!(workspace));
            params.insert("name".to_string(), json!(name));
            call_and_print("/api/updateWorkspace", params, flags)
        }
        "move-project" | "move" => {
            let [project, workspace] = rest else {
                return Err(CliError::Other(
                    "Pass the project and the workspace: ghostex workspace move-project <project> <workspace>."
                        .to_string(),
                ));
            };
            let mut params = Map::new();
            params.insert("project".to_string(), json!(project_reference(project)));
            params.insert("workspaceId".to_string(), json!(workspace));
            call_and_print("/api/moveProjectToWorkspace", params, flags)
        }
        "delete" | "remove" => {
            let workspace = joined(rest, "Pass the workspace: ghostex workspace delete <workspace>.")?;
            let mut params = Map::new();
            params.insert("workspaceId".to_string(), json!(workspace));
            call_and_print("/api/deleteWorkspace", params, flags)
        }
        other => Err(CliError::Other(format!(
            "Unknown workspace command \"{other}\". Use list, create, rename, move-project or delete."
        ))),
    }
}

/// The workspaces in order, each with its kind and the names of its projects.
fn list(flags: &Flags) -> CliResult<()> {
    let document = rpc::call_gxserver_rpc("/api/readWorkspaces", &json!({}), flags)?;
    let document = document
        .get("sidebarWorkspaces")
        .cloned()
        .unwrap_or(Value::Null);
    let default_id = document
        .get("defaultWorkspaceId")
        .and_then(Value::as_str)
        .unwrap_or("personal")
        .to_string();
    let projects = rpc::call_gxserver_rpc("/api/listProjects", &json!({}), flags)
        .ok()
        .and_then(|result| result.get("projects").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let workspace_of = |project: &Value| -> String {
        project
            .pointer("/launchSettings/workspaceId")
            .and_then(Value::as_str)
            .filter(|id| document.pointer(&format!("/workspaces/{id}")).is_some())
            .unwrap_or(&default_id)
            .to_string()
    };
    let workspaces: Vec<Value> = document
        .get("order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|id| document.pointer(&format!("/workspaces/{id}")))
        .map(|workspace| {
            let id = workspace
                .get("workspaceId")
                .and_then(Value::as_str)
                .unwrap_or("");
            // Checkouts only: a worktree project is in its parent checkout's workspace.
            let names: Vec<Value> = projects
                .iter()
                .filter(|project| {
                    project.pointer("/worktree/parentProjectId").is_none()
                        && project.get("isRecentProject").and_then(Value::as_bool) != Some(true)
                        && project.get("visibility").and_then(Value::as_str) != Some("hidden")
                        && workspace_of(project) == id
                })
                .filter_map(|project| project.get("name").cloned())
                .collect();
            json!({
                "workspaceId": id,
                "name": workspace.get("name"),
                "kind": workspace.get("kind"),
                "color": workspace.get("color"),
                "letter": workspace.get("letter"),
                "claudeAccountId": workspace.get("claudeAccountId"),
                "default": id == default_id,
                "projects": names,
            })
        })
        .collect();
    print_json(&json!({ "workspaces": workspaces }));
    Ok(())
}

/// A project argument that names an existing folder is sent as its absolute path, so
/// `ghostex workspace move-project . Acme` works from inside the project.
fn project_reference(reference: &str) -> String {
    let path = std::path::Path::new(reference);
    if reference.contains(['/', '\\']) || reference == "." || reference == ".." {
        if let Ok(absolute) = std::fs::canonicalize(path) {
            let text = absolute.to_string_lossy().to_string();
            // Windows canonical paths start with `\\?\`, which project paths never do.
            return text.strip_prefix(r"\\?\").unwrap_or(&text).to_string();
        }
    }
    reference.to_string()
}

fn joined(rest: &[String], missing: &str) -> CliResult<String> {
    let text = rest.join(" ");
    let text = text.trim();
    if text.is_empty() {
        return Err(CliError::Other(missing.to_string()));
    }
    Ok(text.to_string())
}

fn call_and_print(path: &str, params: Map<String, Value>, flags: &Flags) -> CliResult<()> {
    let result = rpc::call_gxserver_rpc(path, &Value::Object(params), flags)?;
    print_json(&result);
    Ok(())
}
