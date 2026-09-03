import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const settingsModalSource = readFileSync(new URL('./settings-modal.tsx', import.meta.url), 'utf8');
const agentsHubModalSource = readFileSync(new URL('./agents-hub-modal.tsx', import.meta.url), 'utf8');
const skillsPanelSource = readFileSync(new URL('./bundled-agent-skills-panel.tsx', import.meta.url), 'utf8');
const settingsModalStylesSource = readFileSync(new URL('./styles/modals.css', import.meta.url), 'utf8');
const sidebarStylesSource = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
const sharedSidebarContractSource = readFileSync(
  new URL('../shared/session-grid-contract-sidebar.ts', import.meta.url),
  'utf8'
);
// CDXC:RepoStructure 2026-08-22: settings-modal.tsx was split into
// packages/core-ui/settings-modal/*; these mirror the pieces raw-source
// assertions below now need to read from.
const settingsModalFieldsSource = readFileSync(new URL('./settings-modal/fields.tsx', import.meta.url), 'utf8');
const settingsModalTypesSource = readFileSync(new URL('./settings-modal/types.ts', import.meta.url), 'utf8');
const settingsModalAgentsTabSource = readFileSync(new URL('./settings-modal/tabs/agents.tsx', import.meta.url), 'utf8');
const settingsModalIntegrationsTabSource = readFileSync(
  new URL('./settings-modal/tabs/integrations.tsx', import.meta.url),
  'utf8'
);
const settingsModalProjectsTabSource = readFileSync(
  new URL('./settings-modal/tabs/projects.tsx', import.meta.url),
  'utf8'
);
const settingsModalPortlessTabSource = readFileSync(
  new URL('./settings-modal/tabs/portless.tsx', import.meta.url),
  'utf8'
);
// CDXC:RepoStructure 2026-08-23: the inline SettingsModal hook/logic block
// was decomposed into packages/core-ui/settings-modal/*; these mirror the pieces
// the raw-source assertions below now need to read from.
const settingsModalSearchCatalogSource = readFileSync(
  new URL('./settings-modal/search-catalog.ts', import.meta.url),
  'utf8'
);
const settingsModalVisibilitySource = readFileSync(
  new URL('./settings-modal/main-settings-visibility.ts', import.meta.url),
  'utf8'
);
const settingsModalSidebarPagesSource = readFileSync(
  new URL('./settings-modal/sidebar-pages.ts', import.meta.url),
  'utf8'
);
const settingsModalEffectsSource = readFileSync(
  new URL('./settings-modal/use-settings-modal-effects.ts', import.meta.url),
  'utf8'
);
const settingsModalPersistenceSource = readFileSync(
  new URL('./settings-modal/settings-persistence.ts', import.meta.url),
  'utf8'
);
const settingsModalAppIconSource = readFileSync(
  new URL('./settings-modal/use-app-icon-settings.ts', import.meta.url),
  'utf8'
);
const settingsModalActionsSource = readFileSync(
  new URL('./settings-modal/settings-actions.ts', import.meta.url),
  'utf8'
);

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

// CDXC:RepoStructure 2026-08-22: some markers used to bound a slice up to
// the NEXT top-level declaration in the old monolithic settings-modal.tsx.
// After the split that next declaration can live in a different file, so
// there is no longer an `end` marker to search for in the same source; take
// the rest of the (now-standalone) file instead.
function sourceFrom(source: string, start: string): string {
  const startIndex = source.indexOf(start);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  return source.slice(startIndex);
}

