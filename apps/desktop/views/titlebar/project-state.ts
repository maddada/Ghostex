import {
  KEEP_AWAKE_DURATION_OPTIONS,
  getSidebarTitlebarForegroundForBackground,
  normalizeghostexSettings,
  type KeepAwakeDurationMinutes,
} from '@/packages/shared/ghostex-settings';
import { normalizeghostexHotkeySettings } from '@/packages/shared/ghostex-hotkeys';
import { resolveSidebarTheme } from '@/packages/shared/session-grid-contract';
import { createDefaultSidebarProjectDiffStats } from '@/packages/shared/project-diff-stats';
import { formatSidebarHotkeyLabel } from '@/packages/core-ui/hotkey-label';
import {
  createDefaultSidebarGitState,
  type SidebarGitAction,
  type SidebarGitState,
} from '@/packages/shared/sidebar-git';
import {
  KEEP_AWAKE_RUNTIME_CHANGED_EVENT,
  KEEP_AWAKE_RUNTIME_STORAGE_KEY,
  KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY,
  TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX,
  TITLEBAR_TIPS_READ_STORAGE_KEY,
} from './constants';
import {
  normalizeTitlebarUpdateDownloadProgress,
  runNativeKeepAwakeLidSleepPrevention,
  runNativeProcess,
} from './native-bridge';
import { isRecord, normalizeTitlebarMode, parseSharedSettings, resolveInitialTitlebarMode } from './settings-io';
import type {
  KeepAwakeRuntimeState,
  KeepAwakeRuntimeSyncState,
  TitlebarKeepAwakeSettings,
  TitlebarProjectState,
} from './types';

export function mergeTitlebarProjectState(
  current: TitlebarProjectState,
  state: Partial<TitlebarProjectState>
): TitlebarProjectState {
  const customSidebarTitlebarBackgroundColor =
    state.customSidebarTitlebarBackgroundColor ?? current.customSidebarTitlebarBackgroundColor;
  const projectIdentity = {
    projectId: state.projectId ?? current.projectId,
    projectPath: state.projectPath ?? current.projectPath,
  };
  return {
    ...current,
    ...state,
    activeMode: state.activeMode === undefined ? current.activeMode : normalizeTitlebarMode(state.activeMode),
    agentHookStatus: state.agentHookStatus ?? current.agentHookStatus,
    ghostexCliStatus: state.ghostexCliStatus ?? current.ghostexCliStatus,
    portless: state.portless ?? current.portless,
    debuggingMode: state.debuggingMode ?? current.debuggingMode,
    diagnosticLogging:
      state.diagnosticLogging === undefined
        ? current.diagnosticLogging
        : normalizeghostexSettings({ diagnosticLogging: state.diagnosticLogging }).diagnosticLogging,
    showBetaFeatures: state.showBetaFeatures ?? current.showBetaFeatures,
    diffStats: state.diffStats ?? current.diffStats,
    git: resolveTitlebarGitStateForMerge(current.git, state.git, projectIdentity),
    gxserverDaemon: state.gxserverDaemon ?? current.gxserverDaemon,
    hotkeys: normalizeghostexHotkeySettings(state.hotkeys ?? current.hotkeys),
    keepAwake: state.keepAwake ?? current.keepAwake,
    browserTabs: state.browserTabs ?? current.browserTabs,
    codeEditorProjectIds: state.codeEditorProjectIds ?? current.codeEditorProjectIds,
    projectEditorCompanionPaneHidden:
      state.projectEditorCompanionPaneHidden ?? current.projectEditorCompanionPaneHidden,
    projectIsQuick: state.projectIsQuick ?? current.projectIsQuick,
    petOverlayEnabled: state.petOverlayEnabled ?? current.petOverlayEnabled,
    resourceGroups: state.resourceGroups ?? current.resourceGroups,
    sidebarTheme: state.sidebarTheme ?? current.sidebarTheme,
    customSidebarTitlebarForegroundColor: getSidebarTitlebarForegroundForBackground(
      customSidebarTitlebarBackgroundColor
    ),
    customSidebarTitlebarBackgroundColor,
    sidebarActions: state.sidebarActions ?? current.sidebarActions,
    sidebarSide: state.sidebarSide ?? current.sidebarSide,
    toggleSidebarHotkeyLabel: state.toggleSidebarHotkeyLabel ?? current.toggleSidebarHotkeyLabel,
    workspaceOpenTargets: state.workspaceOpenTargets ?? current.workspaceOpenTargets,
    isFocusModeActive: state.isFocusModeActive ?? current.isFocusModeActive,
    promptEditorOpen: state.promptEditorOpen ?? current.promptEditorOpen,
    updateAvailable: state.updateAvailable ?? current.updateAvailable,
    updateDownloadProgress: Object.prototype.hasOwnProperty.call(state, 'updateDownloadProgress')
      ? normalizeTitlebarUpdateDownloadProgress(state.updateDownloadProgress)
      : current.updateDownloadProgress,
    updateDownloading: state.updateDownloading ?? current.updateDownloading,
  };
}

