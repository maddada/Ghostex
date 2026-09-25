/**
 * The gate for the PROJECT moves: what a project drag writes, compared against the shipped
 * TypeScript step by step.
 *
 * Nothing about a move is reimplemented here. `createSidebarGroups` builds the projection, the
 * shipped store normalizer turns it into `groupOrder` / `groupsById`, and the shipped
 * `reorderNativeSidebar`, `runNativeProjectDrop`, `runNativeMembershipAction` and
 * `syncWorkspaceGroupOrder` decide what each payload does. Five edges are replaced, and each one is
 * an edge rather than a decision:
 *
 * - `post`, the message channel back to the runtime, is a recorder.
 * - `window.webkit.messageHandlers.ghostexNativeHost` is a recorder, which is where
 *   `saveNativeCollections` and `metadata.updateSpaces` hand their documents to the app since M5
 *   piece 7d. Recording the hand-off rather than the old write is the point: that IS the write now.
 * - `window.webkit.messageHandlers.ghostexAppModalHost` is a recorder, so New Space is compared
 *   rather than thrown.
 * - `client.rpc` records the call instead of making it.
 * - client storage is the harness's shim.
 *
 * **The GROUP ORDER is derived here rather than taken from the Rust dump.** Which groups the
 * machine has, in which order, is exactly the question a project move is decided by, so handing
 * both sides the same answer would make the gate unable to see a wrong one. Only the presentation
 * and the two documents come from the dump.
 *
 *   cargo run --release --example sidebar_project_move_parity -- <out-dir>
 *   bun tooling/gx-core/project-drag-parity.ts <out-dir> [--inject <mutation>]
 */
// First, so the client-storage adapter finds a `Storage` before any module reads one.
import { resetBrowserStorage, writeStorageItem } from './browser-shim';
import { FrozenProjectDocServerSync } from './project-docs-server-sync-typescript';
import { GpuiSidebarRuntime } from '@/apps/desktop/sidebar/gxserver-runtime/core';
import { frozenWorkspaceGroupEditMethods } from './workspace-groups-edits-frozen';
import { reorderNativeSidebar } from '@/tooling/gx-core/sidebar-page-frozen/reorder';
import { runNativeProjectDrop } from '@/tooling/gx-core/sidebar-page-frozen/project-drag';
import { runNativeMembershipAction } from '@/tooling/gx-core/sidebar-page-frozen/membership';
import { sidebarStore } from '@/packages/core-ui/sidebar-store-model';
import { parseGpuiWorkspaceSessionGroupsState } from './workspace-session-groups-frozen';
import {
  parseSidebarProjectCollectionsFromGxserver,
  serializeSidebarProjectCollectionsForGxserver,
} from '@/packages/core-ui/project-collections';
import { parseSidebarSpacesFromGxserver } from '@/packages/core-ui/spaces';

type Json = Record<string, any>;

/** The recorders every case is driven through. */
type Recorders = {
  writes: Json[];
  posted: Json[];
};

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
function seedStore(groups: Json[], spacesEnabled: boolean): void {
  sidebarStore.getState().applySidebarMessage({
    type: 'hydrate',
    groups,
    hud: { activeSessionsSortMode: 'manual', commands: [], settings: { sidebarSpacesEnabled: spacesEnabled } },
    revision: 1,
    pinnedPrompts: [],
    previousSessions: [],
  } as any);
}

/**
 * The `NativeSidebarUiState` stand-in, with the two documents the moves read and the three fields
 * they write. Rebuilt for every case, so one case cannot carry its edit into the next.
 */
function uiState(dump: Json, machineId: string, recorders: Recorders): Json {
  const collections = parseSidebarProjectCollectionsFromGxserver(serializeFromStorage(dump.collections as Json)) ?? {
    collections: [],
    nextCollectionNumber: 1,
  };
  const spaces = parseSidebarSpacesFromGxserver(dump.spaces);
  const ui: Json = {
    selectedMachineId: machineId,
    sectionKey: machineId === 'local' ? 'local' : `remote:${machineId}`,
    hiddenItems: { groupIds: [], collectionKeys: ['local:C3'] },
    collapse: { selectedSpaceIdBySectionKey: {}, collapsedGroupsById: {}, recentSessionIdsBySpace: {} },
    renameRequest: undefined,
    apply: () => {},
    metadata: {
      collections: { [machineId]: collections },
      spaces: { [machineId]: spaces },
      updateSpaces(id: string, next: Json) {
        ui.metadata.spaces[id] = next;
        // The edge, not a decision: since piece 7d this posts to the app rather than pushing.
        hostPost({ type: 'persistSidebarSpaces', state: serializeSpaces(next) }, recorders);
      },
    },
  };
  return ui;
}

