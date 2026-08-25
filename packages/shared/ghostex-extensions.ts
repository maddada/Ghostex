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
  version: string;
  grantedPermissions: GhostexExtensionPermission[];
}

export type GhostexExtensionStatePatch = Partial<
  Pick<
    GhostexExtensionStoreEntry,
    'enabled' | 'pinned' | 'placement' | 'terminalPlacement' | 'preferences' | 'grantedPermissions'
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
