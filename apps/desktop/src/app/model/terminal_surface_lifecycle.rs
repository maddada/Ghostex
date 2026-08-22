// C1 wave-3 re-cluster: terminal Ghostty surface close/exit confirmation and focused mount-slot/runtime-session-id lookups, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).
#![allow(dead_code)]

use crate::*;


#[cfg(target_os = "macos")]
pub(crate) fn confirmed_agents_terminal_ghostty_surface_close_slots(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) -> Vec<AgentsTerminalBodyMountSlotId> {
    /*
    CDXC:GPUITerminalGhosttyClose 2026-06-23-04:49:
    Confirmed Ghostty close callbacks are consumed only for exact current Running Agents mount slots. Return slot ids without mutating shell state so the app can route mapped gxserver tabs through sidebar-owned lifecycle while unmapped tabs still close through `WorkspaceModel::close_tab`.

    CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
    Confirmation-needed callbacks are consumed into the Agents pending close-confirm map before this confirmed-close path runs. This path may remove shell state only when the current mounted owner still matches the slot and runtime identity, and it clears only the matching pending entry after the existing close callback is consumed.
    */
    let mut confirmed_slots = Vec::new();
    for slot_id in workspace.rendered_terminal_body_mount_slots() {
        if !workspace.is_current_terminal_body_mount_slot(slot_id)
            || !workspace.can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            continue;
        }
        let confirmed = running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)
                    == Some(surface.runtime_session_id())
                && surface.consume_confirmed_close_requested()
        });
        if confirmed {
            confirmed_slots.push(slot_id);
        }
    }
    confirmed_slots
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_confirmed_agents_terminal_ghostty_surface_closes(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    close_confirms: &mut AgentsTerminalCloseConfirmState,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) -> bool {
    let mut changed = false;
    for slot_id in confirmed_agents_terminal_ghostty_surface_close_slots(
        workspace,
        runtime_sessions,
        running_surface_owners,
    ) {
        if workspace.close_tab(slot_id.pane_id, slot_id.session_id) {
            close_confirms.pending_by_slot.remove(&slot_id);
            changed = true;
        }
    }
    changed
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_exited_agents_terminal_ghostty_surfaces(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) -> bool {
    /*
    CDXC:GPUITerminalProcessExit 2026-06-23-05:30:
    Mounted Running Agents process-exit cleanup mirrors native policy by deleting the shell session through `WorkspaceModel::close_tab` without asking Ghostty to close again. Eligibility is limited to exact current Running mount slots with matching process-local runtime ids, and startup maps remain outside this helper so Mounting/Failed startup state cannot be changed by Running process polling.
    */
    let mut changed = false;
    for slot_id in workspace.rendered_terminal_body_mount_slots() {
        if !workspace.is_current_terminal_body_mount_slot(slot_id)
            || !workspace.can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            continue;
        }

        let process_exited = running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)
                    == Some(surface.runtime_session_id())
                && surface.process_exited()
        });
        if process_exited && workspace.close_tab(slot_id.pane_id, slot_id.session_id) {
            changed = true;
        }
    }

    if changed {
        runtime_sessions.reconcile_with_workspace(workspace);
    }
    changed
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_confirmed_command_terminal_ghostty_surface_closes(
    command_pane: &mut CommandPaneModel,
    close_confirms: &mut CommandTerminalCloseConfirmState,
    command_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
) -> bool {
    /*
    CDXC:GPUICommandTerminalGhosttyClose 2026-06-23-05:21:
    Confirmed command Ghostty close callbacks are consumed before command host reconciliation and may remove only the exact current command mount slot through `CommandPaneModel::close_session`. Confirmation-needed callbacks stay in command-only pending runtime state for the normal-layout command close-confirm surface, and this path must not touch Agents runtime maps, startup maps, shell-state JSON, logs, runtime ids, commands, paths, env, output, pids, tty names, or terminal content.

    CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
    Confirmed command closes clear only the matching command pending close-confirm entry after the existing callback path removes shell state. They must not infer confirmation from the pending map or remove command sessions from confirm/cancel actions directly.
    */
    let mut changed = false;
    for slot_id in command_pane.rendered_terminal_body_mount_slots() {
        if !command_pane.is_current_terminal_body_mount_slot(slot_id) {
            continue;
        }
        let confirmed = command_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && surface.runtime_session_id() == command_terminal_runtime_session_id(slot_id)
                && surface.consume_confirmed_close_requested()
        });
        if confirmed && command_pane.close_session(slot_id.group_id, slot_id.session_id) {
            close_confirms.pending_by_slot.remove(&slot_id);
            changed = true;
        }
    }
    changed
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_exited_command_terminal_ghostty_surfaces(
    command_pane: &mut CommandPaneModel,
    command_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
) -> CommandTerminalProcessExitCleanup {
    /*
    CDXC:GPUICommandTerminalProcessExit 2026-06-23-05:30:
    Mounted command process-exit cleanup is command-pane-only and removes only exact current command body slots through `CommandPaneModel::close_session`. Already-exited surfaces must not receive a Ghostty close request, and this helper cannot touch Agents workspace/runtime/startup maps or command close-confirm callback state.

    CDXC:GPUICommandPane 2026-06-25-11:11:
    Mapped Action sessions that disappear through process-exit cleanup must also finish sidebar button feedback before the command tab is removed. The completion record is derived only from live command ownership plus the matching status-file stamp when available; missing or non-idle status stamps become error feedback so the reused SidebarApp cannot keep an orphaned running state.

    CDXC:GPUICommandTerminalProcessExit 2026-06-26-06:28:
    Native `handleNativeSidebarCommandSessionExit` and `cleanupExitedNativeCommandPaneSession` parity requires removing the exact exited Action command tab, then letting the caller's existing command-model prune paths clear stale Delayed Send and Close After Done runtime intents for that tab. Completion records may carry only command id, run id, completed tab ids, exit code, and sound preference; a matching idle status file supplies the exit code, while missing, working, or mismatched status files report error.
    */
    let mut cleanup = CommandTerminalProcessExitCleanup::default();
    for slot_id in command_pane.rendered_terminal_body_mount_slots() {
        if !command_pane.is_current_terminal_body_mount_slot(slot_id) {
            continue;
        }

        let process_exited = command_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && surface.runtime_session_id() == command_terminal_runtime_session_id(slot_id)
                && surface.process_exited()
        });
        if process_exited {
            let completion = command_pane.take_action_run_completion_for_exited_session(
                slot_id.group_id,
                slot_id.session_id,
            );
            if command_pane.close_session(slot_id.group_id, slot_id.session_id) {
                cleanup.changed = true;
                if let Some(completion) = completion {
                    cleanup.completions.push(completion);
                }
            }
        }
    }
    cleanup
}


