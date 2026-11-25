//! Compact delivery-failure notices Ghostex sends back to an agent when a queued or held message
//! to another session could not be delivered.
//!
//! CDXC:SessionChat 2026-11-24 WHY:
//! `notify_sender_of_undelivered_agent_message` used to enqueue one verbose multi-paragraph plain
//! text prompt (with a 400-character body quote) per failed row into the sender's chat queue. When
//! a coordinator closed its own thread that held two queued copies of a brief, Ghostex posted two
//! giant user-looking chat bubbles into the coordinator's chat and woke its agent twice for no
//! reason. New notices are coalesced into one `<ghostex-delivery-notice>` per target (suppressed
//! altogether when the sender or its coordinator closed/resolved the target), and both the new tag
//! and older plain-text notices render here as one compact error status row with an optional Retry
//! action and at most a single short preview line.
//! SEE-ALSO: server/src/session_chat_queue_undelivered.rs, server/src/session_chat_send/with_next.rs,
//! apps/desktop/src/app/native_chat/status_rows.rs.

use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

use crate::transcript::jsstr::{collapse_whitespace, js_trim};
use crate::transcript::noise::may_suppress_turn;
use crate::transcript::prose::joined_text;

/// Tag name wrapping compact delivery-failure notices written by gxserver.
pub const DELIVERY_NOTICE_TAG: &str = "ghostex-delivery-notice";

/// Maximum characters kept for the optional single-line preview of an undelivered message.
pub const DELIVERY_NOTICE_PREVIEW_MAX_CHARS: usize = 80;

/// Legacy opener written before 2026-11-24 by `notify_sender_of_undelivered_agent_message`.
const LEGACY_QUEUE_PREFIX: &str = "Ghostex could not deliver your queued message to \"";
/// Legacy opener written before 2026-11-24 by `with_next::drop_held_for_closed_sessions`.
const LEGACY_WITH_NEXT_PREFIX: &str =
    "Not delivered: the message you sent with --with-next to ";

/// A parsed delivery-failure notice, from either the structured tag or a legacy plain-text notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryNotice {
    pub target: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    pub title: String,
    pub count: usize,
    pub reason: String,
    pub preview: String,
    pub prompt_ids: Vec<String>,
}

/// Splits a `S..:P..:G..` global ref into `(project_id, session_id)`.
fn split_target_ref(target: &str) -> (Option<String>, Option<String>) {
    let mut parts = target.split(':');
    let Some(server) = parts.next() else {
        return (None, None);
    };
    let Some(project) = parts.next() else {
        return (None, None);
    };
    let Some(session) = parts.next() else {
        return (None, None);
    };
    if parts.next().is_some() || server.is_empty() || project.is_empty() || session.is_empty() {
        return (None, None);
    }
    (Some(project.to_string()), Some(session.to_string()))
}

fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' | '\n' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

fn unescape_attr(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Extracts a short single-line preview (at most `max_chars` characters) from a message body.
pub fn short_first_line(body: &str, max_chars: usize) -> String {
    let stripped = crate::session::appended_notes::without_notes(body);
    let first = stripped
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("Message from another agent"))
        .unwrap_or_default();
    let collapsed = collapse_whitespace(first);
    let trimmed = collapsed.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return String::new();
    }
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let keep = max_chars.saturating_sub(1);
        format!("{}\u{2026}", trimmed.chars().take(keep).collect::<String>())
    }
}

/// Normalizes a raw gxserver delivery failure reason into a short, plain-English phrase.
pub fn short_delivery_reason(raw: &str) -> String {
    let collapsed = collapse_whitespace(raw);
    let trimmed = collapsed.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.is_empty() {
        return "delivery failed".to_string();
    }
    if lower.contains("session was closed")
        || lower.contains("closed before its agent took")
        || lower.contains("closed before its next message")
    {
        return "the session was closed".to_string();
    }
    if lower.contains("folder") && lower.contains("no longer exists") {
        return "its folder is missing".to_string();
    }
    if lower.contains("session was removed")
        || lower.contains("no such session")
        || lower.contains("does not exist")
        || lower.contains("session is missing")
        || lower.contains("session missing")
    {
        return "the session is missing".to_string();
    }
    if lower.contains("offline")
        || lower.contains("not connected")
        || lower.contains("couldn't reach")
        || lower.contains("unreachable")
        || lower.contains("computer is away")
    {
        return "its computer is offline".to_string();
    }
    if lower.contains("waited more than") && lower.contains("hours") {
        return "it waited over 24 hours without waking".to_string();
    }
    if lower.contains("could not start") || lower.contains("agent stopped") {
        return "the agent stopped".to_string();
    }
    if lower.contains("input box is not on screen")
        || lower.contains("input box")
        || lower.contains("agent is busy")
        || lower.contains("agent was busy")
    {
        return "the agent was busy".to_string();
    }
    // Trim verbose boilerplate sentences from older or custom reasons and keep one concise phrase.
    let mut concise = trimmed;
    for cut in [
        " Send it again",
        " Read that session's chat",
        " The message is still in that session's queue",
        ", so Ghostex stopped waking it",
        ", so the message was not sent",
    ] {
        if let Some(at) = concise.find(cut) {
            concise = concise[..at].trim();
        }
    }
    concise = concise.trim_end_matches(['.', ' ']);
    if concise.chars().count() > 90 {
        format!(
            "{}\u{2026}",
            concise.chars().take(89).collect::<String>().trim_end()
        )
    } else if concise.is_empty() {
        "delivery failed".to_string()
    } else {
        concise.to_string()
    }
}

