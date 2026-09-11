/*
 * CDXC:AgentSkills 2026-09-09 WHY:
 * The Ghostex Help skill answers "what does this setting do" and "change X for
 * me" from a catalog that is generated from the Settings modal's own search
 * rows, option tables, and defaults, plus the shared hotkey catalog. Generating
 * it (instead of hand-writing a second list the way bb does) is what keeps the
 * skill, `ghostex settings`, and `ghostex guide` from drifting away from the
 * UI: `bun run help:check` fails the typecheck gate when the committed output
 * is stale.
 * SEE-ALSO: packages/core-ui/settings-modal/search-catalog.ts, server/src/ghostex_cli/settings.rs, server/src/ghostex_cli/guide.rs.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  getMainSettingsGroupSearch,
  getMainSettingsSectionNavigation,
  getSettingsSearchSectionDefinitions,
  getSettingsSearchSections,
  MAIN_SETTINGS_GROUP_SECTIONS,
  type MainSettingsGroupId,
  type SettingsSearchSectionId,
} from '../../packages/core-ui/settings-modal/search-catalog';
import { EXTRA_SETTINGS_TAB_SEARCH_SECTIONS } from '../../packages/core-ui/settings-modal/search';
import {
  ADVANCED_MAIN_SETTING_KEYS,
  MAIN_SETTINGS_SECTION_SETTING_KEYS,
  type SettingSearchDefinition,
} from '../../packages/core-ui/settings-modal/types';
import { DEFAULT_ghostex_SETTINGS } from '../../packages/shared/ghostex-settings/defaults';
import {
  APP_SHOTS_HOTKEY_OPTIONS,
  DEFAULT_EDITOR_COMMAND_OPTIONS,
  SESSION_TITLE_GENERATION_AGENT_OPTIONS,
} from '../../packages/shared/ghostex-settings';
import {
  MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  MAX_SIDEBAR_DEFAULT_WIDTH_PX,
  MAX_SIDEBAR_TOOLTIP_DELAY_MS,
  MAX_TERMINAL_PANE_PADDING_PX,
  MAX_TERMINAL_VIEW_WIDTH_PERCENT,
  MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  MIN_SIDEBAR_DEFAULT_WIDTH_PX,
  MIN_SIDEBAR_TOOLTIP_DELAY_MS,
  MIN_TERMINAL_PANE_PADDING_PX,
  MIN_TERMINAL_VIEW_WIDTH_PERCENT,
  SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP,
  SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS,
  SIDEBAR_TOOLTIP_DELAY_STEP_MS,
  TERMINAL_VIEW_WIDTH_PERCENT_STEP,
} from '../../packages/shared/ghostex-settings/types';
import {
  MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
} from '../../packages/shared/ghostex-settings/titlebar-color';
import {
  MAX_AGENT_MANAGER_ZOOM_PERCENT,
  MIN_AGENT_MANAGER_ZOOM_PERCENT,
} from '../../packages/shared/session-grid-contract-core';
import { GHOSTEX_HOTKEY_DEFINITIONS } from '../../packages/shared/ghostex-hotkeys';

type CatalogValueType = 'boolean' | 'number' | 'string' | 'enum' | 'json' | 'ui';

type CatalogOption = { label: string; value: string };

type CatalogEntry = {
  key: string;
  title: string;
  subtitle: string;
  tab: string;
  tabTitle: string;
  group?: string;
  groupTitle?: string;
  section: string;
  sectionTitle: string;
  type: CatalogValueType;
  default?: unknown;
  options?: CatalogOption[];
  min?: number;
  max?: number;
  step?: number;
  advanced?: boolean;
  agentWritable: boolean;
};

type Catalog = {
  version: 1;
  generatedBy: string;
  settings: CatalogEntry[];
  hotkeys: Array<{
    id: string;
    title: string;
    description: string;
    defaultKey: string;
    windowsLinuxDefaultKey?: string;
  }>;
};

const GENERATOR_PATH = 'tooling/ghostex-help/generate.ts';
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const referencesDir = resolve(repoRoot, 'skills/ghostex-help/references');
const outputs = {
  catalog: resolve(referencesDir, 'settings-catalog.json'),
  hotkeys: resolve(referencesDir, 'hotkeys.md'),
  settings: resolve(referencesDir, 'settings.md'),
};

/** Keys whose value must never be read or written by an agent, whatever their type. */
const AGENT_DENIED_KEY_PATTERN = /token|password|secret|apikey|credential/iu;
/** Tabs whose rows are account or pairing state, never plain preferences. */
const AGENT_DENIED_TABS = new Set(['accounts', 'remote']);