export function resolveTitlebarGitStateForMerge(
  current: SidebarGitState,
  incoming: SidebarGitState | undefined,
  projectIdentity: Pick<TitlebarProjectState, 'projectId' | 'projectPath'>
): SidebarGitState {
  const cached = readCachedTitlebarGitState(projectIdentity);
  if (incoming === undefined) {
    return shouldHydrateMissingTitlebarGitStateFromCache(current, cached) ? cached : current;
  }
  if (shouldUseCachedTitlebarGitState(incoming, cached)) {
    /*
     * CDXC:TitlebarGit 2026-06-16-19:19:
     * Git refresh publishes a transient busy/default state before branch and
     * diff probes finish. Keep the last cached project Git snapshot visible
     * during that refresh so titlebar dropdowns do not flash detached/default
     * metadata before the real branch result arrives.
     */
    return {
      ...cached,
      confirmSuggestedCommit: incoming.confirmSuggestedCommit,
      generateCommitBody: incoming.generateCommitBody,
      isBusy: incoming.isBusy,
      primaryAction: incoming.primaryAction,
    };
  }
  return incoming;
}

export function shouldHydrateMissingTitlebarGitStateFromCache(
  current: SidebarGitState,
  cached: SidebarGitState | undefined
): cached is SidebarGitState {
  return cached !== undefined && !isCacheableTitlebarGitState(current);
}

export function shouldUseCachedTitlebarGitState(
  incoming: SidebarGitState,
  cached: SidebarGitState | undefined
): cached is SidebarGitState {
  return (
    cached !== undefined && incoming.isBusy && incoming.branch === null && (cached.branch !== null || cached.isRepo)
  );
}

export function cacheTitlebarGitState(state: TitlebarProjectState): void {
  const cacheKey = titlebarGitStateCacheKey(state);
  if (cacheKey === undefined || state.git.isBusy || !isCacheableTitlebarGitState(state.git)) {
    return;
  }
  localStorage.setItem(cacheKey, JSON.stringify(state.git));
}

export function isCacheableTitlebarGitState(state: SidebarGitState): boolean {
  return state.isRepo || state.hasCheckedGitHubRemote || state.branch !== null || state.files.length > 0;
}

export function readCachedTitlebarGitState(
  projectIdentity: Pick<TitlebarProjectState, 'projectId' | 'projectPath'>
): SidebarGitState | undefined {
  const cacheKey = titlebarGitStateCacheKey(projectIdentity);
  if (cacheKey === undefined) {
    return undefined;
  }
  try {
    const parsed = JSON.parse(localStorage.getItem(cacheKey) || 'null');
    return normalizeCachedTitlebarGitState(parsed);
  } catch {
    return undefined;
  }
}

