//! The published agent model catalog (`agent-model-catalog.json`), kept
//! current while gxserver runs.
//!
//! CDXC:AgentProviders 2026-10-17 DECISION:
//! User: new models must show in the model menu and the quick picker almost
//! as soon as they exist, with a robust combination of every delivery path.
//! gxserver polls the file on GitHub main, keeps the last good copy on disk,
//! and broadcasts `agentModelCatalogChanged` (carrying the whole document) to
//! every event socket, including chat-only ones, and sends the copy it holds
//! to each socket as it connects. The user confirmed (2026-10-17) that the
//! Antigravity, Codex, Cursor and Grok lineups are read from their CLIs instead
//! ("read the list for them and for cursor now please", then Grok's from
//! `grok models`; `agent_model_lineups.rs`), which supersedes the 2026-09-22 rule that the
//! file alone names every agent's models: a lineup a CLI printed takes that
//! agent's place in the document gxserver serves, and the file's rows for it
//! remain only for released apps and for a computer whose CLI never answered.
//! The served document is what screen detection maps footer names through and
//! what the Cursor, Grok and Antigravity picker drivers accept, so a model is
//! selectable and recognised without a release. Until 2026-09-25 the
//! TypeScript clients also polled GitHub themselves (the deleted
//! `packages/shared/agent-model-catalog-state.ts`); the Rust chat clients take
//! the catalog from gxserver and the build's bundled snapshot only.
//!
//! WHY: raw.githubusercontent.com serves the file with a five-minute cache,
//! so polling faster than that finds nothing new. The request carries the
//! last ETag and an unchanged file answers 304 with no body.
//! GHOSTEX_AGENT_MODEL_CATALOG_REMOTE=off turns the network path off, and
//! unit tests never fetch.
//!
//! SEE-ALSO: `packages/gx-chat-core/src/menus/catalog.rs` (the parser this
//! validation mirrors).

use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    events::GxserverEventHub,
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
    paths::GxserverPaths,
};

pub const AGENT_MODEL_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/maddada/Ghostex/main/agent-model-catalog.json";
pub const AGENT_MODEL_CATALOG_CHANGED_EVENT_TYPE: &str = "agentModelCatalogChanged";
const BUNDLED_CATALOG: &str = include_str!("../../agent-model-catalog.json");
const SCHEMA_VERSION: i64 = 1;
const OPT_OUT_ENV: &str = "GHOSTEX_AGENT_MODEL_CATALOG_REMOTE";
const FIRST_POLL_DELAY: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;
const USER_AGENT: &str = "ghostex-gxserver";
/// A lineup read from a CLI is read again after this long, and sooner when the CLI did not answer.
const LINEUP_TTL: Duration = Duration::from_secs(10 * 60);
const LINEUP_RETRY: Duration = Duration::from_secs(2 * 60);
const LINEUP_TICK: Duration = Duration::from_secs(15);

/// One agent's lineup as its CLI last printed it, and when it last changed.
struct Lineup {
    entry: Value,
    changed_at: String,
}

struct CatalogState {
    /// The published document: the newest of the bundle, the cache and GitHub.
    catalog: Arc<Value>,
    /// What gxserver serves: `catalog` with each CLI's lineup in its agent's place.
    effective: Arc<Value>,
    lineups: BTreeMap<String, Lineup>,
    /// True from boot: the effective lineup (the newest of the bundle and
    /// the cache) counts as published, so every client is told it, not only
    /// once a fetch succeeds. See adopt_cached_copy.
    published: bool,
    etag: Option<String>,
}

fn state() -> &'static RwLock<CatalogState> {
    static STATE: OnceLock<RwLock<CatalogState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let bundled: Value =
            serde_json::from_str(BUNDLED_CATALOG).expect("bundled agent model catalog is JSON");
        assert!(
            is_valid_catalog(&bundled),
            "bundled agent model catalog does not validate"
        );
        let mut effective = bundled.clone();
        order_claude_models(&mut effective);
        RwLock::new(CatalogState {
            catalog: Arc::new(bundled),
            effective: Arc::new(effective),
            lineups: BTreeMap::new(),
            published: false,
            etag: None,
        })
    })
}

/// The catalog in effect: the newest of the bundle and the cache from boot,
/// then the last good copy fetched from GitHub, with the lineups the agent
/// CLIs printed in their agents' places.
pub fn current() -> Arc<Value> {
    state()
        .read()
        .map(|state| state.effective.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().effective.clone())
}

