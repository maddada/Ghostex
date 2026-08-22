import {
  clampNumber,
  isRecord,
  readLooseString,
} from "./primitives";

export const SETTINGS_MODAL_NAVIGATION_TABS = [
  "settings",
  "integrations",
  "plugins",
  "osIntegration",
  "remote",
  "projects",
  "agents",
  "actions",
  "openTargets",
  "hotkeys",
  "about",
] as const;
export type SettingsModalNavigationTab = (typeof SETTINGS_MODAL_NAVIGATION_TABS)[number];
export type SettingsModalNavigationState = {
  activeTab: SettingsModalNavigationTab;
  scrollTopByTab: Partial<Record<SettingsModalNavigationTab, number>>;
  version: 1;
};
const SETTINGS_MODAL_NAVIGATION_TAB_SET = new Set<string>(SETTINGS_MODAL_NAVIGATION_TABS);
const MAX_SETTINGS_MODAL_SCROLL_TOP = 1_000_000;
export const DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE: SettingsModalNavigationState = {
  activeTab: "settings",
  scrollTopByTab: {},
  version: 1,
};

export function normalizeSettingsModalNavigationState(
  candidate: unknown,
): SettingsModalNavigationState {
  const source = isRecord(candidate) ? candidate : {};
  const rawActiveTab = readLooseString(source.activeTab);
  const activeTab = SETTINGS_MODAL_NAVIGATION_TAB_SET.has(rawActiveTab)
    ? (rawActiveTab as SettingsModalNavigationTab)
    : DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE.activeTab;
  const scrollSource = isRecord(source.scrollTopByTab) ? source.scrollTopByTab : {};
  const scrollTopByTab: SettingsModalNavigationState["scrollTopByTab"] = {};
  for (const tab of SETTINGS_MODAL_NAVIGATION_TABS) {
    const scrollTop = scrollSource[tab];
    if (typeof scrollTop !== "number" || !Number.isFinite(scrollTop) || scrollTop <= 0) {
      continue;
    }
    scrollTopByTab[tab] = clampNumber(
      scrollTop,
      0,
      MAX_SETTINGS_MODAL_SCROLL_TOP,
      DEFAULT_SETTINGS_MODAL_NAVIGATION_STATE.scrollTopByTab[tab] ?? 0,
    );
  }
  return {
    activeTab,
    scrollTopByTab,
    version: 1,
  };
}
