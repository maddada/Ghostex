//! `ghostex gbots day-watch …`: "keep an eye on my day" (server/src/gbots/day_watch/).

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The day-watch verbs' lines in `ghostex gbots --help`.
pub(crate) const DAY_WATCH_HELP: &str =
    "  ghostex gbots day-watch [status] [--json]      Whether a GBot keeps an eye on your day
  ghostex gbots day-watch on [--brief-time HH:MM] [--timezone <zone>]
                                                 Day Watcher reads (never sends or changes)
                                                 your Gmail and Google Calendar through
                                                 Connectors: a daily brief in Chief of Staff's
                                                 chat (08:00 unless you say), and a heads-up when
                                                 a meeting is about to start or important mail
                                                 comes in. Adds the GBot the first time; turns it
                                                 back on after off
  ghostex gbots day-watch off                    Pause the brief and the watch (the GBot stays)
";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn print_routines(answer: &Value) {
    for routine in answer["routines"].as_array().into_iter().flatten() {
        let state = if routine["paused"] == json!(true) || routine["enabled"] == json!(false) {
            "paused".to_string()
        } else {
            routine["nextLabel"]
                .as_str()
                .map(|next| format!("next {next}"))
                .unwrap_or_else(|| "no next run".to_string())
        };
        println!(
            "  {}: {}, {state}",
            text(routine, "name"),
            text(routine, "scheduleSentence")
        );
        if let Some(check) = routine["precheckSentence"].as_str() {
            println!("    Checks first: {check}.");
        }
    }
}

pub(crate) fn run_day_watch(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("status");
    let mut params = match verb {
        "status" => json!({ "op": "dayWatch" }),
        "on" => json!({ "op": "dayWatchOn" }),
        "off" => json!({ "op": "dayWatchOff" }),
        other => {
            return Err(CliError::Other(format!(
                "Unknown day-watch verb {other}: use status, on or off."
            )))
        }
    };
    if let Some(time) = flags.string_value("briefTime") {
        params["briefTime"] = json!(time);
    }
    if let Some(zone) = flags.string_value("timezone") {
        params["timezone"] = json!(zone);
    }
    let answer = call_gxserver_rpc("/api/gbots", &params, flags)?;
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    // The line Chief of Staff's chat draws as a "GBot added" card with Undo.
    if let Some(card) = answer.get("chatCard").filter(|card| card.is_object()) {
        println!("{}", ghostex_gx_protocol::chat_card::chat_card_line(card));
    }
    let bot = text(&answer["bot"], "name");
    match verb {
        "on" => {
            println!("{bot} keeps an eye on your day.");
            print_routines(&answer);
            let missing: Vec<&str> = answer["missing"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            if !missing.is_empty() {
                println!(
                    "  It can't read {} yet: connect it in Settings > Connectors, then run this again.",
                    missing.join(" or ")
                );
            }
            println!("  Briefs and heads-ups land in Chief of Staff's chat. Turn it off with `ghostex gbots day-watch off`.");
            if let Some(change) = answer["changeId"].as_str() {
                println!("  Undo: ghostex gbots undo {change}");
            }
        }
        "off" => {
            if answer["state"] == json!("off") {
                println!("Nothing keeps an eye on your day.");
            } else {
                println!("Paused: {bot} no longer briefs you or watches your mail and calendar.");
                println!("  Turn it back on with `ghostex gbots day-watch on`, or remove {bot} with `ghostex gbots remove \"{bot}\"`.");
            }
        }
        _ => {
            match text(&answer, "state") {
                "on" => println!("{bot} keeps an eye on your day."),
                "paused" => {
                    println!("Paused: {bot} is set up but its routines or the GBot are paused.")
                }
                _ => println!(
                    "Nothing keeps an eye on your day. `ghostex gbots day-watch on` sets it up."
                ),
            }
            print_routines(&answer);
            let connected = |key: &str| answer[key] == json!(true);
            if answer["connectorsOn"] != json!(true) {
                println!("  Connectors is off (Settings > Extensions).");
            } else {
                println!(
                    "  Gmail: {}. Google Calendar: {}.",
                    if connected("gmailConnected") {
                        "connected"
                    } else {
                        "not connected"
                    },
                    if connected("calendarConnected") {
                        "connected"
                    } else {
                        "not connected"
                    }
                );
            }
        }
    }
    Ok(())
}
