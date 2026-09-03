import { createRoot } from 'react-dom/client';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Toaster, toast } from 'sonner';
import { AddProjectModal } from '@/packages/core-ui/add-project-modal/add-project-modal';
import type {
  AddProjectAddResult,
  AddProjectBrowseResult,
  AddProjectCloneJob,
  AddProjectCloneJobHandle,
  AddProjectClonePreview,
  AddProjectCreateDirectoryResult,
  AddProjectMachineOption,
  AddProjectRepositoryInfo,
  AddProjectSourceControlDiscovery,
} from '@/packages/core-ui/add-project-modal/types';
import { AgentConfigModal, type AgentConfigDraft } from '@/packages/core-ui/agent-config-modal';
import { AgentHooksRequiredModal } from '@/packages/core-ui/agent-hooks-required-modal';
import { AgentsHubModal } from '@/packages/core-ui/agents-hub-modal';
import { CommandPalette } from '@/packages/core-ui/command-palette';
import { DelayedSendModal } from '@/packages/core-ui/delayed-send-modal';
import { DiscoverGhostexModal } from '@/packages/core-ui/discover-ghostex-modal';
import { FirstUserMessageModal } from '@/packages/core-ui/first-user-message-modal';
import { StashedPromptsModal, type StashedPromptsScope } from '@/packages/core-ui/stashed-prompts-modal';
import { PortlessSetupModal, type PortlessSetupModalMode } from '@/packages/core-ui/portless-setup-modal';
import { PreviousSessionsModal } from '@/packages/core-ui/previous-sessions-modal';
import { RecentProjectsModal } from '@/packages/core-ui/recent-projects-modal';
import { RemoteGxserverInstallModal } from '@/packages/core-ui/remote-gxserver-install-modal';
import { RemoteSetupModal, gpuiBootstrapRemoteSetupRpc } from '@/packages/core-ui/remote-setup-modal';
import { RemoteProjectPickerModal } from '@/packages/core-ui/remote-project-picker/remote-project-picker-modal';
import type { RemoteFilesystemBrowseResult } from '@/packages/core-ui/remote-project-picker/remote-filesystem';
import {
  SettingsModal,
  gpuiBootstrapTailcatRpc,
  type MainSettingsInitialSectionId,
  type SettingsModalTab,
} from '@/packages/core-ui/settings-modal';
import {
  ExportTranscriptModal,
  type ExportTranscriptModalStage,
} from '@/packages/core-ui/export-transcript-result-modal';
import { SessionNoteModal } from '@/packages/core-ui/session-note-modal';
import { SessionRenameModal } from '@/packages/core-ui/session-rename-modal';
import { SpaceEditorModal } from '@/packages/core-ui/space-editor-modal';
import { WatchGhostexVideoModal } from '@/packages/core-ui/watch-ghostex-video-modal';
import { UpdateAvailableModal, type UpdateAvailableModalState } from '@/packages/core-ui/update-available-modal';
import { FirstLaunchSetupModal } from '@/packages/core-ui/first-launch-setup-modal';
import { GitFileDiffModal, type GitFileDiffModalDraft } from '@/packages/core-ui/git-file-diff-modal';
import { GitCommitModal, type GitCommitModalDraft } from '@/packages/core-ui/git-commit-modal';
import { WorktreeDeleteModal, type WorktreeDeleteModalDraft } from '@/packages/core-ui/worktree-delete-modal';
import { WorktreeRenameModal, type WorktreeRenameModalDraft } from '@/packages/core-ui/worktree-rename-modal';
import { WorktreeCreateModal } from '@/packages/core-ui/worktree-create-modal';
import { normalizeAppToastDescription, type AppToastRequest } from '@/packages/shared/app-toast-contract';
import type { BundledGhostexAgentSkillId } from '@/packages/shared/ghostex-agent-skills';
import {
  sidebarAgentIconSupportsSessionHistoryTitleGeneration,
  type SidebarAgentButton,
  type SidebarAgentIcon,
} from '@/packages/shared/sidebar-agents';
import type {
  ExtensionToSidebarMessage,
  SidebarAddProjectDialogOperation,
  SidebarAddProjectDialogRequestParams,
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
  SidebarGhostexFolderStatsMessage,
  SidebarPluginSettingsStatusMessage,
  SidebarOSIntegrationStatusMessage,
  // CDXC:AppIconPicker 2026-06-25-21:50: App Icon state flows to Settings through the modal-state relay.
  SidebarAppIconStateMessage,
  SidebarToExtensionMessage,
} from '@/packages/shared/session-grid-contract';
import {
  getWorkspaceThemeForeground,
  normalizeWorkspaceThemeColor,
} from '@/packages/shared/workspace-project-appearance';
import { installAppModalGlobalErrorLogging, logAppModalError } from '@/packages/core-ui/app-modal-error-log';
import { postAppModalHostMessage, type SettingsRemoteSection } from '@/packages/core-ui/app-modal-host-bridge';
import { MissingProjectFolderModal } from '@/packages/core-ui/missing-project-folder-modal';
import { useSidebarStore } from '@/packages/core-ui/sidebar-store';
import {
  DEFAULT_ghostex_SETTINGS,
  isDiagnosticLoggingScenarioEnabled,
  SETTINGS_MODAL_NAVIGATION_TABS,
  type DiagnosticLoggingScenarioId,
} from '@/packages/shared/ghostex-settings';
import type { WebviewApi } from '@/packages/core-ui/webview-api';
import '@/packages/core-ui/styles.css';

type AppModalKind =
  | 'addProject'
  | 'agentConfig'
  | 'agentHooksRequired'
  | 'agentsHub'
  | 'commandPalette'
  | 'configureActions'
  | 'configureAgents'
  | 'delayedSend'
  | 'discoverGhostex'
  | 'exportTranscriptResult'
  | 'watchGhostexVideo'
  | 'hotkeys'
  | 'missingProjectFolder'
  | 'gitCommit'
  | 'gitFileDiff'
  | 'deleteWorktree'
  | 'renameWorktree'
  | 'openTargets'
  | 'portlessSetup'
  | 'previousSessions'
  | 'recentProjects'
  | 'firstUserMessage'
  | 'remoteGxserverInstall'
  | 'remoteProjectPicker'
  | 'remoteSetup'
  | 'renameSession'
  | 'sessionNote'
  | 'settings'
  | 'sidebarSpaceEditor'
  | 'stashedPrompts'
  | 'worktree'
  | 'tipsAndTricks'
  | 'updateAvailable'
  | 'firstLaunchSetup';

/*
 * CDXC:GPUIAppModalScroll 2026-07-26-07:55:
 * GPUI injects this host id into every app-modal child window it owns.
 *
 * CDXC:GPUIAppModalFitHeight 2026-07-28:
 * GPUI now consumes the same one-shot `contentHeightMeasured` message as macOS
 * and fits its child window to the measured dialog height once per open. The
 * frame stays fixed after that fit, so the fixed-window stylesheet caps below
 * still own post-open content growth.
 */
const GPUI_APP_MODAL_HOST_ID = 'gpui';

/*
 * CDXC:AppModals 2026-06-30-16:08:
 * Centered compact native child-window modals should size to their rendered
 * React dialog once, before native presents the panel. Keep Settings out of
 * this path because it remains a user-resizable fixed-size native window.
 */
const ONE_SHOT_NATIVE_FIT_HEIGHT_MODAL_SELECTORS: Partial<Record<AppModalKind, string>> = {
  agentConfig: '.agent-config-modal-shadcn',
  agentHooksRequired: '.agent-hooks-required-modal',
  delayedSend: '.delayed-send-modal-shadcn',
  deleteWorktree: '.worktree-delete-modal-shadcn',
  exportTranscriptResult: '.export-transcript-modal-shadcn',
  firstUserMessage: '.first-user-message-modal',
  missingProjectFolder: '.missing-project-folder-modal',
  portlessSetup: '.portless-setup-modal-shadcn',
  remoteGxserverInstall: '.remote-gxserver-install-modal',
  remoteProjectPicker: '.remote-project-picker-dialog',
  remoteSetup: '.remote-setup-modal',
  renameSession: '.session-rename-modal-shadcn',
  renameWorktree: '.worktree-rename-modal-shadcn',
  sessionNote: '.session-note-modal-shadcn',
  sidebarSpaceEditor: '.space-editor-modal-shadcn',
  worktree: '.worktree-create-modal-shadcn',
  updateAvailable: '.update-available-modal',
};

/*
 * CDXC:AppModals 2026-06-30-16:08:
 * Most measured dialogs are centered, so setting the native window to their
 * element height puts the React shell at y=0. Top-aligned modals keep an
 * intentional WebView inset, so include that inset in the one-shot height.
 */
const ONE_SHOT_NATIVE_FIT_HEIGHT_TOP_OFFSET_MODALS = new Set<AppModalKind>(['previousSessions', 'remoteProjectPicker']);

function oneShotNativeFitHeightSelector(modal: AppModalKind): string | undefined {
  return ONE_SHOT_NATIVE_FIT_HEIGHT_MODAL_SELECTORS[modal];
}

function shouldUseOneShotNativeFitHeight(modal: AppModalKind | null | undefined): modal is AppModalKind {
  return Boolean(modal && oneShotNativeFitHeightSelector(modal));
}

function measureOneShotNativeFitHeight(modal: AppModalKind): number | undefined {
  const selector = oneShotNativeFitHeightSelector(modal);
  if (!selector) {
    return undefined;
  }
  const element = document.querySelector(selector);
  if (!(element instanceof HTMLElement)) {
    return undefined;
  }
  const rect = element.getBoundingClientRect();
  const topOffset = ONE_SHOT_NATIVE_FIT_HEIGHT_TOP_OFFSET_MODALS.has(modal) ? Math.max(0, rect.top) : 0;
  const height = Math.ceil(Math.max(rect.height, element.offsetHeight) + topOffset);
  return Number.isFinite(height) && height > 0 ? height : undefined;
}

type AgentsHubCatalogMessage = Extract<ExtensionToSidebarMessage, { type: 'agentsHubCatalog' }>;
type AgentsHubFileContentMessage = Extract<ExtensionToSidebarMessage, { type: 'agentsHubFileContent' }>;
type AgentHookStatusMessage = Extract<ExtensionToSidebarMessage, { type: 'agentHookStatus' }>;
type GhostexCliStatusMessage = Extract<ExtensionToSidebarMessage, { type: 'ghostexCliStatus' }>;
type OSIntegrationStatusMessage = Extract<ExtensionToSidebarMessage, { type: 'osIntegrationStatus' }>;
type PluginSettingsStatusMessage = Extract<ExtensionToSidebarMessage, { type: 'pluginSettingsStatus' }>;
// CDXC:AppIconPicker 2026-06-25-21:50: App Icon state message threaded through modal state into Settings.
type AppIconStateMessage = Extract<ExtensionToSidebarMessage, { type: 'appIconState' }>;

type AppModalHostMessage =
  | {
      agentDraft?: AgentConfigDraft;
      agentIcon?: SidebarAgentIcon;
      agentId?: string;
      agentName?: string;
      /** CDXC:ExportTranscript 2026-08-20: see ExportTranscriptResultModalState. */
      canReveal?: boolean;
      path?: string;
      closeAfterDoneActive?: boolean;
      delayedSendDeadlineAt?: string;
      delayedSendRemainingLabel?: string;
      sendWhenAllProjectSessionsStopActive?: boolean;
      sendWhenAgentStopsActive?: boolean;
      supportsSendWhenAllProjectSessionsStop?: boolean;
      supportsSendWhenAgentStops?: boolean;
      initialTitle?: string;
      initialQuery?: string;
      /** CDXC:SessionAgentNotes 2026-08-24: see SessionNoteModalState. */
      initialNote?: string;
      sessionTitle?: string;
      message?: string;
      projectId?: string;
      projectName?: string;
      projectPath?: string;
      remoteMachineId?: string;
      remoteMachineName?: string;
      filePath?: string;
      gitCommitDraft?: GitCommitModalDraft;
      gitFileDiff?: GitFileDiffModalDraft;
      groupId?: string;
      hookAgentId?: string;
      worktreeDeleteDraft?: WorktreeDeleteModalDraft;
      worktreeRenameDraft?: WorktreeRenameModalDraft;
      initialRemoteMachineId?: string;
      initialRemoteSection?: SettingsRemoteSection;
      initialSection?: MainSettingsInitialSectionId;
      /** CDXC:StashedPromptSessionAssociation 2026-08-24: see StashedPromptsModalState. */
      initialScope?: StashedPromptsScope;
      initialSearchQuery?: string;
      initialTab?: SettingsModalTab;
      latestSidebarStateMessage?: unknown;
      machineId?: string;
      machineName?: string;
      /** Membership target for a Space created from a group/project menu. */
      memberCollectionId?: string;
      memberProjectId?: string;
      modal: AppModalKind;
      /**
       * CDXC:SidebarSpaces 2026-08-27:
       * `create`/`edit` belong to the Space editor; the two portless values are
       * that dialog's own modes. They share the field because the modal-open
       * message is one flat record keyed by `modal`.
       */
      mode?: PortlessSetupModalMode | 'create' | 'edit';
      prewarm?: boolean;
      protocol?: 'https' | 'http';
      requestId?: string;
      sessionAgentIcon?: string;
      sessionId?: string;
      /** CDXC:SidebarSpaces 2026-08-27: see SidebarSpaceEditorModalState. */
      spaceColor?: string;
      spaceIcon?: string;
      spaceId?: string;
      spaceName?: string;
      showFirstLaunchSetupOnClose?: boolean;
      threadId?: string;
      title?: string;
      notesMarkdown?: string;
      portable?: boolean;
      state?: 'available' | 'ready';
      version?: string;
      type: 'open';
    }
  | { type: 'close' }
  | { type: 'completeFirstLaunchSetup' }
  | AppToastRequest
  | { keepOpen?: boolean; type: 'toastDismissed' }
  | { initialPath?: string; type: 'pickRepositoryFolder' }
  | { path: string; type: 'repositoryFolderPicked' }
  | {
      error?: string;
      ok: boolean;
      projectPath?: string;
      requestId: string;
      type: 'repositoryCloneResult';
    }
  | {
      error?: string;
      ok: boolean;
      preview?: unknown;
      requestId: string;
      type: 'repositoryClonePreviewResult';
    }
  | {
      error?: string;
      ok: boolean;
      requestId: string;
      result?: RemoteFilesystemBrowseResult;
      type: 'remoteProjectDirectoryBrowseResult';
    }
  | {
      error?: string;
      ok: boolean;
      projectPath?: string;
      requestId: string;
      type: 'remoteProjectAddResult';
    }
  | {
      /*
       * CDXC:AddProject 2026-07-30:
       * One answer channel for every add-project dialog round trip. `result` is
       * the daemon's own result object (browse entries, project record, clone
       * job, discovery) forwarded unchanged, and `error` is the daemon's own
       * rejection text so the dialog can show why a path was refused instead of
       * a generic failure line.
       */
      error?: string;
      ok: boolean;
      requestId: string;
      result?: unknown;
      type: 'addProjectDialogResult';
    }
  | { type: 'pickWorktreeImages' }
  | { paths: string[]; type: 'worktreeImageFilesPicked' }
  | { path: string; type: 'terminalBackgroundImageFilePicked' }
  | { path: string; type: 'firstLaunchProjectFolderPicked' }
  | {
      error?: string;
      ok: boolean;
      requestId: string;
      type: 'firstLaunchCreateProjectSessionResult';
    }
  | {
      /*
       * CDXC:RemoteMachines 2026-08-19:
       * Native's answer to `probeRemoteGxserverInstall`: whether the saved
       * remote machine already has a gxserver package and, when native could
       * read it, that package's version. Settings uses it to label the action
       * Install or Update and to show the installed version beside it.
       */
      installed: boolean;
      remoteMachineId: string;
      type: 'remoteGxserverInstallState';
      version?: string;
    }
  | {
      branches?: unknown;
      error?: string;
      ok: boolean;
      requestId: string;
      type: 'projectWorktreesResult';
      worktrees?: unknown;
    }
  | {
      /*
       * CDXC:ExportTranscriptOptions 2026-08-24:
       * The sidebar runtime's answer to `runExportSessionTranscript`: the
       * export finished (path is on the machine that owns the transcript) or
       * failed with the daemon's structured message. Moves the open Export
       * Transcript dialog from its exporting stage to done/failed.
       */
      agentId?: string;
      canReveal?: boolean;
      error?: string;
      ok: boolean;
      path?: string;
      requestId: string;
      type: 'exportSessionTranscriptResult';
    }
  | { details?: string; event: string; type: 'debugLog' }
  | { modal: AppModalKind; requestId?: string; type: 'presented' }
  | { message: unknown; type: 'sidebarState' };

type RenameSessionModalState = {
  initialTitle: string;
  sessionAgentIcon?: string;
  sessionId: string;
};

/*
 * CDXC:SessionAgentNotes 2026-08-24:
 * The session-note editor's open payload. `initialNote` is the note the sidebar
 * row was already rendering, so the dialog opens filled in without a round
 * trip; `projectId` is an optional scope hint the runtime may use to route the
 * write, and `sessionTitle` is heading copy only.
 */
type SessionNoteModalState = {
  initialNote: string;
  projectId?: string;
  sessionId: string;
  sessionTitle?: string;
};

/*
 * CDXC:SidebarSpaces 2026-08-27:
 * The New/Edit Space dialog's open payload. `remoteMachineId` is the only
 * routing token that crosses this boundary — it names the gxserver that owns
 * the Space — and the optional member id carries the group/project that opened
 * a create dialog. The name/icon/color are the values the sidebar row was
 * already rendering, so the dialog opens on live values without a round trip.
 * No Space document crosses here in either direction: the sidebar owns it.
 */