const NUMBER_RANGES: Record<string, { max: number; min: number; step?: number }> = {
  agentManagerZoomPercent: { max: MAX_AGENT_MANAGER_ZOOM_PERCENT, min: MIN_AGENT_MANAGER_ZOOM_PERCENT },
  commandsPanelDefaultHeightPx: {
    max: MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
    min: MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  },
  customSidebarTitlebarBackgroundDarknessPercent: {
    max: MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
    min: MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  },
  projectSessionListCollapsedCount: {
    max: MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
    min: MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  },
  sessionChatTranscriptWidthPercent: {
    max: MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
    min: MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
    step: SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP,
  },
  sidebarCollapseAnimationDurationMs: {
    max: MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
    min: MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
    step: SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS,
  },
  sidebarDefaultWidthPx: { max: MAX_SIDEBAR_DEFAULT_WIDTH_PX, min: MIN_SIDEBAR_DEFAULT_WIDTH_PX },
  sidebarTooltipDelayMs: {
    max: MAX_SIDEBAR_TOOLTIP_DELAY_MS,
    min: MIN_SIDEBAR_TOOLTIP_DELAY_MS,
    step: SIDEBAR_TOOLTIP_DELAY_STEP_MS,
  },
  terminalPaneHorizontalPaddingPx: { max: MAX_TERMINAL_PANE_PADDING_PX, min: MIN_TERMINAL_PANE_PADDING_PX },
  terminalPaneVerticalPaddingPx: { max: MAX_TERMINAL_PANE_PADDING_PX, min: MIN_TERMINAL_PANE_PADDING_PX },
  terminalViewWidthPercent: {
    max: MAX_TERMINAL_VIEW_WIDTH_PERCENT,
    min: MIN_TERMINAL_VIEW_WIDTH_PERCENT,
    step: TERMINAL_VIEW_WIDTH_PERCENT_STEP,
  },
};

const defaults = DEFAULT_ghostex_SETTINGS as unknown as Record<string, unknown>;

type SupplementalRow = {
  group?: MainSettingsGroupId;
  options?: readonly { label: string; value: string }[];
  sectionTitle: string;
  section: string;
  subtitle: string;
  tab: string;
  tabTitle: string;
  title: string;
  /** Force non-writable for values the app derives or that belong to the user alone. */
  userOnly?: boolean;
};

const GENERAL_TAB = { tab: 'settings', tabTitle: 'General' } as const;
const EXTENSIONS_TAB = { tab: 'extensions', tabTitle: 'Extensions' } as const;
const AGENTS_TAB = { tab: 'agents', tabTitle: 'Agents' } as const;
const PROJECTS_TAB = { tab: 'projects', tabTitle: 'Projects' } as const;
const OPEN_TARGETS_TAB = { tab: 'openTargets', tabTitle: 'Open In' } as const;
const HOTKEYS_TAB = { tab: 'hotkeys', tabTitle: 'Hotkeys' } as const;
const titlebarViews = { ...EXTENSIONS_TAB, section: 'viewOrder', sectionTitle: 'Titlebar views' };
const agentsConfig = { ...AGENTS_TAB, section: 'config', sectionTitle: 'Config' };

