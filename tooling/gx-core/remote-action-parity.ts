/**
 * The gate for a remote row's session actions: the calls they send down the machine's tunnel,
 * in order, under every answer script, against the shipped TypeScript.
 *
 *   cargo run --release --example sidebar_remote_action_parity -- <out-dir>   # from packages/gx-core
 *   bun tooling/gx-core/remote-action-parity.ts compare <out-dir> [--inject <mutation>]
 *
 * **What is compared.** For every payload the Rust remote leg answers, and for every answer script
 * (every waited call succeeding, then each one failing in turn), the SEQUENCE of events: each
 * request with its machine, path, body, whether the caller waits for it, and its timeout, and the
 * toast a failure shows. The TypeScript half drives the shipped `handleSidebarMessage`; the only
 * edges replaced are the app modal host (which is where `requestRemoteGxserver` and
 * `postRemoteGxserverSidebarRequest` post, and where the answer comes back from), the timers
 * (`requestRemoteGxserver` books a timeout that must never fire here) and `Date.now` (the request
 * id is built from it).
 *
 * **What is deliberately NOT compared, and counted instead.** After an awaited sleep, wake or fork
 * the old runtime re-reads the machine's presentation snapshot itself (`refreshRemotePresentation
 * FromGxserver`); the store does not, because its own client streams that machine's presentation
 * and the bridge function both routes share refreshes the old runtime's copy after every call that
 * changes it. Those re-reads are counted as `tsRefreshReads`, which is in the zero-check so the
 * probe cannot silently stop seeing them.
 *
 * **Hand-offs.** A payload the Rust leg does not answer goes to the old runtime whole, so there is
 * nothing to compare; it is counted by what the TypeScript then does (`handOffsRemoteWork` when it
 * still calls the machine, `handOffsNothing` when it does not), and both counters must be non-zero.
 *
 * Two local rules ride along: a Full Reload's wake after each of the three sleep answers
 * (`reloadStop`), and whether a paced bulk sleep waits for each request before the next
 * (`bulkWaits`), measured through the shipped helper's own `wait` seam and never a real timer.
 *
 * A gate that cannot fail is worse than none, so `--inject` mutates the Rust dump with a plausible
 * port mistake and the run must then show a difference or collapse a counter.
 */
// First, so the client-storage adapter finds a `Storage` before any module reads one.
import { resetBrowserStorage } from './browser-shim';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { GpuiSidebarRuntime } from '@/apps/desktop/sidebar/gxserver-runtime/core';
import { runGpuiSidebarBulkSleepPaced } from '@/tooling/gx-core/bulk-sleep-pacing-frozen';

type Json = Record<string, any>;

/** The sentence the Rust bridge answers every failed request with. */
const REQUEST_FAILED = 'Remote gxserver request failed.';
/** `gpui_remote_sidebar_request_timeout`: the default and the two bounds the bridge applies. */
const BRIDGE_TIMEOUT_DEFAULT_MS = 15_000;
const BRIDGE_TIMEOUT_MIN_MS = 1_000;
const BRIDGE_TIMEOUT_MAX_MS = 130_000;
/** The reads the old runtime makes on its own after a call, which the store does not repeat. */
const REFRESH_PATHS = new Set(['/api/readPresentationSnapshot', '/api/readSidebarHud']);
const PINNED_NOW_MS = 1_790_000_000_000;

/**
 * Plausible port mistakes, applied to the Rust dump. Each one rewrites the TRACES the plan would
 * have produced, so the comparison sees what the wrong port would send rather than a changed label.
 */
