import {
  IconArrowLeft,
  IconArrowRight,
  IconArrowsDiagonal2,
  IconBrandGithub,
  IconBrowser,
  IconChecklist,
  IconClock,
  IconChevronDown,
  IconChevronLeft,
  IconChevronRight,
  IconChevronUp,
  IconEdit,
  IconExternalLink,
  IconFolderOpen,
  IconFolderPlus,
  IconGitFork,
  IconHistory,
  IconKeyboard,
  IconLayoutSidebarRightExpand,
  IconLayoutDashboard,
  IconLayoutSidebar,
  IconListDetails,
  IconMoon,
  IconNotebook,
  IconPlayerPlay,
  IconPlus,
  IconPinned,
  IconRefresh,
  IconRotateClockwise,
  IconSearch,
  IconServer,
  IconSettings,
  IconSettingsAutomation,
  IconStars,
  IconTerminal2,
  IconWindowMaximize,
  IconX,
} from '@tabler/icons-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from '@/packages/components/ui/command';
import type { SidebarCommandButton } from '../shared/sidebar-commands';
import { DEFAULT_SIDEBAR_COMMAND_ICON } from '../shared/sidebar-command-icons';
import { DEFAULT_ghostex_SETTINGS, type ghostexSettings } from '../shared/ghostex-settings';
import {
  GHOSTEX_HOTKEY_DEFINITIONS,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexFocusedPaneAction,
  type ghostexHotkeyDefinition,
  type ghostexHotkeySettings,
} from '../shared/ghostex-hotkeys';
import type { SidebarToExtensionMessage } from '../shared/session-grid-contract';
import { BUILT_IN_WORKSPACE_OPEN_TARGETS } from '../shared/workspace-open-targets';
import { openAppModal, openQuickAccess } from './app-modal-host-bridge';
import { QuickAccessHeader } from './quick-access-tabs';
import { getSidebarCommandRunModeForClick } from './command-run-feedback';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';
import { formatSidebarHotkeyLabel } from './hotkey-label';
import { filterCommandPaletteItems } from './command-palette-session-search';
import { useSidebarStore } from './sidebar-store';
import type { WebviewApi } from './webview-api';

type CommandPaletteProps = {
  commands: readonly SidebarCommandButton[];
  hotkeys?: ghostexHotkeySettings;
  initialQuery?: string;
  isInitialLoadResolved?: boolean;
  isOpen: boolean;
  isPrewarm?: boolean;
  onBrowserCommandRun?: () => void;
  onOpenChange: (isOpen: boolean) => void;
  openRequestSequence?: number;
  openTargetSettings?: CommandPaletteOpenTargetSettings;
  petOverlayEnabled?: boolean;
  vscode: WebviewApi;
};

type CommandPaletteOpenTargetSettings = Pick<
  ghostexSettings,
  'customWorkspaceOpenTargets' | 'workspaceOpenTargetAvailability' | 'workspaceOpenTargetHiddenIds'
>;

type HotkeyPaletteCommand = {
  definition: ghostexHotkeyDefinition;
  hotkey: string;
  kind: 'hotkey';
  searchText: string;
  title: string;
};

type BuiltInPaletteCommand =
  | HotkeyPaletteCommand
  | {
      commandId: AppModalPaletteCommandId;
      hotkey: '';
      kind: 'appModal';
      modal: AppModalPaletteModal;
      searchText: string;
      title: string;
    }
  | {
      commandId: SidebarMessagePaletteCommandId;
      hotkey: '';
      kind: 'sidebarMessage';
      message: PaletteSidebarMessage;
      searchText: string;
      title: string;
    }
  | {
      commandId: string;
      hotkey: '';
      kind: 'openTarget';
      searchText: string;
      targetId: string;
      title: string;
    }
  | {
      hotkey: '';
      kind: 'pet';
      searchText: string;
      title: string;
    };

type ProjectPaletteCommand = {
  command: SidebarCommandButton;
  hotkey: string;
  slotNumber: number;
};

type CommandPaletteSearchItem =
  | {
      command: BuiltInPaletteCommand;
      kind: 'builtIn';
      searchText: string;
    }
  | {
      command: HotkeyPaletteCommand;
      kind: 'paneAction';
      searchText: string;
    }
  | {
      command: ProjectPaletteCommand;
      kind: 'project';
      searchText: string;
    };

type AppModalPaletteCommandId =
  | 'actions'
  | 'addProject'
  | 'agentsHub'
  | 'configureAgents'
  | 'openTargets'
  | 'pinnedPrompts'
  | 'previousSessions'
  | 'runningSessions'
  | 'scratchPad';

type AppModalPaletteModal =
  | 'addProject'
  | 'agentsHub'
  | 'configureActions'
  | 'configureAgents'
  | 'daemonSessions'
  | 'hotkeys'
  | 'openTargets'
  | 'pinnedPrompts'
  | 'previousSessions'
  | 'scratchPad';

type SidebarMessagePaletteCommandId =
  | 'automations'
  | 'changelog'
  | 'features'
  | 'plugins'
  | 'openCurrentProjectInFinder'
  | 'quickBrowserTab'
  | 'quickTerminal'
  | 'searchByText'
  | 'setupGhostex'
  | 'tutorialVideo';

