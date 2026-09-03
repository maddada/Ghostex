/*
 * CDXC:Extensions 2026-08-30:
 * Settings has one Extensions page. The "Official Extensions" section is the
 * features Ghostex ships itself, backed by the inverted `*Hidden` settings keys
 * in `GHOSTEX_OFFICIAL_EXTENSIONS`; below it the same page embeds the real
 * extension store and installed list, followed by user-defined URL views. All
 * three read as one family of cards, which is why the official and custom rows
 * reuse the `.extensions-*` panel skin instead of the stacked settings-field
 * layout the other Settings pages use.
 *
 * Opening an extension's details replaces the whole page (not just the store
 * section), and the list scroll position is restored on the way back.
 *
 * This replaced the old "Customize" page (tab id `plugins`) and the standalone
 * Extensions app modal.
 */
import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import { useLayoutEffect, useMemo, useRef, useState, type ReactNode, type UIEvent } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import {
  IconBolt,
  IconCodeDots,
  IconDeviceDesktop,
  IconExternalLink,
  IconFileText,
  IconFolderOpen,
  IconGitCommit,
  IconGripVertical,
  IconInfoCircle,
  IconPencil,
  IconPlayerPlay,
  IconPlus,
  IconPuzzle,
  IconRefresh,
  IconTrash,
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
import {
  CUSTOM_VIEW_ID_PREFIX,
  normalizeCustomViewUrl,
  normalizeGhostexCustomViews,
  type GhostexCustomView,
  type ghostexSettings,
} from '../../../shared/ghostex-settings';
import { type WebviewApi } from '../../webview-api';
import { ExtensionsBrowserDetail, ExtensionsBrowserList, useExtensionsBrowserState } from '../../extensions-modal';
import { createExtensionsModalTransport } from '../../extensions-modal/transport';
import { createSettingsCustomViewDragData, getSettingsCustomViewDragData, moveId } from '../drag-data';
import {
  SettingButton,
  SettingsInput,
  SettingsNativeScrollArea,
  SettingsSection,
  setSettingsSortableRowElement,
} from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';

export type OfficialExtensionSettingKey = GhostexOfficialExtensionSettingsKey;
type ExtensionPageSettingKey = OfficialExtensionSettingKey | 'customViews';

type CustomViewEditorState = {
  draft: Pick<GhostexCustomView, 'name' | 'url'>;
  error?: string;
  id?: string;
};

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
  onUpdateSetting: <K extends ExtensionPageSettingKey>(key: K, value: ghostexSettings[K]) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  settings: ghostexSettings;
  status?: SidebarPluginSettingsStatusMessage;
  statusLoading: boolean;
  vscode?: WebviewApi;
}) {
  const [customViewEditor, setCustomViewEditor] = useState<CustomViewEditorState>();
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

  const updateCustomViews = (customViews: GhostexCustomView[]) => {
    onUpdateSetting('customViews', normalizeGhostexCustomViews(customViews));
  };

  const handleCustomViewDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getSettingsCustomViewDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex = 'index' in source && typeof source.index === 'number' ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    const reorderedIds = moveId(
      settings.customViews.map((view) => view.id),
      source.initialIndex,
      targetIndex
    );
    const viewById = new Map(settings.customViews.map((view) => [view.id, view]));
    updateCustomViews(reorderedIds.flatMap((id) => (viewById.get(id) ? [viewById.get(id)!] : [])));
  }) satisfies DragDropEventHandlers['onDragEnd'];

  const saveCustomView = () => {
    if (!customViewEditor) return;
    const name = customViewEditor.draft.name.trim();
    const url = normalizeCustomViewUrl(customViewEditor.draft.url);
    if (!name) {
      setCustomViewEditor({ ...customViewEditor, error: 'Enter a name for the titlebar tab.' });
      return;
    }
    if (!url) {
      setCustomViewEditor({ ...customViewEditor, error: 'Enter a complete HTTP or HTTPS URL.' });
      return;
    }
    const customView: GhostexCustomView = {
      enabled: customViewEditor.id
        ? (settings.customViews.find((view) => view.id === customViewEditor.id)?.enabled ?? true)
        : true,
      id: customViewEditor.id ?? `${CUSTOM_VIEW_ID_PREFIX}${Date.now().toString(36)}`,
      name,
      url,
    };
    updateCustomViews(
      customViewEditor.id
        ? settings.customViews.map((view) => (view.id === customViewEditor.id ? customView : view))
        : [...settings.customViews, customView]
    );
    setCustomViewEditor(undefined);
  };

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

            {shouldShowSettingsSection(search.sections.customViews) ? (
              <SettingsSection
                actions={
                  <SettingButton
                    disabled={Boolean(customViewEditor)}
                    disabledReason='Finish editing the current custom view first.'
                    onClick={() => setCustomViewEditor({ draft: { name: '', url: '' } })}
                    type='button'
                    variant='ghost'
                  >
                    <IconPlus aria-hidden='true' data-icon='inline-start' />
                    Add view
                  </SettingButton>
                }
                description='Add, arrange, and toggle titlebar views that open HTTP or HTTPS pages inside Ghostex.'
                descriptionClassName='pb-2'
                title='Custom Views'
              >
                <DragDropProvider onDragEnd={handleCustomViewDragEnd}>
                  <div className='extensions-group divide-y overflow-hidden'>
                    {settings.customViews.map((view, index) =>
                      customViewEditor?.id === view.id ? (
                        <CustomViewEditor
                          editor={customViewEditor}
                          key={view.id}
                          onCancel={() => setCustomViewEditor(undefined)}
                          onChange={setCustomViewEditor}
                          onSave={saveCustomView}
                        />
                      ) : (
                        <CustomViewRow
                          index={index}
                          key={view.id}
                          onEdit={() => setCustomViewEditor({ draft: { name: view.name, url: view.url }, id: view.id })}
                          onEnabledChange={(enabled) =>
                            updateCustomViews(
                              settings.customViews.map((candidate) =>
                                candidate.id === view.id ? { ...candidate, enabled } : candidate
                              )
                            )
                          }
                          onRemove={() =>
                            updateCustomViews(settings.customViews.filter((candidate) => candidate.id !== view.id))
                          }
                          view={view}
                        />
                      )
                    )}
                    {customViewEditor && !customViewEditor.id ? (
                      <CustomViewEditor
                        editor={customViewEditor}
                        onCancel={() => setCustomViewEditor(undefined)}
                        onChange={setCustomViewEditor}
                        onSave={saveCustomView}
                      />
                    ) : settings.customViews.length === 0 ? (
                      <div className='px-4 py-5 text-center text-[13px] font-normal text-muted-foreground'>
                        No custom views yet.
                      </div>
                    ) : null}
                  </div>
                </DragDropProvider>
              </SettingsSection>
            ) : null}
          </>
        )}
      </div>
    </SettingsNativeScrollArea>
  );
}

