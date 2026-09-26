import { type SettingsModalTab } from '../settings-modal-tabs';
import {
  normalizeSettingsModalNavigationState,
  type SettingsModalNavigationState,
} from '../../shared/ghostex-settings';

export let rememberedSettingsModalTab: SettingsModalTab | undefined;
export const rememberedSettingsModalScrollTopByTab: Partial<Record<SettingsModalTab, number>> = {};

/*
 * CDXC:Settings 2026-06-29-00:40:
 * Settings must keep app-session tab and scroll memory, but the main SettingsModal render needs React Compiler coverage so scroll-section highlight updates do not re-render the whole long settings page.
 * Keep the mutable session memory behind helpers so SettingsModal does not directly reassign module variables and the compiler can memoize the large render tree.
 */
export function getRememberedSettingsModalTab(
  storedNavigation: SettingsModalNavigationState
): SettingsModalTab | undefined {
  return rememberedSettingsModalTab ?? storedNavigation.activeTab;
}

export function rememberSettingsModalTab(tab: SettingsModalTab): void {
  rememberedSettingsModalTab = tab;
}

export function getRememberedSettingsModalScrollTop(
  tab: SettingsModalTab,
  storedNavigation: SettingsModalNavigationState
): number {
  return rememberedSettingsModalScrollTopByTab[tab] ?? storedNavigation.scrollTopByTab[tab] ?? 0;
}

export function rememberSettingsModalScrollTop(tab: SettingsModalTab, scrollTop: number): void {
  rememberedSettingsModalScrollTopByTab[tab] = scrollTop;
}

export function getRememberedSettingsModalNavigationState(
  activeTab: SettingsModalTab,
  storedNavigation: SettingsModalNavigationState
): SettingsModalNavigationState {
  return normalizeSettingsModalNavigationState({
    activeTab,
    scrollTopByTab: {
      ...storedNavigation.scrollTopByTab,
      ...rememberedSettingsModalScrollTopByTab,
    },
  });
}

export function areSettingsModalNavigationStatesEqual(
  left: SettingsModalNavigationState,
  right: SettingsModalNavigationState
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

/** The Theme page's groups that have their own More options. */
export type ThemeMoreOptionsGroup = 'colours' | 'transparency' | 'chatTerminal';

const rememberedThemeMoreOptionsOpen: Record<ThemeMoreOptionsGroup, boolean> = {
  colours: false,
  transparency: false,
  chatTerminal: false,
};

/** Whether a Theme page group's More options was left open, for the rest of this app session. */
export function getRememberedThemeMoreOptionsOpen(group: ThemeMoreOptionsGroup): boolean {
  return rememberedThemeMoreOptionsOpen[group];
}

export function rememberThemeMoreOptionsOpen(group: ThemeMoreOptionsGroup, open: boolean): void {
  rememberedThemeMoreOptionsOpen[group] = open;
}
