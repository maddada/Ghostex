//! `ghostex coordinator item`: the coordinator's work items (server/src/coordinators/work.rs). One
//! item per line of work; the coordinator says where it stands (`now`), records its steps, asks the
//! user (`ask`), says what it waits on (`wait`), and finishes or drops it. `list` prints them by
//! status, Needs you first, as the Work panel shows them.

use serde_json::{json, Map, Value};

use super::command::{flag_text, resolve_thread_session, server_flags, target_coordinator};
use crate::ghostex_cli::{
    agents,
    args::{Flags, ParsedArgs},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const USAGE: &str = "Usage: ghostex coordinator item [add|now|step|ask|wait|done|drop|list] … (see ghostex coordinator --help, WORK ITEMS)";

/// The statuses in the order `list` prints them, with their headings.
const HEADINGS: [(&str, &str); 6] = [
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    ("moving", "Moving"),
    ("waiting", "Waiting"),
    ("later", "Later"),
    ("finished", "Finished today"),
];

fn heading(status: &str) -> &'static str {
    HEADINGS
        .iter()
        .find(|(id, _)| *id == status)
        .map_or("Dropped", |(_, heading)| heading)
}

fn usage_error(message: &str) -> CliError {
    CliError::Other(format!("{message}\n{USAGE}"))
}

/// Every value of a repeatable flag (`--on W8 --on "Apple's review"`, `--thread=G1`): exactly the
/// word after each, so the item's line after them stays a positional word.
fn repeated(args: &[String], name: &str) -> Vec<String> {
    let flag = format!("--{name}");
    let inline = format!("--{name}=");
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(value) = arg.strip_prefix(&inline) {
            values.push(value.to_string());
        } else if *arg == flag {
            if let Some(value) = args.get(index + 1).filter(|value| !value.starts_with("--")) {
                values.push(value.clone());
                index += 1;
            }
        }
        index += 1;
    }
    values.retain(|value| !value.trim().is_empty());
    values
}

/// A switch such as `--done` or `--holds` that swallowed the text after it (`step W3 --done
/// "Rebuild"`): the switch is on and its text goes back to the positional words.
fn switch(parsed: &ParsedArgs, name: &str, words: &mut Vec<String>) -> bool {
    match parsed.flags.0.get(name) {
        Some(crate::ghostex_cli::args::FlagValue::Text(value)) => {
            words.push(value.clone());
            true
        }
        _ => parsed.flags.truthy(name),
    }
}

/// The thread a reference names, as `{serverId, projectId, sessionId}`; a thread on another
/// computer keeps that computer's id.
pub(super) fn thread_ids(reference: &str, flags: &Flags) -> CliResult<Value> {
    let trimmed = reference.trim();
    if crate::ghostex_cli::rpc::is_gxserver_global_session_ref(trimmed) {
        let mut parts = trimmed.splitn(3, ':');
        if let (Some(server), Some(project), Some(session)) =
            (parts.next(), parts.next(), parts.next())
        {
            if crate::ghostex_cli::rpc::local_gxserver_server_id().as_deref() != Some(server) {
                return Ok(
                    json!({ "serverId": server, "projectId": project, "sessionId": session }),
                );
            }
        }
    }
    let (thread, _) = resolve_thread_session(trimmed, &server_flags(flags))?;
    Ok(
        json!({ "serverId": "", "projectId": thread["projectId"], "sessionId": thread["sessionId"] }),
    )
}

