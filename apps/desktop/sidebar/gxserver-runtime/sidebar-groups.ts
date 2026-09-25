/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import type { GpuiSidebarRuntime } from './core';
import { createEmptyGpuiAppUserData } from './helpers/bootstrap';
import type { GpuiSidebarRuntimeSnapshotKind } from './types-and-protocol';
import { postGpuiSidebarRuntimeFactsRows } from './sidebar-runtime-facts';

/*
CDXC:RepoStructure 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeSidebarGroupMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeSidebarGroupMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeSidebarGroupMethods {
  publishPresentation(kind: GpuiSidebarRuntimeSnapshotKind): void;
  publishUnavailable(_reason: string): void;
}

/*
CDXC:Sidebar 2026-09-25 WHY:
This runtime builds no sidebar projection any more. The Rust store draws the sidebar, owns focus,
the workspace session groups document and the remote machines' presentations, so the groups this
file used to build were read by nothing; it was deleted by the app runtime port's sweep (frozen for
the gates in tooling/gx-core/sidebar-projection-frozen.ts). What a publish still does is post the
one fact Rust borrows from here, the remote machines' client-parked projects.
*/
export const gpuiSidebarRuntimeSidebarGroupMethods = {
  publishPresentation(this: GpuiSidebarRuntime, _kind: GpuiSidebarRuntimeSnapshotKind): void {
    if (!this.presentation) {
      this.publishUnavailable('presentation-missing');
      return;
    }
    this.hasHydrated = true;
    postGpuiSidebarRuntimeFactsRows(this);
  },

  publishUnavailable(this: GpuiSidebarRuntime, _reason: string): void {
    this.presentation = undefined;
    this.appUserData = createEmptyGpuiAppUserData();
    this.domainProjects = [];
    this.recentProjects = [];
    this.hasHydrated = true;
    postGpuiSidebarRuntimeFactsRows(this);
  },
};

const gpuiSidebarRuntimeSidebarGroupMethodsShapeCheck: GpuiSidebarRuntimeSidebarGroupMethods =
  gpuiSidebarRuntimeSidebarGroupMethods;
void gpuiSidebarRuntimeSidebarGroupMethodsShapeCheck;
