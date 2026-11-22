//! `ghostex move-project-folder`: moves a project's folder on disk and everything Ghostex keeps
//! about it (gxserver's `/api/moveProjectFolder`, server/src/project_folder_move/).

use serde_json::{Map, Value, json};

use super::args::parse_args;
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

const USAGE: &str = "Usage: ghostex move-project-folder (--project-id id|--path path|--name name) <new folder path> [--dry-run] [--force-open-files] [--json]
       ghostex move-project-folder (--project-id id|--path path|--name name) --resume|--abandon [--json]

Moves the project's folder on disk to <new folder path> (on the same drive) and points Ghostex at it:
the project, every session that runs in it, its worktrees' git links, its board and its automations.
Its sessions keep their history: Claude and Codex resume them in the new folder.

It refuses, changing nothing, while a session in the project runs, while another program works
inside the folder or has files there open (--force-open-files moves it anyway), while Ghostex's
Files editor has unsaved changes there, when the folder is a link or holds Ghostex's own files or
an agent's settings, when the new path exists, is on another drive or in or out of a cloud-synced
folder, and when the folder has worktrees beside it and would leave their parent folder. One move
runs at a time. --dry-run lists what would change.

A move that stops part-way (Ghostex quit or the computer restarted) is finished at gxserver's
next start; --resume finishes it now, --abandon forgets one that can't be finished.

Agents: move a project's folder only when the user asked for that move, and --dry-run first.";

pub fn run(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    if flags.contains("computer") {
        return Err(CliError::Other(
            "move-project-folder works on this computer only; run it on the computer whose folder moves."
                .to_string(),
        ));
    }
    let mut params = Map::new();
    for (flag, key) in [
        ("projectId", "projectId"),
        ("path", "path"),
        ("name", "name"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    if params.is_empty() {
        return Err(CliError::Other(format!(
            "Name the project with --project-id, --path or --name.\n\n{USAGE}"
        )));
    }
    let resume = flags.truthy("resume");
    let abandon = flags.truthy("abandon");
    if resume || abandon {
        params.insert(
            if resume { "resume" } else { "abandon" }.to_string(),
            json!(true),
        );
    } else {
        let new_path = flags
            .string_value("newPath")
            .map(str::to_string)
            .or_else(|| (!parsed.rest.is_empty()).then(|| parsed.rest.join(" ")))
            .ok_or_else(|| CliError::Other(format!("Give the folder's new path.\n\n{USAGE}")))?;
        params.insert("newPath".to_string(), json!(new_path));
        params.insert("dryRun".to_string(), json!(flags.truthy("dryRun")));
        params.insert(
            "forceOpenFiles".to_string(),
            json!(flags.truthy("forceOpenFiles")),
        );
    }
    // A move runs git in every worktree; it is not bound by the usual 15 seconds.
    let mut call_flags = flags.clone();
    if !call_flags.contains("timeout") {
        call_flags.insert_text("timeout", "900000");
    }
    let result = rpc::call_gxserver_rpc(
        "/api/moveProjectFolder",
        &Value::Object(params),
        &call_flags,
    )?;
    if flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    print_human(&result);
    Ok(())
}

fn print_human(result: &Value) {
    let text = |key: &str| result.get(key).and_then(Value::as_str).unwrap_or("");
    let lines = |key: &str| -> Vec<String> {
        result
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect()
    };
    if result.get("dryRun").and_then(Value::as_bool) == Some(true) {
        println!("Would move {} to {}.", text("from"), text("to"));
        if let Some(rows) = result.get("rows").and_then(Value::as_object) {
            let rows: Vec<String> = rows
                .iter()
                .map(|(table, count)| format!("{table} {count}"))
                .collect();
            if !rows.is_empty() {
                println!("Records that would follow it: {}.", rows.join(", "));
            }
        }
        for file in lines("files") {
            println!("File that would follow it: {file}");
        }
        if let Some(worktrees) = result.get("worktrees").and_then(Value::as_array) {
            for worktree in worktrees {
                println!(
                    "Worktree to repair: {}{}",
                    worktree["path"].as_str().unwrap_or(""),
                    if worktree["hasSubmodules"] == true {
                        " (with submodules)"
                    } else {
                        ""
                    }
                );
            }
        }
        for warning in lines("warnings") {
            println!("Note: {warning}");
        }
        return;
    }
    if result.get("abandoned").and_then(Value::as_bool) == Some(true)
        || result.get("forgotten").and_then(Value::as_bool) == Some(true)
    {
        for note in lines("notes") {
            println!("{note}");
        }
        return;
    }
    println!("Moved {} to {}.", text("from"), text("to"));
    for note in lines("notes") {
        println!("- {note}");
    }
}
