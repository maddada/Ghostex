//! `ghostex settings hotkeys`: list, read and change hotkey bindings from the command line.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::ghostex_cli::actions::send_gxserver_cli_action;
use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{CliError, CliResult};
use crate::ghostex_cli::settings::{
    read_settings_file, settings_catalog_json, SETTINGS_UPDATE_SOURCE,
};
use crate::ghostex_cli::usage;

const HOTKEYS_WRITE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);
const HOTKEYS_WRITE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MODIFIER_ORDER: [&str; 4] = ["cmd", "ctrl", "alt", "shift"];
const NAMED_KEYS: &[&str] = &[
    "up",
    "down",
    "left",
    "right",
    "tab",
    "enter",
    "escape",
    "space",
    "backspace",
    "delete",
    "home",
    "end",
    "pageup",
    "pagedown",
];
const UNASSIGN_WORDS: &[&str] = &["none", "off", "unassigned", "unset", "-"];

/// One row of the hotkey catalog in settings-catalog.json, rendered from the Settings
/// catalog crate's hotkey definitions.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HotkeyDefinition {
    id: String,
    title: String,
    description: String,
    default_key: String,
    #[serde(default)]
    windows_linux_default_key: Option<String>,
    #[serde(default)]
    retired_default_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HotkeyCatalog {
    hotkeys: Vec<HotkeyDefinition>,
}

fn definitions() -> CliResult<&'static [HotkeyDefinition]> {
    static CATALOG: OnceLock<Result<HotkeyCatalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str::<HotkeyCatalog>(settings_catalog_json())
                .map_err(|error| format!("Bundled hotkey catalog is invalid: {error}"))
        })
        .as_ref()
        .map(|catalog| catalog.hotkeys.as_slice())
        .map_err(|message| CliError::Other(message.clone()))
}

fn is_mac() -> bool {
    cfg!(target_os = "macos")
}

fn platform_default(definition: &HotkeyDefinition) -> &str {
    if is_mac() {
        &definition.default_key
    } else {
        definition
            .windows_linux_default_key
            .as_deref()
            .unwrap_or(&definition.default_key)
    }
}

/// Mirrors `isReservedghostexHotkeyText`: on macOS the terminal owns Cmd+K.
fn is_reserved(hotkey: &str) -> bool {
    is_mac() && hotkey.split(' ').next() == Some("cmd+k")
}

