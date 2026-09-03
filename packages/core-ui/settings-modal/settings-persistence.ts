/*
 * CDXC:RepoStructure 2026-08-23:
 * Immediate-save settings writes, the trailing debounce used by numeric
 * controls, and Settings navigation persistence. Plain render-time factory (no
 * React hooks) whose returned closures are bound in the component at the same
 * point the inline declarations used to appear.
 */
import { type Dispatch, type RefObject, type SetStateAction } from 'react';
import {
  normalizeghostexSettings,
  setDiagnosticLoggingScenario,
  type DiagnosticLoggingScenarioId,
  type ghostexSettings,
  type ghostexSettingsPatch,
  type ghostexSettingsUpdateSource,
} from '../../shared/ghostex-settings';
import { type SettingsModalTab } from '../settings-modal-tabs';
import { getDiagnosticLoggingScenarioStateForDuration } from './fields';
import { areSettingsModalNavigationStatesEqual, getRememberedSettingsModalNavigationState } from './navigation-memory';
import { DiagnosticLoggingDurationValue } from './types';

const NUMERIC_SETTINGS_DEBOUNCE_MS = 180;
const SETTINGS_MODAL_NAVIGATION_SCROLL_DEBOUNCE_MS = 220;

function createNormalizedSettingsPatch(
  normalizedSettings: ghostexSettings,
  patch: ghostexSettingsPatch
): ghostexSettingsPatch {
  return Object.fromEntries(
    (Object.keys(patch) as Array<keyof ghostexSettings>).map((key) => [key, normalizedSettings[key]])
  ) as ghostexSettingsPatch;
}

