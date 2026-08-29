import { isSidebarCommandIcon, type SidebarCommandIcon } from './sidebar-command-icons';

export const DEFAULT_SIDEBAR_COMMANDS = [
  {
    commandId: 'dev',
    name: 'Dev',
  },
  {
    commandId: 'build',
    name: 'Build',
  },
  {
    commandId: 'test',
    name: 'Test',
  },
  {
    commandId: 'setup',
    name: 'Setup',
  },
] as const;

export const DEFAULT_BROWSER_ACTION_URL = 'http://localhost:5173';
export const DEFAULT_BROWSER_LAUNCH_URL = 'https://www.google.com';
/**
 * CDXC:Mobile 2026-06-16-01:23:
 * Sidebar Mobile opens the product site so download routing stays on ghostex.dev
 * instead of a GitHub README anchor.
 */
export const GHOSTEX_MOBILE_DOWNLOAD_URL = 'https://ghostex.dev';

/**
 * CDXC:ProjectActions 2026-05-19-17:10:
 * Default terminal actions ship without a configured command. Surfaces that show
 * the runnable target use this label until the user saves a command.
 */
export const SIDEBAR_UNCONFIGURED_TERMINAL_COMMAND_LABEL = 'Set the command';

export function isSidebarCommandConfigured(command: SidebarCommandButton): boolean {
  return command.actionType === 'browser' ? Boolean(command.url?.trim()) : Boolean(command.command?.trim());
}

export function getSidebarCommandPreviewLabel(command: SidebarCommandButton): string {
  if (command.actionType === 'browser') {
    return command.url?.trim() ?? '';
  }
  return command.command?.trim() || SIDEBAR_UNCONFIGURED_TERMINAL_COMMAND_LABEL;
}

export type DefaultSidebarCommandId = (typeof DEFAULT_SIDEBAR_COMMANDS)[number]['commandId'];
export type SidebarActionType = 'browser' | 'terminal';
/**
 * CDXC:GlobalActions 2026-08-07:
 * Which Actions list a run-by-id selector names. Global and project Actions are
 * separate id spaces, so a selector carrying only an id is ambiguous once a
 * surface can run either. Absent means project, which keeps every sender that
 * predates Global Actions unchanged.
 */
export type SidebarCommandScope = 'global' | 'project';
export type SidebarCommandRunMode = 'default' | 'debug';
export type SidebarCommandLinkTarget = 'integrated' | 'external';

/**
 * CDXC:ProjectActions 2026-07-31-12:00:
 * Terminal actions can open saved links alongside running their command, so a
 * dev-server action can start the server and surface its localhost URL in one
 * click. Each link picks the integrated browser pane or the system browser.
 */
export type SidebarCommandLink = {
  target: SidebarCommandLinkTarget;
  url: string;
};

export type SidebarCommandButton = {
  actionType: SidebarActionType;
  closeTerminalOnExit: boolean;
  command?: string;
  commandId: string;
  icon?: SidebarCommandIcon;
  isDefault: boolean;
  links?: SidebarCommandLink[];
  name: string;
  playCompletionSound: boolean;
  showOnProjectRow: boolean;
  url?: string;
};

export type StoredSidebarCommand = {
  actionType: SidebarActionType;
  closeTerminalOnExit: boolean;
  commandId: string;
  icon?: SidebarCommandIcon;
  /**
   * CDXC:ProjectActions 2026-06-17-07:40:
   * Legacy storage may still contain per-action icon colors from older Mac app
   * builds. Keep this input field decodable, but normalization strips it so
   * titlebar Action buttons cannot persist or render custom icon colors.
   */
  iconColor?: string;
  isDefault: boolean;
  links?: SidebarCommandLink[];
  name: string;
  playCompletionSound: boolean;
  /**
   * CDXC:ProjectActions 2026-08-01:
   * Actions flagged showOnProjectRow render as inline icon buttons on their
   * project's sidebar row beside the built-in header actions. Older saved
   * records lack the field; normalization defaults it to false so existing
   * Actions keep their titlebar-only behavior.
   */
  showOnProjectRow: boolean;
  command?: string;
  url?: string;
};

