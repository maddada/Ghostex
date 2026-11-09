//! `ghostex gbots team …` and `ghostex gbots preset …`: GBots written to one JSON team file and
//! read back with a preview first, and the presets New GBot starts from
//! (server/src/gbots/team_file/). The file's text travels in the call, so `--computer` works.

use std::io::Read as _;

use serde_json::{json, Map, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

/// The team and preset verbs' lines in `ghostex gbots --help`.
pub(crate) const TEAM_HELP: &str =
    "  ghostex gbots team export [--department <department> | --bots <a,b,…> | --preset <bot>]
                       [--notes] [--name <team name>] [--summary <text>] [--out <file>|-]
                                                 Write your GBots (or a department, or one GBot
                                                 as a preset) to a JSON file. Secrets are
                                                 redacted; chats, agents, models, approval
                                                 levels, computers and keys never go in.
                                                 --notes adds each GBot's memory as starter notes
  ghostex gbots team import <file> [--dry-run] [--agent <id>] [--json]
                                                 Show what a team file adds, then add it (asks
                                                 for your approval). Names already taken get
                                                 \" 2\"; routines arrive paused; Undo it with
                                                 `ghostex gbots undo <change>`. --stdin reads it
  ghostex gbots preset list [--json]             Presets New GBot starts from
  ghostex gbots preset show <preset> [--json]
  ghostex gbots preset import <file>             Keep a preset file for New GBot
  ghostex gbots preset save <bot> [--notes]      Keep a GBot as a preset
  ghostex gbots preset export <preset> [--out <file>|-]
  ghostex gbots preset remove <preset>
  ghostex gbots create --preset <preset> [--name <name>] [any create flag]
                                                 Add a GBot from a preset; flags you pass win
";

fn gbots(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/gbots", &params, flags)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn other(message: impl Into<String>) -> CliError {
    CliError::Other(message.into())
}

/// The file's text: the path in `rest[2]`, `--stdin`, or `--file-base64` (the phone's way).
fn read_file_arg(rest: &[String], flags: &Flags, verb: &str) -> CliResult<String> {
    if let Some(encoded) = flags.string_value("fileBase64") {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|error| other(format!("--file-base64 is not base64: {error}")))?;
        return String::from_utf8(bytes).map_err(|_| other("That file is not text."));
    }
    if flags.contains("stdin") {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| other(format!("Could not read the file from stdin: {error}")))?;
        return Ok(text);
    }
    let path = rest
        .get(2)
        .ok_or_else(|| other(format!("Name the file: ghostex gbots {verb} <file>.")))?;
    std::fs::read_to_string(path).map_err(|error| other(format!("Could not read {path}: {error}")))
}

