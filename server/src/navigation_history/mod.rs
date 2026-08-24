/*
CDXC:NavigationHistory 2026-08-19:
Back/Forward in the titlebar walks ONE chronological trail of everything the
user has had active — sessions and projects, across machines — not a per-project
stack. The trail lives here, in the daemon, because two clients render it (the
gpui desktop titlebar in Rust and the web titlebar in React) and a stack that
each client re-derived on its own would disagree about what "back" means the
moment either one reloaded.

Rules this module commits to:
- gxserver owns the model. Clients send a visit and get the resulting state
  back; they never compute cursor movement, truncation, or button availability
  themselves. `canGoBack`/`canGoForward` in the response ARE the disabled state.
- Entry ids are OPAQUE. A project/session/group id is a routing key this module
  stores and hands back verbatim; it is never parsed, resolved against the
  sessions table, or assumed to belong to this machine. That is what lets one
  trail hold entries from several machines at once, and it is why a client must
  tell us (through `forgetKeys`) when a target no longer resolves.
- Scopes are separate trails. The desktop app and the web app both talk to this
  daemon and must not share a cursor, so each passes its own `scopeId`.
- State is in memory. History is ephemeral UI state, and the daemon outlives app
  restarts, which is the case that matters; keeping it out of SQLite means a
  project switch costs a hash lookup instead of a write. A daemon restart starts
  a fresh trail, exactly like a fresh window.
- Labels are the same project/session titles the sidebar already renders, kept
  only so the buttons can say "Back to <name>". Never paths, prompts, command
  text, tokens, or terminal output — and this module never logs them.
*/

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

/// Trail length cap. Deep enough that Back keeps working across a long working
/// session, bounded so an app that never stops switching cannot grow the table.
pub const MAX_NAVIGATION_HISTORY_ENTRIES: usize = 100;
/// Scope used when a client does not identify itself.
pub const DEFAULT_NAVIGATION_HISTORY_SCOPE: &str = "default";

const MAX_SCOPES: usize = 16;
const MAX_SCOPE_CHARS: usize = 128;
const MAX_ID_CHARS: usize = 512;
const MAX_LABEL_CHARS: usize = 256;
const MAX_FORGET_KEYS: usize = 64;
const KEY_SEPARATOR: char = '\u{1f}';

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigationHistoryEntry {
    pub project_id: String,
    pub session_id: Option<String>,
    pub group_id: Option<String>,
    pub project_label: Option<String>,
    pub session_label: Option<String>,
}

impl NavigationHistoryEntry {
    /// Identity of a visit: the project plus the session inside it. Group ids and
    /// labels are display/routing detail and deliberately stay out of the key, so
    /// a renamed project or a regrouped session is still the same trail stop.
    pub fn key(&self) -> String {
        format!(
            "{}{KEY_SEPARATOR}{}",
            self.project_id,
            self.session_id.as_deref().unwrap_or_default()
        )
    }

    fn to_json(&self) -> Value {
        let mut entry = Map::new();
        entry.insert("key".to_string(), json!(self.key()));
        entry.insert("projectId".to_string(), json!(self.project_id));
        if let Some(session_id) = &self.session_id {
            entry.insert("sessionId".to_string(), json!(session_id));
        }
        if let Some(group_id) = &self.group_id {
            entry.insert("groupId".to_string(), json!(group_id));
        }
        if let Some(project_label) = &self.project_label {
            entry.insert("projectLabel".to_string(), json!(project_label));
        }
        if let Some(session_label) = &self.session_label {
            entry.insert("sessionLabel".to_string(), json!(session_label));
        }
        Value::Object(entry)
    }
}

#[derive(Debug, Default)]
struct ScopeHistory {
    entries: Vec<NavigationHistoryEntry>,
    /// Index of the entry the user is on. Meaningless while `entries` is empty.
    cursor: usize,
}

impl ScopeHistory {
    fn state(&self) -> Value {
        let can_go_back = !self.entries.is_empty() && self.cursor > 0;
        let can_go_forward = self.cursor + 1 < self.entries.len();
        let mut state = Map::new();
        state.insert("canGoBack".to_string(), json!(can_go_back));
        state.insert("canGoForward".to_string(), json!(can_go_forward));
        state.insert("entryCount".to_string(), json!(self.entries.len()));
        if let Some(current) = self.entries.get(self.cursor) {
            state.insert("currentEntry".to_string(), current.to_json());
        }
        if can_go_back {
            if let Some(previous) = self.entries.get(self.cursor - 1) {
                state.insert("backEntry".to_string(), previous.to_json());
            }
        }
        if can_go_forward {
            if let Some(next) = self.entries.get(self.cursor + 1) {
                state.insert("forwardEntry".to_string(), next.to_json());
            }
        }
        Value::Object(state)
    }

