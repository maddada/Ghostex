export const GHOSTEX_EXTENSION_PLACEMENTS = ['view', 'chat-bar', 'popup', 'modal'] as const;

export type GhostexExtensionPlacement = (typeof GHOSTEX_EXTENSION_PLACEMENTS)[number];

/**
 * `projects.read`, `sessions.read`, `sessions.open` and `agents.start` are the scoped permissions of
 * sidebar extensions (CDXC:Extensions 2026-11-14 in server/src/extensions/types.rs): they reach only
 * `/api/ext/v1` and the panel actions clients run.
 */
export const GHOSTEX_EXTENSION_PERMISSIONS = [
  'exec',
  'cli',
  'ssh',
  'network',
  'clipboard',
  'projects.read',
  'sessions.read',
  'sessions.open',
  'agents.start',
] as const;

export type GhostexExtensionPermission = (typeof GHOSTEX_EXTENSION_PERMISSIONS)[number];

export type GhostexExtensionTerminalPlacement = 'splitRight' | 'tab';

export type GhostexExtensionPreferenceType = 'textfield' | 'password' | 'checkbox' | 'dropdown' | 'file' | 'directory';

export type GhostexExtensionPreferenceValue = string | boolean | number;

export interface GhostexExtensionPreferenceOption {
  title: string;
  value: string;
}

export interface GhostexExtensionPreference {
  name: string;
  title: string;
  description: string;
  type: GhostexExtensionPreferenceType;
  required?: boolean;
  default?: GhostexExtensionPreferenceValue;
  placeholder?: string;
  data?: GhostexExtensionPreferenceOption[];
}

export interface GhostexExtensionSize {
  width: number;
  height: number;
}

export interface GhostexExtensionReadiness {
  httpGet: string;
  timeoutSeconds?: number;
}

export interface GhostexExtensionPlatformInstall {
  url: string;
  sha256: string;
}

export type GhostexExtensionPlatformInstalls = Record<string, GhostexExtensionPlatformInstall>;

export interface GhostexExtensionStaticServer {
  static: string;
}

export interface GhostexExtensionCommandServer {
  command: string;
  cwd?: string;
  readiness: GhostexExtensionReadiness;
  install?: GhostexExtensionPlatformInstalls;
}

/**
 * A fixed remote page gxserver neither starts nor serves. It is ready as soon
 * as the extension is installed, and the desktop app opens it without the
 * extension bridge because the origin is not first-party.
 */
export interface GhostexExtensionUrlServer {
  url: string;
}

export type GhostexExtensionServer =
  | GhostexExtensionStaticServer
  | GhostexExtensionCommandServer
  | GhostexExtensionUrlServer;

export interface GhostexExtensionTerminal {
  command: string;
  cwd?: string;
  requires?: string[];
}

interface GhostexExtensionManifestBase {
  $schema?: string;
  name: string;
  title: string;
  description: string;
  version: string;
  author: string;
  icon: string;
  categories: string[];
  preferences?: GhostexExtensionPreference[];
  permissions?: GhostexExtensionPermission[];
}

export interface GhostexWebExtensionManifest extends GhostexExtensionManifestBase {
  placements: GhostexExtensionPlacement[];
  defaultPlacement: GhostexExtensionPlacement;
  server: GhostexExtensionServer;
  modal?: GhostexExtensionSize;
  popup?: GhostexExtensionSize;
  kind?: never;
  terminal?: never;
}

export interface GhostexTerminalExtensionManifest extends GhostexExtensionManifestBase {
  kind: 'terminal-pane';
  terminal: GhostexExtensionTerminal;
  placements?: never;
  defaultPlacement?: never;
  server?: never;
  modal?: never;
  popup?: never;
}

export type GhostexSidebarSectionPlace = 'projectsTop' | 'projectsBottom';

export interface GhostexExtensionSidebar {
  /** Its own sidebar mode: a top-row button that swaps the whole list, like Bots. */
  mode?: { title: string; icon?: string };
  /** Blocks of the sidebar panel shown inside the Projects list. */
  sections?: { block: string; where: GhostexSidebarSectionPlace }[];
  /** The first panel document, a JSON file in the extension. */
  panel?: string;
}

