import Fuse from 'fuse.js';
import { Command } from '@/packages/components/ui/command';
import { APP_SHOTS_HOTKEY_OPTIONS, SESSION_TITLE_GENERATION_AGENT_OPTIONS } from '../../shared/ghostex-settings';
import { BUILT_IN_WORKSPACE_OPEN_TARGETS } from '../../shared/workspace-open-targets';
import { BUNDLED_GHOSTEX_AGENT_SKILLS } from '../../shared/ghostex-agent-skills';
import { DEFAULT_SIDEBAR_AGENTS } from '../../shared/sidebar-agents';
import {
  ADVANCED_MAIN_SETTING_KEYS,
  AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS,
  HOTKEY_SETTINGS_SECTIONS,
  HotkeySettingsDefinitionById,
  HotkeySettingsSectionSearches,
  SettingSearchDefinition,
  SettingsSectionMeasurementItem,
  SettingsSectionSearchResult,
} from './types';

export function getMostlyVisibleSettingsSectionId<SectionId extends string>(
  viewport: HTMLElement,
  sections: readonly SettingsSectionMeasurementItem<SectionId>[]
): SectionId | undefined {
  /*
   * CDXC:SettingsNavigation 2026-06-15-22:28:
   * Settings and Hotkeys section sidebars must track the section that occupies
   * the largest share of the scroll viewport so the highlighted nav item
   * follows reading position while users scroll long settings pages.
   */
  const viewportRect = viewport.getBoundingClientRect();
  const viewportCenter = viewportRect.top + viewportRect.height / 2;
  let bestSection:
    | {
        centerDistance: number;
        id: SectionId;
        visibleHeight: number;
      }
    | undefined;

  for (const section of sections) {
    const element = section.ref.current;
    if (!element) {
      continue;
    }

    const sectionRect = element.getBoundingClientRect();
    const visibleHeight = Math.max(
      0,
      Math.min(sectionRect.bottom, viewportRect.bottom) - Math.max(sectionRect.top, viewportRect.top)
    );
    if (visibleHeight <= 0) {
      continue;
    }

    const sectionCenter = sectionRect.top + sectionRect.height / 2;
    const centerDistance = Math.abs(sectionCenter - viewportCenter);
    if (
      !bestSection ||
      visibleHeight > bestSection.visibleHeight ||
      (visibleHeight === bestSection.visibleHeight && centerDistance < bestSection.centerDistance)
    ) {
      bestSection = { centerDistance, id: section.id, visibleHeight };
    }
  }

  return bestSection?.id;
}

export function getHotkeySettingsSectionSearches({
  definitionsById,
  expandCollapsedProjectsOnJump,
  searchQuery,
}: {
  definitionsById: HotkeySettingsDefinitionById;
  expandCollapsedProjectsOnJump: boolean;
  searchQuery: string;
}): HotkeySettingsSectionSearches {
  return Object.fromEntries(
    HOTKEY_SETTINGS_SECTIONS.map((section) => {
      const projectJumpSettings: SettingSearchDefinition[] =
        section.id === 'projects'
          ? [
              {
                key: 'expandCollapsedProjectsOnJump',
                subtitle: 'Reveal a collapsed Projects row before focusing it from Jump to Project hotkeys.',
                title: 'Expand collapsed projects on jump',
              },
              ...(expandCollapsedProjectsOnJump
                ? [
                    {
                      key: 'showLessForExpandedProjectJumps',
                      subtitle:
                        'After a project jump expands a collapsed project, switch that project session list to Show less.',
                      title: 'Use Show less after jump expand',
                    },
                  ]
                : []),
            ]
          : [];
      return [
        section.id,
        getSettingsSectionSearch(searchQuery, section.title, [
          ...projectJumpSettings,
          ...section.ids.flatMap((id) => {
            const definition = definitionsById.get(id);
            return definition
              ? [
                  {
                    key: definition.id,
                    options: [{ label: definition.defaultKey, value: definition.defaultKey }],
                    subtitle: definition.description,
                    title: definition.title,
                  },
                ]
              : [];
          }),
        ]),
      ];
    })
  ) as HotkeySettingsSectionSearches;
}