/**
 * Settings keys the modal renders without a search row (or with a row keyed
 * by a UI id instead of the settings key). The Settings modal is still the
 * source of truth for copy; these entries only give agents a key to set.
 */
const SUPPLEMENTAL_SETTING_ROWS: Record<string, SupplementalRow> = {
  agentAcceptAllEnabled: {
    ...agentsConfig,
    subtitle:
      'Start supported agents with approvals turned off (full access) by default. Off keeps each agent CLI default approval mode. Projects can override it.',
    title: 'Agent approvals: accept all',
  },
  defaultPromptAgentId: {
    ...agentsConfig,
    subtitle:
      'Agent id used when Ghostex sends a prompt on your behalf (for example PR review). Use an id from the configured agents.',
    title: 'Default Prompt Agent',
  },
  sessionTitleGenerationAgent: {
    ...agentsConfig,
    options: SESSION_TITLE_GENERATION_AGENT_OPTIONS,
    subtitle: 'Headless agent Ghostex uses for first-prompt session title generation.',
    title: 'Title Generation Agent',
  },
  customSessionTitleGenerationCommand: {
    ...agentsConfig,
    subtitle:
      'Custom command run with the title prompt on stdin when Title Generation Agent is custom. It should print only the title.',
    title: 'Custom Title Command',
  },
  appShotsEnabled: {
    ...GENERAL_TAB,
    section: 'appShots',
    sectionTitle: 'App Shots (beta)',
    subtitle: 'Enable App Shots: press the App Shots hotkey to capture a window screenshot into the prompt.',
    title: 'App Shots',
  },
  appShotsHotkey: {
    ...GENERAL_TAB,
    options: APP_SHOTS_HOTKEY_OPTIONS,
    section: 'appShots',
    sectionTitle: 'App Shots (beta)',
    subtitle: 'Which modifier chord captures an App Shot.',
    title: 'App Shots hotkey',
  },
  appShotsMetadataEnabled: {
    ...GENERAL_TAB,
    section: 'appShots',
    sectionTitle: 'App Shots (beta)',
    subtitle: 'Also paste window metadata with the App Shot image link.',
    title: 'App Shots metadata',
  },
  showAdvancedSettings: {
    ...GENERAL_TAB,
    section: 'settingsModal',
    sectionTitle: 'Settings window',
    subtitle: 'Show the rows marked Advanced in Settings.',
    title: 'Show Advanced settings',
  },
  codeViewTabHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Code view tab from the titlebar.',
    title: 'Hide Code view',
  },
  browserViewTabHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Browser view tab from the titlebar.',
    title: 'Hide Browser view',
  },
  kanbanViewTabHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Kanban view tab from the titlebar.',
    title: 'Hide Kanban view',
  },
  automateViewTabHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Automate view tab from the titlebar.',
    title: 'Hide Automate view',
  },
  docsViewTabHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Docs view tab from the titlebar.',
    title: 'Hide Docs view',
  },
  tipsAndTricksTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Tips button from the titlebar.',
    title: 'Hide Tips button',
  },
  notificationsTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Notifications bell from the titlebar.',
    title: 'Hide Notifications bell',
  },
  helpTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Ghostex Help button from the titlebar.',
    title: 'Hide Help button',
  },
  resourcesTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Resources button from the titlebar.',
    title: 'Hide Resources button',
  },
  devServersTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Dev Servers button from the titlebar.',
    title: 'Hide Dev Servers button',
  },
  extensionsTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Extensions button from the titlebar.',
    title: 'Hide Extensions button',
  },
  gitActionsTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Git button from the titlebar.',
    title: 'Hide Git button',
  },
  quickActionsTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Actions button from the titlebar.',
    title: 'Hide Actions button',
  },
  openInTitlebarButtonHidden: {
    ...titlebarViews,
    subtitle: 'Hide the Open In button from the titlebar.',
    title: 'Hide Open In button',
  },
  defaultEditorCommand: {
    ...OPEN_TARGETS_TAB,
    options: DEFAULT_EDITOR_COMMAND_OPTIONS,
    section: 'defaultEditor',
    sectionTitle: 'Default editor',
    subtitle: 'Editor command used by Open In and `ghostex edit` when no other target is chosen.',
    title: 'Default editor',
  },
  customDefaultEditorCommand: {
    ...OPEN_TARGETS_TAB,
    section: 'defaultEditor',
    sectionTitle: 'Default editor',
    subtitle: 'Command used when Default editor is set to other.',
    title: 'Custom editor command',
  },
  manageAdditionalDocsFolders: {
    ...PROJECTS_TAB,
    section: 'docs',
    sectionTitle: 'Docs',
    subtitle: 'Extra folders (one per line) the Docs view lists in every project.',
    title: 'Additional docs folders',
  },
  globalBeadsDisplayKey: {
    ...PROJECTS_TAB,
    section: 'globalDefaults',
    sectionTitle: 'Global Defaults',
    subtitle: 'Default ticket key prefix shown on Kanban cards when a project has none.',
    title: 'Global ticket key',
  },
  analyticsEnabled: {
    tab: 'about',
    tabTitle: 'About',
    section: 'privacy',
    sectionTitle: 'Privacy',
    subtitle: 'Send anonymous usage analytics. Ask the user before changing it.',
    title: 'Anonymous analytics',
    userOnly: true,
  },
  hideAccountEmails: {
    tab: 'accounts',
    tabTitle: 'Accounts',
    section: 'accounts',
    sectionTitle: 'Accounts',
    subtitle: 'Hide account email addresses in the Accounts page and account switchers.',
    title: 'Hide account emails',
  },
  expandCollapsedProjectsOnJump: {
    ...HOTKEYS_TAB,
    section: 'projects',
    sectionTitle: 'Projects',
    subtitle: 'Expand a collapsed project when a project-jump hotkey lands on it.',
    title: 'Expand collapsed projects on jump',
  },
  showLessForExpandedProjectJumps: {
    ...HOTKEYS_TAB,
    section: 'projects',
    sectionTitle: 'Projects',
    subtitle: 'Collapse the session list of a project expanded by a jump back to Show less.',
    title: 'Show less after project jumps',
  },
  windowsWslDistribution: {
    ...GENERAL_TAB,
    group: 'terminal',
    section: 'terminal',
    sectionTitle: 'Terminal',
    subtitle: 'Windows only. Exact distro name from `wsl.exe --list --verbose`; blank uses automatic WSL2 discovery.',
    title: 'WSL distribution',
  },
  portlessEnabled: {
    ...PROJECTS_TAB,
    section: 'portless',
    sectionTitle: 'Portless',
    subtitle: 'Route project dev servers through named local addresses instead of raw ports.',
    title: 'Portless',
  },
  portlessProtocol: {
    ...PROJECTS_TAB,
    options: [
      { label: 'HTTPS', value: 'https' },
      { label: 'HTTP', value: 'http' },
    ],
    section: 'portless',
    sectionTitle: 'Portless',
    subtitle: 'Protocol Portless addresses use.',
    title: 'Portless protocol',
  },
  workspacePaneGap: {
    ...GENERAL_TAB,
    group: 'appearance',
    section: 'theming',
    sectionTitle: 'Theming',
    subtitle: 'Gap in pixels between split panes in the Agents view.',
    title: 'Pane gap',
  },
  customSidebarTitlebarForegroundColor: {
    ...GENERAL_TAB,
    group: 'appearance',
    section: 'theming',
    sectionTitle: 'Theming',
    subtitle: 'Derived from Background Contrast and Background Tint; change those instead.',
    title: 'Sidebar foreground color (derived)',
    userOnly: true,
  },
  customSidebarTitlebarBackgroundColor: {
    ...GENERAL_TAB,
    group: 'appearance',
    section: 'theming',
    sectionTitle: 'Theming',
    subtitle: 'Derived from Background Contrast and Background Tint; change those instead.',
    title: 'Sidebar background color (derived)',
    userOnly: true,
  },
  petOverlayEnabled: {
    ...GENERAL_TAB,
    group: 'statusIndicators',
    section: 'statusIndicators',
    sectionTitle: 'Status Indicators',
    subtitle: 'Show the draggable animated pet in the sidebar (also Wake Pet / Sleep Pet in Quick Access).',
    title: 'Pet overlay',
  },
  selectedPetId: {
    ...GENERAL_TAB,
    group: 'statusIndicators',
    section: 'statusIndicators',
    sectionTitle: 'Status Indicators',
    subtitle: 'Which pet sprite the overlay shows.',
    title: 'Pet',
  },
  remoteTailscaleEnabled: {
    tab: 'remote',
    tabTitle: 'Remote',
    section: 'tailscale',
    sectionTitle: 'Tailscale',
    subtitle: 'Offer the Tailscale path in Remote setup.',
    title: 'Tailscale on or off',
  },
};

