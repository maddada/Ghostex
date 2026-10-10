//! The chat's branches list: the "Branches" item of the composer's More actions menu.
//!
//! CDXC:SessionFork 2026-09-18 SEE-ALSO:
//! The list is what the chat core projects
//! (`packages/gx-chat-core/src/menus/picker/fork_branches.rs`, the port of the deleted
//! `native-fork-branches.ts` and `fork-branches.ts`) from its copy and row rules, so the renderer
//! does not decide what a row says. The pick travels back as the
//! `selectForkBranch` host action, handled in
//! `apps/desktop/src/app/session_chat_fork_branches.rs`.
//!
//! CDXC:SessionFork 2026-10-10 DECISION:
//! User: remove the "This conversation has N branches" button from the chat's top right and put a "Branches" item at the top of the "Chat" section of the More actions menu, above "View". Hovering or clicking it opens a submenu with the list the button's popover showed (the "Branches" header, one row per branch with its title, time, "Current" on this one, the fork marker and "Resumes when opened"); a row does what it did in the popover. The item shows only when the conversation has branches. The phone's More actions has the same item (apps/mobile/app/src/chat/native/composer/menus.ts). This supersedes the floating button, its frosted window and its 20%-at-rest opacity.

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::Hsla;
use serde_json::{Value, json};

/// The lifecycle dot's tint, the tones of `sessionChatForkBranchTone` in React's colours
/// (`bg-emerald-500`, `bg-muted-foreground/60`, `bg-muted-foreground/35`).
pub(super) fn branch_dot_color(tone: &str, appearance: &ChatAppearance) -> Hsla {
    match tone {
        "running" => gpui::rgb(0x10b981).into(),
        "sleeping" => appearance.muted.opacity(0.6),
        _ => appearance.muted.opacity(0.35),
    }
}

impl NativeChatView {
    /// The More actions "Branches" row, or nothing when this session has no family: an unforked
    /// conversation shows no item. Its children are the core's own rows.
    pub(super) fn fork_branches_row(&self) -> Option<Value> {
        let menu = self.snapshot["forkBranches"]["menu"].as_array()?;
        if menu.is_empty() {
            return None;
        }
        Some(json!({
            "label": "Branches",
            "iconPath": "titlebar/git-branch.svg",
            "detail": self.snapshot["forkBranches"]["count"],
            "children": menu,
        }))
    }
}