/// Writes an export's `text` to `--out` (`-` prints it) or `./<fileName>`; answers where.
fn write_out(answer: &Value, flags: &Flags) -> CliResult<Option<String>> {
    let body = text(answer, "text");
    match flags.string_value("out") {
        Some("-") => {
            print!("{body}");
            Ok(None)
        }
        Some(path) => {
            std::fs::write(path, body)
                .map_err(|error| other(format!("Could not write {path}: {error}")))?;
            Ok(Some(path.to_string()))
        }
        None => {
            let path = text(answer, "fileName").to_string();
            if std::path::Path::new(&path).exists() {
                return Err(other(format!(
                    "{path} is already here. Pass --out <file> to write it somewhere else."
                )));
            }
            std::fs::write(&path, body)
                .map_err(|error| other(format!("Could not write {path}: {error}")))?;
            Ok(Some(path))
        }
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn list_of(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The preview in words.
fn print_preview(preview: &Value) {
    let bots = preview["bots"].as_array().cloned().unwrap_or_default();
    let kind = if text(preview, "kind") == "preset" {
        "Preset"
    } else {
        "Team"
    };
    println!("{kind}: {}", text(preview, "name"));
    if !text(preview, "summary").is_empty() {
        println!("{}", text(preview, "summary"));
    }
    println!("\nAdds {}:", plural(bots.len(), "GBot", "GBots"));
    for bot in &bots {
        let mut line = format!("  {}", text(bot, "name"));
        if !text(bot, "title").is_empty() {
            line.push_str(&format!(", {}", text(bot, "title")));
        }
        if let Some(department) = bot["department"].as_str() {
            let lead = if bot["isLead"].as_bool() == Some(true) {
                " (lead)"
            } else {
                ""
            };
            line.push_str(&format!(" [{department}{lead}]"));
        }
        if let Some(from) = bot["renamedFrom"].as_str() {
            line.push_str(&format!(" (was {from})"));
        }
        let notes = &bot["starterNotes"];
        let lines = notes["memoryLines"].as_u64().unwrap_or(0);
        let topics = notes["topics"].as_array().map(Vec::len).unwrap_or(0);
        if lines + topics as u64 > 0 {
            line.push_str(&format!(
                "; starter notes: {}, {}",
                plural(lines as usize, "line", "lines"),
                plural(topics, "topic", "topics")
            ));
        }
        println!("{line}");
    }
    let departments = preview["departments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !departments.is_empty() {
        println!("\nDepartments:");
        for department in &departments {
            let lead = department["lead"]
                .as_str()
                .map(|lead| format!(", led by {lead}"))
                .unwrap_or_default();
            println!("  {}{lead}", text(department, "name"));
        }
    }
    let routines = preview["routines"].as_array().cloned().unwrap_or_default();
    if !routines.is_empty() {
        println!("\nRoutines (arrive paused):");
        for routine in &routines {
            println!(
                "  {}: {} ({})",
                text(routine, "bot"),
                text(routine, "name"),
                text(routine, "scheduleSentence")
            );
        }
    }
    println!("\n{}", text(preview, "approvalSentence"));
    for (title, key) in [
        ("Left out of the file", "leftOut"),
        (
            "Ignored (this Ghostex doesn't know these fields)",
            "ignored",
        ),
        ("Note", "notes"),
        ("Read this", "warnings"),
        ("Can't import", "problems"),
    ] {
        let items = list_of(&preview[key]);
        if !items.is_empty() {
            println!("\n{title}:");
            for item in items {
                println!("  - {item}");
            }
        }
    }
}

/// `ghostex gbots team <verb> …`; `rest[0]` is `team`.
pub(crate) fn run_team(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("");
    match verb {
        "export" => {
            let mut params = Map::new();
            params.insert("op".into(), json!("teamExport"));
            for key in ["department", "preset", "name", "summary", "release"] {
                if let Some(value) = flags.string_value(key) {
                    params.insert(key.into(), json!(value));
                }
            }
            if let Some(bots) = flags.string_value("bots") {
                let bots: Vec<&str> = bots
                    .split(',')
                    .map(str::trim)
                    .filter(|bot| !bot.is_empty())
                    .collect();
                params.insert("bots".into(), json!(bots));
            }
            if flags.contains("notes") {
                params.insert("notes".into(), json!(true));
            }
            let answer = gbots(Value::Object(params), flags)?;
            if flags.contains("json") && flags.string_value("out").is_none() {
                print_json(&answer);
                return Ok(());
            }
            let written = write_out(&answer, flags)?;
            if let Some(path) = written {
                let what = if text(&answer, "kind") == "preset" {
                    "1 GBot as a preset".to_string()
                } else {
                    let count = answer["bots"].as_u64().unwrap_or(0) as usize;
                    let mut what = plural(count, "GBot", "GBots");
                    let departments = answer["departments"].as_u64().unwrap_or(0) as usize;
                    let routines = answer["routines"].as_u64().unwrap_or(0) as usize;
                    if departments > 0 {
                        what.push_str(&format!(
                            ", {}",
                            plural(departments, "department", "departments")
                        ));
                    }
                    if routines > 0 {
                        what.push_str(&format!(", {}", plural(routines, "routine", "routines")));
                    }
                    what
                };
                eprintln!("Exported {what} to {path}.");
                for line in list_of(&answer["leftOut"]) {
                    eprintln!("  Left out: {line}");
                }
                eprintln!("  {}", text(&answer, "neverIncluded"));
            }
            Ok(())
        }
        "import" | "preview" => {
            let file = read_file_arg(rest, flags, "team import")?;
            let mut params = json!({ "file": file });
            if let Some(agent) = flags.string_value("agent") {
                params["agentId"] = json!(agent);
            }
            let dry_run = verb == "preview" || flags.contains("dryRun");
            let mut preview_params = params.clone();
            preview_params["op"] = json!("teamPreview");
            let preview = gbots(preview_params, flags)?;
            if dry_run {
                if flags.contains("json") {
                    print_json(&preview);
                } else {
                    print_preview(&preview["preview"]);
                }
                return Ok(());
            }
            if text(&preview["preview"], "kind") == "preset" {
                return Err(other(
                    "That file is a preset. Keep it for New GBot with `ghostex gbots preset import <file>`.",
                ));
            }
            if preview["preview"]["canImport"].as_bool() != Some(true) {
                if !flags.contains("json") {
                    print_preview(&preview["preview"]);
                }
                return Err(other("That team file can't be imported; see above."));
            }
            if !flags.contains("json") {
                print_preview(&preview["preview"]);
                println!();
            }
            params["op"] = json!("teamImport");
            let answer = gbots(params, flags)?;
            if flags.contains("json") {
                print_json(&answer);
                return Ok(());
            }
            let bots: Vec<&str> = answer["bots"]
                .as_array()
                .map(|bots| bots.iter().map(|bot| text(bot, "name")).collect())
                .unwrap_or_default();
            let routines = answer["routines"].as_array().map(Vec::len).unwrap_or(0);
            println!(
                "Imported {}: {}.{}",
                plural(bots.len(), "GBot", "GBots"),
                bots.join(", "),
                if routines > 0 {
                    format!(
                        " {} arrived paused; resume them in GBot Settings.",
                        plural(routines, "routine", "routines")
                    )
                } else {
                    String::new()
                }
            );
            for line in list_of(&answer["skipped"]) {
                println!("  Not added: {line}");
            }
            println!(
                "Undo the whole import with `ghostex gbots undo {}`.",
                text(&answer, "changeId")
            );
            Ok(())
        }
        _ => Err(other(format!(
            "Use ghostex gbots team export or ghostex gbots team import <file>.\n\n{TEAM_HELP}"
        ))),
    }
}

fn preset_arg(rest: &[String], verb: &str) -> CliResult<String> {
    rest.get(2).cloned().ok_or_else(|| {
        other(format!(
            "Name the preset: ghostex gbots preset {verb} <preset>. `ghostex gbots preset list` lists them."
        ))
    })
}

fn print_preset_line(preset: &Value) {
    let mark = if preset["builtIn"].as_bool() == Some(true) {
        "built in"
    } else {
        text(preset, "source")
    };
    println!(
        "{}\t{}\t{}\t{mark}",
        text(preset, "presetId"),
        text(preset, "name"),
        text(preset, "title")
    );
}

/// `ghostex gbots preset <verb> …`; `rest[0]` is `preset`.
pub(crate) fn run_preset(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("list");
    let answer = match verb {
        "list" => gbots(json!({ "op": "presets" }), flags)?,
        "show" => gbots(json!({ "op": "preset", "preset": preset_arg(rest, verb)? }), flags)?,
        "import" => gbots(
            json!({ "op": "presetSave", "file": read_file_arg(rest, flags, "preset import")? }),
            flags,
        )?,
        "save" => {
            let bot = rest
                .get(2)
                .cloned()
                .ok_or_else(|| other("Name the GBot: ghostex gbots preset save <bot>."))?;
            gbots(
                json!({ "op": "presetSave", "bot": bot, "notes": flags.contains("notes") }),
                flags,
            )?
        }
        "export" => {
            let answer = gbots(json!({ "op": "presetExport", "preset": preset_arg(rest, verb)? }), flags)?;
            if let Some(path) = write_out(&answer, flags)? {
                eprintln!("Exported the preset {} to {path}.", text(&answer["file"], "name"));
            }
            return Ok(());
        }
        "remove" => gbots(json!({ "op": "presetRemove", "preset": preset_arg(rest, verb)? }), flags)?,
        other_verb => {
            return Err(other(format!(
                "unknown preset command `{other_verb}`: use list, show, import, save, export or remove.\n\n{TEAM_HELP}"
            )))
        }
    };
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    match verb {
        "list" => {
            for preset in answer["presets"].as_array().cloned().unwrap_or_default() {
                print_preset_line(&preset);
            }
        }
        "show" => {
            let preset = &answer["preset"];
            print_preset_line(preset);
            if !text(preset, "description").is_empty() {
                println!("{}", text(preset, "description"));
            }
            println!(
                "Look: {} in {}",
                text(preset, "icon"),
                text(preset, "color")
            );
            println!("\nInstructions:\n{}", text(preset, "instructions"));
        }
        "import" | "save" => {
            let preset = &answer["preset"];
            println!(
                "Kept the preset {} ({}). New GBot offers it; or run `ghostex gbots create --preset {} --name <name>`.",
                text(preset, "name"),
                text(preset, "presetId"),
                text(preset, "presetId")
            );
        }
        "remove" => println!("Removed the preset {}.", text(&answer["removed"], "name")),
        _ => print_json(&answer),
    }
    Ok(())
}

/// `create --preset` without `--name`: the preset's own name.
pub(crate) fn preset_name(preset: &str, flags: &Flags) -> CliResult<String> {
    let answer = gbots(json!({ "op": "preset", "preset": preset }), flags)?;
    Ok(text(&answer["preset"], "name").to_string())
}