const INTERNAL_STATE_ROW = {
  tab: 'settings',
  tabTitle: 'General',
  section: 'internalState',
  sectionTitle: 'Internal state (not user settings)',
} as const;

function valueTypeFor(key: string, hasOptions: boolean): CatalogValueType {
  if (!(key in defaults)) {
    return 'ui';
  }
  const value = defaults[key];
  if (typeof value === 'boolean') return 'boolean';
  if (typeof value === 'number') return 'number';
  if (typeof value === 'string') return hasOptions ? 'enum' : 'string';
  return 'json';
}

function catalogEntry(
  definition: SettingSearchDefinition,
  location: Pick<CatalogEntry, 'tab' | 'tabTitle' | 'group' | 'groupTitle' | 'section' | 'sectionTitle'>
): CatalogEntry {
  const options = definition.options?.map((option) => ({ label: option.label, value: option.value }));
  const type = valueTypeFor(definition.key, Boolean(options && options.length > 0));
  const primitive = type === 'boolean' || type === 'number' || type === 'string' || type === 'enum';
  const agentWritable =
    primitive && !AGENT_DENIED_TABS.has(location.tab) && !AGENT_DENIED_KEY_PATTERN.test(definition.key);
  const range = type === 'number' ? NUMBER_RANGES[definition.key] : undefined;
  return {
    key: definition.key,
    title: definition.title,
    subtitle: definition.subtitle ?? '',
    ...location,
    type,
    ...(type === 'ui' ? {} : { default: defaults[definition.key] }),
    ...(options && options.length > 0 ? { options } : {}),
    ...(range ? { min: range.min, max: range.max, ...(range.step ? { step: range.step } : {}) } : {}),
    ...(definition.advanced || ADVANCED_MAIN_SETTING_KEYS.has(definition.key) ? { advanced: true } : {}),
    agentWritable,
  };
}

