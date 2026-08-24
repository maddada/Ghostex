/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GPUI_SIDEBAR_COMMAND_SELECTOR_MESSAGE_KEYS } from './constants';
import type { GpuiSidebarRuntime } from './core';
import { normalizeGpuiReplacementProjectFolderPick, normalizeGpuiWorkspaceFolderPick } from './helpers/folder-picker';
import { isGpuiPresentationQuickDomainProject } from './helpers/presentation-projection';
import { normalizeNonEmptyString } from './helpers/records';
import {
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
} from './helpers/remote-presentation';
import { gpuiProjectNameFromPath, normalizeGpuiProjectPath } from './helpers/worktrees';
import type { GpuiSidebarRuntimeSnapshotKind } from './types-and-protocol';
import { openAppModal, postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import { parseGxserverPresentationProjectGroupId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverProjectDomainState,
  GxserverRecentProjectDomainState,
  GxserverSidebarHudSettingsMutationParams,
  GxserverSidebarHudSettingsMutationResult,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { createSidebarAgentButtons } from '@/packages/shared/sidebar-agents';
import type { SidebarCommandButton, SidebarCommandScope } from '@/packages/shared/sidebar-commands';
import {
  createSidebarCommandButtons,
  isSidebarCommandConfigured,
  isSidebarCommandRunMode,
  normalizeSidebarCommandLinks,
} from '@/packages/shared/sidebar-commands';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeProjectAndCommandMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeProjectAndCommandMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeProjectAndCommandMethods {
  updateProjectBeadsDisplayKey(projectId: string, displayKey: string): Promise<void>;
  updateProjectBeadsDirectory(projectId: string, directory: string): Promise<void>;
  updateProjectDocsDirectory(projectId: string, directory: string): Promise<void>;
  registerDomainProjectPath(project: GxserverProjectDomainState): Promise<GxserverProjectDomainState>;
  registerProjectPath(input: { name: string; path: string }): Promise<GxserverProjectDomainState>;
  saveSidebarAgent(message: Extract<SidebarToExtensionMessage, { type: 'saveSidebarAgent' }>): Promise<void>;
  deleteSidebarAgent(agentId: string): Promise<void>;
  syncSidebarAgentOrder(requestId: string, agentIds: readonly string[]): Promise<void>;
  saveSidebarCommand(message: Extract<SidebarToExtensionMessage, { type: 'saveSidebarCommand' }>): Promise<void>;
  deleteSidebarCommand(commandId: string): Promise<void>;
  syncSidebarCommandOrder(requestId: string, commandIds: readonly string[]): Promise<void>;
  saveGlobalSidebarCommand(
    message: Extract<SidebarToExtensionMessage, { type: 'saveGlobalSidebarCommand' }>
  ): Promise<void>;
  deleteGlobalSidebarCommand(commandId: string): Promise<void>;
  syncGlobalSidebarCommandOrder(requestId: string, commandIds: readonly string[]): Promise<void>;
  pickWorkspaceFolder(originalMessage: SidebarToExtensionMessage): void;
  handleGpuiWorkspaceFolderPicked(payload: unknown): Promise<void>;
  ensureLocalProjectPathAvailable(projectId: string): boolean;
  presentMissingProjectFolder(projectId: string): boolean;
  relocateProjectFolder(projectId: string, path: string): Promise<void>;
  removeProject(projectId: string): Promise<void>;
  restoreRecentProject(projectId: string): Promise<void>;
  removeRecentProject(projectId: string): Promise<void>;
  closeProjectForGroup(groupId: string): Promise<void>;
  removeProjectForGroup(groupId: string): Promise<void>;
  resolveProjectIdForGroup(groupId: string): string | undefined;
  activeDomainProject(): GxserverProjectDomainState | undefined;
  domainProjectById(projectId: string): GxserverProjectDomainState | undefined;
  resolveDomainProjectScope(scope: {
    projectId?: string;
    projectPath?: string;
  }): GxserverProjectDomainState | undefined;
  resolveSidebarAgent(agentId: string): SidebarAgentButton | undefined;
  resolveSidebarCommand(commandId: string, scope?: SidebarCommandScope): SidebarCommandButton | undefined;
  resolveSidebarCommandForProject(commandId: string, projectId: string | undefined): SidebarCommandButton | undefined;
  createSidebarCommandSelectionMessage(
    commandId: string,
    originalMessage: SidebarToExtensionMessage
  ): Extract<SidebarToExtensionMessage, { type: 'runSidebarCommand' }> | undefined;
  runSidebarCommand(commandId: string, originalMessage: SidebarToExtensionMessage, scope?: SidebarCommandScope): void;
  endSidebarCommandRun(commandId: string, originalMessage: SidebarToExtensionMessage): void;
  mutateSidebarHudSettings(
    params: GxserverSidebarHudSettingsMutationParams
  ): Promise<GxserverSidebarHudSettingsMutationResult | undefined>;
  updateProjectDomainState(
    projectId: string,
    params: Record<string, unknown>
  ): Promise<GxserverProjectDomainState | undefined>;
  upsertDomainProject(nextProject: GxserverProjectDomainState): void;
  refreshDomainPresentationFromClient(kind: GpuiSidebarRuntimeSnapshotKind): Promise<void>;
  refreshDomainPresentationSnapshotFromClient(kind: GpuiSidebarRuntimeSnapshotKind): Promise<void>;
}

export const gpuiSidebarRuntimeProjectAndCommandMethods = {
  async updateProjectBeadsDisplayKey(this: GpuiSidebarRuntime, projectId: string, displayKey: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedDisplayKey = displayKey
      .trim()
      .toUpperCase()
      .replace(/[^A-Z0-9]/gu, '')
      .slice(0, 3);
    await this.updateProjectDomainState(project.projectId, {
      gitConfig: {
        ...project.gitConfig,
        beadsDisplayKey: normalizedDisplayKey || null,
      },
      projectBoardConfig: {
        ...project.projectBoardConfig,
        beadsDisplayKey: normalizedDisplayKey || null,
      },
    });
  },

  async updateProjectBeadsDirectory(this: GpuiSidebarRuntime, projectId: string, directory: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedDirectory = directory.trim();
    await this.updateProjectDomainState(project.projectId, {
      projectBoardConfig: {
        ...project.projectBoardConfig,
        beadsDirectory: normalizedDirectory || null,
      },
    });
  },

  /*
  CDXC:DocsRootDirectory 2026-08-09:
  The Docs root override rides in the same per-project config object the Beads
  directory already uses, so Settings -> Projects keeps one storage seam and
  needs no new domain field, column, or migration. A blank value clears the
  override so the project falls back to the Global Default, then the repo root.
  */
  async updateProjectDocsDirectory(this: GpuiSidebarRuntime, projectId: string, directory: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedDirectory = directory.trim();
    await this.updateProjectDomainState(project.projectId, {
      projectBoardConfig: {
        ...project.projectBoardConfig,
        docsDirectory: normalizedDirectory || null,
      },
    });
  },

  async registerDomainProjectPath(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState
  ): Promise<GxserverProjectDomainState> {
    const path = normalizeGpuiProjectPath(project.path);
    if (!path) {
      throw new Error('Project has no registered path.');
    }
    return this.registerProjectPath({
      name: project.name || gpuiProjectNameFromPath(path),
      path,
    });
  },

  async registerProjectPath(
    this: GpuiSidebarRuntime,
    input: {
      name: string;
      path: string;
    }
  ): Promise<GxserverProjectDomainState> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    const response = await this.client.rpc<{ project: GxserverProjectDomainState }>('/api/addProjectPath', {
      name: input.name,
      path: input.path,
    });
    this.upsertDomainProject(response.project);
    return response.project;
  },

  async saveSidebarAgent(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'saveSidebarAgent' }>
  ): Promise<void> {
    const name = message.name.trim();
    const command = message.command.trim();
    if (!name || !command || !this.client || this.domainProjects.length === 0) {
      return;
    }
    await this.mutateSidebarHudSettings({
      acceptAllMode: message.acceptAllMode,
      activeProjectId: this.activeProjectId,
      agentId: message.agentId,
      command,
      icon: message.icon,
      name,
      operation: 'save',
      target: 'agent',
    });
  },

  async deleteSidebarAgent(this: GpuiSidebarRuntime, agentId: string): Promise<void> {
    if (!this.client || this.domainProjects.length === 0) {
      return;
    }
    await this.mutateSidebarHudSettings({
      activeProjectId: this.activeProjectId,
      agentId,
      operation: 'delete',
      target: 'agent',
    });
  },

  async syncSidebarAgentOrder(this: GpuiSidebarRuntime, requestId: string, agentIds: readonly string[]): Promise<void> {
    if (!this.client) {
      return;
    }
    const result = await this.mutateSidebarHudSettings({
      activeProjectId: this.activeProjectId,
      agentIds,
      operation: 'order',
      target: 'agent',
    });
    this.messageSource.postMessage({
      itemIds: result?.itemIds ?? [],
      kind: 'agent',
      requestId,
      status: 'success',
      type: 'sidebarOrderSyncResult',
    });
  },

  async saveSidebarCommand(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'saveSidebarCommand' }>
  ): Promise<void> {
    const project = this.activeDomainProject();
    if (!project || !this.client) {
      return;
    }
    const name = message.name.trim();
    const command = message.command?.trim();
    const url = message.url?.trim();
    if (!name && !message.icon) {
      return;
    }
    if (message.actionType === 'browser' && !url) {
      return;
    }
    if (message.actionType === 'terminal' && !command) {
      return;
    }
    await this.mutateSidebarHudSettings({
      actionType: message.actionType,
      activeProjectId: project.projectId,
      closeTerminalOnExit: message.actionType === 'terminal' ? message.closeTerminalOnExit : false,
      command,
      commandId: message.commandId,
      icon: message.icon,
      links: message.actionType === 'terminal' ? normalizeSidebarCommandLinks(message.links) : undefined,
      name,
      playCompletionSound: message.actionType === 'terminal' ? message.playCompletionSound : false,
      operation: 'save',
      showOnProjectRow: message.showOnProjectRow,
      target: 'command',
      url,
    });
  },

  async deleteSidebarCommand(this: GpuiSidebarRuntime, commandId: string): Promise<void> {
    const project = this.activeDomainProject();
    if (!project || !this.client) {
      return;
    }
    await this.mutateSidebarHudSettings({
      activeProjectId: project.projectId,
      commandId,
      operation: 'delete',
      target: 'command',
    });
  },

  async syncSidebarCommandOrder(
    this: GpuiSidebarRuntime,
    requestId: string,
    commandIds: readonly string[]
  ): Promise<void> {
    const project = this.activeDomainProject();
    if (!project || !this.client) {
      return;
    }
    const result = await this.mutateSidebarHudSettings({
      activeProjectId: project.projectId,
      commandIds,
      operation: 'order',
      target: 'command',
    });
    this.messageSource.postMessage({
      itemIds: result?.itemIds ?? [],
      kind: 'command',
      requestId,
      status: 'success',
      type: 'sidebarOrderSyncResult',
    });
  },

  /*
  CDXC:GlobalActions 2026-08-01:
  Global Action writes are not project writes: they carry no activeProjectId,
  and they do not require an active project to exist. A user with every project
  closed can still edit the actions that apply to all of them. Validation
  mirrors the project path so a save that gxserver would reject never leaves
  the renderer.
  */
  async saveGlobalSidebarCommand(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'saveGlobalSidebarCommand' }>
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const name = message.name.trim();
    const command = message.command?.trim();
    const url = message.url?.trim();
    if (!name && !message.icon) {
      return;
    }
    if (message.actionType === 'browser' && !url) {
      return;
    }
    if (message.actionType === 'terminal' && !command) {
      return;
    }
    await this.mutateSidebarHudSettings({
      actionType: message.actionType,
      closeTerminalOnExit: message.actionType === 'terminal' ? message.closeTerminalOnExit : false,
      command,
      commandId: message.commandId,
      icon: message.icon,
      links: message.actionType === 'terminal' ? normalizeSidebarCommandLinks(message.links) : undefined,
      name,
      playCompletionSound: message.actionType === 'terminal' ? message.playCompletionSound : false,
      operation: 'save',
      /*
      CDXC:GlobalActions 2026-08-07:
      gxserver stores showOnProjectRow for both lists, so a global save that
      omits it writes the flag back as false and the Settings toggle never
      sticks. Forward it exactly like the project save above.
      */
      showOnProjectRow: message.showOnProjectRow,
      target: 'globalCommand',
      url,
    });
  },

  async deleteGlobalSidebarCommand(this: GpuiSidebarRuntime, commandId: string): Promise<void> {
    if (!this.client) {
      return;
    }
    await this.mutateSidebarHudSettings({
      commandId,
      operation: 'delete',
      target: 'globalCommand',
    });
  },

  async syncGlobalSidebarCommandOrder(
    this: GpuiSidebarRuntime,
    requestId: string,
    commandIds: readonly string[]
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const result = await this.mutateSidebarHudSettings({
      commandIds,
      operation: 'order',
      target: 'globalCommand',
    });
    this.messageSource.postMessage({
      itemIds: result?.itemIds ?? [],
      kind: 'command',
      requestId,
      status: 'success',
      type: 'sidebarOrderSyncResult',
    });
  },

  pickWorkspaceFolder(this: GpuiSidebarRuntime, originalMessage: SidebarToExtensionMessage): void {
    try {
      postAppModalHostMessage({ type: 'pickWorkspaceFolder' }, 'GPUISidebarWorkspaceProjects:pickWorkspaceFolder');
    } catch {
      this.handleUnsupportedSidebarMessage(originalMessage);
    }
  },

  async handleGpuiWorkspaceFolderPicked(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    const replacement = normalizeGpuiReplacementProjectFolderPick(payload);
    if (replacement) {
      await this.relocateProjectFolder(replacement.projectId, replacement.path);
      return;
    }
    const pick = normalizeGpuiWorkspaceFolderPick(payload);
    if (!pick) {
      return;
    }
    if (!this.client) {
      this.postSidebarActionToast('error', 'Add Project failed', {
        description: 'gxserver is not connected.',
      });
      return;
    }
    try {
      const response = await this.client.rpc<{ project?: GxserverProjectDomainState }>(
        '/api/addProjectPath',
        pick.name ? { name: pick.name, path: pick.path } : { path: pick.path }
      );
      const project = response.project;
      if (!project) {
        throw new Error('gxserver did not return the added project.');
      }
      this.upsertDomainProject(project);
      this.focusProjectId(project.projectId);
      await this.refreshDomainPresentationSnapshotFromClient('patch').catch(() => {
        this.publishHudPatch();
      });
    } catch {
      this.postSidebarActionToast('error', 'Add Project failed', {
        description: 'Ghostex could not add the selected folder.',
      });
    }
  },

  ensureLocalProjectPathAvailable(this: GpuiSidebarRuntime, projectId: string): boolean {
    const group = this.latestGroups.find(
      (candidate) =>
        candidate.remoteMachineContext === undefined && candidate.projectContext?.editor.projectId === projectId
    );
    const state = group?.projectContext?.pathState;
    if (state === undefined || state === 'available') {
      return true;
    }
    this.presentMissingProjectFolder(projectId);
    return false;
  },

  presentMissingProjectFolder(this: GpuiSidebarRuntime, projectId: string): boolean {
    const group = this.latestGroups.find(
      (candidate) =>
        candidate.remoteMachineContext === undefined && candidate.projectContext?.editor.projectId === projectId
    );
    const projectPath = normalizeNonEmptyString(group?.projectContext?.path);
    if (!group || !projectPath) {
      this.postSidebarActionToast('warning', 'Project folder unavailable', {
        description: "Ghostex could not resolve this project's saved folder.",
      });
      return false;
    }
    openAppModal({
      modal: 'missingProjectFolder',
      projectId,
      projectName: group.title,
      projectPath,
      type: 'open',
    });
    return true;
  },

  async relocateProjectFolder(this: GpuiSidebarRuntime, projectId: string, path: string): Promise<void> {
    if (!this.client) {
      this.postSidebarActionToast('error', 'Could not update project folder', {
        description: 'gxserver is not connected.',
      });
      return;
    }
    try {
      const response = await this.client.rpc<{ project: GxserverProjectDomainState }>('/api/relocateProject', {
        path,
        projectId,
      });
      this.upsertDomainProject(response.project);
      await this.refreshDomainPresentationSnapshotFromClient('patch');
      postAppModalHostMessage({ type: 'close' }, 'GPUIMissingProjectFolder:resolved');
      this.postSidebarActionToast('info', 'Project folder updated');
    } catch (error) {
      this.postSidebarActionToast('error', 'Could not update project folder', {
        description: error instanceof Error ? error.message : 'Ghostex could not use the selected folder.',
      });
    }
  },

  async removeProject(this: GpuiSidebarRuntime, projectId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteReference) {
      await this.removeRemoteProject(remoteReference);
      return;
    }
    if (!this.client) {
      return;
    }
    await this.client.rpc('/api/removeProject', {
      projectId,
    });
  },

  async restoreRecentProject(this: GpuiSidebarRuntime, projectId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteReference) {
      await this.restoreRemoteRecentProject(remoteReference);
      return;
    }
    if (!this.client) {
      return;
    }
    const response = await this.client.rpc<{
      project?: GxserverProjectDomainState;
      recentProjects: GxserverRecentProjectDomainState[];
    }>('/api/restoreRecentProject', {
      projectId,
    });
    /*
    CDXC:GPUIRecentProjects 2026-06-25-19:22:
    Local Recent Project restore must mirror macOS by treating `/api/restoreRecentProject` as the authoritative recent-row mutation, activating the restored local project id, and applying a fresh gxserver presentation so the normal group returns promptly without synthesized drawer rows.
    */
    if (response.project) {
      this.upsertDomainProject(response.project);
    }
    this.recentProjects = [...response.recentProjects];
    this.focusProjectId(projectId);
    await this.refreshDomainPresentationSnapshotFromClient('patch').catch(() => {
      this.publishHudPatch();
    });
  },

  async removeRecentProject(this: GpuiSidebarRuntime, projectId: string): Promise<void> {
    const remoteReference = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteReference) {
      await this.removeRemoteRecentProject(remoteReference);
      return;
    }
    if (!this.client) {
      return;
    }
    const response = await this.client.rpc<{
      recentProjects: GxserverRecentProjectDomainState[];
    }>('/api/removeRecentProject', {
      projectId,
    });
    this.domainProjects = this.domainProjects.filter((project) => project.projectId !== projectId);
    this.recentProjects = [...response.recentProjects];
    this.publishHudPatch();
  },

  async closeProjectForGroup(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({ groupId });
    if (parseGpuiRemotePresentationGroupId(groupId)) {
      if (!remoteScope) {
        this.postRemoteToast('warning', 'Remote project close unavailable', {
          description: 'Reconnect the remote machine before closing the project.',
        });
        return;
      }
      await this.closeRemoteProjectForGroup(remoteScope, groupId);
      return;
    }
    if (!this.client) {
      return;
    }
    const projectId = this.resolveProjectIdForGroup(groupId);
    if (!projectId) {
      return;
    }
    /*
    CDXC:GPUIRecentProjects 2026-06-24-12:38:
    GPUI reuses SidebarApp's macOS close/remove split. Close must call the gxserver park endpoint with the project id resolved from the live presentation group, then consume gxserver's authoritative parked row; never synthesize a Recent Project row or map Close to hard delete when resolution or the daemon mutation fails.
    */
    const response = await this.client.rpc<{
      project: GxserverProjectDomainState;
      recentProjects: GxserverRecentProjectDomainState[];
    }>('/api/closeProjectToRecent', {
      projectId,
    });
    this.upsertDomainProject(response.project);
    this.recentProjects = [...response.recentProjects];
    if (this.activeGroupId === groupId || this.activeProjectId === projectId) {
      this.activeGroupId = undefined;
      this.activeProjectId = undefined;
    }
    this.removeLocalPresentationProject(projectId);
    if (this.presentation) {
      this.publishPresentation('patch');
      return;
    }
    this.publishHudPatch();
  },

  async removeProjectForGroup(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({ groupId });
    if (parseGpuiRemotePresentationGroupId(groupId)) {
      if (!remoteScope) {
        this.postRemoteToast('warning', 'Remote project removal unavailable', {
          description: 'Reconnect the remote machine before removing the project.',
        });
        return;
      }
      await this.removeRemoteProject(remoteScope);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (projectId) {
      await this.removeProject(projectId);
    }
  },

  resolveProjectIdForGroup(this: GpuiSidebarRuntime, groupId: string): string | undefined {
    if (parseGpuiRemotePresentationGroupId(groupId)) {
      return undefined;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    if (!projectId) {
      return undefined;
    }
    const group = this.latestGroups.find((candidate) => candidate.groupId === groupId);
    if (group?.projectContext) {
      return projectId;
    }
    return undefined;
  },

  activeDomainProject(this: GpuiSidebarRuntime): GxserverProjectDomainState | undefined {
    return this.activeProjectId
      ? this.domainProjectById(this.activeProjectId)
      : this.domainProjects.find(
          (project) => project.isRecentProject !== true && !isGpuiPresentationQuickDomainProject(project)
        );
  },

  domainProjectById(this: GpuiSidebarRuntime, projectId: string): GxserverProjectDomainState | undefined {
    return this.domainProjects.find((project) => project.projectId === projectId);
  },

  resolveDomainProjectScope(
    this: GpuiSidebarRuntime,
    scope: {
      projectId?: string;
      projectPath?: string;
    }
  ): GxserverProjectDomainState | undefined {
    if (scope.projectId) {
      const byId = this.domainProjectById(scope.projectId);
      if (byId) {
        return byId;
      }
    }
    const normalizedPath = normalizeGpuiProjectPath(scope.projectPath);
    if (!normalizedPath) {
      return undefined;
    }
    return this.domainProjects.find((project) => normalizeGpuiProjectPath(project.path) === normalizedPath);
  },

  resolveSidebarAgent(this: GpuiSidebarRuntime, agentId: string): SidebarAgentButton | undefined {
    const normalizedAgentId = agentId.trim();
    if (!normalizedAgentId) {
      return undefined;
    }
    const agents = this.sidebarHud
      ? ([...this.sidebarHud.agents] as SidebarAgentButton[])
      : createSidebarAgentButtons([], []);
    return agents.find((agent) => agent.agentId === normalizedAgentId);
  },

  /*
   * CDXC:GlobalActions 2026-08-01:
   * Scope selects the list exclusively rather than falling through from one to
   * the other. A tab strip click names a Global Action, so resolving it against
   * project commands — which a shared id would allow — would run something the
   * user did not click. Global ids are additionally barred from the reserved
   * built-in names at save time, so the two spaces cannot collide there either.
   */
  resolveSidebarCommand(
    this: GpuiSidebarRuntime,
    commandId: string,
    scope: SidebarCommandScope = 'project'
  ): SidebarCommandButton | undefined {
    const normalizedCommandId = commandId.trim();
    if (!normalizedCommandId) {
      return undefined;
    }
    if (scope === 'global') {
      const globalCommands = (this.sidebarHud?.globalCommands ?? []) as SidebarCommandButton[];
      return globalCommands.find((command) => command.commandId === normalizedCommandId);
    }
    const commands = this.sidebarHud
      ? ([...this.sidebarHud.commands] as SidebarCommandButton[])
      : createSidebarCommandButtons([], [], []);
    return commands.find((command) => command.commandId === normalizedCommandId);
  },

  /*
  CDXC:ProjectActions 2026-08-01:
  Project-row Action clicks resolve against the clicked project's own command
  list from the HUD's commandsByProject block, never the active project's list,
  so two projects with different Actions cannot cross-launch. A project id with
  no per-project entry only falls back to the flat active list when it IS the
  active project; otherwise the click is an unsupported no-op.
  */
  resolveSidebarCommandForProject(
    this: GpuiSidebarRuntime,
    commandId: string,
    projectId: string | undefined
  ): SidebarCommandButton | undefined {
    if (!projectId) {
      return this.resolveSidebarCommand(commandId);
    }
    const normalizedCommandId = commandId.trim();
    if (!normalizedCommandId) {
      return undefined;
    }
    const projectCommands = this.sidebarHud?.commandsByProject?.[projectId];
    if (projectCommands) {
      return ([...projectCommands] as SidebarCommandButton[]).find(
        (command) => command.commandId === normalizedCommandId
      );
    }
    if (projectId !== this.activeProjectId) {
      return undefined;
    }
    return this.resolveSidebarCommand(commandId);
  },

  createSidebarCommandSelectionMessage(
    this: GpuiSidebarRuntime,
    commandId: string,
    originalMessage: SidebarToExtensionMessage
  ): Extract<SidebarToExtensionMessage, { type: 'runSidebarCommand' }> | undefined {
    /*
    CDXC:GPUICommandPane 2026-06-27-07:54:
    The GPUI SidebarApp/Command Palette Action launch boundary accepts only selector-shaped `runSidebarCommand` objects: type, command id, and an own optional runMode. Renderer-supplied command text, URLs, cwd/env, paths, output, logs, run ids, and status fields are unsupported instead of being stripped into a launch.
    */
    if (Object.keys(originalMessage).some((key) => !GPUI_SIDEBAR_COMMAND_SELECTOR_MESSAGE_KEYS.has(key))) {
      return undefined;
    }
    if (!Object.prototype.hasOwnProperty.call(originalMessage, 'runMode')) {
      return {
        commandId,
        type: 'runSidebarCommand',
      };
    }
    const runMode = (originalMessage as { runMode?: unknown }).runMode;
    if (!isSidebarCommandRunMode(runMode)) {
      return undefined;
    }
    return {
      commandId,
      runMode,
      type: 'runSidebarCommand',
    };
  },

  runSidebarCommand(
    this: GpuiSidebarRuntime,
    commandId: string,
    originalMessage: SidebarToExtensionMessage,
    scope: SidebarCommandScope = 'project'
  ): void {
    /*
     * CDXC:GPUICommandPane 2026-06-26-05:11:
     * The shared SidebarApp and Command Palette emit `runSidebarCommand` as an
     * Action-selection message: command id plus optional runMode. In GPUI,
     * resolve the selected Action from the live gxserver HUD projection and hand
     * trusted launch metadata to Rust through the fixed command-action bridge so
     * command text, URLs, saved close-on-exit metadata, paths, output, and logs
     * never come from the renderer message.
     *
     * CDXC:GPUICommandPane 2026-06-27-06:37:
     * Match native sidebar dispatch for stale Action selectors: an unknown command id is an unsupported no-op, while an existing but unconfigured Action still opens Settings so the user can supply the missing command or URL.
     *
     * CDXC:GPUICommandPane 2026-06-27-07:54:
     * Treat selector shape as part of the Action contract before looking up the HUD command. Extra launch/run-state fields are unsupported no-ops, not sanitized launches, while valid configured-but-empty selectors still reach Settings like macOS.
     *
     * CDXC:ProjectActions 2026-08-01:
     * Project-row Action buttons pass the row's group id. Resolve the Action
     * from that project's own command list and activate the project through
     * the existing focus flow before dispatching, so the launch bridge payload
     * stays project-blind and the command pane opens in the project the user
     * clicked — the same ordering as clicking the row and then the titlebar
     * action by hand.
     */
    const selectionMessage = this.createSidebarCommandSelectionMessage(commandId, originalMessage);
    if (!selectionMessage) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    /*
     * CDXC:GlobalActions 2026-08-01:
     * A global-scoped selector names an action that belongs to no project, so
     * it resolves against the global list. Project selectors keep the
     * per-project resolution above: the two scopes pick different lists rather
     * than falling through to each other.
     *
     * CDXC:GlobalActions 2026-08-07:
     * Scope and group id answer different questions, so a global selector may
     * carry one: the scope picks the list, the group id picks the project to
     * activate before dispatching. That is what makes a Global Action on a
     * project row run in the row the user clicked instead of whichever project
     * happened to be active. The tab strip still sends no group id — its
     * normalizer rejects the key — so it keeps running in the active project.
     */
    const groupId =
      originalMessage.type !== 'runSidebarCommand' ? undefined : normalizeNonEmptyString(originalMessage.groupId ?? '');
    const targetProjectId = groupId ? parseGxserverPresentationProjectGroupId(groupId) : undefined;
    const command =
      scope === 'global'
        ? this.resolveSidebarCommand(commandId, scope)
        : this.resolveSidebarCommandForProject(commandId, targetProjectId);
    if (!command) {
      this.handleUnsupportedSidebarMessage(originalMessage);
      return;
    }
    if (!isSidebarCommandConfigured(command)) {
      this.openAppModal('settings');
      return;
    }
    if (targetProjectId && targetProjectId !== this.activeProjectId) {
      this.focusProjectId(targetProjectId);
      /*
       * Publish a presentation patch rather than posting focus state and
       * active-project context directly: both read `latestGroups`, which only
       * `publishPresentation` refreshes, so posting them alone would leave the
       * sidebar's active-row highlight on the previous project until an
       * unrelated delta arrived. This matches every other project-switching
       * call site in this file.
       */
      this.publishPresentation('patch');
    }
    if (this.postSidebarCommandAction(command, selectionMessage)) {
      return;
    }
    this.handleUnsupportedSidebarMessage(selectionMessage);
  },

  endSidebarCommandRun(this: GpuiSidebarRuntime, commandId: string, originalMessage: SidebarToExtensionMessage): void {
    if (this.postSidebarCommandRunEnd(commandId, originalMessage)) {
      return;
    }
    this.handleUnsupportedSidebarMessage(originalMessage);
  },

  async mutateSidebarHudSettings(
    this: GpuiSidebarRuntime,
    params: GxserverSidebarHudSettingsMutationParams
  ): Promise<GxserverSidebarHudSettingsMutationResult | undefined> {
    const client = this.client;
    if (!client) {
      return undefined;
    }
    /*
     * CDXC:SidebarHudSettingsMutation 2026-06-24-20:54:
     * GPUI SidebarApp forwards Settings agent/action save, delete, and order
     * intents to gxserver instead of normalizing custom project metadata in the
     * renderer. Apply the returned canonical project rows and HUD projection so
     * Settings rows and sidebar buttons refresh from the same daemon contract.
     */
    const response = await client.mutateSidebarHudSettings({
      ...params,
      /*
       * CDXC:ProjectActions 2026-08-01:
       * The mutation result replaces the whole HUD snapshot, so every settings
       * mutation must carry the per-project command block the sidebar rows
       * render from — otherwise an agent or action save would blank them
       * until the next full HUD poll.
       */
      includeAllProjectCommands: true,
    });
    if (this.client !== client) {
      return undefined;
    }
    for (const project of response.projects) {
      this.upsertDomainProject(project);
    }
    this.sidebarHud = response.hud;
    this.publishHudPatch();
    return response;
  },

  async updateProjectDomainState(
    this: GpuiSidebarRuntime,
    projectId: string,
    params: Record<string, unknown>
  ): Promise<GxserverProjectDomainState | undefined> {
    if (!this.client) {
      return undefined;
    }
    const response = await this.client.rpc<{ project: GxserverProjectDomainState }>('/api/updateProject', {
      ...params,
      projectId,
    });
    this.upsertDomainProject(response.project);
    this.publishHudPatch();
    this.refreshSidebarHudFromClient();
    return response.project;
  },

  upsertDomainProject(this: GpuiSidebarRuntime, nextProject: GxserverProjectDomainState): void {
    const existingIndex = this.domainProjects.findIndex((project) => project.projectId === nextProject.projectId);
    this.domainProjects =
      existingIndex >= 0
        ? this.domainProjects.map((project, index) => (index === existingIndex ? nextProject : project))
        : [...this.domainProjects, nextProject];
  },

  async refreshDomainPresentationFromClient(
    this: GpuiSidebarRuntime,
    kind: GpuiSidebarRuntimeSnapshotKind
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    const [snapshot, domainProjects, recentProjects] = await Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchProjectList(),
      client.fetchRecentProjects().catch(() => this.recentProjects),
    ]);
    if (this.client !== client) {
      return;
    }
    this.domainProjects = [...domainProjects];
    this.recentProjects = [...recentProjects];
    this.applyPresentationSnapshot(snapshot, kind);
  },

  async refreshDomainPresentationSnapshotFromClient(
    this: GpuiSidebarRuntime,
    kind: GpuiSidebarRuntimeSnapshotKind
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      return;
    }
    const [snapshot, domainProjects] = await Promise.all([
      client.fetchPresentationSnapshot(),
      client.fetchProjectList(),
    ]);
    if (this.client !== client) {
      return;
    }
    this.domainProjects = [...domainProjects];
    this.applyPresentationSnapshot(snapshot, kind);
  },
};

const gpuiSidebarRuntimeProjectAndCommandMethodsShapeCheck: GpuiSidebarRuntimeProjectAndCommandMethods =
  gpuiSidebarRuntimeProjectAndCommandMethods;
void gpuiSidebarRuntimeProjectAndCommandMethodsShapeCheck;
