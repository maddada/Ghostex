//! The Claude button in the sidebar's bottom bar, right of Commands (drawn by
//! native_sidebar/navigation.rs, where CDXC:Extensions 2026-11-04 moved it from the top row; the
//! Claude header's copy of the top row still draws it while Claude is on): it turns Claude on and
//! off (claude_web/takeover.rs).
//!
//! CDXC:Extensions 2026-10-28 DECISION:
//! User: the built-in Claude extension, when enabled, "adds a button to the bottom right of the sidebar" (2026-10-25). Then: "move the Claude button here up to where 'Issues' button is and make this button match this color #7C828C (shouldn't be orange)". The button sits in the sidebar's top row beside Issues (whether or not Issues shows), drawn in the row's icon colour like its neighbours (first #7C828C; since 2026-10-29 the header's own icon grey, see CDXC:Sidebar 2026-10-29 in native_sidebar/appearance.rs), and shows as on while Claude is on. Clicking a session in the sidebar while Claude is on turns Claude off and shows the session (the user's answer to "what happens when you click a session", 2026-10-25). What turning Claude on and off does to the layout is the 2026-10-28 decision in claude_web/takeover.rs, which supersedes this comment's earlier rule that the page opened as an expanded side panel view with the sidebar kept docked.
//! SEE-ALSO: native_sidebar/sessions.rs (the session click).

//!
//! CDXC:Extensions 2026-11-09 DECISION:
//! Grok's button sits right of Claude's and works the same way (claude_web/place.rs): each shows as on while its page is, and clicking one while the other is on switches pages in place.

use super::place::WebPlace;
use crate::app::native_sidebar::navigation::SidebarExtensionButton;
use crate::*;

impl GhostexGpuiApp {
    /// The sidebar's bottom-bar extension buttons: Claude's and Grok's, each while it is on and its
    /// view can open here.
    pub(crate) fn sidebar_extension_buttons(&self) -> Vec<SidebarExtensionButton> {
        WebPlace::ALL
            .into_iter()
            .filter(|place| place.enabled() && self.titlebar_mode_available(place.mode()))
            .map(|place| SidebarExtensionButton {
                id: place.view_id_str().to_string(),
                title: place.title().to_string(),
                icon: place.icon(),
                showing: self.web_place_on() == Some(place),
            })
            .collect()
    }

    /// A button: turns its page on, or off when it is on.
    pub(crate) fn toggle_sidebar_extension_view(
        &mut self,
        extension_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(place) = WebPlace::from_view_id(extension_id) {
            self.toggle_web_place(place, window, cx);
        }
    }

    /// A session click: turns Claude or Grok off so the session shows. Returns whether one was on.
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
