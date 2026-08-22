// C1 wave-3 extraction: the terminal launch-payload and close-confirm value types moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalStartupLaunchPayloadSourceKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) startup_body_slot_id: AgentsTerminalStartupBodySlotId,
}


impl AgentsTerminalStartupLaunchPayloadSourceKey {
    pub(crate) fn from_launch_plan(plan: AgentsTerminalStartupLaunchPlan) -> Self {
        Self {
            runtime_session_id: plan.runtime_session_id,
            shell_session_id: plan.shell_session_id,
            startup_body_slot_id: plan.startup_body_slot_id,
        }
    }
}


#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AgentsTerminalStartupExplicitLaunchPayload {
    pub(crate) working_directory: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) env_vars: Vec<(String, String)>,
    pub(crate) initial_input: Option<String>,
    pub(crate) wait_after_command: bool,
}


impl AgentsTerminalStartupExplicitLaunchPayload {
    pub(crate) fn to_ghostty_launch_payload(
        &self,
    ) -> Result<
        terminal_ghostty_surface::GhosttySurfaceLaunchPayload,
        terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
    > {
        terminal_ghostty_surface::GhosttySurfaceLaunchPayload::try_new(
            self.working_directory.clone(),
            self.command.clone(),
            self.env_vars.clone(),
            self.initial_input.clone(),
            self.wait_after_command,
        )
    }
}


#[derive(Default)]
pub(crate) struct AgentsTerminalStartupLaunchPayloadSource {
    pub(crate) explicit_payloads_by_startup_key: HashMap<
        AgentsTerminalStartupLaunchPayloadSourceKey,
        AgentsTerminalStartupExplicitLaunchPayload,
    >,
}


impl AgentsTerminalStartupLaunchPayloadSource {
    pub(crate) fn new_empty() -> Self {
        Self::default()
    }

    pub(crate) fn insert_explicit_payload_for_startup_key(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        shell_session_id: TerminalSessionId,
        startup_body_slot_id: AgentsTerminalStartupBodySlotId,
        payload: AgentsTerminalStartupExplicitLaunchPayload,
    ) {
        self.explicit_payloads_by_startup_key.insert(
            AgentsTerminalStartupLaunchPayloadSourceKey {
                runtime_session_id,
                shell_session_id,
                startup_body_slot_id,
            },
            payload,
        );
    }

    #[allow(dead_code)] // no live caller: the app-owned terminal startup-host reconcile pipeline is not driven any more (agents terminals mount through the surface-host path)
    pub(crate) fn payload_for_launch_plan(
        &self,
        plan: AgentsTerminalStartupLaunchPlan,
    ) -> Result<
        Option<terminal_ghostty_surface::GhosttySurfaceLaunchPayload>,
        terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
    > {
        /*
        CDXC:GPUITerminalStartupLaunchPayloadSource 2026-06-23-04:00:
        GPUI startup launch payloads may enter only through this exact runtime/session/startup-slot key and must be validated before a startup config request receives a Ghostty launch payload; terminal titles, status labels, project names, workspace paths, sidebar labels, delayed-send flags, and fallback project detection are never parsed into launch values.

        CDXC:GPUIRemoteAttach 2026-06-24-19:06:
        Remote attach is now a production explicit launch source, but the payload is inserted by Rust after resolving a saved remote machine and gxserver attach metadata. The source map remains process-local and must not persist or derive commands from renderer text, project/session titles, paths, tokens, stdout/stderr, or terminal content.
        */
        self.explicit_payloads_by_startup_key
            .get(&AgentsTerminalStartupLaunchPayloadSourceKey::from_launch_plan(plan))
            .map(AgentsTerminalStartupExplicitLaunchPayload::to_ghostty_launch_payload)
            .transpose()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn remove_payload_for_launch_plan(&mut self, plan: AgentsTerminalStartupLaunchPlan) {
        self.explicit_payloads_by_startup_key
            .remove(&AgentsTerminalStartupLaunchPayloadSourceKey::from_launch_plan(plan));
    }

    /// Raw payload read for the GPUI-engine startup path, which spawns its
    /// own PTY from the same launch data instead of preparing a Ghostty
    /// surface config. Cleanup stays with
    /// `remove_payload_for_completion_intent` after the startup result is
    /// applied.
    pub(crate) fn explicit_payload_for_launch_plan(
        &self,
        plan: AgentsTerminalStartupLaunchPlan,
    ) -> Option<&AgentsTerminalStartupExplicitLaunchPayload> {
        self.explicit_payloads_by_startup_key
            .get(&AgentsTerminalStartupLaunchPayloadSourceKey::from_launch_plan(plan))
    }

    pub(crate) fn remove_payload_for_completion_intent(
        &mut self,
        completion_intent: AgentsTerminalStartupCompletionIntent,
    ) {
        /*
        CDXC:GPUITerminalStartupRuntimeFailure 2026-06-23-04:46:
        Failed or stale startup completion must retire exact explicit launch payload data on every platform. The real metadata producer is macOS-only today, but the shared Failed-result boundary must not leave cwd, command, env, or initial-input payloads in runtime memory after the matching startup intent is complete.
        */
        self.explicit_payloads_by_startup_key.remove(
            &AgentsTerminalStartupLaunchPayloadSourceKey {
                runtime_session_id: completion_intent.runtime_session_id,
                shell_session_id: completion_intent.shell_session_id,
                startup_body_slot_id: completion_intent.startup_body_slot_id,
            },
        );
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalLaunchPayloadSourceKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) body_mount_slot_id: AgentsTerminalBodyMountSlotId,
}


#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AgentsTerminalExplicitLaunchPayload {
    pub(crate) working_directory: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) env_vars: Vec<(String, String)>,
    pub(crate) initial_input: Option<String>,
    pub(crate) wait_after_command: bool,
}


