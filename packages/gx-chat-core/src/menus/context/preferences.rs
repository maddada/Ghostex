//! The saved row preferences, and what they resolve to.
//!
//! Port of the preferences and resolution halves of
//! `packages/shared/session-chat-presentation/context-details.ts`.
//!
//! **The stored record is user data.** `ghostex.chat.context-details.v1` (Claude) and
//! `ghostex.chat.context-details.codex.v1` (Codex) hold `{shown, starred, order, starredOrder}`
//! and nothing else, and normalization is what makes an old record readable: a saved id for a row
//! that was retired on 2026-09-11 carries over to the rows that now show its values.
//!
//! CDXC:AgentProviders 2026-09-08 DECISION:
//! User: keep the same Claude UI and status line, but save popover and status-line settings
//! independently for Claude and Codex. Claude keeps its existing storage key and configuration;
//! copying to the other agent is a one-time action.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use crate::event::StorageKey;
use crate::menus::context::rows::{
    context_detail_row, context_detail_rows, is_row_id, ContextDetailSession, GroupId,
    RowDefinition, RowInput, CONTEXT_DETAIL_GROUPS,
};
use crate::menus::context::status::{ContextDetailStatus, ContextDetailsAgent};

/// The store the two records live in; the host owns the key prefixes.
pub const CONTEXT_PREFERENCES_STORE_CLAUDE: &str = "claudeContext";
pub const CONTEXT_PREFERENCES_STORE_CODEX: &str = "codexContext";
pub const CONTEXT_PREFERENCES_STORE_CURSOR: &str = "cursorContext";
pub const CONTEXT_PREFERENCES_STORE_HERMES: &str = "hermesContext";
pub const CONTEXT_PREFERENCES_STORE_PI: &str = "piContext";
pub const CONTEXT_PREFERENCES_STORE_BASIC: &str = "basicContext";

/// The record for one agent.
pub fn context_preferences_key(agent: ContextDetailsAgent) -> StorageKey {
    StorageKey {
        store: match agent {
            ContextDetailsAgent::Claude => CONTEXT_PREFERENCES_STORE_CLAUDE,
            ContextDetailsAgent::Codex => CONTEXT_PREFERENCES_STORE_CODEX,
            ContextDetailsAgent::Cursor => CONTEXT_PREFERENCES_STORE_CURSOR,
            ContextDetailsAgent::Hermes => CONTEXT_PREFERENCES_STORE_HERMES,
            ContextDetailsAgent::Pi => CONTEXT_PREFERENCES_STORE_PI,
            ContextDetailsAgent::Basic => CONTEXT_PREFERENCES_STORE_BASIC,
        }
        .to_string(),
        suffix: String::new(),
    }
}

/// What the popover, the status line and the dialog read.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDetailsPreferences {
    /// Row shown in the popover. Absent means the row's `recommended` flag.
    #[serde(default)]
    pub shown: BTreeMap<String, bool>,
    /// Row rendered in the status line under the chat box. Absent means off.
    #[serde(default)]
    pub starred: BTreeMap<String, bool>,
    /// Per-group row order; rows missing here follow in catalog order.
    #[serde(default)]
    pub order: BTreeMap<String, Vec<String>>,
    /// The status line's own order, independent of the groups: starred rows missing here follow
    /// in group order.
    #[serde(default)]
    pub starred_order: Vec<String>,
}

/// Rows retired on 2026-09-11 when every row became one value.
///
/// A saved preference for a retired row carries over to the rows that now show its values, so a
/// starred "Rate limits" or "Cost" keeps its place in the status line after the update; the next
/// save stores only current ids.
fn retired_replacements(saved: &str) -> Option<&'static [&'static str]> {
    Some(match saved {
        "rateLimits" => &["fiveHourLimit", "sevenDayLimit", "fiveHourReset"],
        "accountLimits" => &[
            "fiveHourLimit",
            "sevenDayLimit",
            "modelLimit",
            "fiveHourReset",
            "sevenDayReset",
        ],
        "accountPrimaryLimit" => &["fiveHourLimit", "fiveHourReset"],
        "accountWeeklyLimit" => &["sevenDayLimit", "sevenDayReset"],
        "accountModelLimits" => &["modelLimit"],
        "primaryLimit" => &["fiveHourLimit", "fiveHourReset"],
        "secondaryLimit" => &["sevenDayLimit", "sevenDayReset"],
        "cost" => &["costUsd", "sessionTime", "apiTime"],
        "promptCache" => &["cacheState", "cacheTimeLeft", "cacheHitRate"],
        "lastRequest" => &["lastRequestInput", "lastRequestOutput", "lastRequestCached"],
        "remaining" => &["contextUsed"],
        "permissions" => &["sandbox", "approvalPolicy"],
        _ => return None,
    })
}

