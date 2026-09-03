// C1 wave-1 deferred split: apps/desktop/src/app/helpers/agents_hub.rs (~3.4k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the titlebar Keep Awake working-session
// counting/grace-state helpers and the macOS power (pmset/system_profiler)
// snapshot probing + parsing. See docs/2026-08-22/repo-restructure/SPLITS.md
// C1.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_keep_awake_agents_working_session_count(workspace: &WorkspaceModel) -> usize {
    workspace
        .terminal_sessions
        .iter()
        .filter(|session| {
            session.presentation_state == TerminalSessionPresentationState::Running
                && session.activity == AgentTerminalActivity::Working
        })
        .count()
}

pub(crate) fn gpui_keep_awake_command_working_session_count(
    command_pane: &CommandPaneModel,
) -> usize {
    command_pane
        .flat_tab_ids()
        .into_iter()
        .filter(|(_group_id, session_id)| {
            command_pane.session(*session_id).is_some_and(|session| {
                !session.is_sleeping && session.activity == CommandTerminalActivity::Working
            })
        })
        .count()
}

pub(crate) fn gpui_keep_awake_command_delayed_send_session_count(
    command_pane: &CommandPaneModel,
) -> usize {
    /*
    CDXC:KeepAwake 2026-06-27-01:05:
    Native `createTitlebarKeepAwakeSessionState` treats projected Delayed Send remaining-time fields as a Keep Awake hold input only for non-sleeping terminal sessions. Agents `delayed_send_active` is semantic tab chrome in GPUI, unlike native projected remaining-time fields, so only command tabs with timer-owned runtime Delayed Send state can start the titlebar power hold.
    */
    command_pane
        .flat_tab_ids()
        .into_iter()
        .filter(|(_group_id, session_id)| {
            command_pane.session(*session_id).is_some_and(|session| {
                !session.is_sleeping
                    && session.delayed_send_active
                    && session.delayed_send_timer_owned
            })
        })
        .count()
}

pub(crate) fn gpui_keep_awake_working_session_count(
    workspace: &WorkspaceModel,
    command_pane: &CommandPaneModel,
) -> usize {
    /*
    CDXC:KeepAwake 2026-06-26-00:29:
    GPUI `keepAwakeWhileWorkingSessions` counts only real local terminal model facts equivalent to the macOS titlebar projection: Running Agents with `AgentTerminalActivity::Working` plus live command-tab sessions that are not sleeping and have `CommandTerminalActivity::Working`. Do not infer work from titles, command text, paths, status-file contents, terminal output, gxserver ids, or persisted private metadata.
    */
    gpui_keep_awake_agents_working_session_count(workspace)
        + gpui_keep_awake_command_working_session_count(command_pane)
}

pub(crate) fn gpui_keep_awake_refresh_working_session_grace_state(
    enabled: bool,
    previous_working_session_count: usize,
    current_working_session_count: usize,
    grace_until: Option<Instant>,
    now: Instant,
) -> GpuiKeepAwakeWorkingSessionGraceState {
    if !enabled {
        return GpuiKeepAwakeWorkingSessionGraceState {
            previous_working_session_count: 0,
            grace_until: None,
        };
    }

    let active_grace_until = grace_until.filter(|deadline| *deadline > now);
    let grace_until = if previous_working_session_count > 0 && current_working_session_count == 0 {
        now.checked_add(GPUI_KEEP_AWAKE_WORKING_SESSION_GRACE)
            .or(active_grace_until)
    } else {
        active_grace_until
    };

    GpuiKeepAwakeWorkingSessionGraceState {
        previous_working_session_count: current_working_session_count,
        grace_until,
    }
}

pub(crate) fn gpui_keep_awake_working_session_hold_active(
    settings: shared_settings::SharedKeepAwakeTitlebarSettings,
    observed_working_session_count: usize,
    grace_until: Option<Instant>,
    now: Instant,
) -> bool {
    settings.while_working_sessions
        && (observed_working_session_count > 0
            || grace_until.is_some_and(|deadline| now < deadline))
}

pub(crate) fn gpui_keep_awake_fire_at(
    started_at: Instant,
    duration_minutes: shared_settings::SharedKeepAwakeDurationMinutes,
) -> Option<Instant> {
    (duration_minutes.minutes() > 0)
        .then(|| started_at + Duration::from_secs(duration_minutes.minutes() * 60))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_read_keep_awake_power_snapshot(
    options: GpuiKeepAwakePowerProbeOptions,
) -> Result<GpuiKeepAwakePowerSnapshot, String> {
    /*
    CDXC:KeepAwake 2026-06-25-23:49:
    Power automation probes must use fixed macOS command paths and only when the corresponding Settings rule can act. Parse stdout in memory into battery percent, Low Power Mode boolean, and display count; never log or persist command output, display names, paths, URLs, commands, settings payloads, or private terminal content.
    */
    let battery_output = if options.include_battery {
        Some(gpui_keep_awake_fixed_command_stdout(
            "/usr/bin/pmset",
            &["-g", "batt"],
        )?)
    } else {
        None
    };
    let low_power_output = if options.include_low_power_mode {
        Some(gpui_keep_awake_fixed_command_stdout(
            "/usr/bin/pmset",
            &["-g"],
        )?)
    } else {
        None
    };
    let display_output = if options.include_external_display {
        Some(gpui_keep_awake_fixed_command_stdout(
            "/usr/sbin/system_profiler",
            &["SPDisplaysDataType"],
        )?)
    } else {
        None
    };
    Ok(gpui_parse_keep_awake_power_snapshot(
        battery_output.as_deref(),
        low_power_output.as_deref(),
        display_output.as_deref(),
    ))
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_keep_awake_fixed_command_stdout(
    path: &str,
    args: &[&str],
) -> Result<String, String> {
    let output = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "power probe failed".to_string())?;
    if !output.status.success() {
        return Err("power probe failed".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn gpui_parse_keep_awake_power_snapshot(
    battery_output: Option<&str>,
    low_power_output: Option<&str>,
    display_output: Option<&str>,
) -> GpuiKeepAwakePowerSnapshot {
    GpuiKeepAwakePowerSnapshot {
        battery_percent: battery_output.and_then(gpui_parse_pmset_battery_percent),
        external_display_connected: display_output
            .map(gpui_system_profiler_external_display_connected)
            .unwrap_or(false),
        low_power_mode: low_power_output.and_then(gpui_parse_pmset_low_power_mode),
    }
}

pub(crate) fn gpui_parse_pmset_battery_percent(output: &str) -> Option<f64> {
    for line in output.lines() {
        let Some(percent_index) = line.find('%') else {
            continue;
        };
        let before_percent = &line[..percent_index];
        let digits_reversed: String = before_percent
            .chars()
            .rev()
            .skip_while(|ch| ch.is_ascii_whitespace())
            .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
            .collect();
        if digits_reversed.is_empty() {
            continue;
        }
        let digits: String = digits_reversed.chars().rev().collect();
        let Ok(percent) = digits.parse::<f64>() else {
            continue;
        };
        if percent.is_finite() {
            return Some(percent);
        }
    }
    None
}

pub(crate) fn gpui_parse_pmset_low_power_mode(output: &str) -> Option<bool> {
    output.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next() == Some("lowpowermode")).then(|| parts.next() == Some("1"))
    })
}

pub(crate) fn gpui_system_profiler_external_display_connected(output: &str) -> bool {
    output
        .lines()
        .filter(|line| line.trim_start().starts_with("Resolution:"))
        .count()
        > 1
}
