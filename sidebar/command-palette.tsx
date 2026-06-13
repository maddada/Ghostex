import {
  IconArrowLeft,
  IconArrowRight,
  IconArrowsDiagonal2,
  IconBrowser,
  IconClock,
  IconChevronDown,
  IconChevronLeft,
  IconChevronRight,
  IconChevronUp,
  IconDownload,
  IconEdit,
  IconExternalLink,
  IconGitFork,
  IconKeyboard,
  IconLayoutSidebarRightExpand,
  IconLayoutDashboard,
  IconLayoutSidebar,
  IconMoon,
  IconPlayerPlay,
  IconPlus,
  IconRefresh,
  IconRotateClockwise,
  IconSettings,
  IconTerminal2,
  IconWindowMaximize,
} from "@tabler/icons-react";
import { useMemo } from "react";
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
} from "@/components/ui/command";
import {
  DEFAULT_BROWSER_ACTION_URL,
  type SidebarCommandButton,
} from "../shared/sidebar-commands";
import {
  DEFAULT_SIDEBAR_COMMAND_ICON,
  DEFAULT_SIDEBAR_COMMAND_ICON_COLOR,
} from "../shared/sidebar-command-icons";
import {
  GHOSTEX_HOTKEY_DEFINITIONS,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexFocusedPaneAction,
  type ghostexHotkeyDefinition,
  type ghostexHotkeySettings,
} from "../shared/ghostex-hotkeys";
import { openAppModal } from "./app-modal-host-bridge";
import type { CommandConfigDraft } from "./command-config-modal";
import { SidebarCommandIconGlyph } from "./sidebar-command-icon";
import { formatSidebarHotkeyLabel } from "./hotkey-label";
import type { WebviewApi } from "./webview-api";

type CommandPaletteProps = {
  commands: readonly SidebarCommandButton[];
  hotkeys?: ghostexHotkeySettings;
  isOpen: boolean;
  onBrowserCommandRun?: () => void;
  onOpenChange: (isOpen: boolean) => void;
  petOverlayEnabled?: boolean;
  vscode: WebviewApi;
};

type HotkeyPaletteCommand = {
  definition: ghostexHotkeyDefinition;
  hotkey: string;
  kind: "hotkey";
  searchText: string;
  title: string;
};

type BuiltInPaletteCommand =
  | HotkeyPaletteCommand
  | {
      hotkey: "";
      kind: "cloneRepository";
      searchText: string;
      title: string;
    }
  | {
      hotkey: "";
      kind: "pet";
      searchText: string;
      title: string;
    };

type ProjectPaletteCommand = {
  command: SidebarCommandButton;
  hotkey: string;
  slotNumber: number;
};

const PANE_ACTION_COMMAND_IDS = [
  "openBrowserPane",
  "splitMore",
  "splitMoreDown",
  "rotatePanesClockwise",
  "mergeAllTabs",
  "renameActiveSession",
  "delayedSend",
  "forkSession",
  "reloadSession",
  "popOutPane",
] as const satisfies readonly ghostexHotkeyDefinition["id"][];

