/**
 * The TypeScript half of the lifecycle transition gate: drives the shipped
 * `GpuiSidebarRuntime.setSessionSleeping` through all three daemon answers, and then through all
 * three echoes, and returns what it called and what it left behind.
 *
 * Nothing is reimplemented. `setSessionSleeping`, `gxserverSleepWasDeclined`,
 * `resolveLocalProjectListTransitionFocusTarget`, `localProjectTransitionSessionIds`,
 * `isRunningLocalPresentationSession`, `patchPresentationSession` and
 * `focusMovedElsewhereDuringWake` all run as they ship. Three seams are replaced, and each one is
 * an edge rather than a decision:
 *
 * - `client.rpc` is the daemon. It records the call and returns the answer the case is about.
 * - `focusLocalWorkspaceSession` is the workspace bridge. It records which session was selected,
 *   which is exactly what `LifecycleFollowUp::Focus` carries.
 * - `publishPresentation` is the redraw.
 *
 * The echo is applied with the shipped reducer `reduceGxserverPresentationDelta`, which is what
 * `applyPresentationDelta` uses to decide the resulting row; the bookkeeping that wraps it there
 * (domain projects, attention guards, completion sounds) never touches `lifecycleState`.
 */
// First, so the client-storage adapter finds a `Storage` before any module reads one.
import { resetBrowserStorage } from './browser-shim';
import { reduceGxserverPresentationDelta } from '@/packages/shared/gxserver-presentation-cache';
import { createGxserverPresentationSidebarSessionKey } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import {
  isSidebarSessionSnoozed,
  resolveSessionSnoozeWakeTime,
  SESSION_SNOOZE_PRESETS,
} from '@/packages/shared/session-snooze';
import { GpuiSidebarRuntime } from '@/apps/desktop/sidebar/gxserver-runtime/core';
import { buildLatestGroups } from './action-parity-typescript';

type Json = Record<string, any>;

/** What one starting state produced, in the shape the Rust dump is written in. */
export type LifecycleRun = {
  owned: true;
  rpc: { path: string; params: Json } | null;
  answers: Record<string, Json>;
  echo: Record<string, string | null>;
};

const SIDEBAR_SESSION_PREFIX = 'combined-session:';

function parseSidebarSessionId(id: string): { projectId: string; sessionId: string } | undefined {
  if (!id.startsWith(SIDEBAR_SESSION_PREFIX)) return undefined;
  const [projectId, sessionId] = id.slice(SIDEBAR_SESSION_PREFIX.length).split(':');
  if (!projectId || !sessionId) return undefined;
  return { projectId: decodeURIComponent(projectId), sessionId: decodeURIComponent(sessionId) };
}

function lifecycleOf(presentation: Json, projectId: string, sessionId: string): string | null {
  const row = (presentation?.sessions ?? []).find(
    (candidate: Json) => candidate.projectId === projectId && candidate.sessionId === sessionId
  );
  return row ? String(row.lifecycleState) : null;
}

/**
 * Runs one entry of the Rust dump's `lifecycle` list and returns the same four answers.
 *
 * `focus` names the starting focus the same way the Rust side does, and `elsewhereSessionId` is
 * the row `other` means, so both sides start from the identical state rather than from two
 * independently chosen rows.
 */
async function runOne(
  scenario: Json,
  entry: Json,
  latestGroups: unknown[],
  elsewhere: { projectId: string; sessionId: string } | undefined,
  answer: 'accepted' | 'declined' | 'failed'
): Promise<{ rpc: Json | null; focuses: Json[]; presentation: Json }> {
  const reference = parseSidebarSessionId(String(entry.sessionId));
  const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
  runtime.browserTabs = [];
  runtime.latestGroups = latestGroups;
  // Shared, not cloned. `reduceGxserverPresentationDelta` and everything that reaches
  // `this.presentation` here build new objects and never mutate the one they are given, and a
  // deep clone of a recorded snapshot per run made this harness three times slower than the whole
  // rest of the gate put together.
  runtime.presentation = scenario.snapshot;
  runtime.focusedSessionId =
    entry.focus === 'self' || entry.focus === 'movesDuringCall'
      ? reference?.sessionId
      : entry.focus === 'other'
        ? elsewhere?.sessionId
        : undefined;
  const focuses: Json[] = [];
  let rpc: Json | null = null;
  runtime.client = {
    rpc(path: string, params: Json) {
      rpc = { path, params };
      // The user picks another session WHILE the daemon is answering. `setSessionSleeping` read
      // `focusedSessionId` before the await and reads it again after, so moving it here is what
      // really happens, and it is the only way to reach `focusMovedElsewhereDuringWake`.
      if (entry.focus === 'movesDuringCall') runtime.focusedSessionId = elsewhere?.sessionId;
      if (answer === 'failed') return Promise.reject(new Error('transport'));
      // `gxserverSleepWasDeclined` tests the PRESENCE of the field, so a declined answer carries
      // one and an accepted one carries none.
      return Promise.resolve(answer === 'declined' ? { declined: { reason: 'keepAwake' } } : {});
    },
  };
  runtime.focusLocalWorkspaceSession = (projectId: string, sessionId: string) => {
    focuses.push({
      follow: 'focus',
      session: `${SIDEBAR_SESSION_PREFIX}${encodeURIComponent(projectId)}:${encodeURIComponent(sessionId)}`,
    });
  };
  runtime.publishPresentation = () => {};
  await runtime.setSessionSleeping(String(entry.sessionId), entry.sleeping === true).catch(() => undefined);
  return { rpc, focuses, presentation: runtime.presentation };
}

