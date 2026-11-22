//! `ghostex coordinator status` in parts: the compact snapshot a plain `status` prints, and
//! `status threads`, `status memory`, `status usage` and `coordinator thread <ref>` (the brief on
//! one thread). `status --full` is the whole view as before (command.rs `print_status`), and
//! `--json` stays complete.
//!
//! CDXC:Coordinators 2026-11-22 DECISION:
//! User, on the context-bloat page (Q4390-era fixes, "coordinator status: what the default
//! prints"), picked A, the compact snapshot: what needs a look, usage room, live threads and
//! counts, about 0.7 KB in the same shape every time. Status printed 27 KB, 93% of it every
//! memory note and every sleeping thread's last report, and coordinators ran it at every request
//! and after each compaction. The rest moved to focused parts, read only when needed.
//! A report Ghostex already forwarded to the coordinator's chat is not printed again: `thread
//! <ref>` gives its state, account, the report's headline and its section names (4.1 KB of
//! `read-session-chat --last 2` became about 0.4 KB).
//! SEE-ALSO: server/src/coordinators/playbook.md ("Start of a request", "While threads run"),
//! server/src/ghostex_cli/coordinator/item.rs (`turn_problems`), usage_room.rs.

use serde_json::{Value, json};

use super::command::{flag_text, read_view, resolve_thread_session, server_flags};
use super::{item, usage_room};
use crate::ghostex_cli::{
    agents,
    args::{Flags, ParsedArgs},
    output::print_json,
    rpc::{CliError, CliResult, call_gxserver_rpc},
};

/// The thread states the compact snapshot lists one by one, in this order; the rest are counted.
const LIVE: [&str; 4] = ["waiting", "finished", "working", "unreachable"];

/// A thread's short name: its session id here, its whole reference on another computer.
fn short_ref(thread: &Value) -> &str {
    if agents::text(thread, "computer").is_empty() {
        let id = agents::text(thread, "sessionId");
        if !id.is_empty() {
            return id;
        }
    }
    agents::text(thread, "globalRef")
}

fn threads_of(view: &Value) -> Vec<Value> {
    view["threads"].as_array().cloned().unwrap_or_default()
}

fn count(threads: &[Value], state: &str) -> usize {
    threads
        .iter()
        .filter(|thread| agents::text(thread, "state") == state)
        .count()
}

/// "12m", "3h 5m" since an RFC 3339 time.
fn age(at: &str) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(at).ok()?;
    let minutes = chrono::Utc::now()
        .signed_duration_since(at)
        .num_minutes()
        .max(0);
    Some(match minutes {
        0 => "under a minute".to_string(),
        1..=119 => format!("{minutes}m"),
        120..=2879 => format!("{}h {}m", minutes / 60, minutes % 60),
        _ => format!("{}d", minutes / 1440),
    })
}

fn local_time(at: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(at).ok().map(|at| {
        at.with_timezone(&chrono::Local)
            .format("%b %-d %H:%M")
            .to_string()
    })
}

fn headline(text: &str, max: usize) -> String {
    crate::coordinators::report_headline(text, max)
}

/// One live thread on one line: state, short ref, title and what to know about it now.
fn thread_line(thread: &Value) -> String {
    let state = agents::text(thread, "state");
    let title = match agents::text(thread, "title") {
        "" => "(untitled)",
        title => title,
    };
    let mut line = format!("  {state:<8} {:<6} {}", short_ref(thread), title);
    match state {
        "working" => {
            let mut clock = Vec::new();
            if let Some(since) = age(agents::text(thread, "workingStartedAt")) {
                clock.push(since);
            }
            if let Some(output) = age(agents::text(thread, "lastOutputAt")) {
                clock.push(format!("output {output} ago"));
            }
            if !clock.is_empty() {
                line.push_str(&format!("  ({})", clock.join(", ")));
            }
            if thread["stalled"] == true {
                line.push_str(if thread["stuck"] == true {
                    "  NOT MOVING after Ghostex's nudge: heal it"
                } else {
                    "  quiet: no new output"
                });
            }
        }
        "waiting" => {
            let waiting_for = agents::text(thread, "waitingFor");
            if !waiting_for.is_empty() {
                line.push_str(&format!(": {}", headline(waiting_for, 100)));
            }
        }
        "finished" => {
            let report = agents::text(thread, "lastReport");
            if !report.is_empty() {
                line.push_str(&format!(": {}", headline(report, 80)));
            }
        }
        _ => {}
    }
    line
}