export function CommandPalette({
  commands,
  hotkeys,
  isOpen,
  onBrowserCommandRun,
  onOpenChange,
  petOverlayEnabled = false,
  vscode,
}: CommandPaletteProps) {
  const normalizedHotkeys = useMemo(() => normalizeghostexHotkeySettings(hotkeys), [hotkeys]);
  const createBuiltInCommand = (definition: ghostexHotkeyDefinition): HotkeyPaletteCommand => {
    const hotkey = normalizeHotkeyText(normalizedHotkeys[definition.id] ?? definition.defaultKey);
    return {
      definition,
      hotkey,
      kind: "hotkey",
      searchText: `${definition.title} ${definition.description} ${hotkey}`,
      title: definition.title,
    };
  };
  const builtInCommands = useMemo(
    () => {
      const paneActionIds = new Set<ghostexHotkeyDefinition["id"]>(PANE_ACTION_COMMAND_IDS);
      const hotkeyCommands: BuiltInPaletteCommand[] = GHOSTEX_HOTKEY_DEFINITIONS.filter(
        (definition) =>
          definition.id !== "openCommandPalette" &&
          definition.action.kind !== "runActionSlot" &&
          !paneActionIds.has(definition.id),
      ).map(createBuiltInCommand);
      const petTitle = petOverlayEnabled ? "Sleep Pet" : "Wake Pet";
      const petCommand: BuiltInPaletteCommand = {
        hotkey: "",
        kind: "pet",
        searchText: `${petTitle} pet overlay ${petOverlayEnabled ? "hide sleep" : "show wake"}`,
        title: petTitle,
      };
      const cloneRepositoryCommand: BuiltInPaletteCommand = {
        hotkey: "",
        kind: "cloneRepository",
        searchText: "Clone Repository add project git clone github codeberg repository",
        title: "Clone Repository",
      };
      return [...hotkeyCommands, cloneRepositoryCommand, petCommand];
    },
    [normalizedHotkeys, petOverlayEnabled],
  );
  const paneActionCommands = useMemo(() => {
    const definitionsById = new Map(
      GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => [definition.id, definition]),
    );
    return PANE_ACTION_COMMAND_IDS.map((id) => definitionsById.get(id))
      .filter((definition): definition is ghostexHotkeyDefinition => definition !== undefined)
      .map(createBuiltInCommand);
  }, [normalizedHotkeys]);
  const projectCommands = useMemo(
    () =>
      commands
        .map((command, index): ProjectPaletteCommand => {
          const slotNumber = index + 1;
          const actionSlotId = getActionSlotHotkeyId(slotNumber);
          return {
            command,
            hotkey: actionSlotId
              ? normalizeHotkeyText(normalizedHotkeys[actionSlotId] ?? "")
              : "",
            slotNumber,
          };
        })
        .filter(({ command }) => isRunnableOrConfigurableCommand(command)),
    [commands, normalizedHotkeys],
  );

  const runBuiltInCommand = (command: BuiltInPaletteCommand) => {
    if (command.kind === "pet") {
      onOpenChange(false);
      vscode.postMessage({
        type: "togglePetOverlay",
      });
      return;
    }
    if (command.kind === "cloneRepository") {
      onOpenChange(false);
      openAppModal({ modal: "addRepository", type: "open" });
      return;
    }
    onOpenChange(false);
    vscode.postMessage({
      actionId: command.definition.id,
      type: "runGhostexHotkeyAction",
    });
  };

  const runProjectCommand = (command: SidebarCommandButton) => {
    if (!isConfigured(command)) {
      onOpenChange(false);
      openAppModal({
        commandDraft: createCommandPaletteDraft(command),
        modal: "commandConfig",
        type: "open",
      });
      return;
    }

    if (command.actionType === "browser") {
      onBrowserCommandRun?.();
    }
    onOpenChange(false);
    vscode.postMessage({
      commandId: command.commandId,
      type: "runSidebarCommand",
    });
  };

  return (
    <CommandDialog
      className="ghostex-settings-shadcn ghostex-command-palette-dialog top-1/2 -translate-y-1/2"
      description="Search Ghostex commands and project actions."
      open={isOpen}
      showCloseButton={false}
      title="Command Palette"
      onOpenChange={onOpenChange}
    >
      {/* CDXC:CommandPalette 2026-06-13-10:26:
          Cmd+Shift+P opens a shadcn Base-style command palette that lists the
          current Ghostex hotkey actions plus the project Actions available
          from the active sidebar context. Hotkeys are right-aligned with
          CommandShortcut so discoverability stays inside the command surface.

          CDXC:CommandPalette 2026-05-16-08:18:
          The palette should not list itself as a command, Ghostex built-ins
          should be single-line rows without descriptions, and the pet row must
          reflect the current wake/sleep state before routing through the shared
          settings-owned pet toggle.

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

          CDXC:AddRepository 2026-05-29-11:45:
          Clone Repository should be available from the command palette as a Ghostex built-in command and open the same full-window clone modal as the Projects header button, without going through configurable project actions. */}
      <Command>
        {/*
         * CDXC:CommandPalette 2026-06-11-09:14:
         * CommandInput sits inside InputGroup without an inline-start addon, so
         * add pl-3 so the query text aligns with command-row icons below.
         */}
        <CommandInput className="pl-3" placeholder="Search Ghostex commands..." />
        <CommandList className="ghostex-command-palette-list">
          <CommandEmpty>No commands found.</CommandEmpty>
          <CommandGroup heading="Ghostex">
            {builtInCommands.map((command) => (
              <CommandItem
                key={command.kind === "hotkey" ? command.definition.id : "togglePetOverlay"}
                value={command.searchText}
                onSelect={() => runBuiltInCommand(command)}
              >
                <BuiltInCommandIcon command={command} />
                <span className="ghostex-command-palette-copy">
                  <span className="ghostex-command-palette-title">{command.title}</span>
                </span>
                {command.hotkey ? (
                  <CommandShortcut>{formatSidebarHotkeyLabel(command.hotkey)}</CommandShortcut>
                ) : null}
              </CommandItem>
            ))}
          </CommandGroup>
          {paneActionCommands.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Pane Actions">
                {paneActionCommands.map((command) => (
                  <CommandItem
                    key={command.definition.id}
                    value={command.searchText}
                    onSelect={() => runBuiltInCommand(command)}
                  >
                    <BuiltInCommandIcon command={command} />
                    <span className="ghostex-command-palette-copy">
                      <span className="ghostex-command-palette-title">{command.title}</span>
                    </span>
                    {command.hotkey ? (
                      <CommandShortcut>{formatSidebarHotkeyLabel(command.hotkey)}</CommandShortcut>
                    ) : null}
                  </CommandItem>
                ))}
              </CommandGroup>
            </>
          ) : null}
          {projectCommands.length > 0 ? (
            <>
              <CommandSeparator />
              <CommandGroup heading="Project Actions">
                {projectCommands.map(({ command, hotkey, slotNumber }) => (
                  <CommandItem
                    key={command.commandId}
                    value={`${getCommandTitle(command)} ${getCommandDescription(command)} ${hotkey} action ${slotNumber}`}
                    onSelect={() => runProjectCommand(command)}
                  >
                    <SidebarCommandIconGlyph
                      color={command.iconColor ?? DEFAULT_SIDEBAR_COMMAND_ICON_COLOR}
                      icon={command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON}
                      stroke={1.8}
                    />
                    <span className="ghostex-command-palette-copy">
                      <span className="ghostex-command-palette-title">
                        {getCommandTitle(command)}
                      </span>
                      <span className="ghostex-command-palette-description">
                        {getCommandDescription(command)}
                      </span>
                    </span>
                    {hotkey ? (
                      <CommandShortcut>{formatSidebarHotkeyLabel(hotkey)}</CommandShortcut>
                    ) : null}
                  </CommandItem>
                ))}
              </CommandGroup>
            </>
          ) : null}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}