export function normalizeCachedTitlebarGitState(value: unknown): SidebarGitState | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const baseState = createDefaultSidebarGitState();
  return {
    ...baseState,
    additions: readCachedTitlebarNumber(value.additions),
    aheadCount: readCachedTitlebarNumber(value.aheadCount),
    behindCount: readCachedTitlebarNumber(value.behindCount),
    branch: typeof value.branch === 'string' ? value.branch : null,
    confirmSuggestedCommit: value.confirmSuggestedCommit === true,
    deletions: readCachedTitlebarNumber(value.deletions),
    generateCommitBody: value.generateCommitBody !== false,
    hasCheckedGitHubRemote: value.hasCheckedGitHubRemote === true,
    hasGitHubCli: value.hasGitHubCli === true,
    hasGitHubRemote: value.hasGitHubRemote === true,
    hasOriginRemote: value.hasOriginRemote === true,
    hasUpstream: value.hasUpstream === true,
    hasWorkingTreeChanges: value.hasWorkingTreeChanges === true,
    files: normalizeCachedTitlebarGitFiles(value.files),
    isBusy: value.isBusy === true,
    isRepo: value.isRepo === true,
    isWorktree: value.isWorktree === true,
    pr: normalizeCachedTitlebarGitPullRequest(value.pr),
    primaryAction: normalizeCachedTitlebarGitAction(value.primaryAction, baseState.primaryAction),
    worktreeName: typeof value.worktreeName === 'string' ? value.worktreeName : undefined,
  };
}

export function normalizeCachedTitlebarGitAction(value: unknown, fallback: SidebarGitAction): SidebarGitAction {
  return value === 'commit' ||
    value === 'push' ||
    value === 'pr' ||
    value === 'syncRemote' ||
    value === 'syncMain' ||
    value === 'multiRelease' ||
    value === 'release'
    ? value
    : fallback;
}

export function normalizeCachedTitlebarGitPullRequest(value: unknown): SidebarGitState['pr'] {
  if (
    !isRecord(value) ||
    typeof value.title !== 'string' ||
    typeof value.url !== 'string' ||
    (value.state !== 'open' && value.state !== 'closed' && value.state !== 'merged')
  ) {
    return null;
  }
  return {
    number: typeof value.number === 'number' && Number.isFinite(value.number) ? value.number : undefined,
    state: value.state,
    title: value.title,
    url: value.url,
  };
}

export function normalizeCachedTitlebarGitFiles(value: unknown): SidebarGitState['files'] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.flatMap((file) => {
    if (!isRecord(file) || typeof file.path !== 'string') {
      return [];
    }
    return [
      {
        additions: readCachedTitlebarNumber(file.additions),
        deletions: readCachedTitlebarNumber(file.deletions),
        path: file.path,
      },
    ];
  });
}

export function readCachedTitlebarNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

export function titlebarGitStateCacheKey(
  projectIdentity: Pick<TitlebarProjectState, 'projectId' | 'projectPath'>
): string | undefined {
  const projectKey = projectIdentity.projectId || projectIdentity.projectPath;
  return projectKey ? `${TITLEBAR_GIT_STATE_CACHE_STORAGE_PREFIX}${encodeURIComponent(projectKey)}` : undefined;
}

export function formatToggleSidebarTooltipLabel(hotkey: string | undefined): string {
  if (!hotkey) {
    return '';
  }
  /*
   * CDXC:SidebarCollapse 2026-06-15-13:34:
   * The titlebar-side collapse control tooltip should name the command and
   * show the assigned shortcut, matching native hover help language while
   * preserving the empty label when Toggle Sidebar has no hotkey.
   */
  return `Toggle Sidebar (${formatSidebarHotkeyLabel(hotkey)})`;
}

