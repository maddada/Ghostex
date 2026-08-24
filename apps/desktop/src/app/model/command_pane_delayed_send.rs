// C1 wave-3 re-cluster: command pane delayed-send and close-after-done timers, restore intents, and their label/duration formatting, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuiCommandDelayedSendTimer {
    pub(crate) deadline_at: SystemTime,
    pub(crate) generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiAgentsSendWhenStoppedWatcher {
    pub(crate) generation: u64,
    pub(crate) non_working_since: Option<Instant>,
    pub(crate) scope: GpuiAgentsSendWhenStoppedScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAgentsSendWhenStoppedScope {
    Session,
    Project(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuiAgentsDelayedSendTarget {
    GpuiEngine {
        session_id: TerminalSessionId,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    },
    AgentsNative(AgentsTerminalBodyMountSlotId),
    ProjectEditorCompanionNative(ProjectEditorCompanionTerminalBodyMountSlotId),
    #[cfg(target_os = "macos")]
    AgentsParkedNative(AgentsTerminalRuntimeSessionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandDelayedSendRestoreTimer {
    pub(crate) session_id: CommandSessionId,
    pub(crate) remaining_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiAgentsDelayedSendRestoreIntent {
    pub(crate) session_id: TerminalSessionId,
    pub(crate) trigger: GpuiAgentsDelayedSendRestoreTrigger,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiAgentsDelayedSendRestoreTrigger {
    Timer { remaining_ms: u64 },
    WhenAgentFinishesWorking,
    WhenAllAgentsFinishWorking { project_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiCommandStartupActivityRestoreIntent {
    pub(crate) session_id: CommandSessionId,
    pub(crate) activity: CommandTerminalActivity,
}

impl GpuiCommandDelayedSendTimer {
    pub(crate) fn remaining_ms(self, now: SystemTime) -> u64 {
        self.deadline_at
            .duration_since(now)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuiCommandCloseAfterDoneTimer {
    pub(crate) deadline_at: SystemTime,
    pub(crate) generation: u64,
}

impl GpuiCommandCloseAfterDoneTimer {
    pub(crate) fn remaining_ms(self, now: SystemTime) -> u64 {
        self.deadline_at
            .duration_since(now)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0)
    }
}

pub(crate) fn gpui_command_delayed_send_duration_from_millis(delay_ms: u64) -> Option<Duration> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-15:11:
    The shared Delayed Send modal posts only a session id and delay duration. GPUI must keep the same whole-minute, one-minute-minimum, 24-day JavaScript-timer-compatible bounds as native before arming a command-pane timer.
    */
    if !(COMMAND_PANE_DELAYED_SEND_MIN_DELAY_MS..=COMMAND_PANE_DELAYED_SEND_MAX_DELAY_MS)
        .contains(&delay_ms)
        || delay_ms % COMMAND_PANE_DELAYED_SEND_MIN_DELAY_MS != 0
    {
        return None;
    }
    Some(Duration::from_millis(delay_ms))
}

pub(crate) fn gpui_command_delayed_send_countdown_label(remaining_ms: u64) -> String {
    let total_seconds = remaining_ms.saturating_add(999) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub(crate) fn gpui_agents_send_when_stopped_remaining_label(
    watcher: &GpuiAgentsSendWhenStoppedWatcher,
    is_working: bool,
    now: Instant,
) -> String {
    if is_working {
        return match &watcher.scope {
            GpuiAgentsSendWhenStoppedScope::Session => "Waiting for agent".to_string(),
            GpuiAgentsSendWhenStoppedScope::Project(_) => "Waiting for agents".to_string(),
        };
    }
    let remaining = watcher
        .non_working_since
        .map(|started_at| {
            GPUI_AGENTS_SEND_WHEN_STOPPED_STABILITY_DURATION
                .saturating_sub(now.saturating_duration_since(started_at))
        })
        .unwrap_or(GPUI_AGENTS_SEND_WHEN_STOPPED_STABILITY_DURATION);
    gpui_command_delayed_send_countdown_label(remaining.as_millis().min(u128::from(u64::MAX)) as u64)
}

pub(crate) fn gpui_command_delayed_send_body_badge_label(
    timer: Option<GpuiCommandDelayedSendTimer>,
    now: SystemTime,
) -> Option<String> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-15:42:
    The command body badge projects only a countdown string from a runtime timer. It must not inspect command text, titles, terminal content, paths, shell-state JSON, or persisted delayed-send placeholders.
    */
    timer.map(|timer| gpui_command_delayed_send_countdown_label(timer.remaining_ms(now)))
}

pub(crate) fn gpui_command_delayed_send_duration_label(duration: Duration) -> String {
    let total_seconds = duration.as_secs().max(1);
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

pub(crate) fn gpui_command_delayed_send_restore_remaining_ms(
    value: &serde_json::Value,
) -> Option<u64> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-16:41:
    Restored GPUI command Delayed Send timers should match macOS by resuming from a saved remaining-duration checkpoint, not by spending countdown time while the app is closed. Accept only bounded numeric milliseconds and keep command text, titles, terminal content, paths, runtime ids, and stdout/stderr out of the restart contract.
    */
    let remaining_ms = value.as_u64()?;
    (remaining_ms > 0 && remaining_ms <= COMMAND_PANE_DELAYED_SEND_MAX_DELAY_MS)
        .then_some(remaining_ms)
}

pub(crate) fn gpui_command_delayed_send_restore_duration(remaining_ms: u64) -> Duration {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-16:41:
    macOS gives restored Delayed Send timers a 2s fire grace even when the saved checkpoint is nearly expired. GPUI should use the same startup grace so a restored command tab has time to mount before the pending Return key can fire.
    */
    Duration::from_millis(remaining_ms.max(COMMAND_PANE_DELAYED_SEND_RESTORE_FIRE_GRACE_MS))
}

pub(crate) fn gpui_command_session_id_from_modal_value(
    value: &serde_json::Value,
) -> Option<CommandSessionId> {
    gpui_command_session_id_from_external_id(value.as_str()?)
}

pub(crate) fn gpui_command_session_rename_title_from_modal_value(
    value: &serde_json::Value,
) -> Option<String> {
    /*
    CDXC:GPUICommandPaneRename 2026-06-25-16:33:
    The shared Rename Session modal already applies the normal sidebar rename normalization before posting. Revalidate the GPUI boundary by accepting only non-empty, non-control text and collapsing whitespace so direct bridge messages cannot store multiline terminal content as command-tab chrome.
    */
    let title = value
        .as_str()?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()
        && title.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !title.chars().any(char::is_control))
    .then_some(title)
}
