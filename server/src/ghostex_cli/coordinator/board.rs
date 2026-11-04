//! `ghostex coordinator board`: the coordinator's board (server/src/coordinators/board.rs) printed by
//! group, and the verbs that place what is not a thread: add, move, note, done and drop.
//!
//! An item is named `B7`; anything else is a thread ref, which `move` pins to a group its state
//! cannot tell (Blocked, Needs you, a kind of Later) and `done` resolves like `coordinator resolve`.

use serde_json::{json, Map, Value};

use super::command::{flag_text, resolve_thread_session, server_flags, target_coordinator};
use crate::ghostex_cli::{
    agents,
    args::ParsedArgs,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const USAGE: &str =
    "Usage: ghostex coordinator board [add|move|note|done|drop] … (see ghostex coordinator --help)";

/// Each group's heading, in the board's order.
const HEADINGS: [(&str, &str); 6] = [
    ("later", "Later"),
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    ("moving", "Moving"),
    ("done", "Done today"),
    ("ideas", "Ideas"),
];

pub(super) fn board(parsed: &ParsedArgs) -> CliResult<()> {
    let (coordinator, flags) = target_coordinator(parsed)?;
    let verb = parsed.rest.first().map(String::as_str).unwrap_or("show");
    let mut params = Map::new();
    params.insert("projectId".into(), coordinator["projectId"].clone());
    params.insert("sessionId".into(), coordinator["sessionId"].clone());
    let arg = |index: usize| parsed.rest.get(index).cloned();
    for (key, flag) in [
        ("note", "note"),
        ("link", "link"),
        ("release", "release"),
        ("group", "group"),
        ("title", "title"),
    ] {
        if let Some(value) = flag_text(&parsed.flags, flag) {
            params.insert(key.into(), json!(value));
        }
    }
    if parsed.flags.truthy("untested") {
        params.insert("untested".into(), json!(true));
    } else if parsed.flags.truthy("tested") {
        params.insert("untested".into(), json!(false));
    }
    match verb {
        "show" | "list" => {
            params.insert("op".into(), json!("read"));
            params.insert("all".into(), json!(true));
            let board = call_gxserver_rpc("/api/coordinatorBoard", &Value::Object(params), &flags)?;
            if parsed.flags.truthy("json") {
                print_json(&board);
            } else {
                print_board(&board, parsed.flags.truthy("all"));
            }
            return Ok(());
        }
        "add" => {
            let title =
                arg(1).ok_or_else(|| CliError::Other(format!("{USAGE}\nadd needs a title.")))?;
            params.insert("title".into(), json!(title));
        }
        "move" => {
            name_target(&mut params, arg(1), &flags)?;
            if let Some(group) = arg(2) {
                params.insert("group".into(), json!(group));
            }
        }
        "note" => {
            name_target(&mut params, arg(1), &flags)?;
            if let Some(note) = arg(2) {
                params.insert("note".into(), json!(note));
            }
        }
        "done" | "drop" => {
            let reference = arg(1);
            name_target(&mut params, reference.clone(), &flags)?;
            // A thread's Done is its resolve: closing it as `coordinator resolve` does, which also
            // takes its pin off the board.
            if verb == "done" && !params.contains_key("item") {
                // The pin first, so the board drops it even if the session was already closed.
                params.insert("op".into(), json!("done"));
                let _ = call_gxserver_rpc("/api/coordinatorBoard", &Value::Object(params), &flags);
                let resolve = ParsedArgs {
                    flags: parsed.flags.clone(),
                    rest: vec![reference.unwrap_or_default()],
                };
                return super::threads::set_resolved(&resolve, true);
            }
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown board command: {other}. {USAGE}"
            )))
        }
    }
    params.insert("op".into(), json!(verb));
    let result = call_gxserver_rpc("/api/coordinatorBoard", &Value::Object(params), &flags)?;
    if parsed.flags.truthy("json") {
        print_json(&result);
        return Ok(());
    }
    let name = agents::text(&result, "item");
    let heading = heading(agents::text(&result, "group"));
    match verb {
        "add" => println!("{name} added to {heading}{}.", reason_words(&result)),
        "move" => println!("{name} moved to {heading}{}.", reason_words(&result)),
        "note" => println!("{name} updated."),
        "done" => println!("{name} marked done."),
        _ => println!("Dropped from the board."),
    }
    Ok(())
}

