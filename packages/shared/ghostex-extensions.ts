export const GHOSTEX_EXTENSION_PLACEMENTS = ['view', 'chat-bar', 'popup', 'modal'] as const;

export type GhostexExtensionPlacement = (typeof GHOSTEX_EXTENSION_PLACEMENTS)[number];

export const GHOSTEX_EXTENSION_PERMISSIONS = ['exec', 'cli', 'ssh', 'network', 'clipboard'] as const;

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

export type GhostexExtensionServer = GhostexExtensionStaticServer | GhostexExtensionCommandServer;

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

export type GhostexExtensionManifest = GhostexWebExtensionManifest | GhostexTerminalExtensionManifest;

export interface GhostexExtensionStoreEntry {
  enabled: boolean;
  pinned: boolean;
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
    'enabled' | 'pinned' | 'placement' | 'terminalPlacement' | 'preferences' | 'storage' | 'grantedPermissions'
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

export interface GhostexChatBarPanelShowMessage {
  type: 'ghostexChatBarPanelShow';
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
