//! The System, Notifications and Advanced groups of General: Auto Sleep, Power, Sounds, Sleeping
//! Sessions and Experimental.
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    Chip, PageAction, RowSpec, action_buttons_field, chips_field, reset_key, select_field,
    settings_section, sound_field, toggle_color_field, toggle_field_with,
};
use super::super::super::page::PageBlock;
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, Context, Window};
use serde_json::{Map, json};
use std::rc::Rc;

fn auto_sleep_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("autoSleep", g.power_visible) {
        return None;
    }
    let s = "autoSleep";
    let options = g.options("AUTO_SLEEP_IDLE_MINUTE_OPTIONS");
    let mut rows: Vec<AnyElement> = Vec::new();
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q17): one Views auto sleep after row sets the four
    // per-view times, which stay under Show Advanced; Mixed shows while they differ.
    const VIEW_KEYS: [&str; 4] = [
        "autoSleepCodeEditorIdleMinutes",
        "autoSleepGitEditorIdleMinutes",
        "autoSleepProjectEditorIdleMinutes",
        "autoSleepBrowserIdleMinutes",
    ];
    if g.visible(s, "autoSleepViews") {
        let values: Vec<String> = VIEW_KEYS
            .iter()
            .map(|key| g.values.number_string(key))
            .collect();
        let mut view_options = options.clone();
        let value = if values.iter().all(|value| *value == values[0]) {
            values[0].clone()
        } else {
            view_options.push(SettingOption {
                label: "Mixed".to_string(),
                value: "mixed".to_string(),
            });
            "mixed".to_string()
        };
        let spec = g
            .spec(
                "autoSleepViews",
                "Views auto sleep after",
                "When inactive VS Code, Git, Project and Browser panes sleep, or Off, which also keeps them awake when more than three views are open. Sets the four per-view times.",
            )
            .modified(VIEW_KEYS.iter().any(|key| g.values.is_modified(key)));
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                for key in VIEW_KEYS {
                    store.reset_setting(key, cx);
                }
            });
        });
        rows.push(select_field(
            page,
            &g.p,
            "autoSleepViews",
            spec,
            Some(reset),
            &view_options,
            &value,
            None,
            |page: &mut GeneralTab, next, _window, cx| {
                let Ok(minutes) = next.parse::<f64>() else {
                    return;
                };
                let mut patch = Map::new();
                for key in VIEW_KEYS {
                    patch.insert(key.to_string(), json!(minutes as i64));
                }
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.apply_patch(patch, "settings:control", cx)
                });
            },
            window,
            cx,
        ));
    }
    for (key, label, description) in [
        (
            "autoSleepCodeEditorIdleMinutes",
            "VS Code sleeps after",
            "Choose when inactive VS Code panes sleep, or turn Auto Sleep off. Off also keeps Code awake when more than three views are open.",
        ),
        (
            "autoSleepGitEditorIdleMinutes",
            "Git sleeps after",
            "Choose when inactive Git panes sleep, or turn Auto Sleep off.",
        ),
        (
            "autoSleepProjectEditorIdleMinutes",
            "Project sleeps after",
            "Choose when inactive Project panes sleep, or turn Auto Sleep off. Off also keeps Files, Kanban and Automate awake when more than three views are open.",
        ),
        (
            "autoSleepBrowserIdleMinutes",
            "Browser sleeps after",
            "Choose when inactive browser panes sleep, or turn Auto Sleep off. Off also keeps the Browser awake when more than three views are open.",
        ),
        (
            "autoSleepAgentIdleMinutes",
            "Agents sleep after",
            "Choose when eligible agent terminals sleep, or turn Auto Sleep off.",
        ),
    ] {
        rows.extend(page.number_select(
            g,
            s,
            key,
            label,
            description,
            options.clone(),
            false,
            window,
            cx,
        ));
        if key == "autoSleepBrowserIdleMinutes" {
            rows.extend(page.toggle(
                g,
                s,
                "keepViewsLoaded",
                "Keep views loaded",
                "Keep the side panel views you open loaded when you switch projects or workspaces, without the three-view limit or the previous project's time limit. Auto Sleep and Sleep still apply. Uses more memory.",
                false,
                cx,
            ));
        }
    }
    if g.values.f64("autoSleepAgentIdleMinutes") > 0.0 {
        rows.extend(page.toggle(
            g,
            s,
            "autoSleepRequireAgentResumeCommand",
            "Only sleep agents that can be resumed",
            "Only auto-sleep agent sessions Ghostex can wake with a resume command.",
            true,
            cx,
        ));
        rows.extend(page.toggle(
            g,
            s,
            "autoSleepFavoriteAgentSessions",
            "Sleep favourite sessions too",
            "Allow favorite agent sessions to auto-sleep.",
            true,
            cx,
        ));
    }
    settings_section(&g.p, "Auto Sleep", None, None, rows)
        .map(|section| PageBlock::section("autoSleep", section))
}

