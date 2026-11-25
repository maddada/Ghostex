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
            row("sessionCardHoverButtons", "Session hover buttons (click to toggle, drag to reorder)", "Buttons a session card shows when you hover it in the Project sidebar. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked. By default none are shown on hover. The button beside the strip copies the Unified sidebar's. Shown while the Project sidebar is selected."),
            row("unifiedSessionCardHoverButtons", "Session hover buttons (click to toggle, drag to reorder)", "Buttons a session card shows when you hover it in the Unified sidebar, which keeps its own strip. Click an icon to turn it on or off; drag icons to reorder them. Buttons to the right of the chevron always show, buttons to its left hide until the chevron is clicked, and opening the chevron opens it on every card. By default the strip is Tag, Sleep, chevron, Snooze, Park, Close. The button beside the strip copies the Project sidebar's. Shown while the Unified sidebar is selected."),
            row("sidebarSessionLinks", "Session links in the sidebar", "How a session card shows the pull requests, issues and Linear tickets it is linked to. Always: a line of them under every card that has any. Selected session: that line only under the session you have open. Badge: a small count on the card, coloured by the most pressing one, grey once all are merged or closed. Open work only: a line of the ones still open, none once they are all merged or closed. Hovering a card with links shows them all in a card beside the sidebar, except with Off, which shows nothing.").options(SIDEBAR_SESSION_LINKS_OPTIONS),
            row("titleSessionsByBranch", "Title sessions by their branch", "Name a session after the branch it works on, without your name and the ticket number (yahia/spx-1245-copy-link shows as copy-link), until you rename it yourself. Sessions on main keep their own title."),
            row("offerCleanupWhenPrMerges", "Offer Clean up when a PR merges", "When a session's pull request is merged, its card asks once whether to clean up (remove the session's worktree and park the session; a worktree with uncommitted changes is kept) or keep it."),
            row("cleanUpMergedThreads", "Clean up merged threads by themselves", "A coordinator's thread whose pull request is merged has its worktree removed and goes to sleep by itself, but only when the worktree has no uncommitted or unpushed changes and the thread neither works nor waits on you. The coordinator's banner says which ones were cleaned up, with Undo; the rest wait in the banner for Clean up all or Keep."),
            row("showSessionCardHoverButtonsInContextMenu", "Hover buttons also in context menu", "Also list the enabled hover buttons in the session right-click menu, in their usual place in the menu. Close is never listed while it is on the card. Turn off to leave every button out of the menu once it is on the card."),
            row("showSidebarSessionTooltips", "Session tooltips", "Show a tooltip with the full session name when you hover a session in the sidebar. Turn off to show none; other sidebar tooltips stay."),
            row("sessionsBoldCoordinatorTitle", "Bold coordinator title", "Draw each coordinator's title in bold in the sidebar. Off, a coordinator stands apart from its threads by its crown, its brighter title and its threads' dimmer titles."),
        ],
    )
}

/// CDXC:Spaces 2026-11-10 DECISION (spaces.rs): Spaces is a normal setting, its own section on
/// Sidebar & Sessions right under Sidebar, with its switch first and the two Space rows it shows.
pub(crate) fn spaces() -> Section {
    section(
        "spaces",
        "Spaces",
        vec![
            row("sidebarSpacesEnabled", "Spaces", "Group projects into Spaces and switch between them from a row of icons at the top of the sidebar, or by swiping. On by default; your saved Spaces are kept while it is off."),
            row("sidebarActiveProjects", "Show the Active projects star in the Spaces bar", "Add a star tab at the start of the Spaces bar that pulls together every project you are currently working on across all Spaces (any project with a busy or waiting session, or used in the last 8 hours), so you can switch between them without changing Spaces. Right-click the star to hide it."),
            row("sidebarSpaceSwitchBehavior", "When switching to a Space", "Reopen the session you last had open in a Space when you switch to it, in the view its project was in. Shown while Spaces is on.").options(SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS),
            row("sidebarSpaceFollowActiveSession", "Switch Space when opening a session from another Space", "Switch the selected Space to the one that owns a session you open from outside it, such as through Back/Forward or Search by Prompt. Shown while Spaces is on."),
        ],
    )
}