pub(crate) fn focused_agents_terminal_surface_mount_slot(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
) -> Option<AgentsTerminalBodyMountSlotId> {
    /*
    CDXC:GPUTerminalGhosttySurfaceFocus 2026-06-22-22:59:
    Real Agents Ghostty surface focus is a runtime decision derived from shell focus only: Agents mode, an AgentsPane focus target, a rendered pane, and that pane's selected Running session. Command pane, Browser/project-editor modes, sleeping or hidden Focus-mode panes, stale panes, and missing sessions must leave every mounted terminal surface unfocused without adding input routing or persisted focus ids.
    */
    if active_mode != TitlebarMode::Agents {
        return None;
    }
    let ShellFocusTarget::AgentsPane(focused_pane_id) = shell_focus else {
        return None;
    };

    agents_workspace
        .rendered_terminal_body_mount_slots()
        .into_iter()
        .find(|slot_id| slot_id.pane_id == focused_pane_id)
}


pub(crate) fn focused_project_editor_companion_terminal_surface_mount_slot(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    selected_session_id: Option<TerminalSessionId>,
) -> Option<ProjectEditorCompanionTerminalBodyMountSlotId> {
    let ShellFocusTarget::ProjectEditorCompanion(mode) = shell_focus else {
        return None;
    };
    if active_mode != mode || !mode.is_project_editor_mode() {
        return None;
    }
    Some(ProjectEditorCompanionTerminalBodyMountSlotId {
        mode,
        session_id: selected_session_id?,
    })
}


pub(crate) fn agents_terminal_surface_focus_states_for_slots(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
    mounted_slot_ids: &[AgentsTerminalBodyMountSlotId],
) -> Vec<(AgentsTerminalBodyMountSlotId, bool)> {
    let focused_slot_id =
        focused_agents_terminal_surface_mount_slot(active_mode, shell_focus, agents_workspace)
            .filter(|slot_id| mounted_slot_ids.contains(slot_id));

    mounted_slot_ids
        .iter()
        .copied()
        .map(|slot_id| (slot_id, Some(slot_id) == focused_slot_id))
        .collect()
}


pub(crate) fn command_terminal_runtime_session_id(
    slot_id: CommandTerminalBodyMountSlotId,
) -> AgentsTerminalRuntimeSessionId {
    /*
    CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
    Command-pane surfaces need a process-local owner identity for the shared Ghostty owner, but they must not enter the Agents runtime-session registry or any startup map. Derive this transient id only from the command mount slot and keep it scoped to the command surface owner map.
    */
    const COMMAND_RUNTIME_ID_NAMESPACE: u64 = 0xC000_0000_0000_0000;
    AgentsTerminalRuntimeSessionId(
        COMMAND_RUNTIME_ID_NAMESPACE
            | ((slot_id.group_id.0 & 0x7FFF_FFFF) << 32)
            | (slot_id.session_id.0 & 0xFFFF_FFFF),
    )
}


