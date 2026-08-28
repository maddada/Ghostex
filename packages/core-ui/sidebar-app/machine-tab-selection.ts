/*
 * CDXC:SidebarMachineTabs 2026-08-28:
 * Remote machines are top-level sidebar tabs, not sections stacked under the
 * local project list, so the sidebar shows exactly one machine at a time. Which
 * one is pure per-window UI state, persisted next to the collapse state under
 * the same window scope id so a second app window keeps its own selection.
 *
 * The local machine is the reserved `local` tab id — the same id the add-project
 * flow already uses for "this machine" — and is the default whenever nothing is
 * stored or the stored machine no longer exists.
 */

export const LOCAL_SIDEBAR_MACHINE_TAB_ID = 'local';

export const SIDEBAR_MACHINE_TAB_STORAGE_KEY = 'ghostex-sidebar-selected-machine-tab';

export function getSidebarMachineTabStorageKey(windowScopeId: string): string {
  return `${SIDEBAR_MACHINE_TAB_STORAGE_KEY}:window:${encodeURIComponent(windowScopeId)}`;
}

export function readSidebarSelectedMachineTabId(windowScopeId: string): string {
  if (typeof window === 'undefined') {
    return LOCAL_SIDEBAR_MACHINE_TAB_ID;
  }

  try {
    const storedValue = window.localStorage.getItem(getSidebarMachineTabStorageKey(windowScopeId));
    return storedValue && storedValue.length > 0 ? storedValue : LOCAL_SIDEBAR_MACHINE_TAB_ID;
  } catch {
    return LOCAL_SIDEBAR_MACHINE_TAB_ID;
  }
}

export function writeSidebarSelectedMachineTabId(windowScopeId: string, machineTabId: string): void {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(getSidebarMachineTabStorageKey(windowScopeId), machineTabId);
  } catch {
    // Ignore storage failures; the in-memory selection still applies.
  }
}
