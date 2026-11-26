//! Opening an installed extension's view in a project's side panel: `ghostex extensions open`
//! (`/api/openExtensionView`) and an extension's own server (`POST /api/ext/v1/openView`, behind
//! `views.open`).
//!
//! CDXC:Extensions 2026-11-26 DECISION:
//! User (#772, Mobile Pane): an agent, or the extension's server, opens or focuses an installed extension's view in the side panel of the calling session's project, so a phone mirror appears beside the agent that starts testing on it. An extension may open only its own view, only while it is on, and only with `views.open`, which the user grants on the install card like every other permission. An agent never yanks what the user is looking at: the open follows `ghostex browser open`'s rules (CDXC:SwitchRequests 2026-11-23 and 2026-11-26 in server/src/switch_requests.rs): `--background` adds the tab to the agent's own project without showing it, "Let agents open and switch pages without asking" (or the toast's Always) opens it on screen, and otherwise the user gets the same switch toast with Open and Always. A view already in front of that project needs neither: the app answers that it is showing. The user's own terminal opens it straight away.
//! SEE-ALSO: packages/gx-core/src/renderer_commands/verbs.rs (`OpenExtensionView`), apps/desktop/src/app/extensions/open_view.rs (the app's side), apps/desktop/src/app/window/toast.rs (Always on this toast).
//!
//! CDXC:Extensions 2026-11-26 WHY:
//! #772 review: nothing limited how often an open could come. An extension's server runs on its own, unwatched, so a loop of `openView` calls either pulled its view back in front every time the user left it (with agents allowed to open pages without asking) or kept replacing the one switch toast, so a click meant for an agent's toast could land on the extension's. An agent or an extension now gets one on-screen open or toast per `OPEN_VIEW_MIN_INTERVAL`; a background open and a view already showing don't count, and the user's own opens are never limited. The extension's toast says it is the extension asking and has no Always: Always turns on opens without asking for every agent, which an extension's request must not be the way to grant.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::{
    ExtensionError, ExtensionKind, ExtensionManifest, ExtensionPermission, ExtensionPlacement,
    ExtensionResult, ExtensionStoreEntry,
};
use crate::server::AppState;

/// The renderer command the app performs (gx-core `RendererVerb::OpenExtensionView`). Only
/// gxserver sends it, after the checks here, and the app's own switch toast; it is not in the
/// CLI's forward list (`RENDERER_COMMAND_ACTIONS`).
pub(crate) const OPEN_EXTENSION_VIEW_ACTION: &str = "openExtensionView";

const OPEN_VIEW_TIMEOUT_MS: u64 = 15_000;

/// One on-screen open or toast per agent or extension per this long (CDXC:Extensions 2026-11-26
/// WHY above).
const OPEN_VIEW_MIN_INTERVAL: Duration = Duration::from_secs(10);

/// When each agent or extension last opened a view on screen or asked with the toast.
static LAST_VISIBLE_OPEN: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Who an open is limited for: an agent session or an extension; the user is never limited.
fn rate_key(requester: &OpenViewRequester) -> Option<String> {
    match requester {
        OpenViewRequester::User => None,
        OpenViewRequester::Agent {
            project_id,
            session_id,
        } => Some(format!("agent:{project_id}:{session_id}")),
        OpenViewRequester::Extension { id } => Some(format!("extension:{id}")),
    }
}

/// How long `key` must still wait at `now` after its last visible open at `last`, if at all.
fn open_wait(last: Option<Instant>, now: Instant) -> Option<Duration> {
    let elapsed = now.saturating_duration_since(last?);
    (elapsed < OPEN_VIEW_MIN_INTERVAL).then(|| OPEN_VIEW_MIN_INTERVAL - elapsed)
}

/// Refuses a visible open that comes too soon after the last one of the same requester.
fn check_open_rate(requester: &OpenViewRequester, title: &str) -> ExtensionResult<()> {
    let Some(key) = rate_key(requester) else {
        return Ok(());
    };
    let last = LAST_VISIBLE_OPEN
        .lock()
        .ok()
        .and_then(|opens| opens.get(&key).copied());
    match open_wait(last, Instant::now()) {
        None => Ok(()),
        Some(wait) => Err(ExtensionError {
            code: "rateLimited",
            message: format!(
                "{title} was opened or asked for a moment ago. Try again in {} s, or use --background to add its tab without showing it.",
                wait.as_secs().max(1)
            ),
        }),
    }
}