export async function runTypeScriptLifecycle(scenario: Json, rustActions: Json): Promise<LifecycleRun[]> {
  resetBrowserStorage();
  const latestGroups = buildLatestGroups(scenario, undefined) as unknown[];
  const rows = (scenario.snapshot?.sessions ?? []) as Json[];
  const firstProjectId = rows[0] ? String(rows[0].projectId) : undefined;
  const elsewhereRow = [...rows].reverse().find((row) => String(row.projectId) !== firstProjectId) ?? rows.at(-1);
  const elsewhere = elsewhereRow
    ? { projectId: String(elsewhereRow.projectId), sessionId: String(elsewhereRow.sessionId) }
    : undefined;
  const out: LifecycleRun[] = [];
  for (const entry of (rustActions.lifecycle ?? []) as Json[]) {
    if (entry.owned !== true) {
      out.push({ owned: true, rpc: null, answers: {}, echo: {} } as LifecycleRun);
      continue;
    }
    const reference = parseSidebarSessionId(String(entry.sessionId));
    const answers: Record<string, Json> = {};
    let acceptedPresentation: Json | undefined;
    let acceptedRpc: Json | null = null;
    for (const answer of ['accepted', 'declined', 'failed'] as const) {
      const run = await runOne(scenario, entry, latestGroups, elsewhere, answer);
      answers[answer] = {
        state: reference ? lifecycleOf(run.presentation, reference.projectId, reference.sessionId) : null,
        focus: run.focuses,
      };
      if (answer === 'accepted') {
        acceptedPresentation = run.presentation;
        acceptedRpc = run.rpc;
      }
    }
    const echo: Record<string, string | null> = {};
    if (reference && acceptedPresentation) {
      const original = lifecycleOf(scenario.snapshot, reference.projectId, reference.sessionId);
      const row = rows.find(
        (candidate) =>
          String(candidate.projectId) === reference.projectId && String(candidate.sessionId) === reference.sessionId
      );
      if (row && original !== null) {
        // The three echo values are taken from the Rust dump rather than chosen again here, so
        // both sides are answering the same question. The third one is a value neither side
        // predicted, and which one that is depends on the row.
        const states = (entry.echo?.states ?? {}) as Record<string, string>;
        for (const [name, state] of [
          ['agrees', states.agrees],
          ['stillOld', states.stillOld],
          ['movedOn', states.movedOn],
        ] as const) {
          if (!state) continue;
          const echoed = reduceGxserverPresentationDelta(
            acceptedPresentation as any,
            { session: { ...row, lifecycleState: state } as any, type: 'sessionUpdated' } as any,
            Number(acceptedPresentation.revision ?? 0) + 1
          );
          echo[name] = lifecycleOf(echoed as any, reference.projectId, reference.sessionId);
        }
      }
    }
    out.push({ owned: true, rpc: acceptedRpc, answers, echo });
  }
  return out;
}

/**
 * The close half. Drives the shipped `transitionSession(sessionId, 'close')` through the four
 * answers and then through the three things the daemon can say about a row the client has already
 * taken away.
 *
 * `neverAnswered` is the case the original cannot resolve at all: its `rpc` is a bare `fetch` with
 * no timeout and no abort, so the promise `transitionSession` awaits simply never settles. The
 * harness therefore races it against a tick and reads the state at that point, which IS the
 * TypeScript's permanent answer: the row is gone and nothing will ever put it back.
 */
