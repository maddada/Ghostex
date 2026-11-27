//! Answering a search: the newest matching messages from the index, one result per chat, ranked
//! by how well the message matches and then by how recent it is, each with a snippet around the
//! match.

use std::collections::HashMap;

use rusqlite::{params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde_json::{json, Value};

/// Longer queries are cut to this.
pub(super) const QUERY_MAX_CHARS: usize = 200;
pub(super) const DEFAULT_LIMIT: usize = 20;
pub(super) const MAX_LIMIT: usize = 50;
/// The newest matching messages read per query. A common word stops here instead of reading every
/// message, so the time stays flat as history grows.
const HIT_LIMIT: i64 = 4_000;
/// Without a word of three or more characters the index cannot help: the newest messages are
/// scanned instead, at most this many.
const SCAN_LIMIT: i64 = 20_000;
const SNIPPET_CHARS: usize = 240;
/// Results show one line, so the match sits near the start of the snippet.
const SNIPPET_LEAD: usize = 24;

pub(super) struct Query {
    pub text: String,
    pub project_id: Option<String>,
    pub limit: usize,
}

/// The words a query looks for, lowercased, apostrophes as iPhone keyboards type them folded.
pub(super) fn terms(query: &str) -> Vec<String> {
    let folded: String = query
        .chars()
        .take(QUERY_MAX_CHARS)
        .map(|c| {
            if c == '\u{2019}' || c == '\u{2018}' {
                '\''
            } else {
                c
            }
        })
        .collect();
    folded.split_whitespace().map(str::to_lowercase).collect()
}

struct Group {
    transcript_id: i64,
    newest_rowid: i64,
    latest_at: i64,
    hits: usize,
}

pub(super) fn search(index: &Connection, domain: &Connection, query: &Query) -> Vec<Value> {
    let terms = terms(&query.text);
    if terms.iter().map(|term| term.chars().count()).sum::<usize>() < 2 {
        return Vec::new();
    }
    let groups = match hit_groups(index, &terms, query.project_id.as_deref()) {
        Ok(groups) => groups,
        Err(_) => return Vec::new(),
    };
    let mut ranked: Vec<(f64, i64, Value)> = Vec::new();
    for group in groups {
        if ranked.len() >= query.limit * 3 {
            break;
        }
        let Some(session) = shown_session(index, domain, group.transcript_id, query) else {
            continue;
        };
        let Ok(Some((message_id, role, at_ms, text))) = index
            .query_row(
                "SELECT messageId, role, atMs, text FROM messages WHERE rowid = ?1",
                [group.newest_rowid],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
        else {
            continue;
        };
        let (quality, at) = match_quality(&text, &terms);
        let mut row = session;
        row["source"] = json!(if role == 0 { "user" } else { "assistant" });
        row["snippet"] = json!(snippet(&text, at));
        row["messageId"] = json!(message_id);
        row["messageAtMs"] = if at_ms > 0 { json!(at_ms) } else { Value::Null };
        row["hits"] = json!(group.hits);
        row["quality"] = json!(quality);
        ranked.push((quality, group.latest_at, row));
    }
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.1.cmp(&a.1))
    });
    ranked
        .into_iter()
        .take(query.limit)
        .map(|(_, _, row)| row)
        .collect()
}

/// The newest hits, grouped by transcript, newest group first.
fn hit_groups(
    index: &Connection,
    terms: &[String],
    project_id: Option<&str>,
) -> rusqlite::Result<Vec<Group>> {
    let long: Vec<&String> = terms.iter().filter(|t| t.chars().count() >= 3).collect();
    let short: Vec<&String> = terms.iter().filter(|t| t.chars().count() < 3).collect();
    let mut args: Vec<SqlValue> = Vec::new();
    let mut sql = if long.is_empty() {
        args.push(SqlValue::Integer(SCAN_LIMIT));
        "SELECT m.rowid, m.transcriptId, m.atMs FROM messages m
         WHERE m.rowid > (SELECT coalesce(max(rowid), 0) FROM messages) - ?1"
            .to_string()
    } else {
        // Each word as a quoted phrase: every one must appear, as any piece of a word.
        let phrase = long
            .iter()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ");
        args.push(SqlValue::Text(phrase));
        "SELECT m.rowid, m.transcriptId, m.atMs FROM messages_fts f
         JOIN messages m ON m.rowid = f.rowid
         WHERE messages_fts MATCH ?1"
            .to_string()
    };
    // Words too short for trigrams narrow the hits by their text (ASCII case folded).
    for term in &short {
        args.push(SqlValue::Text((*term).clone()));
        sql.push_str(&format!(" AND instr(lower(m.text), ?{}) > 0", args.len()));
    }
    if let Some(project_id) = project_id {
        args.push(SqlValue::Text(project_id.to_string()));
        sql.push_str(&format!(
            " AND m.transcriptId IN (SELECT transcriptId FROM sessions WHERE projectId = ?{})",
            args.len()
        ));
    }
    args.push(SqlValue::Integer(HIT_LIMIT));
    sql.push_str(&format!(
        " ORDER BY {} DESC LIMIT ?{}",
        if long.is_empty() {
            "m.rowid"
        } else {
            "f.rowid"
        },
        args.len()
    ));
    let mut statement = index.prepare_cached(&sql)?;
    let mut groups: Vec<Group> = Vec::new();
    let mut by_transcript: HashMap<i64, usize> = HashMap::new();
    let mut rows = statement.query(params_from_iter(args))?;
    while let Some(row) = rows.next()? {
        let rowid: i64 = row.get(0)?;
        let transcript_id: i64 = row.get(1)?;
        let at_ms: i64 = row.get(2)?;
        match by_transcript.get(&transcript_id) {
            Some(&at) => {
                let group = &mut groups[at];
                group.hits += 1;
                if at_ms > group.latest_at {
                    group.latest_at = at_ms;
                    group.newest_rowid = rowid;
                }
            }
            None => {
                by_transcript.insert(transcript_id, groups.len());
                groups.push(Group {
                    transcript_id,
                    newest_rowid: rowid,
                    latest_at: at_ms,
                    hits: 1,
                });
            }
        }
    }
    groups.sort_by(|a, b| b.latest_at.cmp(&a.latest_at));
    Ok(groups)
}