impl AgentsTerminalExplicitLaunchPayload {
    pub(crate) fn to_ghostty_launch_payload(
        &self,
    ) -> Result<
        terminal_ghostty_surface::GhosttySurfaceLaunchPayload,
        terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
    > {
        terminal_ghostty_surface::GhosttySurfaceLaunchPayload::try_new(
            self.working_directory.clone(),
            self.command.clone(),
            self.env_vars.clone(),
            self.initial_input.clone(),
            self.wait_after_command,
        )
    }
}


#[derive(Default)]
pub(crate) struct AgentsTerminalLaunchPayloadSource {
    pub(crate) explicit_payloads_by_agents_key:
        HashMap<AgentsTerminalLaunchPayloadSourceKey, AgentsTerminalExplicitLaunchPayload>,
}


impl AgentsTerminalLaunchPayloadSource {
    pub(crate) fn new_empty() -> Self {
        Self::default()
    }

    pub(crate) fn insert_explicit_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: AgentsTerminalBodyMountSlotId,
        payload: AgentsTerminalExplicitLaunchPayload,
    ) {
        /*
        CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:25:
        Sidebar-attached local gxserver sessions start awake like macOS by feeding the daemon-built attach command directly to the exact Running Agents mount slot. Store it only under the process-local runtime id plus body slot, consume it once during Ghostty config, and never derive launch data from titles, paths, renderer labels, logs, terminal content, or fallback focus.
        */
        self.explicit_payloads_by_agents_key.insert(
            AgentsTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            },
            payload,
        );
    }

    pub(crate) fn take_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> Result<
        Option<terminal_ghostty_surface::GhosttySurfaceLaunchPayload>,
        terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
    > {
        self.explicit_payloads_by_agents_key
            .remove(&AgentsTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            })
            .map(|payload| payload.to_ghostty_launch_payload())
            .transpose()
    }

    pub(crate) fn has_payload_for_mount_slot(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.explicit_payloads_by_agents_key
            .contains_key(&AgentsTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            })
    }

    /// One-shot drain of the raw payload for the GPUI-engine terminal path,
    /// which spawns its own PTY process from the same launch data instead of
    /// preparing a Ghostty surface config.
    pub(crate) fn take_explicit_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalExplicitLaunchPayload> {
        self.explicit_payloads_by_agents_key
            .remove(&AgentsTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            })
    }

    pub(crate) fn retain_live_mount_slots(
        &mut self,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) {
        self.explicit_payloads_by_agents_key.retain(|key, _| {
            workspace.session_belongs_to_pane(
                key.body_mount_slot_id.pane_id,
                key.body_mount_slot_id.session_id,
            ) && runtime_sessions
                .runtime_session_id_for_shell_session(key.body_mount_slot_id.session_id)
                == Some(key.runtime_session_id)
        });
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ProjectEditorCompanionTerminalLaunchPayloadSourceKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) body_mount_slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
}


