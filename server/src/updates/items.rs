//! The list Settings > Updates draws: one item per thing this computer uses that Ghostex can
//! update, read from the sources their own Settings rows read.
//!
//! An item is `{ id, kind, group, name, icon, detail, installed, latest, summary, state, progress,
//! note, error, canUpdate, canAuto, auto, topBar, counts, inUse, major }`. `state` is one of
//! `available`, `updating`, `failed`, `needsYou`, `checking`, `unknown` (Ghostex can't tell, such as
//! Cursor's dated builds) and `upToDate`. `topBar` says the item lights the chat's top bar badge
//! (Q1362); `counts` is `topBar` with something to do now, or a failure. A failure the user
//! dismissed reads as the state the item would have without it (`settledState`, internal).
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::store;
use crate::agent_cli::catalog::{Definition, CATALOG};
use crate::agent_cli::latest::version_in;
use crate::domain::DomainRepository;
use crate::presentation::presentation_activity;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// How long a collected list is served before the sources are read again.
const LIST_TTL: Duration = Duration::from_secs(10 * 60);
/// How long the extension catalog is reused (it is a download).
const CATALOG_TTL: Duration = Duration::from_secs(60 * 60);
/// An agent used this recently counts as in use (Q1362).
const RECENT_USE: chrono::Duration = chrono::Duration::days(7);

static LIST: std::sync::Mutex<Option<(Instant, Vec<Value>)>> = std::sync::Mutex::new(None);
static COLLECTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Bumped by `invalidate`, so a list collected while an update finished is not kept.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static EXTENSION_CATALOG: std::sync::Mutex<Option<(Instant, HashMap<String, String>)>> =
    std::sync::Mutex::new(None);

/// The next list reads every source again (after an update finished).
pub(crate) fn invalidate() {
    let mut list = LIST.lock().unwrap_or_else(|e| e.into_inner());
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    *list = None;
}

/// The list with what is running, how the last updates ended and the switches laid over it.
pub(super) async fn list(state: &AppState, fresh: bool) -> Vec<Value> {
    let cached = (!fresh)
        .then(|| {
            LIST.lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .filter(|(at, _)| at.elapsed() < LIST_TTL)
                .map(|(_, items)| items.clone())
        })
        .flatten();
    let items = match cached {
        Some(items) => items,
        None => {
            let _collecting = COLLECTING.lock().await;
            let again = (!fresh)
                .then(|| {
                    LIST.lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_ref()
                        .filter(|(at, _)| at.elapsed() < LIST_TTL)
                        .map(|(_, items)| items.clone())
                })
                .flatten();
            match again {
                Some(items) => items,
                None => {
                    let generation = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
                    let items = collect(state, fresh).await;
                    /*
                    CDXC:Updates 2026-11-07 FIX:
                    User: "open code isn't updating pls fix": the top bar's Updates list kept OpenCode at "Waiting…" with a spinner long after its update had ended.
                    WHY: Update's own answer collected the list while OpenCode's agent job was still queued; the job ended (and invalidated the list) during that collect, and the collect then cached the "Waiting…" row for 10 minutes while saying nothing was running, so the desktop stopped asking. A list collected across an invalidate, or showing a row mid-update, is not cached: those rows change within seconds.
                    */
                    let mut list = LIST.lock().unwrap_or_else(|e| e.into_inner());
                    let settled = !items.iter().any(|item| item["state"] == "updating");
                    if settled && GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation
                    {
                        *list = Some((Instant::now(), items.clone()));
                    }
                    items
                }
            }
        }
    };
    items.into_iter().map(|item| overlay(state, item)).collect()
}

fn overlay(state: &AppState, mut item: Value) -> Value {
    let id = item["id"].as_str().unwrap_or_default().to_string();
    if let Some(phase) = store::running_phase(&id) {
        item["state"] = json!("updating");
        item["progress"] = json!(phase);
    } else if item["state"] == "failed"
        && store::dismissed(&id, item["error"].as_str().unwrap_or_default())
    {
        // Dismissed in the top bar's Updates list or on the page: the item reads as it would
        // without the failure until the next update of it runs.
        item["state"] = item["settledState"].clone();
        if !item["state"].is_string() {
            item["state"] = json!("upToDate");
        }
        item["error"] = Value::Null;
    } else if let Some(outcome) = store::outcome(&id) {
        // A failure that was fixed elsewhere (another page, a terminal) no longer shows.
        if matches!(
            item["state"].as_str(),
            Some("available" | "unknown" | "checking")
        ) {
            item["state"] = json!(outcome.state);
            item["error"] = json!(outcome.message);
        } else if item["state"] == "notInstalled" {
            // A failed Install stays in Not installed, saying why, so it never counts as an update.
            item["error"] = json!(outcome.message);
        }
    }
    if let Some(object) = item.as_object_mut() {
        object.remove("settledState");
    }
    if id != "app" {
        item["auto"] = json!(store::auto_enabled(&state.paths, &id));
    }
    let actionable = matches!(
        item["state"].as_str(),
        Some("available" | "failed" | "needsYou")
    );
    // Q1362: what the item's `topBar` names, and any failure.
    item["counts"] = json!(item["state"] == "failed" || (item["topBar"] == true && actionable));
    item
}