type PaletteSidebarMessage =
  | Extract<SidebarToExtensionMessage, { type: 'createChat' }>
  | Extract<SidebarToExtensionMessage, { type: 'openBrowserChat' }>
  | Extract<SidebarToExtensionMessage, { type: 'openBrowserPane' }>
  | Extract<SidebarToExtensionMessage, { type: 'openCurrentProjectInFinder' }>
  | Extract<SidebarToExtensionMessage, { type: 'openGhostexTutorialVideo' }>
  | Extract<SidebarToExtensionMessage, { type: 'openHighlightedFeatures' }>
  | Extract<SidebarToExtensionMessage, { type: 'openWorkspaceWelcome' }>
  | Extract<SidebarToExtensionMessage, { type: 'pickWorkspaceFolder' }>
  | Extract<SidebarToExtensionMessage, { type: 'searchPreviousSessionsByText' }>
  | Extract<SidebarToExtensionMessage, { type: 'openAutomationsPage' }>
  | Extract<SidebarToExtensionMessage, { type: 'runGhostexHotkeyAction' }>;

const PANE_ACTION_COMMAND_IDS = [
  'openBrowserPane',
  'splitMore',
  'splitMoreDown',
  'rotatePanesClockwise',
  'mergeAllTabs',
  'renameActiveSession',
  'delayedSend',
  'forkSession',
  'reloadSession',
  'sleepFocusedSession',
  'wakeFocusedSession',
  'closeFocusedSession',
  'popOutPane',
] as const satisfies readonly ghostexHotkeyDefinition['id'][];

const COMMAND_PALETTE_INPUT_SELECTOR = '[data-ghostex-command-palette-input="true"]';
const GHOSTEX_CHANGELOG_URL = 'https://github.com/maddada/ghostex/releases';

const APP_MODAL_PALETTE_COMMANDS = [
  {
    commandId: 'previousSessions',
    hotkey: '',
    kind: 'appModal',
    modal: 'previousSessions',
    searchText: 'Reopen a Session history restore previous sessions old sessions',
    title: 'Reopen a Session',
  },
  {
    commandId: 'pinnedPrompts',
    hotkey: '',
    kind: 'appModal',
    modal: 'pinnedPrompts',
    searchText: 'Pinned Prompts prompt library saved prompts modal',
    title: 'Pinned Prompts',
  },
  {
    commandId: 'runningSessions',
    hotkey: '',
    kind: 'appModal',
    modal: 'daemonSessions',
    searchText: 'Running Sessions daemon sessions runtimes modal',
    title: 'Running Sessions',
  },
  {
    commandId: 'scratchPad',
    hotkey: '',
    kind: 'appModal',
    modal: 'scratchPad',
    searchText: 'Scratch Pad notes modal',
    title: 'Scratch Pad',
  },
  {
    commandId: 'agentsHub',
    hotkey: '',
    kind: 'appModal',
    modal: 'agentsHub',
    searchText: 'Agents Hub agents profiles skills prompts modal',
    title: 'Agents Hub',
  },
  {
    commandId: 'configureAgents',
    hotkey: '',
    kind: 'appModal',
    modal: 'configureAgents',
    searchText: 'Configure Agents agents settings modal',
    title: 'Configure Agents',
  },
  {
    commandId: 'actions',
    hotkey: '',
    kind: 'appModal',
    modal: 'configureActions',
    searchText: 'Actions configure project actions settings modal',
    title: 'Actions',
  },
  {
    commandId: 'openTargets',
    hotkey: '',
    kind: 'appModal',
    modal: 'openTargets',
    searchText: 'Open Targets open in editors settings modal',
    title: 'Open Targets',
  },
  {
    /*
     * CDXC:AddProject 2026-07-30:
     * Add Project used to post `pickWorkspaceFolder`, which opened the OS
     * folder picker and could only ever add a folder on this computer. It now
     * opens the shared add-project dialog in the app-modal host, which resolves
     * its own machine list, so the palette command works for remote machines
     * and repository clones too.
     */
    commandId: 'addProject',
    hotkey: '',
    kind: 'appModal',
    modal: 'addProject',
    searchText: 'Add Project add folder workspace clone repository projects',
    title: 'Add Project',
  },
] as const satisfies readonly BuiltInPaletteCommand[];

