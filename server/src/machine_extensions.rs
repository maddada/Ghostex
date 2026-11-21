//! `POST /api/machineExtensions`: the built-in extension switches of the computer this gxserver
//! runs on that gxserver itself obeys (Main Hub, GBots, CLIProxyAPI:
//! `built_in_extensions::COMPUTER_SWITCHES`), as Settings > Extensions shows them with this
//! computer picked on another computer's app.
//!
//! - `{action: "status"}`: `{computer, platform, switches: [{id, title, on, requires?}]}`.
//! - `{action: "set", id, on}`: saves that switch through the app's own settings save (or
//!   gxserver's, on a computer without the app: headless_settings.rs), waits until the settings
//!   file holds it, then answers the status.
//!
//! CDXC:Extensions 2026-11-21 WHY:
//! Main Hub's and CLIProxyAPI's switches are user-only: `ghostex settings set` and `updateSettingsPatch` refuse them for every API caller (settings_agent_writable.rs), so a headless server's Main Hub could never be turned on (GitHub #695). GBots' switch is not: agents may turn it on through those routes, as decided on 10 Nov, and it only matters once Main Hub is on; it is here so the picked computer's GBots card works. This route changes only these three, and only for the owner's own apps on this computer's token: the owner session (`gxo_`) or the token file, which is what a desktop connected through Settings > Remote holds here. An agent's session token and a paired phone (it has no Extensions page) reach this handler and are refused here, with a security-log entry. Another computer's agents, a Collab guest and an extension are stopped earlier by their own allow-lists (connected_computers/scope.rs, collab_shim/registry.rs, the extension registry), with no entry here; this handler would refuse them too. The token file is the same door the user's own terminal uses, so an agent that bypasses its session token to read it is outside what this check can tell apart, as for every other owner-device route.
//! SEE-ALSO: packages/settings-catalog/src/built_in_extensions.rs (CDXC:Extensions 2026-11-21), apps/desktop/src/app/remote_conn/settings_rpc.rs, apps/desktop/src/app/window/settings_modal/tabs/extensions/remote_machine.rs.

use std::time::{Duration, Instant};

use ghostex_settings_catalog::{built_in_extensions, Platform};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::security::Principal;
use crate::server::AppState;

pub(crate) const MACHINE_EXTENSIONS_ENDPOINT: &str = "/api/machineExtensions";

/// What the app's settings save records as the writer.
const SETTINGS_SOURCE: &str = "settings:machineExtensions";
const SAVE_TIMEOUT_MS: u64 = 15_000;
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);
const CONFIRM_POLL: Duration = Duration::from_millis(100);

/// Whether `caller` may flip a switch here: the owner's own apps on this computer's token.
pub(crate) fn may_set(caller: &Principal) -> bool {
    matches!(caller, Principal::Owner { .. } | Principal::LocalAgent)
}

/// The refusal for any other caller.
const REFUSAL: &str = "Only your own Ghostex app can turn these on or off, in Settings > Extensions with this computer picked.";

fn settings(state: &AppState) -> Option<Value> {
    crate::shared_settings_file::read_shared_settings(&state.paths)
}

/// The switches this computer has, each with whether it is on as the settings file says.
pub(crate) fn status_from(settings: Option<&Value>, computer: &str, platform: Platform) -> Value {
    let read = |key: &str| {
        settings
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_bool)
    };
    let switches: Vec<Value> = built_in_extensions::COMPUTER_SWITCHES
        .iter()
        .filter(|id| built_in_extensions::available_on(id, platform))
        .map(|id| {
            let mut entry = json!({
                "id": id,
                "title": built_in_extensions::title(id),
                "on": built_in_extensions::enabled_with(id, read),
            });
            if *id == built_in_extensions::GBOTS {
                entry["requires"] = json!(built_in_extensions::CHIEF);
            }
            entry
        })
        .collect();
    json!({
        "computer": computer,
        "platform": ghostex_settings_catalog::help::platform_id(platform),
        "switches": switches,
    })
}

fn status(state: &AppState) -> Value {
    status_from(
        settings(state).as_ref(),
        &crate::connected_computers::this_computer_name(),
        Platform::current(),
    )
}