/// Reads every source at once.
async fn collect(state: &AppState, fresh: bool) -> Vec<Value> {
    let usage_state = state.clone();
    let usage = tokio::task::spawn_blocking(move || Usage::read(&usage_state))
        .await
        .unwrap_or_default();
    let (agents, helpers, tools, extensions, app) = tokio::join!(
        agent_items(state, &usage),
        helper_items(state, &usage, fresh),
        tool_items(state, &usage, fresh),
        extension_items(state),
        app_item(state),
    );
    let components = component_items();
    let easy_connect_state = state.clone();
    let easy_connect = tokio::task::spawn_blocking(move || easy_connect_item(&easy_connect_state))
        .await
        .ok();
    let mut items = Vec::new();
    items.push(app);
    items.extend(agents);
    items.extend(helpers);
    items.extend(tools);
    items.extend(extensions);
    items.extend(components);
    items.extend(easy_connect);
    items
}

/// The id of the Easy Connect helper's row.
pub(super) const EASY_CONNECT_ITEM: &str = "easyConnect";

/// `v0.6.0` as numbers; `None` for an answer that isn't a version.
fn tailcat_version_parts(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim().trim_start_matches('v');
    let core = core.split(['-', '+', ' ']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ))
}

/*
CDXC:Updates 2026-11-11 DECISION:
User (GitHub #559): "i dont want to bundle tailcat tho since it'll increase size of app. also please add tailcat in updates system so we can click to update it when update comes". The Easy Connect helper is a card like the others: its installed version, the newest one (the version this gxserver installs, `TAILCAT_VERSION`, so a Ghostex release that raises it brings the update), and Update when Ghostex installed the helper Easy Connect hosts with and it is older. A helper Ghostex didn't install (Go, Homebrew, the copy a Linux package ships) is shown and left alone. Not installed puts it in the quiet Not installed part with Install. No "Update automatically": an update restarts the helper, which every connected phone notices for a moment, so it runs only on a click.
SEE-ALSO: server/src/tailcat/install.rs (`start_update`, which keeps the address and the allow-list), server/src/updates/install.rs (`update_easy_connect`).
*/
fn easy_connect_item(state: &AppState) -> Value {
    use crate::tailcat;
    let installation = tailcat::install::installation_status();
    let host = tailcat::resolve_host_tailcat_binary(&state.paths);
    let name = "Easy Connect helper".to_string();
    let detail = "Lets your phones reach this computer from anywhere".to_string();
    let Some(host) = host else {
        let mut item = json!({
            "id": EASY_CONNECT_ITEM,
            "kind": "component",
            "group": "tools",
            "name": name,
            "icon": "easyConnect",
            "detail": detail,
            "installed": null,
            "latest": tailcat::install::TAILCAT_VERSION,
            "major": false,
            "state": "notInstalled",
            "canInstall": true,
            "canUpdate": false,
            "canAuto": false,
            "auto": false,
            "topBar": false,
            "inUse": false,
        });
        if installation.running {
            item["state"] = json!("updating");
            item["progress"] = json!(installation.progress);
        } else if let Some(error) = installation.error {
            item["error"] = json!(error);
        }
        return item;
    };
    let managed = tailcat::binary::managed_tailcat_binary();
    let managed_host = host == managed;
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| ghostex_paths::tailcat::bundled_tailcat_binary(&exe))
        .is_some_and(|bundled| bundled == host);
    let installed = state.tailcat_runtime.binary_version(&host);
    let latest = tailcat::install::TAILCAT_VERSION;
    let older = match (
        installed.as_deref().and_then(tailcat_version_parts),
        tailcat_version_parts(latest),
    ) {
        (Some(installed), Some(latest)) => installed < latest,
        _ => false,
    };
    let mut item = item(
        EASY_CONNECT_ITEM.to_string(),
        "component",
        "tools",
        name,
        "easyConnect",
        detail,
        installed,
        Some(latest.to_string()),
    );
    let can_update = managed_host && older;
    let settled = if can_update { "available" } else { "upToDate" };
    let updating = installation.running && installation.updating;
    let state_word = if updating { "updating" } else { settled };
    if updating {
        item.insert("progress".into(), json!(installation.progress));
    }
    let note = if bundled {
        Some("It comes with Ghostex here and updates with it.".to_string())
    } else if !managed_host {
        Some(format!(
            "Easy Connect uses the helper at {}. Ghostex didn't install it, so it leaves it as it is.",
            host.display()
        ))
    } else if can_update {
        Some("Update restarts the helper once. Paired phones and computers stay paired and reconnect within seconds.".to_string())
    } else {
        None
    };
    item.insert("note".into(), json!(note));
    item.insert("state".into(), json!(state_word));
    item.insert("settledState".into(), json!(settled));
    item.insert("canUpdate".into(), json!(can_update && !updating));
    item.insert("canAuto".into(), json!(false));
    item.insert("topBar".into(), json!(true));
    item.insert("inUse".into(), json!(false));
    Value::Object(item)
}

