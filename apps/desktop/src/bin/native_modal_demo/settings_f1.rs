//! Preview fixtures for the Extensions and Accounts Settings pages: the settings and hydrate fields
//! they read (`customViews`, `hud.projectViewProjects`, `hud.projectViewSpaces`), a scripted
//! gxserver for `/api/listExtensions`, `/api/extensionsCatalog`, `/api/extensionStoreContent` (a
//! Store entry's README and CHANGELOG), the extension state calls and `/api/agentAccounts`, and the
//! icons the page fetches. The
//! fixtures are the ones the React reference screenshots were taken with
//! (settings_fixture/extensions.json and accounts.json).
//!
//! States: `extensions` (the page), `extensions-store` (scrolled to the store), `extensions-views`
//! (scrolled to Your views), `extensions-filter` (a filtered page), `extensions-experimental` (the
//! Experimental group alone), `extensions-empty` (a filter
//! nothing matches), `extensions-loading`, `extensions-error`, `extensions-detail` (an installed
//! extension's page), `extensions-store-detail` (a Store page with its README), `extensions-consent`
//! (the install consent over the Store page), `extensions-folder-install` (Install from folder…'s
//! consent, nothing ticked), `extensions-view-editor` (the Configure view deep
//! link), `extensions-templates` (Add view), `extensions-scope` (the Choose where it's shown deep
//! link), `extensions-scope-menu` (its picker open), `extensions-arrange` (Arrange views), `extensions-delete-view` (Delete
//! confirmation over the Configure view deep link),
//! `extensions-select` (the type filter open), `extensions-offline` (no gxserver); `accounts`,
//! `accounts-editor` (an account's pane), `accounts-advanced` (its Advanced open),
//! `accounts-editor-actions` (scrolled to its buttons), `accounts-remove` (the Remove
//! confirmation), `accounts-needs-sign-in` (an account whose login stopped working),
//! `accounts-disabled` (an account the user turned off),
//! `accounts-reconnect` (its sign-in waiting), `accounts-code` (waiting with the code box and
//! Details open), `accounts-connected`, `accounts-wrong` (signed in to another account),
//! `accounts-blockers` (Codex sessions to pause), `accounts-defaults` / `accounts-rules` (Claude
//! rules), `accounts-add`, `accounts-add-codex`, `accounts-add-waiting`, `accounts-menu` (the
//! helper's ⋯ menu), `accounts-guide`, `accounts-connect` (a sign-in in progress),
//! `accounts-uninstall`, `accounts-error`, `accounts-loading`, `accounts-empty` (no saved
//! accounts).
use serde_json::{Map, Value, json};
use std::cell::RefCell;

const EXTENSIONS: &str = include_str!("settings_fixture/extensions.json");
const ACCOUNTS: &str = include_str!("settings_fixture/accounts.json");

thread_local! {
    /// The installed list as the scripted gxserver keeps it, so toggles stick.
    static INSTALLED: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
    static ACCOUNT_STATE: RefCell<Option<Value>> = const { RefCell::new(None) };
}

fn extensions() -> Value {
    serde_json::from_str(EXTENSIONS).unwrap_or(Value::Null)
}

fn accounts() -> Value {
    ACCOUNT_STATE.with(|state| {
        state
            .borrow_mut()
            .get_or_insert_with(|| {
                let mut data: Value = serde_json::from_str(ACCOUNTS).unwrap_or(Value::Null);
                relative_resets(&mut data);
                data
            })
            .clone()
    })
}

/// The fixture writes reset times as `in:<minutes>` so the page always shows "resets in 2h 14m".
fn relative_resets(data: &mut Value) {
    let Some(accounts) = data["accounts"].as_array_mut() else {
        return;
    };
    for window in accounts
        .iter_mut()
        .filter_map(|account| account["usage"].as_array_mut())
        .flatten()
    {
        if let Some(minutes) = window["resetsAt"]
            .as_str()
            .and_then(|reset| reset.strip_prefix("in:"))
            .and_then(|minutes| minutes.parse::<i64>().ok())
        {
            let at = chrono::Utc::now()
                + chrono::Duration::minutes(minutes)
                + chrono::Duration::seconds(30);
            window["resetsAt"] = json!(at.to_rfc3339());
        }
    }
}

