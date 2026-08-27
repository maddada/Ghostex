import { type ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Field, FieldContent, FieldDescription, FieldTitle } from '@/packages/components/ui/field';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { Switch } from '@/packages/components/ui/switch';
import { AppTooltip } from '../../app-tooltip';
import {
  IconBolt,
  IconCodeDots,
  IconDeviceDesktop,
  IconFileText,
  IconFolderOpen,
  IconGitCommit,
  IconInfoCircle,
  IconPlayerPlay,
  IconRefresh,
  IconWorld,
} from '@tabler/icons-react';
import {
  type SidebarPluginSettingsItem,
  type SidebarPluginSettingsStatusMessage,
} from '../../../shared/session-grid-contract';
import {
  CHAT_FILE_OPEN_VIEW_OPTIONS,
  type ChatFileOpenView,
  type ghostexSettings,
} from '../../../shared/ghostex-settings';
import { SettingButton, SettingRow, SettingsNativeScrollArea, SettingsSection } from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';
import { IntegrationSettingsRow } from './integrations';

export type PluginVisibilitySettingKey =
  | 'codeViewTabHidden'
  | 'browserViewTabHidden'
  | 'kanbanViewTabHidden'
  | 'automateViewTabHidden'
  | 'docsViewTabHidden'
  | 'tipsAndTricksTitlebarButtonHidden'
  | 'resourcesTitlebarButtonHidden'
  | 'gitActionsTitlebarButtonHidden'
  | 'quickActionsTitlebarButtonHidden'
  | 'openInTitlebarButtonHidden';

type CustomizeSettingKey = PluginVisibilitySettingKey | 'markdownFileOpenView' | 'htmlFileOpenView';

