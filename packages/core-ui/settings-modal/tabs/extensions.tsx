/*
 * CDXC:Extensions 2026-08-30:
 * Settings has one Extensions page. The "Official Extensions" section is the
 * features Ghostex ships itself, backed by the inverted `*Hidden` settings keys
 * in `GHOSTEX_OFFICIAL_EXTENSIONS`; below it the same page embeds the real
 * extension store and installed list. Both read as one family of cards, which
 * is why the official rows reuse the `.extensions-*` panel skin instead of the
 * stacked settings-field layout the other Settings pages use.
 *
 * Opening an extension's details replaces the whole page (not just the store
 * section), and the list scroll position is restored on the way back.
 *
 * This replaced the old "Customize" page (tab id `plugins`) and the standalone
 * Extensions app modal.
 */
import { useLayoutEffect, useMemo, useRef, type ReactNode, type UIEvent } from 'react';
import { cn } from '@/packages/components/utils';
import { Switch } from '@/packages/components/ui/switch';
import {
  IconBolt,
  IconCodeDots,
  IconDeviceDesktop,
  IconExternalLink,
  IconFileText,
  IconFolderOpen,
  IconGitCommit,
  IconInfoCircle,
  IconPlayerPlay,
  IconPuzzle,
  IconRefresh,
  IconWorld,
  type Icon as TablerIcon,
} from '@tabler/icons-react';
import {
  GHOSTEX_OFFICIAL_EXTENSIONS,
  isOfficialExtensionEnabled,
  type GhostexOfficialExtension,
  type GhostexOfficialExtensionId,
  type GhostexOfficialExtensionSettingsKey,
} from '../../../shared/ghostex-official-extensions';
import {
  type SidebarPluginSettingsItem,
  type SidebarPluginSettingsStatusMessage,
} from '../../../shared/session-grid-contract';
import { type ghostexSettings } from '../../../shared/ghostex-settings';
import { type WebviewApi } from '../../webview-api';
import { ExtensionsBrowserDetail, ExtensionsBrowserList, useExtensionsBrowserState } from '../../extensions-modal';
import { createExtensionsModalTransport } from '../../extensions-modal/transport';
import { SettingButton, SettingsNativeScrollArea, SettingsSection } from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';

export type OfficialExtensionSettingKey = GhostexOfficialExtensionSettingsKey;

const GHOSTEX_EXTENSIONS_REPO_URL = 'https://github.com/maddada/ghostex-extensions';

const OFFICIAL_EXTENSION_ICONS: Record<GhostexOfficialExtensionId, TablerIcon> = {
  automate: IconBolt,
  browser: IconWorld,
  code: IconCodeDots,
  docs: IconFileText,
  extensionsButton: IconPuzzle,
  gitActions: IconGitCommit,
  kanban: IconPlayerPlay,
  openIn: IconFolderOpen,
  quickActions: IconPlayerPlay,
  resources: IconDeviceDesktop,
  tips: IconInfoCircle,
};

/** Official entries whose runtime component the app can install or reinstall. */
const OFFICIAL_EXTENSION_RUNTIME_IDS: Partial<Record<GhostexOfficialExtensionId, SidebarPluginSettingsItem['id']>> = {
  code: 'code',
  kanban: 'kanban',
};

const OFFICIAL_VIEW_EXTENSIONS = GHOSTEX_OFFICIAL_EXTENSIONS.filter((entry) => entry.placement === 'view');
const OFFICIAL_TITLEBAR_EXTENSIONS = GHOSTEX_OFFICIAL_EXTENSIONS.filter(
  (entry) => entry.placement === 'titlebar-button'
);

