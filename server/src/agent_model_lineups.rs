//! The model lineups gxserver reads from the Antigravity, Codex, Cursor and Grok CLIs themselves, which
//! take those agents' place in the catalog it serves (`agent_model_catalog.rs`).
//!
//! CDXC:AgentProviders 2026-10-17 WHY:
//! Each CLI prints its own lineup in its own shape. `agy models` prints `<id>\t<label>` with the
//! effort flattened into both (`gemini-4-argon-medium`, `Gemini 4 Argon (Medium)`); a model
//! listed once is a whole id with no effort slider (`gpt-oss-120b-medium`). `codex debug models`
//! prints its built-in catalog as JSON, where `visibility: "hide"` marks internal models its
//! `/model` never lists. `cursor-agent --list-models` prints `<id> - <label>` once per effort,
//! Fast and Thinking variant, with the label naming the effort unless it is the model's default
//! ("Claude Opus 5.5 1M" is `claude-opus-5-5-medium`), and needs a login. Grok's is below
//! `grok_lineup`. Every lineup is keyed by
//! the model part and carries the picker row text Ghostex types, so screen detection and the
//! picker drivers read it exactly as they read the published file.
//! SEE-ALSO: packages/gx-chat-core/src/menus/catalog.rs (the `legacyRule` these lineups set).

use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::{json, Value};