pub(super) fn item(args: &[String], parsed: &ParsedArgs) -> CliResult<()> {
    let (coordinator, flags) = target_coordinator(parsed)?;
    let verb = parsed.rest.first().map(String::as_str).unwrap_or("list");
    let mut words: Vec<String> = parsed.rest.iter().skip(1).cloned().collect();
    let mut params = Map::new();
    params.insert("projectId".into(), coordinator["projectId"].clone());
    params.insert("sessionId".into(), coordinator["sessionId"].clone());
    let threads = repeated(args, "thread")
        .iter()
        .map(|reference| thread_ids(reference, &parsed.flags))
        .collect::<CliResult<Vec<_>>>()?;
    if !threads.is_empty() {
        params.insert("threads".into(), json!(threads));
    }
    for (key, flag) in [
        ("link", "link"),
        ("release", "release"),
        ("title", "title"),
        ("status", "status"),
        ("until", "until"),
        ("steps", "steps"),
        ("tag", "tag"),
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
    let json_out = switch(parsed, "json", &mut words);
    let all = switch(parsed, "all", &mut words);
    match verb {
        "list" | "show" => {
            params.insert("op".into(), json!("list"));
            params.insert("all".into(), json!(all || !words.is_empty()));
            let work = call_gxserver_rpc("/api/coordinatorWork", &Value::Object(params), &flags)?;
            if let Some(reference) = words.first() {
                let wanted = reference.trim().to_ascii_uppercase().replacen('B', "W", 1);
                let item = work["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|item| {
                        agents::text(item, "ref").eq_ignore_ascii_case(&wanted)
                            || agents::text(item, "ref") == reference.trim()
                    })
                    .ok_or_else(|| CliError::Other(format!("No item {reference}.")))?;
                if json_out {
                    print_json(item);
                } else {
                    print_item(item, true);
                }
            } else if json_out {
                print_json(&work);
            } else {
                print_list(&work, all);
            }
            return Ok(());
        }
        "add" => {
            let title = words
                .first()
                .cloned()
                .ok_or_else(|| usage_error("add needs a title."))?;
            params.insert("title".into(), json!(title));
            if let Some(line) = flag_text(&parsed.flags, "now") {
                params.insert("now".into(), json!(line));
            }
            if let Some(later) = flag_text(&parsed.flags, "later") {
                params.insert("status".into(), json!(later));
            }
        }
        "now" | "step" | "ask" | "wait" | "done" | "drop" | "restore" | "edit" => {
            let reference = words
                .first()
                .cloned()
                .ok_or_else(|| usage_error(&format!("{verb} needs an item (W7).")))?;
            params.insert("item".into(), json!(reference));
            let done = switch(parsed, "done", &mut words);
            let holds = switch(parsed, "holds", &mut words);
            let chore = switch(parsed, "chore", &mut words);
            let line = words.get(1).cloned();
            match verb {
                "now" => {
                    params.insert(
                        "text".into(),
                        json!(line.ok_or_else(|| usage_error(
                            "now needs a line saying where it stands."
                        ))?),
                    );
                }
                "step" => {
                    params.insert(
                        "text".into(),
                        json!(line.ok_or_else(|| usage_error("step needs the step's words."))?),
                    );
                    params.insert("done".into(), json!(done));
                    params.insert("chore".into(), json!(chore));
                }
                "ask" => {
                    // `ask W7 --done Q12` (the user did it) or `--withdraw Q12` settles an ask.
                    let settle = flag_text(&parsed.flags, "done")
                        .filter(|tag| tag.to_ascii_uppercase().starts_with('Q'))
                        .map(|tag| (tag, false))
                        .or_else(|| flag_text(&parsed.flags, "withdraw").map(|tag| (tag, true)));
                    if let Some((tag, withdraw)) = settle {
                        words.retain(|word| *word != tag);
                        params.insert("op".into(), json!("settle"));
                        params.insert("ask".into(), json!(tag));
                        params.insert("withdraw".into(), json!(withdraw));
                    } else {
                        params.insert(
                            "text".into(),
                            json!(words.get(1).cloned().ok_or_else(|| usage_error(
                                "ask needs what you ask the user, in their words."
                            ))?),
                        );
                        params.insert("holds".into(), json!(holds));
                        // `--answer` repeated keeps each answer whole, commas and all.
                        let each = repeated(args, "answer");
                        if !each.is_empty() {
                            let recommended =
                                flag_text(&parsed.flags, "recommended").unwrap_or_default();
                            params.insert(
                                "answers".into(),
                                json!(crate::coordinators::mark_answer_list(&each, &recommended)),
                            );
                        } else if let Some(answers) = flag_text(&parsed.flags, "answers") {
                            let recommended =
                                flag_text(&parsed.flags, "recommended").unwrap_or_default();
                            params.insert(
                                "answers".into(),
                                json!(crate::coordinators::mark_answers(&answers, &recommended)),
                            );
                        }
                    }
                }
                "wait" => {
                    let on = repeated(args, "on");
                    if on.is_empty() {
                        return Err(usage_error(
                            "wait needs --on W12 (an item), --on Q12 (an ask) or --on \"words\" (something outside).",
                        ));
                    }
                    params.insert("on".into(), json!(on));
                    if let Some(line) = line {
                        params.insert("text".into(), json!(line));
                    }
                }
                "done" => {
                    if let Some(line) = line {
                        params.insert("text".into(), json!(line));
                    }
                }
                "edit" => {
                    let detach = repeated(args, "detach");
                    if !detach.is_empty() {
                        params.insert("detach".into(), json!(detach));
                    }
                }
                _ => {}
            }
        }
        other => return Err(usage_error(&format!("Unknown item command: {other}."))),
    }
    if !params.contains_key("op") {
        params.insert("op".into(), json!(verb));
    }
    let item = call_gxserver_rpc(
        "/api/coordinatorWork",
        &Value::Object(params.clone()),
        &flags,
    )?;
    if json_out {
        print_json(&item);
        return Ok(());
    }
    let name = agents::text(&item, "ref");
    match params.get("op").and_then(Value::as_str).unwrap_or(verb) {
        "add" => println!("{name} added: {}", one_line(&item)),
        "ask" => {
            let ask = item["asks"]
                .as_array()
                .and_then(|asks| asks.last())
                .cloned()
                .unwrap_or(Value::Null);
            println!(
                "{name} asks {}: {}{}",
                agents::text(&ask, "tag"),
                agents::text(&ask, "text"),
                if ask["holds"] == true {
                    " (holds the work up)"
                } else {
                    ""
                }
            );
            println!("  {}", one_line(&item));
        }
        "settle" => println!("{name}: ask settled. {}", one_line(&item)),
        "drop" => {
            println!("{name} dropped. ghostex coordinator item restore {name} brings it back.")
        }
        _ => println!("{name} {}", one_line(&item)),
    }
    Ok(())
}

