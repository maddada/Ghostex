import { CustomViewEditor, type CustomViewEditorState } from '../project-views/editor';
import { ViewScopeEditor, type ViewScopeEditorState } from '../project-views/scope-editor';
import { ProjectViewTemplates } from '../project-views/templates';
import {
  DEFAULT_PROJECT_VIEW_SOURCE,
  type ProjectViewTemplate,
} from '@/packages/shared/ghostex-settings/project-views';
/*
 * CDXC:Extensions 2026-08-30:
 * Settings has one Extensions page. The "Built-in" section is the features Ghostex ships itself, backed by the
 * inverted `*Hidden` settings keys in `GHOSTEX_OFFICIAL_EXTENSIONS`; below it the same page embeds the real
 * extension store and installed list, followed by user-defined URL views. All three read as one family of
 * cards (a three-column grid since 2026-09-24), under one filter bar.
 *
 * Opening an extension's details replaces the whole page (not just the store
 * section), and the list scroll position is restored on the way back.
 *
 * This replaced the old "Customize" page (tab id `plugins`) and the standalone
 * Extensions app modal.
 */
import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation } from '@dnd-kit/react/sortable';
import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type UIEvent } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { IconArrowsSort, IconExternalLink, IconRefresh } from '@tabler/icons-react';
import {
  GHOSTEX_OFFICIAL_EXTENSIONS,
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
import {
  extensionViewScopeKey,
  ghostexViewScope,
  isDefaultGhostexViewScope,
  officialViewScopeKey,
  setGhostexViewScope,
  viewScopeDescription,
} from '../../../shared/ghostex-settings/view-scopes';
import { type WebviewApi } from '../../webview-api';
import {
  DEFAULT_EXTENSION_FILTER,
  ExtensionCardGrid,
  ExtensionCardGridWide,
  ExtensionEmptyStateFilter,
  ExtensionsBrowserDetail,
  ExtensionsBrowserList,
  ExtensionsFilterBar,
  extensionFilterMatches,
  filterStoreExtensions,
  isExtensionFilterActive,
  storeCategories,
  useExtensionsBrowserState,
  type ExtensionFilter,
} from '../../extensions-modal';
import { createExtensionsModalTransport } from '../../extensions-modal/transport';
import { TitlebarAccountUsageSection } from '../../accounts/titlebar-settings-section';
import { TitlebarViewOrderDialog } from './titlebar-view-order-dialog';
import { titlebarViewOrderItems } from '@/packages/shared/ghostex-settings/titlebar-view-order';
import { getSettingsCustomViewDragData, moveId } from '../drag-data';
import { SettingButton, SettingsListItem, SettingsNativeScrollArea, SettingsSection } from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';
import {
  BUILT_IN_CATEGORY_LABELS,
  BuiltInExtensionGroups,
  builtInCounts,
  type ViewScopeControls,
} from './extensions/built-in-cards';
import { AddCustomViewCard, CustomViewCard, customViewFilterSubject } from './extensions/custom-view-cards';

export type OfficialExtensionSettingKey = GhostexOfficialExtensionSettingsKey;
type ExtensionPageSettingKey =
  OfficialExtensionSettingKey | 'customViews' | 'titlebarViewOrder' | 'customViewTemplates' | 'viewScopes';

const GHOSTEX_EXTENSIONS_REPO_URL = 'https://github.com/maddada/ghostex-extensions';

export function ExtensionsSettingsTab({
  initialCustomViewId,
  initialViewScopeKey,
  projects = [],
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
  initialCustomViewId?: string;
  initialViewScopeKey?: string;
  projects?: import('@/packages/shared/ghostex-settings/project-views').ProjectViewProject[];
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
  const [scopeEditor, setScopeEditor] = useState<ViewScopeEditorState>();
  const [choosingTemplate, setChoosingTemplate] = useState(false);
  const [viewOrderOpen, setViewOrderOpen] = useState(false);
  const customViewEditorRef = useRef<HTMLDivElement>(null);
  const targetedCustomViewId = useRef<string | undefined>(undefined);
  const targetedViewScopeKey = useRef<string | undefined>(undefined);
  const focusCustomViewEditor = useRef(false);

  /**
   * CDXC:Extensions 2026-09-16 WHY:
   * Configure view used to open the general Extensions page without identifying the clicked view. Carry its ID through the modal host and open its editor once, preserving edits during settings updates.
   * SEE-ALSO: apps/desktop/src/app/project_views.rs, apps/desktop/views/modal-host.tsx, packages/core-ui/settings-modal.tsx.
   */
  useEffect(() => {
    if (!isActive || !initialCustomViewId) {
      targetedCustomViewId.current = undefined;
      return;
    }
    if (targetedCustomViewId.current === initialCustomViewId) return;
    const view = settings.customViews.find((candidate) => candidate.id === initialCustomViewId);
    if (!view) return;
    targetedCustomViewId.current = initialCustomViewId;
    focusCustomViewEditor.current = true;
    setChoosingTemplate(false);
    setCustomViewEditor({ draft: { ...view }, id: view.id });
  }, [initialCustomViewId, isActive, settings.customViews]);
  const statusById = new Map(status?.plugins.map((plugin) => [plugin.id, plugin]));
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
  /**
   * CDXC:Extensions 2026-09-20 WHY:
   * "Choose where it's shown…" on a view tab names the view it was opened from, so this page opens
   * that view's scope editor instead of dropping the user on the list to find the row again. The key
   * is the same `official:` / `extension:` scope key the rows themselves use.
   * SEE-ALSO: apps/desktop/src/app/view_tab_menus.rs, apps/desktop/views/modal-host.tsx.
   */
  const viewScopeEditorTitle = (key: string): string | undefined => {
    const official = GHOSTEX_OFFICIAL_EXTENSIONS.find((extension) => officialViewScopeKey(extension.id) === key);
    if (official) return official.title;
    const customView = settings.customViews.find((view) => extensionViewScopeKey(view.id) === key);
    if (customView) return customView.name;
    return browser.installed.find((extension) => extensionViewScopeKey(extension.id) === key)?.manifest.title;
  };
  useEffect(() => {
    if (!isActive || !initialViewScopeKey) {
      targetedViewScopeKey.current = undefined;
      return;
    }
    if (targetedViewScopeKey.current === initialViewScopeKey) return;
    const title = viewScopeEditorTitle(initialViewScopeKey);
    if (!title) return;
    targetedViewScopeKey.current = initialViewScopeKey;
    setScopeEditor({
      draft: ghostexViewScope(settings.viewScopes, initialViewScopeKey),
      key: initialViewScopeKey,
      title,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [browser.installed, initialViewScopeKey, isActive, settings.customViews, settings.viewScopes]);
  const detailOpen = Boolean(transport) && browser.detailOpen;
  const showOfficial = (key: string) => shouldShowSetting(search.sections.official, key);

  const updateCustomViews = (customViews: GhostexCustomView[]) => {
    onUpdateSetting('customViews', normalizeGhostexCustomViews(customViews));
  };

  const scopeControls: ViewScopeControls = {
    describe: (key) => {
      const scope = ghostexViewScope(settings.viewScopes, key);
      return isDefaultGhostexViewScope(scope) ? undefined : viewScopeDescription(scope, { projects, spaces });
    },
    edit: (key, title) => setScopeEditor({ draft: ghostexViewScope(settings.viewScopes, key), key, title }),
    editingKey: scopeEditor?.key,
    renderEditor: (key) =>
      scopeEditor?.key === key ? (
        <ViewScopeEditor
          editor={scopeEditor}
          onCancel={() => setScopeEditor(undefined)}
          onChange={(apply) => setScopeEditor((current) => (current ? apply(current) : current))}
          onSave={() => {
            /*
             * CDXC:Extensions 2026-09-20 WHY:
             * A Default of "Hidden unless chosen" with nothing chosen is saved as it stands, and hides the
             * view everywhere. The 2026-09-18 editor refused that save because its allow-list could only
             * ever mean "show it in these", so an empty list read as a mistake; under the override model it
             * is the user asking for the view to be gone, and the view picker is where it comes back.
             */
            onUpdateSetting('viewScopes', setGhostexViewScope(settings.viewScopes, scopeEditor.key, scopeEditor.draft));
            setScopeEditor(undefined);
          }}
          projects={projects}
          spaces={spaces}
        />
      ) : null,
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
        error: !name ? 'Enter a name for the view tab.' : 'Enter a complete HTTP or HTTPS URL.',
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

  useLayoutEffect(() => {
    if (!isActive || !focusCustomViewEditor.current || !customViewEditorRef.current) return;
    const frame = requestAnimationFrame(() => {
      const editor = customViewEditorRef.current;
      if (!editor) return;
      focusCustomViewEditor.current = false;
      editor.scrollIntoView({ block: 'start' });
      editor.querySelector<HTMLInputElement>('input')?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, [customViewEditor?.id, detailOpen, isActive, search.tab.isSearching]);

  /*
   * CDXC:Extensions 2026-09-24 DECISION:
   * User: one filter bar (search, source, type, category, "N shown") covers every extension on the page. The
   * Settings-wide search still narrows the page first; this filter narrows what is left.
   */
  const [filter, setFilter] = useState<ExtensionFilter>(DEFAULT_EXTENSION_FILTER);
  const filterActive = isExtensionFilterActive(filter);
  const showBuiltIn = shouldShowSettingsSection(search.sections.official);
  const showStore = Boolean(transport) && shouldShowSettingsSection(search.sections.store);
  const showCustomViews = shouldShowSettingsSection(search.sections.customViews);
  const builtIn = showBuiltIn ? builtInCounts(filter, showOfficial) : { shown: 0, total: 0 };
  const storeShown = showStore ? filterStoreExtensions(filter, browser.catalog, browser.installed) : undefined;
  const storeAll = showStore
    ? filterStoreExtensions(DEFAULT_EXTENSION_FILTER, browser.catalog, browser.installed)
    : undefined;
  const visibleCustomViews = showCustomViews
    ? orderedCustomViews.filter((view) => extensionFilterMatches(filter, customViewFilterSubject(view)))
    : [];
  const shownCount =
    builtIn.shown +
    (storeShown ? storeShown.installed.length + storeShown.store.length : 0) +
    visibleCustomViews.length;
  const totalCount =
    builtIn.total +
    (storeAll ? storeAll.installed.length + storeAll.store.length : 0) +
    (showCustomViews ? orderedCustomViews.length : 0);
  const filterCategories = useMemo(
    () =>
      [
        ...BUILT_IN_CATEGORY_LABELS,
        ...storeCategories(browser.catalog, browser.installed).sort((a, b) => a.localeCompare(b)),
      ].filter((category, index, all) => all.indexOf(category) === index),
    [browser.catalog, browser.installed]
  );
  // While a filter is active a section with no match disappears; without one every section stays so its empty state and actions show.
  const builtInVisible = showBuiltIn && (!filterActive || builtIn.shown > 0);
  const storeVisible =
    showStore && (!filterActive || (storeShown ? storeShown.installed.length + storeShown.store.length : 0) > 0);
  const customViewsVisible = showCustomViews && (!filterActive || visibleCustomViews.length > 0);
  const anyExtensionSection = showBuiltIn || showStore || showCustomViews;

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
              <SettingsSection title='Views' description='Choose the order of built-in, extension, and custom views.'>
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

            {anyExtensionSection ? (
              <ExtensionsFilterBar
                categories={filterCategories}
                filter={filter}
                loading={browser.loading}
                onChange={setFilter}
                onRefresh={transport ? () => void browser.load() : undefined}
                shownCount={shownCount}
                sources={[
                  ...(showBuiltIn ? (['built-in'] as const) : []),
                  ...(showStore ? (['installed', 'store'] as const) : []),
                  ...(showCustomViews ? (['custom'] as const) : []),
                ]}
                totalCount={totalCount}
              />
            ) : null}
            {anyExtensionSection && filterActive && shownCount === 0 ? (
              <ExtensionEmptyStateFilter onClear={() => setFilter(DEFAULT_EXTENSION_FILTER)} />
            ) : null}

            {builtInVisible ? (
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
                plain
                title='Built-in'
              >
                <BuiltInExtensionGroups
                  filter={filter}
                  onReinstallPlugin={onReinstallPlugin}
                  onToggle={(key, hidden) => onUpdateSetting(key, hidden)}
                  scopeControls={scopeControls}
                  settings={settings}
                  showOfficial={showOfficial}
                  statusById={statusById}
                />
              </SettingsSection>
            ) : null}

            {storeVisible ? (
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
                plain
                title='Extensions Store'
              >
                <ExtensionsBrowserList
                  editingScopeId={
                    scopeEditor?.key.startsWith('extension:') ? scopeEditor.key.slice('extension:'.length) : undefined
                  }
                  filter={filter}
                  onEditScope={(extension) =>
                    scopeControls.edit(extensionViewScopeKey(extension.id), extension.manifest.title)
                  }
                  renderScopeEditor={(extension) => scopeControls.renderEditor(extensionViewScopeKey(extension.id))}
                  scopeSummaryFor={(extension) => scopeControls.describe(extensionViewScopeKey(extension.id))}
                  state={browser}
                />
              </SettingsSection>
            ) : null}

            {customViewsVisible ? (
              <SettingsSection
                description='Websites, project dev servers, and HTML reports. Add from a template or configure your own.'
                descriptionClassName='pb-2'
                plain
                title='Your views'
              >
                {choosingTemplate ? (
                  <div className='pb-3'>
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
                  </div>
                ) : null}
                <DragDropProvider onDragEnd={handleCustomViewDragEnd}>
                  <ExtensionCardGrid>
                    {visibleCustomViews.map((view, index) => (
                      <Fragment key={view.id}>
                        <CustomViewCard
                          editing={customViewEditor?.id === view.id}
                          index={index}
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
                          sortable={!filterActive}
                          view={view}
                        />
                        {customViewEditor?.id === view.id ? (
                          <ExtensionCardGridWide>
                            <div className='settings-list-card'>
                              <CustomViewEditor
                                editorRef={customViewEditorRef}
                                spaces={spaces}
                                editor={customViewEditor}
                                onCancel={() => setCustomViewEditor(undefined)}
                                onChange={setCustomViewEditor}
                                onSave={saveCustomView}
                                onSaveTemplate={saveViewTemplate}
                              />
                            </div>
                          </ExtensionCardGridWide>
                        ) : null}
                      </Fragment>
                    ))}
                    {customViewEditor && !customViewEditor.id ? (
                      <ExtensionCardGridWide>
                        <div className='settings-list-card'>
                          <CustomViewEditor
                            editorRef={customViewEditorRef}
                            spaces={spaces}
                            editor={customViewEditor}
                            onCancel={() => setCustomViewEditor(undefined)}
                            onChange={setCustomViewEditor}
                            onSave={saveCustomView}
                            onSaveTemplate={saveViewTemplate}
                          />
                        </div>
                      </ExtensionCardGridWide>
                    ) : filterActive || choosingTemplate ? null : (
                      <AddCustomViewCard onClick={() => setChoosingTemplate(true)} />
                    )}
                  </ExtensionCardGrid>
                </DragDropProvider>
              </SettingsSection>
            ) : null}

            {shouldShowSettingsSection(search.sections.accountUsage) ? (
              <TitlebarAccountUsageSection active={isActive} hideEmails={settings.hideAccountEmails} />
            ) : null}
          </>
        )}
      </div>
    </SettingsNativeScrollArea>
  );
}
