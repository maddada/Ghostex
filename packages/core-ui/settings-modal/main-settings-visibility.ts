/*
 * CDXC:SettingsModalSplit 2026-08-23:
 * Which General-settings rows, sections, and subsections are visible, and which
 * navigation rail entries survive that filtering. Both factories are plain
 * render-time helpers (no React hooks), so the component keeps calling them at
 * the exact points the inline code used to run.
 */
import {
  FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS,
  isFirstLaunchSetupMainSettingVisible,
  type FirstLaunchSetupMainSettingKey,
} from "../../shared/first-launch-setup-settings";
import { type ghostexSettings } from "../../shared/ghostex-settings";
import { shouldShowSetting, shouldShowSettingsSection } from "./search";
import { getMainSettingsSectionRef } from "./scroll-targets";
import {
  type MainSettingsGroupSearch,
  type MainSettingsSectionNavigation,
  type SettingsSearchSections,
} from "./search-catalog";
import {
  DEBUGGING_MODE_DEPENDENT_SETTING_KEY_SET,
  MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS,
  MAIN_SETTINGS_SECTION_SETTING_KEYS,
  MAIN_SETTINGS_SUBSECTION_NAVIGATION,
  MainSettingsScrollTargetId,
  MainSettingsSectionId,
  MainSettingsSectionRefs,
  MainSettingsSubsectionNavigationItem,
  SettingsSectionMeasurementItem,
  SettingsSectionNavigationItem,
  SettingsSectionSearchResult,
  getMainSettingsSectionGroupId,
} from "./types";

export type MainSettingVisibilityPredicate = (
  sectionResult: SettingsSectionSearchResult,
  settingKey: string,
) => boolean;
export type MainSectionVisibilityPredicate = (
  sectionId: MainSettingsSectionId,
  sectionResult: SettingsSectionSearchResult,
) => boolean;
export type MainSubsectionVisibilityPredicate = (
  sectionId: MainSettingsScrollTargetId,
  sectionResult: SettingsSectionSearchResult,
) => boolean;

export function createMainSettingsVisibility({
  appIconPickerUnavailable,
  draft,
  firstLaunchSetupVisibleSettings,
  isFirstLaunchSetup,
  mainSettingsGroupSearch,
  settingsSearch,
  settingsSearchQuery,
  showAdvancedSettings,
}: {
  appIconPickerUnavailable: boolean;
  draft: ghostexSettings;
  firstLaunchSetupVisibleSettings: ReadonlySet<FirstLaunchSetupMainSettingKey> | undefined;
  isFirstLaunchSetup: boolean;
  mainSettingsGroupSearch: MainSettingsGroupSearch;
  settingsSearch: SettingsSearchSections;
  settingsSearchQuery: string;
  showAdvancedSettings: boolean;
}) {
  const settingMatchesGroupedSectionTitle = (settingKey: string) =>
    (Object.entries(MAIN_SETTINGS_SECTION_SETTING_KEYS) as Array<
      [MainSettingsSectionId, readonly string[]]
    >).some(([sectionId, settingKeys]) => {
      if (sectionId === "agents") {
        return false;
      }
      return (
        mainSettingsGroupSearch[sectionId].groupTitleMatches === true &&
        settingKeys.includes(settingKey)
      );
    });
  const subsectionMatchesGroupedSectionTitle = (sectionId: MainSettingsScrollTargetId) =>
    MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS[sectionId].some((settingKey) =>
      settingMatchesGroupedSectionTitle(settingKey),
    );
  const visibleFirstLaunchMainSettings =
    firstLaunchSetupVisibleSettings ?? FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS;
  const keepAwakeSettingsVisible = isFirstLaunchSetup || draft.showBetaFeatures;
  const debuggingModeDependentSettingsVisible = draft.debuggingMode;
  const mainSettingVisible = (
    sectionResult: SettingsSectionSearchResult,
    settingKey: string,
  ) => {
    if (isFirstLaunchSetup) {
      return isFirstLaunchSetupMainSettingVisible(
        settingKey as FirstLaunchSetupMainSettingKey,
        visibleFirstLaunchMainSettings,
      );
    }
    if (settingsSearchQuery.trim() && settingMatchesGroupedSectionTitle(settingKey)) {
      return true;
    }
    return shouldShowSetting(sectionResult, settingKey, showAdvancedSettings);
  };
  const debuggingSettingVisible = (settingKey: string) => {
    if (
      !debuggingModeDependentSettingsVisible &&
      DEBUGGING_MODE_DEPENDENT_SETTING_KEY_SET.has(settingKey)
    ) {
      return false;
    }
    return mainSettingVisible(settingsSearch.debugging, settingKey);
  };
  const mainSectionVisible = (
    sectionId: MainSettingsSectionId,
    sectionResult: SettingsSectionSearchResult,
  ) => {
    /*
     * CDXC:TitlebarKeepAwake 2026-06-19-13:13:
     * Keep Awake is experimental-only in the regular macOS Settings UI. Hide
     * the Power section until Enable Experimental Features is enabled, while
     * preserving the first-launch lid-close preference required by onboarding.
     */
    if (
      sectionId === "advanced" &&
      !isFirstLaunchSetup &&
      !debuggingModeDependentSettingsVisible
    ) {
      return (
        shouldShowSettingsSection(settingsSearch.beta, showAdvancedSettings) ||
        shouldShowSetting(settingsSearch.debugging, "debuggingMode", showAdvancedSettings)
      );
    }
    if (isFirstLaunchSetup) {
      return MAIN_SETTINGS_SECTION_SETTING_KEYS[sectionId].some((settingKey) =>
        isFirstLaunchSetupMainSettingVisible(
          settingKey as FirstLaunchSetupMainSettingKey,
          visibleFirstLaunchMainSettings,
        ),
      );
    }
    return shouldShowSettingsSection(sectionResult, showAdvancedSettings);
  };
  const mainSubsectionVisible = (
    sectionId: MainSettingsScrollTargetId,
    sectionResult: SettingsSectionSearchResult,
  ) => {
    if (sectionId === "power" && !keepAwakeSettingsVisible) {
      return false;
    }
    if (sectionId === "appIcon" && appIconPickerUnavailable) {
      return false;
    }
    if (
      sectionId === "debugging" &&
      !isFirstLaunchSetup &&
      !debuggingModeDependentSettingsVisible
    ) {
      return (
        subsectionMatchesGroupedSectionTitle(sectionId) ||
        shouldShowSetting(sectionResult, "debuggingMode", showAdvancedSettings)
      );
    }
    if (isFirstLaunchSetup) {
      return MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS[sectionId].some((settingKey) =>
        isFirstLaunchSetupMainSettingVisible(
          settingKey as FirstLaunchSetupMainSettingKey,
          visibleFirstLaunchMainSettings,
        ),
      );
    }
    if (settingsSearchQuery.trim() && subsectionMatchesGroupedSectionTitle(sectionId)) {
      return true;
    }
    return shouldShowSettingsSection(sectionResult, showAdvancedSettings);
  };

  return {
    debuggingSettingVisible,
    mainSectionVisible,
    mainSettingVisible,
    mainSubsectionVisible,
  };
}

