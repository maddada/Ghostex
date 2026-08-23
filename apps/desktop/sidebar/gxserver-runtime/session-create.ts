/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  moveGpuiWorkspaceSessionToSubgroup,
  parseGpuiWorkspaceSessionSubgroupId,
} from "../workspace-session-groups";
import { GpuiGxserverRpcError } from "./client";
import {
  GPUI_AGENT_PROMPT_READY_DELAY_MS,
  GPUI_AGENT_PROMPT_STEP_DELAY_MS,
  GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
  GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
} from "./constants";
import type { GpuiSidebarRuntime } from "./core";
import { createGpuiSidebarSettings } from "./helpers/bootstrap";
import { delayGpuiAgentPromptStep, normalizeNonEmptyString } from "./helpers/records";
import {
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationGroupId,
  parseGpuiRemotePresentationProjectId,
} from "./helpers/remote-presentation";
import { gpuiWorkspaceTerminalTitleCommandForAgent } from "./helpers/terminal-lifecycle";
import { gpuiProjectNameFromPath } from "./helpers/worktrees";
import type {
  GpuiCreatedProjectAgentSessionRecord,
  GpuiFirstPromptTitleRuntimeSettings,
  GpuiGxserverCreatedSessionResult,
  GpuiRemoteProjectReference,
} from "./types-and-protocol";
import type { ghostexSettings } from "@/packages/shared/ghostex-settings";
import {
  createGxserverPresentationProjectGroupId,
  parseGxserverPresentationProjectGroupId,
} from "@/packages/shared/gxserver-presentation-sidebar-projection";
import type { GxserverProjectDomainState } from "@/packages/shared/gxserver-protocol";
import type { SidebarToExtensionMessage } from "@/packages/shared/session-grid-contract";
import {
  DEFAULT_TERMINAL_SESSION_TITLE,
  createAgentSessionDefaultTitle,
} from "@/packages/shared/session-grid-contract";
import type { SidebarAgentButton } from "@/packages/shared/sidebar-agents";
import { DEFAULT_BROWSER_LAUNCH_URL } from "@/packages/shared/sidebar-commands";