function buildCatalog(): Catalog {
  const entries: CatalogEntry[] = [];
  const seen = new Set<string>();
  const push = (entry: CatalogEntry) => {
    if (seen.has(entry.key)) {
      return;
    }
    seen.add(entry.key);
    entries.push(entry);
  };

  const definitions = getSettingsSearchSectionDefinitions();
  const sections = getSettingsSearchSections('', DEFAULT_ghostex_SETTINGS);
  const navigation = getMainSettingsSectionNavigation(getMainSettingsGroupSearch('', sections));
  for (const navItem of navigation) {
    const groupId = navItem.id as MainSettingsGroupId;
    const group = MAIN_SETTINGS_GROUP_SECTIONS[groupId];
    for (const sectionId of group.sections as readonly SettingsSearchSectionId[]) {
      const section = definitions[sectionId];
      for (const definition of section.settings) {
        push(
          catalogEntry(definition, {
            tab: 'settings',
            tabTitle: 'General',
            group: groupId,
            groupTitle: group.title,
            section: sectionId,
            sectionTitle: section.title,
          })
        );
      }
    }
  }

  const groupedSectionIds = new Set<string>(
    Object.values(MAIN_SETTINGS_GROUP_SECTIONS).flatMap((group) => [...group.sections])
  );
  for (const [sectionId, section] of Object.entries(definitions)) {
    if (groupedSectionIds.has(sectionId)) {
      continue;
    }
    // A section outside the navigation groups still renders inside one of
    // them; find the group by the keys it lists so the reference matches the
    // Settings page layout.
    const firstKey = section.settings[0]?.key;
    const groupId = (Object.keys(MAIN_SETTINGS_GROUP_SECTIONS) as MainSettingsGroupId[]).find((candidate) =>
      firstKey ? MAIN_SETTINGS_SECTION_SETTING_KEYS[candidate].includes(firstKey) : false
    );
    for (const definition of section.settings) {
      push(
        catalogEntry(definition, {
          tab: 'settings',
          tabTitle: 'General',
          ...(groupId ? { group: groupId, groupTitle: MAIN_SETTINGS_GROUP_SECTIONS[groupId].title } : {}),
          section: sectionId,
          sectionTitle: section.title,
        })
      );
    }
  }

  for (const [tabId, tab] of Object.entries(EXTRA_SETTINGS_TAB_SEARCH_SECTIONS)) {
    for (const section of tab.sections) {
      for (const definition of section.settings) {
        push(
          catalogEntry(definition, {
            tab: tabId,
            tabTitle: tab.title,
            section: section.id,
            sectionTitle: section.title,
          })
        );
      }
    }
  }

  for (const [key, row] of Object.entries(SUPPLEMENTAL_SETTING_ROWS)) {
    if (!(key in defaults)) {
      throw new Error(`SUPPLEMENTAL_SETTING_ROWS names "${key}", which is not a ghostexSettings key.`);
    }
    const entry = catalogEntry(
      { key, options: row.options, subtitle: row.subtitle, title: row.title },
      {
        tab: row.tab,
        tabTitle: row.tabTitle,
        ...(row.group ? { group: row.group, groupTitle: MAIN_SETTINGS_GROUP_SECTIONS[row.group].title } : {}),
        section: row.section,
        sectionTitle: row.sectionTitle,
      }
    );
    push(row.userOnly ? { ...entry, agentWritable: false } : entry);
  }

  for (const key of Object.keys(defaults)) {
    if (seen.has(key)) {
      continue;
    }
    push({
      ...catalogEntry(
        { key, subtitle: 'App-managed state saved with the settings; not a user preference.', title: key },
        INTERNAL_STATE_ROW
      ),
      agentWritable: false,
    });
  }

  // Keep every row of a tab, group, and section together in first-seen
  // order, so supplemental rows land beside the page they belong to.
  const tabOrder = new Map<string, number>();
  const groupOrder = new Map<string, number>();
  const sectionOrder = new Map<string, number>();
  for (const entry of entries) {
    if (!tabOrder.has(entry.tab)) tabOrder.set(entry.tab, tabOrder.size);
    const groupKey = `${entry.tab}/${entry.group ?? ''}`;
    if (!groupOrder.has(groupKey)) groupOrder.set(groupKey, groupOrder.size);
    const sectionKey = `${groupKey}/${entry.section}`;
    if (!sectionOrder.has(sectionKey)) sectionOrder.set(sectionKey, sectionOrder.size);
  }
  entries.sort((left, right) => {
    const byTab = (tabOrder.get(left.tab) ?? 0) - (tabOrder.get(right.tab) ?? 0);
    if (byTab !== 0) return byTab;
    const byGroup =
      (groupOrder.get(`${left.tab}/${left.group ?? ''}`) ?? 0) -
      (groupOrder.get(`${right.tab}/${right.group ?? ''}`) ?? 0);
    if (byGroup !== 0) return byGroup;
    const sectionRank = (entry: CatalogEntry) =>
      entry.section === INTERNAL_STATE_ROW.section
        ? Number.MAX_SAFE_INTEGER
        : (sectionOrder.get(`${entry.tab}/${entry.group ?? ''}/${entry.section}`) ?? 0);
    return sectionRank(left) - sectionRank(right);
  });

  const hotkeys = GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => ({
    id: definition.id,
    title: definition.title,
    description: definition.description,
    defaultKey: definition.defaultKey,
    ...(definition.windowsLinuxDefaultKey ? { windowsLinuxDefaultKey: definition.windowsLinuxDefaultKey } : {}),
  }));

  return { version: 1, generatedBy: GENERATOR_PATH, settings: entries, hotkeys };
}

