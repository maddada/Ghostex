/*
 * CDXC:RepoStructure 2026-08-23:
 * The Settings left-rail page list: which top-level pages exist, which of them
 * still match the active search query, and the section rows each expandable
 * page contributes. Plain render-time helper, no React hooks.
 */
import { type Dispatch, type SetStateAction } from 'react';
import {
  IconCloud,
  IconCodeDots,
  IconDeviceDesktop,
  IconExternalLink,
  IconFolderOpen,
  IconInfoCircle,
  IconKeyboard,
  IconPlayerPlay,
  IconPuzzle,
  IconSettings,
  IconTools,
} from '@tabler/icons-react';
import { type SettingsModalTab } from '../settings-modal-tabs';
import { SearchableExtraSettingsTabId, getSettingsSectionSearch, settingsTabSearchHasMatches } from './search';
import {
  type ExtraSettingsTabSearches,
  type VisibleHotkeySectionNavigation,
  type VisibleHotkeySections,
} from './use-hotkey-settings';
import { type VisibleMainSettingsSectionNavigation } from './main-settings-visibility';
import {
  HotkeySettingsSectionId,
  MainSettingsScrollTargetId,
  MainSettingsSectionId,
  SettingsSidebarPage,
} from './types';

export function createSettingsSidebarPages({
  activeHotkeySettingsSectionId,
  activeMainSettingsGroupId,
  activeMainSettingsSectionId,
  activeTab,
  extraSettingsTabSearches,
  hasVisibleMainSettings,
  isSettingsSearching,
  scrollHotkeySettingsSectionIntoView,
  scrollMainSettingsSectionIntoView,
  setActiveHotkeySettingsSectionId,
  setActiveMainSettingsSectionId,
  setActiveTab,
  settingsSearchQuery,
  showOSIntegrationSettingsTab,
  visibleHotkeySectionNavigation,
  visibleHotkeySections,
  visibleMainSettingsSectionNavigation,
}: {
  activeHotkeySettingsSectionId: HotkeySettingsSectionId;
  activeMainSettingsGroupId: MainSettingsSectionId;
  activeMainSettingsSectionId: MainSettingsScrollTargetId;
  activeTab: SettingsModalTab;
  extraSettingsTabSearches: ExtraSettingsTabSearches;
  hasVisibleMainSettings: boolean;
  isSettingsSearching: boolean;
  scrollHotkeySettingsSectionIntoView: (sectionId: HotkeySettingsSectionId) => void;
  scrollMainSettingsSectionIntoView: (sectionId: MainSettingsScrollTargetId) => void;
  setActiveHotkeySettingsSectionId: Dispatch<SetStateAction<HotkeySettingsSectionId>>;
  setActiveMainSettingsSectionId: Dispatch<SetStateAction<MainSettingsScrollTargetId>>;
  setActiveTab: (nextTab: SettingsModalTab) => void;
  settingsSearchQuery: string;
  showOSIntegrationSettingsTab: boolean;
  visibleHotkeySectionNavigation: VisibleHotkeySectionNavigation;
  visibleHotkeySections: VisibleHotkeySections;
  visibleMainSettingsSectionNavigation: VisibleMainSettingsSectionNavigation;
}) {
  /*
   * CDXC:Settings 2026-06-24-22:16:
   * Settings no longer has a top tab bar. Keep top-level Settings pages in the
   * left sidebar and let section-rich pages expand there so navigation, section
   * jumps, search results, and the Show Advanced footer share one rail.
   *
   * CDXC:Settings 2026-06-25-17:12:
   * Top-level Settings categories need Tabler icons in the left sidebar, while
   * nested section rows stay text-only so expandable sections do not read as
   * separate main categories.
   */
  const settingsSidebarPageHasSearchMatches = (pageId: SettingsModalTab): boolean => {
    if (!isSettingsSearching) {
      return true;
    }
    if (pageId === 'settings') {
      return hasVisibleMainSettings || getSettingsSectionSearch(settingsSearchQuery, 'General', []).sectionMatches;
    }
    if (pageId === 'hotkeys') {
      return visibleHotkeySections.length > 0;
    }
    return settingsTabSearchHasMatches(extraSettingsTabSearches[pageId as SearchableExtraSettingsTabId]);
  };
  /*
   * CDXC:Settings 2026-07-22-00:00:
   * While searching, the sidebar rail keeps only the Settings pages that have
   * matches so one query locates settings across every page, not just the
   * page currently open.
   */
  const allSettingsSidebarPages: SettingsSidebarPage[] = [
    {
      icon: IconSettings,
      id: 'settings',
      sections: visibleMainSettingsSectionNavigation.map((section) => ({
        active: activeTab === 'settings' && activeMainSettingsGroupId === section.id,
        id: section.id,
        onSelect: () => {
          setActiveMainSettingsSectionId(section.id);
          setActiveTab('settings');
          requestAnimationFrame(() => scrollMainSettingsSectionIntoView(section.id));
        },
        /*
         * CDXC:Settings 2026-08-19:
         * A group whose first anchor carries the group's own name (Sidebar,
         * Terminal) would otherwise render "Sidebar > Sidebar". Drop that row
         * from the rail only: scroll tracking still measures the anchor, so
         * reading that header keeps the group row highlighted.
         */
        subsections: section.subsections
          .filter((subsection) => subsection.title !== section.title)
          .map((subsection) => ({
            active: activeTab === 'settings' && activeMainSettingsSectionId === subsection.id,
            id: subsection.id,
            onSelect: () => {
              setActiveMainSettingsSectionId(subsection.id);
              setActiveTab('settings');
              requestAnimationFrame(() => scrollMainSettingsSectionIntoView(subsection.id));
            },
            title: subsection.title,
          })),
        title: section.title,
      })),
      title: 'General',
    },
    { icon: IconCodeDots, id: 'agents', title: 'Agents' },
    { icon: IconTools, id: 'integrations', title: 'Integrations' },
    { icon: IconPuzzle, id: 'extensions', title: 'Extensions' },
    { icon: IconCloud, id: 'remote', title: 'Remote' },
    { icon: IconFolderOpen, id: 'projects', title: 'Projects' },
    {
      icon: IconKeyboard,
      id: 'hotkeys',
      sections: visibleHotkeySectionNavigation.map((section) => ({
        active: activeTab === 'hotkeys' && activeHotkeySettingsSectionId === section.id,
        id: section.id,
        onSelect: () => {
          setActiveHotkeySettingsSectionId(section.id);
          setActiveTab('hotkeys');
          requestAnimationFrame(() => scrollHotkeySettingsSectionIntoView(section.id));
        },
        title: section.title,
      })),
      title: 'Hotkeys',
    },
    { icon: IconPlayerPlay, id: 'actions', title: 'Actions' },
    { icon: IconExternalLink, id: 'openTargets', title: 'Open In' },
    ...(showOSIntegrationSettingsTab
      ? [{ icon: IconDeviceDesktop, id: 'osIntegration' as const, title: 'OS Integration' }]
      : []),
    { icon: IconInfoCircle, id: 'about', title: 'About' },
  ];
  const settingsSidebarPages: SettingsSidebarPage[] = allSettingsSidebarPages.filter((page) =>
    settingsSidebarPageHasSearchMatches(page.id)
  );
  const settingsSearchMatchingPages = isSettingsSearching ? settingsSidebarPages : [];

  return { settingsSearchMatchingPages, settingsSidebarPages };
}
