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
        let bundled = Arc::new(bundled);
        RwLock::new(CatalogState {
            catalog: bundled.clone(),
            effective: bundled,
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
    if state.lineups.is_empty() {
        state.effective = state.catalog.clone();
        return;
    }
    let mut effective = (*state.catalog).clone();
    if let Some(agents) = effective.get_mut("agents").and_then(Value::as_object_mut) {
        for (agent, lineup) in &state.lineups {
            agents.insert(agent.clone(), lineup.entry.clone());
        }
    }
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
            let entry = crate::agent_model_lineups::read_lineup(agent).filter(is_valid_agent);
            due[index] = Instant::now()
                + if entry.is_some() {
                    LINEUP_TTL
                } else {
                    LINEUP_RETRY
                };
            if entry.is_some_and(|entry| adopt_lineup(agent, entry)) {
                write_cached_lineups(&paths);
                event_hub.broadcast(changed_event(&server_id, &current()));
            }
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

/// One agent's catalog row by its dispatch value.
pub fn catalog_model(agent: &str, value: &str) -> Option<Value> {
    agent_models(&current(), agent)
        .into_iter()
        .find(|row| row.get("value").and_then(Value::as_str) == Some(value))
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
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "type": AGENT_MODEL_CATALOG_CHANGED_EVENT_TYPE,
    })
}

/// The event a socket gets as it connects: the effective lineup, which every
/// server announces from boot (see adopt_cached_copy).
pub fn connect_event(server_id: &str) -> Option<Value> {
    let state = state().read().ok()?;
    state
        .published
        .then(|| changed_event(server_id, &state.effective))
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
