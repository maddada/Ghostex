import type { SessionGridDirection, TerminalViewMode } from "./session-grid-contract-core";

export type ghostexHotkeyActionId =
  | "createSession"
  | "delayedSend"
  | "forkSession"
  | "mergeAllTabs"
  | "openCommandPalette"
  | "openBrowserPane"
  | "openSettings"
  | "moveSidebar"
  | "openCommandsPanel"
  | "popOutPane"
  | "reloadSession"
  | "renameActiveSession"
  | "rotatePanesClockwise"
  | "toggleSidebarCollapsed"
  | "focusPreviousGroup"
  | "focusNextGroup"
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
  | `runActionSlot${1 | 2 | 3 | 4 | 5}`
  | `focusGroup${1 | 2 | 3 | 4 | 5}`
  | `focusSessionSlot${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9}`;

export type ghostexHotkeySettings = Partial<Record<ghostexHotkeyActionId, string>>;

export type ghostexFocusedPaneAction =
  | "delayedSend"
  | "forkSession"
  | "mergeAllTabs"
  | "openBrowserPane"
  | "popOutPane"
  | "reloadSession"
  | "rotatePanesClockwise";

export type ghostexHotkeyAction =
  | { id: ghostexHotkeyActionId; kind: "createSession" }
  | { id: ghostexHotkeyActionId; kind: "focusAdjacentGroup"; direction: -1 | 1 }
  | { id: ghostexHotkeyActionId; kind: "focusDirection"; direction: SessionGridDirection }
  | { id: ghostexHotkeyActionId; kind: "focusGroup"; groupIndex: number }
  | { id: ghostexHotkeyActionId; kind: "focusSessionSlot"; slotNumber: number }
  | { id: ghostexHotkeyActionId; kind: "focusedPaneAction"; focusedPaneAction: ghostexFocusedPaneAction }
  | { id: ghostexHotkeyActionId; kind: "moveSidebar" }
  | { id: ghostexHotkeyActionId; kind: "openCommandPalette" }
  | { id: ghostexHotkeyActionId; kind: "openCommandsPanel" }
  | { id: ghostexHotkeyActionId; kind: "openSettings" }
  | { id: ghostexHotkeyActionId; kind: "renameActiveSession" }
  | { id: ghostexHotkeyActionId; kind: "runActionSlot"; slotNumber: number }
  | { id: ghostexHotkeyActionId; kind: "setViewMode"; viewMode: TerminalViewMode }
  | { id: ghostexHotkeyActionId; kind: "switchWorkareaView"; view: "agents" | "github" | "kanban" | "source" }
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
     */
    defaultKey: "cmd+shift+p",
    description: "Open the Ghostex command palette.",
    id: "openCommandPalette",
    retiredDefaultKeys: ["cmd+k"],
    title: "Open Command Palette",
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
  ] as const).map(([id, view, defaultKey, title]) => ({
    action: {
      id,
      kind: "switchWorkareaView" as const,
      view,
    },
    /**
     * CDXC:Hotkeys 2026-06-06-04:36:
     * Option+1..4 are default workarea view switchers in titlebar order: Agents, Source, GitHub, Kanban. Keep these as named actions instead of overloading group/session slots so AppKit, Settings, and sidebar DOM dispatch switch the same project surface.
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
  },
  {
    action: { focusedPaneAction: "mergeAllTabs", id: "mergeAllTabs", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+m",
    description: "Merge the focused group's panes into one tabbed pane.",
    id: "mergeAllTabs",
    title: "Merge All Tabs",
  },
  {
    action: { focusedPaneAction: "delayedSend", id: "delayedSend", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+s",
    description: "Schedule Enter for the focused terminal session.",
    id: "delayedSend",
    title: "Delayed Send",
  },
  {
    action: { focusedPaneAction: "forkSession", id: "forkSession", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+f",
    description: "Fork the focused session.",
    id: "forkSession",
    title: "Fork Session",
  },
  {
    action: { focusedPaneAction: "reloadSession", id: "reloadSession", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+r",
    description: "Reload the focused session.",
    id: "reloadSession",
    title: "Reload Session",
  },
  {
    action: { focusedPaneAction: "popOutPane", id: "popOutPane", kind: "focusedPaneAction" },
    defaultKey: "ctrl+shift+o",
    description: "Pop out or restore the focused pane.",
    id: "popOutPane",
    title: "Pop Out Pane",
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
  {
    action: { id: "focusPreviousSession", kind: "focusSessionSlot", slotNumber: -1 },
    alternateDefaultKeys: ["cmd+shift+["],
    /**
     * CDXC:Hotkeys 2026-06-07-14:05:
     * Cmd+Shift+[ and Cmd+Shift+] must remain supported alongside Cmd+Shift+Tab and Cmd+Tab. Both shortcut families traverse rendered sidebar order across expanded groups and skip sleeping sessions.
     */
    defaultKey: "cmd+shift+tab",
    description: "Focus the previous awake visible sidebar tab.",
    id: "focusPreviousSession",
    retiredDefaultKeys: ["cmd+["],
    title: "Previous Tab",
  },
  {
    action: { id: "focusNextSession", kind: "focusSessionSlot", slotNumber: 0 },
    alternateDefaultKeys: ["cmd+shift+]"],
    defaultKey: "cmd+tab",
    description: "Focus the next awake visible sidebar tab.",
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
  ...[1, 2, 3, 4, 5].map((groupIndex) => ({
    action: {
      groupIndex,
      id: `focusGroup${groupIndex}` as ghostexHotkeyActionId,
      kind: "focusGroup" as const,
    },
    defaultKey: `cmd+ctrl+${groupIndex}`,
    description: `Focus group ${groupIndex}.`,
    id: `focusGroup${groupIndex}` as ghostexHotkeyActionId,
    title: `Focus Group ${groupIndex}`,
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
  const normalized: ghostexHotkeySettings = {};
  for (const definition of GHOSTEX_HOTKEY_DEFINITIONS) {
    const value = source[definition.id];
    if (typeof value === "string") {
      /**
       * CDXC:Hotkeys 2026-05-11-09:06
       * Users can remove any hotkey from Settings. A missing setting still
       * means "use the default", but an explicitly blank string means the
       * command is intentionally unassigned.
       */
      const hotkeyText = value.trim() ? normalizeHotkeyText(value) : "";
      normalized[definition.id] = definition.retiredDefaultKeys?.includes(hotkeyText)
        ? definition.defaultKey
        : hotkeyText;
      continue;
    }
    normalized[definition.id] = definition.defaultKey;
  }
  return normalized;
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

function capitalize(value: string): string {
  return `${value.slice(0, 1).toUpperCase()}${value.slice(1)}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