export async function runTypeScriptClose(scenario: Json, rustActions: Json): Promise<Json[]> {
  resetBrowserStorage();
  const latestGroups = buildLatestGroups(scenario, undefined) as unknown[];
  const rows = (scenario.snapshot?.sessions ?? []) as Json[];
  const firstProjectId = rows[0] ? String(rows[0].projectId) : undefined;
  const elsewhereRow = [...rows].reverse().find((row) => String(row.projectId) !== firstProjectId) ?? rows.at(-1);
  const elsewhere = elsewhereRow
    ? { projectId: String(elsewhereRow.projectId), sessionId: String(elsewhereRow.sessionId) }
    : undefined;
  const out: Json[] = [];
  for (const entry of (rustActions.close ?? []) as Json[]) {
    if (entry.owned !== true) {
      out.push({ owned: false });
      continue;
    }
    const reference = parseSidebarSessionId(String(entry.sessionId));
    const answers: Json = {};
    let acceptedRuntime: Json | undefined;
    let rpc: Json | null = null;
    let optimisticFocus: Json[] = [];
    let optimisticDrawn = true;
    for (const answer of ['accepted', 'failed', 'neverAnswered'] as const) {
      const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
      runtime.browserTabs = [];
      runtime.latestGroups = latestGroups;
      runtime.presentation = scenario.snapshot;
      runtime.localFirstHiddenPresentationSessionKeys = new Set<string>();
      runtime.focusedSessionId =
        entry.focus === 'self' ? reference?.sessionId : entry.focus === 'other' ? elsewhere?.sessionId : undefined;
      const focuses: Json[] = [];
      runtime.focusLocalWorkspaceSession = (projectId: string, sessionId: string) => {
        focuses.push({
          follow: 'focus',
          session: `${SIDEBAR_SESSION_PREFIX}${encodeURIComponent(projectId)}:${encodeURIComponent(sessionId)}`,
        });
      };
      runtime.publishPresentation = () => {};
      runtime.client = {
        rpc(path: string, params: Json) {
          rpc = { path, params };
          if (answer === 'accepted') return Promise.resolve({ action: 'close', session: {} });
          if (answer === 'failed') return Promise.reject(new Error('transport'));
          return new Promise(() => {});
        },
      };
      // The optimistic half runs synchronously inside `transitionSession` before its await, so the
      // state a tick later is the state for every answer that has not arrived.
      const run = runtime.transitionSession(String(entry.sessionId), 'close');
      await Promise.race([run, new Promise((resolve) => setTimeout(resolve, 0))]);
      if (answer === 'accepted') {
        await run;
        acceptedRuntime = runtime;
        optimisticFocus = focuses;
        optimisticDrawn = closeRowIsDrawn(runtime, reference);
      }
      answers[answer] = { drawn: closeRowIsDrawn(runtime, reference) };
    }
    const echo: Json = {};
    const row = rows.find(
      (candidate) =>
        reference !== undefined &&
        String(candidate.projectId) === reference.projectId &&
        String(candidate.sessionId) === reference.sessionId
    );
    if (acceptedRuntime && reference && row) {
      for (const [name, delta] of [
        ['removed', { projectId: reference.projectId, sessionId: reference.sessionId, type: 'sessionRemoved' }],
        ['stillRunning', { session: { ...row, lifecycleState: 'running' }, type: 'sessionUpdated' }],
        ['stopped', { session: { ...row, lifecycleState: 'stopped' }, type: 'sessionUpdated' }],
      ] as const) {
        const echoed = Object.create(GpuiSidebarRuntime.prototype) as Json;
        echoed.localFirstHiddenPresentationSessionKeys = acceptedRuntime.localFirstHiddenPresentationSessionKeys;
        echoed.presentation = reduceGxserverPresentationDelta(
          acceptedRuntime.presentation as any,
          delta as any,
          Number(acceptedRuntime.presentation.revision ?? 0) + 1
        );
        echo[name] = { drawn: closeRowIsDrawn(echoed, reference) };
      }
    }
    out.push({
      owned: true,
      rpc,
      optimistic: { drawn: optimisticDrawn, focus: optimisticFocus },
      answers,
      echo,
    });
  }
  return out;
}

/**
 * Whether the sidebar would draw the row: `this.presentation` still holds it and the runtime's
 * own local-first hidden set does not name it. Those are the two things
 * `removePresentationSession` changes, and the second is the one nothing ever clears.
 */
