//! The focused row's Space: the per-Space memory a Space switch restores, and the section move
//! `sidebarSpaceFollowActiveSession` asks for.
//!
//! CDXC:Spaces 2026-09-21 WHY:
//! Both halves are `rememberNativeSidebarFocus`, which the deleted sidebar page ran on every focus
//! change. They hang off the end of `gx_store_update_sidebar_list` rather than off a publish
//! (M4d part 2 step 6), so every way focus can move is covered rather than the ones a publish
//! happened to follow, and the list the rule reads is the one this update just built.

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Remembers the focused row under its Space, and moves the section into that Space while
    /// `sidebarSpaceFollowActiveSession` is on. Both halves of `rememberNativeSidebarFocus`, which
    /// ran on every focus change.
    ///
    /// The memory is what a Space switch restores the focus to when `sidebarSpaceSwitchBehavior` is
    /// `restore`, and it is written whatever the follow setting says: the two are asked as one
    /// question so the unfiltered list is built at most once per focus change.
    pub(super) fn gx_store_follow_active_session_space(&mut self, cx: &mut gpui::Context<Self>) {
        // CDXC:Spaces 2026-09-21 WHY:
        // Nothing while the loading skeleton is drawn, and the focused row is not CONSUMED either.
        // This hangs off the end of `gx_store_update_sidebar_list`, which runs before the ready
        // gate, so on a cold launch the restored session's focus arrived while `inputs.ui` was
        // still the empty default: the Space it resolved was wrong or none, and `take_followed_session`
        // spent the row all the same. The read then landed with the user's real Spaces and the
        // comparison said the focused row had not changed, so the memory a Space switch restores
        // was never written for the session the launch focused and the section never followed it.
        if !self.gx_store_sidebar_list_ready() {
            return;
        }
        let focused = self
            .gx_store
            .core
            .focus()
            .focused_session
            .as_ref()
            .map(ghostex_gx_core::SessionKey::to_sidebar_session_id);
        // Only when the focused row CHANGED: the rule belongs to a focus change, and applying it
        // on every update would pull the section back out of any Space the user picked by hand,
        // over and over, with a write behind each one.
        if !self
            .gx_store
            .sidebar_ui
            .take_followed_session(focused.as_deref())
        {
            return;
        }
        let Some(focused) = focused else {
            return;
        };
        let resolved = {
            let store = &self.gx_store;
            ghostex_gx_core::space_for_focused_row(
                &store.core,
                &store.sidebar_list.last_inputs,
                store.sidebar_list.view(),
                &focused,
                super::host::now_ms(),
            )
        };
        let Some(resolved) = resolved else {
            return;
        };
        // The memory first: the follow moves the section, and the row has to be remembered under
        // the Space it belongs to and not under whichever one the section was showing.
        self.gx_store_remember_space_session(&resolved, &focused, cx);
        if self
            .gx_store
            .sidebar_ui
            .take_suppressed_space_follow(&focused)
        {
            return;
        }
        if resolved.follow {
            // Following into a hidden Space turns Show Hidden on first, as a reveal does: the
            // selection never rests on a hidden Space while it is off (gx-core `FocusedRowSpace`).
            if resolved.show_hidden {
                self.gx_store_apply_sidebar_ui_intent(
                    ghostex_gx_core::SidebarUiIntent::ToggleShowHidden,
                    cx,
                );
            }
            // The ROW's section, not the tab's: a focused row on another machine moves that
            // machine's section and leaves the tab where it is (reveal.rs, `space_for_focused_row`).
            self.gx_store_apply_sidebar_ui_intent(
                ghostex_gx_core::SidebarUiIntent::SetSectionSpace {
                    section_key: resolved.section_key,
                    space_id: resolved.space_id,
                },
                cx,
            );
        }
    }
}