fn power_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("power", g.power_visible) {
        return None;
    }
    let s = "power";
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(
        g,
        s,
        "hideKeepAwakeTitlebarControl",
        "Hide Keep Awake from the sidebar menu",
        "Hide the Keep Awake entry from the sidebar menu.",
        false,
        cx,
    ));
    rows.extend(page.number_select(
        g,
        s,
        "keepAwakeDefaultDurationMinutes",
        "Keep Awake lasts",
        "Choose the duration Keep Awake uses by default.",
        g.options("KEEP_AWAKE_DURATION_OPTIONS"),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "keepAwakeAllowDisplaySleep",
        "Let the display sleep while Keep Awake is on",
        "Keep the computer awake but allow the display to turn off.",
        false,
        cx,
    ));
    rows.extend(page.toggle(g, s, "keepAwakePreventLidSleep", "Prevent lid-close sleep", "Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session. Keep Awake itself remains off until you enable it.", false, cx));
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q17): the three activate switches are the chips of
    // Turn on automatically, the two deactivate switches and the battery rule the chips of Turn
    // off automatically (Low battery keeps its level in a row under it).
    const ACTIVATION: [(&str, &str); 3] = [
        ("keepAwakeActivateOnLaunch", "On launch"),
        ("keepAwakeActivateOnExternalDisplay", "External display"),
        ("keepAwakeWhileWorkingSessions", "While sessions work"),
    ];
    if g.any_visible(s, &["keepAwakeActivation"])
        || (g.search.is_searching() && ACTIVATION.iter().any(|(key, _)| g.visible(s, key)))
    {
        let chips: Vec<Chip> = ACTIVATION
            .iter()
            .map(|(key, label)| Chip {
                id: key,
                label,
                on: g.values.bool(key),
            })
            .collect();
        let spec = g
            .spec(
                "keepAwakeActivation",
                "Turn on automatically",
                "When Keep Awake turns itself on: when Ghostex launches, when an external display is connected, or while any session is Working and for 20 minutes after.",
            )
            .modified(ACTIVATION.iter().any(|(key, _)| g.values.is_modified(key)));
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                for (key, _) in ACTIVATION {
                    store.reset_setting(key, cx);
                }
            });
        });
        rows.push(chips_field(
            &g.p,
            "keepAwakeActivation",
            spec,
            Some(reset),
            chips,
            |page: &mut GeneralTab, key, on, _window, cx| save(page, key, json!(on), cx),
            cx,
        ));
    }
    const DEACTIVATION: [(&str, &str); 2] = [
        ("keepAwakeDeactivateOnLowPowerMode", "Low Power Mode"),
        ("keepAwakeDeactivateOnUserSwitch", "User switch"),
    ];
    const BATTERY: &str = "keepAwakeBatteryThresholdPercent";
    let threshold = g.values.f64(BATTERY);
    if g.any_visible(s, &["keepAwakeDeactivation"])
        || (g.search.is_searching()
            && (g.visible(s, BATTERY) || DEACTIVATION.iter().any(|(key, _)| g.visible(s, key))))
    {
        let mut chips: Vec<Chip> = DEACTIVATION
            .iter()
            .map(|(key, label)| Chip {
                id: key,
                label,
                on: g.values.bool(key),
            })
            .collect();
        chips.push(Chip {
            id: BATTERY,
            label: "Low battery",
            on: threshold > 0.0,
        });
        let spec = g
            .spec(
                "keepAwakeDeactivation",
                "Turn off automatically",
                "When Keep Awake turns itself off: in Low Power Mode, when this user session is no longer active, or below a battery level.",
            )
            .modified(
                DEACTIVATION.iter().any(|(key, _)| g.values.is_modified(key))
                    || g.values.is_modified(BATTERY),
            );
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                for (key, _) in DEACTIVATION {
                    store.reset_setting(key, cx);
                }
                store.reset_setting(BATTERY, cx);
            });
        });
        rows.push(chips_field(
            &g.p,
            "keepAwakeDeactivation",
            spec,
            Some(reset),
            chips,
            |page: &mut GeneralTab, key, on, _window, cx| {
                if key == BATTERY {
                    save(page, key, json!(if on { 20 } else { 0 }), cx)
                } else {
                    save(page, key, json!(on), cx)
                }
            },
            cx,
        ));
    }
    let battery: Vec<SettingOption> = std::iter::once(SettingOption {
        label: "Off".to_string(),
        value: "0".to_string(),
    })
    .chain((0..17).map(|index| {
        let percent = 10 + index * 5;
        SettingOption {
            label: format!("{percent}%"),
            value: percent.to_string(),
        }
    }))
    .collect();
    if threshold > 0.0 {
        let levels: Vec<SettingOption> = battery
            .into_iter()
            .filter(|option| option.value != "0")
            .collect();
        rows.extend(page.number_select(
            g,
            s,
            BATTERY,
            "Battery level",
            "Stop preventing sleep below this battery level.",
            levels,
            true,
            window,
            cx,
        ));
    }
    settings_section(&g.p, "Power", None, None, rows)
        .map(|section| PageBlock::section("power", section))
}

