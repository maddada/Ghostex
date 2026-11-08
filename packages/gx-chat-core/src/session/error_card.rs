//! How a refusal or failure the chat shows on its own (the chat box's error, the chat's own load
//! error) is drawn: a one-line card with a tone, a short title, a status chip and the whole
//! message behind it.

use serde_json::{json, Value};

/// The longest lead-in (in characters) that still reads as a title on one line.
const TITLE_MAX: usize = 72;

/// CDXC:SessionChat 2026-11-08 DECISION: User: "i really hate those red line errors we have now let's give them better look please put them in a card just like we have for Message to / Message from and can expand to understand more". An error or wait the chat shows on its own (the chat box's refusal, the chat's own load error) is a one-line card in the Message to / Message from family, collapsed by default: a tinted warning tile (red for a failure, amber for a wait), a one-line title, a status chip (Failed, Waiting), and on opening the whole message and the actions the line used to carry (Retry, Open terminal). No red text. The title is the message's own lead-in (the words before its first colon, else its first sentence), never reworded here; `detail` is the whole message, empty when the title already says all of it. Desktop, the GPUI web build and the phone all draw this one value.
pub fn error_card(message: &str, code: Option<&str>) -> Value {
    let message = message.trim();
    let wait = matches!(code, Some("approvalRequired"));
    let title = lead_in(message);
    let detail = if title == message.trim_end_matches('.') {
        ""
    } else {
        message
    };
    json!({
        "tone": if wait { "wait" } else { "error" },
        "title": title,
        "chip": if wait { "Waiting" } else { "Failed" },
        "detail": detail,
    })
}

/// The words before the first `": "`, else the first sentence, when either is a short lead-in to
/// something longer; otherwise the whole message (the renderer truncates it to one line).
fn lead_in(message: &str) -> &str {
    let first_line = message.lines().next().unwrap_or("").trim();
    let short = |lead: &str| (3..=TITLE_MAX).contains(&lead.chars().count());
    if let Some((lead, rest)) = first_line.split_once(": ") {
        if short(lead) && !rest.trim().is_empty() {
            return lead.trim();
        }
    }
    if let Some(end) = first_line.find(". ") {
        if short(&first_line[..end]) {
            return &first_line[..end];
        }
    }
    first_line.trim_end_matches('.')
}
