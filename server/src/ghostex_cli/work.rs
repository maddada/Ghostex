//! `ghostex work`: work items for any session (server/src/coordinators/session_work.rs). `work item
//! …` is the same command as `ghostex coordinator item …`, which stays its alias; in an ordinary
//! session it keeps that session's own items while the Work items extension is on. `work topic`
//! reads or sets the line at the top of the Work panel, `work next-tag` prints the next free
//! question tag.

use serde_json::json;

use super::args::parse_args;
use super::coordinator;
use super::output::print_json;
use super::rpc::{CliError, CliResult, call_gxserver_rpc};

const HELP: &str = include_str!("work_help.txt");

pub(super) fn work_command(args: &[String]) -> CliResult<()> {
    let verb = args.first().map(String::as_str).unwrap_or("help");
    if matches!(verb, "help" | "-h" | "--help")
        || args.iter().any(|arg| arg == "--help" || arg == "-h")
    {
        print!("{HELP}");
        return Ok(());
    }
    match verb {
        // The coordinator's own verbs, on whichever session the command names.
        "item" | "items" => coordinator::run(args),
        "list" => {
            let mut item_args = vec!["item".to_string(), "list".to_string()];
            item_args.extend(args.iter().skip(1).cloned());
            coordinator::run(&item_args)
        }
        "next-tag" => coordinator::run(args),
        "topic" => topic(&args[1..]),
        other => Err(CliError::Other(format!(
            "Unknown work command: {other}. See ghostex work --help."
        ))),
    }
}

/// `work topic` prints the Topic; `work topic "<one line>"` sets it; `--clear` clears it.
fn topic(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let (session, flags) = coordinator::target_session(&parsed)?;
    let words = parsed.rest.join(" ");
    let clear = parsed.flags.truthy("clear");
    if words.trim().is_empty() && !clear {
        let work = call_gxserver_rpc(
            "/api/coordinatorWork",
            &json!({ "projectId": session["projectId"], "sessionId": session["sessionId"], "op": "list" }),
            &flags,
        )?;
        if parsed.flags.truthy("json") {
            print_json(&work["topic"]);
        } else {
            match work["topic"]["text"].as_str() {
                Some(text) if !text.is_empty() => println!("{text}"),
                _ => println!("No topic yet: ghostex work topic \"<one line>\" sets it."),
            }
        }
        return Ok(());
    }
    let answer = call_gxserver_rpc(
        "/api/coordinatorWork",
        &json!({
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "op": "topic",
            "text": if clear { String::new() } else { words },
        }),
        &flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&answer);
    } else if clear {
        println!("Topic cleared.");
    } else {
        println!(
            "Topic: {}",
            answer["topic"]["text"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}
