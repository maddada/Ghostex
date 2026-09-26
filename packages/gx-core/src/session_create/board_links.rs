//! The Project Board's conversation links: the durable records that tie a Beads ticket to the
//! agent conversation that worked it, read and rewritten the way `bead-conversation-links.ts` does.
//!
//! CDXC:ProjectBoard 2026-05-26-10:16:
//! Project-board cards need durable Ghostex-owned links from Beads tickets to agent conversations.
//! Keep these records outside Beads issue fields so one conversation can own multiple beads without
//! abusing unique external refs or polluting comments/labels with app-routing metadata.
//!
//! CDXC:ProjectBoard 2026-08-07:
//! Beads rewrites every issue id when a board's issue prefix is reconciled (zmux-95421485 becomes
//! ghostex-95421485), but a persisted link keeps the id it was written with. Matching links to a
//! bead by the trailing issue id keeps the conversation attached across those renames. One board can
//! be mounted by several project rows (a checkout and its worktree, the same folder registered
//! twice) and every row keeps its own links, so links are read across the rows that share a board
//! and each write stays on its own row. A session id is stored relative to the row that owns the
//! link, so links are re-expressed against the board project before they are shown.
//!
//! SEE-ALSO: packages/shared/bead-conversation-links.ts, server/src/board_start_work.rs (the
//! daemon's writer of the same links).

use std::collections::{BTreeSet, HashMap};

use serde_json::{json, Map, Value};

use crate::keys::SessionKey;
use crate::sidebar_view::text::{js_trim, parse_iso_ms};

/// One link, with every optional field absent rather than empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeadConversationLink {
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    pub bead_display_id: Option<String>,
    pub bead_id: String,
    pub created_at: String,
    pub ghostex_session_id: String,
    pub id: String,
    pub project_id: String,
    pub session_persistence_name: Option<String>,
    pub session_persistence_provider: Option<String>,
    pub session_project_id: Option<String>,
    /// `active` or `archived`.
    pub status: String,
    pub updated_at: String,
}

fn text(record: &Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .map(js_trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn date(record: &Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| parse_iso_ms(value).is_some())
        .map(str::to_string)
}

impl BeadConversationLink {
    /// `normalizeBeadConversationLink(candidate, fallbackProjectId)`. `now_iso` stands in for the
    /// `new Date().toISOString()` a record without dates gets.
    pub fn normalize(candidate: &Value, fallback_project_id: &str, now_iso: &str) -> Option<Self> {
        let record = candidate.as_object()?;
        let bead_id = text(record, "beadId")?;
        let ghostex_session_id = text(record, "ghostexSessionId")?;
        let id = text(record, "id").unwrap_or_else(|| {
            bead_conversation_link_id(fallback_project_id, &bead_id, &ghostex_session_id)
        });
        Some(Self {
            agent_id: text(record, "agentId"),
            agent_name: text(record, "agentName"),
            agent_session_id: text(record, "agentSessionId"),
            agent_session_path: text(record, "agentSessionPath"),
            bead_display_id: text(record, "beadDisplayId"),
            bead_id,
            created_at: date(record, "createdAt").unwrap_or_else(|| now_iso.to_string()),
            ghostex_session_id,
            id,
            project_id: text(record, "projectId").unwrap_or_else(|| fallback_project_id.to_string()),
            session_persistence_name: text(record, "sessionPersistenceName"),
            session_persistence_provider: record
                .get("sessionPersistenceProvider")
                .and_then(Value::as_str)
                .filter(|value| matches!(*value, "tmux" | "zmx" | "zellij"))
                .map(str::to_string),
            session_project_id: text(record, "sessionProjectId"),
            status: match record.get("status").and_then(Value::as_str) {
                Some("archived") => "archived".to_string(),
                _ => "active".to_string(),
            },
            updated_at: date(record, "updatedAt").unwrap_or_else(|| now_iso.to_string()),
        })
    }

    /// The record as `JSON.stringify` writes it: absent fields left out, keys in the TypeScript's
    /// object order.
    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        let mut put = |key: &str, value: &Option<String>| {
            if let Some(value) = value {
                map.insert(key.to_string(), json!(value));
            }
        };
        put("agentId", &self.agent_id);
        put("agentName", &self.agent_name);
        put("agentSessionId", &self.agent_session_id);
        put("agentSessionPath", &self.agent_session_path);
        put("beadDisplayId", &self.bead_display_id);
        map.insert("beadId".into(), json!(self.bead_id));
        map.insert("createdAt".into(), json!(self.created_at));
        map.insert("ghostexSessionId".into(), json!(self.ghostex_session_id));
        map.insert("id".into(), json!(self.id));
        map.insert("projectId".into(), json!(self.project_id));
        let mut put = |key: &str, value: &Option<String>| {
            if let Some(value) = value {
                map.insert(key.to_string(), json!(value));
            }
        };
        put("sessionPersistenceName", &self.session_persistence_name);
        put("sessionPersistenceProvider", &self.session_persistence_provider);
        put("sessionProjectId", &self.session_project_id);
        map.insert("status".into(), json!(self.status));
        map.insert("updatedAt".into(), json!(self.updated_at));
        Value::Object(map)
    }
}

