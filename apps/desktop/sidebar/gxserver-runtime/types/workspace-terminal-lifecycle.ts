export type GpuiWorkspaceTerminalLifecycleRequest = {
  action: 'close' | 'sleep' | 'wake';
  keepSidebarFocus: boolean;
  projectId: string;
  replacementProjectId?: string;
  replacementSessionId?: string;
  requestId: number;
  sessionId: string;
  skipReplacementFallback: boolean;
};