/// Recomputes the served document after the published copy or a lineup changed. Its `updatedAt`
/// is the later of the file's and the newest lineup change, so clients, which keep the newer
/// document, adopt a changed lineup.
fn rebuild_effective(state: &mut CatalogState) {
    let mut effective = (*state.catalog).clone();
    if state.lineups.is_empty() {
        order_claude_models(&mut effective);
        state.effective = Arc::new(effective);
        return;
    }
    // A lineup read from a CLI replaces its agent whole. Claude's, read from its picker, is then
    // sorted by family like the file's (Q2982: the Fable-first sort stays).
    if let Some(agents) = effective.get_mut("agents").and_then(Value::as_object_mut) {
        for (agent, lineup) in &state.lineups {
            agents.insert(agent.clone(), lineup.entry.clone());
        }
    }
    order_claude_models(&mut effective);
    if let Some(newest) = state
        .lineups
        .values()
        .map(|lineup| lineup.changed_at.as_str())
        .max()
        .filter(|newest| *newest > updated_at(&state.catalog))
    {
        effective["updatedAt"] = json!(newest);
    }
    state.effective = Arc::new(effective);
}

/// Claude's rows by family, Fable, Opus, Sonnet, Haiku, then newest version first, with every
/// older version of a family in the Legacy models group, as the chat core reads them, so the New
/// Coordinator forms and clients built before the core did read the same lineup.
/// SEE-ALSO: packages/gx-chat-core/src/menus/catalog.rs (`order_claude_models` and `group_older_versions`, the rules and the user's words).
fn order_claude_models(catalog: &mut Value) {
    use ghostex_gx_chat_core::menus::catalog as core;
    let Some(agent) = catalog
        .pointer_mut(&format!("/agents/{}", core::CLAUDE_AGENT_ID))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let Some(models) = agent.get_mut("models").and_then(Value::as_array_mut) else {
        return;
    };
    let older = core::older_versions(models.iter().map(|model| {
        model
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }));
    let mut grouped = false;
    for (model, older) in models.iter_mut().zip(older) {
        if let Some(model) = model
            .as_object_mut()
            .filter(|model| older && model.get("group").is_none_or(Value::is_null))
        {
            model.insert("group".to_string(), json!(core::LEGACY_GROUP_ID));
            grouped = true;
        }
    }
    core::order_claude_models(
        models,
        |model| {
            model
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or_default()
        },
        |model| model.get("group").and_then(Value::as_str),
    );
    if grouped {
        let groups = agent.entry("groups").or_insert_with(|| json!([]));
        if let Some(groups) = groups.as_array_mut() {
            let declared = groups.iter().any(|group| {
                group.get("id").and_then(Value::as_str) == Some(core::LEGACY_GROUP_ID)
            });
            if !declared {
                groups.push(json!({
                    "id": core::LEGACY_GROUP_ID,
                    "label": core::LEGACY_GROUP_LABEL,
                }));
            }
        }
    }
}

fn lineups_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_cache_dir.join("agent-model-lineups.json")
}

fn read_cached_lineups(paths: &GxserverPaths) -> BTreeMap<String, Lineup> {
    let Some(cached) = fs::read_to_string(lineups_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return BTreeMap::new();
    };
    crate::agent_model_lineups::RUNTIME_AGENTS
        .iter()
        .filter_map(|agent| {
            let saved = cached.get(*agent)?;
            let entry = saved.get("entry").filter(|entry| is_valid_agent(entry))?;
            Some((
                agent.to_string(),
                Lineup {
                    entry: entry.clone(),
                    changed_at: saved.get("changedAt")?.as_str()?.to_string(),
                },
            ))
        })
        .collect()
}

fn write_cached_lineups(paths: &GxserverPaths) {
    let Some(text) = state().read().ok().and_then(|state| {
        let saved: serde_json::Map<String, Value> = state
            .lineups
            .iter()
            .map(|(agent, lineup)| {
                (
                    agent.clone(),
                    json!({ "changedAt": lineup.changed_at, "entry": lineup.entry }),
                )
            })
            .collect();
        serde_json::to_string_pretty(&Value::Object(saved)).ok()
    }) else {
        return;
    };
    let path = lineups_path(paths);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let staged = path.with_extension("json.tmp");
    if fs::write(&staged, text).is_ok() {
        let _ = fs::rename(&staged, &path);
    }
}

/// Puts a lineup a CLI printed in its agent's place. Returns whether the served document changed.
fn adopt_lineup(agent: &str, entry: Value) -> bool {
    let Ok(mut state) = state().write() else {
        return false;
    };
    if state
        .lineups
        .get(agent)
        .is_some_and(|lineup| lineup.entry == entry)
    {
        return false;
    }
    state.lineups.insert(
        agent.to_string(),
        Lineup {
            entry,
            changed_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        },
    );
    rebuild_effective(&mut state);
    true
}

