//! The Claude button in the sidebar's bottom bar, right of Commands (drawn by
//! native_sidebar/navigation.rs, where CDXC:Extensions 2026-11-04 moved it from the top row; the
//! Claude header's copy of the top row still draws it while Claude is on): it turns Claude on and
//! off (claude_web/takeover.rs).
//!
//! CDXC:Extensions 2026-10-28 DECISION:
//! User: the built-in Claude extension, when enabled, "adds a button to the bottom right of the sidebar" (2026-10-25). Then: "move the Claude button here up to where 'Issues' button is and make this button match this color #7C828C (shouldn't be orange)". The button sits in the sidebar's top row beside Issues (whether or not Issues shows), drawn in the row's icon colour like its neighbours (first #7C828C; since 2026-10-29 the header's own icon grey, see CDXC:Sidebar 2026-10-29 in native_sidebar/appearance.rs), and shows as on while Claude is on. Clicking a session in the sidebar while Claude is on turns Claude off and shows the session (the user's answer to "what happens when you click a session", 2026-10-25). What turning Claude on and off does to the layout is the 2026-10-28 decision in claude_web/takeover.rs, which supersedes this comment's earlier rule that the page opened as an expanded side panel view with the sidebar kept docked.
//! SEE-ALSO: native_sidebar/sessions.rs (the session click).

use crate::app::native_sidebar::navigation::SidebarExtensionButton;
use crate::*;

impl GhostexGpuiApp {
    /// The sidebar's bottom-bar extension buttons: Claude's, while Claude is on and its view can open here.
    pub(crate) fn sidebar_extension_buttons(&self) -> Vec<SidebarExtensionButton> {
        if !super::claude_web_enabled()
            || !self.titlebar_mode_available(TitlebarMode::Extension(super::claude_web_view_id()))
        {
            return Vec::new();
        }
        vec![SidebarExtensionButton {
            id: CLAUDE_WEB_VIEW_ID.to_string(),
            title: "Claude".to_string(),
            icon: "agent-icons/claude.svg",
            showing: self.claude_on(),
        }]
    }

    /// The button: turns Claude on, or off when it is on.
    pub(crate) fn toggle_sidebar_extension_view(
        &mut self,
        extension_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if extension_id == CLAUDE_WEB_VIEW_ID {
            self.toggle_claude(window, cx);
        }
    }

    /// A session click: turns Claude off so the session shows. Returns whether Claude was on.
    pub(crate) fn close_sidebar_extension_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.turn_claude_off(cx) {
            return false;
        }
        self.sync_browser_address_input_if_shown(window, cx);
        true
    }
}
