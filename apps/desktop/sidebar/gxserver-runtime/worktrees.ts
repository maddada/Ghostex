/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GPUI_GXSERVER_CHATS_GROUP_ID } from './constants';
import type { GpuiSidebarRuntime } from './core';
import { createGpuiGitToastId, hasGpuiGitShortStatusChanges } from './helpers/git';
import { normalizeNonEmptyString, stringFromRecord } from './helpers/records';
import {
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
} from './helpers/remote-presentation';
import {
  createGpuiExistingWorktreeOptions,
  createGpuiWorktreeToastId,
  gpuiDirname,
  gpuiProjectNameFromPath,
  gpuiWorktreeFolderSuffix,
  gpuiWorktreeRenameUserVisibleErrorMessage,
  gpuiWorktreeSlugFromPrompt,
  gpuiWorktreeUserVisibleErrorMessage,
  isGpuiManagedWorktreeBranch,
  normalizeGpuiExistingWorktreeOptions,
  normalizeGpuiProjectPath,
  normalizeGpuiWorktreeBaseBranches,
  normalizeGpuiWorktreeDeleteBranchName,
  normalizeGpuiWorktreeMetadata,
  normalizeGpuiWorktreeParentProjectId,
  parseGpuiWorktreeModalCommand,
  resolveGpuiWorktreeDeleteBranchMetadata,
} from './helpers/worktrees';
import type { GpuiCreatedProjectAgentSessionRecord, GpuiRemoteProjectScope } from './types-and-protocol';
import { postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type { AppToastLevel } from '@/packages/shared/app-toast-contract';
import { createAppToastRequest } from '@/packages/shared/app-toast-contract';
import { normalizeghostexSettings } from '@/packages/shared/ghostex-settings';
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectGroupId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverCreateWorktreeSessionResult,
  GxserverDeleteWorktreeProjectResult,
  GxserverPresentationProject,
  GxserverProjectDomainState,
  GxserverProjectWorktreeListResult,
  GxserverRemoveSessionWorktreeResult,
  GxserverTypedOperationResult,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import { createAgentSessionDefaultTitle } from '@/packages/shared/session-grid-contract';

/*
CDXC:RepoStructure 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeWorktreeMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeWorktreeMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeWorktreeMethods {
  handleGpuiWorktreeModalCommand(payload: unknown): void;
  requestProjectWorktrees(
    message: Extract<SidebarToExtensionMessage, { type: 'requestProjectWorktrees' }>
  ): Promise<void>;
  requestRemoteProjectWorktrees(
    message: Extract<SidebarToExtensionMessage, { type: 'requestProjectWorktrees' }>,
    requestId: string
  ): Promise<void>;
  createProjectWorktree(message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>): Promise<void>;
  createNewProjectWorktree(
    message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>,
    sourceProject: GxserverProjectDomainState
  ): Promise<{ projectId: string; session: GpuiCreatedProjectAgentSessionRecord }>;
  createRemoteProjectWorktree(
    message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>
  ): Promise<void>;
  openExistingProjectWorktree(
    message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>,
    sourceProject: GxserverProjectDomainState
  ): Promise<void>;
  postProjectWorktreesResult(
    requestId: string,
    result: {
      branches?: unknown;
      error?: string;
      ok: boolean;
      worktrees?: unknown;
    }
  ): void;
  createWorktreeSession(message: Extract<SidebarToExtensionMessage, { type: 'createWorktreeSession' }>): Promise<void>;
  createRemoteWorktreeSession(
    remoteGroup: { machineId: string; projectId: string },
    message: Extract<SidebarToExtensionMessage, { type: 'createWorktreeSession' }>,
    requestId: string
  ): Promise<void>;
  removeSessionWorktree(message: Extract<SidebarToExtensionMessage, { type: 'removeSessionWorktree' }>): Promise<void>;
  postWorktreeSessionResult(
    requestId: string,
    result: {
      branch?: string;
      error?: string;
      ok: boolean;
      sessionId?: string;
      worktreePath?: string;
    }
  ): void;
  postSessionWorktreeRemovalResult(
    requestId: string,
    worktreePath: string,
    result: {
      dirty?: boolean;
      error?: string;
      ok: boolean;
      removed: boolean;
      warnings?: string[];
    }
  ): void;
  updateProjectWorktreeCommand(projectId: string, command: string): Promise<void>;
  deleteWorktreeAfterCompletedGitAction(worktreeProject: GxserverProjectDomainState): Promise<void>;
  deleteRemoteWorktreeAfterCompletedGitAction(remoteScope: GpuiRemoteProjectScope): Promise<void>;
  promptDeleteWorktreeForGroup(groupId: string): Promise<void>;
  promptRenameWorktreeForGroup(groupId: string): Promise<void>;
  confirmRenameWorktree(message: Extract<SidebarToExtensionMessage, { type: 'confirmRenameWorktree' }>): Promise<void>;
  promptDeleteRemoteWorktreeForGroup(groupId: string): Promise<boolean>;
  confirmDeleteWorktree(message: Extract<SidebarToExtensionMessage, { type: 'confirmDeleteWorktree' }>): Promise<void>;
  confirmDeleteRemoteWorktree(
    message: Extract<SidebarToExtensionMessage, { type: 'confirmDeleteWorktree' }>
  ): Promise<void>;
  postGxserverWorktreeDeleteWarnings(result: GxserverDeleteWorktreeProjectResult): void;
  ensureWorktreeBeadsHooks(project: GxserverProjectDomainState): Promise<void>;
  runWorktreeSetupCommandIfConfigured(
    worktreeProject: GxserverProjectDomainState,
    setupCommandProject: GxserverProjectDomainState
  ): Promise<void>;
  resolveUniqueWorktreeTarget(
    project: GxserverProjectDomainState,
    prompt: string
  ): Promise<{ branch: string; name: string; path: string }>;
  resolveWorktreeFamilyParentProject(project: GxserverProjectDomainState): GxserverProjectDomainState | undefined;
  isTrustedExistingWorktreePath(
    path: string,
    sourceProject: GxserverProjectDomainState,
    parentProject: GxserverProjectDomainState
  ): boolean;
  postWorktreeToast(
    level: AppToastLevel,
    title: string,
    options?: {
      description?: string;
      persistent?: boolean;
      toastId?: string;
    }
  ): void;
}

export const gpuiSidebarRuntimeWorktreeMethods = {
  handleGpuiWorktreeModalCommand(this: GpuiSidebarRuntime, payload: unknown): void {
    const message = parseGpuiWorktreeModalCommand(payload);
    if (!message) {
      return;
    }
    switch (message.type) {
      case 'requestProjectWorktrees':
        void this.requestProjectWorktrees(message);
        return;
      case 'createProjectWorktree':
        void this.createProjectWorktree(message);
        return;
      case 'confirmDeleteWorktree':
        void this.confirmDeleteWorktree(message);
        return;
      case 'confirmRenameWorktree':
        void this.confirmRenameWorktree(message);
        return;
      case 'commitWorktreeBeforeDelete':
        void this.runSidebarGitAction({
          action: 'commit',
          groupId: message.groupId,
          type: 'runSidebarGitAction',
        });
        return;
    }
  },

  async requestProjectWorktrees(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'requestProjectWorktrees' }>
  ): Promise<void> {
    const requestId = message.requestId.trim();
    if (!requestId) {
      return;
    }
    if (message.remoteMachineId?.trim()) {
      await this.requestRemoteProjectWorktrees(message, requestId);
      return;
    }
    const sourceProject = this.resolveDomainProjectScope(message) ?? this.activeDomainProject();
    if (!sourceProject || !this.client) {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: 'No active gxserver project is available.',
        ok: false,
      });
      return;
    }
    const parentProject = this.resolveWorktreeFamilyParentProject(sourceProject) ?? sourceProject;
    try {
      const [worktreeResult, branchResult] = await Promise.all([
        this.client.rpc<GxserverTypedOperationResult>('/api/runWorktreeAction', {
          action: 'list',
          projectId: parentProject.projectId,
        }),
        this.client.rpc<GxserverTypedOperationResult>('/api/runGitAction', {
          action: 'listBranches',
          projectId: parentProject.projectId,
        }),
      ]);
      if (worktreeResult.exitCode !== 0 || branchResult.exitCode !== 0) {
        throw new Error('gxserver could not read worktree metadata.');
      }
      const worktrees = createGpuiExistingWorktreeOptions(
        worktreeResult.worktrees,
        parentProject,
        sourceProject,
        this.domainProjects
      );
      this.trustedExistingWorktreeList = {
        parentProjectId: parentProject.projectId,
        paths: new Set(worktrees.map((worktree) => worktree.path)),
        sourceProjectId: sourceProject.projectId,
      };
      this.postProjectWorktreesResult(requestId, {
        branches: normalizeGpuiWorktreeBaseBranches(branchResult.branches),
        ok: true,
        worktrees,
      });
    } catch {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: 'Could not load gxserver worktrees.',
        ok: false,
      });
    }
  },

  async requestRemoteProjectWorktrees(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'requestProjectWorktrees' }>,
    requestId: string
  ): Promise<void> {
    const sourceProject = this.resolveRemotePresentationProjectScope({
      projectId: message.projectId,
      remoteMachineId: message.remoteMachineId,
    });
    if (!sourceProject) {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: 'Reconnect the remote machine before loading worktrees.',
        ok: false,
      });
      return;
    }
    try {
      const result = await this.requestRemoteGxserver<GxserverProjectWorktreeListResult>(
        sourceProject.machineId,
        '/api/listProjectWorktrees',
        {
          projectId: sourceProject.projectId,
        },
        { timeoutMs: 30_000 }
      );
      const worktrees = normalizeGpuiExistingWorktreeOptions(result.worktrees);
      this.trustedExistingWorktreeList = {
        parentProjectId: result.parentProjectId,
        paths: new Set(worktrees.map((worktree) => worktree.path)),
        remoteMachineId: sourceProject.machineId,
        sourceProjectId: result.sourceProjectId,
        worktreeKeys: new Set(
          worktrees.map((worktree) => worktree.worktreeKey?.trim()).filter((key): key is string => Boolean(key))
        ),
      };
      this.postProjectWorktreesResult(requestId, {
        branches: normalizeGpuiWorktreeBaseBranches(result.branches),
        ok: true,
        worktrees,
      });
    } catch {
      this.trustedExistingWorktreeList = undefined;
      this.postProjectWorktreesResult(requestId, {
        error: 'Could not load remote gxserver worktrees.',
        ok: false,
      });
    }
  },

  async createProjectWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>
  ): Promise<void> {
    const mode =
      message.mode === 'openExisting' ||
      normalizeGpuiProjectPath(message.existingWorktreePath) ||
      message.existingWorktreeKey?.trim()
        ? 'openExisting'
        : 'create';
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast('info', mode === 'openExisting' ? 'Opening worktree' : 'Creating worktree', {
      persistent: true,
      toastId,
    });
    try {
      if (message.remoteMachineId?.trim()) {
        await this.createRemoteProjectWorktree(message);
        this.trustedExistingWorktreeList = undefined;
        this.postWorktreeToast('success', 'Remote worktree ready', { toastId });
        return;
      }
      if (!this.client) {
        throw new Error('gxserver is unavailable.');
      }
      const sourceProject = this.resolveDomainProjectScope(message) ?? this.activeDomainProject();
      if (!sourceProject || !normalizeGpuiProjectPath(sourceProject.path)) {
        throw new Error('Open an active code project before creating a worktree.');
      }
      if (sourceProject.isRecentProject === true) {
        throw new Error('Restore the project before creating a worktree.');
      }

      if (mode === 'openExisting') {
        await this.openExistingProjectWorktree(message, sourceProject);
      } else {
        await this.createNewProjectWorktree(message, sourceProject);
      }
      this.trustedExistingWorktreeList = undefined;
      await this.refreshDomainPresentationFromClient('patch').catch(() => undefined);
      this.postWorktreeToast('success', 'Worktree ready', { toastId });
    } catch (error) {
      this.postWorktreeToast(
        'error',
        mode === 'openExisting' ? 'Could not open worktree' : 'Could not create worktree',
        {
          description: gpuiWorktreeUserVisibleErrorMessage(error),
          toastId,
        }
      );
    }
  },

  async createNewProjectWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>,
    sourceProject: GxserverProjectDomainState
  ): Promise<{ projectId: string; session: GpuiCreatedProjectAgentSessionRecord }> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    const prompt = message.prompt?.trim() ?? '';
    const baseBranch = message.baseBranch?.trim() ?? '';
    const agent = this.resolveSidebarAgent(message.agentId?.trim() ?? '');
    if (!prompt) {
      throw new Error('Worktree prompt is empty.');
    }
    if (!baseBranch) {
      throw new Error('Choose a base branch.');
    }
    if (!agent?.command?.trim()) {
      throw new Error('Choose an agent with a configured command.');
    }

    const parentProject = this.resolveWorktreeFamilyParentProject(sourceProject) ?? sourceProject;
    const gxserverParentProject = await this.registerDomainProjectPath(parentProject);
    let gxserverOperationProject = gxserverParentProject;
    let gxserverSetupCommandProject = gxserverParentProject;
    if (normalizeGpuiProjectPath(sourceProject.path) !== normalizeGpuiProjectPath(parentProject.path)) {
      gxserverOperationProject = await this.registerDomainProjectPath(sourceProject);
      gxserverSetupCommandProject = gxserverOperationProject;
    }

    const target = await this.resolveUniqueWorktreeTarget(gxserverOperationProject, prompt);
    const createResult = await this.client.rpc<GxserverTypedOperationResult>('/api/runWorktreeAction', {
      action: 'create',
      baseRef: baseBranch,
      branch: target.branch,
      projectId: gxserverOperationProject.projectId,
      worktreePath: target.path,
    });
    if (createResult.exitCode !== 0) {
      throw new Error('git worktree add failed.');
    }

    const gxserverWorktreeProject = await this.registerProjectPath({
      name: `${gxserverParentProject.name || gpuiProjectNameFromPath(gxserverParentProject.path ?? '')}-${target.name}`,
      path: target.path,
    });
    if (!normalizeGpuiWorktreeParentProjectId(gxserverWorktreeProject.worktree)) {
      throw new Error('gxserver did not register the new checkout as a worktree project.');
    }
    await this.ensureWorktreeBeadsHooks(gxserverWorktreeProject);
    await this.runWorktreeSetupCommandIfConfigured(gxserverWorktreeProject, gxserverSetupCommandProject);
    const session = await this.createAgentSessionRecordForProject(gxserverWorktreeProject, agent, prompt);
    this.focusProjectId(gxserverWorktreeProject.projectId);
    return { projectId: gxserverWorktreeProject.projectId, session };
  },

  async createRemoteProjectWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({
      projectId: message.projectId,
      remoteMachineId: message.remoteMachineId,
    });
    if (!remoteScope) {
      throw new Error('Reconnect the remote machine before creating a worktree.');
    }
    const mode = message.mode === 'openExisting' || message.existingWorktreeKey?.trim() ? 'openExisting' : 'create';
    const prompt = message.prompt?.trim() ?? '';
    const agentId = message.agentId?.trim() ?? '';
    const agentTitle = createAgentSessionDefaultTitle(this.resolveSidebarAgent(agentId)?.name ?? agentId);
    /*
    CDXC:RemoteMachines 2026-06-24-18:40:
    GPUI remote Add Worktree submits only the selected remote project id plus
    bounded create/open labels to gxserver. The remote daemon derives checkout
    paths, branch names, and open-existing worktree paths; GPUI preserves the
    shared modal's optional Open Existing prompt behavior by creating an agent
    session after the daemon returns a registered project id.
    */
    if (mode === 'openExisting') {
      const worktreeKey = message.existingWorktreeKey?.trim() ?? '';
      if (!worktreeKey || !this.isTrustedRemoteExistingWorktreeKey(worktreeKey, remoteScope)) {
        throw new Error('Choose an existing remote worktree from the latest worktree list.');
      }
      const response = await this.requestRemoteGxserver<{
        project?: GxserverPresentationProject;
      }>(
        remoteScope.machineId,
        '/api/openProjectWorktree',
        {
          projectId: remoteScope.projectId,
          worktreeKey,
        },
        { timeoutMs: 45_000 }
      );
      const project = await this.resolveRemoteWorktreeMutationProject(remoteScope.machineId, response.project);
      if (prompt) {
        if (!agentId) {
          throw new Error('Choose an agent before starting a remote worktree prompt.');
        }
        await this.createRemoteAgentSessionForProject(
          { machineId: remoteScope.machineId, projectId: project.projectId },
          agentId,
          prompt,
          agentTitle
        );
      }
      return;
    }

    const baseRef = message.baseBranch?.trim() ?? '';
    if (!prompt) {
      throw new Error('Worktree prompt is empty.');
    }
    if (!baseRef) {
      throw new Error('Choose a base branch.');
    }
    if (!agentId) {
      throw new Error('Choose an agent before creating a remote worktree.');
    }
    const response = await this.requestRemoteGxserver<{
      project?: GxserverPresentationProject;
    }>(
      remoteScope.machineId,
      '/api/createProjectWorktree',
      {
        baseRef,
        nameHint: gpuiWorktreeSlugFromPrompt(prompt),
        projectId: remoteScope.projectId,
      },
      { timeoutMs: 90_000 }
    );
    const project = await this.resolveRemoteWorktreeMutationProject(remoteScope.machineId, response.project);
    await this.createRemoteAgentSessionForProject(
      { machineId: remoteScope.machineId, projectId: project.projectId },
      agentId,
      prompt,
      agentTitle
    );
  },

  async openExistingProjectWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'createProjectWorktree' }>,
    sourceProject: GxserverProjectDomainState
  ): Promise<void> {
    const existingWorktreePath = normalizeGpuiProjectPath(message.existingWorktreePath);
    if (!existingWorktreePath) {
      throw new Error('Choose an existing worktree.');
    }
    const parentProject = this.resolveWorktreeFamilyParentProject(sourceProject) ?? sourceProject;
    if (!this.isTrustedExistingWorktreePath(existingWorktreePath, sourceProject, parentProject)) {
      throw new Error('Choose an existing worktree from the latest worktree list.');
    }
    const gxserverWorktreeProject = await this.registerProjectPath({
      name: gpuiProjectNameFromPath(existingWorktreePath),
      path: existingWorktreePath,
    });
    if (!normalizeGpuiWorktreeParentProjectId(gxserverWorktreeProject.worktree)) {
      throw new Error('The selected checkout is not a registered worktree.');
    }
    await this.ensureWorktreeBeadsHooks(gxserverWorktreeProject);
    const prompt = message.prompt?.trim() ?? '';
    const agent = this.resolveSidebarAgent(message.agentId?.trim() ?? '');
    if (prompt && !agent?.command?.trim()) {
      throw new Error('Choose an agent with a configured command.');
    }
    if (prompt && agent) {
      await this.createAgentSessionForProject(gxserverWorktreeProject, agent, prompt);
    }
    this.focusProjectId(gxserverWorktreeProject.projectId);
  },

  postProjectWorktreesResult(
    this: GpuiSidebarRuntime,
    requestId: string,
    result: {
      branches?: unknown;
      error?: string;
      ok: boolean;
      worktrees?: unknown;
    }
  ): void {
    /*
    CDXC:Worktrees 2026-07-29:
    The SAME answer also goes to the sidebar document, because Sidebar V2's
    worktree popover asks this question from inside the sidebar itself rather
    than from the app-modal window. Both listeners match on their own
    `requestId`, so each ignores the other's answers and neither had to grow a
    second host implementation of the branch/worktree probe.
    */
    this.messageSource.postMessage({
      branches: result.branches,
      error: result.error,
      ok: result.ok,
      requestId,
      type: 'projectWorktreesResult',
      worktrees: result.worktrees,
    });
    // The Worktree modal lives in the native app-modal window, not in
    // SidebarApp, so the branch/worktree list answer must travel the app-modal
    // host route (the macOS reply path) to reach it.
    try {
      postAppModalHostMessage(
        {
          branches: result.branches,
          error: result.error,
          ok: result.ok,
          requestId,
          type: 'projectWorktreesResult',
          worktrees: result.worktrees,
        },
        'AppModals:gpuiWorktree.projectWorktreesResult'
      );
    } catch {
      // Without the app-modal bridge there is no modal window waiting on this
      // request, so the answer has no destination.
    }
  },

  /*
  CDXC:Worktrees 2026-07-29:
  Sidebar V2's worktree flow is ONE gxserver call, not a client-orchestrated
  sequence. The daemon creates the checkout, runs the project's setup command,
  spawns the session with cwd=worktree, sends the optional first prompt, and
  rolls the whole thing back if any step fails — so this method cannot leave a
  half-made worktree behind the way the older client-driven Add Worktree path
  could.

  Three deliberate choices here:
  - The sidebar id is the input, gxserver ids are derived. `message.projectId`
    is the V2 row's project/group id; only the host turns it into a daemon +
    project, exactly like the settle/snooze path.
  - REMOTE machines route to their OWN daemon over the Rust bridge, exactly
    like the settle/snooze path (CDXC:StateSync 2026-07-29 — the
    bridge allow-list now carries both worktree endpoints with param shapers).
    The daemon that owns the repository is the only one that can cut a checkout
    in it, so the call goes to the machine, never to the local gxserver with a
    remote project id.
  - The created session is focused HERE (same helper quick-create uses) rather
    than by the sidebar, because only the host knows the workspace pane the
    session has to mount into.
  */
  async createWorktreeSession(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'createWorktreeSession' }>
  ): Promise<void> {
    const requestId = message.requestId.trim();
    if (!requestId) {
      return;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(message.projectId);
    if (remoteGroup) {
      await this.createRemoteWorktreeSession(remoteGroup, message, requestId);
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(message.projectId);
    if (!projectId || !this.client) {
      this.postWorktreeSessionResult(requestId, {
        error: 'Open a code project before creating a worktree session.',
        ok: false,
      });
      return;
    }
    const existingWorktreePath = normalizeGpuiProjectPath(message.existingWorktreePath);
    const agentId = message.agentId?.trim() ?? '';
    const baseBranch = message.baseBranch?.trim() ?? '';
    const firstPrompt = message.firstPrompt?.trim() ?? '';
    try {
      const result = await this.client.rpc<GxserverCreateWorktreeSessionResult>('/api/createWorktreeSession', {
        ...(agentId ? { agentId } : {}),
        ...(baseBranch ? { baseBranch } : {}),
        ...(existingWorktreePath ? { existingWorktree: { path: existingWorktreePath } } : {}),
        ...(firstPrompt ? { firstPrompt } : {}),
        projectId,
        ...(message.startFromOrigin === true ? { startFromOrigin: true } : {}),
      });
      /*
      The session row arrives with the next presentation snapshot, so refresh
      before answering: the sidebar's pending state ends on a list that already
      contains the row it was waiting for.
      */
      await this.refreshDomainPresentationFromClient('patch').catch(() => undefined);
      const createdSessionId = normalizeNonEmptyString(result.sessionId);
      const createdProjectId = projectId;
      if (createdSessionId) {
        this.focusLocalWorkspaceSession(createdProjectId, createdSessionId);
      }
      this.postWorktreeSessionResult(requestId, {
        branch: normalizeNonEmptyString(result.branch),
        ok: true,
        sessionId: createdSessionId
          ? createGxserverPresentationProjectSessionId(createdProjectId, createdSessionId)
          : undefined,
        worktreePath: normalizeNonEmptyString(result.worktreePath),
      });
    } catch (error) {
      const description = gpuiWorktreeUserVisibleErrorMessage(error);
      this.postSidebarActionToast('warning', 'Could not create worktree session', {
        description,
      });
      this.postWorktreeSessionResult(requestId, { error: description, ok: false });
    }
  },

  /*
  CDXC:StateSync 2026-07-29:
  The remote half of the worktree create, kept as its own method because every
  step after the RPC differs: the presentation to refresh is that machine's, the
  focus helper is the remote one, and the sidebar session id is machine-scoped.
  Mirrors `runSessionLifecycleCommand`'s routing rule exactly — the machine is
  read out of the id the HOST minted, never guessed from anything the renderer
  supplied.
  */
  async createRemoteWorktreeSession(
    this: GpuiSidebarRuntime,
    remoteGroup: { machineId: string; projectId: string },
    message: Extract<SidebarToExtensionMessage, { type: 'createWorktreeSession' }>,
    requestId: string
  ): Promise<void> {
    const existingWorktreePath = normalizeGpuiProjectPath(message.existingWorktreePath);
    const agentId = message.agentId?.trim() ?? '';
    const baseBranch = message.baseBranch?.trim() ?? '';
    const firstPrompt = message.firstPrompt?.trim() ?? '';
    try {
      const result = await this.requestRemoteGxserver<GxserverCreateWorktreeSessionResult>(
        remoteGroup.machineId,
        '/api/createWorktreeSession',
        {
          ...(agentId ? { agentId } : {}),
          ...(baseBranch ? { baseBranch } : {}),
          ...(existingWorktreePath ? { existingWorktree: { path: existingWorktreePath } } : {}),
          ...(firstPrompt ? { firstPrompt } : {}),
          projectId: remoteGroup.projectId,
          ...(message.startFromOrigin === true ? { startFromOrigin: true } : {}),
        },
        /*
        Cutting a worktree runs a fetch, a `git worktree add`, and the project's
        own setup command on the far side of an SSH tunnel. The bridge's 20s
        default is a create-session budget, not a repository-clone budget.
        */
        { timeoutMs: 120_000 }
      );
      await this.refreshRemotePresentationFromGxserver(remoteGroup.machineId).catch(() => undefined);
      const createdSessionId = normalizeNonEmptyString(result.sessionId);
      if (createdSessionId) {
        this.setRemotePresentationSessionFocus({
          machineId: remoteGroup.machineId,
          projectId: remoteGroup.projectId,
          sessionId: createdSessionId,
        });
      }
      this.postWorktreeSessionResult(requestId, {
        branch: normalizeNonEmptyString(result.branch),
        ok: true,
        sessionId: createdSessionId
          ? createGpuiRemotePresentationSessionId(remoteGroup.machineId, remoteGroup.projectId, createdSessionId)
          : undefined,
        worktreePath: normalizeNonEmptyString(result.worktreePath),
      });
    } catch (error) {
      const description = gpuiWorktreeUserVisibleErrorMessage(error);
      this.postSidebarActionToast('warning', 'Could not create worktree session', {
        description,
      });
      this.postWorktreeSessionResult(requestId, { error: description, ok: false });
    }
  },

  /*
  CDXC:Worktrees 2026-07-29:
  Cleanup for the checkout whose last session just closed. gxserver answers a
  dirty worktree with `removed: false, dirty: true` — a REFUSAL, not a failure —
  and the sidebar re-asks with `force`. That decision stays server-side so the
  client never has to read git status to know whether it is safe to delete.

  CDXC:StateSync 2026-07-29:
  Remote projects route to their own daemon rather than being refused. The
  worktree path travelling here came from that machine's own presentation
  (`session.cwd`), so the daemon is being handed back a path it published, and
  it still applies its own dirty check and its own path-safety normalization
  before deleting anything.
  */
  async removeSessionWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'removeSessionWorktree' }>
  ): Promise<void> {
    const requestId = message.requestId.trim();
    const worktreePath = normalizeGpuiProjectPath(message.worktreePath);
    if (!requestId || !worktreePath) {
      return;
    }
    const remoteGroup = parseGpuiRemotePresentationGroupId(message.projectId);
    const projectId = remoteGroup ? remoteGroup.projectId : parseGxserverPresentationProjectGroupId(message.projectId);
    if (!projectId || (!remoteGroup && !this.client)) {
      this.postSessionWorktreeRemovalResult(requestId, worktreePath, {
        error: 'gxserver is unavailable.',
        ok: false,
        removed: false,
      });
      return;
    }
    try {
      const params = {
        ...(message.force === true ? { force: true } : {}),
        projectId,
        worktreePath,
      };
      const result = remoteGroup
        ? await this.requestRemoteGxserver<GxserverRemoveSessionWorktreeResult>(
            remoteGroup.machineId,
            '/api/removeSessionWorktree',
            params,
            { timeoutMs: 60_000 }
          )
        : await this.client!.rpc<GxserverRemoveSessionWorktreeResult>('/api/removeSessionWorktree', params);
      const removed = result.removed === true;
      if (removed) {
        await (
          remoteGroup
            ? this.refreshRemotePresentationFromGxserver(remoteGroup.machineId)
            : this.refreshDomainPresentationFromClient('patch')
        ).catch(() => undefined);
      }
      this.postSessionWorktreeRemovalResult(requestId, worktreePath, {
        dirty: result.dirty === true,
        ok: true,
        removed,
        warnings: Array.isArray(result.warnings)
          ? result.warnings.filter((warning): warning is string => typeof warning === 'string' && warning.trim() !== '')
          : undefined,
      });
    } catch (error) {
      const description = gpuiWorktreeUserVisibleErrorMessage(error);
      this.postSidebarActionToast('warning', 'Could not remove worktree', { description });
      this.postSessionWorktreeRemovalResult(requestId, worktreePath, {
        error: description,
        ok: false,
        removed: false,
      });
    }
  },

  postWorktreeSessionResult(
    this: GpuiSidebarRuntime,
    requestId: string,
    result: {
      branch?: string;
      error?: string;
      ok: boolean;
      sessionId?: string;
      worktreePath?: string;
    }
  ): void {
    this.messageSource.postMessage({
      branch: result.branch,
      error: result.error,
      ok: result.ok,
      requestId,
      sessionId: result.sessionId,
      type: 'worktreeSessionResult',
      worktreePath: result.worktreePath,
    });
  },

  postSessionWorktreeRemovalResult(
    this: GpuiSidebarRuntime,
    requestId: string,
    worktreePath: string,
    result: {
      dirty?: boolean;
      error?: string;
      ok: boolean;
      removed: boolean;
      warnings?: string[];
    }
  ): void {
    this.messageSource.postMessage({
      dirty: result.dirty,
      error: result.error,
      ok: result.ok,
      removed: result.removed,
      requestId,
      type: 'sessionWorktreeRemovalResult',
      warnings: result.warnings,
      worktreePath,
    });
  },

  async updateProjectWorktreeCommand(this: GpuiSidebarRuntime, projectId: string, command: string): Promise<void> {
    const project = this.domainProjectById(projectId);
    if (!project || !this.client) {
      return;
    }
    const normalizedCommand = command.trim();
    await this.updateProjectDomainState(project.projectId, {
      gitConfig: {
        ...project.gitConfig,
        worktreeCommand: normalizedCommand || null,
      },
    });
  },

  async deleteWorktreeAfterCompletedGitAction(
    this: GpuiSidebarRuntime,
    worktreeProject: GxserverProjectDomainState
  ): Promise<void> {
    if (!this.client) {
      return;
    }
    const currentProject = this.domainProjectById(worktreeProject.projectId) ?? worktreeProject;
    const worktree = normalizeGpuiWorktreeMetadata(currentProject.worktree);
    if (!worktree) {
      this.postGitToast('warning', 'Worktree cleanup skipped', {
        description: 'The selected gxserver project is no longer a worktree.',
      });
      return;
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    const toastId = createGpuiGitToastId();
    this.postGitToast('info', 'Removing worktree', {
      persistent: true,
      toastId,
    });
    /*
    CDXC:Git 2026-07-29:
    Same reasoning as `confirmDeleteWorktree`: gxserver rewrites the parent
    repo's worktree list here and the flow focuses the parent afterwards. The
    parent lease was already dropped by the merge that got us here, but this
    keeps the invalidation attached to the write instead of to the caller, and
    it retires the removed project's own entries.
    */
    if (parentProject) {
      this.gitStateMemoByProjectId.delete(parentProject.projectId);
    }
    this.gitStateMemoByProjectId.delete(currentProject.projectId);
    this.gitHubStateMemoByProjectId.delete(currentProject.projectId);
    try {
      const result = await this.client.rpc<GxserverDeleteWorktreeProjectResult>('/api/deleteWorktreeProject', {
        deleteLocalBranch: false,
        deleteRemoteBranch: false,
        projectId: currentProject.projectId,
      });
      this.postGxserverWorktreeDeleteWarnings(result);
      this.domainProjects = this.domainProjects.filter((project) => project.projectId !== currentProject.projectId);
      if (parentProject) {
        this.focusProjectId(parentProject.projectId);
      } else if (this.activeProjectId === currentProject.projectId) {
        const fallbackProjectId = this.domainProjects[0]?.projectId;
        this.activeProjectId = fallbackProjectId;
        this.activeGroupId = fallbackProjectId
          ? createGxserverPresentationProjectGroupId(fallbackProjectId)
          : GPUI_GXSERVER_CHATS_GROUP_ID;
      }
      await this.refreshDomainPresentationFromClient('patch').catch(() => {
        this.publishHudPatch();
      });
      this.postGitToast('success', 'Worktree removed', { toastId });
    } catch {
      this.postGitToast('error', 'Could not remove worktree', {
        description: 'gxserver worktree cleanup failed.',
        toastId,
      });
    }
  },

  async deleteRemoteWorktreeAfterCompletedGitAction(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope
  ): Promise<void> {
    const currentProject = this.findRemotePresentationProject(remoteScope) ?? remoteScope.project;
    const worktree = normalizeGpuiWorktreeMetadata(currentProject.worktree);
    if (!worktree) {
      this.postRemoteToast('warning', 'Remote worktree cleanup skipped', {
        description: 'The selected remote project is no longer a worktree.',
      });
      return;
    }
    const toastId = createGpuiGitToastId();
    this.postGitToast('info', 'Removing remote worktree', {
      persistent: true,
      toastId,
    });
    try {
      const result = await this.requestRemoteGxserver<GxserverDeleteWorktreeProjectResult>(
        remoteScope.machineId,
        '/api/deleteWorktreeProject',
        {
          deleteLocalBranch: false,
          deleteRemoteBranch: false,
          projectId: remoteScope.projectId,
        },
        { timeoutMs: 45_000 }
      );
      this.postGxserverWorktreeDeleteWarnings(result);
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(() => undefined);
      this.postGitToast('success', 'Remote worktree removed', { toastId });
    } catch {
      this.postGitToast('error', 'Could not remove remote worktree', {
        description: 'Remote gxserver worktree cleanup failed.',
        toastId,
      });
    }
  },

  async promptDeleteWorktreeForGroup(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    if (await this.promptDeleteRemoteWorktreeForGroup(groupId)) {
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    const project = projectId ? this.domainProjectById(projectId) : undefined;
    const worktree = normalizeGpuiWorktreeMetadata(project?.worktree);
    if (!project || !worktree) {
      this.postWorktreeToast('warning', 'Not a worktree', {
        description: 'Only worktree projects can be deleted.',
      });
      return;
    }
    try {
      const [branch, status] = await Promise.all([
        this.runGitAction(project, { action: 'branch' }),
        this.runGitAction(project, { action: 'status' }),
      ]);
      if (branch.exitCode !== 0 || status.exitCode !== 0) {
        throw new Error('Could not read worktree status.');
      }
      const branchName = normalizeGpuiWorktreeDeleteBranchName(branch.stdout, worktree.branch);
      const branchMetadata = await resolveGpuiWorktreeDeleteBranchMetadata(branchName, (remoteName, remoteBranchName) =>
        this.runGitAction(project, {
          action: 'remoteBranchExists',
          branch: remoteBranchName,
          remoteName,
        })
      );
      // Delete Worktree opens only after gxserver collects fresh Git status,
      // so dirty checkouts can offer Commit before the destructive removal.
      postAppModalHostMessage(
        {
          modal: 'deleteWorktree',
          type: 'open',
          worktreeDeleteDraft: {
            ...branchMetadata,
            groupId,
            hasChanges: hasGpuiGitShortStatusChanges(status.stdout),
            projectId: project.projectId,
            statusSummary: status.stdout.trim(),
            worktreeName: project.name || worktree.name || 'worktree',
          },
        },
        'AppModals:gpuiDeleteWorktree'
      );
    } catch (error) {
      this.postWorktreeToast('error', 'Could not inspect worktree', {
        description: error instanceof Error ? error.message : 'git status failed.',
      });
    }
  },

  /*
  CDXC:Worktrees 2026-08-09-18:40:
  Everything the modal needs to decide whether a rename can happen is gathered
  HERE, before the native child window opens, because that window has no channel
  to ask gxserver anything once it is up. The split is deliberate:

  - answers that do not depend on the typed name (submodules, lock, pushed
    branch, uncommitted changes, live sessions, agent history) ride the draft;
  - answers that are pure computation over draft data (a folder that collides
    with the main checkout or with another registered project) are recomputed
    live in the modal as the user types;
  - answers that need git or the filesystem for a name nobody has typed yet (the
    destination already existing, the branch already existing, a ref-namespace
    collision) are enforced by gxserver at submit and surface as the error toast.

  Remote worktrees are out: the rename endpoint is remote-allowed, but the
  presentation-project indirection the remote delete flow needs has no rename
  counterpart yet, so a remote row gets an honest refusal instead of a modal that
  cannot submit.
  */
  async promptRenameWorktreeForGroup(this: GpuiSidebarRuntime, groupId: string): Promise<void> {
    if (parseGpuiRemotePresentationGroupId(groupId)) {
      this.postWorktreeToast('warning', 'Not available for remote worktrees', {
        description: 'Rename a remote worktree from the machine that owns it.',
      });
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    const project = projectId ? this.domainProjectById(projectId) : undefined;
    const worktree = normalizeGpuiWorktreeMetadata(project?.worktree);
    const projectPath = normalizeGpuiProjectPath(project?.path);
    if (!project || !worktree || !projectPath) {
      this.postWorktreeToast('warning', 'Not a worktree', {
        description: 'Only worktree projects can be renamed.',
      });
      return;
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    const parentProjectPath = normalizeGpuiProjectPath(parentProject?.path);
    if (!parentProject || !parentProjectPath) {
      this.postWorktreeToast('warning', 'Parent project unavailable', {
        description: "The worktree's parent project is not registered.",
      });
      return;
    }

    try {
      const [branch, status, submodules] = await Promise.all([
        this.runGitAction(project, { action: 'branch' }),
        this.runGitAction(project, { action: 'status' }),
        /*
        CDXC:Worktrees 2026-08-09-18:40:
        The submodule probe is an early warning, not the guard. A daemon that
        does not know the action rejects it, and that must not cost the user the
        whole dialog — gxserver re-checks submodules inside the rename itself and
        refuses with the same sentence. Degrade to that refusal instead.
        */
        this.runWorktreeAction(parentProject, {
          action: 'hasPopulatedSubmodules',
          worktreePath: projectPath,
        }).catch(() => undefined),
      ]);
      if (branch.exitCode !== 0 || status.exitCode !== 0) {
        throw new Error('Could not read worktree status.');
      }
      const branchName = normalizeGpuiWorktreeDeleteBranchName(branch.stdout, worktree.branch);
      const branchMetadata = await resolveGpuiWorktreeDeleteBranchMetadata(branchName, (remoteName, remoteBranchName) =>
        this.runGitAction(project, {
          action: 'remoteBranchExists',
          branch: remoteBranchName,
          remoteName,
        })
      );
      const parentFolderName = gpuiProjectNameFromPath(parentProjectPath);
      const currentFolderName = gpuiProjectNameFromPath(projectPath);
      const sessions = (this.presentation?.sessions ?? []).filter((session) => session.projectId === project.projectId);
      const runningSessionCount = sessions.filter((session) => session.lifecycleState === 'running').length;
      const warnings: string[] = [];
      if (branchMetadata.remoteBranchExists && branchName) {
        warnings.push(
          `Renaming here only renames the local branch. ${branchMetadata.remoteName}/${branchName} keeps its old name, and your next push will be rejected until you set a new upstream.`
        );
      }
      if (hasGpuiGitShortStatusChanges(status.stdout)) {
        warnings.push('This worktree has uncommitted changes. They move with the folder and are not touched.');
      }
      if (runningSessionCount > 0) {
        warnings.push(
          `${runningSessionCount} running session(s) will keep working, but their shell still thinks it is in the old folder until you cd or restart them.`
        );
      }
      if (sessions.some((session) => Boolean(session.agentId))) {
        warnings.push(
          'Agent history (Claude/Cursor) is filed under the old folder path and will not follow the rename.'
        );
      }

      postAppModalHostMessage(
        {
          modal: 'renameWorktree',
          type: 'open',
          worktreeRenameDraft: {
            ...(submodules?.exitCode === 0
              ? {
                  blockingReason:
                    'This worktree has initialised submodules, and git cannot move those. Remove them (git submodule deinit --all) or move the folder yourself.',
                }
              : {}),
            ...(branchName ? { branch: branchName } : {}),
            currentName: gpuiWorktreeFolderSuffix(currentFolderName, parentFolderName),
            currentPath: projectPath,
            parentFolderName,
            parentProjectPath,
            projectId: project.projectId,
            registeredProjectPaths: this.domainProjects
              .filter((candidate) => candidate.projectId !== project.projectId)
              .map((candidate) => normalizeGpuiProjectPath(candidate.path))
              .filter((path): path is string => Boolean(path)),
            renameBranchDefault: isGpuiManagedWorktreeBranch(branchName),
            warnings,
            worktreeName: project.name || worktree.name || currentFolderName,
          },
        },
        'AppModals:gpuiRenameWorktree'
      );
    } catch (error) {
      this.postWorktreeToast('error', 'Could not inspect worktree', {
        description: error instanceof Error ? error.message : 'git status failed.',
      });
    }
  },

  async confirmRenameWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'confirmRenameWorktree' }>
  ): Promise<void> {
    const project = this.domainProjectById(message.projectId);
    const worktree = normalizeGpuiWorktreeMetadata(project?.worktree);
    if (!project || !worktree || !this.client) {
      this.postWorktreeToast('warning', 'Worktree unavailable', {
        description: 'The selected worktree no longer exists.',
      });
      return;
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast('info', 'Renaming worktree', {
      description: project.name,
      persistent: true,
      toastId,
    });
    /*
    CDXC:Git 2026-07-29 (extended for rename):
    Same reasoning as `confirmDeleteWorktree`: the rename is a git write that
    does not go through the `runGitAction` chokepoint — gxserver moves the
    checkout and can rename the branch in the parent repo — so a memoized state
    taken before the write would otherwise be republished for the rest of the
    TTL, describing a branch and a path that no longer exist.
    */
    if (parentProject) {
      this.gitStateMemoByProjectId.delete(parentProject.projectId);
    }
    this.gitStateMemoByProjectId.delete(project.projectId);
    this.gitHubStateMemoByProjectId.delete(project.projectId);
    try {
      const result = await this.client.rpc<{ project?: GxserverProjectDomainState }>('/api/renameWorktreeProject', {
        name: message.name,
        projectId: project.projectId,
        renameBranch: message.renameBranch === true,
      });
      if (result.project) {
        this.upsertDomainProject(result.project);
      }
      await this.refreshDomainPresentationFromClient('patch').catch(() => {
        this.publishHudPatch();
      });
      this.postWorktreeToast('success', 'Worktree renamed', {
        description: result.project?.name ?? project.name,
        toastId,
      });
    } catch (error) {
      this.postWorktreeToast('error', 'Could not rename worktree', {
        description: gpuiWorktreeRenameUserVisibleErrorMessage(error),
        toastId,
      });
    }
  },

  async promptDeleteRemoteWorktreeForGroup(this: GpuiSidebarRuntime, groupId: string): Promise<boolean> {
    if (!parseGpuiRemotePresentationGroupId(groupId)) {
      return false;
    }
    const remoteScope = this.resolveRemotePresentationProjectScope({ groupId });
    const presentationProject = remoteScope
      ? (this.findRemotePresentationProject(remoteScope) ?? remoteScope.project)
      : undefined;
    const worktree = normalizeGpuiWorktreeMetadata(presentationProject?.worktree);
    if (!remoteScope || !presentationProject || !worktree) {
      this.postRemoteToast('warning', 'Remote worktree unavailable', {
        description: 'Reconnect the remote machine and try deleting the worktree again.',
      });
      return true;
    }
    try {
      const [branch, status] = await Promise.all([
        this.runRemoteGitAction(remoteScope, { action: 'branch' }),
        this.runRemoteGitAction(remoteScope, { action: 'status' }),
      ]);
      if (branch.exitCode !== 0 || status.exitCode !== 0) {
        throw new Error('Could not read remote worktree status.');
      }
      const branchName = normalizeGpuiWorktreeDeleteBranchName(branch.stdout, worktree.branch);
      const branchMetadata = await resolveGpuiWorktreeDeleteBranchMetadata(branchName, (remoteName, remoteBranchName) =>
        this.runRemoteGitAction(remoteScope, {
          action: 'remoteBranchExists',
          branch: remoteBranchName,
          remoteName,
        })
      );
      postAppModalHostMessage(
        {
          modal: 'deleteWorktree',
          type: 'open',
          worktreeDeleteDraft: {
            ...branchMetadata,
            groupId,
            hasChanges: hasGpuiGitShortStatusChanges(status.stdout),
            projectId: createGpuiRemotePresentationProjectId(remoteScope.machineId, remoteScope.projectId),
            statusSummary: status.stdout.trim(),
            worktreeName: presentationProject.title || worktree.name || 'worktree',
          },
        },
        'AppModals:gpuiDeleteWorktree.remote'
      );
    } catch (error) {
      this.postRemoteToast('error', 'Could not inspect remote worktree', {
        description: error instanceof Error ? error.message : 'git status failed.',
      });
    }
    return true;
  },

  async confirmDeleteWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'confirmDeleteWorktree' }>
  ): Promise<void> {
    if (parseGpuiRemotePresentationProjectId(message.projectId)) {
      await this.confirmDeleteRemoteWorktree(message);
      return;
    }
    const project = this.domainProjectById(message.projectId);
    const worktree = normalizeGpuiWorktreeMetadata(project?.worktree);
    if (!project || !worktree || !this.client) {
      this.postWorktreeToast('warning', 'Worktree unavailable', {
        description: 'The selected worktree no longer exists.',
      });
      return;
    }
    const parentProject = this.domainProjectById(worktree.parentProjectId);
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast('info', 'Deleting worktree', {
      description: project.name,
      persistent: true,
      toastId,
    });
    /*
    CDXC:Git 2026-07-29:
    Worktree removal is a Git write that does not go through the `runGitAction`
    chokepoint: gxserver removes the worktree from the parent repo and, when
    asked, deletes the branch there too. This flow then focuses the parent, so
    without this the parent could republish a memoized state taken while the
    branch still existed. The removed project's own entries go as well, so a
    later project registered under the same id cannot inherit a dead worktree's
    Git state. Deleting before the RPC mirrors `runGitAction` and covers a
    removal that fails partway through.
    */
    if (parentProject) {
      this.gitStateMemoByProjectId.delete(parentProject.projectId);
    }
    this.gitStateMemoByProjectId.delete(project.projectId);
    this.gitHubStateMemoByProjectId.delete(project.projectId);
    try {
      const result = await this.client.rpc<GxserverDeleteWorktreeProjectResult>('/api/deleteWorktreeProject', {
        deleteLocalBranch: message.deleteLocalBranch === true,
        deleteRemoteBranch: message.deleteRemoteBranch === true,
        projectId: project.projectId,
      });
      this.postGxserverWorktreeDeleteWarnings(result);
      this.domainProjects = this.domainProjects.filter((candidate) => candidate.projectId !== project.projectId);
      if (parentProject) {
        this.focusProjectId(parentProject.projectId);
      } else if (this.activeProjectId === project.projectId) {
        const fallbackProjectId = this.domainProjects[0]?.projectId;
        this.activeProjectId = fallbackProjectId;
        this.activeGroupId = fallbackProjectId
          ? createGxserverPresentationProjectGroupId(fallbackProjectId)
          : GPUI_GXSERVER_CHATS_GROUP_ID;
      }
      await this.refreshDomainPresentationFromClient('patch').catch(() => {
        this.publishHudPatch();
      });
      this.postWorktreeToast('success', 'Worktree deleted', {
        description: project.name,
        toastId,
      });
    } catch {
      this.postWorktreeToast('error', 'Could not delete worktree', {
        description: 'gxserver worktree removal failed.',
        toastId,
      });
    }
  },

  async confirmDeleteRemoteWorktree(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'confirmDeleteWorktree' }>
  ): Promise<void> {
    const remoteScope = this.resolveRemotePresentationProjectScope({
      projectId: message.projectId,
    });
    if (!remoteScope) {
      this.postRemoteToast('warning', 'Remote worktree unavailable', {
        description: 'Reconnect the remote machine and try deleting the worktree again.',
      });
      return;
    }
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast('info', 'Deleting remote worktree', {
      persistent: true,
      toastId,
    });
    try {
      const result = await this.requestRemoteGxserver<GxserverDeleteWorktreeProjectResult>(
        remoteScope.machineId,
        '/api/deleteWorktreeProject',
        {
          deleteLocalBranch: message.deleteLocalBranch === true,
          deleteRemoteBranch: message.deleteRemoteBranch === true,
          projectId: remoteScope.projectId,
        },
        { timeoutMs: 45_000 }
      );
      this.postGxserverWorktreeDeleteWarnings(result);
      await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(() => undefined);
      this.postWorktreeToast('success', 'Remote worktree deleted', { toastId });
    } catch {
      this.postWorktreeToast('error', 'Could not delete remote worktree', {
        description: 'Remote gxserver worktree removal failed.',
        toastId,
      });
    }
  },

  postGxserverWorktreeDeleteWarnings(this: GpuiSidebarRuntime, result: GxserverDeleteWorktreeProjectResult): void {
    for (const warning of result.warnings) {
      switch (warning.kind) {
        case 'localBranchDeleteFailed':
        case 'localBranchNotResolved':
          this.postGitToast('warning', 'Worktree removed, but local branch cleanup needs attention');
          break;
        case 'remoteBranchDeleteFailed':
        case 'remoteBranchNotResolved':
          this.postGitToast('warning', 'Worktree removed, but remote branch cleanup needs attention');
          break;
        case 'pruneFailed':
          this.postGitToast('warning', 'Worktree removed, but stale metadata cleanup needs attention');
          break;
      }
    }
  },

  async ensureWorktreeBeadsHooks(this: GpuiSidebarRuntime, project: GxserverProjectDomainState): Promise<void> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    const result = await this.client.rpc<GxserverTypedOperationResult>('/api/runWorktreeAction', {
      action: 'ensureBeadsHooks',
      projectId: project.projectId,
    });
    if (result.exitCode !== 0) {
      throw new Error('Could not prepare Beads hooks for this worktree.');
    }
  },

  async runWorktreeSetupCommandIfConfigured(
    this: GpuiSidebarRuntime,
    worktreeProject: GxserverProjectDomainState,
    setupCommandProject: GxserverProjectDomainState
  ): Promise<void> {
    /*
     * CDXC:Projects 2026-08-02:
     * This gate decides whether to call the setup endpoint at all, so it has to
     * see the Global Default the same way gxserver does. Without that, a project
     * inheriting its worktree command would return here and the configured
     * command would never run. gxserver still resolves the command it executes.
     */
    const setupCommand =
      stringFromRecord(setupCommandProject.gitConfig, 'worktreeCommand') ??
      normalizeghostexSettings(this.runtimeSettings?.settings).globalWorktreeCommand;
    if (!setupCommand.trim() || !this.client) {
      return;
    }
    const result = await this.client.rpc<GxserverTypedOperationResult>('/api/runProjectSetupCommand', {
      action: 'worktreeSetupCommand',
      projectId: worktreeProject.projectId,
      setupCommandProjectId: setupCommandProject.projectId,
    });
    if (result.exitCode !== 0) {
      throw new Error('Worktree setup command failed.');
    }
  },

  async resolveUniqueWorktreeTarget(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    prompt: string
  ): Promise<{ branch: string; name: string; path: string }> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    const sourcePath = normalizeGpuiProjectPath(project.path);
    if (!sourcePath) {
      throw new Error('Project has no registered path.');
    }
    const parentDirectory = gpuiDirname(sourcePath);
    const projectFolderName = gpuiProjectNameFromPath(sourcePath);
    const baseSlug = gpuiWorktreeSlugFromPrompt(prompt);
    const registeredPaths = new Set(
      this.domainProjects
        .map((candidate) => normalizeGpuiProjectPath(candidate.path))
        .filter((path): path is string => Boolean(path))
    );
    for (let index = 0; index < 50; index += 1) {
      const name = index === 0 ? baseSlug : `${baseSlug}-${index + 1}`;
      const branch = name;
      const path = `${parentDirectory}/${projectFolderName}-${name}`;
      const [branchCheck, pathCheck] = await Promise.all([
        this.client.rpc<GxserverTypedOperationResult>('/api/runGitAction', {
          action: 'verifyRef',
          projectId: project.projectId,
          ref: `refs/heads/${branch}`,
        }),
        this.client.rpc<GxserverTypedOperationResult>('/api/runWorktreeAction', {
          action: 'pathExists',
          projectId: project.projectId,
          worktreePath: path,
        }),
      ]);
      if (branchCheck.exitCode !== 0 && pathCheck.exitCode !== 0 && !registeredPaths.has(path)) {
        return { branch, name, path };
      }
    }
    throw new Error('Could not find an unused worktree name.');
  },

  resolveWorktreeFamilyParentProject(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState
  ): GxserverProjectDomainState | undefined {
    const parentProjectId = normalizeGpuiWorktreeParentProjectId(project.worktree);
    return parentProjectId ? this.domainProjectById(parentProjectId) : project;
  },

  isTrustedExistingWorktreePath(
    this: GpuiSidebarRuntime,
    path: string,
    sourceProject: GxserverProjectDomainState,
    parentProject: GxserverProjectDomainState
  ): boolean {
    const trusted = this.trustedExistingWorktreeList;
    return Boolean(
      trusted &&
      trusted.sourceProjectId === sourceProject.projectId &&
      trusted.parentProjectId === parentProject.projectId &&
      trusted.paths.has(path)
    );
  },

  postWorktreeToast(
    this: GpuiSidebarRuntime,
    level: AppToastLevel,
    title: string,
    options: {
      description?: string;
      persistent?: boolean;
      toastId?: string;
    } = {}
  ): void {
    try {
      postAppModalHostMessage(
        createAppToastRequest(level, title, options.description, {
          persistent: options.persistent,
          toastId: options.toastId,
        }),
        'AppModals:gpuiWorktreeToast'
      );
    } catch {
      /*
      CDXC:Worktrees 2026-06-24-18:21:
      Worktree mutations should still run when the toast host is unavailable.
      The missing toast bridge is a presentation problem, while gxserver remains
      the production owner for Git, setup, Beads hook, and agent-session state.
      */
    }
  },
};

const gpuiSidebarRuntimeWorktreeMethodsShapeCheck: GpuiSidebarRuntimeWorktreeMethods =
  gpuiSidebarRuntimeWorktreeMethods;
void gpuiSidebarRuntimeWorktreeMethodsShapeCheck;