/// Formats a compact, coalesced delivery-failure notice for an agent's queue/transcript.
pub fn format_delivery_notice(
    target: &str,
    title: &str,
    count: usize,
    reason: &str,
    preview: &str,
    prompt_ids: &[String],
) -> String {
    let count = count.max(1);
    let clean_title = collapse_whitespace(title);
    let clean_title = clean_title.trim();
    let short_reason = short_delivery_reason(reason);
    let short_preview = short_first_line(preview, DELIVERY_NOTICE_PREVIEW_MAX_CHARS);
    let target_label = if !clean_title.is_empty() && !target.is_empty() {
        format!("\"{clean_title}\" ({target})")
    } else if !clean_title.is_empty() {
        format!("\"{clean_title}\"")
    } else if !target.is_empty() {
        target.to_string()
    } else {
        "the other session".to_string()
    };
    let headline = if count == 1 {
        format!("Message to {target_label} wasn't delivered: {short_reason}.")
    } else {
        format!("{count} messages to {target_label} weren't delivered: {short_reason}.")
    };
    let mut attrs = format!(
        "target=\"{}\" title=\"{}\" count=\"{}\" reason=\"{}\"",
        escape_attr(target),
        escape_attr(clean_title),
        count,
        escape_attr(&short_reason),
    );
    let joined_ids: Vec<&str> = prompt_ids
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();
    if !joined_ids.is_empty() {
        attrs.push_str(&format!(
            " prompt-ids=\"{}\"",
            escape_attr(&joined_ids.join(","))
        ));
    }
    if short_preview.is_empty() {
        format!("<{DELIVERY_NOTICE_TAG} {attrs}>\n{headline}\n</{DELIVERY_NOTICE_TAG}>")
    } else {
        format!(
            "<{DELIVERY_NOTICE_TAG} {attrs}>\n{headline}\nFirst line: {short_preview}\n</{DELIVERY_NOTICE_TAG}>"
        )
    }
}

/// Parses an XML attribute `name="value"` from the opening tag header.
fn parse_attr(header: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = header.find(&needle)? + needle.len();
    let rest = &header[start..];
    let end = rest.find('"')?;
    Some(unescape_attr(&rest[..end]))
}

