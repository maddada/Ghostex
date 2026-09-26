import { lazy, Suspense, type ReactNode } from 'react';
import {
  areDiagnosticLoggingSettingsEqual,
  DEFAULT_ghostex_SETTINGS,
  type DiagnosticLoggingScenarioId,
  type ghostexSettings,
} from '@/packages/shared/ghostex-settings';
import type { SidebarGhostexFolderStatsMessage } from '@/packages/shared/session-grid-contract';
import { DiagnosticLoggingSettingsField, SettingsNativeScrollArea, SettingsSection, ToggleField } from '../fields';
import { hasVisibleSettingsSearchResult, shouldShowSetting, type SettingsTabSearch } from '../search';
import type { DiagnosticLoggingDurationValue, SettingModificationProps } from '../types';

const StorageInspector = lazy(() =>
  import('../storage-inspector').then((module) => ({ default: module.StorageInspector }))
);
const FolderStorageStats = lazy(() =>
  import('../folder-storage-stats').then((module) => ({ default: module.FolderStorageStats }))
);

/**
 * CDXC:Diagnostics 2026-09-16 DECISION:
 * User: move debugging to its own Settings page above About, put Show debug UI controls first, and do not show or load the storage inspector until it is enabled.
 * Restore the previously hidden folder storage statistics here under the same gate; this supersedes the 2026-09-12 decision to disable that section.
 */
export function DebuggingSettingsTab({
  settings,
  search,
  searchEmptyState,
  onChange,
  getModificationProps,
  onChangeDiagnosticScenario,
  folderStats,
  folderStatsLoading,
  onRequestFolderStats,
  onOpenFolder,
}: {
  settings: ghostexSettings;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  onChange: <K extends keyof ghostexSettings>(key: K, value: ghostexSettings[K]) => void;
  getModificationProps: (key: keyof ghostexSettings) => SettingModificationProps;
  onChangeDiagnosticScenario: (id: DiagnosticLoggingScenarioId, duration: DiagnosticLoggingDurationValue) => void;
  folderStats?: SidebarGhostexFolderStatsMessage;
  folderStatsLoading: boolean;
  onRequestFolderStats?: () => void;
  onOpenFolder?: () => void;
}) {
  const visible = (section: string, key: string) => shouldShowSetting(search.sections[section], key);
  return (
    <SettingsNativeScrollArea className='settings-main-scroll' viewportClassName='settings-native-scroll-viewport'>
      <div className='settings-page-width grid gap-6 px-5 py-5'>
        <SettingsSection title='Debugging'>
          <ToggleField
            checked={settings.debuggingMode}
            description='Show debug-only controls, load storage statistics, and allow enabled routine diagnostic logs. Important warnings, errors, and crashes remain captured when off.'
            label='Show debug UI controls'
            {...getModificationProps('debuggingMode')}
            onChange={(checked) => onChange('debuggingMode', checked)}
          />
          {settings.debuggingMode ? (
            <>
              {visible('controls', 'diagnosticLogging') ? (
                <DiagnosticLoggingSettingsField
                  dependent
                  isModified={
                    !areDiagnosticLoggingSettingsEqual(
                      settings.diagnosticLogging,
                      DEFAULT_ghostex_SETTINGS.diagnosticLogging
                    )
                  }
                  onChange={onChangeDiagnosticScenario}
                  onResetToDefault={() => onChange('diagnosticLogging', DEFAULT_ghostex_SETTINGS.diagnosticLogging)}
                  value={settings.diagnosticLogging}
                />
              ) : null}
              {visible('controls', 'showSessionCommandCopyActions') ? (
                <ToggleField
                  checked={settings.showSessionCommandCopyActions}
                  description='Show Copy resume and Copy attach command in session context menus.'
                  dependent
                  label='Show command copy actions'
                  {...getModificationProps('showSessionCommandCopyActions')}
                  onChange={(checked) => onChange('showSessionCommandCopyActions', checked)}
                />
              ) : null}
              {visible('controls', 'showSessionDetailsCopyAction') ? (
                <ToggleField
                  checked={settings.showSessionDetailsCopyAction}
                  description='Show Copy Details in session context menus.'
                  dependent
                  label='Show Copy Details option'
                  {...getModificationProps('showSessionDetailsCopyAction')}
                  onChange={(checked) => onChange('showSessionDetailsCopyAction', checked)}
                />
              ) : null}
            </>
          ) : null}
        </SettingsSection>
        {settings.debuggingMode ? (
          <>
            {visible('storage', 'storageUsage') ? (
              <Suspense fallback={<p className='text-sm text-muted-foreground'>Loading storage usage...</p>}>
                <StorageInspector />
              </Suspense>
            ) : null}
            {visible('storage', 'storageStats') ? (
              <Suspense fallback={<p className='text-sm text-muted-foreground'>Loading folder statistics...</p>}>
                <FolderStorageStats
                  stats={folderStats}
                  isLoading={folderStatsLoading}
                  onRequest={onRequestFolderStats}
                  onOpenFolder={onOpenFolder}
                />
              </Suspense>
            ) : null}
          </>
        ) : null}
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
      </div>
    </SettingsNativeScrollArea>
  );
}
