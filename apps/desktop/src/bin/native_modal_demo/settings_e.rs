//! Preview fixtures for the Theme, Hotkeys, Open In, Debugging, About, Integrations and OS
//! Integration Settings pages: the settings each state starts from (the matching Storybook
//! stories of packages/core-ui/settings-modal.stories.tsx (deleted 2026-10-01)) and the answers to the messages those
//! pages post (`requestGhostexCliStatus`, `requestOSIntegrationStatus`, the file pickers, ...).
//!
//! States: `about`, `debugging`, `open-in`, `open-in-editor` (Add target open),
//! `os-integration`, `os-integration-status` (a status with handler updates that need
//! attention), `integrations` (permissions off), `integrations-permissions` (SpaceO needs a restart
//! for Accessibility and lacks Screen Recording), `integrations-trycua-missing`,
//! `integrations-update` (a Trycua update available), `integrations-skills` (some skills installed),
//! `hotkeys-page` (the Hotkeys page), `hotkeys-recording` (a recorder listening),
//! `hotkeys-duplicate` (two actions on one chord), `theme`, `theme-light-scheme`, `theme-more`
//! (every More options open), `theme-custom` (Custom colours in both modes),
//! `theme-custom-picker` (the same, with the dark "+" square's Pick Color dialog open),
//! `theme-glass-picture`, `theme-glass-live`, `theme-glass-live-dark-only`,
//! `theme-glass-live-video`, `theme-glass-opaque` and `theme-glass-blocked`. `theme-presets` opens
//! Interface at its Presets row, and `theme-presets-<preset id>` (`theme-presets-kg-wave`) with that
//! preset picked, which the row scrolls into view. `theme-colours` opens it at Colours, `theme-at-<section>` at that section
//! (`transparency`, `chatTerminal`).
use serde_json::{Map, Value, json};

/// `GHOSTEX_NATIVE_MODAL_DEMO_STATE`.
fn demo_state() -> String {
    super::super::env("GHOSTEX_NATIVE_MODAL_DEMO_STATE")
}

/// Settings the page states start from, on top of the story's `modalSettings`.
pub(super) fn story_settings(state: &str, settings: &mut Map<String, Value>) {
    match state {
        "debugging" => {
            settings.insert("debuggingMode".into(), json!(true));
            settings.insert(
                "diagnosticLogging".into(),
                json!({
                    "scenarios": {
                        "gpui.sessionChat.viewState": { "enabled": true },
                        "native.terminal.focus": { "enabled": true }
                    },
                    "version": 1
                }),
            );
        }
        "os-integration" | "os-integration-status" => {
            settings.insert("showBetaFeatures".into(), json!(true));
        }
        "open-in" | "open-in-editor" => {
            settings.insert(
                "workspaceOpenTargetAvailability".into(),
                json!({
                    "availableTargetIds": ["cursor", "vscode", "zed", "finder"],
                    "checkedAtMs": 1,
                    "resolvedAppNames": {},
                    "resolvedCommands": {}
                }),
            );
            settings.insert("workspaceOpenTargetHiddenIds".into(), json!(["zed"]));
            settings.insert(
                "customWorkspaceOpenTargets".into(),
                json!([{
                    "args": ["--new-window"],
                    "command": "sublime",
                    "id": "custom:sublime-text",
                    "label": "Sublime Text"
                }]),
            );
        }
        "hotkeys-duplicate" => {
            settings.insert(
                "hotkeys".into(),
                json!({ "copyLastChatReply": "cmd+shift+;" }),
            );
        }
        "theme-glass-live" | "theme-glass-live-dark-only" | "theme-glass-live-video" => {
            // `glassLiveSettings` of the story.
            settings.insert(
                "windowGlass".into(),
                json!(if state == "theme-glass-live-dark-only" {
                    "auto"
                } else {
                    "frosted"
                }),
            );
            settings.insert("windowGlassSource".into(), json!("live"));
            settings.insert("windowGlassLiveStyleDark".into(), json!("aurora"));
            settings.insert("windowGlassLiveStyleLight".into(), json!("drift"));
            settings.insert("windowGlassLiveSpeed".into(), json!(1));
            if state == "theme-glass-live-video" {
                settings.insert("windowGlassLiveStyleDark".into(), json!("video"));
                settings.insert(
                    "windowGlassVideoDark".into(),
                    json!("/Users/you/Movies/Rain on glass.mp4"),
                );
            }
        }
        "theme-glass-picture" => {
            settings.insert("windowGlass".into(), json!("frosted"));
            settings.insert("windowGlassSource".into(), json!("customImage"));
            settings.insert(
                "windowGlassImageDark".into(),
                json!("/Users/you/Pictures/night-city.jpg"),
            );
        }
        "theme-glass-opaque" => {
            settings.insert("windowGlass".into(), json!("opaque"));
        }
        "theme-custom" | "theme-custom-picker" => {
            settings.insert("darkThemePreset".into(), json!("custom"));
            settings.insert("lightThemePreset".into(), json!("custom"));
            settings.insert(
                "customSidebarTitlebarBackgroundTintColor".into(),
                json!("#336699"),
            );
            settings.insert("themeSidebarContrast".into(), json!(-8));
            settings.insert("themeWorkAreaContrast".into(), json!(0));
        }
        state if state.starts_with("theme-presets-") => {
            let id = &state["theme-presets-".len()..];
            if let Some((_, patch)) =
                super::super::settings_modal::tabs::theme::presets::theme_preset_choice(id, false)
            {
                settings.extend(patch);
            }
        }
        _ => {}
    }
}

