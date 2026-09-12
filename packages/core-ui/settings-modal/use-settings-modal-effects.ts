/*
 * CDXC:RepoStructure 2026-08-23:
 * The SettingsModal effects that run after the sidebar page list is derived:
 * deep-link section scrolling, active-section tracking, settings-prop draft
 * sync, the one-shot app-icon list request, and pending
 * timeout cleanup. They are kept in one hook, in their original order, so the
 * component's hook sequence is unchanged.
 */
import { useEffect, type Dispatch, type RefObject, type SetStateAction } from 'react';
import { normalizeghostexSettings, type ghostexSettings } from '../../shared/ghostex-settings';
import { type SettingsModalTab } from '../settings-modal-tabs';
import { type WebviewApi } from '../webview-api';
import { getMostlyVisibleSettingsSectionId } from './search';
import { getActiveSettingsModalScrollViewport, getMainSettingsSectionRef } from './scroll-targets';
import { MainSettingsScrollTargetId, SettingsSectionMeasurementItem } from './types';

export function useSettingsModalEffects({
  activeTab,
  agentsOnboardingSectionRef,
  appIconPickerUnavailable,
  appIconSectionRef,
  autoSleepSectionRef,
  betaSectionRef,
  browserSectionRef,
  chatSectionRef,
  debuggingSectionRef,
  dialogContentRef,
  editorSectionRef,
  fileOpeningSectionRef,
  getMainSettingsSectionMeasurementItems,
  ghosttyBehaviorSectionRef,
  ghosttyScrollingSectionRef,
  ghosttyTerminalSectionRef,
  hasRequestedAppIconsRef,
  initialSection,
  isFirstLaunchSetup,
  isOpen,
  pendingNavigationPersistTimeoutRef,
  pendingTimeoutRef,
  powerSectionRef,
  sessionCardsSectionRef,
  setActiveMainSettingsSectionId,
  setActiveTabState,
  setDraft,
  settings,
  settingsSearchQuery,
  sidebarSectionRef,
  sidebarTagsSectionRef,
  soundsSectionRef,
  statusIndicatorsSectionRef,
  terminalDevServersSectionRef,
  themingSectionRef,
  visibleMainSettingsSectionIds,
  vscode,
}: {
  activeTab: SettingsModalTab;
  agentsOnboardingSectionRef: RefObject<HTMLDivElement | null>;
  appIconPickerUnavailable: boolean;
  appIconSectionRef: RefObject<HTMLDivElement | null>;
  autoSleepSectionRef: RefObject<HTMLDivElement | null>;
  betaSectionRef: RefObject<HTMLDivElement | null>;
  browserSectionRef: RefObject<HTMLDivElement | null>;
  chatSectionRef: RefObject<HTMLDivElement | null>;
  debuggingSectionRef: RefObject<HTMLDivElement | null>;
  dialogContentRef: RefObject<HTMLDivElement | null>;
  editorSectionRef: RefObject<HTMLDivElement | null>;
  fileOpeningSectionRef: RefObject<HTMLDivElement | null>;
  getMainSettingsSectionMeasurementItems: () => SettingsSectionMeasurementItem<MainSettingsScrollTargetId>[];
  ghosttyBehaviorSectionRef: RefObject<HTMLDivElement | null>;
  ghosttyScrollingSectionRef: RefObject<HTMLDivElement | null>;
  ghosttyTerminalSectionRef: RefObject<HTMLDivElement | null>;
  hasRequestedAppIconsRef: RefObject<boolean>;
  initialSection: MainSettingsScrollTargetId | undefined;
  isFirstLaunchSetup: boolean;
  isOpen: boolean;
  pendingNavigationPersistTimeoutRef: RefObject<ReturnType<typeof setTimeout> | undefined>;
  pendingTimeoutRef: RefObject<ReturnType<typeof setTimeout> | undefined>;
  powerSectionRef: RefObject<HTMLDivElement | null>;
  sessionCardsSectionRef: RefObject<HTMLDivElement | null>;
  setActiveMainSettingsSectionId: Dispatch<SetStateAction<MainSettingsScrollTargetId>>;
  setActiveTabState: Dispatch<SetStateAction<SettingsModalTab>>;
  setDraft: Dispatch<SetStateAction<ghostexSettings>>;
  settings: ghostexSettings | undefined;
  settingsSearchQuery: string;
  sidebarSectionRef: RefObject<HTMLDivElement | null>;
  sidebarTagsSectionRef: RefObject<HTMLDivElement | null>;
  soundsSectionRef: RefObject<HTMLDivElement | null>;
  statusIndicatorsSectionRef: RefObject<HTMLDivElement | null>;
  terminalDevServersSectionRef: RefObject<HTMLDivElement | null>;
  themingSectionRef: RefObject<HTMLDivElement | null>;
  visibleMainSettingsSectionIds: string;
  vscode: WebviewApi | undefined;
}) {
  useEffect(() => {
    if (!isOpen || activeTab !== 'settings' || initialSection === undefined) {
      return;
    }
    /**
     * CDXC:Settings 2026-05-27-07:32:
     * Titlebar entry points such as Power Settings should land on the matching
     * Settings section, not only open the modal at the previously remembered
     * scroll position.
     */
    const targetSectionRef = getMainSettingsSectionRef(initialSection, {
      advanced: betaSectionRef,
      appearance: themingSectionRef,
      autoSleep: autoSleepSectionRef,
      fileOpening: fileOpeningSectionRef,
      browser: browserSectionRef,
      chat: chatSectionRef,
      editor: editorSectionRef,
      notifications: soundsSectionRef,
      power: powerSectionRef,
      sessionCards: sessionCardsSectionRef,
      sidebar: sidebarSectionRef,
      sounds: soundsSectionRef,
      beta: betaSectionRef,
      statusIndicators: statusIndicatorsSectionRef,
      system: powerSectionRef,
      sidebarTags: sidebarTagsSectionRef,
      debugging: debuggingSectionRef,
      tools: browserSectionRef,
      terminal: ghosttyTerminalSectionRef,
      terminalBehavior: ghosttyBehaviorSectionRef,
      terminalScrolling: ghosttyScrollingSectionRef,
      terminalDevServers: terminalDevServersSectionRef,
      theming: themingSectionRef,
      // CDXC:Icons 2026-06-25-21:50: Allow titlebar/deep-link navigation to scroll to App Icon.
      appIcon: appIconSectionRef,
      agents: agentsOnboardingSectionRef,
    });
    const animationFrame = requestAnimationFrame(() => {
      targetSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [activeTab, initialSection, isOpen]);

  useEffect(() => {
    if (!isOpen || activeTab !== 'settings') {
      return;
    }

    const animationFrame = requestAnimationFrame(() => {
      const viewport = getActiveSettingsModalScrollViewport(dialogContentRef.current);
      if (!viewport) {
        return;
      }
      const mostlyVisibleSectionId = getMostlyVisibleSettingsSectionId(
        viewport,
        getMainSettingsSectionMeasurementItems()
      );
      if (mostlyVisibleSectionId) {
        setActiveMainSettingsSectionId((currentSectionId) =>
          currentSectionId === mostlyVisibleSectionId ? currentSectionId : mostlyVisibleSectionId
        );
      }
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [activeTab, isOpen, settingsSearchQuery, visibleMainSettingsSectionIds]);
  useEffect(() => {
    if (!isOpen) {
      return;
    }
    if (isFirstLaunchSetup) {
      setActiveTabState('settings');
    }
    /**
     * CDXC:Settings 2026-05-13-16:05
     * Saving a control in Hotkeys, Agents, Actions, or Open In updates
     * the incoming settings prop. That prop sync must not reset the selected
     * tab; tab changes are owned by explicit navigation and initial open state.
     *
     * CDXC:Settings 2026-06-12-04:13:
     * Ghostty terminal controls now save from the main Settings tab, so the tab
     * sync rule no longer treats Ghostty as a separate navigation target.
     */
    setDraft(normalizeghostexSettings(settings));
  }, [isFirstLaunchSetup, isOpen, settings]);

  /**
   * CDXC:Icons 2026-06-25-21:50:
   * Request the current icon list once whenever the App Icon settings surface
   * opens, mirroring the lazy native-data requests used elsewhere in Settings.
   * Native answers through the appIconState prop (relayed via the modal host).
   */
  useEffect(() => {
    if (!isOpen || activeTab !== 'settings' || !vscode || appIconPickerUnavailable) {
      hasRequestedAppIconsRef.current = false;
      return;
    }
    if (hasRequestedAppIconsRef.current) {
      return;
    }
    hasRequestedAppIconsRef.current = true;
    vscode.postMessage({ type: 'listAppIcons' });
  }, [activeTab, appIconPickerUnavailable, isOpen, vscode]);

  useEffect(() => {
    return () => {
      if (pendingTimeoutRef.current) {
        clearTimeout(pendingTimeoutRef.current);
      }
      if (pendingNavigationPersistTimeoutRef.current) {
        clearTimeout(pendingNavigationPersistTimeoutRef.current);
      }
    };
  }, []);
}
