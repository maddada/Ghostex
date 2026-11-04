use crate::data::*;
use crate::json::opt;
use crate::json::J;
use crate::rows::{option, row, section, Section, SettingOption};

pub(crate) fn session_cards() -> Section {
    section(
        "sessionCards",
        "Session Cards",
        vec![
            // CDXC:Sessions 2026-05-15-19:46:
            // Settings must not expose the card-hotkey visibility row; session-card shortcut visibility is no longer configurable from the modal.
            row("sessionCardHoverButtons", "Session hover buttons (click to toggle, drag to reorder)", "Buttons a session card shows when you hover it in the Project sidebar. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked. By default the strip is Tag, Park, Sleep, chevron, Close. The button beside the strip copies the Unified sidebar's. Shown while the Project sidebar is selected."),
            row("unifiedSessionCardHoverButtons", "Session hover buttons (click to toggle, drag to reorder)", "Buttons a session card shows when you hover it in the Unified sidebar, which keeps its own strip. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked, and opening the chevron opens it on every card. By default the strip is Tag, Sleep, chevron, Snooze, Park, Close. The button beside the strip copies the Project sidebar's. Shown while the Unified sidebar is selected."),
            row("sidebarSessionLinks", "Session links in the sidebar", "How a session card shows the pull requests, issues and Linear tickets it is linked to. Always: a line of them under every card that has any. Selected session: that line only under the session you have open. Badge: a small count on the card, coloured by the most pressing one, grey once all are merged or closed. Open work only: a line of the ones still open, none once they are all merged or closed. Hovering a card with links shows them all in a card beside the sidebar, except with Off, which shows nothing.").options(SIDEBAR_SESSION_LINKS_OPTIONS),
            row("titleSessionsByBranch", "Title sessions by their branch", "Name a session after the branch it works on, without your name and the ticket number (yahia/spx-1245-copy-link shows as copy-link), until you rename it yourself. Sessions on main keep their own title."),
            row("offerCleanupWhenPrMerges", "Offer Clean up when a PR merges", "When a session's pull request is merged, its card asks once whether to clean up (remove the session's worktree and park the session; a worktree with uncommitted changes is kept) or keep it."),
            row("showSessionCardHoverButtonsInContextMenu", "Hover buttons also in context menu", "Also list the enabled hover buttons in the session right-click menu, in their usual place in the menu. Close is never listed while it is on the card. Turn off to leave every button out of the menu once it is on the card."),
            row("showSidebarSessionTooltips", "Session tooltips", "Show a tooltip with the full session name when you hover a session in the sidebar. Turn off to show none; other sidebar tooltips stay."),
        ],
    )
}