/// `normalizeBeadConversationLinks(candidate, projectId)`.
pub fn normalize_bead_conversation_links(
    candidate: Option<&Value>,
    project_id: &str,
    now_iso: &str,
) -> Vec<BeadConversationLink> {
    candidate
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| BeadConversationLink::normalize(entry, project_id, now_iso))
                .collect()
        })
        .unwrap_or_default()
}

/// The links a project row stores (`projectBoardConfig.beadConversationLinks`).
pub fn project_board_links(project: &Value, now_iso: &str) -> Vec<BeadConversationLink> {
    let project_id = project.get("projectId").and_then(Value::as_str).unwrap_or_default();
    normalize_bead_conversation_links(
        project
            .get("projectBoardConfig")
            .and_then(|config| config.get("beadConversationLinks")),
        project_id,
        now_iso,
    )
}

/// `createBeadConversationLinkId(projectId, beadId, ghostexSessionId)`.
pub fn bead_conversation_link_id(project_id: &str, bead_id: &str, ghostex_session_id: &str) -> String {
    [project_id, bead_id, ghostex_session_id]
        .iter()
        .map(|part| {
            let mut out = String::new();
            let mut in_run = false;
            for character in js_trim(part).chars() {
                if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                    out.push(character);
                    in_run = false;
                } else if !in_run {
                    out.push('-');
                    in_run = true;
                }
            }
            out.trim_matches('-').to_string()
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(":")
}

/// `beadConversationLinkMatchKey(beadId)`: the trailing issue id, lowercased.
pub fn bead_conversation_link_match_key(bead_id: &str) -> String {
    let normalized = js_trim(bead_id).to_lowercase();
    match normalized.rfind('-') {
        Some(index) if index > 0 => {
            let tail = &normalized[index + 1..];
            if tail.is_empty() {
                normalized.clone()
            } else {
                tail.to_string()
            }
        }
        _ => normalized,
    }
}

/// `beadConversationLinkStoreKey(project, globalBeadsDirectory)`.
pub fn bead_conversation_link_store_key(project: &Value, global_beads_directory: &str) -> String {
    let configured = project
        .get("projectBoardConfig")
        .and_then(|config| config.get("beadsDirectory"))
        .and_then(Value::as_str)
        .filter(|directory| !js_trim(directory).is_empty());
    let directory = configured
        .or_else(|| {
            (!js_trim(global_beads_directory).is_empty()).then_some(global_beads_directory)
        })
        .or_else(|| project.get("path").and_then(Value::as_str));
    directory
        .map(|directory| js_trim(directory).trim_end_matches('/').to_string())
        .unwrap_or_default()
}

/// `selectBeadConversationLinkStoreProjects(boardProject, projects, globalBeadsDirectory)`.
pub fn select_link_store_projects(
    board_project: &Value,
    projects: &[Value],
    global_beads_directory: &str,
) -> Vec<Value> {
    let board_key = bead_conversation_link_store_key(board_project, global_beads_directory);
    let mut store = vec![board_project.clone()];
    if board_key.is_empty() {
        return store;
    }
    let board_id = board_project.get("projectId");
    for candidate in projects {
        if candidate.get("projectId") != board_id
            && bead_conversation_link_store_key(candidate, global_beads_directory) == board_key
        {
            store.push(candidate.clone());
        }
    }
    store
}

/// `parseBeadConversationLinkSessionPersistenceName`: `{serverId}-{projectId}-{sessionId}`.
fn parse_persistence_name(name: Option<&str>) -> Option<(String, String)> {
    let parts: Vec<&str> = js_trim(name.unwrap_or("")).split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    if !parts[0].starts_with('S') || !parts[1].starts_with('P') || !parts[2].starts_with('G') {
        return None;
    }
    Some((parts[1].to_string(), parts[2].to_string()))
}

/// `resolveBeadConversationLinkSessionOwner`: `(isKnown, projectId, sessionId)`.
fn session_owner(link: &BeadConversationLink) -> (bool, String, String) {
    if let Some(scoped) = SessionKey::parse_sidebar_session_id(&link.ghostex_session_id)
        .filter(|key| key.machine.is_local())
    {
        return (true, scoped.project_id, scoped.session_id);
    }
    if let Some(project_id) = link
        .session_project_id
        .as_deref()
        .map(js_trim)
        .filter(|id| !id.is_empty())
    {
        return (true, project_id.to_string(), link.ghostex_session_id.clone());
    }
    if let Some((project_id, session_id)) =
        parse_persistence_name(link.session_persistence_name.as_deref())
    {
        if session_id == link.ghostex_session_id {
            return (true, project_id, session_id);
        }
    }
    (false, link.project_id.clone(), link.ghostex_session_id.clone())
}

/// `resolveBeadConversationLinkReconciledSessionOwner`.
fn reconciled_owner(
    link: &BeadConversationLink,
    related: &[BeadConversationLink],
) -> (String, String) {
    let (known, project_id, session_id) = session_owner(link);
    if known {
        return (project_id, session_id);
    }
    let bead_key = bead_conversation_link_match_key(&link.bead_id);
    let mut known_projects = BTreeSet::new();
    for candidate in related {
        if bead_conversation_link_match_key(&candidate.bead_id) != bead_key {
            continue;
        }
        let (candidate_known, candidate_project, candidate_session) = session_owner(candidate);
        if candidate_known && candidate_session == session_id {
            known_projects.insert(candidate_project);
        }
    }
    if known_projects.len() == 1 {
        let project = known_projects.into_iter().next().unwrap_or(project_id);
        return (project, session_id);
    }
    (project_id, session_id)
}

/// A session id as the board names it: raw in the board project, scoped in another.
pub fn board_session_id(project_id: &str, session_id: &str, board_project_id: &str) -> String {
    if project_id == board_project_id {
        session_id.to_string()
    } else {
        SessionKey::local(project_id, session_id).to_sidebar_session_id()
    }
}

/// `resolveBeadConversationLinkBoardSessionId(link, boardProjectId, relatedLinks)`.
pub fn resolve_board_session_id(
    link: &BeadConversationLink,
    board_project_id: &str,
    related: &[BeadConversationLink],
) -> String {
    let (project_id, session_id) = reconciled_owner(link, related);
    board_session_id(&project_id, &session_id, board_project_id)
}

/// `canonicalizeBeadConversationLinksForBoard(links, boardProjectId)`: each conversation once,
/// re-expressed against the board project, the newest record kept.
pub fn canonicalize_links_for_board(
    links: &[BeadConversationLink],
    board_project_id: &str,
) -> Vec<BeadConversationLink> {
    let mut order: Vec<String> = Vec::new();
    let mut by_conversation: HashMap<String, BeadConversationLink> = HashMap::new();
    for link in links {
        let (project_id, session_id) = reconciled_owner(link, links);
        let ghostex_session_id = board_session_id(&project_id, &session_id, board_project_id);
        let key = format!(
            "{}\u{1f}{}",
            bead_conversation_link_match_key(&link.bead_id),
            ghostex_session_id
        );
        if let Some(current) = by_conversation.get(&key) {
            let current_ms = parse_iso_ms(&current.updated_at);
            let next_ms = parse_iso_ms(&link.updated_at);
            // `Date.parse(current) >= Date.parse(next)`: NaN on either side compares false.
            if let (Some(current_ms), Some(next_ms)) = (current_ms, next_ms) {
                if current_ms >= next_ms {
                    continue;
                }
            }
        } else {
            order.push(key.clone());
        }
        by_conversation.insert(
            key,
            BeadConversationLink {
                ghostex_session_id,
                session_project_id: Some(project_id),
                ..link.clone()
            },
        );
    }
    order
        .into_iter()
        .filter_map(|key| by_conversation.remove(&key))
        .collect()
}
