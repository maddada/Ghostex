import type { WebviewApi } from './webview-api';

export const SIDEBAR_ORDER_REPRO_TAG = 'SIDEBAR_ORDER_REPRO';

export function postSidebarOrderReproLog(vscode: WebviewApi, event: string, details?: unknown): void {
  vscode.postMessage({
    details: enrichSidebarOrderReproDetails(details),
    event,
    scenarioId: 'native.pane.reorder',
    type: 'sidebarDebugLog',
  });
}

function enrichSidebarOrderReproDetails(details: unknown): unknown {
  if (!details || typeof details !== 'object' || Array.isArray(details)) {
    return {
      details,
      tag: SIDEBAR_ORDER_REPRO_TAG,
    };
  }

  return {
    tag: SIDEBAR_ORDER_REPRO_TAG,
    ...details,
  };
}