fn parse_tagged_delivery_notice(text: &str) -> Option<DeliveryNotice> {
    let trimmed = js_trim(text);
    let open = format!("<{DELIVERY_NOTICE_TAG}");
    let rest = trimmed.strip_prefix(&open)?;
    let close_bracket = rest.find('>')?;
    let header = &rest[..close_bracket];
    let after_open = &rest[close_bracket + 1..];
    let close_tag = format!("</{DELIVERY_NOTICE_TAG}>");
    let body = after_open
        .strip_suffix(&close_tag)
        .or_else(|| after_open.split(&close_tag).next())
        .unwrap_or(after_open)
        .trim();
    let target = parse_attr(header, "target").unwrap_or_default();
    let title = parse_attr(header, "title").unwrap_or_default();
    let count = parse_attr(header, "count")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let reason = parse_attr(header, "reason")
        .map(|value| short_delivery_reason(&value))
        .unwrap_or_else(|| "delivery failed".to_string());
    let prompt_ids: Vec<String> = parse_attr(header, "prompt-ids")
        .map(|ids| {
            ids.split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let preview = body
        .lines()
        .find_map(|line| line.trim().strip_prefix("First line:"))
        .map(|line| short_first_line(line.trim(), DELIVERY_NOTICE_PREVIEW_MAX_CHARS))
        .unwrap_or_default();
    let (project_id, session_id) = split_target_ref(&target);
    Some(DeliveryNotice {
        target,
        project_id,
        session_id,
        title,
        count,
        reason,
        preview,
        prompt_ids,
    })
}

fn parse_legacy_queue_notice(text: &str) -> Option<DeliveryNotice> {
    let trimmed = js_trim(text);
    let rest = trimmed.strip_prefix(LEGACY_QUEUE_PREFIX)?;
    let (first_line, tail) = rest.split_once("\nReason: ")?;
    let title_end = first_line.rfind("\" (")?;
    let title = first_line[..title_end].trim().to_string();
    let target = first_line[title_end + 3..]
        .trim()
        .strip_suffix(").")
        .or_else(|| first_line[title_end + 3..].trim().strip_suffix(')'))?
        .trim()
        .to_string();
    let reason_line = tail.lines().next().unwrap_or_default();
    let reason = short_delivery_reason(reason_line);
    let preview = tail
        .split_once("\n\nYour message began:\n")
        .map(|(_, excerpt)| short_first_line(excerpt, DELIVERY_NOTICE_PREVIEW_MAX_CHARS))
        .unwrap_or_default();
    let (project_id, session_id) = split_target_ref(&target);
    Some(DeliveryNotice {
        target,
        project_id,
        session_id,
        title,
        count: 1,
        reason,
        preview,
        prompt_ids: Vec::new(),
    })
}

fn parse_legacy_with_next_notice(text: &str) -> Option<DeliveryNotice> {
    let trimmed = js_trim(text);
    let rest = trimmed.strip_prefix(LEGACY_WITH_NEXT_PREFIX)?;
    let (head, after_dropped) = rest.split_once(" was dropped")?;
    // Strip optional ` (request id ...)` suffix from `head`.
    let head = head
        .split_once(" (request id ")
        .map_or(head, |(before, _)| before)
        .trim();
    let (title, target) = if let Some(inner) = head.strip_prefix('\u{201c}') {
        if let Some((title, id_part)) = inner.rsplit_once("\u{201d} (") {
            (
                title.trim().to_string(),
                id_part.trim_end_matches(')').trim().to_string(),
            )
        } else {
            (inner.trim_end_matches('\u{201d}').trim().to_string(), String::new())
        }
    } else {
        (String::new(), head.to_string())
    };
    let preview = after_dropped
        .split_once("It began: ")
        .map(|(_, excerpt)| {
            let clean = excerpt
                .trim()
                .trim_start_matches(['\u{201c}', '"'])
                .trim_end_matches(['\u{201d}', '"']);
            short_first_line(clean, DELIVERY_NOTICE_PREVIEW_MAX_CHARS)
        })
        .unwrap_or_default();
    let (project_id, session_id) = split_target_ref(&target);
    Some(DeliveryNotice {
        target,
        project_id,
        session_id,
        title,
        count: 1,
        reason: "the session was closed".to_string(),
        preview,
        prompt_ids: Vec::new(),
    })
}

/// Parses a delivery-failure notice (new `<ghostex-delivery-notice>` tag or legacy plain text).
pub fn parse_delivery_notice(text: &str) -> Option<DeliveryNotice> {
    parse_tagged_delivery_notice(text)
        .or_else(|| parse_legacy_queue_notice(text))
        .or_else(|| parse_legacy_with_next_notice(text))
}

/// True when `message` is a delivery-failure notice that renders as a compact status row.
pub fn is_delivery_notice(message: &ChatMessage) -> bool {
    may_suppress_turn(&message.role)
        && parse_delivery_notice(&joined_text(&message.blocks)).is_some()
}

/// One-line status card label: `2 messages to <title> weren't delivered: <reason>`.
pub fn delivery_notice_label(notice: &DeliveryNotice) -> String {
    let target_name = if !notice.title.is_empty() {
        notice.title.as_str()
    } else if !notice.target.is_empty() {
        notice.target.as_str()
    } else {
        "another session"
    };
    if notice.count > 1 {
        format!(
            "{} messages to {target_name} weren't delivered: {}",
            notice.count, notice.reason
        )
    } else {
        format!(
            "Message to {target_name} wasn't delivered: {}",
            notice.reason
        )
    }
}

/// The `suppressed` JSON payload for a delivery-failure status row.
pub fn delivery_notice_presentation(notice: &DeliveryNotice) -> Value {
    let label = delivery_notice_label(notice);
    let expandable = !notice.preview.is_empty();
    json!({
        "kind": "status",
        "subject": "delivery",
        "label": label,
        "text": if expandable { notice.preview.as_str() } else { "" },
        "tone": "error",
        "statuses": [{ "label": label, "tone": "error", "detail": "Not delivered" }],
        "expandable": expandable,
        "deliveryNotice": {
            "target": notice.target,
            "projectId": notice.project_id,
            "sessionId": notice.session_id,
            "title": notice.title,
            "count": notice.count,
            "reason": notice.reason,
            "preview": notice.preview,
            "promptIds": notice.prompt_ids,
        },
    })
}
