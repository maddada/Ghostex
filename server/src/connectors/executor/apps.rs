//! A workspace's Executor apps: listing them with whether they still need a sign-in, finding and
//! adding apps from Executor's catalog or by MCP server URL, and removing them.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::client::{self, Endpoint};
use super::live::lock;

/// The owner of Executor's own app-management app, which Ghostex neither lists nor calls.
const BUILT_IN_OWNER: &str = "executor-local";
const CATALOG_FRESH_FOR: Duration = Duration::from_secs(60 * 60);
const DESCRIPTION_LIMIT: usize = 160;

/// Executor's catalog (about 1,800 apps), read once an hour; the same feed for every workspace.
static CATALOG: Mutex<Option<(Vec<Value>, Instant)>> = Mutex::new(None);

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn clip(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((index, _)) => format!("{}…", &text[..index]),
        None => text.to_string(),
    }
}

/// The apps of the workspace's Executor, Executor's own app left out.
pub(super) fn all(endpoint: &Endpoint) -> Result<Vec<Value>, String> {
    let apps = client::request(endpoint, "GET", "/v1/apps", None)?;
    Ok(apps
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|app| text(app, "owner") != BUILT_IN_OWNER)
        .collect())
}

/// The account requirements an app declares: `(name, scope)`.
fn requirements(app: &Value) -> Vec<(String, String)> {
    app.pointer("/requirements/accounts")
        .and_then(Value::as_object)
        .map(|accounts| {
            accounts
                .iter()
                .map(|(name, requirement)| (name.clone(), text(requirement, "scope").to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// The requirements that have no linked account yet.
pub(super) fn missing_sign_ins(
    endpoint: &Endpoint,
    app: &Value,
) -> Result<Vec<(String, String)>, String> {
    let needed = requirements(app);
    let mut linked: Vec<String> = Vec::new();
    for scope in ["user", "app"] {
        if !needed.iter().any(|(_, needed_scope)| needed_scope == scope) {
            continue;
        }
        let links = client::request(
            endpoint,
            "GET",
            &format!("/dashboard/api/apps/{}/accounts/{scope}", text(app, "id")),
            None,
        )?;
        linked.extend(
            links
                .as_array()
                .into_iter()
                .flatten()
                .map(|link| text(link, "requirement").to_string()),
        );
    }
    Ok(needed
        .into_iter()
        .filter(|(name, _)| !linked.contains(name))
        .collect())
}

/// The workspace's apps as Settings shows them.
pub(super) fn list(endpoint: &Endpoint) -> Result<Vec<Value>, String> {
    all(endpoint)?
        .iter()
        .map(|app| {
            let missing = missing_sign_ins(endpoint, app)?;
            Ok(json!({
                "id": text(app, "id"),
                "slug": text(app, "slug"),
                "name": text(app, "name"),
                "signedIn": missing.is_empty(),
                "needsSignIn": missing.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            }))
        })
        .collect()
}

/// Letters and digits only, lowercased: "Hacker News", "hacker-news" and "hackernews" match.
fn loose(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The app a reference names: its id, slug, name or catalog id, ignoring case, spaces and dashes.
pub(super) fn find(endpoint: &Endpoint, reference: &str) -> Result<Value, String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err("Name the app, by its name or slug.".to_string());
    }
    let wanted = loose(reference);
    let apps = all(endpoint)?;
    let catalog_id = |app: &Value| {
        app.pointer("/copiedFrom/reference")
            .and_then(Value::as_str)
            .and_then(|reference| reference.rsplit('/').next())
            .and_then(|file| file.strip_suffix(".json"))
            .map(loose)
            .unwrap_or_default()
    };
    apps.iter()
        .find(|app| text(app, "id") == reference)
        .or_else(|| {
            apps.iter().find(|app| {
                loose(text(app, "slug")) == wanted
                    || loose(text(app, "name")) == wanted
                    || catalog_id(app) == wanted
            })
        })
        .cloned()
        .ok_or_else(|| {
            format!("This workspace's Executor has no app called \"{reference}\". Add it first.")
        })
}

fn catalog(endpoint: &Endpoint) -> Result<Vec<Value>, String> {
    if let Some((entries, read_at)) = lock(&CATALOG).as_ref() {
        if read_at.elapsed() < CATALOG_FRESH_FOR {
            return Ok(entries.clone());
        }
    }
    let entries = client::request(endpoint, "GET", "/dashboard/api/catalog", None)?
        .as_array()
        .cloned()
        .unwrap_or_default();
    *lock(&CATALOG) = Some((entries.clone(), Instant::now()));
    Ok(entries)
}

fn catalog_row(entry: &Value) -> Value {
    let auth = entry
        .get("auth")
        .and_then(Value::as_array)
        .map(|methods| {
            methods
                .iter()
                .map(|method| text(method, "kind").to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "id": text(entry, "id"),
        "name": text(entry, "name"),
        "category": text(entry, "category"),
        "description": clip(text(entry, "description"), DESCRIPTION_LIMIT),
        "signIn": auth,
    })
}

/// Catalog apps matching `query` (name, id, category or description), best matches first.
pub(super) fn search_catalog(
    endpoint: &Endpoint,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, String> {
    let query = query.trim().to_lowercase();
    let mut ranked: Vec<(u8, Value)> = catalog(endpoint)?
        .into_iter()
        .filter_map(|entry| {
            let id = text(&entry, "id").to_lowercase();
            let name = text(&entry, "name").to_lowercase();
            let rank = if query.is_empty() {
                3
            } else if id == query || name == query {
                0
            } else if name.starts_with(&query) || id.starts_with(&query) {
                1
            } else if name.contains(&query) || id.contains(&query) {
                2
            } else if text(&entry, "category").to_lowercase().contains(&query)
                || text(&entry, "description").to_lowercase().contains(&query)
            {
                3
            } else {
                return None;
            };
            Some((rank, entry))
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    Ok(ranked
        .iter()
        .take(limit)
        .map(|(_, entry)| catalog_row(entry))
        .collect())
}

/// Adds a catalog app (by its catalog id or exact name) and returns the new app.
pub(super) fn add_from_catalog(endpoint: &Endpoint, reference: &str) -> Result<Value, String> {
    let lower = reference.trim().to_lowercase();
    let entry = catalog(endpoint)?
        .into_iter()
        .find(|entry| {
            text(entry, "id").to_lowercase() == lower || text(entry, "name").to_lowercase() == lower
        })
        .ok_or_else(|| {
            format!(
                "Executor's catalog has no app called \"{}\". Search it with `ghostex connectors executor catalog <words>`.",
                reference.trim()
            )
        })?;
    client::request_slow(
        endpoint,
        "POST",
        "/dashboard/api/catalog/import",
        Some(&json!({ "entry": text(&entry, "id"), "name": text(&entry, "name") })),
    )
}

/// Adds a remote MCP server by its URL (Executor can't start local command servers).
pub(super) fn add_mcp_server(endpoint: &Endpoint, name: &str, url: &str) -> Result<Value, String> {
    let name = name.trim();
    let url = url.trim();
    if name.is_empty() {
        return Err("Name the MCP server.".to_string());
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(
            "Give the MCP server's web address (https://…). Executor can't start servers that run as a local command."
                .to_string(),
        );
    }
    client::request_slow(
        endpoint,
        "POST",
        "/dashboard/api/apps/import",
        Some(&json!({ "source": { "kind": "mcp", "name": name, "url": url } })),
    )
}

/// Removes an app from the workspace's Executor.
pub(super) fn remove(endpoint: &Endpoint, app: &Value) -> Result<(), String> {
    client::request(
        endpoint,
        "DELETE",
        &format!("/v1/apps/{}", text(app, "id")),
        None,
    )
    .map(|_| ())
}

pub(super) fn app_id(app: &Value) -> &str {
    text(app, "id")
}

pub(super) fn app_name(app: &Value) -> &str {
    text(app, "name")
}

pub(super) fn app_slug(app: &Value) -> &str {
    text(app, "slug")
}
