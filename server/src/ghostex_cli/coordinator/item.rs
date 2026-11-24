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

const USAGE: &str = "Usage: ghostex work item [add|now|step|ask|wait|done|drop|list] … (see ghostex work --help; ghostex coordinator item is the same command)";

/// The statuses in the order `list` prints them, with their headings.
const HEADINGS: [(&str, &str); 7] = [
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    // In progress, never Moving (server/src/coordinators/work.rs `status_heading`).
    ("moving", "In progress"),
    // In progress that nobody is on (work.rs `derive`, turns.rs).
    ("idle", "Not being worked on"),
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
        ("detail", "detail"),
        ("command", "command"),
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
    let check = switch(parsed, "check", &mut words);
    match verb {
        "list" | "show" if check => {
            params.insert("op".into(), json!("list"));
            params.insert("all".into(), json!(false));
            let work = call_gxserver_rpc("/api/coordinatorWork", &Value::Object(params), &flags)?;
            let problems = turn_problems(&work);
            if json_out {
                print_json(&json!({
                    "ok": true,
                    "clear": problems.is_empty(),
                    "problems": problems.iter().map(|problem| json!({
                        "ref": problem.reference,
                        "title": problem.title,
                        "kind": problem.kind,
                        "detail": problem.detail,
                        "fix": PROBLEM_KINDS
                            .iter()
                            .find(|(kind, ..)| *kind == problem.kind)
                            .map(|(_, _, _, fix)| fix.replace("W<n>", &problem.reference)),
                    })).collect::<Vec<_>>(),
                    "counts": work["counts"],
                }));
            } else {
                print_check(&work, &problems);
            }
            return Ok(());
        }
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
        // `item delivered release v11.7.1`: an event happened (the release flow runs it), so the
        // items waiting on it come back to the coordinator to check and finish (work.rs).
        "delivered" => {
            let event = words.first().cloned().ok_or_else(|| {
                usage_error("delivered needs the event: release, ota or rebuild (then its name, such as v11.7.1).")
            })?;
            params.insert("op".into(), json!("delivered"));
            params.insert("event".into(), json!(event));
            if let Some(name) = words.get(1) {
                params.insert("name".into(), json!(name));
            }
            let answer = call_gxserver_rpc("/api/coordinatorWork", &Value::Object(params), &flags)?;
            if json_out {
                print_json(&answer);
                return Ok(());
            }
            let name = agents::text(&answer, "name");
            println!(
                "Recorded: {}{}.",
                agents::text(&answer, "event"),
                if name.is_empty() {
                    String::new()
                } else {
                    format!(" {name}")
                }
            );
            let items = answer["items"].as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                println!("No item of this coordinator was waiting on it.");
            } else {
                println!("Delivered now ({}):", items.len());
                for item in &items {
                    println!(
                        "  {}  {}",
                        agents::text(item, "ref"),
                        agents::text(item, "title")
                    );
                }
                println!("Check each is in what shipped and finish it: ghostex coordinator item done <W> \"<what was done; where it shipped>\".");
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
                            "wait needs --on W12 (an item), --on Q12 (a question), --on release, --on ota or --on rebuild (an event Ghostex follows) or --on \"words\" (something outside).",
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
            let detail = agents::text(&ask, "detail");
            if !detail.is_empty() {
                println!("  {detail}");
            }
            println!("  {}", one_line(&item));
        }
        "settle" => println!("{name}: ask settled. {}", one_line(&item)),
        "drop" => {
            println!("{name} dropped. ghostex work item restore {name} brings it back.")
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

/// `iPhone voice calls · In progress · Coordinator · 3 of 4 steps · 1m`, then where it stands.
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
            "{} of {}",
            item["steps"]["done"].as_u64().unwrap_or(0),
            ghostex_gx_protocol::plural(total, "step", "steps")
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
            "{} of {}",
            item["steps"]["done"].as_u64().unwrap_or(0),
            ghostex_gx_protocol::plural(total, "step", "steps")
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
        let detail = agents::text(ask, "detail");
        if !detail.is_empty() {
            println!("        {detail}");
        }
    }
    for wait in item["waits"].as_array().into_iter().flatten() {
        let until = agents::text(wait, "until");
        println!(
            "      waits on {}{}{}",
            match agents::text(wait, "kind") {
                "item" | "ask"
                    if !agents::text(wait, "label").starts_with(agents::text(wait, "on")) =>
                    format!(
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
            if wait["open"] == true {
                String::new()
            } else if let Some(name) = wait["delivered"].as_str() {
                format!(
                    ": happened ({name}, {})",
                    local_time(agents::text(wait, "deliveredAt"))
                )
            } else {
                ": done".to_string()
            }
        );
    }
    let delivered = agents::text(item, "delivered");
    if !delivered.is_empty() {
        println!("      delivered ({delivered}): check it shipped and finish it");
    }
    let recheck = agents::text(item, "recheck");
    if !recheck.is_empty() {
        println!("      recheck: {recheck}");
    }
    // Only the coordinator sees this: the user reads In progress and nothing to do.
    let quiet = agents::text(item, "quiet");
    if !quiet.is_empty() {
        println!("      thread quiet (heal it, the user is not told to act): {quiet}");
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
        println!("No work items yet: ghostex work item add \"<line of work>\" starts one.");
    }
    let notice = agents::text(work, "idleNotice");
    if !notice.is_empty() {
        println!("\n{notice}");
    }
}

/// One thing the end-of-turn check finds on an item.
pub(super) struct Problem {
    /// `delivered`, `idle`, `mine`, `recheck` or `quiet`.
    pub kind: &'static str,
    pub reference: String,
    pub title: String,
    /// What Ghostex said about it: the delivered event, the recheck reason, the quiet threads.
    pub detail: String,
}

/// The end-of-turn problem kinds in the order `--check` prints them: a heading, the words
/// `status` uses for "Needs a look", and the commands that settle one (`W<n>` stands for its item).
pub(super) const PROBLEM_KINDS: [(&str, &str, &str, &str); 5] = [
    (
        "delivered",
        "Delivered (what it waited on happened)",
        "delivered",
        "Check it shipped: item done W<n> \"<what; where it shipped>\", else item wait W<n> --on ...",
    ),
    (
        "idle",
        "Not being worked on (In progress, no thread on it)",
        "not being worked on",
        "Done? item done W<n> \"<what>\"; else item wait W<n> --on ..., item ask W<n>, or item now W<n>.",
    ),
    (
        "mine",
        "In progress on you, no thread on it (nobody's once your turn ends)",
        "In progress on you",
        "item done W<n> \"<what>\", item wait W<n> --on ..., item ask W<n>, or start a thread on it.",
    ),
    (
        "recheck",
        "Due a recheck",
        "due a recheck",
        "Happened or shipped? item done W<n> \"<line>\"; else item wait W<n> --on ... or item edit W<n> --status parked. Any item command counts as a recheck.",
    ),
    (
        "quiet",
        "A thread on it is not moving, even after Ghostex nudged it",
        "with a thread not moving",
        "Heal it: ghostex coordinator thread <ref>.",
    ),
];

/// CDXC:Coordinators 2026-11-22 DECISION:
/// User (context-bloat page, fix 6): before every turn end the playbook had the coordinator read
/// the whole `item list` (5 to 15 KB, every item with every thread it ever had) to spot a few
/// problems. `item list --check` prints only those, the playbook's end-of-turn rules: what it
/// waited on was delivered, In progress with nobody on it (Not being worked on, or the
/// coordinator's own In progress that ends with its turn), a Waiting or Later item due a recheck,
/// and a thread on it that is not moving even after Ghostex nudged it. `coordinator status`
/// prints the same list as its "Needs a look" line.
pub(super) fn turn_problems(work: &Value) -> Vec<Problem> {
    let mut problems = Vec::new();
    for item in work["items"].as_array().into_iter().flatten() {
        let mut add = |kind: &'static str, detail: &str| {
            problems.push(Problem {
                kind,
                reference: agents::text(item, "ref").to_string(),
                title: agents::text(item, "title").to_string(),
                detail: detail.to_string(),
            })
        };
        let delivered = agents::text(item, "delivered");
        let status = agents::text(item, "status");
        if !delivered.is_empty() {
            add("delivered", delivered);
        } else if status == "idle" {
            add("idle", "");
        } else if status == "moving" && item["owner"]["kind"] == "coordinator" {
            add("mine", "");
        }
        let recheck = agents::text(item, "recheck");
        if !recheck.is_empty() {
            add("recheck", recheck);
        }
        let quiet = agents::text(item, "quiet");
        if !quiet.is_empty() {
            add("quiet", quiet);
        }
    }
    problems
}

/// `item list --check`: the problems by kind, each kind's fix once, or one line saying all is
/// clear with the counts.
fn print_check(work: &Value, problems: &[Problem]) {
    if problems.is_empty() {
        let count = |status: &str| work["counts"][status].as_u64().unwrap_or(0);
        let mut parts = vec![match count("moving") {
            0 => "nothing in progress".to_string(),
            moving => format!("{moving} in progress (each on a running thread)"),
        }];
        for (status, words) in [
            ("waiting", "waiting (none due)"),
            ("needs", "need you (each has its ask)"),
            ("blocked", "blocked on you"),
        ] {
            if count(status) > 0 {
                parts.push(format!("{} {words}", count(status)));
            }
        }
        println!("Turn check: all clear. {}.", parts.join(", "));
        return;
    }
    println!(
        "Turn check: {} to fix before you end your turn:",
        problems.len()
    );
    for (kind, heading, _, fix) in PROBLEM_KINDS {
        let group: Vec<&Problem> = problems
            .iter()
            .filter(|problem| problem.kind == kind)
            .collect();
        if group.is_empty() {
            continue;
        }
        println!("  {heading}:");
        for problem in &group {
            let detail = if problem.detail.is_empty() {
                String::new()
            } else {
                format!(" ({})", problem.detail)
            };
            println!(
                "    {} {}{detail}",
                problem.reference,
                short_title(&problem.title, 50)
            );
        }
        let fix = match group.as_slice() {
            [one] => fix.replace("W<n>", &one.reference),
            _ => fix.to_string(),
        };
        println!("    -> {fix}");
    }
}

/// The title cut at `max` characters on a word, with "…" when it was cut.
pub(super) fn short_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        return title.to_string();
    }
    let cut: String = title.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}