/*
CDXC:GxserverRuntimeSplit 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeSessionCreateMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeSessionCreateMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeSessionCreateMethods {
  createFirstPromptTitleRuntimeSettings(firstUserMessage?: string, firstUserInputDraft?: string): GpuiFirstPromptTitleRuntimeSettings;
  resolveSessionTitleGenerationCommandForGxserver(settings: ghostexSettings): string | undefined;
  createQuickProject(kind: "agent" | "terminal"): Promise<GxserverProjectDomainState | undefined>;
  createQuickTerminal(): Promise<void>;
  createQuickAgentSession(agentId: string): Promise<void>;
  openQuickBrowserTab(): void;
  openBrowserPaneInGroup(groupId: string): void;
  createSession(groupId?: string | undefined): Promise<void>;
  createProjectTerminal(message: Extract<SidebarToExtensionMessage, { type: "createProjectTerminal" }>): Promise<void>;
  startAgentSessionProviderAndSendPrompt(startProvider: () => Promise<unknown>, sendPrompt: (promptText: string) => Promise<unknown>, prompt?: string, renameCommand?: string): Promise<void>;
  startRemoteAgentSessionAndSendPrompt(machineId: string, projectId: string, sessionId: string, prompt?: string): Promise<void>;
  startLocalAgentSessionAndSendPrompt(projectId: string, sessionId: string, prompt?: string, renameCommand?: string): Promise<void>;
  createAgentSession(agentId: string, groupId?: string | undefined): Promise<void>;
  searchPreviousSessionsByText(): void;
  handleGpuiOsIntegrationCommand(payload: unknown): Promise<void>;
  createOsIntegrationTerminal(input: {
    command?: string;
    cwd?: string;
    title?: string;
  }): Promise<void>;
  openOsIntegrationProjectPaths(entries: unknown[]): Promise<void>;
  createAgentSessionForProject(project: GxserverProjectDomainState, agent: SidebarAgentButton, prompt: string, title?: string): Promise<string>;
  createAgentSessionRecordForProject(project: GxserverProjectDomainState, agent: SidebarAgentButton, prompt: string, options?: {
      errorMessage?: string;
      firstUserInputDraft?: string;
      renameTitleAfterStart?: string;
      title?: string;
    }): Promise<GpuiCreatedProjectAgentSessionRecord>;
  createRemoteAgentSessionForProject(remoteScope: GpuiRemoteProjectReference, agentId: string, prompt: string, title: string, options?: { firstUserInputDraft?: string }): Promise<void>;
}

export const gpuiSidebarRuntimeSessionCreateMethods = {

  createFirstPromptTitleRuntimeSettings(this: GpuiSidebarRuntime,
    firstUserMessage?: string,
    firstUserInputDraft?: string,
  ): GpuiFirstPromptTitleRuntimeSettings {
    /*
    CDXC:GPUIFirstPromptTitle 2026-07-04-21:52:
    GPUI agent sessions must carry the same gxserver-owned first-prompt title
    settings as macOS before hooks claim the prompt. The daemon still owns
    eligibility, title generation, and command submission; GPUI only supplies
    the user's saved title-generation agent/command and any already-known first
    prompt.
    */
    const settings = createGpuiSidebarSettings(this.runtimeSettings);
    const runtimeSettings: GpuiFirstPromptTitleRuntimeSettings = {
      firstPromptTitleGenerationAgent: settings.sessionTitleGenerationAgent,
    };
    const command = this.resolveSessionTitleGenerationCommandForGxserver(settings);
    if (command) {
      runtimeSettings.firstPromptTitleGenerationCommand = command;
    }
    const prompt = firstUserMessage?.trim();
    if (prompt) {
      runtimeSettings.firstUserMessage = prompt;
    }
    /*
    CDXC:ExportTranscript 2026-08-20:
    A draft is the opposite of `firstUserMessage`: gxserver types it into the
    new agent's composer once and never submits it. It travels to the daemon
    byte for byte — the trailing space of `@<path> ` is what closes the file
    mention and separates it from the prompt the user writes next — so it is
    deliberately not trimmed here or anywhere else on the way out.
    */
    if (firstUserInputDraft) {
      runtimeSettings.firstUserInputDraft = firstUserInputDraft;
    }
    return runtimeSettings;
  },

  resolveSessionTitleGenerationCommandForGxserver(this: GpuiSidebarRuntime,
    settings: ghostexSettings,
  ): string | undefined {
    if (settings.sessionTitleGenerationAgent === "custom") {
      return settings.customSessionTitleGenerationCommand.trim() || undefined;
    }
    return (
      this.resolveSidebarAgent(settings.sessionTitleGenerationAgent)?.command?.trim() || undefined
    );
  },

  async createQuickProject(this: GpuiSidebarRuntime,
    kind: "agent" | "terminal",
  ): Promise<GxserverProjectDomainState | undefined> {
    if (!this.client) {
      this.postSidebarActionToast("warning", "Quick action unavailable", {
        description: "gxserver is not connected.",
      });
      return undefined;
    }
    try {
      const response = await this.client.rpc<{ project: GxserverProjectDomainState }>(
        "/api/createQuickProject",
        {
          kind,
        },
      );
      this.upsertDomainProject(response.project);
      this.focusProjectId(response.project.projectId);
      this.publishPresentation("patch");
      return response.project;
    } catch {
      this.postSidebarActionToast("error", "Quick action failed", {
        description: "Ghostex could not create the Quick workspace.",
      });
      return undefined;
    }
  },

  async createQuickTerminal(this: GpuiSidebarRuntime): Promise<void> {
    /*
    CDXC:GPUIQuickActions 2026-07-11:
    Match macOS createNativeChat: create and focus a new projectless chat
    workspace first, then create its initial running terminal through the
    ordinary gxserver session path.
    */
    const project = await this.createQuickProject("terminal");
    if (project) {
      await this.createSession(createGxserverPresentationProjectGroupId(project.projectId));
    }
  },

  async createQuickAgentSession(this: GpuiSidebarRuntime, agentId: string): Promise<void> {
    /*
    Match macOS createNativeAgentChat: a Quick agent never launches inside the
    active code project. Give it a new projectless chat workspace, then reuse
    the same configured-agent launch path as project headers.
    */
    const project = await this.createQuickProject("agent");
    if (project) {
      await this.createAgentSession(
        agentId,
        createGxserverPresentationProjectGroupId(project.projectId),
      );
    }
  },

  openQuickBrowserTab(this: GpuiSidebarRuntime): void {
    /*
    GPUI currently owns Browser tabs at the window level instead of as Agents
    workspace sessions. Send the Quick header's explicit browser launch through
    the existing app-owned Browser bridge, with a distinct fixed origin so Rust
    can honor this projectless launcher even while project-scoped Browser mode
    is otherwise disabled in Quick context.
    */
    const post = window.ghostexGpui?.postOpenBrowserUrl;
    if (typeof post !== "function") {
      this.postSidebarActionToast("warning", "Quick Browser unavailable");
      return;
    }
    const accepted = post(
      JSON.stringify({
        origin: "quickHeader",
        reuse: "none",
        type: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
        url: DEFAULT_BROWSER_LAUNCH_URL,
        version: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
      }),
    );
    if (!accepted) {
      this.postSidebarActionToast("warning", "Quick Browser unavailable");
    }
  },

  openBrowserPaneInGroup(this: GpuiSidebarRuntime, groupId: string): void {
    const projectId = this.resolveWorkspaceGroupProjectId(groupId);
    if (!projectId) {
      return;
    }
    /*
    CDXC:GPUIRemoteBrowserTabs 2026-07-12:
    Browser tabs are project-keyed local CEF panes, so remote projects reuse
    the same workarea through their machine-scoped project ids. The payload
    carries the explicit target project id so Rust swaps the browser project
    model before creating the tab instead of racing the async active-project
    context round-trip through React.
    */
    const remoteProject = parseGpuiRemotePresentationProjectId(projectId);
    if (remoteProject) {
      this.activeGroupId = groupId;
      this.publishRemotePresentationPatch();
      /*
      CDXC:GPUIRemotePortsBrowser 2026-07-30:
      A remote project's Browser pane defaults to the machine's listening-ports
      page instead of the generic launch URL, so the tab lands on the remote's
      address with its running apps one click away. Rust owns SSH port
      discovery, page generation, and the final tab URL; the renderer sends
      only the fixed action plus the machine-scoped project id.
      */
      if (
        !this.postRemoteProjectNativeAction("openRemoteProjectPortsBrowser", remoteProject, {
          groupId,
          type: "openBrowserPaneInGroup",
        })
      ) {
        this.postSidebarActionToast("warning", "Browser unavailable");
      }
      return;
    }
    if (!this.presentation) {
      return;
    }
    this.activeProjectId = projectId;
    this.activeGroupId = groupId;
    this.publishPresentation("patch");

    const post = window.ghostexGpui?.postOpenBrowserUrl;
    if (
      typeof post !== "function" ||
      !post(
        JSON.stringify({
          projectId,
          reuse: "none",
          type: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
          url: DEFAULT_BROWSER_LAUNCH_URL,
          version: GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
        }),
      )
    ) {
      this.postSidebarActionToast("warning", "Browser unavailable");
    }
  },

  async createSession(this: GpuiSidebarRuntime, groupId = this.activeGroupId): Promise<void> {
    const subgroup = groupId ? parseGpuiWorkspaceSessionSubgroupId(groupId) : undefined;
    const subgroupRemoteProject = subgroup
      ? parseGpuiRemotePresentationProjectId(subgroup.projectId)
      : undefined;
    const remoteGroup =
      groupId && !subgroup ? parseGpuiRemotePresentationGroupId(groupId) : undefined;
    const remoteTarget = subgroupRemoteProject ?? remoteGroup;
    if (remoteTarget) {
      await this.requestRemoteGxserver<GpuiGxserverCreatedSessionResult>(
        remoteTarget.machineId,
        "/api/createSession",
        {
          kind: "terminal",
          lifecycleState: "running",
          projectId: remoteTarget.projectId,
          surface: "workspace",
          title: DEFAULT_TERMINAL_SESSION_TITLE,
        },
      )
        .then((response) => {
          const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
          if (createdSessionId) {
            if (subgroup && subgroupRemoteProject) {
              this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
                this.workspaceGroups,
                subgroup.projectId,
                createdSessionId,
                subgroup.groupId,
              );
              this.persistWorkspaceGroups();
            }
            const createdReference = {
              machineId: remoteTarget.machineId,
              projectId:
                normalizeNonEmptyString(response.session?.projectId) ?? remoteTarget.projectId,
              sessionId: createdSessionId,
            };
            this.setRemotePresentationSessionFocus(createdReference);
            this.postRemoteSessionNativeAction("openRemoteSessionTerminal", createdReference, {
              sessionId: createGpuiRemotePresentationSessionId(
                createdReference.machineId,
                createdReference.projectId,
                createdReference.sessionId,
              ),
              type: "focusSession",
            });
          }
          this.refreshRemotePresentationFromGxserver(remoteTarget.machineId).catch(() => undefined);
        })
        .catch(() => {
          this.postRemoteToast("warning", "Remote session failed", {
            description: "The remote gxserver could not create that session.",
          });
        });
      return;
    }
    const projectId = subgroup
      ? subgroup.projectId
      : groupId
        ? parseGxserverPresentationProjectGroupId(groupId)
        : this.activeProjectId;
    if (!this.client) {
      return;
    }
    if (projectId && !this.ensureLocalProjectPathAvailable(projectId)) {
      return;
    }
    /*
    CDXC:GPUISidebarGxserverRuntime 2026-07-07:
    gxserver defaults an omitted lifecycleState to "unknown", which the
    presentation layer treats as inactive, so the created terminal never gets a
    sidebar row even though the workspace pane opens. Declare the session
    running at create time like the remote path and the macOS client do.
    */
    let response: GpuiGxserverCreatedSessionResult;
    try {
      response = await this.client.rpc<GpuiGxserverCreatedSessionResult>("/api/createSession", {
        ...(projectId ? { projectId } : {}),
        kind: "terminal",
        lifecycleState: "running",
        surface: "workspace",
        title: DEFAULT_TERMINAL_SESSION_TITLE,
      });
    } catch (error) {
      if (
        projectId &&
        error instanceof GpuiGxserverRpcError &&
        error.code === "projectPathUnavailable" &&
        this.presentMissingProjectFolder(projectId)
      ) {
        void this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => undefined);
        return;
      }
      throw error;
    }
    const createdProjectId = normalizeNonEmptyString(response.session?.projectId) ?? projectId;
    const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
    if (subgroup && createdProjectId === subgroup.projectId && createdSessionId) {
      this.workspaceGroups = moveGpuiWorkspaceSessionToSubgroup(
        this.workspaceGroups,
        subgroup.projectId,
        createdSessionId,
        subgroup.groupId,
      );
      this.persistWorkspaceGroups();
    }
    if (createdProjectId && createdSessionId) {
      this.focusLocalWorkspaceSession(createdProjectId, createdSessionId);
    }
  },

  async createProjectTerminal(this: GpuiSidebarRuntime,
    message: Extract<SidebarToExtensionMessage, { type: "createProjectTerminal" }>,
  ): Promise<void> {
    /*
    CDXC:GPUIWindowsProjectTerminal 2026-07-26:
    The project-heading terminal button is an explicit project-scoped create
    request. On Windows, keep the WSL gxserver create and attach sequence in
    the Rust host by posting only the clicked local project id. The native host
    then reuses the same atomic path as GPUI New Terminal. Remote project
    headings also stay host-owned: posting the bounded project reference lets
    Rust use one create/start/attach operation instead of making CEF create a
    row and then serially wake it before the native tab can appear. Local
    macOS/Linux projects and generic subgroup creation keep their existing
    flows.
    */
    const groupId = message.groupId;
    const remoteGroup = parseGpuiRemotePresentationGroupId(groupId);
    if (remoteGroup) {
      if (!this.postRemoteProjectNativeAction("openRemoteProjectTerminal", remoteGroup, message)) {
        this.postRemoteToast("warning", "Remote session failed", {
          description: "Ghostex could not create that remote terminal.",
        });
      }
      return;
    }
    const projectId = parseGxserverPresentationProjectGroupId(groupId);
    const isWindowsHost =
      typeof navigator !== "undefined" && /Windows/iu.test(navigator.userAgent);
    if (!isWindowsHost) {
      await this.createSession(groupId);
      return;
    }
    const postCreate = window.ghostexGpui?.postCreateProjectTerminal;
    if (!projectId || typeof postCreate !== "function") {
      this.postSidebarActionToast("warning", "Terminal unavailable");
      return;
    }
    try {
      const accepted = postCreate(
        JSON.stringify({
          projectId,
          type: "ghostex.gpui.sidebar.createProjectTerminal",
          version: 1,
        }),
      );
      if (!accepted) {
        this.postSidebarActionToast("warning", "Terminal unavailable");
      }
    } catch {
      this.postSidebarActionToast("warning", "Terminal unavailable");
    }
  },

  async startAgentSessionProviderAndSendPrompt(this: GpuiSidebarRuntime,
    startProvider: () => Promise<unknown>,
    sendPrompt: (promptText: string) => Promise<unknown>,
    prompt?: string,
    renameCommand?: string,
  ): Promise<void> {
    await startProvider();
    const promptText = normalizeNonEmptyString(prompt);
    const renameText = normalizeNonEmptyString(renameCommand);
    if (!promptText && !renameText) {
      return;
    }
    await delayGpuiAgentPromptStep(GPUI_AGENT_PROMPT_READY_DELAY_MS);
    if (renameText) {
      await sendPrompt(renameText);
      await delayGpuiAgentPromptStep(GPUI_AGENT_PROMPT_STEP_DELAY_MS);
    }
    if (promptText) {
      await sendPrompt(promptText);
    }
  },

  async startRemoteAgentSessionAndSendPrompt(this: GpuiSidebarRuntime,
    machineId: string,
    projectId: string,
    sessionId: string,
    prompt?: string,
  ): Promise<void> {
    await this.startAgentSessionProviderAndSendPrompt(
      () =>
        this.requestRemoteGxserver(
          machineId,
          "/api/startSessionProvider",
          {
            projectId,
            sessionId,
          },
          { timeoutMs: 15_000 },
        ),
      (promptText) =>
        this.requestRemoteGxserver(
          machineId,
          "/api/sendSessionMessage",
          {
            projectId,
            sessionId,
            submit: true,
            text: promptText,
          },
          { timeoutMs: 15_000 },
        ),
      prompt,
    );
  },

  async startLocalAgentSessionAndSendPrompt(this: GpuiSidebarRuntime,
    projectId: string,
    sessionId: string,
    prompt?: string,
    renameCommand?: string,
  ): Promise<void> {
    const client = this.client;
    if (!client) {
      throw new Error("gxserver is unavailable.");
    }
    await this.startAgentSessionProviderAndSendPrompt(
      () =>
        client.rpc("/api/startSessionProvider", {
          projectId,
          sessionId,
        }),
      (promptText) =>
        client.rpc("/api/sendSessionMessage", {
          projectId,
          sessionId,
          submit: true,
          text: promptText,
        }),
      prompt,
      renameCommand,
    );
  },

  async createAgentSession(this: GpuiSidebarRuntime, agentId: string, groupId = this.activeGroupId): Promise<void> {
    const remoteGroup = groupId ? parseGpuiRemotePresentationGroupId(groupId) : undefined;
    if (remoteGroup) {
      const normalizedAgentId = agentId.trim();
      if (!normalizedAgentId) {
        this.postRemoteToast("warning", "Remote agent unavailable", {
          description: "Choose a configured agent for this remote project.",
        });
        return;
      }
      /*
      CDXC:GPUIRemoteSessions 2026-06-24-17:19:
      Remote agent launches must let the owning remote gxserver resolve default and project-custom agent commands from remote project metadata. GPUI sends only the selected agent id, project id, surface, and a require-command guard through Rust's authenticated tunnel, never a renderer-provided command string.
      */
      const remoteAgent = this.resolveSidebarAgent(normalizedAgentId);
      const title = createAgentSessionDefaultTitle(remoteAgent?.name ?? normalizedAgentId);
      const response = await this.requestRemoteGxserver<GpuiGxserverCreatedSessionResult>(
        remoteGroup.machineId,
        "/api/createAgentSession",
        {
          agentId: normalizedAgentId,
          projectId: remoteGroup.projectId,
          requireLaunchCommand: true,
          runtimeSettings: this.createFirstPromptTitleRuntimeSettings(),
          surface: "workspace",
          title,
        },
      ).catch(() => {
        this.postRemoteToast("warning", "Remote agent failed", {
          description: "The remote gxserver could not create that agent session.",
        });
        return undefined;
      });
      if (response) {
        const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
        if (createdSessionId) {
          const createdProjectId =
            normalizeNonEmptyString(response.session?.projectId) ?? remoteGroup.projectId;
          await this.startRemoteAgentSessionAndSendPrompt(
            remoteGroup.machineId,
            createdProjectId,
            createdSessionId,
          ).catch(() => {
            this.postRemoteToast("warning", "Remote agent failed", {
              description: "The remote gxserver could not start that agent session.",
            });
          });
          this.setRemotePresentationSessionFocus({
            machineId: remoteGroup.machineId,
            projectId: createdProjectId,
            sessionId: createdSessionId,
          });
          if (
            createGpuiSidebarSettings(this.runtimeSettings).preferredAgentInterface === "chat"
          ) {
            this.postRemoteSessionNativeAction(
              "openRemoteSessionTerminal",
              {
                machineId: remoteGroup.machineId,
                projectId: createdProjectId,
                sessionId: createdSessionId,
              },
              { agentId, groupId, type: "runSidebarAgent" },
              { preferredInterface: "chat" },
            );
          }
        }
        this.refreshRemotePresentationFromGxserver(remoteGroup.machineId).catch(() => undefined);
      }
      return;
    }
    const projectId = groupId
      ? parseGxserverPresentationProjectGroupId(groupId)
      : this.activeProjectId;
    if (projectId && !this.ensureLocalProjectPathAvailable(projectId)) {
      return;
    }
    const isWindowsHost =
      typeof navigator !== "undefined" && /Windows/iu.test(navigator.userAgent);
    if (isWindowsHost) {
      /*
      CDXC:GPUIWindowsProjectAgent 2026-08-11:
      Windows agent creation and attachment must stay in the Rust-owned WSL
      gxserver path. Splitting creation across CEF fetch and native attach can
      address different backend state during WSL bootstrap and leaves the
      project-header click with no materialized terminal. Send only the
      bounded project and agent ids plus the user's interface preference;
      Rust resolves the configured command, starts the provider, obtains its
      attach plan, and opens the exact tab.
      */
      const postCreate = window.ghostexGpui?.postCreateProjectAgent;
      const normalizedAgentId = agentId.trim();
      if (!projectId || !normalizedAgentId || typeof postCreate !== "function") {
        this.postSidebarActionToast("warning", "Agent unavailable");
        return;
      }
      try {
        const accepted = postCreate(
          JSON.stringify({
            agentId: normalizedAgentId,
            preferredInterface: createGpuiSidebarSettings(this.runtimeSettings)
              .preferredAgentInterface,
            projectId,
            type: "ghostex.gpui.sidebar.createProjectAgent",
            version: 1,
          }),
        );
        if (!accepted) {
          this.postSidebarActionToast("warning", "Agent unavailable");
        }
      } catch {
        this.postSidebarActionToast("warning", "Agent unavailable");
      }
      return;
    }
    const agent = this.resolveSidebarAgent(agentId);
    if (!this.client || !projectId || !agent) {
      return;
    }
    if (!agent.command) {
      return;
    }
    let response: GpuiGxserverCreatedSessionResult;
    try {
      response = await this.client.rpc<GpuiGxserverCreatedSessionResult>(
        "/api/createAgentSession",
        {
          agentId: agent.agentId,
          launchSettings: {
            agentCommand: agent.command,
            icon: agent.icon,
          },
          projectId,
          runtimeSettings: this.createFirstPromptTitleRuntimeSettings(),
          surface: "workspace",
          title: createAgentSessionDefaultTitle(agent.name),
        },
      );
    } catch (error) {
      if (
        error instanceof GpuiGxserverRpcError &&
        error.code === "projectPathUnavailable" &&
        this.presentMissingProjectFolder(projectId)
      ) {
        void this.refreshDomainPresentationSnapshotFromClient("patch").catch(() => undefined);
        return;
      }
      throw error;
    }
    const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
    if (createdSessionId) {
      const preferredAgentInterface = createGpuiSidebarSettings(
        this.runtimeSettings,
      ).preferredAgentInterface;
      this.focusLocalWorkspaceSession(
        normalizeNonEmptyString(response.session?.projectId) ?? projectId,
        createdSessionId,
        preferredAgentInterface === "chat" ? { preferredInterface: "chat" } : undefined,
      );
    }
  },

  /*
  CDXC:AgentHistorySearch 2026-08-20:
  Search by Text used to create a terminal and type `gx f` into it. The same
  search is now a first-class modal, so this forwards the native Find action
  and both entry points — the Previous Sessions search row and the command
  palette — land on one implementation instead of two.
  */
  searchPreviousSessionsByText(this: GpuiSidebarRuntime): void {
    this.postGhostexHotkeyAction({
      actionId: "openFindPrompts",
      type: "runGhostexHotkeyAction",
    });
  },

  /*
  GPUI port of the macOS OS-integration sidebar router (`handleNativeCliCommand`
  "createQuickTerminal" / "openPaths" in native-sidebar.tsx). Rust owns URL and
  file parsing, the script Run/Edit/Cancel consent dialog, existence checks,
  and git-root resolution; this handler only registers daemon projects and
  creates/focuses sessions through existing reviewed paths. Payloads are
  first-party fixed shapes from the Rust bridge (bounded action enum plus
  path/command/title strings); unknown actions surface an honest toast instead
  of dropping silently.

  CDXC:GxserverRuntimeSplit 2026-08-22:
  This method was deleted by accident on 2026-08-20 (the Search-by-Text change
  overwrote its body with `searchPreviousSessionsByText`) while both bridge call
  sites in `core.ts` stayed, so every `ghostex://` URL, Finder Open-With, and
  Find "launch session" threw `not a function` in the sidebar. Restored verbatim
  as a module method; the desktop typecheck gate is what surfaced it.
  */
  async handleGpuiOsIntegrationCommand(this: GpuiSidebarRuntime, payload: unknown): Promise<void> {
    const record =
      payload && typeof payload === "object" ? (payload as Record<string, unknown>) : undefined;
    const action = normalizeNonEmptyString(record?.action);
    if (!record || !action) {
      return;
    }
    if (action === "createQuickTerminal") {
      await this.createOsIntegrationTerminal({
        command: normalizeNonEmptyString(record.command),
        cwd: normalizeNonEmptyString(record.cwd),
        title: normalizeNonEmptyString(record.title),
      });
      return;
    }
    if (action === "openProjectPaths") {
      await this.openOsIntegrationProjectPaths(
        Array.isArray(record.projects) ? record.projects : [],
      );
      return;
    }
    this.postSidebarActionToast("warning", "Unsupported OS integration action.");
  },

  /*
  `ghostex://terminal` parity note: macOS creates a client-side projectless
  Quick project per invocation; GPUI's sidebar is daemon-derived, so the
  terminal lands in the daemon project registered (or reused) at the resolved
  cwd. A provided command launches the session with it (the Search-by-Text
  `gx f` launcher contract) instead of macOS's typed `command\r` into a shell.
  */
  async createOsIntegrationTerminal(this: GpuiSidebarRuntime, input: {
    command?: string;
    cwd?: string;
    title?: string;
  }): Promise<void> {
    if (!this.client || !input.cwd) {
      this.postSidebarActionToast("warning", "Open Terminal failed", {
        description: "ghostex://terminal needs the local gxserver.",
      });
      return;
    }
    try {
      const project = await this.registerProjectPath({
        name: gpuiProjectNameFromPath(input.cwd),
        path: input.cwd,
      });
      this.focusProjectId(project.projectId);
      this.publishPresentation("patch");
      const title =
        input.title ??
        normalizeNonEmptyString(gpuiProjectNameFromPath(input.cwd)) ??
        DEFAULT_TERMINAL_SESSION_TITLE;
      const response = input.command
        ? await this.client.rpc<GpuiGxserverCreatedSessionResult>("/api/createAgentSession", {
            agentId: "os-integration-terminal",
            launchSettings: {
              agentCommand: input.command,
            },
            projectId: project.projectId,
            surface: "workspace",
            title,
          })
        : await this.client.rpc<GpuiGxserverCreatedSessionResult>("/api/createSession", {
            kind: "terminal",
            lifecycleState: "running",
            projectId: project.projectId,
            surface: "workspace",
            title,
          });
      const createdSessionId = normalizeNonEmptyString(response.session?.sessionId);
      if (createdSessionId) {
        this.focusLocalWorkspaceSession(
          normalizeNonEmptyString(response.session?.projectId) ?? project.projectId,
          createdSessionId,
        );
      }
    } catch {
      this.postSidebarActionToast("error", "Open Terminal failed", {
        description: "gxserver could not create the requested terminal.",
      });
    }
  },

  async openOsIntegrationProjectPaths(this: GpuiSidebarRuntime, entries: unknown[]): Promise<void> {
    if (!this.client) {
      this.postSidebarActionToast("warning", "Open failed", {
        description: "Opening paths needs the local gxserver.",
      });
      return;
    }
    let focusProjectId: string | undefined;
    let failedCount = 0;
    for (const entry of entries.slice(0, 16)) {
      const record =
        entry && typeof entry === "object" ? (entry as Record<string, unknown>) : undefined;
      const path = normalizeNonEmptyString(record?.path);
      if (!path) {
        continue;
      }
      try {
        const project = await this.registerProjectPath({
          name: gpuiProjectNameFromPath(path),
          path,
        });
        focusProjectId = project.projectId;
      } catch {
        failedCount += 1;
      }
    }
    if (failedCount > 0) {
      this.postSidebarActionToast("error", "Open failed", {
        description: "gxserver could not open a requested folder as a project.",
      });
    }
    if (focusProjectId) {
      this.focusProjectId(focusProjectId);
      this.publishPresentation("patch");
    }
  },

  async createAgentSessionForProject(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    agent: SidebarAgentButton,
    prompt: string,
    title = createAgentSessionDefaultTitle(agent.name),
  ): Promise<string> {
    const defaultTitle = createAgentSessionDefaultTitle(agent.name);
    const renameTitle = title.trim() !== defaultTitle ? title.trim() : undefined;
    /*
    CDXC:GPUIGitAgentWorkflows 2026-07-11-06:14:
    Match macOS `runSidebarGitPromptAction` + `stageNativeAgentPrompt`: create
    Git helpers as fresh neutral agent sessions, start the provider, then submit
    the provider-specific title command, wait for that command to settle, and
    only then submit the workflow prompt. Persisting `Git: Release` or
    `Git: Multiple Commits` before startup makes the missing-provider attach
    path treat a brand-new row as a trusted resume title; a failed lookup then
    leaves the workflow prompt in a plain shell.
    */
    const created = await this.createAgentSessionRecordForProject(project, agent, prompt, {
      renameTitleAfterStart: renameTitle,
      title: defaultTitle,
    });
    return created.sessionId;
  },

  async createAgentSessionRecordForProject(this: GpuiSidebarRuntime,
    project: GxserverProjectDomainState,
    agent: SidebarAgentButton,
    prompt: string,
    options: {
      errorMessage?: string;
      firstUserInputDraft?: string;
      renameTitleAfterStart?: string;
      title?: string;
    } = {},
  ): Promise<GpuiCreatedProjectAgentSessionRecord> {
    if (!this.client) {
      throw new Error("gxserver is unavailable.");
    }
    const response = await this.client.rpc<{
      session?: {
        agentSessionId?: string;
        agentSessionPath?: string;
        runtimeSettings?: { agentSessionId?: string; agentSessionPath?: string };
        sessionId?: string;
        zmxName?: string;
      };
    }>("/api/createAgentSession", {
      agentId: agent.agentId,
      launchSettings: {
        agentCommand: agent.command,
        icon: agent.icon,
      },
      projectId: project.projectId,
      runtimeSettings: this.createFirstPromptTitleRuntimeSettings(
        options.renameTitleAfterStart ? undefined : prompt,
        options.firstUserInputDraft,
      ),
      surface: "workspace",
      title: options.title ?? createAgentSessionDefaultTitle(agent.name),
    });
    const session = response.session;
    const sessionId = normalizeNonEmptyString(session?.sessionId);
    if (!sessionId) {
      throw new Error(options.errorMessage ?? "Could not create an agent session in the worktree.");
    }
    this.focusLocalWorkspaceSession(project.projectId, sessionId);
    const renameTitle = normalizeNonEmptyString(options.renameTitleAfterStart);
    if (normalizeNonEmptyString(prompt) || renameTitle) {
      const renameCommand = renameTitle
        ? `/${gpuiWorkspaceTerminalTitleCommandForAgent(agent.agentId)} ${renameTitle}`
        : undefined;
      await this.startLocalAgentSessionAndSendPrompt(
        project.projectId,
        sessionId,
        prompt,
        renameCommand,
      );
    }
    return {
      agentSessionId:
        normalizeNonEmptyString(session?.agentSessionId) ??
        normalizeNonEmptyString(session?.runtimeSettings?.agentSessionId),
      agentSessionPath:
        normalizeNonEmptyString(session?.agentSessionPath) ??
        normalizeNonEmptyString(session?.runtimeSettings?.agentSessionPath),
      projectId: project.projectId,
      sessionId,
      zmxName: normalizeNonEmptyString(session?.zmxName),
    };
  },

  async createRemoteAgentSessionForProject(this: GpuiSidebarRuntime,
    remoteScope: GpuiRemoteProjectReference,
    agentId: string,
    prompt: string,
    title: string,
    options: { firstUserInputDraft?: string } = {},
  ): Promise<void> {
    const response = await this.requestRemoteGxserver<GpuiGxserverCreatedSessionResult>(
      remoteScope.machineId,
      "/api/createAgentSession",
      {
        agentId,
        projectId: remoteScope.projectId,
        requireLaunchCommand: true,
        runtimeSettings: this.createFirstPromptTitleRuntimeSettings(
          prompt,
          options.firstUserInputDraft,
        ),
        surface: "workspace",
        title,
      },
      { timeoutMs: 20_000 },
    );
    const sessionId = normalizeNonEmptyString(response.session?.sessionId);
    if (sessionId) {
      const projectId =
        normalizeNonEmptyString(response.session?.projectId) ?? remoteScope.projectId;
      await this.startRemoteAgentSessionAndSendPrompt(
        remoteScope.machineId,
        projectId,
        sessionId,
        prompt,
      ).catch(() => {
        this.postRemoteToast("warning", "Remote agent prompt failed", {
          description:
            "The remote gxserver could not start that agent session or deliver its prompt.",
        });
      });
      this.setRemotePresentationSessionFocus({
        machineId: remoteScope.machineId,
        projectId,
        sessionId,
      });
    }
    await this.refreshRemotePresentationFromGxserver(remoteScope.machineId).catch(() => undefined);
  },
};

const gpuiSidebarRuntimeSessionCreateMethodsShapeCheck: GpuiSidebarRuntimeSessionCreateMethods = gpuiSidebarRuntimeSessionCreateMethods;
void gpuiSidebarRuntimeSessionCreateMethodsShapeCheck;
