//! Power User Flags: the Settings rows that only people on Experience › Advanced see. Each page
//! that has some keeps them together in a collapsed "Power User Flags" section at its bottom (the
//! desktop's fold row, `settings_modal/fields/advanced.rs`); the rows never move to another page.
//! The section is hidden unless the saved Experience level is Advanced, or one of that page's
//! flags differs from its default, so nobody loses a setting they changed. Settings search finds
//! a flag only while its page's section shows. The desktop, `ghostex settings` and the Help files
//! read these tables; the phone keeps its own Settings > Advanced page.
//!
//! CDXC:Settings 2026-11-27 DECISION:
//! User: "ok i changed my mind, i don't want advanced flag like this as a new page.. i think let's
//! just hide settings from the advanced area of each page we currently have unless the user has
//! "Advanced" mode enabled. my picks above is everything that should be set to that basically but
//! no moving stuff. call it "Power User Flags" not advanced settings". The picks are the rows of
//! the Advanced Flags research page the user ticked, each kept on the page it lives on today. Rows
//! of the old Advanced folds that were not picked show with the page's other rows. What turns it
//! on: picking Advanced in Settings > Experience (picking Chill or Regular turns it off), saved as
//! `experienceLevel`. People who already changed a flag: "that page's Power User Flags section
//! still shows". Coordinators: "let's not add coordinator tab please keep all coordinator stuff in
//! the coordinator page", so no coordinator row is a flag. Phone: desktop first, no phone change.
//! This supersedes the Advanced fold of CDXC:Settings 2026-10-29 (data/sections.rs).
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/search.rs (`RowTier`), store.rs
//! (`power_user_flags_shown`), tabs/general/experience/page.rs (saving the level).

use serde_json::Value;

/// The section's title on every page.
pub const SECTION_TITLE: &str = "Power User Flags";

/// The Experience level the user last picked for the whole app (`chill`, `regular` or
/// `advanced`); empty until they pick one. Advanced shows every page's Power User Flags.
pub const LEVEL_KEY: &str = "experienceLevel";

/// One Settings page's flags. `page` is the page's tab id (`SettingsTabId::id` on the desktop).
#[derive(Clone, Copy, Debug)]
pub struct PageFlags {
    pub page: &'static str,
    pub keys: &'static [&'static str],
}

