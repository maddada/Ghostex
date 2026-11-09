//! `ghostex gbots`: Ghostex's bots (server/src/gbots/). The work happens in gxserver
//! (`/api/gbots`), which decides who may do what.

use serde_json::{json, Map, Value};

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const HELP_HEAD: &str =
    "GBots: your own team of bots, led by Chief of Staff in Main Hub. Each GBot keeps
its name, instructions and memory in every chat; its chats run on your own Claude Code or Codex.

USAGE
  ghostex gbots list [--json]                    Your GBots
  ghostex gbots show <bot> [--json]              One GBot: instructions, folder, open chats
  ghostex gbots create --name <name> [--title <role>] [--description <text>]
                       [--soul-file <file> | --soul <text>] [--agent <id>] [--model <m>]
                       [--effort <e>] [--icon <icon>] [--color <colour>]
                       [--department <department> [--lead]] [--json]
  ghostex gbots edit <bot> [any create flag]     Change a GBot (new chats pick it up)
  ghostex gbots remove <bot>                     Close its chats, remove it, folder to the Trash
  ghostex gbots chats <bot> [--json]             Its open chats and who opened them
  ghostex gbots new-chat <bot> [--task <text>]   Open another chat with it
  ghostex gbots close-chat <chat>                Close a chat (S…:P…:G… or its session id)
  ghostex gbots brief [--bot <bot>]              What loads into a GBot (its instructions,
                                                 shared preferences, memory and team); a GBot
                                                 runs it in its own chats with no --bot
  ghostex gbots ask <bot> (--task <brief> | --body-file <file>) [--consult]
                       [--chat new|standing|<chat>] [--done-when <text>] [--json]
                                                 Hand a GBot work, or with --consult ask it a
                                                 question only. Returns at once; its report comes
                                                 back to your chat when it finishes. Work goes to
                                                 your standing chat with it, a question to a new
                                                 chat that closes once it answers
  ghostex gbots ask --rework <assignment> --task <what is missing>
                                                 Send a report back for rework (twice at most)
  ghostex gbots assignments [--all] [--json]     Work you handed out and its state
  ghostex gbots changes [--json]                 GBots added, changed and removed, newest first
  ghostex gbots undo <change>                    Undo adding or changing a GBot (refused once it
                                                 changed since)
";

/// The rest of the help, after the memory verbs (gbots_memory_cli.rs).
const HELP_TAIL: &str = "
  <bot> is a GBot's name or id. Colours: periwinkle, sky, mint, gold, coral, lavender, pink, rose.
  Icons are Ghostex icon names (robot, search, pencil, …). Only you and Chief of Staff add, change
  and remove GBots and departments; adding a GBot from a terminal asks for your approval first.
  Chief of Staff and a department's lead hand a GBot work, and a lead changes its own members'
  instructions; any other GBot or coordinator may only ask it a question. Turn on Main Hub and
  GBots in Settings > Extensions. Add --computer <name> to run a command on another computer.
";