export function createVisibleMainSettingsNavigation({
  activeMainSettingsSectionId,
  isFirstLaunchSetup,
  mainSectionVisible,
  mainSettingsSectionNavigation,
  mainSettingsSectionRefs,
  mainSubsectionVisible,
  settingsSearch,
}: {
  activeMainSettingsSectionId: MainSettingsScrollTargetId;
  isFirstLaunchSetup: boolean;
  mainSectionVisible: MainSectionVisibilityPredicate;
  mainSettingsSectionNavigation: MainSettingsSectionNavigation;
  mainSettingsSectionRefs: MainSettingsSectionRefs;
  mainSubsectionVisible: MainSubsectionVisibilityPredicate;
  settingsSearch: SettingsSearchSections;
}) {
  const scrollMainSettingsSectionIntoView = (sectionId: MainSettingsScrollTargetId) => {
    getMainSettingsSectionRef(sectionId, mainSettingsSectionRefs).current?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    });
  };
  const visibleMainSettingsSectionNavigation: Array<
    SettingsSectionNavigationItem<MainSettingsSectionId> & {
      searchResult: SettingsSectionSearchResult;
      subsections: readonly MainSettingsSubsectionNavigationItem[];
    }
  > =
    (isFirstLaunchSetup
      ? [
          {
            id: "agents" as const,
            searchResult: settingsSearch.sidebar,
            title: "Agents",
          },
          ...mainSettingsSectionNavigation,
        ]
      : mainSettingsSectionNavigation
    )
      .filter((section) =>
        section.id === "agents"
          ? mainSectionVisible("agents", settingsSearch.sidebar)
          : mainSectionVisible(section.id, section.searchResult),
      )
      .map((section) => ({
        ...section,
        /*
         * CDXC:SettingsNavigation 2026-08-19:
         * A nested row must not outlive the section it points at, so hide the
         * ones a search query, Show Advanced, or an unavailable capability
         * (Power without experimental features, App Icon off macOS) already
         * removed from the page.
         */
        subsections: (MAIN_SETTINGS_SUBSECTION_NAVIGATION[section.id] ?? []).filter((subsection) =>
          mainSubsectionVisible(subsection.id, settingsSearch[subsection.id]),
        ),
      }));
  const getMainSettingsSectionMeasurementItems = (): SettingsSectionMeasurementItem<MainSettingsScrollTargetId>[] =>
    visibleMainSettingsSectionNavigation.flatMap((section) =>
      (section.subsections.length > 0
        ? section.subsections.map((subsection) => subsection.id)
        : [section.id as MainSettingsScrollTargetId]
      ).map((scrollTargetId) => ({
        id: scrollTargetId,
        ref: getMainSettingsSectionRef(scrollTargetId, mainSettingsSectionRefs),
      })),
    );
  const activeMainSettingsGroupId = getMainSettingsSectionGroupId(activeMainSettingsSectionId);
  const hasVisibleMainSettings = visibleMainSettingsSectionNavigation.length > 0;
  const visibleMainSettingsSectionIds = visibleMainSettingsSectionNavigation
    .map((section) =>
      [section.id, ...section.subsections.map((subsection) => subsection.id)].join(">"),
    )
    .join("|");

  return {
    activeMainSettingsGroupId,
    getMainSettingsSectionMeasurementItems,
    hasVisibleMainSettings,
    scrollMainSettingsSectionIntoView,
    visibleMainSettingsSectionIds,
    visibleMainSettingsSectionNavigation,
  };
}

export type VisibleMainSettingsSectionNavigation = ReturnType<
  typeof createVisibleMainSettingsNavigation
>["visibleMainSettingsSectionNavigation"];