/** The local ordered-array shape back into the wire shape the parser takes. */
function serializeFromStorage(stored: Json): Json {
  return serializeSidebarProjectCollectionsForGxserver({
    collections: (stored.collections ?? []) as any,
    nextCollectionNumber: stored.nextCollectionNumber as number,
  });
}

function serializeSpaces(state: Json): Json {
  return {
    order: [...state.order],
    spaces: Object.fromEntries(
      state.order.flatMap((spaceId: string) => {
        const space = state.spaces[spaceId];
        return space
          ? [
              [
                spaceId,
                {
                  color: space.color,
                  icon: space.icon,
                  memberCollectionIds: [...space.memberCollectionIds],
                  memberProjectIds: [...space.memberProjectIds],
                  name: space.name,
                  spaceId: space.spaceId,
                },
              ],
            ]
          : [];
      })
    ),
  };
}

/** The ordered-array shape the stored key holds, in the key order the Rust probe writes. */
function storageShape(state: Json): Json {
  return {
    collections: state.collections.map((collection: Json) => ({
      collectionId: collection.collectionId,
      color: collection.color,
      projectIds: [...collection.projectIds],
      title: collection.title,
    })),
    nextCollectionNumber: state.nextCollectionNumber,
  };
}

/** The app's two bridges, recording instead of crossing a process boundary. */
function hostPost(message: Json, recorders: Recorders): void {
  if (message.type === 'persistProjectCollections') {
    recorders.writes.push({
      write: 'editCollections',
      document: storageShape(parseSidebarProjectCollectionsFromGxserver(message.state)!),
      wire: message.state,
    });
    return;
  }
  if (message.type === 'persistSidebarSpaces') {
    recorders.writes.push({ write: 'editSpaces', wire: message.state });
  }
}

function installBridges(recorders: Recorders): void {
  const win = globalThis as Json;
  win.window = win.window ?? win;
  win.window.webkit = {
    messageHandlers: {
      ghostexNativeHost: { postMessage: (message: Json) => hostPost(message, recorders) },
      ghostexAppModalHost: {
        postMessage: (message: Json) => {
          if (message?.modal !== 'sidebarSpaceEditor') return;
          recorders.writes.push({
            write: 'openSpaceEditor',
            sectionKey: message.sectionKey,
            remoteMachineId: message.remoteMachineId ?? null,
            memberCollectionId: message.memberCollectionId ?? null,
            memberProjectId: message.memberProjectId ?? null,
          });
        },
      },
    },
  };
}

/** One posted `syncGroupOrder`, through the shipped runtime arm that answers it. */
async function runGroupOrder(snapshot: Json, document: Json, groupIds: string[]): Promise<Json> {
  const runtime = Object.assign(Object.create(GpuiSidebarRuntime.prototype), frozenWorkspaceGroupEditMethods) as Json;
  runtime.workspaceGroups = parseGpuiWorkspaceSessionGroupsState(document);
  runtime.domainProjects = [];
  runtime.recentProjects = [];
  runtime.latestGroups = [];
  runtime.presentation = snapshot;
  runtime.remotePresentations = new Map();
  runtime.persistWorkspaceGroups = () => {};
  runtime.publishPresentation = () => {};
  runtime.publishRemotePresentationPatch = () => {};
  await runtime.syncWorkspaceGroupOrder(groupIds);
  return runtime.workspaceGroups;
}