/// The on-demand extension components gxserver installs itself, while one is installed here:
/// Collab. An app update seals a newer component than the one on disk; Update installs
/// it. (The desktop lists its own: the VS Code view's code-server and CEF, app/updates/items.rs.)
///
/// CDXC:Updates 2026-11-08 DECISION:
/// User (#495, Q1733): every installed extension component with an update shows in the titlebar Updates dropdown and in Settings > Updates, with Update; GBots and Collab were missing. Uninstalled components don't show.
pub(crate) fn component_items() -> Vec<Value> {
    collab_item().into_iter().collect()
}

/// One component's row, from what is on disk and what this release sealed.
#[allow(clippy::too_many_arguments)]
fn component_item(
    name: &str,
    title: &str,
    installed: Option<String>,
    latest: Option<String>,
    phase: &Value,
    available: bool,
    in_use: bool,
) -> Value {
    let mut item = item(
        format!("{COMPONENT_ITEM_PREFIX}{name}"),
        "component",
        "extensions",
        title.to_string(),
        name,
        "Extension".to_string(),
        installed,
        latest,
    );
    let (state, progress, error) = match phase["phase"].as_str() {
        Some("downloading") => {
            let done = phase["doneBytes"].as_f64().unwrap_or(0.0);
            let total = phase["totalBytes"].as_f64().unwrap_or(0.0);
            let progress = if total > 0.0 {
                format!("Downloading\u{2026} {}%", ((done / total) * 100.0).floor())
            } else {
                "Downloading\u{2026}".to_string()
            };
            ("updating", Some(progress), None)
        }
        Some("installing") => ("updating", Some("Installing\u{2026}".to_string()), None),
        Some("failed") if available => (
            "failed",
            None,
            Some(
                phase["error"]
                    .as_str()
                    .unwrap_or("The update failed.")
                    .to_string(),
            ),
        ),
        _ if available => ("available", None, None),
        _ => ("upToDate", None, None),
    };
    item.insert("state".into(), json!(state));
    item.insert(
        "settledState".into(),
        json!(if available { "available" } else { "upToDate" }),
    );
    if let Some(progress) = progress {
        item.insert("progress".into(), json!(progress));
    }
    if let Some(error) = error {
        item.insert("error".into(), json!(error));
    }
    if available && state == "available" {
        item.insert(
            "note".into(),
            json!("This Ghostex brought a newer version. Update installs it."),
        );
    }
    item.insert("canUpdate".into(), json!(true));
    item.insert("topBar".into(), json!(true));
    item.insert("inUse".into(), json!(in_use));
    Value::Object(item)
}

/// The id prefix of [`component_items`]' rows (`component:` is the desktop's own).
pub(super) const COMPONENT_ITEM_PREFIX: &str = "extensionComponent:";

fn collab_item() -> Option<Value> {
    use crate::collab_shim::install;
    let installed = install::installed()?;
    let component = install::item();
    let update = install::update_available();
    Some(component_item(
        "collab",
        install::TITLE,
        Some(installed.version),
        update
            .clone()
            .or_else(|| component["availableVersion"].as_str().map(str::to_string)),
        &component,
        update.is_some(),
        component["running"] == true,
    ))
}