    /// Record the active project/session as the newest trail stop.
    ///
    /// Re-visiting the entry the cursor already sits on only refreshes its
    /// labels. That is what makes Back safe: after a Back the client activates
    /// the target, the activation reports itself as a visit, and this collapse
    /// keeps the forward branch alive instead of truncating it away.
    ///
    /// `replace_current` is the same protection one step wider. Landing on a
    /// trail stop can REFINE it — Back to a project, and the app focuses a
    /// session inside that project a moment later. That is not the user walking
    /// somewhere new, so the client marks it as a replacement and the forward
    /// branch it just walked away from survives.
    fn visit(&mut self, entry: NavigationHistoryEntry, replace_current: bool) {
        if let Some(current) = self.entries.get_mut(self.cursor) {
            if replace_current || current.key() == entry.key() {
                *current = entry;
                return;
            }
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.cursor + 1);
        }
        self.entries.push(entry);
        let overflow = self
            .entries
            .len()
            .saturating_sub(MAX_NAVIGATION_HISTORY_ENTRIES);
        if overflow > 0 {
            self.entries.drain(0..overflow);
        }
        self.cursor = self.entries.len().saturating_sub(1);
    }

    /// Drop entries the client reported as unresolvable (its project or session
    /// is gone) and keep the cursor pointing at the same place in the trail.
    fn forget(&mut self, keys: &[String]) {
        if keys.is_empty() || self.entries.is_empty() {
            return;
        }
        let mut kept: Vec<NavigationHistoryEntry> = Vec::with_capacity(self.entries.len());
        let mut next_cursor = 0usize;
        for (index, entry) in self.entries.iter().enumerate() {
            if keys.iter().any(|key| key == &entry.key()) {
                continue;
            }
            if index < self.cursor {
                next_cursor += 1;
            }
            kept.push(entry.clone());
        }
        self.entries = kept;
        self.cursor = next_cursor.min(self.entries.len().saturating_sub(1));
    }

    fn go(&mut self, back: bool) -> Option<NavigationHistoryEntry> {
        if back {
            if self.entries.is_empty() || self.cursor == 0 {
                return None;
            }
            self.cursor -= 1;
        } else {
            if self.cursor + 1 >= self.entries.len() {
                return None;
            }
            self.cursor += 1;
        }
        self.entries.get(self.cursor).cloned()
    }
}

type ScopeTable = HashMap<String, ScopeHistory>;

fn scopes() -> &'static Mutex<ScopeTable> {
    static SCOPES: OnceLock<Mutex<ScopeTable>> = OnceLock::new();
    SCOPES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn normalize_scope_id(requested: Option<&str>) -> String {
    let trimmed = requested.unwrap_or("").trim();
    if trimmed.is_empty() {
        return DEFAULT_NAVIGATION_HISTORY_SCOPE.to_string();
    }
    trimmed.chars().take(MAX_SCOPE_CHARS).collect()
}

fn bounded_string(value: Option<&str>, max_chars: usize) -> Option<String> {
    let trimmed = value.unwrap_or("").trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(max_chars).collect())
}

fn read_entry(params: &Map<String, Value>) -> Result<NavigationHistoryEntry, DomainStateError> {
    let entry = params
        .get("entry")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            DomainStateError::bad_request("recordNavigationVisit requires an entry object.")
        })?;
    let project_id = bounded_string(entry.get("projectId").and_then(Value::as_str), MAX_ID_CHARS)
        .ok_or_else(|| {
        DomainStateError::bad_request("A navigation visit requires a non-empty projectId.")
    })?;
    Ok(NavigationHistoryEntry {
        project_id,
        session_id: bounded_string(entry.get("sessionId").and_then(Value::as_str), MAX_ID_CHARS),
        group_id: bounded_string(entry.get("groupId").and_then(Value::as_str), MAX_ID_CHARS),
        project_label: bounded_string(
            entry.get("projectLabel").and_then(Value::as_str),
            MAX_LABEL_CHARS,
        ),
        session_label: bounded_string(
            entry.get("sessionLabel").and_then(Value::as_str),
            MAX_LABEL_CHARS,
        ),
    })
}

fn with_scope<T>(scope_id: &str, apply: impl FnOnce(&mut ScopeHistory) -> T) -> T {
    let mut table = scopes().lock().expect("navigation history table poisoned");
    if !table.contains_key(scope_id) && table.len() >= MAX_SCOPES {
        // Scope ids are fixed per client build, so reaching the cap means a
        // caller is generating them. Start over rather than grow without bound.
        table.clear();
    }
    apply(table.entry(scope_id.to_string()).or_default())
}

/// `/api/readNavigationHistory` — the trail state for one client scope.
pub fn read_navigation_history(params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let scope_id = normalize_scope_id(params.get("scopeId").and_then(Value::as_str));
    let state = with_scope(&scope_id, |history| history.state());
    Ok(json!({ "navigationHistory": state, "scopeId": scope_id }))
}

/// `/api/recordNavigationVisit` — the client's active project/session changed.
pub fn record_navigation_visit(params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let scope_id = normalize_scope_id(params.get("scopeId").and_then(Value::as_str));
    let entry = read_entry(params)?;
    let replace_current = params
        .get("replaceCurrent")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let state = with_scope(&scope_id, |history| {
        history.visit(entry, replace_current);
        history.state()
    });
    Ok(json!({ "navigationHistory": state, "scopeId": scope_id }))
}

/// `/api/navigateHistory` — move the cursor and hand back the entry to activate.
///
/// `forgetKeys` carries entries the client just failed to resolve, so a killed
/// session or a removed project is dropped from the trail on the retry instead
/// of parking Back on a stop that can never be reached again.
pub fn navigate_history(params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let scope_id = normalize_scope_id(params.get("scopeId").and_then(Value::as_str));
    let direction = params
        .get("direction")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let back = match direction {
        "back" => true,
        "forward" => false,
        _ => {
            return Err(DomainStateError::bad_request(
                "navigateHistory requires a direction of back or forward.",
            ));
        }
    };
    let forget_keys: Vec<String> = params
        .get("forgetKeys")
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .filter_map(|key| bounded_string(key.as_str(), MAX_ID_CHARS * 2))
                .take(MAX_FORGET_KEYS)
                .collect()
        })
        .unwrap_or_default();

    let (target, state) = with_scope(&scope_id, |history| {
        history.forget(&forget_keys);
        let target = history.go(back);
        (target, history.state())
    });

    let mut response = Map::new();
    response.insert("navigationHistory".to_string(), state);
    response.insert("scopeId".to_string(), json!(scope_id));
    if let Some(target) = target {
        response.insert("target".to_string(), target.to_json());
    }
    Ok(Value::Object(response))
}
