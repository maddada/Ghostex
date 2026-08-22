import type { SidebarHydrateMessage } from "./session-grid-contract";

export type NativeTerminalDebugProjection = {
  alias?: string;
  exitCode?: number;
  isParked: boolean;
  location: string;
  sessionId: string;
  terminalName?: string;
};

export type NativeTerminalDebugEditorSurfaceGroup = {
  labels: string[];
  viewColumn: number;
  visibleIndex: number;
};

export type NativeTerminalDebugMoveAction = {
  currentLocation?: string;
  desiredVisibleIndex?: number;
  detail?: string;
  event?: "complete" | "fallback" | "start";
  kind: string;
  sessionId?: string;
  startedAt: string;
  terminalName?: string;
  timestamp?: string;
};

export type NativeTerminalDebugProcessAssociation = Record<
  string,
  boolean | number | string | undefined
>;

export type NativeTerminalDebugLayout = {
  activeTerminalName?: string;
  editorSurfaceGroups: NativeTerminalDebugEditorSurfaceGroup[];
  parkedTerminals: NativeTerminalDebugProjection[];
  processAssociations: NativeTerminalDebugProcessAssociation[];
  projections: NativeTerminalDebugProjection[];
  rawTabGroups: Record<string, unknown>[];
  terminalCount: number;
  terminalNames: string[];
  trackedSessionIds: string[];
};

export type NativeTerminalDebugBackendState = {
  currentMoveAction?: NativeTerminalDebugMoveAction;
  lastVisibleSnapshot?: unknown;
  layout: NativeTerminalDebugLayout;
  moveHistory: NativeTerminalDebugMoveAction[];
  observedAt: string;
  workspaceId: string;
};

export type NativeTerminalDebugPanelState = {
  backend: NativeTerminalDebugBackendState;
  observedAt: string;
  sidebar: Pick<SidebarHydrateMessage, "groups" | "hud">;
  workspaceId: string;
};

export type NativeTerminalDebugHydrateMessage = {
  state: NativeTerminalDebugPanelState;
  type: "hydrate";
};

export type ExtensionToNativeTerminalDebugMessage = NativeTerminalDebugHydrateMessage;
