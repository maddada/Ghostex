import { CustomViewEditor, type CustomViewEditorState } from '../project-views/editor';
import { ProjectViewTemplates } from '../project-views/templates';
import {
  projectViewDescription,
  DEFAULT_PROJECT_VIEW_SOURCE,
  type ProjectViewTemplate,
} from '@/packages/shared/ghostex-settings/project-views';
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
  IconArrowsSort,
  IconCodeDots,
  IconDeviceDesktop,
  IconExternalLink,
  IconFileText,
  IconFolderOpen,
  IconGitCommit,
  IconGripVertical,
  IconHelpCircle,
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
import { TitlebarAccountUsageSection } from '../../accounts/titlebar-settings-section';
import { TitlebarViewOrderDialog } from './titlebar-view-order-dialog';
import { titlebarViewOrderItems } from '@/packages/shared/ghostex-settings/titlebar-view-order';
import { createSettingsCustomViewDragData, getSettingsCustomViewDragData, moveId } from '../drag-data';
import {
  setSettingsSortableRowElement,
  SettingButton,
  SettingsInput,
  SettingsListItem,
  SettingsNativeScrollArea,
  SettingsSection,
} from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';

export type OfficialExtensionSettingKey = GhostexOfficialExtensionSettingsKey;
type ExtensionPageSettingKey =
  OfficialExtensionSettingKey | 'customViews' | 'titlebarViewOrder' | 'customViewTemplates';

const GHOSTEX_EXTENSIONS_REPO_URL = 'https://github.com/maddada/ghostex-extensions';

