/**
 * The TypeScript half of the workspace-groups guard gate.
 *
 * **This is now a FROZEN REFERENCE, and saying so is the point.** Until M5 piece 7c it drove the
 * shipped `persistWorkspaceGroups`, `scheduleWorkspaceGroupsServerSync`,
 * `pushWorkspaceGroupsToGxserver` and `adoptWorkspaceGroupsFromGxserver` on
 * `GpuiSidebarRuntime.prototype`. Piece 7c deleted all four: the app is the only writer of
 * `ghostex-gpui-workspace-session-groups` and the only thing that pushes it to gxserver, and the
 * old runtime hands its edits over instead. A gate whose reference implementation no longer exists
 * either stops running or keeps a copy; a gate that stopped running is the failure mode this port
 * has hit repeatedly, so the four functions are copied here VERBATIM as they shipped on
 * 2026-09-21, with only the field names shortened to this file's local object.
 *
 * What that costs is real and is not hidden: from here the gate no longer proves that Rust matches
 * the app's TypeScript, because there is none. It proves that Rust still matches the behaviour that
 * was in the app when the port was made, which is what a regression test is. Changing the Rust
 * guard without changing this file still fails the gate; changing both to agree on something wrong
 * no longer fails it.
 *
 * Three edges are replaced and each one is an edge rather than a decision:
 *
 * - `window.setTimeout` is a manual queue, so "the booked push runs now" is an event in the script
 *   instead of a 400 ms wait. The bulk pacing probe learned this the hard way: driving the real
 *   timer under the harness's window shim hung the gate before it printed a line.
 * - `client.updateWorkspaceSessionGroups` returns a promise the script resolves or rejects, which
 *   is what makes an echo DURING a push expressible at all.
 * - client storage is the harness's shim, so a write is counted rather than persisted.
 *
 * `writeStoredGpuiWorkspaceSessionGroupsState` is frozen here too, for the same reason and with the
 * same consequence: it was deleted from the app because nothing called it any more, and a reference
 * implementation that depended on shipped code kept alive only for the reference would be a second
 * writer of `ghostex-gpui-workspace-session-groups` waiting for someone to wire it back.
 *
 * SEE-ALSO: packages/gx-core/src/workspace_groups/sync.rs,
 * apps/desktop/sidebar/gxserver-runtime/workspace-groups-sync.ts.
 */
import { resetBrowserStorage } from './browser-shim';
import { storageScope } from '@/packages/client-storage';
import {
  GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY,
  isEmptyGpuiWorkspaceSessionGroupsState,
  parseGpuiWorkspaceSessionGroupsState,
} from './workspace-session-groups-frozen';
import {
  GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS,
  GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS,
} from '@/apps/desktop/sidebar/gxserver-runtime/constants';

type Json = Record<string, any>;

/**
 * The four functions as they shipped, on a plain object with the same field names. Copied, not
 * rewritten: every branch, every identity test and the `catch` that retries for ever are the
 * originals.
 */
const clientStorage = storageScope(['workspaceGroups']);

