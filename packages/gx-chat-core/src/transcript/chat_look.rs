//! The chat look: five opt-in Settings > Chat rows (Message layout, Your messages, Agent work, Status line in the chat, Message spacing) that change how the transcript reads. With all five at Default the chat is today's, and nothing here runs. (Chat box matches transcript width, the sixth chat look row, sizes only the desktop's chat box and never reaches the core.)
//!
//! The settings ride [`crate::ChatSettings::chat_look`] from the desktop host and the chat read (`chatLook`) from gxserver, so the desktop, the web build and the phone draw the same look. The document carries [`ChatLook`] as `chatLook`, absent while every setting is Default.
//!
//! CDXC:SessionChat 2026-11-07 DECISION:
//! Message layout (`chatMessageLayout`). User: "align all chats to the left like Slack or Claude Code". Left aligned starts every message at the same left edge. It is opt-in: maddada's rule as Sebastian relayed it, "all of these things need to be advanced mode and if we don't enable anything then it should remain to look like it does now".
//!
//! CDXC:SessionChat 2026-11-08 DECISION:
//! Your messages (`chatUserMessageStyle`) and Agent work (`chatAgentWorkStyle`) trimmed to fewer choices. User: "we want to reduce the options in each of the new switches. Your messages: default or band, the rest goes away." / "I want Default / Reduced / Quiet... but Reduced needs to be what grouped is... the current layout just has too much... even multiple tools calls are shown, it's worthless, it needs to be grouped together, all the tools, all the edited files." / "if we want to not see the agent work anymore in the chat, we can enable quiet for it to show next to the other buttons when the options float". So Your messages is Default (the bubble on the right) or Band (a soft full-width strip, which also left-aligns the chat whatever Message layout says, [`ChatLook::effective_layout`]); Agent work is Default (today's Worked for card), Reduced (one dim line per message of yours for all its tool calls and edited files, `transcript/grouped_work.rs`) or Quiet (nothing in the chat, the work button with the reply's hover buttons, `grouped_work::hide_work`). Reduced and Quiet turn the tool call cards off in the projection whatever Tool call cards says (`TranscriptViewState::effective_tool_cards`), and approvals, Workflow cards, artifacts and the read-image strip never fold into the work (sent-message cards, messages from other agents and answers fold into the row since 2026-11-10, `folded_cards.rs`, where that decision is). Supersedes the 2026-11-07 choices (Your messages' Left rule and Names, Agent work's Quiet lines, and Grouped and Hidden under their old names). Default stays today's look: "all of these things need to be advanced mode and if we don't enable anything then it should remain to look like it does now".
//!
//! CDXC:SessionChat 2026-11-07 DECISION:
//! Status line in the chat (`chatStatusInTranscript`). User: "the line that has the status that shows at the top of the input bar, that I really enjoyed in Claude to be part of the messages". When it is on, a finished turn's fold keeps its own "Worked for" time and the card above the chat box never shows the worked line (`transcript::rows`, `extras::working_card`). Off by default: "all of these things need to be advanced mode and if we don't enable anything then it should remain to look like it does now".
//!
//! CDXC:SessionChat 2026-11-07 DECISION:
//! Message spacing (`chatMessageSpacing`). User: "I think there is a lot of space going on between messages, is that something that we can reduce and also adjust?" Compact halves the row gap (16px to 8px) and a quiet line sits closer to the agent text above it. It changes no row the core projects, so the renderers read it from the document's `chatLook.spacing`, which is present only at Compact. Default keeps today's spacing: "all of these things need to be advanced mode and if we don't enable anything then it should remain to look like it does now".
//!
//! CDXC:SessionChat 2026-11-07 DECISION:
//! The hover footer and the work button (Agent work Quiet, named Hidden on 2026-11-07). User: "the hover buttons are not great ... the buttons need to show on the screen ... without cluttering the text. So even the compact view would still need to have room for them"; "not always visible, like it is right now, they appear on hover, but the space needs to be reserved for the bar to show and not bother the text". So at Compact every message keeps a small footer row under it whose buttons and time show on hover (the phone, which has no hover, shows its tappable parts there), replacing the floating bar Compact first had. And: "another option would be to hide what the agent did ... put that into a button that is part of the hover ... 'expand what the agent did'"; "the agent information would show up in the floating bar after the text, only if the agent information needs to be hidden from the option. If not, it will show before the message like it right now"; "I think it should be embedded in the agent work setting". So Agent work Quiet projects the work as Reduced (one `grouped-work` row per message of yours, its `workButton` saying "24s · 5 tools · 2 files"), with every reply's own calls moved into the row's work and the running response left out while the agent works (`grouped_work::hide_work`), and the renderers draw no line before the reply: the button sits at the left of the reply's hover row (the footer at Compact, today's action row at Default) and opens the steps under it. While the agent works nothing is drawn before the reply; the working card or status line says what runs. Default and Reduced keep the work before the reply. On 2026-11-11 the user changed the footer's hover: "I don't like these appear as empty space, please make it show the time and 3 dots under messages everywhere. i click the 3 dots to see the current options", so the row under a prompt or a final reply always shows the time and a ⋯ that opens the message's actions in a menu, at every Message spacing (apps/desktop/src/app/native_chat/message_actions.rs `message_more_button`); the work button stays in that row.
//!
//! CDXC:SessionChat 2026-11-07 SEE-ALSO:
//! packages/settings-catalog/src/general/chat.rs (the rows), server/src/session_chat_read.rs (`chatLook` on every read), apps/desktop/src/app/gx_chat/boot.rs and world.rs (the desktop host's read and live updates), apps/desktop/src/app/native_chat/ (desktop and web) and apps/mobile/app/src/chat/ (phone) draw the document's `chatLook` and must match.

