export type GhostexExtensionPlacement = 'view' | 'chat-bar' | 'popup' | 'modal';

export interface GhostexSessionDetails {
  title?: string;
  alias?: string;
  sessionId?: string;
  routingId?: string;
  kind?: string;
  status?: string;
  activity?: string;
  agent?: string;
  agentSessionId?: string;
  terminalTitle?: string;
  detail?: string;
  persistence?: string;
  remoteMachine?: string;
  project?: string;
  projectPath?: string;
  worktree?: string;
  worktreeBranch?: string;
  parentProject?: string;
  lastActive?: string;
}

export interface GhostexExtensionContext {
  activeSession?: GhostexSessionDetails;
  startSession?: GhostexSessionDetails;
  project: { name: string; path?: string };
  worktree: { isWorktree: boolean; branch?: string };
  placement: GhostexExtensionPlacement;
}

export interface GhostexCliResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

export interface GhostexExecChunk {
  stream: 'stdout' | 'stderr';
  text: string;
}

export interface GhostexExecOptions {
  cwd?: string;
  stream?: (chunk: GhostexExecChunk) => void;
}

export interface GhostexExecResult extends GhostexCliResult {}

export interface GhostexBridgeError extends Error {
  code: 'invalidRequest' | 'notFound' | 'permissionDenied' | 'operationFailed';
  permission?:
    | 'exec'
    | 'cli'
    | 'ssh'
    | 'network'
    | 'clipboard'
    | 'projects.read'
    | 'sessions.read'
    | 'sessions.open'
    | 'agents.start'
    | 'agents.message'
    | 'sessions.message';
}

export interface GhostexExtensionApi {
  readonly __bridgeVersion: 1;
  context(): Promise<GhostexExtensionContext>;
  onContextChange(callback: (context: GhostexExtensionContext) => void): () => void;
  cli(verb: string, args?: string[]): Promise<GhostexCliResult>;
  exec(command: string, options?: GhostexExecOptions): Promise<GhostexExecResult>;
  settings: {
    get(): Promise<Record<string, string | number | boolean>>;
    set(values: Record<string, string | number | boolean>): Promise<Record<string, string | number | boolean>>;
  };
  storage: {
    get<T = unknown>(key: string): Promise<T | null>;
    set<T = unknown>(key: string, value: T): Promise<Record<string, unknown>>;
  };
  ui: {
    toast(message: string): Promise<void>;
    close(): Promise<void>;
    setBadge(lines: string[]): Promise<void>;
  };
  /**
   * A sidebar extension's `web` block only (Ghostex 11.8 and newer): its own panel, read and written
   * through the extension API (`/api/ext/v1`) with the extension's permissions. Elsewhere these reject.
   */
  sidebar: {
    get(): Promise<{ revision: number; document: unknown }>;
    set(document: unknown, revision?: number): Promise<{ revision: number; document: unknown }>;
    patch(patch: unknown, revision?: number): Promise<{ revision: number; document: unknown }>;
    /** A tap on one of the panel's `extension` actions. */
    onAction(callback: (event: { name: string; blockId: string; itemId?: string; data?: unknown }) => void): () => void;
  };
  /** This computer's projects (needs `projects.read`). */
  projects(): Promise<
    Array<{ projectId: string; title: string; path: string; isPinned: boolean; isFavorite: boolean }>
  >;
  /** This computer's sessions, of one project when given (needs `sessions.read`). */
  sessions(project?: string): Promise<
    Array<{
      session: string;
      projectId: string;
      sessionId: string;
      title: string;
      agent: string;
      activity: string;
      lastActiveAt: string;
    }>
  >;
  /** `sessions` and `projects` changed (with the read permission), the `panel` changed, or an `action` was tapped. */
  on(
    name: 'sessions' | 'projects' | 'panel' | 'action',
    callback: (event: Record<string, unknown>) => void
  ): () => void;
}

declare global {
  interface Window {
    /**
     * In a native extension page Ghostex adds this once the page has loaded, after the page's own
     * scripts ran, and then fires `ghostexready` on `window`: a page that needs it at start waits for
     * that event when it is not there yet.
     */
    readonly ghostex: GhostexExtensionApi;
  }
}

export {};
