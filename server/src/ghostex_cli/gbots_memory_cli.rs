//! `ghostex gbots memory …` and `ghostex gbots prefs …`: what a GBot remembers, and the
//! preferences every GBot reads. gxserver does the work (server/src/gbots/memory/) and decides who
//! may; in a GBot's own chat `--bot` is not needed.

use serde_json::{json, Map, Value};

use crate::ghostex_cli::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

pub(super) const MEMORY_HELP: &str = "  ghostex gbots memory show [--bot <bot>] [--json]
                                                 MEMORY.md, how full it is against what loads
                                                 (200 lines, 24 KB), topics, recent changes
  ghostex gbots memory add \"<note>\" [--until YYYY-MM-DD] [--topic <name> [--title <t>]]
                                                 Keep a dated note in MEMORY.md (or a topic);
                                                 the oldest move to memory/archive.md when full
  ghostex gbots memory forget \"<words of the note>\"
                                                 Move a note out of MEMORY.md to the archive
  ghostex gbots memory edit (--file <path> | --text <text>) [--topic <name>] [--base <hash>]
                                                 Replace MEMORY.md (or a topic); with --base,
                                                 refused if it changed since it was read
  ghostex gbots memory undo [<change>]           Undo the newest change to the memory (refused
                                                 when its files changed since)
  ghostex gbots memory journal [--json]          Every change: who, when, what
  ghostex gbots memory topics [--json]           Topic notes and what each is about
  ghostex gbots memory topic <name>              Print a topic note
  ghostex gbots memory log [--days <n> | --date YYYY-MM-DD]
                                                 What the GBot did each day (Ghostex writes it)
  ghostex gbots prefs show [--json]              The preferences every GBot reads
  ghostex gbots prefs set (--body-file <file> | --text <text>)
                                                 Replace them (you and Chief of Staff only)
";

fn gbots(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/gbots", &params, flags)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The words after the verb (`memory add <words…>`), as one text.
fn words(rest: &[String]) -> String {
    rest.iter().skip(2).cloned().collect::<Vec<_>>().join(" ")
}

fn read_file_flag(flags: &Flags, flag: &str) -> CliResult<Option<String>> {
    match flags.string_value(flag) {
        Some(path) => std::fs::read_to_string(path)
            .map(Some)
            .map_err(|error| CliError::Other(format!("Could not read {path}: {error}"))),
        None => Ok(None),
    }
}

fn with(op: &str, flags: &Flags, keys: &[(&str, &str)]) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("op".into(), json!(op));
    for (flag, key) in [("bot", "bot")].iter().chain(keys.iter()) {
        if let Some(value) = flags.string_value(flag) {
            params.insert((*key).to_string(), json!(value));
        }
    }
    params
}

pub(super) fn run_memory(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("show");
    let mut params = match verb {
        "show" => with("memoryShow", flags, &[("topic", "topic")]),
        "add" => {
            let mut params = with(
                "memoryAdd",
                flags,
                &[("until", "until"), ("topic", "topic"), ("title", "title")],
            );
            let note = match flags.string_value("text") {
                Some(note) => note.to_string(),
                None => words(rest),
            };
            if note.trim().is_empty() {
                return Err(CliError::Other(
                    "Say what to keep: ghostex gbots memory add \"<note>\".".into(),
                ));
            }
            params.insert("text".into(), json!(note));
            params
        }
        "forget" => {
            let mut params = with("memoryForget", flags, &[]);
            let what = words(rest);
            if what.trim().is_empty() {
                return Err(CliError::Other(
                    "Say which note: ghostex gbots memory forget \"<words of it>\".".into(),
                ));
            }
            params.insert("match".into(), json!(what));
            params
        }
        "edit" => {
            let mut params = with("memoryEdit", flags, &[("topic", "topic"), ("base", "base")]);
            let body = match read_file_flag(flags, "file")? {
                Some(body) => body,
                None => flags.string_value("text").map(str::to_string).ok_or_else(|| {
                    CliError::Other(
                        "Give the new text: --file <path> or --text <text>.".to_string(),
                    )
                })?,
            };
            params.insert("text".into(), json!(body));
            params
        }
        "undo" => {
            let mut params = with("memoryUndo", flags, &[]);
            if let Some(change) = rest.get(2) {
                params.insert("change".into(), json!(change));
            }
            params
        }
        "journal" => with("memoryJournal", flags, &[]),
        "topics" => with("memoryTopics", flags, &[]),
        "topic" => {
            let mut params = with("memoryTopic", flags, &[]);
            let name = rest.get(2).ok_or_else(|| {
                CliError::Other(
                    "Name the topic: ghostex gbots memory topic <name>. `ghostex gbots memory topics` lists them."
                        .into(),
                )
            })?;
            params.insert("topic".into(), json!(name));
            params
        }
        "log" => {
            let mut params = with("memoryLog", flags, &[("date", "date")]);
            if let Some(days) = flags.number("days") {
                params.insert("days".into(), json!(days.max(1.0) as u64));
            }
            params
        }
        other => {
            return Err(CliError::Other(format!(
                "unknown memory command `{other}`. Use show, add, forget, edit, undo, journal, topics, topic or log.\n\n{MEMORY_HELP}"
            )))
        }
    };
    params.retain(|_, value| !value.is_null());
    let answer = gbots(Value::Object(params), flags)?;
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    print_memory(verb, &answer);
    Ok(())
}

fn print_journal(rows: &[Value]) {
    if rows.is_empty() {
        println!("No changes yet.");
    }
    for row in rows {
        let state = if row["undone"].as_bool() == Some(true) {
            "undone"
        } else if row["undoable"].as_bool() == Some(true) {
            "can undo"
        } else {
            ""
        };
        println!(
            "{}\t{}\t{}\t{}\t{state}",
            text(row, "id"),
            text(row, "at"),
            text(row, "byLabel"),
            text(row, "what"),
        );
    }
}