/// One lineup read at a time per agent, so the model picker's Fetch latest models and the
/// background pass never start the same CLI twice at once.
fn lineup_read_gate(agent: &str) -> Option<&'static std::sync::Mutex<()>> {
    static GATES: OnceLock<Vec<std::sync::Mutex<()>>> = OnceLock::new();
    let agents = crate::agent_model_lineups::RUNTIME_AGENTS;
    let index = agents.iter().position(|known| *known == agent)?;
    GATES
        .get_or_init(|| agents.iter().map(|_| std::sync::Mutex::new(())).collect())
        .get(index)
}

/// Reads one agent's lineup and, when it changed, serves it, caches it and tells every client.
/// `Some(changed)` when the CLI answered, `None` when it did not (its last lineup stays).
fn read_and_adopt_lineup(
    agent: &str,
    fresh: bool,
    paths: &GxserverPaths,
    event_hub: &GxserverEventHub,
    server_id: &str,
) -> Option<bool> {
    let gate = lineup_read_gate(agent)?;
    let _reading = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = crate::agent_model_lineups::read_lineup(agent, &paths.app_cache_dir, fresh)
        .filter(is_valid_agent)?;
    let changed = adopt_lineup(agent, entry);
    if changed {
        write_cached_lineups(paths);
        event_hub.broadcast(changed_event(server_id, &current()));
    }
    Some(changed)
}

/// CDXC:AgentProviders 2026-11-20 WHY:
/// The model picker's Fetch latest models (`/api/refreshAgentModelLineup`) reads one agent's lineup now instead of at the next [`LINEUP_TTL`] pass, and reads its `/model` picker again even when the cached read is of the installed version: Antigravity's list follows the account and the server as well as the CLI (Gemini 4 Neon reached `agy models` with 1.5.13 while Ghostex still served the lineup it read on 1.5.8). A changed lineup reaches every client through the usual `agentModelCatalogChanged`. `Err` names an agent whose list is not read from its CLI.
pub(crate) fn refresh_lineup_now(
    agent: &str,
    paths: &GxserverPaths,
    event_hub: &GxserverEventHub,
    server_id: &str,
) -> Result<Value, String> {
    if !crate::agent_model_lineups::RUNTIME_AGENTS.contains(&agent) {
        return Err(format!("{agent}'s model list is not read from its CLI."));
    }
    let outcome = read_and_adopt_lineup(agent, true, paths, event_hub, server_id);
    Ok(json!({
        "agent": agent,
        "answered": outcome.is_some(),
        "changed": outcome == Some(true),
    }))
}

/// Makes sure `agent`'s lineup was read from its CLI (this boot's read, or the cached copy adopted
/// at boot), reading it now when it was not, after any read already under way.
///
/// WHY: Chief of Staff starts on the newest model of the user's default agent, read from this
/// lineup (server/src/chief/model.rs). A new install has no cached copy, and a Chief of Staff opened
/// before the boot read ends would otherwise pick from the bundled list (seen on a fresh test
/// server: GPT 6.1 Sol while the CLI offered 6.2).
pub(crate) fn ensure_lineup(
    agent: &str,
    paths: &GxserverPaths,
    event_hub: &GxserverEventHub,
    server_id: &str,
) {
    if !crate::agent_model_lineups::RUNTIME_AGENTS.contains(&agent) {
        return;
    }
    let known = state()
        .read()
        .map(|state| state.lineups.contains_key(agent))
        .unwrap_or(true);
    if !known {
        let _ = read_and_adopt_lineup(agent, false, paths, event_hub, server_id);
    }
}

/// `/api/refreshAgentModelLineup {agent}`: [`refresh_lineup_now`], answered once the read ends.
pub(crate) async fn handle_refresh_agent_model_lineup_http(
    state: &crate::server::AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> crate::server::RoutedResponse {
    let agent = body
        .pointer("/params/agent")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let (paths, event_hub, server_id) = (
        state.paths.clone(),
        state.event_hub.clone(),
        state.metadata.server_id.clone(),
    );
    let outcome = tokio::task::spawn_blocking(move || {
        refresh_lineup_now(&agent, &paths, &event_hub, &server_id)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|outcome| outcome);
    match outcome {
        Ok(result) => crate::server::routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, result),
        ),
        Err(message) => crate::server::domain_error_response(
            endpoint_path,
            request_id,
            crate::domain::DomainStateError {
                code: "invalidParams",
                message,
            },
        ),
    }
}

/// Reads each CLI's lineup now and again every [`LINEUP_TTL`] for as long as the server runs,
/// telling every client when one changed. A CLI that cannot answer keeps its last lineup.
fn refresh_lineups(paths: GxserverPaths, event_hub: GxserverEventHub, server_id: String) {
    let agents = crate::agent_model_lineups::RUNTIME_AGENTS;
    let mut due = vec![Instant::now(); agents.len()];
    loop {
        for (index, agent) in agents.iter().enumerate() {
            if Instant::now() < due[index] {
                continue;
            }
            let answered =
                read_and_adopt_lineup(agent, false, &paths, &event_hub, &server_id).is_some();
            due[index] = Instant::now() + if answered { LINEUP_TTL } else { LINEUP_RETRY };
        }
        std::thread::sleep(LINEUP_TICK);
    }
}