/// The settings patch that turns switch `id` on or off, or why it can't be changed here.
pub(crate) fn switch_patch(id: &str, on: bool) -> Result<Map<String, Value>, DomainStateError> {
    let id = built_in_extensions::COMPUTER_SWITCHES
        .iter()
        .copied()
        .find(|known| *known == id)
        .ok_or_else(|| DomainStateError::bad_request("Unknown built-in extension."))?;
    if !built_in_extensions::available_on(id, Platform::current()) {
        return Err(DomainStateError::bad_request(format!(
            "{} isn't available on this computer.",
            built_in_extensions::title(id)
        )));
    }
    let (key, enables) = built_in_extensions::switch_key(id)
        .ok_or_else(|| DomainStateError::bad_request("Unknown built-in extension."))?;
    let mut patch = Map::new();
    patch.insert(key.to_string(), Value::Bool(on == enables));
    Ok(patch)
}

/// The security log's id for turning Main Hub or GBots on through this route: Main Hub gives
/// Chief of Staff control of every agent here, so it reaches the bell and the phones however it was
/// done (audit/notify.rs `OPENING_ACTIONS`), and a token-file misuse is seen.
pub(crate) const AGENT_POWERS_ON_ACTION: &str = "machineExtensions.mainHubOrGBotsOn";

fn audit(id: &str, on: bool, outcome: crate::audit::Outcome) {
    let title = built_in_extensions::title(id);
    let kind = if on && matches!(id, built_in_extensions::CHIEF | built_in_extensions::GBOTS) {
        AGENT_POWERS_ON_ACTION
    } else {
        "settings.machineExtension"
    };
    crate::audit::record(
        crate::audit::Action::new(
            kind,
            format!(
                "Turn {} {title} on {}",
                if on { "on" } else { "off" },
                crate::connected_computers::this_computer_name()
            ),
        )
        .target(title),
        crate::audit::Actor::current(),
        outcome,
        None,
    );
}

/// Why gxserver left the save to an app that isn't running, in words for whoever picked this
/// computer: its settings file was changed outside Ghostex since gxserver saved it (a headless
/// computer someone edited by hand), or it is the desktop app's file.
fn not_saved_here(state: &AppState, patch: &Map<String, Value>) -> (&'static str, String) {
    let computer = crate::connected_computers::this_computer_name();
    let file = crate::headless_settings::settings_file_path(&state.paths);
    let by_hand = patch
        .iter()
        .map(|(key, value)| format!("\"{key}\": {value}"))
        .collect::<Vec<_>>()
        .join(", ");
    match crate::headless_settings::left_to_the_app(&state.paths) {
        crate::headless_settings::LeftToTheApp::ChangedSince => (
            "conflict",
            format!(
                "{computer}'s settings file was changed outside Ghostex, so Ghostex there no longer saves it. On {computer}, set {by_hand} in {} by hand, or open the Ghostex app there if it has one.",
                file.display()
            ),
        ),
        crate::headless_settings::LeftToTheApp::AppsFile => (
            "dependencyUnavailable",
            format!(
                "Ghostex on {computer} saves its settings through its desktop app, which isn't running. Open Ghostex on {computer}, or if it has no app, set {by_hand} in {} by hand.",
                file.display()
            ),
        ),
    }
}