const SIDEBAR_MESSAGE_PALETTE_COMMANDS = [
  {
    commandId: 'quickTerminal',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'createChat' },
    searchText: 'Quick Terminal new chat terminal',
    title: 'Quick Terminal',
  },
  {
    commandId: 'quickBrowserTab',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'openBrowserChat' },
    searchText: 'Quick Browser Tab browser chat',
    title: 'Quick Browser Tab',
  },
  {
    commandId: 'automations',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'openAutomationsPage' },
    searchText: 'Automations schedules agents timers dates recurring',
    title: 'Automations',
  },
  {
    /*
     * CDXC:AgentHistorySearch 2026-08-20:
     * Search by Text used to spawn a terminal running `gx f`. It now opens the
     * Find surface in the focused pane through the native hotkey dispatcher, so
     * the palette row, Alt+F, and the Hotkeys screen all reach one action.
     */
    commandId: 'searchByText',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { actionId: 'openFindPrompts', type: 'runGhostexHotkeyAction' },
    searchText: 'Search by Text Find Prompts previous sessions history gx f',
    title: 'Find Prompts',
  },
  {
    commandId: 'plugins',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { actionId: 'openPlugins', type: 'runGhostexHotkeyAction' },
    searchText: 'Plugins components VS Code code-server CEF gxserver Beads bd runtimes',
    title: 'Plugins',
  },
  {
    commandId: 'openCurrentProjectInFinder',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'openCurrentProjectInFinder' },
    searchText: 'Open Current Project in Finder open folder workspace',
    title: 'Open Current Project in Finder',
  },
  {
    commandId: 'features',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'openGhostexTutorialVideo' },
    /*
     * CDXC:GhostexTutorialVideo 2026-06-18-05:31:
     * The command-palette Features row should open the tutorial video modal so
     * the old Highlighted Features modal remains unused.
     *
     * CDXC:GhostexTutorialVideo 2026-06-18-05:49:
     * The tutorial video now uses Loom and the Ghostty-focused title, so search
     * metadata should match the current walkthrough terms.
     */
    searchText: 'Features Ghostty Loom tutorial video walkthrough modal',
    title: 'Features',
  },
  {
    commandId: 'tutorialVideo',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'openGhostexTutorialVideo' },
    searchText: 'Ghostty Loom tutorial video walkthrough how to use watch 1.5x',
    title: 'Tutorial Video',
  },
  {
    commandId: 'setupGhostex',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'openWorkspaceWelcome' },
    /*
     * CDXC:CommandPalette 2026-06-18-04:53:
     * User-facing setup actions should use the shorter "Setup" label while
     * search text keeps Ghostex and onboarding terms discoverable.
     */
    searchText: 'Ghostex setup onboarding first launch guide modal',
    title: 'Setup',
  },
  {
    commandId: 'changelog',
    hotkey: '',
    kind: 'sidebarMessage',
    message: { type: 'openBrowserPane', url: GHOSTEX_CHANGELOG_URL },
    searchText: 'Changelog release notes releases github browser',
    title: 'Changelog',
  },
] as const satisfies readonly BuiltInPaletteCommand[];

function createOpenTargetPaletteCommands(
  settings: CommandPaletteOpenTargetSettings | undefined
): BuiltInPaletteCommand[] {
  if (!settings) {
    return [];
  }
  /*
   * CDXC:CommandPalette 2026-06-18-03:46:
   * Open In rows should mirror the main titlebar menu: show installed and
   * visible built-in editor targets plus custom targets, but keep Finder as the
   * separate Open Current Project in Finder command so it reads as a project
   * action instead of another editor target.
   */
  const hiddenTargetIds = new Set(settings.workspaceOpenTargetHiddenIds);
  const availableTargetIds = new Set(settings.workspaceOpenTargetAvailability.availableTargetIds);
  const builtInTargets = BUILT_IN_WORKSPACE_OPEN_TARGETS.filter(
    (target) => target.id !== 'finder' && !hiddenTargetIds.has(target.id) && availableTargetIds.has(target.id)
  ).map((target): BuiltInPaletteCommand => ({
    commandId: `openTarget:${target.id}`,
    hotkey: '',
    kind: 'openTarget',
    searchText: `Open In ${target.label} current project workspace editor target`,
    targetId: target.id,
    title: `Open In: ${target.label}`,
  }));
  const customTargets = settings.customWorkspaceOpenTargets.map((target): BuiltInPaletteCommand => ({
    commandId: `openTarget:${target.id}`,
    hotkey: '',
    kind: 'openTarget',
    searchText: `Open In ${target.label} current project workspace custom target`,
    targetId: target.id,
    title: `Open In: ${target.label}`,
  }));
  return [...builtInTargets, ...customTargets];
}

function findCommandPaletteInput(): HTMLInputElement | null {
  return document.querySelector<HTMLInputElement>(COMMAND_PALETTE_INPUT_SELECTOR);
}

function isCommandPaletteTextKey(event: KeyboardEvent): boolean {
  return (
    event.key.length === 1 &&
    event.key !== 'Dead' &&
    !event.altKey &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.isComposing
  );
}