const MUTATIONS: Record<string, (entry: Json) => Json> = {
  // A remote close that waits: the user would wait on a call the shipped code never reads.
  'close-waits': (entry) => withRequests(entry, 'close', (event) => awaited(event)),
  // A pin, tag or favorite that waits.
  'flags-wait': (entry) => withRequests(entry, 'flags', (event) => awaited(event)),
  // The race the shipped comment on `setSessionParked` exists to prevent: the flag posted and the
  // sleep sent without waiting for it, so a failed flag still sleeps.
  'park-sleep-does-not-wait': (entry) =>
    kindOf(entry) !== 'parkThenSleep'
      ? entry
      : mapTraces(entry, (trace) => {
          const events = trace.events as Json[];
          const [update] = events;
          const sleep = (entry.plan.legs as Json[])[1];
          return {
            ...trace,
            events: [{ ...update, awaited: false, timeoutMs: BRIDGE_TIMEOUT_DEFAULT_MS }, requestOf(entry, sleep)],
          };
        }),
  // A Full Reload that wakes after its sleep failed, which is the local bug this change fixed.
  'reload-wakes-after-a-failed-sleep': (entry) =>
    kindOf(entry) !== 'reload'
      ? entry
      : mapTraces(entry, (trace) =>
          canonical(trace.script) === canonical([false])
            ? { ...trace, events: [...(trace.events as Json[]), requestOf(entry, entry.plan.legs[1])] }
            : trace
        ),
  // A snooze the machine refused that sleeps anyway and says nothing.
  'snooze-sleeps-after-a-refusal': (entry) =>
    kindOf(entry) !== 'snooze'
      ? entry
      : mapTraces(entry, (trace) =>
          canonical(trace.script) === canonical([false])
            ? {
                ...trace,
                events: [
                  ...(trace.events as Json[]).filter((event) => event.event !== 'toast'),
                  requestOf(entry, entry.plan.legs[1]),
                ],
              }
            : trace
        ),
  // A failed remote fork that shows nothing.
  'fork-fails-silently': (entry) =>
    kindOf(entry) !== 'fork'
      ? entry
      : mapTraces(entry, (trace) => ({
          ...trace,
          events: (trace.events as Json[]).filter((event) => event.event !== 'toast'),
        })),
  // The machine-scoped project id sent to the machine, which its daemon does not know.
  'send-scoped-ids': (entry) =>
    mapRequests(entry, (event) => ({
      ...event,
      params: { ...event.params, projectId: `remote:${event.machine}:project:${event.params?.projectId}` },
    })),
  // Every call sent to one machine whatever the row said.
  'one-machine-for-all': (entry) => mapRequests(entry, (event) => ({ ...event, machine: 'remote-msgckntd-ecz4w' })),
  // The explicit tag clear dropped from the body, which the daemon then reads as "leave it".
  'tag-clear-dropped': (entry) =>
    mapRequests(entry, (event) => {
      if (event.params?.sessionTag !== null) return event;
      const { sessionTag: _dropped, ...params } = event.params;
      return { ...event, params };
    }),
  // The waited timeout of `requestRemoteGxserver` replaced by the bridge's default.
  'awaited-uses-the-default-timeout': (entry) =>
    mapRequests(entry, (event) => (event.awaited ? { ...event, timeoutMs: BRIDGE_TIMEOUT_DEFAULT_MS } : event)),
  // The remote leg refuses every row, so the old runtime does it all: no difference at all, and
  // the counters that say the Rust side answered anything collapse.
  'refuse-every-remote-row': (entry) => ({ ...entry, owned: false, plan: null, traces: [] }),
  // Unsnooze followed by a sleep, as if it were a snooze.
  'unsnooze-sleeps': (entry) =>
    kindOf(entry) !== 'unsnooze'
      ? entry
      : mapTraces(entry, (trace) =>
          canonical(trace.script) === canonical([true])
            ? {
                ...trace,
                events: [
                  ...(trace.events as Json[]),
                  {
                    ...(trace.events as Json[])[0],
                    path: '/api/sleepSession',
                    params: {
                      projectId: (trace.events as Json[])[0]?.params?.projectId,
                      reason: 'gpui-sidebar',
                      sessionId: (trace.events as Json[])[0]?.params?.sessionId,
                    },
                  },
                ],
              }
            : trace
        ),
};

