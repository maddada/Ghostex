/**
 * The Quick Access wire contract: one fully resolved display snapshot from the Quick Access controller to the native
 * GPUI window, and the interactions that come back. The controller (packages/gx-core/src/quick_access/, which replaced
 * the QuickJS runtime's TypeScript controller on 2026-09-25) owns data, filtering, sorting, grouping and every
 * command; the window only paints and reports.
 *
 * CDXC:AppModal 2026-09-20 SEE-ALSO:
 * Both halves of this contract must change together: packages/gx-core/src/quick_access/wire.rs (the Rust structs that
 * build these snapshots) and apps/desktop/src/app/window/quick_access/model.rs (the Rust structs that read them).
 * The retained React twins are packages/core-ui/command-palette.tsx, recent-projects-modal.tsx,
 * previous-sessions-modal.tsx and stashed-prompts-modal.tsx, which still render Quick Access on web and mobile.
 */

export type QuickAccessTabId = 'commands' | 'recentProjects' | 'recentSessions' | 'savedPrompts';

export type QuickAccessTab = {
  id: QuickAccessTabId;
  label: string;
  /** Already formatted for display, e.g. `⌘1`. */
  hotkey: string;
};

/**
 * A row glyph. `asset` names a bundled SVG under apps/desktop/assets/titlebar/;
 * `image` is a data URL (project icons); `none` leaves the slot empty but sized.
 */
export type QuickAccessIcon =
  { kind: 'asset'; name: string; color?: string } | { kind: 'image'; url: string } | { kind: 'none' };

export type QuickAccessCommandRow = {
  kind: 'command';
  key: string;
  title: string;
  icon: QuickAccessIcon;
  /** Formatted accelerator, empty when the command has none. */
  hotkey: string;
};

export type QuickAccessProjectRow = {
  kind: 'project';
  key: string;
  title: string;
  icon: QuickAccessIcon;
  tooltip: string;
  sessionCount: number;
  isOpen: boolean;
  isHidden: boolean;
};

export type QuickAccessSessionRow = {
  kind: 'session';
  key: string;
  title: string;
  icon: QuickAccessIcon;
  projectLabel: string;
  /** Rendered transcript size, empty until the size answer arrives. */
  fileSize: string;
  fileSizeLoading: boolean;
  time: string;
  inSidebar: boolean;
  sleeping: boolean;
  canActivate: boolean;
};

export type QuickAccessPromptChip = { label: string; color?: string };

export type QuickAccessPromptRow = {
  kind: 'prompt';
  key: string;
  title: string;
  /** The full prompt text, shown in the row tooltip. */
  tooltip: string;
  projectName: string;
  projectIcon: QuickAccessIcon;
  sessionTitle: string;
  tags: QuickAccessPromptChip[];
  time: string;
  isFavorite: boolean;
};

export type QuickAccessPromptAction = 'open' | 'favorite' | 'tag' | 'copy' | 'edit' | 'delete' | 'save' | 'dismiss';

export type QuickAccessRow =
  QuickAccessCommandRow | QuickAccessProjectRow | QuickAccessSessionRow | QuickAccessPromptRow;

export type QuickAccessGroup = {
  key: string;
  heading: string;
  /** Commands draws a hairline above a group that follows another one. */
  separated: boolean;
  rows: QuickAccessRow[];
};

export type QuickAccessOption = {
  value: string;
  label: string;
  /** Second line in the trigger and the menu row (the project filter's path hint). */
  detail: string;
  color: string;
  icon: QuickAccessIcon;
  disabled: boolean;
  selected: boolean;
  /** Renders as a divider above this row. */
  separated: boolean;
};

export type QuickAccessSelect = {
  label: string;
  detail: string;
  color: string;
  options: QuickAccessOption[];
  /** The menu offers its own filter field (the tag and project pickers do). */
  searchable: boolean;
  searchPlaceholder: string;
};

export type QuickAccessSegment = { value: string; label: string };

/** The filters at the right edge of the search line. Commands and Projects have none. */
export type QuickAccessToolbar =
  | { kind: 'none' }
  | {
      kind: 'sessions';
      scope: string;
      scopes: QuickAccessSegment[];
      scopeHotkey: string;
      tagFilterActive: boolean;
      tags: QuickAccessSelect;
      projects: QuickAccessSelect;
    }
  | {
      kind: 'prompts';
      view: string;
      views: QuickAccessSegment[];
      projects: QuickAccessSelect;
      tags: QuickAccessSelect;
    };