export function CommandPalette({
  commands,
  hotkeys,
  initialQuery = '',
  isInitialLoadResolved = true,
  isOpen,
  isPrewarm = false,
  onBrowserCommandRun,
  onOpenChange,
  openRequestSequence = 0,
  openTargetSettings,
  petOverlayEnabled = false,
  vscode,
}: CommandPaletteProps) {
  const [inputValue, setInputValue] = useState(initialQuery);
  const [selectedCommandValue, setSelectedCommandValue] = useState('');
  const commandListRef = useRef<HTMLDivElement>(null);
  const commandRunStates = useSidebarStore((state) => state.commandRunStates);
  /*
   * CDXC:DisabledPluginRouting 2026-08-23:
   * A view turned off in Settings → Customize is gone from the titlebar, and
   * the host refuses to switch to it, so its palette rows would be commands
   * that cannot run. Filter them here — the switchers for the hidden views,
   * plus the two Browser creators — instead of leaving dead rows that report
   * nothing when chosen.
   */
  const browserViewTabHidden = useSidebarStore(
    (state) =>
      (state.hud.settings?.browserViewTabHidden ??
        DEFAULT_ghostex_SETTINGS.browserViewTabHidden) === true
  );
  const codeViewTabHidden = useSidebarStore(
    (state) =>
      (state.hud.settings?.codeViewTabHidden ?? DEFAULT_ghostex_SETTINGS.codeViewTabHidden) === true
  );
  const docsViewTabHidden = useSidebarStore(
    (state) =>
      (state.hud.settings?.docsViewTabHidden ?? DEFAULT_ghostex_SETTINGS.docsViewTabHidden) === true
  );
  const kanbanViewTabHidden = useSidebarStore(
    (state) =>
      (state.hud.settings?.kanbanViewTabHidden ?? DEFAULT_ghostex_SETTINGS.kanbanViewTabHidden) ===
      true
  );
  const hiddenWorkareaCommandIds = useMemo(() => {
    const hidden = new Set<string>();
    if (browserViewTabHidden) {
      hidden.add('switchGitHubView');
      hidden.add('openBrowserPane');
      hidden.add('quickBrowserTab');
    }
    if (codeViewTabHidden) {
      hidden.add('switchSourceView');
    }
    if (docsViewTabHidden) {
      hidden.add('switchManageView');
    }
    if (kanbanViewTabHidden) {
      hidden.add('switchKanbanView');
    }
    return hidden;
  }, [browserViewTabHidden, codeViewTabHidden, docsViewTabHidden, kanbanViewTabHidden]);
  const normalizedHotkeys = useMemo(() => normalizeghostexHotkeySettings(hotkeys), [hotkeys]);
  const commandQuery = inputValue.trim();
  const createBuiltInCommand = (definition: ghostexHotkeyDefinition): HotkeyPaletteCommand => {
    const hotkey = normalizeHotkeyText(normalizedHotkeys[definition.id] ?? definition.defaultKey);
    return {
      definition,
      hotkey,
      kind: 'hotkey',
      searchText: `${definition.title} ${definition.description} ${hotkey}`,
      title: definition.title,
    };
  };
  const builtInCommands = useMemo(() => {
    const paneActionIds = new Set<ghostexHotkeyDefinition['id']>(PANE_ACTION_COMMAND_IDS);
    const hotkeyCommands: BuiltInPaletteCommand[] = GHOSTEX_HOTKEY_DEFINITIONS.filter(
      (definition) =>
        definition.id !== 'openCommandPalette' &&
        definition.id !== 'openSessionSearchPalette' &&
        definition.action.kind !== 'runActionSlot' &&
        !paneActionIds.has(definition.id) &&
        !hiddenWorkareaCommandIds.has(definition.id)
    ).map(createBuiltInCommand);
    const petTitle = petOverlayEnabled ? 'Sleep Pet' : 'Wake Pet';
    const petCommand: BuiltInPaletteCommand = {
      hotkey: '',
      kind: 'pet',
      searchText: `${petTitle} pet overlay ${petOverlayEnabled ? 'hide sleep' : 'show wake'}`,
      title: petTitle,
    };
    const openTargetCommands = createOpenTargetPaletteCommands(openTargetSettings);
    /*
     * CDXC:CommandPalette 2026-06-18-03:32:
     * Cmd+Shift+P must expose the global app-modal launchers users can reach
     * from sidebar and titlebar chrome, including Previous Sessions and the
     * Tips header actions Features, Setup, and Changelog.
     *
     * CDXC:CommandPalette 2026-06-18-03:46:
     * The palette also needs the main-window command buttons Add Project,
     * Search by Text, Quick Terminal, Quick Browser Tab, Automations, Open
     * Current Project in Finder, and visible Open In editor targets. Keep
     * context-dependent modals out of this list unless their required
     * session, draft, file, or target payload is available.
     */
    return [
      ...hotkeyCommands,
      ...APP_MODAL_PALETTE_COMMANDS,
      ...SIDEBAR_MESSAGE_PALETTE_COMMANDS.filter(
        (command) => !hiddenWorkareaCommandIds.has(command.commandId)
      ),
      ...openTargetCommands,
      petCommand,
    ];
  }, [hiddenWorkareaCommandIds, normalizedHotkeys, openTargetSettings, petOverlayEnabled]);
  const paneActionCommands = useMemo(() => {
    const definitionsById = new Map(GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => [definition.id, definition]));
    return PANE_ACTION_COMMAND_IDS.filter((id) => !hiddenWorkareaCommandIds.has(id))
      .map((id) => definitionsById.get(id))
      .filter((definition): definition is ghostexHotkeyDefinition => definition !== undefined)
      .map(createBuiltInCommand);
  }, [hiddenWorkareaCommandIds, normalizedHotkeys]);
  const projectCommands = useMemo(
    () =>
      commands
        .map((command, index): ProjectPaletteCommand => {
          const slotNumber = index + 1;
          const actionSlotId = getActionSlotHotkeyId(slotNumber);
          return {
            command,
            hotkey: actionSlotId ? normalizeHotkeyText(normalizedHotkeys[actionSlotId] ?? '') : '',
            slotNumber,
          };
        })
        .filter(({ command }) => isRunnableOrConfigurableCommand(command)),
    [commands, normalizedHotkeys]
  );
  const commandSearchItems = useMemo<CommandPaletteSearchItem[]>(
    /*
     * A query must rank every command in one population. Keeping the display
     * sections as separate filtered lists would let section order beat fuzzy
     * relevance, which is why Delayed Actions previously stayed below weak
     * Ghostex matches.
     */
    () => [
      ...builtInCommands.map((command) => ({
        command,
        kind: 'builtIn' as const,
        searchText: command.searchText,
      })),
      ...paneActionCommands.map((command) => ({
        command,
        kind: 'paneAction' as const,
        searchText: command.searchText,
      })),
      ...projectCommands.map((command) => ({
        command,
        kind: 'project' as const,
        searchText: getProjectCommandSearchText(command),
      })),
    ],
    [builtInCommands, paneActionCommands, projectCommands]
  );
  const filteredCommandResults = useMemo(
    () => filterCommandPaletteItems(commandSearchItems, commandQuery, (item) => item.searchText),
    [commandQuery, commandSearchItems]
  );
  const isSearchingCommands = commandQuery.length > 0;
  const hasCommandResults = isSearchingCommands
    ? filteredCommandResults.length > 0
    : commandSearchItems.length > 0;
  const topCommandValue = (
    isSearchingCommands ? filteredCommandResults[0] : commandSearchItems[0]
  )?.searchText ?? '';

  useLayoutEffect(() => {
    if (!isOpen || isPrewarm) {
      return;
    }
    /*
     * Filtering is React-owned because cmdk's built-in filter is disabled.
     * Reset cmdk's controlled selection and the list viewport whenever the
     * input changes so typing and deleting always target the newly ranked
     * first visible command instead of preserving a stale row or scroll offset.
     */
    setSelectedCommandValue(topCommandValue);
    if (commandListRef.current) {
      commandListRef.current.scrollTop = 0;
    }
  }, [inputValue, isOpen, isPrewarm, topCommandValue]);

  const focusCommandPaletteInput = () => {
    const input = findCommandPaletteInput();
    input?.focus();
    return input;
  };

  const insertIntoCommandPaletteInput = (text: string) => {
    if (text.length === 0) {
      return;
    }
    const input = focusCommandPaletteInput();
    if (!input) {
      return;
    }
    const selectionStart = input.selectionStart ?? input.value.length;
    const selectionEnd = input.selectionEnd ?? input.value.length;
    const nextValue = input.value.slice(0, selectionStart) + text + input.value.slice(selectionEnd);
    const nextSelection = selectionStart + text.length;
    setInputValue(nextValue);
    window.requestAnimationFrame(() => {
      const focusedInput = focusCommandPaletteInput();
      focusedInput?.setSelectionRange(nextSelection, nextSelection);
    });
  };

  useLayoutEffect(() => {
    if (!isOpen || isPrewarm) {
      return;
    }
    /*
     * CDXC:CommandPalette 2026-06-16-19:24:
     * When the native macOS command-palette child window is open, every plain
     * text input should target the palette search field. Focus the field after
     * each visible open request and after WebKit/AppKit focus handoffs so a
     * visible palette never leaves typing behind on the terminal or dialog body.
     */
    focusCommandPaletteInput();
    const animationFrameId = window.requestAnimationFrame(focusCommandPaletteInput);
    const timeoutIds = [0, 50, 150].map((delay) => window.setTimeout(focusCommandPaletteInput, delay));
    return () => {
      window.cancelAnimationFrame(animationFrameId);
      for (const timeoutId of timeoutIds) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [isOpen, isPrewarm, openRequestSequence]);

  useEffect(() => {
    if (!isOpen || isPrewarm) {
      return;
    }

    const focusAfterCurrentEvent = () => {
      window.setTimeout(focusCommandPaletteInput, 0);
    };
    const handlePaletteKeyDown = (event: KeyboardEvent) => {
      const input = findCommandPaletteInput();
      if (!input || document.activeElement === input) {
        return;
      }
      focusCommandPaletteInput();
      if (!isCommandPaletteTextKey(event)) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      insertIntoCommandPaletteInput(event.key);
    };
    const handlePalettePaste = (event: ClipboardEvent) => {
      const input = findCommandPaletteInput();
      if (!input || document.activeElement === input) {
        return;
      }
      const text = event.clipboardData?.getData('text') ?? '';
      if (!text) {
        focusCommandPaletteInput();
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      insertIntoCommandPaletteInput(text);
    };

    window.addEventListener('focus', focusAfterCurrentEvent);
    window.addEventListener('focusin', focusAfterCurrentEvent);
    window.addEventListener('keydown', handlePaletteKeyDown, { capture: true });
    document.addEventListener('paste', handlePalettePaste, { capture: true });
    return () => {
      window.removeEventListener('focus', focusAfterCurrentEvent);
      window.removeEventListener('focusin', focusAfterCurrentEvent);
      window.removeEventListener('keydown', handlePaletteKeyDown, { capture: true });
      document.removeEventListener('paste', handlePalettePaste, { capture: true });
    };
  }, [isOpen, isPrewarm]);

  useLayoutEffect(() => {
    /*
     * Reset the previous open's query before paint. A passive effect allowed a
     * stale no-result query to flash "No commands found" while the reopened
     * palette was already visible.
     */
    setInputValue(initialQuery);
  }, [initialQuery, isOpen, openRequestSequence]);

  const runBuiltInCommand = (command: BuiltInPaletteCommand) => {
    if (command.kind === 'pet') {
      onOpenChange(false);
      vscode.postMessage({
        type: 'togglePetOverlay',
      });
      return;
    }
    if (command.kind === 'appModal') {
      if (command.modal === 'previousSessions') {
        openQuickAccess('recentSessions');
        return;
      }
      onOpenChange(false);
      openAppModal({ modal: command.modal, type: 'open' });
      return;
    }
    if (command.kind === 'sidebarMessage') {
      onOpenChange(false);
      vscode.postMessage(command.message);
      return;
    }
    if (command.kind === 'openTarget') {
      onOpenChange(false);
      vscode.postMessage({
        targetId: command.targetId,
        type: 'openCurrentProjectInTarget',
      });
      return;
    }
    /*
     * Delayed Send replaces the command palette inside the same native modal
     * host. Closing that host before posting the focused-session action races
     * the replacement open and can dismiss the newly opened timer modal.
     * Keep this one transition in-place; the native owner validates the
     * focused terminal and posts the delayedSend open message back to this
     * React host.
     */
    if (command.definition.id !== 'delayedSend') {
      onOpenChange(false);
    }
    vscode.postMessage({
      actionId: command.definition.id,
      type: 'runGhostexHotkeyAction',
    });
  };

  const runProjectCommand = (command: SidebarCommandButton) => {
    if (!isConfigured(command)) {
      /*
      CDXC:CommandPalette 2026-06-15-15:29:
      Selecting an unconfigured project action should take users to Settings > Actions, where the reusable command list now owns action setup instead of a standalone Configure Action modal.
      */
      onOpenChange(false);
      openAppModal({
        initialTab: 'actions',
        modal: 'settings',
        type: 'open',
      });
      return;
    }

    if (command.actionType === 'browser') {
      onBrowserCommandRun?.();
    }
    /*
    CDXC:GPUICommandPane 2026-06-26-05:11:
    Command Palette Action launches may read saved command metadata to derive the click runMode, including debug reruns for close-on-exit terminal Actions. The runSidebarCommand payload stays an authority selector: commandId plus non-default runMode only. Native and GPUI hosts resolve command text, URLs, close-on-exit, cwd/env, paths, output, and other launch details from trusted saved/HUD state.
    */
    const runMode = getSidebarCommandRunModeForClick(command, commandRunStates[command.commandId]);
    onOpenChange(false);
    vscode.postMessage({
      commandId: command.commandId,
      ...(runMode === 'default' ? {} : { runMode }),
      type: 'runSidebarCommand',
    });
  };

  return (
    <CommandDialog
      className='ghostex-settings-shadcn ghostex-command-palette-dialog top-1/2 -translate-y-1/2'
      description='Search Ghostex commands and project actions.'
      open={isOpen}
      showCloseButton={false}
      title='Ghostex Quick Access'
      onOpenChange={onOpenChange}
    >
      {/* CDXC:CommandPalette 2026-06-13-10:26:
          Cmd+Shift+P opens a shadcn Base-style command palette that lists the
          current Ghostex hotkey actions plus the project Actions available
          from the active sidebar context. Hotkeys are right-aligned with
          CommandShortcut so discoverability stays inside the command surface.

          CDXC:CommandPalette 2026-05-16-08:18:
          The palette should not list itself as a command, Ghostex built-ins
          and project actions should be single-line rows without descriptions,
          and the pet row must reflect the current wake/sleep state before
          routing through the shared settings-owned pet toggle.

          CDXC:CommandPalette 2026-05-16-13:04:
          Command rows without assigned shortcuts should leave the right edge
          blank instead of showing "No hotkey" placeholder text so the palette
          only surfaces concrete accelerators.

          CDXC:ActionsHotkeys 2026-05-17-01:18:
          Project actions must stay in the same order as the Actions settings
          list. The first five rows display and execute positional action-slot
          hotkeys, so reordering actions changes which command Ctrl+Shift+N
          starts without changing the stored hotkey ids.

          CDXC:CommandPalette 2026-05-17-01:32:
          Focused pane-menu commands should appear together in the command
          palette, matching the pane menu order shown in native chrome while
          still using shared configurable hotkey definitions.

          CDXC:FocusedSessionActions 2026-06-19-15:43:
          Sleep, Wake, Close, and Close After Done are focused-session commands
          even when only Sleep has a default shortcut. Keep them in the Pane
          Actions group so users can run them from the palette and bind them in
          Hotkeys without needing a sidebar row context.

          */}
      <Command
        className='quick-access-surface'
        shouldFilter={false}
        value={selectedCommandValue}
        onValueChange={setSelectedCommandValue}
      >
        <QuickAccessHeader activeTab='commands' />
        {/*
         * CDXC:CommandPalette 2026-06-11-09:14:
         * CommandInput sits inside InputGroup without an inline-start addon, so
         * add pl-3 so the query text aligns with command-row icons below.
         *
         * Ghostex Quick Access gives commands and recent sessions separate
         * tabs, so this field is always a normal command query. Characters such
         * as `>` have no mode-switch behavior.
         *
         * CDXC:CommandPalette 2026-06-15-16:21:
         * Escape while the command palette is shown must always close the
         * palette. Do not let the shared CommandInput clear the query first;
         * close the modal directly from the palette-owned key handler.
         */}
        <CommandInput
          className='pl-3'
          clearOnEscape={false}
          clearLabel='Clear command palette search'
          data-ghostex-command-palette-input='true'
          onKeyDown={(event) => {
            if (event.key !== 'Escape') {
              return;
            }

            event.preventDefault();
            event.stopPropagation();
            onOpenChange(false);
          }}
          placeholder='Search commands...'
          value={inputValue}
          onValueChange={setInputValue}
        />
        <CommandList className='ghostex-command-palette-list' ref={commandListRef}>
          {!hasCommandResults ? (
            <CommandEmpty>{isInitialLoadResolved ? 'No commands found.' : 'Loading commands…'}</CommandEmpty>
          ) : null}
          {isSearchingCommands && filteredCommandResults.length > 0 ? (
            <CommandGroup heading='Results'>
              {filteredCommandResults.map((result) => {
                if (result.kind === 'builtIn') {
                  return (
                    <BuiltInCommandRow
                      command={result.command}
                      key={`builtIn:${getBuiltInCommandKey(result.command)}`}
                      onRun={runBuiltInCommand}
                    />
                  );
                }
                if (result.kind === 'paneAction') {
                  return (
                    <BuiltInCommandRow
                      command={result.command}
                      key={`paneAction:${result.command.definition.id}`}
                      onRun={runBuiltInCommand}
                    />
                  );
                }
                return (
                  <ProjectCommandRow
                    item={result.command}
                    key={`project:${result.command.command.commandId}`}
                    onRun={runProjectCommand}
                  />
                );
              })}
            </CommandGroup>
          ) : null}
          {!isSearchingCommands && builtInCommands.length > 0 ? (
            <CommandGroup heading='Ghostex'>
              {builtInCommands.map((command) => (
                <BuiltInCommandRow
                  command={command}
                  key={getBuiltInCommandKey(command)}
                  onRun={runBuiltInCommand}
                />
              ))}
            </CommandGroup>
          ) : null}
          {!isSearchingCommands && paneActionCommands.length > 0 ? (
            <>
              {builtInCommands.length > 0 ? <CommandSeparator /> : null}
              <CommandGroup heading='Pane Actions'>
                {paneActionCommands.map((command) => (
                  <BuiltInCommandRow
                    command={command}
                    key={command.definition.id}
                    onRun={runBuiltInCommand}
                  />
                ))}
              </CommandGroup>
            </>
          ) : null}
          {!isSearchingCommands && projectCommands.length > 0 ? (
            <>
              {builtInCommands.length > 0 || paneActionCommands.length > 0 ? (
                <CommandSeparator />
              ) : null}
              <CommandGroup heading='Project Actions'>
                {projectCommands.map((item) => (
                  <ProjectCommandRow
                    item={item}
                    key={item.command.commandId}
                    onRun={runProjectCommand}
                  />
                ))}
              </CommandGroup>
            </>
          ) : null}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}

function BuiltInCommandRow({
  command,
  onRun,
}: {
  command: BuiltInPaletteCommand;
  onRun: (command: BuiltInPaletteCommand) => void;
}) {
  return (
    <CommandItem value={command.searchText} onSelect={() => onRun(command)}>
      <BuiltInCommandIcon command={command} />
      <span className='ghostex-command-palette-copy'>
        <span className='ghostex-command-palette-title'>{command.title}</span>
      </span>
      {command.hotkey ? <CommandShortcut>{formatSidebarHotkeyLabel(command.hotkey)}</CommandShortcut> : null}
    </CommandItem>
  );
}

function ProjectCommandRow({
  item,
  onRun,
}: {
  item: ProjectPaletteCommand;
  onRun: (command: SidebarCommandButton) => void;
}) {
  const { command, hotkey } = item;
  return (
    <CommandItem value={getProjectCommandSearchText(item)} onSelect={() => onRun(command)}>
      <SidebarCommandIconGlyph icon={command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON} stroke={1.8} />
      <span className='ghostex-command-palette-copy'>
        <span className='ghostex-command-palette-title'>{getCommandTitle(command)}</span>
      </span>
      {hotkey ? <CommandShortcut>{formatSidebarHotkeyLabel(hotkey)}</CommandShortcut> : null}
    </CommandItem>
  );
}

function BuiltInCommandIcon({ command }: { command: BuiltInPaletteCommand }) {
  if (command.kind === 'appModal') {
    return <AppModalCommandIcon modal={command.modal} />;
  }
  if (command.kind === 'sidebarMessage') {
    return <SidebarMessageCommandIcon commandId={command.commandId} />;
  }
  if (command.kind === 'openTarget') {
    return <IconExternalLink aria-hidden='true' />;
  }
  if (command.kind === 'pet') {
    return command.title === 'Sleep Pet' ? <IconMoon aria-hidden='true' /> : <IconPlayerPlay aria-hidden='true' />;
  }

  const action = command.definition.action;
  if (action.kind === 'createSession') {
    return <IconPlus aria-hidden='true' />;
  }
  if (action.kind === 'openCommandsPanel') {
    return <IconTerminal2 aria-hidden='true' />;
  }
  if (action.kind === 'openSettings') {
    return <IconSettings aria-hidden='true' />;
  }
  if (action.kind === 'openHotkeys') {
    return <IconKeyboard aria-hidden='true' />;
  }
  if (action.kind === 'moveSidebar') {
    return <IconLayoutSidebarRightExpand aria-hidden='true' />;
  }
  if (action.kind === 'toggleSidebarCollapsed') {
    return <IconLayoutSidebar aria-hidden='true' />;
  }
  if (action.kind === 'toggleCompanionPane') {
    return <IconLayoutSidebarRightExpand aria-hidden='true' />;
  }
  if (action.kind === 'renameActiveSession') {
    return <IconEdit aria-hidden='true' />;
  }
  if (action.kind === 'focusedPaneAction') {
    return <FocusedPaneCommandIcon action={action.focusedPaneAction} />;
  }
  if (action.kind === 'focusAdjacentGroup') {
    return action.direction < 0 ? <IconChevronLeft aria-hidden='true' /> : <IconChevronRight aria-hidden='true' />;
  }
  if (action.kind === 'focusDirection') {
    return getFocusDirectionIcon(action.direction);
  }
  if (action.kind === 'splitFocusedPane') {
    return <IconArrowsDiagonal2 aria-hidden='true' />;
  }
  if (action.kind === 'setViewMode') {
    return <IconLayoutDashboard aria-hidden='true' />;
  }
  return <IconKeyboard aria-hidden='true' />;
}

function AppModalCommandIcon({ modal }: { modal: AppModalPaletteModal }) {
  if (modal === 'previousSessions') {
    return <IconHistory aria-hidden='true' />;
  }
  if (modal === 'pinnedPrompts') {
    return <IconPinned aria-hidden='true' />;
  }
  if (modal === 'daemonSessions') {
    return <IconServer aria-hidden='true' />;
  }
  if (modal === 'scratchPad') {
    return <IconNotebook aria-hidden='true' />;
  }
  if (modal === 'agentsHub' || modal === 'configureAgents') {
    return <IconSettingsAutomation aria-hidden='true' />;
  }
  if (modal === 'configureActions') {
    return <IconListDetails aria-hidden='true' />;
  }
  if (modal === 'openTargets') {
    return <IconExternalLink aria-hidden='true' />;
  }
  if (modal === 'addProject') {
    return <IconFolderPlus aria-hidden='true' />;
  }
  return <IconKeyboard aria-hidden='true' />;
}

function SidebarMessageCommandIcon({ commandId }: { commandId: SidebarMessagePaletteCommandId }) {
  if (commandId === 'searchByText') {
    return <IconSearch aria-hidden='true' />;
  }
  if (commandId === 'quickTerminal') {
    return <IconTerminal2 aria-hidden='true' />;
  }
  if (commandId === 'quickBrowserTab') {
    return <IconBrowser aria-hidden='true' />;
  }
  if (commandId === 'automations') {
    return <IconSettingsAutomation aria-hidden='true' />;
  }
  if (commandId === 'plugins') {
    return <IconSettings aria-hidden='true' />;
  }
  if (commandId === 'openCurrentProjectInFinder') {
    return <IconFolderOpen aria-hidden='true' />;
  }
  if (commandId === 'features') {
    return <IconStars aria-hidden='true' />;
  }
  if (commandId === 'tutorialVideo') {
    return <IconPlayerPlay aria-hidden='true' />;
  }
  if (commandId === 'setupGhostex') {
    return <IconChecklist aria-hidden='true' />;
  }
  return <IconBrandGithub aria-hidden='true' />;
}

function getBuiltInCommandKey(command: BuiltInPaletteCommand): string {
  if (command.kind === 'hotkey') {
    return command.definition.id;
  }
  if (command.kind === 'appModal' || command.kind === 'sidebarMessage') {
    return command.commandId;
  }
  if (command.kind === 'openTarget') {
    return command.commandId;
  }
  return command.kind;
}

function FocusedPaneCommandIcon({ action }: { action: ghostexFocusedPaneAction }) {
  if (action === 'openBrowserPane') {
    return <IconBrowser aria-hidden='true' />;
  }
  if (action === 'rotatePanesClockwise') {
    return <IconRotateClockwise aria-hidden='true' />;
  }
  if (action === 'mergeAllTabs') {
    return <IconWindowMaximize aria-hidden='true' />;
  }
  if (action === 'delayedSend') {
    return <IconClock aria-hidden='true' />;
  }
  if (action === 'closeAfterDone') {
    return <IconClock aria-hidden='true' />;
  }
  if (action === 'forkSession') {
    return <IconGitFork aria-hidden='true' />;
  }
  if (action === 'reloadSession') {
    return <IconRefresh aria-hidden='true' />;
  }
  if (action === 'sleepFocusedSession') {
    return <IconMoon aria-hidden='true' />;
  }
  if (action === 'wakeFocusedSession') {
    return <IconPlayerPlay aria-hidden='true' />;
  }
  if (action === 'closeFocusedSession') {
    return <IconX aria-hidden='true' />;
  }
  if (action === 'popOutPane') {
    return <IconExternalLink aria-hidden='true' />;
  }
  return <IconLayoutSidebarRightExpand aria-hidden='true' />;
}

function getFocusDirectionIcon(direction: 'down' | 'left' | 'right' | 'up') {
  if (direction === 'up') {
    return <IconChevronUp aria-hidden='true' />;
  }
  if (direction === 'right') {
    return <IconArrowRight aria-hidden='true' />;
  }
  if (direction === 'down') {
    return <IconChevronDown aria-hidden='true' />;
  }
  return <IconArrowLeft aria-hidden='true' />;
}

function getActionSlotHotkeyId(slotNumber: number): ghostexHotkeyDefinition['id'] | undefined {
  if (slotNumber < 1 || slotNumber > 5) {
    return undefined;
  }
  return `runActionSlot${slotNumber}` as ghostexHotkeyDefinition['id'];
}

function isRunnableOrConfigurableCommand(command: SidebarCommandButton): boolean {
  return command.name.trim().length > 0 || command.icon !== undefined;
}

function isConfigured(command: SidebarCommandButton): boolean {
  return command.actionType === 'browser' ? Boolean(command.url) : Boolean(command.command);
}

function getCommandTitle(command: SidebarCommandButton): string {
  const name = command.name.trim();
  if (name) {
    return name;
  }
  return command.actionType === 'browser' ? 'Untitled Webpage' : 'Untitled Action';
}

function getCommandDescription(command: SidebarCommandButton): string {
  const target = getCommandTarget(command);
  const typeLabel = command.actionType === 'browser' ? 'Browser' : 'Terminal';
  if (!target) {
    return `${typeLabel} - Not configured`;
  }
  return `${typeLabel} - ${target}`;
}

function getProjectCommandSearchText({
  command,
  hotkey,
  slotNumber,
}: ProjectPaletteCommand): string {
  return `${getCommandTitle(command)} ${getCommandDescription(command)} ${hotkey} action ${slotNumber}`;
}

function getCommandTarget(command: SidebarCommandButton): string | undefined {
  const target = command.actionType === 'browser' ? command.url?.trim() : command.command?.trim();
  if (!target) {
    return undefined;
  }
  return target.split('\n')[0] || undefined;
}
