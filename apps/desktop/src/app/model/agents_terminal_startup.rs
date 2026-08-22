// C1 wave-3 re-cluster: Agents terminal startup runtime identity, launch-plan derivation, the startup coordinator, and Ghostty surface owner reconciliation, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TerminalSessionId(pub(crate) u64);


/*
CDXC:GPUITerminalRuntimeIdentity 2026-06-22-23:24:
Phase 3 separates process-lifetime Agents terminal runtime identity from durable shell `TerminalSessionId` and pane/body mount slots. Runtime ids bind Ghostty owners to the current app process only; they are not user-facing titles, not logs, not shell-state fields, and restored shell sessions intentionally receive fresh runtime ids.
*/
pub(crate) struct AgentsTerminalRuntimeSessionRegistry {
    pub(crate) runtime_ids_by_shell_session: HashMap<TerminalSessionId, AgentsTerminalRuntimeSessionId>,
    pub(crate) next_runtime_session_id: u64,
}


/*
CDXC:GPUIAgentsTerminalRuntimePerProject 2026-08-05:
Inactive project workspaces keep their live composited terminal owners beside
their parked shell models. The entities own the local shell/SSH attach clients,
so dropping them during a project or machine switch forces a fresh zmx attach
even though the persisted tab still says Running. Runtime ids, terminal
entities, OSC state, and close-confirm intent stay process-local and return only
to the exact project that parked them; none of this state is serialized.
*/
#[derive(Default)]
pub(crate) struct ParkedAgentsTerminalRuntime {
    pub(crate) runtime_sessions: AgentsTerminalRuntimeSessionRegistry,
    pub(crate) gpui_engine_terminals:
        HashMap<TerminalSessionId, terminal_gpui_engine::GpuiEngineTerminalRecord>,
    pub(crate) runtime_osc_states: HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
    pub(crate) gpui_engine_close_confirms: HashSet<AgentsTerminalBodyMountSlotId>,
}


impl Default for AgentsTerminalRuntimeSessionRegistry {
    fn default() -> Self {
        Self {
            runtime_ids_by_shell_session: HashMap::new(),
            next_runtime_session_id: 1,
        }
    }
}


impl AgentsTerminalRuntimeSessionRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reconcile_with_workspace(&mut self, workspace: &WorkspaceModel) {
        let shell_session_ids = workspace.terminal_session_ids();
        let current_shell_session_ids = shell_session_ids.iter().copied().collect::<HashSet<_>>();
        self.runtime_ids_by_shell_session
            .retain(|session_id, _| current_shell_session_ids.contains(session_id));

