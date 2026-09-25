//! Keeping the native Kanban's board in step with Beads: which project it shows, loading and
//! polling the issues, and running Beads work off the main thread.

use std::time::Duration;

use gpui::{AppContext as _, Context, Window};
use gpui_component::input::{InputEvent, InputState};

use super::beads::{KanbanLoadResult, load_board};
use super::model::{
    build_board_columns, columns_signature, issues_signature, normalize_display_issue_key,
    normalize_issue_prefix, to_board_tickets,
};
use super::state::{KanbanLoadState, KanbanProject, KanbanProjectKey, KanbanRefreshMode};
use crate::GhostexGpuiApp;
use crate::app::helpers::{
    ProjectBoardBridgeRuntimeContext, project_board_bridge_runtime_context_from_snapshot,
};
use crate::app::model::TitlebarMode;

/// `PROJECT_BOARD_AUTO_REFRESH_INTERVAL_MS`: the board notices bd changes made elsewhere.
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(8);

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

    /// Called from the app's render: switches the board to the current project, creates the
    /// search box and focus handle once, and starts the poll. True when the project changed.
    pub(crate) fn native_kanban_sync_project(
        &mut self,
        project: &KanbanProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.native_kanban.focus.is_none() {
            self.native_kanban.focus = Some(cx.focus_handle());
        }
        if self.native_kanban.search.is_none() {
            let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search tickets"));
            let subscription = cx.subscribe_in(
                &search,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| match event {
                    InputEvent::Focus => this.native_kanban_input_focused(),
                    InputEvent::Change => {
                        let query = input.read(cx).value().to_string();
                        if query != this.native_kanban.search_query {
                            this.native_kanban.search_query = query;
                            this.native_kanban.invalidate_derived();
                        }
                        this.native_kanban_notify(cx);
                    }
                    _ => {}
                },
            );
            self.native_kanban.search = Some(search);
            self.native_kanban.search_subscription = Some(subscription);
        }
        let project_changed = self.native_kanban.project.as_ref() != Some(&project.key);
        if project_changed {
            self.native_kanban.reset_for_project(project.key.clone());
            self.native_kanban.display_key = normalize_display_issue_key(&project.display_name);
            let prefix_source = if project.display_name.trim().is_empty() {
                project
                    .key
                    .project_path
                    .rsplit('/')
                    .find(|part| !part.is_empty())
                    .unwrap_or_default()
                    .to_string()
            } else {
                project.display_name.clone()
            };
            self.native_kanban.issue_prefix = normalize_issue_prefix(&prefix_source);
            self.native_kanban.load_state = Some(KanbanLoadState::Idle);
            // Out of render: the conversation request reaches into the store (gx_store/create/board.rs).
            let generation = self.native_kanban.generation;
            cx.defer_in(window, move |this, _window, cx| {
                if this.native_kanban.generation == generation {
                    this.native_kanban_refresh(KanbanRefreshMode::Initial, cx);
                    this.native_kanban_request_conversation_state(cx);
                }
            });
        }
        if self.native_kanban.refresh_timer.is_none() {
            self.native_kanban.refresh_timer = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(AUTO_REFRESH_INTERVAL).await;
                    let Ok(()) = this.update(cx, |this, cx| {
                        if this.native_kanban_visible() {
                            this.native_kanban_refresh(KanbanRefreshMode::Background, cx);
                            this.native_kanban_request_conversation_state(cx);
                        }
                    }) else {
                        break;
                    };
                }
            }));
        }
        project_changed
    }

    /// A Kanban text field took focus: the page is GPUI's own, so nothing else keeps the keys.
    pub(crate) fn native_kanban_input_focused(&mut self) {
        self.pending_keyboard_handoff = None;
        self.reclaim_gpui_root_for_chrome_input_focus();
    }

    pub(crate) fn native_kanban_visible(&self) -> bool {
        self.active_mode == TitlebarMode::Kanban
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Kanban)
            && self.native_kanban.project.is_some()
    }

    /// Runs `work` with the current bridge context on the background executor and hands its
    /// result to `done`, unless the board has moved to another project meanwhile. False when there
    /// is no project to run it for.
    pub(crate) fn native_kanban_spawn<T, W, D>(
        &mut self,
        work: W,
        done: D,
        cx: &mut Context<Self>,
    ) -> bool
    where
        T: Send + 'static,
        W: FnOnce(&ProjectBoardBridgeRuntimeContext) -> T + Send + 'static,
        D: FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
    {
        let Some(context) = self.native_kanban_bridge_context() else {
            return false;
        };
        let generation = self.native_kanban.generation;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background.spawn(async move { work(&context) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.native_kanban.generation != generation {
                    return;
                }
                done(this, result, cx);
            });
        })
        .detach();
        true
    }

    /// `loadTickets`: one refresh at a time; a refresh asked for meanwhile runs after it, and a
    /// background poll during one is simply skipped.
    pub(crate) fn native_kanban_refresh(
        &mut self,
        mode: KanbanRefreshMode,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.native_kanban;
        if state.refreshing {
            if mode != KanbanRefreshMode::Background {
                state.queued_refresh = Some(match state.queued_refresh {
                    Some(queued) => queued.stronger(mode),
                    None => mode,
                });
            }
            return;
        }
        state.refreshing = true;
        if mode != KanbanRefreshMode::Background {
            state.load_state = Some(KanbanLoadState::Loading);
            state.error = None;
        }
        let prefix = state.issue_prefix.clone();
        let reconcile = mode.reconciles();
        if !self.native_kanban_spawn(
            move |context| load_board(context, reconcile, &prefix),
            move |this, result, cx| this.native_kanban_apply_load(mode, result, cx),
            cx,
        ) {
            self.native_kanban.refreshing = false;
        }
        // A background poll shows nothing until its answer differs from the board.
        if mode != KanbanRefreshMode::Background {
            self.native_kanban_notify(cx);
        }
    }

    fn native_kanban_apply_load(
        &mut self,
        mode: KanbanRefreshMode,
        result: Result<KanbanLoadResult, String>,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.native_kanban;
        state.refreshing = false;
        let shown_before = (
            state.load_state,
            state.error.clone(),
            state.initial_load_done,
        );
        let mut board_changed = false;
        if mode == KanbanRefreshMode::Initial {
            state.initial_load_done = true;
        }
        match result {
            Ok(loaded) => {
                if loaded.column_config != state.column_config {
                    state.column_config = loaded.column_config;
                }
                let columns = build_board_columns(&state.column_config);
                if columns_signature(&columns) != columns_signature(&state.columns) {
                    state.columns = columns;
                    board_changed = true;
                }
                let mut issues = loaded.issues;
                for issue in &mut issues {
                    if let Some(pending) = state.pending_moves.get(&issue.id) {
                        issue.status = pending.beads_status.clone();
                    }
                }
                let signature = format!(
                    "{}:{}:{}",
                    state.display_key,
                    columns_signature(&state.columns),
                    issues_signature(&issues)
                );
                if signature != state.issues_signature {
                    state.issues_signature = signature;
                    state.tickets = to_board_tickets(&issues, &state.display_key, &state.columns);
                    state.issues = issues;
                    board_changed = true;
                }
                if mode == KanbanRefreshMode::Background {
                    state.error = None;
                    if state.load_state != Some(KanbanLoadState::Loading) {
                        state.load_state = Some(KanbanLoadState::Ready);
                    }
                } else {
                    state.load_state = Some(KanbanLoadState::Ready);
                }
            }
            Err(error) => {
                if mode != KanbanRefreshMode::Background {
                    state.load_state = Some(KanbanLoadState::Error);
                    state.error = Some(error);
                }
            }
        }
        if board_changed {
            state.invalidate_derived();
        }
        let shown_after = (
            state.load_state,
            state.error.clone(),
            state.initial_load_done,
        );
        let queued = state.queued_refresh.take();
        if board_changed || shown_before != shown_after {
            self.native_kanban_notify(cx);
        }
        if let Some(queued) = queued {
            self.native_kanban_refresh(queued, cx);
        }
    }

    /// Rebuilds the tickets from `issues` after a local edit, keeping the lanes and ids in step.
    pub(crate) fn native_kanban_rebuild_tickets(&mut self) {
        let state = &mut self.native_kanban;
        state.tickets = to_board_tickets(&state.issues, &state.display_key, &state.columns);
        // A local edit makes the next refresh compare against something new.
        state.issues_signature.clear();
        state.invalidate_derived();
    }
}