/// The agents whose lineup comes from their CLI.
pub(crate) const RUNTIME_AGENTS: [&str; 5] = ["antigravity", "codex", "cursor", "grok", "opencode"];
const READ_TIMEOUT: Duration = Duration::from_secs(45);
/// Every effort the three CLIs name, lowest first.
const EFFORT_RANK: [&str; 8] = [
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

/// CDXC:AgentProviders 2026-10-17 DECISION:
/// User: GPT-5.6 Terra stays out of Codex's list ("Nobody is going to use that one"), and Cursor's Opus 5 and Sonnet 5 stay out now that Opus 5.5 and Sonnet 5.5 replace them, even though the CLIs still list all three.
const EXCLUDED: [(&str, &str); 3] = [
    ("codex", "gpt-5.6-terra"),
    ("cursor", "claude-opus-5"),
    ("cursor", "claude-sonnet-5"),
];

fn excluded(agent: &str, value: &str) -> bool {
    EXCLUDED.contains(&(agent, value))
}

fn rank(effort: &str) -> usize {
    EFFORT_RANK
        .iter()
        .position(|known| *known == effort)
        .unwrap_or(EFFORT_RANK.len())
}

fn ranked(mut efforts: Vec<String>) -> Vec<String> {
    efforts.sort_by_key(|effort| rank(effort));
    efforts.dedup();
    efforts
}

/// Asks the agent's CLI for its lineup, as a catalog agent entry; `None` while the CLI is missing,
/// logged out, or prints no model.
pub(crate) fn read_lineup(agent: &str) -> Option<Value> {
    if agent == "opencode" {
        return opencode_lineup();
    }
    let (command, args): (&str, &[&str]) = match agent {
        "antigravity" => ("agy", &["models"]),
        "codex" => ("codex", &["debug", "models"]),
        "cursor" => ("cursor-agent", &["--list-models"]),
        "grok" => ("grok", &["models"]),
        _ => return None,
    };
    let home = crate::resume_lookup::home_dir();
    let program = crate::agent_hooks::probing::resolve_probe_command(command, &home)?;
    let process = lineup_command(agent, program, args, &home);
    let output = run_to_end(process)?;
    let entry = match agent {
        "antigravity" => antigravity_lineup(&output),
        "codex" => codex_lineup(&output),
        "grok" => grok_lineup(&output, &home),
        _ => cursor_lineup(&output),
    }?;
    entry
        .get("models")
        .and_then(Value::as_array)
        .is_some_and(|models| !models.is_empty())
        .then_some(entry)
}

/// The lineup read's command. `agy` also gets the switch that keeps it from spawning its updater
/// (see `AGY_BACKGROUND_AUTO_UPDATE_OFF`).
fn lineup_command(
    agent: &str,
    program: impl AsRef<std::ffi::OsStr>,
    args: &[&str],
    home: &std::path::Path,
) -> Command {
    let mut process = Command::new(program);
    process
        .args(args)
        .current_dir(home)
        .env(
            "PATH",
            crate::agent_hooks::probing::normalize_gxserver_process_path(
                std::env::var("PATH").ok().as_deref(),
                home,
            ),
        )
        .env("NO_COLOR", "1");
    if agent == "antigravity" {
        let (key, value) = crate::agent_hooks::probing::AGY_BACKGROUND_AUTO_UPDATE_OFF;
        process.env(key, value);
    }
    process
}

/// CDXC:AgentProviders 2026-11-02 WHY:
/// The model pill's OpenCode tab lists what OpenCode itself lists (issue #390), which depends on the providers and keys set up on that computer (about 400 models with OpenRouter). OpenCode 2's `/api/model` answers it; its background service is asked when it is running, so nothing starts. Otherwise `opencode api --standalone` answers from a private server that exits with the command, so reading the lineup never leaves an OpenCode service running, and no OpenCode 2 installed means no read at all. Both read the home folder's configuration; a thread running OpenCode replaces it with its own folder's lineup (`session_chat_opencode::model_catalog`).
fn opencode_lineup() -> Option<Value> {
    let home = crate::resume_lookup::home_dir();
    let response = opencode_service_models(&home).or_else(|| {
        let program = crate::agent_cli::opencode::opencode2_program(&home)?;
        let mut process = Command::new(program);
        process
            .args(["api", "--standalone", "GET", "/api/model"])
            .current_dir(&home)
            .env(
                "PATH",
                crate::agent_hooks::probing::normalize_gxserver_process_path(
                    std::env::var("PATH").ok().as_deref(),
                    &home,
                ),
            )
            // The Ghostex plugin only reports for a terminal Ghostex started.
            .env_remove("GHOSTEX_SESSION_ID")
            .env("NO_COLOR", "1");
        serde_json::from_str(&run_to_end(process)?).ok()
    })?;
    crate::session_chat_opencode::lineup_entry(&response).filter(|entry| {
        entry["models"]
            .as_array()
            .is_some_and(|models| !models.is_empty())
    })
}

fn opencode_service_models(home: &std::path::Path) -> Option<Value> {
    let client = crate::session_chat_opencode::Client::discover().ok()?;
    let encoded: String =
        url::form_urlencoded::byte_serialize(home.to_string_lossy().as_bytes()).collect();
    client
        .request("GET", &format!("/api/model?directory={encoded}"), None)
        .ok()
}

/// Runs a command to its end and returns its stdout. Codex prints about half a megabyte, so the
/// output is drained while the process runs rather than after it exits.
fn run_to_end(mut command: Command) -> Option<String> {
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < READ_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = reader.join().ok()?;
    status.success().then_some(text)
}

fn agent_entry(name: &str, fast_mode: Value, models: Vec<Value>) -> Value {
    let efforts = ranked(
        models
            .iter()
            .filter_map(|model| model.get("efforts").and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
    );
    json!({
        "name": name,
        "efforts": efforts,
        "fastMode": fast_mode,
        "legacyRule": "newestPerFamily",
        "models": models,
    })
}

/// `agy models`. Antigravity's `/model` picker preselects Medium on every effort slider.
fn antigravity_lineup(output: &str) -> Option<Value> {
    // (model id, label without the effort, [(effort, whole id, whole label)])
    let mut models: Vec<(String, String, Vec<(Option<&str>, String, String)>)> = Vec::new();
    for line in output.lines() {
        let Some((id, label)) = line.split_once('\t') else {
            continue;
        };
        let (id, label) = (id.trim(), label.trim());
        if id.is_empty() || label.is_empty() {
            continue;
        }
        let split = ["low", "medium", "high"].into_iter().find_map(|effort| {
            let base = id.strip_suffix(&format!("-{effort}"))?;
            let title = format!("{}{}", effort[..1].to_uppercase(), &effort[1..]);
            let name = label.strip_suffix(&format!(" ({title})"))?;
            Some((base, name, effort))
        });
        let (base, name, effort) = split.map_or((id, label, None), |(base, name, effort)| {
            (base, name, Some(effort))
        });
        let variant = (effort, id.to_string(), label.to_string());
        match models.iter_mut().find(|(model, _, _)| *model == base) {
            Some((_, _, variants)) => variants.push(variant),
            None => models.push((base.to_string(), name.to_string(), vec![variant])),
        }
    }
    let rows = models
        .into_iter()
        .map(|(base, name, variants)| match variants.as_slice() {
            [(effort, id, label)] => {
                let mut row = json!({
                    "value": id,
                    "label": name,
                    "pickerLabel": label,
                    "efforts": [],
                    "fastMode": false,
                });
                if let Some(effort) = effort {
                    row["description"] = json!(format!(
                        "{}{} reasoning",
                        effort[..1].to_uppercase(),
                        &effort[1..]
                    ));
                }
                row
            }
            _ => {
                let efforts = ranked(
                    variants
                        .iter()
                        .filter_map(|(effort, _, _)| effort.map(str::to_string))
                        .collect(),
                );
                let mut row = json!({
                    "value": base,
                    "label": name,
                    "pickerLabel": name,
                    "efforts": efforts,
                    "fastMode": false,
                });
                if efforts.iter().any(|effort| effort == "medium") {
                    row["defaultEffort"] = json!("medium");
                }
                row
            }
        })
        .collect();
    Some(agent_entry(
        "Antigravity CLI",
        json!({ "available": false }),
        rows,
    ))
}

/// `codex debug models`: the models its `/model` lists, in its own order.
fn codex_lineup(output: &str) -> Option<Value> {
    let document: Value = serde_json::from_str(output.get(output.find('{')?..)?).ok()?;
    let mut listed: Vec<&Value> = document
        .get("models")?
        .as_array()?
        .iter()
        .filter(|model| model.get("visibility").and_then(Value::as_str) == Some("list"))
        .collect();
    listed.sort_by_key(|model| {
        model
            .get("priority")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX)
    });
    let mut rows: Vec<Value> = listed
        .into_iter()
        .filter_map(|model| {
            let value = model.get("slug")?.as_str()?;
            if excluded("codex", value) {
                return None;
            }
            let efforts: Vec<String> = model
                .get("supported_reasoning_levels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|level| level.get("effort").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
            let fast = model
                .get("additional_speed_tiers")
                .and_then(Value::as_array)
                .is_some_and(|tiers| tiers.iter().any(|tier| tier.as_str() == Some("fast")));
            let mut row = json!({
                "value": value,
                "label": model
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or(value)
                    .replace('-', " "),
                "efforts": efforts,
                "fastMode": fast,
            });
            if let Some(description) = model.get("description").and_then(Value::as_str) {
                row["description"] = json!(description);
            }
            if let Some(effort) = model
                .get("default_reasoning_level")
                .and_then(Value::as_str)
                .filter(|effort| efforts.iter().any(|known| known == effort))
            {
                row["defaultEffort"] = json!(effort);
            }
            Some(row)
        })
        .collect();
    // Codex starts on the first model its picker lists.
    if let Some(first) = rows.first_mut() {
        first["default"] = json!(true);
    }
    Some(agent_entry(
        "Codex CLI",
        json!({ "available": true, "command": "/fast", "scope": "default" }),
        rows,
    ))
}

/// CDXC:AgentProviders 2026-10-17 WHY:
/// `grok models` (1.0.62) prints only the ids its `/model` offers, in its order, and marks the default (`* grok-4.6 (default)`). The names, descriptions and reasoning levels of those ids are in `~/.grok/models_cache.json`, which that command refreshes from the server first; only each model's `info` is read from it (the file also holds credentials). An id the file does not describe is named from the id and offered Grok's four levels.
fn grok_lineup(output: &str, home: &std::path::Path) -> Option<Value> {
    let cache: Value = std::fs::read_to_string(home.join(".grok/models_cache.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    let info = |id: &str| -> Option<&Value> {
        let models = cache.get("models")?;
        models
            .get(id)
            .or_else(|| {
                models
                    .as_object()?
                    .values()
                    .find(|model| model.pointer("/info/id").and_then(Value::as_str) == Some(id))
            })?
            .get("info")
    };
    let rows = output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))?;
            let (id, default) = match rest.strip_suffix("(default)") {
                Some(id) => (id.trim(), true),
                None => (rest.trim(), false),
            };
            if id.is_empty() || id.contains(char::is_whitespace) {
                return None;
            }
            let info = info(id);
            if info
                .and_then(|info| info.get("hidden"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                return None;
            }
            let label = info
                .and_then(|info| info.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    let (base, fast) = id
                        .strip_suffix("-build-fast")
                        .map_or((id, ""), |base| (base, " Fast"));
                    let mut name = base.replace('-', " ");
                    if let Some(first) = name.get_mut(..1) {
                        first.make_ascii_uppercase();
                    }
                    format!("{name}{fast}")
                });
            let levels = info
                .and_then(|info| info.get("reasoning_efforts"))
                .and_then(Value::as_array);
            let efforts = match levels {
                Some(levels) => ranked(
                    levels
                        .iter()
                        .filter_map(|level| level.get("value").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect(),
                ),
                None => ["low", "medium", "high", "xhigh"]
                    .map(str::to_string)
                    .to_vec(),
            };
            let default_effort = levels
                .into_iter()
                .flatten()
                .find(|level| level.get("default").and_then(Value::as_bool) == Some(true))
                .and_then(|level| level.get("value").and_then(Value::as_str))
                .or_else(|| {
                    info.and_then(|info| info.get("reasoning_effort"))
                        .and_then(Value::as_str)
                })
                .filter(|effort| efforts.iter().any(|known| known == effort));
            let mut row = json!({
                "value": id,
                "label": label,
                "pickerLabel": label,
                "efforts": efforts,
                "fastMode": false,
            });
            if let Some(description) = info
                .and_then(|info| info.get("description"))
                .and_then(Value::as_str)
            {
                row["description"] = json!(description);
            }
            if let Some(effort) = default_effort {
                row["defaultEffort"] = json!(effort);
            }
            if default {
                row["default"] = json!(true);
            }
            Some(row)
        })
        .collect();
    Some(agent_entry(
        "Grok Build",
        json!({ "available": false }),
        rows,
    ))
}