export function createSettingsPersistence({
  activeTab,
  draft,
  isFirstLaunchSetup,
  onChange,
  onClose,
  onPatch,
  pendingNavigationPersistTimeoutRef,
  pendingSettingsPatchRef,
  pendingSettingsRef,
  pendingTimeoutRef,
  rememberActiveScrollPosition,
  setDraft,
}: {
  activeTab: SettingsModalTab;
  draft: ghostexSettings;
  isFirstLaunchSetup: boolean;
  onChange: (settings: ghostexSettings, source?: ghostexSettingsUpdateSource) => void;
  onClose: () => void;
  onPatch: ((patch: ghostexSettingsPatch, source: ghostexSettingsUpdateSource) => void) | undefined;
  pendingNavigationPersistTimeoutRef: RefObject<ReturnType<typeof setTimeout> | undefined>;
  pendingSettingsPatchRef: RefObject<ghostexSettingsPatch | undefined>;
  pendingSettingsRef: RefObject<ghostexSettings | undefined>;
  pendingTimeoutRef: RefObject<ReturnType<typeof setTimeout> | undefined>;
  rememberActiveScrollPosition: () => void;
  setDraft: Dispatch<SetStateAction<ghostexSettings>>;
}) {
  const clearPendingSettings = () => {
    if (pendingTimeoutRef.current) {
      clearTimeout(pendingTimeoutRef.current);
      pendingTimeoutRef.current = undefined;
    }
  };

  const clearPendingNavigationPersist = () => {
    if (pendingNavigationPersistTimeoutRef.current) {
      clearTimeout(pendingNavigationPersistTimeoutRef.current);
      pendingNavigationPersistTimeoutRef.current = undefined;
    }
  };

  const postSettingsPatch = (
    patch: ghostexSettingsPatch,
    source: ghostexSettingsUpdateSource,
    fallbackSettings: ghostexSettings
  ) => {
    if (Object.keys(patch).length === 0) {
      return;
    }
    if (onPatch) {
      onPatch(patch, source);
      return;
    }
    onChange(fallbackSettings, source);
  };

  const persistSettingsModalNavigation = (navigationActiveTab: SettingsModalTab = activeTab) => {
    rememberActiveScrollPosition();
    const pendingSettings = pendingSettingsRef.current;
    const pendingPatch = pendingSettingsPatchRef.current;
    const baseSettings = pendingSettings ?? draft;
    const nextSettings = isFirstLaunchSetup
      ? baseSettings
      : normalizeghostexSettings({
          ...baseSettings,
          settingsModalNavigation: getRememberedSettingsModalNavigationState(
            navigationActiveTab,
            baseSettings.settingsModalNavigation
          ),
        });
    const shouldPersistNavigation =
      !isFirstLaunchSetup &&
      !areSettingsModalNavigationStatesEqual(
        baseSettings.settingsModalNavigation,
        nextSettings.settingsModalNavigation
      );
    /*
     * CDXC:Settings 2026-06-30-04:47:
     * Native Settings is an AppKit child window, so closing it with native
     * chrome can bypass the React Dialog close callback. Persist page changes
     * immediately and scroll changes after they settle; close remains a final
     * flush for pending numeric edits and any unsaved navigation state.
     *
     * CDXC:RemoteMachines 2026-06-30-15:18:
     * Navigation persistence is a patch-only write. Opening or scrolling Settings
     * must never post a full draft that could overwrite unrelated domains such as
     * remoteMachines from stale modal state.
     */
    clearPendingNavigationPersist();
    clearPendingSettings();
    pendingSettingsRef.current = undefined;
    pendingSettingsPatchRef.current = undefined;
    if (pendingSettings || shouldPersistNavigation) {
      setDraft(nextSettings);
      postSettingsPatch(
        {
          ...(pendingPatch ?? {}),
          ...(shouldPersistNavigation ? { settingsModalNavigation: nextSettings.settingsModalNavigation } : {}),
        },
        pendingPatch ? 'settings:control' : 'settings:navigation',
        nextSettings
      );
    }
  };

  const scheduleSettingsModalNavigationPersist = (navigationActiveTab: SettingsModalTab = activeTab) => {
    if (isFirstLaunchSetup) {
      return;
    }
    clearPendingNavigationPersist();
    pendingNavigationPersistTimeoutRef.current = setTimeout(() => {
      pendingNavigationPersistTimeoutRef.current = undefined;
      persistSettingsModalNavigation(navigationActiveTab);
    }, SETTINGS_MODAL_NAVIGATION_SCROLL_DEBOUNCE_MS);
  };

  const closeSettingsModal = () => {
    persistSettingsModalNavigation(activeTab);
    onClose();
  };

  const applySettings = (nextSettings: ghostexSettings, source: ghostexSettingsUpdateSource = 'settings:bulk') => {
    const normalizedSettings = normalizeghostexSettings(nextSettings);
    clearPendingSettings();
    pendingSettingsRef.current = undefined;
    pendingSettingsPatchRef.current = undefined;
    setDraft(normalizedSettings);
    onChange(normalizedSettings, source);
  };

  const applySettingsPatch = (
    patch: ghostexSettingsPatch,
    source: ghostexSettingsUpdateSource = 'settings:control'
  ) => {
    const normalizedSettings = normalizeghostexSettings({
      ...(pendingSettingsRef.current ?? draft),
      ...patch,
    });
    const normalizedPatch = createNormalizedSettingsPatch(normalizedSettings, patch);
    clearPendingSettings();
    pendingSettingsRef.current = undefined;
    pendingSettingsPatchRef.current = undefined;
    setDraft(normalizedSettings);
    postSettingsPatch(normalizedPatch, source, normalizedSettings);
  };

  /**
   * CDXC:Settings 2026-04-26-11:13: Numeric settings use sliders with adjacent
   * number boxes. Dragging or typing updates the visible value immediately, but
   * persists through a short trailing debounce to avoid flooding settings writes.
   * Number boxes keep local edit text so partial values can be typed cleanly.
   */
  const applySettingsPatchDebounced = (
    patch: ghostexSettingsPatch,
    source: ghostexSettingsUpdateSource = 'settings:control'
  ) => {
    const normalizedSettings = normalizeghostexSettings({
      ...(pendingSettingsRef.current ?? draft),
      ...patch,
    });
    const normalizedPatch = createNormalizedSettingsPatch(normalizedSettings, patch);
    pendingSettingsRef.current = normalizedSettings;
    pendingSettingsPatchRef.current = {
      ...(pendingSettingsPatchRef.current ?? {}),
      ...normalizedPatch,
    };
    setDraft(normalizedSettings);
    clearPendingSettings();
    pendingTimeoutRef.current = setTimeout(() => {
      const pendingSettings = pendingSettingsRef.current;
      const pendingPatch = pendingSettingsPatchRef.current;
      pendingSettingsRef.current = undefined;
      pendingSettingsPatchRef.current = undefined;
      pendingTimeoutRef.current = undefined;
      if (pendingSettings) {
        postSettingsPatch(pendingPatch ?? {}, source, pendingSettings);
      }
    }, NUMERIC_SETTINGS_DEBOUNCE_MS);
  };

  /**
   * CDXC:Settings 2026-04-26-10:12: Settings changes must apply immediately.
   * The settings dialog keeps local state only for responsive controls, then
   * posts every normalized change instead of waiting for Save/Cancel actions.
   */
  const updateDraft = <Key extends keyof ghostexSettings>(key: Key, value: ghostexSettings[Key]) => {
    applySettingsPatch({ [key]: value } as Pick<ghostexSettings, Key>);
  };
  const updateShowAdvancedSettings = (checked: boolean) => {
    /*
     * CDXC:Settings 2026-06-28-08:01:
     * Show Advanced is settings chrome, but it still needs immediate durable
     * persistence so restart hydration reopens Settings with the same advanced
     * row visibility the user explicitly chose.
     */
    applySettingsPatch({ showAdvancedSettings: checked });
  };
  const updateDiagnosticLoggingScenario = (
    scenarioId: DiagnosticLoggingScenarioId,
    duration: DiagnosticLoggingDurationValue
  ) => {
    updateDraft(
      'diagnosticLogging',
      setDiagnosticLoggingScenario(
        (pendingSettingsRef.current ?? draft).diagnosticLogging,
        scenarioId,
        getDiagnosticLoggingScenarioStateForDuration(duration)
      )
    );
  };
  const updateDraftDebounced = <Key extends keyof ghostexSettings>(key: Key, value: ghostexSettings[Key]) => {
    applySettingsPatchDebounced({ [key]: value } as Pick<ghostexSettings, Key>);
  };

  return {
    applySettings,
    applySettingsPatch,
    closeSettingsModal,
    persistSettingsModalNavigation,
    scheduleSettingsModalNavigationPersist,
    updateDiagnosticLoggingScenario,
    updateDraft,
    updateDraftDebounced,
    updateShowAdvancedSettings,
  };
}