function BuiltInCommandIcon({ command }: { command: BuiltInPaletteCommand }) {
  if (command.kind === "cloneRepository") {
    return <IconDownload aria-hidden="true" />;
  }
  if (command.kind === "pet") {
    return command.title === "Sleep Pet" ? (
      <IconMoon aria-hidden="true" />
    ) : (
      <IconPlayerPlay aria-hidden="true" />
    );
  }

  const action = command.definition.action;
  if (action.kind === "createSession") {
    return <IconPlus aria-hidden="true" />;
  }
  if (action.kind === "openCommandsPanel") {
    return <IconTerminal2 aria-hidden="true" />;
  }
  if (action.kind === "openSettings") {
    return <IconSettings aria-hidden="true" />;
  }
  if (action.kind === "moveSidebar") {
    return <IconLayoutSidebarRightExpand aria-hidden="true" />;
  }
  if (action.kind === "toggleSidebarCollapsed") {
    return <IconLayoutSidebar aria-hidden="true" />;
  }
  if (action.kind === "renameActiveSession") {
    return <IconEdit aria-hidden="true" />;
  }
  if (action.kind === "focusedPaneAction") {
    return <FocusedPaneCommandIcon action={action.focusedPaneAction} />;
  }
  if (action.kind === "focusAdjacentGroup") {
    return action.direction < 0 ? (
      <IconChevronLeft aria-hidden="true" />
    ) : (
      <IconChevronRight aria-hidden="true" />
    );
  }
  if (action.kind === "focusDirection") {
    return getFocusDirectionIcon(action.direction);
  }
  if (action.kind === "splitFocusedPane") {
    return <IconArrowsDiagonal2 aria-hidden="true" />;
  }
  if (action.kind === "setViewMode") {
    return <IconLayoutDashboard aria-hidden="true" />;
  }
  return <IconKeyboard aria-hidden="true" />;
}

