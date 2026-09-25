/**
 * The TypeScript half of the drag gate.
 *
 * Nothing about a move is reimplemented here. `createSidebarGroups` builds the projection, the
 * shipped store normalizer turns it into `sessionIdsByGroup` / `groupsById` / `sessionsById`, the
 * shipped `reorderNativeSidebar` decides what a drop posts, and the shipped
 * `syncWorkspaceSubgroupSessionOrder`, `moveSessionToWorkspaceGroup`, `syncSessionOrder` and
 * `createWorkspaceGroupFromSession` decide what those posts write. Four edges are replaced, and
 * each one is an edge rather than a decision:
 *
 * - `post`, the message channel back to the runtime, is a recorder.
 * - `client.rpc` records the call instead of making it.
 * - `window.setTimeout` is a manual queue, because the real timer never resolves under this
 *   harness's window shim and hung the bulk gate before it printed a line.
 * - client storage is the harness's shim, so a write is counted rather than persisted.
 *
 * The MEMBERSHIP is derived here rather than taken from the Rust dump. Which rows a group holds, in
 * which order, is exactly the question a move is decided by, so handing both sides the same answer
 * would make the gate unable to see a wrong one: it is the same reason `action-parity-typescript`
 * builds `latestGroups` itself.
 */
// First, so the client-storage adapter finds a `Storage` before any module reads one.
import { resetBrowserStorage } from './browser-shim';
import { GpuiSidebarRuntime } from '@/apps/desktop/sidebar/gxserver-runtime/core';
import { frozenHandleWorkspaceGroupMessage, frozenWorkspaceGroupEditMethods } from './workspace-groups-edits-frozen';
import { reorderNativeSidebar } from '@/tooling/gx-core/sidebar-page-frozen/reorder';
import { sidebarStore } from '@/packages/core-ui/sidebar-store-model';
import { parseGpuiWorkspaceSessionGroupsState } from './workspace-session-groups-frozen';

type Json = Record<string, any>;

/** The shipped projection, as the runtime builds it for the sidebar store. */
function buildGroups(snapshot: Json, document: Json): Json[] {
  const runtime = Object.assign(Object.create(GpuiSidebarRuntime.prototype), frozenWorkspaceGroupEditMethods) as Json;
  runtime.workspaceGroups = parseGpuiWorkspaceSessionGroupsState(document);
  runtime.domainProjects = [];
  runtime.recentProjects = [];
  runtime.browserTabs = [];
  runtime.localFirstHiddenPresentationSessionKeys = new Set<string>();
  runtime.visibleSessionIds = new Set<string>();
  runtime.focusedSessionId = undefined;
  runtime.activeProjectId = undefined;
  runtime.activeGroupId = undefined;
  runtime.quickAutomationsOverviewOpen = false;
  runtime.presentation = snapshot;
  runtime.remotePresentations = new Map();
  runtime.projectDiffStatsByProjectId = new Map();
  runtime.getCloseAfterDoneProjection = () => undefined;
  runtime.getDelayedSendProjection = () => undefined;
  runtime.createRemoteSidebarGroups = () => [];
  runtime.overlayProjectDiffStats = (groups: Json[]) => groups;
  return runtime.createSidebarGroups(snapshot) as Json[];
}

/** Puts the projection into the sidebar store through its own normalizer. */
function seedStore(groups: Json[], sortMode: string): void {
  sidebarStore.getState().applySidebarMessage({
    type: 'hydrate',
    groups,
    hud: { activeSessionsSortMode: sortMode, commands: [] },
    revision: 1,
    pinnedPrompts: [],
    previousSessions: [],
  } as any);
}

/** A `NativeSidebarUiState` stand-in: the `moveSession` arm reads nothing off it. */
function uiState(): Json {
  return {
    selectedMachineId: 'local',
    metadata: { spaces: {}, collections: {}, updateSpaces: () => {} },
    collapse: { selectedSpaceIdBySectionKey: {}, collapsedGroupsById: {}, recentSessionIdsBySpace: {} },
  };
}

/**
 * Runs every case of the Rust dump through the shipped TypeScript, and returns what each one
 * posted plus what each post wrote, recorded after every step.
 */