/// An RFC 3339 time as this computer's local "2026-11-10 01:52".
fn local_time(at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(at)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| at.to_string())
}

/// How long ago an RFC 3339 time was ("12m", "3h", "2d").
fn age(at: &str) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(at).ok()?;
    let minutes = chrono::Utc::now()
        .signed_duration_since(at)
        .num_minutes()
        .max(0);
    Some(match minutes {
        0 => "just now".to_string(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    })
}

/// The owner in words: You, Coordinator, a thread's title, what it waits on.
fn owner_words(item: &Value) -> String {
    let owner = &item["owner"];
    let label = agents::text(owner, "label");
    match agents::text(owner, "kind") {
        "thread" => format!("Thread {}", label),
        "item" => format!("waits on {} {label}", agents::text(owner, "ref")),
        "outside" if !label.is_empty() => format!("waits on {label}"),
        _ => label.to_string(),
    }
}

/// `iPhone voice calls · Moving · Coordinator · 3 of 4 steps · 1m`, then where it stands.
fn one_line(item: &Value) -> String {
    let mut parts = vec![
        agents::text(item, "title").to_string(),
        heading(agents::text(item, "status")).replace(" today", ""),
    ];
    let owner = owner_words(item);
    if !owner.is_empty() {
        parts.push(owner);
    }
    let total = item["steps"]["total"].as_u64().unwrap_or(0);
    if total > 0 {
        parts.push(format!(
            "{} of {total} steps",
            item["steps"]["done"].as_u64().unwrap_or(0)
        ));
    }
    let mut line = parts.join(" · ");
    let now = agents::text(item, "now");
    if !now.is_empty() {
        line.push_str(&format!(": {now}"));
    }
    line
}