pub(crate) fn command_terminal_runtime_session_id_from_gxserver_key(
    key: &GpuiLocalWorkspaceSessionKey,
) -> AgentsTerminalRuntimeSessionId {
    /*
    CDXC:GPUICommandPaneGxserverAttach 2026-07-04:
    GPUI-engine command terminals use the daemon project/session identity as
    their runtime owner key once gxserver creation succeeds. Keep the value
    process-local and numeric for existing terminal runtime maps, but derive it
    only from the real gxserver ids instead of the command-pane mount slot.
    */
    const COMMAND_GXSERVER_RUNTIME_ID_NAMESPACE: u64 = 0xD000_0000_0000_0000;
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in key
        .project_id
        .bytes()
        .chain([b':'])
        .chain(key.session_id.bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    AgentsTerminalRuntimeSessionId(
        COMMAND_GXSERVER_RUNTIME_ID_NAMESPACE | (hash & 0x0FFF_FFFF_FFFF_FFFF),
    )
}


#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_config_request_with_launch_payload_source(
    slot_id: CommandTerminalBodyMountSlotId,
    request: terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    launch_payload_source: &mut CommandTerminalLaunchPayloadSource,
) -> Result<
    terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
> {
    /*
    CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-27-04:59:
    Command Ghostty config requests may attach launch payloads only after the explicit Action/plain-cwd command source validates a payload for the same current body slot and runtime key. If that explicit payload is invalid, the caller must omit or prune the config request instead of falling back to inferred cwd, command, env, initial input, wait policy, status, titles, labels, paths, shell state, logs, terminal content, or delayed-send state.

    CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-27-04:47:
    Config preparation consumes explicit command launch payloads exactly once for the current mount slot. A failed conversion still consumes the payload so stale startup data cannot survive into a remount or be replaced by inferred launch data.
    */
    let launch_payload = launch_payload_source.take_payload_for_mount_slot(slot_id)?;
    Ok(match launch_payload {
        Some(launch_payload) => request.with_launch_payload(launch_payload),
        None => request,
    })
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_config_request_with_launch_payload_source(
    slot_id: AgentsTerminalBodyMountSlotId,
    runtime_session_id: AgentsTerminalRuntimeSessionId,
    request: terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
) -> Result<
    terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
> {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:25:
    Local gxserver sidebar attach must not wait behind the Mounting startup card. The first config request for the exact Running Agents mount slot consumes the attach launch payload once; invalid payloads prune that mount request instead of falling back to a blank shell or inferred command.
    */
    let launch_payload =
        launch_payload_source.take_payload_for_mount_slot(runtime_session_id, slot_id)?;
    Ok(match launch_payload {
        Some(launch_payload) => request.with_launch_payload(launch_payload),
        None => request,
    })
}


pub(crate) fn focused_command_terminal_surface_mount_slot(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<CommandTerminalBodyMountSlotId> {
    /*
    CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
    Command Ghostty focus is mirrored only when shell focus is the command pane and the focused command group has a mounted active session. Agents, Browser, project-editor focus, collapsed panes, inactive command tabs, and missing sessions clear command focus without synthetic input routing.
    */
    if shell_focus != ShellFocusTarget::CommandPane {
        return None;
    }
    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    let slot_id = CommandTerminalBodyMountSlotId {
        group_id,
        session_id,
    };
    command_pane
        .is_current_terminal_body_mount_slot(slot_id)
        .then_some(slot_id)
}


/// Press-any-key wake for sleeping Agents bodies mirrors the command-pane
/// rule: only the visible focused pane's selected sleeping tab wakes, and only
/// from plain alphanumeric keys; Focus-mode-hidden panes have no visible
/// placeholder and stay parked.
pub(crate) fn focused_sleeping_agents_placeholder_wake_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
    keystroke: &Keystroke,
) -> Option<(WorkspacePaneId, TerminalSessionId)> {
    if active_mode != TitlebarMode::Agents
        || !command_pane_sleeping_placeholder_keystroke_requests_wake(keystroke)
    {
        return None;
    }
    let ShellFocusTarget::AgentsPane(pane_id) = shell_focus else {
        return None;
    };
    if !agents_workspace.rendered_leaf_order().contains(&pane_id) {
        return None;
    }
    let leaf = agents_workspace.find_leaf(pane_id)?;
    let session_id = leaf.tab_group.active_tab;
    agents_workspace
        .session(session_id)
        .is_some_and(|session| {
            session.presentation_state == TerminalSessionPresentationState::Sleeping
        })
        .then_some((pane_id, session_id))
}


pub(crate) fn focused_sleeping_command_placeholder_wake_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
    keystroke: &Keystroke,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-25-14:49:
    Keyboard wake is scoped to the command pane's focused active tab and only when that command session is parked sleeping. Non-command focus, running command terminals, collapsed/missing groups, and non-alphanumeric keys must not create terminals, reroute input, or mutate shell state.

    CDXC:GPUICommandSleepingPlaceholder 2026-06-25-19:07:
    Native key wake belongs to the visible AppKit placeholder first responder. A collapsed command strip may remember command focus, but it has no visible placeholder body, so alphanumeric keys must not wake parked command tabs until the panel is expanded.

    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-04:36:
    Keyboard wake resolves through the visible command body owner, so only the exact focused selected sleeping tab can wake. Stale selected ids, missing sessions, inactive siblings, and collapsed panes have no visible placeholder owner and must not wake or create a terminal.
    */
    if shell_focus != ShellFocusTarget::CommandPane
        || !command_pane.is_expanded()
        || !command_pane_sleeping_placeholder_keystroke_requests_wake(keystroke)
    {
        return None;
    }

    let leaf = command_pane.find_leaf(command_pane.focused_group)?;
    let body_owner = command_pane.visible_command_body_owner_for_leaf(leaf)?;
    body_owner
        .is_sleeping
        .then_some((body_owner.group_id, body_owner.session_id))
}