/** The Saved Prompts add/edit form, shown in place of the list. */
export type QuickAccessPromptEditor = {
  heading: string;
  content: string;
  projects: QuickAccessSelect;
  tags: QuickAccessSelect;
  isFavorite: boolean;
  error: string;
  saving: boolean;
  submitLabel: string;
};

/** The create-tag popover, anchored to whichever control opened it. */
export type QuickAccessTagComposer = {
  name: string;
  color: string;
  colors: string[];
  error: string;
  /** `toolbar` anchors under the tag filter; `row:<key>` under that row's tag button. */
  anchor: string;
};

export type QuickAccessSnapshot = {
  kind: 'snapshot';
  version: 1;
  revision: number;
  tab: QuickAccessTabId;
  tabs: QuickAccessTab[];
  placeholder: string;
  /** The query the runtime holds. Bumping `queryRevision` makes the window adopt it. */
  query: string;
  queryRevision: number;
  loading: boolean;
  loadingLabel: string;
  empty: string;
  groups: QuickAccessGroup[];
  selectedKey: string;
  /** The newest `select` command this snapshot already reflects. */
  selectionSeq: number;
  toolbar: QuickAccessToolbar;
  /** What Return does to the selected row, named in the footer. Empty when the row cannot be activated. */
  primaryAction: string;
  /** The wire hotkeys (`cmd+shift+c`) the window forwards as `actionHotkey` instead of treating as search text. */
  actionHotkeys: string[];
  /** The Saved Prompts stash hint pinned to the bottom-right of the list. */
  hint: string;
  editor: QuickAccessPromptEditor | null;
  tagComposer: QuickAccessTagComposer | null;
};

/** A menu the window asked for: a row's actions (right-click or the Actions panel) or a submenu one of them opened. */
export type QuickAccessMenuUpdate = {
  kind: 'menu';
  version: 1;
  items: QuickAccessMenuItem[];
};

export type QuickAccessMenuItem = {
  id: string;
  label: string;
  icon: QuickAccessIcon;
  /** Already formatted for display, empty when the item has no accelerator. */
  hotkey: string;
  danger: boolean;
  disabled: boolean;
  separator: boolean;
};

/** Tells the window to close itself (a command ran, a row was activated). */
export type QuickAccessCloseUpdate = { kind: 'close'; version: 1 };

export type QuickAccessUpdate = QuickAccessSnapshot | QuickAccessMenuUpdate | QuickAccessCloseUpdate;

export type QuickAccessCommand =
  /** The window opened (or a new open request re-targeted it) on `tab`. */
  | {
      type: 'open';
      tab: QuickAccessTabId;
      query?: string;
      /** Sessions: the project the launcher pinned the filter to. */
      projectId?: string;
      /** Sessions: `all` | `closed` | `external`. */
      scope?: string;
      /** Projects: the remote machine whose recent projects to list. */
      machineId?: string;
      /** Saved Prompts: the launching conversation's project, session and scope. */
      promptProjectId?: string;
      promptSessionId?: string;
      promptScope?: 'all' | 'project' | 'session';
    }
  | { type: 'close' }
  /** The host window is gone; the controller stops publishing until the next open. */
  | { type: 'closed' }
  | { type: 'tab'; tab: QuickAccessTabId }
  | { type: 'query'; query: string }
  | { type: 'select'; key: string; seq: number }
  | { type: 'activate'; key: string }
  /** Asks for the row's actions menu. `key` is empty when the list has no selection. */
  | { type: 'secondary'; key: string; x: number; y: number }
  | { type: 'menuItem'; id: string }
  /** One of `actionHotkeys` was pressed over the selected row. */
  | { type: 'actionHotkey'; key: string; hotkey: string }
  | { type: 'scope'; value: string }
  | { type: 'view'; value: string }
  | { type: 'project'; value: string }
  | { type: 'tagFilter'; value: string }
  | { type: 'loadMore' }
  | { type: 'editorField'; field: 'content' | 'project' | 'tag'; value: string }
  | { type: 'editorFavorite' }
  | { type: 'editorSubmit' }
  | { type: 'editorCancel' }
  | { type: 'tagComposerOpen'; anchor: string }
  | { type: 'tagComposerField'; field: 'name' | 'color'; value: string }
  | { type: 'tagComposerSubmit' }
  | { type: 'tagComposerCancel' };

export type NativeQuickAccessBridge = {
  postNativeQuickAccessSnapshot?: (payload: string) => boolean;
  onNativeQuickAccessCommand?: (command: QuickAccessCommand) => void;
};
