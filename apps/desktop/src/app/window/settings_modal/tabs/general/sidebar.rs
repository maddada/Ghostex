//! The Sidebar group of General: Sidebar, Session Cards and Sidebar Tags.
use super::super::super::catalog::{SettingOption, module, settings_catalog};
use super::super::super::fields::{
    Chip, HoverStripKind, PageAction, RowSpec, card_inset, chips_field, custom_note,
    hover_actions_field, hover_buttons_are_default, normalize_hover_buttons, reset_key,
    segmented_field, select_field, settings_section, tag_list_field, toggle_field_with,
};
use super::super::super::page::PageBlock;
use super::super::super::store::SettingsStore;
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, Context, SharedString, Window};
use serde_json::{Map, Value, json};
use std::rc::Rc;

/// `SIDEBAR_SETTINGS_PRESETS` as `(id, label, settings)`.
fn presets() -> Vec<(String, String, Map<String, Value>)> {
    settings_catalog()
        .module_value(module::SETTINGS, "SIDEBAR_SETTINGS_PRESETS")
        .and_then(Value::as_array)
        .map(|presets| {
            presets
                .iter()
                .filter_map(|preset| {
                    Some((
                        preset.get("id")?.as_str()?.to_string(),
                        preset.get("label")?.as_str()?.to_string(),
                        preset.get("settings")?.as_object()?.clone(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `getSidebarSettingsPresetId`: the preset whose every controlled key matches, if any.
fn active_preset(g: &GeneralCx) -> Option<String> {
    let keys =
        settings_catalog().string_list(module::SETTINGS, "SIDEBAR_SETTINGS_PRESET_KEYS", None);
    presets().into_iter().find_map(|(id, _, settings)| {
        keys.iter()
            .all(|key| {
                settings
                    .get(key)
                    .is_some_and(|value| *value == g.values.value(key))
            })
            .then_some(id)
    })
}

/// `updateSidebarSettingsPreset(presetId)`: the whole draft with the preset's keys applied.
fn apply_preset(page: &mut GeneralTab, preset_id: &str, cx: &mut Context<GeneralTab>) {
    let Some((_, _, preset)) = presets().into_iter().find(|(id, _, _)| id == preset_id) else {
        return;
    };
    let store = page.store.clone();
    store.update(cx, |store: &mut SettingsStore, cx| {
        let mut settings = store.settings().clone();
        for (key, value) in preset {
            settings.insert(key, value);
        }
        store.apply_settings(settings, "settings:bulk", cx);
    });
}

fn sidebar_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("sidebar", g.power_visible) {
        return None;
    }
    let s = "sidebar";
    let mut rows: Vec<AnyElement> = Vec::new();
    // CDXC:Settings 2026-06-12-07:10: Preset leads the Sidebar section.
    if g.visible(s, "sidebarSettingsPreset") {
        let active = active_preset(g);
        let options: Vec<SettingOption> = presets()
            .into_iter()
            .map(|(id, label, _)| SettingOption { label, value: id })
            .collect();
        let spec = RowSpec::new("Preset")
            .description("Apply a sidebar UI preset.")
            .modified(active.as_deref() != Some("recommended"));
        rows.push(segmented_field(
            &g.p,
            "sidebarSettingsPreset",
            spec,
            Some(Rc::new(
                |page: &mut GeneralTab, _window, cx: &mut Context<GeneralTab>| {
                    apply_preset(page, "recommended", cx)
                },
            )),
            &options,
            active.as_deref(),
            active.is_none().then(|| custom_note(&g.p)),
            |page: &mut GeneralTab, next, _window, cx| apply_preset(page, &next, cx),
            cx,
        ));
    }
    // The Project / Unified switch lives here only (CDXC:Sidebar 2026-10-30 on SIDEBAR_VIEW_OPTIONS).
    if g.visible(s, "sidebarView") {
        let spec = RowSpec::new("Sidebar")
            .description("Project sidebar lists each machine, Space and project on its own. Unified sidebar shows every session from every machine, Space and project in one list, with the ones that need you on top.")
            .modified(g.values.is_modified("sidebarView"));
        rows.extend(page.segmented(
            g,
            s,
            "sidebarView",
            spec,
            Some(reset_key::<GeneralTab>("sidebarView")),
            g.options("SIDEBAR_VIEW_OPTIONS"),
            cx,
        ));
    }
    // The Unified sidebar's own Working section switch, under the switch that selects it (CDXC:Sidebar
    // 2026-10-30 in gx-core unified_sidebar/build.rs `unified_shelf`).
    if g.values.string("sidebarView") == "unified" {
        rows.extend(page.toggle(
            g,
            s,
            "unifiedGroupByProject",
            "Group by project",
            "Put every machine's sessions of one repository under one header, with its Working, Snoozed and Parked sections inside it. Drag the headers to arrange them. Drafts stay above the groups. Also in the sidebar's Sort & Filter menu. Shown while the Unified sidebar is selected.",
            false,
            cx,
        ));
        rows.extend(page.toggle(
            g,
            s,
            "unifiedWorkingSection",
            "Working section",
            "Fold sessions whose agent is working or still running something in the background into a Working section at the bottom of the list. They return to the top of the list when they need you. Shown while the Unified sidebar is selected.",
            true,
            cx,
        ));
    }
    // CDXC:Settings 2026-09-11 DECISION: dependent rows are hidden, not disabled, while their parent is off.
    // Their parent is the Spaces switch on the Extensions page now (built_in_extensions.rs), so they
    // sit as plain rows rather than indented under a row this page no longer has.
    if ghostex_settings_catalog::built_in_extensions::enabled_with(
        ghostex_settings_catalog::built_in_extensions::SPACES,
        |key| Some(g.values.bool(key)),
    ) {
        rows.extend(page.select(
            g,
            s,
            "sidebarSpaceSwitchBehavior",
            "When switching to a Space",
            "Reopen the session you last had open in a Space when you switch to it, in the view its project was in. If that session is closed, the one before it is used; a Space with nothing remembered opens its first project.",
            g.options("SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS"),
            Some(256.0),
            false,
            window,
            cx,
        ));
        rows.extend(page.toggle(
            g,
            s,
            "sidebarSpaceFollowActiveSession",
            "Switch Space when opening a session from another Space",
            "Switch the selected Space to the one that owns a session you open from outside it, such as through Back/Forward, Search by Prompt, a notification, or Previous Sessions.",
            false,
            cx,
        ));
    }
    rows.extend(page.slider(
        g,
        s,
        "projectSwitchKeepAliveMinutes",
        "Keep the last project running for (minutes)",
        "After you switch to another project or Space, keep the terminals, chats, and view that were open in the previous project running for this many minutes so switching back is instant. 0 releases them right away. Keep views loaded (Auto Sleep) keeps every side panel view until its Auto Sleep time instead.",
        (
            g.number("MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES"),
            g.number("MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES"),
            1.0,
        ),
        false,
        window,
        cx,
    ));
    // CDXC:Settings 2026-10-29 DECISION (wording Q23): Sidebar visibility memory, a select of Same in
    // every view / Remembered per view, is the switch Remember sidebar per view; the key keeps its
    // two values.
    if g.visible(s, "sidebarVisibilityMemory") {
        let per_view = g.values.string("sidebarVisibilityMemory") == "perView";
        rows.push(toggle_field_with(
            &g.p,
            "sidebarVisibilityMemory",
            g.spec(
                "sidebarVisibilityMemory",
                "Remember sidebar per view",
                "Off: hiding the sidebar hides it everywhere. On: Agents and the wide views (Browser, Code, Files, Kanban, Automate) each keep their own open or hidden sidebar; the Commands pane follows the same split.",
            ),
            per_view,
            Some(reset_key::<GeneralTab>("sidebarVisibilityMemory")),
            |page: &mut GeneralTab, checked, _window, cx| {
                save(
                    page,
                    "sidebarVisibilityMemory",
                    json!(if checked { "perView" } else { "shared" }),
                    cx,
                )
            },
            cx,
        ));
    }
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q13): the six card-detail switches are one Card
    // details row of chips; each chip still writes its own key, so presets keep working. Q18: the
    // Show Menu Bar Session Indicators row is gone with its setting.
    // Wording Q28: Show untracked lines without tracked changes moved here from Files & Links > Editor.
    const CARD_DETAILS: [(&str, &str, bool); 7] = [
        ("showProjectIcons", "Project icons", false),
        ("hideSessionAgentIconUntilHover", "Agent icon", true),
        ("hideBrowserFaviconUntilHover", "Browser favicon", true),
        ("hideLastActiveTimeOnSessionCards", "Last active time", true),
        ("hideProjectHeaderDiffStats", "Project git stats", true),
        (
            "showProjectEditorDiffFileCount",
            "Changed-file count",
            false,
        ),
        (
            "showUntrackedProjectDiffWhenNoTrackedChanges",
            "Untracked lines",
            false,
        ),
    ];
    if CARD_DETAILS.iter().any(|(key, _, _)| g.visible(s, key)) {
        let chips: Vec<Chip> = CARD_DETAILS
            .iter()
            .map(|(key, label, inverted)| Chip {
                id: key,
                label,
                on: g.values.bool(key) != *inverted,
            })
            .collect();
        let spec = g
            .spec(
                "showProjectIcons",
                "Card details",
                "What session and project cards show: project icons, the agent icon and the browser favicon (shown on hover when off), the last active time, the project's git stats, its changed-file count, and untracked lines in the git stats when the tracked diff is +0 -0.",
            )
            .modified(CARD_DETAILS.iter().any(|(key, _, _)| g.values.is_modified(key)));
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                for (key, _, _) in CARD_DETAILS {
                    store.reset_setting(key, cx);
                }
            });
        });
        rows.push(chips_field(
            &g.p,
            "sidebarCardDetails",
            spec,
            Some(reset),
            chips,
            |page: &mut GeneralTab, key, on, _window, cx| {
                let inverted = CARD_DETAILS
                    .iter()
                    .any(|(candidate, _, inverted)| *candidate == key && *inverted);
                save(page, key, json!(on != inverted), cx)
            },
            cx,
        ));
    }
    rows.extend(page.toggle(
        g,
        s,
        "highlightPendingQuestions",
        "Highlight unanswered questions",
        "Give sessions with a detected unanswered question a soft pink background, including while the agent keeps working. Currently supports Codex asynchronous questions.",
        false,
        cx,
    ));
    rows.extend(page.slider(
        g,
        s,
        "sidebarDefaultWidthPx",
        "Sidebar width on double-click",
        "Used when double-clicking the sidebar resize handle. App restart still restores your last manually set sidebar width.",
        (g.number("MIN_SIDEBAR_DEFAULT_WIDTH_PX"), g.number("MAX_SIDEBAR_DEFAULT_WIDTH_PX"), 1.0),
        false,
        window,
        cx,
    ));
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q13): one Animations row sets the panel slide
    // speed and the sidebar fold speed together (Off 0 ms, Slow 600, Normal 400, Fast 200).
    if g.any_visible(
        s,
        &["panelAnimationSpeed", "sidebarCollapseAnimationDurationMs"],
    ) {
        let spec = g
            .spec(
                "panelAnimationSpeed",
                "Animations",
                "How fast the sidebar, the side panel, the Agents Panel and the bottom or right panel slide open and closed, and how fast sidebar sections, groups and projects fold. Reduce Motion in your computer settings always turns them off.",
            )
            .modified(
                g.values.is_modified("panelAnimationSpeed")
                    || g.values.is_modified("sidebarCollapseAnimationDurationMs"),
            );
        let options = g.options("PANEL_ANIMATION_SPEED_OPTIONS");
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let value = g.values.choice("panelAnimationSpeed", &allowed);
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                store.reset_setting("panelAnimationSpeed", cx);
                store.reset_setting("sidebarCollapseAnimationDurationMs", cx);
            });
        });
        rows.push(segmented_field(
            &g.p,
            "panelAnimationSpeed",
            spec,
            Some(reset),
            &options,
            Some(value.as_str()),
            None,
            |page: &mut GeneralTab, next, _window, cx| {
                let fold_ms = match next.as_str() {
                    "off" => 0,
                    "slow" => 600,
                    "fast" => 200,
                    _ => 400,
                };
                let mut patch = Map::new();
                patch.insert("panelAnimationSpeed".to_string(), json!(next));
                patch.insert(
                    "sidebarCollapseAnimationDurationMs".to_string(),
                    json!(fold_ms),
                );
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.apply_patch(patch, "settings:control", cx)
                });
            },
            cx,
        ));
    }
    rows.extend(page.toggle(g, s, "closeSidePanelWithLastTab", "Close side panel with its last tab", "Close the side panel when you close its last tab, instead of showing the Open a view picker.", false, cx));
    rows.extend(page.slider(
        g,
        s,
        "commandsPanelDefaultHeightPx",
        "Commands pane height on double-click",
        "Used when opening the command pane (F12 or sidebar) and when double-clicking its top resize rail.",
        (
            g.number("MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
            g.number("MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX"),
            1.0,
        ),
        false,
        window,
        cx,
    ));
    rows.extend(page.select(
        g,
        s,
        "commandsPanelSide",
        "Commands pane side",
        "Where terminal Actions and F12 open the command pane: below the workspace or as a column to its right.",
        g.options("COMMANDS_PANEL_SIDE_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q21): Auto-minimize Commands pane and Minimize
    // after are one select: Off, or how long the pane stays open after focus and the pointer leave.
    if g.any_visible(
        s,
        &[
            "commandsPanelAutoMinimize",
            "commandsPanelAutoMinimizeDelaySeconds",
        ],
    ) {
        let delay = g
            .values
            .number_string("commandsPanelAutoMinimizeDelaySeconds");
        let mut options = vec![SettingOption {
            label: "Off".to_string(),
            value: "off".to_string(),
        }];
        options.extend(g.options("COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS"));
        if !options.iter().any(|option| option.value == delay) {
            options.insert(
                1,
                SettingOption {
                    label: format!("After {delay} seconds"),
                    value: delay.clone(),
                },
            );
        }
        let value = if g.values.bool("commandsPanelAutoMinimize") {
            delay
        } else {
            "off".to_string()
        };
        let spec = g
            .spec(
                "commandsPanelAutoMinimize",
                "Auto-minimize Commands pane",
                "Minimize the Commands pane this long after you move focus elsewhere and the pointer leaves it. Commands keep running. Off keeps the pane open.",
            )
            .modified(
                g.values.is_modified("commandsPanelAutoMinimize")
                    || g.values.is_modified("commandsPanelAutoMinimizeDelaySeconds"),
            );
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                store.reset_setting("commandsPanelAutoMinimize", cx);
                store.reset_setting("commandsPanelAutoMinimizeDelaySeconds", cx);
            });
        });
        rows.push(select_field(
            page,
            &g.p,
            "commandsPanelAutoMinimize",
            spec,
            Some(reset),
            &options,
            &value,
            None,
            |page: &mut GeneralTab, next, _window, cx| {
                let mut patch = Map::new();
                if next == "off" {
                    patch.insert("commandsPanelAutoMinimize".to_string(), json!(false));
                } else if let Ok(seconds) = next.parse::<f64>() {
                    patch.insert("commandsPanelAutoMinimize".to_string(), json!(true));
                    patch.insert(
                        "commandsPanelAutoMinimizeDelaySeconds".to_string(),
                        json!(seconds.round() as i64),
                    );
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
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q14): the old tab strip's Hide New Terminal and
    // Hide New Browser Tab rows are gone with it.
    // The sidebar's Sort & Filter menu writes the same setting (CDXC:Sidebar 2026-10-05 in gx-core
    // sidebar_view/ordering.rs `is_grouped_working`).
    rows.extend(page.toggle(
        g,
        s,
        "groupWorkingSessions",
        "Group working sessions",
        "Move sessions into a collapsed Working section under their project while their agent works; they return to Sessions when it stops or needs you. Also in the sidebar's Sort & Filter menu.",
        false,
        cx,
    ));
    // The Project sidebar's own; the Unified sidebar lists sessions, so it has no empty projects.
    if g.values.string("sidebarView") != "unified" {
        rows.extend(page.toggle(
            g,
            s,
            "sidebarHideProjectsWithoutSessions",
            "Hide projects without sessions",
            "Leave projects with no sessions out of the Project sidebar, whatever state a session is in; the open project always stays, and a Show all projects row at the end of the list brings them back. Also in the sidebar's Sort & Filter menu.",
            false,
            cx,
        ));
    }
    // CDXC:Sidebar 2026-06-16-18:19: agentManagerZoomPercent is labelled Sidebar Interface Size.
    rows.extend(page.slider(
        g,
        s,
        "agentManagerZoomPercent",
        "Sidebar zoom (%)",
        "Scale the sidebar interface.",
        (50.0, 200.0, 1.0),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "createSessionOnSidebarDoubleClick",
        "Double-click empty sidebar space to create a session",
        "Create a session from empty sidebar space.",
        false,
        cx,
    ));
    rows.extend(page.toggle(g, s, "enableSessionParking", "Enable session parking", "Add Park to session menus and move parked sessions into a collapsible section at the bottom of the sidebar.", false, cx));
    if g.values.bool("enableSessionParking") {
        rows.extend(page.toggle(
            g,
            s,
            "sleepSessionWhenParking",
            "Sleep session when parking",
            "Sleep a session through its normal lifecycle immediately after it is parked.",
            true,
            cx,
        ));
    }
    // CDXC:Settings 2026-09-12 DECISION: Park & Snooze with tags has the ↳ prefix and stays available while parking is off.
    rows.extend(page.toggle(
        g,
        s,
        "showTagMenuWhenParking",
        "Ask for a tag when parking or snoozing",
        "Open the Tag as menu when a session is parked or snoozed so it can be tagged right away.",
        true,
        cx,
    ));
    if g.values.bool("enableSessionParking") {
        rows.extend(page.toggle(g, s, "unparkAfterSendingMessage", "Unpark after sending a message", "Move a parked session out of the Parked section when you send it a message from chat or its terminal.", true, cx));
    }
    if g.visible(s, "renameSessionOnDoubleClick") {
        let label = settings_catalog().text(
            module::SETTINGS_TYPES,
            "RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL",
        );
        let subtitle = settings_catalog().text(
            module::SETTINGS_TYPES,
            "RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE",
        );
        let spec = g
            .spec(
                "renameSessionOnDoubleClick",
                &label,
                "Rename sessions directly from their cards.",
            )
            .subtitle(subtitle);
        rows.push(toggle_field_with(
            &g.p,
            "renameSessionOnDoubleClick",
            spec,
            g.values.bool("renameSessionOnDoubleClick"),
            Some(reset_key::<GeneralTab>("renameSessionOnDoubleClick")),
            |page: &mut GeneralTab, checked, _window, cx| {
                save(page, "renameSessionOnDoubleClick", json!(checked), cx)
            },
            cx,
        ));
    }
    settings_section(&g.p, "Sidebar", None, None, rows)
        .map(|section| PageBlock::section("sidebar", section))
}

fn session_cards_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("sessionCards", g.power_visible) {
        return None;
    }
    let s = "sessionCards";
    let mut rows: Vec<AnyElement> = Vec::new();
    // CDXC:Sidebar 2026-10-31 DECISION: User (Q846): "we show the strip related to the currently active sidebar in the settings. and we have a button that allows copying from the other view (just icon button pls with tooltip)".
    let unified = g.values.string("sidebarView") == "unified";
    let strip = if unified {
        HoverStripKind::Unified
    } else {
        HoverStripKind::Project
    };
    if g.visible(s, strip.key()) {
        let value = g.values.value(strip.key());
        let items = normalize_hover_buttons(&value);
        let spec = g
            .spec(
                strip.key(),
                "Session hover buttons (click to toggle, drag to reorder)",
                strip.description(),
            )
            .modified(!hover_buttons_are_default(&items, strip));
        let other = g.values.value(strip.other().key());
        rows.push(hover_actions_field(
            page,
            &g.p,
            spec,
            Some(reset_key::<GeneralTab>(strip.key())),
            &value,
            strip,
            &other,
            cx,
        ));
    }
    rows.extend(page.select(
        g,
        s,
        "sidebarSessionLinks",
        "Session links in the sidebar",
        "How a session card shows the pull requests, issues and Linear tickets it is linked to. Always: a line of them under every card that has any. Selected session: that line only under the session you have open. Badge: a small count on the card, coloured by the most pressing one, grey once all are merged or closed. Open work only: a line of the ones still open, none once they are all merged or closed. Hovering a card with links shows them all in a card beside the sidebar, except with Off, which shows nothing.",
        g.options("SIDEBAR_SESSION_LINKS_OPTIONS"),
        None,
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "titleSessionsByBranch",
        "Title sessions by their branch",
        "Name a session after the branch it works on, without your name and the ticket number (yahia/spx-1245-copy-link shows as copy-link), until you rename it yourself. Sessions on main keep their own title.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "offerCleanupWhenPrMerges",
        "Offer Clean up when a PR merges",
        "When a session's pull request is merged, its card asks once whether to clean up (remove the session's worktree and park the session; a worktree with uncommitted changes is kept) or keep it.",
        false,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "showSessionCardHoverButtonsInContextMenu",
        "Hover buttons also in context menu",
        "Also list the enabled hover buttons in the session right-click menu, in their usual place in the menu. Close is never listed while it is on the card. Turn off to leave every button out of the menu once it is on the card.",
        false,
        cx,
    ));
    // CDXC:Settings 2026-10-29 DECISION (cleanup Q13): Session tooltips and Tooltip Delay are one
    // select: Off, or the delay before the tooltip shows.
    if g.any_visible(s, &["showSidebarSessionTooltips", "sidebarTooltipDelayMs"])
        || (g.search.is_searching() && g.visible("sidebar", "sidebarTooltipDelayMs"))
    {
        let delay = g.values.f64("sidebarTooltipDelayMs").round().max(0.0) as i64;
        let mut options: Vec<SettingOption> = [
            ("Off", "off"),
            ("Instant", "0"),
            ("After 300 ms", "300"),
            ("After 600 ms", "600"),
            ("After 1 second", "1000"),
            ("After 2 seconds", "2000"),
        ]
        .into_iter()
        .map(|(label, value)| SettingOption {
            label: label.to_string(),
            value: value.to_string(),
        })
        .collect();
        if !options
            .iter()
            .any(|option| option.value == delay.to_string())
        {
            options.insert(
                2,
                SettingOption {
                    label: format!("After {delay} ms"),
                    value: delay.to_string(),
                },
            );
        }
        let value = if g.values.bool("showSidebarSessionTooltips") {
            delay.to_string()
        } else {
            "off".to_string()
        };
        let spec = g
            .spec(
                "showSidebarSessionTooltips",
                "Session tooltips",
                "Show a tooltip with the full session name when you hover a session in the sidebar, after this delay. Off shows none; the other sidebar tooltips stay and wait the same delay.",
            )
            .modified(
                g.values.is_modified("showSidebarSessionTooltips")
                    || g.values.is_modified("sidebarTooltipDelayMs"),
            );
        let reset: PageAction<GeneralTab> = Rc::new(|page: &mut GeneralTab, _window, cx| {
            let store = page.store.clone();
            store.update(cx, |store, cx| {
                store.reset_setting("showSidebarSessionTooltips", cx);
                store.reset_setting("sidebarTooltipDelayMs", cx);
            });
        });
        rows.push(select_field(
            page,
            &g.p,
            "showSidebarSessionTooltips",
            spec,
            Some(reset),
            &options,
            &value,
            None,
            |page: &mut GeneralTab, next, _window, cx| {
                let mut patch = Map::new();
                if next == "off" {
                    patch.insert("showSidebarSessionTooltips".to_string(), json!(false));
                } else if let Ok(delay) = next.parse::<i64>() {
                    patch.insert("showSidebarSessionTooltips".to_string(), json!(true));
                    patch.insert("sidebarTooltipDelayMs".to_string(), json!(delay));
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
    settings_section(&g.p, "Session Cards", None, None, rows)
        .map(|section| PageBlock::section("sessionCards", section))
}

fn sidebar_tags_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("sidebarTags", g.power_visible) {
        return None;
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible("sidebarTags", "sidebarSessionTagListItems") {
        let value = g.values.value("sidebarSessionTagListItems");
        rows.push(card_inset(tag_list_field(
            page,
            &g.p,
            "Reorder, hide, or disable sidebar tag filters and separators.",
            &value,
            g.create_tag,
            reset_key::<GeneralTab>("sidebarSessionTagListItems"),
            window,
            cx,
        )));
    }
    settings_section(&g.p, SharedString::from("Sidebar Tags"), None, None, rows)
        .map(|section| PageBlock::section("sidebarTags", section))
}

pub(super) fn sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        sidebar_section(page, g, window, cx),
        session_cards_section(page, g, window, cx),
        sidebar_tags_section(page, g, window, cx),
        // CDXC:Settings 2026-10-29 (model.rs): Sleeping Sessions left the Advanced group for this page.
        super::system::sleeping_sessions_section(page, g, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}