/** The two local rules, mutated separately because they are not entries. */
const RULE_MUTATIONS = ['reload-ignores-a-failed-sleep', 'paced-sleep-does-not-wait'];

function kindOf(entry: Json): string | undefined {
  return entry.plan?.kind;
}

function mapTraces(entry: Json, map: (trace: Json) => Json): Json {
  return { ...entry, traces: ((entry.traces ?? []) as Json[]).map(map) };
}

function mapRequests(entry: Json, map: (event: Json) => Json): Json {
  return mapTraces(entry, (trace) => ({
    ...trace,
    events: (trace.events as Json[]).map((event) => (event.event === 'request' ? map(event) : event)),
  }));
}

function withRequests(entry: Json, kind: string, map: (event: Json) => Json): Json {
  return kindOf(entry) === kind ? mapRequests(entry, map) : entry;
}

function awaited(event: Json): Json {
  return { ...event, awaited: true, timeoutMs: 20_000 };
}

function requestOf(entry: Json, leg: Json): Json {
  return {
    event: 'request',
    machine: entry.plan.machine,
    path: leg.path,
    params: leg.params,
    awaited: leg.mode === 'awaited',
    timeoutMs: leg.timeoutMs,
  };
}

/** What one run of one payload produced on the TypeScript side. */
type Run = { events: Json[]; refreshReads: number; unanswered: number; other: Json[] };

/**
 * Runs one payload through the shipped runtime with `script` as the bridge's answers to the
 * mutating requests, in order. A request past the end of the script is answered with success,
 * which is what a script that stops early means, and is counted so a trace that ran longer than
 * its script is visible.
 */
async function runPayload(payload: Json, sleepWhenParking: boolean, script: boolean[]): Promise<Run> {
  const run: Run = { events: [], refreshReads: 0, unanswered: 0, other: [] };
  const answers = [...script];
  const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
  runtime.browserTabs = [];
  runtime.latestGroups = [];
  runtime.remotePresentations = new Map();
  runtime.remoteSidebarHuds = new Map();
  runtime.pendingRemoteGxserverRequests = new Map();
  runtime.remoteGxserverRequestSequence = 0;
  // `createGpuiSidebarSettings` normalizes `runtimeSettings.settings`, not `runtimeSettings`: a
  // flag one level too high reads as the default and the park-sleeps leg never fires.
  runtime.runtimeSettings = { settings: { sleepSessionWhenParking: sleepWhenParking } };
  runtime.client = {
    rpc(path: string) {
      run.other.push({ event: 'local', path });
      return Promise.resolve({});
    },
  };
  runtime.publishPresentation = () => {};
  runtime.publishRemotePresentationPatch = () => {};
  runtime.publishHudPatch = () => {};
  const window = (globalThis as Json).window as Json;
  window.__ghostex_APP_MODAL_HOST_SURFACE__ = undefined;
  window.setTimeout = () => 0;
  window.clearTimeout = () => {};
  window.webkit = {
    messageHandlers: {
      ghostexAppModalHost: {
        postMessage(raw: Json) {
          // What reaches the bridge is JSON, so an `undefined` field is no field.
          const message = JSON.parse(JSON.stringify(raw)) as Json;
          if (message.type === 'toast') {
            run.events.push({
              event: 'toast',
              level: message.level,
              title: message.title,
              description: message.description,
            });
            return;
          }
          if (message.type !== 'gpuiRemoteGxserverSidebarRequest') {
            run.other.push({ event: 'appModal', type: message.type });
            return;
          }
          const requestId = message.requestId as string | undefined;
          if (REFRESH_PATHS.has(String(message.path))) {
            run.refreshReads += 1;
            if (requestId) queueMicrotask(() => answer(runtime, message, requestId, true));
            return;
          }
          run.events.push({
            event: 'request',
            machine: message.remoteMachineId,
            path: message.path,
            params: message.params,
            awaited: requestId !== undefined,
            timeoutMs: Math.min(
              Math.max(Number(message.timeoutMs ?? BRIDGE_TIMEOUT_DEFAULT_MS), BRIDGE_TIMEOUT_MIN_MS),
              BRIDGE_TIMEOUT_MAX_MS
            ),
          });
          if (!requestId) return;
          if (!answers.length) run.unanswered += 1;
          const ok = answers.length ? answers.shift()! : true;
          queueMicrotask(() => answer(runtime, message, requestId, ok));
        },
      },
    },
  };
  window.ghostexGpui = {
    ...(window.ghostexGpui as Json),
    postNativeProjectPathAction(payload: string) {
      run.other.push({ event: 'native', action: JSON.parse(payload).action });
      return true;
    },
    postBrowserTabFocus() {
      run.other.push({ event: 'browser' });
    },
  };
  await runtime.handleSidebarMessage(payload).catch(() => undefined);
  return run;
}

