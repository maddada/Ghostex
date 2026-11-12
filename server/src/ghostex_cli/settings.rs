use std::{
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::ghostex_cli::actions::send_gxserver_cli_action;
use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::output::{normalize_js_numbers, print_json};
use crate::ghostex_cli::rpc::{self, CliError, CliResult};
use crate::ghostex_cli::usage;

/// The agent-facing settings catalog, rendered from the Settings catalog crate
/// (`packages/settings-catalog`), the same text `cargo xtask help-generate` writes to
/// skills/ghostex-help/references/settings-catalog.json. Built in, so the installed CLI
/// validates against the catalog of its own build.
pub(super) fn settings_catalog_json() -> &'static str {
    static JSON: OnceLock<String> = OnceLock::new();
    JSON.get_or_init(ghostex_settings_catalog::help::catalog_json)
}

const SETTINGS_FILE_NAME: &str = "native-sidebar-settings.json";
const SETTINGS_WRITE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);
const SETTINGS_WRITE_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub(super) const SETTINGS_UPDATE_SOURCE: &str = "cli:settings";

/// The tabs `--tab` may name. Keep in lockstep with `SETTINGS_MODAL_TABS` in
/// packages/gx-core/src/renderer_commands/verbs.rs, which the desktop app checks again.
const SETTINGS_MODAL_TABS: &[&str] = &[
    "settings",
    "chat",
    "sidebar",
    "terminal",
    "notifications",
    "tools",
    "system",
    "theme",
    "interface",
    "integrations",
    "voiceAgent",
    "voice",
    "connectors",
    "collab",
    "workspaces",
    "cloudBoxes",
    "extensions",
    "osIntegration",
    "remote",
    "projects",
    "agents",
    "accounts",
    "secrets",
    "security",
    "actions",
    "openTargets",
    "hotkeys",
    "debugging",
    "about",
];

#[derive(Clone, Debug, Deserialize)]
pub struct CatalogOption {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    pub tab: String,
    pub tab_title: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub group_title: Option<String>,
    pub section: String,
    pub section_title: String,
    #[serde(rename = "type")]
    pub value_type: String,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub options: Option<Vec<CatalogOption>>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub advanced: Option<bool>,
    pub agent_writable: bool,
}

#[derive(Debug, Deserialize)]
struct SettingsCatalog {
    settings: Vec<CatalogEntry>,
}

fn catalog() -> CliResult<&'static SettingsCatalog> {
    static CATALOG: OnceLock<Result<SettingsCatalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str::<SettingsCatalog>(settings_catalog_json())
                .map(|mut catalog| {
                    // Option B: every setting the API writes, the CLI writes too, whatever the
                    // bundled Help still says (CDXC:Security 2026-11-10 in settings-catalog help).
                    for entry in &mut catalog.settings {
                        entry.agent_writable = ghostex_settings_catalog::help::api_may_write(
                            &entry.key,
                            entry.agent_writable,
                        );
                    }
                    catalog
                })
                .map_err(|error| format!("Bundled settings catalog is invalid: {error}"))
        })
        .as_ref()
        .map_err(|message| CliError::Other(message.clone()))
}

fn find_entry(key: &str) -> CliResult<&'static CatalogEntry> {
    let catalog = catalog()?;
    let trimmed = key.trim();
    if let Some(entry) = catalog.settings.iter().find(|entry| entry.key == trimmed) {
        return Ok(entry);
    }
    let lowered = trimmed.to_ascii_lowercase();
    let mut suggestions: Vec<&str> = catalog
        .settings
        .iter()
        .filter(|entry| {
            entry.key.to_ascii_lowercase().contains(&lowered)
                || entry.title.to_ascii_lowercase().contains(&lowered)
        })
        .map(|entry| entry.key.as_str())
        .take(8)
        .collect();
    suggestions.sort_unstable();
    let hint = if suggestions.is_empty() {
        "Run `ghostex settings list` to see every key.".to_string()
    } else {
        format!("Did you mean: {}", suggestions.join(", "))
    };
    Err(CliError::Other(format!(
        "Unknown setting \"{trimmed}\". {hint}"
    )))
}

fn settings_file_path() -> PathBuf {
    rpc::ghostex_config_home().join(SETTINGS_FILE_NAME)
}

