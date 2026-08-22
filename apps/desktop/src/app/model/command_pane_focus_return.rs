// C1 wave-3 re-cluster: app-modal return-focus target resolution for the command pane, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandPaneAppModalReturnFocusTarget {
    pub(crate) group_id: CommandPaneGroupId,
    pub(crate) session_id: CommandSessionId,
}


pub(crate) fn command_pane_apply_app_modal_return_focus_target(
    command_pane: &mut CommandPaneModel,
    target: CommandPaneAppModalReturnFocusTarget,
) -> bool {
    /*
    CDXC:GPUIAppModalReturnFocus 2026-06-25-22:13:
    Modal dismissal may restore only the exact command group/session captured at open time. Reject stale sessions and mismatched group ids instead of falling back to the focused group or first visible group, because fallback would send typing to the wrong command terminal after a Rename or Delayed Send dialog closes.
    */
    if command_pane.session(target.session_id).is_none()
        || command_pane_group_for_session(command_pane, target.session_id) != Some(target.group_id)
    {
        return false;
    }
    command_pane.select_session_in_group(target.group_id, target.session_id)
}


pub(crate) fn restore_command_pane_app_modal_return_focus(
    command_pane: &mut CommandPaneModel,
    target: CommandPaneAppModalReturnFocusTarget,
) -> bool {
    /*
    CDXC:GPUICommandModalFocus 2026-06-25-22:12:
    Native child app modals restore keyboard focus to the command terminal captured at open time. GPUI keeps only process-local numeric command group/session ids for that internal handoff, selects that exact tab on close, and consumes stale targets without falling back to another command group.
    */
    command_pane_apply_app_modal_return_focus_target(command_pane, target)
}