        for session_id in shell_session_ids {
            self.ensure_runtime_session_id(session_id);
        }
    }

    pub(crate) fn runtime_session_id_for_shell_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        self.runtime_ids_by_shell_session.get(&session_id).copied()
    }

    pub(crate) fn ensure_runtime_session_id(
        &mut self,
        session_id: TerminalSessionId,
    ) -> AgentsTerminalRuntimeSessionId {
        if let Some(runtime_session_id) = self.runtime_ids_by_shell_session.get(&session_id) {
            return *runtime_session_id;
        }

        let runtime_session_id = self.allocate_runtime_session_id();
        self.runtime_ids_by_shell_session
            .insert(session_id, runtime_session_id);
        runtime_session_id
    }

    pub(crate) fn rotate_runtime_session_id_for_shell_session(
        &mut self,
        session_id: TerminalSessionId,
    ) -> AgentsTerminalRuntimeSessionId {
        /*
        CDXC:GPUITerminalStartupRetryIdentity 2026-06-23-18:19:
        Explicit failed-startup retry is a new process-local runtime attempt for the same durable shell session. Rotate only the runtime id so the retry startup candidate, launch plan, and completion intent cannot reuse stale attempt identity while the shell `TerminalSessionId`, tab, title, and persisted state remain unchanged.
        */
        let runtime_session_id = self.allocate_runtime_session_id();
        self.runtime_ids_by_shell_session
            .insert(session_id, runtime_session_id);
        runtime_session_id
    }

    pub(crate) fn allocate_runtime_session_id(&mut self) -> AgentsTerminalRuntimeSessionId {
        let runtime_session_id = AgentsTerminalRuntimeSessionId(self.next_runtime_session_id);
        self.next_runtime_session_id += 1;
        runtime_session_id
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalStartupBodySlotId {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupBodyGeometry {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}


#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupLaunchPlan {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalStartupHostPreservationKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupRecord {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_geometry_available: bool,
}


impl AgentsTerminalStartupRecord {
    pub(crate) fn startup_body_slot_id(self) -> AgentsTerminalStartupBodySlotId {
        AgentsTerminalStartupBodySlotId {
            pane_id: self.pane_id,
            session_id: self.shell_session_id,
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupCompletionIntent {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
}


impl AgentsTerminalStartupCompletionIntent {
    pub(crate) fn from_record(
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        record: AgentsTerminalStartupRecord,
    ) -> Self {
        Self {
            runtime_session_id,
            shell_session_id: record.shell_session_id,
            startup_body_slot_id: record.startup_body_slot_id(),
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupReadinessSignalPreparation {
    pub(crate) completion_intent: AgentsTerminalStartupCompletionIntent,
    pub(crate) surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
}


impl AgentsTerminalStartupReadinessSignalPreparation {
    pub(crate) fn new(
        completion_intent: AgentsTerminalStartupCompletionIntent,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<Self> {
        (completion_intent.startup_body_slot_id == startup_body_slot_id
            && surface_metadata.indicates_ready_metadata())
        .then_some(Self {
            completion_intent,
            surface_metadata,
        })
    }
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalStartupReadinessHandoffPlan {
    pub(crate) completion_intent: AgentsTerminalStartupCompletionIntent,
    pub(crate) startup_launch_plan: AgentsTerminalStartupLaunchPlan,
    pub(crate) mount_slot_id: AgentsTerminalBodyMountSlotId,
}


#[cfg(target_os = "macos")]
impl AgentsTerminalStartupReadinessHandoffPlan {
    pub(crate) fn runtime_session_id(self) -> AgentsTerminalRuntimeSessionId {
        self.completion_intent.runtime_session_id
    }

    pub(crate) fn startup_body_slot_id(self) -> AgentsTerminalStartupBodySlotId {
        self.completion_intent.startup_body_slot_id
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AgentsTerminalStartupCompletionSignal {
    Ready {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
    Failed {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
}


impl AgentsTerminalStartupCompletionSignal {
    pub(crate) fn completion_intent(self) -> AgentsTerminalStartupCompletionIntent {
        match self {
            Self::Ready { completion_intent } | Self::Failed { completion_intent } => {
                completion_intent
            }
        }
    }

    pub(crate) fn into_startup_result(self) -> AgentsTerminalStartupResult {
        match self {
            Self::Ready { completion_intent } => {
                AgentsTerminalStartupResult::Ready { completion_intent }
            }
            Self::Failed { completion_intent } => {
                AgentsTerminalStartupResult::Failed { completion_intent }
            }
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AgentsTerminalStartupResult {
    Ready {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
    Failed {
        completion_intent: AgentsTerminalStartupCompletionIntent,
    },
}


impl AgentsTerminalStartupResult {
    pub(crate) fn completion_intent(self) -> AgentsTerminalStartupCompletionIntent {
        match self {
            Self::Ready { completion_intent } | Self::Failed { completion_intent } => {
                completion_intent
            }
        }
    }

    pub(crate) fn runtime_session_id(self) -> AgentsTerminalRuntimeSessionId {
        self.completion_intent().runtime_session_id
    }

    pub(crate) fn terminal_presentation_state(self) -> TerminalSessionPresentationState {
        match self {
            Self::Ready { .. } => TerminalSessionPresentationState::Running,
            Self::Failed { .. } => TerminalSessionPresentationState::StartupFailed,
        }
    }
}


/*
CDXC:GPUITerminalStartupBoundary 2026-06-22-23:50:
Agents terminal startup is a runtime-only boundary keyed by process-local runtime session id, not by durable shell session id or body mount slot id. Visible selected Mounting tabs may become pending startup records, but this layer does not launch a process, infer success, persist runtime ids, log commands, store cwd/env/stdout/stderr, or create fallback surfaces.

CDXC:GPUITerminalStartupBoundary 2026-06-22-23:50:
Startup results are intentionally enum-only. Ready may promote the same current Mounting shell session to Running; Failed preserves the tab as a safe failed-startup placeholder with no raw error string, command text, path, environment, terminal content, or process details in shell state.

CDXC:GPUITerminalStartupGeometry 2026-06-23-00:10:
Visible selected Mounting Agents bodies need runtime-only startup geometry for future launch preparation, but the startup slot id stays separate from the Running-only libghostty body mount slot so geometry alone never creates Running host state, a Ghostty surface, process, or Running transition.

CDXC:GPUITerminalStartupLaunchPlan 2026-06-23-00:22:
Phase 3 startup launch plans are a runtime-only readiness boundary for visible selected Mounting Agents bodies after exact body geometry exists. Plans may carry only runtime id, shell id, pane id, startup body slot id, bounds, and scale; they must not carry cwd, command, env, terminal content, stdout/stderr, process ids, logs, persisted fields, Ghostty hosts, Ghostty surfaces, Running mount slots, or Ready/Failed transitions by themselves.

CDXC:GPUITerminalStartupHostLifetime 2026-06-23-03:23:
A render-start geometry reset must not churn an already-created hidden startup host/config while the same pending Mounting tab remains current. Preserve only hosts previously created from a launch plan and only by matching runtime id plus `AgentsTerminalStartupBodySlotId`; pending records without prior geometry must not create hosts.

CDXC:GPUITerminalStartupCompletion 2026-06-23-03:51:
GPUI has no exposed GhosttyKit tty/pid or terminalReady-equivalent signal yet, so startup completion is a runtime-only intent plus explicit signal boundary. A current Mounting tab may advertise an exact runtime/session/startup-slot intent, but the producer returns no Ready/Failed result without a real signal and must never infer success from hidden startup host or Ghostty surface creation.

CDXC:GPUITerminalStartupLaunchPayloadSource 2026-06-23-04:00:
Startup config preparation now has a runtime-only launch-payload source boundary, but GPUI does not currently populate it because no explicit app startup state carries cwd, command, env vars, initial input, or wait-after-command. The empty source keeps startup requests inert until a future explicit producer is wired, and invalid future payloads must prune the startup boundary instead of falling back to inferred values.

CDXC:GPUITerminalStartupReadiness 2026-06-23-04:13:
Ghostty startup surface metadata is now a real runtime-only readiness input, but it may only create a handoff plan for the exact current startup completion intent when Ghostty reports a tty name and foreground process while the process has not exited. Promotion may proceed only when startup host/surface ownership can be moved into the Running path without dropping or recreating the process, and this layer must not create Failed, persist ids, log metadata, or expose raw tty names/process ids.

CDXC:GPUITerminalStartupRuntimeFailure 2026-06-23-04:38:
Startup-owned Ghostty metadata is also the real runtime failure input. A process-exited snapshot may produce only the existing Failed result for the exact current runtime/session/startup-slot intent while the shell tab is still the visible selected Mounting body and the startup surface owner identity still matches; cleanup must drop the startup Ghostty surface before the hidden host and must not create Running maps, fallback success, logs, raw process details, paths, commands, env, or terminal content.
*/
pub(crate) struct AgentsTerminalStartupCoordinator {
    pub(crate) pending_startups_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
    pub(crate) startup_launch_plans_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupLaunchPlan>,
    pub(crate) startup_completion_intents_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupCompletionIntent>,
    pub(crate) startup_readiness_signal_preparations_by_runtime_session:
        HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupReadinessSignalPreparation>,
}


impl Default for AgentsTerminalStartupCoordinator {
    fn default() -> Self {
        Self {
            pending_startups_by_runtime_session: HashMap::new(),
            startup_launch_plans_by_runtime_session: HashMap::new(),
            startup_completion_intents_by_runtime_session: HashMap::new(),
            startup_readiness_signal_preparations_by_runtime_session: HashMap::new(),
        }
    }
}


impl AgentsTerminalStartupCoordinator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sync_visible_mounting_startup_candidates(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
        startup_body_geometries: &HashMap<
            AgentsTerminalStartupBodySlotId,
            AgentsTerminalStartupBodyGeometry,
        >,
    ) {
        let current_candidates = if agents_workspace_visible {
            workspace.visible_selected_mounting_startup_candidates()
        } else {
            Vec::new()
        };
        let current_startup_body_slot_ids = current_candidates
            .iter()
            .map(|candidate| candidate.startup_body_slot_id())
            .collect::<HashSet<_>>();

        self.pending_startups_by_runtime_session
            .retain(|runtime_session_id, record| {
                agents_workspace_visible
                    && current_startup_body_slot_ids.contains(&record.startup_body_slot_id())
                    && workspace
                        .session(record.shell_session_id)
                        .is_some_and(|session| {
                            session.presentation_state == TerminalSessionPresentationState::Mounting
                                && runtime_sessions
                                    .runtime_session_id_for_shell_session(record.shell_session_id)
                                    == Some(*runtime_session_id)
                        })
            });
        let pending_runtime_session_ids = self
            .pending_startups_by_runtime_session
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        self.startup_launch_plans_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });
        self.startup_completion_intents_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });
        self.startup_readiness_signal_preparations_by_runtime_session
            .retain(|runtime_session_id, _| {
                pending_runtime_session_ids.contains(runtime_session_id)
            });

        if !agents_workspace_visible {
            return;
        }

        for mut candidate in current_candidates {
            let runtime_session_id =
                runtime_sessions.ensure_runtime_session_id(candidate.shell_session_id);
            candidate.startup_body_geometry_available =
                startup_body_geometries.contains_key(&candidate.startup_body_slot_id());
            self.pending_startups_by_runtime_session
                .insert(runtime_session_id, candidate);
        }
        self.sync_startup_completion_intents(agents_workspace_visible, workspace, runtime_sessions);
    }

    pub(crate) fn sync_startup_launch_plans(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        startup_body_geometries: &HashMap<
            AgentsTerminalStartupBodySlotId,
            AgentsTerminalStartupBodyGeometry,
        >,
    ) {
        self.startup_launch_plans_by_runtime_session = derive_agents_terminal_startup_launch_plans(
            agents_workspace_visible,
            workspace,
            runtime_sessions,
            startup_body_geometries,
            &self.pending_startups_by_runtime_session,
        );
        self.sync_startup_completion_intents(agents_workspace_visible, workspace, runtime_sessions);
    }

    pub(crate) fn sync_startup_completion_intents(
        &mut self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) {
        self.startup_completion_intents_by_runtime_session =
            derive_agents_terminal_startup_completion_intents(
                agents_workspace_visible,
                workspace,
                runtime_sessions,
                &self.pending_startups_by_runtime_session,
            );
        self.startup_readiness_signal_preparations_by_runtime_session
            .retain(|runtime_session_id, preparation| {
                self.startup_completion_intents_by_runtime_session
                    .get(runtime_session_id)
                    .copied()
                    == Some(preparation.completion_intent)
            });
    }

    pub(crate) fn sync_startup_readiness_signal_preparations(
        &mut self,
        metadata_snapshots: impl IntoIterator<
            Item = (
                AgentsTerminalRuntimeSessionId,
                AgentsTerminalStartupBodySlotId,
                terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
            ),
        >,
    ) {
        self.startup_readiness_signal_preparations_by_runtime_session = metadata_snapshots
            .into_iter()
            .filter_map(
                |(runtime_session_id, startup_body_slot_id, surface_metadata)| {
                    self.prepare_startup_readiness_signal(
                        runtime_session_id,
                        startup_body_slot_id,
                        surface_metadata,
                    )
                    .map(|preparation| (runtime_session_id, preparation))
                },
            )
            .collect();
    }

    pub(crate) fn prepare_startup_readiness_signal(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<AgentsTerminalStartupReadinessSignalPreparation> {
        let completion_intent = self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;

        (completion_intent.runtime_session_id == runtime_session_id)
            .then_some(())
            .and_then(|_| {
                AgentsTerminalStartupReadinessSignalPreparation::new(
                    completion_intent,
                    startup_body_slot_id,
                    surface_metadata,
                )
            })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_readiness_handoff_plans(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) -> Vec<AgentsTerminalStartupReadinessHandoffPlan> {
        let mut plans = self
            .startup_readiness_signal_preparations_by_runtime_session
            .keys()
            .copied()
            .filter_map(|runtime_session_id| {
                self.startup_readiness_handoff_plan_for_runtime_session(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    runtime_session_id,
                )
            })
            .collect::<Vec<_>>();
        plans.sort_by_key(|plan| {
            (
                plan.startup_body_slot_id().pane_id.0,
                plan.startup_body_slot_id().session_id.0,
                plan.runtime_session_id().0,
            )
        });
        plans
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_readiness_handoff_plan_for_runtime_session(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Option<AgentsTerminalStartupReadinessHandoffPlan> {
        /*
        CDXC:GPUITerminalStartupHandoff 2026-06-23-04:25:
        A ready metadata snapshot may promote only the exact current Mounting body it was prepared for. Match runtime id, shell session id, startup body slot id, visible selected Mounting state, current launch plan, and the future Running mount slot before any owner map can move.
        */
        if !agents_workspace_visible {
            return None;
        }

        let preparation = self
            .startup_readiness_signal_preparations_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let completion_intent = preparation.completion_intent;
        if completion_intent.runtime_session_id != runtime_session_id
            || !preparation.surface_metadata.indicates_ready_metadata()
            || self
                .startup_completion_intents_by_runtime_session
                .get(&runtime_session_id)
                .copied()
                != Some(completion_intent)
        {
            return None;
        }

        let record = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let startup_launch_plan = self
            .startup_launch_plans_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        let startup_body_slot_id = record.startup_body_slot_id();
        if record.shell_session_id != completion_intent.shell_session_id
            || startup_body_slot_id != completion_intent.startup_body_slot_id
            || startup_launch_plan.runtime_session_id != runtime_session_id
            || startup_launch_plan.shell_session_id != record.shell_session_id
            || startup_launch_plan.pane_id != record.pane_id
            || startup_launch_plan.startup_body_slot_id != startup_body_slot_id
            || runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            || !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id)
            || !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
        {
            return None;
        }

        let mount_slot_id = AgentsTerminalBodyMountSlotId {
            pane_id: startup_body_slot_id.pane_id,
            session_id: completion_intent.shell_session_id,
        };
        (mount_slot_id.session_id == startup_body_slot_id.session_id
            && mount_slot_id.pane_id == startup_launch_plan.pane_id)
            .then_some(AgentsTerminalStartupReadinessHandoffPlan {
                completion_intent,
                startup_launch_plan,
                mount_slot_id,
            })
    }

    #[allow(dead_code)]
    pub(crate) fn produce_startup_result_from_runtime_signal(
        &self,
        signal: Option<AgentsTerminalStartupCompletionSignal>,
    ) -> Option<AgentsTerminalStartupResult> {
        let signal = signal?;
        let completion_intent = signal.completion_intent();
        self.startup_completion_intents_by_runtime_session
            .get(&completion_intent.runtime_session_id)
            .copied()
            .is_some_and(|current_intent| current_intent == completion_intent)
            .then_some(signal.into_startup_result())
    }

    pub(crate) fn produce_failed_startup_result_from_surface_metadata(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        surface_metadata: terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
    ) -> Option<AgentsTerminalStartupResult> {
        if !agents_workspace_visible || !surface_metadata.process_exited() {
            return None;
        }

        let completion_intent = self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        if completion_intent.runtime_session_id != runtime_session_id
            || completion_intent.startup_body_slot_id != startup_body_slot_id
        {
            return None;
        }

        let record = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()?;
        if record.shell_session_id != completion_intent.shell_session_id
            || record.startup_body_slot_id() != startup_body_slot_id
            || runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            || !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id)
            || !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
        {
            return None;
        }

        Some(AgentsTerminalStartupResult::Failed { completion_intent })
    }

    pub(crate) fn apply_startup_result(
        &mut self,
        workspace: &mut WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        result: AgentsTerminalStartupResult,
    ) -> bool {
        let runtime_session_id = result.runtime_session_id();
        let completion_intent = result.completion_intent();
        let Some(record) = self
            .pending_startups_by_runtime_session
            .get(&runtime_session_id)
            .copied()
        else {
            return false;
        };

        if self
            .startup_completion_intents_by_runtime_session
            .get(&runtime_session_id)
            .copied()
            != Some(completion_intent)
            || record.shell_session_id != completion_intent.shell_session_id
            || record.startup_body_slot_id() != completion_intent.startup_body_slot_id
        {
            return false;
        }

        if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
            != Some(runtime_session_id)
        {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_readiness_signal_preparations_by_runtime_session
                .remove(&runtime_session_id);
            return false;
        }

        if !workspace.is_current_terminal_startup_body_slot(completion_intent.startup_body_slot_id)
        {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_readiness_signal_preparations_by_runtime_session
                .remove(&runtime_session_id);
            return false;
        }

        let changed = workspace.transition_terminal_session_presentation_state(
            record.shell_session_id,
            TerminalSessionPresentationState::Mounting,
            result.terminal_presentation_state(),
        );
        if changed || !workspace.session_is_mounting(record.shell_session_id) {
            self.pending_startups_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_launch_plans_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_completion_intents_by_runtime_session
                .remove(&runtime_session_id);
            self.startup_readiness_signal_preparations_by_runtime_session
                .remove(&runtime_session_id);
        }
        changed
    }

    // Consumed by the macOS hidden-host startup path and by the non-macOS
    // GPUI-engine startup path, so it stays ungated.
    pub(crate) fn startup_launch_plans(&self) -> Vec<AgentsTerminalStartupLaunchPlan> {
        let mut plans = self
            .startup_launch_plans_by_runtime_session
            .values()
            .copied()
            .collect::<Vec<_>>();
        plans.sort_by_key(|plan| {
            (
                plan.startup_body_slot_id.pane_id.0,
                plan.startup_body_slot_id.session_id.0,
                plan.runtime_session_id.0,
            )
        });
        plans
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn startup_host_preservation_keys(
        &self,
        agents_workspace_visible: bool,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) -> Vec<AgentsTerminalStartupHostPreservationKey> {
        let mut keys = derive_agents_terminal_startup_host_preservation_keys(
            agents_workspace_visible,
            workspace,
            runtime_sessions,
            &self.pending_startups_by_runtime_session,
        );
        keys.sort_by_key(|key| {
            (
                key.startup_body_slot_id.pane_id.0,
                key.startup_body_slot_id.session_id.0,
                key.runtime_session_id.0,
            )
        });
        keys
    }
}


pub(crate) fn derive_agents_terminal_startup_launch_plans(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    startup_body_geometries: &HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupLaunchPlan> {
    if !agents_workspace_visible {
        return HashMap::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            let geometry = startup_body_geometries
                .get(&startup_body_slot_id)
                .copied()?;
            Some((
                runtime_session_id,
                AgentsTerminalStartupLaunchPlan {
                    runtime_session_id,
                    shell_session_id: record.shell_session_id,
                    pane_id: record.pane_id,
                    startup_body_slot_id,
                    bounds: geometry.bounds,
                    scale_factor: geometry.scale_factor,
                },
            ))
        })
        .collect()
}


pub(crate) fn derive_agents_terminal_startup_host_preservation_keys(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> Vec<AgentsTerminalStartupHostPreservationKey> {
    if !agents_workspace_visible {
        return Vec::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            Some(AgentsTerminalStartupHostPreservationKey {
                runtime_session_id,
                startup_body_slot_id,
            })
        })
        .collect()
}


pub(crate) fn derive_agents_terminal_startup_completion_intents(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    pending_startups: &HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupRecord>,
) -> HashMap<AgentsTerminalRuntimeSessionId, AgentsTerminalStartupCompletionIntent> {
    if !agents_workspace_visible {
        return HashMap::new();
    }

    pending_startups
        .iter()
        .filter_map(|(runtime_session_id, record)| {
            let runtime_session_id = *runtime_session_id;
            let record = *record;

            if runtime_sessions.runtime_session_id_for_shell_session(record.shell_session_id)
                != Some(runtime_session_id)
            {
                return None;
            }

            if !workspace
                .session(record.shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Mounting
                })
            {
                return None;
            }

            let startup_body_slot_id = record.startup_body_slot_id();
            if !workspace.is_current_terminal_startup_body_slot(startup_body_slot_id) {
                return None;
            }

            Some((
                runtime_session_id,
                AgentsTerminalStartupCompletionIntent::from_record(runtime_session_id, record),
            ))
        })
        .collect()
}


pub(crate) fn prune_agents_terminal_startup_body_slot_geometries(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
) {
    let current_slot_ids = if agents_workspace_visible {
        agents_workspace.rendered_terminal_startup_body_slots()
    } else {
        Vec::new()
    };
    startup_body_geometries.retain(|slot_id, _| current_slot_ids.contains(slot_id));
}


pub(crate) fn record_agents_terminal_startup_body_slot_geometry(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    slot_id: AgentsTerminalStartupBodySlotId,
    bounds: Bounds<Pixels>,
    scale_factor: f32,
) {
    prune_agents_terminal_startup_body_slot_geometries(
        agents_workspace_visible,
        agents_workspace,
        startup_body_geometries,
    );

    if agents_workspace_visible && agents_workspace.is_current_terminal_startup_body_slot(slot_id) {
        startup_body_geometries.insert(
            slot_id,
            AgentsTerminalStartupBodyGeometry {
                bounds,
                scale_factor,
            },
        );
    } else {
        startup_body_geometries.remove(&slot_id);
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminal_ghostty_surface_config_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> terminal_ghostty_surface::GhosttySurfaceTerminalConfig {
    /*
    CDXC:GPUITerminalSettings 2026-06-24-11:27:
    GPUI embedded terminal surfaces consume the shared Settings service directly for supported `ghostty_surface_config_s` fields. Only `terminalFontSize` maps to the current FFI request as `font_size`; other Ghostty settings remain unthreaded here because GPUI has no safe direct runtime field or reload contract for them yet.

    CDXC:GPUICommandTerminalSettings 2026-06-27-10:10:
    Command-pane Ghostty surfaces share this bounded GPUI terminal-settings mapper with Agents surfaces. Apply the FFI-supported `terminalFontSize` to recreated/prepared surface requests, and keep font family, theme, cursor, scrollback, clipboard, paste-preview, and mouse settings on the Ghostty config-file path until GhosttyKit exposes a safe live request field or reload contract.
    */
    let terminal_config = settings.terminal_ghostty_surface_config();
    terminal_ghostty_surface::GhosttySurfaceTerminalConfig::with_font_size(
        terminal_config.font_size(),
    )
}


#[cfg(target_os = "macos")]
pub(crate) fn current_gpui_terminal_ghostty_surface_config()
-> terminal_ghostty_surface::GhosttySurfaceTerminalConfig {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    gpui_terminal_ghostty_surface_config_from_shared_settings(&settings)
}


#[cfg(target_os = "macos")]
pub(crate) fn reconcile_agents_terminal_startup_host_config_requests<F>(
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_surface_owners: Option<
        &mut HashMap<
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::StartupGhosttySurfaceOwner,
        >,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    startup_launch_payload_source: &AgentsTerminalStartupLaunchPayloadSource,
    terminal_config: terminal_ghostty_surface::GhosttySurfaceTerminalConfig,
    parent_ns_view: *mut std::ffi::c_void,
    create_host_view: F,
) where
    F: FnMut(
        terminal_native_view::TerminalHostNativeViewCreateRequest,
    ) -> Result<
        terminal_native_view::OwnedTerminalHostNativeView,
        terminal_native_view::TerminalHostNativeViewCreateError,
    >,
{
    /*
    CDXC:GPUITerminalStartupNativeHost 2026-06-23-03:23:
    Startup host/config request reconciliation creates hidden host views only from current Mounting launch plans with exact geometry. If render-start clears geometry before the next body canvas records, preserve an already-owned startup host/config only when the current pending record still matches the same runtime id and `AgentsTerminalStartupBodySlotId`; stale pending state, invalid parent/bounds/config, or missing current records must drop the runtime-only state.

    CDXC:GPUITerminalStartupLaunchPayloadSource 2026-06-23-04:00:
    Startup config requests may receive a launch payload only from the runtime-only explicit source for the same launch plan identity. If a future explicit payload fails validation, skip the config request so the hidden startup host/surface is pruned without falling back to terminal titles, status text, project paths, sidebar labels, delayed-send state, or inferred cwd/command/env values.
    */
    terminal_native_view::reconcile_app_owned_terminal_startup_host_native_view(
        startup_host_native_views,
        startup_launch_plans,
        startup_host_preservation_keys,
        parent_ns_view,
        create_host_view,
    );

    let current_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    *startup_config_requests = startup_host_native_views
        .iter()
        .filter_map(|(slot_id, host_view)| {
            let plan = current_launch_plans_by_slot
                .get(slot_id)
                .copied()
                .unwrap_or_else(|| host_view.startup_launch_plan());
            let request =
                terminal_native_view::ghostty_surface_config_request_for_app_owned_terminal_startup_host_native_view(
                    Some(host_view),
                )
                .ok()
                .flatten()?;
            let request = request.with_terminal_config(terminal_config);
            let launch_payload = startup_launch_payload_source
                .payload_for_launch_plan(plan)
                .ok()?;
            let request = if let Some(launch_payload) = launch_payload {
                request.with_launch_payload(launch_payload)
            } else {
                request
            };
            Some((*slot_id, request))
        })
        .collect();
    if let Some(startup_surface_owners) = startup_surface_owners {
        let startup_host_slots_without_config = startup_host_native_views
            .keys()
            .copied()
            .filter(|slot_id| !startup_config_requests.contains_key(slot_id))
            .collect::<Vec<_>>();
        for slot_id in startup_host_slots_without_config {
            startup_surface_owners.remove(&slot_id);
        }
    }
    startup_host_native_views.retain(|slot_id, _| startup_config_requests.contains_key(slot_id));
}


#[cfg(target_os = "macos")]
pub(crate) fn drop_agents_terminal_startup_ghostty_surface_owners_before_host_reconcile(
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_host_native_views: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    parent_ns_view: *mut std::ffi::c_void,
) {
    let startup_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    let startup_host_preservation_keys_by_slot = startup_host_preservation_keys
        .iter()
        .copied()
        .map(|key| (key.startup_body_slot_id, key))
        .collect::<HashMap<_, _>>();
    let stale_surface_slot_ids = startup_surface_owners
        .keys()
        .copied()
        .filter(|slot_id| {
            let Some(host_view) = startup_host_native_views.get(slot_id) else {
                return true;
            };

            !terminal_native_view::app_owned_terminal_startup_host_native_view_will_survive_reconcile(
                host_view,
                startup_launch_plans_by_slot.get(slot_id).copied(),
                startup_host_preservation_keys_by_slot.get(slot_id).copied(),
                parent_ns_view,
            )
        })
        .collect::<Vec<_>>();

    for slot_id in stale_surface_slot_ids {
        startup_surface_owners.remove(&slot_id);
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn reconcile_agents_terminal_startup_ghostty_surface_owners<F>(
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    ghostty_app: &mut Option<terminal_ghostty_surface::GhosttyAppOwner>,
    startup_config_requests: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    mut create_ghostty_app: F,
) where
    F: FnMut() -> Result<
        terminal_ghostty_surface::GhosttyAppOwner,
        terminal_ghostty_surface::GhosttySurfaceRuntimeError,
    >,
{
    /*
    CDXC:GPUITerminalStartupGhosttySurface 2026-06-23-03:33:
    Startup Ghostty surface owners are runtime-only consumers of prepared startup config requests and launch-created geometry. Create only when a matching config request and launch plan exist, preserve same-slot/same-runtime owners across geometry-gap preservation, and drop stale or invalid owners without showing/focusing hosts, applying startup results, logging, persisting, or touching Running mount-slot maps.
    */
    let startup_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    let startup_host_preservation_keys_by_slot = startup_host_preservation_keys
        .iter()
        .copied()
        .map(|key| (key.startup_body_slot_id, key))
        .collect::<HashMap<_, _>>();

    startup_surface_owners.retain(|slot_id, owner| {
        if !startup_config_requests.contains_key(slot_id)
            || owner.startup_body_slot_id() != *slot_id
        {
            return false;
        }

        if let Some(plan) = startup_launch_plans_by_slot.get(slot_id) {
            owner.runtime_session_id() == plan.runtime_session_id
        } else {
            startup_host_preservation_keys_by_slot
                .get(slot_id)
                .is_some_and(|key| owner.runtime_session_id() == key.runtime_session_id)
        }
    });

    for plan in startup_launch_plans {
        let plan = *plan;
        let slot_id = plan.startup_body_slot_id;
        if !startup_launch_plans_by_slot
            .get(&slot_id)
            .is_some_and(|current_plan| *current_plan == plan)
        {
            continue;
        }

        let Some(request) = startup_config_requests.get(&slot_id) else {
            startup_surface_owners.remove(&slot_id);
            continue;
        };
        if terminal_ghostty_surface::GhosttySurfacePixelSize::from_gpui_bounds(
            plan.bounds,
            f64::from(plan.scale_factor),
        )
        .is_err()
        {
            startup_surface_owners.remove(&slot_id);
            continue;
        }

        if startup_surface_owners.get(&slot_id).is_some_and(|owner| {
            owner.startup_body_slot_id() != slot_id
                || owner.runtime_session_id() != plan.runtime_session_id
        }) {
            startup_surface_owners.remove(&slot_id);
        }

        if !startup_surface_owners.contains_key(&slot_id) {
            if ghostty_app.is_none() {
                let Ok(app) = create_ghostty_app() else {
                    startup_surface_owners.clear();
                    return;
                };
                *ghostty_app = Some(app);
            }

            let Some(app) = ghostty_app.as_ref() else {
                return;
            };
            let Ok(surface) = terminal_ghostty_surface::StartupGhosttySurfaceOwner::new(
                app,
                slot_id,
                plan.runtime_session_id,
                request,
            ) else {
                startup_surface_owners.remove(&slot_id);
                continue;
            };
            startup_surface_owners.insert(slot_id, surface);
        }

        let update_failed = startup_surface_owners
            .get_mut(&slot_id)
            .is_some_and(|surface| {
                surface
                    .update_content_scale_and_size(plan.bounds, f64::from(plan.scale_factor))
                    .is_err()
            });
        if update_failed {
            startup_surface_owners.remove(&slot_id);
        }
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_startup_surface_metadata_snapshots(
    startup_surface_owners: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
) -> Vec<(
    AgentsTerminalRuntimeSessionId,
    AgentsTerminalStartupBodySlotId,
    terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
)> {
    /*
    CDXC:GPUITerminalStartupReadiness 2026-06-23-04:13:
    Reading startup surface metadata prepares only a runtime handoff fact for an exact current startup intent. The caller may promote Ready only through the startup-to-Running owner transfer path, so metadata alone still cannot fake Running, create Failed, persist ids, log tty/process facts, or expose raw terminal data.

    CDXC:GPUITerminalStartupRuntimeFailure 2026-06-23-04:38:
    Surface metadata is sampled only from the current startup-owned surface map entry whose key matches the owner's startup body slot. The snapshot carries redacted booleans only, so runtime failure handling can distinguish process-exited from ready metadata without exposing raw pid, tty, command, cwd/path, env, output, terminal content, or runtime ids outside runtime memory.
    */
    startup_surface_owners
        .iter()
        .filter_map(|(startup_body_slot_id, surface)| {
            (surface.startup_body_slot_id() == *startup_body_slot_id).then(|| {
                (
                    surface.runtime_session_id(),
                    *startup_body_slot_id,
                    surface.metadata_snapshot(),
                )
            })
        })
        .collect()
}


#[cfg(target_os = "macos")]
pub(crate) fn failed_agents_terminal_startup_results_from_metadata(
    startup_coordinator: &AgentsTerminalStartupCoordinator,
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    metadata_snapshots: impl IntoIterator<
        Item = (
            AgentsTerminalRuntimeSessionId,
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
        ),
    >,
) -> Vec<AgentsTerminalStartupResult> {
    metadata_snapshots
        .into_iter()
        .filter_map(
            |(runtime_session_id, startup_body_slot_id, surface_metadata)| {
                startup_coordinator.produce_failed_startup_result_from_surface_metadata(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    runtime_session_id,
                    startup_body_slot_id,
                    surface_metadata,
                )
            },
        )
        .collect()
}


#[cfg(target_os = "macos")]
pub(crate) fn sync_agents_terminal_startup_readiness_signal_preparations(
    startup_coordinator: &mut AgentsTerminalStartupCoordinator,
    metadata_snapshots: impl IntoIterator<
        Item = (
            AgentsTerminalRuntimeSessionId,
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
        ),
    >,
) {
    startup_coordinator.sync_startup_readiness_signal_preparations(metadata_snapshots);
}


#[cfg(target_os = "macos")]
pub(crate) fn prune_agents_terminal_startup_runtime_state_for_completion_intent(
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_launch_payload_source: &mut AgentsTerminalStartupLaunchPayloadSource,
    completion_intent: AgentsTerminalStartupCompletionIntent,
) {
    /*
    CDXC:GPUITerminalStartupRuntimeFailure 2026-06-23-04:38:
    Failed startup cleanup retires startup-only runtime state for the exact completion intent. Remove the startup Ghostty surface before its hidden AppKit host, remove prepared config/geometry/payload state, and leave the shell session as the retryable StartupFailed placeholder without creating Running ownership.
    */
    startup_body_geometries.remove(&completion_intent.startup_body_slot_id);
    startup_surface_owners.remove(&completion_intent.startup_body_slot_id);
    startup_config_requests.remove(&completion_intent.startup_body_slot_id);
    startup_host_native_views.remove(&completion_intent.startup_body_slot_id);
    startup_launch_payload_source.remove_payload_for_completion_intent(completion_intent);
}


#[cfg(not(target_os = "macos"))]
pub(crate) fn prune_agents_terminal_startup_runtime_state_for_completion_intent(
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_launch_payload_source: &mut AgentsTerminalStartupLaunchPayloadSource,
    completion_intent: AgentsTerminalStartupCompletionIntent,
) {
    startup_body_geometries.remove(&completion_intent.startup_body_slot_id);
    startup_launch_payload_source.remove_payload_for_completion_intent(completion_intent);
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_attachment_plan_for_startup_handoff(
    handoff_plan: AgentsTerminalStartupReadinessHandoffPlan,
) -> terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
    terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
        host_id: terminal_surface_host::NativeTerminalSurfaceHostId::from_slot_id(
            handoff_plan.mount_slot_id,
        ),
        slot_id: handoff_plan.mount_slot_id,
        bounds: handoff_plan.startup_launch_plan.bounds,
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn transfer_ready_agents_terminal_startup_handoff(
    startup_coordinator: &mut AgentsTerminalStartupCoordinator,
    workspace: &mut WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    running_mount_slot_bounds: &mut HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    running_config_requests: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    handoff_plan: AgentsTerminalStartupReadinessHandoffPlan,
) -> bool {
    /*
    CDXC:GPUITerminalStartupHandoff 2026-06-23-04:25:
    Ready startup promotion is a single ownership move, not a new launch. Require exact current startup readiness plus empty target Running owner maps, remove the startup host/surface only into local ownership, promote the same shell session to Running, and then insert the same AppKit host and Ghostty surface under the resulting `AgentsTerminalBodyMountSlotId`.
    */
    if startup_coordinator.startup_readiness_handoff_plan_for_runtime_session(
        true,
        workspace,
        runtime_sessions,
        handoff_plan.runtime_session_id(),
    ) != Some(handoff_plan)
    {
        return false;
    }

    let startup_body_slot_id = handoff_plan.startup_body_slot_id();
    let mount_slot_id = handoff_plan.mount_slot_id;
    let attachment_plan = agents_terminal_attachment_plan_for_startup_handoff(handoff_plan);
    if running_host_native_views.contains_key(&mount_slot_id)
        || running_surface_owners.contains_key(&mount_slot_id)
        || running_config_requests.contains_key(&mount_slot_id)
    {
        return false;
    }

    if !startup_host_native_views
        .get(&startup_body_slot_id)
        .is_some_and(|host_view| {
            host_view.startup_launch_plan() == handoff_plan.startup_launch_plan
                && host_view.can_transfer_to_running_attachment_plan(attachment_plan)
        })
    {
        return false;
    }
    if !startup_surface_owners
        .get(&startup_body_slot_id)
        .is_some_and(|surface| {
            surface.startup_body_slot_id() == startup_body_slot_id
                && surface.runtime_session_id() == handoff_plan.runtime_session_id()
        })
    {
        return false;
    }

    let Some(startup_host_view) = startup_host_native_views.remove(&startup_body_slot_id) else {
        return false;
    };
    let Some(startup_surface_owner) = startup_surface_owners.remove(&startup_body_slot_id) else {
        startup_host_native_views.insert(startup_body_slot_id, startup_host_view);
        return false;
    };

    let changed = startup_coordinator.apply_startup_result(
        workspace,
        runtime_sessions,
        AgentsTerminalStartupResult::Ready {
            completion_intent: handoff_plan.completion_intent,
        },
    );
    if !changed {
        startup_surface_owners.insert(startup_body_slot_id, startup_surface_owner);
        startup_host_native_views.insert(startup_body_slot_id, startup_host_view);
        return false;
    }

    running_mount_slot_bounds.insert(mount_slot_id, handoff_plan.startup_launch_plan.bounds);
    running_host_native_views.insert(
        mount_slot_id,
        startup_host_view.into_running_host_native_view(attachment_plan),
    );
    running_surface_owners.insert(
        mount_slot_id,
        startup_surface_owner.into_running_surface_owner(mount_slot_id),
    );
    startup_config_requests.remove(&startup_body_slot_id);
    startup_body_geometries.remove(&startup_body_slot_id);
    true
}


pub(crate) fn activate_agents_terminal_placeholder_with_runtime_attempt_identity(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    pane_id: WorkspacePaneId,
    session_id: TerminalSessionId,
) -> bool {
    /*
    CDXC:GPUITerminalStartupRetryIdentity 2026-06-23-18:19:
    Placeholder activation may change durable shell presentation, but retry attempt identity is process-local app/runtime state. Detect the explicit `StartupFailed` edge before shell activation, then rotate the runtime id only after that same shell session becomes startup-eligible `Mounting` so wake/materialize/reattach placeholders keep their existing runtime identity and cannot enter a retry startup attempt.

    CDXC:GPUITerminalRestoredMaterialization 2026-06-23-19:26:
    Restored-unmounted materialization now enters the startup pipeline, but it is not a retry. Keep the process-local runtime id already associated with the durable shell session; sleeping wake and popped-out reattach remain blocked from startup maps and use the separate slice 236 parked-owner contract.
    */
    let retry_activation = workspace.session(session_id).is_some_and(|session| {
        session.presentation_state == TerminalSessionPresentationState::StartupFailed
    });
    let model_changed = workspace.activate_terminal_placeholder_session(pane_id, session_id);

    if retry_activation
        && workspace
            .session(session_id)
            .is_some_and(TerminalSession::can_enter_startup_pipeline)
    {
        runtime_sessions.rotate_runtime_session_id_for_shell_session(session_id);
    }

    model_changed
}
