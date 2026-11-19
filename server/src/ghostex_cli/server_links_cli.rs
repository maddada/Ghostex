//! `ghostex computers add|remove|access`: server links between two computers' gxservers, with no
//! desktop app holding them (GitHub #674; CDXC:ServerLinks 2026-11-19 DECISION in
//! server/src/remote_access/server_links.rs). gxserver does the work (`/api/serverLinks`); this
//! starts it and follows it, saying what to approve and where.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::args::parse_args;
use super::rpc::{call_gxserver_rpc, CliError, CliResult};

const ENDPOINT: &str = "/api/serverLinks";
const POLL_EVERY: Duration = Duration::from_secs(2);
/// A pairing waits for its approval about this long; the CLI stops following it after this.
const FOLLOW_FOR: Duration = Duration::from_secs(15 * 60);

fn call(params: Value) -> CliResult<Value> {
    call_gxserver_rpc(ENDPOINT, &params, &super::args::Flags::default())
}

fn links() -> Vec<Value> {
    call(json!({ "op": "list" }))
        .ok()
        .and_then(|answer| answer.get("links").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn flag(row: &Value, key: &str) -> bool {
    row.get(key).and_then(Value::as_bool) == Some(true)
}

fn words(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .map(|words| {
            words
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .unwrap_or_default()
}

/// `ghostex computers add <code>`.
pub(crate) fn add(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let Some(code) = parsed.rest.first() else {
        return Err(CliError::Other(
            "Usage: ghostex computers add <code>\nRun ghostex computer-code on the other computer and paste the code it prints."
                .to_string(),
        ));
    };
    let started = call(json!({ "op": "add", "code": code }))?;
    let other = text(&started, "computerName").to_string();
    let server_id = text(&started, "serverId").to_string();
    let here = call(json!({ "op": "list" }))
        .ok()
        .and_then(|answer| answer.get("name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(crate::connected_computers::this_computer_name);
    println!(
        "Asked {other} to link with {here}. Approve it in Ghostex or on your phone, where it asks \"Let {here} link its agents with {other}?\"."
    );
    println!(
        "Approve only if it shows these words: {}",
        words(started.get("words"))
    );
    if parsed.flags.truthy("noWait") {
        return Ok(());
    }
    println!("Waiting for your approval. Press Ctrl+C to stop following (the link keeps going).");
    let deadline = Instant::now() + FOLLOW_FOR;
    let mut said_linked = false;
    let mut said_reverse = String::new();
    while Instant::now() < deadline {
        std::thread::sleep(POLL_EVERY);
        let Some(row) = links()
            .into_iter()
            .find(|row| text(row, "serverId") == server_id)
        else {
            return Err(CliError::Other(format!(
                "The link with {other} is gone. Run ghostex computers add again with a new code."
            )));
        };
        match text(&row, "state") {
            "ended" => {
                return Err(CliError::Other(format!(
                    "Not linked with {other}: {}",
                    text(&row, "error")
                )));
            }
            "linked" if !said_linked => {
                said_linked = true;
                println!("Approved: agents here can now use {other}.");
            }
            _ => {}
        }
        if said_linked {
            if flag(&row, "incoming") && flag(&row, "incomingLive") {
                println!("Linked both ways: agents on {other} can now use {here} too. Run ghostex computers to see it.");
                return Ok(());
            }
            if let Some(reverse) = row.get("reverse").filter(|reverse| !reverse.is_null()) {
                let error = text(reverse, "error");
                let reverse_words = words(reverse.get("words"));
                let said = format!("{error}{reverse_words}");
                if said != said_reverse && !said.is_empty() {
                    said_reverse = said;
                    if error.is_empty() {
                        println!(
                            "Now {other} asks to link back. Approve it in Ghostex or on your phone, where it asks \"Let {other} link its agents with {here}?\", only if it shows these words: {reverse_words}"
                        );
                    } else {
                        println!("{other} couldn't be asked to link back yet ({error}). It is asked again in five minutes.");
                    }
                }
            }
        }
    }
    println!(
        "Stopped following. The link keeps going: run ghostex computers to see where it stands."
    );
    Ok(())
}

/// The server link row `wanted` names: its server id, or its name (exactly, then the start).
fn pick(wanted: &str) -> CliResult<Value> {
    let rows = links();
    let lower = wanted.trim().to_lowercase();
    let exact: Vec<&Value> = rows
        .iter()
        .filter(|row| text(row, "serverId") == wanted || text(row, "name").to_lowercase() == lower)
        .collect();
    let found: Vec<&Value> = if exact.is_empty() {
        rows.iter()
            .filter(|row| text(row, "name").to_lowercase().starts_with(&lower))
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(CliError::Other(format!(
            "No server link is called \"{wanted}\". Server links: {}.",
            if rows.is_empty() {
                "none".to_string()
            } else {
                rows.iter()
                    .map(|row| format!("{} ({})", text(row, "name"), text(row, "serverId")))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ))),
        _ => Err(CliError::Other(format!(
            "\"{wanted}\" matches more than one server link. Use its server id."
        ))),
    }
}

/// `ghostex computers remove <name>`.
pub(crate) fn remove(args: &[String]) -> CliResult<()> {
    let Some(wanted) = parse_args(args).rest.first().cloned() else {
        return Err(CliError::Other(
            "Usage: ghostex computers remove <name>".to_string(),
        ));
    };
    let row = pick(&wanted)?;
    let answer = call(json!({ "op": "remove", "serverId": text(&row, "serverId") }))?;
    let name = text(&row, "name");
    if flag(&answer, "told") || !flag(&row, "outgoing") {
        println!("Ended the server link with {name}, on both computers.");
    } else {
        println!(
            "Ended the server link with {name} here. {name} couldn't be reached, so it ends its side the next time it reaches this computer."
        );
    }
    Ok(())
}

/// `ghostex computers access <name> on|off`.
pub(crate) fn access(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let (Some(wanted), Some(switch)) = (parsed.rest.first(), parsed.rest.get(1)) else {
        return Err(CliError::Other(
            "Usage: ghostex computers access <name> on|off".to_string(),
        ));
    };
    let on = match switch.as_str() {
        "on" => true,
        "off" => false,
        _ => {
            return Err(CliError::Other(
                "Usage: ghostex computers access <name> on|off".to_string(),
            ))
        }
    };
    let row = pick(wanted)?;
    call(json!({ "op": "setAgentAccess", "serverId": text(&row, "serverId"), "on": on }))?;
    println!(
        "Agent access with {} is {}.",
        text(&row, "name"),
        if on { "on" } else { "off" }
    );
    Ok(())
}

/// Whether a server link (not the desktop app) connects this computer with `server_id`.
pub(crate) fn is_server_link(server_id: &str) -> bool {
    links()
        .iter()
        .any(|row| text(row, "serverId") == server_id)
}

/// The server links under `ghostex computers`' table.
pub(crate) fn print_summary() {
    let rows = links();
    if rows.is_empty() {
        return;
    }
    println!("\nServer links (kept by gxserver here, no app needed):");
    for row in rows {
        let state = match text(&row, "state") {
            "pairing" => format!("waiting for approval ({})", words(row.get("words"))),
            "ended" => format!("ended: {}", text(&row, "error")),
            _ if !flag(&row, "agentAccess") => "Agent access off".to_string(),
            _ => match (flag(&row, "outgoing"), flag(&row, "incoming")) {
                (true, true) => "linked both ways".to_string(),
                (true, false) => "linked from here; waiting for it to link back".to_string(),
                (false, true) => "it reaches here; not linked from here".to_string(),
                (false, false) => "not linked".to_string(),
            },
        };
        println!(
            "  {}\t{}\t{state}",
            text(&row, "name"),
            text(&row, "serverId")
        );
    }
}