/// Mirrors `normalizeHotkeyText` in packages/shared/hotkey-text.ts, so a chord typed here is
/// stored with the exact spelling the Settings recorder would write.
fn normalize_hotkey_text(value: &str) -> String {
    let lowered = value
        .trim()
        .to_lowercase()
        .replace('⌘', "cmd")
        .replace("command", "cmd")
        .replace('⌥', "alt")
        .replace("option", "alt")
        .replace('⌃', "ctrl")
        .replace("control", "ctrl")
        .replace('⇧', "shift");
    lowered
        .split_whitespace()
        .map(normalize_hotkey_chord)
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_hotkey_chord(chord: &str) -> String {
    let mut parts: Vec<String> = chord
        .split('+')
        .filter(|part| !part.is_empty())
        .map(|part| if part == "mod" { "cmd" } else { part }.to_string())
        .collect();
    let Some(key) = parts.last().cloned() else {
        return chord.to_string();
    };
    let has = |parts: &[String], modifier: &str| parts.iter().any(|part| part == modifier);
    let replacement = if has(&parts, "alt") && key == "ß" {
        Some("s")
    } else if has(&parts, "shift") {
        match key.as_str() {
            "!" => Some("1"),
            "@" => Some("2"),
            "#" => Some("3"),
            "$" => Some("4"),
            "%" => Some("5"),
            "^" => Some("6"),
            "&" => Some("7"),
            "*" => Some("8"),
            "(" => Some("9"),
            ")" => Some("0"),
            "{" => Some("["),
            "}" => Some("]"),
            _ => None,
        }
    } else {
        None
    };
    if let (Some(replacement), Some(last)) = (replacement, parts.last_mut()) {
        *last = replacement.to_string();
    }
    parts.join("+")
}

/// What the user typed, in the spelling the recorder writes: known aliases resolved, modifiers
/// in cmd/ctrl/alt/shift order, and on Windows and Linux Ctrl written as `cmd` (the shared
/// model's name for the primary modifier there). Rejects anything the app could not bind.
fn parse_user_hotkey(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    // GPUI-style "cmd-shift-o" is accepted too, as long as it has no "+" of its own.
    let spelled = if !trimmed.contains('+') && trimmed.matches('-').count() >= 1 {
        let parts: Vec<&str> = trimmed.split('-').collect();
        let modifiers_first = parts.len() >= 2
            && parts[..parts.len() - 1].iter().all(|part| {
                matches!(
                    part.to_lowercase().as_str(),
                    "cmd" | "command" | "ctrl" | "control" | "alt" | "option" | "shift" | "mod"
                )
            });
        if modifiers_first {
            parts.join("+")
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };
    let normalized = normalize_hotkey_text(&spelled);
    if normalized.is_empty() {
        return Err("no keys given".to_string());
    }
    normalized
        .split(' ')
        .map(canonical_chord)
        .collect::<Result<Vec<_>, _>>()
        .map(|chords| chords.join(" "))
}

fn canonical_chord(chord: &str) -> Result<String, String> {
    let tokens: Vec<&str> = chord.split('+').collect();
    if tokens.iter().any(|token| token.is_empty()) {
        return Err(format!(
            "\"{chord}\" has an empty part; write keys like cmd+shift+o (use \"=\" for the plus key)"
        ));
    }
    let (key, modifiers) = tokens
        .split_last()
        .expect("split produced at least one token");
    let key = match *key {
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        "esc" => "escape",
        "return" => "enter",
        "del" => "delete",
        other => other,
    };
    if MODIFIER_ORDER.contains(&key) {
        return Err(format!(
            "\"{chord}\" ends with a modifier; add a key after it"
        ));
    }
    let key_is_valid = key.chars().count() == 1 && !key.chars().any(char::is_whitespace)
        || NAMED_KEYS.contains(&key)
        || key
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=24).contains(&number));
    if !key_is_valid {
        return Err(format!(
            "\"{key}\" is not a key Ghostex can bind; use a letter, digit, symbol, f1-f24, or one of: {}",
            NAMED_KEYS.join(", ")
        ));
    }
    let mut present = [false; 4];
    for modifier in modifiers {
        let modifier = match *modifier {
            // On Windows and Linux the shared model spells Ctrl as `cmd`.
            "ctrl" if !is_mac() => "cmd",
            other => other,
        };
        let Some(index) = MODIFIER_ORDER.iter().position(|known| *known == modifier) else {
            return Err(format!(
                "\"{modifier}\" is not a modifier; use cmd, ctrl, alt (option) or shift"
            ));
        };
        present[index] = true;
    }
    // Mirrors `gpui_keystroke_from_shared_hotkey`: only Escape and F1-F24 bind without Cmd, Ctrl or Alt.
    let non_typing_key = key == "escape"
        || key
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=24).contains(&number));
    if !non_typing_key && !present[..3].contains(&true) {
        return Err(format!(
            "\"{chord}\" needs cmd, ctrl or alt; only escape and f1-f24 can go without"
        ));
    }
    let mut parts: Vec<&str> = MODIFIER_ORDER
        .iter()
        .zip(present)
        .filter_map(|(modifier, on)| on.then_some(*modifier))
        .collect();
    parts.push(key);
    Ok(parts.join("+"))
}

/// Mirrors `normalizeghostexHotkeySettings`: every catalog id with the chord the app binds for it,
/// in catalog order. A saved blank means "intentionally unassigned".
fn effective_hotkeys(
    definitions: &[HotkeyDefinition],
    saved: Option<&Map<String, Value>>,
) -> Vec<(String, String)> {
    definitions
        .iter()
        .map(|definition| {
            let default = platform_default(definition).to_string();
            let value = saved.and_then(|saved| {
                saved
                    .get(&definition.id)
                    .or_else(|| legacy_project_jump_hotkey(saved, &definition.id))
                    .and_then(Value::as_str)
            });
            let chord = match value {
                Some(value) => {
                    let text = if value.trim().is_empty() {
                        String::new()
                    } else {
                        normalize_hotkey_text(value)
                    };
                    let retired = definition.retired_default_keys.contains(&text);
                    let mac_default_elsewhere = !is_mac()
                        && definition.windows_linux_default_key.is_some()
                        && text == definition.default_key;
                    if retired || mac_default_elsewhere || is_reserved(&text) {
                        default
                    } else {
                        text
                    }
                }
                None => default,
            };
            (definition.id.clone(), chord)
        })
        .collect()
}

