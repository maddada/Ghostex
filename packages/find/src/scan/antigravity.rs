use std::collections::HashMap;
use std::path::Path;

use crate::agent::Agent;

use super::*;

/// How far apart, in seconds, a history line and the first transcript step of the conversation it
/// opened may be stamped.
const FIRST_PROMPT_MATCH_SECONDS: i64 = 5;

impl Scanner {
    // -----------------------------------------------------------------------
    // antigravity (agy)
    // -----------------------------------------------------------------------

    /// CDXC:PromptSearch 2026-10-17 WHY:
    /// agy keeps its conversations encrypted, so its prompt log (`history.jsonl`) is the source. A
    /// conversation's first prompt is logged before agy has a conversation id, so that line is tied
    /// to its conversation through the first step of the plain transcript agy writes under
    /// `brain/<id>/.system_generated/logs/` (same text, same second); without it the opening prompt
    /// of every conversation could not resume. Titles and last-active times come from agy's
    /// `conversation_summaries.db`.
    pub(super) fn scan_antigravity(&mut self) {
        let root = self.path(".gemini/antigravity-cli");
        let Some(data) = read_all(&root.join("history.jsonl")) else {
            return;
        };
        let summaries = read_antigravity_summaries(&root.join("conversation_summaries.db"));
        let first_prompts = read_antigravity_first_prompts(&root.join("brain"));
        self.parse_antigravity_history(&data, &summaries, &first_prompts);
    }

    pub fn parse_antigravity_history(
        &mut self,
        data: &[u8],
        summaries: &HashMap<String, AntigravitySummary>,
        first_prompts: &[AntigravityFirstPrompt],
    ) {
        let record_start = self.records.len();
        let mut last_active: HashMap<String, i64> = HashMap::new();
        for line in data.split(|&b| b == b'\n') {
            let Some(v) = parse_line(line) else { continue };
            if string_field(&v, "type").is_some_and(|kind| !kind.is_empty()) {
                // `slash_command` lines are commands, not prompts.
                continue;
            }
            let Some(display) = string_field(&v, "display") else {
                continue;
            };
            let Some(text) = visible_user_prompt(display) else {
                continue;
            };
            let ts = timestamp_value(field(&v, "timestamp"));
            let session = string_field(&v, "conversationId")
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    first_prompts
                        .iter()
                        .find(|first| {
                            first.text == text
                                && (first.ts - ts).abs() <= FIRST_PROMPT_MATCH_SECONDS
                        })
                        .map(|first| first.session.clone())
                })
                .unwrap_or_default();
            if !session.is_empty() {
                let latest = last_active.entry(session.clone()).or_insert(ts);
                *latest = (*latest).max(ts);
            }
            self.records.push(Record {
                agent: Agent::Antigravity,
                title: String::new(),
                text,
                project: string_field(&v, "workspace").unwrap_or("").to_string(),
                session,
                ts,
                meta: Meta {
                    provider: "google".to_string(),
                    ..Default::default()
                },
            });
        }
        for rec in &mut self.records[record_start..] {
            if rec.session.is_empty() {
                continue;
            }
            let summary = summaries.get(&rec.session);
            if let Some(summary) = summary {
                rec.title = summary.title.clone();
            }
            let ts = summary
                .map(|summary| summary.ts)
                .filter(|ts| *ts > 0)
                .or_else(|| last_active.get(&rec.session).copied());
            if let Some(ts) = ts {
                rec.ts = ts;
            }
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct AntigravitySummary {
    pub title: String,
    pub ts: i64,
}

/// The prompt that opened a conversation, read from its transcript's first step.
#[derive(Clone, Debug)]
pub struct AntigravityFirstPrompt {
    pub session: String,
    pub text: String,
    pub ts: i64,
}

fn read_antigravity_summaries(db_path: &Path) -> HashMap<String, AntigravitySummary> {
    let mut out = HashMap::new();
    if !db_path.is_file() {
        return out;
    }
    let Ok(conn) = crate::agent_store::open_agent_store(db_path) else {
        return out;
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT conversation_id, title, CAST(last_modified_time AS TEXT) FROM conversation_summaries",
    ) else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    }) else {
        return out;
    };
    for (id, title, modified) in rows.flatten() {
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            continue;
        };
        out.insert(
            id,
            AntigravitySummary {
                title: title.unwrap_or_default(),
                ts: modified.as_deref().map(parse_iso8601_seconds).unwrap_or(0),
            },
        );
    }
    out
}

fn read_antigravity_first_prompts(brain: &Path) -> Vec<AntigravityFirstPrompt> {
    let mut out = Vec::new();
    for dir in read_dir_sorted(brain) {
        let Some(session) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        let transcript = dir.join(".system_generated/logs/transcript.jsonl");
        let Some(head) = read_prefix(&transcript, 64 * 1024) else {
            continue;
        };
        let Some(first) = head.split(|&b| b == b'\n').next() else {
            continue;
        };
        let Some(v) = parse_line(first) else { continue };
        if string_field(&v, "type") != Some("USER_INPUT") {
            continue;
        }
        let Some(content) = string_field(&v, "content") else {
            continue;
        };
        let Some(text) = tagged_text(content, "USER_REQUEST").and_then(visible_user_prompt) else {
            continue;
        };
        out.push(AntigravityFirstPrompt {
            session,
            text,
            ts: string_field(&v, "created_at")
                .map(parse_iso8601_seconds)
                .unwrap_or(0),
        });
    }
    out
}
