use crate::data::*;
use crate::json::opt;
use crate::rows::{option, row, section, Section};

pub(crate) fn auto_sleep() -> Section {
    section(
        "autoSleep",
        "Auto Sleep",
        vec![
            // CDXC:Settings 2026-10-29 DECISION (cleanup Q17): one Views auto sleep after row sets the four
            // per-view times; the per-view rows stay under Show Advanced.
            row("autoSleepViews", "Views auto sleep after", "When inactive VS Code, Git, Project and Browser panes sleep, or Off. Sets the four per-view times below.")
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepCodeEditorIdleMinutes",
                "VS Code sleeps after",
                "Choose when inactive VS Code panes sleep, or turn Auto Sleep off. Off also keeps Code awake when more than three views are open.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepGitEditorIdleMinutes",
                "Git sleeps after",
                "Choose when inactive Git panes sleep, or turn Auto Sleep off.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepProjectEditorIdleMinutes",
                "Project sleeps after",
                "Choose when inactive Project panes sleep, or turn Auto Sleep off. Off also keeps Files, Kanban and Automate awake when more than three views are open.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepBrowserIdleMinutes",
                "Browser sleeps after",
                "Choose when inactive browser panes sleep, or turn Auto Sleep off. Off also keeps the Browser awake when more than three views are open.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "keepViewsLoaded",
                "Keep views loaded",
                "Keep the side panel views you open loaded when you switch projects or workspaces, without the three-view limit or the previous project's time limit. Auto Sleep and Sleep still apply. Uses more memory.",
            ),
            row(
                "autoSleepAgentIdleMinutes",
                "Agents sleep after",
                "Choose when eligible agent terminals sleep, or turn Auto Sleep off.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepRequireAgentResumeCommand",
                "Only sleep agents that can be resumed",
                "Only auto-sleep agent sessions Ghostex can wake with a resume command.",
            ),
            row(
                "autoSleepFavoriteAgentSessions",
                "Sleep favourite sessions too",
                "Allow favorite agent sessions to auto-sleep.",
            ),
        ],
    )
}

pub(crate) fn power() -> Section {
    section(
        "power",
        "Power",
        vec![
            row(
                "hideKeepAwakeTitlebarControl",
                "Hide Keep Awake from the sidebar menu",
                "Hide the Keep Awake entry from the sidebar menu.",
            ),
            row(
                "keepAwakeDefaultDurationMinutes",
                "Keep Awake lasts",
                "Choose the duration Keep Awake uses by default.",
            )
            .num_options(KEEP_AWAKE_DURATION_OPTIONS),
            row(
                "keepAwakeAllowDisplaySleep",
                "Let the display sleep while Keep Awake is on",
                "Keep the computer awake but allow the display to turn off.",
            ),
            row(
                "keepAwakePreventLidSleep",
                "Prevent lid-close sleep",
                "Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session.",
            ),
            // CDXC:Settings 2026-10-29 DECISION (cleanup Q17): the three activate rows are the chips of Turn on
            // automatically and the three deactivate rows the chips of Turn off automatically.
            row("keepAwakeActivation", "Turn on automatically", "When Keep Awake turns itself on: when Ghostex launches, when an external display is connected, or while a session is working."),
            row("keepAwakeDeactivation", "Turn off automatically", "When Keep Awake turns itself off: in Low Power Mode, when this user session is no longer active, or below a battery level."),
            row(
                "keepAwakeActivateOnLaunch",
                "Activate on launch",
                "Start preventing sleep when Ghostex launches.",
            ),
            row(
                "keepAwakeActivateOnExternalDisplay",
                "Activate on external display",
                "Start preventing sleep when an external display is connected.",
            ),
            row(
                "keepAwakeWhileWorkingSessions",
                "Keep awake for working sessions",
                "Keep the computer awake while sessions are working and for 20 minutes after.",
            ),
            row(
                "keepAwakeBatteryThresholdPercent",
                "Battery threshold",
                "Stop preventing sleep below this battery level, or turn the rule off.",
            )
            .options(&[opt("Off", "0")])
            .option_list((0..17).map(|index| {
                let percent = 10 + index * 5;
                option(format!("{percent}%"), percent.to_string())
            })),
            row(
                "keepAwakeDeactivateOnLowPowerMode",
                "Deactivate in Low Power Mode",
                "Stop preventing sleep when Low Power Mode is enabled.",
            ),
            row(
                "keepAwakeDeactivateOnUserSwitch",
                "Deactivate on user switch",
                "Stop preventing sleep when this user session is no longer active.",
            ),
        ],
    )
}

pub(crate) fn sleeping_sessions() -> Section {
    section(
        "sleepingSessions",
        "Sleeping Sessions",
        vec![
            row(
                "dimSleepingSessions",
                "Dim sleeping sessions",
                "Fade sleeping sessions in the sidebar so they stand apart from awake ones.",
            ),
            row(
                "wakeSleepingSessionsOnSelect",
                "Wake sleeping sessions when selected",
                "Wake a sleeping session as soon as you select it. Turn off to open it with a Resume button instead, so switching sessions does not wake it by accident.",
            ),
        ],
    )
}