/// The words Cursor's labels add after the model's name: context, effort, Fast and Thinking.
fn cursor_trailing_word(word: &str) -> Option<bool> {
    match word {
        "None" | "Minimal" | "Low" | "Medium" | "High" | "Extra" | "Max" => Some(true),
        "Fast" | "Thinking" | "No" | "1M" => Some(false),
        _ => None,
    }
}

/// `cursor-agent --list-models`.
fn cursor_lineup(output: &str) -> Option<Value> {
    struct Variant {
        effort: Option<&'static str>,
        fast: bool,
        names_effort: bool,
    }
    struct Model {
        base: String,
        picker_label: String,
        thinking: bool,
        variants: Vec<Variant>,
    }
    let mut models: Vec<Model> = Vec::new();
    for line in output.lines() {
        let line: String = line
            .chars()
            .filter(|ch| !matches!(ch, '\u{200b}'..='\u{200d}' | '\u{feff}'))
            .collect();
        let Some((id, label)) = line.trim().split_once(" - ") else {
            continue;
        };
        if id.is_empty() || id.contains(char::is_whitespace) {
            continue;
        }
        let mut tokens: Vec<&str> = id.split('-').collect();
        let fast = tokens.last() == Some(&"fast");
        if fast {
            tokens.pop();
        }
        let thinking = tokens.contains(&"thinking");
        tokens.retain(|token| *token != "thinking");
        let effort = if tokens.ends_with(&["extra", "high"]) {
            tokens.truncate(tokens.len() - 2);
            Some("xhigh")
        } else {
            let effort = tokens
                .last()
                .and_then(|last| EFFORT_RANK.iter().copied().find(|effort| effort == last));
            if effort.is_some() {
                tokens.pop();
            }
            effort
        };
        if tokens.is_empty() {
            continue;
        }
        let base = tokens.join("-");
        let label = label.replace("(NO ZDR)", "").replace("(default)", "");
        let mut words: Vec<&str> = label.split_whitespace().collect();
        let mut names_effort = false;
        while let Some(effort_word) = words.last().and_then(|word| cursor_trailing_word(word)) {
            names_effort |= effort_word;
            words.pop();
        }
        words.retain(|word| *word != "1M");
        let picker_label = words.join(" ");
        if picker_label.is_empty() {
            continue;
        }
        let variant = Variant {
            effort,
            fast,
            names_effort,
        };
        match models.iter_mut().find(|model| model.base == base) {
            Some(model) => {
                model.thinking |= thinking;
                model.variants.push(variant);
            }
            None => models.push(Model {
                base,
                picker_label,
                thinking,
                variants: vec![variant],
            }),
        }
    }
    let rows = models
        .into_iter()
        .filter(|model| !excluded("cursor", &model.base))
        .map(|model| {
            let has_efforts = model
                .variants
                .iter()
                .any(|variant| variant.effort.is_some());
            // The variant whose id names no effort is the model's default level, which Cursor
            // calls Medium where the others are Low and High.
            let effort_of = |variant: &Variant| {
                variant
                    .effort
                    .or_else(|| has_efforts.then_some("medium"))
                    .map(str::to_string)
            };
            let efforts = ranked(model.variants.iter().filter_map(effort_of).collect());
            let default_effort = model
                .variants
                .iter()
                .find(|variant| !variant.fast && !variant.names_effort)
                .and_then(effort_of);
            let label = model
                .picker_label
                .strip_prefix("Claude ")
                .unwrap_or(&model.picker_label);
            let mut row = json!({
                "value": model.base,
                "label": label,
                "pickerLabel": model.picker_label,
                "efforts": efforts,
                "fastMode": model.variants.iter().any(|variant| variant.fast),
            });
            if let Some(effort) = default_effort {
                row["defaultEffort"] = json!(effort);
            }
            if model.thinking {
                row["thinking"] = json!(true);
            }
            // Cursor printed "Cursor Grok 4.6" in its footer before 2026.09.18.
            if model.base.starts_with("cursor-") {
                row["terminalLabels"] = json!([format!("Cursor {}", model.picker_label)]);
            }
            row
        })
        .collect();
    Some(agent_entry(
        "Cursor CLI",
        json!({ "available": true, "scope": "model" }),
        rows,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(command: &Command, key: &str) -> Option<String> {
        command
            .get_envs()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    }

    #[test]
    fn the_antigravity_lineup_probe_turns_agy_auto_update_off() {
        let home = std::env::temp_dir();
        let agy = lineup_command("antigravity", "agy", &["models"], &home);
        assert_eq!(
            env_of(&agy, "AGY_CLI_DISABLE_AUTO_UPDATE").as_deref(),
            Some("true")
        );
        let codex = lineup_command("codex", "codex", &["debug", "models"], &home);
        assert_eq!(env_of(&codex, "AGY_CLI_DISABLE_AUTO_UPDATE"), None);
    }
}
