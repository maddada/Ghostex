import type { Dispatch, SetStateAction } from 'react';
import type { ghostexSettings, KeepAwakeDurationMinutes } from '../../shared/ghostex-settings';
import { openAppModal, openQuickAccess } from '../app-modal-host-bridge';
import type { WebviewApi } from '../webview-api';
import { readSidebarKeepAwakeRuntime } from './collapse-state';
import type { SidebarKeepAwakeRuntimeState } from './types';

export type SidebarOverlayActionsOptions = {
  setIsPreviousSessionsOpen: Dispatch<SetStateAction<boolean>>;
  setIsSessionSearchOpen: Dispatch<SetStateAction<boolean>>;
  setIsSessionSearchSelectionVisible: Dispatch<SetStateAction<boolean>>;
  setSessionSearchQuery: Dispatch<SetStateAction<string>>;
  setSidebarKeepAwakeRuntime: Dispatch<SetStateAction<SidebarKeepAwakeRuntimeState | undefined>>;
  settings: ghostexSettings | undefined;
  vscode: WebviewApi;
};

/*
 * CDXC:SidebarHookDecomposition 2026-08-22:
 * The launchers that hand a surface over to the app-modal host, plus the Keep
 * Awake commands and the session-search close. Every one of them first tears
 * down the transient sidebar drawers, which is why they share a hook: they all
 * close over the same drawer state setters.
 */
export function useSidebarOverlayActions({
  setIsPreviousSessionsOpen,
  setIsSessionSearchOpen,
  setIsSessionSearchSelectionVisible,
  setSessionSearchQuery,
  setSidebarKeepAwakeRuntime,
  settings,
  vscode,
}: SidebarOverlayActionsOptions) {
  const openSidebarSettings = () => {
    if (!settings) {
      vscode.postMessage({ type: 'openSettings' });
      return;
    }
    setIsPreviousSessionsOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
    openAppModal({ modal: 'settings', type: 'open' });
  };

  const openHotkeys = () => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * Cmd+. is advertised in the sidebar Settings dropdown after the menu moved out of the titlebar. Route it to the same full-window app-modal host as Settings and Command Palette, closing transient sidebar drawers first so the shortcut opens one focused Hotkeys surface.
     */
    setIsPreviousSessionsOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
    openAppModal({ modal: 'hotkeys', type: 'open' });
  };

  const openCommandPalette = () => {
    /**
     * CDXC:CommandPalette 2026-06-13-10:26:
     * Cmd+Shift+P should open the full-window app-modal command palette,
     * matching Settings instead of rendering a dialog inside the narrow
     * sidebar. Close transient sidebar drawers first so the centered palette is
     * the only active command surface.
     *
     * CDXC:CommandPalette 2026-06-13-22:18:
     * Ghostex Quick Access gives Commands and Sessions separate tabs.
     * This launcher opens Commands; Cmd+P routes to the Sessions modal
     * id instead of encoding a mode in this input query.
     *
     */
    setIsPreviousSessionsOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
    openQuickAccess('commands');
  };

  const openKeepAwakePowerSettings = () => {
    setIsPreviousSessionsOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
    openAppModal({
      initialSearchQuery: 'Keep awake',
      modal: 'settings',
      type: 'open',
    });
  };

  const startSidebarKeepAwake = (durationMinutes: KeepAwakeDurationMinutes) => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * Keep Awake moved from the macOS titlebar into the sidebar shortcut row, but the titlebar host remains the caffeinate runtime owner. Optimistically reflect the chosen duration in sidebar UI while native forwards the command to the existing titlebar start path.
     */
    setSidebarKeepAwakeRuntime({ durationMinutes });
    vscode.postMessage({
      action: 'start',
      durationMinutes,
      type: 'runTitlebarKeepAwakeCommand',
    });
    window.setTimeout(() => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime() ?? { durationMinutes });
    }, 250);
  };

  const stopSidebarKeepAwake = () => {
    setSidebarKeepAwakeRuntime(undefined);
    vscode.postMessage({
      action: 'stop',
      type: 'runTitlebarKeepAwakeCommand',
    });
    window.setTimeout(() => {
      setSidebarKeepAwakeRuntime(readSidebarKeepAwakeRuntime());
    }, 250);
  };

  const closeSessionSearch = () => {
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
  };

  return {
    closeSessionSearch,
    openCommandPalette,
    openHotkeys,
    openKeepAwakePowerSettings,
    openSidebarSettings,
    startSidebarKeepAwake,
    stopSidebarKeepAwake,
  };
}
