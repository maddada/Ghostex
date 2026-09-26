//! Running a saved Action (`runSidebarCommand`): the project row's Actions, a Global Action, and a
//! Quick Access or command palette run. Which Action, which project it runs in, and the payload
//! the host's command-action entry performs.
//!
//! CDXC:CommandPane 2026-06-26-05:11:
//! The shared SidebarApp and Command Palette emit `runSidebarCommand` as an Action-selection message: command id plus optional runMode. Resolve the selected Action from the live gxserver HUD projection and hand trusted launch metadata to the fixed command-action entry, so command text, URLs, saved close-on-exit metadata, paths, output, and logs never come from the message.
//!
//! CDXC:CommandPane 2026-06-27-07:54:
//! Treat selector shape as part of the Action contract before looking up the HUD command. Extra launch/run-state fields are unsupported no-ops, not sanitized launches, while valid configured-but-empty selectors still reach Settings like macOS.
//!
//! CDXC:AgentLauncher 2026-08-07:
//! Scope and group id answer different questions, so a global selector may carry one: the scope picks the list, the group id picks the project to activate before dispatching. An unrecognized scope is an unsupported no-op, never a silent fallback to the project list, which would run an Action the user did not click.
//!
//! CDXC:Projects 2026-08-01:
//! Project-row Action clicks resolve against the clicked project's own command list (`commandsByProject`), never another project's, so two projects with different Actions cannot cross-launch, and the project is activated before the Action runs so it opens in the project the user clicked.
//!
//! One declared difference from the old runtime: an Action with no group id resolves against the ACTIVE project's own list, which for a remote project is that machine's list; the runtime read this computer's last active project there (`activeProjectId` is local-only), and with no HUD at all it offered the unconfigured defaults, which only opened Settings.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_command_run.rs (the host), packages/gx-core/src/hud/actions.rs
//! (`commandsByProject`, `globalCommands`), apps/desktop/src/app/helpers/sidebar/native_action_exec.rs (the payload's parser).

use serde_json::{json, Map, Value};

use crate::keys::ProjectKey;

/// `ghostex.gpui.sidebar.commandAction`, version 1.
pub const SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE: &str = "ghostex.gpui.sidebar.commandAction";
pub const SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION: u64 = 1;

/// The keys a `runSidebarCommand` selector may carry (`GPUI_SIDEBAR_COMMAND_SELECTOR_MESSAGE_KEYS`).
const SELECTOR_KEYS: [&str; 5] = ["commandId", "groupId", "runMode", "scope", "type"];

/// What a `runSidebarCommand` message does.
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarCommandRun {
    /// Not a valid selector, or no such Action: nothing happens.
    Unsupported,
    /// The Action exists but has no command or URL yet: Settings opens so the user can add one.
    OpenSettings,
    /// Activate `focus_project` first when it is set, then perform `action`.
    Run {
        focus_project: Option<ProjectKey>,
        action: Value,
    },
}

/// Plans one `runSidebarCommand`. `hud` is the composed sidebar HUD (`globalCommands`,
/// `commandsByProject` keyed by workspace project id); `active_project` is the store's.
pub fn plan_sidebar_command_run(
    hud: Option<&Value>,
    message: &Value,
    active_project: Option<&ProjectKey>,
) -> SidebarCommandRun {
    let Some(object) = message.as_object() else {
        return SidebarCommandRun::Unsupported;
    };
    let Some(command_id) = trimmed(object.get("commandId")) else {
        return SidebarCommandRun::Unsupported;
    };
    let scope = match object.get("scope") {
        None => "project",
        Some(Value::String(scope)) if scope == "global" || scope == "project" => scope.as_str(),
        Some(_) => return SidebarCommandRun::Unsupported,
    };
    if object
        .keys()
        .any(|key| !SELECTOR_KEYS.contains(&key.as_str()))
    {
        return SidebarCommandRun::Unsupported;
    }
    let run_mode = match object.get("runMode") {
        None => None,
        Some(Value::String(mode)) if mode == "default" || mode == "debug" => Some(mode.as_str()),
        Some(_) => return SidebarCommandRun::Unsupported,
    };
    let group_id = trimmed(object.get("groupId"));
    // Only a project's own group names a project here: a user-made group and anything else that
    // does not parse leave the Action in the active project, as the runtime's parse did.
    let target = group_id.and_then(ProjectKey::parse_sidebar_group_id);
    let command = match scope {
        "global" => find_command(hud.and_then(|hud| hud.get("globalCommands")), command_id),
        _ => {
            let project = target.as_ref().or(active_project);
            project.and_then(|project| {
                find_command(
                    hud.and_then(|hud| hud.get("commandsByProject"))
                        .and_then(|rows| rows.get(project.to_workspace_project_id())),
                    command_id,
                )
            })
        }
    };
    let Some(command) = command else {
        return SidebarCommandRun::Unsupported;
    };
    let is_browser = command.get("actionType").and_then(Value::as_str) == Some("browser");
    let configured = match is_browser {
        true => trimmed(command.get("url")).is_some(),
        false => trimmed(command.get("command")).is_some(),
    };
    if !configured {
        return SidebarCommandRun::OpenSettings;
    }
    let focus_project = target.filter(|target| Some(target) != active_project);
    SidebarCommandRun::Run {
        focus_project,
        action: command_action_payload(command, is_browser, run_mode),
    }
}

/// `postSidebarCommandAction`'s payload.
///
/// CDXC:CommandPane 2026-06-27-07:54:
/// GPUI command-pane Action launches must match native `runNativeSidebarCommand`: default command-pane runtime forces terminal close-on-exit off even when trusted saved/HUD Action definitions preserve older close-on-exit metadata. Browser Actions omit the terminal-only fields, and only a terminal Action forwards the selector's validated `runMode`.
fn command_action_payload(command: &Value, is_browser: bool, run_mode: Option<&str>) -> Value {
    let mut payload = Map::new();
    payload.insert(
        "actionType".into(),
        json!(if is_browser { "browser" } else { "terminal" }),
    );
    payload.insert("commandId".into(), command["commandId"].clone());
    if let Some(name) = command.get("name").filter(|name| name.is_string()) {
        payload.insert("name".into(), name.clone());
    }
    if is_browser {
        if let Some(url) = command.get("url").and_then(Value::as_str) {
            if !url.is_empty() {
                payload.insert("url".into(), json!(url));
            }
        }
    } else {
        if let Some(run_mode) = run_mode {
            payload.insert("runMode".into(), json!(run_mode));
        }
        payload.insert("closeTerminalOnExit".into(), Value::Bool(false));
        if let Some(sound) = command.get("playCompletionSound").and_then(Value::as_bool) {
            payload.insert("playCompletionSound".into(), Value::Bool(sound));
        }
        if let Some(text) = command.get("command").and_then(Value::as_str) {
            if !text.is_empty() {
                payload.insert("command".into(), json!(text));
            }
        }
        let links: Vec<Value> = command
            .get("links")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|link| json!({ "target": link["target"], "url": link["url"] }))
            .collect();
        if !links.is_empty() {
            payload.insert("links".into(), Value::Array(links));
        }
    }
    payload.insert("type".into(), json!(SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE));
    payload.insert(
        "version".into(),
        json!(SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION),
    );
    Value::Object(payload)
}

fn find_command<'a>(rows: Option<&'a Value>, command_id: &str) -> Option<&'a Value> {
    rows?
        .as_array()?
        .iter()
        .find(|command| command.get("commandId").and_then(Value::as_str) == Some(command_id))
}

fn trimmed(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