pub(crate) fn sidebar() -> Section {
    section(
        "sidebar",
        "Sidebar",
        vec![
            row("sidebarSettingsPreset", "Preset", "Apply a sidebar UI preset or show Custom when controlled settings diverge.").options_of(SIDEBAR_SETTINGS_PRESETS, "label", "id").options(&[opt("Custom", "custom")]),
            row("sidebarView", "Sidebar", "Project sidebar lists each project with its sessions under it. Unified sidebar shows every session in one list, with the ones that need you on top. Both show every connected computer together unless Computers is set to A tab each.").options(SIDEBAR_VIEW_OPTIONS),
            row("sidebarLayout", "Sidebar layout", "Rail adds a narrow column of icons at the window's left edge for Search, Main Hub, Agents Hub, All Automations, Hermes, Claude and Grok Bot, with everything else under its ⋯ menu, where you pin, unpin and drag icons into your own order, Toggle sidebar at its top and the account usage switch at its foot. It stays when the sidebar is hidden. Classic is the sidebar without the rail, with Toggle sidebar, Search and the bell in its top row.").options(SIDEBAR_LAYOUT_OPTIONS),
            row("sidebarSeparateByMachine", "Computers", "One list (the default): every connected computer in one list, where a project on several computers is one entry with the sessions of all of them, and Spaces and project groups are shared by all of them. A tab each: one computer at a time, each with its own tab, project list and Spaces."),
            // CDXC:RemoteMachines 2026-11-09 DECISION in gx-core sidebar_view/merged.rs: on by default.
            row("sidebarMergeRepositoryCopies", "Merge copies of the same repository", "Show two folders on this computer that are copies of the same repository (the same git remote, opened at the same folder inside it) as one project, with the sessions of both. Turn off to give each folder its own project with only its own sessions. Copies on different computers are one project either way. The phone follows this. Shown while Computers is set to One list."),
            row("unifiedWorkingSection", "Group busy sessions in a Moving section", "In the Unified sidebar, move sessions whose agent is currently working (or running background tasks) into a collapsible Moving section at the bottom, so the top stays focused on sessions that need you. They jump back up when done or waiting for you. Shown while the Unified sidebar is selected."),
            row("unifiedGroupByProject", "Group by project", "Put every machine's sessions of one repository under one header, with its Moving, Snoozed and Parked sections inside it. Drag the headers to arrange them. Drafts stay above the groups. Also in the sidebar's Sort & Filter menu. Shown while the Unified sidebar is selected."),
            // CDXC:Sidebar 2026-11-17 DECISION in gx-core unified_sidebar/build.rs `unified_session_cmp`: Recent activity by default.
            row("unifiedSessionOrder", "Session order", "Order the Unified sidebar's sessions by recent activity (a session moves to the top when you send it a message and when it finishes or needs you), alphabetically by title, or by when each was created, newest first. Alphabetical and Created never move a session when it works or finishes; turn off the Moving section too for a list that never moves. Pinned sessions follow Pinned order. Applies to the phone too. Also in the sidebar's Sort & Filter menu. Shown while the Unified sidebar is selected.").options(UNIFIED_SESSION_ORDER_OPTIONS),
            // CDXC:Settings 2026-10-29 DECISION (wording Q23): shown as the switch Remember sidebar per view; Same in every view is off, Remembered per view is on.
            row("sidebarVisibilityMemory", "Remember sidebar per view", "Off: hiding the sidebar hides it everywhere. On: Agents and the wide views (Browser, Code, Files, Beads, Automate) each keep their own open or hidden sidebar; the Commands pane follows the same split.").options(SIDEBAR_VISIBILITY_MEMORY_OPTIONS),
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
            row("sidebarTooltipDelayMs", "Session tooltips delay (set with Session tooltips)", "Set how long sidebar hover labels wait before appearing. Set to 0 to show them immediately."),
            row("sidebarHideProjectsWithoutSessions", "Hide projects without sessions", "Leave projects with no sessions out of the Project sidebar, whatever state a session is in; the open project always stays, and a Show all projects row at the end of the list brings them back. Also in the sidebar's Sort & Filter menu."),
            // CDXC:Worktrees 2026-11-06 DECISION in gx-core sidebar_view/worktree_fold.rs: on by default, an advanced row.
            row("worktreeSessionsInProject", "Worktree sessions", "In their repository: a session started in a worktree is listed under its repository's project, with the worktree's branch on its row; the session still works in the worktree's folder. Own project: each worktree shows as its own project under its repository. Shown while the Project sidebar is selected."),
            // CDXC:RemoteMachines 2026-11-15 DECISION in gx-core sidebar_view/merged.rs: an advanced row, off by default.
            row("sidebarHideDisconnectedMachines", "Hide sessions and projects from computers that aren't connected", "Leave a computer that isn't connected out of the Project sidebar: its sessions leave every project, and a project only it has is hidden. They come back when it reconnects. Off: its last sessions stay, faded. The Unified sidebar always leaves it out. The phone follows this. Shown while Computers is set to One list."),
            // CDXC:RemoteMachines 2026-11-22 DECISION in apps/desktop native_sidebar/computer_mark.rs: on by default.
            row("hideComputerIconUntilHover", "Show computer icons only on hover", "Hide each computer's icon on sidebar project and session rows until you point at the row. Turn off to always show computer icons on rows while more than one computer is connected."),
            // CDXC:RemoteMachines 2026-11-20 DECISION in apps/desktop native_sidebar/computer_mark.rs: on by default.
            row("markComputerYouAreOn", "Mark the computer you're on", "Show a small faint dot under the icon of the computer you're on, like the macOS Dock's, wherever computer icons show (the sidebar's computer tiles, session rows and project headers, the chat's where pill, Session details and Settings > Remote), while another computer is connected. Turn off to show none."),
            // CDXC:Sidebar 2026-11-07 DECISION in gx-core sidebar_view/inputs.rs: an advanced row, Manual by default.
            row("pinnedSessionOrder", "Pinned order", "Keep pinned sessions in the order you drag them into, or put the pinned session that came back to you last (started, sent a message, or finished a turn) first. With Most recently updated, pinned sessions can't be dragged to rearrange them. Applies to both sidebars and the phone. Also in the sidebar's Sort & Filter menu.").options(PINNED_SESSION_ORDER_OPTIONS),
            row("groupWorkingSessions", "Group working sessions", "Move sessions into a collapsed Working section under their project while their agent works; they return to Sessions when it stops or needs you. Also in the sidebar's Sort & Filter menu."),
            row("createSessionOnSidebarDoubleClick", "Double-click empty sidebar space to create a session", "Create a session from empty sidebar space."),
            // CDXC:Settings 2026-11-11 DECISION in apps/desktop settings_modal/model.rs (`RailTier`, Q2511): moved here from Chat; it closes sessions in the sidebar's list.
            row("closeEmptySessionsOnNew", "Close empty sessions when starting a new one", "When you start a new session, close this project's other sessions that have nothing typed, drafted or queued. Only applies when Chat is the default view for that agent; with Terminal it never runs."),
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
        "Session Tags",
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
