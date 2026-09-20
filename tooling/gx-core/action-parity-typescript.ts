/**
 * The TypeScript half of the action parity harness: drives the real
 * `GpuiSidebarRuntime.handleSidebarMessage` over a recorded presentation and returns the CALLS it
 * makes for each payload, in the shape `action-parity.ts` diffs against the Rust plans.
 *
 * Nothing is reimplemented. `handleSidebarMessage`, `postProjectPathActionForGroup`,
 * `resolveProjectIdForGroup`, `copySessionDetails`, `copyWorkspaceProjectRemoteUrl`,
 * `postRemoteSessionNativeAction`, `postRemoteProjectNativeAction` and `postRemoteToast` all run as
 * they ship; only the two edges they end at are replaced by recorders, because those edges are a
 * webview bridge and a native message handler. A difference the diff reports is therefore a
 * difference between the shipped TypeScript and the Rust port.
 *
 * `latestGroups` is built here with the real `createGxserverPresentationSidebarGroups` over the
 * scenario's own snapshot, NOT taken from the Rust dump: which projects have a group is exactly
 * what `resolveProjectIdForGroup` asks, so handing both sides the same answer would make the gate
 * unable to see a wrong one.
 */
// First, so the client-storage adapter finds a `Storage` before any module reads one.
import { resetBrowserStorage } from './browser-shim';
import {
  createGxserverPresentationSidebarGroups,
  type GxserverPresentationSidebarProjectOverlay,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import {
  createGpuiPresentationProjectProjectionMetadata,
  createGpuiSidebarSessionRoutingId,
  resolveGpuiSidebarAgentIcon,
} from '@/apps/desktop/sidebar/gxserver-runtime/helpers/presentation-projection';
import { GpuiSidebarRuntime } from '@/apps/desktop/sidebar/gxserver-runtime/core';
import type { SidebarSessionGroup } from '@/packages/shared/session-grid-contract';

type Json = Record<string, any>;

/** One call, normalized to the vocabulary `SidebarActionPlan::to_json` writes. */
type Call = Json;

const calls: Call[] = [];

/**
 * The two edges the ported actions end at. Everything before them is the shipped code.
 *
 * `postAppModalHostMessage` throws when the handler is missing and its callers catch that and fall
 * back to `handleUnsupportedSidebarMessage`, so the handler is always present here and records
 * instead: a real app always has it, and a harness that made it absent would measure the fallback
 * rather than the action.
 */
function installRecorders(): void {
  const globals = globalThis as Json;
  const window = (globals.window ??= {}) as Json;
  window.webkit = {
    messageHandlers: {
      ghostexAppModalHost: {
        postMessage(message: Json) {
          calls.push(fromAppModalHostMessage(message));
        },
      },
    },
  };
  window.ghostexGpui = {
    ...(window.ghostexGpui as Json),
    postNativeProjectPathAction(payload: string) {
      calls.push({ call: 'nativeProjectPathAction', payload: JSON.parse(payload) });
      return true;
    },
  };
  // `withModalHostSurface` adds this to every message when it is set; the app sets it only inside
  // a modal host window, and the sidebar page is not one.
  window.__ghostex_APP_MODAL_HOST_SURFACE__ = undefined;
}

/**
 * The app modal host is a message bus, so the message says which call it is. The two the ported
 * actions send are the clipboard write and a toast; anything else is recorded verbatim so an
 * unexpected call shows up as a difference rather than passing as one of these two.
 */
function fromAppModalHostMessage(message: Json): Call {
  if (message?.type === 'copySessionDetails') return { call: 'copyText', text: message.detailsText };
  if (message?.type === 'toast') {
    const call: Call = { call: 'toast', level: message.level, title: message.title };
    if (message.description !== undefined) call.description = message.description;
    return call;
  }
  return { call: 'appModalHostMessage', message };
}

/** The projected group inventory, exactly as `createSidebarGroups` builds it for the runtime. */
export function buildLatestGroups(scenario: Json, parkedProjectId: string | undefined): SidebarSessionGroup[] {
  const snapshot = scenario.snapshot as Json;
  const recentProjects = parkedProjectId ? [{ projectId: parkedProjectId } as any] : [];
  const metadata = createGpuiPresentationProjectProjectionMetadata({
    domainProjects: [],
    presentation: snapshot as any,
    projectOrder: snapshot.workspaceGroups?.projectOrder,
    recentProjects,
  });
  const overlays = new Map<string, GxserverPresentationSidebarProjectOverlay>(
    metadata.projectOverlays.map((overlay) => [overlay.projectId, overlay])
  );
  return createGxserverPresentationSidebarGroups({
    activeProjectId: undefined,
    chatProjectIds: metadata.chatProjectIds,
    focusedSessionId: undefined,
    hiddenProjectIds: metadata.hiddenProjectIds,
    hiddenSessionKeys: undefined,
    presentation: snapshot as any,
    projectOverlays: [...overlays.values()],
    resolveAgentIcon: resolveGpuiSidebarAgentIcon,
    resolveSessionRoutingId: createGpuiSidebarSessionRoutingId,
    visibleSessionIds: undefined,
  });
}

/**
 * Runs every payload of `rustActions.entries` through the shipped runtime and returns the calls
 * each one made, in the same order.
 */
export async function runTypeScriptActions(scenario: Json, rustActions: Json): Promise<Call[][]> {
  resetBrowserStorage();
  installRecorders();
  const parkedProjectId = typeof rustActions.parkedProjectId === 'string' ? rustActions.parkedProjectId : undefined;
  const groupsByVariant = new Map<string, SidebarSessionGroup[]>([
    ['none', buildLatestGroups(scenario, undefined)],
    ['firstParked', buildLatestGroups(scenario, parkedProjectId)],
  ]);
  const runtime = Object.create(GpuiSidebarRuntime.prototype) as Json;
  const out: Call[][] = [];
  for (const entry of rustActions.entries as Json[]) {
    runtime.latestGroups = groupsByVariant.get(String(entry.variant)) ?? [];
    calls.length = 0;
    await runtime.handleSidebarMessage(entry.payload);
    out.push(calls.map((call) => JSON.parse(JSON.stringify(call))));
  }
  return out;
}
