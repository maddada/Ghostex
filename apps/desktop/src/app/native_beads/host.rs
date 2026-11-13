//! The app's side of the Beads view, shared by the desktop and the GPUI web build: which project
//! it shows, the sessions "who's on it" reads, the Board drawn under its header, and the answers
//! to what the view asks for.

use ghostex_gx_core::beads_view::{BeadAgentSession, BeadsClock};
use ghostex_gx_core::{MachineId, SessionKey};
use gpui::{
    AnyElement, AppContext as _, Context, IntoElement, ParentElement as _, Styled as _, Window,
};
use serde_json::{Value, json};

use super::view::{BeadsHostInfo, BeadsLayout, BeadsRequest, BeadsTarget, BeadsView};
use crate::GhostexGpuiApp;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::shared_settings;

/// The setting that remembers Tree or Board.
pub(crate) const BEADS_LAYOUT_SETTING: &str = "beadsViewLayout";

/// `None` when unset or unknown (Tree).
fn saved_layout() -> Option<BeadsLayout> {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get(BEADS_LAYOUT_SETTING)
        .and_then(Value::as_str)
        .and_then(BeadsLayout::from_setting)
}

/// The width the details / New bead column was last dragged to, kept with the sidebar's own width
/// in the desktop's native settings file (`sidebarWidth` beside it), so it survives a restart.
#[cfg(not(target_arch = "wasm32"))]
fn saved_side_width() -> Option<f32> {
    crate::app::helpers::read_json_number_field(
        &crate::app::helpers::native_chrome_settings_path(),
        BEADS_SIDE_WIDTH_SETTING,
    )
    .filter(|width| width.is_finite() && *width > 0.0)
}

/// The page keeps no settings file: the width lasts while the page is open.
#[cfg(target_arch = "wasm32")]
fn saved_side_width() -> Option<f32> {
    None
}

/// CDXC:ProjectBoard 2026-11-13 WHY: the divider's width is the user's preference, so it is saved only when they drag; a panel that is narrower for a while clamps what is shown (`side_width_now`) without rewriting it.
#[cfg(not(target_arch = "wasm32"))]
fn save_side_width(width: f32) {
    let Some(number) = serde_json::Number::from_f64(f64::from(width.round())) else {
        return;
    };
    crate::app::helpers::write_native_chrome_number_setting(BEADS_SIDE_WIDTH_SETTING, number);
}

#[cfg(target_arch = "wasm32")]
fn save_side_width(_width: f32) {}

/// The native settings key of the details column's dragged width.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const BEADS_SIDE_WIDTH_SETTING: &str = "beadsDetailsWidth";

impl GhostexGpuiApp {
    /// The active project, as the Beads view reads it: the Board's own context (id, folder,
    /// remote route), so Tree and Board never ask about two different projects.
    ///
    /// CDXC:ProjectBoard 2026-11-13 WHY: the Tree resolved its project through the git scope and sent only the project id, while the Board used the sidebar's board context with the folder and remote route, so the two could ask about different folders or computers and the Tree could show a setup card while the Board showed beads.
    pub(crate) fn beads_target(&self) -> Option<BeadsTarget> {
        let project_key = self.git_active_scoped_project_id()?;
        // A projectless scope has no Beads workspace of its own (the check this had before PR #599).
        if self
            .git_scope_for_project(&project_key)
            .is_ok_and(|scope| scope.projectless)
        {
            return None;
        }
        let context = self.native_kanban_bridge_context()?;
        let machine = context
            .remote_machine_id
            .clone()
            .map(MachineId::Remote)
            .unwrap_or(MachineId::Local);
        // The Board refuses a remote project whose computer has no route (`gxserver_target` in
        // native_kanban/beads.rs); the Tree keeps the target but marks it, so it says so and calls
        // nothing instead of asking this computer's gxserver.
        let offline = !machine.is_local() && context.remote_target.is_none();
        Some(BeadsTarget {
            project_key,
            rpc_project_id: context.project_id?,
            project_path: context.project_path,
            machine,
            remote: context.remote_target,
            offline,
        })
    }