fn note_visible_open(requester: &OpenViewRequester) {
    let Some(key) = rate_key(requester) else {
        return;
    };
    if let Ok(mut opens) = LAST_VISIBLE_OPEN.lock() {
        let now = Instant::now();
        opens.retain(|_, at| now.saturating_duration_since(*at) < OPEN_VIEW_MIN_INTERVAL);
        opens.insert(key, now);
    }
}

/// Who asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OpenViewRequester {
    /// The user: their own terminal, the desktop app, a phone.
    User,
    /// An agent session (`switch_requests::caller_agent_session`).
    Agent {
        project_id: String,
        session_id: String,
    },
    /// The extension's own server (its `GHOSTEX_API_TOKEN`).
    Extension { id: String },
}

/// How the app should open it (the payload's `mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenViewMode {
    /// Open or focus it on screen.
    Show,
    /// Add its tab to the project without showing or focusing it.
    Background,
    /// Answer `alreadyShowing` when it is the view in front of the project, else `needsAsk`, so
    /// gxserver shows the switch toast.
    Ask,
}

impl OpenViewMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Background => "background",
            Self::Ask => "ask",
        }
    }
}

/// Why `manifest`/`state` has no view to open, in words the user (or the agent telling them) can
/// act on; `None` when it has one: a `view` placement chosen for it, or a sidebar extension's
/// side panel view.
pub(crate) fn extension_view_refusal(
    manifest: &ExtensionManifest,
    state: &ExtensionStoreEntry,
) -> Option<String> {
    let id = manifest.name.as_str();
    let title = manifest.title.as_str();
    if !state.enabled {
        return Some(format!(
            "{title} is turned off. Turn it on in Settings > Extensions, or run `ghostex extensions state {id} --set enabled=true`."
        ));
    }
    if manifest.kind == Some(ExtensionKind::Sidebar) {
        return manifest
            .side_panel
            .is_none()
            .then(|| format!("{title} has no side panel view: it shows only in the sidebar."));
    }
    if manifest.kind == Some(ExtensionKind::TerminalPane) {
        return Some(format!(
            "{title} opens in a terminal pane, not in the side panel."
        ));
    }
    if !manifest.placements.contains(&ExtensionPlacement::View) {
        return Some(format!("{title} has no side panel view."));
    }
    match state.placement {
        Some(ExtensionPlacement::View) => None,
        other => Some(format!(
            "{title} is set to open as {}. Choose View for it in Settings > Extensions, or run `ghostex extensions state {id} --set placement=view`.",
            placement_words(other)
        )),
    }
}

fn placement_words(placement: Option<ExtensionPlacement>) -> &'static str {
    match placement {
        Some(ExtensionPlacement::View) => "a view",
        Some(ExtensionPlacement::ChatBar) => "a chat bar panel",
        Some(ExtensionPlacement::Popup) => "a popup",
        Some(ExtensionPlacement::Modal) => "a window",
        None => "nothing",
    }
}

/// An extension's server may open only its own view, and only with `views.open` granted.
pub(crate) fn extension_open_refusal(
    caller_id: &str,
    target_id: &str,
    manifest: &ExtensionManifest,
    state: &ExtensionStoreEntry,
) -> Option<ExtensionError> {
    if caller_id != target_id {
        return Some(forbidden(format!(
            "An extension opens only its own view: {caller_id} can't open {target_id}."
        )));
    }
    let permission = ExtensionPermission::ViewsOpen;
    if state.granted_permissions.contains(&permission) {
        return None;
    }
    Some(forbidden(if manifest.permissions.contains(&permission) {
        format!(
                "The user hasn't granted views.open to {target_id}. They can grant it in Settings > Extensions."
            )
    } else {
        format!("{target_id} doesn't declare the views.open permission in its manifest.")
    }))
}

/// How an open from `requester` goes: the user's opens show at once; an agent's or an
/// extension's open in the background without asking, show when the user lets agents open pages
/// without asking, and ask otherwise.
pub(crate) fn open_view_mode(
    requester: &OpenViewRequester,
    background: bool,
    opens_allowed: bool,
) -> OpenViewMode {
    match requester {
        OpenViewRequester::User => {
            if background {
                OpenViewMode::Background
            } else {
                OpenViewMode::Show
            }
        }
        OpenViewRequester::Agent { .. } | OpenViewRequester::Extension { .. } => {
            if background {
                OpenViewMode::Background
            } else if opens_allowed {
                OpenViewMode::Show
            } else {
                OpenViewMode::Ask
            }
        }
    }
}