export function normalizeSidebarCommandLinks(candidate: unknown): SidebarCommandLink[] {
  if (!Array.isArray(candidate)) {
    return [];
  }

  const normalizedLinks: SidebarCommandLink[] = [];
  for (const item of candidate) {
    if (!item || typeof item !== 'object') {
      continue;
    }
    const partialLink = item as Partial<SidebarCommandLink> & { target?: string; url?: unknown };
    const url = typeof partialLink.url === 'string' ? partialLink.url.trim() : '';
    if (!url) {
      continue;
    }
    normalizedLinks.push({
      target: partialLink.target === 'external' ? 'external' : 'integrated',
      url,
    });
  }
  return normalizedLinks;
}

export function isSidebarCommandRunMode(value: unknown): value is SidebarCommandRunMode {
  return value === 'default' || value === 'debug';
}

export function isSidebarCommandScope(value: unknown): value is SidebarCommandScope {
  return value === 'global' || value === 'project';
}

export function getFirstBrowserSidebarCommandUrl(commands: readonly SidebarCommandButton[]): string | undefined {
  return commands.find((command) => command.actionType === 'browser')?.url;
}

export function createDefaultSidebarCommandButtons(): SidebarCommandButton[] {
  return DEFAULT_SIDEBAR_COMMANDS.map((command) => ({
    actionType: 'terminal',
    closeTerminalOnExit: false,
    command: undefined,
    commandId: command.commandId,
    isDefault: true,
    name: command.name,
    playCompletionSound: true,
    showOnProjectRow: false,
    url: undefined,
  }));
}

export function createSidebarCommandButtons(
  storedCommands: readonly StoredSidebarCommand[],
  storedOrder: readonly string[] = [],
  deletedDefaultCommandIds: readonly string[] = []
): SidebarCommandButton[] {
  const storedCommandById = new Map(storedCommands.map((command) => [command.commandId, command]));
  const deletedDefaultCommandIdSet = new Set(normalizeStoredSidebarCommandOrder(deletedDefaultCommandIds));
  const defaultButtons = DEFAULT_SIDEBAR_COMMANDS.reduce<SidebarCommandButton[]>((buttons, command) => {
    if (deletedDefaultCommandIdSet.has(command.commandId)) {
      return buttons;
    }

    const storedCommand = storedCommandById.get(command.commandId);
    buttons.push(storedCommand ? normalizeStoredCommandButton(storedCommand) : defaultAction(command));
    return buttons;
  }, []);

  const customButtons = storedCommands
    .filter((command) => !isDefaultSidebarCommandId(command.commandId))
    .map((command) => normalizeStoredCommandButton(command));

  return orderSidebarCommandButtons([...defaultButtons, ...customButtons], storedOrder);
}

/**
 * CDXC:GlobalActions 2026-08-01:
 * Global Actions have no built-in members — dev/build/test/setup are
 * project-scoped — so the stored rows are the entire list. That removes the
 * whole default-resurrection and tombstone branch that project actions need,
 * and it means an empty store renders an empty section rather than four
 * unconfigured placeholder buttons.
 */
export function createGlobalSidebarCommandButtons(
  storedCommands: readonly StoredSidebarCommand[],
  storedOrder: readonly string[] = []
): SidebarCommandButton[] {
  return orderSidebarCommandButtons(
    storedCommands.map((command) => normalizeStoredCommandButton(command)),
    storedOrder
  );
}

export function isDefaultSidebarCommandId(commandId: string): commandId is DefaultSidebarCommandId {
  return DEFAULT_SIDEBAR_COMMANDS.some((command) => command.commandId === commandId);
}