export interface GhostexExtensionSidePanel {
  title: string;
  icon?: string;
  /** One document for the computer (`global`, the default) or one per session. */
  scope?: 'global' | 'session';
  panel?: string;
}

/** Sidebar content drawn by Ghostex from a panel document (see `GhostexPanelDocument`). */
export interface GhostexSidebarExtensionManifest extends GhostexExtensionManifestBase {
  kind: 'sidebar';
  sidebar?: GhostexExtensionSidebar;
  sidePanel?: GhostexExtensionSidePanel;
  /** Optional: a command server for outside data and custom actions, or static files. */
  server?: GhostexExtensionStaticServer | GhostexExtensionCommandServer;
  placements?: never;
  defaultPlacement?: never;
  terminal?: never;
  modal?: never;
  popup?: never;
}

export type GhostexExtensionManifest =
  | GhostexWebExtensionManifest
  | GhostexTerminalExtensionManifest
  | GhostexSidebarExtensionManifest;

// ---- sidebar and side panel panels (packages/gx-protocol/src/extension_panel.rs) ----------------

export type GhostexPanelAction =
  | { openProject: string }
  | { focusSession: string }
  | { newAgent: { project: string; draft?: string; agent?: string } }
  | { openUrl: string }
  | { openView: string }
  | { extension: string | { name: string; data?: unknown } };

export interface GhostexPanelMenuItem {
  title: string;
  icon?: string;
  action: GhostexPanelAction;
}

export interface GhostexPanelRow {
  id?: string;
  icon?: string;
  dot?: 'green' | 'yellow' | 'red' | 'blue' | 'gray';
  title?: string;
  subtitle?: string;
  trailing?: string;
  badge?: string;
  /** A project path or id; Ghostex fills in its name, status and menu. */
  project?: string;
  /** A session ref `<projectId>:<sessionId>`. */
  session?: string;
  action?: GhostexPanelAction;
  menu?: GhostexPanelMenuItem[];
}

export interface GhostexPanelChecklistItem {
  id: string;
  text: string;
  done: boolean;
  action?: GhostexPanelAction;
  menu?: GhostexPanelMenuItem[];
}

interface GhostexPanelBlockBase {
  id: string;
  tab?: string;
  title?: string;
}

export type GhostexPanelBlock = GhostexPanelBlockBase &
  (
    | { type: 'rows'; items: GhostexPanelRow[] }
    | { type: 'checklist'; editable?: boolean; items: GhostexPanelChecklistItem[] }
    | {
        type: 'map';
        root?: { title: string; subtitle?: string; icon?: string };
        groups: { title: string; subtitle?: string; icon?: string; items: GhostexPanelRow[] }[];
      }
    | { type: 'progress'; value: number; max?: number; label?: string }
    | { type: 'text'; text: string }
    | {
        type: 'buttons';
        items: {
          id?: string;
          title: string;
          icon?: string;
          style?: 'primary' | 'default';
          action: GhostexPanelAction;
        }[];
      }
    | { type: 'empty'; subtitle?: string; icon?: string; action?: GhostexPanelAction }
    /** A small page of the extension's own (an .html file in its folder), 40 to 480 points tall (120 when absent). */
    | { type: 'web'; path: string; height?: number }
  );

export interface GhostexPanelDocument {
  schema: 1;
  title?: string;
  tabs?: { id: string; title: string; icon?: string }[];
  blocks: GhostexPanelBlock[];
}

/** One sidebar extension in the presentation snapshot's optional `extensionPanels` field. */
export interface GhostexExtensionPanelEntry {
  title: string;
  icon?: string;
  grantedPermissions: GhostexExtensionPermission[];
  sidebar?: {
    mode?: { title: string; icon?: string };
    sections: { block: string; where: GhostexSidebarSectionPlace }[];
    revision: number;
    document: GhostexPanelDocument;
  };
  sidePanel?: {
    title: string;
    icon?: string;
    scope: 'global' | 'session';
    revision: number;
    document: GhostexPanelDocument;
    sessions?: Record<string, { revision: number; document: GhostexPanelDocument }>;
  };
  /** Where the extension's `web` blocks load from (`http://127.0.0.1:<port>/ext/<id>/`). */
  pageBase?: string;
}