/// The work items, for "Needs a look"; `None` when they can't be read.
fn read_work(view: &Value, flags: &Flags) -> Option<Value> {
    let coordinator = &view["coordinator"];
    call_gxserver_rpc(
        "/api/coordinatorWork",
        &json!({
            "op": "list",
            "all": false,
            "projectId": coordinator["projectId"],
            "sessionId": coordinator["sessionId"],
        }),
        flags,
    )
    .ok()
}

/// "W858 not being worked on · 3 due a recheck (W264, W328, W365)": the end-of-turn problems
/// grouped by kind, and the threads that need the coordinator.
fn needs_look(view: &Value, work: Option<&Value>) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(work) = work {
        let problems = item::turn_problems(work);
        for (kind, _, words, _) in item::PROBLEM_KINDS {
            let refs: Vec<&str> = problems
                .iter()
                .filter(|problem| problem.kind == kind)
                .map(|problem| problem.reference.as_str())
                .collect();
            parts.push(match refs.as_slice() {
                [] => continue,
                [one] => format!("{one} {words}"),
                many if many.len() <= 4 => format!("{} {words} ({})", many.len(), many.join(", ")),
                many => format!("{} {words} ({}, …)", many.len(), many[..3].join(", ")),
            });
        }
        let asks = work["asks"].as_array().map_or(0, Vec::len);
        if asks > 0 {
            parts.push(format!(
                "{asks} ask{} open for the user",
                if asks == 1 { "" } else { "s" }
            ));
        }
    }
    let threads = threads_of(view);
    let waiting = count(&threads, "waiting");
    if waiting > 0 {
        parts.push(format!(
            "{waiting} thread{} waiting on you",
            if waiting == 1 { "" } else { "s" }
        ));
    }
    let finished = count(&threads, "finished");
    if finished > 0 {
        parts.push(format!(
            "{finished} finished thread{} to resolve",
            if finished == 1 { "" } else { "s" }
        ));
    }
    parts
}