function FocusedPaneCommandIcon({ action }: { action: ghostexFocusedPaneAction }) {
  if (action === "openBrowserPane") {
    return <IconBrowser aria-hidden="true" />;
  }
  if (action === "rotatePanesClockwise") {
    return <IconRotateClockwise aria-hidden="true" />;
  }
  if (action === "mergeAllTabs") {
    return <IconWindowMaximize aria-hidden="true" />;
  }
  if (action === "delayedSend") {
    return <IconClock aria-hidden="true" />;
  }
  if (action === "forkSession") {
    return <IconGitFork aria-hidden="true" />;
  }
  if (action === "reloadSession") {
    return <IconRefresh aria-hidden="true" />;
  }
  if (action === "popOutPane") {
    return <IconExternalLink aria-hidden="true" />;
  }
  return <IconLayoutSidebarRightExpand aria-hidden="true" />;
}

function getFocusDirectionIcon(direction: "down" | "left" | "right" | "up") {
  if (direction === "up") {
    return <IconChevronUp aria-hidden="true" />;
  }
  if (direction === "right") {
    return <IconArrowRight aria-hidden="true" />;
  }
  if (direction === "down") {
    return <IconChevronDown aria-hidden="true" />;
  }
  return <IconArrowLeft aria-hidden="true" />;
}

function getActionSlotHotkeyId(slotNumber: number): ghostexHotkeyDefinition["id"] | undefined {
  if (slotNumber < 1 || slotNumber > 5) {
    return undefined;
  }
  return `runActionSlot${slotNumber}` as ghostexHotkeyDefinition["id"];
}

function createCommandPaletteDraft(command: SidebarCommandButton): CommandConfigDraft {
  return {
    actionType: command.actionType,
    closeTerminalOnExit: command.closeTerminalOnExit,
    command: command.command ?? (command.actionType === "terminal" ? "" : undefined),
    commandId: command.commandId,
    icon: command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
    iconColor: command.iconColor ?? DEFAULT_SIDEBAR_COMMAND_ICON_COLOR,
    name: command.name,
    playCompletionSound: command.playCompletionSound,
    url: command.url ?? (command.actionType === "browser" ? DEFAULT_BROWSER_ACTION_URL : undefined),
  };
}

function isRunnableOrConfigurableCommand(command: SidebarCommandButton): boolean {
  return command.name.trim().length > 0 || command.icon !== undefined;
}

function isConfigured(command: SidebarCommandButton): boolean {
  return command.actionType === "browser" ? Boolean(command.url) : Boolean(command.command);
}

function getCommandTitle(command: SidebarCommandButton): string {
  const name = command.name.trim();
  if (name) {
    return name;
  }
  return command.actionType === "browser" ? "Untitled Webpage" : "Untitled Action";
}

function getCommandDescription(command: SidebarCommandButton): string {
  const target = getCommandTarget(command);
  const typeLabel = command.actionType === "browser" ? "Browser" : "Terminal";
  if (!target) {
    return `${typeLabel} - Not configured`;
  }
  return `${typeLabel} - ${target}`;
}

function getCommandTarget(command: SidebarCommandButton): string | undefined {
  const target = command.actionType === "browser" ? command.url?.trim() : command.command?.trim();
  if (!target) {
    return undefined;
  }
  return target.split("\n")[0] || undefined;
}
