//! `ghostex gbots routine …`: a GBot's routines (server/src/gbots/routines.rs).

use serde_json::{json, Map, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The routine verbs' lines in `ghostex gbots --help`.
pub(crate) const ROUTINE_HELP: &str = "  ghostex gbots routine list [--bot <bot>] [--json]
                                                 A GBot's routines: schedule, next run, last
                                                 result and check
  ghostex gbots routine add --bot <bot> --name <name> (--prompt <text> | --prompt-file <file>)
                       --schedule-json '<json>' [--precheck-json '<json>']
                       [--deliver chat|chief|brief] [--continuity new|standing] [--json] [--paused]
                                                 Set a routine: each run is a chat of the GBot.
                                                 Schedules: {\"kind\":\"daily\",\"time\":\"09:00\"},
                                                 {\"kind\":\"weekly\",\"days\":[1,5],\"time\":\"09:00\"},
                                                 {\"kind\":\"interval\",\"everyMs\":3600000},
                                                 {\"kind\":\"cron\",\"expression\":\"0 9 1 * *\"},
                                                 {\"kind\":\"once\",\"runAt\":\"<ISO time>\"}; add
                                                 \"timezone\":\"Asia/Dubai\" to daily, weekly or cron
                                                 (this computer's zone otherwise). Runs come at
                                                 least 15 minutes apart, 24 a day at most.
                                                 A check runs first, with no model, and a run
                                                 starts only when it sees something new:
                                                 {\"kind\":\"mail\"}, {\"kind\":\"calendar\",\"withinHours\":4},
                                                 {\"kind\":\"command\",\"command\":\"ls ~/inbox\"},
                                                 {\"kind\":\"url\",\"url\":\"https://…\"},
                                                 {\"kind\":\"files\",\"path\":\"~/inbox\"},
                                                 {\"kind\":\"connector\",\"tool\":\"<read-only tool>\"};
                                                 a calendar check takes \"withinMinutes\" too, and
                                                 {\"kind\":\"any\",\"checks\":[…]} runs when any of
                                                 2 to 4 checks sees something new
  ghostex gbots routine edit <routine> [any add flag] [--no-precheck]
  ghostex gbots routine pause|resume|remove|run-now <routine> [--bot <bot>]
                                                 <routine> is its name or id. Undo a routine you
                                                 added or changed with `ghostex gbots undo`
";

fn gbots(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/gbots", &params, flags)
}

fn json_flag(flags: &Flags, flag: &str, what: &str) -> CliResult<Option<Value>> {
    flags
        .string_value(flag)
        .map(|text| {
            serde_json::from_str::<Value>(text)
                .map_err(|error| CliError::Other(format!("{what} is not valid JSON: {error}")))
        })
        .transpose()
}

fn routine_fields(flags: &Flags, params: &mut Map<String, Value>) -> CliResult<()> {
    for (flag, key) in [
        ("bot", "bot"),
        ("name", "name"),
        ("prompt", "prompt"),
        ("deliver", "deliver"),
        ("continuity", "continuity"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    if let Some(path) = flags.string_value("promptFile") {
        let prompt = std::fs::read_to_string(path)
            .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")))?;
        params.insert("prompt".to_string(), json!(prompt));
    }
    if let Some(schedule) = json_flag(flags, "scheduleJson", "--schedule-json")? {
        params.insert("schedule".to_string(), schedule);
    }
    if let Some(precheck) = json_flag(flags, "precheckJson", "--precheck-json")? {
        params.insert("precheck".to_string(), precheck);
    }
    if flags.contains("noPrecheck") {
        params.insert("precheck".to_string(), Value::Null);
    }
    if flags.contains("paused") {
        params.insert("paused".to_string(), json!(true));
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn print_routine(routine: &Value) {
    let state = if routine["paused"] == json!(true) {
        "paused".to_string()
    } else {
        match routine["nextLabel"].as_str() {
            Some(next) => format!("next {next}"),
            None => "no next run".to_string(),
        }
    };
    println!(
        "{}\t{}\t{}\t{}",
        text(routine, "name"),
        text(routine, "scheduleSentence"),
        state,
        text(routine, "id")
    );
    if let Some(check) = routine["precheckSentence"].as_str() {
        let note = routine["check"]["lastCheckNote"].as_str().unwrap_or("");
        if routine["check"]["lastCheckOutcome"] == json!("failed") {
            println!("  Checks first: {check}. Its last check failed: {note}");
        } else {
            println!("  Checks first: {check}. {note}");
        }
    }
    if routine["deliver"] == json!("chief") {
        println!("  Results that need you go to Chief of Staff too.");
    }
    if routine["deliver"] == json!("brief") {
        println!("  What it finds goes to Chief of Staff, who tells you, with a notification.");
    }
    if let Some(run) = routine["lastRun"].as_object() {
        let summary = run
            .get("summary")
            .and_then(Value::as_str)
            .and_then(|summary| summary.lines().next())
            .unwrap_or("");
        println!(
            "  Last run: {} {} {summary}",
            run.get("status").and_then(Value::as_str).unwrap_or(""),
            run.get("at").and_then(Value::as_str).unwrap_or("")
        );
    }
}

pub(crate) fn run_routine(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("list");
    let reference = || {
        rest.get(2).cloned().ok_or_else(|| {
            CliError::Other(format!(
                "Name the routine: ghostex gbots routine {verb} <routine>. `ghostex gbots routine list --bot <bot>` lists them."
            ))
        })
    };
    let answer = match verb {
        "list" => {
            let mut params = json!({ "op": "routines" });
            if let Some(bot) = flags.string_value("bot") {
                params["bot"] = json!(bot);
            }
            gbots(params, flags)?
        }
        "add" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("routineAdd"));
            routine_fields(flags, &mut params)?;
            gbots(Value::Object(params), flags)?
        }
        "edit" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("routineEdit"));
            params.insert("routine".into(), json!(reference()?));
            routine_fields(flags, &mut params)?;
            gbots(Value::Object(params), flags)?
        }
        "pause" | "resume" | "remove" | "run-now" => {
            let op = match verb {
                "pause" => "routinePause",
                "resume" => "routineResume",
                "remove" => "routineRemove",
                _ => "routineRunNow",
            };
            gbots(json!({ "op": op, "routine": reference()? }), flags)?
        }
        other => {
            return Err(CliError::Other(format!(
            "Unknown routine verb {other}: use list, add, edit, pause, resume, remove or run-now."
        )))
        }
    };
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    // The card line the chat draws as a "Routine set" card (packages/gx-protocol/src/chat_card.rs).
    if let Some(card) = answer.get("chatCard").filter(|card| card.is_object()) {
        println!("{}", ghostex_gx_protocol::chat_card::chat_card_line(card));
    }
    match verb {
        "list" => {
            let routines = answer["routines"].as_array().cloned().unwrap_or_default();
            if routines.is_empty() {
                println!("No routines.");
            }
            routines.iter().for_each(print_routine);
        }
        "remove" => println!(
            "Removed the routine {}. Its past runs stay in its GBot's chats. (change {})",
            text(&answer["removed"], "name"),
            text(&answer, "changeId")
        ),
        "run-now" => {
            print_routine(&answer["routine"]);
            println!(
                "Run: {}{}",
                text(&answer["run"], "status"),
                answer["run"]["error"]
                    .as_str()
                    .map(|error| format!(" ({error})"))
                    .unwrap_or_default()
            );
        }
        _ => {
            let routine = &answer["routine"];
            let title = match verb {
                "add" => "Routine set",
                "edit" if answer["changed"] == json!(false) => "Nothing changed",
                "edit" => "Routine changed",
                "pause" => "Routine paused",
                _ => "Routine resumed",
            };
            println!("{title}: {}", text(routine, "name"));
            print_routine(routine);
            if answer["heldForYou"] == json!(true) {
                println!(
                    "  It runs a command on its own, so it waits paused until the user turns it on (Resume on its card or in the GBot's settings)."
                );
            }
            if let Some(change) = answer["changeId"].as_str() {
                println!("  Undo: ghostex gbots undo {change}");
            }
        }
    }
    Ok(())
}
