import {
  DragDropProvider,
  type DragDropEventHandlers,
} from "@dnd-kit/react";
import {
  isSortableOperation,
  useSortable,
} from "@dnd-kit/react/sortable";
import {
  useEffect,
  useId,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { Button } from "@/packages/components/ui/button";
import { Command } from "@/packages/components/ui/command";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/packages/components/ui/empty";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/packages/components/ui/field";
import {
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/packages/components/ui/select";
import { Switch } from "@/packages/components/ui/switch";
import {
  IconGripVertical,
  IconInfoCircle,
  IconPencil,
  IconPlus,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import { type ghostexSettings } from "../../../shared/ghostex-settings";
import {
  DEFAULT_BROWSER_ACTION_URL,
  isSidebarCommandConfigured,
  type SidebarActionType,
  type SidebarCommandButton,
  type SidebarCommandLink,
} from "../../../shared/sidebar-commands";
import {
  DEFAULT_SIDEBAR_COMMAND_ICON,
  type SidebarCommandIcon,
} from "../../../shared/sidebar-command-icons";
import { CommandIconPicker } from "../../command-icon-picker";
import { SidebarCommandIconGlyph } from "../../sidebar-command-icon";
import { useSidebarStore } from "../../sidebar-store";
import { type WebviewApi } from "../../webview-api";
import {
  createSettingsCommandDragData,
  createSettingsReorderRequestId,
  getSettingsCommandDragData,
  mergeIds,
  moveId,
  reconcileDraftIds,
} from "../drag-data";
import {
  SettingButton,
  SettingsInput,
  SettingsNativeScrollArea,
  SettingsSection,
  SettingsSelect,
  SettingsSelectContent,
  SettingsTextarea,
  ToggleField,
  setSettingsSortableRowElement,
} from "../fields";
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
} from "../search";
import { SettingModificationProps } from "../types";

export type SettingsCommandEditorState = {
  draft: SettingsCommandDraft;
  lockedActionType?: SidebarActionType;
};

export type SettingsCommandDraft = {
  actionType: SidebarActionType;
  closeTerminalOnExit: boolean;
  command?: string;
  commandId?: string;
  icon?: SidebarCommandIcon;
  links?: SidebarCommandLink[];
  name: string;
  playCompletionSound: boolean;
  showOnProjectRow: boolean;
  url?: string;
};

/*
CDXC:GlobalActions 2026-08-01:
Settings > Actions holds two lists that behave identically and differ only in
who owns them: Global Actions apply to every project and live in gxserver,
Project Actions belong to one project (and its worktrees) and live in project
metadata. The scope drives the bridge message types and the copy; everything
else — ordering, drag reorder, the editor, duplicate-title validation — is one
implementation, so the two lists cannot drift apart.
*/
export type SettingsCommandScope = "global" | "project";

export type SettingsCommandScopeEditorState = SettingsCommandEditorState & {
  scope: SettingsCommandScope;
};

export function useSettingsCommandOrder(commands: readonly SidebarCommandButton[]) {
  const [draftCommandIds, setDraftCommandIds] = useState<string[]>();

  useEffect(() => {
    setDraftCommandIds((previousDraft) => reconcileDraftIds(previousDraft, commands, "commandId"));
  }, [commands]);

  const orderedCommands = useMemo(() => {
    const commandById = new Map(commands.map((command) => [command.commandId, command]));
    const orderedCommandIds = draftCommandIds
      ? mergeIds(
          draftCommandIds,
          commands.map((command) => command.commandId),
        )
      : commands.map((command) => command.commandId);

    return orderedCommandIds
      .map((commandId) => commandById.get(commandId))
      .filter((command): command is SidebarCommandButton => command !== undefined);
  }, [commands, draftCommandIds]);

  return { orderedCommands, setDraftCommandIds };
}

export function ActionsSettingsTab({
  getSettingModificationProps,
  hideTabStripNewBrowserButton,
  hideTabStripNewChatButton,
  hideTabStripNewTerminalButton,
  onHideTabStripNewBrowserButtonChange,
  onHideTabStripNewChatButtonChange,
  onHideTabStripNewTerminalButtonChange,
  search,
  searchEmptyState,
  vscode,
}: {
  getSettingModificationProps: <Key extends keyof ghostexSettings>(
    key: Key,
  ) => Required<SettingModificationProps>;
  hideTabStripNewBrowserButton: boolean;
  hideTabStripNewChatButton: boolean;
  hideTabStripNewTerminalButton: boolean;
  onHideTabStripNewBrowserButtonChange: (checked: boolean) => void;
  onHideTabStripNewChatButtonChange: (checked: boolean) => void;
  onHideTabStripNewTerminalButtonChange: (checked: boolean) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  vscode?: WebviewApi;
}) {
  const commands = useSidebarStore((state) => state.hud.commands);
  const globalCommands = useSidebarStore((state) => state.hud.globalCommands);
  const [editorState, setEditorState] = useState<SettingsCommandScopeEditorState>();

  const emptyGlobalCommands = useMemo<SidebarCommandButton[]>(() => [], []);
  const { orderedCommands, setDraftCommandIds } = useSettingsCommandOrder(commands);
  const {
    orderedCommands: orderedGlobalCommands,
    setDraftCommandIds: setDraftGlobalCommandIds,
  } = useSettingsCommandOrder(globalCommands ?? emptyGlobalCommands);
  /*
  CDXC:ProjectActions 2026-06-15-15:29:
  When no Actions have a saved terminal command or browser URL, the top of Settings > Actions should explain that frequently used commands can be set here for one-click or hotkey execution.
  */
  const hasConfiguredActions = useMemo(
    () =>
      [...orderedGlobalCommands, ...orderedCommands].some((command) =>
        isSidebarCommandConfigured(command),
      ),
    [orderedCommands, orderedGlobalCommands],
  );

  const deleteCommand = (scope: SettingsCommandScope, commandId: string) => {
    vscode?.postMessage({
      commandId,
      type: scope === "global" ? "deleteGlobalSidebarCommand" : "deleteSidebarCommand",
    });
    setEditorState(undefined);
  };

  const saveCommand = (scope: SettingsCommandScope, draft: SettingsCommandDraft) => {
    if (!vscode) {
      return;
    }
    /*
     * The two messages carry an identical payload and differ only in `type`,
     * but the bridge message union discriminates on that field, so it is
     * written as a literal in each branch rather than computed.
     */
    const payload = {
      actionType: draft.actionType,
      closeTerminalOnExit: draft.closeTerminalOnExit,
      command: draft.command,
      commandId: draft.commandId,
      icon: draft.icon,
      links: draft.links,
      name: draft.name,
      playCompletionSound: draft.playCompletionSound,
      showOnProjectRow: draft.showOnProjectRow,
      url: draft.url,
    };
    if (scope === "global") {
      vscode.postMessage({ ...payload, type: "saveGlobalSidebarCommand" });
    } else {
      vscode.postMessage({ ...payload, type: "saveSidebarCommand" });
    }
    setEditorState(undefined);
  };

  const reorderCommands = (scope: SettingsCommandScope, nextCommandIds: string[]) => {
    if (scope === "global") {
      setDraftGlobalCommandIds(nextCommandIds);
    } else {
      setDraftCommandIds(nextCommandIds);
    }
    vscode?.postMessage({
      commandIds: nextCommandIds,
      requestId: createSettingsReorderRequestId(scope === "global" ? "globalActions" : "actions"),
      type: scope === "global" ? "syncGlobalSidebarCommandOrder" : "syncSidebarCommandOrder",
    });
  };

  if (!editorState && search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)) {
    return (
      <SettingsNativeScrollArea className="h-full min-h-0">
        <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">{searchEmptyState}</div>
      </SettingsNativeScrollArea>
    );
  }

  /*
   * Editing replaces both lists with the single editor, the same way the one
   * Actions list behaved before Global Actions existed. Showing the other list
   * alongside an open editor would let a user start a second edit and lose the
   * first draft.
   */
  if (editorState) {
    const editorScope = editorState.scope;
    const editorCommandId = editorState.draft.commandId;
    return (
      <SettingsNativeScrollArea className="h-full min-h-0">
        <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
          <SettingsSection title={editorScope === "global" ? "Global Action" : "Action"}>
            <ActionSettingsEditor
              draft={editorState.draft}
              existingCommands={editorScope === "global" ? orderedGlobalCommands : commands}
              lockedActionType={editorState.lockedActionType}
              onCancel={() => setEditorState(undefined)}
              onDelete={
                editorCommandId ? () => deleteCommand(editorScope, editorCommandId) : undefined
              }
              onSave={(draft) => saveCommand(editorScope, draft)}
            />
          </SettingsSection>
        </div>
      </SettingsNativeScrollArea>
    );
  }

  return (
    <SettingsNativeScrollArea className="h-full min-h-0">
      <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
        {!hasConfiguredActions ? (
          <div className="flex items-start gap-3 border border-border bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
            <IconInfoCircle aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-foreground" />
            <p className="m-0">
              Set frequently used terminal or browser commands here so you can run them with one
              click or a hotkey.
            </p>
          </div>
        ) : null}
        <ActionsSettingsSection
          commands={orderedGlobalCommands}
          description="Global actions apply to every project and are stored by the Ghostex daemon, so they follow you to every app that connects to it. They appear in the tab strip above your tabs."
          emptyDescription="Add a terminal or browser action that should be available in every project."
          emptyTitle="No global actions configured"
          onCreate={(actionType) =>
            setEditorState({
              draft: createSettingsCommandDraft(actionType),
              lockedActionType: actionType,
              scope: "global",
            })
          }
          onDelete={(commandId) => deleteCommand("global", commandId)}
          onEdit={(command) =>
            setEditorState({
              draft: createSettingsCommandDraftFromButton(command),
              scope: "global",
            })
          }
          onReorder={(nextCommandIds) => reorderCommands("global", nextCommandIds)}
          title="Global Actions"
          vscode={vscode}
        />
        <ActionsSettingsSection
          commands={orderedCommands}
          /*
           * CDXC:ActionsSettings 2026-06-15-14:00:
           * The Actions section header needs explanatory copy because users may
           * not know that terminal actions run in quick command terminals,
           * browser actions open panes, project actions are shared with
           * worktrees, and right-click exposes every configured project action.
           */
          description="Actions are custom shortcuts for repeat work. Add terminal actions to run saved commands in quick command terminals, or browser actions to open saved URLs in browser panes. These actions are shared between a main project and its worktrees, and you can right-click the action button to show all configured actions for that project."
          emptyDescription="Add a terminal or browser action."
          emptyTitle="No actions configured"
          onCreate={(actionType) =>
            setEditorState({
              draft: createSettingsCommandDraft(actionType),
              lockedActionType: actionType,
              scope: "project",
            })
          }
          onDelete={(commandId) => deleteCommand("project", commandId)}
          onEdit={(command) =>
            setEditorState({
              draft: createSettingsCommandDraftFromButton(command),
              scope: "project",
            })
          }
          onReorder={(nextCommandIds) => reorderCommands("project", nextCommandIds)}
          title="Project Actions"
          vscode={vscode}
        />
        {/*
         * CDXC:GlobalActions 2026-08-01:
         * The built-in tab strip buttons are toggled here, next to the Global
         * Actions that share the strip with them, because that is where a user
         * goes when the strip is too crowded. The pane actions menu has no
         * toggle: it is the only route to the remaining pane actions.
         */}
        <SettingsSection
          description="Global actions share the tab strip with these built-in buttons. Hide the ones you do not use to make room."
          title="Tab Strip Buttons"
        >
          <ToggleField
            checked={hideTabStripNewTerminalButton}
            description="Hide the New Terminal button from the tab strip."
            {...getSettingModificationProps("hideTabStripNewTerminalButton")}
            label="Hide New Terminal button"
            onChange={onHideTabStripNewTerminalButtonChange}
          />
          <ToggleField
            checked={hideTabStripNewBrowserButton}
            description="Hide the New Browser Tab button from the tab strip."
            {...getSettingModificationProps("hideTabStripNewBrowserButton")}
            label="Hide New Browser Tab button"
            onChange={onHideTabStripNewBrowserButtonChange}
          />
        </SettingsSection>
      </div>
    </SettingsNativeScrollArea>
  );
}

export function ActionsSettingsSection({
  commands,
  description,
  emptyDescription,
  emptyTitle,
  onCreate,
  onDelete,
  onEdit,
  onReorder,
  title,
  vscode,
}: {
  commands: readonly SidebarCommandButton[];
  description: string;
  emptyDescription: string;
  emptyTitle: string;
  onCreate: (actionType: SidebarActionType) => void;
  onDelete: (commandId: string) => void;
  onEdit: (command: SidebarCommandButton) => void;
  onReorder: (nextCommandIds: string[]) => void;
  title: string;
  vscode?: WebviewApi;
}) {
  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getSettingsCommandDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex =
      "index" in source && typeof source.index === "number" ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    onReorder(
      moveId(
        commands.map((command) => command.commandId),
        source.initialIndex,
        targetIndex,
      ),
    );
  }) satisfies DragDropEventHandlers["onDragEnd"];

  return (
    <SettingsSection
      actions={
        <>
          <SettingButton
            disabled={!vscode}
            disabledReason="Adding actions needs the Ghostex app connection."
            onClick={() => onCreate("terminal")}
            type="button"
            variant="outline"
          >
            <IconPlus aria-hidden="true" data-icon="inline-start" />
            Terminal Action
          </SettingButton>
          <SettingButton
            disabled={!vscode}
            disabledReason="Adding actions needs the Ghostex app connection."
            onClick={() => onCreate("browser")}
            type="button"
            variant="outline"
          >
            <IconPlus aria-hidden="true" data-icon="inline-start" />
            Browser Action
          </SettingButton>
        </>
      }
      description={description}
      title={title}
    >
      {commands.length > 0 ? (
        <DragDropProvider onDragEnd={handleDragEnd}>
          <div className="flex flex-col gap-2">
            {commands.map((command, index) => (
              <SettingsCommandRow
                command={command}
                index={index}
                key={command.commandId}
                onEdit={() => onEdit(command)}
                onDelete={() => onDelete(command.commandId)}
              />
            ))}
          </div>
        </DragDropProvider>
      ) : (
        <Empty className="border border-border bg-muted/20">
          <EmptyHeader>
            <EmptyTitle>{emptyTitle}</EmptyTitle>
            <EmptyDescription>{emptyDescription}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </SettingsSection>
  );
}

export function SettingsCommandRow({
  command,
  index,
  onDelete,
  onEdit,
}: {
  command: SidebarCommandButton;
  index: number;
  onDelete: () => void;
  onEdit: () => void;
}) {
  const sortable = useSortable({
    accept: "settings-command",
    data: createSettingsCommandDragData(command.commandId),
    group: "settings-commands",
    id: command.commandId,
    index,
    type: "settings-command",
  });
  const { handleRef, isDragging } = sortable;

  const setRowRef = (element: HTMLDivElement | null) => {
    setSettingsSortableRowElement(sortable, element);
  };

  return (
    <div
      className="settings-management-row flex items-center gap-2 border border-border bg-muted/20 p-2"
      data-dragging={String(Boolean(isDragging))}
      ref={setRowRef}
    >
      <Button
        aria-label={`Reorder ${getActionTitle(command)}`}
        ref={handleRef}
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <IconGripVertical aria-hidden="true" />
      </Button>
      <Button
        className="settings-management-edit-button h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-2 text-left"
        onClick={onEdit}
        type="button"
        variant="ghost"
      >
        <span
          aria-hidden="true"
          className="settings-management-icon flex size-9 shrink-0 items-center justify-center bg-muted"
        >
          <SettingsActionIcon command={command} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-medium text-foreground">
            {getActionTitle(command)}
          </span>
          <span className="block truncate text-xs text-muted-foreground">
            {getActionMeta(command)}
          </span>
        </span>
      </Button>
      <span className="settings-management-row-actions">
        <Button
          aria-label={`Edit ${getActionTitle(command)}`}
          onClick={onEdit}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <IconPencil aria-hidden="true" />
        </Button>
        <Button
          aria-label={`Delete ${getActionTitle(command)}`}
          onClick={onDelete}
          size="icon-sm"
          type="button"
          variant="destructive"
        >
          <IconTrash aria-hidden="true" />
        </Button>
      </span>
    </div>
  );
}

export function ActionSettingsEditor({
  draft,
  existingCommands,
  lockedActionType,
  onCancel,
  onDelete,
  onSave,
}: {
  draft: SettingsCommandDraft;
  existingCommands: readonly SidebarCommandButton[];
  lockedActionType?: SidebarActionType;
  onCancel: () => void;
  onDelete?: () => void;
  onSave: (draft: SettingsCommandDraft) => void;
}) {
  const [actionType, setActionType] = useState<SidebarActionType>(draft.actionType);
  const [closeTerminalOnExit, setCloseTerminalOnExit] = useState(draft.closeTerminalOnExit);
  const [command, setCommand] = useState(draft.command ?? "");
  const [icon, setIcon] = useState<SidebarCommandIcon>(
    draft.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
  );
  const [links, setLinks] = useState<SidebarCommandLink[]>(draft.links ?? []);
  const [name, setName] = useState(draft.name);
  const [playCompletionSound, setPlayCompletionSound] = useState(draft.playCompletionSound);
  const [showOnProjectRow, setShowOnProjectRow] = useState(draft.showOnProjectRow);
  const [url, setUrl] = useState(
    draft.url ??
      ((lockedActionType ?? draft.actionType) === "browser" ? DEFAULT_BROWSER_ACTION_URL : ""),
  );
  const actionTypeId = useId();
  const closeTerminalOnExitId = useId();
  const commandId = useId();
  const nameId = useId();
  const showOnProjectRowId = useId();
  const soundId = useId();
  const urlId = useId();
  const isActionTypeLocked = lockedActionType !== undefined;
  const targetValue = actionType === "browser" ? url.trim() : command.trim();
  const trimmedName = name.trim();
  const commandTitle = getSettingsCommandDraftTitle({ actionType, command, name, url });
  /**
   * CDXC:CommandPanes 2026-05-16-15:08:
   * Settings must enforce one action title per project because command-pane
   * reuse uses that title as the pane identifier. Blocking duplicates here
   * prevents saving an action that could target another action's command tab.
   */
  const hasDuplicateTitle = existingCommands.some(
    (commandButton) =>
      commandButton.commandId !== draft.commandId &&
      getSettingsCommandTitleKey(getSettingsCommandButtonTitle(commandButton)) ===
        getSettingsCommandTitleKey(commandTitle),
  );
  const isSaveDisabled = targetValue.length === 0 || hasDuplicateTitle;

  const getDraft = (): SettingsCommandDraft => ({
    actionType,
    closeTerminalOnExit: actionType === "terminal" ? closeTerminalOnExit : false,
    command: actionType === "terminal" ? command.trim() : undefined,
    commandId: draft.commandId,
    icon,
    links:
      actionType === "terminal"
        ? links
            .map((link) => ({ target: link.target, url: link.url.trim() }))
            .filter((link) => link.url.length > 0)
        : undefined,
    name: trimmedName,
    playCompletionSound: actionType === "terminal" ? playCompletionSound : false,
    showOnProjectRow,
    url: actionType === "browser" ? url.trim() : undefined,
  });

  const updateLink = (index: number, update: Partial<SidebarCommandLink>) => {
    setLinks((currentLinks) =>
      currentLinks.map((link, linkIndex) =>
        linkIndex === index ? { ...link, ...update } : link,
      ),
    );
  };

  return (
    <>
      {isActionTypeLocked ? null : (
        <Field className="gap-2.5">
          <FieldContent>
            <FieldLabel className="text-sm" htmlFor={actionTypeId}>
              Type
            </FieldLabel>
          </FieldContent>
          <SettingsSelect
            onValueChange={(value) => {
              const nextActionType = value === "browser" ? "browser" : "terminal";
              setActionType(nextActionType);
              if (nextActionType === "browser" && url.trim().length === 0) {
                setUrl(DEFAULT_BROWSER_ACTION_URL);
              }
            }}
            value={actionType}
          >
            <SelectTrigger className="h-8 w-full px-3 text-[13px]" id={actionTypeId}>
              <SelectValue />
            </SelectTrigger>
            <SettingsSelectContent>
              <SelectGroup>
                <SelectItem value="terminal">Terminal</SelectItem>
                <SelectItem value="browser">Browser</SelectItem>
              </SelectGroup>
            </SettingsSelectContent>
          </SettingsSelect>
        </Field>
      )}
      <Field className="gap-2.5" data-invalid={hasDuplicateTitle || undefined}>
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={nameId}>
            Text
          </FieldLabel>
        </FieldContent>
        <SettingsInput
          autoFocus
          aria-invalid={hasDuplicateTitle || undefined}
          className="h-8 px-3 text-[13px]"
          id={nameId}
          onChange={(event) => setName(event.currentTarget.value)}
          placeholder={actionType === "browser" ? "Docs" : "Dev"}
          value={name}
        />
        {hasDuplicateTitle ? (
          <FieldDescription className="text-sm">
            Another action already uses this title.
          </FieldDescription>
        ) : null}
      </Field>
      <CommandIconPicker
        icon={icon}
        onIconChange={setIcon}
      />
      {actionType === "browser" ? (
        <Field className="gap-2.5">
          <FieldContent>
            <FieldLabel className="text-sm" htmlFor={urlId}>
              URL
            </FieldLabel>
          </FieldContent>
          <SettingsTextarea
            id={urlId}
            onChange={(event) => setUrl(event.currentTarget.value)}
            placeholder={DEFAULT_BROWSER_ACTION_URL}
            rows={3}
            value={url}
          />
        </Field>
      ) : (
        <>
          <Field className="gap-2.5">
            <FieldContent>
              <FieldLabel className="text-sm" htmlFor={commandId}>
                Command
              </FieldLabel>
            </FieldContent>
            <SettingsTextarea
              id={commandId}
              onChange={(event) => setCommand(event.currentTarget.value)}
              placeholder="vp dev"
              rows={3}
              value={command}
            />
          </Field>
          <Field className="items-center justify-between" orientation="horizontal">
            <FieldContent>
              <FieldLabel className="text-sm" htmlFor={closeTerminalOnExitId}>
                Close terminal after the command finishes
              </FieldLabel>
            </FieldContent>
            <Switch
              checked={closeTerminalOnExit}
              id={closeTerminalOnExitId}
              onCheckedChange={setCloseTerminalOnExit}
            />
          </Field>
          <Field className="items-center justify-between" orientation="horizontal">
            <FieldContent>
              <FieldLabel className="text-sm" htmlFor={soundId}>
                Play completion sound
              </FieldLabel>
            </FieldContent>
            <Switch
              checked={playCompletionSound}
              id={soundId}
              onCheckedChange={setPlayCompletionSound}
            />
          </Field>
          {/*
           * CDXC:ProjectActions 2026-07-31-12:00:
           * Terminal actions can open saved links whenever they run, so a dev
           * action can start the server and bring up its localhost URL in the
           * same click. Each link picks the project's integrated browser or the
           * user's default external browser.
           */}
          <Field className="gap-2.5">
            <FieldContent>
              <FieldLabel className="text-sm">Open links when this action runs</FieldLabel>
              <FieldDescription className="text-sm">
                Open saved URLs, like your dev server&apos;s localhost address, alongside the
                command. Each link can open in the project&apos;s integrated browser or your
                default browser.
              </FieldDescription>
            </FieldContent>
            {links.length > 0 ? (
              <div className="flex flex-col gap-2">
                {links.map((link, index) => (
                  <div className="flex items-center gap-2" key={index}>
                    <SettingsInput
                      aria-label={`Link ${index + 1} URL`}
                      autoFocus={link.url.length === 0}
                      className="h-8 min-w-0 flex-1 px-3 text-[13px]"
                      onChange={(event) => updateLink(index, { url: event.currentTarget.value })}
                      placeholder={DEFAULT_BROWSER_ACTION_URL}
                      value={link.url}
                    />
                    <SettingsSelect
                      onValueChange={(value) =>
                        updateLink(index, {
                          target: value === "external" ? "external" : "integrated",
                        })
                      }
                      value={link.target}
                    >
                      <SelectTrigger
                        aria-label={`Link ${index + 1} target`}
                        className="h-8 w-44 shrink-0 px-3 text-[13px]"
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SettingsSelectContent>
                        <SelectGroup>
                          <SelectItem value="integrated">Integrated browser</SelectItem>
                          <SelectItem value="external">External browser</SelectItem>
                        </SelectGroup>
                      </SettingsSelectContent>
                    </SettingsSelect>
                    <Button
                      aria-label={`Remove link ${index + 1}`}
                      onClick={() =>
                        setLinks((currentLinks) =>
                          currentLinks.filter((_, linkIndex) => linkIndex !== index),
                        )
                      }
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <IconX aria-hidden="true" />
                    </Button>
                  </div>
                ))}
              </div>
            ) : null}
            <Button
              className="self-start"
              onClick={() => setLinks([...links, { target: "integrated", url: "" }])}
              type="button"
              variant="outline"
            >
              <IconPlus aria-hidden="true" data-icon="inline-start" />
              Add link
            </Button>
          </Field>
        </>
      )}
      {/*
       * CDXC:ProjectActions 2026-08-01:
       * Both terminal and browser actions can opt into the project's sidebar
       * row, so this toggle lives outside the action-type branch above.
       */}
      <Field className="items-center justify-between" orientation="horizontal">
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={showOnProjectRowId}>
            Show on the project&apos;s sidebar row
          </FieldLabel>
        </FieldContent>
        <Switch
          checked={showOnProjectRow}
          id={showOnProjectRowId}
          onCheckedChange={setShowOnProjectRow}
        />
      </Field>
      {/*
       * CDXC:ActionsSettings 2026-06-18-10:11:
       * Settings > Actions must let users delete any selected action from the edit surface itself, including default Build/Test actions whose deletion is represented by deletedDefaultCommandIds. Keep this wired to the same deleteSidebarCommand path as the row trash button so default and custom actions share one behavior.
       */}
      <div className="flex items-center justify-between gap-3">
        {onDelete ? (
          <Button onClick={onDelete} type="button" variant="destructive">
            <IconTrash aria-hidden="true" data-icon="inline-start" />
            Delete
          </Button>
        ) : (
          <span aria-hidden="true" />
        )}
        <div className="flex justify-end gap-3">
          <Button onClick={onCancel} type="button" variant="outline">
            Cancel
          </Button>
          <SettingButton
            disabled={isSaveDisabled}
            disabledReason={
              hasDuplicateTitle
                ? "Choose a unique action title."
                : actionType === "browser"
                  ? "Enter a URL first."
                  : "Enter a command first."
            }
            onClick={() => onSave(getDraft())}
            type="button"
          >
            Save
          </SettingButton>
        </div>
      </div>
    </>
  );
}

export function getSettingsCommandDraftTitle({
  actionType,
  command,
  name,
  url,
}: {
  actionType: SidebarActionType;
  command: string;
  name: string;
  url: string;
}): string {
  const normalizedName = normalizeSettingsCommandTitle(name);
  if (normalizedName) {
    return normalizedName;
  }
  const target = normalizeSettingsCommandTitle(actionType === "browser" ? url : command);
  return target?.slice(0, 20) ?? "";
}

export function getSettingsCommandButtonTitle(command: SidebarCommandButton): string {
  const normalizedName = normalizeSettingsCommandTitle(command.name);
  if (normalizedName) {
    return normalizedName;
  }
  const target = normalizeSettingsCommandTitle(command.command ?? command.url);
  return target?.slice(0, 20) ?? "";
}

export function getSettingsCommandTitleKey(value: string | undefined): string {
  return normalizeSettingsCommandTitle(value)?.toLocaleLowerCase() ?? "";
}

export function normalizeSettingsCommandTitle(value: string | undefined): string | undefined {
  const normalized = value?.trim().replace(/\s+/g, " ");
  return normalized ? normalized : undefined;
}


export function SettingsActionIcon({ command }: { command: SidebarCommandButton }) {
  return (
    <SidebarCommandIconGlyph
      icon={command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON}
      stroke={1.8}
    />
  );
}

export function getActionTitle(command: SidebarCommandButton): string {
  const name = command.name.trim();
  if (name.length > 0) {
    return name;
  }

  const target = getActionTarget(command);
  return target ?? "Untitled Action";
}

export function getActionMeta(command: SidebarCommandButton): string {
  const target = getActionTarget(command);
  const typeLabel = command.actionType === "browser" ? "Browser" : "Terminal";
  if (!target) {
    return `${typeLabel} - Not configured`;
  }

  return `${typeLabel} - ${target}`;
}

export function getActionTarget(command: SidebarCommandButton): string | undefined {
  const target = command.actionType === "browser" ? command.url?.trim() : command.command?.trim();
  if (!target) {
    return undefined;
  }

  return target.split("\n")[0] || undefined;
}

export function createSettingsCommandDraft(actionType: SidebarActionType): SettingsCommandDraft {
  return {
    actionType,
    closeTerminalOnExit: false,
    command: actionType === "terminal" ? "" : undefined,
    commandId: undefined,
    icon: DEFAULT_SIDEBAR_COMMAND_ICON,
    links: [],
    name: "",
    playCompletionSound: actionType === "terminal",
    showOnProjectRow: false,
    url: actionType === "browser" ? DEFAULT_BROWSER_ACTION_URL : undefined,
  };
}

export function createSettingsCommandDraftFromButton(command: SidebarCommandButton): SettingsCommandDraft {
  return {
    actionType: command.actionType,
    closeTerminalOnExit: command.closeTerminalOnExit,
    command: command.command,
    commandId: command.commandId,
    icon: command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
    links: command.links ?? [],
    name: command.name,
    playCompletionSound: command.playCompletionSound,
    showOnProjectRow: command.showOnProjectRow,
    url: command.url,
  };
}
