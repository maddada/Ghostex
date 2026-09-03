// The per-session action contract a host hands to the chat surfaces. It is a
// types-only module because both the chat composer's dots menu
// (session-chat-composer-actions.tsx) and the terminal surface's bottom bar
// (session-terminal-action-bar.tsx) consume it, and hosts that only build the
// object (chat-main.tsx, the web session chat host) should not pull either
// component into their bundle.

/** One row of a submenu host action (see `SessionChatHostAction.items`). */
export interface SessionChatHostActionItem {
  /** Passed back to onAction as the value when the row is picked. */
  id: string;
  label: string;
  /** Sidebar agent icon id (`claude`, `codex`, …) drawn beside the label. */
  icon?: string;
}

export interface SessionChatHostAction {
  /** Host-defined action id, passed back verbatim to onAction. */
  id: string;
  label: string;
  /** Formatted effective shortcut shown beside the label in the tooltip. */
  shortcut?: string;
  /**
   * CDXC:AgentProviders 2026-09-03:
   * When set, the action renders as a submenu of these rows and picking one
   * calls `onAction(action.id, item.id)`. An empty list hides the action, so a
   * host can always list "Switch Account" and let the rows decide.
   */
  items?: readonly SessionChatHostActionItem[];
  /**
   * When set, picking the action swaps its control row for an inline text
   * field (e.g. Rename); onAction receives the submitted value as its second
   * argument.
   */
  input?: { initialValue?: string; placeholder?: string };
}

/**
 * Host-injected per-session actions: the surface switch (rendered as a footer
 * control on the chat surface and as a bar control on the terminal surface)
 * plus the host's action list, which each surface folds into its own dots
 * menu. Hosts whose own chrome already offers these (e.g. the mobile app's
 * native header) simply omit the prop.
 */
export interface SessionChatHostActions {
  onSwitchToTerminal: () => void;
  /** Route the system clipboard through the host's native editor paste command. */
  onPasteIntoComposer?: () => void;
  /** Formatted shortcut for switching between Terminal View and Chat View. */
  switchViewShortcut?: string;
  /**
   * Formatted shortcut for opening the dots menu. Like `switchViewShortcut`, it
   * is a separate field because the control is the surface's own rather than a
   * row in `actions`, and the surface cannot resolve the user's chord itself.
   */
  moreActionsShortcut?: string;
  /** Formatted shortcut for the session-note footer control. */
  sessionNoteShortcut?: string;
  /** Optional plain switch reserved for opening an agent-owned model picker. */
  onSwitchToTerminalForAgentPicker?: () => void;
  /** The host's per-session action list; omit to hide the menu entirely. */
  actions?: readonly SessionChatHostAction[];
  onAction?: (id: string, value?: string) => void;
}
