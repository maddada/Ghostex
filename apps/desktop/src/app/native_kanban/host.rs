//! The desktop app's side of the board: which project it shows (the live sidebar snapshot), the
//! bridge context its Beads calls run with, when it counts as on screen, and the project editor
//! the conversation requests name; and the app's context menu and skeleton pulse, which the shared
//! files reach through this module. The GPUI web build compiles every other file of this folder and
//! answers these from the page (`apps/gpui-web/src/app/web_host/kanban.rs`).

pub(crate) use crate::app::context_menu::GpuiContextMenu;
pub(crate) use crate::app::helpers::ProjectBoardBridgeRuntimeContext;
pub(crate) use crate::app::session_chat_skeleton::skeleton_pulse;

use super::state::{KanbanProject, KanbanProjectKey};
use crate::GhostexGpuiApp;
use crate::app::helpers::project_board_bridge_runtime_context_from_snapshot;
use crate::app::model::TitlebarMode;

impl GhostexGpuiApp {
    /// The project the board shows, from the live sidebar snapshot: the same identity the Kanban
    /// CEF page was given. `None` for Quick/projectless contexts and projects without Kanban.
    pub(crate) fn native_kanban_project(&self) -> Option<KanbanProject> {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        if !snapshot.feature_availability.kanban || snapshot.is_quick_projectless {
            return None;
        }
        snapshot.active_project_id.as_ref()?;
        let context = project_board_bridge_runtime_context_from_snapshot(Some(snapshot))?;
        Some(KanbanProject {
            key: KanbanProjectKey {
                project_id: context.project_id,
                project_path: context.project_path,
                remote_machine_id: context.remote_machine_id,
            },
            display_name: snapshot.display_name.clone(),
        })
    }

    /// The bridge context Beads calls run with, including the remote gxserver target for a remote
    /// project.
    pub(crate) fn native_kanban_bridge_context(&self) -> Option<ProjectBoardBridgeRuntimeContext> {
        let mut context = project_board_bridge_runtime_context_from_snapshot(
            self.latest_sidebar_project_snapshot.as_ref(),
        )?;
        if let Some(remote_machine_id) = context.remote_machine_id.clone() {
            context.remote_target = self.gpui_remote_gxserver_request_target(&remote_machine_id);
        }
        Some(context)
    }

    /// A Kanban text field took focus: the page is GPUI's own, so nothing else keeps the keys.
    pub(crate) fn native_kanban_input_focused(&mut self) {
        self.pending_keyboard_handoff = None;
        self.reclaim_gpui_root_for_chrome_input_focus();
    }

    /// Whether the board is on screen: its panel tab is the active, awake view, or it is out in
    /// its own window.
    pub(crate) fn native_kanban_visible(&self) -> bool {
        let in_panel = self.active_mode == TitlebarMode::Kanban
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Kanban);
        (in_panel || self.native_kanban.detached.is_some()) && self.native_kanban.project.is_some()
    }

    /// The desktop's board menus are the app's own context menu windows, so the board draws no
    /// menu of its own.
    pub(crate) fn native_kanban_menu_overlay(
        &self,
        _palette: &super::palette::KanbanPalette,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        None
    }

    /// The Kanban surface's project editor id, which the board's conversation requests carry.
    pub(crate) fn native_kanban_project_editor_id(&self) -> Option<String> {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.surface_ids.kanban_board_id.clone())
    }
}