describe('settings modal source', () => {
  test('keeps Agents Hub tab rail visibly bordered while Settings uses sidebar navigation', () => {
    /*
     * CDXC:AppModal 2026-06-24-04:25:
     * Agents Hub top-level tabs use the app-modal tab style, and that style
     * must keep a visible 1px #252525 outside border plus single-pixel internal
     * dividers.
     *
     * CDXC:Settings 2026-06-24-22:16:
     * Settings must not render the app-modal top tab rail anymore; Settings
     * page navigation belongs in the left sidebar.
     */
    const tabRailStyles = sourceBetween(
      sidebarStylesSource,
      ".app-modal-tab-rail[data-slot='tabs-list'] {",
      ".app-modal-tab-rail [data-slot='tabs-trigger']:hover"
    );
    expect(tabRailStyles).toContain('border: 1px solid #252525 !important;');
    expect(tabRailStyles).toContain('border: 0 !important;');
    expect(tabRailStyles).toContain(".app-modal-tab-rail [data-slot='tabs-trigger'] + [data-slot='tabs-trigger']");
    expect(tabRailStyles).toContain('border-left: 1px solid #252525 !important;');
    expect(settingsModalSource).not.toContain("<TabsList className='app-modal-tab-rail'>");
    expect(settingsModalSource).not.toContain('settings-modal-tabs-scroll');
    expect(agentsHubModalSource).toContain("<TabsList className='agents-hub-tabs-list app-modal-tab-rail'>");
  });

  test('keeps Settings page navigation and Show Advanced inside the sidebar', () => {
    /*
     * CDXC:Settings 2026-06-19-08:40:
     * The macOS Settings section list and Show Advanced filter should render
     * as one sidebar surface, not as separate floating controls.
     *
     * CDXC:Settings 2026-06-24-22:16:
     * Top-level Settings pages and expandable page sections now share that
     * sidebar, replacing the old top tab bar.
     *
     * CDXC:Settings 2026-06-25-17:12:
     * Only top-level Settings categories should get Tabler icons; nested
     * expandable section rows remain text-only.
     *
     * CDXC:Settings 2026-06-25-17:58:
     * Expanded subsection titles should indent 14px farther than the base
     * sidebar button text while keeping the full-row highlight intact.
     *
     * CDXC:Settings 2026-06-25-18:05:
     * Active subsection rows should not have a filled background; use dim
     * inactive text and brighter active text to show section selection.
     *
     * CDXC:Settings 2026-06-25-22:10:
     * The macOS Settings sidebar container should sit 1px higher than the
     * default grid alignment so it lines up with the native window chrome.
     *
     * CDXC:Settings 2026-06-28-18:14:
     * Show Advanced is persisted in the settings draft. Do not keep a duplicate
     * local state copy that can initialize before native settings hydrate and
     * reset the switch on reopen.
     */
    const settingsSidebar = sourceBetween(
      settingsModalSource,
      "<aside aria-label='Settings pages and sections' className='settings-section-sidebar'>",
      '</aside>'
    );
    expect(settingsSidebar).toContain('settings-sidebar-tabs-list');
    expect(settingsSidebar).toContain('settings-sidebar-page-disclosure');
    expect(settingsSidebar).toContain('settings-sidebar-subsection-list');
    expect(settingsSidebar).toContain('settings-section-sidebar-footer');
    expect(settingsSidebar).toContain('Show Advanced');
    expect(settingsModalSource).toContain('const showAdvancedSettings = draft.showAdvancedSettings;');
    expect(settingsModalSource).not.toContain('const [showAdvancedSettings, setShowAdvancedSettings]');
    expect(settingsModalPersistenceSource).toContain('showAdvancedSettings: checked');
    expect(settingsSidebar).toContain("<PageIcon aria-hidden='true' data-icon='inline-start' />");
    expect(settingsSidebar).toContain("className='settings-sidebar-page-title truncate'");
    expect(settingsModalSource).not.toContain('settings-show-advanced-anchor');
    for (const categoryIcon of [
      'icon: IconSettings',
      'icon: IconDeviceDesktop',
      'icon: IconCloud',
      'icon: IconFolderOpen',
      'icon: IconKeyboard',
      'icon: IconCodeDots',
      'icon: IconTools',
      'icon: IconPlayerPlay',
      'icon: IconExternalLink',
    ]) {
      expect(settingsModalSidebarPagesSource).toContain(categoryIcon);
    }
    const subsectionButton = sourceBetween(
      settingsSidebar,
      "className='settings-section-sidebar-button settings-sidebar-subsection-button'",
      '</Button>'
    );
    expect(subsectionButton).toContain('{section.title}');
    expect(subsectionButton).not.toContain('data-icon');

    const sidebarContainerStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .settings-section-sidebar {',
      ".ghostex-settings-shadcn .settings-sidebar-tabs-list[data-slot='tabs-list'] {"
    );
    expect(sidebarContainerStyles).toContain('top: -1px;');
    const sidebarPageRowStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .settings-sidebar-page-row {',
      ".ghostex-settings-shadcn .settings-sidebar-tab-trigger[data-slot='tabs-trigger'] {"
    );
    expect(sidebarPageRowStyles).toContain(
      '.settings-sidebar-page-row:has(.settings-sidebar-tab-trigger[data-active])'
    );
    expect(sidebarPageRowStyles).toContain('background: var(--accent);');
    expect(sidebarPageRowStyles).toContain('including the disclosure chevron');
    const sidebarTriggerStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .settings-sidebar-tab-trigger[data-slot='tabs-trigger'] {",
      ".ghostex-settings-shadcn .settings-sidebar-tab-trigger[data-slot='tabs-trigger']:hover"
    );
    expect(sidebarTriggerStyles).toContain('gap: 0.5rem;');
    expect(sidebarTriggerStyles).toContain('.settings-sidebar-page-title');
    const sidebarSubsectionStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .settings-sidebar-subsection-button {',
      '.ghostex-settings-shadcn .settings-section-anchor {'
    );
    expect(sidebarSubsectionStyles).toContain(
      'color: color-mix(in srgb, var(--muted-foreground) 72%, var(--background));'
    );
    expect(sidebarSubsectionStyles).toContain('padding-left: calc(0.625rem + 14px) !important;');
    expect(sidebarSubsectionStyles).toContain(".settings-sidebar-subsection-button[data-active='true']");
    expect(sidebarSubsectionStyles).toContain('background: transparent !important;');
    expect(sidebarSubsectionStyles).toContain('color: var(--foreground);');
    expect(sidebarSubsectionStyles).not.toContain('background: var(--accent);');
  });

  test('persists Settings location while browsing the native Settings window', () => {
    /*
     * CDXC:Settings 2026-06-30-04:47:
     * The native AppKit Settings close button can tear down the child window
     * before React's Dialog close callback runs. Persist the selected Settings
     * page immediately and persist scroll after a short idle window so app
     * relaunch restore does not depend on close-event delivery.
     */
    const scrollHandler = sourceBetween(
      settingsModalSource,
      'const handleSettingsModalScrollCapture',
      'const handleSettingsModalKeyDownCapture'
    );
    expect(scrollHandler).toContain('rememberSettingsModalScrollTop(activeTab, event.target.scrollTop);');
    expect(scrollHandler).toContain('scheduleSettingsModalNavigationPersist(activeTab);');

    const tabSetter = sourceBetween(settingsModalSource, 'const setActiveTab =', 'const toggleSettingsSidebarPage');
    expect(tabSetter).toContain('rememberSettingsModalTab(visibleTab);');
    expect(tabSetter).toContain('persistSettingsModalNavigation(visibleTab);');

    const navigationPersistence = sourceBetween(
      settingsModalPersistenceSource,
      'const persistSettingsModalNavigation',
      'const closeSettingsModal'
    );
    expect(navigationPersistence).toContain('Native Settings is an AppKit child window');
    expect(navigationPersistence).toContain(
      'settingsModalNavigation: getRememberedSettingsModalNavigationState(\n            navigationActiveTab,'
    );
    expect(navigationPersistence).toContain('clearPendingNavigationPersist();');
    expect(navigationPersistence).toContain('SETTINGS_MODAL_NAVIGATION_SCROLL_DEBOUNCE_MS');
  });

  test('uses the native window title instead of duplicate Settings chrome', () => {
    /*
     * CDXC:Settings 2026-06-25-17:05:
     * The native Settings titlebar owns the visible "Ghostex Settings" title.
     * React should not duplicate a large Settings heading or render an extra
     * close button inside the content surface.
     */
    const dialogHeader = sourceBetween(
      settingsModalSource,
      "<DialogHeader className='ghostex-modal-heading-bar'>",
      '</DialogHeader>'
    );
    expect(dialogHeader).toContain('Ghostex Settings');
    expect(dialogHeader).toContain("!isFirstLaunchSetup && 'sr-only'");
    expect(settingsModalSource).not.toContain("aria-label='Close Settings'");
    expect(settingsModalSource).not.toContain('ghostex-modal-icon-close');
  });

  test('keeps settings search at the top of the content column outside the sidebar', () => {
    /*
     * CDXC:Settings 2026-06-19-12:18:
     * Settings and Hotkeys search should center above the settings card column,
     * independent of the floating left sidebar.
     *
     * CDXC:Settings 2026-06-24-22:16:
     * Settings has one search field above the active content column while page
     * and section navigation lives in the sidebar.
     */
    const headerSearch = sourceBetween(
      settingsModalSource,
      "<div className='settings-modal-search-row'>",
      "toolbarClassName='settings-modal-search-toolbar'"
    );
    expect(headerSearch).toContain('<SidebarSessionSearchField');
    expect(headerSearch).toContain("placeholder='Search settings'");
  });

  test('keeps focused text fields from being redirected into settings search', () => {
    /*
     * CDXC:Settings 2026-06-19-16:53:
     * Font Family and other Settings text fields must keep printable typing in
     * the focused input while immediate-save settings updates round-trip
     * through the native modal host.
     *
     * CDXC:Settings 2026-06-25-21:21:
     * Settings search may prefill from deep links and printable-key capture,
     * but automatic focus must not move from an already-focused Settings text
     * field, including portal-rendered popover inputs, into the search field.
     */
    const keyCapture = sourceBetween(
      settingsModalSource,
      'const handleSettingsModalKeyDownCapture',
      'const setActiveTab'
    );
    const searchFocusPolicy = sourceBetween(
      settingsModalSource,
      'const shouldFocusSettingsSearchInput',
      'const focusSearchInput'
    );
    const deepLinkSearchPrefill = sourceBetween(
      settingsModalSource,
      'setSettingsSearchQuery(nextQuery);',
      '}, [focusSearchInput, initialSearchQuery, initialTab, isFirstLaunchSetup, isOpen]);'
    );
    const headerSearch = sourceBetween(
      settingsModalSource,
      "<div className='settings-modal-search-row'>",
      "toolbarClassName='settings-modal-search-toolbar'"
    );
    const textField = sourceBetween(
      settingsModalFieldsSource,
      'function TextField',
      'function DisabledCommandPreviewField'
    );

    expect(keyCapture).toContain('isEditableSettingsModalElement(event.currentTarget.ownerDocument.activeElement)');
    expect(searchFocusPolicy).toContain('isEditableSettingsModalElement(activeElement)');
    expect(searchFocusPolicy).not.toContain('dialogContentRef.current?.contains(activeElement)');
    expect(deepLinkSearchPrefill).toContain('if (focusSearchInput())');
    expect(headerSearch).toContain('shouldFocusOnQueryChange={shouldFocusSettingsSearchInput}');
    expect(textField).toContain('const inputRef = useRef<HTMLInputElement>(null);');
    expect(textField).toContain('const [inputValue, setInputValue] = useState(value);');
    expect(textField).toContain('value={inputValue}');
  });

  test('keeps hook and skill uninstall controls beside their install controls', () => {
    /*
     * CDXC:RemotePairing 2026-06-21-02:54:
     * Hooks & Skills uninstall controls must disable their no-op states when
     * hooks or bundled skills are already absent.
     *
     * CDXC:AgentHooks 2026-06-29-01:26:
     * Agent hook setup belongs in Settings > Agents, so Integrations must not
     * duplicate the Agent Hooks row.
     *
     * CDXC:AgentHooks 2026-08-19-11:20:
     * Removal lives next to the install control it undoes — a per-agent remove
     * on each installed hook row plus one Uninstall All in the hooks toolbar —
     * instead of a separate Hooks & Skills recovery card.
     *
     * CDXC:AgentHooks 2026-08-28:
     * Settings > Agents has one roster card. The standalone Agent Hooks card is
     * gone, so hook install/uninstall must live in the merged card's toolbar and
     * in each expandable agent row.
     */
    const navigation = sourceFrom(settingsModalSearchCatalogSource, 'const mainSettingsSectionNavigation');
    const integrationsTab = sourceBetween(
      settingsModalIntegrationsTabSource,
      'function IntegrationsSettingsTab',
      'function IntegrationSettingsRow'
    );
    const agentsTab = sourceBetween(
      settingsModalAgentsTabSource,
      'function AgentsSettingsTab',
      'function resolveSettingsTitleGenerationCommand'
    );
    const hookStatusRow = sourceBetween(
      settingsModalAgentsTabSource,
      'function SettingsAgentRow',
      'function AgentSettingsEditor'
    );

    expect(navigation).not.toContain("title: 'Hooks & Skills'");
    expect(settingsModalSource).not.toContain('hooksSkills');
    expect(settingsModalSource).not.toContain("title='Hooks & Skills'");
    expect(settingsModalSource).not.toContain('search.sections.recovery');
    expect(integrationsTab).not.toContain("title='Agent Hooks'");
    expect(integrationsTab).not.toContain('Install Hooks');
    expect(integrationsTab).not.toContain('Uninstall Hooks');
    expect(settingsModalAgentsTabSource).not.toContain("title='Agent Hooks'");
    expect(agentsTab).toContain('agentHooksAvailableForUninstall');
    expect(agentsTab).toContain('Uninstall All');
    expect(agentsTab).toContain('onClick={() => onUninstallAgentHooks?.()}');
    expect(agentsTab).toContain('() => onInstallAgentHooks([hookAgentId])');
    expect(agentsTab).toContain('() => onUninstallAgentHooks([hookAgentId])');
    expect(hookStatusRow).toContain('hasRemovableAgentHookStatus(hookStatus)');
    expect(hookStatusRow).toContain('aria-label={`Uninstall ${agent.name} hook`}');
    expect(integrationsTab).toContain('onUninstallAllSkills={onUninstallBundledAgentSkills}');
    expect(skillsPanelSource).toContain('onUninstallAllSkills');
    expect(skillsPanelSource).toContain('Uninstall All');
    expect(skillsPanelSource).toContain('const uninstallDisabled = ghostexCliStatusLoading || !onUninstall;');
    expect(skillsPanelSource).toContain('disabled={uninstallDisabled}');
  });

  test('gates Keep Awake settings behind Enable Experimental Features', () => {
    /*
     * CDXC:Settings 2026-06-28-07:41:
     * Keep Awake is experimental-only in regular macOS Settings, but the
     * Experimental section must name the hidden Power settings and titlebar
     * button so search can lead users to the opt-in gate.
     */
    const betaSearch = sourceBetween(
      settingsModalSearchCatalogSource,
      "beta: getSettingsSectionSearch(settingsSearchQuery, 'Experimental', [",
      "debugging: getSettingsSectionSearch(settingsSearchQuery, 'Debugging', ["
    );
    const betaSection = sourceBetween(
      settingsModalSource,
      "<SettingsSection sectionRef={betaSectionRef} title='Experimental'>",
      "{mainSubsectionVisible('debugging', settingsSearch.debugging) ? ("
    );
    const mainVisibility = sourceBetween(
      settingsModalVisibilitySource,
      'const keepAwakeSettingsVisible =',
      'const visibleMainSettingsSectionNavigation'
    );

    expect(betaSearch).toContain('Keep Awake');
    expect(settingsModalSource).toContain('keepAwakeWhileWorkingSessions');
    expect(settingsModalSource).toContain('Keep awake for working sessions');
    expect(betaSection).toContain('Title bar and Power settings: Keep Awake');
    expect(betaSection).toContain('Keep Awake title-bar button');
    expect(mainVisibility).toContain("sectionId === 'power' && !keepAwakeSettingsVisible");
    expect(settingsModalSource).toContain('system: powerSectionRef');
    expect(mainVisibility).toContain('first-launch lid-close preference');
  });

  test('hides debugging settings below Show debug UI controls when disabled', () => {
    /*
     * CDXC:Diagnostics 2026-06-28-18:14:
     * Disabling Show debug UI controls should hide the related Debugging rows
     * below it, and Settings search/navigation should use the same gate so
     * hidden diagnostic rows do not leave an empty Debugging section.
     */
    const dependentKeys = sourceBetween(
      settingsModalTypesSource,
      'const DEBUGGING_MODE_DEPENDENT_SETTING_KEYS = [',
      '] as const;'
    );
    const debuggingVisibility = sourceFrom(
      settingsModalVisibilitySource,
      'const debuggingModeDependentSettingsVisible = draft.debuggingMode;'
    );
    const debuggingSection = sourceBetween(
      settingsModalSource,
      "<SettingsSection sectionRef={debuggingSectionRef} title='Debugging'>",
      '{!isFirstLaunchSetup && !hasVisibleMainSettings ? ('
    );

    expect(dependentKeys).toContain("'diagnosticLogging'");
    expect(dependentKeys).toContain("'showSessionCommandCopyActions'");
    expect(dependentKeys).toContain("'showSessionDetailsCopyAction'");
    expect(debuggingVisibility).toContain('DEBUGGING_MODE_DEPENDENT_SETTING_KEY_SET.has(settingKey)');
    expect(debuggingVisibility).toContain("sectionId === 'debugging'");
    expect(debuggingVisibility).toContain('subsectionMatchesGroupedSectionTitle(sectionId)');
    expect(debuggingVisibility).toContain("shouldShowSetting(sectionResult, 'debuggingMode', showAdvancedSettings)");
    expect(debuggingVisibility).toContain(
      'const hasVisibleMainSettings = visibleMainSettingsSectionNavigation.length > 0;'
    );
    expect(debuggingSection).toContain("debuggingSettingVisible('debuggingMode')");
    expect(debuggingSection).toContain("debuggingSettingVisible('diagnosticLogging')");
    expect(debuggingSection).toContain("debuggingSettingVisible('showSessionCommandCopyActions')");
    expect(debuggingSection).toContain("debuggingSettingVisible('showSessionDetailsCopyAction')");
    expect(debuggingSection).toContain('Turn on to reveal debug-only controls and allow routine diagnostic logging.');
  });

  test('shows unavailable gxserver-owned default prompt agents without selecting Codex', () => {
    /*
     * CDXC:AgentProviders 2026-06-19-08:58:
     * Settings must preserve and display a gxserver-owned Default Prompt Agent
     * even when the local launcher registry cannot currently provide a command.
     * Showing an unavailable row is preferable to visually falling back to Codex.
     */
    const agentsTab = sourceBetween(
      settingsModalAgentsTabSource,
      'function AgentsSettingsTab',
      'function resolveSettingsTitleGenerationCommand'
    );

    expect(agentsTab).toContain('const promptAgentSelectOptions = promptAgentHasSavedDefault');
    expect(agentsTab).toContain('Unavailable (${normalizedDefaultPromptAgentId})');
    expect(agentsTab).toContain('const selectedDefaultPromptAgentId = normalizedDefaultPromptAgentId;');
    expect(agentsTab).not.toContain('promptAgentOptions.find');
  });

  test('routes settings select popups through the close-before-write wrapper', () => {
    /*
     * CDXC:Settings 2026-06-19-19:22:
     * Changing Settings dropdowns in the macOS modal must close the popup
     * before native and gxserver settings hydration can re-render the dialog,
     * otherwise the portaled popup can keep input trapped.
     */
    const settingsSelect = sourceBetween(
      settingsModalFieldsSource,
      'function SettingsSelect',
      'function SettingsSelectContent'
    );
    const selectField = sourceBetween(settingsModalFieldsSource, 'function SelectField', 'function StaticNoteField');
    const fieldsSourceWithoutSettingsSelect = settingsModalFieldsSource.replace(settingsSelect, '');

    expect(settingsModalFieldsSource).toContain("import { flushSync } from 'react-dom';");
    expect(settingsSelect).toContain('const [selectOpen, setSelectOpen] = useState(false);');
    expect(settingsSelect).toContain('flushSync(() => {');
    expect(settingsSelect).toContain('onOpenChange={(nextOpen, eventDetails) => {');
    expect(settingsSelect).toContain('open={selectOpen}');
    expect(selectField).toContain('<SettingsSelect');
    expect(fieldsSourceWithoutSettingsSelect).not.toMatch(/<Select(?:\s|>)/u);
    expect(settingsModalSource).not.toMatch(/<Select(?:\s|>)/u);
  });

  test('keeps dev-server controls in the Terminal settings flow', () => {
    /*
     * CDXC:Resources 2026-06-23-19:22:
     * Dev-server detection and ignored ports should live in a dedicated Terminal settings section rather than the generic Browser section or a per-browser checklist.
     *
     * CDXC:Navigation 2026-08-19:
     * The launch choice moved the other way: it is now the Browser section's single web-link target, shared with terminal and session chat links.
     */
    const sectionKeys = sourceBetween(settingsModalTypesSource, 'terminalDevServers: [', 'browser: [');
    const settingsNavigation = sourceFrom(settingsModalSearchCatalogSource, 'const mainSettingsSectionNavigation');
    const devServersSection = sourceBetween(
      settingsModalSource,
      "title='Dev Servers'",
      "{mainSubsectionVisible('editor', settingsSearch.editor) ? ("
    );

    expect(sectionKeys).toContain('terminalDevServerDetectionEnabled');
    expect(sectionKeys).not.toContain('terminalDevServerOpenTarget');
    expect(sectionKeys).toContain('terminalDevServerIgnoredPortRules');
    expect(settingsNavigation).toContain("id: 'tools'");
    expect(settingsNavigation).toContain('mainSettingsGroupSearch.tools');
    expect(settingsModalSource).toContain("'terminalDevServers'");
    expect(devServersSection).not.toContain('WEB_LINK_OPEN_TARGET_OPTIONS');
    expect(devServersSection).not.toContain('TerminalDevServerBrowserTargetsField');
    expect(devServersSection).toContain('TerminalDevServerIgnoredPortsField');
  });

  test('keeps terminal pane padding sliders in Terminal settings', () => {
    /*
     * CDXC:Terminal 2026-06-25-21:27:
     * Inner terminal pane padding is a Terminal settings control with separate
     * horizontal and vertical sliders. It should not be modeled as Workspace
     * pane gap or Terminal Behavior because it changes AppKit terminal content
     * frames, not split spacing or Ghostty config behavior.
     */
    const terminalSectionKeys = sourceBetween(settingsModalTypesSource, 'terminal: [', 'tools: [');
    const terminalSearch = sourceBetween(
      settingsModalSearchCatalogSource,
      "terminal: getSettingsSectionSearch(settingsSearchQuery, 'Terminal', [",
      "terminalBehavior: getSettingsSectionSearch(settingsSearchQuery, 'Terminal Behavior', ["
    );
    const terminalSection = sourceBetween(
      settingsModalSource,
      "title='Terminal'",
      "{mainSubsectionVisible('terminalBehavior', settingsSearch.terminalBehavior) ? ("
    );

    expect(terminalSectionKeys).toContain('terminalPaneHorizontalPaddingPx');
    expect(terminalSectionKeys).toContain('terminalPaneVerticalPaddingPx');
    expect(terminalSearch).toContain('Horizontal Padding');
    expect(terminalSearch).toContain('Vertical Padding');
    expect(terminalSection).toContain('MAX_TERMINAL_PANE_PADDING_PX');
    expect(terminalSection).toContain('MIN_TERMINAL_PANE_PADDING_PX');
    expect(terminalSection).toContain("updateDraft('terminalPaneHorizontalPaddingPx'");
    expect(terminalSection).toContain("updateDraft('terminalPaneVerticalPaddingPx'");
  });

  test('closes the custom tint picker dialog before final setting commits', () => {
    /*
     * CDXC:Theming 2026-06-19-19:51:
     * The custom Background Tint picker is a nested dialog, not a dropdown,
     * but it still must close before final settings persistence can re-render
     * the macOS Settings modal.
     */
    const colorPickerField = sourceBetween(
      settingsModalFieldsSource,
      'function WebColorPickerField',
      'function normalizeColorInputValue'
    );

    expect(colorPickerField).toContain('const commitColorAfterClosingPicker');
    expect(colorPickerField).toContain('setPickerOpen(false);');
    expect(colorPickerField).toContain('commitColor(nextColor);');
    expect(colorPickerField).toContain('commitColorAfterClosingPicker(colorValue);');
  });

  test('keeps project deletion out of the Projects settings page', () => {
    /*
     * CDXC:Projects 2026-06-19-12:11:
     * Projects settings edits selected-project metadata only. The standalone
     * trash action should not be available from this page.
     */
    const projectsPanel = sourceFrom(settingsModalProjectsTabSource, 'function ProjectsSettingsPanel');
    const selectedProjectEditor = sourceFrom(
      settingsModalProjectsTabSource,
      "<Card className='settings-project-command-card'>"
    );

    expect(projectsPanel).not.toContain("type: 'removeProject'");
    expect(projectsPanel).not.toContain('removeSelectedProject');
    expect(projectsPanel).not.toContain('Remove project');
    expect(selectedProjectEditor).not.toContain('<IconTrash');
  });

  test('keeps Portless global settings out of the Projects tab', () => {
    /*
     * CDXC:Portless 2026-06-23-03:47 / CDXC:RemotePairing 2026-09-03:
     * Phase 14 put app-wide Portless controls at the top of Settings ->
     * Projects. The panel was later hidden behind PORTLESS_SETTINGS_VISIBLE and
     * its gated render plus the dead portless props were removed from the
     * Projects tab; tabs/portless.tsx stays until its own removal.
     */
    const projectsPanel = sourceFrom(settingsModalProjectsTabSource, 'function ProjectsSettingsPanel');
    const settingsModalProjectsTab = sourceBetween(
      settingsModalSource,
      "<TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='projects'>",
      '</TabsContent>'
    );

    expect(projectsPanel).not.toContain('<PortlessGlobalSettingsPanel');
    expect(settingsModalProjectsTabSource).not.toContain("from './portless'");
    expect(projectsPanel).toContain("className='projects-settings-selector'");
    expect(settingsModalProjectsTab).not.toContain('portless={portless}');
    expect(settingsModalStylesSource).toContain('.settings-projects-global-settings');
  });

  test('keeps the Portless panel defaults wired to normalized global settings', () => {
    /*
     * CDXC:Portless 2026-06-23-03:47:
     * The retained Portless panel still reads the global settings defaults
     * from normalizeghostexSettings (Portless enabled and HTTPS protocol) even
     * though the Projects tab no longer mounts it.
     */
    const settingsModalProjectsTab = sourceBetween(
      settingsModalSource,
      "<TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='projects'>",
      '</TabsContent>'
    );
    const globalPanel = sourceBetween(
      settingsModalPortlessTabSource,
      'function PortlessGlobalSettingsPanel',
      'function PortlessSettingsAdminActionButton'
    );

    expect(settingsModalProjectsTab).not.toContain('onPortlessEnabledChange');
    expect(settingsModalProjectsTab).not.toContain('onPortlessProtocolChange');
    expect(globalPanel).toContain('checked={settings.portlessEnabled}');
    expect(globalPanel).toContain('value={settings.portlessProtocol}');
    expect(settingsModalPortlessTabSource).toContain("{ label: 'HTTPS', value: 'https' }");
    expect(settingsModalPortlessTabSource).toContain("{ label: 'HTTP', value: 'http' }");
  });

  test('keeps Portless settings actions explicit and sanitized', () => {
    /*
     * CDXC:Portless 2026-06-23-03:47:
     * Settings actions can install, reconfigure, retry, disable, or remove the
     * Ghostex-managed proxy, but the sidebar command may carry only enum action,
     * request id, and selected protocol metadata.
     */
    const projectsPanel = sourceFrom(settingsModalProjectsTabSource, 'function ProjectsSettingsPanel');
    const sharedSettingsCommand = sourceBetween(
      sharedSidebarContractSource,
      'Settings -> Projects exposes explicit Portless setup actions',
      "type: 'postponePortlessSetupPrompt'"
    );

    // CDXC:RemotePairing 2026-09-03: the Projects tab no longer posts Portless
    // admin actions; the sidebar command contract is unchanged.
    expect(projectsPanel).not.toContain("type: 'runPortlessSettingsAdminAction'");
    expect(settingsModalPortlessTabSource).toContain('onClick={() => onEnabledChange(false)}');
    expect(settingsModalPortlessTabSource).toContain("remove: 'Remove background proxy'");
    expect(sharedSettingsCommand).toContain('action: NativePortlessAdminInstallAction;');
    expect(sharedSettingsCommand).toContain('protocol: NativePortlessProtocol;');
    expect(sharedSettingsCommand).toContain("action: 'remove';");
  });

  test('shows assigned Portless domains as read-only project and worktree summaries', () => {
    /*
     * CDXC:Portless 2026-06-23-03:47:
     * Phase 14 displays generated project/worktree domains without slug edit,
     * reset, or input controls. Worktree grouping needs only stable project ids.
     */
    const domainsSummary = sourceBetween(
      settingsModalPortlessTabSource,
      'function PortlessAssignedDomainsSummary',
      'function getPortlessSettingsStatus'
    );
    const domainGrouping = sourceBetween(
      settingsModalPortlessTabSource,
      'function getProjectPortlessDomainSummaries',
      'function getPortlessAssignedDomainsEmptyMessage'
    );

    expect(domainsSummary).toContain("aria-label='Assigned Portless domains'");
    expect(domainsSummary).toContain('settings-portless-domain-hostname');
    expect(domainsSummary).toContain('Generated project and worktree domains are read-only.');
    expect(domainsSummary).not.toContain('SettingsInput');
    expect(domainsSummary).not.toContain('SettingsTextarea');
    expect(domainsSummary).not.toContain('slug');
    expect(domainsSummary).not.toContain('reset');
    expect(domainGrouping).toContain('assignedDomains');
    expect(domainGrouping).toContain('liveRoutesByProjectAndHostname');
    expect(domainGrouping).toContain('worktreeParentProjectId');
    expect(sharedSidebarContractSource).toContain('worktreeParentProjectId?: string');
  });

  test('wires the App Icon picker to the native wire contract with prop-driven confirm-before-persist', () => {
    /*
     * CDXC:Icons 2026-06-28-06:05:
     * The App Icon section must remain an advanced custom-image flow, speak the
     * exact native wire-contract messages, receive appIconState as a PROP
     * relayed through the modal host (mirroring osIntegrationStatus, not direct
     * host-event listeners), only persist appIconSourceId after an ok state
     * (confirm-before-persist), and render one preview with Select Image plus
     * an inline default-restore X.
     *
     * CDXC:Settings 2026-06-30-01:23:
     * App Icon now lives under the grouped Appearance navigation item rather
     * than owning a separate sidebar row.
     */
    const settingsNavigation = sourceFrom(settingsModalSearchCatalogSource, 'const mainSettingsSectionNavigation');
    const advancedMainSettings = sourceBetween(
      settingsModalTypesSource,
      'const ADVANCED_MAIN_SETTING_KEYS',
      'type HotkeySettingsSectionId'
    );
    const appIconSearch = sourceBetween(
      settingsModalSearchCatalogSource,
      "appIcon: getSettingsSectionSearch(settingsSearchQuery, 'App Icon', [",
      "browser: getSettingsSectionSearch(settingsSearchQuery, 'Browser', ["
    );
    const appIconField = sourceBetween(settingsModalFieldsSource, 'function AppIconPickerField', 'function SoundField');

    // Section is registered as advanced and grouped under Appearance.
    expect(settingsNavigation).toContain("id: 'appearance'");
    expect(settingsNavigation).not.toContain("id: 'appIcon'");
    expect(settingsNavigation).toContain('mainSettingsGroupSearch.appearance');
    expect(settingsModalSource).toContain('appIcon: appIconSectionRef');
    expect(advancedMainSettings).toContain("'appIconSourceId'");
    expect(appIconSearch).toContain('appIconSourceId');

    // Exact outbound wire-contract messages used by the simplified UI.
    expect(settingsModalEffectsSource).toContain("vscode.postMessage({ type: 'listAppIcons' });");
    expect(settingsModalAppIconSource).toContain("vscode.postMessage({ type: 'setAppIcon', sourceId });");
    expect(settingsModalAppIconSource).toContain("vscode.postMessage({ type: 'pickAppIconFile' });");
    expect(settingsModalAppIconSource).not.toContain('revealAppIconsFolder');

    // Inbound appIconState is prop-driven (relayed via the modal host like
    // osIntegrationStatus), NOT direct window host-event listeners.
    expect(settingsModalSource).toContain('appIconState?: SidebarAppIconStateMessage;');
    expect(settingsModalAppIconSource).toContain('}, [appIconState, draft]);');
    expect(settingsModalAppIconSource).not.toContain('handleAppIconHostEvent');
    expect(settingsModalAppIconSource).not.toContain('isSidebarAppIconStateMessage');

    // Confirm-before-persist: only an ok prop state writes appIconSourceId.
    expect(settingsModalAppIconSource).toContain('if (appIconState.ok) {');
    expect(settingsModalAppIconSource).toContain('handledAppIconStateRef.current === appIconState');
    expect(settingsModalAppIconSource).toContain("updateDraft('appIconSourceId', confirmedSourceId);");
    expect(settingsModalActionsSource).toContain("vscode?.postMessage({ type: 'setAppIcon', sourceId: '' });");

    // The field is one preview plus Select Image; no gallery, reveal action, or
    // separate reset button. The preview X selects the empty/default source id.
    expect(appIconField).toContain("const defaultIcon = allIcons.find((icon) => icon.id === '');");
    expect(appIconField).toContain("const icons = allIcons.filter((icon) => icon.id !== '');");
    expect(appIconField).toContain('Select Image');
    expect(appIconField).toContain('Use default icon');
    expect(appIconField).toContain("onClick={() => onSelect('')}");
    expect(appIconField).not.toContain('Choose File');
    expect(appIconField).not.toContain('Reveal in Finder');
    expect(appIconField).not.toContain('Reset to default');
    expect(settingsModalSource).not.toContain('function AppIconPickerTile');
    expect(settingsModalSource).toContain(
      "description='Changes the Dock and app-switcher icon. The app file icon may also change when macOS allows it.'"
    );
  });

  test('relays appIconState through the modal host like osIntegrationStatus', () => {
    /*
     * CDXC:Icons 2026-06-28-06:05:
     * SettingsModal renders in the modal-host child window, so the native
     * appIconState host event must be relayed to the modal host through the same
     * main-bus + sidebarState plumbing used by osIntegrationStatus. App-icon
     * commands must also forward through native-sidebar to Swift so Select Image
     * can open the AppKit file picker from the modal-host surface.
     */
    const modalHostSource = readFileSync(new URL('../../apps/desktop/views/modal-host.tsx', import.meta.url), 'utf8');

    // modal-host.tsx: route the relayed message into modal state and pass it on.
    expect(modalHostSource).toContain('isAppIconStateMessage(message.message)');
    expect(modalHostSource).toContain('setAppIconState(message.message);');
    expect(modalHostSource).toContain('appIconState={appIconState}');
  });

  test('keeps the open project selector neutral', () => {
    /*
     * CDXC:Projects 2026-06-19-12:22:
     * The Projects dropdown trigger should use neutral Settings colors when
     * open, not the app accent color that appears blue in dark themes.
     */
    const openSelectorStyles = sourceBetween(
      settingsModalStylesSource,
      '.projects-settings-selector-trigger[data-popup-open]',
      '}'
    );

    expect(openSelectorStyles).not.toContain('--app-button-background');
    expect(openSelectorStyles).toContain('--app-card-active');
    expect(openSelectorStyles).toContain('--app-border');
  });
});