fn forbidden(message: String) -> ExtensionError {
    ExtensionError {
        code: "forbidden",
        message,
    }
}

/// The installed extension `id`: its manifest and store entry.
fn installed_extension(
    state: &AppState,
    id: &str,
) -> ExtensionResult<(ExtensionManifest, ExtensionStoreEntry)> {
    let installed = state.extension_registry.list()?;
    installed
        .into_iter()
        .find(|extension| extension.id == id)
        .map(|extension| (extension.manifest, extension.state))
        .ok_or_else(|| {
            ExtensionError::not_found(format!(
                "Extension {id:?} is not installed. Run `ghostex extensions list` to see what is, or install it in Settings > Extensions."
            ))
        })
}

/// One open, already checked for who may ask: opens `extension_id`'s view in `project_id`'s side
/// panel the way `requester` may.
pub(crate) async fn open_extension_view(
    state: &AppState,
    requester: OpenViewRequester,
    extension_id: &str,
    project_id: &str,
    background: bool,
) -> ExtensionResult<Value> {
    if let OpenViewRequester::Extension { id } = &requester {
        if id != extension_id {
            return Err(forbidden(format!(
                "An extension opens only its own view: {id} can't open {extension_id}."
            )));
        }
    }
    let (manifest, entry) = installed_extension(state, extension_id)?;
    if let OpenViewRequester::Extension { id } = &requester {
        if let Some(error) = extension_open_refusal(id, extension_id, &manifest, &entry) {
            return Err(error);
        }
    }
    if let Some(message) = extension_view_refusal(&manifest, &entry) {
        return Err(ExtensionError::bad_request(message));
    }
    // Like an agent's background page, a background view goes to the agent's own project,
    // whatever session it named (CDXC:SwitchRequests 2026-11-26).
    let project_id = match (&requester, background) {
        (OpenViewRequester::Agent { project_id, .. }, true) => project_id.clone(),
        _ => project_id.to_string(),
    };
    let opens_allowed =
        crate::switch_requests::agent_page_opens_allowed(state, OPEN_EXTENSION_VIEW_ACTION);
    let mode = open_view_mode(&requester, background, opens_allowed);
    if mode == OpenViewMode::Show {
        check_open_rate(&requester, &manifest.title)?;
    }
    let mut payload = Map::new();
    payload.insert("extensionId".to_string(), json!(extension_id));
    payload.insert("projectId".to_string(), json!(project_id));
    payload.insert("mode".to_string(), json!(mode.as_str()));
    // The user's own open goes to that project, as a click there would.
    if requester == OpenViewRequester::User {
        payload.insert("switchProject".to_string(), Value::Bool(true));
    }
    let answer = state
        .event_hub
        .dispatch_renderer_command(
            OPEN_EXTENSION_VIEW_ACTION.to_string(),
            payload.clone(),
            OPEN_VIEW_TIMEOUT_MS,
        )
        .await
        .map_err(|error| ExtensionError {
            code: "unavailable",
            message: if error.code == "dependencyUnavailable" {
                "The Ghostex app isn't open, so there is no side panel to open the view in."
                    .to_string()
            } else {
                error.message
            },
        })?;
    if answer.get("ok") == Some(&Value::Bool(false)) {
        let message = answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("The view could not be opened.")
            .to_string();
        return Err(ExtensionError::bad_request(message));
    }
    if answer.get("needsAsk") != Some(&Value::Bool(true)) {
        if mode == OpenViewMode::Show && answer.get("alreadyShowing") != Some(&Value::Bool(true)) {
            note_visible_open(&requester);
        }
        return Ok(answer);
    }
    check_open_rate(&requester, &manifest.title)?;
    note_visible_open(&requester);
    payload.insert("mode".to_string(), json!(OpenViewMode::Show.as_str()));
    // Open on the toast lands the user on that project, as Switch does.
    payload.insert("switchProject".to_string(), Value::Bool(true));
    // The app leaves Always off an extension's toast (CDXC:Extensions 2026-11-26 WHY above).
    if matches!(requester, OpenViewRequester::Extension { .. }) {
        payload.insert("requester".to_string(), json!("extension"));
    }
    let request = crate::switch_requests::emit_extension_view_toast(
        state,
        &requester,
        &manifest.title,
        &project_id,
        payload,
    );
    Ok(json!({
        "accepted": true,
        "action": OPEN_EXTENSION_VIEW_ACTION,
        "extensionId": extension_id,
        "ok": true,
        "projectId": project_id,
        "reason": "switchRequestToast",
        "shown": false,
        "switchRequestId": request.id,
        "toastShown": true,
    }))
}

