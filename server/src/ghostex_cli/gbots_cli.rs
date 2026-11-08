//! `ghostex gbots`: Ghostex's bots (server/src/gbots/). The work happens in gxserver
//! (`/api/gbots`), which decides who may do what.

use serde_json::{json, Map, Value};

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const HELP: &str =
    "GBots: your own team of bots, led by Chief of Staff in Main Hub. Each GBot keeps
its name, instructions and memory in every chat; its chats run on your own Claude Code or Codex.

USAGE
  ghostex gbots list [--json]                    Your GBots
  ghostex gbots show <bot> [--json]              One GBot: instructions, folder, open chats
  ghostex gbots create --name <name> [--title <role>] [--description <text>]
                       [--soul-file <file> | --soul <text>] [--agent <id>] [--model <m>]
                       [--effort <e>] [--icon <icon>] [--color <colour>] [--json]
  ghostex gbots edit <bot> [any create flag]     Change a GBot (new chats pick it up)
  ghostex gbots remove <bot>                     Close its chats, remove it, folder to the Trash
  ghostex gbots chats <bot> [--json]             Its open chats and who opened them
  ghostex gbots new-chat <bot> [--task <text>]   Open another chat with it
  ghostex gbots close-chat <chat>                Close a chat (S…:P…:G… or its session id)
  ghostex gbots brief [--bot <bot>]              What loads into a GBot (its instructions,
                                                 shared preferences, memory and team); a GBot
                                                 runs it in its own chats with no --bot

  <bot> is a GBot's name or id. Colours: periwinkle, sky, mint, gold, coral, lavender, pink, rose.
  Icons are Ghostex icon names (robot, search, pencil, …). Only you and Chief of Staff add, change
  and remove GBots; adding one from a terminal asks for your approval first. Turn on Main Hub and
  GBots in Settings > Extensions. Add --computer <name> to run a command on another computer.
";

fn gbots(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/gbots", &params, flags)
}

fn bot_arg(rest: &[String], verb: &str) -> CliResult<String> {
    rest.get(1).cloned().ok_or_else(|| {
        CliError::Other(format!(
            "Name the GBot: ghostex gbots {verb} <bot>. Run `ghostex gbots list` to see them."
        ))
    })
}

/// The persona fields of `create` and `edit`, from the flags.
fn bot_fields(flags: &Flags, params: &mut Map<String, Value>) -> CliResult<()> {
    for (flag, key) in [
        ("name", "name"),
        ("title", "title"),
        ("description", "description"),
        ("soul", "soul"),
        ("agent", "agentId"),
        ("model", "model"),
        ("effort", "effort"),
        ("icon", "icon"),
        ("color", "color"),
        ("colour", "color"),
        ("approval", "approval"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    if let Some(path) = flags.string_value("soulFile") {
        let soul = std::fs::read_to_string(path)
            .map_err(|error| CliError::Other(format!("Could not read {path}: {error}")))?;
        params.insert("soul".to_string(), json!(soul));
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn print_chats(chats: &[Value]) {
    if chats.is_empty() {
        println!("No open chats.");
    }
    for chat in chats {
        println!(
            "{}\t{}\t{}\topened by {}",
            text(chat, "globalRef"),
            text(chat, "state"),
            text(chat, "title"),
            text(chat, "openedBy")
        );
    }
}

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags.clone();
    let rest = &parsed.rest;
    if flags.contains("help") || flags.contains("h") || rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    let json_out = flags.contains("json");
    let verb = rest[0].as_str();
    let answer = match verb {
        "list" => gbots(json!({ "op": "list" }), &flags)?,
        "show" => gbots(json!({ "op": "show", "bot": bot_arg(rest, verb)? }), &flags)?,
        "chats" => gbots(
            json!({ "op": "chats", "bot": bot_arg(rest, verb)? }),
            &flags,
        )?,
        "brief" => {
            let mut params = json!({ "op": "brief" });
            if let Some(bot) = flags.string_value("bot") {
                params["bot"] = json!(bot);
            }
            gbots(params, &flags)?
        }
        "create" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("create"));
            bot_fields(&flags, &mut params)?;
            gbots(Value::Object(params), &flags)?
        }
        "edit" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("edit"));
            params.insert("bot".into(), json!(bot_arg(rest, verb)?));
            bot_fields(&flags, &mut params)?;
            gbots(Value::Object(params), &flags)?
        }
        "remove" => gbots(
            json!({ "op": "remove", "bot": bot_arg(rest, verb)? }),
            &flags,
        )?,
        "new-chat" => {
            let mut params = json!({ "op": "newChat", "bot": bot_arg(rest, verb)? });
            if let Some(task) = flags.string_value("task") {
                params["task"] = json!(task);
            }
            gbots(params, &flags)?
        }
        "close-chat" => {
            let chat = rest.get(1).cloned().ok_or_else(|| {
                CliError::Other(
                    "Name the chat: ghostex gbots close-chat <chat>. `ghostex gbots chats <bot>` lists them."
                        .to_string(),
                )
            })?;
            gbots(json!({ "op": "closeChat", "chat": chat }), &flags)?
        }
        other => {
            return Err(CliError::Other(format!(
                "unknown gbots command `{other}`.\n\n{HELP}"
            )))
        }
    };
    if json_out {
        print_json(&answer);
        return Ok(());
    }
    print_answer(verb, &answer);
    Ok(())
}

