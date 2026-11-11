//! The Beads view's state and behaviour: which project it shows, polling `/api/beadsView`, the
//! Tree's options, the details panel, the new-bead form, the open menu, and every write. Drawing
//! is in render.rs, tree.rs, details.rs, create.rs and menus.rs.
//!
//! CDXC:ProjectBoard 2026-11-11 DECISION:
//! User: the Kanban view becomes the Beads view in the right side panel, with a Tree | Board switch in its header. Tree is the default: every bead under its epic, "Needs a decision" pinned on top, filters All / Needs a decision / Ready / In progress / Blocked / Deferred, dense two-line rows. Board is the existing Kanban board, muted to match Tree. Tapping a bead opens its details beside the tree, never over it; ✕ or Escape closes them. From the view a bead can be created (alone or as a child of an epic), claimed, closed, reopened, deferred and brought back, given a priority and labels, and commented on; decisions can be answered; Pull and Push sync. Starting an agent from a bead stays on the Board only.
//! SEE-ALSO: server/src/beads_view/ (the snapshot and the cache), packages/gx-core/src/beads_view/ (the Tree's rules), apps/desktop/src/app/native_beads/host.rs (the app's side).

use std::rc::Rc;
use std::time::Duration;

use ghostex_gx_core::beads_view::{
    BeadAgentSession, BeadAgents, BeadsClock, BeadsFilter, BeadsSort, BeadsTreeModel,
    BeadsTreeOptions, bead_agents, build_tree,
};
use ghostex_gx_protocol::beads_view::{BeadsViewRow, BeadsViewSnapshot, BeadsWorkspaceState};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, ListAlignment, ListState,
    Subscription, Task, Window, px,
};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};
use web_time::Instant;

use super::create::CreateForm;
use super::details::Details;
use super::menus::BeadsMenu;
use super::tree::TreeItem;
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::app::native_kanban::palette::KanbanPalette;
use crate::app::window::native_modal_kit::ModalPalette;

/// The view asks gxserver this often while it is on screen; an unchanged workspace costs gxserver
/// a file read, never a bd process.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Without a draw for this long the view is off screen and stops asking.
const VISIBLE_GRACE: Duration = Duration::from_secs(4);
/// Linked sessions live on project records; they are read again this often.
const PROJECTS_REFRESH: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BeadsLayout {
    #[default]
    Tree,
    Board,
}

impl BeadsLayout {
    pub(crate) fn setting_value(self) -> &'static str {
        match self {
            BeadsLayout::Tree => "tree",
            BeadsLayout::Board => "board",
        }
    }

    pub(crate) fn from_setting(value: &str) -> Option<Self> {
        match value {
            "tree" => Some(BeadsLayout::Tree),
            "board" => Some(BeadsLayout::Board),
            _ => None,
        }
    }
}

/// The project the view shows.
#[derive(Clone)]
pub(crate) struct BeadsTarget {
    /// The store's scoped id (`remote:<machine>:<id>` for a remote project).
    pub(crate) project_key: String,
    pub(crate) rpc_project_id: String,
    pub(crate) project_path: String,
    pub(crate) remote: Option<GpuiRemoteGxserverRequestTarget>,
}

impl PartialEq for BeadsTarget {
    fn eq(&self, other: &Self) -> bool {
        self.project_key == other.project_key && self.rpc_project_id == other.rpc_project_id
    }
}

/// What the app gives the view each frame.
pub(crate) struct BeadsHostInfo {
    pub(crate) palette: KanbanPalette,
    pub(crate) modal: ModalPalette,
    pub(crate) sessions: Vec<BeadAgentSession>,
    pub(crate) global_beads_directory: String,
    pub(crate) clock: BeadsClock,
    /// The Board's lanes, drawn by the app (`native_kanban`), when the layout is Board.
    pub(crate) board: Option<AnyElement>,
    /// The app can start an agent on a bead (the Board's Start work); the web build can too.
    pub(crate) can_start_work: bool,
    /// The `beadsViewLayout` setting (`None`: unset, Tree).
    pub(crate) saved_layout: Option<BeadsLayout>,
}