function formatDefault(value: unknown): string {
  if (typeof value === 'string') {
    return value === '' ? '(empty)' : value;
  }
  return JSON.stringify(value);
}

function describeType(entry: CatalogEntry): string {
  switch (entry.type) {
    case 'boolean':
      return `boolean, default ${formatDefault(entry.default)}`;
    case 'enum':
      return `one of ${entry.options?.map((option) => option.value).join(' | ') ?? ''}; default ${formatDefault(entry.default)}`;
    case 'number': {
      const range =
        entry.min !== undefined && entry.max !== undefined
          ? ` ${entry.min} to ${entry.max}${entry.step ? ` step ${entry.step}` : ''}`
          : '';
      const allowed = entry.options ? ` one of ${entry.options.map((option) => option.value).join(' | ')};` : '';
      return `number${range}${allowed} default ${formatDefault(entry.default)}`;
    }
    case 'string':
      return `text, default ${formatDefault(entry.default)}`;
    case 'json':
      return 'structured value; change it in Settings, not with `ghostex settings set`';
    case 'ui':
      return 'Settings UI row without a settings key; use `ghostex settings open`';
  }
}

function renderEntry(entry: CatalogEntry): string {
  const flags: string[] = [];
  if (entry.advanced) flags.push('advanced');
  if (!entry.agentWritable && entry.type !== 'json' && entry.type !== 'ui') flags.push('not agent-writable');
  const flagText = flags.length > 0 ? ` [${flags.join(', ')}]` : '';
  const optionLabels =
    entry.type === 'enum' || (entry.type === 'number' && entry.options)
      ? entry.options
          ?.filter((option) => option.label !== option.value)
          .map((option) => `${option.value} = ${option.label}`)
          .join(', ')
      : '';
  const optionText = optionLabels ? ` Option labels: ${optionLabels}.` : '';
  return `- **${entry.title}** \`${entry.key}\` (${describeType(entry)})${flagText}: ${entry.subtitle}${optionText}`;
}