fn print_log(days: &[Value]) {
    if days
        .iter()
        .all(|day| day["lines"].as_array().is_none_or(Vec::is_empty))
    {
        println!("Nothing logged yet.");
        return;
    }
    for day in days {
        println!("{}", text(day, "date"));
        for line in day["lines"].as_array().into_iter().flatten() {
            println!("  {}", line.as_str().unwrap_or_default());
        }
    }
}

fn print_memory(verb: &str, answer: &Value) {
    let bot = text(&answer["bot"], "name");
    match verb {
        "show" => {
            if let Some(topic) = answer.get("topic").filter(|topic| topic.is_object()) {
                print!("{}", text(topic, "text"));
                return;
            }
            let memory = &answer["memory"];
            let level = match text(memory, "level") {
                "red" => " (over: the rest does not load)",
                "amber" => " (nearly full)",
                _ => "",
            };
            println!(
                "{bot}'s memory: {} of {} lines, {:.1} of {} KB{level}",
                memory["lines"].as_u64().unwrap_or(0),
                memory["maxLines"].as_u64().unwrap_or(0),
                memory["bytes"].as_f64().unwrap_or(0.0) / 1024.0,
                memory["maxBytes"].as_u64().unwrap_or(0) / 1024,
            );
            println!(
                "Archive: {} notes · Topics: {}\n",
                answer["archive"]["notes"].as_u64().unwrap_or(0),
                answer["topics"].as_array().map(Vec::len).unwrap_or(0)
            );
            print!("{}", text(memory, "text"));
            let journal = answer["journal"].as_array().cloned().unwrap_or_default();
            if !journal.is_empty() {
                println!("\nRecent changes:");
                print_journal(&journal[..journal.len().min(5)]);
            }
        }
        "add" => {
            if answer["added"].as_bool() == Some(false) {
                println!("{}", text(answer, "message"));
                return;
            }
            println!(
                "Kept in {bot}'s {} ({}).",
                text(answer, "file"),
                text(answer, "change")
            );
            if let Some(moved) = answer["movedToArchive"].as_u64().filter(|moved| *moved > 0) {
                println!(
                    "MEMORY.md was full: {moved} older {} moved to memory/archive.md.",
                    if moved == 1 { "note" } else { "notes" }
                );
            }
            if let Some(note) = answer["redactedNote"].as_str() {
                println!("{note}");
            }
        }
        "forget" => println!(
            "Moved to the archive: {} ({}).",
            text(answer, "forgot"),
            text(answer, "change")
        ),
        "edit" => {
            if answer["changed"].as_bool() == Some(false) {
                println!("Nothing changed.");
                return;
            }
            println!("Saved {bot}'s memory ({}).", text(answer, "change"));
            if let Some(moved) = answer["movedToArchive"].as_u64().filter(|moved| *moved > 0) {
                println!(
                    "It was over what loads: {moved} older {} moved to memory/archive.md.",
                    if moved == 1 { "note" } else { "notes" }
                );
            }
            if let Some(note) = answer["redactedNote"].as_str() {
                println!("{note}");
            }
        }
        "undo" => {
            let undid = &answer["undid"];
            println!(
                "Undid {} ({}, by {}).",
                text(undid, "what"),
                text(undid, "id"),
                text(undid, "by")
            );
        }
        "journal" => print_journal(
            answer["journal"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        ),
        "topics" => {
            let topics = answer["topics"].as_array().cloned().unwrap_or_default();
            if topics.is_empty() {
                println!("No topics yet. Start one with `ghostex gbots memory add --topic <name> \"<text>\"`.");
            }
            for topic in topics {
                println!(
                    "{}\t{}\t{}",
                    text(&topic, "name"),
                    text(&topic, "title"),
                    text(&topic, "description")
                );
            }
        }
        "topic" => print!("{}", text(&answer["topic"], "text")),
        "log" => print_log(answer["log"].as_array().map(Vec::as_slice).unwrap_or(&[])),
        _ => print_json(answer),
    }
}

pub(super) fn run_prefs(rest: &[String], flags: &Flags) -> CliResult<()> {
    let verb = rest.get(1).map(String::as_str).unwrap_or("show");
    let params = match verb {
        "show" => json!({ "op": "prefsShow" }),
        "set" => {
            let body = match read_file_flag(flags, "bodyFile")? {
                Some(body) => body,
                None => flags
                    .string_value("text")
                    .map(str::to_string)
                    .ok_or_else(|| {
                        CliError::Other(
                            "Give the preferences: --body-file <file> or --text <text>."
                                .to_string(),
                        )
                    })?,
            };
            json!({ "op": "prefsSet", "text": body })
        }
        other => {
            return Err(CliError::Other(format!(
                "unknown prefs command `{other}`. Use show or set."
            )))
        }
    };
    let answer = gbots(params, flags)?;
    if flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    let preferences = &answer["preferences"];
    if verb == "set" {
        println!(
            "Saved the shared preferences ({}). Every GBot's next chat starts with them; open chats are told to read them.",
            match preferences["lines"].as_u64().unwrap_or(0) {
                1 => "1 line".to_string(),
                lines => format!("{lines} lines"),
            }
        );
        if preferences["redacted"].as_u64().unwrap_or(0) > 0 {
            println!("Things that looked like keys or passwords were replaced with «redacted».");
        }
        return Ok(());
    }
    let body = text(preferences, "text");
    if body.trim().is_empty() {
        println!(
            "No shared preferences yet. Set them with `ghostex gbots prefs set --text \"…\"`."
        );
    } else {
        print!("{body}");
    }
    Ok(())
}