/// What the view asks the app to do.
pub(crate) enum BeadsRequest {
    OpenSession {
        project_id: String,
        session_id: String,
    },
    SaveLayout(BeadsLayout),
    /// The header's search, which the Board filters by too.
    Search(String),
    /// A write landed while the Board is shown: it reads Beads again (without its setup writes).
    Changed,
    /// The header's Refresh while the Board is shown: it reloads as its own Refresh does.
    Refresh,
    /// The Board's Create & Start: start an agent on the bead bd just created (its `create`
    /// row, so the Board knows the bead before it starts work on it).
    StartWork {
        issue: Value,
    },
    InstallBeads,
}

pub(crate) type BeadsHostFn = Rc<dyn Fn(BeadsRequest, &mut Window, &mut App)>;

/// A write's failure, shown as a card above the tree.
#[derive(Clone, Debug)]
pub(crate) struct BeadsNotice {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) details: Option<String>,
    pub(crate) pull_now: bool,
}

pub(crate) struct BeadsView {
    pub(crate) host: BeadsHostFn,
    pub(crate) focus: FocusHandle,
    pub(crate) target: Option<BeadsTarget>,
    pub(crate) generation: u64,
    pub(crate) layout: BeadsLayout,
    /// The setting as last seen, so a change made in Settings applies (and a page, which cannot
    /// save it, keeps the user's switch).
    last_saved_layout: Option<Option<BeadsLayout>>,
    pub(crate) palette: Option<KanbanPalette>,
    pub(crate) modal: Option<ModalPalette>,
    pub(crate) dismissed_sync: Option<String>,
    pub(crate) notice_details_open: Option<&'static str>,
    pub(crate) clock: BeadsClock,
    pub(crate) board: Option<AnyElement>,
    pub(crate) can_start_work: bool,
    pub(crate) sessions: Vec<BeadAgentSession>,
    pub(crate) global_beads_directory: String,
    pub(crate) snapshot: Option<BeadsViewSnapshot>,
    pub(crate) rows: Vec<BeadsViewRow>,
    /// The computer's gxserver predates the Beads view (`/api/beadsView` is unknown there).
    pub(crate) legacy: bool,
    pub(crate) load_error: Option<String>,
    pub(crate) loading: bool,
    pub(crate) options: BeadsTreeOptions,
    pub(crate) projects: Vec<Value>,
    pub(crate) projects_read_at: Option<Instant>,
    pub(crate) agents: BeadAgents,
    pub(crate) tree: BeadsTreeModel,
    pub(crate) items: Vec<TreeItem>,
    pub(crate) list: ListState,
    pub(crate) search: Entity<InputState>,
    pub(crate) details: Option<Details>,
    pub(crate) create: Option<CreateForm>,
    pub(crate) menu: Option<BeadsMenu>,
    pub(crate) menu_serial: u64,
    /// The open menu's row actions, as they were when it opened.
    pub(crate) menu_runs: Vec<Option<super::menus::MenuRun>>,
    /// Where the view was last drawn: its menus stay inside it.
    pub(crate) bounds: super::menus::ViewBounds,
    pub(crate) notice: Option<BeadsNotice>,
    pub(crate) writes_in_flight: usize,
    /// A create from the New bead form is on its way; the form ignores another submit until bd
    /// has answered (other writes do not hold the form).
    pub(crate) create_in_flight: bool,
    pub(crate) next_create_form_id: u64,
    pub(crate) hovered_row: Option<String>,
    pub(crate) last_drawn: Option<Instant>,
    poll: Option<Task<()>>,
    /// The generation the running poll was started for; it stops itself when that moves on.
    poll_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl BeadsView {
    pub(crate) fn new(host: BeadsHostFn, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search beads"));
        let subscription = cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| {
                if let InputEvent::Change = event {
                    let query = input.read(cx).value().to_string();
                    if query != this.options.query {
                        this.options.query = query.clone();
                        this.rebuild_tree();
                        (this.host.clone())(BeadsRequest::Search(query), window, cx);
                        cx.notify();
                    }
                }
            },
        );
        Self {
            host,
            focus: cx.focus_handle(),
            target: None,
            generation: 0,
            layout: BeadsLayout::Tree,
            last_saved_layout: None,
            palette: None,
            modal: None,
            dismissed_sync: None,
            notice_details_open: None,
            clock: BeadsClock::default(),
            board: None,
            can_start_work: false,
            sessions: Vec::new(),
            global_beads_directory: String::new(),
            snapshot: None,
            rows: Vec::new(),
            legacy: false,
            load_error: None,
            loading: false,
            options: BeadsTreeOptions::default(),
            projects: Vec::new(),
            projects_read_at: None,
            agents: BeadAgents::new(),
            tree: BeadsTreeModel::default(),
            items: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(600.0)),
            search,
            details: None,
            create: None,
            menu: None,
            menu_serial: 0,
            menu_runs: Vec::new(),
            bounds: Default::default(),
            notice: None,
            writes_in_flight: 0,
            create_in_flight: false,
            next_create_form_id: 0,
            hovered_row: None,
            last_drawn: None,
            poll: None,
            poll_generation: 0,
            _subscriptions: vec![subscription],
        }
    }

    /// Called by the app each frame the view is drawn.
    pub(crate) fn sync(
        &mut self,
        target: Option<BeadsTarget>,
        info: BeadsHostInfo,
        cx: &mut Context<Self>,
    ) {
        self.last_drawn = Some(Instant::now());
        self.palette = Some(info.palette);
        self.modal = Some(info.modal);
        self.clock = info.clock;
        self.board = info.board;
        self.can_start_work = info.can_start_work;
        self.global_beads_directory = info.global_beads_directory;
        if self.last_saved_layout != Some(info.saved_layout) {
            self.last_saved_layout = Some(info.saved_layout);
            self.layout = info.saved_layout.unwrap_or_default();
        }
        if self.sessions != info.sessions {
            self.sessions = info.sessions;
            self.recompute_agents();
        }
        if self.target != target {
            self.target = target;
            self.generation += 1;
            self.snapshot = None;
            self.rows.clear();
            self.legacy = false;
            self.load_error = None;
            self.details = None;
            self.create = None;
            self.menu = None;
            self.notice = None;
            self.projects.clear();
            self.projects_read_at = None;
            self.agents.clear();
            self.rebuild_tree();
            self.poll = None;
        }
        if (self.poll.is_none() || self.poll_generation != self.generation) && self.target.is_some()
        {
            self.start_polling(cx);
        }
    }

    pub(crate) fn workspace_state(&self) -> Option<BeadsWorkspaceState> {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.workspace.state.clone())
    }

    pub(crate) fn workspace_ready(&self) -> bool {
        self.workspace_state() == Some(BeadsWorkspaceState::Ready)
    }

    pub(crate) fn has_capability(&self, name: &str) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.has_capability(name))
    }

    pub(crate) fn row(&self, id: &str) -> Option<&BeadsViewRow> {
        self.rows.iter().find(|row| row.id == id)
    }

    pub(crate) fn rebuild_tree(&mut self) {
        let readiness = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.readiness.clone())
            .unwrap_or_default();
        self.tree = build_tree(
            &self.rows,
            &readiness,
            &self.agents,
            &self.options,
            &self.clock,
        );
        self.items = super::tree::flatten(&self.tree);
        self.list.reset(self.items.len());
    }

    fn recompute_agents(&mut self) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let Some(board) = self
            .projects
            .iter()
            .find(|project| {
                project.get("projectId").and_then(Value::as_str)
                    == Some(target.rpc_project_id.as_str())
            })
            .cloned()
        else {
            return;
        };
        let now_iso = ghostex_gx_core::iso_string_from_ms(self.clock.now_ms);
        self.agents = bead_agents(
            &board,
            &self.projects,
            &self.global_beads_directory,
            &now_iso,
            &self.sessions,
        );
        self.rebuild_tree();
    }

    fn visible(&self) -> bool {
        self.last_drawn
            .is_some_and(|drawn| drawn.elapsed() < VISIBLE_GRACE)
    }

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        let generation = self.generation;
        self.poll_generation = generation;
        self.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let Ok((current, should)) = this.update(cx, |this, _| {
                    (this.generation == generation, this.visible())
                }) else {
                    break;
                };
                if !current {
                    break;
                }
                if should {
                    let Ok(Some(fetch)) = this.update(cx, |this, cx| this.fetch_now(false, cx))
                    else {
                        break;
                    };
                    fetch.await;
                }
                cx.background_executor().timer(POLL_INTERVAL).await;
            }
        }));
    }

    /// One `/api/beadsView` read (and the project records when they are due). `refresh` asks
    /// gxserver to look for the workspace again (the header's Refresh).
    pub(crate) fn fetch_now(&mut self, refresh: bool, cx: &mut Context<Self>) -> Option<Task<()>> {
        let target = self.target.clone()?;
        let generation = self.generation;
        let known_revision = if refresh {
            None
        } else {
            self.snapshot
                .as_ref()
                .map(|snapshot| snapshot.revision.clone())
                .filter(|revision| !revision.is_empty())
        };
        let projects_due = self
            .projects_read_at
            .is_none_or(|read| read.elapsed() >= PROJECTS_REFRESH);
        if self.snapshot.is_none() {
            self.loading = true;
        }
        let mut params = json!({ "projectId": target.rpc_project_id });
        if let Some(revision) = known_revision {
            params["knownRevision"] = json!(revision);
        }
        if refresh {
            params["refresh"] = json!(true);
        }
        Some(cx.spawn(async move |this, cx| {
            let answer = gx_rpc(target.remote.clone(), "/api/beadsView", params).await;
            // CDXC:ProjectBoard 2026-11-11 WHY: an older gxserver answers `notFound` for /api/beadsView, but a page's fetch of a path that server does not know fails before any answer (no CORS headers on its 404), so a failure with no code is checked against a call every gxserver answers; if that one works, the computer's Ghostex predates the view.
            let server_older = match &answer {
                Err(error) if error.code.as_deref() == Some("notFound") => true,
                Err(error) if error.code.is_none() => {
                    gx_rpc(target.remote.clone(), "/api/listProjects", json!({}))
                        .await
                        .is_ok()
                }
                _ => false,
            };
            let projects = if projects_due {
                gx_rpc(target.remote.clone(), "/api/listProjects", json!({}))
                    .await
                    .ok()
                    .and_then(|answer| answer.get("projects").and_then(Value::as_array).cloned())
            } else {
                None
            };
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                if let Some(projects) = projects {
                    this.projects = projects;
                    this.projects_read_at = Some(Instant::now());
                    this.recompute_agents();
                }
                match answer {
                    Ok(answer) => this.apply_snapshot(answer),
                    Err(error) => {
                        if server_older {
                            this.legacy = true;
                            this.load_error = None;
                        } else {
                            this.load_error = Some(error.message);
                        }
                    }
                }
                cx.notify();
            });
        }))
    }

    fn apply_snapshot(&mut self, answer: Value) {
        let Ok(snapshot) = serde_json::from_value::<BeadsViewSnapshot>(answer) else {
            self.load_error = Some("Ghostex could not read the Beads answer.".to_string());
            return;
        };
        self.legacy = false;
        self.load_error = None;
        let unchanged = snapshot.unchanged;
        let rows = snapshot.beads.clone();
        let workspace_changed = self
            .snapshot
            .as_ref()
            .is_some_and(|old| old.workspace.key != snapshot.workspace.key);
        if workspace_changed {
            // CDXC:ProjectBoard 2026-11-11 WHY: another workspace (a redirect changed) is another set of beads: writes still in flight for the old one must not land their callbacks on this one, so the generation moves on and the poll restarts with it.
            self.generation += 1;
            self.details = None;
            self.create = None;
            self.menu = None;
            self.notice = None;
        }
        let mut snapshot = snapshot;
        snapshot.beads = None;
        if unchanged {
            // Keep the rows; take the rest (busy, sync progress) from the new answer.
            if let Some(old) = self.snapshot.as_ref() {
                snapshot.readiness = old.readiness.clone();
                snapshot.custom_statuses = old.custom_statuses.clone();
            }
        }
        self.snapshot = Some(snapshot);
        if let Some(rows) = rows {
            let changed = self.rows != rows;
            self.rows = rows;
            if changed {
                self.rebuild_tree();
                if let Some(details) = self.details.as_mut() {
                    details.stale = true;
                }
            }
        } else if workspace_changed {
            self.rows.clear();
            self.rebuild_tree();
        }
    }

    // ---- Options -------------------------------------------------------------------------------

    pub(crate) fn set_filter(&mut self, filter: BeadsFilter, cx: &mut Context<Self>) {
        self.options.filter = filter;
        self.rebuild_tree();
        cx.notify();
    }

    pub(crate) fn set_sort(&mut self, sort: BeadsSort, cx: &mut Context<Self>) {
        self.options.sort = sort;
        self.rebuild_tree();
        cx.notify();
    }

    pub(crate) fn toggle_show_done(&mut self, cx: &mut Context<Self>) {
        self.options.show_done = !self.options.show_done;
        self.rebuild_tree();
        cx.notify();
    }

    pub(crate) fn toggle_collapsed(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.options.collapsed.remove(key) {
            self.options.collapsed.insert(key.to_string());
        }
        self.rebuild_tree();
        cx.notify();
    }

    pub(crate) fn toggle_done_fold(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.options.open_done.remove(key) {
            self.options.open_done.insert(key.to_string());
        }
        self.rebuild_tree();
        cx.notify();
    }

    pub(crate) fn set_layout(
        &mut self,
        layout: BeadsLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layout == layout {
            return;
        }
        self.layout = layout;
        self.menu = None;
        (self.host.clone())(BeadsRequest::SaveLayout(layout), window, cx);
        cx.notify();
    }

    /// Asks for a menu; the next draw shows it with the sidebar's menu, which also closes it when
    /// its button is pressed again.
    pub(crate) fn open_menu(&mut self, menu: BeadsMenu, cx: &mut Context<Self>) {
        self.menu = Some(menu);
        cx.notify();
    }

    pub(crate) fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    // ---- Writes --------------------------------------------------------------------------------

    /// Runs one Beads action through gxserver, then reads the view again. `done` gets bd's reply.
    pub(crate) fn run_action(
        &mut self,
        params: Value,
        failure_title: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, &Value, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let generation = self.generation;
        let mut params = params;
        params["projectId"] = json!(target.rpc_project_id);
        params["projectPath"] = json!(target.project_path);
        params["projectBoardScope"] = json!(true);
        let is_create = params.get("action").and_then(Value::as_str) == Some("create");
        self.writes_in_flight += 1;
        self.create_in_flight |= is_create;
        self.notice = None;
        cx.notify();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let answer = gx_rpc(target.remote.clone(), "/api/runBeadsAction", params).await;
            let _ = cx.update_window(window_handle, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.writes_in_flight = this.writes_in_flight.saturating_sub(1);
                    if is_create {
                        this.create_in_flight = false;
                    }
                    if this.generation != generation {
                        return;
                    }
                    match answer {
                        Ok(reply) if reply.get("exitCode").and_then(Value::as_i64) == Some(0) => {
                            done(this, &reply, window, cx);
                            // CDXC:ProjectBoard 2026-11-11 WHY: a write from the Tree reloads the Board only while the Board is shown, and then without the Board's setup writes (backlog/test/review statuses, the prefix), so using the Tree never changes a workspace's configuration.
                            if this.layout == BeadsLayout::Board {
                                (this.host.clone())(BeadsRequest::Changed, window, cx);
                            }
                        }
                        Ok(reply) => {
                            this.notice = Some(super::cards::write_failure(failure_title, &reply));
                        }
                        Err(error) => {
                            this.notice = Some(BeadsNotice {
                                title: failure_title.to_string(),
                                message: error.message,
                                details: None,
                                pull_now: false,
                            });
                        }
                    }
                    if let Some(fetch) = this.fetch_now(false, cx) {
                        fetch.detach();
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub(crate) fn simple_action(
        &mut self,
        action: &str,
        issue_id: &str,
        extra: Value,
        failure_title: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut params = json!({ "action": action, "issueId": issue_id });
        if let (Some(params), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                params.insert(key.clone(), value.clone());
            }
        }
        self.run_action(params, failure_title, window, cx, |_, _, _, _| {});
    }

    pub(crate) fn start_sync(
        &mut self,
        kind: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        let action = if kind == "push" {
            "doltPush"
        } else {
            "doltPull"
        };
        self.run_action(
            json!({ "action": action }),
            if kind == "push" {
                "Push didn't start"
            } else {
                "Pull didn't start"
            },
            window,
            cx,
            |_, _, _, _| {},
        );
    }
}

impl gpui::Focusable for BeadsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}
