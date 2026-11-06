use crate::*;

pub(crate) type GpuiTerminalViewerOwner = (
    Option<String>,
    GpuiEngineTerminalEventTarget,
    AgentsTerminalRuntimeSessionId,
);

#[derive(Clone)]
pub(crate) struct GpuiTerminalViewerRecipe {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) working_directory: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) env_vars: Vec<(String, String)>,
    pub(crate) wait_after_command: bool,
    /// The grid the released viewer was last shown at; the next viewer spawns its PTY at it.
    ///
    /// CDXC:Zmx 2026-11-06 WHY:
    /// A viewer that started at the 80x24 placeholder and resized on its first prepaint lost the
    /// race whenever `zmx attach` read its size first: the daemon applied 80 columns to the agent,
    /// then the pane's real width, two resizes per switch back from Chat. Agents built on pi-tui
    /// (ZCode, Pi) clear the screen and reprint the whole conversation on every width change, so
    /// a session days old flooded the terminal for minutes, worst over a remote link (GitHub #429).
    pub(crate) grid: Option<(u16, u16)>,
}

pub(crate) struct GpuiRetiringTerminalViewer {
    view: Entity<terminal_element::TerminalView>,
    _observation: gpui::Subscription,
}

/// Windows: how many recently shown terminals keep their viewer while their chat is shown.
const WARM_TERMINAL_VIEWERS: usize = 3;

pub(crate) struct GpuiTerminalChatClaimState {
    runtime_session_id: AgentsTerminalRuntimeSessionId,
    owner: Option<terminal_chat_claim::TerminalChatClaim>,
    retry_after: Instant,
}

impl GhostexGpuiApp {
    pub(crate) fn agents_terminal_chat_is_visible(&self, session_id: TerminalSessionId) -> bool {
        self.agents_chat_mode_sessions.contains(&session_id)
            && self.agents_workspace_visible()
            && self
                .agents_workspace
                .rendered_leaf_order()
                .iter()
                .any(|pane_id| {
                    self.agents_workspace.active_session_in_pane(*pane_id) == Some(session_id)
                })
    }

    pub(crate) fn sync_agents_terminal_chat_claims(&mut self, cx: &mut gpui::Context<Self>) {
        if self.terminal_bell_notifications_enabled() {
            let ids = self
                .agents_gpui_terminal_viewer_recipes
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for id in ids {
                self.ensure_agents_gpui_engine_terminal_view(id, cx);
            }
            self.agents_terminal_chat_claims.clear();
            return;
        }
        // Windows: gxserver holds the chat claim for a shown chat (`chat_grid_claims.rs`); a second
        // attach here would cost a PowerShell start and its `ZMX_CHAT` would never reach wmx.
        if cfg!(windows) {
            self.agents_terminal_chat_claims.clear();
            return;
        }
        let retained =
            self.agents_terminal_chat_claims
                .keys()
                .copied()
                .filter(|id| {
                    self.agents_terminal_has_detachable_viewer(*id)
                        && (self.agents_terminal_chat_is_visible(*id)
                            || (self.agents_terminal_viewer_is_visible(*id)
                                && self.agents_gpui_engine_terminals.get(id).is_none_or(
                                    |record| record.view.read(cx).zmx_visible_announce_pending(),
                                )))
                })
                .collect::<HashSet<_>>();
        self.agents_terminal_chat_claims.retain(|id, state| {
            retained.contains(id)
                && self
                    .agents_gpui_terminal_viewer_recipes
                    .get(id)
                    .is_some_and(|recipe| recipe.runtime_session_id == state.runtime_session_id)
        });
        let needed = self
            .agents_gpui_terminal_viewer_recipes
            .keys()
            .copied()
            .filter(|id| self.agents_terminal_chat_is_visible(*id))
            .collect::<Vec<_>>();
        let mut pending = false;
        let mut poll_delay = Duration::from_secs(2);
        for id in needed {
            if self.terminal_bell_notifications_enabled()
                || self
                    .agents_gpui_engine_terminals
                    .get(&id)
                    .is_some_and(|record| {
                        record.viewer_is_pinned()
                            || record.view.read(cx).model().has_pending_input()
                            || !record.view.read(cx).model().viewer_detach_supported()
                            || self.agents_terminal_viewer_has_pending_work(id)
                            || self.retiring_gpui_terminal_viewers.contains_key(
                                &Self::terminal_viewer_owner(
                                    self.agents_workspace_project_id.as_deref(),
                                    GpuiEngineTerminalEventTarget::Agents(id),
                                    record.runtime_session_id,
                                ),
                            )
                    })
            {
                self.agents_terminal_chat_claims.remove(&id);
                continue;
            }
            if let Some(state) = self.agents_terminal_chat_claims.get(&id) {
                if state.owner.as_ref().is_some_and(|owner| owner.is_alive()) {
                    let waiting = state.owner.as_ref().is_some_and(|owner| !owner.is_ready());
                    pending |= waiting;
                    if waiting && state.retry_after > Instant::now() {
                        poll_delay = Duration::from_millis(250);
                    }
                    continue;
                }
                if state.retry_after > Instant::now() {
                    pending = true;
                    continue;
                }
            }
            let recipe = &self.agents_gpui_terminal_viewer_recipes[&id];
            let mut config = terminal_gpui_engine::gpui_engine_terminal_spawn_config(
                recipe.working_directory.clone(),
                recipe.command.clone(),
                recipe.env_vars.clone(),
                0,
            );
            if let Some(record) = self.agents_gpui_engine_terminals.get(&id) {
                config.rows = record.view.read(cx).model().size().1;
            }
            let owner = terminal_chat_claim::TerminalChatClaim::spawn(config).ok();
            if owner.is_some() {
                poll_delay = Duration::from_millis(250);
            }
            self.agents_terminal_chat_claims.insert(
                id,
                GpuiTerminalChatClaimState {
                    runtime_session_id: recipe.runtime_session_id,
                    owner,
                    retry_after: Instant::now() + Duration::from_secs(2),
                },
            );
            pending = true;
        }
        if pending {
            self.schedule_terminal_viewer_reconcile(poll_delay, cx);
        }
    }