/** `writeStoredGpuiWorkspaceSessionGroupsState`, as it shipped on 2026-09-21. */
function writeStoredGpuiWorkspaceSessionGroupsState(state: Json): void {
  try {
    if (state.projectOrder.length === 0 && Object.keys(state.projects).length === 0) {
      clientStorage.removeItem(GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY);
      return;
    }
    clientStorage.setItem(GPUI_WORKSPACE_SESSION_GROUPS_STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Storage availability must never gate sidebar group behavior.
  }
}

function createFrozenRuntime(start: Json, settleHolder: { settle?: (ok: boolean) => void }): Json {
  const runtime: Json = {
    workspaceGroups: parseGpuiWorkspaceSessionGroupsState(start),
    workspaceGroupsServerSyncPending: false,
    workspaceGroupsServerSyncTimeoutId: undefined as number | undefined,
    client: {
      updateWorkspaceSessionGroups: () =>
        new Promise<void>((resolve, reject) => {
          settleHolder.settle = (ok: boolean) => (ok ? resolve() : reject(new Error('offline')));
        }),
    },
  };

  runtime.persistWorkspaceGroups = (): void => {
    writeStoredGpuiWorkspaceSessionGroupsState(runtime.workspaceGroups);
    runtime.scheduleWorkspaceGroupsServerSync();
  };

  runtime.scheduleWorkspaceGroupsServerSync = (): void => {
    runtime.workspaceGroupsServerSyncPending = true;
    if (runtime.workspaceGroupsServerSyncTimeoutId !== undefined) {
      window.clearTimeout(runtime.workspaceGroupsServerSyncTimeoutId);
    }
    runtime.workspaceGroupsServerSyncTimeoutId = window.setTimeout(() => {
      runtime.workspaceGroupsServerSyncTimeoutId = undefined;
      void runtime.pushWorkspaceGroupsToGxserver();
    }, GPUI_WORKSPACE_GROUPS_SERVER_SYNC_DELAY_MS);
  };

  runtime.pushWorkspaceGroupsToGxserver = async (): Promise<void> => {
    const client = runtime.client;
    if (!client) {
      return;
    }
    const pushed = runtime.workspaceGroups;
    try {
      await client.updateWorkspaceSessionGroups(pushed);
      if (runtime.workspaceGroups === pushed) {
        runtime.workspaceGroupsServerSyncPending = false;
      }
    } catch {
      if (
        runtime.client === client &&
        runtime.workspaceGroupsServerSyncTimeoutId === undefined &&
        runtime.workspaceGroupsServerSyncPending
      ) {
        runtime.workspaceGroupsServerSyncTimeoutId = window.setTimeout(() => {
          runtime.workspaceGroupsServerSyncTimeoutId = undefined;
          void runtime.pushWorkspaceGroupsToGxserver();
        }, GPUI_WORKSPACE_GROUPS_SERVER_SYNC_RETRY_DELAY_MS);
      }
    }
  };

  runtime.adoptWorkspaceGroupsFromGxserver = (serverState: unknown): void => {
    if (serverState === undefined || runtime.workspaceGroupsServerSyncPending) {
      return;
    }
    const parsed = parseGpuiWorkspaceSessionGroupsState(serverState);
    if (isEmptyGpuiWorkspaceSessionGroupsState(parsed)) {
      if (!isEmptyGpuiWorkspaceSessionGroupsState(runtime.workspaceGroups)) {
        runtime.scheduleWorkspaceGroupsServerSync();
      }
      return;
    }
    if (JSON.stringify(parsed) === JSON.stringify(runtime.workspaceGroups)) {
      return;
    }
    runtime.workspaceGroups = parsed;
    writeStoredGpuiWorkspaceSessionGroupsState(parsed);
  };

  return runtime;
}

/** One script, run against the frozen guard. */
export async function runTypeScriptWorkspaceGroupsCase(
  start: Json,
  script: string[],
  documents: Json[]
): Promise<Json[]> {
  resetBrowserStorage();
  const timers: (() => void)[] = [];
  const realSetTimeout = globalThis.window.setTimeout;
  const realClearTimeout = globalThis.window.clearTimeout;
  // A manual timer queue. The id is the index; clearing replaces the callback with a no-op, which
  // is what `clearTimeout` means for a booking that was replaced by a newer one.
  (globalThis.window as any).setTimeout = (fn: () => void) => {
    timers.push(fn);
    return timers.length;
  };
  (globalThis.window as any).clearTimeout = (id: number) => {
    if (typeof id === 'number' && id >= 1 && id <= timers.length) timers[id - 1] = () => {};
  };
  const settleHolder: { settle?: (ok: boolean) => void } = {};
  const runtime = createFrozenRuntime(start, settleHolder);
  const steps: Json[] = [];
  try {
    for (const event of script) {
      switch (event) {
        case 'editA':
        case 'editB': {
          runtime.workspaceGroups = parseGpuiWorkspaceSessionGroupsState(documents[event === 'editA' ? 1 : 2]);
          runtime.persistWorkspaceGroups();
          break;
        }
        case 'echoNone':
          runtime.adoptWorkspaceGroupsFromGxserver(undefined);
          break;
        case 'echoEmpty':
          runtime.adoptWorkspaceGroupsFromGxserver({});
          break;
        case 'echoA':
          runtime.adoptWorkspaceGroupsFromGxserver(documents[1]);
          break;
        case 'echoB':
          runtime.adoptWorkspaceGroupsFromGxserver(documents[2]);
          break;
        case 'pushStart': {
          // Fire whichever booking is outstanding, which is what the real timer would do.
          const id = runtime.workspaceGroupsServerSyncTimeoutId;
          if (typeof id === 'number' && id >= 1 && id <= timers.length) {
            const fn = timers[id - 1];
            timers[id - 1] = () => {};
            fn();
          }
          await flush();
          break;
        }
        case 'pushOk':
        case 'pushFail': {
          settleHolder.settle?.(event === 'pushOk');
          settleHolder.settle = undefined;
          await flush();
          break;
        }
      }
      steps.push({
        event,
        document: runtime.workspaceGroups,
        pending: runtime.workspaceGroupsServerSyncPending === true,
      });
    }
  } finally {
    (globalThis.window as any).setTimeout = realSetTimeout;
    (globalThis.window as any).clearTimeout = realClearTimeout;
  }
  return steps;
}

/** Lets the promise chain inside `pushWorkspaceGroupsToGxserver` run to its `catch`. */
async function flush(): Promise<void> {
  for (let index = 0; index < 8; index += 1) await Promise.resolve();
}