export function normalizeStoredSidebarCommands(candidate: unknown): StoredSidebarCommand[] {
  if (!Array.isArray(candidate)) {
    return [];
  }

  /**
   * CDXC:ProjectActions 2026-06-17-07:40:
   * Mac app updates must not drop existing titlebar Actions because older saved
   * records still carry the removed per-action icon color field or lack newer
   * optional fields. Treat iconColor as legacy input only, infer browser
   * actions from saved URLs when actionType is absent, and default optional
   * terminal behavior flags while preserving the runnable action definition.
   */
  const normalizedCommands: StoredSidebarCommand[] = [];
  const seenCommandIds = new Set<string>();

  for (const item of candidate) {
    if (!item || typeof item !== 'object') {
      continue;
    }

    const partialItem = item as Partial<StoredSidebarCommand> & {
      actionType?: string;
      command?: string;
      icon?: string;
      iconColor?: string;
      playCompletionSound?: unknown;
      url?: string;
    };
    const commandId = partialItem.commandId?.trim();
    const name = partialItem.name?.trim() ?? '';
    const actionType = normalizeActionType(partialItem.actionType, partialItem.url);
    const icon = isSidebarCommandIcon(partialItem.icon) ? partialItem.icon : undefined;
    const isDefault = partialItem.isDefault === true || (commandId ? isDefaultSidebarCommandId(commandId) : false);
    if (!commandId || seenCommandIds.has(commandId)) {
      continue;
    }

    if (actionType === 'browser') {
      const url = partialItem.url?.trim();
      if (!url) {
        continue;
      }

      normalizedCommands.push({
        actionType,
        closeTerminalOnExit: false,
        commandId,
        isDefault,
        name,
        playCompletionSound: false,
        showOnProjectRow: partialItem.showOnProjectRow === true,
        ...(icon ? { icon } : {}),
        url,
      });
      seenCommandIds.add(commandId);
      continue;
    }

    const command = partialItem.command?.trim();
    if (!command) {
      continue;
    }

    const links = normalizeSidebarCommandLinks(partialItem.links);
    normalizedCommands.push({
      actionType,
      closeTerminalOnExit:
        typeof partialItem.closeTerminalOnExit === 'boolean' ? partialItem.closeTerminalOnExit : false,
      command,
      commandId,
      isDefault,
      name,
      playCompletionSound:
        typeof partialItem.playCompletionSound === 'boolean' ? partialItem.playCompletionSound : true,
      showOnProjectRow: partialItem.showOnProjectRow === true,
      ...(icon ? { icon } : {}),
      ...(links.length > 0 ? { links } : {}),
    });
    seenCommandIds.add(commandId);
  }

  return normalizedCommands;
}

export function normalizeStoredSidebarCommandOrder(candidate: unknown): string[] {
  if (!Array.isArray(candidate)) {
    return [];
  }

  const normalizedOrder: string[] = [];
  const seenCommandIds = new Set<string>();

  for (const item of candidate) {
    if (typeof item !== 'string') {
      continue;
    }

    const commandId = item.trim();
    if (!commandId || seenCommandIds.has(commandId)) {
      continue;
    }

    normalizedOrder.push(commandId);
    seenCommandIds.add(commandId);
  }

  return normalizedOrder;
}

function defaultAction(command: (typeof DEFAULT_SIDEBAR_COMMANDS)[number]): SidebarCommandButton {
  return {
    actionType: 'terminal',
    closeTerminalOnExit: false,
    command: undefined,
    commandId: command.commandId,
    isDefault: true,
    name: command.name,
    playCompletionSound: true,
    showOnProjectRow: false,
    url: undefined,
  };
}

function normalizeStoredCommandButton(command: StoredSidebarCommand): SidebarCommandButton {
  return command.actionType === 'browser'
    ? {
        actionType: 'browser',
        closeTerminalOnExit: false,
        command: undefined,
        commandId: command.commandId,
        isDefault: command.isDefault,
        name: command.name,
        playCompletionSound: false,
        showOnProjectRow: command.showOnProjectRow === true,
        ...(command.icon ? { icon: command.icon } : {}),
        url: command.url,
      }
    : {
        actionType: 'terminal',
        closeTerminalOnExit: command.closeTerminalOnExit,
        command: command.command,
        commandId: command.commandId,
        isDefault: command.isDefault,
        name: command.name,
        playCompletionSound: command.playCompletionSound,
        showOnProjectRow: command.showOnProjectRow === true,
        ...(command.icon ? { icon: command.icon } : {}),
        ...(command.links && command.links.length > 0 ? { links: command.links } : {}),
        url: undefined,
      };
}

function normalizeActionType(value: string | undefined, url: string | undefined): SidebarActionType {
  if (value === 'browser') {
    return 'browser';
  }
  if (value === 'terminal') {
    return 'terminal';
  }
  return url?.trim() ? 'browser' : 'terminal';
}

function orderSidebarCommandButtons(
  buttons: readonly SidebarCommandButton[],
  storedOrder: readonly string[]
): SidebarCommandButton[] {
  const buttonById = new Map(buttons.map((button) => [button.commandId, button] as const));
  const orderedButtons: SidebarCommandButton[] = [];

  for (const commandId of normalizeStoredSidebarCommandOrder(storedOrder)) {
    const button = buttonById.get(commandId);
    if (button) {
      orderedButtons.push(button);
    }
  }

  for (const button of buttons) {
    if (!orderedButtons.some((candidate) => candidate.commandId === button.commandId)) {
      orderedButtons.push(button);
    }
  }

  return orderedButtons;
}