    /// CDXC:Terminal 2026-09-13 DECISION:
    /// User: create terminal viewers only when needed, and release unused viewers while zmx daemons and their agents remain running.
    /// Keep direct PTY owners and pending native operations attached; a detached zmx viewer retains only its native attach recipe, never its emulator or rendering subscription.
    /// Carve-out 2026-09-19: viewers that were on screen when their project was left stay attached for the `projectSwitchKeepAliveMinutes` window (see `project_keep_alive.rs`); hidden tabs still release as above.
    pub(crate) fn agents_terminal_viewer_is_visible(&self, session_id: TerminalSessionId) -> bool {
        if self.agents_chat_mode_sessions.contains(&session_id) {
            return false;
        }
        self.agents_workspace_visible()
            && self
                .agents_workspace
                .rendered_leaf_order()
                .iter()
                .any(|pane_id| {
                    self.agents_workspace.active_session_in_pane(*pane_id) == Some(session_id)
                })
    }

    pub(crate) fn agents_terminal_has_detachable_viewer(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        self.agents_gpui_terminal_viewer_recipes
            .get(&session_id)
            .is_some_and(|recipe| {
                self.agents_terminal_runtime_sessions
                    .runtime_session_id_for_shell_session(session_id)
                    == Some(recipe.runtime_session_id)
            })
            && self.agents_gpui_engine_terminal_is_zmx_client(session_id)
    }

    pub(crate) fn gpui_terminal_viewer_matches_entity(
        &self,
        target: GpuiEngineTerminalEventTarget,
        view_id: gpui::EntityId,
    ) -> bool {
        match target {
            GpuiEngineTerminalEventTarget::Agents(id) => self.agents_gpui_engine_terminals.get(&id),
            GpuiEngineTerminalEventTarget::Command(id) => {
                self.command_gpui_engine_terminals.get(&id)
            }
        }
        .is_some_and(|record| record.view.entity_id() == view_id)
    }