/// `currentRowIds`: the current row ids a saved id stands for.
fn current_row_ids(saved: &str, agent: ContextDetailsAgent) -> Vec<String> {
    if is_row_id(saved, agent) {
        return vec![saved.to_string()];
    }
    retired_replacements(saved)
        .map(|replacements| {
            replacements
                .iter()
                .filter(|id| is_row_id(id, agent))
                .map(|id| (*id).to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// `normalizeFlags`.
fn normalize_flags(
    candidate: Option<&Value>,
    agent: ContextDetailsAgent,
) -> BTreeMap<String, bool> {
    let mut flags = BTreeMap::new();
    let Some(Value::Object(object)) = candidate else {
        return flags;
    };
    for (id, flag) in object {
        let Some(flag) = flag.as_bool() else {
            continue;
        };
        if is_row_id(id, agent) {
            flags.insert(id.clone(), flag);
            continue;
        }
        // A retired row's flag never overrides one saved for a current row.
        for replacement in current_row_ids(id, agent) {
            flags.entry(replacement).or_insert(flag);
        }
    }
    flags
}

/// `normalizeOrder`.
fn normalize_order(
    candidate: Option<&Value>,
    agent: ContextDetailsAgent,
) -> BTreeMap<String, Vec<String>> {
    let mut order = BTreeMap::new();
    let Some(Value::Object(object)) = candidate else {
        return order;
    };
    let rows = context_detail_rows(agent);
    for group in CONTEXT_DETAIL_GROUPS {
        let Some(Value::Array(ids)) = object.get(group.as_str()) else {
            continue;
        };
        let mut kept: Vec<String> = Vec::new();
        for id in ids {
            let Some(id) = id.as_str() else {
                continue;
            };
            for current in current_row_ids(id, agent) {
                if rows
                    .iter()
                    .find(|row| row.id == current)
                    .is_some_and(|row| row.group == group)
                    && !kept.contains(&current)
                {
                    kept.push(current);
                }
            }
        }
        order.insert(group.as_str().to_string(), kept);
    }
    order
}

/// `defaultSessionChatContextDetailsPreferences`: what an agent uses before anything is saved. The
/// recommended popover rows, and nothing pinned to the status line under the chat box.
///
/// CDXC:SessionChatDetectedOptions 2026-11-09 DECISION:
/// User: "let's please disable showing the status line below the chat composer for claude/codex/etc by default for new users (nothing pinned there in statusline config). keep people's configs as they are tho". A record that was never saved pins nothing, for every agent, so no bar shows under the chat box until the person pins something. Everyone who already had Ghostex keeps the line they saw: the chat host writes [`recommended_preferences`] into each agent's record once (`gx_chat/status_line_default.rs`). This supersedes the 2026-09-23 default below.
pub fn default_preferences(agent: ContextDetailsAgent) -> ContextDetailsPreferences {
    ContextDetailsPreferences {
        starred: BTreeMap::new(),
        starred_order: Vec::new(),
        ..recommended_preferences(agent)
    }
}

/// The maintainer's own setup per agent: what Reset to recommended restores, and what every
/// existing user's never-saved record was before 2026-11-09 (which the one-time write keeps).
///
/// CDXC:SessionChatDetectedOptions 2026-09-23 DECISION:
/// User: a new install starts with the maintainer's own "More details" and status-line setup for
/// Claude and Codex, and Reset to recommended returns to it. Claude stars Account, Model limit, 5h
/// limit, 7d limit and Repository; Codex stars Account email, 7d limit, 7d reset and Account
/// resets. This supersedes "starred is never a default". User: Cursor never shows the model or
/// reasoning effort by default, in the status line or More details, because the chat box already
/// shows both; its status line is Context used, Branch and Lines changed (2026-09-24, superseding
/// Context used and Context tokens).
///
/// CDXC:AgentProviders 2026-09-26 DECISION:
/// User (Bots mockup, screen 08): a Hermes status line reads context, cost, tokens and session
/// time; the model stays out because the chat box already shows it.
///
/// CDXC:AgentProviders 2026-10-06 DECISION:
/// User: Pi's status line shows "model, usage, context, like Claude's": model, context used, cost and tokens.
pub fn recommended_preferences(agent: ContextDetailsAgent) -> ContextDetailsPreferences {
    let (shown, starred): (&[(&str, bool)], &[&str]) = match agent {
        ContextDetailsAgent::Claude => (
            &[("fiveHourReset", false)],
            &[
                "accountName",
                "modelLimit",
                "fiveHourLimit",
                "sevenDayLimit",
                "repo",
            ],
        ),
        ContextDetailsAgent::Codex => (
            &[
                ("fiveHourLimit", false),
                ("fiveHourReset", false),
                ("sevenDayReset", true),
                ("accountEmail", true),
            ],
            &[
                "accountEmail",
                "sevenDayLimit",
                "sevenDayReset",
                "accountResets",
            ],
        ),
        ContextDetailsAgent::Cursor => (
            &[
                ("thinking", false),
                ("contextUsed", true),
                ("contextTokens", true),
                ("totalOutputTokens", true),
                ("pr", true),
            ],
            &["contextUsed", "branch", "lines"],
        ),
        ContextDetailsAgent::Hermes => (&[], &["contextUsed", "costUsd", "tokens", "sessionTime"]),
        ContextDetailsAgent::Pi => (&[], &["model", "contextUsed", "costUsd", "tokens"]),
        ContextDetailsAgent::Basic => (&[], &["repo", "branch"]),
    };
    ContextDetailsPreferences {
        shown: shown
            .iter()
            .map(|(id, flag)| (id.to_string(), *flag))
            .collect(),
        starred: starred.iter().map(|id| (id.to_string(), true)).collect(),
        order: BTreeMap::new(),
        starred_order: starred.iter().map(|id| id.to_string()).collect(),
    }
}

/// `normalizeSessionChatContextDetailsPreferences`.
pub fn normalize_preferences(
    candidate: Option<&Value>,
    agent: ContextDetailsAgent,
) -> ContextDetailsPreferences {
    let Some(Value::Object(record)) = candidate else {
        return default_preferences(agent);
    };
    let starred_order = match record.get("starredOrder") {
        Some(Value::Array(ids)) => {
            let mut out: Vec<String> = Vec::new();
            for id in ids {
                let Some(id) = id.as_str() else { continue };
                for current in current_row_ids(id, agent) {
                    if !out.contains(&current) {
                        out.push(current);
                    }
                }
            }
            out
        }
        _ => Vec::new(),
    };
    ContextDetailsPreferences {
        shown: normalize_flags(record.get("shown"), agent),
        starred: normalize_flags(record.get("starred"), agent),
        order: normalize_order(record.get("order"), agent),
        starred_order,
    }
}

/// `JSON.parse` a saved record, then normalize it. An unreadable record is the default.
pub fn parse_preferences(
    raw: Option<&str>,
    agent: ContextDetailsAgent,
) -> ContextDetailsPreferences {
    let value = raw
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or(Value::Null);
    normalize_preferences(Some(&value), agent)
}

/// The record written back, which must stay exactly `JSON.stringify(normalized)`.
pub fn serialize_preferences(preferences: &ContextDetailsPreferences) -> String {
    serde_json::to_string(preferences).unwrap_or_else(|_| "null".to_string())
}

/// `isSessionChatContextDetailShown`.
pub fn is_shown(preferences: &ContextDetailsPreferences, row: &RowDefinition) -> bool {
    preferences
        .shown
        .get(row.id)
        .copied()
        .unwrap_or(row.recommended)
}

/// `isSessionChatContextDetailStarred`.
pub fn is_starred(preferences: &ContextDetailsPreferences, row: &RowDefinition) -> bool {
    preferences.starred.get(row.id) == Some(&true)
}

/// `orderedSessionChatContextDetailRows`: the group's rows in the user's order, missing rows
/// appended in catalog order.
pub fn ordered_rows(
    preferences: &ContextDetailsPreferences,
    group: GroupId,
    agent: ContextDetailsAgent,
) -> Vec<RowDefinition> {
    let catalog: Vec<RowDefinition> = context_detail_rows(agent)
        .into_iter()
        .filter(|row| row.group == group)
        .collect();
    let mut ordered: Vec<RowDefinition> = Vec::new();
    if let Some(ids) = preferences.order.get(group.as_str()) {
        for id in ids {
            if let Some(row) = context_detail_row(agent, id) {
                if row.group == group {
                    ordered.push(row);
                }
            }
        }
    }
    let seen: Vec<&str> = ordered.iter().map(|row| row.id).collect();
    ordered.extend(catalog.into_iter().filter(|row| !seen.contains(&row.id)));
    ordered
}

/// `orderedSessionChatStarredRows`: the starred rows in the status line's own order, then any
/// others in group order.
pub fn ordered_starred_rows(
    preferences: &ContextDetailsPreferences,
    agent: ContextDetailsAgent,
) -> Vec<RowDefinition> {
    let starred: Vec<RowDefinition> = CONTEXT_DETAIL_GROUPS
        .into_iter()
        .flat_map(|group| {
            ordered_rows(preferences, group, agent)
                .into_iter()
                .filter(|row| is_starred(preferences, row))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut ordered: Vec<RowDefinition> = Vec::new();
    for id in &preferences.starred_order {
        if let Some(row) = starred.iter().find(|row| row.id == id) {
            ordered.push(*row);
        }
    }
    let seen: Vec<&str> = ordered.iter().map(|row| row.id).collect();
    ordered.extend(starred.into_iter().filter(|row| !seen.contains(&row.id)));
    ordered
}

/// One resolved row.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextDetailItem {
    pub id: String,
    pub label: String,
    pub value: String,
    /// Present when a click on the status line item copies something.
    pub copy: Option<(String, String)>,
}

impl ContextDetailItem {
    /// The document's shape for one item; `copy` is spread in only when there is one.
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("id".into(), json!(self.id));
        object.insert("label".into(), json!(self.label));
        object.insert("value".into(), json!(self.value));
        if let Some((text, label)) = &self.copy {
            object.insert("copy".into(), json!({ "text": text, "label": label }));
        }
        Value::Object(object)
    }
}

/// One resolved group.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextDetailGroup {
    pub id: GroupId,
    pub items: Vec<ContextDetailItem>,
}

impl ContextDetailGroup {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id.as_str(),
            "label": self.id.label(),
            "items": self.items.iter().map(ContextDetailItem::to_json).collect::<Vec<_>>(),
        })
    }
}

