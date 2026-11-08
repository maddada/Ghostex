//! `ghostex gbots problem …` (Chief of Staff and you) and `ghostex gbots flag` (a GBot):
//! problems of GBot chats (server/src/gbots/problems/).

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The problem verbs' lines in `ghostex gbots --help`.
pub(crate) const PROBLEM_HELP: &str = "  ghostex gbots problem list [--all] [--json]    Problems to look at: GBot chats that could not
                                                 start, failed, stalled or wait on an answer
                                                 (--all adds closed ones)
  ghostex gbots problem retry <problem>          Restart its chat where it stopped and tell it
                                                 to carry on (a routine run that opened no chat
                                                 runs again). Twice at most; refused while the
                                                 chat works or once it moved on since the report
  ghostex gbots problem rebrief <problem> \"<text>\"
                                                 Send its chat a new message
  ghostex gbots problem close <problem> [--handed]
                                                 Done with it; --handed when you told the user
                                                 because only they can fix it
  ghostex gbots flag \"<what you need and why>\"   From a GBot's own chat: tell Chief of Staff you
                                                 are stuck on something only it or the user can
                                                 give (an answer, a sign-in, a key, a decision)
";

fn gbots(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/gbots", &params, flags)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn problem_arg(rest: &[String], verb: &str) -> CliResult<String> {
    rest.get(2).cloned().ok_or_else(|| {
        CliError::Other(format!(
            "Name the problem: ghostex gbots problem {verb} <problem>. `ghostex gbots problem list` shows them."
        ))
    })
}

/// `ghostex gbots flag "<text>"`.
pub(crate) fn run_flag(rest: &[String], flags: &Flags) -> CliResult<()> {
    let what = flags
        .string_value("text")
        .map(str::to_string)
        .or_else(|| (rest.len() > 1).then(|| rest[1..].join(" ")))
        .filter(|what| !what.trim().is_empty())
        .ok_or_else(|| {
            CliError::Other(
                "Say what you need: ghostex gbots flag \"<what you need and why>\".".to_string(),
            )
        })?;
    let answer = gbots(json!({ "op": "flag", "text": what }), flags)?;
    if flags.contains("json") {
        print_json(&answer);
    } else {
        println!("{}", text(&answer, "message"));
    }
    Ok(())
}

fn print_problem(problem: &Value) {
    let who = match (problem["botName"].as_str(), problem["routineName"].as_str()) {
        (Some(bot), Some(routine)) => format!("{bot} (routine \"{routine}\")"),
        (Some(bot), None) => bot.to_string(),
        _ => "a removed GBot".to_string(),
    };
    let mut line = format!(
        "{}\t{}\t{}\t{}",
        text(problem, "problemId"),
        text(problem, "status"),
        text(problem, "kind"),
        who
    );
    let attempts = problem["attempts"].as_i64().unwrap_or(0);
    if attempts > 0 {
        line.push_str(&format!(
            "\ttries {attempts}/{}",
            problem["maxAttempts"].as_i64().unwrap_or(2)
        ));
    }
    if problem["userOnly"].as_bool() == Some(true) {
        line.push_str("\tthe user's to fix");
    }
    println!("{line}");
    println!("    {}", text(problem, "detail"));
    if let Some(chat) = problem["chat"].as_str() {
        println!("    chat {chat}");
    }
    if let Some(closed) = problem["closedBy"].as_str() {
        println!("    {closed}");
    }
}

/// `ghostex gbots problem …`.
pub(crate) fn run_problem(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("list");
    let answer = match verb {
        "list" | "ls" => gbots(
            json!({ "op": "problems", "all": flags.contains("all") }),
            flags,
        )?,
        "retry" => gbots(
            json!({ "op": "problemRetry", "problem": problem_arg(rest, verb)? }),
            flags,
        )?,
        "rebrief" => {
            let problem = problem_arg(rest, verb)?;
            let note = flags
                .string_value("text")
                .map(str::to_string)
                .or_else(|| (rest.len() > 3).then(|| rest[3..].join(" ")))
                .filter(|note| !note.trim().is_empty())
                .ok_or_else(|| {
                    CliError::Other(format!(
                        "Say what to tell it: ghostex gbots problem rebrief {problem} \"<text>\"."
                    ))
                })?;
            gbots(
                json!({ "op": "problemRebrief", "problem": problem, "text": note }),
                flags,
            )?
        }
        "close" => gbots(
            json!({
                "op": "problemClose",
                "problem": problem_arg(rest, verb)?,
                "handed": flags.contains("handed"),
            }),
            flags,
        )?,
        other => {
            return Err(CliError::Other(format!(
                "unknown command `ghostex gbots problem {other}`.\n\n{PROBLEM_HELP}"
            )))
        }
    };
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    match verb {
        "list" | "ls" => {
            let problems = answer["problems"].as_array().cloned().unwrap_or_default();
            if problems.is_empty() {
                println!("No problems to look at.");
            } else if flags.contains("all") {
                println!("Problems, newest first:");
            } else {
                println!("Problems to look at:");
            }
            for problem in &problems {
                print_problem(problem);
            }
        }
        _ => {
            let problem = &answer["problem"];
            let id = text(problem, "problemId");
            let line = match (verb, text(problem, "status")) {
                ("retry", _) => format!(
                    "Restarted {id} ({}/{} tries). Ghostex closes it once the chat works again, and tells you if it fails again.",
                    problem["attempts"].as_i64().unwrap_or(0),
                    problem["maxAttempts"].as_i64().unwrap_or(2)
                ),
                ("rebrief", _) => format!(
                    "Sent {id}'s chat your message ({}/{} tries).",
                    problem["attempts"].as_i64().unwrap_or(0),
                    problem["maxAttempts"].as_i64().unwrap_or(2)
                ),
                (_, "handed") => format!("{id} is the user's now."),
                _ => format!("Closed {id}."),
            };
            println!("{line}");
        }
    }
    Ok(())
}