export function getSettingsSectionSearch(
  query: string,
  sectionTitle: string,
  settings: ReadonlyArray<SettingSearchDefinition>
): SettingsSectionSearchResult {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) {
    return {
      isSearching: false,
      sectionMatches: true,
      visibleSettingKeys: new Set(settings.map((setting) => setting.key)),
    };
  }

  const searchItems = [
    {
      id: '__section',
      options: [],
      subtitle: '',
      title: sectionTitle,
    },
    ...settings.map((setting) => ({
      id: setting.key,
      options: setting.options?.flatMap((option) => [option.label, option.value]) ?? [],
      subtitle: setting.subtitle ?? '',
      title: setting.title,
    })),
  ];
  const fuse = new Fuse(searchItems, {
    ignoreLocation: true,
    includeScore: true,
    keys: [
      { name: 'title', weight: 0.55 },
      { name: 'subtitle', weight: 0.25 },
      { name: 'options', weight: 0.2 },
    ],
    /**
     * CDXC:SettingsSearch 2026-05-13-16:05
     * Search should be useful without feeling random. A lower Fuse threshold
     * keeps section/settings/hotkey results close to the user's query instead
     * of surfacing weak fuzzy matches from unrelated settings.
     */
    threshold: 0.24,
  });
  const results = fuse.search(trimmedQuery);
  const sectionMatches = results.some((result) => result.item.id === '__section');
  return {
    isSearching: true,
    sectionMatches,
    visibleSettingKeys: new Set(
      results.map((result) => result.item.id).filter((settingKey) => settingKey !== '__section')
    ),
  };
}

export function getGroupedSettingsSectionSearch(
  query: string,
  sectionTitle: string,
  sections: readonly SettingsSectionSearchResult[]
): SettingsSectionSearchResult {
  const groupTitleResult = getSettingsSectionSearch(query, sectionTitle, []);
  const visibleSettingKeys = new Set<string>(groupTitleResult.visibleSettingKeys);
  for (const section of sections) {
    for (const settingKey of section.visibleSettingKeys) {
      visibleSettingKeys.add(settingKey);
    }
  }
  return {
    groupTitleMatches: groupTitleResult.sectionMatches,
    isSearching: groupTitleResult.isSearching || sections.some((section) => section.isSearching),
    sectionMatches: groupTitleResult.sectionMatches || sections.some((section) => section.sectionMatches),
    visibleSettingKeys,
  };
}

export function hasVisibleSettingsSearchResult(result: SettingsSectionSearchResult): boolean {
  return result.sectionMatches || result.visibleSettingKeys.size > 0;
}

export type SettingsTabSearchSectionDefinition = {
  id: string;
  settings: readonly SettingSearchDefinition[];
  title: string;
};

export type SettingsTabSearch = {
  sections: Record<string, SettingsSectionSearchResult>;
  tab: SettingsSectionSearchResult;
};

export type SearchableExtraSettingsTabId =
  'about' | 'actions' | 'agents' | 'integrations' | 'plugins' | 'openTargets' | 'osIntegration' | 'projects' | 'remote';

export type ExtraSettingsTabSearches = Record<SearchableExtraSettingsTabId, SettingsTabSearch>;

/**
 * CDXC:SettingsSearch 2026-07-22-00:00:
 * The one global Settings search field must find settings on every Settings
 * page, not only General and Hotkeys. Non-General pages keep their own static
 * search definitions here so the sidebar can filter pages to those with
 * matches and each page can filter its own sections and rows.
 */
export const EXTRA_SETTINGS_TAB_SEARCH_SECTIONS: Record<
  SearchableExtraSettingsTabId,
  { sections: readonly SettingsTabSearchSectionDefinition[]; title: string }
