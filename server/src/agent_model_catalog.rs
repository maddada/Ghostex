//! The published agent model catalog (`agent-model-catalog.json`), kept
//! current while gxserver runs.
//!
//! CDXC:AgentProviders 2026-09-22 DECISION:
//! User: new models must show in the model menu and the quick picker almost
//! as soon as the catalog is pushed, with a robust combination of every
//! delivery path. gxserver polls the file on GitHub main, keeps the last good
//! copy on disk, and broadcasts `agentModelCatalogChanged` (carrying the whole
//! document) to every event socket, including chat-only ones, and sends the
//! copy it holds to each socket as it connects. The same copy is what screen
//! detection maps footer names through and what the Cursor, Grok and
//! Antigravity picker drivers accept, so a model added to the file is
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
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
    time::Duration,
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

struct CatalogState {
    catalog: Arc<Value>,
    /// True once the copy came from GitHub (fetched now or cached from an
    /// earlier run) rather than from the bundle.
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
        RwLock::new(CatalogState {
            catalog: Arc::new(bundled),
            published: false,
            etag: None,
        })
    })
}

/// The catalog in effect: the last good published copy, else the bundle.
pub fn current() -> Arc<Value> {
    state()
        .read()
        .map(|state| state.catalog.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().catalog.clone())
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

/// The catalog's own name for a model the terminal printed with a " (1M context)" suffix, when
/// the catalog labels its 1M row with the bare name: "Opus 5.5 (1M context)" is the `opus[1m]` row
/// labelled "Opus 5.5", even beside its 200K `opus` twin. A 1M model the catalog does not list
/// under that name keeps its suffix (`None`).
pub fn long_context_label(agent: &str, name: &str) -> Option<String> {
    let base = name.trim().strip_suffix(" (1M context)")?;
    agent_models(&current(), agent)
        .into_iter()
        .any(|row| {
            row.get("label").and_then(Value::as_str) == Some(base)
                && row
                    .get("value")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.ends_with("[1m]"))
        })
        .then(|| base.to_string())
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

/// The event a socket gets as it connects, when this server holds a
/// published copy; a bundle-only server has nothing a client lacks.
pub fn connect_event(server_id: &str) -> Option<Value> {
    let state = state().read().ok()?;
    state
        .published
        .then(|| changed_event(server_id, &state.catalog))
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
/// one (an app update must not be shadowed by a stale cache).
fn adopt_cached_copy(paths: &GxserverPaths) {
    let Some(cached) = fs::read_to_string(cache_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(is_valid_catalog)
    else {
        return;
    };
    let Ok(mut state) = state().write() else {
        return;
    };
    if updated_at(&cached) >= updated_at(&state.catalog) {
        state.catalog = Arc::new(cached);
        state.published = true;
    }
}

enum Fetched {
    Unchanged,
    Catalog {
        catalog: Value,
        etag: Option<String>,
    },
}

fn fetch_published(etag: Option<&str>) -> Result<Fetched, String> {
    let mut request = ureq::get(AGENT_MODEL_CATALOG_URL)
        .set("User-Agent", USER_AGENT)
        .timeout(REQUEST_TIMEOUT);
    if let Some(etag) = etag {
        request = request.set("If-None-Match", etag);
    }
    let response = request.call().map_err(|error| match error {
        ureq::Error::Status(status, _) => format!("HTTP {status}"),
        ureq::Error::Transport(transport) => transport.to_string(),
    })?;
    if response.status() == 304 {
        return Ok(Fetched::Unchanged);
    }
    let etag = response.header("ETag").map(str::to_string);
    let mut body = String::new();
    response
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
        let changed = *state.catalog != catalog;
        if changed {
            state.catalog = Arc::new(catalog.clone());
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
        event_hub.broadcast(changed_event(server_id, &catalog));
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
        && optional_string_list(agent.get("quickPickerOrder"))
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
