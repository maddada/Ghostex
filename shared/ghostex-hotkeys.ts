import type { SessionGridDirection, TerminalViewMode } from "./session-grid-contract-core";

export type ghostexHotkeyActionId =
  | "attachFileOrFolder"
  | "closeAfterDone"
  | "closeFocusedSession"
  | "createSession"
  | "delayedSend"
  | "exportTranscript"
  | "forkSession"
  | "mergeAllTabs"
  | "openCommandPalette"
  | "openSessionSearchPalette"
  | "openBrowserPane"
  | "openSettings"
  | "openHotkeys"
  | "moveSidebar"
  | "openCommandsPanel"
  | "openPlugins"
  | "popOutPane"
  | "promptEditor"
  | "reloadSession"
  | "renameActiveSession"
  | "rotatePanesClockwise"
  | "sleepFocusedSession"
  | "scrollTerminalToBottom"
  | "scrollTerminalToTop"
  | "stashPrompt"
  | "stashedPrompts"
  | "toggleChatView"
  | "openFindPrompts"
  | "toggleCompanionPane"
  | "toggleAgentActions"
  | "toggleSidebarCollapsed"
  | "wakeFocusedSession"
  | "focusPreviousGroup"
  | "focusNextGroup"
  | "navigateHistoryBack"
  | "navigateHistoryForward"
  | "focusPreviousSession"
  | "focusNextSession"
  | "focusUp"
  | "focusRight"
  | "focusDown"
  | "focusLeft"
  | "splitMore"
  | "splitMoreDown"
  | "switchAgentsView"
  | "switchSourceView"
  | "switchGitHubView"
  | "switchKanbanView"
  | "switchManageView"
  | `runActionSlot${1 | 2 | 3 | 4 | 5}`
  | `jumpToProject${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`
  | `focusSessionSlot${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`;

export type ghostexHotkeySettings = Partial<Record<ghostexHotkeyActionId, string>>;

export type ghostexFocusedPaneAction =
  | "closeAfterDone"
  | "closeFocusedSession"
  | "delayedSend"
  | "forkSession"
  | "mergeAllTabs"
  | "openBrowserPane"
  | "popOutPane"
  | "reloadSession"
  | "rotatePanesClockwise"
  | "sleepFocusedSession"
  | "wakeFocusedSession";

export type ghostexTerminalToolbarAction =
  | "attachFileOrFolder"
  | "exportTranscript"
  | "promptEditor"
  | "scrollTerminalToBottom"
  | "scrollTerminalToTop"
  | "stashPrompt"
  | "stashedPrompts"
  | "toggleAgentActions"
  | "toggleChatView";


export type ghostexHotkeyAction =
  | { id: ghostexHotkeyActionId; kind: "createSession" }
  | { id: ghostexHotkeyActionId; kind: "focusAdjacentGroup"; direction: -1 | 1 }
  | { id: ghostexHotkeyActionId; kind: "focusDirection"; direction: SessionGridDirection }
  | { id: ghostexHotkeyActionId; kind: "focusSessionSlot"; slotNumber: number }
  | { id: ghostexHotkeyActionId; kind: "focusedPaneAction"; focusedPaneAction: ghostexFocusedPaneAction }
  | { id: ghostexHotkeyActionId; kind: "jumpToProject"; projectIndex: number }
  | { direction: "back" | "forward"; id: ghostexHotkeyActionId; kind: "navigateHistory" }
  | { id: ghostexHotkeyActionId; kind: "moveSidebar" }
  | { id: ghostexHotkeyActionId; kind: "openCommandPalette" }
  | { id: ghostexHotkeyActionId; kind: "openSessionSearchPalette" }
  | { id: ghostexHotkeyActionId; kind: "openCommandsPanel" }
  | { id: ghostexHotkeyActionId; kind: "openDocsFoldersSettings" }
  | { id: ghostexHotkeyActionId; kind: "openSettings" }
  | { id: ghostexHotkeyActionId; kind: "openFindPrompts" }
  | { id: ghostexHotkeyActionId; kind: "openHotkeys" }
  | { id: ghostexHotkeyActionId; kind: "renameActiveSession" }
  | { id: ghostexHotkeyActionId; kind: "runActionSlot"; slotNumber: number }
  | { id: ghostexHotkeyActionId; kind: "setViewMode"; viewMode: TerminalViewMode }
  | { id: ghostexHotkeyActionId; kind: "switchWorkareaView"; view: "agents" | "github" | "kanban" | "manage" | "source" }
  | { id: ghostexHotkeyActionId; kind: "terminalToolbarAction"; terminalToolbarAction: ghostexTerminalToolbarAction }
  | { id: ghostexHotkeyActionId; kind: "toggleCompanionPane" }
  | { id: ghostexHotkeyActionId; kind: "toggleSidebarCollapsed" }
  | { direction: "horizontal" | "vertical"; id: ghostexHotkeyActionId; kind: "splitFocusedPane" };