/// Which agents this computer uses: switched on in Settings > Agents, used in the last week, or
/// running now; and which of their sessions are working.
#[derive(Default)]
pub(super) struct Usage {
    enabled: HashSet<String>,
    recent: HashSet<String>,
    /// Catalog agent id to its running sessions, `(projectId, sessionId)`.
    running: HashMap<String, Vec<(String, String)>>,
    working: HashSet<String>,
    any_working: bool,
}

impl Usage {
    pub(super) fn read(state: &AppState) -> Self {
        let mut usage = Usage::default();
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return usage;
        };
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let now = chrono::Utc::now();
        if let (Ok(projects), Ok(last_used)) = (
            repository.list_projects_without_session_history(),
            repository.agent_launcher_last_used(),
        ) {
            let roster = crate::sidebar_hud::read_sidebar_agent_roster(&projects, &last_used);
            for entry in roster.as_array().into_iter().flatten() {
                let agent = entry["agentId"]
                    .as_str()
                    .and_then(catalog_id)
                    .or_else(|| entry["icon"].as_str().and_then(catalog_id));
                let Some(agent) = agent else {
                    continue;
                };
                if entry["enabled"] == true {
                    usage.enabled.insert(agent.to_string());
                }
                let recent = entry["lastUsedAt"]
                    .as_str()
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    .is_some_and(|at| now.signed_duration_since(at) < RECENT_USE);
                if recent {
                    usage.recent.insert(agent.to_string());
                }
            }
        }
        if let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") {
            let now = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            for session in &sessions {
                let working = presentation_activity(session, &now) == "working";
                usage.any_working |= working;
                let Some(agent) = session_agent(session) else {
                    continue;
                };
                if working {
                    usage.working.insert(agent.to_string());
                }
                if let (Some(project), Some(id)) =
                    (session["projectId"].as_str(), session["sessionId"].as_str())
                {
                    usage
                        .running
                        .entry(agent.to_string())
                        .or_default()
                        .push((project.to_string(), id.to_string()));
                }
            }
        }
        usage
    }

    pub(super) fn working(&self, agent: &str) -> bool {
        self.working.contains(agent)
    }

    pub(super) fn any_working(&self) -> bool {
        self.any_working
    }

    pub(super) fn running_sessions(&self, agent: &str) -> Vec<(String, String)> {
        self.running.get(agent).cloned().unwrap_or_default()
    }
}

/// The catalog agent a session runs: its agent id, else (a custom agent) its icon.
fn session_agent(session: &Value) -> Option<&'static str> {
    session["agentId"]
        .as_str()
        .and_then(catalog_id)
        .or_else(|| {
            session
                .pointer("/launchSettings/icon")
                .and_then(Value::as_str)
                .and_then(catalog_id)
        })
        .or_else(|| {
            session
                .pointer("/runtimeSettings/agentName")
                .and_then(Value::as_str)
                .and_then(catalog_id)
        })
}

/// The agent icons whose names differ from their catalog id (`agent-icons/`).
const ICON_ALIASES: [(&str, &str); 5] = [
    ("cursor-cli", "cursor"),
    ("factory-droid", "droid"),
    ("grok-build", "grok"),
    ("amp-cli", "amp"),
    ("antigravity-cli", "antigravity"),
];

/// The catalog agent an agent id or icon names.
pub(super) fn catalog_id(text: &str) -> Option<&'static str> {
    let text = ICON_ALIASES
        .iter()
        .find(|(icon, _)| *icon == text)
        .map_or(text, |(_, id)| id);
    CATALOG
        .iter()
        .find(|definition| definition.agent_id == text)
        .map(|definition| definition.agent_id.as_str())
}

pub(super) fn definition(agent: &str) -> Option<&'static Definition> {
    CATALOG
        .iter()
        .find(|definition| definition.agent_id == agent)
}

/// The icon the desktop draws for a catalog agent (`agent-icons/`).
pub(super) fn agent_icon(agent: &str) -> &str {
    ICON_ALIASES
        .iter()
        .find(|(_, id)| *id == agent)
        .map_or(agent, |(icon, _)| icon)
}

pub(super) fn method_label(method: &str) -> &str {
    match method {
        "native" => "installer",
        "self" => "its own updater",
        "brew" => "Homebrew",
        other => other,
    }
}

fn major(version: &str) -> Option<u64> {
    version.split(['.', '-']).next()?.parse().ok()
}