export function createInitialProjectState(bootstrap: Record<string, unknown>): TitlebarProjectState {
  const projectPath = typeof bootstrap.cwd === 'string' ? bootstrap.cwd : '';
  const pathParts = projectPath.split('/').filter(Boolean);
  const sharedSettingsJson = isRecord(bootstrap.sharedSidebarStorage)
    ? bootstrap.sharedSidebarStorage.settings
    : undefined;
  const settings = normalizeghostexSettings(parseSharedSettings(sharedSettingsJson));
  const initialState: TitlebarProjectState = {
    activeMode: resolveInitialTitlebarMode(bootstrap),
    agentHookStatus: undefined,
    ghostexCliStatus: undefined,
    portless: undefined,
    browserTabs: [],
    codeEditorProjectIds: [],
    debuggingMode: settings.debuggingMode,
    diagnosticLogging: settings.diagnosticLogging,
    showBetaFeatures: settings.showBetaFeatures,
    codeViewTabHidden: settings.codeViewTabHidden,
    browserViewTabHidden: settings.browserViewTabHidden,
    kanbanViewTabHidden: settings.kanbanViewTabHidden,
    automateViewTabHidden: settings.automateViewTabHidden,
    docsViewTabHidden: settings.docsViewTabHidden,
    tipsAndTricksTitlebarButtonHidden: settings.tipsAndTricksTitlebarButtonHidden,
    resourcesTitlebarButtonHidden: settings.resourcesTitlebarButtonHidden,
    gitActionsTitlebarButtonHidden: settings.gitActionsTitlebarButtonHidden,
    quickActionsTitlebarButtonHidden: settings.quickActionsTitlebarButtonHidden,
    openInTitlebarButtonHidden: settings.openInTitlebarButtonHidden,
    diffStats: createDefaultSidebarProjectDiffStats(false),
    editorIsOpen: false,
    editorIsSleeping: false,
    editorStatus: 'idle',
    git: createDefaultSidebarGitState(),
    gxserverDaemon: {
      alwaysStart: true,
      state: 'unknown',
    },
    hotkeys: settings.hotkeys,
    keepAwake: createTitlebarKeepAwakeSettings(settings),
    projectEditorCompanionPaneHidden: false,
    projectIsQuick: false,
    projectName:
      (typeof bootstrap.workspaceName === 'string' && bootstrap.workspaceName) ||
      pathParts[pathParts.length - 1] ||
      'Ghostex',
    projectPath,
    petOverlayEnabled: settings.petOverlayEnabled,
    resourceGroups: [],
    sidebarTheme: resolveSidebarTheme(settings.sidebarTheme, 'dark'),
    customSidebarTitlebarForegroundColor: getSidebarTitlebarForegroundForBackground(
      settings.customSidebarTitlebarBackgroundColor
    ),
    customSidebarTitlebarBackgroundColor: settings.customSidebarTitlebarBackgroundColor,
    sidebarCollapsed: bootstrap.sidebarCollapsed === true,
    sidebarSide: bootstrap.sidebarSide === 'right' ? 'right' : settings.sidebarSide,
    sidebarActions: {
      commands: [],
    },
    showProjectEditorDiffFileCount: settings.showProjectEditorDiffFileCount,
    webLinkOpenTarget: settings.webLinkOpenTarget,
    toggleSidebarHotkeyLabel: formatToggleSidebarTooltipLabel(settings.hotkeys.toggleSidebarCollapsed),
    workspaceOpenTargets: {
      availability: settings.workspaceOpenTargetAvailability,
      customTargets: settings.customWorkspaceOpenTargets,
      hiddenTargetIds: settings.workspaceOpenTargetHiddenIds,
    },
    updateAvailable: readInitialTitlebarUpdateAvailable(bootstrap),
    updateDownloadProgress: readInitialTitlebarUpdateDownloadProgress(bootstrap),
    updateDownloading: readInitialTitlebarUpdateDownloading(bootstrap),
  };
  /*
   * CDXC:ReactTitlebar 2026-06-11-18:06:
   * Native dropdown child windows need the latest titlebar project/resource
   * payload before first render. Swift injects that payload into the bootstrap
   * object at document start; merge it here so Resources does not briefly or
   * permanently render default state when the post-load bridge push races React.
   */
  const mergedState = mergeTitlebarProjectState(initialState, bootstrap as Partial<TitlebarProjectState>);
  cacheTitlebarGitState(mergedState);
  return mergedState;
}

export function readInitialTitlebarUpdateAvailable(bootstrap: Record<string, unknown>): boolean {
  /**
   * CDXC:AutoUpdate 2026-06-08-18:21:
   * The native launch probe can finish before or during titlebar startup.
   * Accept both the injected bootstrap boolean and the pending native bridge
   * boolean so detected updates show the titlebar button on first render.
   */
  return bootstrap.updateAvailable === true || window.__ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__ === true;
}

export function readInitialTitlebarUpdateDownloading(bootstrap: Record<string, unknown>): boolean {
  /**
   * CDXC:AutoUpdate 2026-06-13-17:52:
   * Download animation is native-owned Sparkle state. Accept both the injected
   * bootstrap boolean and the pending bridge boolean so titlebar reloads do not
   * lose the active download indicator while an update is already downloading.
   */
  return bootstrap.updateDownloading === true || window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOADING__ === true;
}