/// Jump to Project 1-5 were Focus Group 1-5; an old saved chord or blank still counts.
fn legacy_project_jump_hotkey<'a>(saved: &'a Map<String, Value>, id: &str) -> Option<&'a Value> {
    let slot = id.strip_prefix("jumpToProject")?;
    matches!(slot, "1" | "2" | "3" | "4" | "5")
        .then(|| saved.get(&format!("focusGroup{slot}")))
        .flatten()
}

fn saved_hotkeys() -> CliResult<Option<Map<String, Value>>> {
    Ok(read_settings_file()?
        .get("hotkeys")
        .and_then(Value::as_object)
        .cloned())
}

/// Whether the hotkey has a normal binding (everything but Copy, Select All and Unfocus Terminal)
/// and a Terminal focused one (CDXC:Hotkeys 2026-10-27 DECISION in
/// packages/settings-catalog/src/hotkey_definitions.rs).
fn has_normal(id: &str) -> bool {
    ghostex_settings_catalog::hotkey_has_normal_binding(id)
}

fn has_terminal(id: &str) -> bool {
    ghostex_settings_catalog::hotkey_definition(id)
        .is_some_and(ghostex_settings_catalog::hotkey_has_terminal_binding)
}

fn saved_terminal_hotkeys() -> CliResult<Option<Map<String, Value>>> {
    Ok(read_settings_file()?
        .get(ghostex_settings_catalog::TERMINAL_HOTKEYS_SETTING_KEY)
        .and_then(Value::as_object)
        .cloned())
}

/// One Terminal focused binding: the chord it runs on, and the one it falls back to while the
/// user has not recorded it (their customized normal chord, or else the Terminal focused default).
struct TerminalHotkey {
    id: String,
    chord: String,
    fallback: String,
    follows_normal: bool,
}

/// Every Terminal focused binding, resolved exactly like the desktop app and the Settings page:
/// the saved `terminalHotkeys` value wins, else the normal chord when the user changed it from its
/// default, else the Terminal focused default.
fn effective_terminal_hotkeys(
    definitions: &[HotkeyDefinition],
    saved_normal: Option<&Map<String, Value>>,
    saved_terminal: Option<&Map<String, Value>>,
) -> Vec<TerminalHotkey> {
    let platform = ghostex_settings_catalog::Platform::current();
    let normal = effective_hotkeys(definitions, saved_normal);
    definitions
        .iter()
        .zip(normal.iter())
        .filter(|(definition, _)| has_terminal(&definition.id))
        .filter_map(|(definition, (_, normal))| {
            let customized = (has_normal(&definition.id)
                && normal.as_str() != platform_default(definition))
            .then_some(normal.as_str());
            let saved = saved_terminal
                .and_then(|saved| saved.get(&definition.id))
                .and_then(Value::as_str)
                .map(normalize_hotkey_text);
            let fallback = ghostex_settings_catalog::terminal_hotkey(
                &definition.id,
                None,
                customized,
                platform,
            )?;
            let chord = ghostex_settings_catalog::terminal_hotkey(
                &definition.id,
                saved.as_deref(),
                customized,
                platform,
            )?;
            Some(TerminalHotkey {
                id: definition.id.clone(),
                follows_normal: saved.is_none() && customized.is_some(),
                chord,
                fallback,
            })
        })
        .collect()
}

fn terminal_hotkey_json(definition: &HotkeyDefinition, terminal: &TerminalHotkey) -> Value {
    json!({
        "id": definition.id,
        "title": definition.title,
        "description": definition.description,
        "section": "terminalFocused",
        "current": terminal.chord,
        "default": terminal.fallback,
        "followsNormalBinding": terminal.follows_normal,
        "customized": terminal.chord != terminal.fallback,
    })
}

fn chord_label(chord: &str) -> &str {
    if chord.is_empty() {
        "(unassigned)"
    } else {
        chord
    }
}