/// Which selection a resolution uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowSelection {
    Shown,
    Starred,
}

/// `resolveSessionChatContextDetailGroups`: groups with at least one row that is selected AND has
/// a value; a group with nothing under it is dropped so its label never renders alone.
#[allow(clippy::too_many_arguments)]
pub fn resolve_context_detail_groups(
    status: Option<&ContextDetailStatus>,
    preferences: &ContextDetailsPreferences,
    context: &crate::ChatContext,
    select: RowSelection,
    session: Option<&ContextDetailSession>,
    agent: ContextDetailsAgent,
) -> Vec<ContextDetailGroup> {
    let Some(status) = status else {
        return Vec::new();
    };
    let selected = |preferences: &ContextDetailsPreferences, row: &RowDefinition| match select {
        RowSelection::Shown => is_shown(preferences, row),
        RowSelection::Starred => is_starred(preferences, row),
    };
    let mut groups = Vec::new();
    for group in CONTEXT_DETAIL_GROUPS {
        let mut items = Vec::new();
        for row in ordered_rows(preferences, group, agent) {
            if !selected(preferences, &row) {
                continue;
            }
            let input = RowInput {
                status,
                session,
                context,
            };
            if let Some(value) = (row.value)(&input) {
                items.push(ContextDetailItem {
                    id: row.id.to_string(),
                    label: row.label.to_string(),
                    value,
                    copy: None,
                });
            }
        }
        if !items.is_empty() {
            groups.push(ContextDetailGroup { id: group, items });
        }
    }
    groups
}

/// `resolveSessionChatStarredContextDetails`.
///
/// CDXC:AgentProviders 2026-09-12 DECISION:
/// User: skip status-line items when their value is unavailable; never show an "unavailable"
/// placeholder. This supersedes the 2026-09-09 decision to keep starred items visible without a
/// value.
pub fn resolve_starred_context_details(
    status: Option<&ContextDetailStatus>,
    preferences: &ContextDetailsPreferences,
    context: &crate::ChatContext,
    session: Option<&ContextDetailSession>,
    agent: ContextDetailsAgent,
) -> Vec<ContextDetailItem> {
    // `status ?? {}`: the status line resolves even before the agent has reported anything.
    let empty = ContextDetailStatus::default();
    let status = status.unwrap_or(&empty);
    let mut items = Vec::new();
    for row in ordered_starred_rows(preferences, agent) {
        let input = RowInput {
            status,
            session,
            context,
        };
        let Some(value) = (row.value)(&input) else {
            continue;
        };
        let copy = row.copy.and_then(|copy| copy(&input));
        items.push(ContextDetailItem {
            id: row.id.to_string(),
            label: row.label.to_string(),
            value,
            copy,
        });
    }
    items
}