fn post(page: &mut GeneralTab, kind: &str, cx: &mut Context<GeneralTab>) {
    let store = page.store.clone();
    super::super::super::store::post_store_message(&store, json!({ "type": kind }), cx);
}

fn sounds_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("sounds", g.power_visible) {
        return None;
    }
    let s = "sounds";
    let p = g.p;
    let mut rows: Vec<AnyElement> = Vec::new();
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q18): Show notification on terminal bell moved here from Terminal.
    rows.extend(page.toggle(
        g,
        s,
        "showNotificationOnTerminalBell",
        "Show notification on terminal bell",
        "Treat terminal bell events as session attention.",
        false,
        cx,
    ));
    // CDXC:Settings 2026-09-09 DECISION: the notification toggle leads Sounds, the two sound pickers sit together under it.
    if g.visible(s, "showMacOSAttentionNotifications") {
        let spec = g.spec(
            "showMacOSAttentionNotifications",
            "System notification when a session needs you",
            "Show a system notification when a session needs attention.",
        );
        rows.push(toggle_field_with(
            &p,
            "showMacOSAttentionNotifications",
            spec,
            g.values.bool("showMacOSAttentionNotifications"),
            Some(reset_key::<GeneralTab>("showMacOSAttentionNotifications")),
            |page: &mut GeneralTab, checked, _window, cx| {
                save(page, "showMacOSAttentionNotifications", json!(checked), cx);
                if checked {
                    post(page, "requestMacOSNotificationPermission", cx);
                }
            },
            cx,
        ));
    }
    // CDXC:Notifications 2026-11-01 DECISION: User (Q929): "keep the color and the switch in same row in settings. only show color picker if toggled on."
    if g.visible(s, "showFinishedWorkBorder") || g.visible(s, "finishedWorkBorderColor") {
        let spec = g
            .spec(
                "showFinishedWorkBorder",
                "Finished-work border",
                "Outline a session's pane in a colour when its agent finishes work or needs you, until you look at it. The sidebar dot, sounds and notifications stay either way.",
            )
            .modified(
                g.values.is_modified("showFinishedWorkBorder")
                    || g.values.is_modified("finishedWorkBorderColor"),
            );
        let color = g.values.string("finishedWorkBorderColor");
        rows.push(toggle_color_field(
            page,
            &p,
            "showFinishedWorkBorder",
            "finishedWorkBorderColor",
            spec,
            g.values.bool("showFinishedWorkBorder"),
            &color,
            window,
            cx,
        ));
    }
    if g.visible(s, "resetExpirySystemNotifications") {
        let spec = g.spec(
            "resetExpirySystemNotifications",
            "Warn before a banked usage reset expires",
            "Show a system notification when a banked Claude or Codex usage reset expires within 3 days and again within 24 hours. The red notification in the bell stays either way.",
        );
        rows.push(toggle_field_with(
            &p,
            "resetExpirySystemNotifications",
            spec,
            g.values.bool("resetExpirySystemNotifications"),
            Some(reset_key::<GeneralTab>("resetExpirySystemNotifications")),
            |page: &mut GeneralTab, checked, _window, cx| {
                save(page, "resetExpirySystemNotifications", json!(checked), cx);
                if checked {
                    post(page, "requestMacOSNotificationPermission", cx);
                }
            },
            cx,
        ));
    }
    if g.visible(s, "completionSound") {
        let spec = g.spec(
            "completionSound",
            "Agent finished sound",
            "The sound when an agent finishes a task or needs you, or Off.",
        );
        let value = g.values.string("completionSound");
        rows.push(sound_field(
            page,
            &p,
            "completionSound",
            spec,
            Some(reset_key::<GeneralTab>("completionSound")),
            true,
            &value,
            window,
            cx,
        ));
    }
    if g.visible(s, "actionCompletionSound") {
        let spec = g.spec(
            "actionCompletionSound",
            "Command finished sound",
            "The sound when a command run from the Commands pane finishes.",
        );
        let value = g.values.string("actionCompletionSound");
        rows.push(sound_field(
            page,
            &p,
            "actionCompletionSound",
            spec,
            Some(reset_key::<GeneralTab>("actionCompletionSound")),
            false,
            &value,
            window,
            cx,
        ));
    }
    rows.extend(page.toggle(
        g,
        s,
        "copySound",
        "Copy sound",
        "Play a short sound when copying to the clipboard, including text from the chat composer.",
        false,
        cx,
    ));
    // CDXC:Notifications 2026-05-11-01:14: the test runs the real completion alert path.
    if g.visible(s, "attentionNotificationActions") {
        let spec = RowSpec::new("Test alerts")
            .description("Play the agent finished sound and show the notification as they are set now, or open the system's notification settings for Ghostex.");
        let test: PageAction<GeneralTab> =
            Rc::new(|page: &mut GeneralTab, _window, cx| post(page, "testAgentTaskCompletion", cx));
        let settings: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            post(page, "openMacOSNotificationSettings", cx)
        });
        rows.push(action_buttons_field(
            &p,
            "attentionNotificationActions",
            spec,
            vec![
                ("Play a test alert", test),
                ("System notification settings", settings),
            ],
            cx,
        ));
    }
    settings_section(&p, "Sounds", None, None, rows)
        .map(|section| PageBlock::section("notifications", section))
}

