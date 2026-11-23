//! Which controls the chat box's row shows (Settings > Experience): the mode picker, the terminal
//! button, the context ring and the computer's Maximize. Each host sends its own switches (the
//! desktop and the web build from the computer's settings file, the phone from its own settings),
//! and the core says what the row leaves out, so both renderers draw the same row.
//!
//! CDXC:Experience 2026-11-15 DECISION:
//! User, Q3281 and Q3282 (packages/settings-catalog/src/experience.rs has the whole decision): a control the row leaves out moves into ••• (Terminal View and Maximize fold there as a narrow row folds them; the context ring stays under More › Context window), the effort always shows, and a hidden mode picker shows no mode at all. Of the terminal: "we need to color the 3 dots button red when there's an issue with terminal, so ppl would know (hovering over the red 3 dots button says the issue)", so while the terminal button is off the row and the terminal reads not ready, `terminalIssue` says why and the ••• button turns red with it (a hover on the computer, the Terminal View row's words on the phone).
//! SEE-ALSO: apps/desktop/src/app/native_chat/toolbar.rs and actions.rs (the computer's row and •••), apps/mobile/app/src/chat/native/composer/NativeComposer.tsx and menus.ts (the phone's), server/src/agents/mode_picker_off.rs (Auto or Bypass permissions with the picker off).

use serde::{Deserialize, Serialize};

use crate::state::ChatState;

/// The four switches, all on by default (today's row).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerRowSettings {
    /// `chatComposerModePicker`.
    #[serde(default = "on")]
    pub mode_picker: bool,
    /// `chatComposerTerminalButton`.
    #[serde(default = "on")]
    pub terminal_button: bool,
    /// `chatComposerContextRing`.
    #[serde(default = "on")]
    pub context_ring: bool,
    /// `chatComposerMaximize` (the computer's only).
    #[serde(default = "on")]
    pub maximize: bool,
}

fn on() -> bool {
    true
}

impl Default for ComposerRowSettings {
    fn default() -> Self {
        Self {
            mode_picker: true,
            terminal_button: true,
            context_ring: true,
            maximize: true,
        }
    }
}

/// The document's `composerRow`: what the row leaves out. Empty is today's row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerRow {
    /// Toolbar controls (`COMPOSER_CONTROLS` ids) that always sit in ••• instead of the row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folded: Vec<String>,
    /// No mode picker: the mode pill, the mode in the model pill and its menu are gone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_mode: bool,
    /// No context ring on the row; More › Context window still opens it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_context_ring: bool,
    /// Why the terminal reads not ready, while its button is not on the row: the ••• button turns
    /// red and says this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_issue: Option<String>,
}

/// What a not-ready terminal says when its read gave no reason.
const NOT_READY: &str = "The terminal is not ready for a message. Open Terminal View to see why.";

/// The `composerRow` of this state.
///
/// CDXC:SessionChat 2026-11-24 DECISION:
/// User: for Native (Non-CLI) sessions, move "Switch to Terminal" out of the composer toolbar into
/// the More actions (•••) menu instead of showing the `>_` button in the regular chat toolbar
/// (Terminal sessions keep their toolbar button as before). Native sessions have no CLI terminal
/// input box, so they never report a `terminalIssue` either.
/// SEE-ALSO: apps/desktop/src/app/native_chat/actions.rs (`folded_toolbar_rows`), apps/mobile/app/src/chat/native/composer/menus.ts.
pub fn document(state: &ChatState) -> ComposerRow {
    let settings = state.core.composer_row;
    let non_cli = state.session.non_cli_mode;
    let mut folded = Vec::new();
    if !settings.maximize {
        folded.push("maximize".to_string());
    }
    if !settings.terminal_button || non_cli {
        folded.push("terminal".to_string());
    }
    let terminal_issue = (!settings.terminal_button && !non_cli)
        .then(|| crate::extras::terminal_tail::project(&state.extras.terminal_tail))
        .filter(|tail| tail.readiness.as_deref() == Some("notReady"))
        .map(|tail| {
            tail.reason
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| NOT_READY.to_string())
        });
    ComposerRow {
        folded,
        hide_mode: !settings.mode_picker,
        hide_context_ring: !settings.context_ring,
        terminal_issue,
    }
}

/// `overflowed` with the controls the row always leaves out added, so both renderers fold them
/// into ••• the way a narrow row folds them.
pub fn with_folded(mut overflowed: Vec<String>, row: &ComposerRow) -> Vec<String> {
    for id in &row.folded {
        if !overflowed.contains(id) {
            overflowed.push(id.clone());
        }
    }
    overflowed
}