pub(crate) fn terminal_close_confirm_surface_signature(
    family: TerminalCloseConfirmSurfaceFamily,
) -> TerminalCloseConfirmSurfaceSignature {
    /*
    CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
    Close-confirm UI copy must stay generic while the action is enabled by the real GhosttyKit `needs_confirm_quit` ABI contract. It may identify only the safe family scope, Keep Open cancel action, and generic close action; it must not display session names, terminal titles, command text, paths, URLs, stdout/stderr, terminal content, runtime ids, tokens, raw callback payloads, or fallback close behavior.
    */
    let message = match family {
        TerminalCloseConfirmSurfaceFamily::Agents => {
            "An Agents terminal is asking for confirmation before closing."
        }
        TerminalCloseConfirmSurfaceFamily::Command => {
            "A command terminal is asking for confirmation before closing."
        }
    };

    TerminalCloseConfirmSurfaceSignature {
        title: "Terminal close requested",
        message,
        keep_open_label: "Keep Open",
        confirm_action_label: "Close Terminal",
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn pending_agents_terminal_close_confirm_for_slot(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    slot_id: AgentsTerminalBodyMountSlotId,
) -> Option<PendingAgentsTerminalCloseConfirm> {
    if !workspace.is_current_terminal_body_mount_slot(slot_id)
        || !workspace.can_close_tab(slot_id.pane_id, slot_id.session_id)
    {
        return None;
    }

    let runtime_session_id =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)?;
    let surface = running_surface_owners.get(&slot_id)?;
    (surface.mount_slot_id() == slot_id
        && surface.runtime_session_id() == runtime_session_id
        && surface.needs_confirm_quit())
    .then_some(PendingAgentsTerminalCloseConfirm {
        slot_id,
        runtime_session_id,
    })
}


#[cfg(target_os = "macos")]
pub(crate) fn pending_command_terminal_close_confirm_for_slot(
    command_pane: &CommandPaneModel,
    command_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
    slot_id: CommandTerminalBodyMountSlotId,
) -> Option<PendingCommandTerminalCloseConfirm> {
    if !command_pane.is_current_terminal_body_mount_slot(slot_id) {
        return None;
    }

    let runtime_session_id = command_terminal_runtime_session_id(slot_id);
    let surface = command_surface_owners.get(&slot_id)?;
    (surface.mount_slot_id() == slot_id
        && surface.runtime_session_id() == runtime_session_id
        && surface.needs_confirm_quit())
    .then_some(PendingCommandTerminalCloseConfirm {
        slot_id,
        runtime_session_id,
    })
}


pub(crate) fn terminal_session_title_for_id(_id: TerminalSessionId) -> String {
    "Terminal Session".to_string()
}


pub(crate) fn command_session_title_for_id(_id: CommandSessionId) -> String {
    /*
    CDXC:GPUICommandPane 2026-06-25-11:56:
    Restored GPUI command placeholders must use the same generic `Command Terminal` fallback as newly created macOS command-pane terminals. Do not derive fallback titles from command ids or persist visible/private command titles, because shell-state JSON is layout metadata rather than command-content storage.
    */
    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string()
}


pub(crate) fn gpui_command_pane_sidebar_indicator_text(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()
        && normalized.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !normalized.chars().any(char::is_control))
    .then_some(normalized)
}
