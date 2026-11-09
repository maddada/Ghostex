//! `ghostex gbots computer …`: where a GBot's chats run (server/src/gbots/computer/).

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The computer verbs' lines in `ghostex gbots --help`. `--computer` stays the flag that runs a
/// command on another computer, so the pick is `--use`.
pub(crate) const COMPUTER_HELP: &str =
    "  ghostex gbots computer <bot> [--json]          Where its chats run, the computers and Cloud
                                                 Boxes it could have, and its chats elsewhere
  ghostex gbots computer <bot> --use this|<computer>|<cloud box> [--folder <path there>]
                       [--screen on|off]         Run its new chats on a connected computer or
                                                 one of your Cloud Boxes (from Ghostex on your
                                                 computer only; this computer back from here);
                                                 --screen lets it use that computer's screen and
                                                 apps. Its folder, memory and routines stay here
";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// `ghostex gbots computer <bot> …`.
pub(crate) fn run_computer(rest: &[String], flags: &Flags) -> CliResult<()> {
    let bot = rest.get(1).cloned().ok_or_else(|| {
        CliError::Other("Name the GBot: ghostex gbots computer <bot>.".to_string())
    })?;
    let setting = flags.contains("use") || flags.contains("folder") || flags.contains("screen");
    let mut params = json!({ "op": if setting { "computerSet" } else { "computer" }, "bot": bot });
    if let Some(wanted) = flags.string_value("use") {
        params["computer"] = json!(wanted);
    }
    if let Some(folder) = flags.string_value("folder") {
        params["folder"] = json!(folder);
    }
    if let Some(screen) = flags.string_value("screen") {
        params["computerUse"] = json!(match screen {
            "on" | "yes" | "true" => true,
            "off" | "no" | "false" => false,
            _ => return Err(CliError::Other("--screen is on or off.".to_string())),
        });
    }
    let answer = call_gxserver_rpc("/api/gbots", &params, flags)?;
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    if let Some(note) = answer["note"].as_str() {
        println!("{note}");
    }
    let name = text(&answer["bot"], "name");
    let computer = &answer["computer"];
    let place = match text(computer, "kind") {
        "computer" => format!(
            "{} (folder {}{})",
            text(computer, "name"),
            text(computer, "folder"),
            if computer["available"].as_bool() == Some(true) {
                ""
            } else {
                "; not connected now"
            }
        ),
        "cloudBox" => format!("the Cloud Box {}", text(computer, "name")),
        _ => "this computer".to_string(),
    };
    println!("{name}'s chats run on {place}.");
    if computer["computerUse"].as_bool() == Some(true) {
        println!("It may use that computer's screen and apps.");
    }
    let computers = answer["choices"]["computers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let boxes = answer["choices"]["cloudBoxes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !computers.is_empty() {
        println!("Connected computers:");
        for row in &computers {
            println!(
                "  {}{}",
                text(row, "name"),
                if row["agentAccess"].as_bool() == Some(true) {
                    ""
                } else {
                    " (Agent access off)"
                }
            );
        }
    }
    if !boxes.is_empty() {
        println!("Cloud Boxes:");
        for row in &boxes {
            println!(
                "  {} ({}, {}, {})",
                text(row, "name"),
                text(row, "projectName"),
                text(row, "provider"),
                text(row, "state")
            );
        }
    }
    for chat in answer["remoteChats"].as_array().into_iter().flatten() {
        println!(
            "Chat elsewhere: {}\t{}",
            text(chat, "globalRef"),
            text(chat, "title")
        );
    }
    Ok(())
}