function answer(runtime: Json, message: Json, requestId: string, ok: boolean): void {
  runtime.resolveRemoteGxserverRequest(
    ok
      ? { ok: true, remoteMachineId: message.remoteMachineId, requestId, result: {}, type: 'remoteGxserverResponse' }
      : {
          error: REQUEST_FAILED,
          ok: false,
          remoteMachineId: message.remoteMachineId,
          requestId,
          type: 'remoteGxserverResponse',
        }
  );
}

/**
 * A Full Reload driven through the SHIPPED `setSessionSleeping` with the daemon answering the sleep
 * as the case says: does the wake go out? The presentation is the smallest one the sleep's
 * replacement-focus lookup accepts.
 */
async function reloadContinues(answerKind: string): Promise<boolean> {
  const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
  const session = {
    projectId: 'P',
    sessionId: 'S',
    groupId: 'G',
    sortKey: 'a',
    lifecycleState: 'running',
    activity: 'idle',
    visibleInSidebarByDefault: true,
    surface: 'agents',
  };
  runtime.presentation = {
    revision: 1,
    projects: [{ projectId: 'P' }],
    groups: [{ groupId: 'G', projectId: 'P', sessionIds: ['S'] }],
    sessions: [session],
  };
  runtime.browserTabs = [];
  runtime.latestGroups = [];
  runtime.localFirstHiddenPresentationSessionKeys = new Set();
  runtime.focusedSessionId = undefined;
  runtime.publishPresentation = () => {};
  runtime.focusLocalWorkspaceSession = () => {};
  const paths: string[] = [];
  runtime.client = {
    rpc(path: string) {
      paths.push(path);
      if (path === '/api/sleepSession') {
        if (answerKind === 'failed') return Promise.reject(new Error('transport'));
        if (answerKind === 'declined') return Promise.resolve({ declined: { reason: 'keepAwake' } });
      }
      return Promise.resolve({});
    },
  };
  await runtime.fullReloadSession('combined-session:P:S').catch(() => undefined);
  if (!paths.includes('/api/sleepSession'))
    throw new Error(`reloadStop ${answerKind}: the shipped reload never asked for the sleep`);
  return paths.includes('/api/wakeSession');
}

/**
 * Whether the shipped paced helper sends a request while the one before it is still in flight.
 * Each target settles on a later microtask and records when; the helper's own `wait` seam records
 * the interval instead of sleeping.
 */
async function pacedWaitsForEach(): Promise<boolean> {
  const order: string[] = [];
  await runGpuiSidebarBulkSleepPaced(
    ['a', 'b', 'c'],
    (target) =>
      new Promise<void>((resolve) => {
        order.push(`start:${target}`);
        queueMicrotask(() => {
          order.push(`settle:${target}`);
          resolve();
        });
      }),
    { wait: async () => void order.push('wait') }
  );
  return (
    canonical(order) ===
    canonical(['start:a', 'settle:a', 'wait', 'start:b', 'settle:b', 'wait', 'start:c', 'settle:c'])
  );
}

/**
 * Whether the concurrent fan-outs (a bulk wake and a bulk close) wait for anything: they are
 * `Promise.all`, so every request starts before any settles.
 */