function renderSettingsMarkdown(catalog: Catalog): string {
  const lines: string[] = [];
  lines.push('# Ghostex settings reference');
  lines.push('');
  lines.push(
    `Generated by \`${GENERATOR_PATH}\` from the Settings modal search catalog. Do not edit by hand; run \`bun run help:generate\`.`
  );
  lines.push('');
  lines.push('How to use this file:');
  lines.push('');
  lines.push(
    '- Every row is a Settings control. The backtick word is the settings key used by `ghostex settings get|set|reset <key>`.'
  );
  lines.push(
    '- `ghostex settings list --json` prints the same catalog with the current values, so prefer it over reading this file when the app is available.'
  );
  lines.push(
    '- Rows marked `not agent-writable`, `structured value`, or `Settings UI row` must be changed by the user in Settings. Open the right page with `ghostex settings open <key>`.'
  );
  lines.push(
    '- The desktop app must be running for `set`; it applies the change exactly like a save in the Settings modal.'
  );
  lines.push('');

  let currentTab = '';
  let currentGroup = '';
  let currentSection = '';
  for (const entry of catalog.settings) {
    if (entry.tab !== currentTab) {
      currentTab = entry.tab;
      currentGroup = '';
      currentSection = '';
      lines.push(`## ${entry.tabTitle} (tab \`${entry.tab}\`)`);
      lines.push('');
    }
    if (entry.group && entry.group !== currentGroup) {
      currentGroup = entry.group;
      currentSection = '';
      lines.push(`### ${entry.groupTitle}`);
      lines.push('');
    }
    if (entry.section !== currentSection) {
      currentSection = entry.section;
      const heading = entry.group ? '####' : '###';
      lines.push(`${heading} ${entry.sectionTitle}`);
      lines.push('');
    }
    lines.push(renderEntry(entry));
  }
  lines.push('');
  return `${lines.join('\n').replace(/\n{3,}/gu, '\n\n')}\n`.replace(/\n\n$/u, '\n');
}