pub(crate) fn sidebar() -> Section {
    section(
        "sidebar",
        "Sidebar",
        vec![
            row("sidebarSettingsPreset", "Preset", "Apply a sidebar UI preset or show Custom when controlled settings diverge.").options_of(SIDEBAR_SETTINGS_PRESETS, "label", "id").options(&[opt("Custom", "custom")]),
            row("sidebarView", "Sidebar", "Project sidebar lists each machine, Space and project on its own. Unified sidebar shows every session from every machine, Space and project in one list, with the ones that need you on top; the same repository on two machines is one project there.").options(SIDEBAR_VIEW_OPTIONS),
            row("unifiedWorkingSection", "Working section", "Fold sessions whose agent is working or still running something in the background into a Working section at the bottom of the list. They return to the top of the list when they need you. Shown while the Unified sidebar is selected."),
            row("unifiedGroupByProject", "Group by project", "Put every machine's sessions of one repository under one header, with its Working, Snoozed and Parked sections inside it. Drag the headers to arrange them. Drafts stay above the groups. Also in the sidebar's Sort & Filter menu. Shown while the Unified sidebar is selected."),
            row("sidebarSpaceSwitchBehavior", "When switching to a Space", "Reopen the session you last had open in a Space when you switch to it, in the view its project was in. Shown while the Spaces extension is on.").options(SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS),
            row("sidebarSpaceFollowActiveSession", "Switch Space when opening a session from another Space", "Switch the selected Space to the one that owns a session you open from outside it, such as through Back/Forward or Search by Prompt. Shown while the Spaces extension is on."),
            row("projectSwitchKeepAliveMinutes", "Keep the last project running for", "After you switch to another project or Space, keep the terminals, chats, and view that were open in the previous project running for this many minutes so switching back is instant. 0 releases them right away. Keep views loaded (Auto Sleep) keeps every side panel view until its Auto Sleep time instead."),
            // CDXC:Settings 2026-10-29 DECISION (wording Q23): shown as the switch Remember sidebar per view; Same in every view is off, Remembered per view is on.
            row("sidebarVisibilityMemory", "Remember sidebar per view", "Off: hiding the sidebar hides it everywhere. On: Agents and the wide views (Browser, Code, Files, Kanban, Automate) each keep their own open or hidden sidebar; the Commands pane follows the same split.").options(SIDEBAR_VISIBILITY_MEMORY_OPTIONS),
            // CDXC:Settings 2026-10-29 DECISION (cleanup Q13): the six card-detail switches are one Card
            // details row of chips in Settings; each chip still writes its own key. Q14: the tab strip's
            // Hide New Terminal / New Browser Tab rows are gone with the old tab strip, and Q18: Show Menu
            // Bar Session Indicators is gone (the macOS menu bar item is always on; remove it from the menu bar itself).
            row("showProjectIcons", "Card details: project icons", "Show project artwork or a square with the project’s first letter beside project names."),
            // CDXC:Settings 2026-06-30-22:22:
            // Search metadata follows the visible row order: preset-controlled rows
            // sit immediately after Preset, before independent sidebar sizing and
            // placement controls.
            row("hideSessionAgentIconUntilHover", "Card details: agent icon", "Hide session agent icons until a session row is hovered."),
            row("hideBrowserFaviconUntilHover", "Card details: browser favicon", "Hide browser page favicons until a session row is hovered."),
            row("hideLastActiveTimeOnSessionCards", "Card details: last active time", "Hide Last Active timestamps from session-card title rows."),
            row("highlightPendingQuestions", "Highlight unanswered questions", "Give sessions with a detected unanswered question a soft pink background, including while the agent keeps working. Currently supports Codex asynchronous questions."),
            row("hideProjectHeaderDiffStats", "Card details: project git stats", "Hide +added/-removed line counts in sidebar project rows."),
            row("showProjectEditorDiffFileCount", "Card details: changed-file count", "Show changed-file counts in sidebar project row git stats."),
            // Wording Q28: moved here from the Editor section of Files & Links.
            row("showUntrackedProjectDiffWhenNoTrackedChanges", "Card details: untracked lines", "Count untracked lines in a project's git stats when the tracked diff is +0 -0 (prompts like Starship ignore untracked lines)."),
            row("sidebarCollapseAnimationDurationMs", "Animations (fold speed, set with Animations)", "Set how quickly sidebar sections, groups, and projects expand or collapse, and how quickly the floating sidebar and Agents Panel slide in from the window edge. Set to 0 for no animation."),
            row("panelAnimationSpeed", "Animations", "Set how fast the sidebar, the side panel, the Agents Panel and the bottom or right panel slide open and closed: Off, Slow, Normal or Fast. Reduce Motion in your computer settings always turns it off.").options(PANEL_ANIMATION_SPEED_OPTIONS),
            row("closeSidePanelWithLastTab", "Close side panel with its last tab", "Close the side panel when you close its last tab, instead of showing the Open a view picker."),
            row("sidebarTooltipDelayMs", "Session tooltips delay (set with Session tooltips)", "Set how long sidebar hover labels wait before appearing. Set to 0 to show them immediately."),
            row("sidebarDefaultWidthPx", "Sidebar width on double-click", "Width restored when double-clicking the sidebar resize handle."),
            row("commandsPanelDefaultHeightPx", "Commands pane height on double-click", "Height used when opening the command pane and when double-clicking its top resize rail."),
            row("commandsPanelSide", "Commands pane side", "Dock the command pane below the workspace or to its right.").options(COMMANDS_PANEL_SIDE_OPTIONS),
            row("commandsPanelAutoMinimize", "Auto-minimize Commands pane", "Minimize the Commands pane this long after you move focus elsewhere and the pointer leaves it. Commands keep running. Off keeps the pane open."),
            row("commandsPanelAutoMinimizeDelaySeconds", "Minimize after (set with Auto-minimize Commands pane)", "How long the Commands pane stays open after focus and the pointer leave it.").num_options(COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS),
            row("sidebarHideProjectsWithoutSessions", "Hide projects without sessions", "Leave projects with no sessions out of the Project sidebar, whatever state a session is in; the open project always stays, and a Show all projects row at the end of the list brings them back. Also in the sidebar's Sort & Filter menu."),
            row("groupWorkingSessions", "Group working sessions", "Move sessions into a collapsed Working section under their project while their agent works; they return to Sessions when it stops or needs you. Also in the sidebar's Sort & Filter menu."),
            row("agentManagerZoomPercent", "Sidebar zoom (%)", "Scale the sidebar interface."),
            row("createSessionOnSidebarDoubleClick", "Double-click empty sidebar space to create a session", "Create a session from empty sidebar space."),
            row("enableSessionParking", "Enable session parking", "Move deferred sessions into a collapsible Parked section at the bottom of the sidebar."),
            row("sleepSessionWhenParking", "Sleep session when parking", "Sleep a session automatically when it is moved into the Parked section."),
            row("showTagMenuWhenParking", "Ask for a tag when parking or snoozing", "Open the Tag as menu when a session is parked or snoozed so it can be tagged right away."),
            row("unparkAfterSendingMessage", "Unpark after sending a message", "Move a parked session out of the Parked section when you send it a message."),
            row("renameSessionOnDoubleClick", "Double-click session cards to rename", "Makes clicking on a session respond a bit slower so we can detect the double click"),
        ],
    )
}

pub(crate) fn sidebar_tags() -> Section {
    section(
        "sidebarTags",
        "Sidebar Tags",
        vec![
            row("sidebarSessionTagListItems", "Tag filter list", "Add your own tags, then reorder, hide, disable, or delete tags and their separators for the sidebar and the Tag as menu.").option_list(tag_list_item_options()).options(&[opt("Hide tag", "hide"), opt("Disable tag", "disable"), opt("Reorder tags", "reorder")]),
        ],
    )
}

/// `getSidebarSessionTagListItemLabel` over the default tag list: a tag's label, "No tag", or "Separator".
fn tag_list_item_options() -> Vec<SettingOption> {
    DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS
        .as_array()
        .iter()
        .map(|item| {
            let id = item.get("id").and_then(J::as_str).unwrap_or_default();
            let label = match item.get("type").and_then(J::as_str) {
                Some("tag") => {
                    let tag = item.get("tag").and_then(J::as_str).unwrap_or_default();
                    SIDEBAR_SESSION_TAG_OPTIONS
                        .iter()
                        .find(|option| option.value == tag)
                        .map_or(tag, |option| option.label)
                }
                Some("untagged") => "No tag",
                _ => "Separator",
            };
            option(label, id)
        })
        .collect()
}
