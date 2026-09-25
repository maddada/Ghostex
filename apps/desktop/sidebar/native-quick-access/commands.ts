/**
 * The Commands tab: the same command population, ranking, grouping and rows as
 * packages/core-ui/command-palette.tsx, projected onto the native snapshot.
 */
import { filterCommandPaletteItems } from '@/packages/core-ui/command-palette-session-search';
import { getSidebarCommandRunModeForClick } from '@/packages/core-ui/command-run-feedback';
import { formatSidebarHotkeyLabel } from '@/packages/core-ui/hotkey-label';
import { openAppModal, openQuickAccess } from '@/packages/core-ui/app-modal-host-bridge';
import { sidebarStore } from '@/packages/core-ui/sidebar-store-model';
import { DEFAULT_SIDEBAR_COMMAND_ICON } from '@/packages/shared/sidebar-command-icons';
import { DEFAULT_ghostex_SETTINGS } from '@/packages/shared/ghostex-settings';
import {
  ghostexViewScope,
  isViewScopeVisible,
  officialViewScopeKey,
} from '@/packages/shared/ghostex-settings/view-scopes';
import {
  GHOSTEX_HOTKEY_DEFINITIONS,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexHotkeyDefinition,
} from '@/packages/shared/ghostex-hotkeys';
import type { SidebarCommandButton } from '@/packages/shared/sidebar-commands';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import { BUILT_IN_WORKSPACE_OPEN_TARGETS } from '@/packages/shared/workspace-open-targets';
import type { QuickAccessGroup, QuickAccessRow } from '@/packages/shared/native-quick-access';
import { appModalIconName, assetIcon, hotkeyActionIconName, sidebarMessageIconName } from './icons';

const GHOSTEX_CHANGELOG_URL = 'https://github.com/maddada/ghostex/releases';

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

type PaletteCommand =
  | { kind: 'hotkey'; definition: ghostexHotkeyDefinition; hotkey: string; searchText: string; title: string }
  | { kind: 'appModal'; commandId: string; modal: string; searchText: string; title: string }
  | { kind: 'sidebarMessage'; commandId: string; message: SidebarToExtensionMessage; searchText: string; title: string }
  | { kind: 'openTarget'; commandId: string; targetId: string; searchText: string; title: string }
  | { kind: 'pet'; searchText: string; title: string }
  | { kind: 'project'; command: SidebarCommandButton; hotkey: string; slotNumber: number; searchText: string };

const APP_MODAL_PALETTE_COMMANDS: PaletteCommand[] = [
  {
    kind: 'appModal',
    commandId: 'previousSessions',
    modal: 'previousSessions',
    searchText: 'Reopen a Session history restore previous sessions old sessions',
    title: 'Reopen a Session',
  },
  {
    kind: 'appModal',
    commandId: 'agentsHub',
    modal: 'agentsHub',
    searchText: 'Agents Hub agents profiles skills prompts modal',
    title: 'Agents Hub',
  },
  {
    kind: 'appModal',
    commandId: 'configureAgents',
    modal: 'configureAgents',
    searchText: 'Configure Agents agents settings modal',
    title: 'Configure Agents',
  },
  {
    kind: 'appModal',
    commandId: 'actions',
    modal: 'configureActions',
    searchText: 'Actions configure project actions settings modal',
    title: 'Actions',
  },
  {
    kind: 'appModal',
    commandId: 'openTargets',
    modal: 'openTargets',
    searchText: 'Open Targets open in editors settings modal',
    title: 'Open Targets',
  },
  {
    kind: 'appModal',
    commandId: 'addProject',
    modal: 'addProject',
    searchText: 'Add Project add folder workspace clone repository projects',
    title: 'Add Project',
  },
];

