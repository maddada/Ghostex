//! The app's side of the Beads view, shared by the desktop and the GPUI web build: which project
//! it shows, the sessions "who's on it" reads, the Board drawn under its header, and the answers
//! to what the view asks for.

use ghostex_gx_core::beads_view::{BeadAgentSession, BeadsClock};
use ghostex_gx_core::{MachineId, SessionKey};
use gpui::{AnyElement, AppContext as _, ClipboardItem, Context, IntoElement, Window};
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

impl GhostexGpuiApp {
    /// The active project, as the Beads view reads it.
    pub(crate) fn beads_target(&self) -> Option<BeadsTarget> {
        let key = self.git_active_scoped_project_id()?;
        let scope = self.git_scope_for_project(&key).ok()?;
        if scope.projectless {
            return None;
        }
        Some(BeadsTarget {
            project_key: scope.scoped_id(),
            rpc_project_id: scope.project_id(),
            project_path: scope.path.clone().unwrap_or_default(),
            remote: scope.remote.clone(),
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
        let view = cx.new(|cx| BeadsView::new(host, window, cx));
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
        let machine = self
            .git_scope_for_project(&target.project_key)
            .map(|scope| scope.key.machine.clone())
            .unwrap_or(MachineId::Local);
        let sessions = self.beads_agent_sessions(&machine);
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
        Some(gpui::AnyView::from(view).into_any_element())
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
                    .and_then(|target| self.git_scope_for_project(&target.project_key).ok())
                    .map(|scope| scope.key.machine.clone())
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
            BeadsRequest::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
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
        if let Some(view) = self.native_beads.clone() {
            view.update(cx, |view, cx| view.run_menu_command(command, window, cx));
        }
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
    #[cfg(not(target_arch = "wasm32"))]
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
        let kanban_tab_saved =
            self.view_strip_layout
                .order
                .contains(&crate::app::model::ViewStripTabKey::View(
                    crate::app::model::TitlebarMode::Kanban,
                ));
        self.save_beads_layout(if kanban_tab_saved {
            BeadsLayout::Board
        } else {
            BeadsLayout::Tree
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
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
    #[cfg(target_arch = "wasm32")]
    fn save_beads_layout(&mut self, _layout: BeadsLayout) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn beads_install(&mut self, cx: &mut Context<Self>) {
        self.native_kanban_install_beads(cx);
    }

    #[cfg(target_arch = "wasm32")]
    fn beads_install(&mut self, cx: &mut Context<Self>) {
        self.native_kanban_toast(
            "Install Beads from the desktop app",
            "Open Ghostex on that computer and use Install Beads there.",
            cx,
        );
    }
}