async fn set(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let on = params
        .get("on")
        .and_then(Value::as_bool)
        .ok_or_else(|| DomainStateError::bad_request("Say whether to turn it on or off."))?;
    if !may_set(caller) {
        audit(&id, on, crate::audit::Outcome::Refused(REFUSAL.to_string()));
        return Err(DomainStateError {
            code: "forbidden",
            message: REFUSAL.to_string(),
        });
    }
    let patch = switch_patch(&id, on)?;
    let mut payload = Map::new();
    payload.insert("patch".into(), Value::Object(patch.clone()));
    payload.insert("source".into(), json!(SETTINGS_SOURCE));
    if let Err(failure) = crate::server::save_settings_patch(state, payload, SAVE_TIMEOUT_MS).await
    {
        let (code, message) = if failure.code == "dependencyUnavailable" {
            not_saved_here(state, &patch)
        } else {
            ("internalError", failure.message)
        };
        audit(&id, on, crate::audit::Outcome::Failed(message.clone()));
        return Err(DomainStateError { code, message });
    }
    // The app saves on its own thread: wait until the file says so, so the answer is the new state.
    let started = Instant::now();
    let confirmed = loop {
        let saved = settings(state);
        let done = patch.iter().all(|(key, value)| {
            saved
                .as_ref()
                .and_then(|settings| settings.get(key))
                .is_some_and(|saved| saved == value)
        });
        if done || started.elapsed() >= CONFIRM_TIMEOUT {
            break done;
        }
        tokio::time::sleep(CONFIRM_POLL).await;
    };
    if !confirmed {
        let message = format!(
            "Ghostex on {} took the change but its settings don't show it yet. Check again in a moment.",
            crate::connected_computers::this_computer_name()
        );
        audit(&id, on, crate::audit::Outcome::Failed(message.clone()));
        return Err(DomainStateError {
            code: "internalError",
            message,
        });
    }
    audit(&id, on, crate::audit::Outcome::Done);
    Ok(status(state))
}

pub(crate) async fn dispatch(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    match params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("status")
    {
        "status" => Ok(status(state)),
        "set" => set(state, caller, params).await,
        _ => Err(DomainStateError::bad_request(
            "Unknown built-in extensions action.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_owners_own_apps_may_flip_a_switch() {
        assert!(may_set(&Principal::Owner {
            device_id: "d".into()
        }));
        assert!(may_set(&Principal::LocalAgent));
        for caller in [
            Principal::Device {
                device_id: "phone".into(),
            },
            Principal::Session {
                project_id: "p".into(),
                session_id: "s".into(),
                role: crate::security::SessionRole::Agent,
            },
            Principal::Peer {
                server_id: "S1".into(),
                name: "Laptop".into(),
            },
            Principal::Guest {
                share_id: "g".into(),
            },
            Principal::Extension {
                extension_id: "e".into(),
            },
            Principal::Anonymous,
        ] {
            assert!(!may_set(&caller), "{}", caller.kind());
        }
    }

    #[test]
    fn another_computers_agents_and_guests_never_reach_the_route() {
        assert!(!crate::connected_computers::is_peer_allowed(
            MACHINE_EXTENSIONS_ENDPOINT
        ));
        assert!(!crate::collab_shim::registry::GUEST_READ_ENDPOINTS
            .contains(&MACHINE_EXTENSIONS_ENDPOINT));
    }

    #[test]
    fn only_the_computer_switches_can_be_set() {
        assert_eq!(
            switch_patch("chief", true).expect("chief"),
            json!({ "mainHubHidden": false })
                .as_object()
                .unwrap()
                .clone()
        );
        assert_eq!(
            switch_patch("gbots", true).expect("gbots"),
            json!({ "gbotsEnabled": true }).as_object().unwrap().clone()
        );
        assert_eq!(
            switch_patch("cliProxyApi", false).expect("hub"),
            json!({ "cliProxyApiEnabled": false })
                .as_object()
                .unwrap()
                .clone()
        );
        assert!(switch_patch("connectors", true).is_err());
        assert!(switch_patch("mainHubHidden", false).is_err());
    }

    #[test]
    fn the_status_reads_each_switch_with_its_default() {
        let status = status_from(
            Some(&json!({ "mainHubHidden": false })),
            "Kubuntu VM",
            Platform::Linux,
        );
        assert_eq!(status["computer"], "Kubuntu VM");
        assert_eq!(status["platform"], "linux");
        let switches = status["switches"].as_array().expect("switches");
        let on = |id: &str| {
            switches
                .iter()
                .find(|entry| entry["id"] == id)
                .map(|entry| entry["on"].clone())
        };
        assert_eq!(on("chief"), Some(json!(true)));
        assert_eq!(on("gbots"), Some(json!(false)));
        assert_eq!(on("cliProxyApi"), Some(json!(false)));
        let gbots = switches
            .iter()
            .find(|entry| entry["id"] == "gbots")
            .unwrap();
        assert_eq!(gbots["requires"], "chief");
    }
}