fn print_answer(verb: &str, answer: &Value) {
    match verb {
        "list" => {
            let bots = answer["bots"].as_array().cloned().unwrap_or_default();
            if bots.is_empty() {
                println!("No GBots yet. Add one with `ghostex gbots create --name <name>`.");
            }
            for bot in bots {
                println!(
                    "{}\t{}\t{}\t{} open chats",
                    text(&bot, "name"),
                    text(&bot, "botId"),
                    text(&bot, "title"),
                    bot["openChats"].as_u64().unwrap_or(0)
                );
            }
        }
        "show" => {
            let bot = &answer["bot"];
            println!("{} ({})", text(bot, "name"), text(bot, "botId"));
            for (label, key) in [
                ("Title", "title"),
                ("Description", "description"),
                ("Agent", "agentId"),
                ("Approval", "approval"),
                ("Look", "icon"),
                ("Folder", "folder"),
                ("Created by", "createdBy"),
            ] {
                let value = if key == "icon" {
                    format!("{} {}", text(bot, "icon"), text(bot, "color"))
                } else {
                    text(bot, key).to_string()
                };
                if !value.trim().is_empty() {
                    println!("{label}: {value}");
                }
            }
            if let Some(message) = bot["soulDriftMessage"].as_str() {
                println!("\nNote: {message}");
            }
            println!("\nInstructions:\n{}", text(bot, "soul"));
            println!("\nOpen chats:");
            print_chats(bot["chats"].as_array().map(Vec::as_slice).unwrap_or(&[]));
        }
        "chats" => {
            print_chats(answer["chats"].as_array().map(Vec::as_slice).unwrap_or(&[]));
        }
        "brief" => print!("{}", text(answer, "brief")),
        "create" => {
            let bot = &answer["bot"];
            println!(
                "Added the GBot {} ({}). Its folder: {}\nOpen a chat with it: ghostex gbots new-chat \"{}\"",
                text(bot, "name"),
                text(bot, "botId"),
                text(bot, "folder"),
                text(bot, "name")
            );
        }
        "edit" => {
            let bot = &answer["bot"];
            if answer["changed"].as_bool() == Some(true) {
                println!(
                    "Saved {}. Chats opened from now on start with it; open chats pick it up when they run `ghostex gbots brief`.",
                    text(bot, "name")
                );
            } else {
                println!("Nothing changed for {}.", text(bot, "name"));
            }
        }
        "remove" => {
            let bot = &answer["removed"];
            let folder = if answer["folderInTrash"].as_bool() == Some(true) {
                "its folder is in the Trash".to_string()
            } else {
                format!(
                    "its folder stayed where it was ({})",
                    answer["folderError"].as_str().unwrap_or("not found")
                )
            };
            println!(
                "Removed the GBot {}: closed {} chats, {folder}.",
                text(bot, "name"),
                answer["closedChats"].as_u64().unwrap_or(0)
            );
        }
        "new-chat" => {
            let chat = &answer["chat"];
            let started = if answer["providerStarted"].as_bool() == Some(true) {
                "started"
            } else {
                "created; its agent starts when the chat opens"
            };
            println!(
                "Opened a chat with {}: {} ({started}).",
                text(&answer["bot"], "name"),
                text(chat, "globalRef")
            );
        }
        "close-chat" => {
            println!("Closed {}.", text(&answer["closed"], "globalRef"));
        }
        _ => print_json(answer),
    }
}