    pub(crate) fn terminal_viewer_target_is_daemon_backed(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> bool {
        if !cfg!(unix) {
            return self.agents_terminal_is_local_wmx_viewer(target);
        }
        match target {
            GpuiEngineTerminalEventTarget::Agents(id) => {
                self.agents_gpui_engine_terminal_is_zmx_client(id)
            }
            GpuiEngineTerminalEventTarget::Command(id) => {
                self.command_gxserver_session_key_for_command_tab(id)
                    .is_some()
                    || self
                        .command_remote_action_session_for_command_tab(id)
                        .is_some()
            }
        }
    }

    /// Windows: an Agents viewer whose command attaches to a session on this computer's wmx, the
    /// only viewer whose claims and detach can use the attach control pipe. A remote session's
    /// viewer runs `zmx attach` over SSH, where nothing reaches it around ConPTY.
    pub(crate) fn agents_terminal_is_local_wmx_viewer(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> bool {
        let GpuiEngineTerminalEventTarget::Agents(id) = target else {
            return false;
        };
        cfg!(windows)
            && self
                .agents_workspace
                .session(id)
                .is_some_and(|session| session.zmx_session_name.is_some())
            && !self
                .remote_attach_sessions
                .values()
                .any(|remote_session_id| *remote_session_id == id)
    }

    /// CDXC:Zmx 2026-10-08 DECISION:
    /// User: "pls dont break things for users who set default to terminal at all". On Windows a viewer costs a PowerShell start (about 0.6 s), so only a session shown in Chat View waits for its viewer (it is spawned on the first switch to its terminal) or gives it up; a session in terminal view keeps its viewer exactly as before, shown or hidden, in this project or a parked one. macOS and Linux keep their existing policy, where a viewer is a cheap `zmx attach`.
    pub(crate) fn agents_terminal_viewer_may_wait(&self, session_id: TerminalSessionId) -> bool {
        !cfg!(windows) || self.agents_chat_mode_sessions.contains(&session_id)
    }

    /// Windows: the sessions whose terminal was shown most recently keep their viewer while their
    /// chat is shown, so switching back to their terminal starts no PowerShell.
    pub(crate) fn note_agents_terminal_viewer_shown(&mut self, session_id: TerminalSessionId) {
        if !cfg!(windows) {
            return;
        }
        let warm = &mut self.agents_terminal_warm_viewers;
        if warm.front() == Some(&session_id) {
            return;
        }
        warm.retain(|id| *id != session_id);
        warm.push_front(session_id);
        warm.truncate(WARM_TERMINAL_VIEWERS);
    }

    pub(crate) fn remember_gpui_terminal_viewer_recipe(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        recipe: GpuiTerminalViewerRecipe,
    ) {
        if !self.terminal_viewer_target_is_daemon_backed(target) || recipe.command.is_none() {
            return;
        }
        match target {
            GpuiEngineTerminalEventTarget::Agents(id) => {
                self.agents_gpui_terminal_viewer_recipes.insert(id, recipe);
            }
            GpuiEngineTerminalEventTarget::Command(id) => {
                self.command_gpui_terminal_viewer_recipes.insert(id, recipe);
            }
        }
    }

    pub(crate) fn ensure_agents_gpui_engine_terminal_view(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.agents_gpui_engine_terminals.contains_key(&session_id) {
            return true;
        }
        if !self.agents_terminal_has_detachable_viewer(session_id) {
            return false;
        }
        if !self
            .agents_workspace
            .session(session_id)
            .is_some_and(|session| {
                session.presentation_state == TerminalSessionPresentationState::Running
            })
        {
            return false;
        }
        let recipe = self.agents_gpui_terminal_viewer_recipes[&session_id].clone();
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        // A session first opened in Chat has no grid of its own yet; the last Agents terminal
        // shown is the likeliest size of its pane, and a wrong guess costs no more than 80x24.
        let grid = recipe.grid.or(self.agents_terminal_last_shown_grid);
        let Some(record) = self.spawn_gpui_engine_terminal_record(
            GpuiEngineTerminalEventTarget::Agents(session_id),
            recipe.runtime_session_id,
            recipe.working_directory,
            recipe.command,
            recipe.env_vars,
            None,
            recipe.wait_after_command,
            grid,
            &settings,
            cx,
        ) else {
            return false;
        };
        self.agents_gpui_engine_terminals.insert(session_id, record);
        if crate::support_logs::scenario_enabled(
            crate::support_logs::GpuiDiagnosticScenario::TerminalFocus,
        ) {
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::TerminalFocus,
                "engineViewerSpawned",
                serde_json::json!({"session": session_id.0}),
            );
        }
        cx.notify();
        true
    }

    pub(crate) fn ensure_command_gpui_engine_terminal_view(
        &mut self,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.command_gpui_engine_terminals.contains_key(&session_id) {
            return true;
        }
        let Some(recipe) = self
            .command_gpui_terminal_viewer_recipes
            .get(&session_id)
            .cloned()
        else {
            return false;
        };
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        let Some(record) = self.spawn_gpui_engine_terminal_record(
            GpuiEngineTerminalEventTarget::Command(session_id),
            recipe.runtime_session_id,
            recipe.working_directory,
            recipe.command,
            recipe.env_vars,
            None,
            recipe.wait_after_command,
            recipe.grid,
            &settings,
            cx,
        ) else {
            return false;
        };
        self.command_gpui_engine_terminals
            .insert(session_id, record);
        cx.notify();
        true
    }

    pub(crate) fn agents_terminal_viewer_has_pending_work(&self, id: TerminalSessionId) -> bool {
        self.agents_workspace
            .session(id)
            .is_some_and(|session| session.delayed_send_active)
            || self.agents_delayed_send_timers.contains_key(&id)
            || self.agents_send_when_stopped_watchers.contains_key(&id)
            || self
                .pending_session_terminal_composer_insert
                .contains_key(&id)
            || self.pending_session_chat_draft_handoffs.contains(&id)
            || self.agents_sessions_pending_surface_transfer.contains(&id)
            || self
                .agents_terminal_runtime_sessions
                .runtime_session_id_for_shell_session(id)
                .is_some_and(|runtime| {
                    self.has_pending_remote_prompt_editor_for_engine_target(
                        GpuiEngineTerminalEventTarget::Agents(id),
                        runtime,
                    )
                })
            || self.terminal_paste_confirmation_dialog_open
    }

    pub(crate) fn terminal_bell_notifications_enabled(&self) -> bool {
        shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("showNotificationOnTerminalBell")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    fn terminal_viewer_owner(
        project: Option<&str>,
        target: GpuiEngineTerminalEventTarget,
        runtime: AgentsTerminalRuntimeSessionId,
    ) -> GpuiTerminalViewerOwner {
        (project.map(str::to_owned), target, runtime)
    }

    fn terminal_viewer_can_begin_retiring(
        record: &terminal_gpui_engine::GpuiEngineTerminalRecord,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        !matches!(
            record
                .view
                .update(cx, |view, _| view.model_mut().prepare_viewer_detach()),
            terminal_model::ViewerDetachPreparation::Unsupported
        )
    }

    fn finish_retiring_gpui_terminal_viewers(&mut self, cx: &mut gpui::Context<Self>) {
        let mut pending = HashMap::new();
        for (owner, retired) in std::mem::take(&mut self.retiring_gpui_terminal_viewers) {
            let ready = retired.view.update(cx, |view, _| {
                matches!(
                    view.model_mut().prepare_viewer_detach(),
                    terminal_model::ViewerDetachPreparation::Ready
                )
            });
            if ready {
                drop(retired._observation);
                Self::detach_gpui_terminal_view(retired.view, cx);
            } else {
                pending.insert(owner, retired);
            }
        }
        self.retiring_gpui_terminal_viewers = pending;
    }

    fn schedule_terminal_viewer_reconcile(
        &mut self,
        delay: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.terminal_viewer_reconcile_pending {
            return;
        }
        self.terminal_viewer_reconcile_pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                this.terminal_viewer_reconcile_pending = false;
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn release_unused_agents_gpui_terminal_viewers(
        &mut self,
        parking: bool,
        kept_alive: &HashSet<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.finish_retiring_gpui_terminal_viewers(cx);
        if parking {
            self.agents_terminal_chat_claims.clear();
        }
        if self.terminal_bell_notifications_enabled() {
            return;
        }
        if parking {
            self.agents_terminal_chat_claims.clear();
        } else {
            self.sync_agents_terminal_chat_claims(cx);
        }
        if !parking
            && (self.workspace_tab_drag_active
                || self.command_tab_drag_active
                || self.workspace_split_drag.is_some()
                || self.workarea_split_drag.is_some())
        {
            return;
        }
        let remove = self
            .agents_gpui_engine_terminals
            .iter()
            .filter_map(|(id, record)| {
                (self.agents_terminal_has_detachable_viewer(*id)
                    && !(parking && kept_alive.contains(id))
                    && (parking || !self.agents_terminal_viewer_is_visible(*id))
                    && (!cfg!(windows)
                        || (self.agents_terminal_viewer_may_wait(*id)
                            && !self.agents_terminal_warm_viewers.contains(id)))
                    && (parking
                        || !self.agents_terminal_chat_is_visible(*id)
                        || cfg!(windows)
                        || self
                            .agents_terminal_chat_claims
                            .get(id)
                            .and_then(|state| state.owner.as_ref())
                            .is_some_and(|owner| owner.is_ready()))
                    && !record.viewer_is_pinned()
                    && !record.view.read(cx).model().has_pending_input()
                    && !self.agents_terminal_viewer_has_pending_work(*id)
                    && record.view.read(cx).exit_status().is_none()
                    && !self.retiring_gpui_terminal_viewers.contains_key(
                        &Self::terminal_viewer_owner(
                            self.agents_workspace_project_id.as_deref(),
                            GpuiEngineTerminalEventTarget::Agents(*id),
                            record.runtime_session_id,
                        ),
                    )
                    && Self::terminal_viewer_can_begin_retiring(record, cx))
                .then_some(*id)
            })
            .collect::<Vec<_>>();
        for id in remove {
            if let Some(record) = self.agents_gpui_engine_terminals.remove(&id) {
                if let Some(grid) = record.view.read(cx).shown_grid() {
                    self.agents_terminal_last_shown_grid = Some(grid);
                    if let Some(recipe) = self.agents_gpui_terminal_viewer_recipes.get_mut(&id) {
                        recipe.grid = Some(grid);
                    }
                }
                if crate::support_logs::scenario_enabled(
                    crate::support_logs::GpuiDiagnosticScenario::TerminalFocus,
                ) {
                    crate::support_logs::append(
                        crate::support_logs::GpuiSupportLog::TerminalFocus,
                        "engineViewerRetired",
                        serde_json::json!({"session": id.0}),
                    );
                }
                let owner = Self::terminal_viewer_owner(
                    self.agents_workspace_project_id.as_deref(),
                    GpuiEngineTerminalEventTarget::Agents(id),
                    record.runtime_session_id,
                );
                self.retire_gpui_terminal_viewer(owner, record, cx);
                self.agents_gpui_engine_terminal_zmx_visibility.remove(&id);
                self.agents_gpui_engine_close_confirms
                    .retain(|slot| slot.session_id != id);
            }
        }
        let mut retired = Vec::new();
        let keep = self.project_switch_keep_alive();
        // Windows keeps a parked project's viewers, as it always did (`agents_terminal_viewer_may_wait`).
        for (project_id, parked) in self
            .parked_agents_terminal_runtimes_by_project
            .iter_mut()
            .filter(|_| !cfg!(windows))
        {
            let ids = parked
                .gpui_engine_terminals
                .iter()
                .filter_map(|(id, record)| {
                    (parked.viewer_recipes.get(id).is_some_and(|recipe| {
                        recipe.runtime_session_id == record.runtime_session_id
                    }) && !record.viewer_is_pinned()
                        && !record.view.read(cx).model().has_pending_input()
                        && !parked.protected_viewer_sessions.contains(id)
                        && !parked.viewer_kept_alive(*id, keep)
                        && record.view.read(cx).exit_status().is_none()
                        && !self.retiring_gpui_terminal_viewers.contains_key(
                            &Self::terminal_viewer_owner(
                                Some(project_id),
                                GpuiEngineTerminalEventTarget::Agents(*id),
                                record.runtime_session_id,
                            ),
                        )
                        && Self::terminal_viewer_can_begin_retiring(record, cx))
                    .then_some(*id)
                })
                .collect::<Vec<_>>();
            for id in ids {
                if let Some(record) = parked.gpui_engine_terminals.remove(&id) {
                    if let Some(grid) = record.view.read(cx).shown_grid() {
                        if let Some(recipe) = parked.viewer_recipes.get_mut(&id) {
                            recipe.grid = Some(grid);
                        }
                    }
                    let owner = Self::terminal_viewer_owner(
                        Some(project_id),
                        GpuiEngineTerminalEventTarget::Agents(id),
                        record.runtime_session_id,
                    );
                    retired.push((owner, record));
                }
            }
        }
        for (owner, record) in retired {
            self.retire_gpui_terminal_viewer(owner, record, cx);
        }
        let workspace = &self.agents_workspace;
        let runtimes = &self.agents_terminal_runtime_sessions;
        self.agents_gpui_terminal_viewer_recipes
            .retain(|id, recipe| {
                workspace.has_session(*id)
                    && runtimes.runtime_session_id_for_shell_session(*id)
                        == Some(recipe.runtime_session_id)
                    && !workspace.session(*id).is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Sleeping
                    })
            });
    }

    pub(crate) fn release_unused_command_gpui_terminal_viewers(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.terminal_bell_notifications_enabled() {
            let ids = self
                .command_gpui_terminal_viewer_recipes
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for id in ids {
                self.ensure_command_gpui_engine_terminal_view(id, cx);
            }
            return;
        }
        if self.command_tab_drag_active {
            return;
        }
        let visible = self
            .command_pane
            .rendered_terminal_body_mount_slots()
            .into_iter()
            .map(|slot| slot.session_id)
            .collect::<HashSet<_>>();
        let remove = self
            .command_gpui_engine_terminals
            .iter()
            .filter_map(|(id, record)| {
                let session = self.command_pane.session(*id)?;
                (self.command_gpui_terminal_viewer_recipes.contains_key(id)
                    && !visible.contains(id)
                    && !self.has_pending_remote_prompt_editor_for_engine_target(
                        GpuiEngineTerminalEventTarget::Command(*id),
                        record.runtime_session_id,
                    )
                    && !record.viewer_is_pinned()
                    && !record.view.read(cx).model().has_pending_input()
                    && !session.delayed_send_active
                    && session.action_run_id.is_none()
                    && !self.terminal_paste_confirmation_dialog_open
                    && record.view.read(cx).exit_status().is_none()
                    && !self.retiring_gpui_terminal_viewers.contains_key(
                        &Self::terminal_viewer_owner(
                            self.command_pane_project_id.as_deref(),
                            GpuiEngineTerminalEventTarget::Command(*id),
                            record.runtime_session_id,
                        ),
                    )
                    && Self::terminal_viewer_can_begin_retiring(record, cx))
                .then_some(*id)
            })
            .collect::<Vec<_>>();
        for id in remove {
            if let Some(record) = self.command_gpui_engine_terminals.remove(&id) {
                if let Some(grid) = record.view.read(cx).shown_grid() {
                    if let Some(recipe) = self.command_gpui_terminal_viewer_recipes.get_mut(&id) {
                        recipe.grid = Some(grid);
                    }
                }
                let owner = Self::terminal_viewer_owner(
                    self.command_pane_project_id.as_deref(),
                    GpuiEngineTerminalEventTarget::Command(id),
                    record.runtime_session_id,
                );
                self.retire_gpui_terminal_viewer(owner, record, cx);
                self.command_gpui_engine_close_confirms
                    .retain(|slot| slot.session_id != id);
            }
        }
        let pane = &self.command_pane;
        self.command_gpui_terminal_viewer_recipes.retain(|id, _| {
            pane.session(*id)
                .is_some_and(|session| !session.is_sleeping)
        });
    }

    fn retire_gpui_terminal_viewer(
        &mut self,
        owner: GpuiTerminalViewerOwner,
        record: terminal_gpui_engine::GpuiEngineTerminalRecord,
        cx: &mut gpui::Context<Self>,
    ) {
        let terminal_gpui_engine::GpuiEngineTerminalRecord {
            view,
            _subscription,
            ..
        } = record;
        drop(_subscription);
        let ready = view.update(cx, |view, _| {
            matches!(
                view.model_mut().prepare_viewer_detach(),
                terminal_model::ViewerDetachPreparation::Ready
            )
        });
        if ready {
            Self::detach_gpui_terminal_view(view, cx);
        } else {
            view.update(cx, |view, _| view.release_viewer_emulator());
            let observation = cx.observe(&view, |_, _, cx| cx.notify());
            self.retiring_gpui_terminal_viewers.insert(
                owner,
                GpuiRetiringTerminalViewer {
                    view,
                    _observation: observation,
                },
            );
        }
    }

    fn detach_gpui_terminal_view(
        view: Entity<terminal_element::TerminalView>,
        cx: &mut gpui::Context<Self>,
    ) {
        view.update(cx, |view, _| {
            if let Err(error) = view.model_mut().detach_viewer() {
                eprintln!("Could not schedule terminal viewer detachment: {error}");
            }
        });
    }
}