pub(super) fn read_settings_file() -> CliResult<Map<String, Value>> {
    let path = settings_file_path();
    let read = ghostex_paths::settings_file::read(&path).map_err(|error| {
        CliError::Other(format!(
            "Could not read {}: {error}",
            path.to_string_lossy()
        ))
    })?;
    if !read.problems.is_empty() {
        for problem in &read.problems {
            eprintln!("Ghostex settings: {problem}");
        }
        crate::shared_settings_file::log_problems(
            &crate::paths::get_gxserver_paths(None),
            &read.problems,
        );
    }
    let mut object = read.object.unwrap_or_default();
    // The experimental switches in place of Enable Experimental Features (CDXC:Extensions
    // 2026-11-09 in the settings catalog), as the app and gxserver read them.
    ghostex_settings_catalog::migrate_experimental_features_switch(&mut object);
    ghostex_settings_catalog::migrate_retired_chat_looks(&mut object);
    Ok(object)
}

/// The catalog default of `entry` on this computer: a platform default (Enable transparency is
/// Never on Windows) wins over the shared one the catalog JSON carries.
fn effective_default(entry: &CatalogEntry) -> Option<Value> {
    ghostex_settings_catalog::availability::platform_default(
        ghostex_settings_catalog::Platform::current(),
        &entry.key,
    )
    .map(|value| ghostex_settings_catalog::ToJson::to_json(value).to_value())
    .or_else(|| entry.default.clone())
}

/// Whether Settings leaves the row out on this computer for the saved values (Blur on Windows
/// while the glass shows the desktop, Menu blur off macOS), or the page is one a built-in extension
/// this platform does not have owns (Cloud Boxes on Windows).
fn hidden_here(entry: &CatalogEntry, file: &Map<String, Value>) -> bool {
    !ghostex_settings_catalog::built_in_extensions::page_available_on(
        &entry.tab,
        ghostex_settings_catalog::Platform::current(),
    ) || ghostex_settings_catalog::availability::row_hidden(
        ghostex_settings_catalog::Platform::current(),
        &entry.key,
        |key| file.get(key).and_then(Value::as_str).map(str::to_string),
    )
}

fn current_value(entry: &CatalogEntry, file: &Map<String, Value>) -> (Value, &'static str) {
    match file.get(&entry.key) {
        Some(value) if !value.is_null() => (value.clone(), "file"),
        _ => (effective_default(entry).unwrap_or(Value::Null), "default"),
    }
}

fn format_value(value: &Value) -> String {
    let mut normalized = value.clone();
    normalize_js_numbers(&mut normalized);
    match &normalized {
        Value::String(text) if text.is_empty() => "(empty)".to_string(),
        Value::String(text) => text.clone(),
        Value::Null => "(unset)".to_string(),
        other => other.to_string(),
    }
}

fn option_values(entry: &CatalogEntry) -> Vec<&str> {
    entry
        .options
        .as_ref()
        .map(|options| options.iter().map(|option| option.value.as_str()).collect())
        .unwrap_or_default()
}