fn is_major(installed: Option<&str>, latest: Option<&str>) -> bool {
    match (installed.and_then(major), latest.and_then(major)) {
        (Some(installed), Some(latest)) => latest > installed,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn item(
    id: String,
    kind: &str,
    group: &str,
    name: String,
    icon: &str,
    detail: String,
    installed: Option<String>,
    latest: Option<String>,
) -> Map<String, Value> {
    let major = is_major(installed.as_deref(), latest.as_deref());
    let mut item = Map::new();
    item.insert("id".into(), json!(id));
    item.insert("kind".into(), json!(kind));
    item.insert("group".into(), json!(group));
    item.insert("name".into(), json!(name));
    item.insert("icon".into(), json!(icon));
    item.insert("detail".into(), json!(detail));
    item.insert("installed".into(), json!(installed));
    item.insert("latest".into(), json!(latest));
    item.insert("major".into(), json!(major));
    item.insert("canAuto".into(), json!(true));
    item.insert("auto".into(), json!(false));
    item
}

fn job_active(job: &Value) -> bool {
    matches!(job["status"].as_str(), Some("queued" | "running"))
}

async fn agent_items(state: &AppState, usage: &Usage) -> Vec<Value> {
    let mut params = Map::new();
    params.insert("action".into(), json!("list"));
    let Ok(answer) = crate::agent_cli::endpoint::dispatch(&state.paths, &params).await else {
        return Vec::new();
    };
    answer["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|agent| {
            agent_item(agent, usage).or_else(|| super::missing::missing_agent_item(agent))
        })
        .collect()
}

fn agent_item(agent: &Value, usage: &Usage) -> Option<Value> {
    let id = agent["agentId"].as_str()?;
    agent["executablePath"].as_str()?;
    let in_use = usage.recent.contains(id) || usage.running.contains_key(id);
    if !usage.enabled.contains(id) && !in_use {
        return None;
    }
    let definition = definition(id)?;
    let name = agent["name"]
        .as_str()
        .unwrap_or(&definition.binary)
        .to_string();
    let version_line = agent["version"].as_str();
    let installed = version_line.map(|line| version_in(line).unwrap_or_else(|| line.to_string()));
    let latest = agent["updateInstallable"]
        .as_str()
        .or(agent["latestVersion"].as_str())
        .map(str::to_string);
    let method = agent["detectedMethodId"].as_str();
    let job = &agent["job"];
    let held_back = agent["updateHeldBack"].as_str();
    let available = agent["updateAvailable"] == true && held_back.is_none();
    let can_tell = definition.latest_version.is_some()
        || definition.npm_package.is_some()
        || definition.update_check.is_some();
    // A CLI that updates itself (Antigravity) has no update to run, unless mise manages it.
    let updates_itself = definition.updates_itself() && method != Some("mise");
    let can_run = !updates_itself && (method.is_some() || definition.self_updater().is_some());
    let detail = match method {
        Some(method) => format!("Agent · {}", method_label(method)),
        None => "Agent".to_string(),
    };
    let mut item = item(
        format!("agent:{id}"),
        "agent",
        "agents",
        name.clone(),
        agent_icon(id),
        detail,
        installed,
        latest,
    );
    let failed = job["status"] == "failed" && job["operation"] == "update";
    let settled = if available {
        "available"
    } else if agent["updateChecking"] == true {
        "checking"
    } else if !can_tell || version_line.is_none() {
        // A CLI that didn't say its version can't be called up to date.
        "unknown"
    } else {
        "upToDate"
    };
    let state = if job_active(job) {
        "updating"
    } else if failed {
        "failed"
    } else {
        settled
    };
    if job_active(job) {
        item.insert(
            "progress".into(),
            json!(if job["status"] == "queued" {
                "Waiting…"
            } else {
                "Updating…"
            }),
        );
    }
    if failed {
        item.insert(
            "error".into(),
            json!(job["error"].as_str().unwrap_or("The update failed.")),
        );
        item.insert("settledState".into(), json!(settled));
    }
    if let Some(summary) = agent["updateSummary"].as_str() {
        item.insert("summary".into(), json!(summary));
    }
    let sessions = usage.running.get(id).map_or(0, Vec::len);
    let note = if let Some(held_back) = held_back {
        Some(held_back.to_string())
    } else if updates_itself {
        Some(
            "Updates itself while it runs; start a new session to use the newest version."
                .to_string(),
        )
    } else if available && definition.update_stops_sessions {
        Some(format!(
            "Its updater stops running {name} sessions, so it waits until they're idle."
        ))
    } else if let Some(note) = agent["updateNote"].as_str() {
        Some(note.to_string())
    } else if available && sessions > 0 {
        Some(if sessions == 1 {
            "1 session restarts on it when idle".to_string()
        } else {
            format!("{sessions} sessions restart on it when idle")
        })
    } else {
        None
    };
    item.insert("note".into(), json!(note));
    item.insert("state".into(), json!(state));
    item.insert("canUpdate".into(), json!(can_run && state != "updating"));
    item.insert("canAuto".into(), json!(can_run && can_tell));
    item.insert("topBar".into(), json!(in_use));
    item.insert("inUse".into(), json!(usage.working(id)));
    Some(Value::Object(item))
}

async fn helper_items(state: &AppState, usage: &Usage, fresh: bool) -> Vec<Value> {
    let state = state.clone();
    let answer = tokio::task::spawn_blocking(move || {
        let mut params = Map::new();
        params.insert("operation".into(), json!("helperStatus"));
        params.insert("fresh".into(), json!(fresh));
        crate::accounts::endpoint::dispatch(&state, &params)
    })
    .await;
    let Ok(Ok(answer)) = answer else {
        return Vec::new();
    };
    answer["helperTools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            helper_item(tool, usage).or_else(|| super::missing::missing_helper_item(tool))
        })
        .collect()
}

