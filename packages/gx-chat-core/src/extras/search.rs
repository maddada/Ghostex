//! Transcript search's own state, ported from
//! `packages/shared/session-chat-controller/native-search.ts`.
//!
//! What is typed, which occurrence is selected, and which rows carry a match. Navigation bumps
//! `revision` so the renderer scrolls exactly once per explicit move; a transcript refresh keeps
//! the selected occurrence within its row instead of snapping back to the first result (the rule
//! React's search settled on).

use serde_json::{json, Value};

use crate::extras::transcript_search::{search_count_label, transcript_matches, SearchFolds};
use crate::state::SearchState;

/// `searchOpen`.
pub fn open(search: &mut SearchState) {
    search.open = true;
    search.revision += 1;
}

/// `searchClose`.
pub fn close(search: &mut SearchState) {
    search.open = false;
    search.query = String::new();
    search.active_index = 0;
    search.active_key = None;
    search.matches = Vec::new();
}

/// `searchQuery`: a new query restarts the cursor, the same query does nothing.
pub fn set_query(search: &mut SearchState, next: &str) {
    if next == search.query {
        return;
    }
    search.query = next.to_string();
    search.active_index = 0;
    search.active_key = None;
    search.revision += 1;
}

/// `searchNext` and `searchPrevious`, which wrap.
pub fn move_by(search: &mut SearchState, offset: i64) {
    if search.matches.is_empty() {
        return;
    }
    let total = search.matches.len() as i64;
    let next = (search.active_index as i64 + offset + total).rem_euclid(total);
    search.active_index = next as usize;
    search.active_key = search
        .matches
        .get(search.active_index)
        .map(|found| found.key.clone());
    search.revision += 1;
}

/// `searchFolds`: the renderer's open and closed folds and the two modes their defaults follow.
/// Sent whenever one of them changes, find bar open or not: the working card reads them too.
pub fn set_folds(search: &mut SearchState, params: &serde_json::Map<String, Value>) {
    search.folds = SearchFolds::from_action(params);
}

/// `project`: re-runs the query over the current rows and re-anchors the cursor.
///
/// This is the mutating half of the TypeScript's `project`, which runs on every publish.
pub fn settle(search: &mut SearchState, items: &[Value]) {
    if !search.open {
        return;
    }
    search.matches = transcript_matches(items, &search.query, &search.folds);
    let retained = search
        .active_key
        .as_ref()
        .and_then(|key| search.matches.iter().position(|found| &found.key == key));
    search.active_index = match retained {
        Some(index) => index,
        // `Math.max(0, Math.min(activeIndex, matches.length - 1))` on an empty list is 0.
        None => search
            .active_index
            .min(search.matches.len().saturating_sub(1)),
    };
    search.active_key = search
        .matches
        .get(search.active_index)
        .map(|found| found.key.clone());
}

/// The `transcriptSearch` document value, or `null` when the field is closed.
pub fn project(search: &SearchState) -> Value {
    if !search.open {
        return Value::Null;
    }
    let active = search.matches.get(search.active_index);
    let earlier = |found: &crate::extras::transcript_search::TranscriptMatch| {
        search.matches[..search.active_index]
            .iter()
            .filter(|earlier| {
                earlier.item_index == found.item_index && earlier.label == found.label
            })
            .count()
    };
    // Which occurrence of its row's Markdown the selected one is, so a renderer that highlights
    // the words can mark that one: the Markdown matches before it that share its row. A match in a
    // one-line label (a tool row, a thinking headline) is named by that label's key instead.
    let active_in_row = active.filter(|found| found.label.is_none()).map(earlier);
    let active_label = active
        .and_then(|found| {
            found
                .label
                .as_ref()
                .map(|key| json!({ "key": key, "occurrence": earlier(found) }))
        })
        .unwrap_or(Value::Null);
    // Row indices that hold a match, deduplicated and in transcript order.
    let mut rows: Vec<usize> = Vec::new();
    for found in &search.matches {
        if !rows.contains(&found.item_index) {
            rows.push(found.item_index);
        }
    }
    json!({
        "open": true,
        "query": search.query,
        "total": search.matches.len(),
        "activeIndex": search.active_index,
        "activeItem": match active {
            Some(found) => Value::from(found.item_index),
            None => Value::Null,
        },
        "activeInRow": active_in_row,
        "activeLabel": active_label,
        "items": rows,
        "label": search_count_label(&search.query, search.matches.len(), search.active_index),
        "revision": search.revision,
    })
}