/// `ghostex extensions open`'s endpoint: `{id, projectId?, sessionId?, background?}`.
pub(crate) const OPEN_EXTENSION_VIEW_ENDPOINT: &str = "/api/openExtensionView";

/// `/api/openExtensionView`: the project is the named session's, else the calling session's.
pub(crate) async fn handle_open_extension_view_http(
    state: &AppState,
    caller: &crate::security::Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> crate::server::RoutedResponse {
    use axum::http::StatusCode;
    let outcome = async {
        let params = crate::domain::read_domain_rpc_params(body).map_err(|error| {
            ExtensionError {
                code: error.code,
                message: error.message,
            }
        })?;
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let id = text("id").ok_or_else(|| {
            ExtensionError::bad_request("Name the extension: ghostex extensions open <id>.")
        })?;
        let background = params.get("background").and_then(Value::as_bool) == Some(true);
        let agent = crate::switch_requests::caller_agent_session(state, caller)
            .map(|(project_id, session_id)| (project_id.to_string(), session_id.to_string()));
        let (project_id, session_id) = match (text("projectId"), text("sessionId")) {
            (Some(project_id), session_id) => (project_id, session_id),
            (None, _) => match caller {
                crate::security::Principal::Session {
                    project_id,
                    session_id,
                    ..
                } => (project_id.clone(), Some(session_id.clone())),
                _ => {
                    return Err(ExtensionError::bad_request(
                        "Name the session whose project gets the view: --session <ref>. This terminal is not in a Ghostex session.",
                    ))
                }
            },
        };
        if let Some(session_id) = &session_id {
            if crate::switch_requests::lookup_session(state, &project_id, session_id).is_none() {
                return Err(ExtensionError::not_found(format!(
                    "No session {session_id} in project {project_id}. Run `ghostex sessions` to list sessions."
                )));
            }
        } else if crate::switch_requests::lookup_project_name(state, &project_id).is_none() {
            return Err(ExtensionError::not_found(format!(
                "No project {project_id} on this computer."
            )));
        }
        let requester = match (agent, caller) {
            (Some((project_id, session_id)), _) => OpenViewRequester::Agent {
                project_id,
                session_id,
            },
            // Another computer's agents ask like this computer's do.
            (None, crate::security::Principal::Peer { .. }) => OpenViewRequester::Agent {
                project_id: project_id.clone(),
                session_id: String::new(),
            },
            (None, _) => OpenViewRequester::User,
        };
        open_extension_view(state, requester, &id, &project_id, background).await
    }
    .await;
    match outcome {
        Ok(result) => crate::server::routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            crate::protocol::rpc_success(request_id, result),
        ),
        Err(error) => {
            let status = match error.code {
                "badRequest" => StatusCode::BAD_REQUEST,
                "notFound" => StatusCode::NOT_FOUND,
                "forbidden" => StatusCode::FORBIDDEN,
                "unavailable" => StatusCode::SERVICE_UNAVAILABLE,
                "rateLimited" => StatusCode::TOO_MANY_REQUESTS,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            crate::server::routed_json(
                Some(endpoint_path),
                status,
                crate::protocol::rpc_error(error.code, error.message, Some(request_id)),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn manifest(id: &str, placements: Vec<ExtensionPlacement>) -> ExtensionManifest {
        serde_json::from_value(json!({
            "name": id,
            "title": "Mobile Pane",
            "description": "Mirrors a phone.",
            "version": "1.0.0",
            "author": "someone",
            "icon": "icon.svg",
            "categories": ["Tools"],
            "permissions": ["views.open"],
            "placements": placements,
            "defaultPlacement": "view",
        }))
        .unwrap()
    }

    fn entry(
        placement: ExtensionPlacement,
        granted: Vec<ExtensionPermission>,
    ) -> ExtensionStoreEntry {
        ExtensionStoreEntry {
            enabled: true,
            pinned: false,
            chat_bar_auto_open: false,
            placement: Some(placement),
            terminal_placement: super::super::ExtensionTerminalPlacement::SplitRight,
            preferences: BTreeMap::new(),
            storage: BTreeMap::new(),
            version: "1.0.0".to_string(),
            granted_permissions: granted,
            sidebar_mode_hidden: false,
            sidebar_sections_hidden: false,
            skill_installed: None,
        }
    }

    #[test]
    fn an_extension_opens_only_its_own_view() {
        let manifest = manifest("mobile-pane", vec![ExtensionPlacement::View]);
        let state = entry(
            ExtensionPlacement::View,
            vec![ExtensionPermission::ViewsOpen],
        );
        assert!(extension_open_refusal("mobile-pane", "mobile-pane", &manifest, &state).is_none());
        let refusal = extension_open_refusal("other-ext", "mobile-pane", &manifest, &state)
            .expect("another extension's view is refused");
        assert_eq!(refusal.code, "forbidden");
        assert!(refusal.message.contains("only its own view"));
    }

    #[test]
    fn opening_needs_views_open_granted() {
        let manifest = manifest("mobile-pane", vec![ExtensionPlacement::View]);
        let declared_not_granted = entry(ExtensionPlacement::View, vec![]);
        let refusal = extension_open_refusal(
            "mobile-pane",
            "mobile-pane",
            &manifest,
            &declared_not_granted,
        )
        .expect("not granted is refused");
        assert!(refusal.message.contains("hasn't granted views.open"));
        let mut undeclared = manifest.clone();
        undeclared.permissions.clear();
        let refusal = extension_open_refusal(
            "mobile-pane",
            "mobile-pane",
            &undeclared,
            &declared_not_granted,
        )
        .expect("undeclared is refused");
        assert!(refusal.message.contains("doesn't declare"));
        // Another granted permission is not this one.
        let other_grant = entry(
            ExtensionPlacement::View,
            vec![ExtensionPermission::SessionsOpen],
        );
        assert!(
            extension_open_refusal("mobile-pane", "mobile-pane", &manifest, &other_grant).is_some()
        );
    }

    #[test]
    fn only_an_enabled_view_placement_opens() {
        let manifest = manifest(
            "mobile-pane",
            vec![ExtensionPlacement::View, ExtensionPlacement::ChatBar],
        );
        assert_eq!(
            extension_view_refusal(&manifest, &entry(ExtensionPlacement::View, vec![])),
            None
        );
        let mut off = entry(ExtensionPlacement::View, vec![]);
        off.enabled = false;
        assert!(extension_view_refusal(&manifest, &off)
            .unwrap()
            .contains("turned off"));
        assert!(
            extension_view_refusal(&manifest, &entry(ExtensionPlacement::ChatBar, vec![]))
                .unwrap()
                .contains("set to open as a chat bar panel")
        );
        let popup_only = self::manifest("pop", vec![ExtensionPlacement::Popup]);
        assert!(
            extension_view_refusal(&popup_only, &entry(ExtensionPlacement::Popup, vec![]))
                .unwrap()
                .contains("no side panel view")
        );
    }

    #[test]
    fn visible_opens_are_limited_per_agent_or_extension_never_for_the_user() {
        let now = Instant::now();
        assert_eq!(open_wait(None, now), None);
        assert!(open_wait(Some(now), now).is_some());
        assert_eq!(open_wait(Some(now), now + OPEN_VIEW_MIN_INTERVAL), None);
        assert_eq!(rate_key(&OpenViewRequester::User), None);
        assert!(check_open_rate(&OpenViewRequester::User, "Mobile Pane").is_ok());
        let extension = OpenViewRequester::Extension {
            id: "rate-test-extension".to_string(),
        };
        assert!(check_open_rate(&extension, "Mobile Pane").is_ok());
        note_visible_open(&extension);
        let refusal = check_open_rate(&extension, "Mobile Pane").expect_err("too soon");
        assert_eq!(refusal.code, "rateLimited");
        assert!(refusal.message.contains("--background"));
        // Another requester is not held back by this one.
        let agent = OpenViewRequester::Agent {
            project_id: "P-rate".to_string(),
            session_id: "S-rate".to_string(),
        };
        assert!(check_open_rate(&agent, "Mobile Pane").is_ok());
    }

    #[test]
    fn agents_and_extensions_ask_unless_background_or_allowed() {
        let agent = OpenViewRequester::Agent {
            project_id: "P1".to_string(),
            session_id: "S1".to_string(),
        };
        let extension = OpenViewRequester::Extension {
            id: "mobile-pane".to_string(),
        };
        for requester in [&agent, &extension] {
            assert_eq!(open_view_mode(requester, false, false), OpenViewMode::Ask);
            assert_eq!(
                open_view_mode(requester, true, false),
                OpenViewMode::Background
            );
            assert_eq!(open_view_mode(requester, false, true), OpenViewMode::Show);
        }
        assert_eq!(
            open_view_mode(&OpenViewRequester::User, false, false),
            OpenViewMode::Show
        );
    }
}