use serde::{Deserialize, Serialize};

pub use ghostex_gx_protocol::ChatLookSettings;

/// Message layout (`chatMessageLayout`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageLayout {
    #[default]
    Default,
    Left,
}

/// Your messages (`chatUserMessageStyle`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserStyle {
    #[default]
    Default,
    Band,
}

/// Agent work (`chatAgentWorkStyle`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentWork {
    #[default]
    Default,
    /// One `grouped-work` line per message of yours (`TranscriptViewState::grouped_work`).
    Reduced,
    /// Projected as Reduced, its replies' calls moved into the work (`grouped_work::hide_work`);
    /// the renderers draw no line and put the work in the reply's work button instead.
    Quiet,
}

/// Message spacing (`chatMessageSpacing`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageSpacing {
    #[default]
    Default,
    Compact,
}

impl MessageSpacing {
    fn is_default(&self) -> bool {
        *self == Self::Default
    }
}

/// The chat look settings the core projects and publishes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatLook {
    pub message_layout: MessageLayout,
    pub user_style: UserStyle,
    pub agent_work: AgentWork,
    /// Status line in the chat (`chatStatusInTranscript`).
    pub status_in_transcript: bool,
    /// Message spacing (`chatMessageSpacing`): published only at Compact, so a document whose look
    /// was published before this setting existed reads the same.
    #[serde(default, skip_serializing_if = "MessageSpacing::is_default")]
    pub spacing: MessageSpacing,
}

/// A stored Message layout; an unknown value reads as Default.
pub fn parse_message_layout(value: &str) -> MessageLayout {
    match value {
        "left" => MessageLayout::Left,
        _ => MessageLayout::Default,
    }
}

/// A stored Your messages style; an unknown value reads as Default.
pub fn parse_user_style(value: &str) -> UserStyle {
    match value {
        "band" => UserStyle::Band,
        _ => UserStyle::Default,
    }
}

/// A stored Agent work style; an unknown value reads as Default.
pub fn parse_agent_work(value: &str) -> AgentWork {
    match value {
        "reduced" => AgentWork::Reduced,
        "quiet" => AgentWork::Quiet,
        _ => AgentWork::Default,
    }
}

/// A stored Message spacing; an unknown value reads as Default.
pub fn parse_spacing(value: &str) -> MessageSpacing {
    match value {
        "compact" => MessageSpacing::Compact,
        _ => MessageSpacing::Default,
    }
}

impl ChatLook {
    /// Every setting is at Default: today's chat.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The layout the chat draws: Left whenever Your messages is Band.
    pub fn effective_layout(&self) -> MessageLayout {
        if self.user_style != UserStyle::Default {
            MessageLayout::Left
        } else {
            self.message_layout
        }
    }

    /// This look with what `settings` names; a field it leaves out keeps its value.
    pub fn with_settings(&self, settings: &ChatLookSettings) -> Self {
        Self {
            message_layout: settings
                .message_layout
                .as_deref()
                .map_or(self.message_layout, parse_message_layout),
            user_style: settings
                .user_style
                .as_deref()
                .map_or(self.user_style, parse_user_style),
            agent_work: settings
                .agent_work
                .as_deref()
                .map_or(self.agent_work, parse_agent_work),
            status_in_transcript: settings
                .status_in_transcript
                .unwrap_or(self.status_in_transcript),
            spacing: settings
                .spacing
                .as_deref()
                .map_or(self.spacing, parse_spacing),
        }
    }

    /// The value the document publishes as `chatLook`: `None` at Default, and the effective layout
    /// in `messageLayout`, so a renderer never resolves Your messages' implied Left itself.
    pub fn published(&self) -> Option<Self> {
        (!self.is_default()).then(|| Self {
            message_layout: self.effective_layout(),
            ..*self
        })
    }
}

/// Adopts the look the host sent with its chat settings. From then on the chat read's `chatLook`
/// is ignored ([`apply_from_read`]).
pub fn apply_from_host(view: &mut crate::state::TranscriptViewState, settings: &ChatLookSettings) {
    view.chat_look_from_host = true;
    apply(view, settings);
}

/// Adopts the chat read's `chatLook` (gxserver's settings), unless the host has sent its own look.
///
/// CDXC:SessionChat 2026-11-07 WHY:
/// The desktop's own setting wins over the read's: a cached snapshot or a remote gxserver's read carries a look that is stale or another computer's, and applying it flipped the chat (and threw its projection away) until the next settings pass. The phone and a web page whose settings have not arrived send no look, so the read still sets it there.
pub fn apply_from_read(view: &mut crate::state::TranscriptViewState, settings: &ChatLookSettings) {
    if !view.chat_look_from_host {
        apply(view, settings);
    }
}

