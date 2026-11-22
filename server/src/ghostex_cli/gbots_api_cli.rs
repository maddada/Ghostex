//! `ghostex gbots api …`: a GBot's API, which lets an app or script message it with a key
//! (server/src/gbots/api/).

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The API verbs' lines in `ghostex gbots --help`.
pub(crate) const API_HELP: &str = "  ghostex gbots api [status] <bot> [--json]      Whether apps may message it, its key's label and
                                                 last use, and the command to paste
  ghostex gbots api on <bot> [--label <who uses it>] [--json]
                                                 Let apps message it: makes its key and prints it
                                                 once (asks for your approval; yours alone)
  ghostex gbots api new-key <bot> [--label <who>] A new key; the old one stops working (asks)
  ghostex gbots api off <bot>                    Stop it; its key stops working
  ghostex gbots api share-memory <bot> on|off     Let its chats for apps read its memory and
                                                 your shared preferences (off by default;
                                                 turning it on asks for your approval)
                                                 Apps on this computer (and devices that reach it
                                                 over SSH) POST {\"message\": \"…\"} to its address
                                                 with `Authorization: Bearer <key>` and get its
                                                 reply; POST …/routines/<routine>/run starts a
                                                 routine
";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// `ghostex gbots api …`.
pub(crate) fn run_api(rest: &[String], flags: &Flags) -> CliResult<()> {
    let (verb, bot) = match (rest.get(1).map(String::as_str), rest.get(2)) {
        (Some(verb @ ("status" | "on" | "new-key" | "off" | "share-memory")), Some(bot)) => {
            (verb, bot.clone())
        }
        (Some("status" | "on" | "new-key" | "off" | "share-memory") | None, _) => {
            return Err(CliError::Other(
                "Name the GBot: ghostex gbots api status|on|new-key|off <bot>.".to_string(),
            ))
        }
        (Some(bot), _) => ("status", bot.to_string()),
    };
    let op = match verb {
        "on" => "apiOn",
        "new-key" => "apiNewKey",
        "off" => "apiOff",
        "share-memory" => "apiShareMemory",
        _ => "api",
    };
    let mut params = json!({ "op": op, "bot": bot });
    // A one-time nonce for this call and its approved repeat: gxserver shows the key only to the
    // call whose nonce the approval was asked with (server/src/server/security_gates.rs).
    if matches!(op, "apiOn" | "apiNewKey") {
        params["keyNonce"] = json!(uuid::Uuid::new_v4().simple().to_string());
    }
    if op == "apiShareMemory" {
        let on = match rest.get(3).map(String::as_str) {
            Some("on") => true,
            Some("off") => false,
            _ => {
                return Err(CliError::Other(
                    "Say on or off: ghostex gbots api share-memory <bot> on|off.".to_string(),
                ))
            }
        };
        params["on"] = json!(on);
    }
    if let Some(label) = flags.string_value("label") {
        params["label"] = json!(label);
    }
    let answer = call_gxserver_rpc("/api/gbots", &params, flags)?;
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    let name = text(&answer["bot"], "name");
    let api = &answer["api"];
    if let Some(message) = answer["message"].as_str() {
        println!("{message}");
    }
    if let Some(key) = answer["key"].as_str() {
        println!("\nKey: {key}\n");
    }
    if api["on"] == true {
        if answer["key"].is_null() {
            println!("{name}'s API is on.");
        }
        let key = &api["key"];
        println!(
            "Memory: {}",
            if key["shareMemory"] == true {
                "shared with its chats for apps"
            } else {
                "not shared with apps"
            }
        );
        let label = text(key, "label");
        if !label.is_empty() {
            println!("Used by: {label}");
        }
        match key["lastUsedAt"].as_str() {
            Some(at) => println!(
                "Last used: {at} ({} calls)",
                key["uses"].as_i64().unwrap_or(0)
            ),
            None => println!("Not used yet."),
        }
        println!("Address: {}", text(api, "url"));
        println!("\n{}\n", text(api, "command"));
        println!("{}", text(api, "reach"));
    } else if answer["message"].is_null() {
        println!(
            "{name}'s API is off. Turn it on with `ghostex gbots api on {name}` (it asks for your approval)."
        );
    }
    Ok(())
}