/** Everything one case does, in the shape the Rust probe records it. */
export async function runTypeScriptProjectMoveCases(dump: Json): Promise<Json[]> {
  resetBrowserStorage();
  // The CLOCK is an edge, and it is the sixth one replaced here: `createSidebarProjectCollection`
  // mints its id from `Date.now().toString(36)`, so a real clock would make every created
  // collection's id differ between the two halves and every later comparison meaningless.
  const realNow = Date.now;
  Date.now = () => dump.createMs as number;
  try {
    return await runCases(dump);
  } finally {
    Date.now = realNow;
  }
}

async function runCases(dump: Json): Promise<Json[]> {
  const snapshot = (dump.scenario as Json).snapshot as Json;
  const groups = buildGroups(snapshot, dump.document as Json);
  const results: Json[] = [];
  for (const entry of dump.cases as Json[]) {
    const recorders: Recorders = { writes: [], posted: [] };
    installBridges(recorders);
    const machineId = entry.variant === 'remoteTab' ? 'remote-ab12' : 'local';
    const spacesEnabled = entry.variant !== 'spacesOff';
    seedStore(groups, spacesEnabled);
    const ui = uiState(dump, machineId, recorders);
    const hiddenBefore = [...ui.hiddenItems.groupIds];
    const command = entry.command as Json;
    const post = (message: Json) => {
      recorders.posted.push(message);
      if (message.type === 'syncGroupOrder') recorders.writes.push({ write: 'groupOrder', groupIds: message.groupIds });
    };
    try {
      if (command.type === 'moveGroup' || command.type === 'moveSpace')
        reorderNativeSidebar(ui as any, command as any, post as any);
      else if (
        command.type === 'moveToSpace' ||
        command.type === 'moveToCollection' ||
        command.type === 'moveCollection'
      )
        runNativeProjectDrop(ui as any, command as any, post as any);
      else runNativeMembershipAction(ui as any, command as any, post as any);
    } catch (error) {
      results.push({ threw: String(error) });
      continue;
    }
    if (ui.renameRequest)
      recorders.writes.push({ write: 'renameCollection', collectionId: ui.renameRequest.collectionId });
    const hiddenAfter = ui.hiddenItems.groupIds as string[];
    if (hiddenBefore.length !== hiddenAfter.length) {
      const groupId = command.groupId as string;
      recorders.writes.push({
        write: 'hiddenGroup',
        groupId,
        hidden: hiddenAfter.includes(groupId),
      });
    }
    // The documents after every write, in order, which is what makes oscillation visible.
    let document = dump.document as Json;
    const steps: Json[] = [];
    let collections = storageShape(ui.metadata.collections[machineId]);
    let spaces = serializeSpaces(ui.metadata.spaces[machineId] ?? { order: [], spaces: {} });
    // Re-derived per write rather than taken at the end: a write that lands and is undone by the
    // next one has the same final state as one that never happened.
    let seenCollections = storageShape(
      parseSidebarProjectCollectionsFromGxserver(serializeFromStorage(dump.collections as Json))!
    );
    let seenSpaces = dump.spaces as Json;
    for (const write of recorders.writes) {
      if (write.write === 'editCollections') seenCollections = write.document;
      if (write.write === 'editSpaces') seenSpaces = write.wire;
      if (write.write === 'groupOrder') document = await runGroupOrder(snapshot, document, write.groupIds);
      steps.push({ write, collections: seenCollections, spaces: seenSpaces, document });
    }
    void collections;
    void spaces;
    results.push({ writes: recorders.writes, steps, posted: recorders.posted });
  }
  return results;
}

/**
 * The launch and counter halves, against the page's `local` adopt as it was on 2026-09-21, which is
 * the real twin of K5's guard: its `firstAdoption` flag is the first-echo-only empty rule and its
 * `Math.max(parsed, previous)` is the monotonic counter.
 *
 * FROZEN rather than shipped since the day the page stopped adopting this computer's echoes, and
 * `project-collections-typescript.ts` says what that costs. The pending half is frozen too since
 * 2026-09-21: `queueSidebarProjectCollectionsServerSync` and
 * `forwardSidebarProjectCollectionsFromGxserver` were deleted from the runtime as dead code and
 * live in `project-docs-server-sync-typescript.ts`. Driving the page alone is still the harness bug
 * this gate found on its first run, so the frozen pending half is still what suppresses the
 * forward.
 */