export type GhostexExtensionPanels = Record<string, GhostexExtensionPanelEntry>;

/** What an extension hears on `GET /api/ext/v1/events` (server-sent events, `event:` = `type`). */
export type GhostexExtensionApiEvent =
  | { type: 'ready'; extension: string }
  | {
      type: 'panelChanged';
      surface: 'sidebar' | 'sidePanel';
      session?: string;
      revision: number;
      by: 'extension' | 'user';
    }
  | {
      type: 'checklistEdited';
      surface: 'sidebar' | 'sidePanel';
      session?: string;
      blockId: string;
      revision: number;
      edit:
        | { op: 'check'; itemId: string; done: boolean }
        | { op: 'add'; text: string; itemId: string | null }
        | { op: 'rename'; itemId: string; text: string }
        | { op: 'delete'; itemId: string };
    }
  | {
      type: 'panelAction';
      surface: 'sidebar' | 'sidePanel';
      session?: string;
      blockId: string;
      itemId?: string;
      name: string;
      data?: unknown;
    }
  | { type: 'projectsChanged'; projectIds: string[] }
  | { type: 'sessionsChanged'; sessions: string[] }
  | { type: 'lagged'; missed: number };

export interface GhostexExtensionStoreEntry {
  enabled: boolean;
  pinned: boolean;
  chatBarAutoOpen: boolean;
  placement?: GhostexExtensionPlacement;
  terminalPlacement: GhostexExtensionTerminalPlacement;
  preferences: Record<string, GhostexExtensionPreferenceValue>;
  storage: Record<string, unknown>;
  version: string;
  grantedPermissions: GhostexExtensionPermission[];
}

export type GhostexExtensionStatePatch = Partial<
  Pick<
    GhostexExtensionStoreEntry,
    | 'enabled'
    | 'pinned'
    | 'chatBarAutoOpen'
    | 'placement'
    | 'terminalPlacement'
    | 'preferences'
    | 'storage'
    | 'grantedPermissions'
  >
>;

export interface GhostexInstalledExtension {
  id: string;
  manifest: GhostexExtensionManifest;
  state: GhostexExtensionStoreEntry;
  runtime: GhostexExtensionRuntimeStatus;
  badge?: GhostexExtensionBadge;
}

export type GhostexExtensionRuntimeState = 'stopped' | 'starting' | 'ready' | 'failed';

export interface GhostexExtensionRuntimeStatus {
  state: GhostexExtensionRuntimeState;
  url?: string;
  pid?: number;
  error?: string;
}

export interface GhostexExtensionBadge {
  lines: string[];
}

export interface GhostexExtensionLaunchContext {
  sessionId?: string;
  projectPath?: string;
  projectName?: string;
  worktree?: boolean;
  worktreeBranch?: string;
}

export type GhostexExtensionCatalogEntry = GhostexExtensionManifest & {
  readme: string;
  changelog: string;
  screenshots: string[];
  zip: string;
  sha256: string;
};

export interface GhostexExtensionCatalog {
  schemaVersion: number;
  publishedAt: string;
  extensions: GhostexExtensionCatalogEntry[];
}

export interface GhostexListExtensionsRequest {
  type: 'listExtensions';
}

export interface GhostexInstallExtensionRequest {
  type: 'installExtension';
  id?: string;
  localPath?: string;
  url?: string;
  sha256?: string;
}

export interface GhostexUninstallExtensionRequest {
  type: 'uninstallExtension';
  id: string;
}

export interface GhostexSetExtensionStateRequest {
  type: 'setExtensionState';
  id: string;
  patch: GhostexExtensionStatePatch;
}

export interface GhostexStartExtensionRequest {
  type: 'startExtension';
  id: string;
  context?: GhostexExtensionLaunchContext;
}

export interface GhostexStopExtensionRequest {
  type: 'stopExtension';
  id: string;
}

export interface GhostexExtensionStatusRequest {
  type: 'extensionStatus';
  id: string;
}

export interface GhostexSetExtensionBadgeRequest {
  type: 'setExtensionBadge';
  id: string;
  lines: string[];
}

