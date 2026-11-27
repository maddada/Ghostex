//! Catch me up: the composer's button and the card that shows where the session stands.
//!
//! The button asks gxserver (`/api/catchUpSession`) for a short summary of the session's recent
//! transcript, written by the small title-generation agent in the Marked handovers format; the card
//! above the composer shows it loading, then the summary drawn with the transcript's own marked
//! sections, or the refusal in plain words. Every click while the card is closed asks for a fresh
//! summary; Refresh asks again with the card open.
//!
//! CDXC:CatchMeUp 2026-10-11 DECISION:
//! User: a "Catch me up" button in the composer itself, at its top right, that has a smaller, cheaper agent read the session's last messages and shows a popup explaining where the session is, in the coloured marked-handover format, so they can continue quickly after reading it. Off by default; its built-in extension on Settings > Extensions turns it on (`chatCatchMeUp`; it was a Settings > Chat row until 2026-11-27), and the button shows only while it is on.
//! CDXC:CatchMeUp 2026-10-12 DECISION:
//! User: "make catch me up only appear when the composer is empty please! otherwise when i type it should go away (also make it fade in/out when appearing/ disappearing)". The button (`button`) shows only while the chat box has no text and no picture still attaching, and the renderers fade it in and out; an open card stays open while the user types.
//! CDXC:CatchMeUp 2026-10-11 WHY:
//! The switch reaches the core two ways and the latest wins: the desktop host pushes it with the chat settings (so flipping it shows or hides the button within seconds), and gxserver puts it on every chat read (`catchUp`), which is the only way the phone, which has no settings file, learns it.
//! SEE-ALSO: server/src/session_chat_catch_up.rs (the summary), apps/desktop/src/app/native_chat/catch_up.rs and apps/mobile/app/src/chat/native/cards/CatchUpCard.tsx (drawing), transcript/marked_sections.rs (the colouring).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::effect::Effect;
use crate::state::ChatState;
use crate::transcript::line_breaks::{agent_line_breaks, AgentLineBreaks};
use crate::transcript::marked_sections::mark_sections;
use crate::transcript::native_markdown::native_markdown;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// What the card remembers between frames.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatchUpState {
    pub open: bool,
    /// The `catchUpSession` call in flight; an answer to any other id is dropped.
    pub request: Option<u64>,
    /// The summary as the agent wrote it.
    pub summary: Option<String>,
    pub error: Option<String>,
    /// "Claude", "Codex": who wrote the summary.
    pub agent_label: Option<String>,
    /// What the summary read, for the footer: "30 agent messages" (an older gxserver counts rows,
    /// "120 messages").
    pub read: Option<String>,
}

/// The document's `catchUp`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatchUp {
    /// The setting is on and the session has a conversation to read.
    pub enabled: bool,
    /// The composer shows the button: `enabled` and the chat box is empty.
    pub button: bool,
    /// The card is open.
    pub open: bool,
    /// The summary is being written.
    pub loading: bool,
    /// The summary as the renderers draw Markdown, its marked sections tagged; empty while none.
    pub markdown: String,
    /// The summary's link pills, as a message's `markdownReferences`.
    pub references: Vec<Value>,
    /// The summary as the agent wrote it, for Copy.
    pub copy_text: String,
    /// Why there is no summary, in words for the user.
    pub error: Option<String>,
    /// Who wrote it from how much, under the summary.
    pub caption: Option<String>,
}

/// The button: closes an open card, or opens it and asks for a fresh summary.
pub fn toggle(state: &mut ChatState) -> Vec<Effect> {
    if state.composer.catch_up.open {
        return close(state);
    }
    if !available(state) {
        return Vec::new();
    }
    state.composer.catch_up.open = true;
    run(state)
}

