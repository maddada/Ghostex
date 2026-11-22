//! Signing an Executor app in to the user's account: Executor's own sign-in page (a form for a
//! key, or the app's OAuth), opened in the browser, then watched until it finishes.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::paths::GxserverPaths;

use super::apps;
use super::client::{self, Endpoint};
use super::live::{lock, notify_changed};

const WATCH_EVERY: Duration = Duration::from_secs(3);
/// Executor's sign-in links last 30 minutes.
const WATCH_FOR: Duration = Duration::from_secs(30 * 60);

/// Account connections being watched.
static WATCHING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// A started sign-in: the page to open, or none when the app needs no (more) sign-in.
pub(crate) struct ExecutorConnectStart {
    pub(crate) name: String,
    pub(crate) url: Option<String>,
}

/// Starts the sign-in for the app's first requirement that has no account yet.
pub(super) fn start(
    paths: &GxserverPaths,
    workspace: &str,
    endpoint: &Endpoint,
    reference: &str,
) -> Result<ExecutorConnectStart, String> {
    let app = apps::find(endpoint, reference)?;
    let name = apps::app_name(&app).to_string();
    let Some((requirement, scope)) = apps::missing_sign_ins(endpoint, &app)?.into_iter().next()
    else {
        return Ok(ExecutorConnectStart { name, url: None });
    };
    let scope = if scope == "app" { "app" } else { "user" };
    let link = client::request(
        endpoint,
        "POST",
        &format!(
            "/account-connect/api/apps/{}/accounts/{scope}/{requirement}/connections",
            apps::app_id(&app)
        ),
        Some(&json!({})),
    )?;
    let connection = link
        .get("connection")
        .and_then(Value::as_str)
        .ok_or("Executor didn't start a sign-in for this app.")?
        .to_string();
    let url = link
        .get("url")
        .and_then(Value::as_str)
        .ok_or("Executor didn't return a sign-in page for this app.")?
        .to_string();
    watch(paths, workspace, endpoint, &connection);
    Ok(ExecutorConnectStart {
        name,
        url: Some(url),
    })
}

/// Checks every few seconds whether the sign-in finished, and announces the change once it did.
fn watch(paths: &GxserverPaths, workspace: &str, endpoint: &Endpoint, connection: &str) {
    if !lock(&WATCHING).insert(connection.to_string()) {
        return;
    }
    let paths = paths.clone();
    let workspace = workspace.to_string();
    let endpoint = endpoint.clone();
    let connection = connection.to_string();
    std::thread::spawn(move || {
        let deadline = Instant::now() + WATCH_FOR;
        while Instant::now() < deadline {
            std::thread::sleep(WATCH_EVERY);
            let Ok(state) = client::request(
                &endpoint,
                "GET",
                &format!("/v1/account-connections/{connection}"),
                None,
            ) else {
                break;
            };
            let status = state
                .pointer("/state/status")
                .and_then(Value::as_str)
                .unwrap_or("pending");
            if status != "pending" {
                if status == "completed" {
                    super::ops::refresh_apps(&paths, &workspace, &endpoint);
                    notify_changed(&workspace);
                }
                break;
            }
        }
        lock(&WATCHING).remove(&connection);
    });
}