const SIDEBAR_MESSAGE_PALETTE_COMMANDS: PaletteCommand[] = [
  {
    kind: 'sidebarMessage',
    commandId: 'quickTerminal',
    message: { type: 'createChat' },
    searchText: 'Quick Terminal new chat terminal',
    title: 'Quick Terminal',
  },
  {
    kind: 'sidebarMessage',
    commandId: 'quickBrowserTab',
    message: { type: 'openBrowserChat' },
    searchText: 'Quick Browser Tab browser chat',
    title: 'Quick Browser Tab',
  },
  {
    kind: 'sidebarMessage',
    commandId: 'automations',
    message: { type: 'openAutomationsPage' },
    searchText: 'All Automations schedules agents timers dates recurring',
    title: 'All Automations',
  },
  {
    kind: 'sidebarMessage',
    commandId: 'searchByText',
    message: { actionId: 'openFindPrompts', type: 'runGhostexHotkeyAction' },
    searchText: 'Search by Text Find Prompts previous sessions history gx f',
    title: 'Find Prompts',
  },
  {
    kind: 'sidebarMessage',
    commandId: 'extensions',
    message: { actionId: 'openExtensions', type: 'runGhostexHotkeyAction' },
    searchText:
      'Extensions store installed add-ons official built-in features components VS Code code-server CEF gxserver Beads bd runtimes',
    title: 'Extensions',
  },
  {
    kind: 'sidebarMessage',
    commandId: 'openCurrentProjectInFinder',
    message: { type: 'openCurrentProjectInFinder' },
    searchText: 'Open File/Folder Location current project open folder workspace',
    title: 'Open File/Folder Location',
  },
  {
    kind: 'sidebarMessage',
    commandId: 'setupGhostex',
    message: { type: 'openWorkspaceWelcome' },
    searchText: 'Ghostex setup onboarding first launch guide modal',
    title: 'Setup',
  },
  {
    kind: 'sidebarMessage',
    commandId: 'changelog',
    message: { type: 'openBrowserPane', url: GHOSTEX_CHANGELOG_URL },
    searchText: 'Changelog release notes releases github browser',
    title: 'Changelog',
  },
] as PaletteCommand[];

function hiddenWorkareaCommandIds(): Set<string> {
  const state = sidebarStore.getState();
  const settings = state.hud.settings;
  const viewScopes = settings?.viewScopes;
  const outOfScope = (officialExtensionId: string) =>
    !isViewScopeVisible({
      projectId: state.hud.activeProjectId,
      projectSpaceRefs: state.hud.activeProjectSpaceRefs ?? [],
      scope: ghostexViewScope(viewScopes, officialViewScopeKey(officialExtensionId)),
    });
  const flag = (
    key:
      | 'browserViewTabHidden'
      | 'codeViewTabHidden'
      | 'docsViewTabHidden'
      | 'kanbanViewTabHidden'
      | 'terminalViewTabHidden'
  ) => (settings?.[key] ?? DEFAULT_ghostex_SETTINGS[key]) === true;
  const hidden = new Set<string>();
  if (flag('browserViewTabHidden') || outOfScope('browser')) {
    hidden.add('switchGitHubView');
    hidden.add('openBrowserPane');
    hidden.add('quickBrowserTab');
  }
  if (flag('codeViewTabHidden') || outOfScope('code')) hidden.add('switchSourceView');
  if (flag('docsViewTabHidden') || outOfScope('docs')) hidden.add('switchManageView');
  if (flag('kanbanViewTabHidden') || outOfScope('kanban')) hidden.add('switchKanbanView');
  if (flag('terminalViewTabHidden') || outOfScope('terminal')) hidden.add('switchTerminalView');
  return hidden;
}

function openTargetCommands(): PaletteCommand[] {
  const settings = sidebarStore.getState().hud.settings;
  if (!settings) return [];
  const hiddenTargetIds = new Set(settings.workspaceOpenTargetHiddenIds ?? []);
  const availableTargetIds = new Set(settings.workspaceOpenTargetAvailability?.availableTargetIds ?? []);
  const builtIn = BUILT_IN_WORKSPACE_OPEN_TARGETS.filter(
    (target) => target.id !== 'finder' && !hiddenTargetIds.has(target.id) && availableTargetIds.has(target.id)
  ).map((target): PaletteCommand => ({
    kind: 'openTarget',
    commandId: `openTarget:${target.id}`,
    targetId: target.id,
    searchText: `Open In ${target.label} current project workspace editor target`,
    title: `Open In: ${target.label}`,
  }));
  const custom = (settings.customWorkspaceOpenTargets ?? []).map((target): PaletteCommand => ({
    kind: 'openTarget',
    commandId: `openTarget:${target.id}`,
    targetId: target.id,
    searchText: `Open In ${target.label} current project workspace custom target`,
    title: `Open In: ${target.label}`,
  }));
  return [...builtIn, ...custom];
}

function actionSlotHotkeyId(slotNumber: number): ghostexHotkeyDefinition['id'] | undefined {
  if (slotNumber < 1 || slotNumber > 5) return undefined;
  return `runActionSlot${slotNumber}` as ghostexHotkeyDefinition['id'];
}

function isRunnableOrConfigurableCommand(command: SidebarCommandButton): boolean {
  return commandName(command).length > 0 || command.icon !== undefined;
}

/**
 * The saved Action's name. Unlike the React palette this list builds each row's
 * search text before filtering, so a slot saved without a name must read as
 * empty here rather than throwing and taking the whole tab down with it.
 */