type SidebarSpaceEditorModalState = {
  memberCollectionId?: string;
  memberProjectId?: string;
  mode: 'create' | 'edit';
  remoteMachineId?: string;
  spaceColor?: string;
  spaceIcon?: string;
  spaceId?: string;
  spaceName?: string;
};

type PromptAgentModalKey = 'gitCommit' | 'renameSession';

const PROMPT_AGENT_MODAL_STORAGE_KEYS: Record<PromptAgentModalKey, string> = {
  gitCommit: 'ghostex.promptAgent.gitCommit',
  renameSession: 'ghostex.promptAgent.renameSession',
};

type FirstUserMessageModalState = {
  message: string;
  title?: string;
};

type RemoteProjectPickerState = {
  initialQuery?: string;
  remoteMachineId: string;
  remoteMachineName: string;
};

/*
 * CDXC:AddProject 2026-07-30:
 * The add-project dialog resolves its own machine list through the host, so the
 * only thing an open message carries is which machine to preselect. A remote
 * machine header sends one; the projects header, the V2 create menu, and the
 * command palette send none and let the dialog decide.
 */
type AddProjectModalState = {
  machineId?: string;
};

type RecentProjectsModalState = {
  machineId?: string;
  machineName?: string;
};

/*
 * CDXC:StashedPrompts 2026-07-29:
 * The session Prompts modal carries the launching session's project scope and
 * the terminal session the selected prompt is inserted back into. Both are
 * optional so the modal can open in all-projects browse mode.
 *
 * CDXC:StashedPromptSessionAssociation 2026-08-24:
 * `initialScope` is the launcher's pinned origin filter. It stays optional so
 * an opener with no opinion lets the modal choose its own default.
 */
type StashedPromptsModalState = {
  initialScope?: StashedPromptsScope;
  projectId?: string;
  sessionId?: string;
};

/*
 * CDXC:ExportTranscript 2026-08-20 / CDXC:ExportTranscriptOptions 2026-08-24:
 * The Export Transcript dialog. It opens on its include-toggle options stage;
 * the export runs only when the user confirms it, and the sidebar runtime
 * answers with `exportSessionTranscriptResult`, which moves `stage` to
 * done/failed. `path` on the done stage is absolute on the machine that owns
 * the transcript, so `canReveal` is false for a remote session's export: the
 * host running this dialog has no such file.
 */
type ExportTranscriptResultModalState = {
  agentId?: string;
  canReveal: boolean;
  requestId?: string;
  stage: ExportTranscriptModalStage;
};

type RemoteGxserverInstallState = {
  remoteMachineId: string;
  remoteMachineName: string;
};

type DelayedSendModalState = {
  agentIcon?: SidebarAgentIcon;
  closeAfterDoneActive?: boolean;
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  sendWhenAllProjectSessionsStopActive?: boolean;
  sendWhenAgentStopsActive?: boolean;
  sessionId: string;
  supportsSendWhenAllProjectSessionsStop?: boolean;
  supportsSendWhenAgentStops?: boolean;
  title?: string;
};

type MissingProjectFolderModalState = {
  projectId: string;
  projectName: string;
  projectPath: string;
};

/**
 * CDXC:AppToasts 2026-06-03-16:12:
 * macOS and crossplatform app-modal toasts should sit 23px higher than
 * Sonner's 24px bottom default, so progress notices stay clear of lower app
 * chrome while preserving the bottom-center stack behavior.
 */
const APP_MODAL_TOAST_BOTTOM_OFFSET_PX = 47;
type WorktreeModalState = {
  projectId?: string;
  projectName?: string;
  projectPath?: string;
  remoteMachineId?: string;
  remoteMachineName?: string;
};

type PortlessSetupModalState = {
  mode: PortlessSetupModalMode;
  protocol: 'https' | 'http';
};

const APP_MODAL_CONTEXT_MENU_EDITABLE_SELECTOR = "input, textarea, select, [contenteditable='true'], [role='textbox']";

function isEditableAppModalContextMenuTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }

  return target.closest(APP_MODAL_CONTEXT_MENU_EDITABLE_SELECTOR) !== null;
}

type ConfigModalState = {
  agentDraft?: AgentConfigDraft;
};

type AgentHooksRequiredModalState = {
  agentId: string;
  agentName: string;
  groupId?: string;
  hookAgentId: string;
};

declare global {
  interface Window {
    webkit?: {
      messageHandlers?: {
        ghostexAppModalHost?: {
          postMessage: (message: unknown) => void;
        };
        ghostexNativeHost?: {
          postMessage: (message: unknown) => void;
        };
        ghostexNativeHostDiagnostics?: {
          postMessage: (message: unknown) => void;
        };
      };
    };
    __ghostex_APP_MODAL_HOST_ID__?: string;
    __ghostex_APP_MODAL_HOST_SURFACE__?: 'main' | 'nativeWindow';
  }
}

const vscode: WebviewApi = {
  postMessage(message) {
    if (isAppModalDebugLoggingEnabled()) {
      console.debug('[ghostex-app-modal-host] sidebarCommand', redactAppModalDebugMessage(message));
    }
    /**
     * CDXC:AppModals 2026-06-13-01:09:
     * Previous Sessions no longer sends agent-prompt search commands, but modal
     * commands still cross this full-window host before native dispatch. Keep a
     * single debug boundary for restore, delete, and direct text-search commands.
     */
    postAppModalHostMessage({ message, type: 'sidebarCommand' }, 'AppModals:sidebarCommand');
  },
};

function redactAppModalDebugMessage(message: unknown): unknown {
  if (
    typeof message === 'object' &&
    message !== null &&
    !Array.isArray(message) &&
    (message as { type?: unknown }).type === 'saveRemoteMachinePassword'
  ) {
    /*
     * CDXC:RemoteMachines 2026-06-09-18:23:
     * SSH password saves are intentionally one-shot Keychain writes. Modal
     * debug logging must redact the transient password before it reaches the
     * console so diagnostics cannot capture user credentials.
     */
    return {
      ...(message as Record<string, unknown>),
      password: '[redacted]',
    };
  }
  return message;
}

function isDiagnosticLoggingEnabledForScenario(scenarioId: DiagnosticLoggingScenarioId): boolean {
  const settings = useSidebarStore.getState().hud.settings ?? DEFAULT_ghostex_SETTINGS;
  return isDiagnosticLoggingScenarioEnabled(settings.diagnosticLogging, scenarioId);
}

function isAppModalDebugLoggingEnabled(): boolean {
  return isDiagnosticLoggingEnabledForScenario('native.app.modal');
}

function isRemoteGxserverInstallDebugLoggingEnabled(): boolean {
  return isDiagnosticLoggingEnabledForScenario('native.remote.gxserver.install');
}

type AppModalDebugDetails = Record<string, string | number | boolean | null | undefined>;

function postAppModalDebugLog(event: string, details: AppModalDebugDetails) {
  if (!isAppModalDebugLoggingEnabled()) {
    return;
  }
  /*
   * CDXC:SettingsModalDiagnostics 2026-06-20-05:38:
   * Settings and setup modal diagnostics must stay limited to lifecycle
   * booleans, revisions, timings, modal ids, and safe enum-like metadata.
   */
  postAppModalHostMessage(
    {
      details: JSON.stringify({
        performanceNow: Math.round(performance.now()),
        ...details,
      }),
      event,
      type: 'debugLog',
    },
    'AppModals:debug'
  );
}

function postSettingsModalDebugLog(event: string, details: AppModalDebugDetails) {
  postAppModalDebugLog(event, details);
}

function postRemoteGxserverInstallDebugLog(event: string, details: AppModalDebugDetails) {
  if (!isRemoteGxserverInstallDebugLoggingEnabled()) {
    return;
  }
  /*
   * CDXC:RemoteMachines 2026-06-30-03:05:
   * Persist remote install modal-host breadcrumbs under the dedicated scenario
   * without machine names, hosts, paths, URLs, command text, passwords, tokens,
   * or raw errors.
   */
  postAppModalHostMessage(
    {
      details: JSON.stringify({
        performanceNow: Math.round(performance.now()),
        ...details,
      }),
      event,
      type: 'remoteGxserverInstallDebugLog',
    },
    'RemoteGxserverInstall:debug'
  );
}

function notifyNativeModalClosed() {
  postAppModalHostMessage({ type: 'close' }, 'AppModals:close');
}

function notifyNativeFirstLaunchSetupCompleted() {
  postAppModalHostMessage({ type: 'completeFirstLaunchSetup' }, 'FirstLaunchSetup:complete');
}

function isSettingsModalKind(modal: AppModalKind | undefined): boolean {
  return (
    modal === 'settings' ||
    modal === 'configureAgents' ||
    modal === 'configureActions' ||
    modal === 'openTargets' ||
    modal === 'hotkeys'
  );
}

function isFirstLaunchSetupModalKind(modal: AppModalKind | undefined): boolean {
  return modal === 'firstLaunchSetup' || modal === 'tipsAndTricks';
}

function shouldApplySidebarStateBeforeModalOpen(modal: AppModalKind | undefined): boolean {
  /*
   * CDXC:FirstLaunchSetup 2026-06-29-13:46:
   * First-launch setup reads the same hydrated Settings store as the Settings
   * modal. Apply the native sidebar snapshot before setting activeModal so the
   * child-window setup flow cannot stay blank behind its native backdrop while
   * React waits at revision 0.
   */
  return isSettingsModalKind(modal) || isFirstLaunchSetupModalKind(modal);
}

function getSettingsInitialTab(modal: AppModalKind | undefined): SettingsModalTab {
  /**
   * CDXC:UnifiedSettings 2026-05-09-15:30
   * Existing entry points still request their historic modal kind, but the
   * app-modal host now routes Settings, Agents, Actions, and Hotkeys into one
   * tabbed Settings dialog so users have a single configuration surface.
   */
  if (modal === 'configureAgents') {
    return 'agents';
  }
  if (modal === 'configureActions') {
    return 'actions';
  }
  if (modal === 'hotkeys') {
    return 'hotkeys';
  }
  if (modal === 'openTargets') {
    return 'openTargets';
  }
  return 'settings';
}

/**
 * CDXC:SettingsNavigation 2026-08-19-00:00:
 * Settings deep links carry a tab id over the app-modal host message. Validate
 * it against the single canonical tab list instead of a hand-maintained copy,
 * which silently dropped newer pages such as Extensions (`extensions`) and made
 * their entry points open the remembered tab instead.
 */
const SETTINGS_MODAL_TAB_SET = new Set<string>(SETTINGS_MODAL_NAVIGATION_TABS);

function isSettingsModalTab(value: unknown): value is SettingsModalTab {
  return typeof value === 'string' && SETTINGS_MODAL_TAB_SET.has(value);
}

function readPromptAgentModalOverride(modal: PromptAgentModalKey): string | undefined {
  const value = localStorage.getItem(PROMPT_AGENT_MODAL_STORAGE_KEYS[modal])?.trim();
  return value || undefined;
}

function writePromptAgentModalOverride(modal: PromptAgentModalKey, agentId: string): void {
  const normalizedAgentId = agentId.trim();
  if (!normalizedAgentId) {
    localStorage.removeItem(PROMPT_AGENT_MODAL_STORAGE_KEYS[modal]);
    return;
  }
  localStorage.setItem(PROMPT_AGENT_MODAL_STORAGE_KEYS[modal], normalizedAgentId);
}

function clearPromptAgentModalOverrides(): void {
  for (const key of Object.values(PROMPT_AGENT_MODAL_STORAGE_KEYS)) {
    localStorage.removeItem(key);
  }
}

function resolvePromptAgentModalSelection(
  agents: readonly SidebarAgentButton[],
  savedAgentId: string | undefined,
  defaultAgentId: string | undefined
): string | undefined {
  const commandAgents = agents.filter((agent) => agent.command?.trim());
  return (
    commandAgents.find((agent) => agent.agentId === savedAgentId)?.agentId ??
    commandAgents.find((agent) => agent.agentId === defaultAgentId)?.agentId ??
    commandAgents[0]?.agentId
  );
}

function createRemoteProjectRequestId(kind: 'add' | 'browse'): string {
  return `remote-project-${kind}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

const FIRST_LAUNCH_SKILL_INSTALL_TIMEOUT_MS = 150_000;

type FirstLaunchSkillInstallAction =
  | 'installBrowserControl'
  | 'installBrowserUseSkill'
  | 'installComputerUseSkill'
  | 'installCliSkill'
  | 'installFable56OrchestrationSkill'
  | 'installGenerateTitleSkill'
  | 'installManageBeadsSkill'
  | 'installMoveCodexSessionSkill';

const FIRST_LAUNCH_SKILL_INSTALL_ACTION_BY_ID: Record<BundledGhostexAgentSkillId, FirstLaunchSkillInstallAction> = {
  browserUse: 'installBrowserUseSkill',
  cli: 'installCliSkill',
  computerUse: 'installComputerUseSkill',
  embeddedBrowserUse: 'installBrowserControl',
  fable56Orchestration: 'installFable56OrchestrationSkill',
  generateTitle: 'installGenerateTitleSkill',
  manageBeads: 'installManageBeadsSkill',
  moveCodexSession: 'installMoveCodexSessionSkill',
};

function requestAppModalSettingsAction(action: FirstLaunchSkillInstallAction): Promise<void> {
  return new Promise((resolve, reject) => {
    let timeoutId = 0;
    const handleMessage = (event: Event) => {
      const hostMessage = (event as CustomEvent<AppModalHostMessage>).detail;
      if (!hostMessage || typeof hostMessage !== 'object' || hostMessage.type !== 'sidebarState') {
        return;
      }
      const status = hostMessage.message;
      if (
        !status ||
        typeof status !== 'object' ||
        !('type' in status) ||
        status.type !== 'settingsActionStatus' ||
        !('action' in status) ||
        status.action !== action
      ) {
        return;
      }
      window.clearTimeout(timeoutId);
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      if ('available' in status && status.available === true) {
        resolve();
        return;
      }
      reject(
        new Error(
          'message' in status && typeof status.message === 'string'
            ? status.message
            : 'Ghostex could not install the selected skill.'
        )
      );
    };

    window.addEventListener('ghostex-app-modal-host-message', handleMessage);
    timeoutId = window.setTimeout(() => {
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      reject(new Error('Installing the selected skills timed out.'));
    }, FIRST_LAUNCH_SKILL_INSTALL_TIMEOUT_MS);
    try {
      vscode.postMessage({ type: action });
    } catch (error) {
      window.clearTimeout(timeoutId);
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      reject(error);
    }
  });
}

async function requestFirstLaunchInstallSelectedSkills(skillIds: readonly BundledGhostexAgentSkillId[]): Promise<void> {
  for (const skillId of skillIds) {
    await requestAppModalSettingsAction(FIRST_LAUNCH_SKILL_INSTALL_ACTION_BY_ID[skillId]);
  }
}

function startFirstLaunchCreateProjectSession(agentId: string, path: string): void {
  const requestId = `first-launch-project-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  vscode.postMessage({ agentId, path, requestId, type: 'firstLaunchCreateProjectSession' });
}

function waitForRemoteProjectDirectoryBrowseResult(requestId: string): Promise<RemoteFilesystemBrowseResult> {
  return new Promise((resolve, reject) => {
    let timeoutId = 0;
    const handleMessage = (event: Event) => {
      const message = (event as CustomEvent<AppModalHostMessage>).detail;
      if (
        !message ||
        typeof message !== 'object' ||
        message.type !== 'remoteProjectDirectoryBrowseResult' ||
        message.requestId !== requestId
      ) {
        return;
      }
      window.clearTimeout(timeoutId);
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      if (!message.ok || !isRemoteFilesystemBrowseResult(message.result)) {
        reject(new Error(message.error || 'Remote directory browse failed.'));
        return;
      }
      resolve(message.result);
    };

    window.addEventListener('ghostex-app-modal-host-message', handleMessage);
    timeoutId = window.setTimeout(() => {
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      reject(new Error('Remote directory browse timed out.'));
    }, 15_000);
  });
}

function waitForRemoteProjectAddResult(requestId: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let timeoutId = 0;
    const handleMessage = (event: Event) => {
      const message = (event as CustomEvent<AppModalHostMessage>).detail;
      if (
        !message ||
        typeof message !== 'object' ||
        message.type !== 'remoteProjectAddResult' ||
        message.requestId !== requestId
      ) {
        return;
      }
      window.clearTimeout(timeoutId);
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      if (!message.ok) {
        reject(new Error(message.error || 'Remote project add failed.'));
        return;
      }
      resolve();
    };

    window.addEventListener('ghostex-app-modal-host-message', handleMessage);
    /*
     * CDXC:AddProject 2026-07-30:
     * A remote add right after a reconnect has been measured at ~19s, so the
     * old 20s waiter routinely declared failure for adds that then landed on
     * the machine. This waiter now matches the host's own 60s add budget.
     */
    timeoutId = window.setTimeout(() => {
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      reject(new Error('Remote project add timed out.'));
    }, ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS);
  });
}

/*
 * CDXC:AddProject 2026-07-30:
 * The add-project dialog's callbacks are host round trips: mint a requestId,
 * post the bounded operation, and wait for the host's answer on the app-modal
 * message channel. The waiter budget MATCHES the host's own timeout for the
 * same operation (60s for an add or a clone start, per the reconnect-time add
 * that used to take ~19s against a 20s ceiling), so neither end can give up
 * while the other is still working. Dismissing the dialog unmounts it and
 * abandons the pending answer; nothing here is optimistic.
 */
const ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS = 60_000;
const ADD_PROJECT_DIALOG_BROWSE_TIMEOUT_MS = 15_000;
const ADD_PROJECT_DIALOG_DISCOVERY_TIMEOUT_MS = 30_000;
const ADD_PROJECT_DIALOG_LOOKUP_TIMEOUT_MS = 20_000;
const ADD_PROJECT_DIALOG_JOB_TIMEOUT_MS = 20_000;

function createAddProjectRequestId(operation: SidebarAddProjectDialogOperation): string {
  return `add-project-${operation}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function requestAddProjectDialogOperation(
  operation: SidebarAddProjectDialogOperation,
  timeoutMs: number,
  input: {
    machineId?: string;
    params?: SidebarAddProjectDialogRequestParams;
  } = {}
): Promise<unknown> {
  const requestId = createAddProjectRequestId(operation);
  const answer = new Promise<unknown>((resolve, reject) => {
    let timeoutId = 0;
    const handleMessage = (event: Event) => {
      const message = (event as CustomEvent<AppModalHostMessage>).detail;
      if (
        !message ||
        typeof message !== 'object' ||
        message.type !== 'addProjectDialogResult' ||
        message.requestId !== requestId
      ) {
        return;
      }
      window.clearTimeout(timeoutId);
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      if (!message.ok) {
        reject(new Error(message.error || 'The request failed.'));
        return;
      }
      resolve(message.result);
    };

    window.addEventListener('ghostex-app-modal-host-message', handleMessage);
    timeoutId = window.setTimeout(() => {
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
      reject(new Error('The machine did not answer in time.'));
    }, timeoutMs);
  });
  vscode.postMessage({
    ...(input.machineId ? { machineId: input.machineId } : {}),
    operation,
    ...(input.params ? { params: input.params } : {}),
    requestId,
    type: 'addProjectDialogRequest',
  });
  return answer;
}

/*
 * CDXC:AddProject 2026-07-30:
 * The host forwards gxserver result objects unchanged, so these readers are the
 * boundary that turns one into the dialog's prop shape. They THROW on anything
 * unexpected rather than substituting a default: the dialog renders a thrown
 * message in its persistent error region, which is the honest outcome for a
 * daemon answer this build does not understand.
 */
function readAddProjectResultObject(value: unknown, key: string): Record<string, unknown> {
  const container = value as Record<string, unknown> | null | undefined;
  const entry = container && typeof container === 'object' ? container[key] : undefined;
  if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
    throw new Error('The machine returned an unexpected answer.');
  }
  return entry as Record<string, unknown>;
}

function readAddProjectRequiredString(source: Record<string, unknown>, key: string): string {
  const value = source[key];
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error('The machine returned an unexpected answer.');
  }
  return value;
}

function readAddProjectMachineOptions(value: unknown): readonly AddProjectMachineOption[] {
  const container = value as { machines?: unknown } | null | undefined;
  const machines = container && typeof container === 'object' ? container.machines : undefined;
  if (!Array.isArray(machines)) {
    throw new Error('Ghostex could not list its machines.');
  }
  return machines.map((machine) => {
    const record = machine as Record<string, unknown>;
    return {
      ...(typeof record.description === 'string' ? { description: record.description } : {}),
      label: readAddProjectRequiredString(record, 'label'),
      machineId: readAddProjectRequiredString(record, 'machineId'),
      ...(typeof record.platform === 'string' ? { platform: record.platform } : {}),
    };
  });
}

function readAddProjectBrowseResult(value: unknown): AddProjectBrowseResult {
  if (!isRemoteFilesystemBrowseResult(value)) {
    throw new Error('The machine returned an unexpected answer.');
  }
  return { entries: value.entries, parentPath: value.parentPath };
}

function readAddProjectCreateDirectoryResult(
  value: unknown,
  requestedParentPath: string,
  requestedName: string
): AddProjectCreateDirectoryResult {
  const record = (value ?? {}) as Record<string, unknown>;
  return {
    name: typeof record.name === 'string' && record.name ? record.name : requestedName,
    parentPath: typeof record.parentPath === 'string' && record.parentPath ? record.parentPath : requestedParentPath,
    path: readAddProjectRequiredString(record, 'path'),
  };
}

function readAddProjectAddResult(value: unknown, machineId: string, requestedPath: string): AddProjectAddResult {
  const project = readAddProjectResultObject(value, 'project');
  return {
    machineId,
    path: typeof project.path === 'string' && project.path ? project.path : requestedPath,
    ...(typeof project.projectId === 'string' ? { projectId: project.projectId } : {}),
  };
}

function readAddProjectDiscovery(value: unknown): AddProjectSourceControlDiscovery {
  const discovery = readAddProjectResultObject(value, 'discovery');
  if (!Array.isArray(discovery.providers)) {
    throw new Error('The machine returned an unexpected answer.');
  }
  return discovery as unknown as AddProjectSourceControlDiscovery;
}

function readAddProjectRepositoryInfo(value: unknown): AddProjectRepositoryInfo {
  const repository = readAddProjectResultObject(value, 'repository');
  return {
    nameWithOwner: readAddProjectRequiredString(repository, 'nameWithOwner'),
    provider: readAddProjectRequiredString(repository, 'provider') as AddProjectRepositoryInfo['provider'],
    sshUrl: readAddProjectRequiredString(repository, 'sshUrl'),
    url: readAddProjectRequiredString(repository, 'url'),
  };
}

function readAddProjectCloneHandle(value: unknown): AddProjectCloneJobHandle {
  const job = readAddProjectResultObject(value, 'job');
  return { jobId: readAddProjectRequiredString(job, 'jobId') };
}

function readAddProjectClonePreview(value: unknown): AddProjectClonePreview {
  const preview = readAddProjectResultObject(value, 'preview');
  const destinationExistsKind = preview.destinationExistsKind;
  if (
    destinationExistsKind !== undefined &&
    destinationExistsKind !== 'directory' &&
    destinationExistsKind !== 'file' &&
    destinationExistsKind !== 'other'
  ) {
    throw new Error('The machine returned an unexpected clone destination.');
  }
  return {
    ...(typeof preview.branchName === 'string' ? { branchName: preview.branchName } : {}),
    cloneMainOnly: preview.cloneMainOnly === true,
    cloneUrl: readAddProjectRequiredString(preview, 'cloneUrl'),
    destinationBlocked: preview.destinationBlocked === true,
    destinationExists: preview.destinationExists === true,
    ...(destinationExistsKind ? { destinationExistsKind } : {}),
    destinationFolderName: readAddProjectRequiredString(preview, 'destinationFolderName'),
    ...(typeof preview.destinationIsEmpty === 'boolean' ? { destinationIsEmpty: preview.destinationIsEmpty } : {}),
    destinationPath: readAddProjectRequiredString(preview, 'destinationPath'),
    parentPath: readAddProjectRequiredString(preview, 'parentPath'),
    repositoryName: readAddProjectRequiredString(preview, 'repositoryName'),
    shallowClone: preview.shallowClone === true,
    ...(typeof preview.warning === 'string' ? { warning: preview.warning } : {}),
  };
}

function readAddProjectCloneJob(value: unknown): AddProjectCloneJob {
  const job = readAddProjectResultObject(value, 'job');
  const state = readAddProjectRequiredString(job, 'state');
  if (state !== 'canceled' && state !== 'completed' && state !== 'failed' && state !== 'running') {
    throw new Error('The machine returned an unexpected clone state.');
  }
  return {
    ...(typeof job.error === 'string' ? { error: job.error } : {}),
    jobId: readAddProjectRequiredString(job, 'jobId'),
    ...(typeof job.message === 'string' ? { message: job.message } : {}),
    ...(typeof job.projectPath === 'string' ? { projectPath: job.projectPath } : {}),
    state,
  };
}

function isRemoteFilesystemBrowseResult(value: unknown): value is RemoteFilesystemBrowseResult {
  if (!value || typeof value !== 'object') {
    return false;
  }
  const candidate = value as Partial<RemoteFilesystemBrowseResult>;
  return (
    typeof candidate.parentPath === 'string' &&
    Array.isArray(candidate.entries) &&
    candidate.entries.every(
      (entry) =>
        Boolean(entry) &&
        typeof entry === 'object' &&
        typeof (entry as { fullPath?: unknown }).fullPath === 'string' &&
        typeof (entry as { name?: unknown }).name === 'string'
    )
  );
}

function AppModalHost() {
  const {
    activeModal,
    activeModalRequestId,
    addProject,
    agentHooksRequired,
    agentsHubCatalog,
    agentsHubFileContent,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    worktreeRename,
    missingProjectFolder,
    commandPaletteInitialQuery,
    commandPaletteOpenRequestSequence,
    isCommandPalettePrewarm,
    closeGitFileDiff,
    closeModal,
    completeFirstLaunchSetup,
    recentProjects,
    remoteGxserverInstall,
    remoteProjectPicker,
    renameSession,
    sessionNote,
    sidebarSpaceEditor,
    stashedPrompts,
    beginExportTranscriptExport,
    exportTranscriptResult,
    updateAvailable,
    worktree,
    agentHookStatus,
    ghostexCliStatus,
    ghostexFolderStats,
    osIntegrationStatus,
    pluginSettingsStatus,
    // CDXC:AppIconPicker 2026-06-25-21:50: Pull relayed App Icon state for the Settings modal.
    appIconState,
    portlessSetup,
    settingsInitialSection,
    settingsInitialRemoteMachineId,
    settingsInitialRemoteSection,
    settingsInitialSearchQuery,
    settingsInitialTabOverride,
  } = useModalStateFromNative();
  const [agentHookStatusLoading, setAgentHookStatusLoading] = useState(false);
  const [ghostexCliStatusLoading, setGhostexCliStatusLoading] = useState(false);
  const [ghostexFolderStatsLoading, setGhostexFolderStatsLoading] = useState(false);
  const [osIntegrationStatusLoading, setOSIntegrationStatusLoading] = useState(false);
  const [pluginSettingsStatusLoading, setPluginSettingsStatusLoading] = useState(false);
  const [isPreviousSessionsInitialLoadReady, setIsPreviousSessionsInitialLoadReady] = useState(false);
  const [isRecentProjectsInitialLoadReady, setIsRecentProjectsInitialLoadReady] = useState(false);
  const sentNativeFitHeightMeasurementKeysRef = useRef<Set<string>>(new Set());
  const previousSettingsRenderStateLogRef = useRef('');
  const previousFirstLaunchSetupRenderStateLogRef = useRef('');
  const latestSettingsPresentedLogDetailsRef = useRef<Record<string, string | number | boolean | null | undefined>>({});
  const latestFirstLaunchSetupPresentedLogDetailsRef = useRef<
    Record<string, string | number | boolean | null | undefined>
  >({});
  const settings = useSidebarStore((state) => state.hud.settings);
  const appIconPickerUnavailable = useSidebarStore((state) => state.hud.appIconPickerUnavailable === true);
  const revision = useSidebarStore((state) => state.revision);
  const agents = useSidebarStore((state) => state.hud.agents);
  const commands = useSidebarStore((state) => state.hud.commands);
  const projectSettingsProjects = useSidebarStore((state) => state.hud.projectSettingsProjects ?? []);
  const portless = useSidebarStore((state) => state.hud.portless);
  const customThemeColor = useSidebarStore((state) => state.hud.customThemeColor);
  const theme = useSidebarStore((state) => state.hud.theme);
  const delayedSendCloseAfterDoneActive = useSidebarStore((state) => {
    const sessionId = delayedSend?.sessionId;
    if (!sessionId) {
      return false;
    }
    return (
      delayedSend.closeAfterDoneActive ??
      state.sessionsById[sessionId]?.closeAfterDone ??
      state.hud.commandSessionIndicators.find((session) => session.sessionId === sessionId)?.closeAfterDone ??
      false
    );
  });
  const [gitCommitPromptAgentId, setGitCommitPromptAgentId] = useState(() => readPromptAgentModalOverride('gitCommit'));
  const [renamePromptAgentId, setRenamePromptAgentId] = useState(() => readPromptAgentModalOverride('renameSession'));
  const previousDefaultPromptAgentIdRef = useRef(settings?.defaultPromptAgentId);
  const resolvedGitCommitPromptAgentId = resolvePromptAgentModalSelection(
    agents,
    gitCommitPromptAgentId,
    settings?.defaultPromptAgentId
  );
  const resolvedRenamePromptAgentId = resolvePromptAgentModalSelection(
    agents,
    renamePromptAgentId,
    settings?.defaultPromptAgentId
  );
  /*
   * CDXC:GxserverAgentSettings 2026-06-19-08:58:
   * The modal store starts with DEFAULT_ghostex_SETTINGS before the native
   * hydrate arrives. Keep Settings and First Launch closed until revision > 0
   * so their full-setting save messages cannot seed gxserver-owned Default
   * Prompt Agent back to Codex from a pre-hydrate placeholder.
   */
  const hasNativeSettingsHydrated = revision > 0;
  const isSettingsModal = isSettingsModalKind(activeModal);
  const isSettingsRenderable = isSettingsModal && hasNativeSettingsHydrated;
  const isFirstLaunchSetupModal = isFirstLaunchSetupModalKind(activeModal);
  const isFirstLaunchSetupRenderable = isFirstLaunchSetupModal && hasNativeSettingsHydrated;
  const settingsInitialTab = settingsInitialTabOverride ?? getSettingsInitialTab(activeModal);
  const hasSettings = settings !== undefined;
  const hasSettingsInitialSection = settingsInitialSection !== undefined;
  const hasSettingsInitialRemoteMachineId = settingsInitialRemoteMachineId !== undefined;
  const hasSettingsInitialSearchQuery = settingsInitialSearchQuery !== undefined;
  const isBaseActiveModalRenderable = isModalRenderable({
    activeModal,
    addProject,
    agentHooksRequired,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    worktreeRename,
    missingProjectFolder,
    remoteGxserverInstall,
    remoteProjectPicker,
    recentProjects,
    renameSession,
    sessionNote,
    sidebarSpaceEditor,
    stashedPrompts,
    exportTranscriptResult,
    updateAvailable,
    settings,
    worktree,
    portlessSetup,
  });
  /*
  CDXC:PreviousSessions 2026-08-07:
  The native app-modal host is hidden until React posts `presented`. Previous
  Sessions delays that signal until its first gxserver history query resolves;
  command-palette time preloads the same retained result so switching tabs can
  present immediately without a loading or premature empty state.
  */
  /*
   * CDXC:SettingsModalStuckBlank 2026-06-20-23:02:
   * Settings must not send native `presented` from the generic modal-ready path
   * while the actual Settings component is still closed on revision 0. Tie
   * Settings-family presentation to the same hydrated renderability condition
   * used by SettingsModal so native cannot believe Settings is open while
   * React is showing no Settings UI.
   */
  const isActiveModalRenderable =
    isBaseActiveModalRenderable &&
    (!isSettingsModal || isSettingsRenderable) &&
    (!isFirstLaunchSetupModal || isFirstLaunchSetupRenderable) &&
    (activeModal !== 'previousSessions' || isPreviousSessionsInitialLoadReady) &&
    (activeModal !== 'recentProjects' || isRecentProjectsInitialLoadReady);
  /*
   * CDXC:SettingsModalDiagnostics 2026-06-20-20:24:
   * Settings presented diagnostics must not add sidebar revision or hydration
   * fields to the `presented` effect dependencies, because that would re-send
   * native presented messages on ordinary sidebar updates. Keep the latest safe
   * diagnostic payload in a ref while preserving the original present trigger.
   */
  latestSettingsPresentedLogDetailsRef.current = {
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    hasSettingsInitialRemoteMachineId,
    hasSettingsInitialSearchQuery,
    hasSettingsInitialSection,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isSettingsRenderable,
    nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
    revision,
    settingsInitialTab,
  };
  latestFirstLaunchSetupPresentedLogDetailsRef.current = {
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isFirstLaunchSetupModal,
    isFirstLaunchSetupRenderable,
    nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
    revision,
  };

  useEffect(() => {
    if (!isSettingsModalKind(activeModal)) {
      previousSettingsRenderStateLogRef.current = '';
      return;
    }
    const signature = JSON.stringify({
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      hasSettingsInitialRemoteMachineId,
      hasSettingsInitialSearchQuery,
      hasSettingsInitialSection,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isSettingsRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
      revision,
      settingsInitialTab,
    });
    if (previousSettingsRenderStateLogRef.current === signature) {
      return;
    }
    previousSettingsRenderStateLogRef.current = signature;
    postSettingsModalDebugLog('modalHost.settings.renderState', {
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      hasSettingsInitialRemoteMachineId,
      hasSettingsInitialSearchQuery,
      hasSettingsInitialSection,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isSettingsRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
      revision,
      settingsInitialTab,
    });
  }, [
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    hasSettingsInitialRemoteMachineId,
    hasSettingsInitialSearchQuery,
    hasSettingsInitialSection,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isSettingsRenderable,
    revision,
    settingsInitialTab,
  ]);

  useEffect(() => {
    if (!isFirstLaunchSetupModalKind(activeModal)) {
      previousFirstLaunchSetupRenderStateLogRef.current = '';
      return;
    }
    /*
     * CDXC:FirstLaunchSetupDiagnostics 2026-06-29-22:08:
     * Setup can feel slow before it ever becomes visible because native waits
     * for React renderability before presenting the child NSPanel. Log each
     * distinct setup renderability state with no settings values or user text.
     */
    const signature = JSON.stringify({
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isFirstLaunchSetupRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
      revision,
    });
    if (previousFirstLaunchSetupRenderStateLogRef.current === signature) {
      return;
    }
    previousFirstLaunchSetupRenderStateLogRef.current = signature;
    postAppModalDebugLog('modalHost.setup.renderState', {
      activeModal,
      hasNativeSettingsHydrated,
      hasSettings,
      isActiveModalRenderable,
      isBaseActiveModalRenderable,
      isFirstLaunchSetupRenderable,
      nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
      revision,
    });
  }, [
    activeModal,
    hasNativeSettingsHydrated,
    hasSettings,
    isActiveModalRenderable,
    isBaseActiveModalRenderable,
    isFirstLaunchSetupRenderable,
    revision,
  ]);

  useEffect(() => {
    if (activeModal !== 'previousSessions') {
      setIsPreviousSessionsInitialLoadReady(false);
    }
  }, [activeModal]);

  const handlePreviousSessionsInitialLoadReady = useCallback(() => {
    setIsPreviousSessionsInitialLoadReady(true);
  }, []);

  useEffect(() => {
    if (activeModal !== 'recentProjects') {
      setIsRecentProjectsInitialLoadReady(false);
    }
  }, [activeModal]);

  const handleRecentProjectsInitialLoadReady = useCallback(() => {
    setIsRecentProjectsInitialLoadReady(true);
  }, []);

  useEffect(() => {
    const previousDefaultPromptAgentId = previousDefaultPromptAgentIdRef.current;
    const nextDefaultPromptAgentId = settings?.defaultPromptAgentId;
    previousDefaultPromptAgentIdRef.current = nextDefaultPromptAgentId;
    if (!previousDefaultPromptAgentId || previousDefaultPromptAgentId === nextDefaultPromptAgentId) {
      return;
    }

    /*
     * CDXC:PromptAgents 2026-05-29-10:53:
     * Per-modal prompt-agent choices are temporary overrides. When the global
     * Settings default prompt agent changes, clear every modal override so Git
     * commit review and Rename Generate Name immediately show the new default.
     */
    clearPromptAgentModalOverrides();
    setGitCommitPromptAgentId(undefined);
    setRenamePromptAgentId(undefined);
  }, [settings?.defaultPromptAgentId]);

  const updateGitCommitPromptAgentId = useCallback((agentId: string) => {
    writePromptAgentModalOverride('gitCommit', agentId);
    setGitCommitPromptAgentId(agentId);
  }, []);

  const updateRenamePromptAgentId = useCallback((agentId: string) => {
    writePromptAgentModalOverride('renameSession', agentId);
    setRenamePromptAgentId(agentId);
  }, []);

  useEffect(() => {
    if (!activeModal) {
      sentNativeFitHeightMeasurementKeysRef.current.clear();
    }
  }, [activeModal]);

  useLayoutEffect(() => {
    if (window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow' && shouldUseOneShotNativeFitHeight(activeModal)) {
      document.body.dataset.appModalFitHeight = 'true';
    } else {
      delete document.body.dataset.appModalFitHeight;
    }
    return () => {
      delete document.body.dataset.appModalFitHeight;
    };
  }, [activeModal]);

  /**
   * CDXC:AppModals 2026-05-08-09:00
   * Native should unhide the transparent modal webview only after the requested
   * modal has enough state to render. This prevents a blank overlay flash while
   * sidebar state is still syncing into the app-modal host.
   *
   * CDXC:AppModals 2026-06-30-16:08:
   * Approved compact native-window modals send their fitted React dialog height
   * once before `presented`, so AppKit can resize the child window without
   * later height churn while the user interacts with the form.
   */
  useLayoutEffect(() => {
    if (!activeModal || !isActiveModalRenderable) {
      return;
    }
    const presentedMessage: { modal: AppModalKind; requestId?: string; type: 'presented' } = {
      modal: activeModal,
      type: 'presented',
    };
    if (activeModalRequestId) {
      presentedMessage.requestId = activeModalRequestId;
    }
    if (isSettingsModalKind(activeModal)) {
      postSettingsModalDebugLog('modalHost.settings.presented.sent', latestSettingsPresentedLogDetailsRef.current);
    }
    if (isFirstLaunchSetupModalKind(activeModal)) {
      postAppModalDebugLog('modalHost.setup.presented.sent', latestFirstLaunchSetupPresentedLogDetailsRef.current);
    }
    if (window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow' && shouldUseOneShotNativeFitHeight(activeModal)) {
      const measurementKey = `${activeModal}:${activeModalRequestId ?? 'none'}`;
      if (!sentNativeFitHeightMeasurementKeysRef.current.has(measurementKey)) {
        const measuredHeight = measureOneShotNativeFitHeight(activeModal);
        if (measuredHeight) {
          sentNativeFitHeightMeasurementKeysRef.current.add(measurementKey);
          const contentHeightMeasuredMessage: {
            height: number;
            modal: AppModalKind;
            nativeWindowHostId?: string;
            requestId?: string;
            type: 'contentHeightMeasured';
          } = {
            height: measuredHeight,
            modal: activeModal,
            type: 'contentHeightMeasured',
          };
          if (window.__ghostex_APP_MODAL_HOST_ID__) {
            contentHeightMeasuredMessage.nativeWindowHostId = window.__ghostex_APP_MODAL_HOST_ID__;
          }
          if (activeModalRequestId) {
            contentHeightMeasuredMessage.requestId = activeModalRequestId;
          }
          postAppModalHostMessage(contentHeightMeasuredMessage, 'AppModals:contentHeightMeasured');
        }
      }
    }
    postAppModalHostMessage(presentedMessage, 'AppModals:presented');
  }, [activeModal, activeModalRequestId, isActiveModalRenderable]);

  useEffect(() => {
    if (activeModal !== 'settings') {
      setGhostexFolderStatsLoading(false);
    }
  }, [activeModal]);

  useEffect(() => {
    if (!activeModal) {
      return;
    }

    const suppressModalWebviewContextMenu = (event: MouseEvent) => {
      if (isEditableAppModalContextMenuTarget(event.target)) {
        return;
      }

      /**
       * CDXC:AppModalContextMenu 2026-05-15-18:15:
       * Right-clicking modal backdrops, blank modal chrome, or modal buttons
       * must not expose WKWebView's native Reload menu. Suppress the webview
       * default while a modal is active, but keep editable fields eligible for
       * their normal editing context menus.
       */
      event.preventDefault();
    };

    document.addEventListener('contextmenu', suppressModalWebviewContextMenu, true);
    return () => {
      document.removeEventListener('contextmenu', suppressModalWebviewContextMenu, true);
    };
  }, [activeModal]);

  useEffect(() => {
    if (ghostexFolderStats) {
      setGhostexFolderStatsLoading(false);
    }
  }, [ghostexFolderStats]);

  useEffect(() => {
    if (agentHookStatus) {
      setAgentHookStatusLoading(false);
    }
  }, [agentHookStatus]);

  useEffect(() => {
    if (ghostexCliStatus) {
      setGhostexCliStatusLoading(false);
    }
  }, [ghostexCliStatus]);

  useEffect(() => {
    if (osIntegrationStatus) {
      setOSIntegrationStatusLoading(false);
    }
  }, [osIntegrationStatus]);

  useEffect(() => {
    if (pluginSettingsStatus) {
      setPluginSettingsStatusLoading(false);
    }
  }, [pluginSettingsStatus]);

  useEffect(() => {
    if (activeModal !== 'settings' || pluginSettingsStatus || pluginSettingsStatusLoading) {
      return;
    }
    setPluginSettingsStatusLoading(true);
    vscode.postMessage({ type: 'requestPluginSettingsStatus' });
  }, [activeModal, pluginSettingsStatus, pluginSettingsStatusLoading]);

  useEffect(() => {
    /*
     * Settings requests CLI status only when Integrations is active. Preserve
     * that request's loading marker here; clearing it makes the still-unknown
     * status render as "Not installed" until the native probe finishes.
     */
    if (activeModal === 'settings') {
      return;
    }
    if (activeModal !== 'firstLaunchSetup' && activeModal !== 'tipsAndTricks') {
      setGhostexCliStatusLoading(false);
      return;
    }
    if (ghostexCliStatus || ghostexCliStatusLoading) {
      return;
    }
    /**
     * CDXC:FirstLaunchSetup 2026-05-26-17:12:
     * The production first-launch modal should reflect the app-bundled CLI that
     * native auto-links on startup. Request native PATH inspection when the setup
     * flow opens and render Storybook through the same status prop.
     *
     * CDXC:FirstLaunchSetup 2026-05-27-02:41:
     * Tips & Tricks now opens the first-launch modal, so the legacy modal id must
     * receive the same CLI status request while old menu messages are still in use.
     */
    setGhostexCliStatusLoading(true);
    vscode.postMessage({ type: 'requestGhostexCliStatus' });
  }, [activeModal, ghostexCliStatus, ghostexCliStatusLoading]);

  useEffect(() => {
    document.body.dataset.sidebarTheme = theme;
    /**
     * CDXC:AccentColor 2026-08-24:
     * Modals read their accent from --ghostex-accent, so publish the setting
     * onto the modal host body alongside the workspace theme variables. Before
     * the HUD settings arrive the normalized default is the correct value.
     */
    document.body.style.setProperty('--ghostex-accent', settings?.accentColor ?? DEFAULT_ghostex_SETTINGS.accentColor);
    const normalizedThemeColor = normalizeWorkspaceThemeColor(customThemeColor);
    if (normalizedThemeColor) {
      document.body.dataset.sidebarCustomTheme = 'true';
      document.body.style.setProperty('--workspace-sidebar-theme-color', normalizedThemeColor);
      document.body.style.setProperty(
        '--workspace-sidebar-theme-foreground',
        getWorkspaceThemeForeground(normalizedThemeColor)
      );
    } else {
      delete document.body.dataset.sidebarCustomTheme;
      document.body.style.removeProperty('--workspace-sidebar-theme-color');
      document.body.style.removeProperty('--workspace-sidebar-theme-foreground');
    }

    return () => {
      delete document.body.dataset.sidebarTheme;
      delete document.body.dataset.sidebarCustomTheme;
      document.body.style.removeProperty('--workspace-sidebar-theme-color');
      document.body.style.removeProperty('--workspace-sidebar-theme-foreground');
      document.body.style.removeProperty('--ghostex-accent');
    };
  }, [customThemeColor, settings?.accentColor, theme]);

  return (
    <>
      <PreviousSessionsModal
        isOpen={activeModal === 'previousSessions'}
        onClose={closeModal}
        onInitialLoadReady={handlePreviousSessionsInitialLoadReady}
        shouldPreload={
          activeModal === 'commandPalette' || activeModal === 'recentProjects' || activeModal === 'stashedPrompts'
        }
        vscode={vscode}
      />
      <UpdateAvailableModal
        isOpen={activeModal === 'updateAvailable' && updateAvailable !== undefined}
        onCancel={closeModal}
        onDownload={() => {
          postAppModalHostMessage({ type: 'downloadGhostexUpdate' }, 'AppModals:update:download');
        }}
        onRestart={() => {
          postAppModalHostMessage({ type: 'restartAndUpdateGhostex' }, 'AppModals:update:restart');
        }}
        update={updateAvailable}
      />
      <RecentProjectsModal
        isOpen={activeModal === 'recentProjects' && recentProjects !== undefined}
        machineId={recentProjects?.machineId}
        machineName={recentProjects?.machineName}
        onClose={closeModal}
        onInitialLoadReady={handleRecentProjectsInitialLoadReady}
        vscode={vscode}
      />
      <StashedPromptsModal
        initialScope={stashedPrompts?.initialScope}
        isOpen={activeModal === 'stashedPrompts' && stashedPrompts !== undefined}
        onClose={closeModal}
        projectId={stashedPrompts?.projectId}
        sessionId={stashedPrompts?.sessionId}
        vscode={vscode}
      />
      <FirstUserMessageModal
        isOpen={activeModal === 'firstUserMessage' && firstUserMessage !== undefined}
        message={firstUserMessage?.message ?? ''}
        onClose={closeModal}
        title={firstUserMessage?.title}
      />
      <AgentHooksRequiredModal
        agentName={agentHooksRequired?.agentName ?? 'this agent'}
        isOpen={activeModal === 'agentHooksRequired' && agentHooksRequired !== undefined}
        onClose={closeModal}
        onInstall={() => {
          if (!agentHooksRequired) {
            return;
          }
          vscode.postMessage({
            agentId: agentHooksRequired.agentId,
            groupId: agentHooksRequired.groupId,
            hookAgentId: agentHooksRequired.hookAgentId,
            installHooks: true,
            type: 'confirmAgentHookLaunch',
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onSkip={() => {
          if (!agentHooksRequired) {
            return;
          }
          vscode.postMessage({
            agentId: agentHooksRequired.agentId,
            groupId: agentHooksRequired.groupId,
            hookAgentId: agentHooksRequired.hookAgentId,
            installHooks: false,
            type: 'confirmAgentHookLaunch',
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
      />
      <MissingProjectFolderModal
        isOpen={activeModal === 'missingProjectFolder' && missingProjectFolder !== undefined}
        onCancel={closeModal}
        onLocate={() => {
          if (!missingProjectFolder) {
            return;
          }
          vscode.postMessage({
            projectId: missingProjectFolder.projectId,
            type: 'pickReplacementProjectFolder',
          });
        }}
        onRemove={() => {
          if (!missingProjectFolder) {
            return;
          }
          vscode.postMessage({
            projectId: missingProjectFolder.projectId,
            type: 'removeProject',
          });
          closeModal();
        }}
        projectName={missingProjectFolder?.projectName ?? 'this project'}
        projectPath={missingProjectFolder?.projectPath ?? ''}
      />
      <RemoteGxserverInstallModal
        isOpen={activeModal === 'remoteGxserverInstall' && remoteGxserverInstall !== undefined}
        machineName={remoteGxserverInstall?.remoteMachineName ?? 'Remote'}
        onApprove={() => {
          if (!remoteGxserverInstall) {
            postRemoteGxserverInstallDebugLog('remoteGxserverInstall.approve.missingState', {
              activeModal: activeModal ?? null,
              hasRemoteGxserverInstall: false,
              nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
            });
            return;
          }
          postRemoteGxserverInstallDebugLog('remoteGxserverInstall.approve.clicked', {
            activeModal: activeModal ?? null,
            hasRemoteGxserverInstall: true,
            installApproved: true,
            nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
            remoteMachineId: remoteGxserverInstall.remoteMachineId,
          });
          vscode.postMessage({
            installApproved: true,
            remoteMachineId: remoteGxserverInstall.remoteMachineId,
            type: 'reconnectRemoteMachine',
          });
          postRemoteGxserverInstallDebugLog('remoteGxserverInstall.approve.commandPosted', {
            activeModal: activeModal ?? null,
            installApproved: true,
            remoteMachineId: remoteGxserverInstall.remoteMachineId,
          });
          closeModal();
        }}
        onCancel={closeModal}
      />
      <RemoteSetupModal
        isOpen={activeModal === 'remoteSetup'}
        onClose={closeModal}
        onOpenExternalUrl={(url) => {
          vscode.postMessage({ type: 'openExternalUrl', url });
        }}
        rpc={gpuiBootstrapRemoteSetupRpc()}
      />
      <RemoteProjectPickerModal
        initialQuery={remoteProjectPicker?.initialQuery}
        isOpen={activeModal === 'remoteProjectPicker' && remoteProjectPicker !== undefined}
        machineName={remoteProjectPicker?.remoteMachineName ?? 'Remote'}
        onAddProject={async (path) => {
          if (!remoteProjectPicker) {
            return;
          }
          const requestId = createRemoteProjectRequestId('add');
          vscode.postMessage({
            path,
            remoteMachineId: remoteProjectPicker.remoteMachineId,
            requestId,
            type: 'addRemoteProjectPath',
          });
          await waitForRemoteProjectAddResult(requestId);
        }}
        onBrowse={async (input) => {
          if (!remoteProjectPicker) {
            return null;
          }
          const requestId = createRemoteProjectRequestId('browse');
          vscode.postMessage({
            partialPath: input.partialPath,
            remoteMachineId: remoteProjectPicker.remoteMachineId,
            requestId,
            type: 'browseRemoteProjectDirectories',
          });
          return waitForRemoteProjectDirectoryBrowseResult(requestId);
        }}
        onClose={closeModal}
      />
      {/*
       * CDXC:AddProject 2026-07-30:
       * The shared add-project dialog replaces both the native OS folder picker
       * and the remote project picker. It is transport-free by design, so every
       * callback here is the same bounded host round trip and the machine id it
       * was handed is the only routing information that crosses back.
       */}
      <AddProjectModal
        addProject={async ({ createIfMissing, machineId, path }) =>
          readAddProjectAddResult(
            await requestAddProjectDialogOperation('add', ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS, {
              machineId,
              params: { createIfMissing, path },
            }),
            machineId,
            path
          )
        }
        browse={async ({ cwd, machineId, partialPath }) =>
          readAddProjectBrowseResult(
            await requestAddProjectDialogOperation('browse', ADD_PROJECT_DIALOG_BROWSE_TIMEOUT_MS, {
              machineId,
              params: cwd ? { cwd, partialPath } : { partialPath },
            })
          )
        }
        cancelCloneJob={async ({ jobId, machineId }) => {
          await requestAddProjectDialogOperation('cancelCloneJob', ADD_PROJECT_DIALOG_JOB_TIMEOUT_MS, {
            machineId,
            params: { jobId },
          });
        }}
        createDirectory={async ({ machineId, name, parentPath }) =>
          readAddProjectCreateDirectoryResult(
            await requestAddProjectDialogOperation('createDirectory', ADD_PROJECT_DIALOG_JOB_TIMEOUT_MS, {
              machineId,
              params: { name, parentPath },
            }),
            parentPath,
            name
          )
        }
        discoverSourceControl={async ({ machineId }) =>
          readAddProjectDiscovery(
            await requestAddProjectDialogOperation('discoverSourceControl', ADD_PROJECT_DIALOG_DISCOVERY_TIMEOUT_MS, {
              machineId,
            })
          )
        }
        initialMachineId={addProject?.machineId}
        isOpen={activeModal === 'addProject' && addProject !== undefined}
        listMachineOptions={async () =>
          readAddProjectMachineOptions(
            await requestAddProjectDialogOperation('listMachines', ADD_PROJECT_DIALOG_JOB_TIMEOUT_MS)
          )
        }
        lookupRepository={async ({ machineId, provider, repository }) =>
          readAddProjectRepositoryInfo(
            await requestAddProjectDialogOperation('lookupRepository', ADD_PROJECT_DIALOG_LOOKUP_TIMEOUT_MS, {
              machineId,
              params: { provider, repository },
            })
          )
        }
        onClose={closeModal}
        previewClone={async ({ branchName, cloneMainOnly, destinationPath, machineId, remoteUrl, shallowClone }) =>
          readAddProjectClonePreview(
            await requestAddProjectDialogOperation('previewClone', ADD_PROJECT_DIALOG_LOOKUP_TIMEOUT_MS, {
              machineId,
              params: {
                branchName,
                cloneMainOnly,
                destinationPath,
                remoteUrl,
                shallowClone,
              },
            })
          )
        }
        readCloneJob={async ({ jobId, machineId }) =>
          readAddProjectCloneJob(
            await requestAddProjectDialogOperation('readCloneJob', ADD_PROJECT_DIALOG_JOB_TIMEOUT_MS, {
              machineId,
              params: { jobId },
            })
          )
        }
        startClone={async ({ branchName, cloneMainOnly, destinationPath, machineId, remoteUrl, shallowClone }) =>
          readAddProjectCloneHandle(
            await requestAddProjectDialogOperation('startClone', ADD_PROJECT_DIALOG_ADD_TIMEOUT_MS, {
              machineId,
              params: {
                branchName,
                cloneMainOnly,
                destinationPath,
                remoteUrl,
                shallowClone,
              },
            })
          )
        }
      />
      <AgentsHubModal
        catalog={agentsHubCatalog}
        fileContent={agentsHubFileContent}
        isOpen={activeModal === 'agentsHub'}
        onClose={closeModal}
        vscode={vscode}
      />
      {/*
       * CDXC:CommandPalette 2026-06-13-10:26:
       * The configured command-palette hotkey must render in the same
       * full-window app-modal host as Settings, not inside the sidebar webview.
       * The palette reads mirrored sidebar state here so its command list
       * remains current while the dialog is centered over the whole Ghostex
       * window.
       */}
      <CommandPalette
        commands={commands}
        hotkeys={settings?.hotkeys}
        initialQuery={commandPaletteInitialQuery}
        isInitialLoadResolved={hasNativeSettingsHydrated}
        isOpen={activeModal === 'commandPalette'}
        isPrewarm={isCommandPalettePrewarm}
        onOpenChange={(isOpen) => {
          if (!isOpen) {
            closeModal();
          }
        }}
        openRequestSequence={commandPaletteOpenRequestSequence}
        openTargetSettings={settings}
        petOverlayEnabled={settings?.petOverlayEnabled}
        vscode={vscode}
      />
      <DelayedSendModal
        agentIcon={delayedSend?.agentIcon}
        closeAfterDoneActive={delayedSendCloseAfterDoneActive}
        delayedSendDeadlineAt={delayedSend?.delayedSendDeadlineAt}
        delayedSendRemainingLabel={delayedSend?.delayedSendRemainingLabel}
        isOpen={activeModal === 'delayedSend' && delayedSend !== undefined}
        onCancel={closeModal}
        onCancelTimer={() => {
          if (!delayedSend) {
            return;
          }
          vscode.postMessage({
            sessionId: delayedSend.sessionId,
            type: 'cancelDelayedSend',
          });
          closeModal();
        }}
        onConfirm={(delayMs, sendWhenAgentStops, sendWhenAllProjectSessionsStop) => {
          if (!delayedSend) {
            return;
          }
          vscode.postMessage({
            ...(delayMs === undefined ? {} : { delayMs }),
            sendWhenAllProjectSessionsStop,
            sendWhenAgentStops,
            sessionId: delayedSend.sessionId,
            type: 'scheduleDelayedSend',
          });
          closeModal();
        }}
        onToggleCloseAfterDone={() => {
          if (!delayedSend) {
            return;
          }
          vscode.postMessage({
            sessionId: delayedSend.sessionId,
            type: 'toggleCloseAfterDone',
          });
          closeModal();
        }}
        sendWhenAllProjectSessionsStopActive={delayedSend?.sendWhenAllProjectSessionsStopActive}
        sendWhenAgentStopsActive={delayedSend?.sendWhenAgentStopsActive}
        sessionTitle={delayedSend?.title}
        supportsSendWhenAgentStops={delayedSend?.supportsSendWhenAgentStops}
        supportsSendWhenAllProjectSessionsStop={delayedSend?.supportsSendWhenAllProjectSessionsStop}
      />
      <GitCommitModal
        agents={agents}
        draft={
          gitCommit ?? {
            confirmLabel: 'Commit',
            description: '',
            changedFiles: [],
            requestId: '',
            showCommitMessage: true,
            suggestedBody: undefined,
            suggestedSubject: '',
          }
        }
        isOpen={activeModal === 'gitCommit' && gitCommit !== undefined}
        fileDiffDraft={gitFileDiff}
        onCancel={(requestId) => {
          vscode.postMessage({ requestId, type: 'cancelSidebarGitCommit' });
          closeModal();
        }}
        onConfirm={(requestId, message, options) => {
          vscode.postMessage({
            agentId: options.agentId,
            commitOnNewRef: options.commitOnNewRef,
            deleteWorktreeAfter: options.deleteWorktreeAfter,
            filePaths: options.filePaths,
            message,
            requestId,
            type: 'confirmSidebarGitCommit',
          });
          closeModal();
        }}
        onDirectMerge={(requestId, message, options) => {
          vscode.postMessage({
            agentId: options.agentId,
            deleteWorktreeAfter: options.deleteWorktreeAfter,
            filePaths: options.filePaths,
            message,
            requestId,
            type: 'confirmSidebarGitDirectMerge',
          });
          closeModal();
        }}
        onMultipleCommits={(requestId, agentId) => {
          vscode.postMessage({ agentId, requestId, type: 'runSidebarGitMultipleCommits' });
          closeModal();
        }}
        onOpenFileDiff={(filePath, requestId) => {
          vscode.postMessage({ filePath, requestId, type: 'openSidebarGitChangedFileDiff' });
        }}
        onPromptAgentIdChange={updateGitCommitPromptAgentId}
        promptAgentId={resolvedGitCommitPromptAgentId}
        theme={theme}
      />
      {activeModal === 'gitCommit' ? null : (
        <GitFileDiffModal
          draft={
            gitFileDiff ?? {
              filePath: '',
              patch: 'No diff is available for this file.',
            }
          }
          isOpen={gitFileDiff !== undefined}
          onClose={closeGitFileDiff}
          theme={theme}
        />
      )}
      <WorktreeDeleteModal
        draft={
          worktreeDelete ?? {
            branch: null,
            canDeleteLocalBranch: false,
            groupId: '',
            hasChanges: false,
            projectId: '',
            remoteBranchExists: false,
            statusSummary: '',
            worktreeName: 'worktree',
          }
        }
        isOpen={activeModal === 'deleteWorktree' && worktreeDelete !== undefined}
        onCancel={closeModal}
        onCommit={(groupId) => {
          vscode.postMessage({ groupId, type: 'commitWorktreeBeforeDelete' });
          closeModal();
        }}
        onDelete={(projectId, options) => {
          vscode.postMessage({
            deleteLocalBranch: options.deleteLocalBranch,
            deleteRemoteBranch: options.deleteRemoteBranch,
            projectId,
            type: 'confirmDeleteWorktree',
          });
          closeModal();
        }}
        theme={theme}
      />
      <WorktreeRenameModal
        draft={
          worktreeRename ?? {
            currentName: '',
            currentPath: '',
            parentFolderName: '',
            parentProjectPath: '',
            projectId: '',
            renameBranchDefault: false,
            worktreeName: 'worktree',
          }
        }
        isOpen={activeModal === 'renameWorktree' && worktreeRename !== undefined}
        onCancel={closeModal}
        onRename={(projectId, options) => {
          vscode.postMessage({
            name: options.name,
            projectId,
            renameBranch: options.renameBranch,
            type: 'confirmRenameWorktree',
          });
          closeModal();
        }}
        theme={theme}
      />
      {/*
       * CDXC:Worktrees 2026-06-02-13:41:
       * Creating a project worktree is a full-window modal flow because macOS
       * owns the agent, first prompt, and image attachment drafts before submit,
       * while gxserver owns the branch/worktree mutation and returned project.
       *
       * CDXC:GPUIWorktrees 2026-06-24-14:06:
       * Open Existing mode shares the worktree first-prompt controls. Blank
       * prompt submits remain project-open-only; non-blank prompts carry the
       * user-selected agent and prompt alongside the selected worktree path so
       * native and GPUI receivers can start the actual agent session.
       *
       * CDXC:WorktreeBaseBranch 2026-06-24-11:32:
       * Create New mode must send the selected base branch through the sidebar
       * command so the worktree starts from the chosen branch instead of the
       * currently checked-out HEAD.
       */}
      <WorktreeCreateModal
        agents={agents}
        defaultAgentId={settings?.defaultPromptAgentId}
        isOpen={activeModal === 'worktree' && worktree !== undefined}
        onCancel={closeModal}
        onConfirm={(draft) => {
          vscode.postMessage({
            agentId: draft.agentId,
            baseBranch: draft.mode === 'create' ? draft.baseBranch : undefined,
            existingWorktreeKey: draft.mode === 'openExisting' ? draft.existingWorktreeKey : undefined,
            existingWorktreePath: draft.mode === 'openExisting' ? draft.existingWorktreePath : undefined,
            mode: draft.mode,
            projectId: worktree?.projectId,
            projectPath: worktree?.projectPath,
            prompt: draft.prompt,
            remoteMachineId: worktree?.remoteMachineId,
            type: 'createProjectWorktree',
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onRequestExistingWorktrees={(requestId) => {
          vscode.postMessage({
            projectId: worktree?.projectId,
            projectPath: worktree?.projectPath,
            remoteMachineId: worktree?.remoteMachineId,
            requestId,
            type: 'requestProjectWorktrees',
          } satisfies SidebarToExtensionMessage);
        }}
        projectName={worktree?.projectName}
      />
      <PortlessSetupModal
        isOpen={activeModal === 'portlessSetup' && portlessSetup !== undefined}
        mode={portlessSetup?.mode ?? 'firstSetup'}
        onAdminAction={(action, protocol, requestId) => {
          vscode.postMessage({
            action,
            protocol,
            requestId,
            type: 'runPortlessSetupPromptAdminAction',
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onCancel={() => {
          vscode.postMessage({ type: 'cancelPortlessSetupPrompt' } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onDisable={() => {
          vscode.postMessage({
            enabled: false,
            type: 'setPortlessEnabled',
          } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        onPostpone={() => {
          vscode.postMessage({ type: 'postponePortlessSetupPrompt' } satisfies SidebarToExtensionMessage);
          closeModal();
        }}
        protocol={portlessSetup?.protocol ?? 'https'}
      />
      <SettingsModal
        agentHookStatus={agentHookStatus}
        agentHookStatusLoading={agentHookStatusLoading}
        appIconPickerUnavailable={appIconPickerUnavailable}
        automateIsExperimental={window.__ghostex_APP_MODAL_HOST_ID__ !== 'gpui'}
        initialSection={settingsInitialSection}
        initialRemoteMachineId={settingsInitialRemoteMachineId}
        initialRemoteSection={settingsInitialRemoteSection}
        initialSearchQuery={settingsInitialSearchQuery}
        initialTab={settingsInitialTab}
        isOpen={isSettingsRenderable}
        onChange={(nextSettings, source = 'settings:bulk') => {
          vscode.postMessage({
            settings: nextSettings,
            source,
            type: 'updateSettings',
          });
        }}
        onPatch={(patch, source) => {
          vscode.postMessage({
            baseRevision: revision,
            patch,
            source,
            type: 'updateSettingsPatch',
          });
        }}
        onGhosttySettingsAction={(action) => {
          vscode.postMessage({ type: action });
        }}
        onInstallGhostexCli={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installGhostexCli' });
        }}
        onInstallBrowserControl={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installBrowserControl' });
        }}
        onInstallBrowserUseSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installBrowserUseSkill' });
        }}
        onInstallComputerUseSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installComputerUseSkill' });
        }}
        onInstallCliSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installCliSkill' });
        }}
        onInstallFable56OrchestrationSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installFable56OrchestrationSkill' });
        }}
        onInstallManageBeadsSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installManageBeadsSkill' });
        }}
        onInstallGenerateTitleSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installGenerateTitleSkill' });
        }}
        onInstallMoveCodexSessionSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installMoveCodexSessionSkill' });
        }}
        onInstallCuaDriver={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installCuaDriver' });
        }}
        onSetOSIntegrationDefaults={(target) => {
          setOSIntegrationStatusLoading(true);
          vscode.postMessage({ target, type: 'setOSIntegrationDefaults' });
        }}
        onPlayCompletionSound={(sound) => {
          vscode.postMessage({ sound, type: 'playCompletionSoundPreview' });
        }}
        onOpenAccessibilityPreferences={() => {
          /**
           * CDXC:AccessibilityPermissions 2026-05-27-07:24
           * The settings modal button should open macOS Accessibility settings
           * directly for desktop integrations without enabling any removed
           * IDE attachment behavior.
           */
          vscode.postMessage({ type: 'openAccessibilityPreferences' });
        }}
        onOpenMacOSNotificationSettings={() => {
          vscode.postMessage({ type: 'openMacOSNotificationSettings' });
        }}
        onOpenScreenRecordingPreferences={() => {
          vscode.postMessage({ type: 'openScreenRecordingPreferences' });
        }}
        onOpenGhostexFolder={() => {
          vscode.postMessage({ type: 'openGhostexFolder' });
        }}
        onRequestMacOSNotificationPermission={() => {
          vscode.postMessage({ type: 'requestMacOSNotificationPermission' });
        }}
        onRequestGhostexFolderStats={() => {
          setGhostexFolderStatsLoading(true);
          vscode.postMessage({ type: 'requestGhostexFolderStats' });
        }}
        onRequestAgentHookStatus={() => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ type: 'requestAgentHookStatus' });
        }}
        onRequestGhostexCliStatus={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'requestGhostexCliStatus' });
        }}
        onRequestOSIntegrationStatus={() => {
          setOSIntegrationStatusLoading(true);
          vscode.postMessage({ type: 'requestOSIntegrationStatus' });
        }}
        onRequestPluginSettingsStatus={() => {
          setPluginSettingsStatusLoading(true);
          vscode.postMessage({ type: 'requestPluginSettingsStatus' });
        }}
        onReinstallPlugin={(pluginId) => {
          setPluginSettingsStatusLoading(true);
          vscode.postMessage({ pluginId, type: 'reinstallPlugin' });
        }}
        onInstallAgentHooks={(agentIds) => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ agentIds, type: 'installAgentHooks' });
        }}
        onUninstallAgentHooks={(agentIds) => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ agentIds, type: 'uninstallAgentHooks' });
        }}
        onUninstallBundledAgentSkill={(skillId) => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ skillId, type: 'uninstallBundledAgentSkill' });
        }}
        onUninstallBundledAgentSkills={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'uninstallBundledAgentSkills' });
        }}
        onTestAgentTaskCompletion={() => {
          vscode.postMessage({ type: 'testAgentTaskCompletion' });
        }}
        onClose={closeModal}
        portless={portless}
        projects={projectSettingsProjects}
        settings={settings}
        tailcatRpc={gpuiBootstrapTailcatRpc()}
        vscode={vscode}
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        ghostexFolderStats={ghostexFolderStats}
        ghostexFolderStatsLoading={ghostexFolderStatsLoading}
        osIntegrationStatus={osIntegrationStatus}
        osIntegrationStatusLoading={osIntegrationStatusLoading}
        pluginSettingsStatus={pluginSettingsStatus}
        pluginSettingsStatusLoading={pluginSettingsStatusLoading}
        // CDXC:AppIconPicker 2026-06-25-21:50: Prop-driven App Icon state for Settings (mirrors osIntegrationStatus).
        appIconState={appIconState}
      />
      <DiscoverGhostexModal isOpen={activeModal === 'discoverGhostex'} onClose={closeModal} theme={theme} />
      <WatchGhostexVideoModal isOpen={activeModal === 'watchGhostexVideo'} onClose={closeModal} theme={theme} />
      <FirstLaunchSetupModal
        agentHookStatus={agentHookStatus}
        agentHookStatusLoading={agentHookStatusLoading}
        ghostexCliStatus={ghostexCliStatus}
        ghostexCliStatusLoading={ghostexCliStatusLoading}
        hasProjects={projectSettingsProjects.length > 0}
        isOpen={isFirstLaunchSetupRenderable}
        onChange={(nextSettings) => {
          vscode.postMessage({
            settings: nextSettings,
            source: 'firstLaunch:preferences',
            type: 'updateSettings',
          });
        }}
        onClose={completeFirstLaunchSetup}
        onInstallAgentHooks={(agentIds) => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ agentIds, type: 'installAgentHooks' });
        }}
        onInstallGhostexCli={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installGhostexCli' });
        }}
        onInstallBrowserControl={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installBrowserControl' });
        }}
        onInstallBrowserUseSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installBrowserUseSkill' });
        }}
        onInstallComputerUseSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installComputerUseSkill' });
        }}
        onInstallCliSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installCliSkill' });
        }}
        onInstallFable56OrchestrationSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installFable56OrchestrationSkill' });
        }}
        onInstallManageBeadsSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installManageBeadsSkill' });
        }}
        onInstallGenerateTitleSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installGenerateTitleSkill' });
        }}
        onInstallMoveCodexSessionSkill={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installMoveCodexSessionSkill' });
        }}
        onInstallSelectedSkills={requestFirstLaunchInstallSelectedSkills}
        onUninstallBundledAgentSkill={(skillId) => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ skillId, type: 'uninstallBundledAgentSkill' });
        }}
        onInstallCuaDriver={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'installCuaDriver' });
        }}
        onOpenAccessibilityPreferences={() => {
          vscode.postMessage({ type: 'openAccessibilityPreferences' });
        }}
        onOpenScreenRecordingPreferences={() => {
          vscode.postMessage({ type: 'openScreenRecordingPreferences' });
        }}
        onPickProjectFolder={() => {
          vscode.postMessage({ type: 'pickFirstLaunchProjectFolder' });
        }}
        onFinishFirstLaunch={({ agentId, path }) => {
          /*
          CDXC:FirstLaunchSetup 2026-08-24:
          Add 1st project registers the folder chosen by the footer action and
          starts the first session in it. Rust forwards this to the sidebar
          runtime over the workspaceFolderPicked chain, which owns project
          registration + focus.
          */
          startFirstLaunchCreateProjectSession(agentId, path);
        }}
        onRequestAgentHookStatus={(agentIds) => {
          setAgentHookStatusLoading(true);
          vscode.postMessage({ agentIds, type: 'requestAgentHookStatus' });
        }}
        onRequestGhostexCliStatus={() => {
          setGhostexCliStatusLoading(true);
          vscode.postMessage({ type: 'requestGhostexCliStatus' });
        }}
        settings={settings}
        theme={theme}
        vscode={vscode}
      />
      <SessionRenameModal
        agents={agents}
        /*
        CDXC:SessionHistoryTitleSource 2026-07-29:
        Empty-title Generate Name summarizes the session's recent transcript
        user messages through gxserver. Only the gpui host routes renameSession
        through the gxserver runtime that supports the empty-text generate
        call, so the deprecated Swift host keeps the pasted-text-only rule.
        */
        canGenerateNameFromSessionHistory={
          window.__ghostex_APP_MODAL_HOST_ID__ === GPUI_APP_MODAL_HOST_ID &&
          sidebarAgentIconSupportsSessionHistoryTitleGeneration(renameSession?.sessionAgentIcon)
        }
        initialTitle={renameSession?.initialTitle ?? ''}
        isOpen={activeModal === 'renameSession' && renameSession !== undefined}
        onCancel={closeModal}
        onConfirm={(title, options) => {
          if (!renameSession) {
            return;
          }
          vscode.postMessage({
            agentId: options?.agentId,
            sessionId: renameSession.sessionId,
            ...(options?.shouldGenerateTitle ? { shouldGenerateTitle: true } : {}),
            title,
            type: 'renameSession',
          });
          closeModal();
        }}
        onPromptAgentIdChange={updateRenamePromptAgentId}
        promptAgentId={resolvedRenamePromptAgentId}
      />
      {/*
      CDXC:SessionAgentNotes 2026-08-24:
      Saving posts the shared `setSessionNote` sidebar command exactly the way
      Rename posts `renameSession`: the dialog reports the typed text and the
      session it belongs to, and the sidebar runtime owns the daemon call and
      the provider-conversation resolution. An empty string is the explicit
      clear, so it is sent rather than suppressed.
      */}
      <SessionNoteModal
        initialNote={sessionNote?.initialNote ?? ''}
        isOpen={activeModal === 'sessionNote' && sessionNote !== undefined}
        onCancel={closeModal}
        onConfirm={(note) => {
          if (!sessionNote) {
            return;
          }
          vscode.postMessage({
            note,
            ...(sessionNote.projectId ? { projectId: sessionNote.projectId } : {}),
            sessionId: sessionNote.sessionId,
            type: 'setSessionNote',
          });
          closeModal();
        }}
        sessionTitle={sessionNote?.sessionTitle}
      />
      {/*
      CDXC:SidebarSpaces 2026-08-27:
      New/Edit Space. The dialog reports field values only; the sidebar runtime
      forwards them to SidebarApp, which is the one place that owns the Space
      document and can apply an edit to the CURRENT one. That is the same
      dialog-reports / host-writes split Rename Session and Session Note use, and
      the reason a Space edit can never clobber a concurrent membership change.
      */}
      <SpaceEditorModal
        initialColor={sidebarSpaceEditor?.spaceColor}
        initialIcon={sidebarSpaceEditor?.spaceIcon}
        initialName={sidebarSpaceEditor?.spaceName}
        isOpen={activeModal === 'sidebarSpaceEditor' && sidebarSpaceEditor !== undefined}
        mode={sidebarSpaceEditor?.mode ?? 'create'}
        onCancel={closeModal}
        onDelete={() => {
          if (!sidebarSpaceEditor?.spaceId) {
            return;
          }
          vscode.postMessage({
            mode: 'delete',
            ...(sidebarSpaceEditor.remoteMachineId ? { remoteMachineId: sidebarSpaceEditor.remoteMachineId } : {}),
            spaceId: sidebarSpaceEditor.spaceId,
            type: 'sidebarSpaceEditorResult',
          });
          closeModal();
        }}
        onSubmit={(space) => {
          if (!sidebarSpaceEditor) {
            return;
          }
          vscode.postMessage({
            color: space.color,
            icon: space.icon,
            ...(sidebarSpaceEditor.memberCollectionId
              ? { memberCollectionId: sidebarSpaceEditor.memberCollectionId }
              : {}),
            ...(sidebarSpaceEditor.memberProjectId ? { memberProjectId: sidebarSpaceEditor.memberProjectId } : {}),
            mode: sidebarSpaceEditor.mode,
            name: space.name,
            ...(sidebarSpaceEditor.remoteMachineId ? { remoteMachineId: sidebarSpaceEditor.remoteMachineId } : {}),
            ...(sidebarSpaceEditor.mode === 'edit' && sidebarSpaceEditor.spaceId
              ? { spaceId: sidebarSpaceEditor.spaceId }
              : {}),
            type: 'sidebarSpaceEditorResult',
          });
          closeModal();
        }}
      />
      {/*
      CDXC:ExportTranscript 2026-08-20 / CDXC:ExportTranscriptOptions 2026-08-24:
      Copy Path is settled inside the dialog; the export itself, Reveal, and
      Start New Conversation are host side effects, so they leave through the
      same sidebarCommand boundary every other modal action uses. Neither
      carries the exported path back out — the host still holds it from the
      runtime's own result message.
      */}
      <ExportTranscriptModal
        agents={agents}
        defaultAgentId={exportTranscriptResult?.agentId}
        isOpen={activeModal === 'exportTranscriptResult' && exportTranscriptResult !== undefined}
        onClose={() => {
          if (exportTranscriptResult?.requestId) {
            vscode.postMessage({
              requestId: exportTranscriptResult.requestId,
              type: 'cancelExportSessionTranscript',
            });
          }
          closeModal();
        }}
        onExport={(options) => {
          if (!exportTranscriptResult?.requestId) {
            return;
          }
          beginExportTranscriptExport();
          vscode.postMessage({
            ...options,
            requestId: exportTranscriptResult.requestId,
            type: 'runExportSessionTranscript',
          });
        }}
        onRevealInFinder={
          exportTranscriptResult?.canReveal
            ? () => {
                vscode.postMessage({ type: 'revealExportedTranscript' });
                closeModal();
              }
            : undefined
        }
        onStartNewConversation={(agentId) => {
          if (!exportTranscriptResult?.requestId) {
            return;
          }
          vscode.postMessage({
            agentId,
            requestId: exportTranscriptResult.requestId,
            type: 'startExportedTranscriptConversation',
          });
          closeModal();
        }}
        stage={exportTranscriptResult?.stage ?? { stage: 'options' }}
      />
      <AgentConfigModal
        draft={config.agentDraft ?? createEmptyAgentDraft()}
        isOpen={activeModal === 'agentConfig' && config.agentDraft !== undefined}
        onCancel={closeModal}
        onSave={(draft) => {
          vscode.postMessage({
            acceptAllMode: draft.acceptAllMode,
            agentId: draft.agentId,
            command: draft.command,
            icon: draft.icon,
            name: draft.name,
            type: 'saveSidebarAgent',
          });
          closeModal();
        }}
        theme={theme}
      />
      {/*
       * CDXC:AppToasts 2026-05-21-12:21:
       * Native/sidebar status feedback should appear as dark Ghostex toasts,
       * not Sonner's bright default surface, so non-blocking Delayed Send and
       * worktree/git notices stay visually consistent with the dark app chrome.
       *
       * CDXC:AppModals 2026-05-28-13:52:
       * Toast overlay chrome should use the same background family as modal
       * and menu overlays instead of the older #181818 surface.
       *
       * CDXC:SidebarTheme 2026-06-15-01:43:
       * Toasts inherit --app-modal-background so Dark 1, Dark 2, and Light
       * keep transient modal-host feedback on the selected app surface.
       */}
      <Toaster
        offset={{ bottom: APP_MODAL_TOAST_BOTTOM_OFFSET_PX }}
        position='bottom-center'
        richColors
        theme='dark'
        toastOptions={{
          style: {
            background: 'var(--app-modal-background)',
            border: '1px solid rgba(255, 255, 255, 0.14)',
            color: '#f4f4f5',
          },
        }}
      />
    </>
  );
}

/**
 * CDXC:AppModals 2026-04-26-15:10
 * Sidebar-owned modals must render from a full-window host so settings and
 * other management dialogs center over the whole application instead of being
 * constrained by the narrow sidebar WKWebView.
 */
function useModalStateFromNative() {
  const [activeModal, setActiveModal] = useState<AppModalKind | undefined>();
  /*
   * CDXC:CommandPalette 2026-06-13-09:53:
   * Native command-palette prewarm opens the real modal host while hidden.
   * Preserve the request id through React state so the presented event lets
   * AppKit hide the warmed host instead of showing it to the user.
   */
  const [activeModalRequestId, setActiveModalRequestId] = useState<string>();
  const [agentHooksRequired, setAgentHooksRequired] = useState<AgentHooksRequiredModalState>();
  const [agentsHubCatalog, setAgentsHubCatalog] = useState<AgentsHubCatalogMessage>();
  const [agentsHubFileContent, setAgentsHubFileContent] = useState<AgentsHubFileContentMessage>();
  const [config, setConfig] = useState<ConfigModalState>({});
  const [delayedSend, setDelayedSend] = useState<DelayedSendModalState>();
  const [firstUserMessage, setFirstUserMessage] = useState<FirstUserMessageModalState>();
  const [gitCommit, setGitCommit] = useState<GitCommitModalDraft>();
  const [gitFileDiff, setGitFileDiff] = useState<GitFileDiffModalDraft>();
  const [worktreeDelete, setWorktreeDelete] = useState<WorktreeDeleteModalDraft>();
  const [worktreeRename, setWorktreeRename] = useState<WorktreeRenameModalDraft>();
  const [missingProjectFolder, setMissingProjectFolder] = useState<MissingProjectFolderModalState>();
  const [remoteGxserverInstall, setRemoteGxserverInstall] = useState<RemoteGxserverInstallState>();
  const [remoteProjectPicker, setRemoteProjectPicker] = useState<RemoteProjectPickerState>();
  const [addProject, setAddProject] = useState<AddProjectModalState>();
  const [recentProjects, setRecentProjects] = useState<RecentProjectsModalState>();
  const [renameSession, setRenameSession] = useState<RenameSessionModalState>();
  const [sessionNote, setSessionNote] = useState<SessionNoteModalState>();
  const [sidebarSpaceEditor, setSidebarSpaceEditor] = useState<SidebarSpaceEditorModalState>();
  const [stashedPrompts, setStashedPrompts] = useState<StashedPromptsModalState>();
  const [exportTranscriptResult, setExportTranscriptResult] = useState<ExportTranscriptResultModalState>();
  const [worktree, setWorktree] = useState<WorktreeModalState>();
  const [portlessSetup, setPortlessSetup] = useState<PortlessSetupModalState>();
  const [updateAvailable, setUpdateAvailable] = useState<UpdateAvailableModalState>();
  const [agentHookStatus, setAgentHookStatus] = useState<AgentHookStatusMessage>();
  const [commandPaletteInitialQuery, setCommandPaletteInitialQuery] = useState('');
  const [commandPaletteOpenRequestSequence, setCommandPaletteOpenRequestSequence] = useState(0);
  const [isCommandPalettePrewarm, setIsCommandPalettePrewarm] = useState(false);
  const [ghostexCliStatus, setGhostexCliStatus] = useState<GhostexCliStatusMessage>();
  const [ghostexFolderStats, setGhostexFolderStats] = useState<SidebarGhostexFolderStatsMessage>();
  const [osIntegrationStatus, setOSIntegrationStatus] = useState<OSIntegrationStatusMessage>();
  const [pluginSettingsStatus, setPluginSettingsStatus] = useState<PluginSettingsStatusMessage>();
  // CDXC:AppIconPicker 2026-06-25-21:50: Latest native App Icon state passed to Settings.
  const [appIconState, setAppIconState] = useState<AppIconStateMessage>();
  const [settingsInitialSection, setSettingsInitialSection] = useState<MainSettingsInitialSectionId>();
  const [settingsInitialRemoteMachineId, setSettingsInitialRemoteMachineId] = useState<string>();
  const [settingsInitialRemoteSection, setSettingsInitialRemoteSection] = useState<SettingsRemoteSection>();
  const [settingsInitialSearchQuery, setSettingsInitialSearchQuery] = useState<string>();
  const [settingsInitialTabOverride, setSettingsInitialTabOverride] = useState<SettingsModalTab>();
  const activeModalRef = useRef<AppModalKind | undefined>(activeModal);
  const toastTokenRef = useRef(0);

  const clearActiveModalState = useCallback(() => {
    setActiveModal(undefined);
    setActiveModalRequestId(undefined);
    setAgentHooksRequired(undefined);
    setConfig({});
    setDelayedSend(undefined);
    setFirstUserMessage(undefined);
    setGitCommit(undefined);
    setGitFileDiff(undefined);
    setWorktreeDelete(undefined);
    setWorktreeRename(undefined);
    setMissingProjectFolder(undefined);
    setRemoteGxserverInstall(undefined);
    setRemoteProjectPicker(undefined);
    setAddProject(undefined);
    setRecentProjects(undefined);
    setRenameSession(undefined);
    setSessionNote(undefined);
    setSidebarSpaceEditor(undefined);
    setStashedPrompts(undefined);
    setExportTranscriptResult(undefined);
    setWorktree(undefined);
    setPortlessSetup(undefined);
    setUpdateAvailable(undefined);
    setGhostexFolderStats(undefined);
    setOSIntegrationStatus(undefined);
    setPluginSettingsStatus(undefined);
    // CDXC:AppIconPicker 2026-06-25-21:50: Drop stale App Icon state when the modal closes.
    setAppIconState(undefined);
    setAgentsHubCatalog(undefined);
    setAgentsHubFileContent(undefined);
    setCommandPaletteInitialQuery('');
    setCommandPaletteOpenRequestSequence(0);
    setIsCommandPalettePrewarm(false);
    setSettingsInitialSection(undefined);
    setSettingsInitialRemoteMachineId(undefined);
    setSettingsInitialSearchQuery(undefined);
    setSettingsInitialTabOverride(undefined);
  }, []);

  const closeModal = useCallback(() => {
    /**
     * CDXC:AppModals 2026-05-22-16:55:
     * Modal controls such as Previous Sessions Escape and the X button must
     * dismiss the React dialog immediately, then notify native to hide the
     * transparent modal-host WKWebView. Do not require the native echo before
     * clearing visible modal state.
     */
    clearActiveModalState();
    notifyNativeModalClosed();
  }, [clearActiveModalState]);

  const completeFirstLaunchSetup = useCallback(() => {
    clearActiveModalState();
    notifyNativeFirstLaunchSetupCompleted();
  }, [clearActiveModalState]);

  const closeGitFileDiff = useCallback(() => {
    setGitFileDiff(undefined);
  }, []);

  /*
   * CDXC:ExportTranscriptOptions 2026-08-24:
   * The Export button's stage move. The sidebar runtime answers with
   * `exportSessionTranscriptResult`, which lands the dialog on done/failed.
   */
  const beginExportTranscriptExport = useCallback(() => {
    setExportTranscriptResult((current) => (current ? { ...current, stage: { stage: 'exporting' } } : current));
  }, []);

  useEffect(() => {
    activeModalRef.current = activeModal;
  }, [activeModal]);

  useEffect(() => {
    const handleMessage = (event: Event) => {
      try {
        const message = (event as CustomEvent<AppModalHostMessage>).detail;
        if (!message || typeof message !== 'object') {
          return;
        }

        if (message.type === 'open') {
          const hasInlineSidebarStateMessage = message.latestSidebarStateMessage !== undefined;
          const shouldApplyInlineSidebarState = shouldApplySidebarStateBeforeModalOpen(message.modal);
          if (shouldApplyInlineSidebarState && hasInlineSidebarStateMessage) {
            /*
             * CDXC:SettingsModalStuckBlank 2026-06-20-23:02:
             * Settings opens must apply the native window's latest sidebar
             * snapshot before setting activeModal. This keeps Debugging Mode,
             * revision, and settings data in the modal host before React decides
             * whether the Settings component can actually render.
             *
             * CDXC:FirstLaunchSetup 2026-06-29-13:46:
             * The first-launch setup modal uses the same hydrated settings store,
             * so it must receive the inline native snapshot before activeModal is
             * set and before native waits for the React presented acknowledgement.
             */
            applySidebarStateMessage(message.latestSidebarStateMessage);
          }
          const sidebarStateAtOpen = useSidebarStore.getState();
          if (isAppModalDebugLoggingEnabled()) {
            postAppModalHostMessage(
              {
                details: JSON.stringify({
                  hasSettings: sidebarStateAtOpen.hud.settings !== undefined,
                  inlineSidebarStateApplied: shouldApplyInlineSidebarState && hasInlineSidebarStateMessage,
                  modal: message.modal,
                  performanceNow: performance.now(),
                }),
                event: 'modalHost.open.received',
                type: 'debugLog',
              },
              'AppModals:debug'
            );
          }
          if (isSettingsModalKind(message.modal)) {
            postSettingsModalDebugLog('modalHost.settings.open.received', {
              activeModalBeforeOpen: activeModalRef.current ?? null,
              hasInitialRemoteMachineId:
                typeof message.initialRemoteMachineId === 'string' && message.initialRemoteMachineId.trim().length > 0,
              hasInitialSearchQuery: typeof message.initialSearchQuery === 'string',
              hasSettings: sidebarStateAtOpen.hud.settings !== undefined,
              hasInlineSidebarStateMessage: message.latestSidebarStateMessage !== undefined,
              initialSection: typeof message.initialSection === 'string' ? message.initialSection : null,
              initialTab: isSettingsModalTab(message.initialTab) ? message.initialTab : null,
              modal: message.modal,
              nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
              revision: sidebarStateAtOpen.revision,
            });
          }
          if (isFirstLaunchSetupModalKind(message.modal)) {
            /*
             * CDXC:FirstLaunchSetupDiagnostics 2026-06-29-22:08:
             * Capture the setup open boundary after any inline sidebar-state
             * hydrate has applied so a slow repro can tell whether React already
             * has settings state before renderability waits begin.
             */
            postAppModalDebugLog('modalHost.setup.open.received', {
              activeModalBeforeOpen: activeModalRef.current ?? null,
              hasInlineSidebarStateMessage,
              hasNativeSettingsHydrated: sidebarStateAtOpen.revision > 0,
              hasSettings: sidebarStateAtOpen.hud.settings !== undefined,
              inlineSidebarStateApplied: shouldApplyInlineSidebarState && hasInlineSidebarStateMessage,
              modal: message.modal,
              nativeWindowSurface: window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow',
              revision: sidebarStateAtOpen.revision,
            });
          }
          /*
           * CDXC:AddProject 2026-07-30:
           * Set alongside the other payload-only modals rather than inside the
           * open-message if/else chain: the dialog has no draft to validate, so
           * every non-addProject open simply clears it.
           */
          setAddProject(
            message.modal === 'addProject'
              ? {
                  machineId:
                    typeof message.machineId === 'string' && message.machineId.trim() ? message.machineId : undefined,
                }
              : undefined
          );
          setAgentHooksRequired(
            message.modal === 'agentHooksRequired' &&
              typeof message.agentId === 'string' &&
              message.agentId.trim() &&
              typeof message.agentName === 'string' &&
              message.agentName.trim() &&
              typeof message.hookAgentId === 'string' &&
              message.hookAgentId.trim()
              ? {
                  agentId: message.agentId,
                  agentName: message.agentName,
                  groupId: typeof message.groupId === 'string' && message.groupId.trim() ? message.groupId : undefined,
                  hookAgentId: message.hookAgentId,
                }
              : undefined
          );
          setRecentProjects(
            message.modal === 'recentProjects'
              ? {
                  machineId: typeof message.machineId === 'string' ? message.machineId : undefined,
                  machineName: typeof message.machineName === 'string' ? message.machineName : undefined,
                }
              : undefined
          );
          /*
           * CDXC:SessionAgentNotes 2026-08-24:
           * Set alongside the other payload-only modals rather than inside the
           * open-message if/else chain below: the dialog validates nothing of
           * its own, so every non-sessionNote open simply clears it. A note
           * open without a session id is dropped — the write would have no
           * target.
           */
          setSessionNote(
            message.modal === 'sessionNote' &&
              typeof message.sessionId === 'string' &&
              message.sessionId.trim().length > 0
              ? {
                  initialNote: typeof message.initialNote === 'string' ? message.initialNote : '',
                  projectId:
                    typeof message.projectId === 'string' && message.projectId.trim() ? message.projectId : undefined,
                  sessionId: message.sessionId,
                  sessionTitle:
                    typeof message.sessionTitle === 'string' && message.sessionTitle.trim()
                      ? message.sessionTitle
                      : undefined,
                }
              : undefined
          );
          setStashedPrompts(
            message.modal === 'stashedPrompts'
              ? {
                  initialScope: isStashedPromptsScope(message.initialScope) ? message.initialScope : undefined,
                  projectId:
                    typeof message.projectId === 'string' && message.projectId.trim() ? message.projectId : undefined,
                  sessionId:
                    typeof message.sessionId === 'string' && message.sessionId.trim() ? message.sessionId : undefined,
                }
              : undefined
          );
          setExportTranscriptResult(() => {
            if (message.modal !== 'exportTranscriptResult') {
              return undefined;
            }
            const agentId = typeof message.agentId === 'string' && message.agentId.trim() ? message.agentId : undefined;
            const canReveal = message.canReveal === true;
            const requestId =
              typeof message.requestId === 'string' && message.requestId.trim() ? message.requestId : undefined;
            // A done-stage open (path present) stays supported so a host that
            // already exported can show the result directly; the normal flow
            // opens on the include-toggle options stage.
            const stage: ExportTranscriptModalStage =
              typeof message.path === 'string' && message.path.trim()
                ? { agentId, canReveal, path: message.path, stage: 'done' }
                : { stage: 'options' };
            return { agentId, canReveal, requestId, stage };
          });
          setUpdateAvailable(
            message.modal === 'updateAvailable' &&
              typeof message.version === 'string' &&
              (message.state === 'available' || message.state === 'ready')
              ? {
                  notesMarkdown: typeof message.notesMarkdown === 'string' ? message.notesMarkdown : '',
                  portable: message.portable === true,
                  state: message.state,
                  version: message.version,
                }
              : undefined
          );
          if (message.modal === 'missingProjectFolder') {
            if (
              typeof message.projectId !== 'string' ||
              !message.projectId.trim() ||
              typeof message.projectName !== 'string' ||
              !message.projectName.trim() ||
              typeof message.projectPath !== 'string' ||
              !message.projectPath.trim()
            ) {
              throw new Error('Missing-project modal request is missing project details.');
            }
            setMissingProjectFolder({
              projectId: message.projectId,
              projectName: message.projectName,
              projectPath: message.projectPath,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'renameSession') {
            if (!message.sessionId) {
              throw new Error('Rename modal request is missing sessionId.');
            }
            setRenameSession({
              initialTitle: message.initialTitle ?? '',
              sessionAgentIcon: typeof message.sessionAgentIcon === 'string' ? message.sessionAgentIcon : undefined,
              sessionId: message.sessionId,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'sidebarSpaceEditor') {
            /*
             * CDXC:SidebarSpaces 2026-08-27:
             * Edit mode has to name a Space; create mode must not, or Save would
             * patch whichever Space id happened to be left on the message.
             */
            const spaceEditorMode = message.mode === 'edit' ? 'edit' : 'create';
            if (spaceEditorMode === 'edit' && (typeof message.spaceId !== 'string' || !message.spaceId.trim())) {
              throw new Error('Space editor request is missing spaceId.');
            }
            setSidebarSpaceEditor({
              ...(spaceEditorMode === 'create' &&
              typeof message.memberCollectionId === 'string' &&
              message.memberCollectionId.trim()
                ? { memberCollectionId: message.memberCollectionId }
                : {}),
              ...(spaceEditorMode === 'create' &&
              typeof message.memberProjectId === 'string' &&
              message.memberProjectId.trim()
                ? { memberProjectId: message.memberProjectId }
                : {}),
              mode: spaceEditorMode,
              ...(typeof message.remoteMachineId === 'string' && message.remoteMachineId.trim()
                ? { remoteMachineId: message.remoteMachineId }
                : {}),
              ...(spaceEditorMode === 'edit' ? { spaceId: message.spaceId } : {}),
              ...(typeof message.spaceColor === 'string' ? { spaceColor: message.spaceColor } : {}),
              ...(typeof message.spaceIcon === 'string' ? { spaceIcon: message.spaceIcon } : {}),
              ...(typeof message.spaceName === 'string' ? { spaceName: message.spaceName } : {}),
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'firstUserMessage') {
            if (typeof message.message !== 'string' || !message.message.trim()) {
              throw new Error('First message modal request is missing message text.');
            }
            setFirstUserMessage({
              message: message.message,
              title: typeof message.title === 'string' ? message.title : undefined,
            });
            setConfig({});
            setDelayedSend(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'remoteGxserverInstall') {
            if (
              typeof message.remoteMachineId !== 'string' ||
              !message.remoteMachineId.trim() ||
              typeof message.remoteMachineName !== 'string' ||
              !message.remoteMachineName.trim()
            ) {
              throw new Error('Remote gxserver install request is missing machine details.');
            }
            /*
             * CDXC:RemoteMachines 2026-06-23-08:30:
             * SSH-reachable Ubuntu and macOS machines that are missing gxserver
             * must keep install approval state populated so Remote Settings
             * shows the Install gxserver button instead of only the warning
             * toast that explains the missing daemon.
             */
            setRemoteGxserverInstall({
              remoteMachineId: message.remoteMachineId,
              remoteMachineName: message.remoteMachineName,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'remoteProjectPicker') {
            if (
              typeof message.remoteMachineId !== 'string' ||
              !message.remoteMachineId.trim() ||
              typeof message.remoteMachineName !== 'string' ||
              !message.remoteMachineName.trim()
            ) {
              throw new Error('Remote project picker request is missing machine details.');
            }
            /*
             * CDXC:RemoteProjectPicker 2026-06-03-00:18:
             * Remote machine Add Project opens in the full-window modal host
             * with the selected machine carried as immutable request state.
             * Directory browsing remains machine-scoped through native so the
             * picker cannot accidentally browse local folders.
             */
            setRemoteProjectPicker({
              initialQuery: typeof message.initialQuery === 'string' ? message.initialQuery : undefined,
              remoteMachineId: message.remoteMachineId,
              remoteMachineName: message.remoteMachineName,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'delayedSend') {
            if (!message.sessionId) {
              throw new Error('Delayed Actions modal request is missing sessionId.');
            }
            setDelayedSend({
              agentIcon: message.agentIcon,
              closeAfterDoneActive:
                typeof message.closeAfterDoneActive === 'boolean' ? message.closeAfterDoneActive : undefined,
              delayedSendDeadlineAt:
                typeof message.delayedSendDeadlineAt === 'string' ? message.delayedSendDeadlineAt : undefined,
              delayedSendRemainingLabel:
                typeof message.delayedSendRemainingLabel === 'string' ? message.delayedSendRemainingLabel : undefined,
              sendWhenAllProjectSessionsStopActive: message.sendWhenAllProjectSessionsStopActive === true,
              sendWhenAgentStopsActive: message.sendWhenAgentStopsActive === true,
              sessionId: message.sessionId,
              supportsSendWhenAgentStops: message.supportsSendWhenAgentStops === true,
              supportsSendWhenAllProjectSessionsStop: message.supportsSendWhenAllProjectSessionsStop === true,
              title: typeof message.title === 'string' ? message.title : undefined,
            });
            setConfig({});
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'worktree') {
            setWorktree({
              projectId: typeof message.projectId === 'string' ? message.projectId : undefined,
              projectName: typeof message.projectName === 'string' ? message.projectName : undefined,
              projectPath: typeof message.projectPath === 'string' ? message.projectPath : undefined,
              remoteMachineId: typeof message.remoteMachineId === 'string' ? message.remoteMachineId : undefined,
              remoteMachineName: typeof message.remoteMachineName === 'string' ? message.remoteMachineName : undefined,
            });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setGitCommit(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'portlessSetup') {
            if (message.mode !== 'firstSetup' && message.mode !== 'standaloneReconfigure') {
              throw new Error('Portless setup modal request is missing setup mode.');
            }
            if (message.protocol !== 'https' && message.protocol !== 'http') {
              throw new Error('Portless setup modal request is missing protocol.');
            }
            setPortlessSetup({ mode: message.mode, protocol: message.protocol });
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setGitCommit(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'deleteWorktree') {
            if (!message.worktreeDeleteDraft) {
              throw new Error('Delete worktree modal request is missing worktreeDeleteDraft.');
            }
            setWorktreeDelete(message.worktreeDeleteDraft);
            setWorktreeRename(undefined);
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setGitCommit(undefined);
          } else if (message.modal === 'renameWorktree') {
            if (!message.worktreeRenameDraft) {
              throw new Error('Rename worktree modal request is missing worktreeRenameDraft.');
            }
            setWorktreeRename(message.worktreeRenameDraft);
            setWorktreeDelete(undefined);
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setGitCommit(undefined);
          } else if (message.modal === 'gitCommit') {
            if (!message.gitCommitDraft) {
              throw new Error('Git commit modal request is missing gitCommitDraft.');
            }
            setGitCommit(message.gitCommitDraft);
            setGitFileDiff(undefined);
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else if (message.modal === 'gitFileDiff') {
            if (!message.gitFileDiff) {
              throw new Error('Git file diff modal request is missing gitFileDiff.');
            }
            setGitFileDiff(message.gitFileDiff);
            return;
          } else if (message.modal === 'agentConfig') {
            if (!message.agentDraft) {
              throw new Error('Agent config modal request is missing agentDraft.');
            }
            setConfig({ agentDraft: message.agentDraft });
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          } else {
            setConfig({});
            setDelayedSend(undefined);
            setFirstUserMessage(undefined);
            setRemoteGxserverInstall(undefined);
            setRemoteProjectPicker(undefined);
            setRenameSession(undefined);
            setWorktree(undefined);
            setPortlessSetup(undefined);
            setWorktreeDelete(undefined);
            setWorktreeRename(undefined);
          }
          if (message.modal === 'settings') {
            setGhostexFolderStats(undefined);
            setSettingsInitialSection(typeof message.initialSection === 'string' ? message.initialSection : undefined);
            /**
             * CDXC:SessionPersistence 2026-06-04-02:52:
             * Titlebar Tips notices can open Settings directly to a searchable
             * tab and pre-fill the query with a setting name. Carry that state
             * through the full-window modal host instead of requiring titlebar
             * code to know the Settings DOM.
             */
            setSettingsInitialSearchQuery(
              typeof message.initialSearchQuery === 'string' ? message.initialSearchQuery : undefined
            );
            /**
             * CDXC:RemoteMachines 2026-06-10-09:54:
             * Sidebar Remote machine Edit opens Settings directly on the Remote
             * tab and carries the selected machine id so the modal can scroll to
             * and focus that machine's editable fields.
             */
            setSettingsInitialRemoteMachineId(
              typeof message.initialRemoteMachineId === 'string' && message.initialRemoteMachineId.trim()
                ? message.initialRemoteMachineId
                : undefined
            );
            setSettingsInitialRemoteSection(
              message.initialRemoteSection === 'easyConnect' || message.initialRemoteSection === 'tailscale'
                ? message.initialRemoteSection
                : undefined
            );
            setSettingsInitialTabOverride(isSettingsModalTab(message.initialTab) ? message.initialTab : undefined);
          } else {
            setSettingsInitialSection(undefined);
            setSettingsInitialRemoteMachineId(undefined);
            setSettingsInitialRemoteSection(undefined);
            setSettingsInitialSearchQuery(undefined);
            setSettingsInitialTabOverride(undefined);
          }
          if (message.modal === 'commandPalette') {
            /*
             * CDXC:CommandPalette 2026-06-13-22:18:
             * The Commands tab owns only command fuzzy finding. Preserve an
             * optional caller query as normal search text; Recent Sessions is
             * selected through its own modal id instead of a query prefix.
             *
             * CDXC:CommandPalette 2026-06-15-10:27:
             * Increment a request sequence for every Commands open so React can
             * refocus and apply the requested command query on repeat opens.
             */
            setCommandPaletteInitialQuery(typeof message.initialQuery === 'string' ? message.initialQuery : '');
            setCommandPaletteOpenRequestSequence((sequence) => sequence + 1);
            setIsCommandPalettePrewarm(message.prewarm === true);
          } else {
            setCommandPaletteInitialQuery('');
            setCommandPaletteOpenRequestSequence(0);
            setIsCommandPalettePrewarm(false);
          }
          if (message.modal !== 'agentsHub') {
            setAgentsHubCatalog(undefined);
            setAgentsHubFileContent(undefined);
          }
          setActiveModalRequestId(typeof message.requestId === 'string' ? message.requestId : undefined);
          setActiveModal(message.modal);
          return;
        }

        if (message.type === 'exportSessionTranscriptResult') {
          /*
           * CDXC:ExportTranscriptOptions 2026-08-24:
           * Answers only the dialog that asked: the runtime posts this while
           * the Export Transcript dialog sits on its exporting stage, so a
           * result arriving after the user closed it is dropped.
           */
          if (activeModalRef.current !== 'exportTranscriptResult') {
            return;
          }
          setExportTranscriptResult((current) => {
            if (!current || current.requestId !== message.requestId) {
              return current;
            }
            if (message.ok && typeof message.path === 'string' && message.path.trim()) {
              const agentId =
                typeof message.agentId === 'string' && message.agentId.trim() ? message.agentId : current.agentId;
              const canReveal = message.canReveal === true;
              return {
                agentId,
                canReveal,
                requestId: current.requestId,
                stage: { agentId, canReveal, path: message.path, stage: 'done' },
              };
            }
            return {
              ...current,
              stage: {
                message:
                  typeof message.error === 'string' && message.error.trim()
                    ? message.error
                    : 'The transcript export failed.',
                stage: 'failed',
              },
            };
          });
          return;
        }

        if (message.type === 'close') {
          if (isAppModalDebugLoggingEnabled()) {
            postAppModalHostMessage(
              {
                details: JSON.stringify({ performanceNow: performance.now() }),
                event: 'modalHost.close.received',
                type: 'debugLog',
              },
              'AppModals:debug'
            );
          }
          clearActiveModalState();
          return;
        }

        if (message.type === 'toast') {
          /**
           * CDXC:Worktrees 2026-06-02-15:27:
           * Git and worktree command execution belongs to gxserver after the ownership split. The app-modal host owns only the visible toast surface, so gxserver-backed progress feedback appears over the full Ghostex window without stealing focus from terminal panes.
           *
           * CDXC:GitActionModel 2026-05-30-05:34:
           * Long-running Git actions and agent workflows need persistent status
           * toasts. Reuse Sonner ids so native can update a running toast to a
           * success or error state instead of stacking transient progress notices.
           *
           * CDXC:GitActionToasts 2026-05-30-06:39:
           * Persistent Git/worktree toasts need an explicit spinner, error
           * toasts need a red-tinted surface, and success toasts need a subtle
           * green tint so users can distinguish completion states even when the
           * toast copy is partially clipped.
           */
          toastTokenRef.current += 1;
          const toastToken = toastTokenRef.current;
          const isPersistent = message.persistent === true;
          const toastDescription = normalizeAppToastDescription(
            message.title,
            typeof message.description === 'string' ? message.description : undefined
          );
          const toastClassName = [
            'ghostex-app-toast',
            isPersistent ? 'ghostex-app-toast-persistent' : '',
            message.level === 'error' ? 'ghostex-app-toast-error' : '',
            message.level === 'success' ? 'ghostex-app-toast-success' : '',
          ]
            .filter(Boolean)
            .join(' ');
          const toastOptions = {
            action: message.action
              ? {
                  label: message.action.label,
                  onClick: () => {
                    if (message.action) {
                      vscode.postMessage(message.action.sidebarMessage);
                    }
                  },
                }
              : undefined,
            className: toastClassName,
            description: toastDescription,
            duration: isPersistent ? Number.POSITIVE_INFINITY : undefined,
            id: message.toastId,
            style:
              message.level === 'error'
                ? {
                    background:
                      'linear-gradient(0deg, rgba(95, 24, 31, 0.28), rgba(95, 24, 31, 0.28)), var(--app-modal-background)',
                    border: '1px solid rgba(248, 113, 113, 0.32)',
                    color: '#fff1f2',
                  }
                : message.level === 'success'
                  ? {
                      background:
                        'linear-gradient(0deg, rgba(22, 101, 52, 0.24), rgba(22, 101, 52, 0.24)), var(--app-modal-background)',
                      border: '1px solid rgba(74, 222, 128, 0.3)',
                      color: '#f0fdf4',
                    }
                  : undefined,
          };
          if (message.level === 'error') {
            toast.error(message.title, toastOptions);
          } else if (message.level === 'warning') {
            toast.warning(message.title, toastOptions);
          } else if (message.level === 'success') {
            toast.success(message.title, toastOptions);
          } else {
            toast.message(message.title, toastOptions);
          }
          if (isPersistent) {
            return;
          }
          window.setTimeout(() => {
            if (toastToken !== toastTokenRef.current) {
              return;
            }
            postAppModalHostMessage(
              { keepOpen: activeModalRef.current !== undefined, type: 'toastDismissed' },
              'AppModals:toastDismissed'
            );
          }, 4_200);
          return;
        }

        if (message.type === 'sidebarState') {
          if (isAgentsHubCatalogMessage(message.message)) {
            setAgentsHubCatalog(message.message);
            setAgentsHubFileContent(undefined);
            return;
          }
          if (isAgentsHubFileContentMessage(message.message)) {
            setAgentsHubFileContent(message.message);
            return;
          }
          if (isGhostexFolderStatsMessage(message.message)) {
            setGhostexFolderStats(message.message);
            return;
          }
          if (isAgentHookStatusMessage(message.message)) {
            setAgentHookStatus(message.message);
            return;
          }
          if (isGhostexCliStatusMessage(message.message)) {
            setGhostexCliStatus(message.message);
            return;
          }
          if (isOSIntegrationStatusMessage(message.message)) {
            setOSIntegrationStatus(message.message);
            return;
          }
          if (isPluginSettingsStatusMessage(message.message)) {
            setPluginSettingsStatus(message.message);
            return;
          }
          // CDXC:AppIconPicker 2026-06-25-21:50: Route relayed App Icon state into Settings modal state.
          if (isAppIconStateMessage(message.message)) {
            setAppIconState(message.message);
            return;
          }
          if (
            isPreviousSessionsResultMessage(message.message) ||
            isSessionTranscriptSizesResultMessage(message.message) ||
            isRecentProjectsResultMessage(message.message) ||
            isStashedPromptsTransientMessage(message.message)
          ) {
            window.postMessage(message.message, '*');
            return;
          }
          applySidebarStateMessage(message.message);
        }
      } catch (error) {
        logAppModalError('AppModals:hostMessage', error);
        throw error;
      }
    };

    window.addEventListener('ghostex-app-modal-host-message', handleMessage);
    postAppModalHostMessage(
      { nativeWindowHostId: window.__ghostex_APP_MODAL_HOST_ID__, type: 'ready' },
      'AppModals:ready'
    );
    /*
     * CDXC:AppModals 2026-06-11-19:46:
     * Native child windows reuse modal-host.html for the app modal family.
     */
    return () => {
      window.removeEventListener('ghostex-app-modal-host-message', handleMessage);
    };
  }, []);

  return {
    activeModal,
    activeModalRequestId,
    addProject,
    agentHooksRequired,
    agentsHubCatalog,
    agentsHubFileContent,
    config,
    delayedSend,
    firstUserMessage,
    gitCommit,
    gitFileDiff,
    worktreeDelete,
    worktreeRename,
    missingProjectFolder,
    commandPaletteInitialQuery,
    commandPaletteOpenRequestSequence,
    isCommandPalettePrewarm,
    closeGitFileDiff,
    closeModal,
    completeFirstLaunchSetup,
    recentProjects,
    remoteProjectPicker,
    renameSession,
    sessionNote,
    sidebarSpaceEditor,
    stashedPrompts,
    beginExportTranscriptExport,
    exportTranscriptResult,
    updateAvailable,
    remoteGxserverInstall,
    worktree,
    portlessSetup,
    agentHookStatus,
    ghostexCliStatus,
    ghostexFolderStats,
    osIntegrationStatus,
    pluginSettingsStatus,
    // CDXC:AppIconPicker 2026-06-25-21:50: Expose App Icon state to the modal component.
    appIconState,
    settingsInitialSection,
    settingsInitialRemoteMachineId,
    settingsInitialRemoteSection,
    settingsInitialSearchQuery,
    settingsInitialTabOverride,
  };
}

function isAgentHookStatusMessage(message: unknown): message is SidebarAgentHookStatusMessage {
  return Boolean(message && typeof message === 'object' && 'type' in message && message.type === 'agentHookStatus');
}

function isGhostexCliStatusMessage(message: unknown): message is SidebarGhostexCliStatusMessage {
  return Boolean(message && typeof message === 'object' && 'type' in message && message.type === 'ghostexCliStatus');
}

function isGhostexFolderStatsMessage(message: unknown): message is SidebarGhostexFolderStatsMessage {
  return Boolean(message && typeof message === 'object' && 'type' in message && message.type === 'ghostexFolderStats');
}

function isOSIntegrationStatusMessage(message: unknown): message is SidebarOSIntegrationStatusMessage {
  return Boolean(message && typeof message === 'object' && 'type' in message && message.type === 'osIntegrationStatus');
}

function isPluginSettingsStatusMessage(message: unknown): message is SidebarPluginSettingsStatusMessage {
  return Boolean(
    message && typeof message === 'object' && 'type' in message && message.type === 'pluginSettingsStatus'
  );
}

// CDXC:AppIconPicker 2026-06-25-21:50: Narrow relayed sidebarState payloads to the App Icon contract.
function isAppIconStateMessage(message: unknown): message is SidebarAppIconStateMessage {
  return Boolean(message && typeof message === 'object' && 'type' in message && message.type === 'appIconState');
}

// CDXC:StashedPromptSessionAssociation 2026-08-24: Narrow a launcher-pinned origin filter to the modal's scope vocabulary.
function isStashedPromptsScope(value: unknown): value is StashedPromptsScope {
  return value === 'all' || value === 'project' || value === 'session';
}

function isStashedPromptsTransientMessage(message: unknown): message is Extract<
  ExtensionToSidebarMessage,
  {
    type: 'saveStashedPromptResult' | 'setStashedPromptTagsResult' | 'stashedPromptTagsResult' | 'stashedPromptsResult';
  }
> {
  /*
   * CDXC:StashedPrompts 2026-07-29:
   * Stashed-prompt query answers are transient sidebarState payloads. Forward
   * them to the Prompts modal as window messages instead of storing prompt
   * bodies in the reusable modal-host hydrate snapshot.
   *
   * CDXC:StashedPromptTags 2026-08-24:
   * The two tag answers belong in the same relay. They were missing, so every
   * tag mutation made from the GPUI modal host — create, delete, file a prompt
   * under a tag — was answered into a window message the modal never received:
   * the rail only refreshed on the next full reopen, and a failed mutation
   * reported no error at all.
   */
  return Boolean(
    message &&
    typeof message === 'object' &&
    'type' in message &&
    (message.type === 'stashedPromptsResult' ||
      message.type === 'saveStashedPromptResult' ||
      message.type === 'stashedPromptTagsResult' ||
      message.type === 'setStashedPromptTagsResult')
  );
}

function isPreviousSessionsResultMessage(
  message: unknown
): message is Extract<ExtensionToSidebarMessage, { type: 'previousSessionsResult' }> {
  /*
  CDXC:PreviousSessionsModal 2026-06-01-22:01:
  The full-window Previous Sessions modal lives in the app modal host WebView, while gxserver previous-session queries are requested through the native sidebar bridge. Forward the result as a normal window message so the shared modal component receives the same response path it uses inside the sidebar WebView.
  */
  return Boolean(
    message && typeof message === 'object' && 'type' in message && message.type === 'previousSessionsResult'
  );
}

function isSessionTranscriptSizesResultMessage(
  message: unknown
): message is Extract<ExtensionToSidebarMessage, { type: 'sessionTranscriptSizesResult' }> {
  /*
  CDXC:QuickAccessSessionSizes 2026-08-28:
  Transcript sizes are transient answers owned by PreviousSessionsModal, not
  persistent sidebar store state. Relay them through the modal window just like
  the paged previous-session result so the request can leave its loading state.
  */
  return Boolean(
    message && typeof message === 'object' && 'type' in message && message.type === 'sessionTranscriptSizesResult'
  );
}

function isRecentProjectsResultMessage(
  message: unknown
): message is Extract<ExtensionToSidebarMessage, { type: 'recentProjectsResult' }> {
  return Boolean(
    message && typeof message === 'object' && 'type' in message && message.type === 'recentProjectsResult'
  );
}

function isAgentsHubCatalogMessage(message: unknown): message is AgentsHubCatalogMessage {
  return Boolean(message && typeof message === 'object' && 'type' in message && message.type === 'agentsHubCatalog');
}

function isAgentsHubFileContentMessage(message: unknown): message is AgentsHubFileContentMessage {
  return Boolean(
    message && typeof message === 'object' && 'type' in message && message.type === 'agentsHubFileContent'
  );
}

function createEmptyAgentDraft(): AgentConfigDraft {
  return {
    command: '',
    name: '',
  };
}

function isModalRenderable({
  activeModal,
  addProject,
  agentHooksRequired,
  config,
  delayedSend,
  firstUserMessage,
  gitCommit,
  gitFileDiff,
  worktreeDelete,
  worktreeRename,
  missingProjectFolder,
  recentProjects,
  remoteProjectPicker,
  remoteGxserverInstall,
  renameSession,
  sessionNote,
  sidebarSpaceEditor,
  stashedPrompts,
  exportTranscriptResult,
  updateAvailable,
  settings,
  worktree,
  portlessSetup,
}: {
  activeModal: AppModalKind | undefined;
  addProject: AddProjectModalState | undefined;
  agentHooksRequired: AgentHooksRequiredModalState | undefined;
  config: ConfigModalState;
  delayedSend: DelayedSendModalState | undefined;
  firstUserMessage: FirstUserMessageModalState | undefined;
  gitCommit: GitCommitModalDraft | undefined;
  gitFileDiff: GitFileDiffModalDraft | undefined;
  worktreeDelete: WorktreeDeleteModalDraft | undefined;
  worktreeRename: WorktreeRenameModalDraft | undefined;
  missingProjectFolder: MissingProjectFolderModalState | undefined;
  recentProjects: RecentProjectsModalState | undefined;
  remoteProjectPicker: RemoteProjectPickerState | undefined;
  remoteGxserverInstall: RemoteGxserverInstallState | undefined;
  renameSession: RenameSessionModalState | undefined;
  sessionNote: SessionNoteModalState | undefined;
  sidebarSpaceEditor: SidebarSpaceEditorModalState | undefined;
  stashedPrompts: StashedPromptsModalState | undefined;
  exportTranscriptResult: ExportTranscriptResultModalState | undefined;
  updateAvailable: UpdateAvailableModalState | undefined;
  settings: unknown;
  worktree: WorktreeModalState | undefined;
  portlessSetup: PortlessSetupModalState | undefined;
}): boolean {
  switch (activeModal) {
    case undefined:
      return false;
    case 'addProject':
      return addProject !== undefined;
    case 'agentConfig':
      return config.agentDraft !== undefined;
    case 'agentHooksRequired':
      return agentHooksRequired !== undefined;
    case 'agentsHub':
    case 'commandPalette':
      return true;
    case 'delayedSend':
      return delayedSend !== undefined;
    case 'firstUserMessage':
      return firstUserMessage !== undefined;
    case 'gitCommit':
      return gitCommit !== undefined;
    case 'gitFileDiff':
      return gitFileDiff !== undefined;
    case 'missingProjectFolder':
      return missingProjectFolder !== undefined;
    case 'deleteWorktree':
      return worktreeDelete !== undefined;
    case 'renameWorktree':
      return worktreeRename !== undefined;
    case 'recentProjects':
      return recentProjects !== undefined;
    case 'remoteProjectPicker':
      return remoteProjectPicker !== undefined;
    case 'remoteGxserverInstall':
      return remoteGxserverInstall !== undefined;
    case 'renameSession':
      return renameSession !== undefined;
    case 'sessionNote':
      return sessionNote !== undefined;
    case 'sidebarSpaceEditor':
      return sidebarSpaceEditor !== undefined;
    case 'stashedPrompts':
      return stashedPrompts !== undefined;
    case 'exportTranscriptResult':
      return exportTranscriptResult !== undefined;
    case 'updateAvailable':
      return updateAvailable !== undefined;
    case 'settings':
    case 'configureActions':
    case 'configureAgents':
    case 'hotkeys':
    case 'openTargets':
      return settings !== undefined;
    case 'worktree':
      return worktree !== undefined;
    case 'portlessSetup':
      return portlessSetup !== undefined;
    case 'previousSessions':
    case 'discoverGhostex':
    case 'remoteSetup':
    case 'watchGhostexVideo':
    case 'tipsAndTricks':
    case 'firstLaunchSetup':
      return true;
  }
}

function applySidebarStateMessage(message: unknown) {
  if (!message || typeof message !== 'object' || !('type' in message)) {
    return;
  }

  if (message.type === 'hydrate' || message.type === 'sessionState') {
    useSidebarStore
      .getState()
      .applySidebarMessage(
        message as Parameters<ReturnType<typeof useSidebarStore.getState>['applySidebarMessage']>[0]
      );
    return;
  }
}

document.body.classList.add('app-modal-host-body');
if (window.__ghostex_APP_MODAL_HOST_SURFACE__ === 'nativeWindow') {
  document.documentElement.classList.add('app-modal-host-native-window-document');
  document.body.classList.add('app-modal-host-native-window-body');
  /*
   * CDXC:GPUIAppModalScroll 2026-07-26-07:55:
   * GPUI child windows fit to the one-shot measured dialog height and then
   * keep that frame for the rest of the open. Mark that host so the
   * stylesheet can bound growable regions (long pasted rename text, long
   * prompts) and scroll them instead of pushing the title row and action row
   * outside the window, and so the duplicated in-dialog close button stays
   * hidden in these native child windows.
   */
  if (window.__ghostex_APP_MODAL_HOST_ID__ === GPUI_APP_MODAL_HOST_ID) {
    document.body.dataset.appModalFixedWindow = 'true';
  }
}
installAppModalGlobalErrorLogging('AppModals:modalHost');
createRoot(document.getElementById('root')!).render(<AppModalHost />);