    /// The sessions of the machine the project lives on, with whether each is working now.
    fn beads_agent_sessions(&self, machine: &MachineId) -> Vec<BeadAgentSession> {
        let Some(loaded) = self.gx_store.core.presentation().loaded(machine) else {
            return Vec::new();
        };
        loaded
            .server_sessions()
            .map(|session| BeadAgentSession {
                project_id: session.project_id.clone(),
                session_id: session.session_id.clone(),
                title: session.title.clone(),
                agent: session
                    .agent_name
                    .clone()
                    .or_else(|| session.agent_id.clone()),
                working: session.activity == ghostex_gx_core::protocol::SessionActivity::Working,
                sleeping: session.lifecycle_state
                    == ghostex_gx_core::protocol::LifecycleState::Sleeping,
            })
            .collect()
    }

    fn beads_view_entity(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<BeadsView> {
        if let Some(view) = self.native_beads.clone() {
            return view;
        }
        let app = cx.weak_entity();
        let host: super::view::BeadsHostFn = std::rc::Rc::new(move |request, window, cx| {
            let _ = app.update(cx, |app, cx| app.handle_beads_request(request, window, cx));
        });
        let view = cx.new(|cx| {
            let mut view = BeadsView::new(host, window, cx);
            if let Some(width) = saved_side_width() {
                view.side_width = width;
            }
            view
        });
        self.native_beads = Some(view.clone());
        view
    }

    /// The Beads view for the active project; `None` where the project has no Beads view
    /// (projectless contexts), which keeps the placeholder.
    pub(crate) fn render_beads_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let target = self.beads_target()?;
        let view = self.beads_view_entity(window, cx);
        let layout = view.read(cx).layout;
        let board = if layout == BeadsLayout::Board {
            self.render_native_kanban_embedded(window, cx)
        } else {
            None
        };
        let sessions = self.beads_agent_sessions(&target.machine);
        let global_beads_directory = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("globalBeadsDirectory")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let now = chrono::Local::now();
        let info = BeadsHostInfo {
            palette: KanbanPalette::current(window),
            modal: self.gpui_native_modal_palette(),
            sessions,
            global_beads_directory,
            clock: BeadsClock {
                now_ms: now.timestamp_millis(),
                offset_ms: i64::from(now.offset().local_minus_utc()) * 1_000,
            },
            board,
            can_start_work: true,
            saved_layout: saved_layout(),
        };
        view.update(cx, |view, cx| view.sync(Some(target), info, cx));
        // The view's menus are drawn here, from the view's own area, so they show whether or not
        // the sidebar is drawn.
        let menu = self.render_native_region_menu(window, cx);
        Some(
            gpui::div()
                .relative()
                .size_full()
                .child(gpui::AnyView::from(view))
                .children(menu)
                .into_any_element(),
        )
    }

    pub(crate) fn handle_beads_request(
        &mut self,
        request: BeadsRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        match request {
            BeadsRequest::OpenSession {
                project_id,
                session_id,
            } => {
                // The session lives on the computer that owns the project (a remote one too).
                let machine = self
                    .beads_target()
                    .map(|target| target.machine)
                    .unwrap_or(MachineId::Local);
                let session = SessionKey {
                    machine,
                    project_id,
                    session_id,
                }
                .to_sidebar_session_id();
                self.dispatch_native_sidebar_ui(
                    json!({ "type": "selectSession", "mode": "focus", "sessionId": session }),
                    cx,
                );
            }
            BeadsRequest::SaveLayout(layout) => self.save_beads_layout(layout),
            BeadsRequest::SaveSideWidth(width) => save_side_width(width),
            BeadsRequest::Search(query) => self.native_kanban_set_search_query(query, cx),
            BeadsRequest::Changed => self.native_kanban_refresh_quietly(cx),
            BeadsRequest::Refresh => self.native_kanban_refresh_now(cx),
            BeadsRequest::StartWork { issue } => {
                let issue = crate::app::native_kanban::model::BeadsIssue::from_value(&issue);
                match issue {
                    Some(issue) => {
                        let id = issue.id.clone();
                        self.native_kanban_upsert_local_issue(issue);
                        self.native_kanban_start_work(&id, cx);
                    }
                    None => self.native_kanban_toast(
                        "Couldn't start work",
                        "The new bead was created; start work on it from its card.",
                        cx,
                    ),
                }
            }
            BeadsRequest::InstallBeads => self.beads_install(cx),
        }
        cx.notify();
    }

