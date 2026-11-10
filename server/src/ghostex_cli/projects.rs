//! `ghostex projects [--all] [--json] [--computer name]`: this computer's projects (or a connected
//! computer's) with the ids `create-session`, `create-agent` and the other `--project-id` verbs
//! take.
//!
//! CDXC:Cli 2026-11-11 WHY:
//! No command listed project ids, so an agent creating a session dug them out of gxserver's state JSON (GitHub #557). This reads `/api/listProjects` and nothing else, so it is read-only, and `--computer` works as for `ghostex sessions` because that endpoint is one a connected computer may answer. Hidden system projects are not the user's and are never listed; parked projects (Recent Projects) only with `--all`.

use serde_json::{json, Value};

use super::args::parse_args;
use super::output::print_json;
use super::rpc::{call_gxserver_rpc, CliResult};

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let all = flags.truthy("all");
    let response = call_gxserver_rpc("/api/listProjects", &json!({}), flags)?;
    let empty = Vec::new();
    let mut projects: Vec<&Value> = response
        .get("projects")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter(|project| project.get("visibility").and_then(Value::as_str) != Some("hidden"))
        .filter(|project| all || !parked(project))
        .collect();
    projects.sort_by_key(|project| text(project, "name").to_lowercase());
    if flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "projects": projects
                .iter()
                .map(|project| json!({
                    "projectId": text(project, "projectId"),
                    "name": text(project, "name"),
                    "path": text(project, "path"),
                    "pinned": project.get("isPinned").and_then(Value::as_bool) == Some(true),
                    "parked": parked(project),
                }))
                .collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    if projects.is_empty() {
        println!(
            "No projects. Add one with \"ghostex add-project <path>\"{}.",
            if all {
                ""
            } else {
                ", or pass --all to include parked ones"
            }
        );
        return Ok(());
    }
    println!("NAME\tPROJECT ID\tPATH");
    for project in &projects {
        println!(
            "{}\t{}\t{}{}",
            text(project, "name"),
            text(project, "projectId"),
            text(project, "path"),
            if parked(project) { "\t(parked)" } else { "" }
        );
    }
    Ok(())
}

/// A project closed to Recent Projects: kept, but not in the sidebar.
fn parked(project: &Value) -> bool {
    let flag = project.get("isRecentProject");
    flag.and_then(Value::as_bool) == Some(true) || flag.and_then(Value::as_i64) == Some(1)
}

fn text<'a>(project: &'a Value, key: &str) -> &'a str {
    project.get(key).and_then(Value::as_str).unwrap_or("")
}
