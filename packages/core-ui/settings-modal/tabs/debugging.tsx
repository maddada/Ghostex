import type { ReactNode } from 'react';
import {
  areDiagnosticLoggingSettingsEqual,
  DEFAULT_ghostex_SETTINGS,
  type DiagnosticLoggingScenarioId,
  type ghostexSettings,
} from '@/packages/shared/ghostex-settings';
import { DiagnosticLoggingSettingsField, SettingsNativeScrollArea, SettingsSection, ToggleField } from '../fields';
import { hasVisibleSettingsSearchResult, shouldShowSetting, type SettingsTabSearch } from '../search';
import type { DiagnosticLoggingDurationValue, SettingModificationProps } from '../types';

/**
 * CDXC:Diagnostics 2026-09-26 DECISION:
 * User: debugging lives on its own Settings page above About, with Show debug UI controls first.
 * User: hide and disable the Storage section (storage usage and folder statistics); this supersedes the 2026-09-16 decision that showed it behind Show debug UI controls.
 */
export function DebuggingSettingsTab({
  settings,
  search,
  searchEmptyState,
  onChange,
  getModificationProps,
  onChangeDiagnosticScenarios,
}: {
  settings: ghostexSettings;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  onChange: <K extends keyof ghostexSettings>(key: K, value: ghostexSettings[K]) => void;
  getModificationProps: (key: keyof ghostexSettings) => SettingModificationProps;
  onChangeDiagnosticScenarios: (
    ids: readonly DiagnosticLoggingScenarioId[],
    duration: DiagnosticLoggingDurationValue
  ) => void;
}) {
  const visible = (section: string, key: string) => shouldShowSetting(search.sections[section], key);
  return (
    <SettingsNativeScrollArea className='settings-main-scroll' viewportClassName='settings-native-scroll-viewport'>
      <div className='settings-page-width grid gap-6 px-5 py-5'>
        <SettingsSection title='Debugging'>
          <ToggleField
            checked={settings.debuggingMode}
            description='Show diagnostic logs, and Copy Resume and Copy Attach in session menus. Warnings, errors, and crashes are always captured.'
            label='Show debug UI controls'
            {...getModificationProps('debuggingMode')}
            onChange={(checked) => onChange('debuggingMode', checked)}
          />
          {settings.debuggingMode && visible('controls', 'diagnosticLogging') ? (
            <DiagnosticLoggingSettingsField
              dependent
              isModified={
                !areDiagnosticLoggingSettingsEqual(
                  settings.diagnosticLogging,
                  DEFAULT_ghostex_SETTINGS.diagnosticLogging
                )
              }
              onChange={onChangeDiagnosticScenarios}
              onResetToDefault={() => onChange('diagnosticLogging', DEFAULT_ghostex_SETTINGS.diagnosticLogging)}
              value={settings.diagnosticLogging}
            />
          ) : null}
        </SettingsSection>
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
      </div>
    </SettingsNativeScrollArea>
  );
}