async function runTypeScriptLaunchCases(dump: Json): Promise<{ launch: Json[]; monotonic: Json[] }> {
  const { createFrozenCollectionsHolder, frozenAdoptCollections } = await import('./project-collections-typescript');
  const { readSidebarProjectCollections } = await import('@/packages/core-ui/project-collections');
  const launch: Json[] = [];
  // The frozen `queue` books a real timer. The push must NEVER fire here:
  // "a push is outstanding" is exactly the state these cases are about, and a timer that resolved
  // would clear the flag the guard is being asked about.
  const win = globalThis as Json;
  win.window.setTimeout = () => 1;
  win.window.clearTimeout = () => {};
  for (const entry of dump.launch as Json[]) {
    resetBrowserStorage();
    seedStoredCollections(entry.stored as Json);
    const holder = createFrozenCollectionsHolder(readSidebarProjectCollections());
    // **The TypeScript twin of K5's guard is NOT the adopt alone.** The pending flag lived in the
    // gxserver runtime, which suppressed a forward while a push was outstanding, and the page
    // adopted whatever reached it. Driving only the page made the SECOND empty echo look adopted
    // where the real app never delivers it, which is the harness bug this gate found on its first
    // run: the first cut would have reported a port bug that was not there.
    const pushed: Json[] = [];
    const sync = new FrozenProjectDocServerSync({
      client: undefined,
      isState: () => true,
      messageSource: {
        postMessage: (message: Json) =>
          frozenAdoptCollections(holder, 'local', message.sidebarProjectCollections, post),
      },
      messageType: 'sidebarProjectCollectionsChanged',
      stateKey: 'sidebarProjectCollections',
    });
    const post = (message: Json) => {
      if (message.type !== 'updateSidebarProjectCollections') return;
      pushed.push(message);
      sync.queue(message.state);
    };
    const steps: Json[] = [];
    for (const step of entry.steps as Json[]) {
      const before = pushed.length;
      sync.forward(step.echo);
      steps.push({
        held: storageShape(holder.collections.local),
        pushedBack: pushed.length - before,
      });
    }
    launch.push({ label: entry.label, steps });
  }
  const monotonic: Json[] = [];
  for (const entry of dump.monotonic as Json[]) {
    resetBrowserStorage();
    seedStoredCollections(entry.held as Json);
    const holder = createFrozenCollectionsHolder(readSidebarProjectCollections());
    frozenAdoptCollections(holder, 'local', entry.echo, () => {});
    const merged = storageShape(holder.collections.local);
    const { createSidebarProjectCollection, moveProjectsToSidebarCollection } =
      await import('@/packages/core-ui/project-collections');
    const now = Date.now;
    Date.now = () => dump.createMs as number;
    const created = createSidebarProjectCollection(holder.collections.local, 'P15');
    const createdState = moveProjectsToSidebarCollection(created.state, ['P15'], created.collectionId);
    Date.now = now;
    monotonic.push({ label: entry.label, merged, created: storageShape(createdState) });
  }
  return { launch, monotonic };
}

function seedStoredCollections(stored: Json): void {
  writeStorageItem('ghostex.sidebar.projectCollections.v1', JSON.stringify(stored));
}

/** A key-order-insensitive comparison, because the two sides build their objects differently. */
function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, entry) =>
    entry && typeof entry === 'object' && !Array.isArray(entry)
      ? Object.fromEntries(Object.entries(entry as Json).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : entry
  );
}