/// Puts the item (`B7`) or the thread a reference names into `params`.
fn name_target(
    params: &mut Map<String, Value>,
    reference: Option<String>,
    flags: &crate::ghostex_cli::args::Flags,
) -> CliResult<()> {
    let reference = reference
        .ok_or_else(|| CliError::Other(format!("{USAGE}\nName an item (B7) or a thread.")))?;
    let trimmed = reference.trim();
    if let Some(number) = trimmed
        .strip_prefix(['B', 'b'])
        .filter(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    {
        params.insert("item".into(), json!(format!("B{number}")));
        return Ok(());
    }
    // A thread on another computer keeps that computer's id; it cannot be read from here.
    if crate::ghostex_cli::rpc::is_gxserver_global_session_ref(trimmed) {
        let mut parts = trimmed.splitn(3, ':');
        if let (Some(server), Some(project), Some(session)) =
            (parts.next(), parts.next(), parts.next())
        {
            if crate::ghostex_cli::rpc::local_gxserver_server_id().as_deref() != Some(server) {
                params.insert("threadServerId".into(), json!(server));
                params.insert("threadProjectId".into(), json!(project));
                params.insert("threadSessionId".into(), json!(session));
                return Ok(());
            }
        }
    }
    let (thread, _) = resolve_thread_session(trimmed, &server_flags(flags))?;
    params.insert("threadProjectId".into(), thread["projectId"].clone());
    params.insert("threadSessionId".into(), thread["sessionId"].clone());
    Ok(())
}

fn heading(group: &str) -> &'static str {
    HEADINGS
        .iter()
        .find(|(id, _)| *id == group)
        .map_or("the board", |(_, heading)| heading)
}

/// " (held)" after a Later group, so the answer says which kind of Later.
fn reason_words(result: &Value) -> String {
    match agents::text(result, "reason") {
        "next" => " (up next)".to_string(),
        "held" => " (held for release)".to_string(),
        "parked" => " (parked)".to_string(),
        _ => String::new(),
    }
}

/// The tags a row carries, as words: its Later reason and release, its PR, computer, untested.
fn tags(row: &Value) -> Vec<String> {
    let mut tags = Vec::new();
    match agents::text(row, "reason") {
        "next" => tags.push("up next".to_string()),
        "held" => tags.push(match agents::text(row, "release") {
            "" => "held".to_string(),
            release => format!("held for {release}"),
        }),
        "parked" => tags.push("parked".to_string()),
        _ => {}
    }
    if let Some(number) = row["pr"]["number"].as_u64() {
        let checks = match agents::text(&row["pr"], "checks") {
            "failing" => " CI failing",
            "pending" => " CI running",
            "passing" => " CI green",
            _ => "",
        };
        tags.push(format!("PR #{number}{checks}"));
    }
    let computer = agents::text(row, "computer");
    if !computer.is_empty() {
        tags.push(format!("on {computer}"));
    }
    if row["untested"] == true {
        tags.push("untested".to_string());
    }
    if row["pinned"] == true {
        tags.push("pinned".to_string());
    }
    tags
}

fn print_board(board: &Value, all: bool) {
    let mut empty = true;
    for group in board["groups"].as_array().into_iter().flatten() {
        let id = agents::text(group, "id");
        let rows = group["rows"].as_array().cloned().unwrap_or_default();
        // Done and Ideas only with --all, as they are folded in the app.
        if rows.is_empty() || (!all && matches!(id, "done" | "ideas")) {
            if !rows.is_empty() {
                println!("{} ({}): --all lists them", heading(id), rows.len());
            }
            continue;
        }
        empty = false;
        println!("{} ({})", heading(id), rows.len());
        for row in rows {
            let mut line = format!(
                "  {}  {}",
                agents::text(&row, "ref"),
                agents::text(&row, "title")
            );
            let tags = tags(&row);
            if !tags.is_empty() {
                line.push_str(&format!("  [{}]", tags.join(", ")));
            }
            let detail = agents::text(&row, "detail");
            if !detail.is_empty() {
                line.push_str(&format!("\n      {detail}"));
            }
            println!("{line}");
        }
    }
    if empty {
        println!("The board is empty: no open lines of work.");
    }
}