/// The plain `coordinator status`: the compact snapshot (about 0.7 KB).
pub(super) fn print_compact(view: &Value, flags: &Flags) {
    for key in ["playbookNotice", "networkNotice"] {
        let notice = agents::text(view, key);
        if !notice.is_empty() {
            println!("{notice}");
        }
    }
    let coordinator = &view["coordinator"];
    let goal = agents::text(coordinator, "goal");
    let instructions = agents::text(coordinator, "instructions");
    println!(
        "Coordinator: {} ({}) · {} · {}",
        agents::text(coordinator, "title"),
        agents::text(coordinator, "sessionId"),
        if goal.is_empty() {
            "goal not set".to_string()
        } else {
            format!("goal: {}", headline(goal, 80))
        },
        match instructions
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
        {
            0 => "no standing instructions".to_string(),
            1 => "1 line of standing instructions (status memory)".to_string(),
            lines => format!("{lines} lines of standing instructions (status memory)"),
        }
    );
    let work = read_work(view, flags);
    let look = needs_look(view, work.as_ref());
    if look.is_empty() {
        println!("Needs a look: nothing");
    } else {
        println!("Needs a look: {}", look.join(" · "));
    }
    if !view["usageRoom"].is_null() {
        usage_room::print_compact(&view["usageRoom"]);
    }
    let threads = threads_of(view);
    if threads.is_empty() {
        println!(
            "Threads: none yet. Start one with: ghostex coordinator start-thread --title \"<title>\" --task \"<brief>\""
        );
    } else {
        let counts: Vec<String> = LIVE
            .iter()
            .chain(["sleeping", "closed", "done"].iter())
            .filter_map(|state| {
                let n = count(&threads, state);
                (n > 0).then(|| {
                    format!(
                        "{n} {}",
                        match *state {
                            "unreachable" => "on a computer that isn't connected",
                            other => other,
                        }
                    )
                })
            })
            .collect();
        println!("Threads: {}", counts.join(", "));
        for state in LIVE {
            for thread in threads
                .iter()
                .filter(|thread| agents::text(thread, "state") == state)
            {
                println!("{}", thread_line(thread));
            }
        }
    }
    let memory = coordinator["memory"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let newest = memory
        .iter()
        .filter_map(|note| note["createdAt"].as_str())
        .max()
        .and_then(local_time);
    println!(
        "Memory: {}{} (ghostex coordinator status memory prints them)",
        match memory.len() {
            0 => "no notes".to_string(),
            1 => "1 note".to_string(),
            n => format!("{n} notes"),
        },
        newest
            .map(|at| format!(", newest {at}"))
            .unwrap_or_default()
    );
    println!(
        "More: status threads [--sleeping] · status thread <ref> · status memory · status usage · item list --check · accounts status · --full"
    );
}

/// `status memory`: the goal, the standing instructions and every memory note.
pub(super) fn print_memory(view: &Value) {
    let coordinator = &view["coordinator"];
    let goal = agents::text(coordinator, "goal");
    println!("Goal: {}", if goal.is_empty() { "(not set)" } else { goal });
    let instructions = agents::text(coordinator, "instructions");
    if instructions.is_empty() {
        println!("Standing instructions: (none)");
    } else {
        println!("Standing instructions:");
        for line in instructions.lines() {
            println!("  {line}");
        }
    }
    let memory = coordinator["memory"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if memory.is_empty() {
        println!("Memory: (no notes)");
        return;
    }
    println!("Memory ({} notes):", memory.len());
    for (index, note) in memory.iter().enumerate() {
        println!("  {}. {}", index + 1, agents::text(note, "text"));
    }
}

/// `status threads`: every live thread with its detail; `--sleeping` lists the sleeping ones (one
/// line each, newest report first), `--all` the done ones too.
pub(super) fn print_threads(view: &Value, parsed: &ParsedArgs) {
    let threads = threads_of(view);
    let sleeping = parsed.flags.truthy("sleeping");
    let all = parsed.flags.truthy("all");
    if !sleeping && !all {
        let live: Vec<&Value> = LIVE
            .iter()
            .flat_map(|state| {
                threads
                    .iter()
                    .filter(move |thread| agents::text(thread, "state") == *state)
            })
            .collect();
        if live.is_empty() {
            println!("No live threads.");
        }
        for thread in live {
            println!("{}", thread_line(thread));
            let branch = agents::text(thread, "branch");
            if !branch.is_empty() {
                println!("           branch {branch}");
            }
            if agents::text(thread, "state") == "working"
                && agents::text(thread, "lastReport").is_empty()
            {
                println!(
                    "           task: {}",
                    headline(agents::text(thread, "task"), 160)
                );
            }
        }
        println!(
            "{} sleeping (status threads --sleeping), {} done (--all); status thread <ref> shows one.",
            count(&threads, "sleeping"),
            count(&threads, "done")
        );
        return;
    }
    let states: &[&str] = if all {
        &["sleeping", "closed", "done"]
    } else {
        &["sleeping"]
    };
    for state in states {
        let mut group: Vec<&Value> = threads
            .iter()
            .filter(|thread| agents::text(thread, "state") == *state)
            .collect();
        if group.is_empty() {
            continue;
        }
        // Newest report first; a thread that never reported sorts by when it started.
        group.sort_by_key(|thread| {
            std::cmp::Reverse(
                match agents::text(thread, "reportedAt") {
                    "" => agents::text(thread, "createdAt"),
                    at => at,
                }
                .to_string(),
            )
        });
        let heading = match *state {
            "sleeping" => "Sleeping",
            "closed" => "Closed",
            _ => "Done",
        };
        println!(
            "{heading} ({}), newest report first; status thread <ref> shows one:",
            group.len()
        );
        for thread in group {
            println!(
                "  {:<6} {}",
                short_ref(thread),
                agents::text(thread, "title")
            );
        }
    }
}

/// The report's section names ("Done", "Untested", "Read this"): its marked or bold headings.
fn report_sections(report: &str) -> Vec<String> {
    let mut sections = Vec::new();
    for line in report.lines() {
        let trimmed = line.trim();
        let rest = trimmed
            .trim_start_matches(|c: char| !c.is_ascii_alphanumeric() && c != '*' && c != '#');
        let name = if let Some(heading) = rest.strip_prefix('#') {
            heading.trim_start_matches('#').trim()
        } else if let Some(inner) = rest.strip_prefix("**").and_then(|r| r.strip_suffix("**")) {
            // A bold line that is its own heading, not a bold phrase opening a bullet.
            if inner.contains("**")
                || trimmed.starts_with('-')
                || trimmed.starts_with('*') && !trimmed.starts_with("**")
            {
                continue;
            }
            inner
        } else {
            continue;
        };
        let name = name.trim().trim_end_matches(':').trim();
        if !name.is_empty() && name.len() <= 40 && !sections.iter().any(|known| known == name) {
            sections.push(name.to_string());
        }
    }
    sections
}

/// The account a session runs on, as "Claude #4", from the roster Ghostex keeps.
fn account_of(global_ref: &str, flags: &Flags) -> Option<String> {
    let roster = call_gxserver_rpc(
        "/api/agentAccounts",
        &json!({ "operation": "roster", "cachedOnly": true }),
        flags,
    )
    .ok()?;
    roster["accounts"]
        .as_array()?
        .iter()
        .find(|row| {
            row["sessions"].as_array().is_some_and(|sessions| {
                sessions
                    .iter()
                    .any(|session| agents::text(session, "globalRef") == global_ref)
            })
        })
        .map(|row| {
            format!(
                "{} #{}",
                match agents::text(row, "provider") {
                    "claude" => "Claude",
                    "codex" => "Codex",
                    other => other,
                },
                agents::text(row, "number")
            )
        })
}

/// The newest tool call in a working thread's chat, as "Bash: Build the server".
fn last_tool_call(global_ref: &str, flags: &Flags) -> Option<String> {
    let chat = crate::ghostex_cli::actions::send_gxserver_cli_action(
        "readSessionChat",
        &json!({ "sessionId": global_ref, "limit": 3 }),
        flags,
    )
    .ok()?;
    let call = chat["messages"]
        .as_array()?
        .iter()
        .rev()
        .flat_map(|message| {
            message["blocks"]
                .as_array()
                .into_iter()
                .flatten()
                .rev()
                .collect::<Vec<_>>()
        })
        .find(|block| block["type"] == "tool-call")?;
    let input = &call["input"];
    // Claude's calls carry an object; Codex's `exec` carries its script as one string.
    let words = input.as_str().map(str::trim).unwrap_or_else(|| {
        [
            "description",
            "command",
            "cmd",
            "file_path",
            "path",
            "pattern",
            "prompt",
        ]
        .iter()
        .find_map(|key| input[*key].as_str().filter(|text| !text.trim().is_empty()))
        .unwrap_or_default()
    });
    let name = agents::text(call, "name");
    Some(if words.is_empty() {
        name.to_string()
    } else {
        format!("{name}: {}", headline(words, 90))
    })
}

/// The thread's last report as its chat has it: the stored `lastReport` is cut short, so its size
/// and section names come from the chat's newest assistant text when it starts the same way.
fn full_report(global_ref: &str, stored: &str, flags: &Flags) -> Option<String> {
    let chat = crate::ghostex_cli::actions::send_gxserver_cli_action(
        "readSessionChat",
        &json!({ "sessionId": global_ref, "limit": 2 }),
        flags,
    )
    .ok()?;
    let text: String = chat["messages"]
        .as_array()?
        .iter()
        .rev()
        .find(|message| message["role"] == "assistant")?["blocks"]
        .as_array()?
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let start: String = stored.trim().chars().take(40).collect();
    (text.len() > stored.len() && text.trim_start().starts_with(&start)).then_some(text)
}

/// `coordinator thread <ref>` (also `status thread <ref>`): one thread in a few lines.
pub(super) fn thread_brief(parsed: &ParsedArgs, reference: Option<&str>) -> CliResult<()> {
    let reference = reference.ok_or_else(|| {
        CliError::Other("Usage: ghostex coordinator thread <thread-ref> [--json]".into())
    })?;
    let mut base = server_flags(&parsed.flags);
    if let Some(computer) = flag_text(&parsed.flags, "computer") {
        base.insert_text("computer", &computer);
    }
    let (session, flags) = resolve_thread_session(reference, &base)?;
    // A thread's key reads its coordinator's view (endpoint.rs `resolve_coordinator`).
    let view = read_view(&session, &flags)?;
    let thread = threads_of(&view)
        .into_iter()
        .find(|thread| {
            thread["sessionId"] == session["sessionId"]
                && thread["projectId"] == session["projectId"]
        })
        .ok_or_else(|| {
            CliError::Other(format!(
                "{reference} is not a thread of {} ({}). read-session-chat {reference} --last 1 reads it.",
                agents::text(&view["coordinator"], "title"),
                agents::text(&view["coordinator"], "globalRef")
            ))
        })?;
    let global_ref = agents::text(&session, "globalRef").to_string();
    let state = agents::text(&thread, "state").to_string();
    let account = account_of(&global_ref, &flags);
    let tool = (state == "working")
        .then(|| last_tool_call(&global_ref, &flags))
        .flatten();
    let stored = agents::text(&thread, "lastReport").to_string();
    let report = if stored.is_empty() || state == "working" {
        stored
    } else {
        full_report(&global_ref, &stored, &flags).unwrap_or(stored)
    };
    if parsed.flags.truthy("json") {
        let mut out = thread.clone();
        out["ok"] = json!(true);
        out["account"] = json!(account);
        out["lastToolCall"] = json!(tool);
        out["reportSections"] = json!(report_sections(&report));
        out["reportBytes"] = json!(report.len());
        print_json(&out);
        return Ok(());
    }
    let short = short_ref(&thread).to_string();
    let mut head = format!("{short} {} · {state}", agents::text(&thread, "title"));
    let since = match state.as_str() {
        "working" => agents::text(&thread, "workingStartedAt"),
        _ => match agents::text(&thread, "reportedAt") {
            "" => agents::text(&thread, "createdAt"),
            at => at,
        },
    };
    if let Some(at) = local_time(since) {
        head.push_str(&format!(" since {at}"));
    }
    if let Some(account) = &account {
        head.push_str(&format!(" · on {account}"));
    }
    let branch = agents::text(&thread, "branch");
    if !branch.is_empty() {
        head.push_str(&format!(" · branch {branch}"));
    }
    println!("{head}");
    if state == "working" {
        let output = age(agents::text(&thread, "lastOutputAt"))
            .map(|ago| format!("last output {ago} ago"))
            .unwrap_or_default();
        match &tool {
            Some(tool) => println!(
                "Now: {tool}{}",
                if output.is_empty() {
                    String::new()
                } else {
                    format!(" ({output})")
                }
            ),
            None if !output.is_empty() => println!("Now: {output}"),
            None => {}
        }
        if thread["stalled"] == true {
            println!(
                "{}: ghostex read-text {short} shows its screen.",
                if thread["stuck"] == true {
                    "Not moving even after Ghostex nudged it"
                } else {
                    "No new output for a while"
                }
            );
        }
    }
    let waiting_for = agents::text(&thread, "waitingFor");
    if !waiting_for.is_empty() {
        println!("Waiting for you: {}", headline(waiting_for, 240));
    }
    if report.is_empty() {
        println!("Task: {}", headline(agents::text(&thread, "task"), 200));
        println!(
            "No report yet. Its chat: ghostex read-session-chat {short} --last 1 --format text"
        );
        return Ok(());
    }
    let size = if report.len() >= 1024 {
        format!("{:.1} KB", report.len() as f64 / 1024.0)
    } else {
        format!("{} B", report.len())
    };
    println!(
        "Last report ({size}, Ghostex forwarded it to your chat): \"{}\"",
        headline(&report, 160)
    );
    let sections = report_sections(&report);
    if !sections.is_empty() {
        println!("  Sections: {}", sections.join(", "));
    }
    println!("Full text: ghostex read-session-chat {short} --last 1 --format text");
    Ok(())
}
