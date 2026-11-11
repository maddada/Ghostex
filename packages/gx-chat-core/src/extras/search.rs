//! Transcript search's own state, ported from
//! `packages/shared/session-chat-controller/native-search.ts`.
//!
//! What is typed, which occurrence is selected, and which rows carry a match. Navigation bumps
//! `revision` so the renderer scrolls exactly once per explicit move; a transcript refresh keeps
//! the selected occurrence within its row instead of snapping back to the first result (the rule
//! React's search settled on). Find starts at the newest match (nearest the bottom of the chat),
//! "Previous" walks up to older ones, paging older history in when the loaded rows run out.

use serde_json::{json, Value};

use crate::effect::Effect;
use crate::extras::transcript_search::{search_count_label, transcript_matches, SearchFolds};
use crate::state::{ChatContext, ChatState, SearchSeek, SearchState};

/// Pages of older history one search step may ask for before it gives up (one page per settle).
const SEEK_PAGES: u32 = 10;

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
    search.seek = None;
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
    // CDXC:SessionChat 2026-11-11 DECISION:
    // User: "when i search for anything please ALWAYS start me at the bottom of the chat". New words start again from the newest match (`settle` picks it while no occurrence is selected), and when the loaded rows hold none, `advance_seek` pages older history in until one turns up.
    search.seek = (!js_blank(next)).then(SearchSeek::default);
}

fn js_blank(query: &str) -> bool {
    crate::extras::agent_tasks::js_trim(query).is_empty()
}

/// `searchNext` and `searchPrevious`. "Previous" goes up toward older matches, "Next" back down;
/// both wrap. When "Previous" is on the oldest loaded match (or nothing matched yet) and older
/// history is not loaded, it pages that history in for the next older match first.
pub fn navigate(state: &mut ChatState, offset: i64, context: &ChatContext) -> Vec<Effect> {
    let search = &state.extras.search;
    if js_blank(&search.query) {
        return Vec::new();
    }
    let empty = search.matches.is_empty();
    if state.messages.has_more && (empty || (offset < 0 && search.active_index == 0)) {
        let anchor = if empty {
            None
        } else {
            search.active_key.clone()
        };
        state.extras.search.seek = Some(SearchSeek { pages: 0, anchor });
        return advance_seek(state, context);
    }
    move_by(&mut state.extras.search, offset);
    Vec::new()
}

/// One step of the walk back through older history, run on every settle: waits for a page read
/// in flight, ends when a match older than the anchor is among the loaded rows (and selects it),
/// else asks for the next page. Out of pages (or history) it ends; a "Previous" that found
/// nothing older wraps to the newest match, as it did before.
pub fn advance_seek(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(seek) = state.extras.search.seek.clone() else {
        return Vec::new();
    };
    if !state.extras.search.open {
        state.extras.search.seek = None;
        return Vec::new();
    }
    if state.messages.loading_earlier || state.messages.load_earlier_request.is_some() {
        return Vec::new();
    }
    let search = &mut state.extras.search;
    let select = |search: &mut SearchState, index: usize| {
        search.active_index = index;
        search.active_key = search.matches.get(index).map(|found| found.key.clone());
        search.revision += 1;
        search.seek = None;
    };
    match &seek.anchor {
        None if !search.matches.is_empty() => {
            // `settle` already chose the newest while nothing was selected.
            search.seek = None;
            return Vec::new();
        }
        Some(key) => {
            if let Some(at) = search.matches.iter().position(|found| &found.key == key) {
                if at > 0 {
                    select(search, at - 1);
                    return Vec::new();
                }
            } else {
                // The selected match left the list (a fold closed): nothing to walk from.
                search.seek = None;
                return Vec::new();
            }
        }
        None => {}
    }
    if state.messages.has_more && seek.pages < SEEK_PAGES {
        let effects = crate::session::actions::load_earlier(state, context);
        if !effects.is_empty() {
            if let Some(seek) = state.extras.search.seek.as_mut() {
                seek.pages += 1;
            }
            return effects;
        }
    }
    // The walk is over. Pressing "Previous" past the oldest match wraps to the newest.
    let search = &mut state.extras.search;
    search.seek = None;
    if seek.anchor.is_some() && !search.matches.is_empty() {
        let last = search.matches.len() - 1;
        select(search, last);
    }
    Vec::new()
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
    let fresh = search.active_key.is_none();
    let retained = search
        .active_key
        .as_ref()
        .and_then(|key| search.matches.iter().position(|found| &found.key == key));
    search.active_index = match retained {
        Some(index) => index,
        // Nothing selected yet (new words, or the first rows to match): the newest match, the one
        // nearest the bottom of the chat, whatever the scroll position was.
        None if fresh => search.matches.len().saturating_sub(1),
        // `Math.max(0, Math.min(activeIndex, matches.length - 1))` on an empty list is 0.
        None => search
            .active_index
            .min(search.matches.len().saturating_sub(1)),
    };
    search.active_key = search
        .matches
        .get(search.active_index)
        .map(|found| found.key.clone());
    if fresh && search.active_key.is_some() {
        // The renderer scrolls once per revision: this is the move to the newest match.
        search.revision += 1;
    }
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
        "label": search_count_label(
            &search.query,
            search.matches.len(),
            search.active_index,
            search.more_unloaded,
            search.seek.is_some(),
        ),
        "revision": search.revision,
    })
}
