export type GpuiResourcesSnapshotBridge = {
  onResourcesSnapshotResult?: (payload: unknown) => void;
  pendingResourcesSnapshotResults?: unknown[];
  postResourcesSnapshotRequest?: (payload: string) => boolean;
};

export type GpuiPendingResourcesSnapshotRequest = {
  reject: (error: Error) => void;
  resolve: (snapshot: Record<string, unknown>) => void;
  timeoutId: number;
};
