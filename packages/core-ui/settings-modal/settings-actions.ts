/*
 * CDXC:RepoStructure 2026-08-23:
 * Sidebar preset selection, reset-to-default actions, per-setting modification
 * metadata, and the Ghostty recommended/default buttons. Plain render-time
 * factory (no React hooks).
 */
import { type Dispatch, type RefObject, type SetStateAction } from 'react';
import {
  DEFAULT_ghostex_SETTINGS,
  applySidebarSettingsPreset,
  getSidebarSettingsPresetId,
  type SidebarSettingsPresetId,
  type ghostexSettings,
  type ghostexSettingsPatch,
  type ghostexSettingsUpdateSource,
} from '../../shared/ghostex-settings';
import { type WebviewApi } from '../webview-api';
import { isAdvancedMainSetting } from './search';
import { SettingModificationProps } from './types';

export type GhosttySettingsAction =
  | 'applyRecommendedGhosttySettings'
  | 'openGhosttyConfigFile'
  | 'openGhosttySettingsDocs'
  | 'resetGhosttySettingsToDefault';

export function createSettingsActions({
  applySettings,
  applySettingsPatch,
  draft,
  onGhosttySettingsAction,
  pendingAppIconSourceIdRef,
  pendingSettingsRef,
  setAppIconError,
  vscode,
}: {
  applySettings: (nextSettings: ghostexSettings, source?: ghostexSettingsUpdateSource) => void;
  applySettingsPatch: (patch: ghostexSettingsPatch, source?: ghostexSettingsUpdateSource) => void;
  draft: ghostexSettings;
  onGhosttySettingsAction: ((action: GhosttySettingsAction) => void) | undefined;
  pendingAppIconSourceIdRef: RefObject<string | undefined>;
  pendingSettingsRef: RefObject<ghostexSettings | undefined>;
  setAppIconError: Dispatch<SetStateAction<string | undefined>>;
  vscode: WebviewApi | undefined;
}) {
  const activeSidebarSettingsPresetId = getSidebarSettingsPresetId(draft);
  const updateSidebarSettingsPreset = (presetId: SidebarSettingsPresetId) => {
    applySettings(applySidebarSettingsPreset(pendingSettingsRef.current ?? draft, presetId));
  };

  const resetSettings = () => {
    /*
     * CDXC:Icons 2026-06-26-23:42:
     * Reset to defaults must update the runtime Dock/app-switcher icon as well
     * as persisted settings. Post the default source id to native before writing
     * defaults so the current app session does not keep showing a stale custom
     * icon until restart.
     */
    pendingAppIconSourceIdRef.current = '';
    setAppIconError(undefined);
    vscode?.postMessage({ type: 'setAppIcon', sourceId: '' });
    applySettings({
      ...DEFAULT_ghostex_SETTINGS,
      remoteMachines: (pendingSettingsRef.current ?? draft).remoteMachines,
    });
  };
  const resetSetting = <Key extends keyof ghostexSettings>(key: Key) => {
    applySettingsPatch({ [key]: DEFAULT_ghostex_SETTINGS[key] } as Pick<ghostexSettings, Key>);
  };
  const getSettingModificationProps = <Key extends keyof ghostexSettings>(
    key: Key
  ): Required<SettingModificationProps> => ({
    advanced: isAdvancedMainSetting(String(key)),
    isModified: !Object.is(draft[key], DEFAULT_ghostex_SETTINGS[key]),
    onResetToDefault: () => resetSetting(key),
  });

  const applyRecommendedGhosttySettings = () => {
    /**
     * CDXC:Terminal 2026-04-30-01:48
     * The recommended Ghostty button must update both the visible ghostex controls
     * and the real Ghostty config keys that are not modeled in ghostex settings.
     */
    applySettings({
      ...draft,
      terminalCursorStyle: 'bar',
      terminalFontFamily: 'JetBrains Mono',
      terminalFontSize: 13,
      terminalFontWeight: 400,
      terminalLetterSpacing: 0,
      terminalLineHeight: 1.2,
      terminalMouseScrollMultiplierDiscrete: 1,
      terminalMouseScrollMultiplierPrecision: 1,
    });
    onGhosttySettingsAction?.('applyRecommendedGhosttySettings');
  };

  const resetGhosttySettingsToDefault = () => {
    /**
     * CDXC:Terminal 2026-04-30-01:48
     * Resetting Ghostty defaults should also move the visible terminal
     * controls back to ghostex defaults, then remove managed keys from the real
     * Ghostty config so Ghostty's own defaults take effect.
     */
    applySettings({
      ...draft,
      terminalCursorStyle: DEFAULT_ghostex_SETTINGS.terminalCursorStyle,
      terminalFontFamily: DEFAULT_ghostex_SETTINGS.terminalFontFamily,
      terminalFontSize: DEFAULT_ghostex_SETTINGS.terminalFontSize,
      terminalFontWeight: DEFAULT_ghostex_SETTINGS.terminalFontWeight,
      terminalLetterSpacing: DEFAULT_ghostex_SETTINGS.terminalLetterSpacing,
      terminalLineHeight: DEFAULT_ghostex_SETTINGS.terminalLineHeight,
      terminalMouseScrollMultiplierDiscrete: DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete,
      terminalMouseScrollMultiplierPrecision: DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision,
      terminalScrollToBottomWhenTyping: DEFAULT_ghostex_SETTINGS.terminalScrollToBottomWhenTyping,
    });
    onGhosttySettingsAction?.('resetGhosttySettingsToDefault');
  };

  return {
    activeSidebarSettingsPresetId,
    applyRecommendedGhosttySettings,
    getSettingModificationProps,
    resetGhosttySettingsToDefault,
    resetSettings,
    updateSidebarSettingsPreset,
  };
}