function commandName(command: SidebarCommandButton): string {
  return typeof command.name === 'string' ? command.name.trim() : '';
}

function isConfigured(command: SidebarCommandButton): boolean {
  return command.actionType === 'browser' ? Boolean(command.url) : Boolean(command.command);
}

function commandTitle(command: SidebarCommandButton): string {
  const name = commandName(command);
  if (name) return name;
  return command.actionType === 'browser' ? 'Untitled Webpage' : 'Untitled Action';
}

function commandTarget(command: SidebarCommandButton): string | undefined {
  const target = command.actionType === 'browser' ? command.url?.trim() : command.command?.trim();
  if (!target) return undefined;
  return target.split('\n')[0] || undefined;
}

function commandDescription(command: SidebarCommandButton): string {
  const target = commandTarget(command);
  const typeLabel = command.actionType === 'browser' ? 'Browser' : 'Terminal';
  return target ? `${typeLabel} - ${target}` : `${typeLabel} - Not configured`;
}

/** The three command populations, in the order the React palette lists them. */
export function quickAccessCommandPopulations(petOverlayEnabled: boolean): {
  builtIn: PaletteCommand[];
  paneActions: PaletteCommand[];
  projectActions: PaletteCommand[];
} {
  const state = sidebarStore.getState();
  const hotkeys = normalizeghostexHotkeySettings(state.hud.settings?.hotkeys);
  const hidden = hiddenWorkareaCommandIds();
  const paneActionIds = new Set<string>(PANE_ACTION_COMMAND_IDS);
  const toHotkeyCommand = (definition: ghostexHotkeyDefinition): PaletteCommand => {
    const hotkey = normalizeHotkeyText(hotkeys[definition.id] ?? definition.defaultKey);
    return {
      kind: 'hotkey',
      definition,
      hotkey,
      searchText: `${definition.title} ${definition.description} ${hotkey}`,
      title: definition.title,
    };
  };
  const hotkeyCommands = GHOSTEX_HOTKEY_DEFINITIONS.filter(
    (definition) =>
      definition.id !== 'openCommandPalette' &&
      definition.id !== 'openSessionSearchPalette' &&
      definition.id !== 'openProjectSearchPalette' &&
      definition.id !== 'openExtensions' &&
      definition.action.kind !== 'runActionSlot' &&
      definition.action.kind !== 'chatAction' &&
      !paneActionIds.has(definition.id) &&
      !hidden.has(definition.id)
  ).map(toHotkeyCommand);
  const petTitle = petOverlayEnabled ? 'Sleep Pet' : 'Wake Pet';
  const builtIn: PaletteCommand[] = [
    ...hotkeyCommands,
    ...APP_MODAL_PALETTE_COMMANDS,
    ...SIDEBAR_MESSAGE_PALETTE_COMMANDS.filter(
      (command) => command.kind !== 'sidebarMessage' || !hidden.has(command.commandId)
    ),
    ...openTargetCommands(),
    {
      kind: 'pet',
      searchText: `${petTitle} pet overlay ${petOverlayEnabled ? 'hide sleep' : 'show wake'}`,
      title: petTitle,
    },
  ];
  const definitionsById = new Map(GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => [definition.id, definition]));
  const paneActions = PANE_ACTION_COMMAND_IDS.filter((id) => !hidden.has(id))
    .map((id) => definitionsById.get(id))
    .filter((definition): definition is ghostexHotkeyDefinition => definition !== undefined)
    .map(toHotkeyCommand);
  const projectActions = (state.hud.commands ?? [])
    .map((command, index): PaletteCommand => {
      const slotNumber = index + 1;
      const actionSlotId = actionSlotHotkeyId(slotNumber);
      const hotkey = actionSlotId ? normalizeHotkeyText(hotkeys[actionSlotId] ?? '') : '';
      return {
        kind: 'project',
        command,
        hotkey,
        slotNumber,
        searchText: `${commandTitle(command)} ${commandDescription(command)} ${hotkey} action ${slotNumber}`,
      };
    })
    .filter((item) => item.kind === 'project' && isRunnableOrConfigurableCommand(item.command));
  return { builtIn, paneActions, projectActions };
}

export function commandRowKey(command: PaletteCommand): string {
  switch (command.kind) {
    case 'hotkey':
      return `hotkey:${command.definition.id}`;
    case 'appModal':
    case 'sidebarMessage':
    case 'openTarget':
      return `${command.kind}:${command.commandId}`;
    case 'project':
      return `project:${command.command.commandId}`;
    default:
      return 'pet';
  }
}

