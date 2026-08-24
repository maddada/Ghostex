/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY,
  GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_CACHE_MAX,
  GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_TTL_MS,
  GPUI_PROJECT_BOARD_RESUMABLE_AGENT_IDS,
  GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE,
  GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION,
} from './constants';
import type { GpuiSidebarRuntime } from './core';
import {
  gpuiProjectBoardPreviousSessionRowTitle,
  normalizeGpuiProjectBoardConversationRequest,
  normalizeGpuiProjectBoardToastLevel,
} from './helpers/project-board';
import { normalizeNonEmptyString } from './helpers/records';
import {
  createGpuiWorktreeToastId,
  gpuiWorktreeUserVisibleErrorMessage,
  normalizeGpuiProjectPath,
  normalizeGpuiWorktreeParentProjectId,
} from './helpers/worktrees';
import type { GpuiCreatedProjectAgentSessionRecord } from './types-and-protocol';
import type {
  BeadConversationLink,
  ProjectBoardAgentOption,
  ProjectBoardConversationLinkView,
  ProjectBoardConversationState,
  ProjectBoardSessionOption,
} from '@/packages/shared/bead-conversation-links';
import {
  beadConversationLinkMatchKey,
  canonicalizeBeadConversationLinksForBoard,
  createBeadConversationLinkId,
  normalizeBeadConversationLinks,
  resolveBeadConversationLinkBoardSessionId,
  selectBeadConversationLinkStoreProjects,
} from '@/packages/shared/bead-conversation-links';
import { normalizeghostexSettings } from '@/packages/shared/ghostex-settings';
import {
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectSessionId,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type {
  GxserverAgentResumePlan,
  GxserverForkSessionResult,
  GxserverPresentationSearchResponse,
  GxserverPresentationSearchResult,
  GxserverProjectDomainState,
} from '@/packages/shared/gxserver-protocol';
import type { SidebarAgentButton } from '@/packages/shared/sidebar-agents';
import { createSidebarAgentButtons } from '@/packages/shared/sidebar-agents';

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeProjectBoardMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeProjectBoardMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeProjectBoardMethods {
  handleGpuiProjectBoardConversationRequest(payload: unknown): Promise<void>;
  postGpuiProjectBoardConversationResponse(response: {
    error?: string;
    ok: boolean;
    payload?: unknown;
    requestId: string;
  }): void;
  resolveGpuiProjectBoardDomainProject(request: {
    projectId?: string;
    projectPath?: string;
  }): Promise<GxserverProjectDomainState>;
  resolveGpuiProjectBoardDomainScope(request: { projectId?: string; projectPath?: string }): Promise<{
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: GxserverProjectDomainState[];
  }>;
  listGpuiProjectBoardDomainProjects(): Promise<GxserverProjectDomainState[]>;
  selectGpuiProjectBoardDomainProject(
    request: { projectId?: string; projectPath?: string },
    projects: readonly GxserverProjectDomainState[]
  ): GxserverProjectDomainState;
  createGpuiProjectBoardConversationState(request: {
    projectId?: string;
    projectPath?: string;
  }): Promise<ProjectBoardConversationState>;
  readGpuiProjectBoardConversationLinks(
    linkStoreProjects: readonly GxserverProjectDomainState[]
  ): BeadConversationLink[];
  createGpuiProjectBoardAgentOptions(): ProjectBoardAgentOption[];
  createGpuiProjectBoardSessionOptions(
    boardProject: GxserverProjectDomainState,
    linkStoreProjects?: readonly GxserverProjectDomainState[]
  ): ProjectBoardSessionOption[];
  findGpuiProjectBoardLinkedSessionOption(
    boardProject: GxserverProjectDomainState,
    ghostexSessionId: string
  ): ProjectBoardSessionOption | undefined;
  checkGpuiProjectBoardLinkAvailability(
    boardProject: GxserverProjectDomainState,
    ghostexSessionId: string
  ): Promise<{ restorable: boolean; resumable: boolean; title?: string }>;
  checkGpuiProjectBoardLinkResumable(reference: { projectId: string; sessionId: string }): Promise<boolean>;
  findGpuiProjectBoardPreviousSessionRow(reference: {
    projectId: string;
    sessionId: string;
  }): Promise<GxserverPresentationSearchResult | undefined>;
  reloadGpuiProjectBoardDomainScope(
    boardProject: GxserverProjectDomainState,
    fallbackLinkStoreProjects: readonly GxserverProjectDomainState[]
  ): Promise<{
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: GxserverProjectDomainState[];
  }>;
  writeGpuiProjectBoardConversationLinks(
    boardProject: GxserverProjectDomainState,
    nextLinks: BeadConversationLink[]
  ): Promise<void>;
  upsertGpuiProjectBoardConversationLink(
    boardProject: GxserverProjectDomainState,
    initialLinkStoreProjects: readonly GxserverProjectDomainState[],
    args: {
      agent?: SidebarAgentButton;
      beadDisplayId?: string;
      beadId: string;
      session: GpuiCreatedProjectAgentSessionRecord;
    }
  ): Promise<void>;
  associateGpuiProjectBoardFocusedSession(request: {
    beadDisplayId?: string;
    beadId?: string;
    projectId?: string;
    projectPath?: string;
  }): Promise<void>;
  startGpuiProjectBoardWork(request: {
    agentId?: string;
    beadDisplayId?: string;
    beadId?: string;
    projectId?: string;
    projectPath?: string;
    prompt?: string;
    startLocation?: string;
  }): Promise<void>;
  startGpuiProjectBoardWorktreeWork(
    boardProject: GxserverProjectDomainState,
    agent: SidebarAgentButton,
    prompt: string
  ): Promise<GpuiCreatedProjectAgentSessionRecord>;
  jumpToGpuiProjectBoardConversation(request: {
    beadId?: string;
    projectId?: string;
    projectPath?: string;
    sessionId?: string;
  }): Promise<void>;
  resumeGpuiProjectBoardConversation(args: {
    beadId?: string;
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: readonly GxserverProjectDomainState[];
    oldGhostexSessionId: string;
    reference: { projectId: string; sessionId: string };
  }): Promise<void>;
  replaceGpuiProjectBoardConversationLinkSession(
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[],
    args: {
      beadId?: string;
      oldGhostexSessionId: string;
      restoredProjectId: string;
      restoredSessionId: string;
      restoredSessionPersistenceName?: string;
    }
  ): Promise<void>;
  unlinkGpuiProjectBoardConversation(request: {
    beadId?: string;
    projectId?: string;
    projectPath?: string;
    sessionId?: string;
  }): Promise<void>;
  mutateGpuiProjectBoardConversationLinkStores(
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[],
    mutate: (currentLinks: BeadConversationLink[], storeProject: GxserverProjectDomainState) => BeadConversationLink[]
  ): Promise<void>;
}

export const gpuiSidebarRuntimeProjectBoardMethods = {
  async handleGpuiProjectBoardConversationRequest(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    /*
    macOS `handleProjectBoardRequest` parity for the conversation half of the
    Kanban board bridge: Rust forwards the first-party page request here
    because the sidebar runtime — the GPUI equivalent of native-sidebar.tsx —
    owns agents, presentation state, focus routing, worktree creation, and the
    gxserver client. Links persist in the daemon's
    `projectBoardConfig.beadConversationLinks`, the same durable storage the
    macOS remote board flow writes.
    */
    const request = normalizeGpuiProjectBoardConversationRequest(payload);
    if (!request) {
      return;
    }
    const respond = (response: { error?: string; ok: boolean; payload?: unknown }) => {
      this.postGpuiProjectBoardConversationResponse({
        ...response,
        requestId: request.requestId,
      });
    };
    try {
      switch (request.action) {
        case 'showToast': {
          this.postSidebarActionToast(
            normalizeGpuiProjectBoardToastLevel(request.toastLevel),
            request.toastTitle?.trim() || 'Project Board update failed',
            { description: request.toastDescription?.trim() || undefined }
          );
          respond({ ok: true });
          return;
        }
        case 'appendDebugLog':
        case 'getState': {
          // appendDebugLog answers with state like macOS; the sanitized log
          // line itself is written by Rust before this request is forwarded
          // (dispatch_gpui_project_board_conversation_request), so the
          // runtime only supplies the state echo.
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case 'associateFocusedSession': {
          await this.associateGpuiProjectBoardFocusedSession(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case 'startWork': {
          await this.startGpuiProjectBoardWork(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case 'jumpToConversation': {
          await this.jumpToGpuiProjectBoardConversation(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
        case 'unlinkConversation': {
          await this.unlinkGpuiProjectBoardConversation(request);
          respond({
            ok: true,
            payload: await this.createGpuiProjectBoardConversationState(request),
          });
          return;
        }
      }
    } catch (error) {
      respond({
        error:
          error instanceof Error && error.message.trim() ? error.message : 'Project board conversation action failed.',
        ok: false,
      });
    }
  },

  postGpuiProjectBoardConversationResponse(
    this: GpuiSidebarRuntime,
    response: {
      error?: string;
      ok: boolean;
      payload?: unknown;
      requestId: string;
    }
  ): void {
    const post = window.ghostexGpui?.postProjectBoardConversationResponse;
    if (typeof post !== 'function') {
      return;
    }
    post(
      JSON.stringify({
        response,
        type: GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE,
        version: GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION,
      })
    );
  },

  async resolveGpuiProjectBoardDomainProject(
    this: GpuiSidebarRuntime,
    request: {
      projectId?: string;
      projectPath?: string;
    }
  ): Promise<GxserverProjectDomainState> {
    return (await this.resolveGpuiProjectBoardDomainScope(request)).boardProject;
  },

  async resolveGpuiProjectBoardDomainScope(
    this: GpuiSidebarRuntime,
    request: {
      projectId?: string;
      projectPath?: string;
    }
  ): Promise<{
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: GxserverProjectDomainState[];
  }> {
    const projects = await this.listGpuiProjectBoardDomainProjects();
    const boardProject = this.selectGpuiProjectBoardDomainProject(request, projects);
    return {
      boardProject,
      linkStoreProjects: selectBeadConversationLinkStoreProjects(boardProject, projects),
    };
  },

  async listGpuiProjectBoardDomainProjects(this: GpuiSidebarRuntime): Promise<GxserverProjectDomainState[]> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    // Read fresh domain projects so link mutations from other clients are
    // visible to every board response.
    const response = await this.client.rpc<{ projects?: GxserverProjectDomainState[] }>('/api/listProjects', {});
    return Array.isArray(response.projects) ? response.projects : [];
  },

  selectGpuiProjectBoardDomainProject(
    this: GpuiSidebarRuntime,
    request: { projectId?: string; projectPath?: string },
    projects: readonly GxserverProjectDomainState[]
  ): GxserverProjectDomainState {
    // macOS `resolveProjectBoardProject` order: project id, then path, then
    // the active project.
    const projectId = request.projectId?.trim();
    const byId = projectId ? projects.find((candidate) => candidate.projectId === projectId) : undefined;
    if (byId) {
      return byId;
    }
    const normalizedPath = normalizeGpuiProjectPath(request.projectPath);
    const byPath = normalizedPath
      ? projects.find((candidate) => normalizeGpuiProjectPath(candidate.path) === normalizedPath)
      : undefined;
    if (byPath) {
      return byPath;
    }
    const active = this.activeDomainProject();
    if (active) {
      return projects.find((candidate) => candidate.projectId === active.projectId) ?? active;
    }
    throw new Error('Project not found.');
  },

  async createGpuiProjectBoardConversationState(
    this: GpuiSidebarRuntime,
    request: {
      projectId?: string;
      projectPath?: string;
    }
  ): Promise<ProjectBoardConversationState> {
    const { boardProject, linkStoreProjects } = await this.resolveGpuiProjectBoardDomainScope(request);
    const sessionOptions = this.createGpuiProjectBoardSessionOptions(boardProject, linkStoreProjects);
    const sessionById = new Map(sessionOptions.map((session) => [session.sessionId, session]));
    const activeLinks = canonicalizeBeadConversationLinksForBoard(
      this.readGpuiProjectBoardConversationLinks(linkStoreProjects).filter((link) => link.status !== 'archived'),
      boardProject.projectId
    );
    const linkViews: ProjectBoardConversationLinkView[] = [];
    for (let start = 0; start < activeLinks.length; start += GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY) {
      linkViews.push(
        ...(await Promise.all(
          activeLinks.slice(start, start + GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY).map(async (link) => {
            const session =
              sessionById.get(link.ghostexSessionId) ??
              this.findGpuiProjectBoardLinkedSessionOption(boardProject, link.ghostexSessionId);
            const availability = session
              ? undefined
              : await this.checkGpuiProjectBoardLinkAvailability(boardProject, link.ghostexSessionId);
            return {
              ...link,
              agentId: link.agentId ?? session?.agentId,
              isFocused: session?.isFocused,
              isLive: Boolean(session),
              isRestorable: availability?.restorable === true,
              isResumable: availability?.resumable === true,
              isSleeping: session?.isSleeping,
              sessionTitle: session?.label ?? availability?.title,
            };
          })
        ))
      );
    }
    return {
      activeSessionId: this.activeProjectId === boardProject.projectId ? this.focusedSessionId : undefined,
      agents: this.createGpuiProjectBoardAgentOptions(),
      debuggingMode: this.runtimeSettings?.debuggingMode === true,
      // The board page gates appendDebugLog breadcrumbs on the
      // native.project.board scenario; Rust owns the actual writer and also
      // enforces the global Show debug UI controls gate.
      diagnosticLogging: normalizeghostexSettings(this.runtimeSettings?.settings).diagnosticLogging,
      defaultAgentId: this.resolveDefaultPromptAgentId(),
      focusedTerminalSessionId: sessionOptions.find((session) => session.isFocused)?.sessionId,
      links: linkViews,
      projectId: boardProject.projectId,
      sessions: sessionOptions,
    };
  },

  readGpuiProjectBoardConversationLinks(
    this: GpuiSidebarRuntime,
    linkStoreProjects: readonly GxserverProjectDomainState[]
  ): BeadConversationLink[] {
    return linkStoreProjects.flatMap((project) =>
      normalizeBeadConversationLinks(project.projectBoardConfig?.beadConversationLinks, project.projectId)
    );
  },

  createGpuiProjectBoardAgentOptions(this: GpuiSidebarRuntime): ProjectBoardAgentOption[] {
    // macOS `createProjectBoardAgentOptions` sources the configured prompt
    // agents; GPUI's configured agent registry is the gxserver-fetched HUD
    // (the same source the daemon's automation agent list reads). Commandless
    // agents cannot run board prompts.
    const agents: SidebarAgentButton[] = this.sidebarHud
      ? ([...this.sidebarHud.agents] as SidebarAgentButton[])
      : createSidebarAgentButtons([], []);
    return agents
      .filter((agent) => Boolean(agent.command?.trim()))
      .map((agent) => ({
        agentId: agent.agentId,
        command: agent.command,
        icon: agent.icon,
        label: agent.name,
      }));
  },

  createGpuiProjectBoardSessionOptions(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[] = []
  ): ProjectBoardSessionOption[] {
    const presentation = this.presentation;
    if (!presentation) {
      return [];
    }
    /*
    macOS `createProjectBoardConversationProjects` parity: a bead can be worked
    from a sibling worktree while its ticket stays on the parent board, so the
    option list spans the worktree family.

    CDXC:ProjectBoardBeads 2026-08-07:
    Rows that mount the same Beads board are part of the same board too. Their
    sessions belong in the list, or a link inherited from one of them reads as
    dead while its session is still running.
    */
    const familyParentId = normalizeGpuiWorktreeParentProjectId(boardProject.worktree) ?? boardProject.projectId;
    const relatedProjectIds = new Set<string>([
      boardProject.projectId,
      ...linkStoreProjects.map((project) => project.projectId),
    ]);
    for (const candidate of this.domainProjects) {
      if (
        candidate.projectId === familyParentId ||
        normalizeGpuiWorktreeParentProjectId(candidate.worktree) === familyParentId
      ) {
        relatedProjectIds.add(candidate.projectId);
      }
    }
    const presentationProjectTitleById = new Map(
      presentation.projects.map((project) => [project.projectId as string, project.title])
    );
    return presentation.sessions.flatMap((session): ProjectBoardSessionOption[] => {
      if (session.kind !== 'terminal' && session.kind !== 'agent') {
        return [];
      }
      if (!relatedProjectIds.has(session.projectId)) {
        return [];
      }
      const isBoardProject = session.projectId === boardProject.projectId;
      const label = isBoardProject
        ? session.title
        : `${presentationProjectTitleById.get(session.projectId) ?? session.projectId} · ${session.title}`;
      return [
        {
          agentId: session.agentName ?? session.agentId,
          agentSessionId: session.agentSessionId,
          isFocused: session.projectId === this.activeProjectId && session.sessionId === this.focusedSessionId,
          isSleeping: this.isSleepingLocalPresentationSession(session.projectId, session.sessionId),
          label,
          sessionId: isBoardProject
            ? session.sessionId
            : createGxserverPresentationProjectSessionId(session.projectId, session.sessionId),
        },
      ];
    });
  },

  findGpuiProjectBoardLinkedSessionOption(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    ghostexSessionId: string
  ): ProjectBoardSessionOption | undefined {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    The option list is scoped to the board's worktree family and board mounts,
    but a bead can be worked from any project. Jump already focuses such a
    session straight from presentation, so liveness is resolved the same way —
    otherwise a card offers to resume a conversation that is running right now.
    */
    const presentation = this.presentation;
    if (!presentation) {
      return undefined;
    }
    const reference = parseGxserverPresentationProjectSessionId(ghostexSessionId) ?? {
      projectId: boardProject.projectId,
      sessionId: ghostexSessionId,
    };
    const session = presentation.sessions.find(
      (candidate) =>
        candidate.projectId === reference.projectId &&
        candidate.sessionId === reference.sessionId &&
        (candidate.kind === 'terminal' || candidate.kind === 'agent')
    );
    if (!session) {
      return undefined;
    }
    const isBoardProject = session.projectId === boardProject.projectId;
    const projectTitle = presentation.projects.find((project) => project.projectId === session.projectId)?.title;
    return {
      agentId: session.agentName ?? session.agentId,
      agentSessionId: session.agentSessionId,
      isFocused: session.projectId === this.activeProjectId && session.sessionId === this.focusedSessionId,
      isSleeping: this.isSleepingLocalPresentationSession(session.projectId, session.sessionId),
      label: isBoardProject ? session.title : `${projectTitle ?? session.projectId} · ${session.title}`,
      sessionId: ghostexSessionId,
    };
  },

  async checkGpuiProjectBoardLinkAvailability(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    ghostexSessionId: string
  ): Promise<{ restorable: boolean; resumable: boolean; title?: string }> {
    /*
    macOS resolves link restorability from its previous-sessions cache with a
    gxserver fallback; GPUI keeps no such cache, so non-live links check the
    daemon directly behind a short TTL because getState re-runs on the board's
    8s auto-refresh.

    CDXC:ProjectBoardBeads 2026-08-07:
    Previous-session history only carries rows that closed with a trusted
    resume title, so a bead worked by a since-closed agent session usually has
    no restorable row at all. The daemon can still plan a resume from the
    session row's own agent identity, so ask it before calling the link dead.
    */
    const reference = parseGxserverPresentationProjectSessionId(ghostexSessionId) ?? {
      projectId: boardProject.projectId,
      sessionId: ghostexSessionId,
    };
    const cacheKey = `${reference.projectId}:${reference.sessionId}`;
    const now = Date.now();
    const cached = this.projectBoardRestorableLinkChecks.get(cacheKey);
    if (cached && now - cached.checkedAt < GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_TTL_MS) {
      return cached;
    }
    let result: { checkedAt: number; restorable: boolean; resumable: boolean; title?: string } = {
      checkedAt: now,
      restorable: false,
      resumable: false,
    };
    if (this.client) {
      try {
        const row = await this.findGpuiProjectBoardPreviousSessionRow(reference);
        result = {
          checkedAt: now,
          restorable: Boolean(row),
          resumable: row ? false : await this.checkGpuiProjectBoardLinkResumable(reference),
          title: row ? (row.displayTitle ?? row.primaryTitle ?? row.title) : undefined,
        };
      } catch {
        // An unavailable history lookup renders the link as not restorable
        // for this cycle; the next TTL window re-checks.
      }
    }
    if (this.projectBoardRestorableLinkChecks.size >= GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_CACHE_MAX) {
      this.projectBoardRestorableLinkChecks.clear();
    }
    this.projectBoardRestorableLinkChecks.set(cacheKey, result);
    return result;
  },

  async checkGpuiProjectBoardLinkResumable(
    this: GpuiSidebarRuntime,
    reference: {
      projectId: string;
      sessionId: string;
    }
  ): Promise<boolean> {
    // `/api/readAgentResumePlan` is the daemon's own answer to "can this
    // conversation come back": it plans from the stored agent session id,
    // session path, or trusted title, and returns no primary command when
    // there is nothing to resume. Command construction stays in gxserver.
    if (!this.client) {
      return false;
    }
    try {
      const response = await this.client.rpc<{ plan?: GxserverAgentResumePlan }>('/api/readAgentResumePlan', {
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      });
      return (
        Boolean(normalizeNonEmptyString(response.plan?.primaryCommand)) &&
        GPUI_PROJECT_BOARD_RESUMABLE_AGENT_IDS.has(normalizeNonEmptyString(response.plan?.agentId)?.toLowerCase() ?? '')
      );
    } catch {
      // A removed session row answers with an error; that is a dead link.
      return false;
    }
  },

  async findGpuiProjectBoardPreviousSessionRow(
    this: GpuiSidebarRuntime,
    reference: {
      projectId: string;
      sessionId: string;
    }
  ): Promise<GxserverPresentationSearchResult | undefined> {
    if (!this.client) {
      return undefined;
    }
    const response = await this.client.rpc<GxserverPresentationSearchResponse>('/api/listPreviousSessions', {
      includeActive: false,
      includePrevious: true,
      limit: 20,
      projectId: reference.projectId,
      query: reference.sessionId,
    });
    return response.results?.find(
      (result) =>
        result.projectId === reference.projectId &&
        result.sessionId === reference.sessionId &&
        result.lifecycleState !== 'running'
    );
  },

  async reloadGpuiProjectBoardDomainScope(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    fallbackLinkStoreProjects: readonly GxserverProjectDomainState[]
  ): Promise<{
    boardProject: GxserverProjectDomainState;
    linkStoreProjects: GxserverProjectDomainState[];
  }> {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    Starting work can take minutes before the link is written (the worktree
    path registers a project, runs setup, and refreshes presentation), and
    /api/updateProject replaces projectBoardConfig wholesale. Re-read the row
    so the link write extends the current links instead of persisting a
    snapshot taken before the session existed — otherwise a link that landed
    during the gap is dropped and its card reads as never worked.
    */
    try {
      const projects = await this.listGpuiProjectBoardDomainProjects();
      const latestBoardProject =
        projects.find((candidate) => candidate.projectId === boardProject.projectId) ?? boardProject;
      return {
        boardProject: latestBoardProject,
        linkStoreProjects: selectBeadConversationLinkStoreProjects(latestBoardProject, projects),
      };
    } catch {
      return {
        boardProject,
        linkStoreProjects: fallbackLinkStoreProjects.length > 0 ? [...fallbackLinkStoreProjects] : [boardProject],
      };
    }
  },

  async writeGpuiProjectBoardConversationLinks(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    nextLinks: BeadConversationLink[]
  ): Promise<void> {
    if (!this.client) {
      throw new Error('gxserver is unavailable.');
    }
    await this.client.rpc('/api/updateProject', {
      projectBoardConfig: {
        ...(boardProject.projectBoardConfig ?? {}),
        beadConversationLinks: nextLinks,
      },
      projectId: boardProject.projectId,
    });
  },

  async upsertGpuiProjectBoardConversationLink(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    initialLinkStoreProjects: readonly GxserverProjectDomainState[],
    args: {
      agent?: SidebarAgentButton;
      beadDisplayId?: string;
      beadId: string;
      session: GpuiCreatedProjectAgentSessionRecord;
    }
  ): Promise<void> {
    const now = new Date().toISOString();
    const presentationSession = this.presentation?.sessions.find(
      (session) => session.projectId === args.session.projectId && session.sessionId === args.session.sessionId
    );
    const { boardProject: latestBoardProject, linkStoreProjects } = await this.reloadGpuiProjectBoardDomainScope(
      boardProject,
      initialLinkStoreProjects
    );
    // A shared board is read across every row that mounts it, so update the row
    // that already holds this conversation instead of adding a second copy.
    const boardSessionId =
      args.session.projectId === latestBoardProject.projectId
        ? args.session.sessionId
        : createGxserverPresentationProjectSessionId(args.session.projectId, args.session.sessionId);
    const beadMatchKey = beadConversationLinkMatchKey(args.beadId);
    const storedLinks = this.readGpuiProjectBoardConversationLinks(linkStoreProjects);
    const linkProject =
      linkStoreProjects.find((storeProject) =>
        normalizeBeadConversationLinks(
          storeProject.projectBoardConfig?.beadConversationLinks,
          storeProject.projectId
        ).some(
          (link) =>
            beadConversationLinkMatchKey(link.beadId) === beadMatchKey &&
            resolveBeadConversationLinkBoardSessionId(link, latestBoardProject.projectId, storedLinks) ===
              boardSessionId
        )
      ) ?? latestBoardProject;
    const ghostexSessionId =
      args.session.projectId === linkProject.projectId
        ? args.session.sessionId
        : createGxserverPresentationProjectSessionId(args.session.projectId, args.session.sessionId);
    const nextLink: BeadConversationLink = {
      agentId: args.agent?.agentId ?? presentationSession?.agentId,
      agentName: args.agent?.name ?? presentationSession?.agentName,
      agentSessionId: args.session.agentSessionId ?? presentationSession?.agentSessionId,
      agentSessionPath: args.session.agentSessionPath ?? presentationSession?.agentSessionPath,
      beadDisplayId: args.beadDisplayId,
      beadId: args.beadId,
      createdAt: now,
      ghostexSessionId,
      id: createBeadConversationLinkId(linkProject.projectId, args.beadId, ghostexSessionId),
      projectId: linkProject.projectId,
      sessionPersistenceName: args.session.zmxName ?? presentationSession?.zmxName,
      sessionPersistenceProvider: 'zmx',
      sessionProjectId: args.session.projectId,
      status: 'active',
      updatedAt: now,
    };
    const currentLinks = normalizeBeadConversationLinks(
      linkProject.projectBoardConfig?.beadConversationLinks,
      linkProject.projectId
    );
    const existingLink = currentLinks.find(
      (link) =>
        beadConversationLinkMatchKey(link.beadId) === beadMatchKey &&
        resolveBeadConversationLinkBoardSessionId(link, latestBoardProject.projectId, storedLinks) === boardSessionId
    );
    const nextLinks = existingLink
      ? currentLinks.map((link) =>
          link.id === existingLink.id ? { ...link, ...nextLink, createdAt: link.createdAt } : link
        )
      : [...currentLinks, nextLink];
    await this.writeGpuiProjectBoardConversationLinks(linkProject, nextLinks);
  },

  async associateGpuiProjectBoardFocusedSession(
    this: GpuiSidebarRuntime,
    request: {
      beadDisplayId?: string;
      beadId?: string;
      projectId?: string;
      projectPath?: string;
    }
  ): Promise<void> {
    const beadId = request.beadId?.trim();
    if (!beadId) {
      throw new Error('No bead id is available.');
    }
    const { boardProject, linkStoreProjects } = await this.resolveGpuiProjectBoardDomainScope(request);
    const focusedOption = this.createGpuiProjectBoardSessionOptions(boardProject, linkStoreProjects).find(
      (session) => session.isFocused
    );
    if (!focusedOption) {
      throw new Error('Focus an agent session before associating this bead.');
    }
    const reference = parseGxserverPresentationProjectSessionId(focusedOption.sessionId) ?? {
      projectId: boardProject.projectId,
      sessionId: focusedOption.sessionId,
    };
    await this.upsertGpuiProjectBoardConversationLink(boardProject, linkStoreProjects, {
      beadDisplayId: request.beadDisplayId?.trim() || undefined,
      beadId,
      session: {
        projectId: reference.projectId,
        sessionId: reference.sessionId,
      },
    });
  },

  async startGpuiProjectBoardWork(
    this: GpuiSidebarRuntime,
    request: {
      agentId?: string;
      beadDisplayId?: string;
      beadId?: string;
      projectId?: string;
      projectPath?: string;
      prompt?: string;
      startLocation?: string;
    }
  ): Promise<void> {
    const beadId = request.beadId?.trim();
    if (!beadId) {
      throw new Error('No bead id is available.');
    }
    const prompt = request.prompt?.trim();
    if (!prompt) {
      throw new Error('No bead prompt is available.');
    }
    const { boardProject, linkStoreProjects } = await this.resolveGpuiProjectBoardDomainScope(request);
    const agent = this.resolveDefaultPromptAgent(request.agentId);
    if (!agent?.command?.trim()) {
      throw new Error('Choose a configured agent before starting work.');
    }
    let session: GpuiCreatedProjectAgentSessionRecord;
    if (request.startLocation === 'newWorktree') {
      session = await this.startGpuiProjectBoardWorktreeWork(boardProject, agent, prompt);
    } else {
      // macOS `handleProjectBoardStartWork` current-project path: focus the
      // board project, then launch the agent with the bead prompt staged as
      // the gxserver first user message (the created session is focused by
      // the create path itself).
      if (this.activeProjectId !== boardProject.projectId) {
        this.focusProjectId(boardProject.projectId);
      }
      session = await this.createAgentSessionRecordForProject(boardProject, agent, prompt, {
        errorMessage: 'Could not create an agent session for this bead.',
      });
    }
    await this.upsertGpuiProjectBoardConversationLink(boardProject, linkStoreProjects, {
      agent,
      beadDisplayId: request.beadDisplayId?.trim() || undefined,
      beadId,
      session,
    });
  },

  async startGpuiProjectBoardWorktreeWork(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    agent: SidebarAgentButton,
    prompt: string
  ): Promise<GpuiCreatedProjectAgentSessionRecord> {
    /*
    macOS board "New worktree" starts ride `createNativeWorktreeForAgentPrompt`
    with baseBranch HEAD and a "Worktree started" toast; GPUI reuses the
    reviewed worktree-modal creation path (unique target, git worktree add,
    project registration, beads hooks, setup command, prompt-staged agent
    session) with the same toast lifecycle.
    */
    const toastId = createGpuiWorktreeToastId();
    this.postWorktreeToast('info', 'Creating worktree', {
      persistent: true,
      toastId,
    });
    try {
      const created = await this.createNewProjectWorktree(
        {
          agentId: agent.agentId,
          baseBranch: 'HEAD',
          mode: 'create',
          projectId: boardProject.projectId,
          prompt,
          type: 'createProjectWorktree',
        },
        boardProject
      );
      this.trustedExistingWorktreeList = undefined;
      await this.refreshDomainPresentationFromClient('patch').catch(() => undefined);
      this.postWorktreeToast('success', 'Worktree started', { toastId });
      return created.session;
    } catch (error) {
      this.postWorktreeToast('error', 'Could not create worktree', {
        description: gpuiWorktreeUserVisibleErrorMessage(error),
        toastId,
      });
      throw error;
    }
  },

  async jumpToGpuiProjectBoardConversation(
    this: GpuiSidebarRuntime,
    request: {
      beadId?: string;
      projectId?: string;
      projectPath?: string;
      sessionId?: string;
    }
  ): Promise<void> {
    const sessionId = request.sessionId?.trim();
    if (!sessionId) {
      throw new Error('No linked conversation is selected.');
    }
    const { boardProject, linkStoreProjects } = await this.resolveGpuiProjectBoardDomainScope(request);
    const reference = parseGxserverPresentationProjectSessionId(sessionId) ?? {
      projectId: boardProject.projectId,
      sessionId,
    };
    const live = this.presentation?.sessions.some(
      (session) => session.projectId === reference.projectId && session.sessionId === reference.sessionId
    );
    if (live) {
      await this.focusSession(createGxserverPresentationProjectSessionId(reference.projectId, reference.sessionId));
      return;
    }
    /*
    macOS restores dead links through the previous-sessions owner and rewrites
    the link to the restored session; GPUI uses the same daemon restore
    contract as the Previous Sessions modal (`createSession` with
    `restoredFromSessionId`, then remove the stopped history row).
    */
    const row = await this.findGpuiProjectBoardPreviousSessionRow(reference);
    if (!row) {
      await this.resumeGpuiProjectBoardConversation({
        beadId: request.beadId?.trim() || undefined,
        boardProject,
        linkStoreProjects,
        oldGhostexSessionId: sessionId,
        reference,
      });
      return;
    }
    if (!this.client) {
      throw new Error('The linked Ghostex session is no longer available.');
    }
    const created = await this.client.rpc<{
      session?: { projectId?: string; sessionId?: string; zmxName?: string };
    }>('/api/createSession', {
      kind: 'terminal',
      lifecycleState: 'running',
      projectId: reference.projectId,
      restoredFromSessionId: reference.sessionId,
      ...(row.sessionTag ? { sessionTag: row.sessionTag } : {}),
      ...(row.sidebarOrder !== undefined ? { sidebarOrder: row.sidebarOrder } : {}),
      surface: 'workspace',
      title: gpuiProjectBoardPreviousSessionRowTitle(row),
    });
    const restoredSessionId = normalizeNonEmptyString(created.session?.sessionId);
    if (!restoredSessionId) {
      throw new Error('The linked Ghostex session could not be restored.');
    }
    const restoredProjectId = normalizeNonEmptyString(created.session?.projectId) ?? reference.projectId;
    await this.client
      .rpc('/api/removeSession', {
        projectId: reference.projectId,
        reason: 'projectBoardJumpToConversationRestore',
        sessionId: reference.sessionId,
      })
      .catch(() => undefined);
    this.projectBoardRestorableLinkChecks.delete(`${reference.projectId}:${reference.sessionId}`);
    await this.replaceGpuiProjectBoardConversationLinkSession(boardProject, linkStoreProjects, {
      beadId: request.beadId?.trim() || undefined,
      oldGhostexSessionId: sessionId,
      restoredProjectId,
      restoredSessionId,
      restoredSessionPersistenceName: normalizeNonEmptyString(created.session?.zmxName),
    });
    this.focusLocalWorkspaceSession(restoredProjectId, restoredSessionId);
  },

  async resumeGpuiProjectBoardConversation(
    this: GpuiSidebarRuntime,
    args: {
      beadId?: string;
      boardProject: GxserverProjectDomainState;
      linkStoreProjects: readonly GxserverProjectDomainState[];
      oldGhostexSessionId: string;
      reference: { projectId: string; sessionId: string };
    }
  ): Promise<void> {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    A bead's session usually closes without leaving a restorable history row,
    but the agent conversation it worked is still resumable from the session
    row's agent identity. `/api/forkSession` is the daemon-owned path for that:
    it plans the resume command in gxserver, starts the provider, and hands
    back a live session, which the bead then follows through the same link
    replacement the restore path uses.
    */
    if (!this.client) {
      throw new Error('The linked Ghostex session is no longer available.');
    }
    const { fork } = await this.client.rpc<{ fork?: GxserverForkSessionResult }>('/api/forkSession', {
      projectId: args.reference.projectId,
      reason: 'projectBoardResumeConversation',
      sessionId: args.reference.sessionId,
    });
    const resumedSessionId = normalizeNonEmptyString(fork?.session.sessionId);
    if (!resumedSessionId) {
      throw new Error('The linked conversation could not be resumed.');
    }
    const resumedProjectId = normalizeNonEmptyString(fork?.session.projectId) ?? args.reference.projectId;
    this.projectBoardRestorableLinkChecks.delete(`${args.reference.projectId}:${args.reference.sessionId}`);
    await this.replaceGpuiProjectBoardConversationLinkSession(args.boardProject, args.linkStoreProjects, {
      beadId: args.beadId,
      oldGhostexSessionId: args.oldGhostexSessionId,
      restoredProjectId: resumedProjectId,
      restoredSessionId: resumedSessionId,
      restoredSessionPersistenceName: normalizeNonEmptyString(fork?.session.zmxName),
    });
    this.focusLocalWorkspaceSession(resumedProjectId, resumedSessionId);
  },

  async replaceGpuiProjectBoardConversationLinkSession(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[],
    args: {
      beadId?: string;
      oldGhostexSessionId: string;
      restoredProjectId: string;
      restoredSessionId: string;
      restoredSessionPersistenceName?: string;
    }
  ): Promise<void> {
    // macOS `replaceProjectBoardConversationLinkSession`: every link on the
    // old session id moves to the restored one (scoped to one bead when the
    // jump carried a bead id), collapsing any pre-existing duplicate link.
    const now = new Date().toISOString();
    const ghostexSessionId =
      args.restoredProjectId === boardProject.projectId
        ? args.restoredSessionId
        : createGxserverPresentationProjectSessionId(args.restoredProjectId, args.restoredSessionId);
    const storedLinks = this.readGpuiProjectBoardConversationLinks(linkStoreProjects);
    const beadMatchKey = args.beadId ? beadConversationLinkMatchKey(args.beadId) : undefined;
    await this.mutateGpuiProjectBoardConversationLinkStores(
      boardProject,
      linkStoreProjects,
      (currentLinks, storeProject) => {
        return currentLinks.flatMap((link) => {
          const linkBeadMatches = !beadMatchKey || beadConversationLinkMatchKey(link.beadId) === beadMatchKey;
          const boardSessionId = resolveBeadConversationLinkBoardSessionId(link, boardProject.projectId, storedLinks);
          const isTarget = boardSessionId === args.oldGhostexSessionId && linkBeadMatches;
          const isDuplicateForTarget = Boolean(beadMatchKey) && linkBeadMatches && boardSessionId === ghostexSessionId;
          if (!isTarget) {
            return isDuplicateForTarget ? [] : [link];
          }
          return [
            {
              ...link,
              ghostexSessionId,
              id: createBeadConversationLinkId(storeProject.projectId, link.beadId, ghostexSessionId),
              // The stored provider name describes the session being replaced,
              // so it is re-stated from the new session rather than left to
              // describe a session this link no longer points at.
              sessionPersistenceName: args.restoredSessionPersistenceName,
              sessionProjectId: args.restoredProjectId,
              updatedAt: now,
            },
          ];
        });
      }
    );
  },

  async unlinkGpuiProjectBoardConversation(
    this: GpuiSidebarRuntime,
    request: {
      beadId?: string;
      projectId?: string;
      projectPath?: string;
      sessionId?: string;
    }
  ): Promise<void> {
    const beadId = request.beadId?.trim();
    if (!beadId) {
      throw new Error('No bead id is available.');
    }
    const sessionId = request.sessionId?.trim();
    if (!sessionId) {
      throw new Error('No linked conversation is selected.');
    }
    const { boardProject, linkStoreProjects } = await this.resolveGpuiProjectBoardDomainScope(request);
    const now = new Date().toISOString();
    const beadMatchKey = beadConversationLinkMatchKey(beadId);
    const storedLinks = this.readGpuiProjectBoardConversationLinks(linkStoreProjects);
    await this.mutateGpuiProjectBoardConversationLinkStores(boardProject, linkStoreProjects, (currentLinks) =>
      currentLinks.map((link) =>
        beadConversationLinkMatchKey(link.beadId) === beadMatchKey &&
        resolveBeadConversationLinkBoardSessionId(link, boardProject.projectId, storedLinks) === sessionId
          ? { ...link, status: 'archived' as const, updatedAt: now }
          : link
      )
    );
  },

  async mutateGpuiProjectBoardConversationLinkStores(
    this: GpuiSidebarRuntime,
    boardProject: GxserverProjectDomainState,
    linkStoreProjects: readonly GxserverProjectDomainState[],
    mutate: (currentLinks: BeadConversationLink[], storeProject: GxserverProjectDomainState) => BeadConversationLink[]
  ): Promise<void> {
    /*
    CDXC:ProjectBoardBeads 2026-08-07:
    The board reads links from every project row that mounts the same Beads
    board, so a link the user acts on can be stored on a row other than the one
    whose board is open. Apply link mutations to each row that actually holds a
    matching link; a row whose links come back unchanged is never written.
    */
    const projects = linkStoreProjects.length > 0 ? linkStoreProjects : [boardProject];
    for (const storeProject of projects) {
      const currentLinks = normalizeBeadConversationLinks(
        storeProject.projectBoardConfig?.beadConversationLinks,
        storeProject.projectId
      );
      const nextLinks = mutate(currentLinks, storeProject);
      if (JSON.stringify(nextLinks) === JSON.stringify(currentLinks)) {
        continue;
      }
      await this.writeGpuiProjectBoardConversationLinks(storeProject, nextLinks);
    }
  },
};

const gpuiSidebarRuntimeProjectBoardMethodsShapeCheck: GpuiSidebarRuntimeProjectBoardMethods =
  gpuiSidebarRuntimeProjectBoardMethods;
void gpuiSidebarRuntimeProjectBoardMethodsShapeCheck;