async function main(): Promise<void> {
  const [outDir, ...flags] = process.argv.slice(2);
  if (!outDir) throw new Error('usage: project-drag-parity.ts <out-dir> [--inject <mutation>]');
  const injectAt = flags.indexOf('--inject');
  const inject = injectAt >= 0 ? flags[injectAt + 1] : undefined;
  const path = `${outDir}/rust-project-move.json`;
  const dump = JSON.parse(await Bun.file(path).text()) as Json;
  if (inject) mutate(dump, inject);

  const ours = await runTypeScriptProjectMoveCases(dump);
  const { launch, monotonic } = await runTypeScriptLaunchCases(dump);

  let differences = 0;
  const shown: string[] = [];
  const counters: Json = {
    cases: 0,
    writes: 0,
    collectionEdits: 0,
    spaceEdits: 0,
    orderWrites: 0,
    renameRequests: 0,
    spaceEditors: 0,
    refusals: 0,
    handOffs: 0,
    handOffsPerformed: 0,
    collectionsEmptied: 0,
    orderDiffers: 0,
  };
  (dump.cases as Json[]).forEach((entry, index) => {
    counters.cases += 1;
    if (entry.refusal) counters.refusals += 1;
    if (entry.orderDiffersFromDrawn) counters.orderDiffers += 1;
    // A HAND-OFF is compared by what the TypeScript does with it, not skipped: the old runtime
    // performs the whole gesture, so the two sides really do differ there and the difference is the
    // point. That the TypeScript performs SOMETHING is now COUNTED rather than claimed in a
    // comment, because a refusal unreachable on both sides at once is a refusal nothing covers and
    // this port has shipped one. `projectMembership: hide` is the hand-off that made this worth
    // measuring: it writes no document, so the store leaves it to the sidebar-UI command path, which
    // is also what forwards it to the page.
    if (entry.handedOff) {
      counters.handOffs += 1;
      if (((ours[index] ?? {}).writes ?? []).length > 0) counters.handOffsPerformed += 1;
      return;
    }
    const theirs = ours[index] ?? {};
    const rustSteps = (entry.steps as Json[]).map((step) => ({
      write: step.write,
      collections: step.collections,
      spaces: step.spaces,
      document: step.document,
    }));
    for (const step of entry.steps as Json[]) {
      counters.writes += 1;
      if (step.write.write === 'editCollections') counters.collectionEdits += 1;
      if (step.write.write === 'editSpaces') counters.spaceEdits += 1;
      if (step.write.write === 'groupOrder') counters.orderWrites += 1;
      if (step.write.write === 'renameCollection') counters.renameRequests += 1;
      if (step.write.write === 'openSpaceEditor') counters.spaceEditors += 1;
      if (step.collectionsEmptied) counters.collectionsEmptied += 1;
    }
    if (canonical(rustSteps) !== canonical(theirs.steps ?? [])) {
      differences += 1;
      if (shown.length < 5)
        shown.push(
          `${entry.variant} ${entry.command.type} ${entry.command.groupId ?? entry.command.sourceId ?? entry.command.spaceId ?? ''}: rust ${canonical(rustSteps).slice(0, 320)} ts ${canonical(theirs.steps ?? []).slice(0, 320)}`
        );
    }
  });

  let launchDifferences = 0;
  (dump.launch as Json[]).forEach((entry, index) => {
    const theirs = launch[index] ?? {};
    const rust = (entry.steps as Json[]).map((step) => ({
      held: step.held,
      pushedBack: step.outcome === 'ScheduledPush' ? 1 : 0,
    }));
    if (canonical(rust) !== canonical(theirs.steps ?? [])) {
      launchDifferences += 1;
      if (shown.length < 8)
        shown.push(`launch ${entry.label}: rust ${canonical(rust)} ts ${canonical(theirs.steps ?? [])}`);
    }
  });
  let counterDifferences = 0;
  (dump.monotonic as Json[]).forEach((entry, index) => {
    const theirs = monotonic[index] ?? {};
    const rust = { merged: entry.merged, created: entry.created };
    if (canonical(rust) !== canonical({ merged: theirs.merged, created: theirs.created })) {
      counterDifferences += 1;
      if (shown.length < 10)
        shown.push(
          `counter ${entry.label}: rust ${canonical(rust)} ts ${canonical({ merged: theirs.merged, created: theirs.created })}`
        );
    }
  });

  const total = differences + launchDifferences + counterDifferences;
  const summary = Object.entries(counters)
    .map(([name, value]) => `${name} ${value}`)
    .join(' ');
  console.log(
    `project moves: ${summary} launchCases ${launch.length} counterCases ${monotonic.length} differences ${total}${inject ? ` (injected ${inject})` : ''}`
  );
  for (const line of shown) console.log(`  ${line}`);
  // A clean run whose coverage counters are all zero has measured nothing, so it FAILS: that is the
  // criterion piece 3a's `never-restore-the-row` mutation established.
  const zeroes = Object.entries(counters).filter(([, value]) => value === 0);
  if (!inject && zeroes.length > 0) {
    console.log(`  coverage counters at zero: ${zeroes.map(([name]) => name).join(', ')}`);
    process.exit(1);
  }
  if (inject) {
    if (total === 0) {
      console.log('  the mutation produced no difference: the gate did not measure it');
      process.exit(1);
    }
    return;
  }
  if (total > 0) process.exit(1);
}

