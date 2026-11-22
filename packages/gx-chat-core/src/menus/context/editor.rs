//! The row editor: the draft the dialog edits, and what it draws.
//!
//! Ports of `packages/shared/session-chat-presentation/context-editor.ts` and
//! `packages/shared/session-chat-controller/native-context-editor.ts`.

use serde_json::{json, Value};

use crate::menus::context::preferences::{
    is_more_row, is_shown, is_starred, ordered_rows, ordered_starred_rows,
    ContextDetailsPreferences, MORE_ROWS,
};
use crate::menus::context::rows::{
    context_detail_row, context_detail_rows, ContextDetailSession, GroupId, RowDefinition,
    RowInput, CONTEXT_DETAIL_GROUPS,
};
use crate::menus::context::status::{ContextDetailStatus, ContextDetailsAgent};

/// The open dialog's draft.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextEditorState {
    pub agent: ContextDetailsAgent,
    pub draft: ContextDetailsPreferences,
    pub query: String,
    pub saving: bool,
    pub error: Option<String>,
    /// The More group is unfolded (`contextMore`); it opens folded each time.
    pub more_open: bool,
}

/// `moveContextRow`.
pub fn move_context_row<T: Clone>(rows: &[T], from: usize, to: usize) -> Vec<T> {
    let mut next = rows.to_vec();
    if from >= next.len() {
        return next;
    }
    let moved = next.remove(from);
    let to = to.min(next.len());
    next.insert(to, moved);
    next
}

/// `matchesContextDetailFilter`.
pub fn matches_context_detail_filter(
    query: &str,
    row: &RowDefinition,
    sample: Option<&str>,
) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    [row.label, row.description, sample.unwrap_or("")]
        .into_iter()
        .any(|text| text.to_lowercase().contains(&needle))
}

/// `toggleContextDetailStar`.
pub fn toggle_context_detail_star(
    current: &ContextDetailsPreferences,
    row: &RowDefinition,
    agent: ContextDetailsAgent,
) -> ContextDetailsPreferences {
    let starred = !is_starred(current, row);
    let without_row: Vec<String> = ordered_starred_rows(current, agent)
        .into_iter()
        .map(|entry| entry.id.to_string())
        .filter(|id| id != row.id)
        .collect();
    let mut next = current.clone();
    next.starred.insert(row.id.to_string(), starred);
    next.starred_order = if starred {
        let mut order = without_row;
        order.push(row.id.to_string());
        order
    } else {
        without_row
    };
    next
}

/// `reorderContextDetails`. `group` of `None` is the status line's own section.
pub fn reorder_context_details(
    current: &ContextDetailsPreferences,
    agent: ContextDetailsAgent,
    group: Option<GroupId>,
    from_id: &str,
    to_id: &str,
) -> ContextDetailsPreferences {
    let rows = match group {
        None => ordered_starred_rows(current, agent),
        Some(group) => ordered_rows(current, group, agent),
    };
    let from = rows.iter().position(|row| row.id == from_id);
    let to = rows.iter().position(|row| row.id == to_id);
    let (Some(from), Some(to)) = (from, to) else {
        return current.clone();
    };
    if from == to {
        return current.clone();
    }
    let order: Vec<String> = move_context_row(&rows, from, to)
        .into_iter()
        .map(|row| row.id.to_string())
        .collect();
    let mut next = current.clone();
    match group {
        None => next.starred_order = order,
        Some(group) => {
            next.order.insert(group.as_str().to_string(), order);
        }
    }
    next
}

/// `nativeContextEditor`: the `contextEditor` document key, or `null` when the dialog is closed.
#[allow(clippy::too_many_arguments)]
pub fn context_editor_projection(
    editor: Option<&ContextEditorState>,
    status: &ContextDetailStatus,
    session: Option<&ContextDetailSession>,
    context: &crate::ChatContext,
    status_line_off: bool,
    mask: impl Fn(&str) -> String,
) -> Value {
    let Some(editor) = editor else {
        return Value::Null;
    };
    let agent = editor.agent;
    let catalog = context_detail_rows(agent);
    let project = |row: &RowDefinition| {
        let input = RowInput {
            status,
            session,
            context,
        };
        let sample = (row.value)(&input);
        json!({
            "id": row.id,
            "label": row.label,
            "description": row.description,
            "sample": sample,
            "shown": is_shown(&editor.draft, row),
            "starred": is_starred(&editor.draft, row),
        })
    };
    // The rows the query keeps, with their samples masked.
    let listed = |rows: Vec<RowDefinition>| -> Vec<Value> {
        rows.iter()
            .map(project)
            .filter(|item| {
                let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                let sample = item.get("sample").and_then(Value::as_str);
                catalog
                    .iter()
                    .find(|row| row.id == id)
                    .is_some_and(|row| matches_context_detail_filter(&editor.query, row, sample))
            })
            .map(|mut item| {
                if let Some(sample) = item.get("sample").and_then(Value::as_str) {
                    let masked = mask(sample);
                    if let Some(object) = item.as_object_mut() {
                        object.insert("sample".into(), json!(masked));
                    }
                }
                item
            })
            .collect()
    };
    let mut groups: Vec<Value> = CONTEXT_DETAIL_GROUPS
        .into_iter()
        .filter_map(|group| {
            let rows = listed(
                ordered_rows(&editor.draft, group, agent)
                    .into_iter()
                    .filter(|row| !is_more_row(row.id))
                    .collect(),
            );
            (!rows.is_empty())
                .then(|| json!({ "id": group.as_str(), "label": group.label(), "rows": rows }))
        })
        .collect();
    // The More group (the 2026-11-22 decision on `MORE_ROWS`), in its own order. A search
    // unfolds it, so a match inside is never hidden.
    let more = listed(
        MORE_ROWS
            .iter()
            .filter_map(|id| catalog.iter().find(|row| row.id == *id).copied())
            .collect(),
    );
    if !more.is_empty() {
        let searching = !editor.query.trim().is_empty();
        groups.push(json!({
            "id": "more",
            "label": "More",
            "subtitle": "Already shown by the Ghostex UI, so none is pinned by default.",
            "collapsed": !editor.more_open && !searching,
            "toggle": {"type": "contextMore"},
            "rows": more,
        }));
    }
    let mut object = serde_json::Map::new();
    object.insert("agent".into(), json!(agent.as_str()));
    object.insert("query".into(), json!(editor.query));
    object.insert("saving".into(), json!(editor.saving));
    // `error: undefined` is a key `JSON.stringify` leaves out.
    if let Some(error) = &editor.error {
        object.insert("error".into(), json!(error));
    }
    object.insert(
        "description".into(),
        json!(if status_line_off {
            format!(
                "Star a row to pin its value to the Chat Status Line under the chat box in {} sessions. The line is off: turn on Chat Status Line in Settings > Extensions to see it.",
                agent.display_name()
            )
        } else {
            format!(
                "Star a row to pin its value to the Chat Status Line under the chat box in {} sessions.",
                agent.display_name()
            )
        }),
    );
    object.insert("groups".into(), Value::Array(groups));
    object.insert(
        "starred".into(),
        Value::Array(
            ordered_starred_rows(&editor.draft, agent)
                .iter()
                .map(project)
                .collect(),
        ),
    );
    Value::Object(object)
}

/// The row an editor command names, looked up in the draft's own agent catalog.
pub fn editor_row(editor: &ContextEditorState, id: Option<&str>) -> Option<RowDefinition> {
    context_detail_row(editor.agent, id?)
}