/// Refresh: asks again, replacing what the card shows.
pub fn run(state: &mut ChatState) -> Vec<Effect> {
    if !available(state) {
        return Vec::new();
    }
    let catch_up = &mut state.composer.catch_up;
    catch_up.open = true;
    catch_up.summary = None;
    catch_up.error = None;
    // The card opens and shows its loading state on this turn, not when the answer lands.
    state.core.request_publish();
    let request_id = state.core.allocate_request_id();
    state.composer.catch_up.request = Some(request_id);
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::CatchUpSession,
        params: Box::new(json!({})),
    }]
}

/// Close: the card goes, and an answer still on its way is dropped.
pub fn close(state: &mut ChatState) -> Vec<Effect> {
    let catch_up = &mut state.composer.catch_up;
    catch_up.open = false;
    catch_up.request = None;
    Vec::new()
}

/// The setting is on and the session has a conversation to read.
fn available(state: &ChatState) -> bool {
    state.core.catch_up_enabled && state.session.agent_session_id.is_some()
}

/// Nothing typed and no picture still being attached. Pills and attached pictures are text in the
/// draft (`@path`, `[Image #n]`), so an empty draft has none of them either.
fn composer_empty(state: &ChatState) -> bool {
    state.composer.text.trim().is_empty() && state.composer.pending_attachments == 0
}

/// The summary's answer: it fills the card, and a refusal is the card's, never the composer's
/// error line.
pub fn settle(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> Option<Vec<Effect>> {
    if state.composer.catch_up.request != Some(request_id) {
        return None;
    }
    state.core.claim_refusal();
    let catch_up = &mut state.composer.catch_up;
    catch_up.request = None;
    match outcome {
        RpcOutcome::Ok { result } => {
            let summary = result
                .get("summary")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if summary.is_empty() {
                catch_up.error = Some("The summary came back empty. Try again.".to_string());
            } else {
                catch_up.summary = Some(summary.to_string());
                catch_up.agent_label = result
                    .get("agentLabel")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                catch_up.read = match (
                    result.get("agentMessages").and_then(Value::as_u64),
                    result.get("rows").and_then(Value::as_u64),
                ) {
                    (Some(count), _) if count > 0 => Some(ghostex_gx_protocol::plural(
                        count,
                        "agent message",
                        "agent messages",
                    )),
                    (None, Some(rows)) if rows > 0 => {
                        Some(ghostex_gx_protocol::plural(rows, "message", "messages"))
                    }
                    _ => None,
                };
            }
        }
        // An older gxserver has no `catchUpSession` and refuses it without a sentence (`{}`).
        RpcOutcome::Err { message, .. } if !message.chars().any(char::is_alphabetic) => {
            catch_up.error = Some(
                "Ghostex on this computer is too old for Catch Me Up. Update it and try again."
                    .to_string(),
            );
        }
        RpcOutcome::Err { message, .. } => catch_up.error = Some(message.clone()),
    }
    state.core.request_publish();
    Some(Vec::new())
}

/// The document's `catchUp`.
pub fn document(state: &ChatState) -> CatchUp {
    let catch_up = &state.composer.catch_up;
    let enabled = available(state);
    let open = enabled && catch_up.open;
    let (markdown, references, copy_text) = match catch_up.summary.as_deref() {
        Some(summary) if open => {
            let native =
                native_markdown(&agent_line_breaks(summary, AgentLineBreaks::Every), false);
            let references = crate::transcript::markdown_links::markdown_references(&native);
            (mark_sections(native), references, summary.to_string())
        }
        _ => (String::new(), Vec::new(), String::new()),
    };
    let caption = catch_up.summary.as_ref().filter(|_| open).map(|_| {
        let agent = catch_up
            .agent_label
            .as_deref()
            .unwrap_or("the title generation agent");
        match catch_up.read.as_deref() {
            Some(read) => format!("Written by {agent} from the last {read}"),
            None => format!("Written by {agent}"),
        }
    });
    CatchUp {
        enabled,
        button: enabled && composer_empty(state),
        open,
        loading: open && catch_up.request.is_some(),
        markdown,
        references,
        copy_text,
        error: catch_up.error.clone().filter(|_| open),
        caption,
    }
}