> = {
  about: {
    sections: [
      {
        id: 'about',
        settings: [
          { key: 'version', subtitle: 'Ghostex app version.', title: 'Version' },
          { key: 'discord', subtitle: 'Chat with the community and get help.', title: 'Join Discord' },
          {
            key: 'github',
            subtitle: 'View the source, releases, and report issues.',
            title: 'View on GitHub',
          },
          {
            key: 'sponsor',
            subtitle: 'Support the continued development of Ghostex.',
            title: 'Sponsor Ghostex',
          },
        ],
        title: 'About',
      },
    ],
    title: 'About',
  },
  actions: {
    sections: [
      {
        id: 'actions',
        settings: [
          {
            key: 'terminalAction',
            subtitle:
              'Add terminal actions to run saved commands in quick command terminals with one click or a hotkey.',
            title: 'Terminal Action',
          },
          {
            key: 'browserAction',
            subtitle: 'Add browser actions to open saved URLs in browser panes.',
            title: 'Browser Action',
          },
          {
            key: 'actionShortcuts',
            subtitle: 'Actions are custom shortcuts for repeat work, shared between a main project and its worktrees.',
            title: 'Custom actions',
          },
          {
            key: 'globalActions',
            subtitle:
              'Global actions apply to every project, are stored by the Ghostex daemon, and appear in the tab strip above your tabs.',
            title: 'Global Actions',
          },
          {
            key: 'hideTabStripNewTerminalButton',
            subtitle: 'Hide the New Terminal button from the tab strip.',
            title: 'Hide New Terminal button',
          },
          {
            key: 'hideTabStripNewBrowserButton',
            subtitle: 'Hide the New Browser Tab button from the tab strip.',
            title: 'Hide New Browser Tab button',
          },
        ],
        title: 'Actions',
      },
    ],
    title: 'Actions',
  },
  agents: {
    sections: [
      {
        id: 'agentHooks',
        settings: [
          {
            key: 'agentResumeHooks',
            options: AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.map((agent) => ({
              label: agent.name,
              value: agent.name,
            })),
            subtitle:
              "Install hooks so Ghostex can capture each agent's native session id and resume the exact conversation after sleep, reload, or app restart. Uninstall a single agent's hook from its row, or remove every Ghostex-owned hook with Uninstall All.",
            title: 'Agent resume hooks',
          },
        ],
        title: 'Agent Hooks',
      },
      {
        id: 'config',
        settings: [
          {
            key: 'defaultPromptAgent',
            subtitle:
              'Choose the agent used by Git helper prompts, project board Start Work, and the default worktree first-prompt selection.',
            title: 'Default Prompt Agent',
          },
          {
            key: 'titleGenerationAgent',
            options: SESSION_TITLE_GENERATION_AGENT_OPTIONS,
            subtitle: 'Choose the headless agent Ghostex uses for first-prompt session title generation.',
            title: 'Title Generation Agent',
          },
          {
            key: 'titleGenerationCommand',
            subtitle: 'Preview of the command Ghostex sends to generate automatic first-prompt session titles.',
            title: 'Title Generation Command',
          },
          {
            key: 'customTitleCommand',
            subtitle: 'Run this command with the title prompt on stdin. It should print only the title.',
            title: 'Custom Title Command',
          },
          {
            key: 'acceptAll',
            subtitle:
              "Enable each supported agent's permission-bypass mode when launching sessions. Per-agent settings can inherit or override this default.",
            title: 'Accept All',
          },
        ],
        title: 'Config',
      },
      {
        id: 'agentList',
        settings: [
          {
            key: 'addAgent',
            options: DEFAULT_SIDEBAR_AGENTS.map((agent) => ({
              label: agent.name,
              value: agent.name,
            })),
            subtitle: 'Add, reorder, edit, or delete agent launchers used to start new sessions.',
            title: 'Add Agent',
          },
        ],
        title: 'Agents',
      },
    ],
    title: 'Agents',
  },
  integrations: {
    sections: [
      {
        id: 'integrations',
        settings: [
          {
            key: 'ghostexCli',
            subtitle:
              'Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup.',
            title: 'Ghostex CLI',
          },
          {
            key: 'bundledAgentSkills',
            options: BUNDLED_GHOSTEX_AGENT_SKILLS.map((skill) => ({
              label: skill.name,
              value: skill.skillName,
            })),
            subtitle:
              'Install the Ghostex skills you want agents to discover. Each skill is copied to ~/.agents/skills and can be updated or uninstalled independently, or removed together with Uninstall All.',
            title: 'Bundled Agent Skills',
          },
          {
            key: 'appShots',
            options: APP_SHOTS_HOTKEY_OPTIONS,
            subtitle:
              'Capture the frontmost app window, then stage it in the focused or recent agent session as local image context.',
            title: 'App Shots',
          },
          {
            key: 'cuaPermissions',
            subtitle:
              'Cua Driver needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop.',
            title: 'Cua Permissions',
          },
        ],
        title: 'Integrations',
      },
    ],
    title: 'Integrations',
  },
  plugins: {
    sections: [
      {
        id: 'viewTabs',
        settings: [
          { key: 'code', subtitle: 'Show Code in the title bar and manage its VS Code runtime.', title: 'Code' },
          { key: 'browser', subtitle: 'Show or hide Browser in the title bar.', title: 'Browser' },
          { key: 'kanban', subtitle: 'Show Kanban in the title bar and manage its Beads runtime.', title: 'Kanban' },
          { key: 'automate', subtitle: 'Show or hide Automate in the title bar.', title: 'Automate' },
          { key: 'docs', subtitle: 'Show or hide Docs in the title bar.', title: 'Docs' },
        ],
        title: 'Plugins',
      },
      {
        id: 'components',
        settings: [
          {
            key: 'cuaDriver',
            subtitle: 'Install or upgrade Cua Driver for Ghostex Browser Use and native Desktop Control.',
            title: 'Cua Driver',
          },
          {
            key: 'cef',
            subtitle: 'Inspect or reinstall the Chromium runtime used by Ghostex web surfaces.',
            title: 'Chromium runtime (CEF)',
          },
        ],
        title: 'Shared components',
      },
      {
        id: 'quickAccessButtons',
        settings: [
          { key: 'tips', subtitle: 'Show or hide the Tips & Tricks titlebar button.', title: 'Tips & Tricks' },
          { key: 'resources', subtitle: 'Show or hide the Resources titlebar button.', title: 'Resources' },
          { key: 'gitActions', subtitle: 'Show or hide the Git actions titlebar button.', title: 'Git actions' },
          { key: 'quickActions', subtitle: 'Show or hide the Quick Actions titlebar button.', title: 'Quick Actions' },
          { key: 'openIn', subtitle: 'Show or hide the Open In titlebar button.', title: 'Open In' },
        ],
        title: 'Quick access buttons',
      },
    ],
    title: 'Customize',
  },
  openTargets: {
    sections: [
      {
        id: 'openIn',
        settings: BUILT_IN_WORKSPACE_OPEN_TARGETS.map((target) => ({
          key: `builtin:${target.id}`,
          subtitle: 'Show or hide this app on session Open In menus.',
          title: target.label,
        })),
        title: 'Open In',
      },
      {
        id: 'customOpenTargets',
        settings: [
          {
            key: 'addTarget',
            subtitle: 'Add a custom command Ghostex uses to open workspaces.',
            title: 'Add target',
          },
        ],
        title: 'Custom Open Targets',
      },
    ],
    title: 'Open In',
  },
  osIntegration: {
    sections: [
      {
        id: 'defaults',
        settings: [
          {
            key: 'setDefaultEditor',
            subtitle: 'Make Ghostex the default macOS editor for supported file types.',
            title: 'Set as Default Editor',
          },
          {
            key: 'setTerminalLinks',
            subtitle: 'Make Ghostex the handler for ghostex:// terminal links.',
            title: 'Set Terminal Links',
          },
          {
            key: 'setScriptRunner',
            subtitle: 'Make Ghostex the default macOS script runner.',
            title: 'Set Script Runner',
          },
          {
            key: 'setAll',
            subtitle: 'Set Ghostex as default editor, terminal-link handler, and script runner.',
            title: 'Set All',
          },
        ],
        title: 'Defaults',
      },
      {
        id: 'cli',
        settings: [
          {
            key: 'cliCommands',
            subtitle: 'Command-line examples: ghostex open, ghostex edit, ghostex terminal.',
            title: 'ghostex command line',
          },
        ],
        title: 'CLI',
      },
      {
        id: 'diagnostics',
        settings: [
          {
            key: 'handlerStatus',
            subtitle:
              'Check macOS Launch Services registration for editor defaults, script runner, and ghostex:// links.',
            title: 'macOS handler status',
          },
        ],
        title: 'Diagnostics',
      },
    ],
    title: 'OS Integration',
  },
  projects: {
    sections: [
      {
        id: 'docs',
        settings: [
          {
            key: 'docsFolders',
            subtitle: 'Comma-separated project-relative folders to scan recursively in Docs.',
            title: 'Docs folders',
          },
        ],
        title: 'Docs',
      },
      {
        id: 'globalDefaults',
        settings: [
          {
            key: 'globalWorktreeCommand',
            subtitle: 'Worktree command every project uses unless it sets its own.',
            title: 'Global worktree command',
          },
          {
            key: 'globalTicketKey',
            subtitle: 'Ticket key every project uses unless it sets its own.',
            title: 'Global ticket key',
          },
          {
            key: 'globalBeadsDirectory',
            subtitle: 'Beads directory every project uses unless it sets its own.',
            title: 'Global Beads directory',
          },
          {
            key: 'globalDocsDirectory',
            subtitle: "Extra folder Docs shows in every project, alongside that project's own docs.",
            title: 'Global Docs directory',
          },
        ],
        title: 'Global Defaults',
      },
      {
        id: 'projectSettings',
        settings: [
          {
            key: 'worktreeCommand',
            subtitle:
              'Runs in the new worktree folder before the project is added (useful for .envs, installing dependencies, etc.).',
            title: 'Worktree command',
          },
          {
            key: 'ticketKey',
            subtitle: 'Three-letter prefix used for Linear-style ticket numbers on the Project board.',
            title: 'Ticket key',
          },
          {
            key: 'beadsDirectory',
            subtitle: 'Absolute path the Project board reads its Beads workspace (.beads) from.',
            title: 'Beads directory',
          },
          {
            key: 'docsDirectory',
            subtitle: "Extra folder this project's Docs surface shows, in addition to its own docs.",
            title: 'Docs directory',
          },
        ],
        title: 'Project settings',
      },
    ],
    title: 'Projects',
  },
  remote: {
    sections: [
      {
        id: 'remoteMachines',
        settings: [
          {
            key: 'addMachine',
            subtitle: 'Saved SSH machines appear as separate sidebar sections.',
            title: 'Add remote machine',
          },
          { key: 'sshHost', subtitle: 'Remote machine SSH host.', title: 'SSH host' },
          { key: 'sshUser', subtitle: 'Remote machine SSH user.', title: 'SSH user' },
          { key: 'sshPort', subtitle: 'Remote machine SSH port.', title: 'SSH port' },
          {
            key: 'identityFile',
            subtitle: 'SSH identity file used to connect to the remote machine.',
            title: 'Identity file',
          },
          {
            key: 'password',
            subtitle: 'SSH passwords are stored in macOS Keychain.',
            title: 'Password',
          },
          {
            key: 'tailscaleSetup',
            subtitle: 'Use Tailscale when the remote machine is not reachable on your local network.',
            title: 'Tailscale setup',
          },
          {
            key: 'installGxserver',
            subtitle: 'Install, update, or connect gxserver on a saved remote machine.',
            title: 'Install / Connect gxserver',
          },
        ],
        title: 'Remote machines',
      },
    ],
    title: 'Remote',
  },
};