export async function runTypeScriptDragCases(
  scenario: Json,
  dump: Json
): Promise<{ membershipByGroup: Json; cases: Json[]; creates: Json[] }> {
  resetBrowserStorage();
  const snapshot = scenario.snapshot as Json;
  const groups = buildGroups(snapshot, dump.document as Json);

  seedStore(groups, 'manual');
  const membershipByGroup: Json = {};
  for (const [groupId, sessionIds] of Object.entries(sidebarStore.getState().sessionIdsByGroup)) {
    membershipByGroup[groupId] = sessionIds;
  }

  const cases: Json[] = [];
  for (const entry of dump.cases as Json[]) {
    // The tag filter and the sort mode change what is DRAWN, never what `reorderNativeSidebar`
    // reads: it reads `sessionIdsByGroup`, which is the projection. Only the sort mode reaches it,
    // through the one `hud.activeSessionsSortMode` guard.
    seedStore(groups, entry.variant === 'lastActivity' ? 'lastActivity' : 'manual');
    const posted: Json[] = [];
    reorderNativeSidebar(uiState() as any, entry.command as any, (message: Json) => posted.push(message));
    const steps: Json[] = [];
    let document = dump.document as Json;
    for (const message of posted) {
      const result = await runOrderWrite(snapshot, document, message);
      document = result.document;
      steps.push({ message, writes: result.writes, document });
    }
    cases.push({ messages: posted, steps });
  }

  const creates: Json[] = [];
  for (const entry of dump.creates as Json[]) {
    const start = (entry.label === 'atTheLimit' ? dump.fullDocument : dump.document) as Json;
    creates.push({ ...(await runOrderWrite(snapshot, start, entry.message as Json)) });
  }
  return { membershipByGroup, cases, creates };
}

/**
 * One posted message, through the shipped runtime arm that answers it. The document it leaves
 * behind and the calls it made are what the gate compares.
 */
async function runOrderWrite(
  snapshot: Json,
  document: Json,
  message: Json
): Promise<{ document: Json; writes: Json[] }> {
  const writes: Json[] = [];
  const runtime = Object.assign(Object.create(GpuiSidebarRuntime.prototype), frozenWorkspaceGroupEditMethods) as Json;
  runtime.workspaceGroups = parseGpuiWorkspaceSessionGroupsState(document);
  runtime.domainProjects = [];
  runtime.recentProjects = [];
  runtime.latestGroups = [];
  // `syncSessionOrder` returns before its call when there is no presentation, so the real one is
  // here: a harness that left it undefined would report no call for every project-group drag and
  // would have agreed with a port that made none.
  runtime.presentation = snapshot;
  runtime.activeProjectId = undefined;
  runtime.activeGroupId = undefined;
  // `persistWorkspaceGroups` is where an edit leaves this page. Since M5 piece 7c it posts the
  // document to the app rather than writing client storage and pushing it, and either way it is an
  // edge; the guard that owns the write and the push is gated separately by
  // `workspace_groups_guard`. What this gate is about is the DOCUMENT each message leaves behind.
  // The identity semantics, captured at the shipped decision point: `persistWorkspaceGroups` runs
  // exactly when the TypeScript decided to write, including when the document it writes is equal to
  // the one it replaced. Comparing the documents instead would hide a write.
  runtime.persistWorkspaceGroups = () => writes.push({ write: 'editDocument', document: runtime.workspaceGroups });
  runtime.publishPresentation = () => {};
  runtime.publishRemotePresentationPatch = () => {};
  runtime.refreshSidebarHudFromClient = () => {};
  runtime.postSidebarActionToast = (level: string, title: string) => writes.push({ write: 'toast', level, title });
  runtime.client = {
    rpc: (path: string, params: Json) => {
      writes.push({ write: 'sessionOrderCall', rpc: { path, params } });
      return Promise.resolve({});
    },
  };

  await frozenHandleWorkspaceGroupMessage(runtime, message);
  if (message.type === 'createGroupFromSession' && runtime.activeGroupId)
    writes.push({
      write: 'activateSubgroup',
      activeProjectId: runtime.activeProjectId,
      activeGroupId: runtime.activeGroupId,
    });
  return { document: runtime.workspaceGroups, writes };
}