    /// A Beads view menu row was picked in the sidebar's menu.
    pub(crate) fn run_beads_menu_command(
        &mut self,
        command: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.native_beads.clone() else {
            return;
        };
        // CDXC:ProjectBoard 2026-11-11 WHY: the sidebar's menu runs a row while it updates this app, and a row can call back into the app (copy, the Board's refresh); running it after that update ends keeps the app from being borrowed twice.
        let command = command.clone();
        cx.defer_in(window, move |_, window, cx| {
            view.update(cx, |view, cx| view.run_menu_command(&command, window, cx));
        });
    }

    /// The Board's lane `+`: the Beads view's New bead form with that lane's status. False when
    /// there is no Beads view to show it in.
    pub(crate) fn beads_open_create_in_lane(
        &mut self,
        lane_key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.native_beads.clone() else {
            return false;
        };
        let status = crate::app::native_kanban::model::board_status_beads_value(
            lane_key,
            &self.native_kanban.columns,
        );
        view.update(cx, |view, cx| {
            view.open_create(None, Some(status), window, cx)
        });
        true
    }

    /// CDXC:ProjectBoard 2026-11-11 DECISION:
    /// User: someone who had the Kanban tab saved opens the Beads view on Board; everyone else starts on Tree. Read once, the first time this Ghostex starts without a saved `beadsViewLayout`: the restored view tabs say whether the Kanban tab was saved, and the answer is written so later starts (and a Beads tab opened since) never move it again.
    #[cfg(not(ghostex_web))]
    pub(crate) fn migrate_beads_layout_setting(&mut self) {
        let saved: Option<serde_json::Map<String, Value>> =
            std::fs::read_to_string(shared_settings::shared_sidebar_settings_path())
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok());
        if saved
            .as_ref()
            .is_some_and(|saved| saved.contains_key(BEADS_LAYOUT_SETTING))
        {
            return;
        }
        // Any saved trace of an open Kanban tab counts: the open views (older saved states have
        // no view strip), the strip, and every project's own saved views.
        let kanban = crate::app::model::TitlebarMode::Kanban;
        let in_strip = |layout: &crate::app::model::GpuiViewStripLayout| {
            layout
                .order
                .contains(&crate::app::model::ViewStripTabKey::View(kanban))
        };
        let kanban_tab_saved = self.open_views.contains(&kanban)
            || in_strip(&self.view_strip_layout)
            || self.project_view_states_by_project.values().any(|state| {
                state.open_views.contains(&kanban) || in_strip(&state.view_strip_layout)
            });
        self.save_beads_layout(if kanban_tab_saved {
            BeadsLayout::Board
        } else {
            BeadsLayout::Tree
        });
    }

    #[cfg(not(ghostex_web))]
    fn save_beads_layout(&mut self, layout: BeadsLayout) {
        let mut settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.insert(
            BEADS_LAYOUT_SETTING.to_string(),
            Value::String(layout.setting_value().to_string()),
        );
        let _ = shared_settings::write_shared_sidebar_settings_object(settings);
    }

    /// The page cannot save settings: the choice lasts while the page is open.
    #[cfg(ghostex_web)]
    fn save_beads_layout(&mut self, _layout: BeadsLayout) {}

    #[cfg(not(ghostex_web))]
    fn beads_install(&mut self, cx: &mut Context<Self>) {
        self.native_kanban_install_beads(cx);
    }

    #[cfg(ghostex_web)]
    fn beads_install(&mut self, cx: &mut Context<Self>) {
        self.native_kanban_toast(
            "Install Beads from the desktop app",
            "Open Ghostex on that computer and use Install Beads there.",
            cx,
        );
    }
}
