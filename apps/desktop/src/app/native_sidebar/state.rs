use gpui::ScrollHandle;
use std::sync::Arc;

use super::model::NativeSidebarSnapshot;
use crate::GhostexGpuiApp;

#[derive(Default)]
pub(crate) struct NativeSidebarState {
    /// The Unified sidebar, installed only while Settings › Sidebar selects it
    /// (gx_store/unified_sidebar.rs); the classic snapshot below is still built and kept.
    pub(crate) unified: Option<Arc<super::unified::model::UnifiedSidebarSnapshot>>,
    /// The machine tabs' sideways scroll, while they need one.
    pub(crate) machine_tabs_scroll: gpui::ScrollHandle,
    /// The selected tab and row width the scroll last revealed, so a manual scroll stays put.
    pub(crate) machine_tabs_revealed: std::cell::RefCell<Option<(String, u32)>>,
    /// A dragged project header is over this group, and below its middle when `true`.
    pub(crate) unified_drop_target: Option<(String, bool)>,
    /// Redraws the Unified sidebar once a second while a card shows a running timer.
    pub(crate) unified_tick: Option<gpui::Task<()>>,
    pub(crate) disclosures: super::disclosure::SidebarDisclosures,
    pub(crate) group_bounds: std::collections::HashMap<String, gpui::Bounds<gpui::Pixels>>,
    pub(crate) space_gesture: super::space_gesture::SpaceGesture,
    pub(crate) completion_flashes: std::collections::HashMap<String, web_time::Instant>,
    pub(crate) bounds: gpui::Bounds<gpui::Pixels>,
    pub(crate) menu: Option<super::menu_state::SidebarMenuState>,
    pub(crate) next_menu_request: u64,
    /// Where the sidebar menu button was last painted, so its menu can drop down from it.
    pub(crate) more_button_bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The trigger of the menu the last outside press dismissed, and where that press landed: a
    /// click on that trigger made by the same press is the closing half of a toggle (menus.rs).
    pub(crate) menu_dismissed_by_press: Option<(String, gpui::Point<gpui::Pixels>)>,
    /// CDXC:Sidebar 2026-09-17 WHY:
    /// A frame profile found snapshot and session deep copies dominating the UI thread during redraws.
    /// Share immutable snapshots with row callbacks; incoming patches and clock updates use copy-on-write mutation.
    pub(crate) snapshot: Option<Arc<NativeSidebarSnapshot>>,
    /// The docked sidebar's cached view, created on its first draw (native_sidebar/host.rs).
    pub(crate) host: Option<gpui::Entity<super::host::NativeSidebarHost>>,
    pub(crate) scroll: ScrollHandle,
    pub(crate) scroll_offsets: std::collections::HashMap<String, gpui::Point<gpui::Pixels>>,
    pub(crate) pending_scroll_offset: Option<gpui::Point<gpui::Pixels>>,
    pub(crate) pending_reveal: Option<super::model::NativeSidebarRevealRequest>,
    pub(crate) handled_rename: Option<u64>,
    pub(crate) handled_reveal: Option<u64>,
    pub(crate) scroll_animation: Option<super::scroll::SidebarScrollAnimation>,
    pub(crate) reveal_flash: Option<(String, web_time::Instant)>,
    pub(crate) dragging: Option<(&'static str, String)>,
    pub(crate) drop_command: Option<serde_json::Value>,
    /// The last drop target resolved and what it resolved to (the command with its landing, or
    /// nothing), so a drag that keeps moving over one row plans that drop once (drag.rs).
    pub(crate) drop_memo: Option<(serde_json::Value, Option<serde_json::Value>)>,
    /// The list's scroll offset the last frame of a drag drew (scroll.rs).
    pub(crate) drag_scroll_offset: Option<gpui::Point<gpui::Pixels>>,
    pub(crate) name_editor: Option<super::rename::SidebarNameEditor>,
    pub(crate) pointer_inside: bool,
    /// Whether last frame's rows carried their hover tooltips; see `render_native_sidebar`.
    pub(crate) row_tooltips_attached: bool,
    pub(crate) hovered_collection: Option<String>,
    pub(crate) hovered_section: Option<String>,
    pub(crate) hovered_group: Option<String>,
    /// This frame's project header probes; `hovered_group` follows them.
    pub(crate) header_hover: super::project_hover::ProjectHeaderHoverProbes,
    pub(crate) hovered_session: Option<String>,
    /// The row whose floating links card is open (window/session_links_card.rs on the desktop,
    /// web_host/session_links.rs in the web build).
    pub(crate) links_card_session: Option<String>,
    /// The pointer is on the floating links card.
    pub(crate) links_card_hovered: bool,
    /// Where the web build drew the card last, for telling whether the pointer is on it.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub(crate) links_card_bounds: std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>,
    /// The card's window on the desktop.
    pub(crate) links_card_window: Option<gpui::AnyWindowHandle>,
    /// The list the card shows (the web build draws it in its own page).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub(crate) links_card_list: Option<gpui::Entity<super::links_list::SessionLinksList>>,
    /// The session whose links list is open in the header dropdown (native_sidebar/links_list.rs).
    pub(crate) links_list_session: Option<ghostex_gx_core::SessionKey>,
    /// The pointer is on the Commands row's account-usage pin (usage.rs); the unpinned strip peeks while it is.
    pub(crate) usage_pin_hovered: bool,
    /// The pointer is on the peeking usage strip itself, so leaving the pin for a card keeps it up.
    pub(crate) usage_peek_hovered: bool,
    /// The pointer is on the peeking strip's frosted window (under window glass it draws there).
    pub(crate) usage_frosted_hovered: bool,
    /// Every painted session card's bounds. A tooltip captures its span the moment hover starts, so the card it belongs to must already be known then.
    pub(crate) session_card_bounds: std::collections::HashMap<String, gpui::Bounds<gpui::Pixels>>,
    /// Armed Delayed Send / Close After Done labels by sidebar session id, for every session rather than only the rows the snapshot shows (session_chat_armed_actions.rs).
    pub(crate) armed_actions: std::collections::HashMap<String, serde_json::Value>,
}

impl NativeSidebarState {
    pub(crate) fn is_dragging(&self, kind: &str, id: &str) -> bool {
        self.dragging
            .as_ref()
            .is_some_and(|(drag_kind, drag_id)| *drag_kind == kind && drag_id == id)
    }
}

impl GhostexGpuiApp {
    /// Installs the list the renderer draws: the scroll scope it belongs to, the reveal it carries,
    /// the disclosure animations, the open menu, the project view scope options, and the browser
    /// focus the row highlight reads.
    pub(crate) fn install_native_sidebar_snapshot(
        &mut self,
        snapshot: Arc<NativeSidebarSnapshot>,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:Sidebar 2026-10-30 WHY:
        // User: "when unified sidebar is enabled then i often see the sidebar scroll by itself when i activate another session". The Unified list shares this scroll box, but it is one list for every Space and machine, so it has no per-Space position to restore: opening a session in another Space switched the Project sidebar's hidden Space, which put that Space's saved offset on the Unified list. While the Unified sidebar is drawn the scope swap is skipped and the list stays where the user left it.
        if self.native_sidebar.unified.is_none()
            && let Some(previous) = &self.native_sidebar.snapshot
            && previous.scroll_scope != snapshot.scroll_scope
        {
            self.native_sidebar.scroll_offsets.insert(
                previous.scroll_scope.clone(),
                self.native_sidebar.scroll.offset(),
            );
            self.native_sidebar.pending_scroll_offset = Some(
                self.native_sidebar
                    .scroll_offsets
                    .get(&snapshot.scroll_scope)
                    .copied()
                    .unwrap_or_default(),
            );
            self.native_sidebar.scroll_animation = None;
            self.native_sidebar.group_bounds.clear();
            self.native_sidebar.session_card_bounds.clear();
        }
        if let Some(request) = &snapshot.reveal_request
            && self.native_sidebar.handled_reveal != Some(request.request_id)
        {
            self.native_sidebar.pending_reveal = Some(request.clone());
            self.native_sidebar.handled_reveal = Some(request.request_id);
        }
        self.native_sidebar
            .disclosures
            .sync(&snapshot, self.gpui_reduce_motion_enabled);
        if let Some(menu) = self.native_sidebar.menu.as_mut() {
            menu.refresh(&snapshot);
        }
        // The old runtime draws no session row focused while a browser tab of the active group owns focus; rows read that per frame, so it is derived here, once per snapshot (gx_store/local_focus.rs).
        let browser_focus = snapshot.groups.iter().any(|group| {
            group.is_active
                && group
                    .sessions
                    .iter()
                    .any(|session| session.is_focused && session.is_browser())
        });
        self.gx_store_note_sidebar_snapshot_browser_focus(browser_focus);
        let work_view_was_available = self.work_view_available();
        self.native_sidebar.snapshot = Some(snapshot);
        self.finish_project_view_restore(cx);
        self.leave_work_view_if_unavailable(work_view_was_available, cx);
        self.work_view_sync_current_session(cx);
        cx.notify();
    }
}