fn type_summary(entry: &CatalogEntry) -> String {
    match entry.value_type.as_str() {
        "boolean" => "boolean".to_string(),
        "enum" => format!("one of {}", option_values(entry).join("|")),
        "number" => {
            let values = option_values(entry);
            if !values.is_empty() {
                format!("number, one of {}", values.join("|"))
            } else if let (Some(min), Some(max)) = (entry.min, entry.max) {
                let step = entry
                    .step
                    .map(|step| format!(" step {}", format_number(step)))
                    .unwrap_or_default();
                format!(
                    "number {} to {}{step}",
                    format_number(min),
                    format_number(max)
                )
            } else {
                "number".to_string()
            }
        }
        "string" => "text".to_string(),
        "json" => "structured (Settings only)".to_string(),
        _ => "Settings UI only".to_string(),
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

fn entry_json(entry: &CatalogEntry, file: &Map<String, Value>) -> Value {
    let (current, source) = current_value(entry, file);
    let mut object = Map::new();
    object.insert("key".into(), json!(entry.key));
    object.insert("title".into(), json!(entry.title));
    object.insert("subtitle".into(), json!(entry.subtitle));
    object.insert("tab".into(), json!(entry.tab));
    object.insert("tabTitle".into(), json!(entry.tab_title));
    if let Some(group) = &entry.group {
        object.insert("group".into(), json!(group));
    }
    if let Some(group_title) = &entry.group_title {
        object.insert("groupTitle".into(), json!(group_title));
    }
    object.insert("section".into(), json!(entry.section));
    object.insert("sectionTitle".into(), json!(entry.section_title));
    object.insert("type".into(), json!(entry.value_type));
    if let Some(default) = effective_default(entry) {
        object.insert("default".into(), default);
    }
    if let Some(options) = &entry.options {
        object.insert(
            "options".into(),
            Value::Array(
                options
                    .iter()
                    .map(|option| json!({ "label": option.label, "value": option.value }))
                    .collect(),
            ),
        );
    }
    if let Some(min) = entry.min {
        object.insert("min".into(), json!(min));
    }
    if let Some(max) = entry.max {
        object.insert("max".into(), json!(max));
    }
    if let Some(step) = entry.step {
        object.insert("step".into(), json!(step));
    }
    if entry.advanced == Some(true) {
        object.insert("advanced".into(), json!(true));
    }
    object.insert("agentWritable".into(), json!(entry.agent_writable));
    object.insert("current".into(), current);
    object.insert("currentSource".into(), json!(source));
    Value::Object(object)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{head}~")
    }
}

pub fn settings_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    if subcommand == "help" || subcommand == "-h" || subcommand == "--help" {
        println!("{}", usage::settings_usage());
        return Ok(());
    }
    if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", usage::settings_usage());
        return Ok(());
    }
    match subcommand {
        "list" | "ls" => list_command(&rest),
        "get" => get_command(&rest),
        "set" => set_command(&rest),
        "reset" => reset_command(&rest),
        "open" => open_command(&rest),
        "hotkeys" | "hotkey" => super::settings_hotkeys::hotkeys_command(&rest),
        other => Err(CliError::Other(format!(
            "Unknown settings command: {other}\n\n{}",
            usage::settings_usage()
        ))),
    }
}

fn list_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let catalog = catalog()?;
    let file = read_settings_file()?;
    let writable_only = parsed.flags.truthy("writable");
    let tab_filter = parsed
        .flags
        .text("tab")
        .map(|tab| tab.trim().to_string())
        .filter(|tab| !tab.is_empty());
    if let Some(tab) = &tab_filter {
        if !SETTINGS_MODAL_TABS.contains(&tab.as_str()) {
            return Err(CliError::Other(format!(
                "Unknown Settings tab \"{tab}\". Tabs: {}",
                SETTINGS_MODAL_TABS.join(", ")
            )));
        }
    }
    let entries: Vec<&CatalogEntry> = catalog
        .settings
        .iter()
        .filter(|entry| !hidden_here(entry, &file))
        .filter(|entry| !writable_only || entry.agent_writable)
        // The Interface page's id is still `theme`.
        .filter(|entry| {
            tab_filter
                .as_deref()
                .is_none_or(|tab| entry.tab == if tab == "interface" { "theme" } else { tab })
        })
        .collect();
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "settingsFile": settings_file_path().to_string_lossy(),
            "settings": entries.iter().map(|entry| entry_json(entry, &file)).collect::<Vec<_>>(),
        }));
        return Ok(());
    }
    let mut current_tab = "";
    let mut current_group: Option<&str> = None;
    let mut current_section = "";
    for entry in entries {
        if entry.tab != current_tab {
            current_tab = entry.tab.as_str();
            current_group = None;
            current_section = "";
            println!("\n== {} (tab {}) ==", entry.tab_title, entry.tab);
        }
        let group = entry.group.as_deref();
        if group.is_some() && group != current_group {
            current_group = group;
            current_section = "";
            println!("\n  [{}]", entry.group_title.as_deref().unwrap_or_default());
        }
        if entry.section != current_section {
            current_section = entry.section.as_str();
            println!("  -- {} --", entry.section_title);
        }
        let (current, _) = current_value(entry, &file);
        let mut markers = String::new();
        if !entry.agent_writable {
            markers.push_str(" [read-only for agents]");
        }
        if entry.advanced == Some(true) {
            markers.push_str(" [advanced]");
        }
        println!(
            "  {:<44} {:<18} {:<30} {}{}",
            truncate(&entry.key, 44),
            truncate(&format_value(&current), 18),
            truncate(&type_summary(entry), 30),
            entry.title,
            markers
        );
    }
    println!();
    println!(
        "Use `ghostex settings get <key>` for details and `ghostex settings set <key> <value>` to change an agent-writable setting."
    );
    Ok(())
}

