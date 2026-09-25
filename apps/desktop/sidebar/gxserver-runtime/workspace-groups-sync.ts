/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiSidebarRuntime } from './core';
import { isSidebarProjectCollectionsState, isSidebarSpacesState } from './helpers/remote-presentation';
import type {
  GxserverSidebarProjectCollectionsState,
  GxserverSidebarSpacesState,
} from '@/packages/shared/gxserver-protocol';

/*
CDXC:RepoStructure 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeWorkspaceGroupMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeWorkspaceGroupMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeWorkspaceGroupMethods {
  forwardRemoteSidebarProjectCollectionsFromGxserver(
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState
  ): void;
  updateRemoteSidebarProjectCollections(
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState
  ): Promise<void>;
  forwardRemoteSidebarSpacesFromGxserver(remoteMachineId: string, state: GxserverSidebarSpacesState): void;
  updateRemoteSidebarSpaces(remoteMachineId: string, state: GxserverSidebarSpacesState): Promise<void>;
}

export const gpuiSidebarRuntimeWorkspaceGroupMethods = {
  /*
  CDXC:Projects 2026-09-21 WHY:
  The LOCAL half of this relay is gone, for both documents. `queueSidebarProjectCollectionsServerSync`,
  its debounced `pushSidebarProjectCollectionsToGxserver`, the forward suppression in
  `forwardSidebarProjectCollectionsFromGxserver` and the identical Spaces trio were what the
  2026-07-18-00:00 and 2026-08-27 notes described, and they stopped being reachable when Rust became
  the only desktop writer of this computer's copies (apps/desktop/src/app/gx_store/project_docs.rs,
  M4d part 2 blocker 3): nothing posts an `updateSidebarProjectCollections` or
  `updateSidebarSpaces` without a `remoteMachineId`, and the page that adopted the forward is
  deleted. Only the REMOTE methods are left, and they are direct tunnel calls with no queue and no
  pending flag. The deleted bodies are frozen for the gates in
  tooling/gx-core/project-docs-server-sync-typescript.ts.
  */
  forwardRemoteSidebarProjectCollectionsFromGxserver(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState
  ): void {
    const stateJson = JSON.stringify(state);
    if (this.lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId.get(remoteMachineId) === stateJson) {
      return;
    }
    this.lastForwardedRemoteSidebarProjectCollectionsJsonByMachineId.set(remoteMachineId, stateJson);
    this.messageSource.postMessage({
      remoteMachineId,
      sidebarProjectCollections: state,
      type: 'sidebarProjectCollectionsChanged',
    });
  },

  async updateRemoteSidebarProjectCollections(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    state: GxserverSidebarProjectCollectionsState
  ): Promise<void> {
    const response = await this.requestRemoteGxserver<{
      sidebarProjectCollections?: unknown;
    }>(remoteMachineId, '/api/updateSidebarProjectCollections', { state });
    if (!isSidebarProjectCollectionsState(response.sidebarProjectCollections)) {
      throw new Error('Remote gxserver returned invalid project collections.');
    }
    const snapshot = this.remotePresentations.get(remoteMachineId);
    if (snapshot) {
      this.remotePresentations.set(remoteMachineId, {
        ...snapshot,
        sidebarProjectCollections: response.sidebarProjectCollections,
      });
    }
    this.forwardRemoteSidebarProjectCollectionsFromGxserver(remoteMachineId, response.sidebarProjectCollections);
  },

  forwardRemoteSidebarSpacesFromGxserver(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    state: GxserverSidebarSpacesState
  ): void {
    const stateJson = JSON.stringify(state);
    if (this.lastForwardedRemoteSidebarSpacesJsonByMachineId.get(remoteMachineId) === stateJson) {
      return;
    }
    this.lastForwardedRemoteSidebarSpacesJsonByMachineId.set(remoteMachineId, stateJson);
    this.messageSource.postMessage({
      remoteMachineId,
      sidebarSpaces: state,
      type: 'sidebarSpacesChanged',
    });
  },

  async updateRemoteSidebarSpaces(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    state: GxserverSidebarSpacesState
  ): Promise<void> {
    const response = await this.requestRemoteGxserver<{
      sidebarSpaces?: unknown;
    }>(remoteMachineId, '/api/updateSidebarSpaces', { state });
    if (!isSidebarSpacesState(response.sidebarSpaces)) {
      throw new Error('Remote gxserver returned invalid sidebar spaces.');
    }
    const snapshot = this.remotePresentations.get(remoteMachineId);
    if (snapshot) {
      this.remotePresentations.set(remoteMachineId, {
        ...snapshot,
        sidebarSpaces: response.sidebarSpaces,
      });
    }
    this.forwardRemoteSidebarSpacesFromGxserver(remoteMachineId, response.sidebarSpaces);
  },
};

const gpuiSidebarRuntimeWorkspaceGroupMethodsShapeCheck: GpuiSidebarRuntimeWorkspaceGroupMethods =
  gpuiSidebarRuntimeWorkspaceGroupMethods;
void gpuiSidebarRuntimeWorkspaceGroupMethodsShapeCheck;
