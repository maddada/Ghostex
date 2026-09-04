/**
 * CDXC:Resources 2026-09-04 WHY:
 * `ghostex resources` needs the exact numbers the native Resources panel shows,
 * and only the Rust app can compute them (session titles, zmx names, and
 * browser tab ids live there). gxserver delivers the CLI's request as a
 * `readResourcesSnapshot` renderer command; this module forwards it to Rust
 * over the sidebar bridge and resolves the pending renderer command with the
 * JSON Rust posts back. The runtime never inspects or reshapes the snapshot.
 * SEE-ALSO: apps/desktop/src/app/titlebar/resources_snapshot_export.rs,
 * server/src/ghostex_cli/resources.rs.
 */
import type { GpuiSidebarRuntime } from './core';
import { isObjectRecord, readGpuiRecordString } from './helpers/records';

const RESOURCES_SNAPSHOT_REQUEST_TIMEOUT_MS = 10_000;

export interface GpuiSidebarRuntimeResourcesSnapshotMethods {
  requestNativeResourcesSnapshot(): Promise<Record<string, unknown>>;
  handleResourcesSnapshotResult(payload: unknown): void;
}

export const gpuiSidebarRuntimeResourcesSnapshotMethods = {
  requestNativeResourcesSnapshot(this: GpuiSidebarRuntime): Promise<Record<string, unknown>> {
    const post = window.ghostexGpui?.postResourcesSnapshotRequest;
    if (typeof post !== 'function') {
      return Promise.reject(new Error('Resources snapshot bridge unavailable.'));
    }
    const requestId = `resources-${Date.now().toString(36)}-${++this.resourcesSnapshotRequestSequence}`;
    return new Promise<Record<string, unknown>>((resolve, reject) => {
      const timeoutId = window.setTimeout(() => {
        this.pendingResourcesSnapshotRequests.delete(requestId);
        reject(new Error('Resources snapshot request timed out.'));
      }, RESOURCES_SNAPSHOT_REQUEST_TIMEOUT_MS);
      this.pendingResourcesSnapshotRequests.set(requestId, { reject, resolve, timeoutId });
      if (!post(JSON.stringify({ requestId }))) {
        window.clearTimeout(timeoutId);
        this.pendingResourcesSnapshotRequests.delete(requestId);
        reject(new Error('Resources snapshot bridge unavailable.'));
      }
    });
  },

  handleResourcesSnapshotResult(this: GpuiSidebarRuntime, payload: unknown): void {
    if (!isObjectRecord(payload)) {
      return;
    }
    const requestId = readGpuiRecordString(payload, 'requestId');
    if (!requestId) {
      return;
    }
    const pending = this.pendingResourcesSnapshotRequests.get(requestId);
    if (!pending) {
      return;
    }
    window.clearTimeout(pending.timeoutId);
    this.pendingResourcesSnapshotRequests.delete(requestId);
    const snapshot = payload.snapshot;
    if (!isObjectRecord(snapshot)) {
      pending.reject(new Error('Resources snapshot result was malformed.'));
      return;
    }
    pending.resolve(snapshot);
  },
};