/// Every page's Power User Flags, in rail order.
pub const PAGES: &[PageFlags] = &[
    PageFlags {
        page: "theme",
        keys: &["windowGlassMenuBlurRadius"],
    },
    PageFlags {
        page: "textFonts",
        keys: &[
            "terminalBoldUsesHeavierFont",
            "sessionChatTextStrengthPercent",
            "sessionChatMutedThinking",
        ],
    },
    PageFlags {
        page: "sizeZoom",
        keys: &["sidebarDefaultWidthPx", "commandsPanelDefaultHeightPx"],
    },
    PageFlags {
        page: "windowLayout",
        keys: &[
            "showWindowButtonsWhenSidebarCollapsed",
            "workspacePaneGap",
            "sidebarCollapseAnimationDurationMs",
            "closeSidePanelWithLastTab",
        ],
    },
    PageFlags {
        page: "chat",
        keys: &[
            "chatCopyOnSelect",
            "sessionChatCustomTranscriptWidthEnabled",
            "sessionChatTranscriptWidthPercent",
            "chatComposerMatchesTranscriptWidth",
            "chatMessageLayout",
            "chatUserMessageStyle",
            "chatMessageSpacing",
            "chatVariedWorkingWords",
            "sessionChatKeepComposerExpanded",
            "sessionChatConfirmEscapeInterrupt",
            "chatSendOnPick",
            "chatAnswersWaitForIdle",
            "chatInboxReplyArrow",
        ],
    },
    PageFlags {
        page: "sidebar",
        keys: &[
            "sidebarMergeRepositoryCopies",
            "sidebarVisibilityMemory",
            "highlightPendingQuestions",
            "showUntrackedProjectDiffWhenNoTrackedChanges",
            "showSidebarSessionTooltips",
            "sidebarTooltipDelayMs",
            "worktreeSessionsInProject",
            "sidebarHideDisconnectedMachines",
            "hideComputerIconUntilHover",
            "markComputerYouAreOn",
            "pinnedSessionOrder",
            "createSessionOnSidebarDoubleClick",
            "closeEmptySessionsOnNew",
            "renameSessionOnDoubleClick",
            "sidebarSpaceFollowActiveSession",
            "sessionCardHoverButtons",
            "unifiedSessionCardHoverButtons",
            "titleSessionsByBranch",
            "showSessionCardHoverButtonsInContextMenu",
            "sessionsBoldCoordinatorTitle",
            "dimSleepingSessions",
            "dimInactiveProjects",
            "wakeSleepingSessionsOnSelect",
        ],
    },
    PageFlags {
        page: "terminal",
        keys: &[
            "ghosttySettingsActions",
            "terminalViewWidthMode",
            "terminalViewWidthPercent",
            "terminalWidthApplyToCommandPaneTerminals",
            "terminalPaneHorizontalPaddingPx",
            "terminalPaneVerticalPaddingPx",
            "terminalCursorStyleBlink",
            "clickToWakeSleepingSessions",
            "showQuickModelPickerInTerminal",
            "showSessionIdInTerminalPanes",
            "commandsPanelAutoMinimize",
            "commandsPanelAutoMinimizeDelaySeconds",
            "promptEditorBackend",
            "terminalScrollbackLimitMb",
            "terminalCopyOnSelect",
            "terminalConfirmCloseSurface",
            "terminalClipboardTrimTrailingSpaces",
            "terminalClipboardPasteProtection",
            "terminalPastePreviewableImages",
            "terminalMouseHideWhileTyping",
            "terminalScrollbar",
            "terminalMouseScrollMultiplierPrecision",
            "terminalMouseScrollMultiplierDiscrete",
            "terminalScrollToBottomWhenTyping",
        ],
    },
    PageFlags {
        page: "notifications",
        keys: &[
            "resetExpirySystemNotifications",
            "actionCompletionSound",
            "copySound",
        ],
    },
    PageFlags {
        page: "tools",
        keys: &[
            "changesHideTestFiles",
            "docsOpenFilesAsTabs",
            "codeServerLinkVscodeUserConfig",
            "codeServerUseVscodeInsidersUserConfig",
            "codeServerNewWindowsAsTabs",
            "terminalDevServerIgnoredPortRules",
        ],
    },
    PageFlags {
        page: "browser",
        keys: &["reloadBrowserTabsOnStartup", "autoSleepBrowserIdleMinutes"],
    },
    PageFlags {
        page: "system",
        keys: &[
            "autoSleepCodeEditorIdleMinutes",
            "autoSleepGitEditorIdleMinutes",
            "autoSleepProjectEditorIdleMinutes",
            "keepViewsLoaded",
            "projectSwitchKeepAliveMinutes",
            "autoSleepRequireAgentResumeCommand",
            "autoSleepFavoriteAgentSessions",
            "hideKeepAwakeTitlebarControl",
            "keepAwakeAllowDisplaySleep",
            "keepAwakePreventLidSleep",
            "keepAwakeActivateOnLaunch",
            "keepAwakeActivateOnExternalDisplay",
            "keepAwakeBatteryThresholdPercent",
            "keepAwakeDeactivateOnLowPowerMode",
            "keepAwakeDeactivateOnUserSwitch",
        ],
    },
    PageFlags {
        page: "hotkeys",
        keys: &[
            "expandCollapsedProjectsOnJump",
            "sidebarSessionCycleSkipsSleeping",
        ],
    },
    PageFlags {
        page: "agents",
        keys: &[
            "claudeOneHourPromptCache",
            "piMessagesWhileWorking",
            "customTitleCommand",
            "agentGhostexNote",
            "sessionDetailsClaudeTaskTools",
            "agentHooksAutoInstall",
            "readAgentLiveStatus",
            "useAgentHooks",
        ],
    },
    PageFlags {
        page: "accounts",
        keys: &[
            "screenshotMode",
            "hideAccountEmails",
            "claudeAutoRedeemResetsAtLimit",
            "codexAutoRedeemResetsAtLimit",
        ],
    },
    PageFlags {
        page: "remote",
        keys: &[
            "easyConnectRelay",
            "servedPorts",
            "allowedClientKeys",
            "pairingAddress",
            "binary",
            "gxserver",
            "rawStatus",
        ],
    },
    PageFlags {
        page: "debugging",
        keys: &["debuggingMode", "diagnosticLogging"],
    },
];

/// Whether `key` is a Power User Flag on any page.
pub fn is_flag(key: &str) -> bool {
    PAGES.iter().any(|page| page.keys.contains(&key))
}

/// The page whose Power User Flags hold `key`.
pub fn page_of(key: &str) -> Option<&'static str> {
    PAGES
        .iter()
        .find(|page| page.keys.contains(&key))
        .map(|page| page.page)
}

/// Page `page`'s flags (empty for a page with none).
pub fn keys_of(page: &str) -> &'static [&'static str] {
    PAGES
        .iter()
        .find(|flags| flags.page == page)
        .map_or(&[], |flags| flags.keys)
}

/// Whether the saved Experience level (`value(LEVEL_KEY)`) is Advanced.
pub fn level_is_advanced(value: &dyn Fn(&str) -> Value) -> bool {
    value(LEVEL_KEY).as_str() == Some("advanced")
}

/// Whether page `page` shows its Power User Flags section: on Advanced, or while one of its flags
/// differs from its default (`is_modified`).
pub fn page_shown(page: &str, advanced: bool, is_modified: &dyn Fn(&str) -> bool) -> bool {
    advanced || keys_of(page).iter().any(|key| is_modified(key))
}

/// The flags of every page whose section is hidden: Settings search and the pages leave them out.
pub fn hidden_keys(advanced: bool, is_modified: &dyn Fn(&str) -> bool) -> Vec<&'static str> {
    if advanced {
        return Vec::new();
    }
    PAGES
        .iter()
        .filter(|flags| !page_shown(flags.page, false, is_modified))
        .flat_map(|flags| flags.keys.iter().copied())
        .collect()
}
