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

/// The working card (`extras::working_card`): one line with the spark, a label and the pills, in its
/// own row right above the composer. It never opens; each pill opens its own menu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingCard {
    /// The row above the composer shows the card: it has a working (or worked) line. The pills
    /// show in the chat box's strip whether or not the card does.
    pub visible: bool,
    /// `working`, `worked` (the turn ended and its Worked line shows) or `idle`.
    pub phase: String,
    /// The spark turns: the agent is working.
    pub working: bool,
    /// The one-line label: the running tool's ("Bash(cargo test)"), a live activity's
    /// ("Compacting conversation"), the working word ("Considering…") or the Worked line
    /// ("Worked for 2m 13s"); `null` when the card carries only pills.
    pub label: Option<String>,
    /// Which of those the label is: `tool`, `activity`, `word` or `worked`.
    pub label_kind: Option<String>,
    /// A live activity's clock and progress (compaction): `elapsedLabel`, `tokens`, `percent`,
    /// `indeterminate`, `hint`, as `workingStrip.presentation` carried them.
    pub activity: Option<Value>,
    /// The pill ids (`chatPills`) for the chat box's pill strip, in the pills' order.
    pub pills: Vec<String>,
    /// What a screen reader says for the card.
    pub aria_label: String,
    /// A tool label in runs for the two-row top row, its file paths shortened (`dim` folders);
    /// empty for any other label (extras/working_card_label.rs).
    #[serde(default)]
    pub label_parts: Vec<crate::extras::working_card_label::WorkingCardLabelPart>,
}

/// One pill on the working card (`extras::chat_pills`): a panel folded into a pill that opens a
/// floating menu with the panel's rows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPill {
    /// `threads`, `tasks` or `subagents`: which panel value the menu draws.
    pub id: String,
    pub label: String,
    /// The number after the label ("3", "4/9"), or "" for none.
    pub count: String,
    /// What in the menu waits on the user: `approval` (a thread needs an approval: the sidebar's red
    /// approval dot), `question` (a thread waits on an answer: the sidebar's pink question dot), or
    /// "" for nothing.
    pub waiting: String,
    /// A subagent or thread in the menu is working: the ? pill's amber corner dot, pulsing.
    pub running: bool,
    /// The panel's muted detail ("2 active · 212 total"), the menu's header line.
    pub meta: String,
    pub aria_label: String,
    /// What in the menu needs a look, drawn inside the pill after its label as coloured dots, each
    /// followed by its count, in place of `count`; empty for a pill with nothing to flag. Renderers
    /// draw no corner dot on any pill (`extras::chat_pills::pill_dots`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dots: Vec<PillDot>,
    /// A pill that is a switch (Autopilot, `extras::autopilot`): on or off, drawn with a green dot
    /// after its label while on, its `ariaLabel` as its tooltip; its click opens `menu`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<bool>,
    /// The rows of a switch pill's small menu, in the chat menus' row shape (`label`, `iconPath`,
    /// `command`, `separator`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<Value>,
}

/// One status dot inside a pill: its tone and the count after it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PillDot {
    /// `question` (the sidebar's pink), `approval` (its red), `blocked` (red) or `working` (amber).
    pub tone: String,
    pub count: String,
}

/// One status chip at the left of the chat box's pill strip (`extras::chat_pills::view_chips`): a
/// view mode that is on, with the action that turns it off.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewChip {
    /// `summary` or `detailed`.
    pub id: String,
    /// "Summary view" or "Detailed view".
    pub label: String,
    /// What a screen reader says for the chip's turn-off button.
    pub aria_label: String,
    /// The action the chip (or its hover X) dispatches to turn the mode off.
    pub command: Value,
}

/// The Saved draft pill pinned at the left of the chat box's pill strip
/// (`extras::chat_pills::saved_draft_pill`) and the menu it opens: a draft of this chat saved
/// elsewhere, offered with Use draft and Dismiss.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDraftPill {
    /// "Saved draft", where the strip has room.
    pub label: String,
    /// "Draft", where it is narrow; narrower still the pill is its icon alone.
    pub short_label: String,
    /// What a screen reader says for the pill.
    pub aria_label: String,
    /// The menu's muted detail beside its title: "from another device or the terminal".
    pub detail: String,
    /// The draft's text for the menu, its first `SAVED_DRAFT_PREVIEW_CHARS` characters.
    pub preview: String,
    /// "Replaces what's in the box" while the box holds text Use draft would replace, else empty.
    pub hint: String,
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
