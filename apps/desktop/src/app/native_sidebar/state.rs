use gpui::ScrollHandle;
use std::sync::Arc;

use super::model::NativeSidebarSnapshot;
use crate::GhostexGpuiApp;
use crate::app::project_views::PROJECT_VIEW_SCOPE_OPTION_HUD_KEYS;

#[derive(Default)]
pub(crate) struct NativeSidebarState {
    pub(crate) disclosures: super::disclosure::SidebarDisclosures,
    pub(crate) group_bounds: std::collections::HashMap<String, gpui::Bounds<gpui::Pixels>>,
    pub(crate) space_gesture: super::space_gesture::SpaceGesture,
    pub(crate) completion_flashes: std::collections::HashMap<String, web_time::Instant>,
    pub(crate) bounds: gpui::Bounds<gpui::Pixels>,
    pub(crate) menu: Option<super::menu_state::SidebarMenuState>,
    pub(crate) next_menu_request: u64,
    /// Where the sidebar menu button was last painted, so its menu can drop down from it.
    pub(crate) more_button_bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// When the More menu was dismissed by the same press that is still on its button.
    pub(crate) more_menu_dismissed_at: Option<web_time::Instant>,
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
        if let Some(previous) = &self.native_sidebar.snapshot
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
            .sync(&snapshot, self.gpui_pet_overlay_reduce_motion_enabled);
        if let Some(menu) = self.native_sidebar.menu.as_mut() {
            menu.refresh(&snapshot);
        }
        if let Some(handle) = self.app_modal_window {
            let previous = self.native_sidebar.snapshot.as_ref().map(|s| &s.hud);
            let changed = PROJECT_VIEW_SCOPE_OPTION_HUD_KEYS
                .iter()
                .filter_map(|key| {
                    let value = snapshot.hud.get(*key)?;
                    (previous.and_then(|hud| hud.get(*key)) != Some(value))
                        .then(|| (*key, value.clone()))
                })
                .collect::<Vec<_>>();
            if !changed.is_empty() {
                let _ = handle.update(cx, |host, _, cx| {
                    host.refresh_project_view_scope_options(&changed, cx);
                });
            }
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
        self.native_sidebar.snapshot = Some(snapshot);
        cx.notify();
    }
}