export type GhostexExtensionSidebarRequest =
  | GhostexListExtensionsRequest
  | GhostexInstallExtensionRequest
  | GhostexUninstallExtensionRequest
  | GhostexSetExtensionStateRequest
  | GhostexStartExtensionRequest
  | GhostexStopExtensionRequest
  | GhostexExtensionStatusRequest
  | GhostexSetExtensionBadgeRequest;

export interface GhostexListExtensionsResult {
  extensions: GhostexInstalledExtension[];
}

export interface GhostexExtensionsCatalogResult {
  catalog: GhostexExtensionCatalog;
  source: 'remote' | 'cache';
  url: string;
}

export interface GhostexInstallExtensionResult {
  extension: GhostexInstalledExtension;
}

export interface GhostexUninstallExtensionResult {
  id: string;
  uninstalled: true;
}

export interface GhostexSetExtensionStateResult {
  extension: GhostexInstalledExtension;
}

export interface GhostexExtensionRuntimeResult {
  status: GhostexExtensionRuntimeStatus;
}

export interface GhostexSetExtensionBadgeResult {
  id: string;
  badge: GhostexExtensionBadge;
}

/*
Chat-bar pages are CEF subframes, while the native extension bridge is
main-frame-only. A proxy-aware vendored SDK posts only these typed messages to
its parent; the chat panel verifies the frame window and loopback origin before
answering. This protocol deliberately has no generic script/eval message.
*/
export const GHOSTEX_CHAT_BAR_PANEL_STORAGE_KEY = 'ghostex.chatBar.panelSessions';
export const GHOSTEX_CHAT_BAR_BRIDGE_VERSION = 1 as const;

export interface GhostexChatBarPanelSessionState {
  open: boolean;
  minimized: boolean;
  activeExtensionId?: string;
}

export type GhostexChatBarPanelSessions = Record<string, GhostexChatBarPanelSessionState>;

export interface GhostexChatBarPanelToggleMessage {
  type: 'ghostexChatBarPanelToggle';
  extensionId: string;
}

export interface GhostexChatBarPanelStateMessage {
  type: 'ghostexChatBarPanelState';
  sessionKey: string;
  state: GhostexChatBarPanelSessionState;
}

export type GhostexChatBarBridgeMethod =
  | 'context'
  | 'cli'
  | 'exec'
  | 'settings.get'
  | 'settings.set'
  | 'storage.get'
  | 'storage.set'
  | 'ui.toast'
  | 'ui.close'
  | 'ui.setBadge';

export interface GhostexChatBarBridgeRequestMessage {
  type: 'ghostexChatBarBridgeRequest';
  bridgeVersion: typeof GHOSTEX_CHAT_BAR_BRIDGE_VERSION;
  requestId: string;
  method: GhostexChatBarBridgeMethod;
  params?: Record<string, unknown>;
}

export interface GhostexChatBarBridgeResponseMessage {
  type: 'ghostexChatBarBridgeResponse';
  bridgeVersion: typeof GHOSTEX_CHAT_BAR_BRIDGE_VERSION;
  requestId: string;
  ok: boolean;
  result?: unknown;
  error?: {
    code: 'invalidRequest' | 'notFound' | 'permissionDenied' | 'operationFailed';
    message: string;
    permission?: GhostexExtensionPermission;
  };
}

export interface GhostexChatBarBridgeChunkMessage {
  type: 'ghostexChatBarBridgeChunk';
  bridgeVersion: typeof GHOSTEX_CHAT_BAR_BRIDGE_VERSION;
  requestId: string;
  chunk: {
    stream: 'stdout' | 'stderr';
    text: string;
  };
}

export interface GhostexChatBarBridgeContextChangedMessage {
  type: 'ghostexChatBarBridgeContextChanged';
  bridgeVersion: typeof GHOSTEX_CHAT_BAR_BRIDGE_VERSION;
  context: import('./ghostex-extension-sdk').GhostexExtensionContext;
}

export interface GhostexChatBarBridgeReadyMessage {
  type: 'ghostexChatBarBridgeReady';
  bridgeVersion: typeof GHOSTEX_CHAT_BAR_BRIDGE_VERSION;
}

export const GHOSTEX_CHAT_BAR_CONTEXT_CHANGED_EVENT = 'ghostex:chat-bar-extension-context-changed';