export function readInitialTitlebarUpdateDownloadProgress(bootstrap: Record<string, unknown>): number | null {
  /**
   * CDXC:AutoUpdate 2026-06-30-22:18:
   * Download progress is a nullable native-owned ratio. Prefer the pending
   * bridge value over bootstrap because `null` is an intentional clear when
   * Sparkle leaves the download phase.
   */
  if (Object.prototype.hasOwnProperty.call(window, '__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__')) {
    return normalizeTitlebarUpdateDownloadProgress(window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__);
  }
  return normalizeTitlebarUpdateDownloadProgress(bootstrap.updateDownloadProgress);
}

export function createTitlebarKeepAwakeSettings(
  settings: ReturnType<typeof normalizeghostexSettings>
): TitlebarKeepAwakeSettings {
  /*
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * The macOS Keep Awake feature is experimental-only. Build the titlebar-facing
   * state with one effective visibility flag so startup, Settings sync, and
   * native child dropdown windows all hide the button when Enable Experimental
   * Features is off.
   */
  const featureEnabled = settings.showBetaFeatures;
  return {
    activateOnExternalDisplay: settings.keepAwakeActivateOnExternalDisplay,
    activateOnLaunch: settings.keepAwakeActivateOnLaunch,
    allowDisplaySleep: settings.keepAwakeAllowDisplaySleep,
    batteryThresholdPercent: settings.keepAwakeBatteryThresholdPercent,
    deactivateBelowBatteryThreshold: settings.keepAwakeBatteryThresholdPercent > 0,
    deactivateOnLowPowerMode: settings.keepAwakeDeactivateOnLowPowerMode,
    deactivateOnUserSwitch: settings.keepAwakeDeactivateOnUserSwitch,
    defaultDurationMinutes: settings.keepAwakeDefaultDurationMinutes,
    delayedSendSessionCount: 0,
    featureEnabled,
    hideTitlebarControl: !featureEnabled || settings.hideKeepAwakeTitlebarControl,
    preventLidSleep: settings.keepAwakePreventLidSleep,
    whileWorkingSessions: settings.keepAwakeWhileWorkingSessions,
    workingSessionCount: 0,
  };
}

export function readStoredKeepAwakeRuntime(): KeepAwakeRuntimeState | undefined {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY) || 'null');
    return parseKeepAwakeRuntimeState(parsed);
  } catch {
    return undefined;
  }
}

export function parseKeepAwakeRuntimeState(value: unknown): KeepAwakeRuntimeState | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const pid = typeof value.pid === 'number' ? value.pid : Number.NaN;
  const durationMinutes = typeof value.durationMinutes === 'number' ? value.durationMinutes : Number.NaN;
  if (
    !Number.isFinite(pid) ||
    pid <= 0 ||
    !KEEP_AWAKE_DURATION_OPTIONS.some((option) => option.value === durationMinutes)
  ) {
    return undefined;
  }
  return {
    durationMinutes: durationMinutes as KeepAwakeDurationMinutes,
    fireAtMs: typeof value.fireAtMs === 'number' ? value.fireAtMs : undefined,
    pid,
    source: value.source === 'automatic' ? 'automatic' : 'manual',
    startedAtMs: typeof value.startedAtMs === 'number' ? value.startedAtMs : Date.now(),
  };
}

export function readKeepAwakeRuntimeSyncState(raw: string | null): KeepAwakeRuntimeSyncState | undefined {
  try {
    const parsed = JSON.parse(raw || 'null');
    if (!isRecord(parsed)) {
      return undefined;
    }
    const hasRuntime = Object.prototype.hasOwnProperty.call(parsed, 'runtime');
    const runtime = parseKeepAwakeRuntimeState(parsed.runtime);
    return {
      ...(hasRuntime ? { runtime: runtime ?? null } : {}),
      suppressAutoStart: parsed.suppressAutoStart === true,
    };
  } catch {
    return undefined;
  }
}

