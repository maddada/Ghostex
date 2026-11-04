//! The generated content the ? pill's list names besides questions: every published page that
//! asks nothing and every chart (a ```visual drawing: a chart, stats or a table) in the chat, in
//! transcript order. Question forms and question pages come from the askers
//! (`transcript/open_questions.rs`) with their answered state. Kept per message, so a long chat
//! scans only the messages that changed since the last event.
//!
//! The user's decision is on `questions/open_questions.rs`.

use std::collections::HashMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};

use crate::state::ChatState;
use crate::transcript::question_forms::{content_fences, ContentKind};

/// One page or chart in the chat.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentItem {
    /// The page's id, or the chart's (`chart_id`): what a jump flashes.
    pub id: String,
    pub kind: ContentKind,
    /// The reply that holds it.
    pub message_id: String,
    pub title: String,
    /// When that reply arrived (epoch ms), if the transcript says.
    pub at: Option<i64>,
}

/// The scan of every message, and what each one held when it was read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentScan {
    rows: HashMap<String, (usize, Vec<ContentItem>)>,
    /// Every page that asks nothing and every chart, in transcript order.
    pub items: Vec<ContentItem>,
}

/// The id a chart's card is flashed by: its block's source, trimmed, hashed (FNV-1a, 32 bits). The
/// desktop's and the phone's chart cards compute the same from the fence they draw (the phone's
/// copy: apps/mobile/app/src/chat/native/transcript/chartId.ts).
pub fn chart_id(body: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in body.trim().bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("chart-{hash:08x}")
}

/// What decides whether a message must be read again: its blocks and the length of their text.
fn fingerprint(message: &ChatMessage) -> usize {
    message
        .blocks
        .iter()
        .map(|block| match block {
            ChatBlock::Text { text } => text.len() + 1,
            _ => 1,
        })
        .sum()
}

/// The pages that ask nothing and the charts one message holds.
fn read(message: &ChatMessage) -> Vec<ContentItem> {
    let markdown = crate::transcript::prose::prose_markdown(&message.blocks);
    content_fences(&markdown)
        .into_iter()
        .filter_map(|fence| {
            let (id, title) = match fence.kind {
                ContentKind::Form => return None,
                ContentKind::Page => {
                    let page = ghostex_gx_visual::page_reference(&fence.body)?;
                    // A page that asks questions is listed with its answered state.
                    if page.questions.is_some() && page.page_id().is_some() {
                        return None;
                    }
                    let id = page.page_id().map_or(page.url.clone(), str::to_string);
                    (id, page.title.trim().to_string())
                }
                ContentKind::Chart => (
                    chart_id(&fence.body),
                    ghostex_gx_visual::drawing_title(&fence.body)
                        .flatten()
                        .unwrap_or_default(),
                ),
            };
            Some(ContentItem {
                id,
                kind: fence.kind,
                message_id: message.id.clone(),
                title,
                at: message.timestamp,
            })
        })
        .collect()
}

/// Brings the scan up to date with the transcript.
pub fn refresh(state: &mut ChatState) {
    let messages = &state.messages.composed;
    let scan = &mut state.questions.content;
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for message in messages
        .iter()
        .filter(|message| message.role == ChatRole::Assistant)
    {
        let print = fingerprint(message);
        let entry = scan
            .rows
            .entry(message.id.clone())
            .or_insert_with(|| (usize::MAX, Vec::new()));
        if entry.0 != print {
            *entry = (print, read(message));
        }
        seen.insert(message.id.as_str());
        items.extend(entry.1.iter().cloned());
    }
    scan.rows.retain(|id, _| seen.contains(id.as_str()));
    if scan.items != items {
        scan.items = items;
        state.core.request_publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(id: &str, text: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "role": "assistant",
            "source": "transcript",
            "timestamp": 1000,
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    #[test]
    fn pages_that_ask_nothing_and_charts_are_listed() {
        let mut state = ChatState::default();
        state.messages.composed = vec![
            reply("a", "```visual\n{\"page\":{\"title\":\"Plain page\",\"url\":\"http://127.0.0.1:1/visual/0123456789abcdef0123456789abcdef\"}}\n```"),
            reply("b", "```visual\n{\"page\":{\"title\":\"Asks\",\"url\":\"http://127.0.0.1:1/visual/ffffffffffffffffffffffffffffffff\",\"questions\":[\"Q1\"]}}\n```"),
            reply("c", "Chart:\n\n```visual\n{\"title\":\"Weekly runs\",\"stats\":[{\"label\":\"Runs\",\"value\":3}]}\n```"),
        ];
        refresh(&mut state);
        let found: Vec<(&str, ContentKind, &str)> = state
            .questions
            .content
            .items
            .iter()
            .map(|item| (item.message_id.as_str(), item.kind, item.title.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                ("a", ContentKind::Page, "Plain page"),
                ("c", ContentKind::Chart, "Weekly runs"),
            ],
            "a page that asks questions is listed with the questions"
        );
        assert_eq!(
            state.questions.content.items[0].id,
            "0123456789abcdef0123456789abcdef"
        );
        assert!(state.questions.content.items[1].id.starts_with("chart-"));
        // A message that left the transcript leaves the list.
        state.messages.composed.remove(0);
        refresh(&mut state);
        assert_eq!(state.questions.content.items.len(), 1);
    }

    #[test]
    fn a_chart_id_ignores_the_space_around_its_source() {
        assert_eq!(chart_id("{\"a\":1}"), chart_id("\n{\"a\":1}\n "));
        assert_ne!(chart_id("{\"a\":1}"), chart_id("{\"a\":2}"));
        // The phone computes the same (apps/mobile/app/src/chat/native/transcript/chartId.ts).
        assert_eq!(chart_id("{\"a\":1}"), "chart-8b9e4511");
    }
}