pub(crate) fn command_pane_mounted_slot_for_session(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<CommandTerminalBodyMountSlotId> {
    command_pane
        .rendered_terminal_body_mount_slots()
        .into_iter()
        .find(|slot_id| slot_id.session_id == session_id)
}


pub(crate) fn command_pane_group_for_session(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<CommandPaneGroupId> {
    command_pane
        .flat_tab_ids()
        .into_iter()
        .find_map(|(group_id, candidate)| (candidate == session_id).then_some(group_id))
}


pub(crate) fn gpui_app_modal_command_return_focus_target_for_session(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<CommandPaneAppModalReturnFocusTarget> {
    let group_id = command_pane_group_for_session(command_pane, session_id)?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some(CommandPaneAppModalReturnFocusTarget {
            group_id,
            session_id,
        })
}


pub(crate) fn gpui_app_modal_sidebar_command_live_command_tab(
    command_pane: &CommandPaneModel,
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUIAppModalCommandBridge 2026-06-25-22:54:
    Direct command-session sidebar commands such as toggleCloseAfterDone must carry an explicit external `G{u64}` sessionId that still belongs to a live command tab group. Reject raw numeric JSON ids, legacy numeric strings, lowercase/malformed ids, stale tabs, and orphan stored sessions without falling back to the focused command group.

    CDXC:GPUIAppModalCommandBridge 2026-06-25-23:04:
    scheduleDelayedSend and cancelDelayedSend submissions use this same live-tab resolver before arming or clearing a timer. Stale stored command-session rows, including orphan rows that still carry timer-owned Delayed Send flags, must not arm/cancel another timer or fall back to the focused command group.
    */
    let session_id = command
        .get("sessionId")
        .and_then(gpui_command_session_id_from_modal_value)?;
    let group_id = command_pane_group_for_session(command_pane, session_id)?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some((group_id, session_id))
}


pub(crate) fn gpui_app_modal_requires_live_requested_command_session(modal: GpuiAppModalKind) -> bool {
    matches!(
        modal,
        GpuiAppModalKind::RenameSession | GpuiAppModalKind::DelayedSend
    )
}


pub(crate) fn gpui_app_modal_requested_live_command_session_id(
    modal: GpuiAppModalKind,
    open_message: &serde_json::Value,
    command_pane: &CommandPaneModel,
) -> Option<CommandSessionId> {
    /*
    CDXC:GPUIAppModalCommandBridge 2026-06-25-22:24:
    Rename Session and Delayed Send app-modal opens are command-tab requests. They must carry an external `G{u64}` sessionId for a live CommandPaneModel session; stale, malformed, legacy numeric, or missing ids no-op instead of opening a modal for another command tab or inventing a fallback target.
    */
    if !gpui_app_modal_requires_live_requested_command_session(modal) {
        return None;
    }
    let session_id = open_message
        .get("sessionId")
        .and_then(gpui_command_session_id_from_modal_value)?;
    gpui_app_modal_command_return_focus_target_for_session(command_pane, session_id)
        .map(|target| target.session_id)
}


pub(crate) fn gpui_app_modal_has_required_live_command_session(
    modal: GpuiAppModalKind,
    open_message: &serde_json::Value,
    command_pane: &CommandPaneModel,
) -> bool {
    !gpui_app_modal_requires_live_requested_command_session(modal)
        || gpui_app_modal_requested_live_command_session_id(modal, open_message, command_pane)
            .is_some()
}


pub(crate) fn gpui_app_modal_command_return_focus_target(
    modal: GpuiAppModalKind,
    open_message: &serde_json::Value,
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<CommandPaneAppModalReturnFocusTarget> {
    /*
    CDXC:GPUIAppModalReturnFocus 2026-06-25-22:13:
    App-modal dismissal should remember only a command-pane terminal target. Rename Session and Delayed Send parse the requested external `G{u64}` bridge id into an internal numeric command session id and reject stale or malformed ids; unrelated app modals may return to the currently shell-focused expanded command group. Keep the target runtime-only and bounded to command group/session ids, with no titles, paths, command text, terminal output, URLs, or modal payload details.
    */
    if matches!(
        modal,
        GpuiAppModalKind::RenameSession | GpuiAppModalKind::DelayedSend
    ) {
        let session_id =
            gpui_app_modal_requested_live_command_session_id(modal, open_message, command_pane)?;
        return gpui_app_modal_command_return_focus_target_for_session(command_pane, session_id);
    }

    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (_group_id, session_id) = command_pane.focused_group_active_session_id()?;
    gpui_app_modal_command_return_focus_target_for_session(command_pane, session_id)
}


pub(crate) fn gpui_app_modal_command_return_focus_target_for_active_modal(
    existing: Option<CommandPaneAppModalReturnFocusTarget>,
    incoming: Option<CommandPaneAppModalReturnFocusTarget>,
) -> Option<CommandPaneAppModalReturnFocusTarget> {
    /*
    CDXC:GPUIAppModalReturnFocus 2026-06-25-22:32:
    Native `rememberAppModalReturnFocusTarget` returns early after the first command terminal is captured for an active app-modal window. GPUI existing-host modal updates must preserve that first target so duplicate or nested modal opens do not retarget dismissal focus away from the terminal that opened the original modal.
    */
    existing.or(incoming)
}


pub(crate) fn gpui_command_close_after_done_session_marked_done(session: &CommandTerminalSession) -> bool {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-15:24:
    GPUI command Close After Done mirrors native's terminal-scoped watcher for command-pane Actions: Attention is done/error, and action-owned tabs become done once their live run is no longer Working. Generic idle command placeholders without an Action identity are not treated as done.
    */
    if session.is_sleeping {
        return false;
    }
    if session.activity == CommandTerminalActivity::Attention {
        return true;
    }
    session.action_command_id.is_some()
        && session.action_run_id.is_none()
        && session.activity != CommandTerminalActivity::Working
}


pub(crate) fn gpui_command_close_after_done_runtime_timer_member(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> Option<(CommandPaneGroupId, &CommandTerminalSession)> {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-22:39:
    Runtime Close After Done countdown membership is command-tab membership, not stored-session presence. Resolve the live group with `command_pane_group_for_session`; orphan stored sessions may keep their armed boolean but cannot refresh, prune, or fire runtime timers.
    */
    let group_id = command_pane_group_for_session(command_pane, session_id)?;
    command_pane
        .session(session_id)
        .map(|session| (group_id, session))
}


pub(crate) fn gpui_command_close_after_done_timer_should_count_down(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> bool {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-27-01:37:
    A Close After Done runtime deadline is valid only while the armed command tab is live, awake, and still Done. Sleeping or newly Working tabs keep the armed intent but must stop spending the three-minute countdown until a later refresh finds them awake and Done again.
    */
    gpui_command_close_after_done_runtime_timer_member(command_pane, session_id).is_some_and(
        |(_group_id, session)| {
            session.close_after_done_armed
                && gpui_command_close_after_done_session_marked_done(session)
        },
    )
}


pub(crate) fn gpui_command_close_after_done_stale_runtime_timer_session_ids(
    command_pane: &CommandPaneModel,
    close_after_done_timers: &HashMap<CommandSessionId, GpuiCommandCloseAfterDoneTimer>,
) -> Vec<CommandSessionId> {
    close_after_done_timers
        .keys()
        .copied()
        .filter(|session_id| {
            !gpui_command_close_after_done_timer_should_count_down(command_pane, *session_id)
        })
        .collect()
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandCloseAfterDoneToggleTarget {
    ArmLiveSession,
    ClearStoredSession,
    NoOp,
}


pub(crate) fn gpui_command_close_after_done_toggle_target(
    command_pane: &CommandPaneModel,
    session_id: CommandSessionId,
) -> GpuiCommandCloseAfterDoneToggleTarget {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-22:54:
    Direct Close After Done toggles may arm only sessions that are still attached to a command tab group. Already-armed stored sessions, including stale orphans, may still clear so persisted booleans can be cleaned without reattaching, falling back, or arming an unrelated tab.
    */
    let Some(session) = command_pane.session(session_id) else {
        return GpuiCommandCloseAfterDoneToggleTarget::NoOp;
    };
    if session.close_after_done_armed {
        return GpuiCommandCloseAfterDoneToggleTarget::ClearStoredSession;
    }
    if command_pane_group_for_session(command_pane, session_id).is_some() {
        GpuiCommandCloseAfterDoneToggleTarget::ArmLiveSession
    } else {
        GpuiCommandCloseAfterDoneToggleTarget::NoOp
    }
}


pub(crate) fn command_session_is_reusable_for_action(session: &CommandTerminalSession) -> bool {
    /*
    CDXC:GPUICommandPaneActions 2026-08-08:
    A sleeping command tab is still the existing pane owned by its Action. An
    Action launch already re-selects that exact tab, marks it awake, and sends
    startup text through the existing gxserver attach path, so excluding parked
    tabs here discarded the reusable owner and allocated a duplicate pane.
    Working tabs remain non-reusable so concurrent runs keep separate owners.
    */
    session.activity == CommandTerminalActivity::Idle
}


pub(crate) fn focused_command_pane_close_after_done_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandCloseAfterDone 2026-06-25-16:52:
    Native routes Close After Done for command terminals by focused session id, not by mounted terminal surface. GPUI should resolve the expanded shell-focused command tab even when it is sleeping so users can arm or cancel the safe persisted intent without waking or mounting the terminal.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some((group_id, session_id))
}