const OFFICIAL_EXTENSION_ICONS: Record<GhostexOfficialExtensionId, TablerIcon> = {
  automate: IconBolt,
  browser: IconWorld,
  code: IconCodeDots,
  devServers: IconWorld,
  docs: IconFileText,
  extensionsButton: IconPuzzle,
  gitActions: IconGitCommit,
  help: IconHelpCircle,
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
  spaces = [],
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
  spaces?: import('@/packages/shared/ghostex-settings/project-views').ProjectViewSpace[];
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
  const [choosingTemplate, setChoosingTemplate] = useState(false);
  const [viewOrderOpen, setViewOrderOpen] = useState(false);
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
  const orderedViews = titlebarViewOrderItems(settings, browser.installed);
  const customViewsById = new Map(settings.customViews.map((view) => [`extension:${view.id}`, view]));
  const orderedCustomViews = orderedViews.flatMap((item) => {
    const view = customViewsById.get(item.id);
    return view ? [view] : [];
  });
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
      orderedCustomViews.map((view) => `extension:${view.id}`),
      source.initialIndex,
      targetIndex
    );
    let customIndex = 0;
    const ids = orderedViews.map((item) => (customViewsById.has(item.id) ? reorderedIds[customIndex++]! : item.id));
    const knownIds = new Set(ids);
    onUpdateSetting('titlebarViewOrder', [...ids, ...settings.titlebarViewOrder.filter((id) => !knownIds.has(id))]);
  }) satisfies DragDropEventHandlers['onDragEnd'];

  const saveCustomView = () => {
    if (!customViewEditor) return;
    const name = customViewEditor.draft.name.trim();
    const draft = customViewEditor.draft;
    const source = draft.source;
    const needsUrl = !source || (source.kind === 'website' && source.destination === 'fixed');
    const url = normalizeCustomViewUrl(draft.url) ?? '';
    if (!name || (needsUrl && !url)) {
      setCustomViewEditor({
        ...customViewEditor,
        error: !name ? 'Enter a name for the titlebar tab.' : 'Enter a complete HTTP or HTTPS URL.',
      });
      return;
    }
    if (draft.availability === 'spaces' && !draft.spaceRefs?.length) {
      setCustomViewEditor({ ...customViewEditor, error: 'Choose at least one space.' });
      return;
    }
    if (source?.kind === 'report' && !source.reportDirectory.trim()) {
      setCustomViewEditor({ ...customViewEditor, error: 'Enter the report directory.' });
      return;
    }
    const customView: GhostexCustomView = {
      ...draft,
      id: customViewEditor.id ?? `${CUSTOM_VIEW_ID_PREFIX}${crypto.randomUUID()}`,
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

  const saveViewTemplate = () => {
    if (!customViewEditor) return;
    if (!customViewEditor.draft.name.trim()) {
      setCustomViewEditor({ ...customViewEditor, error: 'Enter a name for the template.' });
      return;
    }
    const draft = customViewEditor.draft;
    const source = { ...(draft.source ?? DEFAULT_PROJECT_VIEW_SOURCE) };
    if (source.kind === 'website' && source.destination === 'fixed') source.destination = 'project';
    if (/^(?:[A-Za-z]:|[/\\])/.test(source.cwd)) source.cwd = '.';
    const template: ProjectViewTemplate = {
      id: `personal-${crypto.randomUUID()}`,
      name: draft.name.trim(),
      url: '',
      source,
      availability: 'matching',
    };
    onUpdateSetting('customViewTemplates', [...settings.customViewTemplates, template]);
    setCustomViewEditor({ ...customViewEditor, notice: 'Template saved. Choose Add view to use it.' });
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
            {shouldShowSettingsSection(search.sections.viewOrder) ? (
              <SettingsSection
                title='Titlebar views'
                description='Choose the order of built-in, extension, and custom views.'
              >
                <SettingsListItem title='View order'>
                  <Button onClick={() => setViewOrderOpen(true)} type='button' variant='outline'>
                    <IconArrowsSort aria-hidden='true' data-icon='inline-start' />
                    Arrange views
                  </Button>
                </SettingsListItem>
              </SettingsSection>
            ) : null}
            <TitlebarViewOrderDialog
              open={viewOrderOpen}
              onOpenChange={setViewOrderOpen}
              settings={settings}
              installed={browser.installed}
              onChange={(order) => onUpdateSetting('titlebarViewOrder', order)}
            />
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

            {shouldShowSettingsSection(search.sections.accountUsage) ? (
              <TitlebarAccountUsageSection active={isActive} hideEmails={settings.hideAccountEmails} />
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
                    onClick={() => setChoosingTemplate(true)}
                    type='button'
                    variant='ghost'
                  >
                    <IconPlus aria-hidden='true' data-icon='inline-start' />
                    Add view
                  </SettingButton>
                }
                description='Websites, project dev servers, and HTML reports. Add from a template or configure your own.'
                descriptionClassName='pb-2'
                title='Your views'
              >
                {choosingTemplate ? (
                  <ProjectViewTemplates
                    templates={settings.customViewTemplates}
                    onCancel={() => setChoosingTemplate(false)}
                    onRemove={(id) =>
                      onUpdateSetting(
                        'customViewTemplates',
                        settings.customViewTemplates.filter((t) => t.id !== id)
                      )
                    }
                    onSelect={(template) => {
                      setChoosingTemplate(false);
                      setCustomViewEditor({
                        draft: { ...template, id: '', enabled: true, projectBindings: {}, templateId: template.id },
                      });
                    }}
                  />
                ) : null}
                <DragDropProvider onDragEnd={handleCustomViewDragEnd}>
                  <div className='settings-list-rows'>
                    {orderedCustomViews.map((view, index) =>
                      customViewEditor?.id === view.id ? (
                        <CustomViewEditor
                          spaces={spaces}
                          editor={customViewEditor}
                          key={view.id}
                          onCancel={() => setCustomViewEditor(undefined)}
                          onChange={setCustomViewEditor}
                          onSave={saveCustomView}
                          onSaveTemplate={saveViewTemplate}
                        />
                      ) : (
                        <CustomViewRow
                          index={index}
                          key={view.id}
                          onEdit={() => setCustomViewEditor({ draft: { ...view }, id: view.id })}
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
                        spaces={spaces}
                        editor={customViewEditor}
                        onCancel={() => setCustomViewEditor(undefined)}
                        onChange={setCustomViewEditor}
                        onSave={saveCustomView}
                        onSaveTemplate={saveViewTemplate}
                      />
                    ) : settings.customViews.length === 0 ? (
                      <div className='py-5 text-center text-[13px] font-normal text-muted-foreground'>
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
      className='extensions-row group/row flex min-h-14 items-center gap-3 py-2 transition-colors'
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
        <p className='mt-0.5 truncate text-[13px] font-normal leading-relaxed text-foreground/75'>
          {projectViewDescription(view)}
        </p>
      </div>
      <div className='flex shrink-0 items-center gap-1'>
        <Button aria-label={`Edit ${view.name}`} onClick={onEdit} size='icon-sm' type='button' variant='ghost'>
          <IconPencil aria-hidden='true' className='size-4' />
        </Button>
        <Button aria-label={`Remove ${view.name}`} onClick={onRemove} size='icon-sm' type='button' variant='ghost'>
          <IconTrash aria-hidden='true' className='size-4' />
        </Button>
        <div className='ml-1 flex shrink-0 items-center gap-2'>
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
    <>
      <div className='settings-list-group-label'>{label}</div>
      <div className='settings-list-rows'>{children}</div>
    </>
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
    <div className='extensions-row group/row flex min-h-14 items-center gap-3 py-2 transition-colors'>
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
            <p className='max-w-56 truncate text-right text-[13px] font-normal text-muted-foreground'>
              {metadata.join(' · ')}
            </p>
          ) : null}
        </div>
      ) : null}
      {onEnabledChange && enabled !== undefined ? (
        <div className='ml-1 flex shrink-0 items-center gap-2'>
          <Switch
            aria-label={`${enabled ? 'Disable' : 'Enable'} ${title}`}
            checked={enabled}
            onCheckedChange={onEnabledChange}
            size='sm'
          />
        </div>
      ) : (
        /* CDXC:Settings 2026-09-09 DECISION: User: no On or Off text beside toggles anywhere in Settings. A view that cannot be turned off shows a locked-on switch instead of an "Always on" caption. */
        <div className='ml-1 flex shrink-0 items-center gap-2'>
          <Switch aria-label={`${title} is always on`} checked disabled size='sm' />
        </div>
      )}
    </div>
  );
}
