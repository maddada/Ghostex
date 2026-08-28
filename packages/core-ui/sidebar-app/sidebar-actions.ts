import type { Dispatch, SetStateAction } from 'react';
import type { SidebarActiveSessionsSortMode } from '../../shared/session-grid-contract';
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
  enabledVisibleSidebarSessionTagSet: ReadonlySet<SidebarSessionTagFilter>;
  setIsPreviousSessionsOpen: Dispatch<SetStateAction<boolean>>;
  setIsSessionSearchOpen: Dispatch<SetStateAction<boolean>>;
  setIsSessionSearchSelectionVisible: Dispatch<SetStateAction<boolean>>;
  setPrimaryAgentLauncherId: Dispatch<SetStateAction<string | undefined>>;
  setSelectedSessionTagFilters: Dispatch<SetStateAction<SidebarSessionTagFilter[]>>;
  setSessionSearchQuery: Dispatch<SetStateAction<string>>;
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
  enabledVisibleSidebarSessionTagSet,
  setIsPreviousSessionsOpen,
  setIsSessionSearchOpen,
  setIsSessionSearchSelectionVisible,
  setPrimaryAgentLauncherId,
  setSelectedSessionTagFilters,
  setSessionSearchQuery,
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
    setIsSessionSearchSelectionVisible(false);
    setIsSessionSearchOpen(false);
    setSessionSearchQuery('');
    openQuickAccess('recentSessions');
  };

  const searchPreviousSessionsByPrompt = () => {
    dismissAppModalForSidebarNavigation('SettingsDismissal:previousSessionsPromptSearch');
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
    searchPreviousSessionsByPrompt,
    setActiveSessionsSortMode,
    toggleActiveSessionsSortMode,
    toggleSessionTagFilter,
    toggleSidebarCollapsed,
  };
}