export function publishKeepAwakeRuntimeSync(state: KeepAwakeRuntimeSyncState): void {
  const payload = {
    runtime: state.runtime,
    suppressAutoStart: state.suppressAutoStart,
    updatedAtMs: Date.now(),
  };
  localStorage.setItem(KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY, JSON.stringify(payload));
  window.dispatchEvent(
    new CustomEvent<KeepAwakeRuntimeSyncState>(KEEP_AWAKE_RUNTIME_CHANGED_EVENT, {
      detail: {
        runtime: state.runtime,
        suppressAutoStart: state.suppressAutoStart,
      },
    })
  );
}

export function readStoredTitlebarTipIds(): Set<string> {
  try {
    const parsed = JSON.parse(localStorage.getItem(TITLEBAR_TIPS_READ_STORAGE_KEY) || '[]');
    if (!Array.isArray(parsed)) {
      return new Set();
    }
    return new Set(parsed.filter((id): id is string => typeof id === 'string' && id.length > 0));
  } catch {
    return new Set();
  }
}

export function writeStoredTitlebarTipIds(ids: Set<string>) {
  localStorage.setItem(TITLEBAR_TIPS_READ_STORAGE_KEY, JSON.stringify([...ids]));
}

export async function applyKeepAwakeLidSleepPrevention(
  enabled: boolean,
  options: { installIfNeeded?: boolean } = {}
): Promise<boolean> {
  /**
   * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
   * User-requested closed-lid wakefulness requires a privileged helper because
   * `caffeinate` cannot cover MacBook lid-close sleep. The helper is installed
   * only when this setting and Keep Awake are both active. Lease refreshes never
   * request installation, so cancelling the administrator prompt does not create
   * repeated password prompts; the user can retry by starting Keep Awake again.
   */
  try {
    const result = await runNativeKeepAwakeLidSleepPrevention(enabled, {
      installIfNeeded: options.installIfNeeded,
    });
    if (result.exitCode !== 0) {
      console.warn('Failed to update lid-close sleep prevention', result.stderr || result.stdout);
      return false;
    }
  } catch (error) {
    console.warn('Failed to update lid-close sleep prevention', error);
    return false;
  }
  return true;
}

export async function readKeepAwakePowerSnapshot(options: {
  includeBattery: boolean;
  includeExternalDisplay: boolean;
  includeLowPowerMode: boolean;
}): Promise<
  | {
      batteryPercent?: number;
      externalDisplayConnected: boolean;
      lowPowerMode?: boolean;
    }
  | undefined
> {
  try {
    /*
    CDXC:TitlebarKeepAwake 2026-06-07-16:20:
    Keep Awake automation should not run heavyweight power probes just because
    Keep Awake is active. Build the shell command from the enabled rules so
    hidden checks skip system_profiler, pmset battery, or low-power reads when no
    rule can act on that value.
    */
    const result = await runNativeProcess('/bin/sh', [
      '-lc',
      [
        options.includeBattery
          ? "battery=$(/usr/bin/pmset -g batt 2>/dev/null | /usr/bin/awk -F';' '/InternalBattery/ {gsub(/[^0-9]/, \"\", $1); print $1; exit}')"
          : 'battery=',
        options.includeLowPowerMode
          ? "low=$(/usr/bin/pmset -g 2>/dev/null | /usr/bin/awk '/lowpowermode/ {print $2; exit}')"
          : 'low=',
        options.includeExternalDisplay
          ? "displays=$(/usr/sbin/system_profiler SPDisplaysDataType 2>/dev/null | /usr/bin/awk '/Resolution:/ {count++} END {print count+0}')"
          : 'displays=0',
        '/bin/echo "battery=${battery:-};low=${low:-};displays=${displays:-0}"',
      ].join('; '),
    ]);
    if (result.exitCode !== 0) {
      return undefined;
    }
    const fields = new Map(
      result.stdout
        .trim()
        .split(';')
        .map((field) => {
          const [key, value = ''] = field.split('=');
          return [key, value] as const;
        })
    );
    const batteryPercent = Number(fields.get('battery'));
    const displays = Number(fields.get('displays'));
    return {
      batteryPercent: Number.isFinite(batteryPercent) ? batteryPercent : undefined,
      externalDisplayConnected: Number.isFinite(displays) && displays > 1,
      lowPowerMode: fields.get('low') === '1',
    };
  } catch (error) {
    console.warn('Failed to read keep-awake power state', error);
    return undefined;
  }
}