/// Adds the hud fields these pages read to a `sidebarState` hydrate.
pub(super) fn extend_sidebar_state(message: &mut Value) {
    let state = demo_state();
    if !message["hud"].is_object() {
        message["hud"] = json!({});
    }
    if state == "theme-glass-blocked" {
        message["hud"]["windowGlassBlockedBySystem"] = json!(true);
    }
}

/// The `open` message fields of a page state, when the state is one of these pages'.
pub(super) fn open_message(state: &str) -> Option<Value> {
    // General > Session Cards, where the Issues Workflow switches sit.
    if state == "session-cards" {
        return Some(json!({ "initialTab": "settings", "initialSection": "sessionCards" }));
    }
    let tab = match state {
        "about" => "about",
        "debugging" => "debugging",
        "open-in" | "open-in-editor" => "openTargets",
        "os-integration" | "os-integration-status" => "osIntegration",
        "integrations"
        | "integrations-permissions"
        | "integrations-trycua-missing"
        | "integrations-update"
        | "integrations-skills" => "integrations",
        "hotkeys-page" | "hotkeys-recording" | "hotkeys-duplicate" => "hotkeys",
        state if state.starts_with("theme") => "theme",
        _ => return None,
    };
    if state == "theme-colours" {
        return Some(json!({ "initialTab": "theme", "initialSection": "theming" }));
    }
    if let Some(section) = state.strip_prefix("theme-at-") {
        return Some(json!({ "initialTab": "theme", "initialSection": section }));
    }
    if state.starts_with("theme-presets") {
        return Some(json!({ "initialTab": "theme", "initialSection": "presets" }));
    }
    // `GHOSTEX_NATIVE_MODAL_DEMO_QUERY` filters the page, e.g. "permissions" for the grant rows.
    let query = super::super::env("GHOSTEX_NATIVE_MODAL_DEMO_QUERY");
    if !query.is_empty() {
        return Some(json!({ "initialTab": tab, "initialSearchQuery": query }));
    }
    Some(json!({ "initialTab": tab }))
}

