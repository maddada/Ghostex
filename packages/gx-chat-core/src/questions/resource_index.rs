//! The session's resource index: every question form, question page, plain page and chart the
//! whole conversation ever held, as small records gxserver keeps per session and sends the chat
//! (`resourceIndex` on its frames), so the Resources pill and list cover the conversation and not
//! only the transcript rows the chat has loaded.
//!
//! The chat derives the same things from its loaded rows (`transcript/open_questions.rs`,
//! `questions/content.rs`) and that stays authoritative for what it holds: an index record is
//! listed only for an id the loaded rows do not have (`questions/open_questions.rs`). gxserver builds
//! the records with the functions here, from the same parsers, so the two never read a block
//! differently.
//!
//! CDXC:SessionChat 2026-10-30 DECISION:
//! User: "i only see 42 resources here for your session but i have waaaay more can u please somehow remember the number?" and "i also want to be able to see any of the inline html files that were posted in a thread". The pill showed 42 because it counted only the rows the chat had loaded (the newest 300 messages, more as the reader scrolls up) while the conversation held several hundred forms and pages. gxserver now keeps a resource index per session, fed by the same incremental transcript read that counts open questions, stored in its own table so it survives restarts, compaction and a partly loaded chat, and backfilled once from the transcript off every UI path. A coordinator's index also lists the pages its threads posted (`ThreadRef`), resolved threads included.

use std::collections::HashSet;

use ghostex_gx_protocol::ChatMessage;
use serde::{Deserialize, Serialize};

use crate::transcript::open_questions::{Asker, AskerKind};
use crate::transcript::prose::prose_markdown;
use crate::transcript::question_forms::{finished_fences, is_questions_language, parse_form};

/// The thread a page was posted in, on an entry of a coordinator's index.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRef {
    pub title: String,
    pub project_id: String,
    pub session_id: String,
}

/// One resource of a session, as gxserver stores it and the chat reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceEntry {
    /// The form's id, the page's id, or the chart's (`questions/content.rs` `chart_id`).
    pub id: String,
    /// `form` (Q&A), `page` (HTML) or `chart`.
    pub kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// When the reply holding it arrived (epoch ms), when the transcript says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<i64>,
    /// `waiting`, `answered`, `withdrawn`, or `other` for a page or chart that asks nothing.
    #[serde(default)]
    pub status: String,
    /// A page's address, which opens it where the reader's Open chat pages in setting says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Set on a page a thread posted, in its coordinator's index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<ThreadRef>,
    /// Where the reply that holds it starts in the transcript file (bytes), so gxserver can read
    /// that one reply back without reading the file (`readSessionChatResource`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// Where the message that answered it starts, when it is answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers_offset: Option<u64>,
    /// Where the next user message after the reply starts, which ends the reply's turn: a history
    /// page that ends there holds the whole turn (`questions/resource_modal.rs`, "Go to chat").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_end: Option<u64>,
}

/// The resources one assistant reply holds, in order, with no status yet (`settle` decides it).
pub fn scan_message(message: &ChatMessage) -> Vec<ResourceEntry> {
    if message.role != ghostex_gx_protocol::ChatRole::Assistant {
        return Vec::new();
    }
    let markdown = prose_markdown(&message.blocks);
    if !markdown.contains("```") && !markdown.contains("~~~") {
        return Vec::new();
    }
    let mut found = Vec::new();
    for (language, body) in finished_fences(&markdown) {
        let entry = if is_questions_language(&language) {
            let form = parse_form(&body);
            if !form.error.is_empty() || form.questions.is_empty() {
                continue;
            }
            let mut tags: Vec<String> = Vec::new();
            for tag in form.questions.iter().map(|question| question.tag.as_str()) {
                if !tag.is_empty() && !tags.iter().any(|known| known.eq_ignore_ascii_case(tag)) {
                    tags.push(tag.to_string());
                }
            }
            let title = form
                .questions
                .iter()
                .map(|question| question.title.trim())
                .find(|title| !title.is_empty())
                .unwrap_or(form.title.trim())
                .to_string();
            ResourceEntry {
                id: form.id,
                kind: "form".to_string(),
                title,
                tags,
                ..ResourceEntry::default()
            }
        } else if ghostex_gx_visual::is_visual_language(&language) {
            if let Some(page) = ghostex_gx_visual::page_reference(&body) {
                let id = page.page_id().map_or(page.url.clone(), str::to_string);
                ResourceEntry {
                    id,
                    kind: "page".to_string(),
                    title: page.title.trim().to_string(),
                    tags: page.questions.clone().unwrap_or_default(),
                    url: Some(page.url.clone()),
                    ..ResourceEntry::default()
                }
            } else {
                ResourceEntry {
                    id: crate::questions::content::chart_id(&body),
                    kind: "chart".to_string(),
                    title: ghostex_gx_visual::drawing_title(&body)
                        .flatten()
                        .unwrap_or_default(),
                    ..ResourceEntry::default()
                }
            }
        } else {
            continue;
        };
        found.push(ResourceEntry {
            at_ms: message.timestamp,
            offset: message.byte_offset,
            ..entry
        });
    }
    found
}