function commandRow(command: PaletteCommand): QuickAccessRow {
  if (command.kind === 'project') {
    return {
      kind: 'command',
      key: commandRowKey(command),
      title: commandTitle(command.command),
      icon: assetIcon(command.command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON),
      hotkey: command.hotkey ? formatSidebarHotkeyLabel(command.hotkey) : '',
    };
  }
  const iconName =
    command.kind === 'hotkey'
      ? hotkeyActionIconName(command.definition.action)
      : command.kind === 'appModal'
        ? appModalIconName(command.modal)
        : command.kind === 'sidebarMessage'
          ? sidebarMessageIconName(command.commandId)
          : command.kind === 'openTarget'
            ? 'external-link'
            : command.title === 'Sleep Pet'
              ? 'moon'
              : 'player-play';
  return {
    kind: 'command',
    key: commandRowKey(command),
    title: command.title,
    icon: assetIcon(iconName),
    hotkey: command.kind === 'hotkey' && command.hotkey ? formatSidebarHotkeyLabel(command.hotkey) : '',
  };
}

/**
 * The Commands list: one ranked "Results" group while searching, and the three
 * hairline-separated sections at rest.
 */
export function buildCommandGroups(query: string, petOverlayEnabled: boolean): QuickAccessGroup[] {
  const { builtIn, paneActions, projectActions } = quickAccessCommandPopulations(petOverlayEnabled);
  const all = [...builtIn, ...paneActions, ...projectActions];
  const trimmed = query.trim();
  if (trimmed) {
    const results = filterCommandPaletteItems(all, trimmed, (command) => command.searchText);
    return results.length > 0
      ? [{ key: 'results', heading: 'Results', separated: false, rows: results.map(commandRow) }]
      : [];
  }
  const groups: QuickAccessGroup[] = [];
  if (builtIn.length > 0) {
    groups.push({ key: 'ghostex', heading: 'Ghostex', separated: false, rows: builtIn.map(commandRow) });
  }
  if (paneActions.length > 0) {
    groups.push({
      key: 'paneActions',
      heading: 'Pane Actions',
      separated: groups.length > 0,
      rows: paneActions.map(commandRow),
    });
  }
  if (projectActions.length > 0) {
    groups.push({
      key: 'projectActions',
      heading: 'Project Actions',
      separated: groups.length > 0,
      rows: projectActions.map(commandRow),
    });
  }
  return groups;
}

/**
 * Runs the row `key` names, with the React palette's exact routing: Reopen a
 * Session re-targets Quick Access, Delayed Send keeps the window open while the
 * native owner replaces it, and everything else closes first.
 */
export function runCommandRow(
  key: string,
  petOverlayEnabled: boolean,
  post: (message: SidebarToExtensionMessage) => void,
  close: () => void
): void {
  const { builtIn, paneActions, projectActions } = quickAccessCommandPopulations(petOverlayEnabled);
  const command = [...builtIn, ...paneActions, ...projectActions].find((item) => commandRowKey(item) === key);
  if (!command) return;
  if (command.kind === 'pet') {
    close();
    post({ type: 'togglePetOverlay' } as SidebarToExtensionMessage);
    return;
  }
  if (command.kind === 'appModal') {
    if (command.modal === 'previousSessions') {
      openQuickAccess('recentSessions');
      return;
    }
    close();
    openAppModal({ modal: command.modal, type: 'open' } as Parameters<typeof openAppModal>[0]);
    return;
  }
  if (command.kind === 'sidebarMessage') {
    close();
    post(command.message);
    return;
  }
  if (command.kind === 'openTarget') {
    close();
    post({ targetId: command.targetId, type: 'openCurrentProjectInTarget' } as SidebarToExtensionMessage);
    return;
  }
  if (command.kind === 'project') {
    if (!isConfigured(command.command)) {
      close();
      openAppModal({ initialTab: 'actions', modal: 'settings', type: 'open' });
      return;
    }
    const runMode = getSidebarCommandRunModeForClick(
      command.command,
      sidebarStore.getState().commandRunStates[command.command.commandId]
    );
    close();
    post({
      commandId: command.command.commandId,
      ...(runMode === 'default' ? {} : { runMode }),
      type: 'runSidebarCommand',
    } as SidebarToExtensionMessage);
    return;
  }
  /*
   * CDXC:AppModal 2026-09-20 WHY:
   * Delayed Send replaces Quick Access inside the same native window, so closing
   * before posting would race the replacement open and dismiss the timer dialog.
   * The React palette skips its own close for exactly this one action.
   */
  if (command.definition.id !== 'delayedSend') close();
  post({ actionId: command.definition.id, type: 'runGhostexHotkeyAction' } as SidebarToExtensionMessage);
}
