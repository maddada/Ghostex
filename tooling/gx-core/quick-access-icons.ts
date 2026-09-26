import type { QuickAccessIcon } from '@/packages/shared/native-quick-access';
import type { ghostexFocusedPaneAction, ghostexHotkeyAction } from '@/packages/shared/ghostex-hotkeys';

/**
 * The bundled `titlebar/<name>.svg` assets that replace the React rows' Tabler
 * components. Names are kebab-case; the host turns them into asset paths with
 * the same helper the sidebar's action icons use.
 */
export function assetIcon(name: string, color?: string): QuickAccessIcon {
  return color ? { kind: 'asset', name, color } : { kind: 'asset', name };
}

export function imageIcon(url: string | undefined): QuickAccessIcon | undefined {
  return url ? { kind: 'image', url } : undefined;
}

export const NO_ICON: QuickAccessIcon = { kind: 'none' };

/** Port of `FocusedPaneCommandIcon`. */
export function focusedPaneIconName(action: ghostexFocusedPaneAction): string {
  switch (action) {
    case 'openBrowserPane':
      return 'browser';
    case 'rotatePanesClockwise':
      return 'rotate-clockwise';
    case 'mergeAllTabs':
      return 'window-maximize';
    case 'delayedSend':
    case 'closeAfterDone':
      return 'clock';
    case 'forkSession':
      return 'git-fork';
    case 'reloadSession':
      return 'refresh';
    case 'sleepFocusedSession':
      return 'moon';
    case 'wakeFocusedSession':
      return 'player-play';
    case 'closeFocusedSession':
      return 'x';
    case 'popOutPane':
      return 'external-link';
    default:
      return 'layout-sidebar-right-expand';
  }
}

/** Port of `getFocusDirectionIcon`. */
function focusDirectionIconName(direction: 'down' | 'left' | 'right' | 'up'): string {
  if (direction === 'up') return 'chevron-up';
  if (direction === 'right') return 'arrow-right';
  if (direction === 'down') return 'chevron-down';
  return 'arrow-left';
}

/** Port of the `BuiltInCommandIcon` branch for hotkey-backed rows. */
export function hotkeyActionIconName(action: ghostexHotkeyAction): string {
  if (action.kind === 'createSession' || action.kind === 'createAgentSession') return 'plus';
  if (action.kind === 'openCommandsPanel') return 'terminal-2';
  if (action.kind === 'openSettings') return 'settings';
  if (action.kind === 'openHotkeys') return 'keyboard';
  if (action.kind === 'toggleSidebarCollapsed') return 'layout-sidebar';
  if (action.kind === 'toggleViewPanel') return 'layout-sidebar-right-expand';
  if (action.kind === 'expandViewPanel' || action.kind === 'expandViewPanelFully') {
    return 'arrows-diagonal';
  }
  if (action.kind === 'renameActiveSession') return 'edit';
  if (action.kind === 'focusedPaneAction') return focusedPaneIconName(action.focusedPaneAction);
  if (action.kind === 'focusAdjacentGroup' || action.kind === 'cyclePaneTab') {
    return action.direction < 0 ? 'chevron-left' : 'chevron-right';
  }
  if (action.kind === 'focusDirection') return focusDirectionIconName(action.direction);
  if (action.kind === 'splitFocusedPane') return 'arrows-diagonal-2';
  if (action.kind === 'setViewMode') return 'layout-dashboard';
  return 'keyboard';
}

/** Port of `AppModalCommandIcon`. */
export function appModalIconName(modal: string): string {
  if (modal === 'previousSessions') return 'history';
  if (modal === 'agentsHub' || modal === 'configureAgents') return 'settings-automation';
  if (modal === 'configureActions') return 'list-details';
  if (modal === 'openTargets') return 'external-link';
  if (modal === 'addProject') return 'folder-plus';
  return 'keyboard';
}

/** Port of `SidebarMessageCommandIcon`. */
export function sidebarMessageIconName(commandId: string): string {
  if (commandId === 'searchByText') return 'search';
  if (commandId === 'quickTerminal') return 'terminal-2';
  if (commandId === 'quickBrowserTab') return 'browser';
  if (commandId === 'automations') return 'settings-automation';
  if (commandId === 'extensions') return 'puzzle';
  if (commandId === 'openCurrentProjectInFinder') return 'folder-open';
  if (commandId === 'setupGhostex') return 'checklist';
  return 'brand-github';
}
