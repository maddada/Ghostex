import type { Dispatch, SetStateAction } from 'react';
import type { SidebarActiveSessionsSortMode } from '../../shared/session-grid-contract';
import type {
  ghostexSettings,
  SidebarNewSessionEnvMode,
  SidebarProjectGroupingMode,
  SidebarV2Layout,
  SidebarVersion,
} from '../../shared/ghostex-settings';
import type { SidebarAgentButton } from '../../shared/sidebar-agents';
import { openAppModal, openQuickAccess } from '../app-modal-host-bridge';
import { writePrimaryAgentLauncherId } from '../primary-agent-launcher';
import type { SidebarSessionTagFilter } from '../session-tag-ui';
import type { WebviewApi } from '../webview-api';
import type { SessionIdsByGroup } from './types';

export type SidebarActionsOptions = {
  activeSessionsSortMode: SidebarActiveSessionsSortMode;
  dismissAppModalForSidebarNavigation: (area: string) => void;
  displayedReferenceChatGroupIds: readonly string[];
  effectiveSessionIdsByGroup: SessionIdsByGroup;
  effectiveSettings: ghostexSettings;
  enabledVisibleSidebarSessionTagSet: ReadonlySet<SidebarSessionTagFilter>;
  revision: number;
  setIsPreviousSessionsOpen: Dispatch<SetStateAction<boolean>>;
  setIsScratchPadOpen: Dispatch<SetStateAction<boolean>>;
  setIsSessionSearchOpen: Dispatch<SetStateAction<boolean>>;
  setIsSessionSearchSelectionVisible: Dispatch<SetStateAction<boolean>>;
  setPrimaryAgentLauncherId: Dispatch<SetStateAction<string | undefined>>;
  setSelectedSessionTagFilters: Dispatch<SetStateAction<SidebarSessionTagFilter[]>>;
  setSessionSearchQuery: Dispatch<SetStateAction<string>>;
  sidebarV2Layout: SidebarV2Layout;
  sidebarVersion: SidebarVersion;
  vscode: WebviewApi;
  workspaceGroupIds: readonly string[];
};

/*
 * CDXC:SidebarHookDecomposition 2026-08-22:
 * The sidebar's command surface: sort/version/layout preferences, tag filter
 * toggles, the native chrome requests, agent launches, and the remaining
 * top-chrome entry points. All of them are plain closures over render values,
 * so this hook holds no hook calls of its own.
 */