function renderHotkeysMarkdown(catalog: Catalog): string {
  const lines: string[] = [];
  lines.push('# Ghostex hotkeys reference');
  lines.push('');
  lines.push(
    `Generated by \`${GENERATOR_PATH}\` from the shared hotkey catalog. Do not edit by hand; run \`bun run help:generate\`.`
  );
  lines.push('');
  lines.push(
    'Default bindings are listed for macOS (`cmd`) with the Windows/Linux default where it differs. Users rebind them in Settings > Hotkeys; open it with `ghostex settings open --tab hotkeys`. Hotkey bindings are a structured setting, so agents cannot change them with `ghostex settings set`.'
  );
  lines.push('');
  lines.push('| Action | Default | Windows/Linux | What it does | Id |');
  lines.push('| --- | --- | --- | --- | --- |');
  for (const hotkey of catalog.hotkeys) {
    const defaultKey = hotkey.defaultKey ? `\`${hotkey.defaultKey}\`` : 'unassigned';
    const winKey = hotkey.windowsLinuxDefaultKey ? `\`${hotkey.windowsLinuxDefaultKey}\`` : '';
    lines.push(
      `| ${hotkey.title} | ${defaultKey} | ${winKey} | ${hotkey.description.replace(/\|/gu, '\\|')} | \`${hotkey.id}\` |`
    );
  }
  lines.push('');
  return `${lines.join('\n')}\n`.replace(/\n\n$/u, '\n');
}

function main(): void {
  const check = process.argv.includes('--check');
  const catalog = buildCatalog();
  const rendered: Record<keyof typeof outputs, string> = {
    catalog: `${JSON.stringify(catalog, null, 2)}\n`,
    hotkeys: renderHotkeysMarkdown(catalog),
    settings: renderSettingsMarkdown(catalog),
  };
  const stale: string[] = [];
  for (const [name, path] of Object.entries(outputs) as Array<[keyof typeof outputs, string]>) {
    let existing: string | undefined;
    try {
      existing = readFileSync(path, 'utf8');
    } catch {
      existing = undefined;
    }
    if (existing === rendered[name]) {
      continue;
    }
    stale.push(path.slice(repoRoot.length + 1));
    if (!check) {
      writeFileSync(path, rendered[name]);
    }
  }
  if (check && stale.length > 0) {
    console.error(
      `Ghostex Help references are stale. Run \`bun run help:generate\` and commit:\n  ${stale.join('\n  ')}`
    );
    process.exit(1);
  }
  console.log(
    stale.length === 0
      ? `Ghostex Help references are up to date (${catalog.settings.length} settings, ${catalog.hotkeys.length} hotkeys).`
      : `Wrote ${stale.length} Ghostex Help reference file(s) (${catalog.settings.length} settings, ${catalog.hotkeys.length} hotkeys).`
  );
}

main();