/// The session a transcript's result opens: the most recently updated session that reads it and
/// still exists (a session deleted since the last sweep is left out), in the asked project.
fn shown_session(
    index: &Connection,
    domain: &Connection,
    transcript_id: i64,
    query: &Query,
) -> Option<Value> {
    let mut statement = index
        .prepare_cached(
            "SELECT projectId, sessionId FROM sessions WHERE transcriptId = ?1
             ORDER BY updatedAt DESC",
        )
        .ok()?;
    let candidates: Vec<(String, String)> = statement
        .query_map([transcript_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok()?
        .flatten()
        .collect();
    let mut lookup = domain
        .prepare_cached(
            "SELECT title, lifecycleState, agentId, updatedAt FROM sessions
             WHERE projectId = ?1 AND sessionId = ?2",
        )
        .ok()?;
    for (project_id, session_id) in candidates {
        if query
            .project_id
            .as_deref()
            .is_some_and(|wanted| wanted != project_id)
        {
            continue;
        }
        let found = lookup
            .query_row([&project_id, &session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .optional()
            .ok()
            .flatten();
        if let Some((title, lifecycle, agent_id, updated_at)) = found {
            return Some(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "title": title,
                "lifecycleState": lifecycle,
                "agentId": agent_id,
                "updatedAt": updated_at,
            }));
        }
    }
    None
}

/// How well `text` matches: 1 when every word is found as a whole word, 0.8 when one only starts
/// a word, 0.3 when one only appears inside a word (or only with other accents), plus 0.25 when
/// the words appear together as typed. Also where the first word was found (a byte offset).
fn match_quality(text: &str, terms: &[String]) -> (f64, Option<usize>) {
    let lower = text.to_lowercase();
    // Lowercasing can change byte lengths outside ASCII; offsets then do not map back to `text`.
    let same_offsets = lower.len() == text.len();
    let mut quality = 1.0f64;
    let mut first_at = None;
    for (position, term) in terms.iter().enumerate() {
        let mut best = 0.3f64;
        let mut found_at = None;
        for (at, _) in lower.match_indices(term.as_str()) {
            found_at.get_or_insert(at);
            let before = lower[..at].chars().next_back();
            let after = lower[at + term.len()..].chars().next();
            let starts = before.is_none_or(|c| !c.is_alphanumeric());
            let ends = after.is_none_or(|c| !c.is_alphanumeric());
            let score = match (starts, ends) {
                (true, true) => 1.0,
                (true, false) => 0.8,
                _ => 0.3,
            };
            if score > best {
                best = score;
                found_at = Some(at);
            }
            if best >= 1.0 {
                break;
            }
        }
        quality = quality.min(best);
        if position == 0 && same_offsets {
            first_at = found_at;
        }
    }
    if terms.len() > 1 && lower.contains(&terms.join(" ")) {
        quality += 0.25;
    }
    (quality, first_at)
}

/// Up to 240 characters starting a few words before the match at `at`, whitespace collapsed and
/// emphasis and code marks dropped, with ellipses where the text was cut.
fn snippet(text: &str, at: Option<usize>) -> String {
    let at = at.unwrap_or(0).min(text.len());
    let mut start = at.saturating_sub(SNIPPET_LEAD);
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    if start > 0 {
        if let Some(space) = text[start..at].find(char::is_whitespace) {
            start += space + 1;
        }
    }
    let mut raw_end = (start + SNIPPET_CHARS * 4).min(text.len());
    while !text.is_char_boundary(raw_end) {
        raw_end += 1;
    }
    let shown = text[start..raw_end]
        .replace("**", "")
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut out: String = shown.chars().take(SNIPPET_CHARS - 2).collect();
    let cut = out.len() < shown.len() || raw_end < text.len();
    if start > 0 {
        out.insert(0, '…');
    }
    if cut {
        out.push('…');
    }
    out
}