export type ghostexHotkeyDefinition = {
  action: ghostexHotkeyAction;
  alternateDefaultKeys?: readonly string[];
  defaultKey: string;
  description: string;
  id: ghostexHotkeyActionId;
  retiredDefaultKeys?: readonly string[];
  title: string;
  windowsLinuxDefaultKey?: string;
};

/**
 * CDXC:Hotkeys 2026-04-28-05:20
 * The native app must start with the same primary shortcuts as the reference
 * agent-tiler repo, while storing them as app settings so users can redefine
 * the bindings without changing code or relying on hard-coded VS Code keys.
 */
export const GHOSTEX_HOTKEY_DEFINITIONS: readonly ghostexHotkeyDefinition[] = [
  {
    action: { id: "createSession", kind: "createSession" },
    /**
     * CDXC:Hotkeys 2026-05-11-09:26
     * Default hotkeys should prefer plain Cmd chords so the app feels like a
     * Mac-first terminal workspace instead of requiring Cmd+Option layers for
     * everyday navigation.
     *
     * CDXC:Hotkeys 2026-06-06-04:36:
     * Cmd+T is the default New Terminal Tab action. It creates a terminal tab in the focused workspace split pane, immediately after the currently focused tab.
     */
    defaultKey: "cmd+t",
    description: "Create a terminal session.",
    id: "createSession",
    retiredDefaultKeys: ["cmd+n"],
    title: "Create Session",
  },
  {
    action: { id: "openCommandPalette", kind: "openCommandPalette" },
    /**
     * CDXC:CommandPalette 2026-06-13-10:26:
     * Cmd+Shift+P is the default command-palette shortcut for the macOS app. It
     * must live in the shared hotkey model so terminal-focused AppKit dispatch
     * and sidebar DOM dispatch both open the same shadcn command surface.
     *
     * CDXC:CommandPalette 2026-06-13-22:18:
     * Cmd+Shift+P opens Ghostex Quick Access directly on its Commands tab.
     */
    defaultKey: "cmd+shift+p",
    description: "Open Ghostex Quick Access on Commands.",
    id: "openCommandPalette",
    retiredDefaultKeys: ["cmd+k"],
    title: "Open Quick Access: Commands",
  },
  {
    action: { id: "openSessionSearchPalette", kind: "openSessionSearchPalette" },
    /**
     * CDXC:CommandPalette 2026-06-13-22:18:
     * Cmd+P opens Ghostex Quick Access directly on Recent Sessions. Users can
     * rebind it separately because session recovery and command finding are
     * distinct habits.
     */
    defaultKey: "cmd+p",
    description: "Open Ghostex Quick Access on Recent Sessions.",
    id: "openSessionSearchPalette",
    title: "Open Quick Access: Recent Sessions",
  },
  {
    action: { id: "openCommandsPanel", kind: "openCommandsPanel" },
    defaultKey: "f12",
    description: "Open the project command terminal panel.",
    id: "openCommandsPanel",
    title: "Open Commands Panel",
  },
  {
    action: { id: "openSettings", kind: "openSettings" },
    defaultKey: "cmd+,",
    description: "Open app settings.",
    id: "openSettings",
    title: "Open Settings",
  },
  {
    action: { id: "openHotkeys", kind: "openHotkeys" },
    /**
     * CDXC:Hotkeys 2026-06-19-00:35:
     * The far-right titlebar Settings menu advertises Cmd+. beside Hotkeys. Make Hotkeys a real configurable app shortcut so the menu label, Settings editor, sidebar dispatch, and terminal-focused AppKit dispatch all describe the same behavior.
     */
    defaultKey: "cmd+.",
    description: "Open app hotkeys.",
    id: "openHotkeys",
    title: "Hotkeys",
  },
  {
    action: { id: "toggleSidebarCollapsed", kind: "toggleSidebarCollapsed" },
    /**
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * Cmd+B should completely collapse or expand the native sidebar chrome. Keep this separate from sidebar placement so the same shortcut never moves the sidebar between left and right sides.
     */
    defaultKey: "cmd+b",
    description: "Collapse or expand the sidebar.",
    id: "toggleSidebarCollapsed",
    title: "Toggle Sidebar",
  },
  {
    action: { id: "toggleCompanionPane", kind: "toggleCompanionPane" },
    /**
     * CDXC:ProjectEditorCompanion 2026-07-29-05:03:
     * Cmd+Option+B toggles the project-editor companion independently from the main app sidebar. Collapsing transfers focus to the active Code, Browser, Kanban, Automate, or Docs pane; expanding restores and focuses the companion.
     */
    defaultKey: "cmd+alt+b",
    description: "Collapse or expand the project companion pane.",
    id: "toggleCompanionPane",
    title: "Toggle Companion Pane",
  },
  {
    action: { id: "moveSidebar", kind: "moveSidebar" },
    /**
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * Sidebar side switching remains configurable, but it must be unassigned by default now that Cmd+B is the complete sidebar collapse toggle.
     */
    defaultKey: "",
    description: "Move the sidebar to the other side.",
    id: "moveSidebar",
    retiredDefaultKeys: ["cmd+b"],
    title: "Move Sidebar",
  },
  {
    action: { id: "renameActiveSession", kind: "renameActiveSession" },
    defaultKey: "cmd+r",
    description: "Rename the focused session.",
    id: "renameActiveSession",
    title: "Rename Active Session",
  },
  {
    action: { focusedPaneAction: "openBrowserPane", id: "openBrowserPane", kind: "focusedPaneAction" },
    /**
     * CDXC:CommandPalette 2026-05-17-01:32:
     * Pane context-menu actions should also be command-palette commands with
     * configurable shortcuts. These hotkeys target the focused pane/session so
     * keyboard use follows the same scope as the visible pane menu.
     *
     * CDXC:Hotkeys 2026-06-06-04:36:
     * Cmd+N is the default New Browser Tab action. It opens the browser as the next tab in the focused workspace split pane instead of creating a separate split or app window.
     */
    defaultKey: "cmd+n",
    description: "Open a browser tab beside the focused tab.",
    id: "openBrowserPane",
    retiredDefaultKeys: ["ctrl+shift+b"],
    title: "Open Browser Pane",
  },
  ...([
    ["switchAgentsView", "agents", "alt+1", "Agents"],
    ["switchSourceView", "source", "alt+2", "Source"],
    ["switchGitHubView", "github", "alt+3", "GitHub"],
    ["switchKanbanView", "kanban", "alt+4", "Kanban"],
    ["switchManageView", "manage", "alt+5", "Docs"],
  ] as const).map(([id, view, defaultKey, title]) => ({
    action: {
      id,
      kind: "switchWorkareaView" as const,
      view,
    },
    /**
     * CDXC:Hotkeys 2026-06-06-04:36:
     * Option+1..5 are default workarea view switchers for the five hotkey-backed views: Agents, Source, Browser, Kanban, Docs. Keep these as named actions instead of overloading group/session slots so AppKit, Settings, and sidebar DOM dispatch switch the same project surface.
     *
     * CDXC:Manage 2026-06-20-04:36:
     * Manage is a first-party project workarea beside Kanban, so it needs a named configurable hotkey action instead of sharing another mode's shortcut or command id.
     *
     * CDXC:Docs 2026-06-28-06:24:
     * The switchManageView id and "manage" view enum remain compatibility
     * handles, but Settings and command labels should call the feature Docs.
     */
    defaultKey,
    description: `Switch to ${title} view.`,
    id,
    title: `Switch to ${title}`,
  })),
  {
    action: {
      focusedPaneAction: "rotatePanesClockwise",
      id: "rotatePanesClockwise",
      kind: "focusedPaneAction",
    },
    /**
     * CDXC:CommandPalette 2026-05-17-01:34:
     * Rotate and Reload defaults are intentionally swapped so Ctrl+Shift+L
     * rotates the layout while Ctrl+Shift+R keeps the common reload mnemonic.
     */
    defaultKey: "ctrl+shift+l",
    description: "Rotate panes clockwise in the focused group.",
    id: "rotatePanesClockwise",
    title: "Rotate Panes Clockwise",
    windowsLinuxDefaultKey: "cmd+alt+l",
  },
  {
    action: { focusedPaneAction: "mergeAllTabs", id: "mergeAllTabs", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+m",
    description: "Merge the focused group's panes into one tabbed pane.",
    id: "mergeAllTabs",
    title: "Merge All Tabs",
    windowsLinuxDefaultKey: "cmd+alt+m",
  },
  {
    action: { focusedPaneAction: "delayedSend", id: "delayedSend", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+s",
    description: "Open delayed actions for the focused terminal session.",
    id: "delayedSend",
    title: "Delayed Actions",
    windowsLinuxDefaultKey: "cmd+alt+s",
  },
  {
    action: { focusedPaneAction: "closeAfterDone", id: "closeAfterDone", kind: "focusedPaneAction" },
    /**
     * CDXC:FocusedSessionActions 2026-06-19-15:43:
     * Focused-session commands that already exist in native pane menus should also be configurable hotkey actions and command-palette rows. Close After Done starts unassigned so adding discoverability does not introduce a new default shortcut.
     */
    defaultKey: "",
    description: "Toggle Close After Done for the focused terminal session.",
    id: "closeAfterDone",
    title: "Close After Done",
  },
  {
    action: {
      id: "promptEditor",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "promptEditor",
    },
    defaultKey: "ctrl+g",
    description: "Open the prompt editor for the focused terminal.",
    id: "promptEditor",
    title: "Prompt Editor",
    windowsLinuxDefaultKey: "cmd+shift+g",
  },
  {
    action: {
      id: "attachFileOrFolder",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "attachFileOrFolder",
    },
    defaultKey: "",
    description: "Attach a file or folder to the focused terminal.",
    id: "attachFileOrFolder",
    title: "Attach File or Folder",
  },
  {
    action: {
      id: "stashPrompt",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "stashPrompt",
    },
    defaultKey: "",
    description: "Stash the current prompt in the focused agent terminal.",
    id: "stashPrompt",
    title: "Stash Prompt",
  },
  {
    action: {
      id: "stashedPrompts",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "stashedPrompts",
    },
    defaultKey: "alt+shift+s",
    description: "Open stashed prompts for the focused agent session.",
    id: "stashedPrompts",
    title: "Stashed Prompts",
  },
  {
    action: {
      id: "exportTranscript",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "exportTranscript",
    },
    /**
     * CDXC:ExportTranscript 2026-08-20:
     * Export transcript writes the focused agent session's conversation to a
     * markdown file on the machine that owns the transcript, then offers to
     * seed a new conversation from it. It ships unassigned by default, like
     * the other Agent Actions menu rows (Stash Prompt, Attach File or Folder),
     * so it cannot claim a chord users already rely on.
     */
    defaultKey: "",
    description: "Export the focused agent session's transcript to a markdown file.",
    id: "exportTranscript",
    title: "Export Transcript",
  },
  {
    action: {
      id: "toggleAgentActions",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "toggleAgentActions",
    },
    defaultKey: "",
    description: "Show or hide the focused terminal's Agent Actions buttons.",
    id: "toggleAgentActions",
    title: "Toggle Agent Actions",
  },
  {
    action: {
      id: "toggleChatView",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "toggleChatView",
    },
    /**
     * CDXC:SessionChatView 2026-07-31:
     * Session Chat swaps the focused agent terminal's pane body with the shared
     * chat surface for supported transcript agents (claude/openclaude/codex/grok).
     * Alt+G keeps the chat toggle compact, and the same action id must always
     * be able to toggle a chat-mode session back to its terminal.
     */
    defaultKey: "alt+g",
    description: "Toggle between the terminal and chat view for the focused agent session.",
    id: "toggleChatView",
    retiredDefaultKeys: ["ctrl+shift+j", "cmd+alt+j", "ctrl+shift+g", "cmd+alt+g"],
    title: "Toggle Chat View",
  },
  {
    action: { id: "openFindPrompts", kind: "openFindPrompts" },
    /**
     * CDXC:AgentHistorySearch 2026-08-20:
     * Find is the GUI for `gx f`: it swaps the focused pane for a searchable
     * list of every prompt this machine has ever sent to an agent. It is
     * dispatched natively like Chat View, so the same id always toggles back
     * out of Find even while the terminal is hidden behind it.
     */
    defaultKey: "alt+f",
    description: "Search every prompt you have sent to an agent, then resume or fork it.",
    id: "openFindPrompts",
    title: "Find Prompts",
  },
  {
    action: {
      id: "scrollTerminalToTop",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "scrollTerminalToTop",
    },
    defaultKey: "",
    description: "Scroll the focused terminal to the top.",
    id: "scrollTerminalToTop",
    title: "Scroll Terminal to Top",
  },
  {
    action: {
      id: "scrollTerminalToBottom",
      kind: "terminalToolbarAction",
      terminalToolbarAction: "scrollTerminalToBottom",
    },
    defaultKey: "",
    description: "Scroll the focused terminal to the bottom.",
    id: "scrollTerminalToBottom",
    title: "Scroll Terminal to Bottom",
  },
  {
    action: { focusedPaneAction: "forkSession", id: "forkSession", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+f",
    description: "Fork the focused session.",
    id: "forkSession",
    title: "Fork Session",
    windowsLinuxDefaultKey: "cmd+alt+f",
  },
  {
    action: { focusedPaneAction: "reloadSession", id: "reloadSession", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+r",
    description: "Reload the focused session.",
    id: "reloadSession",
    title: "Reload Session",
    windowsLinuxDefaultKey: "cmd+alt+r",
  },
  {
    action: {
      focusedPaneAction: "sleepFocusedSession",
      id: "sleepFocusedSession",
      kind: "focusedPaneAction",
    },
    defaultKey: "",
    description: "Sleep the focused terminal session.",
    id: "sleepFocusedSession",
    retiredDefaultKeys: ["alt+shift+s"],
    title: "Sleep Focused Session",
  },
  {
    action: {
      focusedPaneAction: "wakeFocusedSession",
      id: "wakeFocusedSession",
      kind: "focusedPaneAction",
    },
    /**
     * CDXC:FocusedSessionActions 2026-06-19-15:43:
     * Wake is the inverse focused-session lifecycle command. Keep it unassigned by default but available in Hotkeys and command palette so sleeping focused tabs can be restored without a row-specific sidebar click.
     */
    defaultKey: "",
    description: "Wake the focused sleeping terminal session.",
    id: "wakeFocusedSession",
    title: "Wake Focused Session",
  },
  {
    action: {
      focusedPaneAction: "closeFocusedSession",
      id: "closeFocusedSession",
      kind: "focusedPaneAction",
    },
    /**
     * CDXC:FocusedSessionActions 2026-06-19-15:43:
     * Close is already available from pane/tab chrome and Cmd+W, but it should be bindable and runnable from the command palette without claiming a second default shortcut.
     */
    defaultKey: "",
    description: "Close the focused pane or session.",
    id: "closeFocusedSession",
    title: "Close Focused Session",
  },
  {
    action: { focusedPaneAction: "popOutPane", id: "popOutPane", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+o",
    description: "Pop out or restore the focused pane.",
    id: "popOutPane",
    title: "Pop Out Pane",
    windowsLinuxDefaultKey: "cmd+alt+o",
  },
  {
    action: { direction: -1, id: "focusPreviousGroup", kind: "focusAdjacentGroup" },
    defaultKey: "cmd+[",
    description: "Focus the previous group.",
    id: "focusPreviousGroup",
    retiredDefaultKeys: ["cmd+shift+["],
    title: "Previous Group",
  },
  {
    action: { direction: 1, id: "focusNextGroup", kind: "focusAdjacentGroup" },
    defaultKey: "cmd+]",
    description: "Focus the next group.",
    id: "focusNextGroup",
    retiredDefaultKeys: ["cmd+shift+]"],
    title: "Next Group",
  },
  /**
   * CDXC:NavigationHistory 2026-08-19:
   * Back/Forward walk the chronological trail of previously active sessions and
   * projects — where you have BEEN, not where a session sits in an ordered list.
   * That is why they are not bound to the bracket chords beside them: Cmd+[ / ]
   * already move between groups in render order and Cmd+Shift+[ / ] between tabs
   * in a pane. Cmd+Ctrl reuses the modifier this app already spends on
   * cross-project movement (Jump to Project 1..9), with the same Cmd+Alt
   * Windows/Linux substitution those entries use.
   */
  {
    action: { direction: "back", id: "navigateHistoryBack", kind: "navigateHistory" },
    defaultKey: "cmd+ctrl+[",
    description: "Go back to the previously active session or project.",
    id: "navigateHistoryBack",
    title: "Back",
    windowsLinuxDefaultKey: "cmd+alt+[",
  },
  {
    action: { direction: "forward", id: "navigateHistoryForward", kind: "navigateHistory" },
    defaultKey: "cmd+ctrl+]",
    description: "Go forward again after going back.",
    id: "navigateHistoryForward",
    title: "Forward",
    windowsLinuxDefaultKey: "cmd+alt+]",
  },
  {
    action: { id: "focusPreviousSession", kind: "focusSessionSlot", slotNumber: -1 },
    alternateDefaultKeys: ["cmd+shift+["],
    /**
     * CDXC:Hotkeys 2026-06-13-19:36:
     * Cmd+Shift+[ and Cmd+Shift+] remain supported alongside Cmd+Shift+Tab and Cmd+Tab, but both shortcut families are focused split-pane tab switchers.
     *
     * CDXC:Hotkeys 2026-06-13-20:08:
     * Previous/next tab traversal must stay inside the active pane's tab group and include sleeping placeholder tabs, then native dispatch applies the same select/wake/attach logic as clicking that tab.
     */
    defaultKey: "cmd+shift+tab",
    description: "Select the previous tab in the focused split pane.",
    id: "focusPreviousSession",
    retiredDefaultKeys: ["cmd+["],
    title: "Previous Tab",
  },
  {
    action: { id: "focusNextSession", kind: "focusSessionSlot", slotNumber: 0 },
    alternateDefaultKeys: ["cmd+shift+]"],
    defaultKey: "cmd+tab",
    description: "Select the next tab in the focused split pane.",
    id: "focusNextSession",
    retiredDefaultKeys: ["cmd+]"],
    title: "Next Tab",
  },
  ...(["up", "right", "down", "left"] as const).map((direction) => ({
    action: {
      direction,
      id: `focus${capitalize(direction)}` as ghostexHotkeyActionId,
      kind: "focusDirection" as const,
    },
    /**
     * CDXC:Hotkeys 2026-05-15-13:31:
     * Plain Cmd+Arrow belongs to terminal and prompt text editing, including jump-to-line-boundary behavior.
     * Directional pane focus uses Cmd+Alt+Arrow so app navigation no longer steals common editing shortcuts.
     */
    defaultKey: `cmd+alt+${direction}`,
    description: `Move focus ${direction}.`,
    id: `focus${capitalize(direction)}` as ghostexHotkeyActionId,
    retiredDefaultKeys: [`cmd+${direction}`],
    title: `Focus ${capitalize(direction)}`,
  })),
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((projectIndex) => ({
    action: {
      id: `jumpToProject${projectIndex}` as ghostexHotkeyActionId,
      kind: "jumpToProject" as const,
      projectIndex,
    },
    /**
     * CDXC:ProjectHotkeys 2026-06-15-11:12:
     * Cmd+Ctrl+1..9 are project jump shortcuts, not workspace-group shortcuts.
     * Resolve these against the Projects rows as shown in the sidebar so numbered project navigation follows the same ordering model users can see.
     */
    defaultKey: `cmd+ctrl+${projectIndex}`,
    description: `Jump to project ${projectIndex} as shown in the sidebar.`,
    id: `jumpToProject${projectIndex}` as ghostexHotkeyActionId,
    title: `Jump to Project ${projectIndex}`,
    windowsLinuxDefaultKey: `cmd+alt+${projectIndex}`,
  })),
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((slotNumber) => ({
    action: {
      id: `focusSessionSlot${slotNumber}` as ghostexHotkeyActionId,
      kind: "focusSessionSlot" as const,
      slotNumber,
    },
    defaultKey: `cmd+${slotNumber}`,
    description: `Focus session slot ${slotNumber}.`,
    id: `focusSessionSlot${slotNumber}` as ghostexHotkeyActionId,
    title: `Focus Session ${slotNumber}`,
  })),
  ...[1, 2, 3, 4, 5].map((slotNumber) => ({
    action: {
      id: `runActionSlot${slotNumber}` as ghostexHotkeyActionId,
      kind: "runActionSlot" as const,
      slotNumber,
    },
    /**
     * CDXC:ActionsHotkeys 2026-05-17-01:18:
     * Action hotkeys are positional by the Actions settings list, not tied to
     * command ids, so users can reorder actions without rebinding the first
     * five launcher shortcuts.
     */
    defaultKey: `ctrl+shift+${slotNumber}`,
    description: `Start action ${slotNumber} from the Actions list.`,
    id: `runActionSlot${slotNumber}` as ghostexHotkeyActionId,
    title: `Start Action ${slotNumber}`,
    windowsLinuxDefaultKey: `cmd+shift+${slotNumber}`,
  })),
  {
    action: { direction: "horizontal", id: "splitMore", kind: "splitFocusedPane" },
    /**
     * CDXC:NativeSplits 2026-05-10-18:30
     * Cmd+D creates a real terminal session beside the focused pane instead of
     * only increasing the visible split count. This matches terminal split
     * muscle memory and lets users immediately send work into the new pane.
     */
    defaultKey: "cmd+d",
    description: "Create a terminal beside the focused pane.",
    id: "splitMore",
    title: "Split Sideways",
  },
  {
    action: { direction: "vertical", id: "splitMoreDown", kind: "splitFocusedPane" },
    defaultKey: "cmd+shift+d",
    description: "Create a terminal below the focused pane.",
    id: "splitMoreDown",
    title: "Split Downwards",
  },
];

export const DEFAULT_ghostex_HOTKEYS: ghostexHotkeySettings = Object.fromEntries(
  GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => [definition.id, definition.defaultKey]),
);

export function normalizeghostexHotkeySettings(candidate: unknown): ghostexHotkeySettings {
  const source = isRecord(candidate) ? candidate : {};
  const platform = detectghostexHotkeyPlatform();
  const normalized: ghostexHotkeySettings = {};
  for (const definition of GHOSTEX_HOTKEY_DEFINITIONS) {
    const platformDefaultKey =
      platform === "mac"
        ? definition.defaultKey
        : definition.windowsLinuxDefaultKey ?? definition.defaultKey;
    const value = source[definition.id] ?? readLegacyProjectJumpHotkey(source, definition.id);
    if (typeof value === "string") {
      /**
       * CDXC:Hotkeys 2026-05-11-09:06
       * Users can remove any hotkey from Settings. A missing setting still
       * means "use the default", but an explicitly blank string means the
       * command is intentionally unassigned.
       */
      const hotkeyText = value.trim() ? normalizeHotkeyText(value) : "";
      normalized[definition.id] = definition.retiredDefaultKeys?.includes(hotkeyText)
        ? platformDefaultKey
        : platform !== "mac" &&
            definition.windowsLinuxDefaultKey &&
            hotkeyText === definition.defaultKey
          ? platformDefaultKey
          : hotkeyText;
      continue;
    }
    normalized[definition.id] = platformDefaultKey;
  }
  return normalized;
}

function readLegacyProjectJumpHotkey(
  source: Record<string, unknown>,
  actionId: ghostexHotkeyActionId,
): unknown {
  const match = /^jumpToProject([1-5])$/u.exec(actionId);
  if (!match) {
    return undefined;
  }

  /**
   * CDXC:ProjectHotkeys 2026-06-15-11:12:
   * Existing users may have customized or cleared the old Focus Group 1..5 bindings.
   * When those action ids become Jump to Project 1..5, preserve the persisted chord or explicit blank value instead of silently restoring the default.
   */
  return source[`focusGroup${match[1]}`];
}

export function getghostexHotkeyActionById(id: string): ghostexHotkeyAction | undefined {
  return GHOSTEX_HOTKEY_DEFINITIONS.find((definition) => definition.id === id)?.action;
}

export function getghostexHotkeyActionIdForKey(
  hotkeys: ghostexHotkeySettings,
  hotkeyText: string,
): ghostexHotkeyActionId | undefined {
  const normalizedHotkeyText = normalizeHotkeyText(hotkeyText);
  const matchedDefinition = Object.entries(hotkeys).find(([, value]) => value === normalizedHotkeyText);
  if (matchedDefinition) {
    return matchedDefinition[0] as ghostexHotkeyActionId;
  }
  return GHOSTEX_HOTKEY_DEFINITIONS.find(
    (definition) =>
      definition.alternateDefaultKeys?.includes(normalizedHotkeyText) && hotkeys[definition.id] !== "",
  )?.id;
}

export function normalizeHotkeyText(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/⌘|command/g, "cmd")
    .replace(/⌥|option/g, "alt")
    .replace(/⌃|control/g, "ctrl")
    .replace(/⇧|shift/g, "shift")
    .replace(/\bmod\b/g, "cmd")
    .replace(/\s+/g, " ")
    .split(" ")
    .map(normalizeHotkeyChordText)
    .join(" ");
}

const SHIFTED_DIGIT_KEYS: Record<string, string> = {
  "!": "1",
  "@": "2",
  "#": "3",
  $: "4",
  "%": "5",
  "^": "6",
  "&": "7",
  "*": "8",
  "(": "9",
  ")": "0",
};

const SHIFTED_SYMBOL_KEYS: Record<string, string> = {
  "{": "[",
  "}": "]",
};

function normalizeHotkeyChordText(chord: string): string {
  const parts = chord.split("+").filter(Boolean);
  const key = parts.at(-1);
  if (!key) {
    return chord;
  }
  if (parts.includes("alt") && key === "ß") {
    /**
     * CDXC:Hotkeys 2026-07-30:
     * The old Settings recorder serialized KeyboardEvent.key. On macOS,
     * Option+S reports the produced character `ß`, so that binding could
     * neither match the physical S key used by GPUI nor render correctly
     * (`"ß".toUpperCase()` is `"SS"`). Normalize the persisted legacy value
     * to the physical key spelling used by the corrected recorder.
     */
    parts[parts.length - 1] = "s";
  }
  if (parts.includes("shift") && SHIFTED_DIGIT_KEYS[key]) {
    /**
     * CDXC:ActionsHotkeys 2026-05-26-13:20:
     * Browser/WebKit keydown events report Ctrl+Shift+1 as "ctrl+shift+!" while
     * AppKit and Settings store the same physical action shortcut as "ctrl+shift+1".
     * Normalize shifted digit glyphs at the shared matcher so action-slot
     * hotkeys run from sidebar, browser, and terminal focus without duplicate bindings.
     */
    parts[parts.length - 1] = SHIFTED_DIGIT_KEYS[key];
  }
  if (parts.includes("shift") && SHIFTED_SYMBOL_KEYS[key]) {
    /**
     * CDXC:Hotkeys 2026-06-07-14:24:
     * WebKit reports Cmd+Shift+[ and Cmd+Shift+] as the shifted glyphs "{" and
     * "}" when sidebar chrome owns focus. Normalize those back to the physical
     * bracket keys so the alternate next/previous-session defaults match the
     * same stored shortcut text as AppKit and Settings.
     */
    parts[parts.length - 1] = SHIFTED_SYMBOL_KEYS[key];
  }
  return parts.join("+");
}

export type ghostexHotkeyPlatform = "linux" | "mac" | "windows";

export function detectghostexHotkeyPlatform(): ghostexHotkeyPlatform {
  if (typeof navigator === "undefined") {
    return "mac";
  }
  const platform = navigator.platform?.toLowerCase() ?? "";
  const userAgent = navigator.userAgent?.toLowerCase() ?? "";
  if (platform.includes("mac") || userAgent.includes("mac")) {
    return "mac";
  }
  if (platform.includes("win") || userAgent.includes("win")) {
    return "windows";
  }
  return "linux";
}

export function ghostexHotkeyTextFromKeyboardEvent(
  event: Pick<
    KeyboardEvent,
    "altKey" | "code" | "ctrlKey" | "key" | "metaKey" | "shiftKey"
  >,
  platform: ghostexHotkeyPlatform = detectghostexHotkeyPlatform(),
): string | undefined {
  const key = physicalHotkeyKeyFromKeyboardEvent(event);
  if (!key) {
    return undefined;
  }
  const parts: string[] = [];
  if (platform === "mac" ? event.metaKey : event.ctrlKey) {
    parts.push("cmd");
  }
  if (platform === "mac" && event.ctrlKey) {
    parts.push("ctrl");
  }
  if (event.altKey) {
    parts.push("alt");
  }
  if (event.shiftKey) {
    parts.push("shift");
  }
  parts.push(key);
  return normalizeHotkeyText(parts.join("+"));
}

const PHYSICAL_HOTKEY_KEYS_BY_CODE: Readonly<Record<string, string>> = {
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  Backquote: "`",
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Comma: ",",
  Delete: "delete",
  End: "end",
  Enter: "enter",
  Equal: "=",
  Escape: "escape",
  Home: "home",
  Minus: "-",
  PageDown: "pagedown",
  PageUp: "pageup",
  Period: ".",
  Quote: "'",
  Semicolon: ";",
  Slash: "/",
  Space: "space",
  Tab: "tab",
};

function physicalHotkeyKeyFromKeyboardEvent(
  event: Pick<KeyboardEvent, "code" | "key">,
): string | undefined {
  const letterMatch = /^Key([A-Z])$/u.exec(event.code);
  if (letterMatch) {
    return letterMatch[1]?.toLowerCase();
  }
  const digitMatch = /^(?:Digit|Numpad)([0-9])$/u.exec(event.code);
  if (digitMatch) {
    return digitMatch[1];
  }
  if (/^F(?:[1-9]|1[0-9]|2[0-4])$/u.test(event.code)) {
    return event.code.toLowerCase();
  }
  const physicalKey = PHYSICAL_HOTKEY_KEYS_BY_CODE[event.code];
  if (physicalKey) {
    return physicalKey;
  }
  switch (event.key) {
    case "Alt":
    case "AltGraph":
    case "Control":
    case "Dead":
    case "Meta":
    case "Shift":
    case "Unidentified":
      return undefined;
    default:
      return event.key.length === 1 ? event.key.toLowerCase() : event.key.toLowerCase();
  }
}

function capitalize(value: string): string {
  return `${value.slice(0, 1).toUpperCase()}${value.slice(1)}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