fn helper_item(tool: &Value, usage: &Usage) -> Option<Value> {
    if tool["installed"] != true {
        return None;
    }
    let provider = tool["provider"].as_str()?;
    let (name, icon) = match provider {
        "claude" => ("Claude Swap", "claude-swap"),
        "codex" => ("Codex Swap", "codex-swap"),
        _ => return None,
    };
    let detail = match tool["installMethod"].as_str() {
        Some(method) => format!("Account switcher · {}", method_label(method)),
        None => "Account switcher".to_string(),
    };
    let mut item = item(
        format!("helper:{provider}"),
        "helper",
        "tools",
        name.to_string(),
        icon,
        detail,
        tool["version"].as_str().map(str::to_string),
        tool["latestVersion"].as_str().map(str::to_string),
    );
    let job = &tool["job"];
    let can_update = tool["actions"]
        .as_array()
        .is_some_and(|actions| actions.iter().any(|action| action == "update"));
    let failed = job["status"] == "failed" && job["action"] == "update";
    let settled = if tool["pendingSwitch"].is_object() {
        "checking"
    } else if tool["updateAvailable"] == true {
        "available"
    } else if tool["checkError"].is_string() {
        "unknown"
    } else {
        "upToDate"
    };
    let state = if job["status"] == "running" {
        "updating"
    } else if failed {
        "failed"
    } else {
        settled
    };
    if failed {
        item.insert(
            "error".into(),
            json!(job["error"].as_str().unwrap_or("The update failed.")),
        );
        item.insert("settledState".into(), json!(settled));
    }
    let note = tool
        .pointer("/pendingSwitch/message")
        .or(tool.get("checkError"))
        .and_then(Value::as_str);
    item.insert("note".into(), json!(note));
    item.insert("state".into(), json!(state));
    item.insert("canUpdate".into(), json!(can_update && state != "updating"));
    item.insert("canAuto".into(), json!(can_update));
    item.insert("topBar".into(), json!(true));
    item.insert("inUse".into(), json!(usage.working(provider)));
    Some(Value::Object(item))
}

async fn tool_items(state: &AppState, usage: &Usage, fresh: bool) -> Vec<Value> {
    let mut params = Map::new();
    params.insert("action".into(), json!("list"));
    params.insert("fresh".into(), json!(fresh));
    params.insert(
        "includeDesktopControl".into(),
        json!(cfg!(target_os = "macos")),
    );
    let Ok(answer) = crate::managed_tools::endpoint::dispatch(&state.paths, &params).await else {
        return Vec::new();
    };
    let hex_wanted = crate::voice_agent::dictate_uses_hex(&state.paths);
    answer["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            tool_item(tool, usage, hex_wanted).or_else(|| {
                let dictate_hex = hex_wanted && tool["id"] == "hex";
                super::missing::missing_tool_item(tool, dictate_hex)
            })
        })
        .collect()
}