fn find_definition<'a>(
    definitions: &'a [HotkeyDefinition],
    query: &str,
) -> CliResult<&'a HotkeyDefinition> {
    let trimmed = query.trim();
    let lowered = trimmed.to_lowercase();
    if let Some(definition) = definitions
        .iter()
        .find(|definition| definition.id == trimmed)
        .or_else(|| {
            definitions.iter().find(|definition| {
                definition.id.to_lowercase() == lowered
                    || definition.title.to_lowercase() == lowered
            })
        })
    {
        return Ok(definition);
    }
    let suggestions: Vec<&str> = definitions
        .iter()
        .filter(|definition| {
            definition.id.to_lowercase().contains(&lowered)
                || definition.title.to_lowercase().contains(&lowered)
        })
        .map(|definition| definition.id.as_str())
        .take(8)
        .collect();
    let hint = if suggestions.is_empty() {
        "Run `ghostex settings hotkeys list` to see every id.".to_string()
    } else {
        format!("Did you mean: {}", suggestions.join(", "))
    };
    Err(CliError::Other(format!(
        "Unknown hotkey \"{trimmed}\". {hint}"
    )))
}

fn title_for<'a>(definitions: &'a [HotkeyDefinition], id: &'a str) -> &'a str {
    definitions
        .iter()
        .find(|definition| definition.id == id)
        .map(|definition| definition.title.as_str())
        .unwrap_or(id)
}

fn hotkey_json(definition: &HotkeyDefinition, current: &str) -> Value {
    let default = platform_default(definition);
    json!({
        "id": definition.id,
        "title": definition.title,
        "description": definition.description,
        "current": current,
        "default": default,
        "customized": current != default,
    })
}

/// CDXC:Hotkeys 2026-09-25 DECISION:
/// User: "Allow cli to set hotkeys pls and list etc". `ghostex settings hotkeys` lists, reads, sets, unassigns and resets bindings for agents and users; the Settings UI is no longer the only writer.
/// It sends the complete resolved map, exactly what the Hotkeys page saves, through the running app's `updateSettingsPatch` path (CDXC:Settings 2026-09-09), so every save gets the same fan-out and the new keys bind at once.
/// SEE-ALSO: packages/settings-catalog/src/hotkey_definitions.rs (the hotkey catalog, including retired keys), packages/gx-core/src/quick_access/hotkeys.rs (`normalize_hotkey_settings`) and apps/desktop/src/app/window/settings_modal/tabs/hotkeys.rs resolve a saved map the same way.
pub(super) fn hotkeys_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    match subcommand {
        "help" | "-h" | "--help" => {
            println!("{}", usage::settings_usage());
            Ok(())
        }
        "list" | "ls" => list_command(&rest),
        // `ghostex settings hotkeys --json` lists, like the bare command.
        flag if flag.starts_with("--") => list_command(args),
        "get" => get_command(&rest),
        "set" => set_command(&rest),
        "reset" => reset_command(&rest),
        other => Err(CliError::Other(format!(
            "Unknown hotkeys command: {other}\n\n{}",
            usage::settings_usage()
        ))),
    }
}

fn list_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    if parsed.flags.truthy("terminal") {
        return list_terminal_command(&parsed);
    }
    let definitions = definitions()?;
    let saved = saved_hotkeys()?;
    let effective = effective_hotkeys(definitions, saved.as_ref());
    let query = parsed.rest.join(" ").trim().to_lowercase();
    let changed_only = parsed.flags.truthy("changed");
    let rows: Vec<(&HotkeyDefinition, &str)> = definitions
        .iter()
        .zip(effective.iter())
        .map(|(definition, (_, chord))| (definition, chord.as_str()))
        .filter(|(definition, _)| has_normal(&definition.id))
        .filter(|(definition, chord)| !changed_only || *chord != platform_default(definition))
        .filter(|(definition, chord)| {
            query.is_empty()
                || definition.id.to_lowercase().contains(&query)
                || definition.title.to_lowercase().contains(&query)
                || definition.description.to_lowercase().contains(&query)
                || *chord == query
        })
        .collect();
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "platform": if is_mac() { "mac" } else { "windowsLinux" },
            "hotkeys": rows
                .iter()
                .map(|(definition, chord)| hotkey_json(definition, chord))
                .collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    for (definition, chord) in &rows {
        let default = platform_default(definition);
        let note = if *chord == default {
            String::new()
        } else {
            format!("  (default {})", chord_label(default))
        };
        println!(
            "  {:<32} {:<20} {}{note}",
            definition.id,
            chord_label(chord),
            definition.title
        );
    }
    if rows.is_empty() {
        println!("No hotkeys match.");
    }
    println!();
    if !is_mac() {
        println!("On Windows and Linux, cmd in a chord means Ctrl.");
    }
    println!("Change one with `ghostex settings hotkeys set <id> <keys>`; `none` unassigns it and `ghostex settings hotkeys reset <id>` restores its default.");
    println!("These apply outside a terminal; add --terminal for the keys that apply while a terminal has the keyboard.");
    Ok(())
}

