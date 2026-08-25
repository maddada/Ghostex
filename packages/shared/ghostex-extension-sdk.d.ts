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
  permission?: 'exec' | 'cli' | 'ssh' | 'network' | 'clipboard';
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
}

declare global {
  interface Window {
    readonly ghostex: GhostexExtensionApi;
  }
}

export {};
