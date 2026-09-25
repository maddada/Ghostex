import type { GpuiSidebarRuntime } from './core';

/**
 * CDXC:Sidebar 2026-09-21 DECISION:
 * User (M4d part 2, question 1 option A): the facts the Rust sidebar still borrows from the page's
 * projection reach Rust on a narrow one-way channel the runtime posts, rather than by porting the
 * HUD, the git probe and the Close After Done timers into Rust now. The channel carries no
 * diffing and no patches and dies with QuickJS in M8.
 */
type SidebarRuntimeFactsBridge = {
  postSidebarRuntimeFacts?: (payload: string) => boolean;
};

function post(payload: unknown): void {
  const bridge = window.ghostexGpui as (typeof window.ghostexGpui & SidebarRuntimeFactsBridge) | undefined;
  bridge?.postSidebarRuntimeFacts?.(JSON.stringify(payload));
}

/**
 * CDXC:Sidebar 2026-09-25 WHY:
 * The HUD is composed in Rust since the app runtime port's F2 (apps/desktop/src/app/gx_store/hud/).
 * Its one input this runtime still writes, the remote machines' client-parked projects, goes over
 * on its own post, only when it moved.
 */
let lastPostedRemoteRecentProjects: string | undefined;

function postRemoteRecentProjects(runtime: GpuiSidebarRuntime): void {
  const remoteRecentProjects = JSON.stringify([...runtime.remoteRecentProjectsByMachineId]);
  if (remoteRecentProjects === lastPostedRemoteRecentProjects) return;
  lastPostedRemoteRecentProjects = remoteRecentProjects;
  post({ kind: 'remoteRecentProjects', remoteRecentProjects: JSON.parse(remoteRecentProjects), version: 1 });
}

/**
 * What every publish hands over. The per-row facts are all Rust's own now (Delayed Send since the
 * app runtime port's F2, Close After Done and the git numbers since F3 and F5), so this is the
 * remote machines' client-parked projects alone, which the Rust HUD reads.
 */
export function postGpuiSidebarRuntimeFactsRows(runtime: GpuiSidebarRuntime): void {
  postRemoteRecentProjects(runtime);
}