/// The story's `ghostexCliStatus` for the Integrations states.
fn ghostex_cli_status(state: &str) -> Value {
    let permissions_granted = match state {
        "integrations" => Some(false),
        "integrations-update" | "integrations-skills" => Some(true),
        _ => None,
    };
    let driver_installed = state != "integrations-trycua-missing";
    // `integrations-permissions`: SpaceO granted Accessibility after it started and still lacks
    // Screen Recording; Fast Computer & Browser Use has both.
    let mixed = state == "integrations-permissions";
    let spaceo_permission = |mixed_state: &str| match permissions_granted {
        _ if mixed => json!(mixed_state),
        Some(true) => json!("granted"),
        Some(false) => json!("notGranted"),
        None => json!("unknown"),
    };
    let cua_permission = if mixed {
        Some(true)
    } else {
        permissions_granted
    };
    let update_available = state == "integrations-update";
    let skills = state == "integrations-skills";
    json!({
        "cliSkillInstalled": skills,
        "browserSkillInstalled": false,
        "computerUseSkillInstalled": skills,
        "embeddedBrowserSkillInstalled": false,
        "helpSkillInstalled": skills,
        "cuaAppInstalled": false,
        "cuaDriverAccessibilityPermissionGranted": cua_permission,
        "cuaDriverAccessibilityPermission": cua_permission.map(|granted| if granted { "granted" } else { "notGranted" }),
        "cuaDriverInstallCommand": "/bin/bash -c \"$(curl -fsSL https://cua.ai/driver/install.sh)\"",
        "cuaDriverInstalled": driver_installed,
        "cuaDriverLatestVersion": if update_available { "0.29.1" } else { "0.23.2" },
        "cuaDriverManagedUpdatesSupported": true,
        "cuaDriverScreenRecordingPermissionGranted": cua_permission,
        "cuaDriverScreenRecordingPermission": cua_permission.map(|granted| if granted { "granted" } else { "notGranted" }),
        "cuaDriverUpdateAvailable": update_available,
        "cuaDriverVersion": "0.23.2",
        "spaceoAccessibilityPermission": spaceo_permission("needsRestart"),
        "spaceoDaemonRunning": driver_installed,
        "spaceoInstallCommand": "curl -fsSL https://raw.githubusercontent.com/ParthJadhav/SpaceO/main/install.sh | bash -s -- --no-clients",
        "spaceoInstalled": driver_installed,
        "spaceoLatestVersion": if update_available { "1.0.6" } else { "1.0.5" },
        "spaceoPermissionApp": "spaceo (/Users/you/.local/bin/spaceo)",
        "spaceoScreenRecordingPermission": spaceo_permission("notGranted"),
        "spaceoRestarting": false,
        "spaceoSkillInstalled": skills,
        "spaceoSupported": true,
        "spaceoUpdateAvailable": update_available,
        "spaceoVersion": "1.0.5",
        "detail": "Ghostex CLI is installed automatically with the app.",
        "generateTitleSkillInstalled": false,
        "generatedAt": "2026-05-27T04:17:00.000Z",
        "ghostexPath": "/opt/homebrew/bin/ghostex",
        "gxBlockedByExistingCommand": false,
        "gxUsable": false,
        "installed": true,
        "moveCodexSessionSkillInstalled": false,
        "type": "ghostexCliStatus"
    })
}

fn os_integration_status(state: &str) -> Value {
    let items = if state == "os-integration-status" {
        json!([
            { "target": "editor", "extension": "ts", "reason": "launchServicesRejected" },
            { "target": "editor", "extension": "md", "reason": "launchServicesRejected" },
            { "target": "scriptRunner", "extension": "sh", "reason": "contentTypeUnavailable" },
            { "target": "ghostexLinks", "scheme": "ghostex", "reason": "launchServicesRejected" },
            { "target": "bundleRegistration", "operation": "registerBundle", "reason": "bundleRegistrationFailed" },
            { "target": "editor", "extension": "rs", "reason": "launchServicesRejected" },
            { "target": "editor", "extension": "py", "reason": "launchServicesRejected" }
        ])
    } else {
        json!([])
    };
    json!({
        "bundleIdentifier": "com.ghostex.app",
        "editorDefaults": { "ts": "com.ghostex.app", "md": "com.microsoft.VSCode", "json": "com.ghostex.app" },
        "registeredEditableFiles": true,
        "registeredGhostexURLScheme": true,
        "registeredScriptRunner": state != "os-integration-status",
        "scriptDefaults": { "sh": "com.apple.Terminal", "command": "com.ghostex.app" },
        "statusItems": items,
        "terminalLinkDefaultBundleId": "com.apple.Terminal",
        "type": "osIntegrationStatus"
    })
}

/// What the app answers to a message these pages post.
pub(super) fn answers(message: &Value) -> Vec<Value> {
    let state = demo_state();
    let state = state.as_str();
    match message.get("type").and_then(Value::as_str) {
        Some("requestOSIntegrationStatus") | Some("setOSIntegrationDefaults") => {
            vec![os_integration_status(state)]
        }
        Some(
            "requestGhostexCliStatus"
            | "installGhostexCli"
            | "checkCuaDriverUpdate"
            | "reinstallCuaDriver"
            | "checkSpaceoUpdate"
            | "reinstallSpaceo"
            | "requestToolPermissionStatus",
        ) => vec![ghostex_cli_status(state)],
        Some("pickWindowGlassImageFile") => vec![json!({
            "appearance": message.get("appearance").cloned().unwrap_or(json!("dark")),
            "path": "/Users/you/Pictures/morning-fields.jpg",
            "type": "windowGlassImageFilePicked"
        })],
        Some("pickWindowGlassVideoFile") => {
            let light = message.get("appearance").and_then(Value::as_str) == Some("light");
            vec![json!({
                "appearance": if light { "light" } else { "dark" },
                "path": format!("/Users/you/Movies/{}.mp4", if light { "Morning light" } else { "Rain on glass" }),
                "type": "windowGlassVideoFilePicked"
            })]
        }
        _ => Vec::new(),
    }
}