/*
CDXC:GPUIProjectEditorCompanionAttach 2026-07-06:
The project-editor companion pane displays an existing zmx-backed workspace
session by attaching its own zmx client, mirroring how mobile clients mirror a
session; it must never mount a default shell for a slot without a daemon-built
attach payload. Payloads are process-local, keyed by runtime id plus companion
body slot, consumed once at Ghostty config time, and never derived from titles,
paths, renderer labels, logs, terminal content, or inferred commands.
*/
#[derive(Default)]
pub(crate) struct ProjectEditorCompanionTerminalLaunchPayloadSource {
    pub(crate) explicit_payloads_by_companion_key: HashMap<
        ProjectEditorCompanionTerminalLaunchPayloadSourceKey,
        AgentsTerminalExplicitLaunchPayload,
    >,
}


impl ProjectEditorCompanionTerminalLaunchPayloadSource {
    pub(crate) fn new_empty() -> Self {
        Self::default()
    }

    pub(crate) fn insert_explicit_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        payload: AgentsTerminalExplicitLaunchPayload,
    ) {
        self.explicit_payloads_by_companion_key.insert(
            ProjectEditorCompanionTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            },
            payload,
        );
    }

    /// One-shot drain of the raw attach payload for the GPUI-composited
    /// terminal path. The same validated payload boundary feeds both retained
    /// renderer implementations without deriving launch data from UI state.
    pub(crate) fn take_explicit_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalExplicitLaunchPayload> {
        self.explicit_payloads_by_companion_key.remove(
            &ProjectEditorCompanionTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            },
        )
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn take_payload_for_mount_slot(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> Result<
        Option<terminal_ghostty_surface::GhosttySurfaceLaunchPayload>,
        terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
    > {
        self.explicit_payloads_by_companion_key
            .remove(&ProjectEditorCompanionTerminalLaunchPayloadSourceKey {
                runtime_session_id,
                body_mount_slot_id: slot_id,
            })
            .map(|payload| payload.to_ghostty_launch_payload())
            .transpose()
    }

    pub(crate) fn retain_current_mount_slots(
        &mut self,
        current_slot_ids: &[ProjectEditorCompanionTerminalBodyMountSlotId],
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    ) {
        self.explicit_payloads_by_companion_key.retain(|key, _| {
            current_slot_ids.contains(&key.body_mount_slot_id)
                && runtime_sessions
                    .runtime_session_id_for_shell_session(key.body_mount_slot_id.session_id)
                    == Some(key.runtime_session_id)
        });
    }
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CommandTerminalLaunchPayloadSourceKey {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) body_mount_slot_id: CommandTerminalBodyMountSlotId,
}


impl CommandTerminalLaunchPayloadSourceKey {
    pub(crate) fn from_mount_slot(slot_id: CommandTerminalBodyMountSlotId) -> Self {
        Self {
            runtime_session_id: command_terminal_runtime_session_id(slot_id),
            body_mount_slot_id: slot_id,
        }
    }
}


#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CommandTerminalExplicitLaunchPayload {
    pub(crate) working_directory: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) env_vars: Vec<(String, String)>,
    pub(crate) initial_input: Option<String>,
    pub(crate) wait_after_command: bool,
}


impl CommandTerminalExplicitLaunchPayload {
    pub(crate) fn to_ghostty_launch_payload(
        &self,
    ) -> Result<
        terminal_ghostty_surface::GhosttySurfaceLaunchPayload,
        terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
    > {
        terminal_ghostty_surface::GhosttySurfaceLaunchPayload::try_new(
            self.working_directory.clone(),
            self.command.clone(),
            self.env_vars.clone(),
            self.initial_input.clone(),
            self.wait_after_command,
        )
    }
}