/// `list --terminal`: the Terminal focused bindings.
fn list_terminal_command(parsed: &crate::ghostex_cli::args::ParsedArgs) -> CliResult<()> {
    let definitions = definitions()?;
    let terminal = effective_terminal_hotkeys(
        definitions,
        saved_hotkeys()?.as_ref(),
        saved_terminal_hotkeys()?.as_ref(),
    );
    let query = parsed.rest.join(" ").trim().to_lowercase();
    let changed_only = parsed.flags.truthy("changed");
    let rows: Vec<(&HotkeyDefinition, &TerminalHotkey)> = terminal
        .iter()
        .filter_map(|row| {
            let definition = definitions
                .iter()
                .find(|definition| definition.id == row.id)?;
            Some((definition, row))
        })
        .filter(|(_, row)| !changed_only || row.chord != row.fallback)
        .filter(|(definition, row)| {
            query.is_empty()
                || definition.id.to_lowercase().contains(&query)
                || definition.title.to_lowercase().contains(&query)
                || definition.description.to_lowercase().contains(&query)
                || row.chord == query
        })
        .collect();
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "platform": if is_mac() { "mac" } else { "windowsLinux" },
            "section": "terminalFocused",
            "hotkeys": rows
                .iter()
                .map(|(definition, row)| terminal_hotkey_json(definition, row))
                .collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    for (definition, row) in &rows {
        let note = if row.chord != row.fallback {
            format!("  (default {})", chord_label(&row.fallback))
        } else if row.follows_normal {
            "  (follows your normal key)".to_string()
        } else {
            String::new()
        };
        println!(
            "  {:<32} {:<20} {}{note}",
            definition.id,
            chord_label(&row.chord),
            definition.title
        );
    }
    if rows.is_empty() {
        println!("No hotkeys match.");
    }
    println!();
    if !is_mac() {
        println!("On Windows and Linux, cmd in a chord means Ctrl.");
    }
    println!("These apply while a terminal has the keyboard. Change one with `ghostex settings hotkeys set <id> <keys> --terminal`; `reset <id> --terminal` makes it follow the normal key again.");
    Ok(())
}

fn required_id(rest: &[String], verb: &str) -> CliResult<String> {
    rest.first()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            CliError::Other(format!(
                "ghostex settings hotkeys {verb} requires a hotkey id.\n\n{}",
                usage::settings_usage()
            ))
        })
}

