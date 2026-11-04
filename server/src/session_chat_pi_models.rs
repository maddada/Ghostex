/*
CDXC:AgentProviders 2026-09-30 WHY:
Pi and OMP have no model catalog on disk that says which models the user can reach: that depends on the providers they are logged in to and on the extensions they load (the Cursor extension alone adds about 250 Pi models). So the lineup comes from the CLI itself: Pi answers `get_available_models` in RPC mode with every model its own `/model` offers (with the reasoning levels each supports), and OMP prints the same with `omp models --json`. Both take seconds to start, so they run on a background thread, never inside the detect tick, and the chat keeps the read-only pills until the first answer lands.
SEE-ALSO: packages/gx-chat-core/src/menus/option_catalog.rs builds the picker from this document, server/src/session_chat_provider_model_picker.rs types the pick.
*/

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::server::read_runtime_text;
use crate::session_chat_options::{
    SessionChatDetectedChoice, SessionChatDetectedSelection, SessionChatOptionAgent,
    SessionChatOptionEvidence,
};

/// How long a lineup is served before the CLI is asked again (a login or an extension changes it).
const LINEUP_TTL: Duration = Duration::from_secs(120);
/// A CLI that could not answer is asked again sooner.
const LINEUP_RETRY: Duration = Duration::from_secs(30);
const LINEUP_TIMEOUT: Duration = Duration::from_secs(30);
const CATALOG_UPDATED_AT: &str = "2026-09-30";

/// Pi's thinking levels, lowest first (`EXTENDED_THINKING_LEVELS` in pi-ai).
const PI_THINKING_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
/// Whether `level` is one of Pi's thinking levels.
pub(crate) fn is_pi_thinking_level(level: &str) -> bool {
    PI_THINKING_LEVELS.contains(&level)
}

/// OMP's efforts, lowest first (`THINKING_EFFORTS` in pi-catalog); `off` is not one of them.
const OMP_EFFORTS: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];

/// CDXC:AgentProviders 2026-10-06 WHY:
/// Empryo 3.9.0-beta's `--list-models` prints each ready provider's models as `provider/id` with no reasoning levels, and the levels a model takes come from its models.dev entry inside Empryo (`off, low, medium, high, xhigh, max` for the subscription GPT and Claude models, `off, high, max` for GLM-5.2). Every row offers this ladder, and a level the model lacks is refused by the picker with the ladder Empryo's own `/effort` panel shows for it.
const EMPRYO_EFFORTS: [&str; 6] = ["off", "low", "medium", "high", "xhigh", "max"];

const PI_MODELS_REQUEST: &str = "{\"id\":\"ghostex-models\",\"type\":\"get_available_models\"}\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PiFamilyAgent {
    Pi,
    Omp,
    /// Not a Pi fork, but its lineup comes from its CLI the same way.
    Empryo,
}

impl PiFamilyAgent {
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        match id {
            "pi" => Some(Self::Pi),
            "omp" => Some(Self::Omp),
            "empryo" => Some(Self::Empryo),
            _ => None,
        }
    }

    fn from_option_agent(agent: SessionChatOptionAgent) -> Option<Self> {
        match agent {
            SessionChatOptionAgent::Pi => Some(Self::Pi),
            SessionChatOptionAgent::Omp => Some(Self::Omp),
            SessionChatOptionAgent::Empryo => Some(Self::Empryo),
            _ => None,
        }
    }

    /// The catalog's agent key and the CLI's command name.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::Omp => "omp",
            Self::Empryo => "empryo",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Pi => "Pi Agent",
            Self::Omp => "OMP",
            Self::Empryo => "Empryo",
        }
    }
}

struct Lineup {
    read_at: Instant,
    catalog: Option<Value>,
}

static LINEUPS: Mutex<Option<HashMap<PiFamilyAgent, Lineup>>> = Mutex::new(None);
static REFRESHING: Mutex<Vec<PiFamilyAgent>> = Mutex::new(Vec::new());