fn required_key(rest: &[String], verb: &str) -> CliResult<String> {
    rest.first()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .ok_or_else(|| {
            CliError::Other(format!(
                "ghostex settings {verb} requires a settings key.\n\n{}",
                usage::settings_usage()
            ))
        })
}

fn get_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let key = required_key(&parsed.rest, "get")?;
    let entry = find_entry(&key)?;
    let file = read_settings_file()?;
    if parsed.flags.truthy("json") {
        let mut object = entry_json(entry, &file);
        if let Some(map) = object.as_object_mut() {
            map.insert("ok".into(), json!(true));
        }
        print_json(&object);
        return Ok(());
    }
    let (current, source) = current_value(entry, &file);
    println!("{}: {}", entry.key, format_value(&current));
    println!("  title:     {}", entry.title);
    if !entry.subtitle.is_empty() {
        println!("  about:     {}", entry.subtitle);
    }
    println!("  type:      {}", type_summary(entry));
    if let Some(default) = effective_default(entry) {
        println!("  default:   {}", format_value(&default));
    }
    if hidden_here(entry, &file) {
        println!("  note:      Settings does not show this row on this computer right now.");
    }
    println!(
        "  source:    {}",
        if source == "file" {
            "saved value"
        } else {
            "default (not saved)"
        }
    );
    if let Some(options) = &entry.options {
        let labelled: Vec<String> = options
            .iter()
            .map(|option| {
                if option.label == option.value {
                    option.value.clone()
                } else {
                    format!("{} ({})", option.value, option.label)
                }
            })
            .collect();
        println!("  options:   {}", labelled.join(", "));
    }
    let location = match (&entry.group_title, &entry.section_title) {
        (Some(group), section) if group != section => {
            format!("{} > {} > {}", entry.tab_title, group, section)
        }
        (_, section) => format!("{} > {}", entry.tab_title, section),
    };
    println!("  location:  Settings > {location}");
    println!(
        "  agents:    {}",
        if entry.agent_writable {
            "may change it with `ghostex settings set`".to_string()
        } else {
            format!(
                "read-only; ask the user to change it, or run `ghostex settings open {}`",
                entry.key
            )
        }
    );
    Ok(())
}

fn not_writable_error(entry: &CatalogEntry) -> CliError {
    if entry.key == "hotkeys" {
        return CliError::Other(
            "Hotkeys are changed one at a time: `ghostex settings hotkeys list`, `ghostex settings hotkeys set <id> <keys>`, `ghostex settings hotkeys reset <id>|--all`.".to_string(),
        );
    }
    let reason = match entry.value_type.as_str() {
        "json" => "it is a structured value that only the Settings UI can edit".to_string(),
        "ui" => "it is a Settings UI action, not a stored value".to_string(),
        _ if entry.tab == "accounts" || entry.tab == "remote" => {
            "it is account or remote-pairing state".to_string()
        }
        _ => "it may hold a secret".to_string(),
    };
    CliError::Other(format!(
        "\"{}\" ({}) is not agent-writable: {reason}. Open it for the user with `ghostex settings open {}`.",
        entry.title, entry.key, entry.key
    ))
}

fn parse_boolean(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" | "enabled" | "enable" => Some(true),
        "false" | "off" | "no" | "0" | "disabled" | "disable" => Some(false),
        _ => None,
    }
}

fn number_value(number: f64) -> Value {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        json!(number as i64)
    } else {
        json!(number)
    }
}