fn get_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let definitions = definitions()?;
    let definition = find_definition(definitions, &required_id(&parsed.rest, "get")?)?;
    let saved = saved_hotkeys()?;
    let terminal = effective_terminal_hotkeys(
        definitions,
        saved.as_ref(),
        saved_terminal_hotkeys()?.as_ref(),
    );
    let terminal_row = terminal.iter().find(|row| row.id == definition.id);
    if parsed.flags.truthy("terminal") || !has_normal(&definition.id) {
        let Some(row) = terminal_row else {
            return Err(CliError::Other(format!(
                "{} has no Terminal focused binding; drop --terminal.",
                definition.id
            )));
        };
        if parsed.flags.truthy("json") {
            let mut object = terminal_hotkey_json(definition, row);
            if let Some(map) = object.as_object_mut() {
                map.insert("ok".into(), json!(true));
            }
            print_json(&object);
            return Ok(());
        }
        println!(
            "{} (Terminal focused): {}",
            definition.id,
            chord_label(&row.chord)
        );
        println!("  title:     {}", definition.title);
        println!("  about:     {}", definition.description);
        println!("  default:   {}", chord_label(&row.fallback));
        if row.follows_normal {
            println!("  follows:   the normal key you set");
        }
        return Ok(());
    }
    let effective = effective_hotkeys(definitions, saved.as_ref());
    let current = effective
        .iter()
        .find(|(id, _)| *id == definition.id)
        .map(|(_, chord)| chord.as_str())
        .unwrap_or_default();
    let shared_with: Vec<&str> = effective
        .iter()
        .filter(|(id, chord)| {
            has_normal(id) && !current.is_empty() && *id != definition.id && chord == current
        })
        .map(|(id, _)| id.as_str())
        .collect();
    if parsed.flags.truthy("json") {
        let mut object = hotkey_json(definition, current);
        if let Some(map) = object.as_object_mut() {
            map.insert("ok".into(), json!(true));
            map.insert("sharedWith".into(), json!(shared_with));
            if let Some(row) = terminal_row {
                map.insert("terminal".into(), terminal_hotkey_json(definition, row));
            }
        }
        print_json(&object);
        return Ok(());
    }
    println!("{}: {}", definition.id, chord_label(current));
    println!("  title:     {}", definition.title);
    println!("  about:     {}", definition.description);
    println!("  default:   {}", chord_label(platform_default(definition)));
    if !shared_with.is_empty() {
        println!("  also on:   {}", shared_with.join(", "));
    }
    if let Some(row) = terminal_row {
        println!(
            "  terminal:  {} (while a terminal has the keyboard)",
            chord_label(&row.chord)
        );
    }
    Ok(())
}

fn set_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let definitions = definitions()?;
    let definition = find_definition(definitions, &required_id(&parsed.rest, "set")?)?;
    let raw = parsed.rest.get(1).ok_or_else(|| {
        CliError::Other(format!(
            "ghostex settings hotkeys set requires keys for \"{}\", like cmd+shift+o, or none to unassign it.",
            definition.id
        ))
    })?;
    let chord = if UNASSIGN_WORDS.contains(&raw.trim().to_lowercase().as_str()) {
        String::new()
    } else {
        parse_user_hotkey(raw).map_err(|reason| {
            CliError::Other(format!(
                "Cannot use \"{raw}\" for {}: {reason}.",
                definition.id
            ))
        })?
    };
    if is_reserved(&chord) {
        return Err(CliError::Other(
            "Cmd+K belongs to the terminal (it clears the screen), so no hotkey can use it."
                .to_string(),
        ));
    }
    // The app's normalizer turns these back into the default on every read, so they cannot stick.
    let terminal_binding = parsed.flags.truthy("terminal") || !has_normal(&definition.id);
    let mac_default_elsewhere = !terminal_binding
        && !is_mac()
        && definition.windows_linux_default_key.is_some()
        && chord == definition.default_key;
    if definition.retired_default_keys.contains(&chord) || mac_default_elsewhere {
        return Err(CliError::Other(format!(
            "{chord} is an old default of {} that Ghostex moves back to {} on every start, so it cannot be kept; pick other keys.",
            definition.id,
            chord_label(platform_default(definition))
        )));
    }
    if parsed.flags.truthy("terminal") || !has_normal(&definition.id) {
        return set_terminal(definitions, definition, chord, &parsed.flags);
    }
    let saved = saved_hotkeys()?;
    let mut next = effective_hotkeys(definitions, saved.as_ref());
    let conflicts: Vec<String> = next
        .iter()
        .filter(|(id, existing)| {
            has_normal(id) && !chord.is_empty() && *id != definition.id && *existing == chord
        })
        .map(|(id, _)| id.clone())
        .collect();
    if !conflicts.is_empty() && !parsed.flags.truthy("replace") {
        let names: Vec<String> = conflicts
            .iter()
            .map(|id| format!("{id} ({})", title_for(definitions, id)))
            .collect();
        return Err(CliError::Other(format!(
            "{chord} is already used by {}. Add --replace to move it to {} and unassign it there, or pick other keys.",
            names.join(", "),
            definition.id
        )));
    }
    for (id, existing) in next.iter_mut() {
        if *id == definition.id {
            *existing = chord.clone();
        } else if conflicts.contains(id) {
            existing.clear();
        }
    }
    save_hotkeys(definitions, saved.as_ref(), next, &parsed.flags)
}