/**
 * The mutations, each a plausible port mistake applied to the RUST dump. A mutation that produces
 * no difference is a gate that stopped measuring, which is why the run above fails on one.
 */
function mutate(dump: Json, name: string): void {
  const steps = (dump.cases as Json[]).flatMap((entry) => entry.steps as Json[]);
  switch (name) {
    // The tempting simplification: skip a write whose document did not move. The shipped code
    // writes either way, so this is the "one fewer push per drag" the gate exists to catch.
    case 'skip-the-no-op-write': {
      // The first cut of this mutation dropped writes whose document had NO collections, which is a
      // shape the fixture never produces: it changed nothing and reported zero differences, which
      // is the "mutation that cannot fail" this port has written four times. The real tempting
      // simplification is dropping a write whose document is EQUAL to the one it replaced, which is
      // what `moveProjectsToSidebarCollection` returns for an empty id list or a target that is not
      // there, and which `saveNativeCollections` writes anyway.
      const base = canonical(dump.collections);
      for (const entry of dump.cases as Json[]) {
        entry.steps = (entry.steps as Json[]).filter(
          (step) => step.write.write !== 'editCollections' || canonical(step.write.document) !== base
        );
        entry.writes = (entry.writes as Json[])?.filter(
          (write) => write.write !== 'editCollections' || canonical(write.document) !== base
        );
      }
      return;
    }
    // A project dropped into the middle of a collection reorders but does NOT join it, which is
    // exactly the bug `updateNativeProjectDropMembership` exists to prevent.
    case 'reorder-without-joining-the-collection':
      for (const entry of dump.cases as Json[]) {
        if (entry.command.type !== 'moveGroup') continue;
        entry.steps = (entry.steps as Json[]).filter((step) => step.write.write !== 'editCollections');
        entry.writes = (entry.writes as Json[])?.filter((write) => write.write !== 'editCollections');
      }
      return;
    // The family is one project rather than the whole worktree family, so a parent moves without
    // its worktrees.
    case 'move-the-project-without-its-worktrees':
      for (const step of steps) {
        if (step.write.write !== 'groupOrder') continue;
        step.write.groupIds = (step.write.groupIds as string[]).slice().reverse();
      }
      return;
    // The counter moves backwards with the server's, so the next folder reuses a name on screen.
    case 'let-the-counter-go-backwards':
      for (const entry of dump.monotonic as Json[]) {
        entry.merged.nextCollectionNumber = (entry.echo.nextCollectionNumber as number) || 1;
        entry.created.nextCollectionNumber = ((entry.echo.nextCollectionNumber as number) || 1) + 1;
      }
      return;
    // The empty-server rule applies to EVERY echo rather than the first, so the user who deleted
    // their last collection gets it back on the next echo for ever.
    case 'push-back-every-empty-echo':
      for (const entry of dump.launch as Json[]) {
        for (const step of entry.steps as Json[]) {
          if (step.outcome === 'Adopted' && (step.echo.order as string[]).length === 0) step.outcome = 'ScheduledPush';
        }
      }
      return;
    // A drop onto the built-in Other view is treated as an unknown Space and refused, where the
    // shipped code takes the member out of every Space.
    case 'refuse-the-other-view':
      for (const entry of dump.cases as Json[]) {
        if (entry.command.spaceId !== 'other') continue;
        entry.steps = [];
        entry.writes = [];
      }
      return;
    default:
      throw new Error(`unknown mutation: ${name}`);
  }
}

if (import.meta.main) await main();
