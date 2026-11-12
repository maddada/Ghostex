//! The text a session row dragged from the sidebar types into a chat's composer: each session's
//! Copy ID mention, the same `[<session title>](<Global Ref>)` its menu copies.

use crate::GhostexGpuiApp;
use crate::app::native_sidebar::drag::SidebarDrag;
use ghostex_gx_core::SidebarMenus;

impl GhostexGpuiApp {
    /// The mentions a dropped session row stands for, joined by a space: every selected session
    /// when the dragged row is part of a multi-selection (in the selection's order), else the row
    /// alone. `None` for a row that is not a session, or when no session has a ref yet (its
    /// machine's daemon id is not known), since a mention without the ref reaches nobody.
    ///
    /// CDXC:SessionChat 2026-11-12 DECISION:
    /// User: dropping a session row from the sidebar on a chat's composer inserts that session's link at the caret, "in the same form the app already uses for session links, e.g. [Win Chungus](S8g:P5zq6:G4snt)", instead of splitting the pane; several selected rows insert one link per session, separated by a space, and a remote computer's session carries its computer in the ref. The text is the row's own Copy ID (gx-core `SidebarMenus::row_mention`), so a drop and Copy ID never disagree.
    pub(crate) fn sidebar_drag_session_mentions(&mut self, drag: &SidebarDrag) -> Option<String> {
        if drag.kind != "session" {
            return None;
        }
        let selected = &self.gx_store.sidebar_ui.state().selected_session_ids;
        let ids: Vec<String> =
            if selected.len() > 1 && selected.iter().any(|selected| *selected == drag.id) {
                selected.clone()
            } else {
                vec![drag.id.clone()]
            };
        let host = self.gx_store_menu_host();
        let now_ms = super::host::now_ms();
        let store = &self.gx_store;
        let menus = SidebarMenus::new(
            &store.core,
            store.sidebar_list.view(),
            &store.sidebar_list.last_inputs,
            &host,
            now_ms,
        );
        let mentions: Vec<String> = ids
            .iter()
            .filter_map(|id| {
                // A Unified card's session may be outside the Project sidebar's list.
                store
                    .unified_sidebar
                    .row_mention(&store.core, &host, id, now_ms)
                    .or_else(|| menus.row_mention(id))
            })
            .collect();
        (!mentions.is_empty()).then(|| mentions.join(" "))
    }
}
