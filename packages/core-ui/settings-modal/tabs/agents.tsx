import { AccountsSettingsSection } from '@/packages/core-ui/accounts/manager';
import { DragDropProvider, type DragDropEventHandlers } from '@dnd-kit/react';
import { isSortableOperation, useSortable } from '@dnd-kit/react/sortable';
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Command } from '@/packages/components/ui/command';
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/packages/components/ui/empty';
import { Field, FieldContent, FieldDescription, FieldLabel } from '@/packages/components/ui/field';
import { SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { type SettingsAgentsSection } from '../../app-modal-host-bridge';
import { AppTooltip } from '../../app-tooltip';
import {
  IconAlertTriangle,
  IconChevronDown,
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
import { AgentApprovalPolicyControl } from '../../agent-approval-policy-control';
import { AgentTypeSelectOption } from '../../agent-type-select-option';
import { DisabledSettingControlTooltip } from '../../disabled-setting-control-tooltip';
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
  initialAgentsSection,
  isActive,
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
  /** Card a deep link scrolls to once the tab is active; see the bridge contract. */
  initialAgentsSection?: SettingsAgentsSection;
  isActive: boolean;
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
  onInstallAgentHooks?: (agentIds?: readonly string[]) => void;
  onPreferredAgentInterfaceOverridesChange: (overrides: Readonly<Record<string, PreferredAgentInterface>>) => void;
  onRequestAgentHookStatus?: () => void;
  onSessionTitleGenerationAgentChange: (agent: SessionTitleGenerationAgent) => void;
  onUninstallAgentHooks?: (agentIds?: readonly string[]) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  vscode?: WebviewApi;
}) {
  const agents = useSidebarStore((state) => state.hud.agents);
  const agentApprovalsControlId = useId();
  const agentHooksAvailableForUninstall = hasRemovableAgentHooks(agentHookStatus);
  const [editorState, setEditorState] = useState<SettingsAgentEditorState>();
  const agentRosterSectionRef = useRef<HTMLDivElement>(null);
  const lastTargetedAgentsSectionRef = useRef<SettingsAgentsSection | undefined>(undefined);
  useEffect(() => {
    if (!isActive) {
      lastTargetedAgentsSectionRef.current = undefined;
      return;
    }
    if (!initialAgentsSection || lastTargetedAgentsSectionRef.current === initialAgentsSection) {
      return;
    }
    lastTargetedAgentsSectionRef.current = initialAgentsSection;
    const animationFrame = requestAnimationFrame(() => {
      agentRosterSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [initialAgentsSection, isActive]);
  const [draftAgentIds, setDraftAgentIds] = useState<string[]>();
  /*
   * CDXC:AgentHooks 2026-08-28:
   * Row expansion is view state only: every row starts collapsed each time
   * Settings opens so the Agents tab reads as one compact roster.
   */
  const [expandedAgentIds, setExpandedAgentIds] = useState<readonly string[]>([]);

  const toggleExpandedAgent = (agentId: string) => {
    setExpandedAgentIds((previous) =>
      previous.includes(agentId) ? previous.filter((id) => id !== agentId) : [...previous, agentId]
    );
  };

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
         * CDXC:AgentProviders 2026-06-19-08:58:
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
   * CDXC:AgentProviders 2026-08-27:
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
        {!editorState && shouldShowSettingsSection(search.sections.accounts) && <AccountsSettingsSection active={isActive} />}
        {!editorState && shouldShowSettingsSection(search.sections.config) ? (
          <SettingsSection title='Config'>
            {/*
             * CDXC:Settings 2026-06-12-04:40:
             * Default prompt, title generation, custom title command, and global agent approvals are configuration controls, not agent management rows. Group them under the same labeled SettingsSection chrome as the Agents roster so the Agents tab scans as two consistent areas: config and the agent roster.
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
             * CDXC:SessionTitles 2026-06-04-08:24:
             * First-prompt session-title generation needs its own agent selector instead of reusing Default Prompt Agent, because title generation is a gxserver-owned background job while prompt-launch defaults affect Git helpers, project-board prompts, and worktree starts.
             *
             * CDXC:SessionTitles 2026-06-04-22:44:
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
                  <FieldLabel className='text-sm' htmlFor={agentApprovalsControlId}>
                    Agent approvals
                  </FieldLabel>
                  <FieldDescription className='text-xs text-muted-foreground'>
                    Choose whether supported agents ask before editing files or running commands. Per-agent settings can
                    override this default.
                  </FieldDescription>
                </FieldContent>
                <DisabledSettingControlTooltip
                  disabled={!vscode}
                  reason='This change needs the Ghostex app connection.'
                >
                  <AgentApprovalPolicyControl
                    disabled={!vscode}
                    enabled={agentAcceptAllEnabled}
                    id={agentApprovalsControlId}
                    onChange={onAgentAcceptAllEnabledChange}
                    size='sm'
                  />
                </DisabledSettingControlTooltip>
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
            sectionRef={agentRosterSectionRef}
            title={editorState ? 'Agent' : 'Agents'}
          >
            {editorState ? (
              <AgentSettingsEditor
                draft={editorState.draft}
                onCancel={() => setEditorState(undefined)}
                onSave={saveAgent}
              />
            ) : (
              <div className='flex flex-col gap-3'>
                {/*
                 * CDXC:AgentHooks 2026-08-28:
                 * Hook setup lives inside the one Agents roster instead of a
                 * second card that repeats every agent. The toolbar keeps the
                 * whole-set controls quiet (ghost buttons plus the readiness
                 * chip and an info tooltip for the long explanation) because
                 * per-agent install is the primary action: every row without a
                 * hook shows its own install button while collapsed, and each
                 * row expands to the full hook detail for that agent.
                 */}
                <div className='flex flex-wrap items-center gap-x-3 gap-y-2 rounded-none border border-border/70 bg-muted/10 px-3 py-2'>
                  <div className='flex min-w-0 flex-1 items-center gap-2'>
                    <AppTooltip
                      content={
                        <>
                          Install hooks so Ghostex can capture each agent&apos;s native session id and resume the exact
                          conversation after sleep, reload, or app restart. Hooks write only session metadata into
                          Ghostex&apos;s session-state files. The existing title-based restore path remains available
                          when a hook has not captured an id yet.
                        </>
                      }
                      contentClassName='max-w-[22rem]'
                    >
                      <span
                        aria-label='About session resume hooks'
                        className='inline-flex shrink-0 items-center text-muted-foreground'
                        role='img'
                      >
                        <IconInfoCircle aria-hidden='true' className='size-4' />
                      </span>
                    </AppTooltip>
                    <span className='min-w-0 truncate text-xs text-muted-foreground'>
                      Session resume hooks let Ghostex capture each agent&apos;s native session id and resume the exact
                      conversation.
                    </span>
                  </div>
                  <span className='shrink-0 rounded-none border border-border/70 bg-muted/40 px-2 py-1 text-[11px] font-medium text-muted-foreground'>
                    {hookStatusSummary}
                  </span>
                  <div className='flex shrink-0 flex-wrap items-center gap-1.5'>
                    <SettingButton
                      disabled={!onInstallAgentHooks || agentHookStatusLoading}
                      disabledReason={
                        agentHookStatusLoading
                          ? 'Hook status is being checked.'
                          : 'Hook installation isn’t available here.'
                      }
                      onClick={() => onInstallAgentHooks?.()}
                      size='sm'
                      type='button'
                      variant='ghost'
                    >
                      <IconDownload aria-hidden='true' data-icon='inline-start' />
                      {updateRequiredHookCount > 0 ? 'Update All' : 'Install All'}
                    </SettingButton>
                    {/*
                     * CDXC:AgentHooks 2026-08-19-11:20:
                     * Hook removal lives beside the install control it undoes: one Uninstall All for the whole set, plus a per-agent removal in each expanded row. Both stay disabled while status is loading or when no Ghostex-owned hook is present, so users cannot fire a no-op removal.
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
                      size='sm'
                      type='button'
                      variant='ghost'
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
                      size='sm'
                      type='button'
                      variant='ghost'
                    >
                      <IconRefresh aria-hidden='true' data-icon='inline-start' />
                      Refresh
                    </SettingButton>
                  </div>
                </div>
                {agentHookStatus?.errorMessage ? (
                  <div className='rounded-none border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive'>
                    {agentHookStatus.errorMessage}
                  </div>
                ) : null}
                {orderedAgents.length > 0 ? (
                  <DragDropProvider onDragEnd={handleDragEnd}>
                    <div className='flex flex-col gap-2'>
                      {orderedAgents.map((agent, index) => {
                        /*
                         * CDXC:AgentHooks 2026-08-28:
                         * Hooks are per CLI, not per launcher row, so a roster
                         * agent resolves to its default agent through the same
                         * icon mapping session creation uses. Custom launchers
                         * without a default agent have no hook to manage.
                         */
                        const hookAgentId = getDefaultSidebarAgentByIcon(agent.icon)?.agentId;
                        return (
                          <SettingsAgentRow
                            acceptAllMode={agent.acceptAllMode ?? 'inherit'}
                            agent={agent}
                            hookStatus={hookAgentId ? hookStatusByAgentId.get(hookAgentId) : undefined}
                            index={index}
                            isExpanded={expandedAgentIds.includes(agent.agentId)}
                            isHookStatusLoading={agentHookStatusLoading}
                            isHookStatusPending={agentHookStatusLoading && !agentHookStatus}
                            key={agent.agentId}
                            onAcceptAllModeChange={
                              vscode
                                ? (acceptAllMode) =>
                                    saveAgent({
                                      acceptAllMode,
                                      agentId: agent.agentId,
                                      command: agent.command ?? '',
                                      icon: agent.icon,
                                      name: agent.name,
                                    })
                                : undefined
                            }
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
                            onInstallHook={
                              hookAgentId && onInstallAgentHooks ? () => onInstallAgentHooks([hookAgentId]) : undefined
                            }
                            onPreferredInterfaceOverrideChange={(next) =>
                              setPreferredAgentInterfaceOverride(agent.agentId, next)
                            }
                            onToggleExpanded={() => toggleExpandedAgent(agent.agentId)}
                            onUninstallHook={
                              hookAgentId && onUninstallAgentHooks
                                ? () => onUninstallAgentHooks([hookAgentId])
                                : undefined
                            }
                            preferredAgentInterface={preferredAgentInterface}
                            preferredInterfaceOverride={preferredAgentInterfaceOverrides[agent.agentId]}
                            supportsHooks={Boolean(hookAgentId)}
                          />
                        );
                      })}
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
                {agentHookStatus ? (
                  <FieldDescription className='truncate text-[11px] text-muted-foreground'>
                    Hook state: {agentHookStatus.hookStateDirectory}
                  </FieldDescription>
                ) : null}
              </div>
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

/*
 * CDXC:AgentProviders 2026-08-27:
 * Only chat-capable agents get this control. A terminal-only agent has no
 * second view to choose, so a disabled select there would be noise; its row
 * simply ends at the hook status.
 */
export function AgentPreferredInterfaceOverrideSelect({
  agentName,
  className,
  id,
  onChange,
  preferredAgentInterface,
  value,
}: {
  agentName: string;
  className?: string;
  id?: string;
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
      <SelectTrigger
        aria-label={`Default view for ${agentName}`}
        className={cn('h-8 w-full px-3 text-[13px]', className)}
        id={id}
      >
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

/*
 * CDXC:AgentHooks 2026-08-28:
 * One roster row owns everything about an agent: reorder, identity, its session
 * resume hook, and the agent actions. The collapsed row stays compact and keeps
 * the single-click install affordance for any agent whose hook is missing; the
 * expanded panel carries the hook path, the per-agent selects, and the
 * install/uninstall and edit/remove actions.
 */
export function SettingsAgentRow({
  acceptAllMode,
  agent,
  hookStatus,
  index,
  isExpanded,
  isHookStatusLoading,
  isHookStatusPending,
  onAcceptAllModeChange,
  onDelete,
  onEdit,
  onInstallHook,
  onPreferredInterfaceOverrideChange,
  onToggleExpanded,
  onUninstallHook,
  preferredAgentInterface,
  preferredInterfaceOverride,
  supportsHooks,
}: {
  acceptAllMode: AgentAcceptAllMode;
  agent: SidebarAgentButton;
  hookStatus?: SidebarAgentHookStatusItem;
  index: number;
  isExpanded: boolean;
  isHookStatusLoading: boolean;
  isHookStatusPending: boolean;
  onAcceptAllModeChange?: (acceptAllMode: AgentAcceptAllMode) => void;
  onDelete: () => void;
  onEdit: () => void;
  onInstallHook?: () => void;
  onPreferredInterfaceOverrideChange: (preferredInterface: PreferredAgentInterface | undefined) => void;
  onToggleExpanded: () => void;
  onUninstallHook?: () => void;
  preferredAgentInterface: PreferredAgentInterface;
  preferredInterfaceOverride?: PreferredAgentInterface;
  supportsHooks: boolean;
}) {
  const acceptAllModeId = useId();
  const panelId = useId();
  const preferredInterfaceId = useId();
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

  const acceptAllSupported = supportsAgentAcceptAll(agent.agentId, agent.icon);
  const supportsChatView = agentSupportsChatView(agent);
  const hookInstalled = hookStatus?.status === 'installed';
  const hookRemovable = hasRemovableAgentHookStatus(hookStatus);
  const hookInstallLabel = hookInstalled
    ? 'Reinstall'
    : hookStatus?.status === 'updateRequired'
      ? 'Update hook'
      : 'Install hook';
  const showInlineInstall =
    supportsHooks && !hookInstalled && hookStatus?.status !== 'notRequired' && !isHookStatusPending;
  const hookInstallDisabled = !onInstallHook || isHookStatusLoading;
  const hookInstallDisabledReason = isHookStatusLoading
    ? 'Hook status is being checked.'
    : 'Hook installation isn’t available here.';

  return (
    <div
      className='rounded-none border border-border bg-muted/20'
      data-dragging={String(Boolean(isDragging))}
      ref={setRowRef}
    >
      <div className='settings-management-row flex items-center gap-2 p-2'>
        <Button aria-label={`Reorder ${agent.name}`} ref={handleRef} size='icon-sm' type='button' variant='ghost'>
          <IconGripVertical aria-hidden='true' />
        </Button>
        <Button
          aria-controls={panelId}
          aria-expanded={isExpanded}
          className='settings-management-edit-button h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-2 text-left'
          onClick={onToggleExpanded}
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
            {/*
             * CDXC:AgentProviders 2026-08-27:
             * The chat-bubble badge sits with the agent name, not with the hook
             * status pill: it describes the agent, not its hook state, and the
             * two must not read as one combined status. Terminal-only agents get
             * no badge at all rather than a negative one.
             */}
            <span className='flex min-w-0 items-center gap-1.5'>
              <span className='truncate text-sm font-medium text-foreground'>{agent.name}</span>
              <AgentChatViewSupportBadge agent={agent} />
            </span>
            <span className='block truncate text-xs text-muted-foreground'>
              {agent.command?.trim() || 'Not configured'}
            </span>
          </span>
        </Button>
        {supportsHooks ? (
          <span
            className={cn(
              'flex shrink-0 items-center gap-1.5 rounded-none px-2 py-1 text-xs font-medium',
              getAgentHookStatusClassName(hookStatus, isHookStatusPending)
            )}
          >
            <AgentHookStatusIcon isLoading={isHookStatusPending} status={hookStatus} />
            {getAgentHookStatusText(hookStatus, isHookStatusPending)}
          </span>
        ) : null}
        {showInlineInstall ? (
          <SettingButton
            className='shrink-0'
            disabled={hookInstallDisabled}
            disabledReason={hookInstallDisabledReason}
            onClick={onInstallHook}
            size='sm'
            type='button'
            variant='outline'
          >
            <IconDownload aria-hidden='true' data-icon='inline-start' />
            {hookInstallLabel}
          </SettingButton>
        ) : null}
        <Button
          aria-controls={panelId}
          aria-expanded={isExpanded}
          aria-label={isExpanded ? `Collapse ${agent.name} details` : `Expand ${agent.name} details`}
          onClick={onToggleExpanded}
          size='icon-sm'
          type='button'
          variant='ghost'
        >
          <IconChevronDown
            aria-hidden='true'
            className={cn('transition-transform duration-150', isExpanded && 'rotate-180')}
          />
        </Button>
      </div>
      {isExpanded ? (
        <div className='flex flex-col gap-3 border-t border-border/70 px-3 py-3' id={panelId}>
          {supportsHooks ? (
            <>
              <span className='text-[11px] font-medium uppercase tracking-wide text-muted-foreground'>
                Session resume hook
              </span>
              <div className='flex items-center gap-2 border-b border-border/60 pb-3 text-xs text-muted-foreground'>
                <AgentHookStatusIcon isLoading={isHookStatusPending} status={hookStatus} />
                <span className='min-w-0 truncate'>{hookStatus?.detail ?? 'Waiting for hook check'}</span>
              </div>
            </>
          ) : null}
          <div className='grid gap-3 sm:grid-cols-2'>
            <Field className='gap-1.5'>
              <FieldContent>
                <FieldLabel className='text-xs text-muted-foreground' htmlFor={acceptAllModeId}>
                  Permission mode
                </FieldLabel>
              </FieldContent>
              <SettingsSelect
                disabled={!acceptAllSupported || !onAcceptAllModeChange}
                disabledReason={
                  acceptAllSupported
                    ? 'This change needs the Ghostex app connection.'
                    : 'This agent doesn’t support approval policy changes.'
                }
                disabledTooltipClassName='w-full'
                items={AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS}
                onValueChange={(value) => onAcceptAllModeChange?.(value as AgentAcceptAllMode)}
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
            {supportsChatView ? (
              <Field className='gap-1.5'>
                <FieldContent>
                  <FieldLabel className='text-xs text-muted-foreground' htmlFor={preferredInterfaceId}>
                    Default interface
                  </FieldLabel>
                </FieldContent>
                <AgentPreferredInterfaceOverrideSelect
                  agentName={agent.name}
                  id={preferredInterfaceId}
                  onChange={onPreferredInterfaceOverrideChange}
                  preferredAgentInterface={preferredAgentInterface}
                  value={preferredInterfaceOverride}
                />
              </Field>
            ) : null}
          </div>
          {supportsHooks ? (
            <div className='flex flex-wrap items-center justify-end gap-2'>
              <SettingButton
                disabled={hookInstallDisabled}
                disabledReason={hookInstallDisabledReason}
                onClick={onInstallHook}
                size='sm'
                type='button'
                variant={hookInstalled ? 'outline' : 'default'}
              >
                {hookInstalled ? (
                  <IconRefresh aria-hidden='true' data-icon='inline-start' />
                ) : (
                  <IconDownload aria-hidden='true' data-icon='inline-start' />
                )}
                {hookInstallLabel}
              </SettingButton>
              {hookRemovable ? (
                <SettingButton
                  aria-label={`Uninstall ${agent.name} hook`}
                  disabled={isHookStatusLoading || !onUninstallHook}
                  disabledReason={
                    isHookStatusLoading ? 'Hook status is being checked.' : 'Hook removal isn’t available here.'
                  }
                  onClick={onUninstallHook}
                  size='sm'
                  type='button'
                  variant='destructive'
                >
                  <IconTrash aria-hidden='true' data-icon='inline-start' />
                  Uninstall hook
                </SettingButton>
              ) : null}
            </div>
          ) : null}
          <div className='flex flex-wrap items-center gap-2 border-t border-border/60 pt-3'>
            <span className='text-[11px] font-medium uppercase tracking-wide text-muted-foreground'>Agent</span>
            <span className='flex-1' />
            <Button aria-label={`Edit ${agent.name}`} onClick={onEdit} size='sm' type='button' variant='outline'>
              <IconPencil aria-hidden='true' data-icon='inline-start' />
              Edit agent
            </Button>
            <Button
              aria-label={`Delete ${agent.name}`}
              onClick={onDelete}
              size='sm'
              type='button'
              variant='destructive'
            >
              <IconTrash aria-hidden='true' data-icon='inline-start' />
              Remove agent
            </Button>
          </div>
        </div>
      ) : null}
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
  const selectedDefaultAgent = getDefaultSidebarAgentByIcon(icon === 'custom' ? undefined : icon);

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
            <SelectValue>
              <AgentTypeSelectOption icon={icon} name={selectedDefaultAgent?.name ?? 'Custom'} />
            </SelectValue>
          </SelectTrigger>
          <SettingsSelectContent>
            <SelectGroup>
              <SelectItem value='custom'>
                <AgentTypeSelectOption icon='custom' name='Custom' />
              </SelectItem>
              {DEFAULT_SIDEBAR_AGENTS.map((agent) => (
                <SelectItem key={agent.agentId} value={agent.icon}>
                  <AgentTypeSelectOption icon={agent.icon} name={agent.name} />
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
            Agent approvals
          </FieldLabel>
          <FieldDescription className='text-xs text-muted-foreground'>
            {acceptAllSupported
              ? "Use app default follows the global Agents setting. Skip permissions applies this agent's permission-bypass mode at launch without changing the stored command."
              : 'This agent does not expose a supported approval policy in Ghostex.'}
          </FieldDescription>
        </FieldContent>
        <SettingsSelect
          disabled={!acceptAllSupported}
          disabledReason='This agent doesn’t support approval policy changes.'
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