fn parse_value(entry: &CatalogEntry, raw: &str) -> CliResult<Value> {
    match entry.value_type.as_str() {
        "boolean" => parse_boolean(raw).map(Value::Bool).ok_or_else(|| {
            CliError::Other(format!(
                "\"{}\" is a boolean; pass true or false (also on/off, yes/no).",
                entry.key
            ))
        }),
        "number" => {
            let number: f64 = raw.trim().parse().map_err(|_| {
                CliError::Other(format!(
                    "\"{}\" is a number ({}); \"{raw}\" is not numeric.",
                    entry.key,
                    type_summary(entry)
                ))
            })?;
            if !number.is_finite() {
                return Err(CliError::Other(format!(
                    "\"{}\" must be a finite number.",
                    entry.key
                )));
            }
            let allowed = option_values(entry);
            if !allowed.is_empty() {
                let matches = allowed
                    .iter()
                    .any(|value| value.parse::<f64>().ok() == Some(number));
                if !matches {
                    return Err(CliError::Other(format!(
                        "\"{}\" must be one of: {}",
                        entry.key,
                        allowed.join(", ")
                    )));
                }
            }
            if let Some(min) = entry.min {
                if number < min {
                    return Err(CliError::Other(format!(
                        "\"{}\" must be at least {}.",
                        entry.key,
                        format_number(min)
                    )));
                }
            }
            if let Some(max) = entry.max {
                if number > max {
                    return Err(CliError::Other(format!(
                        "\"{}\" must be at most {}.",
                        entry.key,
                        format_number(max)
                    )));
                }
            }
            if let (Some(step), Some(min)) = (entry.step, entry.min) {
                if step > 0.0 {
                    let steps = (number - min) / step;
                    if (steps - steps.round()).abs() > 1e-9 {
                        return Err(CliError::Other(format!(
                            "\"{}\" changes in steps of {} from {}.",
                            entry.key,
                            format_number(step),
                            format_number(min)
                        )));
                    }
                }
            }
            Ok(number_value(number))
        }
        "enum" => {
            let allowed = option_values(entry);
            if allowed.contains(&raw) {
                Ok(json!(raw))
            } else {
                let lowered = raw.to_ascii_lowercase();
                let case_hint = allowed
                    .iter()
                    .find(|value| value.to_ascii_lowercase() == lowered)
                    .map(|value| format!(" Values are case-sensitive; did you mean \"{value}\"?"))
                    .unwrap_or_default();
                Err(CliError::Other(format!(
                    "\"{}\" must be one of: {}.{case_hint}",
                    entry.key,
                    allowed.join(", ")
                )))
            }
        }
        "string" => Ok(json!(raw)),
        _ => Err(not_writable_error(entry)),
    }
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => (left - right).abs() < 1e-9,
        _ => left == right,
    }
}

fn app_not_running_error(error: CliError, entry: &CatalogEntry) -> CliError {
    let is_dependency_unavailable = match &error {
        CliError::Rpc { response, .. } => ["error", "code"].iter().any(|field| {
            response.get(*field).and_then(Value::as_str) == Some("dependencyUnavailable")
        }),
        _ => false,
    };
    if is_dependency_unavailable {
        CliError::Other(format!(
            "Ghostex desktop app is not running. Open Ghostex and retry, or change \"{}\" in Settings.",
            entry.title
        ))
    } else {
        error
    }
}

/// CDXC:Settings 2026-09-09 DECISION:
/// User: agent settings writes go through the running desktop app (renderer command -> the Settings modal's own save path), not a direct file write, so every save gets the same normalization and fan-out; when the app is not running the command fails instead of writing the file.
/// SEE-ALSO: packages/gx-core/src/renderer_commands/verbs.rs (`updateSettingsPatch`), apps/desktop/src/app/gx_store/renderer_commands/perform.rs, packages/settings-catalog/src/help/ (the catalog this validates against), server/src/headless_settings.rs (a computer without the desktop app, where gxserver saves the write itself, issue #475).
fn apply_setting(entry: &CatalogEntry, value: Value, flags: &Flags, verb: &str) -> CliResult<()> {
    if !entry.agent_writable {
        return Err(not_writable_error(entry));
    }
    let file = read_settings_file()?;
    let (previous, previous_source) = current_value(entry, &file);
    let payload = json!({
        "patch": { entry.key.clone(): value.clone() },
        "source": SETTINGS_UPDATE_SOURCE,
    });
    send_gxserver_cli_action("updateSettingsPatch", &payload, flags)
        .map_err(|error| app_not_running_error(error, entry))?;
    let started = Instant::now();
    let mut confirmed = false;
    while started.elapsed() < SETTINGS_WRITE_CONFIRM_TIMEOUT {
        let latest = read_settings_file()?;
        if latest
            .get(&entry.key)
            .is_some_and(|saved| values_equal(saved, &value))
        {
            confirmed = true;
            break;
        }
        std::thread::sleep(SETTINGS_WRITE_POLL_INTERVAL);
    }
    if !confirmed {
        return Err(CliError::Other(format!(
            "Ghostex accepted the {verb} for \"{}\" but the saved settings did not show the new value within {} seconds. Check Settings for \"{}\".",
            entry.key,
            SETTINGS_WRITE_CONFIRM_TIMEOUT.as_secs(),
            entry.title
        )));
    }
    if flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "key": entry.key,
            "title": entry.title,
            "previous": previous,
            "previousSource": previous_source,
            "value": value,
            "confirmed": true,
        }));
    } else {
        println!(
            "{}: {} -> {}",
            entry.key,
            format_value(&previous),
            format_value(&value)
        );
    }
    Ok(())
}

