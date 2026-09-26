//! The strips and side panels around the transcript: the working strip, the terminal tail, the
//! per-turn deferred work rows, and the composer menu's host actions.

use ghostex_gx_protocol::Tri;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The line above the composer while the agent works.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingStrip {
    /// The working word plus its clock, or `null` when the session is idle.
    pub label: Option<String>,
    /// What gxserver read off the agent's screen.
    ///
    /// Free-form for now: the activity record belongs to family f.
    pub activity: Option<Value>,
    /// The activity projected for drawing (the elapsed clock and its tone).
    ///
    /// Free-form for now: family f.
    pub presentation: Option<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// The agent terminal's last lines, offered when the chat cannot explain what is happening.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTail {
    /// Whether a tail can be read at all, once the host has looked.
    pub readiness: Option<String>,
    /// The single line the composer shows.
    pub preview: String,
    /// Why no tail is available.
    pub reason: Option<String>,
    pub notice: TerminalTailNotice,
}

/// The expanded tail sheet.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTailNotice {
    pub open: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub excerpt: String,
    /// Why the sheet has nothing to show, or `null` when it has something.
    ///
    /// A sentence, not a flag: `apps/desktop/src/app/native_chat/composer_not_ready.rs` draws it
    /// as text, and `emptyCopy` in
    /// `packages/shared/session-chat-controller/native-terminal-tail.ts` told an unreadable
    /// screen apart from a blank one, as this does.
    pub empty: Option<String>,
}

/// One turn's off-screen work read: in flight, or failed and offering a retry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeferredWorkRow {
    pub loading: bool,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub error: Tri<String>,
}

/// One entry in the composer's host-action menu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAction {
    pub id: String,
    pub label: String,
    /// The hotkey this action is bound to, absent when it has none.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub hotkey: Tri<String>,
    /// `agent` or `session`: which half of the menu the row sits in.
    pub group: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// The account switch's progress, as the controls read it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    /// The card to draw, or `null` when no switch is on screen.
    ///
    /// Free-form for now: the switch card belongs to family e.
    pub visible: Option<Value>,
    /// The clock the card's relative time is measured against, passed in by the host.
    pub now: i64,
    /// A switch is in flight, so sending is held.
    pub busy: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
