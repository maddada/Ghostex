/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { normalizeNonEmptyString } from './records';

export function normalizeGpuiWorkspaceFolderPick(
  payload: unknown
): { firstLaunchAgentId?: string; name?: string; path: string } | undefined {
  if (typeof payload !== 'object' || payload === null) {
    return undefined;
  }
  const record = payload as { firstLaunchAgentId?: unknown; name?: unknown; path?: unknown; type?: unknown };
  if (record.type !== 'workspaceFolderPicked') {
    return undefined;
  }
  const path = normalizeNonEmptyString(record.path);
  if (!path) {
    return undefined;
  }
  /*
  CDXC:FirstLaunchSetup 2026-08-24:
  The onboarding Finish step rides this same message: `firstLaunchAgentId` is a
  sidebar agent id (or 'terminal') asking the runtime to start the first
  session in the freshly registered project.
  */
  return {
    firstLaunchAgentId: normalizeNonEmptyString(record.firstLaunchAgentId),
    name: normalizeNonEmptyString(record.name),
    path,
  };
}

export function normalizeGpuiReplacementProjectFolderPick(
  payload: unknown
): { path: string; projectId: string } | undefined {
  if (typeof payload !== 'object' || payload === null) {
    return undefined;
  }
  const record = payload as { path?: unknown; projectId?: unknown; type?: unknown };
  if (record.type !== 'replacementProjectFolderPicked') {
    return undefined;
  }
  const path = normalizeNonEmptyString(record.path);
  const projectId = normalizeNonEmptyString(record.projectId);
  if (!path || !projectId) {
    return undefined;
  }
  return { path, projectId };
}