/// The last lineup read from the CLI, or `None` until the first read lands. A stale or missing
/// lineup starts a background read; the caller never waits for it.
pub(crate) fn pi_family_model_catalog(agent: PiFamilyAgent) -> Option<Value> {
    let (catalog, stale) = LINEUPS
        .lock()
        .ok()
        .and_then(|lineups| {
            let lineup = lineups.as_ref()?.get(&agent)?;
            let ttl = if lineup.catalog.is_some() {
                LINEUP_TTL
            } else {
                LINEUP_RETRY
            };
            Some((lineup.catalog.clone(), lineup.read_at.elapsed() >= ttl))
        })
        .unwrap_or((None, true));
    if stale {
        refresh_in_background(agent);
    }
    catalog
}

fn refresh_in_background(agent: PiFamilyAgent) {
    {
        let Ok(mut refreshing) = REFRESHING.lock() else {
            return;
        };
        if refreshing.contains(&agent) {
            return;
        }
        refreshing.push(agent);
    }
    let spawned = std::thread::Builder::new()
        .name(format!("gx-{}-models", agent.id()))
        .spawn(move || {
            let catalog = read_lineup(agent);
            if let Ok(mut lineups) = LINEUPS.lock() {
                let lineups = lineups.get_or_insert_with(HashMap::new);
                // A failed read keeps serving the last good lineup until the next attempt.
                let catalog =
                    catalog.or_else(|| lineups.get(&agent).and_then(|old| old.catalog.clone()));
                lineups.insert(
                    agent,
                    Lineup {
                        read_at: Instant::now(),
                        catalog,
                    },
                );
            }
            if let Ok(mut refreshing) = REFRESHING.lock() {
                refreshing.retain(|entry| *entry != agent);
            }
        });
    if spawned.is_err() {
        if let Ok(mut refreshing) = REFRESHING.lock() {
            refreshing.retain(|entry| *entry != agent);
        }
    }
}

fn read_lineup(agent: PiFamilyAgent) -> Option<Value> {
    let home = crate::resume_lookup::home_dir();
    let program = crate::agent_hooks::probing::resolve_probe_command(agent.id(), &home)?;
    let models = match agent {
        PiFamilyAgent::Pi => {
            let output = run_cli(
                &program,
                &["--mode", "rpc", "--no-session", "--offline"],
                Some(PI_MODELS_REQUEST),
                &home,
                is_pi_models_response,
            )?;
            output
                .lines()
                .filter(|line| is_pi_models_response(line))
                .find_map(|line| serde_json::from_str::<Value>(line).ok())?
                .pointer("/data/models")?
                .as_array()?
                .iter()
                .filter_map(pi_model_row)
                .collect::<Vec<_>>()
        }
        PiFamilyAgent::Omp => {
            let output = run_cli(&program, &["models", "--json"], None, &home, |_| false)?;
            output
                .lines()
                .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())?
                .get("models")?
                .as_array()?
                .iter()
                .filter_map(omp_model_row)
                .collect::<Vec<_>>()
        }
        PiFamilyAgent::Empryo => {
            let output = run_cli(&program, &["--list-models"], None, &home, |_| false)?;
            let rows = empryo_model_rows(&output);
            if rows.is_empty() {
                return None;
            }
            rows
        }
    };
    let (default, scoped) = pinned_models(agent, &home);
    Some(build_catalog(agent, models, default.as_deref(), &scoped))
}