export function ExtensionsSettingsTab({
  isActive,
  onRequestStatus,
  onReinstallPlugin,
  onUpdateSetting,
  search,
  searchEmptyState,
  settings,
  status,
  statusLoading,
  vscode,
}: {
  isActive: boolean;
  onRequestStatus?: () => void;
  onReinstallPlugin?: (pluginId: SidebarPluginSettingsItem['id']) => void;
  onUpdateSetting: <K extends OfficialExtensionSettingKey>(key: K, value: ghostexSettings[K]) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  settings: ghostexSettings;
  status?: SidebarPluginSettingsStatusMessage;
  statusLoading: boolean;
  vscode?: WebviewApi;
}) {
  const statusById = new Map(status?.plugins.map((plugin) => [plugin.id, plugin]));
  const cef = statusById.get('cef');
  /*
   * CDXC:Extensions 2026-08-30:
   * Only the desktop shell exposes a gxserver bootstrap, so the store section
   * is built once per mount and the whole third-party section is dropped where
   * there is nothing to talk to (the web app mounts this same Settings modal).
   */
  const transport = useMemo(() => createExtensionsModalTransport(), []);
  const browser = useExtensionsBrowserState({ active: isActive && Boolean(transport), transport });
  const detailOpen = Boolean(transport) && browser.detailOpen;
  const showOfficial = (key: string) => shouldShowSetting(search.sections.official, key);

  /*
   * CDXC:Extensions 2026-08-30:
   * The detail page replaces the whole list, so the list's scroll offset would
   * be lost when the shorter/longer detail DOM swaps in. Record the offset
   * while the list is visible, start the detail page at the top, and restore
   * the recorded offset when the user navigates back.
   */
  const contentRef = useRef<HTMLDivElement | null>(null);
  const listScrollTop = useRef(0);
  const handleScrollCapture = (event: UIEvent<HTMLDivElement>) => {
    if (detailOpen) return;
    const viewport = event.target as HTMLElement;
    if (viewport.dataset.slot === 'scroll-area-viewport') listScrollTop.current = viewport.scrollTop;
  };
  useLayoutEffect(() => {
    const viewport = contentRef.current?.closest('[data-slot="scroll-area-viewport"]');
    if (viewport) viewport.scrollTop = detailOpen ? 0 : listScrollTop.current;
  }, [detailOpen]);

  return (
    <SettingsNativeScrollArea className='h-full min-h-0' onScrollCapture={handleScrollCapture}>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5' ref={contentRef}>
        {detailOpen ? (
          <div className='pt-5'>
            <ExtensionsBrowserDetail state={browser} />
          </div>
        ) : (
          <>
            {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
            {shouldShowSettingsSection(search.sections.official) ? (
              <SettingsSection
                actions={
                  <SettingButton
                    disabled={statusLoading || !onRequestStatus}
                    disabledReason={
                      statusLoading ? 'Component status is being checked.' : 'Status refresh isn’t available here.'
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
                description='Extensions Ghostex ships and maintains.'
                descriptionClassName='pb-2'
                title='Official Extensions'
              >
                <OfficialExtensionList
                  extensions={OFFICIAL_VIEW_EXTENSIONS}
                  label='Workareas'
                  onReinstallPlugin={onReinstallPlugin}
                  onUpdateSetting={onUpdateSetting}
                  settings={settings}
                  showOfficial={showOfficial}
                  statusById={statusById}
                />
                <OfficialExtensionList
                  extensions={OFFICIAL_TITLEBAR_EXTENSIONS}
                  label='Title bar buttons'
                  onReinstallPlugin={onReinstallPlugin}
                  onUpdateSetting={onUpdateSetting}
                  settings={settings}
                  showOfficial={showOfficial}
                  statusById={statusById}
                />
                {showOfficial('cef') ? (
                  <OfficialExtensionGroup label='Shared runtime'>
                    <OfficialExtensionRow
                      description='Chromium Embedded Framework powers Ghostex web surfaces and stays on because the app requires it.'
                      icon={IconDeviceDesktop}
                      onReinstall={onReinstallPlugin ? () => onReinstallPlugin('cef') : undefined}
                      reinstallAvailable={Boolean(onReinstallPlugin && cef?.canReinstall)}
                      runtime={cef}
                      title='Chromium runtime (CEF)'
                    />
                  </OfficialExtensionGroup>
                ) : null}
              </SettingsSection>
            ) : null}

            {transport && shouldShowSettingsSection(search.sections.store) ? (
              <SettingsSection
                description={
                  <>
                    Extensions published to the{' '}
                    <a
                      className='inline-flex items-baseline gap-0.5 text-foreground/90 underline underline-offset-2 hover:text-foreground'
                      href={GHOSTEX_EXTENSIONS_REPO_URL}
                      onClick={(event) => {
                        if (!vscode) return;
                        event.preventDefault();
                        vscode.postMessage({ type: 'openExternalUrl', url: GHOSTEX_EXTENSIONS_REPO_URL });
                      }}
                      rel='noreferrer'
                      target='_blank'
                    >
                      ghostex-extensions
                      <IconExternalLink aria-hidden='true' className='self-center' size={12} />
                    </a>{' '}
                    repo. Reviewed and tested by @maddada.
                  </>
                }
                descriptionClassName='pb-2'
                title='Extensions Store'
              >
                <ExtensionsBrowserList state={browser} />
              </SettingsSection>
            ) : null}
          </>
        )}
      </div>
    </SettingsNativeScrollArea>
  );
}

function OfficialExtensionList({
  extensions,
  label,
  onReinstallPlugin,
  onUpdateSetting,
  settings,
  showOfficial,
  statusById,
}: {
  extensions: readonly GhostexOfficialExtension[];
  label: string;
  onReinstallPlugin?: (pluginId: SidebarPluginSettingsItem['id']) => void;
  onUpdateSetting: <K extends OfficialExtensionSettingKey>(key: K, value: ghostexSettings[K]) => void;
  settings: ghostexSettings;
  showOfficial: (key: string) => boolean;
  statusById: ReadonlyMap<SidebarPluginSettingsItem['id'], SidebarPluginSettingsItem>;
}) {
  const visible = extensions.filter((extension) => showOfficial(extension.id));
  if (!visible.length) {
    return null;
  }
  return (
    <OfficialExtensionGroup label={label}>
      {visible.map((extension) => {
        const runtimeId = OFFICIAL_EXTENSION_RUNTIME_IDS[extension.id];
        const runtime = runtimeId ? statusById.get(runtimeId) : undefined;
        return (
          <OfficialExtensionRow
            description={extension.description}
            enabled={isOfficialExtensionEnabled(settings, extension)}
            icon={OFFICIAL_EXTENSION_ICONS[extension.id]}
            key={extension.id}
            onEnabledChange={(enabled) => onUpdateSetting(extension.settingsKey, !enabled)}
            onReinstall={runtimeId && onReinstallPlugin ? () => onReinstallPlugin(runtimeId) : undefined}
            reinstallAvailable={Boolean(onReinstallPlugin && runtime?.canReinstall)}
            runtime={runtime}
            title={extension.title}
          />
        );
      })}
    </OfficialExtensionGroup>
  );
}

function OfficialExtensionGroup({ children, label }: { children: ReactNode; label: string }) {
  return (
    <section className='flex flex-col gap-2.5'>
      <h3 className='text-[13px] font-normal text-muted-foreground'>{label}</h3>
      <div className='extensions-group divide-y overflow-hidden'>{children}</div>
    </section>
  );
}

function OfficialExtensionRow({
  description,
  enabled,
  icon: Icon,
  onEnabledChange,
  onReinstall,
  reinstallAvailable,
  runtime,
  title,
}: {
  description: string;
  enabled?: boolean;
  icon: TablerIcon;
  onEnabledChange?: (enabled: boolean) => void;
  onReinstall?: () => void;
  reinstallAvailable?: boolean;
  runtime?: SidebarPluginSettingsItem;
  title: string;
}) {
  const busy = runtime !== undefined && !['installed', 'notInstalled', 'failed'].includes(runtime.status);
  const actionLabel = runtime?.status === 'notInstalled' ? 'Install' : 'Reinstall';
  const metadata = [
    runtime?.statusLabel,
    runtime?.version ? `v${runtime.version}` : undefined,
    runtime?.errorMessage,
  ].filter(Boolean);

  return (
    <div className='extensions-row group/row flex min-h-20 items-center gap-3 px-3 py-2.5 transition-colors'>
      <span
        aria-hidden='true'
        className={cn('size-1.5 shrink-0 rounded-full', enabled === false ? 'bg-white/20' : 'bg-emerald-400/80')}
      />
      <span
        aria-hidden='true'
        className='extensions-icon flex size-9 shrink-0 items-center justify-center p-1.5 text-[#b9b9b9]'
      >
        <Icon className='size-4' />
      </span>
      <div className='min-w-0 flex-1'>
        <span className='block truncate text-sm font-normal text-foreground'>{title}</span>
        <p className='mt-0.5 text-[13px] font-normal leading-relaxed text-foreground/75'>{description}</p>
      </div>
      {onReinstall ? (
        <div className='flex shrink-0 flex-col items-end gap-1'>
          <SettingButton
            className='shrink-0 font-normal'
            disabled={busy || !reinstallAvailable}
            disabledReason={
              busy ? `${title} is being installed.` : 'This build does not provide a reinstallable remote component.'
            }
            onClick={onReinstall}
            size='sm'
            type='button'
            variant='outline'
          >
            <IconRefresh aria-hidden='true' className={cn(busy && 'animate-spin')} data-icon='inline-start' />
            {actionLabel}
          </SettingButton>
          {metadata.length ? (
            <p className='max-w-56 truncate text-right text-xs font-normal text-muted-foreground'>
              {metadata.join(' · ')}
            </p>
          ) : null}
        </div>
      ) : null}
      {onEnabledChange && enabled !== undefined ? (
        <div className='ml-1 flex shrink-0 items-center gap-2'>
          <span className='text-xs font-normal text-muted-foreground'>{enabled ? 'On' : 'Off'}</span>
          <Switch
            aria-label={`${enabled ? 'Disable' : 'Enable'} ${title}`}
            checked={enabled}
            onCheckedChange={onEnabledChange}
            size='sm'
          />
        </div>
      ) : (
        <span className='ml-1 shrink-0 text-xs font-normal text-muted-foreground'>Always on</span>
      )}
    </div>
  );
}