fn reset_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let definitions = definitions()?;
    if parsed.flags.truthy("terminal") {
        let mut saved = saved_terminal_hotkeys()?.unwrap_or_default();
        if parsed.flags.truthy("all") {
            saved.clear();
        } else {
            let definition = find_definition(definitions, &required_id(&parsed.rest, "reset")?)?;
            saved.remove(&definition.id);
        }
        return save_terminal_hotkeys(definitions, saved, &parsed.flags);
    }
    let saved = saved_hotkeys()?;
    let mut next = effective_hotkeys(definitions, saved.as_ref());
    if parsed.flags.truthy("all") {
        for ((_, chord), definition) in next.iter_mut().zip(definitions) {
            *chord = platform_default(definition).to_string();
        }
    } else {
        let definition = find_definition(definitions, &required_id(&parsed.rest, "reset")?)?;
        let default = platform_default(definition).to_string();
        if let Some((_, chord)) = next.iter_mut().find(|(id, _)| *id == definition.id) {
            *chord = default;
        }
    }
    save_hotkeys(definitions, saved.as_ref(), next, &parsed.flags)
}

/// `set <id> <keys> --terminal`: records the Terminal focused binding. Only that row's value is
/// stored, as the Hotkeys page does, so every other Terminal focused binding keeps following its
/// normal key; keys equal to what the row would follow anyway are dropped instead of pinned.
fn set_terminal(
    definitions: &[HotkeyDefinition],
    definition: &HotkeyDefinition,
    chord: String,
    flags: &Flags,
) -> CliResult<()> {
    if !has_terminal(&definition.id) {
        return Err(CliError::Other(format!(
            "{} has no Terminal focused binding; drop --terminal.",
            definition.id
        )));
    }
    let saved_normal = saved_hotkeys()?;
    let mut saved = saved_terminal_hotkeys()?.unwrap_or_default();
    let terminal = effective_terminal_hotkeys(definitions, saved_normal.as_ref(), Some(&saved));
    // Conflicts are checked within the Terminal focused section only: the same keys may be set
    // for one action there and another outside a terminal.
    let conflicts: Vec<&str> = terminal
        .iter()
        .filter(|row| !chord.is_empty() && row.id != definition.id && row.chord == chord)
        .map(|row| row.id.as_str())
        .collect();
    if !conflicts.is_empty() && !flags.truthy("replace") {
        let names: Vec<String> = conflicts
            .iter()
            .map(|id| format!("{id} ({})", title_for(definitions, id)))
            .collect();
        return Err(CliError::Other(format!(
            "{chord} is already used in a terminal by {}. Add --replace to move it to {} and unassign it there, or pick other keys.",
            names.join(", "),
            definition.id
        )));
    }
    for id in conflicts {
        saved.insert(id.to_string(), Value::String(String::new()));
    }
    let fallback = terminal
        .iter()
        .find(|row| row.id == definition.id)
        .map(|row| row.fallback.as_str());
    if fallback == Some(chord.as_str()) {
        saved.remove(&definition.id);
    } else {
        saved.insert(definition.id.clone(), Value::String(chord));
    }
    save_terminal_hotkeys(definitions, saved, flags)
}