export function getExtraSettingsTabSearch(query: string, tab: SearchableExtraSettingsTabId): SettingsTabSearch {
  const definition = EXTRA_SETTINGS_TAB_SEARCH_SECTIONS[tab];
  const tabTitleResult = getSettingsSectionSearch(query, definition.title, []);
  const sections = Object.fromEntries(
    definition.sections.map((section) => {
      const sectionResult = getSettingsSectionSearch(query, section.title, section.settings);
      return [
        section.id,
        // A tab-title match (e.g. "remote") should reveal the whole page, so
        // treat every section on that page as matching.
        tabTitleResult.sectionMatches ? { ...sectionResult, sectionMatches: true } : sectionResult,
      ];
    })
  );
  return {
    sections,
    tab: getGroupedSettingsSectionSearch(query, definition.title, Object.values(sections)),
  };
}

export function getExtraSettingsTabSearches(query: string): ExtraSettingsTabSearches {
  return Object.fromEntries(
    (Object.keys(EXTRA_SETTINGS_TAB_SEARCH_SECTIONS) as SearchableExtraSettingsTabId[]).map((tab) => [
      tab,
      getExtraSettingsTabSearch(query, tab),
    ])
  ) as ExtraSettingsTabSearches;
}

export function settingsTabSearchHasMatches(search: SettingsTabSearch): boolean {
  return hasVisibleSettingsSearchResult(search.tab);
}

export function isAdvancedMainSetting(settingKey: string): boolean {
  return ADVANCED_MAIN_SETTING_KEYS.has(settingKey);
}

export function shouldShowSettingsSection(result: SettingsSectionSearchResult, showAdvancedSettings = true): boolean {
  if (!hasVisibleSettingsSearchResult(result)) {
    return false;
  }
  if (result.isSearching || showAdvancedSettings) {
    return true;
  }
  return Array.from(result.visibleSettingKeys).some((settingKey) => !isAdvancedMainSetting(settingKey));
}

export function shouldShowSetting(
  result: SettingsSectionSearchResult,
  settingKey: string,
  showAdvancedSettings = true
): boolean {
  if (result.isSearching) {
    return result.sectionMatches || result.visibleSettingKeys.has(settingKey);
  }
  return showAdvancedSettings || !isAdvancedMainSetting(settingKey);
}
