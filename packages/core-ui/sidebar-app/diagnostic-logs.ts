import { useEffectEvent, type RefObject } from 'react';
import {
  isDiagnosticLoggingScenarioEnabled,
  DEFAULT_ghostex_SETTINGS,
  type DiagnosticLoggingScenarioId,
} from '../../shared/ghostex-settings';
import { logSidebarDebug } from '../sidebar-debug';
import { postSidebarRefreshDebugLog } from '../sidebar-refresh-debug-log';
import { SIDEBAR_COLLAPSE_STATE_DEBUG_EVENT_PREFIX } from '../sidebar-collapse-state-debug';
import { useSidebarStore } from '../sidebar-store';
import type { WebviewApi } from '../webview-api';
import { getSidebarStartupElapsedMs } from './session-ordering';

export const SIDEBAR_STARTUP_REPRO_WINDOW_MS = 15_000;

export type SidebarDiagnosticLogsOptions = {
  debuggingMode: boolean | undefined;
  firstHydrateRevisionRef: RefObject<number | undefined>;
  hasAppliedHydrateRef: RefObject<boolean>;
  hasEstablishedStartupGroupCollapseBaselineRef: RefObject<boolean>;
  refreshDebugInstanceIdRef: RefObject<string>;
  revision: number;
  sidebarCollapseDiagnosticLoggingEnabled: boolean;
  sidebarStartupStartedAtRef: RefObject<number>;
  vscode: WebviewApi;
};

/*
 * CDXC:SidebarHookDecomposition 2026-08-22:
 * The sidebar's five diagnostic log posters are one concern and were one
 * contiguous block of `useEffectEvent`s inside SidebarApp. They stay effect
 * events (not callbacks) so every caller keeps reading the latest render's
 * settings, revision, and refs without re-subscribing anything.
 */
export function useSidebarDiagnosticLogs({
  debuggingMode,
  firstHydrateRevisionRef,
  hasAppliedHydrateRef,
  hasEstablishedStartupGroupCollapseBaselineRef,
  refreshDebugInstanceIdRef,
  revision,
  sidebarCollapseDiagnosticLoggingEnabled,
  sidebarStartupStartedAtRef,
  vscode,
}: SidebarDiagnosticLogsOptions) {
  const postSidebarDebugLog = useEffectEvent(
    (scenarioId: DiagnosticLoggingScenarioId, event: string, details: unknown) => {
      if (!debuggingMode) {
        return;
      }

      logSidebarDebug(debuggingMode, event, details);
      vscode.postMessage({
        details,
        event,
        scenarioId,
        type: 'sidebarDebugLog',
      });
    }
  );

  const postSidebarCollapseStateLog = useEffectEvent(
    (event: string, details: Record<string, unknown>, options: { enabled?: boolean } = {}) => {
      /*
       * CDXC:SidebarCollapseDiagnostics 2026-06-02-23:52:
       * Sidebar restart repros need a dedicated low-volume trace for localStorage
       * collapse-state reads, writes, hydrate timing, and user toggles. Keep the
       * payload privacy-safe by recording counts, booleans, revisions, elapsed
       * timings, and hashed group identifiers instead of project names or paths.
       */
      if (!(options.enabled ?? sidebarCollapseDiagnosticLoggingEnabled)) {
        return;
      }

      vscode.postMessage({
        details: {
          ...details,
          elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
          firstHydrateRevision: firstHydrateRevisionRef.current,
          hasEstablishedStartupGroupCollapseBaseline: hasEstablishedStartupGroupCollapseBaselineRef.current,
          hasHydrate: hasAppliedHydrateRef.current,
          instanceId: refreshDebugInstanceIdRef.current,
          revision,
        },
        event: `${SIDEBAR_COLLAPSE_STATE_DEBUG_EVENT_PREFIX}${event}`,
        scenarioId: 'native.sidebar.collapse',
        type: 'sidebarDebugLog',
      });
    }
  );

  const postPinnedSessionReorderLog = useEffectEvent((event: string, details: unknown) => {
    /*
     * CDXC:PinnedSessions 2026-05-28-15:33:
     * Pinned reorder failures need click-scoped repro breadcrumbs even when
     * broad Debugging Mode is off. Keep these events low-volume and explicit
     * so a user drag can reveal which guard prevented syncSessionOrder.
     */
    vscode.postMessage({
      details,
      event: `repro.pinnedSessionReorder.${event}`,
      scenarioId: 'native.pane.reorder',
      type: 'sidebarDebugLog',
    });
  });

  const postSidebarStartupReproLog = useEffectEvent((event: string, details: unknown) => {
    if (getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current) > SIDEBAR_STARTUP_REPRO_WINDOW_MS) {
      return;
    }

    vscode.postMessage({
      details,
      event: `repro.sidebarStartup.${event}`,
      scenarioId: 'native.sidebar.refresh',
      type: 'sidebarDebugLog',
    });
  });

  const postSidebarRefreshLifecycleLog = useEffectEvent((event: string, details: Record<string, unknown>) => {
    const currentSettings = useSidebarStore.getState().hud.settings ?? DEFAULT_ghostex_SETTINGS;
    postSidebarRefreshDebugLog(
      isDiagnosticLoggingScenarioEnabled(currentSettings.diagnosticLogging, 'native.sidebar.refresh'),
      vscode,
      event,
      details
    );
  });

  return {
    postPinnedSessionReorderLog,
    postSidebarCollapseStateLog,
    postSidebarDebugLog,
    postSidebarRefreshLifecycleLog,
    postSidebarStartupReproLog,
  };
}
