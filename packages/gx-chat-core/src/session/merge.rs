//! The append stream's merger: id-only dedup with in-place replacement that preserves first-seen
//! order.
//!
//! Ported from `packages/core-ui/chat/session-chat-merge.ts`. Source priority uses `>=` so an
//! equal-priority re-emit still refreshes content.
//!
//! The live list is NEVER trimmed. A trim would drop the OLDEST rows with no way to page them back
//! (the pagination cursor points at the pre-snapshot history), leaving a permanent
//! mid-conversation hole against the terminal. Windowing belongs to the reads that seed the list,
//! not to the append stream.

use std::collections::BTreeMap;

use std::sync::Arc;

use ghostex_gx_protocol::ChatMessage;

use crate::session::appended_notes::shown_message;
use crate::session::assembler::{id_collides, shadowed_id, source_priority};
use crate::state::{ChatRow, MessagesState, PushedSince};

/// `raw` as the list keeps it: a user row without the notes added under it for the agent
/// (`session/appended_notes.rs`), so every reader of the list sees what was written. A row with
/// no notes is the same shared row.
fn shown(raw: &ChatRow) -> ChatRow {
    shown_message(raw)
        .map(Arc::new)
        .unwrap_or_else(|| Arc::clone(raw))
}

/// Rows from the wire, each held once from here on ([`ChatRow`]).
pub fn rows_of(messages: &[ChatMessage]) -> Vec<ChatRow> {
    messages.iter().cloned().map(Arc::new).collect()
}

/// Points each of `rows` that is equal to the list's row of the same id at the list's row, so the
/// two hold one copy. For a fold rebuilt from a snapshot frame or a read, whose rows arrived as
/// new values beside the list's.
pub fn share_with_list(rows: &mut [ChatRow], messages: &MessagesState) {
    for row in rows.iter_mut() {
        if let Some(listed) = messages
            .index_by_id
            .get(&row.id)
            .and_then(|at| messages.list.get(*at))
        {
            if !Arc::ptr_eq(listed, row) && **listed == **row {
                *row = Arc::clone(listed);
            }
        }
    }
}

/// Applies `incoming` to `list`, keeping `index_by_id` aligned. True when no row already in the
/// list was replaced: every row was pushed at the end or left out.
fn apply_incoming(
    list: &mut Vec<ChatRow>,
    index_by_id: &mut BTreeMap<String, usize>,
    incoming: &[ChatRow],
) -> bool {
    let mut pushed_only = true;
    for raw in incoming {
        let mut message = shown(raw);
        if let Some(at) = index_by_id.get(&message.id).copied() {
            if let Some(occupant) = list.get(at) {
                if id_collides(occupant, &message) {
                    // A different row wearing the same id (rows without a record uuid share their
                    // API response id): re-key it so neither row is lost.
                    Arc::make_mut(&mut message).id = shadowed_id(&message);
                }
            }
        }
        match index_by_id.get(&message.id).copied() {
            None => {
                index_by_id.insert(message.id.clone(), list.len());
                list.push(message);
            }
            Some(at) => {
                let replace = list.get(at).is_some_and(|existing| {
                    source_priority(&message.source) >= source_priority(&existing.source)
                });
                if replace {
                    list[at] = message;
                    pushed_only = false;
                }
            }
        }
    }
    pushed_only
}

/// [`merge_messages`] into `list` itself. The index `apply_incoming` reads is a position per id,
/// the last row holding it; for the few rows a frame carries that is found by searching back from
/// the end rather than by indexing every row of a long list again. A frame carrying many rows
/// searches once per row, so it is indexed after all.
pub fn merge_messages_into(list: &mut Vec<ChatRow>, incoming: &[ChatRow]) {
    if incoming.len() > 16 {
        *list = merge_messages(list, incoming);
        return;
    }
    for raw in incoming {
        let mut message = shown(raw);
        let last = |list: &[ChatRow], id: &str| list.iter().rposition(|entry| entry.id == id);
        if let Some(at) = last(list, &message.id) {
            if id_collides(&list[at], &message) {
                Arc::make_mut(&mut message).id = shadowed_id(&message);
            }
        }
        match last(list, &message.id) {
            None => list.push(message),
            Some(at) => {
                if source_priority(&message.source) >= source_priority(&list[at].source) {
                    list[at] = message;
                }
            }
        }
    }
}

/// `mergeSessionChatMessagesWith`: `existing` with `incoming` folded in.
pub fn merge_messages(existing: &[ChatRow], incoming: &[ChatRow]) -> Vec<ChatRow> {
    if incoming.is_empty() {
        return existing.to_vec();
    }
    let mut list = existing.to_vec();
    let mut index_by_id = BTreeMap::new();
    for (index, entry) in list.iter().enumerate() {
        index_by_id.insert(entry.id.clone(), index);
    }
    apply_incoming(&mut list, &mut index_by_id, incoming);
    list
}

impl MessagesState {
    /// Rebuilds the list and its index from `list`.
    ///
    /// Routed through the same path as an append so a window carrying two rows that share an id
    /// (no record uuid, so a shared response id) keeps both and the index never points at the
    /// wrong row.
    pub fn replace_list(&mut self, list: &[ChatRow]) {
        self.list = Vec::new();
        self.index_by_id = BTreeMap::new();
        let mut next = Vec::new();
        let mut index = BTreeMap::new();
        apply_incoming(&mut next, &mut index, list);
        self.list = next;
        self.index_by_id = index;
        self.list_revision = self.list_revision.wrapping_add(1);
        self.pushed_since = None;
    }

    /// Folds an append into the list in place.
    pub fn apply_append(&mut self, incoming: &[ChatRow]) {
        let mut list = std::mem::take(&mut self.list);
        let mut index = std::mem::take(&mut self.index_by_id);
        let before = PushedSince {
            revision: self.list_revision,
            len: list.len(),
        };
        let pushed_only = apply_incoming(&mut list, &mut index, incoming);
        self.list = list;
        self.index_by_id = index;
        self.list_revision = self.list_revision.wrapping_add(1);
        self.pushed_since = if pushed_only {
            self.pushed_since.or(Some(before))
        } else {
            None
        };
        // The merger put the incoming OBJECT in the list wherever it replaced or appended, so a
        // row that landed carries a new `deferredWork`.
        for raw in incoming {
            let shown_row = shown_message(raw);
            let message = shown_row.as_ref().unwrap_or(&**raw);
            let landed = self
                .index_by_id
                .get(&message.id)
                .and_then(|at| self.list.get(*at))
                .is_some_and(|stored| Arc::ptr_eq(stored, raw) || **stored == *message);
            if landed {
                self.note_arrival(message, false);
            }
        }
    }

    /// Drops rows the server retracted (abandoned prompts), returning whether anything went.
    ///
    /// Rebuilding the index is the only safe way to keep it aligned after a removal, and
    /// retractions are rare enough that the cost never matters.
    pub fn remove_ids(&mut self, ids: &[String]) -> bool {
        if ids.is_empty() {
            return false;
        }
        let kept: Vec<ChatRow> = self
            .list
            .iter()
            .filter(|message| !ids.contains(&message.id))
            .cloned()
            .collect();
        if kept.len() == self.list.len() {
            return false;
        }
        self.replace_list(&kept);
        true
    }
}