/// Adopts what `settings` names, throwing the cached projections away when the look moved, the
/// same way [`crate::transcript::tool_card::set_enabled`] does: every row's model depends on it.
/// Message spacing alone moves no row (only the renderers read it), so it keeps them.
///
/// CDXC:SessionChat 2026-11-13 DECISION:
/// The user answered the chat view modes page, Q2773 and Q2774 "retire": Agent work Reduced and Quiet are gone, with their setting (`chatAgentWorkStyle`), and anyone who had one picked lands on Quiet View (`migrate_retired_chat_looks` in the settings catalog). A host or an older computer that still sends an Agent work value is ignored here, so every chat draws Agent work Default under its quiet lines.
fn apply(view: &mut crate::state::TranscriptViewState, settings: &ChatLookSettings) {
    let settings = ChatLookSettings {
        agent_work: None,
        ..settings.clone()
    };
    let next = view.chat_look.with_settings(&settings);
    if next != view.chat_look {
        let rows_moved = ChatLook {
            spacing: view.chat_look.spacing,
            ..next
        } != view.chat_look;
        view.chat_look = next;
        if rows_moved {
            view.invalidate();
        }
    }
}

/// Whether a row holds something the user must see or answer: a call or edit waiting on an approval ([`waits_on_approval`]), a question it asks (a form, one-click answers or a question page), or a card that stays out of the work. This is the one check every look that hides work makes first (`grouped_work::hide_work`), so such a row is never dropped.
///
/// CDXC:SessionChat 2026-11-08 DECISION: User: "we can't block that AT ALL, the only thing I want to is to hide all the tool calls and the file edits, a question or approval needs to surface IMMEDIATELY". Agent work Reduced and Quiet may hide tool calls and file edits and nothing that needs the user, in a running or a finished turn, from the first frame it exists. The cards above the composer (the question card, permission and plan dialogs, relayed approvals, the async question strip) read no look at all; a transcript row's waiting call is marked (`approval_wait::open_waiting_call`) before anything is hidden, and is named eagerly even far up the list, and while an approval is up under Quiet the running turn's rows join no settled chunk, which would have hidden them already (`settled_build::build_after`). Under Quiet a turn waiting on the user counts as running, so only what waits draws beside its card (`working::quiet_holds_turn_open`, `grouped_work::only_what_waits`). Quiet shows only the status line while the agent runs: its thinking and what it writes between tool calls stay out until the reply is done (user: "that's why we have the status bar to show that"), and everything in this rule still draws at once; a row kept only for a card that was already answered or needs no answer draws that card alone ([`holds_only_answered_cards`]); Reduced keeps drawing them as it runs.
pub(crate) fn needs_the_user(row: &serde_json::Value) -> bool {
    let has = |key: &str| row[key].as_array().is_some_and(|list| !list.is_empty());
    waits_on_approval(row)
        || [
            "questions",
            "sentMessages",
            "gbotCards",
            "workflows",
            "images",
        ]
        .iter()
        .any(|key| has(key))
        || row["agentMessage"].is_object()
        || asks_in_words(row)
}

/// Whether a row needs the user only for an answered question, a sent message, a "GBot added" card or a Workflow card: nothing in it waits for an answer, and its words are the agent's between its tool calls (`grouped_work::hide_work`). A form, a page, a picture or a message from another agent still needs what the row holds.
pub(crate) fn holds_only_answered_cards(row: &serde_json::Value) -> bool {
    let has = |key: &str| row[key].as_array().is_some_and(|list| !list.is_empty());
    ["questions", "sentMessages", "gbotCards", "workflows"]
        .iter()
        .any(|key| has(key))
        && !has("images")
        && !row["agentMessage"].is_object()
        && !asks_in_words(row)
        && !waits_on_approval(row)
}

/// Whether a row's words hold a form, one-click answers, a page or a picture.
pub(crate) fn asks_in_words(row: &serde_json::Value) -> bool {
    use crate::transcript::inline_answers::ANSWERS_OPEN;
    use crate::transcript::native_markdown::{
        NATIVE_IMAGE_OPEN, NATIVE_QUESTIONS_OPEN, NATIVE_VISUAL_OPEN,
    };
    row["text"].as_str().is_some_and(|text| {
        [
            NATIVE_QUESTIONS_OPEN,
            ANSWERS_OPEN,
            NATIVE_VISUAL_OPEN,
            NATIVE_IMAGE_OPEN,
        ]
        .iter()
        .any(|mark| text.contains(mark))
    })
}

/// Whether one of a row's calls or edits waits on an approval (`approval_wait::open_waiting_call`).
pub(crate) fn waits_on_approval(row: &serde_json::Value) -> bool {
    ["tools", "files"].iter().any(|key| {
        row[*key]
            .as_array()
            .is_some_and(|list| list.iter().any(|entry| entry["awaitingApproval"] == true))
    })
}