fn help() -> String {
    format!(
        "{HELP_HEAD}{}{}{}{}{}{}{}{}{}{HELP_TAIL}",
        super::gbots_department_cli::DEPARTMENT_HELP,
        super::gbots_memory_cli::MEMORY_HELP,
        super::gbots_routine_cli::ROUTINE_HELP,
        super::gbots_day_watch_cli::DAY_WATCH_HELP,
        super::gbots_problem_cli::PROBLEM_HELP,
        super::gbots_team_cli::TEAM_HELP,
        super::gbots_api_cli::API_HELP,
        super::gbots_computer_cli::COMPUTER_HELP,
        super::gbots_voice_cli::VOICE_HELP
    )
}

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
        // CDXC:GBots 2026-11-09 DECISION: User: "pls fully hide gbots if disabled in experimental under extensions". While GBots are off here the help is one line, that they are off and where to turn them on; with --computer the other computer's own switch decides, so the whole help shows.
        let paths = crate::paths::get_gxserver_paths(None);
        if !flags.contains("computer") && !crate::gbots::gbots_on(&paths) {
            println!("{}", crate::gbots::gbots_off_message(&paths));
            return Ok(());
        }
        print!("{}", help());
        return Ok(());
    }
    let json_out = flags.contains("json");
    let verb = rest[0].as_str();
    match verb {
        "memory" => return super::gbots_memory_cli::run_memory(rest, &flags),
        "prefs" => return super::gbots_memory_cli::run_prefs(rest, &flags),
        "routine" | "routines" => return super::gbots_routine_cli::run_routine(rest, &flags),
        "problem" | "problems" => return super::gbots_problem_cli::run_problem(rest, &flags),
        "api" => return super::gbots_api_cli::run_api(rest, &flags),
        "computer" => return super::gbots_computer_cli::run_computer(rest, &flags),
        "voice" => return super::gbots_voice_cli::run_voice(rest, &flags),
        "flag" => return super::gbots_problem_cli::run_flag(rest, &flags),
        "day-watch" => return super::gbots_day_watch_cli::run_day_watch(rest, &flags),
        "department" | "departments" => {
            return super::gbots_department_cli::run_department(rest, &flags)
        }
        "team" => return super::gbots_team_cli::run_team(rest, &flags),
        "preset" | "presets" => return super::gbots_team_cli::run_preset(rest, &flags),
        _ => {}
    }
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
            // From a preset: the fields left out come from it, its name too when --name is.
            if let Some(preset) = flags.string_value("preset") {
                params.insert("preset".into(), json!(preset));
                if !params.contains_key("name") {
                    let name = super::gbots_team_cli::preset_name(preset, &flags)?;
                    params.insert("name".into(), json!(name));
                }
            }
            super::gbots_department_cli::placement_flags(&flags, &mut params)?;
            gbots(Value::Object(params), &flags)?
        }
        "edit" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("edit"));
            params.insert("bot".into(), json!(bot_arg(rest, verb)?));
            bot_fields(&flags, &mut params)?;
            super::gbots_department_cli::placement_flags(&flags, &mut params)?;
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
        "ask" => {
            let mut params = json!({ "op": "ask" });
            if let Some(rework) = flags.string_value("rework") {
                params["rework"] = json!(rework);
            } else {
                params["bot"] = json!(bot_arg(rest, verb)?);
            }
            let task =
                match flags.string_value("bodyFile") {
                    Some(path) => Some(std::fs::read_to_string(path).map_err(|error| {
                        CliError::Other(format!("Could not read {path}: {error}"))
                    })?),
                    None => flags.string_value("task").map(str::to_string),
                };
            let task = task.filter(|task| !task.trim().is_empty()).ok_or_else(|| {
                CliError::Other(
                    "Say what you want: --task \"<brief>\" or --body-file <file>.".to_string(),
                )
            })?;
            params["task"] = json!(task);
            if flags.contains("consult") {
                params["consult"] = json!(true);
            }
            if let Some(chat) = flags.string_value("chat") {
                params["chat"] = json!(chat);
            }
            if let Some(done_when) = flags.string_value("doneWhen") {
                params["doneWhen"] = json!(done_when);
            }
            gbots(params, &flags)?
        }
        "assignments" => gbots(
            json!({ "op": "assignments", "all": flags.contains("all") }),
            &flags,
        )?,
        "changes" => gbots(json!({ "op": "changes" }), &flags)?,
        "undo" => {
            let change = rest.get(1).cloned().ok_or_else(|| {
                CliError::Other(
                    "Name the change: ghostex gbots undo <change>. `ghostex gbots changes` lists them."
                        .to_string(),
                )
            })?;
            gbots(json!({ "op": "undo", "change": change }), &flags)?
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
                "unknown gbots command `{other}`.\n\n{}",
                help()
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
            let row = |bot: &Value| {
                let lead = if bot["isLead"].as_bool() == Some(true) {
                    "\tlead"
                } else {
                    ""
                };
                println!(
                    "{}\t{}\t{}\t{} open chats{lead}",
                    text(bot, "name"),
                    text(bot, "botId"),
                    text(bot, "title"),
                    bot["openChats"].as_u64().unwrap_or(0)
                );
            };
            // The GBots of no department first, then each department's, its lead first (the
            // order Main Hub draws them in).
            for bot in bots.iter().filter(|bot| bot["departmentId"].is_null()) {
                row(bot);
            }
            for department in answer["departments"]
                .as_array()
                .cloned()
                .unwrap_or_default()
            {
                let id = text(&department, "departmentId");
                let mut members: Vec<&Value> = bots
                    .iter()
                    .filter(|bot| text(bot, "departmentId") == id)
                    .collect();
                members.sort_by_key(|bot| bot["isLead"].as_bool() != Some(true));
                println!("\n{}:", text(&department, "name"));
                if members.is_empty() {
                    println!("(no GBots yet)");
                }
                for bot in members {
                    row(bot);
                }
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
                ("Department", "departmentName"),
                ("Look", "icon"),
                ("Folder", "folder"),
                ("Created by", "createdBy"),
            ] {
                let value = if key == "icon" {
                    format!("{} {}", text(bot, "icon"), text(bot, "color"))
                } else if key == "departmentName" && bot["isLead"].as_bool() == Some(true) {
                    format!("{} (its lead)", text(bot, key))
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
            // Chief of Staff's adds: the line its chat draws as the "GBot added" card.
            if answer["chatCard"].is_object() {
                println!(
                    "{}",
                    ghostex_gx_protocol::chat_card::chat_card_line(&answer["chatCard"])
                );
            }
        }
        "ask" => {
            let assignment = &answer["assignment"];
            let id = text(assignment, "assignmentId");
            let bot = text(&answer["bot"], "name");
            let chat = text(&answer["chat"], "globalRef");
            let what = match (answer["rework"].as_i64(), text(assignment, "mode")) {
                (Some(round), _) => format!("Sent {id} back to {bot} (rework {round})"),
                (None, "consult") => format!("Asked {bot} ({id})"),
                _ => format!("Handed {id} to {bot}"),
            };
            let state = if text(answer, "status") == "started" {
                "it has it"
            } else {
                "it takes it once its agent is ready"
            };
            println!("{what} in chat {chat}: {state}.");
            if let Some(note) = answer["note"].as_str() {
                println!("Note: {note}");
            }
            if answer["reportsBack"].as_bool() == Some(true) {
                println!("Its report comes back to you when it finishes; end your turn now instead of waiting.");
            } else {
                println!("Read its answer in that chat.");
            }
        }
        "assignments" => {
            let rows = answer["assignments"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if rows.is_empty() {
                println!("No assignments yet.");
            }
            for row in rows {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    text(&row, "assignmentId"),
                    text(&row, "botName"),
                    text(&row, "mode"),
                    text(&row, "status"),
                    text(&row, "brief").lines().next().unwrap_or_default()
                );
            }
        }
        "changes" => {
            let rows = answer["changes"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                println!("No changes yet.");
            }
            for row in rows {
                let state = if row["undoneAt"].is_string() {
                    "undone"
                } else if row["undoable"].as_bool() == Some(true) {
                    "can undo"
                } else {
                    ""
                };
                println!(
                    "{}\t{}\t{}\tby {}\t{}\t{state}",
                    text(&row, "changeId"),
                    text(&row, "kind"),
                    text(&row, "name"),
                    text(&row, "by"),
                    text(&row, "at"),
                );
            }
        }
        "undo" => {
            let change = &answer["change"];
            println!("Undid {} ({}).", text(change, "kind"), text(change, "name"));
            // An import's Undo says what went (team_file/import.rs).
            let done = &answer["done"];
            for (label, key) in [
                ("Removed", "removedBots"),
                ("Removed departments", "removedDepartments"),
                (
                    "Kept departments (other GBots are in them)",
                    "keptDepartments",
                ),
            ] {
                let names: Vec<&str> = done[key]
                    .as_array()
                    .map(|items| items.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if !names.is_empty() {
                    println!("  {label}: {}", names.join(", "));
                }
            }
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
