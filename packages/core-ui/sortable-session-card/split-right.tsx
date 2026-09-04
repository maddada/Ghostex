import { IconLayoutColumns } from '@tabler/icons-react';
import type { SidebarSessionItem } from '../../shared/session-grid-contract';
import type { WebviewApi } from '../webview-api';
import type { SessionContextMenuAction } from '../sortable-session-card';

/**
 * CDXC:Workarea 2026-09-04 DECISION:
 * User: Advanced > Split Right opens the session in a pane to the right of
 * the focused agents pane, for local and remote machine rows alike. The Rust
 * workspace owns pane topology, so the item needs the GPUI bridge and is
 * hidden in the web app.
 */
export function canSplitSidebarSessionRight(
  session: SidebarSessionItem | undefined,
  canUseTerminalAgentMenuAction: boolean,
): boolean {
  return (
    canUseTerminalAgentMenuAction &&
    session !== undefined &&
    session.isDraft !== true &&
    gpuiWorkspaceTerminalFocusBridgeAvailable()
  );
}

export function createSplitSessionRightContextMenuAction(
  vscode: WebviewApi,
  sessionId: string,
  dismissContextMenus: () => void,
): SessionContextMenuAction {
  return {
    icon: <IconLayoutColumns aria-hidden="true" className="session-context-menu-icon" size={16} stroke={1.8} />,
    key: 'split-right',
    label: 'Split Right',
    onClick: () => {
      dismissContextMenus();
      vscode.postMessage({
        sessionId,
        type: 'splitSessionRight',
      });
    },
  };
}

function gpuiWorkspaceTerminalFocusBridgeAvailable(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }
  const bridge = (window as { ghostexGpui?: { postWorkspaceTerminalFocus?: unknown } }).ghostexGpui;
  return typeof bridge?.postWorkspaceTerminalFocus === 'function';
}
