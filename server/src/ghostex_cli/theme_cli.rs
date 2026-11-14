//! `ghostex theme`: read the look settings, change several at once as one change, undo and redo
//! it, and list the history (server/src/theme/). Run in an agent's session, `set` prints the
//! "Theme changed" card line its chat draws (packages/gx-protocol/src/chat_card.rs).

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, caller_session_from_env, CliError, CliResult};

pub(crate) const THEME_HELP: &str =
    "Ghostex Theme - restyle Ghostex: read the look settings, change them, undo

Usage:
  gx theme [read] [--json]
  gx theme set <key>=<value> [<key>=<value> ...] [--json]
  gx theme undo [<change-id>] [--json]
  gx theme redo [<change-id>] [--json]
  gx theme history [--json]

read     Every look setting on Settings > Interface, grouped, with its current
         value and what it does, and anything the current look makes hard to read.
set      Changes one or more look settings as one change, saved at once in the
         running app. Values are read as `ghostex settings set` reads them.
         Prints what changed and anything it made hard to read; the change is
         saved either way. In an agent's chat it shows a Theme changed card
         with Undo.
undo     Puts back what the newest change (or <change-id>) replaced. Refused
         when one of its settings was changed again since.
redo     Applies the newest undone change (or <change-id>) again.
history  The last 20 changes: when, who made them and what they changed.

Limits:
  Only the settings `read` lists can change. Borders, dividers and grey text
  have no setting yet. You can't see the result: make small changes and ask
  the user how it looks.

Examples:
  gx theme set darkThemePreset=blue themeSidebarContrast=-6
  gx theme set windowGlass=frosted windowGlassSidebarOpacityDark=85
  gx theme undo
";

fn theme(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/theme", &params, flags)
}

fn with_caller(mut params: Value) -> Value {
    if let Some(session) = caller_session_from_env() {
        params["callerSession"] = json!(session);
    }
    params
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn print_warnings(title: &str, warnings: &Value) {
    let warnings: Vec<&str> = warnings
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if warnings.is_empty() {
        return;
    }
    println!("{title}");
    for warning in warnings {
        println!("  - {warning}");
    }
}

fn print_items(change: &Value, undone: bool) {
    for item in change["items"].as_array().into_iter().flatten() {
        let (from, to) = if undone {
            (text(item, "afterShown"), text(item, "beforeShown"))
        } else {
            (text(item, "beforeShown"), text(item, "afterShown"))
        };
        println!(
            "  {} ({}): {from} -> {to}",
            text(item, "title"),
            text(item, "key")
        );
    }
}

fn read(flags: &Flags) -> CliResult<()> {
    let answer = theme(json!({ "op": "read" }), flags)?;
    if flags.truthy("json") {
        print_json(&answer);
        return Ok(());
    }
    for group in answer["groups"].as_array().into_iter().flatten() {
        println!("{}", text(group, "title"));
        for setting in group["settings"].as_array().into_iter().flatten() {
            let options = setting["options"]
                .as_array()
                .map(|options| {
                    options
                        .iter()
                        .map(|option| text(option, "value"))
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .filter(|options| !options.is_empty())
                .map(|options| format!("one of {options}"))
                .or_else(|| {
                    setting["optionCount"].as_u64().map(|count| {
                        format!(
                            "one of {count} values (`ghostex settings get {}`)",
                            text(setting, "key")
                        )
                    })
                })
                .unwrap_or_else(|| text(setting, "type").to_string());
            println!(
                "  {} = {}    {} ({options})",
                text(setting, "key"),
                text(setting, "shown"),
                text(setting, "title"),
            );
            let about = text(setting, "about");
            if !about.is_empty() {
                println!("      {about}");
            }
        }
        println!();
    }
    print_warnings("Hard to read now:", &answer["warnings"]);
    println!(
        "Change several at once with `ghostex theme set <key>=<value> ...`; `ghostex theme undo` puts the last change back."
    );
    Ok(())
}

fn set(rest: &[String], flags: &Flags) -> CliResult<()> {
    let mut changes = Map::new();
    for pair in rest {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(CliError::Other(format!(
                "\"{pair}\" is not <key>=<value>. Example: ghostex theme set darkThemePreset=blue"
            )));
        };
        changes.insert(key.trim().to_string(), json!(value));
    }
    if changes.is_empty() {
        return Err(CliError::Other(
            "ghostex theme set needs at least one <key>=<value>. `ghostex theme read` lists the keys."
                .to_string(),
        ));
    }
    let answer = theme(
        with_caller(json!({ "op": "set", "changes": changes })),
        flags,
    )?;
    // The card is drawn in the chat of the agent that changed the look; your own terminal has none.
    let in_session = caller_session_from_env().is_some();
    if flags.truthy("json") {
        let mut answer = answer;
        if !in_session {
            if let Some(object) = answer.as_object_mut() {
                object.remove("chatCard");
            }
        }
        print_json(&answer);
        return Ok(());
    }
    if answer["unchanged"] == true {
        println!("Nothing changed: every setting already has that value.");
        return Ok(());
    }
    let change = &answer["change"];
    println!("Theme changed ({}):", text(change, "changeId"));
    print_items(change, false);
    print_warnings(
        "Saved, but hard to read (tell the user, or change it back):",
        &change["warnings"],
    );
    if in_session && answer["chatCard"].is_object() {
        println!(
            "{}",
            ghostex_gx_protocol::chat_card::chat_card_line(&answer["chatCard"])
        );
    }
    Ok(())
}

fn undo_or_redo(verb: &str, rest: &[String], flags: &Flags) -> CliResult<()> {
    let mut params = json!({ "op": verb });
    if let Some(change) = rest.first() {
        params["change"] = json!(change);
    }
    let answer = theme(with_caller(params), flags)?;
    if flags.truthy("json") {
        print_json(&answer);
        return Ok(());
    }
    let change = &answer["change"];
    let undo = verb == "undo";
    println!(
        "{} {}:",
        if undo { "Undid" } else { "Redid" },
        text(change, "changeId")
    );
    print_items(change, undo);
    print_warnings("Hard to read now:", &answer["warnings"]);
    Ok(())
}

fn history(flags: &Flags) -> CliResult<()> {
    let answer = theme(json!({ "op": "history" }), flags)?;
    if flags.truthy("json") {
        print_json(&answer);
        return Ok(());
    }
    let changes = answer["changes"].as_array().cloned().unwrap_or_default();
    if changes.is_empty() {
        println!("No theme changes yet.");
        return Ok(());
    }
    for change in &changes {
        let state = if change["undoneAt"].is_string() {
            format!(" [undone by {}]", text(&change["undoneBy"], "name"))
        } else {
            String::new()
        };
        println!(
            "{}  {}  by {}{state}",
            text(change, "changeId"),
            text(change, "at"),
            text(&change["by"], "name"),
        );
        print_items(change, false);
    }
    Ok(())
}

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = parsed.flags.clone();
    if flags.contains("help") || flags.contains("h") {
        print!("{THEME_HELP}");
        return Ok(());
    }
    let rest = &parsed.rest;
    let verb = rest.first().map(String::as_str).unwrap_or("read");
    let tail = rest.get(1..).unwrap_or_default();
    match verb {
        "read" | "show" | "get" => read(&flags),
        "set" => set(tail, &flags),
        "undo" => undo_or_redo("undo", tail, &flags),
        "redo" => undo_or_redo("redo", tail, &flags),
        "history" | "changes" => history(&flags),
        "help" => {
            print!("{THEME_HELP}");
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown theme command: {other}\n\n{THEME_HELP}"
        ))),
    }
}