/// Adds `more` to `entries`, each id once (the first one wins, as the askers' dedupe does).
pub fn append_unique(entries: &mut Vec<ResourceEntry>, more: Vec<ResourceEntry>) {
    let mut known: HashSet<String> = entries.iter().map(|entry| entry.id.clone()).collect();
    for entry in more {
        if known.insert(entry.id.clone()) {
            entries.push(entry);
        }
    }
}

/// Decides each entry's status from the settled askers (`transcript::open_questions::settle_askers`,
/// with the inherited ones marked): a form or question page is `waiting`, `answered` or
/// `withdrawn`; a page that asks nothing and a chart are `other`. An entry whose asker was
/// inherited from a parent session is dropped.
pub fn settle(entries: Vec<ResourceEntry>, askers: &[Asker]) -> Vec<ResourceEntry> {
    entries
        .into_iter()
        .filter_map(|mut entry| {
            let asks = entry.kind == "form" || (entry.kind == "page" && !entry.tags.is_empty());
            let asker = askers
                .iter()
                .find(|asker| asker.id == entry.id)
                .filter(|asker| {
                    matches!(
                        (&asker.kind, entry.kind.as_str()),
                        (AskerKind::Form, "form") | (AskerKind::Page, "page")
                    )
                });
            match (asks, asker) {
                (_, Some(asker)) if asker.inherited => return None,
                (_, Some(asker)) => {
                    entry.status = if asker.answered {
                        "answered"
                    } else if asker.obsolete {
                        "withdrawn"
                    } else {
                        "waiting"
                    }
                    .to_string();
                }
                (true, None) => entry.status = "answered".to_string(),
                (false, None) => entry.status = "other".to_string(),
            }
            Some(entry)
        })
        .collect()
}

