//! Everything below the transcript: the composer's chrome, the controls the host can serve, the
//! session note, the cross-client draft, and the interaction timings both renderers share.

use ghostex_gx_protocol::Tri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which composer controls the host can actually serve.
///
/// React gated these by only passing the handler it had; the native renderer needs the answer in
/// the document so a control that would do nothing stays out of the toolbar.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerActions {
    pub summary: bool,
    pub note: bool,
    pub stash: bool,
    pub attach: bool,
    pub terminal: bool,
}

/// Which toolbar controls did not fit, measured by the renderer and folded back in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerOverflow {
    pub overflowed: Vec<String>,
    pub options_overflowed: bool,
}

/// The composer's own buttons: the note dot, the stash badge, and the pressed states.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerChrome {
    pub note_presence: bool,
    pub note_pressed: bool,
    /// The count on the stash button as text, or `null` when nothing is stashed.
    ///
    /// A string, not a number: `native-composer-chrome.ts` writes `String(Math.min(count, 9))` and
    /// `apps/desktop/src/app/native_chat/composer.rs` reads it with `as_str`, so a numeric value
    /// would silently draw no badge at all.
    pub stash_badge: Option<String>,
    pub summary_pressed: bool,
}

/// The session note sheet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub open: bool,
    pub value: String,
    pub saved: String,
    pub edited: bool,
    pub loading: bool,
}

/// The cross-client composer draft.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    /// This client's opaque id, echoed back as a draft's `originClientId`.
    pub client_id: String,
    /// Whether this host can push at all; false means local-only drafts.
    pub can_sync: bool,
    /// The latest draft gxserver reported, from any device.
    ///
    /// Free-form for now: the draft record belongs to family d.
    pub synced: Option<Value>,
}

/// Timings both renderers must agree on, shipped so neither hard-codes them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interaction {
    pub queue_long_press_ms: u64,
    pub stop_button_cooldown_ms: u64,
}

/// A draft handed to this chat from somewhere else, offered before it replaces what is typed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingDraft {
    pub content: String,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub version: Tri<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