export function PluginsSettingsTab({
  onRequestStatus,
  onReinstallPlugin,
  onUpdateSetting,
  search,
  searchEmptyState,
  settings,
  status,
  statusLoading,
}: {
  onRequestStatus?: () => void;
  onReinstallPlugin?: (pluginId: SidebarPluginSettingsItem['id']) => void;
  onUpdateSetting: <K extends CustomizeSettingKey>(key: K, value: ghostexSettings[K]) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  settings: ghostexSettings;
  status?: SidebarPluginSettingsStatusMessage;
  statusLoading: boolean;
}) {
  const statusById = new Map(status?.plugins.map((plugin) => [plugin.id, plugin]));
  const code = statusById.get('code');
  const kanban = statusById.get('kanban');
  const cef = statusById.get('cef');
  const showViewTab = (key: string) => shouldShowSetting(search.sections.viewTabs, key);
  const showFileOpeningSetting = (key: string) => shouldShowSetting(search.sections.fileOpening, key);
  const showQuickAccessButton = (key: string) => shouldShowSetting(search.sections.quickAccessButtons, key);

  return (
    <SettingsNativeScrollArea className='h-full min-h-0'>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
        {shouldShowSettingsSection(search.sections.viewTabs) ? (
          <SettingsSection
            description='Choose which project workareas appear in the title bar. Hiding a tab does not stop its runtime or disable its other entry points.'
            title='Plugins'
          >
            {showViewTab('code') ? (
              <PluginManagedSettingsRow
                description='Explore, edit, and search your project in a familiar, full-featured workspace without ever leaving Ghostex.'
                icon={IconCodeDots}
                onReinstall={() => onReinstallPlugin?.('code')}
                onVisibleChange={(visible) => onUpdateSetting('codeViewTabHidden', !visible)}
                reinstallAvailable={Boolean(onReinstallPlugin && code?.canReinstall)}
                runtime={code}
                title='Code'
                visible={!settings.codeViewTabHidden}
              />
            ) : null}
            {showViewTab('browser') ? (
              <PluginManagedSettingsRow
                description='Open websites alongside your project and keep useful pages organized without leaving Ghostex. If it’s the last choice beside Agents, hiding it clears the switcher too.'
                icon={IconWorld}
                onVisibleChange={(visible) => onUpdateSetting('browserViewTabHidden', !visible)}
                title='Browser'
                visible={!settings.browserViewTabHidden}
              />
            ) : null}
            {showViewTab('kanban') ? (
              <PluginManagedSettingsRow
                description='Plan upcoming work, organize tasks by progress, and keep your whole project easy to follow at a glance.'
                icon={IconPlayerPlay}
                onReinstall={() => onReinstallPlugin?.('kanban')}
                onVisibleChange={(visible) => onUpdateSetting('kanbanViewTabHidden', !visible)}
                reinstallAvailable={Boolean(onReinstallPlugin && kanban?.canReinstall)}
                runtime={kanban}
                title='Kanban'
                visible={!settings.kanbanViewTabHidden}
              />
            ) : null}
            {showViewTab('automate') ? (
              <PluginManagedSettingsRow
                description='Turn repeatable project routines into simple workflows you can run whenever you need them.'
                icon={IconBolt}
                onVisibleChange={(visible) => onUpdateSetting('automateViewTabHidden', !visible)}
                title='Automate'
                visible={!settings.automateViewTabHidden}
              />
            ) : null}
            {showViewTab('docs') ? (
              <PluginManagedSettingsRow
                description='Browse your project’s notes, plans, and reference files together in one focused reading space.'
                icon={IconFileText}
                onVisibleChange={(visible) => onUpdateSetting('docsViewTabHidden', !visible)}
                title='Docs'
                visible={!settings.docsViewTabHidden}
              />
            ) : null}
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.fileOpening) ? (
          <SettingsSection
            description='Choose where supported file links from agent chat open. If that view is unavailable, Ghostex uses the other available view.'
            title='File opening'
          >
            {showFileOpeningSetting('markdown') ? (
              <ChatFileOpenViewSetting
                id='markdown-file-open-view'
                label='Markdown files'
                onChange={(value) => onUpdateSetting('markdownFileOpenView', value)}
                subtitle='Applies to .md, .markdown, .mdown, and .mkdn links in agent chat.'
                value={settings.markdownFileOpenView}
              />
            ) : null}
            {showFileOpeningSetting('html') ? (
              <ChatFileOpenViewSetting
                id='html-file-open-view'
                label='HTML files'
                onChange={(value) => onUpdateSetting('htmlFileOpenView', value)}
                subtitle='Applies to .html and .htm links in agent chat.'
                value={settings.htmlFileOpenView}
              />
            ) : null}
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.components) ? (
          <SettingsSection
            actions={
              <SettingButton
                disabled={statusLoading || !onRequestStatus}
                disabledReason={
                  statusLoading ? 'Plugin status is being checked.' : 'Status refresh isn’t available here.'
                }
                onClick={onRequestStatus}
                type='button'
                variant='ghost'
              >
                <IconRefresh
                  aria-hidden='true'
                  className={cn(statusLoading && 'animate-spin')}
                  data-icon='inline-start'
                />
                Refresh
              </SettingButton>
            }
            description={
              <>
                <span className='block'>Runtime components shared by Ghostex surfaces and agent workflows.</span>
                <span className='block'>Check their status and keep them up to date here.</span>
              </>
            }
            descriptionClassName='pb-2'
            title='Shared components'
          >
            {shouldShowSetting(search.sections.components, 'cef') ? (
              <PluginManagedSettingsRow
                description='Chromium Embedded Framework powers Ghostex web surfaces and remains enabled because the app requires it.'
                icon={IconDeviceDesktop}
                onReinstall={() => onReinstallPlugin?.('cef')}
                reinstallAvailable={Boolean(onReinstallPlugin && cef?.canReinstall)}
                runtime={cef}
                title='Chromium runtime (CEF)'
              />
            ) : null}
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.quickAccessButtons) ? (
          <SettingsSection
            description='This is the same button cluster shown on the right side of the title bar. Click any button to show or hide it; its feature stays available everywhere else.'
            title='Quick access buttons'
          >
            <Field className='rounded-none border border-border bg-muted/20 px-4 py-3'>
              <div className='flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between'>
                <FieldContent>
                  <FieldTitle className='text-sm'>Titlebar preview</FieldTitle>
                  <FieldDescription className='text-xs text-muted-foreground'>
                    Bright buttons are enabled and shown. Outlined buttons are hidden.
                  </FieldDescription>
                </FieldContent>
                <div
                  aria-label='Quick access button visibility'
                  className='flex w-fit shrink-0 items-stretch gap-[2px]'
                  role='group'
                >
                  {showQuickAccessButton('tips') ? (
                    <QuickAccessTitlebarButton
                      icon={IconInfoCircle}
                      label='Tips'
                      onToggle={() =>
                        onUpdateSetting(
                          'tipsAndTricksTitlebarButtonHidden',
                          !settings.tipsAndTricksTitlebarButtonHidden
                        )
                      }
                      visible={!settings.tipsAndTricksTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton('resources') ? (
                    <QuickAccessTitlebarButton
                      icon={IconDeviceDesktop}
                      label='Resources'
                      onToggle={() =>
                        onUpdateSetting('resourcesTitlebarButtonHidden', !settings.resourcesTitlebarButtonHidden)
                      }
                      visible={!settings.resourcesTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton('gitActions') ? (
                    <QuickAccessTitlebarButton
                      icon={IconGitCommit}
                      label='Git actions'
                      onToggle={() =>
                        onUpdateSetting('gitActionsTitlebarButtonHidden', !settings.gitActionsTitlebarButtonHidden)
                      }
                      visible={!settings.gitActionsTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton('quickActions') ? (
                    <QuickAccessTitlebarButton
                      icon={IconPlayerPlay}
                      label='Quick Actions'
                      onToggle={() =>
                        onUpdateSetting('quickActionsTitlebarButtonHidden', !settings.quickActionsTitlebarButtonHidden)
                      }
                      visible={!settings.quickActionsTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton('openIn') ? (
                    <QuickAccessTitlebarButton
                      icon={IconFolderOpen}
                      label='Open In'
                      onToggle={() =>
                        onUpdateSetting('openInTitlebarButtonHidden', !settings.openInTitlebarButtonHidden)
                      }
                      visible={!settings.openInTitlebarButtonHidden}
                    />
                  ) : null}
                </div>
              </div>
            </Field>
          </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

function ChatFileOpenViewSetting({
  id,
  label,
  onChange,
  subtitle,
  value,
}: {
  id: string;
  label: string;
  onChange: (value: ChatFileOpenView) => void;
  subtitle: string;
  value: ChatFileOpenView;
}) {
  return (
    <SettingRow htmlFor={id} label={label} subtitle={subtitle}>
      <SegmentedControl
        aria-label={`${label} open view`}
        id={id}
        onValueChange={(nextValue) => onChange(nextValue as ChatFileOpenView)}
        size='sm'
        value={value}
      >
        {CHAT_FILE_OPEN_VIEW_OPTIONS.map((option) => (
          <SegmentedControlItem key={option.value} value={option.value}>
            {option.label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
    </SettingRow>
  );
}

export function PluginManagedSettingsRow({
  description,
  icon,
  onReinstall,
  onVisibleChange,
  reinstallAvailable,
  runtime,
  title,
  visible,
}: {
  description: string;
  icon: typeof IconInfoCircle;
  onReinstall?: () => void;
  onVisibleChange?: (visible: boolean) => void;
  reinstallAvailable?: boolean;
  runtime?: SidebarPluginSettingsItem;
  title: string;
  visible?: boolean;
}) {
  const busy = runtime !== undefined && !['installed', 'notInstalled', 'failed'].includes(runtime.status);
  const actionLabel = runtime?.status === 'notInstalled' ? 'Install' : 'Reinstall';
  const detail = runtime ? `${description}${runtime.errorMessage ? ` · ${runtime.errorMessage}` : ''}` : description;
  const tone = runtime
    ? runtime.status === 'installed'
      ? 'success'
      : runtime.status === 'failed'
        ? 'warning'
        : 'neutral'
    : 'success';
  return (
    <IntegrationSettingsRow
      description={detail}
      icon={icon}
      status={runtime?.statusLabel ?? 'Built in'}
      title={title}
      tone={tone}
      version={runtime?.version}
    >
      {onReinstall ? (
        <SettingButton
          disabled={busy || !reinstallAvailable}
          disabledReason={
            busy ? `${title} is being installed.` : 'This build does not provide a reinstallable remote component.'
          }
          onClick={onReinstall}
          type='button'
          variant='outline'
        >
          <IconRefresh aria-hidden='true' className={cn(busy && 'animate-spin')} data-icon='inline-start' />
          {actionLabel}
        </SettingButton>
      ) : null}
      {onVisibleChange && visible !== undefined ? (
        <label className='flex h-8 items-center gap-2 px-1 text-xs text-muted-foreground'>
          Visible
          <Switch aria-label={`Show ${title} in the title bar`} checked={visible} onCheckedChange={onVisibleChange} />
        </label>
      ) : null}
    </IntegrationSettingsRow>
  );
}

export function QuickAccessTitlebarButton({
  icon: Icon,
  label,
  onToggle,
  visible,
}: {
  icon: typeof IconInfoCircle;
  label: string;
  onToggle: () => void;
  visible: boolean;
}) {
  return (
    <AppTooltip content={`${label} is ${visible ? 'shown' : 'hidden'}. Click to ${visible ? 'hide' : 'show'}.`}>
      <Button
        aria-label={`${visible ? 'Hide' : 'Show'} ${label} in the title bar`}
        aria-pressed={visible}
        onClick={onToggle}
        size='icon'
        style={visible ? { backgroundColor: '#e5e5e5', borderColor: '#e5e5e5', color: '#0a0a0a' } : undefined}
        type='button'
        variant='outline'
      >
        <Icon aria-hidden='true' />
      </Button>
    </AppTooltip>
  );
}
