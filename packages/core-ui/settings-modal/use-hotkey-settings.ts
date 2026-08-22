/*
 * CDXC:SettingsModalSplit 2026-08-23:
 * Hotkeys-page search memos plus the derived hotkey section refs, visible
 * sections, and section scrolling. The three useMemo calls stay adjacent and in
 * their original order, so this hook must keep being called from the same spot
 * in SettingsModal.
 */
import { useMemo, type RefObject } from "react";
import { GHOSTEX_HOTKEY_DEFINITIONS } from "../../shared/ghostex-hotkeys";
import { type ghostexSettings } from "../../shared/ghostex-settings";
import {
  getExtraSettingsTabSearches,
  getHotkeySettingsSectionSearches,
  getSettingsSectionSearch,
  shouldShowSettingsSection,
} from "./search";
import {
  HOTKEY_SETTINGS_SECTIONS,
  HotkeySettingsDefinitionById,
  HotkeySettingsSectionId,
  HotkeySettingsSectionRefs,
  HotkeySettingsSectionSearches,
  SettingsSectionNavigationItem,
} from "./types";

export function useHotkeySettings({
  draft,
  hotkeyActionsSectionRef,
  hotkeyGeneralSectionRef,
  hotkeyNavigationSectionRef,
  hotkeyPaneActionsSectionRef,
  hotkeyProjectsSectionRef,
  hotkeySessionSlotsSectionRef,
  isFirstLaunchSetup,
  settingsSearchQuery,
}: {
  draft: ghostexSettings;
  hotkeyActionsSectionRef: RefObject<HTMLDivElement | null>;
  hotkeyGeneralSectionRef: RefObject<HTMLDivElement | null>;
  hotkeyNavigationSectionRef: RefObject<HTMLDivElement | null>;
  hotkeyPaneActionsSectionRef: RefObject<HTMLDivElement | null>;
  hotkeyProjectsSectionRef: RefObject<HTMLDivElement | null>;
  hotkeySessionSlotsSectionRef: RefObject<HTMLDivElement | null>;
  isFirstLaunchSetup: boolean;
  settingsSearchQuery: string;
}) {
  const hotkeyDefinitionsById = useMemo<HotkeySettingsDefinitionById>(
    () => new Map(GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => [definition.id, definition])),
    [],
  );
  const hotkeySectionSearches = useMemo(() => {
    const sectionSearches = getHotkeySettingsSectionSearches({
      definitionsById: hotkeyDefinitionsById,
      expandCollapsedProjectsOnJump: draft.expandCollapsedProjectsOnJump,
      searchQuery: settingsSearchQuery,
    });
    /*
     * CDXC:SettingsSearch 2026-07-22-00:00:
     * A query matching the Hotkeys page title (e.g. "hotkeys") should reveal
     * the whole page, mirroring how section-title matches reveal their rows.
     */
    if (!getSettingsSectionSearch(settingsSearchQuery, "Hotkeys", []).sectionMatches) {
      return sectionSearches;
    }
    return Object.fromEntries(
      Object.entries(sectionSearches).map(([sectionId, sectionResult]) => [
        sectionId,
        { ...sectionResult, sectionMatches: true },
      ]),
    ) as HotkeySettingsSectionSearches;
  }, [draft.expandCollapsedProjectsOnJump, hotkeyDefinitionsById, settingsSearchQuery]);
  const extraSettingsTabSearches = useMemo(
    () => getExtraSettingsTabSearches(settingsSearchQuery),
    [settingsSearchQuery],
  );
  const isSettingsSearching = !isFirstLaunchSetup && settingsSearchQuery.trim().length > 0;
  const hotkeySectionRefs: HotkeySettingsSectionRefs = {
    actions: hotkeyActionsSectionRef,
    general: hotkeyGeneralSectionRef,
    navigation: hotkeyNavigationSectionRef,
    paneActions: hotkeyPaneActionsSectionRef,
    projects: hotkeyProjectsSectionRef,
    sessionSlots: hotkeySessionSlotsSectionRef,
  };
  const visibleHotkeySections = HOTKEY_SETTINGS_SECTIONS.filter((section) =>
    shouldShowSettingsSection(hotkeySectionSearches[section.id]),
  );
  const visibleHotkeySectionNavigation: SettingsSectionNavigationItem<HotkeySettingsSectionId>[] =
    visibleHotkeySections.map((section) => ({
      id: section.id,
      title: section.title,
    }));
  const scrollHotkeySettingsSectionIntoView = (sectionId: HotkeySettingsSectionId) => {
    hotkeySectionRefs[sectionId].current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  return {
    extraSettingsTabSearches,
    hotkeyDefinitionsById,
    hotkeySectionRefs,
    hotkeySectionSearches,
    isSettingsSearching,
    scrollHotkeySettingsSectionIntoView,
    visibleHotkeySectionNavigation,
    visibleHotkeySections,
  };
}

export type ExtraSettingsTabSearches = ReturnType<typeof useHotkeySettings>["extraSettingsTabSearches"];
export type VisibleHotkeySections = ReturnType<typeof useHotkeySettings>["visibleHotkeySections"];
export type VisibleHotkeySectionNavigation = ReturnType<
  typeof useHotkeySettings
>["visibleHotkeySectionNavigation"];