/// The models the CLI itself puts first: its default model as `provider/id`, and (Pi only) the
/// patterns of its `enabledModels` scope, in their order.
fn pinned_models(agent: PiFamilyAgent, home: &Path) -> (Option<String>, Vec<String>) {
    match agent {
        PiFamilyAgent::Pi => {
            let agent_dir = crate::session_chat_paths::configured_agent_directory(
                "PI_CODING_AGENT_DIR",
                ".pi/agent",
            );
            let settings = std::fs::read(agent_dir.join("settings.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .unwrap_or(Value::Null);
            let text = |key: &str| {
                settings
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            };
            let default = match (text("defaultProvider"), text("defaultModel")) {
                (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
                _ => None,
            };
            let scoped = settings
                .get("enabledModels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|pattern| pattern.as_str().map(str::trim))
                .filter(|pattern| !pattern.is_empty())
                .map(str::to_string)
                .collect();
            (default, scoped)
        }
        PiFamilyAgent::Omp => {
            let config =
                std::fs::read_to_string(home.join(".omp").join("agent").join("config.yml"))
                    .unwrap_or_default();
            (omp_default_role(&config), Vec::new())
        }
        // A project's own `.empryo/config.json` can name another; the lineup is the user's.
        PiFamilyAgent::Empryo => (
            empryo_config_default_model(&crate::agent_hooks::config::empryo_home(
                &crate::agent_hooks::config::HookPaths::from_paths(
                    &crate::paths::get_gxserver_paths(None),
                ),
            )),
            Vec::new(),
        ),
    }
}

/// `empryo --list-models`: a `<Name> (<id>)` line per ready provider, then one
/// `  <id>/<model>  <N>k ctx  <prices>` line per model. Empryo's statusline names the model
/// `<Name>/<model>`.
fn empryo_model_rows(output: &str) -> Vec<LineupModel> {
    let mut rows = Vec::new();
    let mut provider: Option<(String, String)> = None;
    for line in output.lines() {
        let line = crate::session_chat_options::strip_ansi_sgr(line);
        if !line.starts_with(char::is_whitespace) {
            let head = line.trim().trim_end_matches("[custom]").trim_end();
            provider = head
                .strip_suffix(')')
                .and_then(|head| head.rsplit_once(" ("))
                .map(|(name, id)| (name.trim().to_string(), id.trim().to_string()))
                .filter(|(name, id)| !name.is_empty() && !id.is_empty());
            continue;
        }
        let Some((name, id)) = provider.as_ref() else {
            continue;
        };
        let mut words = line.split_whitespace();
        let Some(model) = words
            .next()
            .and_then(|value| value.strip_prefix(&format!("{id}/")))
            .filter(|model| !model.is_empty())
        else {
            continue;
        };
        let context_window = match (words.next(), words.next()) {
            (Some(size), Some("ctx")) => size
                .strip_suffix('k')
                .and_then(|thousands| thousands.parse::<u64>().ok())
                .map(|thousands| thousands * 1000),
            _ => None,
        };
        rows.push(LineupModel {
            provider: id.clone(),
            id: model.to_string(),
            name: model.to_string(),
            efforts: EMPRYO_EFFORTS.to_vec(),
            terminal_labels: vec![format!("{name}/{model}")],
            context_window,
        });
    }
    rows
}

/// The `defaultModel` (`provider/model`) of the Empryo `config.json` in `dir`.
pub(crate) fn empryo_config_default_model(dir: &Path) -> Option<String> {
    let config: Value =
        serde_json::from_slice(&std::fs::read(dir.join("config.json")).ok()?).ok()?;
    config
        .get("defaultModel")?
        .as_str()
        .map(str::trim)
        .filter(|model| model.contains('/'))
        .map(str::to_string)
}

/// `modelRoles.default` from OMP's `config.yml`, without its `:<level>` suffix.
fn omp_default_role(config: &str) -> Option<String> {
    let mut in_roles = false;
    for line in config.lines() {
        if !line.starts_with([' ', '\t']) {
            in_roles = line.trim_end() == "modelRoles:";
            continue;
        }
        let Some(value) = line.trim().strip_prefix("default:").filter(|_| in_roles) else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']);
        let value = match value.rsplit_once(':') {
            Some((model, level)) if PI_THINKING_LEVELS.contains(&level) || level == "auto" => model,
            _ => value,
        };
        return (!value.is_empty()).then(|| value.to_string());
    }
    None
}

/// Whether a Pi `enabledModels` pattern names `value`: exactly, with a `:<level>` suffix, or as a
/// `*` glob. Pi's fuzzy matches are not reproduced; an unmatched pattern pins nothing.
fn pattern_names(pattern: &str, value: &str) -> bool {
    if pattern == value {
        return true;
    }
    if let Some((model, level)) = pattern.rsplit_once(':') {
        if PI_THINKING_LEVELS.contains(&level) && model == value {
            return true;
        }
    }
    if !pattern.contains('*') {
        return false;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !value.starts_with(first) || value.len() < first.len() + last.len() || !value.ends_with(last)
    {
        return false;
    }
    let mut rest = &value[first.len()..value.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

fn is_pi_models_response(line: &str) -> bool {
    line.contains("\"command\":\"get_available_models\"") && line.contains("\"type\":\"response\"")
}

/// Runs the CLI with the PATH a session's shell gets and returns its stdout once it exits or
/// `done` accepts a line. Stdout is read while the child runs: Pi's answer is far larger than a
/// pipe buffer, so waiting for the exit first would never finish.
fn run_cli(
    program: &str,
    args: &[&str],
    request: Option<&str>,
    home: &Path,
    done: fn(&str) -> bool,
) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(home)
        .env(
            "PATH",
            crate::agent_hooks::probing::normalize_gxserver_process_path(
                std::env::var("PATH").ok().as_deref(),
                home,
            ),
        )
        .stdin(if request.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
    let mut child = command.spawn().ok()?;
    // Pi's RPC mode answers while its stdin stays open, and exits when it closes.
    let mut stdin = child.stdin.take();
    if let (Some(stdin), Some(request)) = (stdin.as_mut(), request) {
        if stdin
            .write_all(request.as_bytes())
            .and_then(|()| stdin.flush())
            .is_err()
        {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    }
    let stdout = child.stdout.take()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut text = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    text.push_str(&line);
                    if done(&line) {
                        break;
                    }
                }
            }
        }
        let _ = sender.send(text);
    });
    let output = receiver.recv_timeout(LINEUP_TIMEOUT).ok();
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    output
}

/// One catalog row, before the document is assembled.
struct LineupModel {
    provider: String,
    id: String,
    name: String,
    efforts: Vec<&'static str>,
    /// What the CLI's statusline prints for this model.
    terminal_labels: Vec<String>,
    /// Tokens the model takes in one request, which the status line's context meter divides by.
    context_window: Option<u64>,
}

fn context_window_of(model: &Value) -> Option<u64> {
    model
        .get("contextWindow")
        .and_then(Value::as_u64)
        .filter(|tokens| *tokens > 0)
}

/// A Pi `Model` object from `get_available_models`. Its levels follow `getSupportedThinkingLevels`:
/// none without reasoning, a level mapped to `null` is unsupported, and `xhigh` and `max` exist only
/// when mapped.
fn pi_model_row(model: &Value) -> Option<LineupModel> {
    let provider = model.get("provider")?.as_str()?.trim().to_string();
    let id = model.get("id")?.as_str()?.trim().to_string();
    if provider.is_empty() || id.is_empty() {
        return None;
    }
    let reasoning = model
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let map = model.get("thinkingLevelMap").and_then(Value::as_object);
    let efforts = if reasoning {
        PI_THINKING_LEVELS
            .into_iter()
            .filter(|level| match map.and_then(|map| map.get(*level)) {
                Some(Value::Null) => false,
                Some(_) => true,
                None => !matches!(*level, "xhigh" | "max"),
            })
            .collect()
    } else {
        Vec::new()
    };
    let name = model
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&id)
        .to_string();
    Some(LineupModel {
        terminal_labels: vec![id.clone()],
        context_window: context_window_of(model),
        provider,
        id,
        name,
        efforts,
    })
}

/// A row of `omp models --json`. OMP's statusline prints the model's name, less a leading
/// `Claude ` (`modelSegment` in pi-tui's status line).
fn omp_model_row(model: &Value) -> Option<LineupModel> {
    let provider = model.get("provider")?.as_str()?.trim().to_string();
    let id = model.get("id")?.as_str()?.trim().to_string();
    if provider.is_empty() || id.is_empty() {
        return None;
    }
    let efforts = model
        .get("thinking")
        .and_then(Value::as_array)
        .map(|levels| {
            OMP_EFFORTS
                .into_iter()
                .filter(|effort| levels.iter().any(|level| level.as_str() == Some(*effort)))
                .collect()
        })
        .unwrap_or_default();
    let name = model
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&id)
        .to_string();
    let shown = name.strip_prefix("Claude ").unwrap_or(&name).to_string();
    let mut terminal_labels = vec![shown];
    if !terminal_labels.contains(&id) {
        terminal_labels.push(id.clone());
    }
    Some(LineupModel {
        context_window: context_window_of(model),
        provider,
        id,
        name,
        efforts,
        terminal_labels,
    })
}

/// The `AgentModelCatalog` document the chat's picker reads: one row per model, keyed
/// `provider/id` (what `/model` and `/switch` take), grouped by provider.
///
/// CDXC:AgentProviders 2026-09-30 WHY:
/// Rows follow the CLI's own `/model` list: its default model and Pi's scoped models (`enabledModels`, which Pi's selector opens on) first, then each provider's models, smallest provider first. One extension can bring hundreds of models (Pi's Cursor extension lists about 250), and listing providers in the CLI's order buried a subscription's handful of models, such as a Codex login's, below all of them.
fn build_catalog(
    agent: PiFamilyAgent,
    mut models: Vec<LineupModel>,
    default: Option<&str>,
    scoped: &[String],
) -> Value {
    let pin_rank = |model: &LineupModel| {
        let value = format!("{}/{}", model.provider, model.id);
        if default == Some(value.as_str()) {
            return Some(0);
        }
        scoped
            .iter()
            .position(|pattern| pattern_names(pattern, &value))
            .map(|index| index + 1)
    };
    let mut provider_sizes: HashMap<String, usize> = HashMap::new();
    for model in &models {
        *provider_sizes.entry(model.provider.clone()).or_default() += 1;
    }
    // Stable: a provider's models keep the CLI's order.
    models.sort_by(|left, right| {
        let rank = |model: &LineupModel| {
            (
                pin_rank(model).unwrap_or(usize::MAX),
                provider_sizes.get(&model.provider).copied().unwrap_or(0),
                model.provider.clone(),
            )
        };
        rank(left).cmp(&rank(right))
    });
    let mut groups: Vec<Value> = Vec::new();
    let mut seen_groups: Vec<String> = Vec::new();
    let mut all_efforts: Vec<&'static str> = Vec::new();
    let mut rows = Vec::with_capacity(models.len());
    for model in models {
        if !seen_groups.contains(&model.provider) {
            seen_groups.push(model.provider.clone());
            groups.push(json!({ "id": model.provider, "label": model.provider }));
        }
        for effort in &model.efforts {
            if !all_efforts.contains(effort) {
                all_efforts.push(effort);
            }
        }
        let value = format!("{}/{}", model.provider, model.id);
        let is_default = default == Some(value.as_str());
        let mut row = json!({
            "value": value,
            "label": model.name,
            "description": if is_default { format!("{value} (default)") } else { value.clone() },
            "default": is_default,
            "efforts": model.efforts,
            "group": model.provider,
            "terminalLabels": model.terminal_labels,
        });
        if let Some(tokens) = model.context_window {
            row["contextWindow"] = json!(tokens);
        }
        rows.push(row);
    }
    let levels: &[&str] = match agent {
        PiFamilyAgent::Pi => &PI_THINKING_LEVELS,
        PiFamilyAgent::Omp => &OMP_EFFORTS,
        PiFamilyAgent::Empryo => &EMPRYO_EFFORTS,
    };
    let efforts: Vec<&str> = levels
        .iter()
        .copied()
        .filter(|level| all_efforts.contains(level))
        .collect();
    let mut agent_entry = Map::new();
    agent_entry.insert("name".into(), json!(agent.name()));
    agent_entry.insert("efforts".into(), json!(efforts));
    if !groups.is_empty() {
        agent_entry.insert("groups".into(), Value::Array(groups));
    }
    agent_entry.insert("models".into(), Value::Array(rows));
    json!({
        "schemaVersion": 1,
        "updatedAt": CATALOG_UPDATED_AT,
        "effortLabels": {},
        "agents": { agent.id(): Value::Object(agent_entry) },
    })
}

/// The detection layer for a Pi or OMP session: the lineup, and the model the transcript last
/// recorded (`model_change`), which is exact where the statusline is not: Pi drops the provider
/// when the footer is narrow or only one provider is logged in, and OMP prints only the name.
pub fn read_pi_family_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent: SessionChatOptionAgent,
) -> Option<SessionChatDetectedSelection> {
    let family = PiFamilyAgent::from_option_agent(agent)?;
    let catalog = pi_family_model_catalog(family);
    let (path, model) = if family == PiFamilyAgent::Empryo {
        (None, empryo_tab_model(repository, project_id, session_id))
    } else {
        let path = transcript_path(repository, project_id, session_id);
        let model = path.as_deref().and_then(transcript_model);
        (path, model)
    };
    // CDXC:SessionChat 2026-10-17 WHY: OMP's transcript records the same per-reply `usage` as Pi's, and its context meter read "not yet reported" while OMP's own footer showed the usage, so its context comes from the transcript too; the Pi status line stays Pi's.
    let status = path
        .as_deref()
        .filter(|_| matches!(family, PiFamilyAgent::Pi | PiFamilyAgent::Omp))
        .and_then(crate::session_chat_pi_status::read_pi_transcript_status);
    if catalog.is_none() && model.is_none() && status.is_none() {
        return None;
    }
    let row = |value: &str| {
        catalog.as_ref().and_then(|catalog| {
            catalog
                .pointer(&format!("/agents/{}/models", family.id()))?
                .as_array()?
                .iter()
                .find(|row| row.get("value").and_then(Value::as_str) == Some(value))
                .cloned()
        })
    };
    let status_row = status
        .as_ref()
        .and_then(|status| status.model.as_deref())
        .and_then(row);
    let context_window = status_row
        .as_ref()
        .and_then(|row| row.get("contextWindow"))
        .and_then(Value::as_u64);
    let context_usage = status
        .as_ref()
        .and_then(|status| status.context_tokens)
        .map(
            |tokens| crate::session_chat_options::SessionChatContextUsage {
                used_percentage: context_window
                    .map(|window| ((tokens as f64 / window as f64) * 100.0).round() as u32),
                used_tokens: Some(tokens),
                window_size: context_window,
            },
        );
    let pi_status = status
        .as_ref()
        .filter(|_| family == PiFamilyAgent::Pi)
        .map(|status| {
            crate::session_chat_pi_status::pi_status_value(
                status,
                status_row
                    .as_ref()
                    .and_then(|row| row.get("label"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                context_window,
            )
        });
    Some(SessionChatDetectedSelection {
        // The lineup's name for the model (`GPT-5.5`), as the picker's own row reads.
        model: model.map(|value| SessionChatDetectedChoice {
            label: row(&value)
                .and_then(|row| row.get("label")?.as_str().map(str::to_string))
                .unwrap_or_else(|| value.clone()),
            value,
            source: SessionChatOptionEvidence::Transcript,
        }),
        context_usage,
        pi_status,
        model_catalog: catalog,
        ..SessionChatDetectedSelection::default()
    })
}

fn transcript_path(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Option<std::path::PathBuf> {
    let session = repository.get_session(project_id, session_id).ok()??;
    crate::session_chat::resolve_session_chat_transcript_path(
        crate::session_chat::SessionChatTranscriptAgent::Pi,
        read_runtime_text(&session, "agentSessionId").as_deref(),
        read_runtime_text(&session, "agentSessionPath").as_deref(),
    )
}

/// The model of the Empryo tab the session shows (`activeModel` in its `meta.json`, beside the
/// `session.jsonl` the hooks report), which Empryo rewrites the moment `/models` picks one.
fn empryo_tab_model(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Option<String> {
    let session = repository.get_session(project_id, session_id).ok()??;
    empryo_tab_model_at(&empryo_session_log(repository, &session)?)
}

/// An Empryo session row's `session.jsonl`: the path its hooks reported, else the folder
/// `empryo --session` opens, `<cwd>/.empryo/sessions/<agentSessionId>/` under the session's working
/// folder (its project's path when it has none), which is where a launch model's seeded session
/// lives before any hook has named it.
pub(crate) fn empryo_session_log(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Option<std::path::PathBuf> {
    if let Some(path) = read_runtime_text(session, "agentSessionPath") {
        return Some(path.into());
    }
    let id = read_runtime_text(session, "agentSessionId")
        .filter(|id| crate::session_chat_empryo_mirror::is_safe_empryo_session_id(id))?;
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let cwd = text(session, "cwd").or_else(|| {
        let project = repository
            .get_project(&text(session, "projectId")?)
            .ok()??;
        text(&project, "path")
    })?;
    Some(
        Path::new(&cwd)
            .join(".empryo")
            .join("sessions")
            .join(id)
            .join("session.jsonl"),
    )
}

/// [`empryo_tab_model`] for the `session.jsonl` at `log`.
pub(crate) fn empryo_tab_model_at(log: &Path) -> Option<String> {
    let meta = std::fs::read(log.with_file_name("meta.json")).ok()?;
    let meta = serde_json::from_slice::<Value>(&meta).ok()?;
    let active = meta.get("activeTabId").and_then(Value::as_str);
    let tabs = meta.get("tabs")?.as_array()?;
    tabs.iter()
        .find(|tab| tab.get("id").and_then(Value::as_str) == active)
        .or_else(|| tabs.first())?
        .get("activeModel")?
        .as_str()
        .map(str::trim)
        .filter(|model| model.contains('/'))
        .map(str::to_string)
}

/// The last `model_change` in the transcript's tail, as `provider/id`: Pi writes `provider` and
/// `modelId`, OMP writes `model` already joined.
fn transcript_model(path: &Path) -> Option<String> {
    let text = crate::session_chat_options::transcript_tail_text(path).ok()?;
    text.lines().rev().find_map(|line| {
        if !line.contains("\"model_change\"") {
            return None;
        }
        let entry = serde_json::from_str::<Value>(line).ok()?;
        if entry.get("type").and_then(Value::as_str) != Some("model_change") {
            return None;
        }
        let text = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        match (text("provider"), text("modelId")) {
            (Some(provider), Some(id)) => Some(format!("{provider}/{id}")),
            _ => text("model").map(str::to_string),
        }
    })
}

/// Lowercase alphanumeric words joined by `-`, so `Claude Opus 5.5` and `claude-opus-5-5` compare.
fn spelled(text: &str) -> String {
    text.to_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Whether an Empryo display name (a model panel row, `Subscriptions · Claude Pro/Max Claude Opus
/// 5`, or the input box border's `Anthropic-sub/Claude Opus 5.5`) ends with the model `id`, or with
/// the id less a trailing `-YYYYMMDD` date.
pub(crate) fn empryo_name_spells_id(name: &str, id: &str) -> bool {
    let name = format!("-{}", spelled(name));
    let id = spelled(id);
    let undated = match id.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|byte| byte.is_ascii_digit()) => {
            Some(head.to_string())
        }
        _ => None,
    };
    std::iter::once(id)
        .chain(undated)
        .any(|id| name.ends_with(&format!("-{id}")))
}

/// Whether the model Empryo 3.9.1-beta shows on its input box border, `<vendor>/<display name>`
/// (only the name in a narrow box), is `value` (`provider/id`): the name spells the id, and the
/// vendor, when shown, names the provider. Empryo prints `<Vendor>-sub` for its `proxy` and
/// `subscriptions` providers, `<Vendor>-<provider name without spaces>` for any other, and the bare
/// vendor for the vendor's own provider, which is what tells `subscriptions/gpt-6-luna` from
/// `opencode-go/gpt-6-luna`.
pub(crate) fn empryo_shown_names_value(shown: &str, value: &str) -> bool {
    let Some((provider, id)) = value.split_once('/') else {
        return false;
    };
    if !empryo_name_spells_id(shown, id) {
        return false;
    }
    let Some((vendor, _)) = shown.split_once('/') else {
        return true;
    };
    let compact = |text: &str| spelled(text).replace('-', "");
    match vendor.rsplit_once('-') {
        Some((_, "sub")) => matches!(provider, "proxy" | "subscriptions"),
        Some((_, suffix)) => compact(suffix) == compact(provider),
        None => compact(vendor) == compact(provider),
    }
}

/// The catalog row a statusline reading names, when the reading is not a row's value already:
/// the one row whose `terminalLabels` holds it (for Empryo, else the one its display name names),
/// or, when several do, the one the transcript recorded. `None` leaves the reading as it is.
pub(crate) fn pi_family_catalog_value(
    catalog: &Value,
    shown: &str,
    recorded: Option<&str>,
) -> Option<String> {
    let (agent, rows) = catalog
        .get("agents")?
        .as_object()?
        .iter()
        .find_map(|(agent, entry)| PiFamilyAgent::from_id(agent).map(|agent| (agent, entry)))?;
    let rows = rows.get("models")?.as_array()?;
    fn value_of(row: &Value) -> Option<&str> {
        row.get("value").and_then(Value::as_str)
    }
    if rows.iter().any(|row| value_of(row) == Some(shown)) {
        return None;
    }
    let candidates: Vec<&str> = rows
        .iter()
        .filter(|row| {
            row.get("terminalLabels")
                .and_then(Value::as_array)
                .is_some_and(|labels| labels.iter().any(|label| label.as_str() == Some(shown)))
        })
        .filter_map(value_of)
        .collect();
    // Empryo 3.9.1-beta names the model by display name (`OpenAI-sub/GPT-6 Luna`), never its id.
    let candidates = if candidates.is_empty() && agent == PiFamilyAgent::Empryo {
        rows.iter()
            .filter_map(value_of)
            .filter(|value| empryo_shown_names_value(shown, value))
            .collect()
    } else {
        candidates
    };
    if let Some(recorded) = recorded.filter(|recorded| candidates.contains(recorded)) {
        return Some(recorded.to_string());
    }
    (candidates.len() == 1).then(|| candidates[0].to_string())
}

/// What the statusline prints for a `provider/id` catalog value, from the lineup this server holds,
/// falling back to the bare id.
pub(crate) fn pi_family_terminal_labels(agent: PiFamilyAgent, value: &str) -> Vec<String> {
    let id = value
        .split_once('/')
        .map_or(value, |(_, id)| id)
        .to_string();
    let labels = LINEUPS.lock().ok().and_then(|lineups| {
        let catalog = lineups.as_ref()?.get(&agent)?.catalog.clone()?;
        let row = catalog
            .pointer(&format!("/agents/{}/models", agent.id()))?
            .as_array()?
            .iter()
            .find(|row| row.get("value").and_then(Value::as_str) == Some(value))?
            .clone();
        Some(
            row.get("terminalLabels")?
                .as_array()?
                .iter()
                .filter_map(|label| label.as_str().map(str::to_string))
                .collect::<Vec<_>>(),
        )
    });
    let mut labels = labels.unwrap_or_default();
    if !labels.contains(&id) {
        labels.push(id);
    }
    labels
}