fn set_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let key = required_key(&parsed.rest, "set")?;
    let entry = find_entry(&key)?;
    if !entry.agent_writable {
        return Err(not_writable_error(entry));
    }
    let raw = parsed.rest.get(1).ok_or_else(|| {
        CliError::Other(format!(
            "ghostex settings set requires a value for \"{}\" ({}).",
            entry.key,
            type_summary(entry)
        ))
    })?;
    let value = parse_value(entry, raw)?;
    apply_setting(entry, value, &parsed.flags, "change")
}

fn reset_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let key = required_key(&parsed.rest, "reset")?;
    let entry = find_entry(&key)?;
    if !entry.agent_writable {
        return Err(not_writable_error(entry));
    }
    let default = effective_default(entry).ok_or_else(|| {
        CliError::Other(format!(
            "\"{}\" has no catalog default to reset to.",
            entry.key
        ))
    })?;
    apply_setting(entry, default, &parsed.flags, "reset")
}

fn open_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let key = parsed
        .rest
        .first()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    let entry = match &key {
        Some(key) => Some(find_entry(key)?),
        None => None,
    };
    let explicit_tab = parsed
        .flags
        .text("tab")
        .map(|tab| tab.trim().to_string())
        .filter(|tab| !tab.is_empty());
    if let Some(tab) = &explicit_tab {
        if !SETTINGS_MODAL_TABS.contains(&tab.as_str()) {
            return Err(CliError::Other(format!(
                "Unknown Settings tab \"{tab}\". Tabs: {}",
                SETTINGS_MODAL_TABS.join(", ")
            )));
        }
    }
    let tab = explicit_tab
        .or_else(|| entry.map(|entry| entry.tab.clone()))
        .unwrap_or_else(|| "settings".to_string());
    // A page of a built-in extension that is off (Actions, Open In) is not in Settings to open.
    if let Some(feature) = ghostex_settings_catalog::built_in_extensions::feature_owning_page(&tab)
    {
        super::built_in_extensions::require_built_in_extension(feature)?;
    }
    /*
    CDXC:Settings 2026-10-05 WHY:
    Settings searches only its own rows. A key the Help catalog adds without one (a supplemental row such as `hideAccountEmails`, or app state) was opened with its title searched, which matched nothing and showed "No settings match your search" over a page that has the setting. Those keys open on their page without a search.
    */
    let search_query = entry
        .filter(|entry| {
            ghostex_settings_catalog::has_search_row(
                ghostex_settings_catalog::Platform::current(),
                &entry.key,
            )
        })
        .map(|entry| entry.title.clone());
    let mut payload = Map::new();
    payload.insert("tab".into(), json!(tab));
    if let Some(query) = &search_query {
        payload.insert("searchQuery".into(), json!(query));
    }
    let result = send_gxserver_cli_action("openSettings", &Value::Object(payload), &parsed.flags)
        .map_err(|error| match (&error, entry) {
        (CliError::Rpc { response, .. }, _)
            if ["error", "code"].iter().any(|field| {
                response.get(*field).and_then(Value::as_str) == Some("dependencyUnavailable")
            }) =>
        {
            CliError::Other(
                "Ghostex desktop app is not running. Open Ghostex and retry.".to_string(),
            )
        }
        _ => error,
    })?;
    if parsed.flags.truthy("json") {
        print_json(&json!({
            "ok": true,
            "tab": tab,
            "searchQuery": search_query,
            "key": entry.map(|entry| entry.key.clone()),
            "result": result,
        }));
    } else {
        match (&search_query, entry) {
            (Some(query), Some(entry)) => println!(
                "Opened Settings > {} with \"{query}\" searched ({}).",
                entry.tab_title, entry.key
            ),
            (None, Some(entry)) => println!(
                "Opened Settings > {}; \"{}\" is on that page ({}).",
                entry.tab_title, entry.title, entry.key
            ),
            _ => println!("Opened Settings tab {tab}."),
        }
    }
    Ok(())
}
