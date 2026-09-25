//! Rename and Note: the two sidebar actions that call nothing and only open a dialog.
//!
//! CDXC:Sessions 2026-09-20 WHY:
//! The boundary is the whole point and it is easy to get wrong in the other direction. A sidebar
//! Rename does NOT rename anything: it closes whatever dialog is open and opens the rename dialog
//! seeded with the title the row already shows. The title the user then types comes back through
//! the app modal host as a separate `renameSession` message and reaches
//! `/api/requestSessionRename`; the note comes back as `setSessionNote` and reaches
//! `/api/saveSessionAgentNote`. Neither of those ever passes through the sidebar command funnel,
//! so neither is part of this action surface, and a port that "moved rename" by moving the call
//! would have moved something the sidebar never does.
//!
//! What that means for the user's text: nothing here produces it and nothing here may keep it. The
//! seed values (the row's current title and note) ARE the user's text, they travel to the dialog
//! because the dialog would otherwise open empty over an existing note and overwrite it on
//! confirm, and they are reported to a log as presence and never as content.
//!
//! Ported from the sidebar page's `runNativeSessionAction` (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/session-actions.ts`) and `renameSession` and
//! `saveSessionNote` in the deleted `gxserver-runtime/sessions-and-focus.ts` (see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_modals.rs.

use serde_json::{json, Value};

use crate::sidebar_view::SidebarView;

use super::resolve::{drawn_row, text_field};

/// The dialog a sidebar action opens, and the dismissal it closes the previous one with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModalAction {
    /// `closeAppModal(<reason>)`, which runs first so a Settings window does not stay behind the
    /// dialog that is about to open.
    pub close_reason: &'static str,
    pub open: Value,
}

impl ModalAction {
    pub fn to_json(&self) -> Value {
        json!({ "closeReason": self.close_reason, "open": self.open })
    }
}

/// The `sessionAction` payloads that open a dialog, or `None` when this file does not own one.
///
/// The row is looked up in the DRAWN list, which is the right place for exactly these two: both
/// are context-menu items, and a context menu can only be opened on a row the list is drawing.
///
/// Refused, with the reason at each refusal:
///
/// - `firstMessage` is gated on `firstUserMessage`, which the presentation stream does not carry,
///   so the item is structurally unreachable on this client on both sides (declared difference
///   14). It stays refused rather than ported to something that can never run.
/// - `delayedSend` opens a dialog seeded with seven Delayed Send fields that the store mirrors
///   from the HUD rather than owns. It moves when the HUD does.
/// - `snooze` is not a dialog at all; it is the clock-reading action and has its own file.
pub fn plan_modal_action(view: &SidebarView, message: &Value) -> Option<ModalAction> {
    if text_field(message, "type")? != "sessionAction" {
        return None;
    }
    let sidebar_session_id = text_field(message, "sessionId")?;
    let row = drawn_row(view, sidebar_session_id)?;
    let title = rename_seed_title(
        row.menu_facts.primary_title.as_deref(),
        row.menu_facts.terminal_title.as_deref(),
        &row.alias,
    );
    match text_field(message, "action")? {
        "rename" => {
            let mut open = serde_json::Map::new();
            open.insert("type".to_string(), Value::String("open".to_string()));
            open.insert(
                "modal".to_string(),
                Value::String("renameSession".to_string()),
            );
            open.insert(
                "sessionId".to_string(),
                Value::String(sidebar_session_id.to_string()),
            );
            // Absent, not null. `JSON.stringify` drops an `undefined` value, so a row with no
            // agent icon sends no key at all, and a `null` here would be a field the dialog has
            // to interpret rather than one it never sees. Same class as the cleared tag.
            if let Some(agent_icon) = &row.agent_icon {
                open.insert(
                    "sessionAgentIcon".to_string(),
                    Value::String(agent_icon.clone()),
                );
            }
            open.insert("initialTitle".to_string(), Value::String(title));
            Some(ModalAction {
                close_reason: "SettingsDismissal:sessionRowRename",
                open: Value::Object(open),
            })
        }
        "note" => Some(ModalAction {
            close_reason: "SettingsDismissal:sessionRowNote",
            open: json!({
                "type": "open",
                "modal": "sessionNote",
                "sessionId": sidebar_session_id,
                "sessionTitle": title,
                // `session.sessionNote ?? ''`: the dialog opens with the note the row holds, and
                // an absent note is an empty field rather than a missing key.
                "initialNote": row.session_note.clone().unwrap_or_default(),
            }),
        }),
        _ => None,
    }
}

/// `session.primaryTitle?.trim() || session.terminalTitle?.trim() || session.alias`.
///
/// The two titles are trimmed before the test and the alias is not, which is what the `||` chain
/// does: a title of only spaces falls through, and an alias of only spaces does not.
///
/// Public and taking its three values directly, because a recording is not guaranteed to contain
/// a padded or blank title: driving this rule only through drawn rows made the gate's own
/// untrimmed-title mutation inert, which is a gate that cannot fail.
pub fn rename_seed_title(
    primary_title: Option<&str>,
    terminal_title: Option<&str>,
    alias: &str,
) -> String {
    for candidate in [primary_title, terminal_title] {
        let trimmed = crate::sidebar_view::text::js_trim(candidate.unwrap_or_default());
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    alias.to_string()
}

/// Whether this payload is one this file answers, without building anything.
pub fn owns_modal_message(message: &Value) -> bool {
    text_field(message, "type") == Some("sessionAction")
        && matches!(text_field(message, "action"), Some("rename" | "note"))
}