/// Merges a session's stored entries with the ones just read: an id read again takes the new
/// record (its status may have moved), an id the read no longer finds (the transcript was
/// compacted or rewritten) keeps its stored record, and the newest `limit` survive.
pub fn merge(
    stored: Vec<ResourceEntry>,
    read: Vec<ResourceEntry>,
    limit: usize,
) -> Vec<ResourceEntry> {
    let fresh: HashSet<&str> = read.iter().map(|entry| entry.id.as_str()).collect();
    let mut merged: Vec<ResourceEntry> = stored
        .into_iter()
        .filter(|entry| !fresh.contains(entry.id.as_str()))
        .collect();
    merged.extend(read);
    merged.sort_by_key(|entry| entry.at_ms.unwrap_or(i64::MIN));
    if merged.len() > limit {
        merged.drain(..merged.len() - limit);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::open_questions::{asker_entries, settle_askers, Found};

    fn reply(id: &str, at: i64, text: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "id": id, "role": "assistant", "source": "transcript", "timestamp": at,
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    fn form(tag: &str, title: &str) -> String {
        format!(
            "```questions\n{{\"title\":\"{title}\",\"questions\":[{{\"tag\":\"{tag}\",\"title\":\"{title}?\",\"choices\":[\"A\",\"B\"]}}]}}\n```"
        )
    }

    const PAGE: &str = "```visual\n{\"page\":{\"title\":\"Notes\",\"url\":\"http://127.0.0.1:1/visual/0123456789abcdef0123456789abcdef\"}}\n```";
    const CHART: &str = "```visual\n{\"title\":\"Runs\",\"rows\":[{\"stats\":[{\"label\":\"Passed\",\"value\":3}]}]}\n```";

    fn index(messages: &[ChatMessage], answered: &[String]) -> Vec<ResourceEntry> {
        let mut entries = Vec::new();
        let mut found: Vec<Found> = Vec::new();
        for message in messages {
            append_unique(&mut entries, scan_message(message));
            found.extend(asker_entries(std::slice::from_ref(message), &found));
        }
        settle(entries, &settle_askers(&found, answered))
    }

    #[test]
    fn a_reply_yields_its_forms_pages_and_charts_with_what_opens_them() {
        let messages = [reply(
            "m1",
            1_000,
            &format!("{}\n{PAGE}\n{CHART}", form("Q1", "Pick")),
        )];
        let entries = index(&messages, &[]);
        let kinds: Vec<(&str, &str)> = entries
            .iter()
            .map(|entry| (entry.kind.as_str(), entry.status.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [("form", "waiting"), ("page", "other"), ("chart", "other")]
        );
        assert_eq!(entries[0].tags, ["Q1"]);
        assert_eq!(entries[1].title, "Notes");
        assert!(entries[1]
            .url
            .as_deref()
            .is_some_and(|url| url.contains("/visual/")));
        assert_eq!(entries[2].title, "Runs");
        assert!(entries.iter().all(|entry| entry.at_ms == Some(1_000)));
    }

    #[test]
    fn statuses_follow_answers_and_withdrawals() {
        let withdraw = "```questions\n{\"withdraw\":[\"Q2\"]}\n```";
        let messages = [
            reply("m1", 1, &form("Q1", "One")),
            reply("m2", 2, &form("Q2", "Two")),
            reply("m3", 3, &form("Q3", "Three")),
            reply("m4", 4, withdraw),
        ];
        let one = index(&messages, &[]);
        let id = |tag: &str| {
            one.iter()
                .find(|entry| entry.tags == [tag])
                .map(|entry| entry.id.clone())
                .unwrap()
        };
        let status = |entries: &[ResourceEntry], tag: &str| {
            entries
                .iter()
                .find(|entry| entry.tags == [tag])
                .unwrap()
                .status
                .clone()
        };
        assert_eq!(status(&one, "Q1"), "waiting");
        assert_eq!(status(&one, "Q2"), "withdrawn");
        let answered = index(&messages, &[id("Q1")]);
        assert_eq!(status(&answered, "Q1"), "answered");
        assert_eq!(status(&answered, "Q3"), "waiting");
    }

    #[test]
    fn the_same_id_is_listed_once() {
        let messages = [reply("m1", 1, CHART), reply("m2", 2, CHART)];
        assert_eq!(index(&messages, &[]).len(), 1);
    }

    #[test]
    fn a_merge_keeps_what_a_compacted_transcript_no_longer_holds() {
        let old = ResourceEntry {
            id: "old".into(),
            kind: "form".into(),
            status: "waiting".into(),
            at_ms: Some(1),
            ..ResourceEntry::default()
        };
        let moved = ResourceEntry {
            status: "answered".into(),
            ..old.clone()
        };
        let new = ResourceEntry {
            id: "new".into(),
            at_ms: Some(5),
            ..old.clone()
        };
        // The transcript was rewritten: it lost `old` and holds `new`.
        let merged = merge(vec![old.clone()], vec![new.clone()], 10);
        assert_eq!(
            merged.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["old", "new"]
        );
        // An id read again takes the new status.
        let merged = merge(vec![old], vec![moved], 10);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].status, "answered");
        // The newest `limit` survive.
        let many: Vec<ResourceEntry> = (0..5)
            .map(|n| ResourceEntry {
                id: n.to_string(),
                at_ms: Some(n),
                ..new.clone()
            })
            .collect();
        let kept = merge(Vec::new(), many, 2);
        assert_eq!(
            kept.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["3", "4"]
        );
    }

    /// A measurement, not a check: `GX_COUNT_JSONL=<claude transcript> cargo test --release
    /// -- --ignored count_a_claude_transcript --nocapture` streams a Claude transcript (never held
    /// whole in memory) and prints what the resource index would hold for it.
    #[test]
    #[ignore]
    fn count_a_claude_transcript() {
        use std::io::BufRead;
        let Ok(path) = std::env::var("GX_COUNT_JSONL") else {
            return;
        };
        let reader = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
        let mut entries: Vec<ResourceEntry> = Vec::new();
        let mut found: Vec<Found> = Vec::new();
        let mut answered: Vec<String> = Vec::new();
        let (mut lines, mut replies) = (0u64, 0u64);
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            lines += 1;
            let asks = line.contains("```") || line.contains("~~~");
            let answers = line.contains("ghostex-page-answers");
            if !(asks || answers) {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let role = value["type"].as_str().unwrap_or_default();
            let text: String = value["message"]["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|block| block["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                })
                .or_else(|| value["message"]["content"].as_str().map(str::to_string))
                .unwrap_or_default();
            if role == "user" {
                if let Some(id) = crate::transcript::page_answers::answers_page_id(&text) {
                    answered.push(id);
                }
                continue;
            }
            if role != "assistant" {
                continue;
            }
            replies += 1;
            let message = reply(&format!("m{lines}"), lines as i64, &text);
            append_unique(&mut entries, scan_message(&message));
            found.extend(asker_entries(std::slice::from_ref(&message), &found));
        }
        let index = settle(entries, &settle_askers(&found, &answered));
        let count = |kind: &str, status: &str| {
            index
                .iter()
                .filter(|e| e.kind == kind && (status.is_empty() || e.status == status))
                .count()
        };
        eprintln!(
            "lines {lines}, replies with fences {replies}: {} resources = {} forms ({} waiting, {} answered, {} withdrawn) + {} pages ({} ask) + {} charts",
            index.len(), count("form", ""), count("form", "waiting"), count("form", "answered"), count("form", "withdrawn"),
            count("page", ""), index.iter().filter(|e| e.kind == "page" && !e.tags.is_empty()).count(), count("chart", ""),
        );
    }
}
