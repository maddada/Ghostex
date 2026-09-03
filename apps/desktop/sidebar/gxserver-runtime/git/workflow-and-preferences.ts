/*
CDXC:RepoStructure 2026-08-23:
Directory split of gxserver-runtime/git.ts (~3,251 lines) into git/. This
slice covers the prompt-agent commit/PR workflow (local and remote) and Git
preference persistence/resolution. See `index.ts` for how the runtime's Git
methods are recombined.
*/
import { DEFAULT_GPUI_PROMPT_AGENT_ID } from '../constants';
import type { GpuiSidebarRuntime } from '../core';
import { buildGpuiGitPullRequestAgentPrompt, formatGpuiGitAgentWorkflowTitle } from '../helpers/git';
import { booleanFromRecord, stringFromRecord } from '../helpers/records';
import {
  createGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
} from '../helpers/remote-presentation';
import type { GpuiGitPreferences, GpuiRemoteProjectScope } from '../types-and-protocol';
import type { GxserverPresentationProject, GxserverProjectDomainState } from '@/packages/shared/gxserver-protocol';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import type { SidebarGitState } from '@/packages/shared/sidebar-git';
import { normalizeSidebarGitAction } from '@/packages/shared/sidebar-git';

export const gpuiSidebarRuntimeGitWorkflowAndPreferencesMethods = {
  async runSidebarGitPromptAction(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    title: string,
    prompt: string,
    agentId?: string
  ): Promise<void> {
    const gitState = await this.refreshGitState({
      force: true,
      project,
      publishBusy: true,
      toastOnFailure: true,
    });
    if (!gitState.isRepo) {
      this.postGitToast('warning', 'Git unavailable', {
        description: 'Open a Git repository to use this workflow.',
      });
      return;
    }
    const agent = this.resolveDefaultPromptAgent(agentId);
    if (!agent?.command?.trim()) {
      this.postGitToast('error', 'Agent unavailable', {
        description: 'Choose a configured prompt agent before starting this Git workflow.',
      });
      return;
    }
    await this.createAgentSessionForProject(project, agent, prompt, formatGpuiGitAgentWorkflowTitle(title));
    this.postGitToast('success', 'Git workflow started');
  },

  async runRemoteSidebarGitPromptAction(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    title: string,
    prompt: string,
    agentId?: string
  ): Promise<void> {
    const gitState = await this.readRemoteSidebarGitState(remoteScope);
    if (!gitState.isRepo) {
      this.postRemoteToast('warning', 'Remote Git unavailable', {
        description: 'Open a Git repository on the remote machine to use this workflow.',
      });
      return;
    }
    const resolvedAgentId = this.resolveDefaultPromptAgentId(agentId);
    try {
      await this.createRemoteAgentSessionForProject(
        remoteScope,
        resolvedAgentId,
        prompt,
        formatGpuiGitAgentWorkflowTitle(title)
      );
      this.postRemoteToast('success', 'Remote Git workflow started');
    } catch {
      this.postRemoteToast('error', 'Remote Git workflow failed', {
        description: 'The remote gxserver could not start the selected prompt agent.',
      });
    }
  },

  async runSidebarGitPullRequestAgentWorkflow(
    this: GpuiSidebarRuntime,
    input: {
      agentId?: string;
      filePaths?: readonly string[];
      gitState: SidebarGitState;
      hasExplicitFileSelection: boolean;
      hasCommit: boolean;
      message: string;
      project: GxserverProjectDomainState;
    }
  ): Promise<void> {
    const agent = this.resolveDefaultPromptAgent(input.agentId);
    if (!agent?.command?.trim()) {
      this.postGitToast('error', 'Agent unavailable', {
        description: 'Choose a configured prompt agent before creating a pull request.',
      });
      return;
    }
    /*
    CDXC:Git 2026-06-24-16:45:
    Visible PR-agent workflows are for user-observable, non-delete PR creation only. The terminal session can report gxserver lifecycle/activity, but it cannot prove that `gh pr create` produced an open PR; delete-after cleanup must stay on the direct gxserver PR result path.
    */
    const prompt = buildGpuiGitPullRequestAgentPrompt({
      filePaths: input.filePaths,
      hasExplicitFileSelection: input.hasExplicitFileSelection,
      hasCommit: input.hasCommit,
      message: input.message.trim(),
      selectedFiles:
        input.filePaths && input.filePaths.length > 0 ? input.filePaths : input.gitState.files.map((file) => file.path),
    });
    try {
      await this.createAgentSessionForProject(
        input.project,
        agent,
        prompt,
        formatGpuiGitAgentWorkflowTitle('Commit, Push & PR')
      );
      this.postGitToast('success', 'Pull request workflow started');
    } catch {
      this.postGitToast('error', 'Pull request workflow failed', {
        description: 'gxserver could not start the selected prompt agent.',
      });
    }
  },

  async runRemoteSidebarGitPullRequestAgentWorkflow(
    this: GpuiSidebarRuntime,
    input: {
      agentId?: string;
      filePaths?: readonly string[];
      gitState: SidebarGitState;
      hasExplicitFileSelection: boolean;
      hasCommit: boolean;
      message: string;
      remoteScope: GpuiRemoteProjectScope;
    }
  ): Promise<void> {
    const resolvedAgentId = this.resolveDefaultPromptAgentId(input.agentId);
    const prompt = buildGpuiGitPullRequestAgentPrompt({
      filePaths: input.filePaths,
      hasExplicitFileSelection: input.hasExplicitFileSelection,
      hasCommit: input.hasCommit,
      message: input.message.trim(),
      selectedFiles:
        input.filePaths && input.filePaths.length > 0 ? input.filePaths : input.gitState.files.map((file) => file.path),
    });
    try {
      await this.createRemoteAgentSessionForProject(
        input.remoteScope,
        resolvedAgentId,
        prompt,
        formatGpuiGitAgentWorkflowTitle('Commit, Push & PR')
      );
      this.postRemoteToast('success', 'Remote pull request workflow started');
    } catch {
      this.postRemoteToast('error', 'Remote pull request workflow failed', {
        description: 'The remote gxserver could not start the selected prompt agent.',
      });
    }
  },

  async persistGitPreferences(
    this: GpuiSidebarRuntime,
    updates: Partial<GpuiGitPreferences>,
    scopeMessage?: {
      groupId?: string;
      projectId?: string;
    }
  ): Promise<void> {
    const explicitScope = Boolean(scopeMessage?.groupId?.trim() || scopeMessage?.projectId?.trim());
    const remoteScope = this.resolveGitPreferenceRemoteScope(scopeMessage);
    if (remoteScope) {
      await this.persistRemoteGitPreferences(remoteScope, updates);
      return;
    }
    if (explicitScope && this.isGitPreferenceRemoteScope(scopeMessage)) {
      this.postRemoteToast('warning', 'Remote Git preferences unavailable', {
        description: 'Reconnect the remote machine before changing Git preferences.',
      });
      return;
    }

    const scopedProject = this.resolveGitPreferenceLocalProject(scopeMessage);
    if (explicitScope && !scopedProject) {
      this.postGitToast('warning', 'Git preferences unavailable', {
        description: 'Choose a current project before changing Git preferences.',
      });
      return;
    }
    const currentPreferences = this.gitPreferencesForProject(scopedProject ?? this.activeDomainProject());
    const nextPreferences: GpuiGitPreferences = {
      ...currentPreferences,
      ...updates,
      primaryAction: normalizeSidebarGitAction(updates.primaryAction ?? currentPreferences.primaryAction),
    };
    if (scopedProject && this.client) {
      const nextProject = await this.updateProjectDomainState(scopedProject.projectId, {
        gitConfig: {
          ...scopedProject.gitConfig,
          confirmCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        },
      });
      if (this.activeProjectId === scopedProject.projectId || this.activeProjectId === nextProject?.projectId) {
        this.gitState = {
          ...this.gitState,
          confirmSuggestedCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        };
        this.publishHudPatch();
      }
      return;
    }
    if (!this.client || this.domainProjects.length === 0) {
      this.gitState = {
        ...this.gitState,
        confirmSuggestedCommit: nextPreferences.confirmCommit,
        generateCommitBody: nextPreferences.generateCommitBody,
        primaryAction: nextPreferences.primaryAction,
      };
      this.publishHudPatch();
      return;
    }
    await Promise.all(
      this.domainProjects.map((project) =>
        this.updateProjectDomainState(project.projectId, {
          gitConfig: {
            ...project.gitConfig,
            confirmCommit: nextPreferences.confirmCommit,
            generateCommitBody: nextPreferences.generateCommitBody,
            primaryAction: nextPreferences.primaryAction,
          },
        })
      )
    );
    this.gitState = {
      ...this.gitState,
      confirmSuggestedCommit: nextPreferences.confirmCommit,
      generateCommitBody: nextPreferences.generateCommitBody,
      primaryAction: nextPreferences.primaryAction,
    };
    this.publishHudPatch();
  },

  resolveGitPreferenceRemoteScope(
    this: GpuiSidebarRuntime,
    scopeMessage?: {
      groupId?: string;
      projectId?: string;
    }
  ): GpuiRemoteProjectScope | undefined {
    if (!scopeMessage) {
      return undefined;
    }
    if (scopeMessage.groupId && parseGpuiRemotePresentationGroupId(scopeMessage.groupId)) {
      return this.resolveRemotePresentationProjectScope({ groupId: scopeMessage.groupId });
    }
    const remoteProject = scopeMessage.projectId
      ? parseGpuiRemotePresentationProjectId(scopeMessage.projectId)
      : undefined;
    return remoteProject ? this.resolveRemotePresentationProjectScope(remoteProject) : undefined;
  },

  isGitPreferenceRemoteScope(
    this: GpuiSidebarRuntime,
    scopeMessage?: {
      groupId?: string;
      projectId?: string;
    }
  ): boolean {
    return Boolean(
      (scopeMessage?.groupId && parseGpuiRemotePresentationGroupId(scopeMessage.groupId)) ||
      (scopeMessage?.projectId && parseGpuiRemotePresentationProjectId(scopeMessage.projectId))
    );
  },

  resolveGitPreferenceLocalProject(
    this: GpuiSidebarRuntime,
    scopeMessage?: {
      groupId?: string;
      projectId?: string;
    }
  ): GxserverProjectDomainState | undefined {
    if (scopeMessage?.groupId) {
      const projectId = this.resolveProjectIdForGroup(scopeMessage.groupId);
      return projectId ? this.domainProjectById(projectId) : undefined;
    }
    if (scopeMessage?.projectId) {
      return this.domainProjectById(scopeMessage.projectId);
    }
    return undefined;
  },

  async persistRemoteGitPreferences(
    this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectScope,
    updates: Partial<GpuiGitPreferences>
  ): Promise<void> {
    const currentPreferences = this.gitPreferencesForPresentationProject(
      this.findRemotePresentationProject(remoteScope) ?? remoteScope.project
    );
    const nextPreferences: GpuiGitPreferences = {
      ...currentPreferences,
      ...updates,
      primaryAction: normalizeSidebarGitAction(updates.primaryAction ?? currentPreferences.primaryAction),
    };
    /*
    CDXC:Git 2026-06-24-18:22:
    Remote Git preference writes use only the selected machine id, gxserver project id, and the three known preference keys. Rust owns the tunnel and response shaping; the renderer never sends paths, labels, branch names, command text, URLs, tokens, stdout/stderr, or raw daemon bodies as write authority.
    */
    try {
      const response = await this.requestRemoteGxserver<{
        project?: GxserverPresentationProject;
      }>(remoteScope.machineId, '/api/updateProject', {
        gitConfig: {
          confirmCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        },
        projectId: remoteScope.projectId,
      });
      if (response.project) {
        this.upsertRemotePresentationProject(remoteScope.machineId, response.project);
      } else {
        await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(() => undefined);
      }
      if (this.activeGroupId === createGpuiRemotePresentationGroupId(remoteScope.machineId, remoteScope.projectId)) {
        this.gitState = {
          ...this.gitState,
          confirmSuggestedCommit: nextPreferences.confirmCommit,
          generateCommitBody: nextPreferences.generateCommitBody,
          primaryAction: nextPreferences.primaryAction,
        };
      }
      this.publishRemotePresentationPatch();
    } catch {
      this.postRemoteToast('warning', 'Remote Git preferences unavailable', {
        description: 'The remote gxserver could not save that Git preference.',
      });
    }
  },

  resolveGitProjectForMessage(
    this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: 'runSidebarGitAction' }>
  ): GxserverProjectDomainState | undefined {
    const projectId = message.groupId
      ? this.resolveProjectIdForGroup(message.groupId)
      : (message.projectId ?? this.activeProjectId);
    const project = projectId ? this.domainProjectById(projectId) : this.activeDomainProject();
    if (project && this.activeProjectId !== project.projectId) {
      this.focusProjectId(project.projectId);
      this.publishPresentation('patch');
    }
    return project;
  },

  gitStateForHud(this: GpuiSidebarRuntime): SidebarGitState {
    const preferences = this.gitPreferencesForProject(this.activeDomainProject());
    return {
      ...this.gitState,
      confirmSuggestedCommit: preferences.confirmCommit,
      generateCommitBody: preferences.generateCommitBody,
      primaryAction: preferences.primaryAction,
    };
  },

  gitPreferencesForProject(
    this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState | undefined
  ): GpuiGitPreferences {
    return {
      confirmCommit: booleanFromRecord(project?.gitConfig, 'confirmCommit') ?? false,
      generateCommitBody: booleanFromRecord(project?.gitConfig, 'generateCommitBody') ?? true,
      primaryAction: normalizeSidebarGitAction(stringFromRecord(project?.gitConfig, 'primaryAction')),
    };
  },

  gitPreferencesForPresentationProject(
    this: GpuiSidebarRuntime,
    project: GxserverPresentationProject | undefined
  ): GpuiGitPreferences {
    return {
      confirmCommit: booleanFromRecord(project?.gitConfig, 'confirmCommit') ?? false,
      generateCommitBody: booleanFromRecord(project?.gitConfig, 'generateCommitBody') ?? true,
      primaryAction: normalizeSidebarGitAction(stringFromRecord(project?.gitConfig, 'primaryAction')),
    };
  },

  resolveDefaultPromptAgent(this: GpuiSidebarRuntime, agentId?: string): SidebarAgentButton | undefined {
    const requestedAgentId = this.resolveDefaultPromptAgentId(agentId);
    return this.resolveSidebarAgent(requestedAgentId);
  },

  resolveDefaultPromptAgentId(this: GpuiSidebarRuntime, agentId?: string): string {
    return agentId?.trim() || this.latestHud.settings?.defaultPromptAgentId?.trim() || DEFAULT_GPUI_PROMPT_AGENT_ID;
  },
};