#[derive(Default)]
pub(crate) struct CommandTerminalLaunchPayloadSource {
    pub(crate) explicit_payloads_by_command_key:
        HashMap<CommandTerminalLaunchPayloadSourceKey, CommandTerminalExplicitLaunchPayload>,
}


impl CommandTerminalLaunchPayloadSource {
    pub(crate) fn new_empty() -> Self {
        Self::default()
    }

    pub(crate) fn insert_explicit_payload_for_mount_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        payload: CommandTerminalExplicitLaunchPayload,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Titlebar terminal Actions are allowed to feed command text only through the command-terminal launch-payload boundary for the exact command-pane mount slot they create. Keep the payload process-local and keyed by command runtime identity plus body slot; do not persist it, log it, infer it from labels/paths, or run it from the titlebar handler.
        */
        self.explicit_payloads_by_command_key.insert(
            CommandTerminalLaunchPayloadSourceKey::from_mount_slot(slot_id),
            payload,
        );
    }

    pub(crate) fn take_payload_for_mount_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> Result<
        Option<terminal_ghostty_surface::GhosttySurfaceLaunchPayload>,
        terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
    > {
        /*
        CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-27-04:59:
        Command terminal launch data now has explicit current producers for titlebar/command-palette terminal Actions and plain command-terminal project cwd. Payloads remain exact-slot, one-shot, process-local startup inputs keyed by command runtime identity plus body slot; never parse or infer launch cwd, command, env, initial input, or wait policy from shell state, command titles, project names, display paths, logs, stdout/stderr, terminal content, delayed-send state, fallbacks, or helper detection.

        CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-27-04:47:
        Explicit command launch payloads are one-shot startup inputs. Remove the exact slot/runtime key before conversion so Action/plain command startup payloads cannot be reattached by a later remount; invalid payloads are also consumed and pruned without fallback.
        */
        self.explicit_payloads_by_command_key
            .remove(&CommandTerminalLaunchPayloadSourceKey::from_mount_slot(
                slot_id,
            ))
            .map(|payload| payload.to_ghostty_launch_payload())
            .transpose()
    }

    /// One-shot drain of the raw payload for the GPUI-engine terminal path.
    pub(crate) fn take_explicit_payload_for_mount_slot(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> Option<CommandTerminalExplicitLaunchPayload> {
        self.explicit_payloads_by_command_key.remove(
            &CommandTerminalLaunchPayloadSourceKey::from_mount_slot(slot_id),
        )
    }

    pub(crate) fn remove_payloads_for_command_session(&mut self, session_id: CommandSessionId) {
        self.explicit_payloads_by_command_key
            .retain(|key, _| key.body_mount_slot_id.session_id != session_id);
    }

    pub(crate) fn remove_all_payloads(&mut self) {
        self.explicit_payloads_by_command_key.clear();
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalCloseConfirmSurfaceFamily {
    Agents,
    Command,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalCloseConfirmSurfaceSignature {
    pub(crate) title: &'static str,
    pub(crate) message: &'static str,
    pub(crate) keep_open_label: &'static str,
    pub(crate) confirm_action_label: &'static str,
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalCloseConfirmDialogKey {
    Agents(AgentsTerminalBodyMountSlotId),
    Command(CommandTerminalBodyMountSlotId),
}


#[cfg(target_os = "macos")]
impl TerminalCloseConfirmDialogKey {
    pub(crate) fn family(self) -> TerminalCloseConfirmSurfaceFamily {
        match self {
            Self::Agents(_) => TerminalCloseConfirmSurfaceFamily::Agents,
            Self::Command(_) => TerminalCloseConfirmSurfaceFamily::Command,
        }
    }
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingAgentsTerminalCloseConfirm {
    pub(crate) slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
}


#[cfg(target_os = "macos")]
#[derive(Default)]
pub(crate) struct AgentsTerminalCloseConfirmState {
    pub(crate) pending_by_slot: HashMap<AgentsTerminalBodyMountSlotId, PendingAgentsTerminalCloseConfirm>,
}


#[cfg(target_os = "macos")]
impl AgentsTerminalCloseConfirmState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sync_from_confirmation_needed_callbacks(
        &mut self,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        running_surface_owners: &HashMap<
            AgentsTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner,
        >,
    ) -> bool {
        /*
        CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
        Mounted Running Agents close-confirm state is runtime-only and keyed by the exact current body mount slot plus process-local surface owner identity. Confirmation-needed callbacks move into this map once, while stale, inactive, mismatched, startup, command, shell JSON, launch payload, log, path, command text, stdout/stderr, tty, pid, token, and terminal content data stay out of the boundary. Final-root confirmations are allowed because macOS-style close can leave an empty workspace pane after the user confirms.
        */
        let mut changed = self.prune_stale(workspace, runtime_sessions, running_surface_owners);
        for slot_id in workspace.rendered_terminal_body_mount_slots() {
            let Some(pending) = pending_agents_terminal_close_confirm_for_slot(
                workspace,
                runtime_sessions,
                running_surface_owners,
                slot_id,
            ) else {
                continue;
            };
            let Some(surface) = running_surface_owners.get(&slot_id) else {
                continue;
            };
            if surface.consume_confirmation_needed_close_requested()
                && self.pending_by_slot.insert(slot_id, pending) != Some(pending)
            {
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn confirm(
        &mut self,
        workspace: &mut WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        running_surface_owners: &HashMap<
            AgentsTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner,
        >,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        let Some(pending) = self.pending_by_slot.get(&slot_id).copied() else {
            return false;
        };
        /*
        CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
        Confirming an Agents close now follows upstream Ghostty's source contract: after an exact pending/current/runtime/surface match and a true `needs_confirm_quit` query, remove only that shell tab through `WorkspaceModel::close_tab` and clear only the matching prompt. Do not request another close, synthesize callbacks, touch startup/command state, or use fallback broad removal.
        */
        let Some(current) = pending_agents_terminal_close_confirm_for_slot(
            workspace,
            runtime_sessions,
            running_surface_owners,
            slot_id,
        ) else {
            self.pending_by_slot.remove(&slot_id);
            return false;
        };
        if pending != current {
            self.pending_by_slot.remove(&slot_id);
            return false;
        }

        if workspace.close_tab(slot_id.pane_id, slot_id.session_id) {
            self.pending_by_slot.remove(&slot_id);
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel(
        &mut self,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        running_surface_owners: &mut HashMap<
            AgentsTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner,
        >,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        let Some(pending) = self.pending_by_slot.get(&slot_id).copied() else {
            return false;
        };
        let current = pending_agents_terminal_close_confirm_for_slot(
            workspace,
            runtime_sessions,
            running_surface_owners,
            slot_id,
        );
        if current != Some(pending) {
            self.pending_by_slot.remove(&slot_id);
            return false;
        }

        let Some(surface) = running_surface_owners.get_mut(&slot_id) else {
            self.pending_by_slot.remove(&slot_id);
            return false;
        };
        if !surface.cancel_pending_close_request() {
            return false;
        }
        self.pending_by_slot.remove(&slot_id);
        true
    }

    pub(crate) fn prune_stale(
        &mut self,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        running_surface_owners: &HashMap<
            AgentsTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner,
        >,
    ) -> bool {
        let before = self.pending_by_slot.len();
        self.pending_by_slot.retain(|slot_id, pending| {
            pending_agents_terminal_close_confirm_for_slot(
                workspace,
                runtime_sessions,
                running_surface_owners,
                *slot_id,
            ) == Some(*pending)
        });
        before != self.pending_by_slot.len()
    }

    pub(crate) fn exact_current_pending_slot(
        &self,
        workspace: &WorkspaceModel,
        runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
        running_surface_owners: &HashMap<
            AgentsTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner,
        >,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalBodyMountSlotId> {
        let pending = self.pending_by_slot.get(&slot_id).copied()?;
        let current = pending_agents_terminal_close_confirm_for_slot(
            workspace,
            runtime_sessions,
            running_surface_owners,
            slot_id,
        )?;
        (pending == current).then_some(slot_id)
    }
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingCommandTerminalCloseConfirm {
    pub(crate) slot_id: CommandTerminalBodyMountSlotId,
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
}


#[cfg(target_os = "macos")]
#[derive(Default)]
pub(crate) struct CommandTerminalCloseConfirmState {
    pub(crate) pending_by_slot: HashMap<CommandTerminalBodyMountSlotId, PendingCommandTerminalCloseConfirm>,
}


#[cfg(target_os = "macos")]
impl CommandTerminalCloseConfirmState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sync_from_confirmation_needed_callbacks(
        &mut self,
        command_pane: &CommandPaneModel,
        command_surface_owners: &HashMap<
            CommandTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
        >,
    ) -> bool {
        /*
        CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
        Mounted command close-confirm state is command-pane-only runtime state keyed by the command body mount slot and its transient surface owner identity. It must never touch Agents workspace maps, Agents runtime sessions, startup maps, shell JSON, launch payloads, logs, command text, paths, env, stdout/stderr, tty, pid, tokens, or terminal content.
        */
        let mut changed = self.prune_stale(command_pane, command_surface_owners);
        for slot_id in command_pane.rendered_terminal_body_mount_slots() {
            let Some(pending) = pending_command_terminal_close_confirm_for_slot(
                command_pane,
                command_surface_owners,
                slot_id,
            ) else {
                continue;
            };
            let Some(surface) = command_surface_owners.get(&slot_id) else {
                continue;
            };
            if surface.consume_confirmation_needed_close_requested()
                && self.pending_by_slot.insert(slot_id, pending) != Some(pending)
            {
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn confirm(
        &mut self,
        command_pane: &mut CommandPaneModel,
        command_surface_owners: &HashMap<
            CommandTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
        >,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        let Some(pending) = self.pending_by_slot.get(&slot_id).copied() else {
            return false;
        };
        /*
        CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
        Confirming a command close validates the exact pending command slot, transient runtime identity, mounted surface owner, and `needs_confirm_quit` boolean before closing through `CommandPaneModel::close_session`. The command prompt clear is local to that slot and never routes through Agents/startup state or runtime callback synthesis.
        */
        let Some(current) = pending_command_terminal_close_confirm_for_slot(
            command_pane,
            command_surface_owners,
            slot_id,
        ) else {
            self.pending_by_slot.remove(&slot_id);
            return false;
        };
        if pending != current {
            self.pending_by_slot.remove(&slot_id);
            return false;
        }

        if command_pane.close_session(slot_id.group_id, slot_id.session_id) {
            self.pending_by_slot.remove(&slot_id);
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel(
        &mut self,
        command_pane: &CommandPaneModel,
        command_surface_owners: &mut HashMap<
            CommandTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
        >,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        let Some(pending) = self.pending_by_slot.get(&slot_id).copied() else {
            return false;
        };
        let current = pending_command_terminal_close_confirm_for_slot(
            command_pane,
            command_surface_owners,
            slot_id,
        );
        if current != Some(pending) {
            self.pending_by_slot.remove(&slot_id);
            return false;
        }

        let Some(surface) = command_surface_owners.get_mut(&slot_id) else {
            self.pending_by_slot.remove(&slot_id);
            return false;
        };
        if !surface.cancel_pending_close_request() {
            return false;
        }
        self.pending_by_slot.remove(&slot_id);
        true
    }

    pub(crate) fn prune_stale(
        &mut self,
        command_pane: &CommandPaneModel,
        command_surface_owners: &HashMap<
            CommandTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
        >,
    ) -> bool {
        let before = self.pending_by_slot.len();
        self.pending_by_slot.retain(|slot_id, pending| {
            pending_command_terminal_close_confirm_for_slot(
                command_pane,
                command_surface_owners,
                *slot_id,
            ) == Some(*pending)
        });
        before != self.pending_by_slot.len()
    }

    pub(crate) fn exact_current_pending_slot(
        &self,
        command_pane: &CommandPaneModel,
        command_surface_owners: &HashMap<
            CommandTerminalBodyMountSlotId,
            terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
        >,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> Option<CommandTerminalBodyMountSlotId> {
        let pending = self.pending_by_slot.get(&slot_id).copied()?;
        let current = pending_command_terminal_close_confirm_for_slot(
            command_pane,
            command_surface_owners,
            slot_id,
        )?;
        (pending == current).then_some(slot_id)
    }
}