/// `hex_wanted`: Dictate transcribes with Hex here, so Hex is listed (and lights the top bar)
/// even before it is installed, with Install as its update.
pub(super) fn tool_item(tool: &Value, usage: &Usage, hex_wanted: bool) -> Option<Value> {
    let id = tool["id"].as_str()?;
    let hex = id == "hex";
    if hex && tool["supported"] != true {
        return None;
    }
    if tool["installed"] != true && !(hex && hex_wanted) {
        return None;
    }
    let can_update = tool["actions"]
        .as_array()
        .is_some_and(|actions| actions.iter().any(|action| action == "update"));
    let can_install = tool["actions"]
        .as_array()
        .is_some_and(|actions| actions.iter().any(|action| action == "install"));
    let desktop_control = matches!(id, "trycua" | "spaceo");
    // Only what Ghostex installed is Ghostex's to update; cua-driver, SpaceO and Hex always are.
    if !desktop_control && !hex && !(can_update && tool["source"] == "ghostex") {
        return None;
    }
    let detail = match id {
        "trycua" => "Tool · cua-driver".to_string(),
        "hex" => "Dictation app".to_string(),
        _ => "Tool".to_string(),
    };
    let mut item = item(
        format!("tool:{id}"),
        "tool",
        "tools",
        tool["label"].as_str().unwrap_or(id).to_string(),
        id,
        detail,
        tool["version"].as_str().map(str::to_string),
        tool["latestVersion"].as_str().map(str::to_string),
    );
    let job = &tool["job"];
    let failed = job["status"] == "failed"
        && (job["operation"] == "update" || (hex && job["operation"] == "install"));
    let missing = tool["installed"] != true;
    let settled = if tool["updateAvailable"] == true || (missing && can_install) {
        "available"
    } else if tool["checkError"].is_string() || tool["notAnswering"].is_string() {
        "unknown"
    } else {
        "upToDate"
    };
    let state = if job_active(job) {
        "updating"
    } else if failed {
        "failed"
    } else {
        settled
    };
    if failed {
        item.insert(
            "error".into(),
            json!(job["error"].as_str().unwrap_or("The update failed.")),
        );
        item.insert("settledState".into(), json!(settled));
    }
    let note = if missing && state == "available" {
        Some("Not installed. Dictate is set to use Hex.")
    } else if hex && state == "available" {
        tool["detail"].as_str()
    } else if tool["needsPassword"] == true && state == "available" {
        Some("Asks for your password.")
    } else if state == "available" {
        tool["permissionNote"].as_str()
    } else {
        tool["notAnswering"].as_str()
    };
    item.insert("note".into(), json!(note));
    item.insert("state".into(), json!(state));
    item.insert(
        "canUpdate".into(),
        json!((can_update || (missing && can_install)) && state != "updating"),
    );
    // Hex installs and updates only when the user clicks (CDXC:Dictate 2026-11-07 in managed_tools/hex.rs).
    item.insert(
        "canAuto".into(),
        json!(can_update && !hex && tool["needsPassword"] != true),
    );
    item.insert(
        "topBar".into(),
        json!(desktop_control || (hex && hex_wanted)),
    );
    item.insert("inUse".into(), json!(usage.any_working()));
    Some(Value::Object(item))
}

/// The newest catalog version of each extension, reused for an hour.
fn extension_catalog_versions(state: &AppState) -> HashMap<String, String> {
    if let Some((at, versions)) = EXTENSION_CATALOG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        if at.elapsed() < CATALOG_TTL {
            return versions.clone();
        }
    }
    let Ok(snapshot) = state.extension_registry.catalog() else {
        return HashMap::new();
    };
    let versions: HashMap<String, String> = snapshot
        .catalog
        .extensions
        .into_iter()
        .map(|entry| (entry.manifest.name, entry.manifest.version))
        .collect();
    *EXTENSION_CATALOG.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((Instant::now(), versions.clone()));
    versions
}