/// The two sleeping-session toggles (CDXC:SessionSleep 2026-09-29 DECISION in
/// packages/shared/ghostex-settings/types.ts (deleted 2026-10-01)), on the Sidebar & Sessions page.
pub(super) fn sleeping_sessions_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g
        .search
        .subsection_visible("sleepingSessions", g.power_visible)
    {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(
        g,
        "sleepingSessions",
        "dimSleepingSessions",
        "Dim sleeping sessions",
        "Fade sleeping sessions in the sidebar so they stand apart from awake ones.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        "sleepingSessions",
        "wakeSleepingSessionsOnSelect",
        "Wake sleeping sessions when selected",
        "Wake a sleeping session as soon as you select it. Turn off to open it with a Resume button instead, so switching sessions does not wake it by accident.",
        false,
        cx,
    ));
    settings_section(&g.p, "Sleeping Sessions", None, None, rows)
        .map(|section| PageBlock::section("sleepingSessions", section))
}

/// The Power & Sleep page (the old System group: CDXC:Settings 2026-10-29 in model.rs).
pub(super) fn power_sleep_sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        auto_sleep_section(page, g, window, cx),
        power_section(page, g, window, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The Notifications page.
pub(super) fn notifications_sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    sounds_section(page, g, window, cx).into_iter().collect()
}