/// Sends `terminalHotkeys`, then waits for the app to write it.
fn save_terminal_hotkeys(
    definitions: &[HotkeyDefinition],
    next: Map<String, Value>,
    flags: &Flags,
) -> CliResult<()> {
    let saved_normal = saved_hotkeys()?;
    let before = effective_terminal_hotkeys(
        definitions,
        saved_normal.as_ref(),
        saved_terminal_hotkeys()?.as_ref(),
    );
    let after = effective_terminal_hotkeys(definitions, saved_normal.as_ref(), Some(&next));
    let changes: Vec<(&str, &str, &str)> = before
        .iter()
        .zip(after.iter())
        .filter(|(before, after)| before.chord != after.chord)
        .map(|(before, after)| {
            (
                before.id.as_str(),
                before.chord.as_str(),
                after.chord.as_str(),
            )
        })
        .collect();
    let payload = json!({
        "patch": { ghostex_settings_catalog::TERMINAL_HOTKEYS_SETTING_KEY: Value::Object(next.clone()) },
        "source": SETTINGS_UPDATE_SOURCE,
    });
    send_gxserver_cli_action("updateSettingsPatch", &payload, flags)
        .map_err(app_not_running_error)?;
    let started = Instant::now();
    let mut confirmed = false;
    while started.elapsed() < HOTKEYS_WRITE_CONFIRM_TIMEOUT {
        if saved_terminal_hotkeys()?.unwrap_or_default() == next {
            confirmed = true;
            break;
        }
        std::thread::sleep(HOTKEYS_WRITE_POLL_INTERVAL);
    }
    if !confirmed {
        return Err(CliError::Other(format!(
            "Ghostex accepted the hotkey change but the saved settings did not show it within {} seconds. Check Settings > Hotkeys > Terminal focused.",
            HOTKEYS_WRITE_CONFIRM_TIMEOUT.as_secs()
        )));
    }
    if flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "section": "terminalFocused",
            "changes": changes
                .iter()
                .map(|(id, before, after)| json!({ "id": id, "before": before, "after": after }))
                .collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    if changes.is_empty() {
        println!("No Terminal focused hotkey changed.");
    }
    for (id, before, after) in &changes {
        println!(
            "{id} (Terminal focused, {}): {} -> {}",
            title_for(definitions, id),
            chord_label(before),
            chord_label(after)
        );
    }
    Ok(())
}

fn app_not_running_error(error: CliError) -> CliError {
    let is_dependency_unavailable = match &error {
        CliError::Rpc { response, .. } => ["error", "code"].iter().any(|field| {
            response.get(*field).and_then(Value::as_str) == Some("dependencyUnavailable")
        }),
        _ => false,
    };
    if is_dependency_unavailable {
        CliError::Other(
            "Ghostex desktop app is not running. Open Ghostex and retry, or change the hotkey in Settings > Hotkeys."
                .to_string(),
        )
    } else {
        error
    }
}

/// Sends the whole map, as the Hotkeys page does, then waits for the app to write it.
fn save_hotkeys(
    definitions: &[HotkeyDefinition],
    saved: Option<&Map<String, Value>>,
    next: Vec<(String, String)>,
    flags: &Flags,
) -> CliResult<()> {
    let previous = effective_hotkeys(definitions, saved);
    let changes: Vec<(&str, &str, &str)> = previous
        .iter()
        .zip(next.iter())
        .filter(|((_, before), (_, after))| before != after)
        .map(|((id, before), (_, after))| (id.as_str(), before.as_str(), after.as_str()))
        .collect();
    if !changes.is_empty() {
        let map: Map<String, Value> = next
            .iter()
            .map(|(id, chord)| (id.clone(), Value::String(chord.clone())))
            .collect();
        let payload = json!({
            "patch": { "hotkeys": Value::Object(map) },
            "source": SETTINGS_UPDATE_SOURCE,
        });
        send_gxserver_cli_action("updateSettingsPatch", &payload, flags)
            .map_err(app_not_running_error)?;
        let started = Instant::now();
        let mut confirmed = false;
        while started.elapsed() < HOTKEYS_WRITE_CONFIRM_TIMEOUT {
            if effective_hotkeys(definitions, saved_hotkeys()?.as_ref()) == next {
                confirmed = true;
                break;
            }
            std::thread::sleep(HOTKEYS_WRITE_POLL_INTERVAL);
        }
        if !confirmed {
            return Err(CliError::Other(format!(
                "Ghostex accepted the hotkey change but the saved settings did not show it within {} seconds. Check Settings > Hotkeys.",
                HOTKEYS_WRITE_CONFIRM_TIMEOUT.as_secs()
            )));
        }
    }
    if flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "confirmed": true,
            "changes": changes
                .iter()
                .map(|(id, before, after)| json!({ "id": id, "previous": before, "value": after }))
                .collect::<Vec<_>>(),
        }));
    } else if changes.is_empty() {
        println!("No change: the hotkeys already had those keys.");
    } else {
        for (id, before, after) in &changes {
            println!("{id}: {} -> {}", chord_label(before), chord_label(after));
        }
    }
    Ok(())
}
