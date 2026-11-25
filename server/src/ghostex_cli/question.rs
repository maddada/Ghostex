//! `ghostex question show Q12`: an agent reads one question whole (its words, detail, choices,
//! how it stands, the answer and where it was asked), found by its tag as the quote under a
//! message that points at it finds it (server/src/chat_item_quotes/).

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, ParsedArgs};
use crate::ghostex_cli::message::{normalized, subcommand_at};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::{notify, selector, usage};

pub fn question_command(args: &[String]) -> CliResult<()> {
    let at = subcommand_at(args);
    let subcommand = at.map_or("help", |at| args[at].as_str());
    if matches!(subcommand, "help")
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        println!("{}", usage::question_usage());
        return Ok(());
    }
    if subcommand != "show" {
        return Err(CliError::Other(format!(
            "Unknown question command: {subcommand}\n\n{}",
            usage::question_usage()
        )));
    }
    let parsed = parse_args(&normalized(args, at));
    let reference = parsed.rest.first().cloned().ok_or_else(|| {
        CliError::Other(format!(
            "show needs a question tag: Q12, or G9jt7.Q12 for another chat's.\n\n{}",
            usage::question_usage()
        ))
    })?;
    let (project_id, session_id) = session(&parsed)?;
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    params.insert("op".into(), json!("show"));
    params.insert("ref".into(), json!(reference));
    let answer = call_gxserver_rpc(
        crate::chat_item_quotes::CHAT_QUESTIONS_ENDPOINT,
        &Value::Object(params),
        &parsed.flags,
    )?;
    if parsed.flags.truthy("json") {
        print_json(&answer);
        return Ok(());
    }
    let questions = answer["questions"].as_array().cloned().unwrap_or_default();
    for (index, question) in questions.iter().enumerate() {
        if index > 0 {
            println!();
            println!("---");
            println!();
        }
        print_question(question);
    }
    let missing: Vec<&str> = answer["missing"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !missing.is_empty() {
        println!();
        println!(
            "Ghostex has nothing tagged {} in this chat.",
            missing.join(", ")
        );
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// `Q4933 (open)`, where and when it was asked, its words and detail, its choices, the answer and
/// the command it gives the user to run.
fn print_question(question: &Value) {
    println!("{} ({})", text(question, "ref"), text(question, "state"));
    let asked = text(question, "asked");
    let place = text(question, "where");
    match (asked.is_empty(), place.is_empty()) {
        (false, false) => println!("Asked {asked} in {place}"),
        (true, false) => println!("Asked in {place}"),
        (false, true) => println!("Asked {asked}"),
        (true, true) => {}
    }
    println!();
    println!("{}", text(question, "title"));
    let detail = text(question, "detail");
    if !detail.trim().is_empty() {
        println!();
        println!("{detail}");
    }
    let choices: Vec<String> = question["choices"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|choice| {
            let label = text(choice, "label").to_string();
            if choice["recommended"] == true {
                format!("{label} (recommended)")
            } else {
                label
            }
        })
        .collect();
    if !choices.is_empty() || question["answer"].is_string() || question["command"].is_string() {
        println!();
    }
    if !choices.is_empty() {
        println!("Choices: {}", choices.join(", "));
    }
    if let Some(answer) = question["answer"].as_str() {
        println!("Answered: {answer}");
    }
    if let Some(command) = question["command"].as_str() {
        println!("Command for the user: {command}");
    }
}

fn flag(parsed: &ParsedArgs, key: &str) -> Option<String> {
    parsed
        .flags
        .string_value(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session(parsed: &ParsedArgs) -> CliResult<(String, String)> {
    if let Some(selector_text) = flag(parsed, "sessionId").or_else(|| flag(parsed, "session")) {
        let session = selector::resolve_cli_session_selector(&selector_text, &parsed.flags)?;
        let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
        return match (text("projectId"), text("sessionId")) {
            (Some(project_id), Some(session_id)) => Ok((project_id, session_id)),
            _ => Err(CliError::Other(format!(
                "Session \"{selector_text}\" does not report a project id."
            ))),
        };
    }
    notify::session_reference_from_environment().ok_or_else(|| {
        CliError::Other(
            "question must run inside a Ghostex session (it reads that chat's questions), or name one with --session."
                .to_string(),
        )
    })
}