fn agent_models(catalog: &Value, agent: &str) -> Vec<Value> {
    catalog
        .get("agents")
        .and_then(|agents| agents.get(agent))
        .and_then(|agent| agent.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// One agent's entry in the published file (the newest of the bundle, the cache and GitHub),
/// before any CLI's lineup takes its place.
pub(crate) fn published_agent(agent: &str) -> Option<Value> {
    state()
        .read()
        .ok()?
        .catalog
        .get("agents")?
        .get(agent)
        .cloned()
}

/// One agent's catalog rows, in the catalog's order, plus the agent's own `efforts` and
/// `defaultEffort` for rows that name none.
pub fn catalog_agent(agent: &str) -> Option<Value> {
    current()
        .get("agents")
        .and_then(|agents| agents.get(agent))
        .cloned()
}

/// One agent's catalog row by its dispatch value.
pub fn catalog_model(agent: &str, value: &str) -> Option<Value> {
    let family = ghostex_gx_chat_core::menus::catalog::canonical_model_family(agent);
    agent_models(&current(), family)
        .into_iter()
        .find(|row| row.get("value").and_then(Value::as_str) == Some(value))
}

/// Concrete default `(value, label, effort)` for `family` from the live model catalog:
/// Claude Code -> newest Opus (today Opus 5.5), Codex -> newest Sol (today GPT 6.2 Sol),
/// Antigravity -> newest Neon (today Gemini 4 Neon), other agents -> default/first concrete row.
/// Falls back to a hard-coded current model only when no catalog list exists.
pub fn preferred_default_model(family: &str) -> Option<(String, String, Option<String>)> {
    let canonical = ghostex_gx_chat_core::menus::catalog::canonical_model_family(family);
    if let Some(parsed) =
        ghostex_gx_chat_core::menus::catalog::parse_agent_model_catalog(&current())
    {
        if let Some(found) =
            ghostex_gx_chat_core::menus::catalog::resolved_default_model(&parsed, canonical)
        {
            return Some(found);
        }
    }
    ghostex_gx_chat_core::menus::catalog::preferred_default_model_fallback(canonical).map(
        |(value, label, effort)| {
            (
                value.to_string(),
                label.to_string(),
                effort.map(str::to_string),
            )
        },
    )
}

/// Display label for an effort id (`"xhigh"` -> `"xHigh"`, `"high"` -> `"High"`).
pub fn effort_display_label(effort: &str) -> String {
    let effort = effort.trim();
    if effort.eq_ignore_ascii_case("xhigh") {
        return "xHigh".to_string();
    }
    if let Some(label) = current()
        .pointer(&format!("/effortLabels/{}", effort.to_ascii_lowercase()))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
    {
        return label.to_string();
    }
    let mut chars = effort.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Splits an Antigravity flattened model id (`"gemini-4-neon-high"`) into its catalog base id
/// (`"gemini-4-neon"`) and effort (`"high"`) when the base model is known in the catalog.
pub fn split_antigravity_model_id(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    for effort in ["low", "medium", "high", "xhigh"] {
        if let Some(base) = trimmed.strip_suffix(&format!("-{effort}")) {
            if catalog_model("antigravity", base).is_some() {
                return Some((base.to_string(), effort.to_string()));
            }
        }
    }
    None
}

/// Default reasoning effort for `model_value` on `family` from the live catalog.
/// Returns `None` when the catalog explicitly declares no efforts for that model (e.g. Haiku 4.5).
pub fn default_effort_for_model(family: &str, model_value: &str) -> Option<String> {
    let canonical = ghostex_gx_chat_core::menus::catalog::canonical_model_family(family);
    if let Some(parsed) =
        ghostex_gx_chat_core::menus::catalog::parse_agent_model_catalog(&current())
    {
        if let Some(agent) = parsed.agents.get(canonical) {
            if let Some((model, _)) = agent.resolve_model(model_value) {
                return ghostex_gx_chat_core::menus::catalog::preferred_default_effort(
                    canonical, agent, model,
                );
            }
            if let Some(default_effort) = agent
                .default_effort
                .as_deref()
                .filter(|e| !e.trim().is_empty())
            {
                return Some(default_effort.to_ascii_lowercase());
            }
            if matches!(canonical, "claude" | "codex" | "antigravity")
                && agent.efforts.iter().any(|e| e == "high")
            {
                return Some("high".to_string());
            }
            return agent.efforts.first().cloned();
        }
    }
    ghostex_gx_chat_core::menus::catalog::preferred_default_model_fallback(canonical)
        .and_then(|(_, _, effort)| effort.map(str::to_string))
}

/// Resolves an ACP/Native session's reported model + effort into a concrete catalog-aligned
/// `(value, label, effort)` tuple. Never returns "Default", "Default (recommended)", "Auto",
/// a raw slug like `"claude-opus-5-5"`, or blank.
pub fn resolve_acp_model(
    family: &str,
    raw_model: Option<&str>,
    raw_effort: Option<&str>,
    available_models: Option<&[Value]>,
) -> Option<(String, String, Option<String>)> {
    use ghostex_gx_chat_core::menus::catalog::{
        canonical_model_family, humanize_raw_model_slug, is_placeholder_model_label,
        is_placeholder_model_value, split_parenthesized_effort,
    };
    let canonical = canonical_model_family(family);
    let mut effort = raw_effort
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase());
    let raw = raw_model
        .map(str::trim)
        .filter(|s| !is_placeholder_model_value(s));

    let Some(raw) = raw else {
        let (value, label, default_effort) = preferred_default_model(canonical)?;
        return Some((value, label, effort.or(default_effort)));
    };

    if canonical == "hermes" {
        let label = crate::acp::hermes::strip_hermes_provider_prefix(raw).to_string();
        return Some((raw.to_string(), label, effort));
    }

    // Antigravity ACP reports flattened `<base>-<effort>` ids (e.g. `gemini-4-neon-high`); split
    // them so the model pill and picker match the catalog's base model + effort.
    if canonical == "antigravity" {
        if let Some((base, parsed_effort)) = split_antigravity_model_id(raw) {
            let label = catalog_model("antigravity", &base)
                .and_then(|row| row.get("label").and_then(Value::as_str).map(str::to_string))
                .filter(|label| !is_placeholder_model_label(label))
                .or_else(|| humanize_raw_model_slug(&base))
                .unwrap_or_else(|| base.clone());
            let resolved_effort = effort
                .or(Some(parsed_effort))
                .or_else(|| default_effort_for_model("antigravity", &base));
            return Some((base, label, resolved_effort));
        }
    }

    // Claude Native reports resolved API ids (`claude-opus-5-5`) whereas the catalog keys Opus 5.5
    // as `opus[1m]` (and Sonnet 5.5 as `sonnet`, Haiku 5.5 as `haiku`). Map to the catalog row so
    // the pill shows "Opus 5.5" and the picker selects that row.
    if canonical == "claude" {
        if let Some(row) = catalog_model("claude", raw) {
            let label = row
                .get("label")
                .and_then(Value::as_str)
                .filter(|label| !is_placeholder_model_label(label))
                .map(str::to_string)
                .or_else(|| humanize_raw_model_slug(raw))
                .unwrap_or_else(|| raw.to_string());
            let resolved_effort = effort.or_else(|| default_effort_for_model("claude", raw));
            return Some((raw.to_string(), label, resolved_effort));
        }
        if let Some(mapped) = crate::session_chat_options::claude_transcript_model_choice(raw) {
            let label = catalog_model("claude", &mapped.value)
                .and_then(|row| row.get("label").and_then(Value::as_str).map(str::to_string))
                .filter(|label| !is_placeholder_model_label(label))
                .unwrap_or_else(|| {
                    mapped
                        .label
                        .strip_suffix(" (1M)")
                        .unwrap_or(&mapped.label)
                        .to_string()
                });
            if !is_placeholder_model_label(&label) {
                let resolved_effort =
                    effort.or_else(|| default_effort_for_model("claude", &mapped.value));
                return Some((mapped.value, label, resolved_effort));
            }
        }
    }

    if canonical == "opencode" {
        let available_name = available_models.and_then(|models| {
            models.iter().find_map(|entry| {
                let id = entry
                    .get("modelId")
                    .or_else(|| entry.get("id"))
                    .and_then(Value::as_str)?;
                if id == raw {
                    entry
                        .get("name")
                        .or_else(|| entry.get("displayName"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !is_placeholder_model_label(s))
                        .map(str::to_string)
                } else {
                    None
                }
            })
        });
        let catalog_label = catalog_model("opencode", raw)
            .and_then(|row| row.get("label").and_then(Value::as_str).map(str::to_string))
            .filter(|label| !is_placeholder_model_label(label));
        let label = available_name
            .or(catalog_label)
            .unwrap_or_else(|| raw.split_once('/').map_or(raw, |(_, id)| id).to_string());
        return Some((raw.to_string(), label, effort));
    }

    let available_name = available_models.and_then(|models| {
        models.iter().find_map(|entry| {
            let id = entry
                .get("modelId")
                .or_else(|| entry.get("id"))
                .and_then(Value::as_str)?;
            if id == raw {
                entry
                    .get("name")
                    .or_else(|| entry.get("displayName"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !is_placeholder_model_label(s))
                    .map(str::to_string)
            } else {
                None
            }
        })
    });
    let catalog_label = catalog_model(canonical, raw)
        .and_then(|row| row.get("label").and_then(Value::as_str).map(str::to_string))
        .filter(|label| !is_placeholder_model_label(label));
    let raw_label = catalog_label
        .or(available_name)
        .or_else(|| humanize_raw_model_slug(raw))
        .unwrap_or_else(|| raw.to_string());
    let (clean_label, extracted_effort) = split_parenthesized_effort(&raw_label);
    let label = humanize_raw_model_slug(clean_label).unwrap_or_else(|| clean_label.to_string());
    if effort.is_none() {
        effort = extracted_effort;
    }
    if is_placeholder_model_label(&label) {
        let (value, fallback_label, default_effort) = preferred_default_model(canonical)?;
        return Some((value, fallback_label, effort.or(default_effort)));
    }
    if effort.is_none() {
        effort = default_effort_for_model(canonical, raw);
    }
    Some((raw.to_string(), label, effort))
}

/// Preserves Claude's `[1m]` context-window suffix when a live report (`system/init`, assistant
/// message, or terminal statusline) emits the bare API id (`claude-opus-5-5`) for a session that
/// was already running on `opus[1m]` (or reported a 1M context window).
pub(crate) fn preserve_claude_1m_context(
    existing: Option<&str>,
    candidate: &str,
    window_tokens: Option<u64>,
) -> String {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return existing.unwrap_or_default().trim().to_string();
    }
    if candidate.to_ascii_lowercase().ends_with("[1m]") {
        return candidate.to_string();
    }
    let resolved_candidate = resolve_acp_model("claude", Some(candidate), None, None);
    if let Some(existing_raw) = existing.map(str::trim).filter(|s| !s.is_empty()) {
        if existing_raw.to_ascii_lowercase().ends_with("[1m]") {
            let resolved_existing = resolve_acp_model("claude", Some(existing_raw), None, None);
            let same_model = match (&resolved_existing, &resolved_candidate) {
                (Some((ev, el, _)), Some((cv, cl, _))) => {
                    ev.eq_ignore_ascii_case(cv)
                        || el.eq_ignore_ascii_case(cl)
                        || ev
                            .trim_end_matches("[1m]")
                            .eq_ignore_ascii_case(cv.trim_end_matches("[1m]"))
                }
                _ => false,
            };
            if same_model {
                return existing_raw.to_string();
            }
        }
    }
    if window_tokens.is_some_and(|w| w >= 1_000_000) {
        if let Some((value, _, _)) = &resolved_candidate {
            if value.to_ascii_lowercase().ends_with("[1m]") {
                return value.clone();
            }
            let with_1m = format!("{value}[1m]");
            if catalog_model("claude", &with_1m).is_some() {
                return with_1m;
            }
        }
    }
    if let Some((value, _, _)) = resolved_candidate {
        return value;
    }
    candidate.to_string()
}

/// The name chat shows for a model the terminal printed, when the catalog offers it in a 1M
/// context window: "Opus 5.5 (1M context)" is the `opus[1m]` row labelled "Opus 5.5", and
/// "Opus 5.5" is its 200K twin `opus`, named "Opus 5.5 (200K)" so the two stay apart. `None` for
/// every other name.
pub fn context_window_label(agent: &str, name: &str) -> Option<String> {
    let name = name.trim();
    let (base, long) = match name.strip_suffix(" (1M context)") {
        Some(base) => (base, true),
        None => (name, false),
    };
    let values: Vec<String> = agent_models(&current(), agent)
        .into_iter()
        .filter(|row| row.get("label").and_then(Value::as_str) == Some(base))
        .filter_map(|row| row.get("value").and_then(Value::as_str).map(str::to_string))
        .collect();
    let has_long = values.iter().any(|value| value.ends_with("[1m]"));
    let has_standard = values.iter().any(|value| !value.ends_with("[1m]"));
    if long && has_long {
        Some(base.to_string())
    } else if !long && has_long && has_standard {
        Some(format!("{base} (200K)"))
    } else {
        None
    }
}

/// The dispatch value of the row whose `label`, `pickerLabel` or one of its
/// `terminalLabels` is exactly `name`, the way the agent's footer prints it.
pub fn model_value_for_label(agent: &str, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    agent_models(&current(), agent).into_iter().find_map(|row| {
        let names = ["label", "pickerLabel"]
            .into_iter()
            .filter_map(|key| row.get(key).and_then(Value::as_str))
            .chain(
                row.get("terminalLabels")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            );
        names
            .into_iter()
            .any(|candidate| candidate == name)
            .then(|| row.get("value").and_then(Value::as_str).map(str::to_string))
            .flatten()
    })
}

fn changed_event(server_id: &str, catalog: &Value) -> Value {
    json!({
        "catalog": catalog,
        "catalogRevision": catalog_revision(catalog),
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "type": AGENT_MODEL_CATALOG_CHANGED_EVENT_TYPE,
    })
}

/// A short hash of the catalog's text, which names this lineup on the wire.
fn catalog_revision(catalog: &Value) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(catalog.to_string().as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The event a socket gets as it connects: the effective lineup, which every
/// server announces from boot (see adopt_cached_copy).
///
/// CDXC:SessionChat 2026-11-12 DECISION:
/// Part of the user's "open a chat once, not twice" (Q2720): every new chat socket began with the whole catalog, 79 KiB, so each reconnect and return to the app paid it again. A client that holds the lineup sends its `catalogRevision` when it opens the socket (`catalogRevision=` on `/api/events`, behind the `modelCatalogRevision` capability) and gets no catalog when it matches; a later change is still broadcast to every socket. A client that sends none gets it as before.
pub fn connect_event(server_id: &str, known_revision: Option<&str>) -> Option<Value> {
    let state = state().read().ok()?;
    if !state.published {
        return None;
    }
    if known_revision.is_some_and(|known| known == catalog_revision(&state.effective)) {
        return None;
    }
    Some(changed_event(server_id, &state.effective))
}

fn remote_enabled() -> bool {
    if cfg!(test) {
        return false;
    }
    let value = std::env::var(OPT_OUT_ENV).unwrap_or_default();
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "off" | "false" | "no"
    )
}

fn cache_path(paths: &GxserverPaths) -> PathBuf {
    paths.app_cache_dir.join("agent-model-catalog.json")
}

fn updated_at(catalog: &Value) -> &str {
    catalog
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Adopts the copy an earlier run cached, unless this build bundles a newer
/// one (an app update must not be shadowed by a stale cache). A `ghostex`
/// command calls it too, so it reads the lineup gxserver serves.
pub fn adopt_cached_copy(paths: &GxserverPaths) {
    /*
    CDXC:AgentModelCatalog 2026-10-04 WHY:
    The effective lineup counts as published from boot, not only once a fetch succeeds: a build
    whose bundled catalog is newer than the published file (a release ahead of the catalog repo)
    would otherwise never push it, and a client that connects before the first poll — the fetch
    is delayed and can lose that race — would keep the previous release's lineup for its whole
    session. Clients date-guard what they adopt, so the always-push is safe.
    */
    let cached = fs::read_to_string(cache_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(is_valid_catalog);
    let lineups = read_cached_lineups(paths);
    let Ok(mut state) = state().write() else {
        return;
    };
    if let Some(cached) = cached {
        if updated_at(&cached) >= updated_at(&state.catalog) {
            state.catalog = Arc::new(cached);
        }
    }
    // The lineups the CLIs printed on an earlier run serve until this run reads them again.
    if state.lineups.is_empty() {
        state.lineups = lineups;
    }
    rebuild_effective(&mut state);
    state.published = true;
}

enum Fetched {
    Unchanged,
    Catalog {
        catalog: Value,
        etag: Option<String>,
    },
}

fn fetch_published(etag: Option<&str>) -> Result<Fetched, String> {
    let mut request = crate::http_agent::agent()
        .get(AGENT_MODEL_CATALOG_URL)
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build();
    if let Some(etag) = etag {
        request = request.header("If-None-Match", etag);
    }
    let response = request.call().map_err(|error| match error {
        ureq::Error::StatusCode(status) => format!("HTTP {status}"),
        error => error.to_string(),
    })?;
    if response.status() == 304 {
        return Ok(Fetched::Unchanged);
    }
    let etag = crate::http_agent::header(&response, "ETag").map(str::to_string);
    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_CATALOG_BYTES)
        .read_to_string(&mut body)
        .map_err(|error| format!("read failed: {error}"))?;
    let catalog: Value =
        serde_json::from_str(&body).map_err(|error| format!("not JSON: {error}"))?;
    if !is_valid_catalog(&catalog) {
        return Err("the published catalog does not validate".to_string());
    }
    Ok(Fetched::Catalog { catalog, etag })
}

/// One poll: fetch, and when the document changed adopt it, cache it and
/// tell every connected client. Returns whether the lineup changed.
fn poll_once(
    paths: &GxserverPaths,
    event_hub: &GxserverEventHub,
    server_id: &str,
) -> Result<bool, String> {
    let etag = state().read().ok().and_then(|state| state.etag.clone());
    let (catalog, etag) = match fetch_published(etag.as_deref())? {
        Fetched::Unchanged => return Ok(false),
        Fetched::Catalog { catalog, etag } => (catalog, etag),
    };
    let changed = {
        let mut state = state()
            .write()
            .map_err(|_| "catalog state poisoned".to_string())?;
        state.etag = etag;
        state.published = true;
        // A published copy older than the one in effect (this build's bundle, before main has
        // caught up) never rolls the lineup back; `updatedAt` orders every copy.
        let changed =
            *state.catalog != catalog && updated_at(&catalog) >= updated_at(&state.catalog);
        if changed {
            state.catalog = Arc::new(catalog.clone());
            rebuild_effective(&mut state);
        }
        changed
    };
    if changed {
        if let Ok(text) = serde_json::to_string_pretty(&catalog) {
            let path = cache_path(paths);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let staged = path.with_extension("json.tmp");
            if fs::write(&staged, text).is_ok() {
                let _ = fs::rename(&staged, &path);
            }
        }
        event_hub.broadcast(changed_event(server_id, &current()));
    }
    Ok(changed)
}

/// Loads the cached copy now and polls GitHub on a background thread for as
/// long as the server runs.
pub fn start(
    paths: GxserverPaths,
    event_hub: GxserverEventHub,
    server_id: String,
    logger: Arc<GxserverLogger>,
) {
    adopt_cached_copy(&paths);
    if !cfg!(test) {
        let (paths, event_hub, server_id) = (paths.clone(), event_hub.clone(), server_id.clone());
        let _ = std::thread::Builder::new()
            .name("agent-model-lineups".to_string())
            .spawn(move || refresh_lineups(paths, event_hub, server_id));
    }
    if !remote_enabled() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("agent-model-catalog".to_string())
        .spawn(move || {
            std::thread::sleep(FIRST_POLL_DELAY);
            loop {
                if let Ok(true) = poll_once(&paths, &event_hub, &server_id) {
                    let current = current();
                    let _ = logger.log_routine(
                        DiagnosticLogScenario::ServerLifecycle,
                        GxserverLogInput {
                            level: LogLevel::Info,
                            event: "agentModelCatalogUpdated".to_string(),
                            server_id: None,
                            request_id: None,
                            client: None,
                            duration_ms: None,
                            error: None,
                            details: Some(json!({ "updatedAt": updated_at(&current) })),
                        },
                    );
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        });
}

// ---------------------------------------------------------------------------
// Validation, mirroring `parseAgentModelCatalog`: a document any client would
// reject is never adopted or pushed, so the last good lineup stays in effect.
// ---------------------------------------------------------------------------

fn non_blank(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

fn string_list(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|entries| entries.iter().all(|entry| non_blank(Some(entry))))
}

fn optional_string_list(value: Option<&Value>) -> bool {
    value.is_none_or(string_list)
}

fn is_valid_model(model: &Value, groups: &[&str]) -> bool {
    let Some(model) = model.as_object() else {
        return false;
    };
    non_blank(model.get("value"))
        && non_blank(model.get("label"))
        && optional_string_list(model.get("efforts"))
        && optional_string_list(model.get("terminalLabels"))
        && model
            .get("group")
            .and_then(Value::as_str)
            .filter(|group| !group.trim().is_empty())
            .is_none_or(|group| groups.contains(&group))
}

fn is_valid_agent(agent: &Value) -> bool {
    let Some(agent) = agent.as_object() else {
        return false;
    };
    let groups = match agent.get("groups") {
        None => Vec::new(),
        Some(groups) => {
            let Some(entries) = groups.as_array() else {
                return false;
            };
            let mut ids = Vec::with_capacity(entries.len());
            for entry in entries {
                let Some(id) = entry.get("id").and_then(Value::as_str) else {
                    return false;
                };
                if id.trim().is_empty() || !non_blank(entry.get("label")) || ids.contains(&id) {
                    return false;
                }
                ids.push(id);
            }
            ids
        }
    };
    non_blank(agent.get("name"))
        && agent.get("efforts").is_some_and(string_list)
        && agent
            .get("models")
            .and_then(Value::as_array)
            .is_some_and(|models| models.iter().all(|model| is_valid_model(model, &groups)))
}

fn is_valid_catalog(catalog: &Value) -> bool {
    catalog.get("schemaVersion").and_then(Value::as_f64) == Some(SCHEMA_VERSION as f64)
        && non_blank(catalog.get("updatedAt"))
        && catalog
            .get("effortLabels")
            .and_then(Value::as_object)
            .is_some_and(|labels| labels.values().all(|label| non_blank(Some(label))))
        && catalog
            .get("agents")
            .and_then(Value::as_object)
            .is_some_and(|agents| agents.values().all(is_valid_agent))
}