fn print_item(item: &Value, full: bool) {
    let mut head = format!(
        "  {}  {}",
        agents::text(item, "ref"),
        agents::text(item, "title")
    );
    let mut tags = Vec::new();
    let owner = owner_words(item);
    if !owner.is_empty() {
        tags.push(owner);
    }
    let total = item["steps"]["total"].as_u64().unwrap_or(0);
    if total > 0 {
        tags.push(format!(
            "{} of {total} steps",
            item["steps"]["done"].as_u64().unwrap_or(0)
        ));
    }
    let reason = agents::text(item, "reason");
    if !reason.is_empty() {
        tags.push(match reason {
            "next" => "up next".to_string(),
            "held" => match agents::text(item, "release") {
                "" => "held".to_string(),
                release => format!("held for {release}"),
            },
            other => other.to_string(),
        });
    }
    let flag = agents::text(item, "flag");
    if !flag.is_empty() {
        tags.push(flag.to_string());
    }
    if item["untested"] == true {
        tags.push("untested".to_string());
    }
    if let Some(age) = age(agents::text(item, "at")) {
        tags.push(age);
    }
    if !tags.is_empty() {
        head.push_str(&format!("  [{}]", tags.join(", ")));
    }
    println!("{head}");
    let now = agents::text(item, "now");
    if !now.is_empty() {
        println!("      {now}");
    }
    for ask in item["asks"].as_array().into_iter().flatten() {
        if ask["open"] != true && !full {
            continue;
        }
        let answers: Vec<String> = ask["answers"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|choice| {
                let label = agents::text(choice, "label").to_string();
                if choice["recommended"] == true {
                    format!("{label}*")
                } else {
                    label
                }
            })
            .collect();
        let state = if ask["open"] == true {
            if ask["holds"] == true {
                "for you, holds it up".to_string()
            } else {
                "for you".to_string()
            }
        } else if ask["withdrawn"] == true {
            "withdrawn".to_string()
        } else {
            format!("answered: {}", agents::text(ask, "answered"))
        };
        println!(
            "      ? {} {}{}  ({state})",
            agents::text(ask, "tag"),
            agents::text(ask, "text"),
            if answers.is_empty() {
                String::new()
            } else {
                format!(" [{}]", answers.join(" | "))
            }
        );
    }
    for wait in item["waits"].as_array().into_iter().flatten() {
        let until = agents::text(wait, "until");
        println!(
            "      waits on {}{}{}",
            match agents::text(wait, "kind") {
                "item" | "ask" => format!(
                    "{} {}",
                    agents::text(wait, "on"),
                    agents::text(wait, "label")
                ),
                _ => agents::text(wait, "label").to_string(),
            },
            if until.is_empty() {
                String::new()
            } else {
                format!(" (until {until})")
            },
            if wait["open"] == true { "" } else { ": done" }
        );
    }
    if item["kind"] == "item" {
        for thread in item["threads"].as_array().into_iter().flatten() {
            let state = agents::text(thread, "state");
            println!(
                "      thread {} {}{}",
                agents::text(thread, "sessionId"),
                agents::text(thread, "title"),
                if state.is_empty() {
                    String::new()
                } else {
                    format!(" ({state})")
                }
            );
        }
    }
    if full {
        let rows = item["steps"]["rows"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !rows.is_empty() {
            println!("      Steps:");
            for step in rows {
                println!(
                    "        [{}] {}",
                    if step["done"] == true { "x" } else { " " },
                    agents::text(&step, "text")
                );
            }
        }
        let history = item["history"].as_array().cloned().unwrap_or_default();
        if !history.is_empty() {
            println!("      History:");
            for event in history {
                let words = agents::text(&event, "text");
                println!(
                    "        {}  {}{}",
                    local_time(agents::text(&event, "at")),
                    agents::text(&event, "kind"),
                    if words.is_empty() {
                        String::new()
                    } else {
                        format!(": {words}")
                    }
                );
            }
        }
    }
}

fn print_list(work: &Value, all: bool) {
    let items = work["items"].as_array().cloned().unwrap_or_default();
    let mut empty = true;
    for (status, _) in HEADINGS {
        // A task left for the user by a finished item is still theirs: it lists under Needs you,
        // as the Work panel shows it.
        let has_open_ask = |item: &&Value| {
            item["asks"]
                .as_array()
                .is_some_and(|asks| asks.iter().any(|ask| ask["open"] == true))
        };
        let listed: Vec<&Value> = items
            .iter()
            .filter(|item| {
                item["status"] == status
                    || (status == "needs" && item["status"] == "finished" && has_open_ask(item))
            })
            // Finished means today's, unless --all.
            .filter(|item| all || status != "finished" || item["today"] != false)
            .collect();
        if listed.is_empty() {
            continue;
        }
        empty = false;
        let title = if all && status == "finished" {
            "Finished"
        } else {
            heading(status)
        };
        if status == "finished" && !all {
            println!("{title} ({}): --all lists them", listed.len());
            continue;
        }
        println!("{title} ({})", listed.len());
        for item in listed {
            print_item(item, false);
        }
    }
    if all {
        let dropped: Vec<&Value> = items
            .iter()
            .filter(|item| item["status"] == "dropped")
            .collect();
        if !dropped.is_empty() {
            empty = false;
            println!("Dropped ({})", dropped.len());
            for item in dropped {
                print_item(item, false);
            }
        }
    }
    if empty {
        println!("No work items yet: ghostex coordinator item add \"<line of work>\" starts one.");
    }
}