async function concurrentWaitsForEach(payload: Json): Promise<boolean> {
  const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
  const order: string[] = [];
  const target = (id: string) =>
    new Promise<void>((resolve) => {
      order.push(`start:${id}`);
      queueMicrotask(() => {
        order.push(`settle:${id}`);
        resolve();
      });
    });
  runtime.setSessionSleeping = (id: string) => target(id);
  runtime.transitionSession = (id: string) => target(id);
  await runtime.handleSidebarMessage(payload);
  const firstSettle = order.findIndex((step) => step.startsWith('settle:'));
  const lastStart = order.map((step) => step.startsWith('start:')).lastIndexOf(true);
  return lastStart < firstSettle ? false : true;
}

async function compare([outDir, ...flags]: string[]) {
  if (!outDir) throw new Error('compare <out-dir> [--inject <mutation>]');
  resetBrowserStorage();
  (globalThis as Json).Date.now = () => PINNED_NOW_MS;
  const injectAt = flags.indexOf('--inject');
  const mutationName = injectAt >= 0 ? flags[injectAt + 1] : undefined;
  if (mutationName && !MUTATIONS[mutationName] && !RULE_MUTATIONS.includes(mutationName))
    throw new Error(`unknown mutation ${mutationName}; known: ${[...Object.keys(MUTATIONS), ...RULE_MUTATIONS]}`);
  const mutate = mutationName ? MUTATIONS[mutationName] : undefined;
  const rust = JSON.parse(readFileSync(join(outDir, 'rust-remote-actions.json'), 'utf8')) as Json;
  const differences: string[] = [];
  let remoteOwned = 0;
  let remoteTraces = 0;
  let awaitedRequests = 0;
  let fireAndForgetRequests = 0;
  let stops = 0;
  let failureToasts = 0;
  let handOffsRemoteWork = 0;
  let handOffsNothing = 0;
  let tsRefreshReads = 0;
  for (const [index, original] of ((rust.entries ?? []) as Json[]).entries()) {
    const entry = mutate ? mutate(original) : original;
    const payload = entry.payload as Json;
    const where = `#${index} ${payload.type} ${String(payload.sessionId ?? '')}`;
    const sleepWhenParking = entry.sleepWhenParking === true;
    if (entry.owned !== true) {
      // Handed over whole: the old runtime does all of it, so there is nothing to compare, only
      // what it does to count.
      const run = await runPayload(payload, sleepWhenParking, []);
      if (run.events.some((event) => event.event === 'request') || run.other.some((e) => e.event === 'native'))
        handOffsRemoteWork += 1;
      else handOffsNothing += 1;
      continue;
    }
    remoteOwned += 1;
    for (const trace of (entry.traces ?? []) as Json[]) {
      remoteTraces += 1;
      const run = await runPayload(payload, sleepWhenParking, (trace.script ?? []) as boolean[]);
      tsRefreshReads += run.refreshReads;
      const mine = (trace.events ?? []) as Json[];
      for (const event of mine) {
        if (event.event === 'request' && event.awaited) awaitedRequests += 1;
        if (event.event === 'request' && !event.awaited) fireAndForgetRequests += 1;
        if (event.event === 'toast') failureToasts += 1;
      }
      if (((trace.script ?? []) as boolean[]).includes(false)) stops += 1;
      if (canonical(mine) !== canonical(run.events))
        differences.push(
          `${where} script ${canonical(trace.script)}: rust ${canonical(mine)} ts ${canonical(run.events)}`
        );
      if (run.unanswered)
        differences.push(`${where} script ${canonical(trace.script)}: the TypeScript waited on ${run.unanswered} more`);
      if (run.other.length)
        differences.push(`${where}: the TypeScript also did ${canonical(run.other)}, which a remote leg never does`);
    }
  }
  // The local Full Reload rule, against the shipped reload.
  let reloadStopCases = 0;
  for (const rule of (rust.reloadStop ?? []) as Json[]) {
    reloadStopCases += 1;
    const mine = mutationName === 'reload-ignores-a-failed-sleep' ? true : (rule.continues as boolean);
    const theirs = await reloadContinues(String(rule.answer));
    if (mine !== theirs) differences.push(`reloadStop ${rule.answer}: rust continues ${mine} ts continues ${theirs}`);
  }
  // Whether each bulk fan-out waits for a request before the next.
  let bulkWaitCases = 0;
  for (const rule of (rust.bulkWaits ?? []) as Json[]) {
    bulkWaitCases += 1;
    const payload = rule.payload as Json;
    const paced = payload.type === 'setSessionsSleeping' && payload.sleeping === true;
    const theirs = paced ? await pacedWaitsForEach() : await concurrentWaitsForEach(payload);
    const mine = mutationName === 'paced-sleep-does-not-wait' && paced ? false : rule.waitsForEach;
    if (mine !== theirs)
      differences.push(`bulkWaits ${payload.type} sleeping=${payload.sleeping}: rust ${mine} ts ${theirs}`);
  }
  console.log(
    `payloads ${(rust.entries ?? []).length} remoteOwned ${remoteOwned} remoteTraces ${remoteTraces} awaitedRequests ${awaitedRequests} fireAndForgetRequests ${fireAndForgetRequests} stops ${stops} failureToasts ${failureToasts} handOffsRemoteWork ${handOffsRemoteWork} handOffsNothing ${handOffsNothing} tsRefreshReads ${tsRefreshReads} reloadStopCases ${reloadStopCases} bulkWaitCases ${bulkWaitCases} differences ${differences.length}${
      mutationName ? ` (injected ${mutationName})` : ''
    }`
  );
  for (const difference of differences.slice(0, 30)) console.log(`  ${difference}`);
  if (differences.length > 30) console.log(`  ... and ${differences.length - 30} more`);
  const measured: [string, number][] = [
    ['remoteOwned', remoteOwned],
    ['remoteTraces', remoteTraces],
    ['awaitedRequests', awaitedRequests],
    ['fireAndForgetRequests', fireAndForgetRequests],
    ['stops', stops],
    ['failureToasts', failureToasts],
    ['handOffsRemoteWork', handOffsRemoteWork],
    ['handOffsNothing', handOffsNothing],
    ['tsRefreshReads', tsRefreshReads],
    ['reloadStopCases', reloadStopCases],
    ['bulkWaitCases', bulkWaitCases],
  ];
  const collapsed = measured.filter(([, count]) => count === 0);
  if (mutationName) {
    const noticed = differences.length > 0 || collapsed.length > 0;
    process.exitCode = noticed ? 0 : 1;
    if (!noticed)
      console.log(`  the injected mutation ${mutationName} produced NO difference and collapsed no counter`);
    else if (collapsed.length) console.log(`  collapsed: ${collapsed.map(([label]) => label).join(', ')}`);
    return;
  }
  if (!differences.length && collapsed.length) {
    console.log(`  ${collapsed.map(([label]) => label).join(', ')} counted nothing, so the gate is not measuring`);
    process.exitCode = 1;
    return;
  }
  process.exitCode = differences.length ? 1 : 0;
}

function canonical(value: unknown): string {
  return JSON.stringify(sortKeys(value));
}

function sortKeys(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (value && typeof value === 'object') {
    const object = value as Json;
    const sorted: Json = {};
    // Code-point order, not `localeCompare`: the app runs the shipped code in QuickJS, whose
    // collation is not the ICU one Bun would use here.
    for (const key of Object.keys(object).sort((left, right) => (left < right ? -1 : left > right ? 1 : 0)))
      if (object[key] !== undefined) sorted[key] = sortKeys(object[key]);
    return sorted;
  }
  return value;
}

const [command, ...rest] = process.argv.slice(2);
if (command === 'compare') await compare(rest);
else {
  console.error('usage: remote-action-parity.ts compare <out-dir> [--inject <mutation>]');
  process.exitCode = 2;
}
