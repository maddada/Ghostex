import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import { useEffect, useId, useMemo, useState, type ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Command } from '@/packages/components/ui/command';
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/packages/components/ui/empty';
import { Field, FieldContent, FieldDescription, FieldLabel } from '@/packages/components/ui/field';
import { SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { AppTooltip } from '../../app-tooltip';
import { DisabledSettingControlTooltip } from '../../disabled-setting-control-tooltip';
import {
  IconAlertTriangle,
  IconChevronRight,
  IconCircleCheckFilled,
  IconCircleX,
  IconCodeDots,
  IconDownload,
  IconGripVertical,
  IconInfoCircle,
  IconPencil,
  IconPlus,
  IconRefresh,
  IconTrash,
} from '@tabler/icons-react';
import {
  type SidebarAgentHookStatusMessage,
  type SidebarAgentHookStatusItem,
  type SidebarGhostexCliStatusMessage,
} from '../../../shared/session-grid-contract';
import {
  DEFAULT_ghostex_SETTINGS,
  PREFERRED_AGENT_INTERFACE_INHERIT_VALUE,
  SESSION_TITLE_GENERATION_AGENT_OPTIONS,
  getPreferredAgentInterfaceOverrideOptions,
  getSessionTitleGenerationCommandPreview,
  type PreferredAgentInterface,
  type SessionTitleGenerationAgent,
} from '../../../shared/ghostex-settings';
import {
  AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS,
  supportsAgentAcceptAll,
  type AgentAcceptAllMode,
} from '../../../shared/sidebar-agent-accept-all';
import {
  DEFAULT_SIDEBAR_AGENTS,
  getDefaultSidebarAgentByIcon,
  type SidebarAgentButton,
  type SidebarAgentIcon,
} from '../../../shared/sidebar-agents';
import { AgentChatViewSupportBadge, agentSupportsChatView } from '../../agent-menu-chat-indicator';
import { getBrandAgentLogoStyle } from '../../agent-logos';
import { useSidebarStore } from '../../sidebar-store';
import { type AgentConfigDraft } from '../../agent-config-modal';
import { type WebviewApi } from '../../webview-api';
import {
  createSettingsAgentDragData,
  createSettingsReorderRequestId,
  getSettingsAgentDragData,
  mergeIds,
  moveId,
  reconcileDraftIds,
} from '../drag-data';
import {
  DisabledCommandPreviewField,
  SelectField,
  SettingButton,
  SettingSwitch,
  SettingsInput,
  SettingsNativeScrollArea,
  SettingsSection,
  SettingsSelect,
  SettingsSelectContent,
  SettingsTextarea,
  StaticNoteField,
  TextField,
  setSettingsSortableRowElement,
} from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';
import { AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS } from '../types';

export type SettingsAgentEditorState = {
  draft: AgentConfigDraft;
};

export const AGENT_TYPE_SELECT_ITEMS = [
  { label: 'Custom', value: 'custom' },
  ...DEFAULT_SIDEBAR_AGENTS.map((agent) => ({
    label: agent.name,
    value: agent.icon,
  })),
];

export function hasRemovableAgentHooks(agentHookStatus: SidebarAgentHookStatusMessage | undefined): boolean {
  if (!agentHookStatus || agentHookStatus.errorMessage) {
    return false;
  }
  return agentHookStatus.agents.some(hasRemovableAgentHookStatus);
}

export function hasInstalledBundledAgentSkills(ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined): boolean {
  return (
    ghostexCliStatus?.cliSkillInstalled === true ||
    ghostexCliStatus?.browserSkillInstalled === true ||
    ghostexCliStatus?.embeddedBrowserSkillInstalled === true ||
    ghostexCliStatus?.computerUseSkillInstalled === true ||
    ghostexCliStatus?.fable56OrchestrationSkillInstalled === true ||
    ghostexCliStatus?.manageBeadsSkillInstalled === true ||
    ghostexCliStatus?.generateTitleSkillInstalled === true ||
    ghostexCliStatus?.moveCodexSessionSkillInstalled === true
  );
}

export function AgentsSettingsTab({
  agentHookStatus,
  agentHookStatusLoading,
  agentAcceptAllEnabled,
  customSessionTitleGenerationCommand,
  defaultPromptAgentId,
  preferredAgentInterface,
  preferredAgentInterfaceOverrides,
  sessionTitleGenerationAgent,
  onAgentAcceptAllEnabledChange,
  onCustomSessionTitleGenerationCommandChange,
  onDefaultPromptAgentIdChange,
  onInstallAgentHooks,
  onPreferredAgentInterfaceOverridesChange,
  onRequestAgentHookStatus,
  onSessionTitleGenerationAgentChange,
  onUninstallAgentHooks,
  search,
  searchEmptyState,
  vscode,
}: {
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentHookStatusLoading: boolean;
  agentAcceptAllEnabled: boolean;
  customSessionTitleGenerationCommand: string;
  defaultPromptAgentId: string;
  preferredAgentInterface: PreferredAgentInterface;
  preferredAgentInterfaceOverrides: Readonly<Record<string, PreferredAgentInterface>>;
  sessionTitleGenerationAgent: SessionTitleGenerationAgent;
  onAgentAcceptAllEnabledChange: (checked: boolean) => void;
  onCustomSessionTitleGenerationCommandChange: (command: string) => void;
  onDefaultPromptAgentIdChange: (agentId: string) => void;
  onInstallAgentHooks?: () => void;
  onPreferredAgentInterfaceOverridesChange: (overrides: Readonly<Record<string, PreferredAgentInterface>>) => void;
  onRequestAgentHookStatus?: () => void;
  onSessionTitleGenerationAgentChange: (agent: SessionTitleGenerationAgent) => void;
  onUninstallAgentHooks?: (agentIds?: readonly string[]) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  vscode?: WebviewApi;
}) {
  const agents = useSidebarStore((state) => state.hud.agents);
  const acceptAllToggleId = useId();
  const agentHooksAvailableForUninstall = hasRemovableAgentHooks(agentHookStatus);
  const [editorState, setEditorState] = useState<SettingsAgentEditorState>();
  const [draftAgentIds, setDraftAgentIds] = useState<string[]>();

  useEffect(() => {
    setDraftAgentIds((previousDraft) => reconcileDraftIds(previousDraft, agents, 'agentId'));
  }, [agents]);

  const orderedAgents = useMemo(() => {
    const agentById = new Map(agents.map((agent) => [agent.agentId, agent]));
    const orderedAgentIds = draftAgentIds
      ? mergeIds(
          draftAgentIds,
          agents.map((agent) => agent.agentId)
        )
      : agents.map((agent) => agent.agentId);

    return orderedAgentIds
      .map((agentId) => agentById.get(agentId))
      .filter((agent): agent is SidebarAgentButton => agent !== undefined);
  }, [agents, draftAgentIds]);
  const promptAgentOptions = useMemo(
    () =>
      agents
        .filter((agent) => Boolean(agent.command?.trim()))
        .map((agent) => ({ label: agent.name.trim() || agent.agentId, value: agent.agentId })),
    [agents]
  );
  const normalizedDefaultPromptAgentId = defaultPromptAgentId.trim() || DEFAULT_ghostex_SETTINGS.defaultPromptAgentId;
  const promptAgentHasSavedDefault = promptAgentOptions.some(
    (option) => option.value === normalizedDefaultPromptAgentId
  );
  const promptAgentSelectOptions = promptAgentHasSavedDefault
    ? promptAgentOptions
    : [
        /*
         * CDXC:GxserverAgentSettings 2026-06-19-08:58:
         * Default Prompt Agent is gxserver-owned and may name a custom or hidden
         * agent before the local launcher registry has a command for it. Show
         * that saved id as unavailable instead of rendering Codex as selected,
         * so Settings never silently rewrites or masks the canonical choice.
         */
        {
          label: `Unavailable (${normalizedDefaultPromptAgentId})`,
          value: normalizedDefaultPromptAgentId,
        },
        ...promptAgentOptions,
      ];
  const selectedDefaultPromptAgentId = normalizedDefaultPromptAgentId;
  const titleGenerationCommandPreview = getSessionTitleGenerationCommandPreview(sessionTitleGenerationAgent, {
    command: resolveSettingsTitleGenerationCommand(
      sessionTitleGenerationAgent,
      orderedAgents,
      customSessionTitleGenerationCommand
    ),
  });
  const hookStatusByAgentId = useMemo(
    () => new Map(agentHookStatus?.agents.map((status) => [status.agentId, status]) ?? []),
    [agentHookStatus]
  );
  const installedHookCount = agentHookStatus?.agents.filter((status) => status.status === 'installed').length ?? 0;
  const updateRequiredHookCount =
    agentHookStatus?.agents.filter((status) => status.status === 'updateRequired').length ?? 0;
  const updateRequiredHookSummary =
    updateRequiredHookCount === 1 ? '1 needs update' : `${updateRequiredHookCount} need update`;
  const hookStatusSummary = agentHookStatus
    ? agentHookStatus.errorMessage
      ? 'Unable to check hooks'
      : updateRequiredHookCount > 0
        ? `${installedHookCount}/${AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.length} hooks ready, ${updateRequiredHookSummary}`
        : `${installedHookCount}/${AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.length} hooks ready`
    : agentHookStatusLoading
      ? 'Checking hooks'
      : 'Hook status not checked';

  /*
   * CDXC:PerAgentDefaultView 2026-08-27:
   * Inherit is stored as an absent key, never as a third stored value, so an
   * agent the user never touched keeps following the global Default Agent View
   * when that global setting changes later.
   */
  const setPreferredAgentInterfaceOverride = (agentId: string, next: PreferredAgentInterface | undefined) => {
    const overrides: Record<string, PreferredAgentInterface> = { ...preferredAgentInterfaceOverrides };
    if (next) {
      overrides[agentId] = next;
    } else {
      delete overrides[agentId];
    }
    onPreferredAgentInterfaceOverridesChange(overrides);
  };

  const saveAgent = (draft: AgentConfigDraft) => {
    if (!vscode) {
      return;
    }
    vscode.postMessage({
      acceptAllMode: draft.acceptAllMode,
      agentId: draft.agentId,
      command: draft.command,
      icon: draft.icon,
      name: draft.name,
      type: 'saveSidebarAgent',
    });
    setEditorState(undefined);
  };

  const deleteAgent = (agent: SidebarAgentButton) => {
    vscode?.postMessage({
      agentId: agent.agentId,
      type: 'deleteSidebarAgent',
    });
  };

  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getSettingsAgentDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex = 'index' in source && typeof source.index === 'number' ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    const nextAgentIds = moveId(
      orderedAgents.map((agent) => agent.agentId),
      source.initialIndex,
      targetIndex
    );
    setDraftAgentIds(nextAgentIds);
    vscode?.postMessage({
      agentIds: nextAgentIds,
      requestId: createSettingsReorderRequestId('agents'),
      type: 'syncSidebarAgentOrder',
    });
  }) satisfies DragDropEventHandlers['onDragEnd'];

  return (
    <SettingsNativeScrollArea className='h-full min-h-0'>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
        {!editorState && shouldShowSettingsSection(search.sections.agentHooks) ? (
          <SettingsSection title='Agent Hooks'>
            <details className='group w-full' open={search.tab.isSearching || undefined}>
              {/*
               * CDXC:AgentHookSettings 2026-05-23-10:05:
               * Settings -> Agents starts with a collapsed hook setup panel so reliable-resume requirements are discoverable without pushing normal agent ordering/editing controls down the tab. The panel covers every current Ghostex CLI resume-hook agent.
               *
               * CDXC:AgentHookSettings 2026-06-11-17:45:
               * The collapsed header must use the same field label/description typography and bordered row spacing as the other Agents settings rows. The disclosure chevron points right when collapsed and rotates down when expanded.
               *
               * CDXC:AgentHookSettings 2026-06-12-04:34:
               * The hook setup UI should use the same labeled section card chrome as the Agents management list below so the Agents tab scans as consistent grouped settings instead of a loose disclosure row followed by a bordered list.
               */}
              <summary className='settings-management-row flex cursor-pointer list-none items-center justify-between gap-3 border border-border bg-muted/20 px-3 py-3 marker:hidden [&::-webkit-details-marker]:hidden'>
                <div className='flex min-w-0 flex-1 items-center gap-2.5'>
                  <IconChevronRight
                    aria-hidden='true'
                    className='size-4 shrink-0 text-muted-foreground transition-transform duration-150 group-open:rotate-90'
                  />
                  <FieldContent className='min-w-0 gap-1'>
                    <FieldLabel className='text-sm'>Agent resume hooks</FieldLabel>
                    <FieldDescription className='text-xs text-muted-foreground'>{hookStatusSummary}</FieldDescription>
                  </FieldContent>
                </div>
                <span className='flex shrink-0 items-center'>
                  <AgentHookStatusIcon isLoading={agentHookStatusLoading} status={undefined} />
                </span>
              </summary>
              <div className='mt-3 flex flex-col gap-4 border border-border/80 bg-muted/10 px-4 pb-4 pt-4'>
                <div className='space-y-2 text-xs leading-5 text-muted-foreground'>
                  <p>
                    Install hooks so Ghostex can capture each agent&apos;s native session id and resume the exact
                    conversation after sleep, reload, or app restart.
                  </p>
                  <p>
                    Hooks write only session metadata into Ghostex&apos;s session-state files. The existing title-based
                    restore path remains available when a hook has not captured an id yet.
                  </p>
                </div>
                <div className='flex flex-wrap gap-2'>
                  <SettingButton
                    disabled={!onInstallAgentHooks || agentHookStatusLoading}
                    disabledReason={
                      agentHookStatusLoading
                        ? 'Hook status is being checked.'
                        : 'Hook installation isn’t available here.'
                    }
                    onClick={onInstallAgentHooks}
                    type='button'
                    variant='outline'
                  >
                    <IconDownload aria-hidden='true' data-icon='inline-start' />
                    {updateRequiredHookCount > 0 ? 'Update Hooks' : 'Install Hooks'}
                  </SettingButton>
                  {/*
                   * CDXC:AgentHookSettings 2026-08-19-11:20:
                   * Hook removal lives beside the install control it undoes: one Uninstall All for the whole set, plus an icon-only remove on each installed agent row. Both stay disabled while status is loading or when no Ghostex-owned hook is present, so users cannot fire a no-op removal.
                   */}
                  <SettingButton
                    disabled={agentHookStatusLoading || !agentHooksAvailableForUninstall || !onUninstallAgentHooks}
                    disabledReason={
                      agentHookStatusLoading
                        ? 'Hook status is being checked.'
                        : !agentHooksAvailableForUninstall
                          ? 'No Ghostex hooks are installed.'
                          : 'Hook removal isn’t available here.'
                    }
                    onClick={() => onUninstallAgentHooks?.()}
                    type='button'
                    variant='outline'
                  >
                    <IconTrash aria-hidden='true' data-icon='inline-start' />
                    Uninstall All
                  </SettingButton>
                  <SettingButton
                    disabled={!onRequestAgentHookStatus || agentHookStatusLoading}
                    disabledReason={
                      agentHookStatusLoading
                        ? 'Hook status is being checked.'
                        : 'Hook status refresh isn’t available here.'
                    }
                    onClick={onRequestAgentHookStatus}
                    type='button'
                    variant='ghost'
                  >
                    <IconRefresh aria-hidden='true' data-icon='inline-start' />
                    Refresh
                  </SettingButton>
                </div>
                <div className='flex flex-col gap-2'>
                  {agentHookStatus?.errorMessage ? (
                    <div className='rounded-none border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive'>
                      {agentHookStatus.errorMessage}
                    </div>
                  ) : null}
                  {AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.map((agent) => (
                    <AgentHookStatusRow
                      agent={{
                        agentId: agent.agentId,
                        command: agent.command,
                        icon: agent.icon,
                        isDefault: true,
                        name: agent.name,
                      }}
                      isLoading={agentHookStatusLoading && !agentHookStatus}
                      isStatusLoading={agentHookStatusLoading}
                      key={agent.agentId}
                      onPreferredInterfaceOverrideChange={(next) =>
                        setPreferredAgentInterfaceOverride(agent.agentId, next)
                      }
                      onUninstall={onUninstallAgentHooks ? () => onUninstallAgentHooks([agent.agentId]) : undefined}
                      preferredAgentInterface={preferredAgentInterface}
                      preferredInterfaceOverride={preferredAgentInterfaceOverrides[agent.agentId]}
                      status={hookStatusByAgentId.get(agent.agentId)}
                    />
                  ))}
                </div>
                {agentHookStatus ? (
                  <FieldDescription className='truncate text-[11px] text-muted-foreground'>
                    Hook state: {agentHookStatus.hookStateDirectory}
                  </FieldDescription>
                ) : null}
              </div>
            </details>
          </SettingsSection>
        ) : null}
        {!editorState && shouldShowSettingsSection(search.sections.config) ? (
          <SettingsSection title='Config'>
            {/*
             * CDXC:AgentConfigSettings 2026-06-12-04:40:
             * Default prompt, title generation, custom title command, and global Accept All are configuration controls, not agent management rows. Group them under the same labeled SettingsSection chrome as Agent Hooks and Agents so the Agents tab scans as three consistent areas: hooks, config, and launchers.
             */}
            {!shouldShowSetting(search.sections.config, 'defaultPromptAgent') ? null : promptAgentOptions.length > 0 ? (
              <SelectField
                description='Choose the agent used by Git helper prompts, project board Start Work, and the default worktree first-prompt selection.'
                isModified={defaultPromptAgentId !== DEFAULT_ghostex_SETTINGS.defaultPromptAgentId}
                label='Default Prompt Agent'
                onChange={onDefaultPromptAgentIdChange}
                onResetToDefault={() => onDefaultPromptAgentIdChange(DEFAULT_ghostex_SETTINGS.defaultPromptAgentId)}
                options={promptAgentSelectOptions}
                value={selectedDefaultPromptAgentId}
              />
            ) : (
              <StaticNoteField
                description='Configure at least one CLI agent before selecting a default prompt agent.'
                label='Default Prompt Agent'
              />
            )}
            {/*
             * CDXC:GxserverSessionTitle 2026-06-04-08:24:
             * First-prompt session-title generation needs its own agent selector instead of reusing Default Prompt Agent, because title generation is a gxserver-owned background job while prompt-launch defaults affect Git helpers, project-board prompts, and worktree starts.
             *
             * CDXC:GxserverSessionTitle 2026-06-04-22:44:
             * Show the disabled command preview directly under the selector so users can inspect the exact Codex, Cursor CLI, Claude, Grok Build, or Custom command template before Ghostex sends a background title-generation prompt.
             */}
            {shouldShowSetting(search.sections.config, 'titleGenerationAgent') ? (
              <SelectField
                description='Choose the headless agent Ghostex uses for first-prompt session title generation.'
                isModified={sessionTitleGenerationAgent !== DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent}
                label='Title Generation Agent'
                onChange={(value) => onSessionTitleGenerationAgentChange(value as SessionTitleGenerationAgent)}
                onResetToDefault={() =>
                  onSessionTitleGenerationAgentChange(DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent)
                }
                options={SESSION_TITLE_GENERATION_AGENT_OPTIONS}
                value={sessionTitleGenerationAgent}
              />
            ) : null}
            {shouldShowSetting(search.sections.config, 'titleGenerationCommand') ? (
              <DisabledCommandPreviewField
                description='Preview of the command Ghostex sends to generate automatic first-prompt session titles.'
                label='Title Generation Command'
                value={titleGenerationCommandPreview}
              />
            ) : null}
            {sessionTitleGenerationAgent === 'custom' &&
            shouldShowSetting(search.sections.config, 'customTitleCommand') ? (
              <TextField
                description='Run this command with the title prompt on stdin. It should print only the title.'
                isModified={
                  customSessionTitleGenerationCommand !== DEFAULT_ghostex_SETTINGS.customSessionTitleGenerationCommand
                }
                label='Custom Title Command'
                onChange={onCustomSessionTitleGenerationCommandChange}
                onResetToDefault={() =>
                  onCustomSessionTitleGenerationCommandChange(
                    DEFAULT_ghostex_SETTINGS.customSessionTitleGenerationCommand
                  )
                }
                placeholder='title-generator'
                value={customSessionTitleGenerationCommand}
              />
            ) : null}
            {shouldShowSetting(search.sections.config, 'acceptAll') ? (
              <Field
                className='items-center justify-between rounded-none border border-border bg-muted/20 px-4 py-3'
                orientation='horizontal'
              >
                <FieldContent>
                  <FieldLabel className='text-sm' htmlFor={acceptAllToggleId}>
                    Accept All
                  </FieldLabel>
                  <FieldDescription className='text-xs text-muted-foreground'>
                    Enable each supported agent&apos;s permission-bypass mode when launching sessions. Per-agent
                    settings can inherit or override this default.
                  </FieldDescription>
                </FieldContent>
                <SettingSwitch
                  checked={agentAcceptAllEnabled}
                  disabled={!vscode}
                  disabledReason='This change needs the Ghostex app connection.'
                  id={acceptAllToggleId}
                  onCheckedChange={onAgentAcceptAllEnabledChange}
                />
              </Field>
            ) : null}
          </SettingsSection>
        ) : null}
        {editorState || shouldShowSettingsSection(search.sections.agentList) ? (
          <SettingsSection
            actions={
              !editorState ? (
                <SettingButton
                  disabled={!vscode}
                  disabledReason='Adding agents needs the Ghostex app connection.'
                  onClick={() => setEditorState({ draft: { command: '', name: '' } })}
                  type='button'
                  variant='outline'
                >
                  <IconPlus aria-hidden='true' data-icon='inline-start' />
                  Add Agent
                </SettingButton>
              ) : null
            }
            title={editorState ? 'Agent' : 'Agents'}
          >
            {editorState ? (
              <AgentSettingsEditor
                draft={editorState.draft}
                onCancel={() => setEditorState(undefined)}
                onSave={saveAgent}
              />
            ) : (
              <>
                {orderedAgents.length > 0 ? (
                  <DragDropProvider onDragEnd={handleDragEnd}>
                    <div className='flex flex-col gap-2'>
                      {orderedAgents.map((agent, index) => (
                        <SettingsAgentRow
                          agent={agent}
                          index={index}
                          key={agent.agentId}
                          onDelete={() => deleteAgent(agent)}
                          onEdit={() =>
                            setEditorState({
                              draft: {
                                acceptAllMode: agent.acceptAllMode ?? 'inherit',
                                agentId: agent.agentId,
                                command: agent.command ?? '',
                                icon: agent.icon,
                                name: agent.name,
                              },
                            })
                          }
                        />
                      ))}
                    </div>
                  </DragDropProvider>
                ) : (
                  <Empty className='border border-border bg-muted/20'>
                    <EmptyHeader>
                      <EmptyTitle>No agents configured</EmptyTitle>
                      <EmptyDescription>Add an agent launcher to start new sessions.</EmptyDescription>
                    </EmptyHeader>
                  </Empty>
                )}
              </>
            )}
          </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

export function resolveSettingsTitleGenerationCommand(
  agent: SessionTitleGenerationAgent,
  agents: readonly SidebarAgentButton[],
  customCommand: string
): string | undefined {
  if (agent === 'custom') {
    return customCommand.trim();
  }
  return agents.find((candidate) => candidate.agentId === agent)?.command?.trim();
}

export function AgentHookStatusRow({
  agent,
  isLoading,
  isStatusLoading,
  onPreferredInterfaceOverrideChange,
  onUninstall,
  preferredAgentInterface,
  preferredInterfaceOverride,
  status,
}: {
  agent: SidebarAgentButton;
  isLoading: boolean;
  isStatusLoading: boolean;
  onPreferredInterfaceOverrideChange?: (preferredInterface: PreferredAgentInterface | undefined) => void;
  onUninstall?: () => void;
  preferredAgentInterface?: PreferredAgentInterface;
  preferredInterfaceOverride?: PreferredAgentInterface;
  status?: SidebarAgentHookStatusItem;
}) {
  const statusText = getAgentHookStatusText(status, isLoading);
  const removable = hasRemovableAgentHookStatus(status);
  const uninstallDisabled = isStatusLoading || !onUninstall;
  const supportsChatView = agentSupportsChatView(agent);
  return (
    <div className='flex items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2'>
      <div className='flex min-w-0 flex-1 items-center gap-3'>
        <span
          aria-hidden='true'
          className='settings-management-icon flex size-8 shrink-0 items-center justify-center bg-muted'
        >
          <SettingsAgentIcon agent={agent} />
        </span>
        <span className='min-w-0'>
          {/*
           * CDXC:PerAgentDefaultView 2026-08-27:
           * The chat-bubble badge sits with the agent name, not with the hook
           * status pill: it describes the agent, not its hook state, and the
           * two must not read as one combined status. Terminal-only agents get
           * no badge at all rather than a negative one.
           */}
          <span className='flex min-w-0 items-center gap-1.5'>
            <span className='truncate text-sm font-medium'>{agent.name}</span>
            <AgentChatViewSupportBadge agent={agent} />
          </span>
          <span className='block truncate text-xs text-muted-foreground'>
            {status?.detail ?? agent.command ?? 'Waiting for hook check'}
          </span>
        </span>
      </div>
      {supportsChatView && preferredAgentInterface && onPreferredInterfaceOverrideChange ? (
        <AgentPreferredInterfaceOverrideSelect
          agentName={agent.name}
          onChange={onPreferredInterfaceOverrideChange}
          preferredAgentInterface={preferredAgentInterface}
          value={preferredInterfaceOverride}
        />
      ) : null}
      <div className='flex w-32 shrink-0 items-center justify-end gap-3'>
        <span
          className={cn(
            'flex shrink-0 items-center gap-1.5 rounded-none px-2 py-1 text-xs font-medium',
            getAgentHookStatusClassName(status, isLoading)
          )}
        >
          <AgentHookStatusIcon isLoading={isLoading} status={status} />
          {statusText}
        </span>
        {removable ? (
          <DisabledSettingControlTooltip
            disabled={uninstallDisabled}
            reason={isStatusLoading ? 'Hook status is being checked.' : 'Hook removal isn’t available here.'}
          >
            <AppTooltip content={`Uninstall ${agent.name} hook`}>
              <Button
                aria-label={`Uninstall ${agent.name} hook`}
                className='shrink-0'
                disabled={uninstallDisabled}
                onClick={onUninstall}
                size='icon'
                type='button'
                variant='destructive'
              >
                <IconTrash aria-hidden='true' />
              </Button>
            </AppTooltip>
          </DisabledSettingControlTooltip>
        ) : null}
      </div>
    </div>
  );
}

/*
 * CDXC:PerAgentDefaultView 2026-08-27:
 * Only chat-capable agents get this control. A terminal-only agent has no
 * second view to choose, so a disabled select there would be noise; its row
 * simply ends at the hook status.
 */
export function AgentPreferredInterfaceOverrideSelect({
  agentName,
  onChange,
  preferredAgentInterface,
  value,
}: {
  agentName: string;
  onChange: (preferredInterface: PreferredAgentInterface | undefined) => void;
  preferredAgentInterface: PreferredAgentInterface;
  value?: PreferredAgentInterface;
}) {
  const options = getPreferredAgentInterfaceOverrideOptions(preferredAgentInterface);
  return (
    <SettingsSelect
      items={options}
      onValueChange={(nextValue) =>
        onChange(
          nextValue === PREFERRED_AGENT_INTERFACE_INHERIT_VALUE ? undefined : (nextValue as PreferredAgentInterface)
        )
      }
      value={value ?? PREFERRED_AGENT_INTERFACE_INHERIT_VALUE}
    >
      <SelectTrigger aria-label={`Default view for ${agentName}`} className='h-7 w-[9.5rem] shrink-0 px-2 text-xs'>
        <SelectValue />
      </SelectTrigger>
      <SettingsSelectContent>
        <SelectGroup>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectGroup>
      </SettingsSelectContent>
    </SettingsSelect>
  );
}

export function hasRemovableAgentHookStatus(status: SidebarAgentHookStatusItem | undefined): boolean {
  if (!status) {
    return false;
  }
  return status.hookInstalled || status.status === 'installed' || status.status === 'updateRequired';
}

export function AgentHookStatusIcon({
  isLoading,
  status,
}: {
  isLoading: boolean;
  status?: SidebarAgentHookStatusItem;
}) {
  if (isLoading) {
    return <IconRefresh aria-hidden='true' className='size-3.5 animate-spin' />;
  }
  if (!status) {
    return <IconInfoCircle aria-hidden='true' className='size-3.5 text-muted-foreground' />;
  }
  switch (status.status) {
    case 'installed':
      return <IconCircleCheckFilled aria-hidden='true' className='size-3.5 text-emerald-400' />;
    case 'updateRequired':
      return <IconAlertTriangle aria-hidden='true' className='size-3.5 text-amber-400' />;
    case 'cliMissing':
      return <IconAlertTriangle aria-hidden='true' className='size-3.5 text-amber-400' />;
    case 'notRequired':
      return <IconInfoCircle aria-hidden='true' className='size-3.5 text-muted-foreground' />;
    case 'missing':
      return <IconCircleX aria-hidden='true' className='size-3.5 text-destructive' />;
  }
}

export function getAgentHookStatusText(status: SidebarAgentHookStatusItem | undefined, isLoading: boolean): string {
  if (isLoading) {
    return 'Checking';
  }
  if (!status) {
    return 'Not checked';
  }
  switch (status.status) {
    case 'installed':
      return 'Installed';
    case 'updateRequired':
      return 'Needs update';
    case 'cliMissing':
      return 'CLI missing';
    case 'notRequired':
      return 'Not required';
    case 'missing':
      return 'Missing';
  }
}

export function getAgentHookStatusClassName(
  status: SidebarAgentHookStatusItem | undefined,
  isLoading: boolean
): string {
  if (isLoading || !status) {
    return 'bg-muted text-muted-foreground';
  }
  switch (status.status) {
    case 'installed':
      return 'bg-emerald-500/10 text-emerald-300';
    case 'updateRequired':
      return 'bg-amber-500/10 text-amber-300';
    case 'cliMissing':
      return 'bg-amber-500/10 text-amber-300';
    case 'notRequired':
      return 'bg-muted text-muted-foreground';
    case 'missing':
      return 'bg-destructive/10 text-destructive';
  }
}

export function SettingsAgentRow({
  agent,
  index,
  onDelete,
  onEdit,
}: {
  agent: SidebarAgentButton;
  index: number;
  onDelete: () => void;
  onEdit: () => void;
}) {
  const sortable = useSortable({
    accept: 'settings-agent',
    data: createSettingsAgentDragData(agent.agentId),
    group: 'settings-agents',
    id: agent.agentId,
    index,
    type: 'settings-agent',
  });
  const { handleRef, isDragging } = sortable;

  const setRowRef = (element: HTMLDivElement | null) => {
    setSettingsSortableRowElement(sortable, element);
  };

  return (
    <div
      className='settings-management-row flex items-center gap-2 border border-border bg-muted/20 p-2'
      data-dragging={String(Boolean(isDragging))}
      ref={setRowRef}
    >
      <Button aria-label={`Reorder ${agent.name}`} ref={handleRef} size='icon-sm' type='button' variant='ghost'>
        <IconGripVertical aria-hidden='true' />
      </Button>
      <Button
        className='settings-management-edit-button h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-2 text-left'
        onClick={onEdit}
        type='button'
        variant='ghost'
      >
        <span
          aria-hidden='true'
          className='settings-management-icon flex size-9 shrink-0 items-center justify-center bg-muted'
        >
          <SettingsAgentIcon agent={agent} />
        </span>
        <span className='min-w-0 flex-1'>
          <span className='flex min-w-0 items-center gap-1.5'>
            <span className='truncate text-sm font-medium text-foreground'>{agent.name}</span>
            <AgentChatViewSupportBadge agent={agent} />
          </span>
          <span className='block truncate text-xs text-muted-foreground'>
            {agent.command?.trim() || 'Not configured'}
          </span>
        </span>
      </Button>
      <span className='settings-management-row-actions'>
        <Button aria-label={`Edit ${agent.name}`} onClick={onEdit} size='icon-sm' type='button' variant='ghost'>
          <IconPencil aria-hidden='true' />
        </Button>
        <Button
          aria-label={`Delete ${agent.name}`}
          onClick={onDelete}
          size='icon-sm'
          type='button'
          variant='destructive'
        >
          <IconTrash aria-hidden='true' />
        </Button>
      </span>
    </div>
  );
}

export function AgentSettingsEditor({
  draft,
  onCancel,
  onSave,
}: {
  draft: AgentConfigDraft;
  onCancel: () => void;
  onSave: (draft: AgentConfigDraft) => void;
}) {
  const [acceptAllMode, setAcceptAllMode] = useState<AgentAcceptAllMode>(draft.acceptAllMode ?? 'inherit');
  const [command, setCommand] = useState(draft.command);
  const [icon, setIcon] = useState<SidebarAgentIcon | 'custom'>(draft.icon ?? 'custom');
  const [name, setName] = useState(draft.name);
  const acceptAllModeId = useId();
  const agentTypeId = useId();
  const commandId = useId();
  const nameId = useId();
  const isSaveDisabled = name.trim().length === 0 || command.trim().length === 0;
  const resolvedAgentId =
    draft.agentId ?? getDefaultSidebarAgentByIcon(icon === 'custom' ? undefined : icon)?.agentId ?? '';
  const acceptAllSupported = supportsAgentAcceptAll(resolvedAgentId, icon === 'custom' ? undefined : icon);

  const updateAgentType = (value: string) => {
    const nextType = value as SidebarAgentIcon | 'custom';
    const previousDefaultAgent = getDefaultSidebarAgentByIcon(icon === 'custom' ? undefined : icon);
    const nextDefaultAgent = getDefaultSidebarAgentByIcon(nextType === 'custom' ? undefined : nextType);

    setIcon(nextType);
    if (!nextDefaultAgent) {
      return;
    }

    setName((previousName) =>
      previousName.trim().length === 0 || previousName === previousDefaultAgent?.name
        ? nextDefaultAgent.name
        : previousName
    );
    setCommand((previousCommand) =>
      previousCommand.trim().length === 0 || previousCommand === previousDefaultAgent?.command
        ? nextDefaultAgent.command
        : previousCommand
    );
  };

  return (
    <>
      <Field className='gap-2.5'>
        <FieldContent>
          <FieldLabel className='text-sm' htmlFor={agentTypeId}>
            Agent type
          </FieldLabel>
        </FieldContent>
        <SettingsSelect items={AGENT_TYPE_SELECT_ITEMS} onValueChange={updateAgentType} value={icon}>
          <SelectTrigger className='h-8 w-full px-3 text-[13px]' id={agentTypeId}>
            <SelectValue />
          </SelectTrigger>
          <SettingsSelectContent>
            <SelectGroup>
              <SelectItem value='custom'>Custom</SelectItem>
              {DEFAULT_SIDEBAR_AGENTS.map((agent) => (
                <SelectItem key={agent.agentId} value={agent.icon}>
                  {agent.name}
                </SelectItem>
              ))}
            </SelectGroup>
          </SettingsSelectContent>
        </SettingsSelect>
      </Field>
      <Field className='gap-2.5'>
        <FieldContent>
          <FieldLabel className='text-sm' htmlFor={nameId}>
            Name
          </FieldLabel>
        </FieldContent>
        <SettingsInput
          autoFocus
          className='h-8 px-3 text-[13px]'
          id={nameId}
          onChange={(event) => setName(event.currentTarget.value)}
          placeholder='Codex'
          value={name}
        />
      </Field>
      <Field className='gap-2.5'>
        <FieldContent>
          <FieldLabel className='text-sm' htmlFor={commandId}>
            Command
          </FieldLabel>
        </FieldContent>
        <SettingsTextarea
          id={commandId}
          onChange={(event) => setCommand(event.currentTarget.value)}
          placeholder='codex'
          rows={3}
          value={command}
        />
      </Field>
      <Field className='gap-2.5'>
        <FieldContent>
          <FieldLabel className='text-sm' htmlFor={acceptAllModeId}>
            Accept All
          </FieldLabel>
          <FieldDescription className='text-xs text-muted-foreground'>
            {acceptAllSupported
              ? "Inherit uses the global Agents setting. Accept All applies this agent's permission-bypass mode at launch without changing the stored command."
              : 'This agent does not expose a supported Accept All mode in Ghostex.'}
          </FieldDescription>
        </FieldContent>
        <SettingsSelect
          disabled={!acceptAllSupported}
          disabledReason='This agent doesn’t support Accept All.'
          disabledTooltipClassName='w-full'
          items={AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS}
          onValueChange={(value) => setAcceptAllMode(value as AgentAcceptAllMode)}
          value={acceptAllMode}
        >
          <SelectTrigger className='h-8 w-full px-3 text-[13px]' id={acceptAllModeId}>
            <SelectValue />
          </SelectTrigger>
          <SettingsSelectContent>
            <SelectGroup>
              {AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SettingsSelectContent>
        </SettingsSelect>
      </Field>
      <div className='flex justify-end gap-3'>
        <Button onClick={onCancel} type='button' variant='outline'>
          Cancel
        </Button>
        <SettingButton
          disabled={isSaveDisabled}
          disabledReason={
            name.trim().length === 0 && command.trim().length === 0
              ? 'Enter a name and command first.'
              : name.trim().length === 0
                ? 'Enter an agent name first.'
                : 'Enter an agent command first.'
          }
          onClick={() =>
            onSave({
              acceptAllMode,
              agentId: draft.agentId,
              command: command.trim(),
              icon: icon === 'custom' ? undefined : icon,
              name: name.trim(),
            })
          }
          type='button'
        >
          Save
        </SettingButton>
      </div>
    </>
  );
}

export function SettingsAgentIcon({ agent }: { agent: SidebarAgentButton }) {
  if (agent.icon) {
    return (
      <span
        aria-hidden='true'
        className='configure-agents-list-agent-icon'
        style={getBrandAgentLogoStyle(agent.icon)}
      />
    );
  }

  return <IconCodeDots aria-hidden='true' />;
}