function closeRowIsDrawn(runtime: Json, reference: { projectId: string; sessionId: string } | undefined): boolean {
  if (!reference) return false;
  const present = ((runtime.presentation?.sessions ?? []) as Json[]).some(
    (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
  );
  // The shipped key builder, not a hand-rolled one: it joins with a NUL, and guessing a colon
  // here would have made every hidden row read as drawn and the whole close gate as passing.
  const hidden = (runtime.localFirstHiddenPresentationSessionKeys as Set<string> | undefined)?.has(
    createGxserverPresentationSidebarSessionKey(reference.projectId, reference.sessionId)
  );
  return present && !hidden;
}

/**
 * The fork half. Drives the shipped `forkSession` through the four things `/api/forkSession` can
 * do and records what it called and what it moved.
 *
 * The seams are the same three as everywhere else, plus `postLocalWorkspaceTerminalFocus`, which
 * IS the pane move and so is the thing being compared rather than a thing being replaced: it
 * records its session and its placement target. `postSidebarActionToast` records the level and
 * the title; its description is the daemon's or the transport's text, which the two clients word
 * differently by construction, so only its presence is compared.
 */
export async function runTypeScriptFork(scenario: Json, rustActions: Json): Promise<Json[]> {
  resetBrowserStorage();
  const latestGroups = buildLatestGroups(scenario, undefined) as unknown[];
  const out: Json[] = [];
  for (const entry of (rustActions.fork ?? []) as Json[]) {
    if (entry.owned !== true) {
      out.push({ owned: false });
      continue;
    }
    const reference = parseSidebarSessionId(String(entry.sessionId));
    const answers: Json = {};
    let rpc: Json | null = null;
    let activate: string | null = null;
    for (const answer of ['accepted', 'emptyFork', 'failed', 'neverAnswered'] as const) {
      const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
      runtime.browserTabs = [];
      runtime.latestGroups = latestGroups;
      runtime.presentation = scenario.snapshot;
      runtime.workspaceGroups = { groups: {}, projectOrder: [] };
      // The starting pair the Rust side actually planned against, not a label. A loaded snapshot
      // re-homes the focus to some project on its own, so "elsewhere" is a real project and for a
      // session of that project it means the opposite of what it is called.
      runtime.activeProjectId = (entry.activeBefore?.project ?? undefined) as string | undefined;
      runtime.activeGroupId = (entry.activeBefore?.group ?? undefined) as string | undefined;
      runtime.refreshSidebarHudFromClient = () => {};
      runtime.publishPresentation = () => {};
      runtime.setLocalPresentationSessionFocus = () => {};
      runtime.refreshDomainPresentationSnapshotFromClient = () => Promise.resolve();
      runtime.workspaceSubgroupSidebarIdForSession = () => undefined;
      const follow: Json[] = [];
      runtime.postLocalWorkspaceTerminalFocus = (
        projectId: string,
        sessionId: string,
        placementTargetSessionId?: string
      ) => {
        follow.push({
          follow: 'placePane',
          session: `${SIDEBAR_SESSION_PREFIX}${encodeURIComponent(projectId)}:${encodeURIComponent(sessionId)}`,
          placementTarget: `${SIDEBAR_SESSION_PREFIX}${encodeURIComponent(projectId)}:${encodeURIComponent(String(placementTargetSessionId ?? ''))}`,
        });
      };
      runtime.postSidebarActionToast = (level: string, title: string, options?: Json) => {
        follow.push({
          follow: 'toast',
          level,
          title,
          hasDescription: typeof options?.description === 'string' && options.description.length > 0,
        });
      };
      runtime.client = {
        rpc(path: string, params: Json) {
          rpc = { path, params };
          if (answer === 'accepted')
            return Promise.resolve({ fork: { session: { sessionId: `${reference?.sessionId}-fork` } } });
          if (answer === 'emptyFork') return Promise.resolve({ fork: { session: {} } });
          return Promise.reject(new Error(answer === 'failed' ? 'transport' : 'timeout'));
        },
      };
      await runtime.forkSession(String(entry.sessionId));
      answers[answer] = follow;
      // Whether `forkSession` moved the active project before its call, which is the one thing it
      // does that the daemon's answer cannot undo.
      // Whether `forkSession` moved the active place before its call, which is the one thing it
      // does that the daemon's answer cannot undo. It is the PAIR that moves: a project can
      // already be active with another of its groups selected, and comparing only the project id
      // reported that as no activation at all.
      if (answer === 'accepted')
        activate =
          runtime.activeProjectId !== (entry.activeBefore?.project ?? undefined) ||
          runtime.activeGroupId !== (entry.activeBefore?.group ?? undefined)
            ? String(runtime.activeGroupId)
            : null;
    }
    out.push({ owned: true, rpc, activate, answers });
  }
  return out;
}

/**
 * The flags half. Drives the shipped `updateSessionFlags` and `setSessionParked` and records the
 * call, the row the patch leaves behind, and whether parking asked for a sleep.
 *
 * `setSessionSleeping` is replaced by a recorder rather than run: parking's sleep is the lifecycle
 * action that already has its own half of this gate, and running it here would compare it twice
 * and for the wrong reasons.
 */
export async function runTypeScriptFlags(scenario: Json, rustActions: Json): Promise<Json[]> {
  resetBrowserStorage();
  const out: Json[] = [];
  for (const entry of (rustActions.flags ?? []) as Json[]) {
    if (entry.owned !== true) {
      out.push({ owned: false });
      continue;
    }
    const payload = entry.payload as Json;
    const reference = parseSidebarSessionId(String(payload.sessionId));
    const answers: Json = {};
    let rpc: Json | null = null;
    let thenSleep = false;
    for (const accepted of [true, false]) {
      const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
      runtime.presentation = scenario.snapshot;
      // `createGpuiSidebarSettings` normalizes `runtimeSettings.settings`, not `runtimeSettings`
      // itself, so the flag has to sit one level down or it reads as the default and the whole
      // parking-sleeps leg silently never fires.
      runtime.runtimeSettings = { settings: { sleepSessionWhenParking: entry.sleepWhenParking === true } };
      runtime.publishPresentation = () => {};
      const follow: Json[] = [];
      runtime.setSessionSleeping = (sessionId: string) => {
        follow.push({ follow: 'sleep', session: sessionId });
        return Promise.resolve();
      };
      runtime.client = {
        rpc(path: string, params: Json) {
          rpc = { path, params };
          return accepted ? Promise.resolve({}) : Promise.reject(new Error('transport'));
        },
      };
      // `handleSidebarMessage` awaits these and lets a rejection out, which nothing catches.
      try {
        if (payload.type === 'setSessionParked')
          await runtime.setSessionParked(String(payload.sessionId), payload.parked === true);
        else if (payload.type === 'setSessionPinned')
          await runtime.updateSessionFlags(String(payload.sessionId), { isPinned: payload.pinned === true });
        else if (payload.type === 'setSessionFavorite')
          await runtime.updateSessionFlags(String(payload.sessionId), {
            isFavorite: payload.favorite === true,
            sessionTag: payload.favorite === true ? 'favorite' : null,
          });
        else
          await runtime.updateSessionFlags(String(payload.sessionId), {
            isFavorite: payload.sessionTag === 'favorite',
            sessionTag: payload.sessionTag ?? null,
          });
      } catch {
        // The unhandled rejection the shipped code produces. Nothing local follows it.
      }
      if (accepted) thenSleep = follow.some((item) => item.follow === 'sleep');
      answers[accepted ? 'accepted' : 'failed'] = {
        // The patch is compared by the row it leaves, like every other action in this gate: the
        // two sides express an optimistic value differently and only the drawn row is comparable.
        follow,
        row: flagsRow(runtime.presentation as Json, reference),
      };
    }
    out.push({ owned: true, rpc, thenSleep, answers });
  }
  return out;
}

function flagsRow(presentation: Json, reference: { projectId: string; sessionId: string } | undefined): Json | null {
  if (!reference) return null;
  const row = ((presentation?.sessions ?? []) as Json[]).find(
    (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
  );
  if (!row) return null;
  return {
    isPinned: row.isPinned ?? false,
    isParked: row.isParked ?? false,
    isFavorite: row.isFavorite ?? false,
    sessionTag: row.sessionTag ?? null,
  };
}

/**
 * The dialog half. Drives the shipped `runNativeSessionAction` and records the two host calls it
 * makes: the dismissal the open dialog is closed with, and the payload the new one carries.
 *
 * The row it reads is handed over from the Rust dump rather than re-derived here, the same
 * contract the menu gate uses: which rows a group holds is M4a's question, and what this one asks
 * is what the two sides make of the SAME row. The app modal host is the only seam.
 */
export async function runTypeScriptModals(scenario: Json, rustActions: Json): Promise<Json[]> {
  resetBrowserStorage();
  const { runNativeSessionAction } = await import('@/apps/desktop/sidebar/native-sidebar/session-actions');
  const { sidebarStore } = await import('@/packages/core-ui/sidebar-store-model');
  const out: Json[] = [];
  for (const entry of (rustActions.modals ?? []) as Json[]) {
    const payload = entry.payload as Json;
    const row = (entry.row ?? {}) as Json;
    const sessionId = String(payload.sessionId);
    sidebarStore.setState({
      sessionsById: {
        [sessionId]: {
          sessionId,
          alias: row.alias ?? '',
          primaryTitle: row.primaryTitle ?? undefined,
          terminalTitle: row.terminalTitle ?? undefined,
          agentIcon: row.agentIcon ?? undefined,
          sessionNote: row.sessionNote ?? undefined,
        } as never,
      },
    });
    const calls: Json[] = [];
    installModalRecorder(calls);
    runNativeSessionAction(payload as never, () => {});
    out.push({ calls });
  }
  return out;
}

/**
 * The app modal host, recording instead of posting. `closeAppModal` sends `{type:'close'}` and
 * carries its reason only as a log label, so the label is captured from the recorder's own
 * knowledge of which call it is rather than invented.
 */
function installModalRecorder(calls: Json[]): void {
  const globals = globalThis as Json;
  const window = (globals.window ??= {}) as Json;
  window.webkit = {
    messageHandlers: {
      ghostexAppModalHost: {
        postMessage(message: Json) {
          calls.push(message?.type === 'close' ? { call: 'close' } : { call: 'open', open: message });
        },
      },
    },
  };
  window.__ghostex_APP_MODAL_HOST_SURFACE__ = undefined;
}

/**
 * The bulk half. Drives the shipped `setGroupSleeping`, `wakeProjectSleepingSessions`,
 * `sleepInactiveProjectSessions`, `closeInactiveProjectSessions`, `setSessionsSleeping` and the
 * `closeSessions` arm, and records which per-session call each one made, in order.
 *
 * The two seams are the per-session actions themselves, `setSessionSleeping` and
 * `transitionSession`, which is the boundary the port draws too: everything below them is already
 * compared by the lifecycle and close halves of this gate, and running them again here would
 * compare them twice and for the wrong reasons. `focusProjectId` is recorded because a project
 * wake moves the active project before it fans out.
 *
 * `browserTabs` is empty on purpose. A project with app tabs is refused WHOLE by the port (the
 * host cannot reach the browser bridge from this path), so the set the two sides compare is the
 * set for a project without them; the refusal itself is asserted separately.
 */
export async function runTypeScriptBulk(scenario: Json, rustActions: Json): Promise<Json[]> {
  resetBrowserStorage();
  const out: Json[] = [];
  for (const entry of (rustActions.bulk ?? []) as Json[]) {
    const payload = entry.payload as Json;
    const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
    runtime.presentation = scenario.snapshot;
    runtime.browserTabs = [];
    runtime.remotePresentations = new Map();
    // The shape `getGpuiWorkspaceSessionSubgroups` indexes: a user-made session group id reaches
    // it before anything else in `setGroupSleeping`, and the port refuses exactly that shape.
    runtime.workspaceGroups = { groups: {}, projectOrder: [], projects: {} };
    runtime.publishPresentation = () => {};
    const calls: Json[] = [];
    let focusProject: string | null = null;
    runtime.focusProjectId = (projectId: string) => {
      focusProject = projectId;
    };
    runtime.setSessionSleeping = (sessionId: string, sleeping: boolean) => {
      calls.push({ call: sleeping ? 'sleep' : 'wake', session: sessionId });
      return Promise.resolve();
    };
    runtime.transitionSession = (sessionId: string, action: string) => {
      calls.push({ call: action, session: sessionId });
      return Promise.resolve();
    };
    await runTypeScriptBulkPayload(runtime, payload);
    out.push({ calls, focusProject });
  }
  return out;
}

async function runTypeScriptBulkPayload(runtime: Json, payload: Json): Promise<void> {
  switch (String(payload.type)) {
    case 'setGroupSleeping':
      await runtime.setGroupSleeping(String(payload.groupId), payload.sleeping === true);
      return;
    case 'wakeProjectSleepingSessions':
      await runtime.wakeProjectSleepingSessions(String(payload.groupId));
      return;
    case 'sleepInactiveProjectSessions':
      await runtime.sleepInactiveProjectSessions(String(payload.groupId));
      return;
    case 'closeInactiveProjectSessions':
      await runtime.closeInactiveProjectSessions(String(payload.groupId));
      return;
    case 'setSessionsSleeping':
      await runtime.setSessionsSleeping((payload.sessionIds ?? []) as string[], payload.sleeping === true);
      return;
    case 'closeSessions':
      // `handleSidebarMessage`'s own arm, which is one line and has no method of its own.
      await Promise.all(((payload.sessionIds ?? []) as string[]).map((id) => runtime.transitionSession(id, 'close')));
      return;
  }
}

/**
 * The pacing, measured rather than asserted.
 *
 * `setSessionsSleeping` sends a bulk SLEEP one request at a time with a real 350 ms timer between
 * them and sends everything else together, and which of the two a payload gets is invisible in the
 * call list the half above compares. So a few small sets are run through the SHIPPED helper with
 * its real wait and the elapsed time is bounded on both sides: a paced run of n rows cannot finish
 * sooner than interval * (n - 1), and an unpaced one cannot take that long. No tolerance, because a
 * tolerance would accept a nearly-right interval.
 *
 * Kept to a handful of three-row sets: at 350 ms a row this is the one probe in the gate whose cost
 * is wall-clock time.
 */
export async function runTypeScriptBulkPacing(): Promise<Json[]> {
  const { GPUI_SIDEBAR_BULK_SLEEP_INTERVAL_MS } = await import('@/apps/desktop/sidebar/bulk-sleep-pacing');
  const ids = ['combined-session:P:A', 'combined-session:P:B', 'combined-session:P:C'];
  const out: Json[] = [];
  for (const sleeping of [true, false]) {
    const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
    runtime.setSessionSleeping = () => Promise.resolve();
    const started = Date.now();
    await runtime.setSessionsSleeping(ids, sleeping);
    out.push({
      sleeping,
      rows: ids.length,
      elapsedMs: Date.now() - started,
      intervalMs: GPUI_SIDEBAR_BULK_SLEEP_INTERVAL_MS,
    });
  }
  return out;
}

/**
 * The wake-time half. Runs the shipped `resolveSessionSnoozeWakeTime` at each fixed instant the
 * clock file names and returns the ISO string it produces.
 *
 * `now` is a PARAMETER of the shipped function, so nothing has to be stubbed here: the gate gives
 * both sides the same instant rather than letting either read a clock. The time zone is pinned by
 * the caller and asserted there, because "Tomorrow" is 09:00 local and a run in another zone would
 * compare two different questions.
 */
export function runTypeScriptSnoozeClock(rustActions: Json): Json[] {
  return ((rustActions.snoozeClock ?? []) as Json[]).map((entry) => {
    const now = new Date(Number(entry.nowMs));
    const wake: Json = {};
    for (const preset of SESSION_SNOOZE_PRESETS) wake[preset] = resolveSessionSnoozeWakeTime(preset, now).toISOString();
    // The shape the `switch` has no case for. It falls out with `undefined` and `.toISOString()`
    // throws, which is why the port refuses the payload instead of answering it, and why a port
    // that DID answer it would be a difference here.
    try {
      wake.notAPreset = (resolveSessionSnoozeWakeTime as (preset: string, now: Date) => Date)(
        'notAPreset',
        now
      ).toISOString();
    } catch {
      wake.notAPreset = null;
    }
    return { wake };
  });
}

/**
 * The boundary half: `isSidebarSessionSnoozed` and `Date.parse`, on the same stamps the Rust side
 * drew a row with. The Rust entry carries three answers (the shared predicate, the parsed stamp and
 * the section the row really landed in) and all three are held against this one value, because the
 * failure this probe exists for is the drawing and the deciding disagreeing by a tick.
 */
export function runTypeScriptSnoozeBoundary(rustActions: Json): Json[] {
  return ((rustActions.snoozeBoundary ?? []) as Json[]).map((entry) => {
    const snoozedUntil = entry.snoozedUntil === null ? undefined : String(entry.snoozedUntil);
    const nowMs = Number(entry.nowMs);
    const parsed = snoozedUntil ? Date.parse(snoozedUntil) : Number.NaN;
    return {
      isSnoozed: isSidebarSessionSnoozed({ snoozedUntil }, nowMs),
      parsedMs: Number.isFinite(parsed) ? parsed : null,
    };
  });
}

/**
 * The menu-row half. Drives the shipped `runNativeSessionAction` and records what it POSTS, which
 * is the whole of what a snooze menu row does.
 *
 * The clock is the one seam: `resolveSessionSnoozeWakeTime` is called with no `now` inside that
 * function, so `Date` is replaced with one pinned to the entry's instant for the duration of the
 * call. Whether the row is drawn is handed over from the Rust dump, the same contract the dialog
 * half uses.
 */
export async function runTypeScriptSnoozeActions(rustActions: Json): Promise<Json[]> {
  resetBrowserStorage();
  const { runNativeSessionAction } = await import('@/apps/desktop/sidebar/native-sidebar/session-actions');
  const { sidebarStore } = await import('@/packages/core-ui/sidebar-store-model');
  const out: Json[] = [];
  for (const entry of (rustActions.snoozeActions ?? []) as Json[]) {
    const payload = entry.payload as Json;
    const sessionId = String(payload.sessionId);
    sidebarStore.setState({
      sessionsById: entry.drawn === true ? ({ [sessionId]: { sessionId, alias: '' } } as never) : ({} as never),
    });
    const posts: Json[] = [];
    withFixedNow(Number(entry.nowMs), () => runNativeSessionAction(payload as never, (message) => posts.push(message)));
    out.push({ posts });
  }
  return out;
}

/**
 * `Date` pinned to one instant, for the one shipped function that reads it with no parameter.
 *
 * Nothing else in this harness stubs a clock, and this one is restored in a `finally` so a throw
 * cannot leave the rest of the gate running against a frozen `Date`.
 */
function withFixedNow<T>(nowMs: number, run: () => T): T {
  const RealDate = Date;
  class FixedDate extends RealDate {
    constructor(...args: unknown[]) {
      if (args.length === 0) super(nowMs);
      else super(...(args as []));
    }
    static now(): number {
      return nowMs;
    }
  }
  (globalThis as Json).Date = FixedDate;
  try {
    return run();
  } finally {
    (globalThis as Json).Date = RealDate;
  }
}

/**
 * The call half. Drives the shipped `snoozeSession` and `runSessionLifecycleCommand` through both
 * answers and records the call, the toast and the sleep.
 *
 * `setSessionSleeping` is recorded rather than run, exactly as the flags half records parking's
 * sleep: it is the lifecycle action that already has its own half of this gate, and running it here
 * would compare it twice and for the wrong reasons. `requestRemoteGxserver` is recorded too, so a
 * refused remote row can be shown to be a HAND-OFF to something this port cannot do rather than a
 * lost call.
 */
export async function runTypeScriptSnoozeCalls(scenario: Json, rustActions: Json): Promise<Json[]> {
  resetBrowserStorage();
  const out: Json[] = [];
  for (const entry of (rustActions.snoozeCalls ?? []) as Json[]) {
    const payload = entry.payload as Json;
    const answers: Json = {};
    let rpc: Json | null = null;
    let remote = false;
    for (const accepted of [true, false]) {
      const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
      runtime.presentation = scenario.snapshot;
      runtime.publishPresentation = () => {};
      const follow: Json[] = [];
      runtime.setSessionSleeping = (sessionId: string) => {
        follow.push({ follow: 'sleep', session: sessionId });
        return Promise.resolve();
      };
      runtime.postSidebarActionToast = (level: string, title: string, options?: Json) => {
        follow.push({
          follow: 'toast',
          level,
          title,
          hasDescription: typeof options?.description === 'string' && options.description.length > 0,
        });
      };
      runtime.requestRemoteGxserver = () => {
        remote = true;
        return accepted ? Promise.resolve({}) : Promise.reject(new Error('transport'));
      };
      runtime.client = {
        rpc(path: string, params: Json) {
          rpc = { path, params };
          return accepted ? Promise.resolve({}) : Promise.reject(new Error('transport'));
        },
      };
      if (payload.type === 'snoozeSession')
        await runtime.snoozeSession(String(payload.sessionId), payload.snoozedUntil as string);
      else await runtime.runSessionLifecycleCommand(String(payload.sessionId), '/api/unsnoozeSession', {});
      answers[accepted ? 'accepted' : 'failed'] = follow;
    }
    out.push({ rpc, remote, answers });
  }
  return out;
}

/**
 * The seed-title rule on its own, over triples chosen to reach every branch of the `||` chain.
 *
 * This exists because the rule could not be reached through the recording: no drawn row in it has
 * a padded or blank title, so the gate's untrimmed-title mutation produced zero differences and
 * proved nothing. The expression below is the one `runNativeSessionAction` uses, character for
 * character.
 */
export function runTypeScriptTitleRule(rustActions: Json): (string | null)[] {
  return ((rustActions.titleRule ?? []) as Json[]).map((entry) => {
    const primaryTitle = entry.primaryTitle as string | null | undefined;
    const terminalTitle = entry.terminalTitle as string | null | undefined;
    const alias = String(entry.alias ?? '');
    return primaryTitle?.trim() || terminalTitle?.trim() || alias;
  });
}