async fn extension_items(state: &AppState) -> Vec<Value> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let Ok(installed) = state.extension_registry.list() else {
            return Vec::new();
        };
        let versions = extension_catalog_versions(&state);
        installed
            .into_iter()
            .filter(|extension| extension.state.enabled)
            .filter_map(|extension| {
                let latest = versions.get(&extension.id)?.clone();
                let current = extension.manifest.version.clone();
                let available = crate::managed_tools::tools::newer(&latest, &current);
                let mut item = item(
                    format!("extension:{}", extension.id),
                    "extension",
                    "extensions",
                    extension.manifest.title.clone(),
                    "extension",
                    "Extension".to_string(),
                    Some(current),
                    Some(latest),
                );
                item.insert(
                    "state".into(),
                    json!(if available { "available" } else { "upToDate" }),
                );
                item.insert("canUpdate".into(), json!(true));
                item.insert("topBar".into(), json!(false));
                item.insert("inUse".into(), json!(false));
                Some(Value::Object(item))
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

async fn app_item(state: &AppState) -> Value {
    let status = crate::app_update::current_app_update_status(state).await;
    let text = |key: &str| {
        status[key]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let mut item = item(
        "app".to_string(),
        "app",
        "ghostex",
        "Ghostex".to_string(),
        "ghostex",
        "This computer".to_string(),
        text("appVersion").or_else(|| text("version")),
        text("availableVersion"),
    );
    let supported = status["supported"] == true;
    let phase = status["phase"].as_str().unwrap_or("idle");
    let available = status["updateAvailable"] == true;
    let progress = status["progress"].as_f64();
    let settled = match phase {
        _ if available => "available",
        "appNotRunning" => "unknown",
        _ => "upToDate",
    };
    let state = match phase {
        "downloading" | "installing" | "restarting" => "updating",
        "failed" => "failed",
        _ => settled,
    };
    if state == "updating" {
        item.insert(
            "progress".into(),
            json!(match (phase, progress) {
                ("downloading", Some(progress)) =>
                    format!("Downloading {}%", (progress * 100.0).round() as i64),
                ("downloading", None) => "Downloading…".to_string(),
                ("installing", _) => "Installing…".to_string(),
                _ => "Restarting Ghostex…".to_string(),
            }),
        );
    }
    if state == "failed" {
        item.insert(
            "error".into(),
            json!(text("message").unwrap_or_else(|| "The update failed.".to_string())),
        );
        item.insert("settledState".into(), json!(settled));
    }
    let note = if available && !supported {
        text("unsupportedReason")
    } else if available {
        // Short enough for two lines of Settings > Updates' card (the remote tile audit, finding 9:
        // the longer note ended "your agents keep runni…").
        Some("Restarts Ghostex once; agents keep running.".to_string())
    } else {
        None
    };
    item.insert("note".into(), json!(note));
    item.insert("state".into(), json!(state));
    item.insert(
        "canUpdate".into(),
        json!(supported && available && state != "updating"),
    );
    item.insert("canAuto".into(), json!(supported));
    item.insert("auto".into(), json!(status["autoUpdate"] == true));
    item.insert("topBar".into(), json!(true));
    item.insert("inUse".into(), json!(false));
    Value::Object(item)
}

#[cfg(test)]
mod hex_tests {
    use super::{tool_item, Usage};
    use serde_json::json;

    fn hex(installed: bool, actions: &[&str], extra: serde_json::Value) -> serde_json::Value {
        let mut tool = json!({
            "id": "hex", "label": "Hex", "supported": true, "installed": installed,
            "actions": actions, "source": "system", "latestVersion": "3.3.0",
        });
        for (key, value) in extra.as_object().unwrap() {
            tool[key] = value.clone();
        }
        tool
    }

    #[test]
    fn a_missing_hex_is_listed_with_install_only_when_dictate_uses_it() {
        let usage = Usage::default();
        assert!(tool_item(&hex(false, &["install"], json!({})), &usage, false).is_none());
        let item = tool_item(&hex(false, &["install"], json!({})), &usage, true).expect("listed");
        assert_eq!(item["state"], "available");
        assert_eq!(item["canUpdate"], true);
        assert_eq!(item["canAuto"], false, "Hex installs only on a click");
        assert_eq!(item["topBar"], true);
        assert_eq!(item["latest"], "3.3.0");
        assert_eq!(item["note"], "Not installed. Dictate is set to use Hex.");
    }

    #[test]
    fn an_installed_hex_shows_its_version_and_a_newer_release() {
        let usage = Usage::default();
        let tool = hex(
            true,
            &["update"],
            json!({"version": "3.2.0", "updateAvailable": true}),
        );
        let item = tool_item(&tool, &usage, false).expect("listed");
        assert_eq!(item["installed"], "3.2.0");
        assert_eq!(item["latest"], "3.3.0");
        assert_eq!(item["state"], "available");
        assert_eq!(item["canUpdate"], true);
        assert_eq!(item["canAuto"], false);
        assert_eq!(
            item["topBar"], false,
            "only lights the top bar when Dictate uses Hex"
        );
        let current = hex(
            true,
            &["update"],
            json!({"version": "3.3.0", "updateAvailable": false}),
        );
        assert_eq!(
            tool_item(&current, &usage, true).unwrap()["state"],
            "upToDate"
        );
    }

    #[test]
    fn hex_where_hex_3_does_not_ship_is_not_listed() {
        let mut tool = hex(false, &[], json!({}));
        tool["supported"] = json!(false);
        assert!(tool_item(&tool, &Usage::default(), true).is_none());
    }
}