export function useSidebarActions({
  activeSessionsSortMode,
  dismissAppModalForSidebarNavigation,
  displayedReferenceChatGroupIds,
  effectiveSessionIdsByGroup,
  effectiveSettings,
  enabledVisibleSidebarSessionTagSet,
  revision,
  setIsPreviousSessionsOpen,
  setIsScratchPadOpen,
  setIsSessionSearchOpen,
  setIsSessionSearchSelectionVisible,
  setPrimaryAgentLauncherId,
  setSelectedSessionTagFilters,
  setSessionSearchQuery,
  sidebarV2Layout,
  sidebarVersion,
  vscode,
  workspaceGroupIds,
}: SidebarActionsOptions) {
  const setActiveSessionsSortMode = (sortMode: SidebarActiveSessionsSortMode) => {
    vscode.postMessage({
      manualSessionIdsByGroup:
        sortMode === 'manual' && activeSessionsSortMode !== 'manual'
          ? Object.fromEntries(
              workspaceGroupIds.map((groupId) => [groupId, [...(effectiveSessionIdsByGroup[groupId] ?? [])]])
            )
          : undefined,
      sortMode,
      type: 'setActiveSessionsSortMode',
    });
  };

  /*
   * CDXC:SidebarV2 2026-07-29:
   * Sidebar version and its Group by Project sub-mode are persisted settings,
   * not sidebar-local view state. Write them through the same settings patch
   * channel the Settings modal uses so gpui persists them and hydrates every
   * surface back, instead of the sort-mode channel that gpui does not handle.
   */
  const updateSidebarVersionSettings = (patch: {
    sidebarV2Layout?: SidebarV2Layout;
    sidebarVersion?: SidebarVersion;
  }) => {
    vscode.postMessage({
      baseRevision: revision,
      patch,
      source: 'sidebar:sidebarVersion',
      type: 'updateSettingsPatch',
    });
  };

  const setSidebarVersion = (nextSidebarVersion: SidebarVersion) => {
    if (nextSidebarVersion === sidebarVersion) {
      return;
    }
    updateSidebarVersionSettings({ sidebarVersion: nextSidebarVersion });
  };

  const setSidebarV2Layout = (nextLayout: SidebarV2Layout) => {
    if (nextLayout === sidebarV2Layout) {
      return;
    }
    updateSidebarVersionSettings({ sidebarV2Layout: nextLayout });
  };

  const toggleActiveSessionsSortMode = () => {
    setActiveSessionsSortMode(activeSessionsSortMode === 'manual' ? 'lastActivity' : 'manual');
  };

  const toggleSessionTagFilter = (sessionTag: SidebarSessionTagFilter) => {
    if (!enabledVisibleSidebarSessionTagSet.has(sessionTag)) {
      return;
    }
    setSelectedSessionTagFilters((current) =>
      current.includes(sessionTag) ? current.filter((tag) => tag !== sessionTag) : [...current, sessionTag]
    );
  };

  const moveSidebar = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:moveSidebar');
    vscode.postMessage({ type: 'moveSidebarToOtherSide' });
  };

  const toggleSidebarCollapsed = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:toggleSidebar');
    /**
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * Sidebar collapse is native chrome state. React requests the toggle, while
     * AppKit owns hiding the sidebar WebView, divider, and workspace border.
     */
    vscode.postMessage({ type: 'toggleSidebarCollapsed' });
  };

  /*
   * CDXC:AddProject 2026-07-30:
   * Add Project opens the shared add-project dialog in the app-modal host for
   * every entry point. The local header sends no machine (the dialog resolves
   * the machine list itself and skips its machine step when there is only one),
   * while a remote machine header preselects its own machine so the flow can
   * never silently browse this computer's filesystem instead of that machine's.
   */
  const openAddProjectModal = (machineId?: string) => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:addProject');
    openAppModal({ ...(machineId ? { machineId } : {}), modal: 'addProject', type: 'open' });
  };

  const createReferenceAgentChat = (agent: SidebarAgentButton) => {
    const quickGroupId = displayedReferenceChatGroupIds[0];
    if (!quickGroupId) {
      return;
    }

    dismissAppModalForSidebarNavigation('SettingsDismissal:createQuickAgent');
    /**
     * CDXC:QuickAgents 2026-06-08-18:25:
     * The Quick section header should expose the same selected-agent split picker as project headers. Launch through runSidebarAgent with the synthetic Quick group id so native creates a new projectless agent chat instead of targeting the active code project.
     */
    setPrimaryAgentLauncherId(agent.agentId);
    writePrimaryAgentLauncherId(agent.agentId);
    vscode.postMessage({
      agentId: agent.agentId,
      groupId: quickGroupId,
      type: 'runSidebarAgent',
    });
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * Sidebar V2's "+" launches through THIS function so the instant path stays
   * byte-identical to the classic sidebar's: a project click posts the same
   * `runSidebarAgent` the project header posts.
   *
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * V2 no longer reaches the `!groupId` branch from any ordinary create path.
   * Its header "+" and its agent picker both resolve a REAL project first (V2's
   * own `headerCreateGroupId`: scoped project, then active project, then the
   * first project), so a session can no longer silently land in Quick just
   * because the click came from the header rather than from a project row.
   *
   * The branch stays because `groupId` is genuinely optional in one case: a
   * workspace with ZERO project groups, where V2's resolution has nothing to
   * return. Quick is then the only place a session can go, so falling through to
   * the Quick launcher is the correct answer rather than a downgrade. Quick
   * creation ON PURPOSE happens through the chevron's explicitly-labelled
   * "Quick Terminal" / "Quick Browser Tab" items, which never come here.
   */
  const runSidebarV2Agent = (agent: SidebarAgentButton, groupId?: string) => {
    if (!groupId) {
      createReferenceAgentChat(agent);
      return;
    }
    dismissAppModalForSidebarNavigation('SettingsDismissal:createSidebarV2Agent');
    setPrimaryAgentLauncherId(agent.agentId);
    writePrimaryAgentLauncherId(agent.agentId);
    vscode.postMessage({
      agentId: agent.agentId,
      groupId,
      type: 'runSidebarAgent',
    });
  };

  /*
   * CDXC:SidebarV2Worktree 2026-07-29:
   * The "default new sessions to worktree" preference is GLOBAL and rides the
   * same settings patch channel as the sidebar version switch, so gpui persists
   * it and every surface hydrates it back.
   */
  const setNewSessionsDefaultEnvMode = (mode: SidebarNewSessionEnvMode) => {
    if (mode === effectiveSettings.newSessionsDefaultEnvMode) {
      return;
    }
    vscode.postMessage({
      baseRevision: revision,
      patch: { newSessionsDefaultEnvMode: mode },
      source: 'sidebar:newSessionsDefaultEnvMode',
      type: 'updateSettingsPatch',
    });
  };

  /*
   * CDXC:SidebarV2LogicalProjects 2026-07-29:
   * Cross-machine grouping overrides ride the SAME settings patch channel as
   * the sidebar version switch, under their own source so a grouping change can
   * never be mistaken for a remote-machine-capable save. V2 hands over the
   * whole record, so the patch is a straight replacement rather than a merge
   * the settings pipeline would have to interpret.
   */
  const setSidebarProjectGroupingOverrides = (overrides: Readonly<Record<string, SidebarProjectGroupingMode>>) => {
    vscode.postMessage({
      baseRevision: revision,
      patch: { sidebarProjectGroupingOverrides: overrides },
      source: 'sidebar:projectGrouping',
      type: 'updateSettingsPatch',
    });
  };

  const openConfigureAgentsModal = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:configureAgents');
    openAppModal({ modal: 'configureAgents', type: 'open' });
  };

  const openReferenceAutomations = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:automations');
    vscode.postMessage({ type: 'openAutomationsPage' });
  };

  const openReferenceMobile = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:mobile');
    vscode.postMessage({ type: 'openMobileBrowserChat' });
  };

  const openReferenceAgentsHub = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:agentsHub');
    openAppModal({ modal: 'agentsHub', type: 'open' });
  };

  const openPreviousSessions = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:previousSessions');
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
    openQuickAccess('recentSessions');
  };

  const searchPreviousSessionsByPrompt = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:previousSessionsPromptSearch');
    setIsScratchPadOpen(false);
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
    vscode.postMessage({ type: 'searchPreviousSessionsByText' });
  };

  return {
    createReferenceAgentChat,
    moveSidebar,
    openAddProjectModal,
    openConfigureAgentsModal,
    openPreviousSessions,
    openReferenceAgentsHub,
    openReferenceAutomations,
    openReferenceMobile,
    runSidebarV2Agent,
    searchPreviousSessionsByPrompt,
    setActiveSessionsSortMode,
    setNewSessionsDefaultEnvMode,
    setSidebarProjectGroupingOverrides,
    setSidebarV2Layout,
    setSidebarVersion,
    toggleActiveSessionsSortMode,
    toggleSessionTagFilter,
    toggleSidebarCollapsed,
  };
}