function CustomViewRow({
  index,
  onEdit,
  onEnabledChange,
  onRemove,
  view,
}: {
  index: number;
  onEdit: () => void;
  onEnabledChange: (enabled: boolean) => void;
  onRemove: () => void;
  view: GhostexCustomView;
}) {
  const sortable = useSortable({
    accept: 'settings-custom-view',
    data: createSettingsCustomViewDragData(view.id),
    group: 'settings-custom-views',
    id: view.id,
    index,
    type: 'settings-custom-view',
  });
  const { handleRef, isDragging } = sortable;

  const setRowRef = (element: HTMLDivElement | null) => {
    setSettingsSortableRowElement(sortable, element);
  };

  return (
    <div
      className='extensions-row group/row flex min-h-20 items-center gap-3 px-3 py-2.5 transition-colors'
      data-dragging={String(Boolean(isDragging))}
      ref={setRowRef}
    >
      <Button aria-label={`Reorder ${view.name}`} ref={handleRef} size='icon-sm' type='button' variant='ghost'>
        <IconGripVertical aria-hidden='true' />
      </Button>
      <span
        aria-hidden='true'
        className={cn('size-1.5 shrink-0 rounded-full', view.enabled ? 'bg-emerald-400/80' : 'bg-white/20')}
      />
      <span
        aria-hidden='true'
        className='extensions-icon flex size-9 shrink-0 items-center justify-center p-1.5 text-[#b9b9b9]'
      >
        <IconWorld className='size-4' />
      </span>
      <div className='min-w-0 flex-1'>
        <span className='block truncate text-sm font-normal text-foreground'>{view.name}</span>
        <p className='mt-0.5 truncate text-[13px] font-normal leading-relaxed text-foreground/75'>{view.url}</p>
      </div>
      <div className='flex shrink-0 items-center gap-1'>
        <Button aria-label={`Edit ${view.name}`} onClick={onEdit} size='icon-sm' type='button' variant='ghost'>
          <IconPencil aria-hidden='true' className='size-4' />
        </Button>
        <Button aria-label={`Remove ${view.name}`} onClick={onRemove} size='icon-sm' type='button' variant='ghost'>
          <IconTrash aria-hidden='true' className='size-4' />
        </Button>
        <div className='ml-1 flex shrink-0 items-center gap-2'>
          <span className='text-xs font-normal text-muted-foreground'>{view.enabled ? 'On' : 'Off'}</span>
          <Switch
            aria-label={`${view.enabled ? 'Disable' : 'Enable'} ${view.name}`}
            checked={view.enabled}
            onCheckedChange={onEnabledChange}
            size='sm'
          />
        </div>
      </div>
    </div>
  );
}

function CustomViewEditor({
  editor,
  onCancel,
  onChange,
  onSave,
}: {
  editor: CustomViewEditorState;
  onCancel: () => void;
  onChange: (editor: CustomViewEditorState) => void;
  onSave: () => void;
}) {
  return (
    <div className='flex flex-col gap-3 px-3 py-3'>
      <SettingsInput
        aria-label='Custom view name'
        autoFocus
        onChange={(event) =>
          onChange({ ...editor, draft: { ...editor.draft, name: event.currentTarget.value }, error: undefined })
        }
        placeholder='Titlebar name'
        value={editor.draft.name}
      />
      <SettingsInput
        aria-invalid={Boolean(editor.error)}
        aria-label='Custom view URL'
        onChange={(event) =>
          onChange({ ...editor, draft: { ...editor.draft, url: event.currentTarget.value }, error: undefined })
        }
        placeholder='https://example.com'
        type='url'
        value={editor.draft.url}
      />
      {editor.error ? <p className='text-xs font-normal text-destructive'>{editor.error}</p> : null}
      <div className='flex justify-end gap-2'>
        <Button onClick={onCancel} type='button' variant='ghost'>
          Cancel
        </Button>
        <Button onClick={onSave} type='button'>
          {editor.id ? 'Save changes' : 'Add view'}
        </Button>
      </div>
    </div>
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
  onUpdateSetting: <K extends ExtensionPageSettingKey>(key: K, value: ghostexSettings[K]) => void;
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