fn installed() -> Vec<Value> {
    INSTALLED.with(|list| {
        list.borrow_mut()
            .get_or_insert_with(|| {
                extensions()["installed"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .clone()
    })
}

fn set_installed(next: Vec<Value>) {
    INSTALLED.with(|list| *list.borrow_mut() = Some(next));
}

/// Whether a state belongs to these pages.
pub(super) fn owns(state: &str) -> bool {
    state.starts_with("extensions") || state.starts_with("accounts")
}

pub(super) fn story_settings(state: &str, settings: &mut Map<String, Value>) {
    if !owns(state) {
        return;
    }
    let fixture = extensions();
    settings.insert("customViews".into(), fixture["customViews"].clone());
    if matches!(
        state,
        "accounts-editor-actions" | "accounts-remove" | "accounts-menu" | "accounts-uninstall"
    ) {
        settings.insert(
            "settingsModalNavigation".into(),
            json!({ "activeTab": "accounts", "scrollTopByTab": { "accounts": 420 }, "version": 1 }),
        );
    }
}

pub(super) fn extend_sidebar_state(message: &mut Value) {
    let fixture = extensions();
    message["hud"]["projectViewProjects"] = fixture["projectViewProjects"].clone();
    message["hud"]["projectViewSpaces"] = fixture["projectViewSpaces"].clone();
}

/// The `open` message of a state.
pub(super) fn open_message(state: &str) -> Option<Value> {
    match state {
        "extensions-view-editor" | "extensions-delete-view" => Some(json!({
            "initialTab": "extensions",
            "initialCustomViewId": "custom-view-tasks",
        })),
        "extensions-scope" | "extensions-scope-menu" => Some(json!({
            "initialTab": "extensions",
            "initialViewScopeKey": "official:kanban",
        })),
        // `GHOSTEX_NATIVE_MODAL_DEMO_QUERY` opens the page with that Settings search.
        state if state.starts_with("extensions") => Some(json!({
            "initialTab": "extensions",
            "initialSearchQuery": super::super::env("GHOSTEX_NATIVE_MODAL_DEMO_QUERY"),
        })),
        state if state.starts_with("accounts") => Some(json!({ "initialTab": "accounts" })),
        _ => None,
    }
}

pub(super) fn gxserver_rpc_available(state: &str) -> bool {
    owns(state) && state != "extensions-offline"
}

/// What the scripted gxserver does with a call.
pub(super) enum Answer {
    /// Not one of these pages' endpoints.
    NotMine,
    /// Never answers (the loading states).
    Never,
    Reply(Result<Value, String>),
}

pub(super) fn rpc(state: &str, path: &str, params: &Value) -> Answer {
    match path {
        "/api/listExtensions" | "/api/extensionsCatalog" if state == "extensions-loading" => {
            Answer::Never
        }
        "/api/listExtensions" | "/api/extensionsCatalog" if state == "extensions-error" => {
            Answer::Reply(Err("The extension registry could not be read.".to_string()))
        }
        "/api/listExtensions" => Answer::Reply(Ok(json!({ "extensions": installed() }))),
        "/api/extensionsCatalog" => {
            let fixture = extensions();
            Answer::Reply(Ok(json!({
                "catalog": fixture["catalog"],
                "source": "remote",
                "url": fixture["catalogUrl"],
            })))
        }
        "/api/extensionStoreContent" => {
            let fixture = extensions();
            Answer::Reply(Ok(json!({
                "id": params["id"],
                "readme": fixture["readme"],
                "changelog": fixture["changelog"],
            })))
        }
        "/api/updateExtensionState" => {
            let id = params["id"].as_str().unwrap_or_default();
            let mut list = installed();
            let Some(extension) = list.iter_mut().find(|extension| extension["id"] == id) else {
                return Answer::Reply(Err(format!("Extension {id} is not installed.")));
            };
            if let Some(patch) = params["patch"].as_object() {
                for (key, value) in patch {
                    extension["state"][key] = value.clone();
                }
            }
            let answer = json!({ "extension": extension.clone() });
            set_installed(list);
            Answer::Reply(Ok(answer))
        }
        "/api/uninstallExtension" => {
            let id = params["id"].as_str().unwrap_or_default().to_string();
            let list: Vec<Value> = installed()
                .into_iter()
                .filter(|extension| extension["id"] != id.as_str())
                .collect();
            set_installed(list);
            Answer::Reply(Ok(json!({ "id": id, "uninstalled": true })))
        }
        "/api/installExtension" => {
            let id = params["id"].as_str().unwrap_or_default();
            let fixture = extensions();
            let Some(entry) = fixture["catalog"]["extensions"]
                .as_array()
                .and_then(|entries| entries.iter().find(|entry| entry["name"] == id))
                .cloned()
            else {
                return Answer::Reply(Err(format!("{id} is not in the catalog.")));
            };
            let extension = json!({
                "id": id,
                "manifest": entry,
                "runtime": { "state": "stopped" },
                "state": {
                    "chatBarAutoOpen": false,
                    "enabled": true,
                    "grantedPermissions": entry["permissions"],
                    "pinned": false,
                    "placement": entry["defaultPlacement"],
                    "preferences": {},
                    "storage": {},
                    "terminalPlacement": "splitRight",
                    "version": entry["version"],
                },
            });
            let mut list: Vec<Value> = installed()
                .into_iter()
                .filter(|existing| existing["id"] != id)
                .collect();
            list.push(extension.clone());
            set_installed(list);
            Answer::Reply(Ok(json!({ "extension": extension })))
        }
        "/api/agentAccounts" => accounts_rpc(state, params),
        _ => Answer::NotMine,
    }
}

fn accounts_rpc(state: &str, params: &Value) -> Answer {
    if state == "accounts-loading" {
        return Answer::Never;
    }
    if state == "accounts-error" && params["operation"] != "helperStatus" {
        return Answer::Reply(Err("cswap list failed: permission denied".to_string()));
    }
    let mut data = accounts();
    if state == "accounts-empty" {
        data["accounts"] = json!([]);
    }
    // A Gemini account with Antigravity's Claude/GPT windows beside its own 5h and 7d.
    if state == "accounts-gemini"
        && let Some(accounts) = data["accounts"].as_array_mut()
    {
        let window = |id: &str, label: &str, seconds: i64, model: Option<&str>| {
            let minutes = if seconds == 18_000 { 299 } else { 7_000 };
            let at = chrono::Utc::now() + chrono::Duration::minutes(minutes);
            json!({
                "id": id, "label": label, "usedPercent": 0, "limitWindowSeconds": seconds,
                "resetsAt": at.to_rfc3339(), "model": model,
            })
        };
        accounts.push(json!({
            "id": "gemini-1", "provider": "gemini", "usageOnly": true,
            "name": "m.yahia@example.com", "email": "m.yahia@example.com",
            "registered": true, "status": "ready", "plan": "Google AI Pro",
            "showInTitlebar": true, "selector": "", "color": "neutral", "eligible": false,
            "usage": [
                window("fiveHour", "5h", 18_000, None),
                window("sevenDay", "7d", 604_800, None),
                window("3p-5h", "Claude/GPT 5h", 18_000, Some("Claude/GPT")),
                window("3p-weekly", "Claude/GPT 7d", 604_800, Some("Claude/GPT")),
            ],
        }));
    }
    match params["operation"].as_str() {
        Some("setTitlebar") => {
            let id = params["id"].as_str().unwrap_or_default();
            if let Some(account) = data["accounts"]
                .as_array_mut()
                .and_then(|accounts| accounts.iter_mut().find(|account| account["id"] == id))
            {
                account["showInTitlebar"] = params["shown"].clone();
            }
            ACCOUNT_STATE.with(|stored| *stored.borrow_mut() = Some(data.clone()));
        }
        Some("setEnabled") => {
            let id = params["id"].as_str().unwrap_or_default();
            let enabled = params["enabled"].as_bool() == Some(true);
            if let Some(account) = data["accounts"]
                .as_array_mut()
                .and_then(|accounts| accounts.iter_mut().find(|account| account["id"] == id))
            {
                account["userDisabled"] = json!(!enabled);
                account["status"] = json!(if enabled { "ready" } else { "disabled" });
            }
            ACCOUNT_STATE.with(|stored| *stored.borrow_mut() = Some(data.clone()));
        }
        Some("defaults") => {
            if let Some(provider) = params["provider"].as_str() {
                data["defaults"][provider] = params["policy"].clone();
                ACCOUNT_STATE.with(|stored| *stored.borrow_mut() = Some(data.clone()));
            }
        }
        Some("setupStatus") if state == "accounts-connect" => {
            data["setupJobs"] = json!([{
                "createdAt": 1,
                "id": "job-1",
                "provider": "claude",
                "email": "new@example.com",
                "status": "signingIn",
                "url": "https://claude.ai/oauth/authorize",
                "output": "Opening the sign-in page…",
                "acknowledged": false,
            }]);
        }
        Some("helperStatus") if state == "accounts-uninstall" => {}
        _ => {}
    }
    Answer::Reply(Ok(data))
}

/// The files the page fetches over HTTP (icons).
pub(super) fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let fixture = extensions();
    let text = if url.starts_with("/ext/") {
        fixture["icon"].as_str().unwrap_or_default()
    } else {
        return Err("The request failed with HTTP 404.".to_string());
    };
    Ok(text.as_bytes().to_vec())
}
