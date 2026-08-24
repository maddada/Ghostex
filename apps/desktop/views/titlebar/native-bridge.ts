import type { SidebarPortlessState } from '@/packages/shared/session-grid-contract-sidebar';
import type { NativePortlessAdminInstallAction } from '@/packages/shared/native-ghostty-host-protocol';
import { isDiagnosticLoggingScenarioEnabled, type DiagnosticLoggingSettings } from '@/packages/shared/ghostex-settings';
import { KEEP_AWAKE_ADMIN_PROCESS_TIMEOUT_MS } from './constants';
import type {
  KeepAwakeRuntimeSyncState,
  NativeProcessResult,
  NativeTitlebarCommand,
  TitlebarMode,
  TitlebarProjectState,
} from './types';

export const pendingProcessResults = new Map<
  string,
  {
    reject: (error: Error) => void;
    resolve: (result: NativeProcessResult) => void;
    timeout: number;
  }
>();

export function postNative(command: NativeTitlebarCommand): void {
  window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage(command);
}

export function setTitlebarNativePointerInside(isInside: boolean): void {
  /*
   * CDXC:ReactTitlebar 2026-06-10-23:44:
   * AppKit owns the effective titlebar hit boundary because the WKWebView spans
   * the window for portals. Store native pointer ownership on the body for
   * bridge state only; this flag must not own titlebar hover visibility.
   *
   * CDXC:TooltipLifecycle 2026-06-13-02:30:
   * Do not use this flag as a titlebar tooltip or hover gate. AppKit can leave
   * the flag false until a titlebar click updates strip ownership, so titlebar
   * tooltips must rely on normal CSS hover and local tooltip state instead.
   */
  document.body.dataset.nativePointerInside = isInside ? 'true' : 'false';
}

export function setTitlebarWindowFocused(isFocused: boolean): void {
  /*
   * CDXC:ReactTitlebar 2026-06-20-17:10:
   * AppKit owns the key-window state for titlebar chrome. React keeps the
   * existing body dataset bridge so titlebar CSS can track active/inactive
   * windows without adding new native hit-test or routing behavior.
   */
  window.__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__ = isFocused;
  document.body.dataset.windowFocused = isFocused ? 'true' : 'false';
}

export function suppressTitlebarTooltipsFromDom(): void {
  setTitlebarNativePointerInside(false);
}

export function enableTitlebarTooltipsFromDom(): void {
  setTitlebarNativePointerInside(true);
}

export function normalizeTitlebarUpdateDownloadProgress(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return null;
  }
  return Math.min(Math.max(value, 0), 1);
}

export function postTitlebarSidebarCommand(
  message:
    | { type: 'openBrowserPane'; url: string }
    | { type: 'openGhostexTutorialVideo' }
    | { type: 'openWorkspaceWelcome' }
    | { type: 'requestAgentHookStatus' }
    | { type: 'requestGhostexCliStatus' }
    | { type: 'refreshDaemonSessions' }
    | { type: 'refreshGitState' }
    | {
        action: NativePortlessAdminInstallAction;
        protocol: SidebarPortlessState['health']['protocol'];
        requestId: string;
        type: 'runPortlessSettingsAdminAction';
      }
): void {
  /*
  CDXC:AgentHooks 2026-06-07-11:05:
  Opening Tips & Tricks should refresh gxserver hook status instead of relying
  on the titlebar's cached layout snapshot. Route through the existing
  app-modal sidebarCommand bridge so the native sidebar remains the owner of
  authenticated gxserver requests and hook-status state publication.

  CDXC:CliInstall 2026-06-07-15:26:
  Tips & Tricks CLI notices must use the native sidebar's real PATH inspection
  instead of probing from the isolated titlebar webview.

  CDXC:TipsAndTricks 2026-06-16-08:17:
  Tips & Tricks header actions should launch Features and setup
  flow through the sidebar command bridge because the native sidebar owns app
  modal presentation.

  CDXC:GhostexTutorialVideo 2026-06-18-05:31:
  The Features action now opens the tutorial video modal through the sidebar
  command bridge, leaving the old Highlighted Features modal unused.

  CDXC:TipsAndTricks 2026-06-16-19:42:
  The Changelog header action should reuse the sidebar browser-pane command so
  the releases page opens in the current project as a new browser session.

  CDXC:TitlebarGit 2026-06-16-18:41:
  Opening the titlebar Git menu should request fresh Git stats through the
  sidebar-owned bridge before showing the dropdown, including right-click opens.
  */
  window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
    message,
    type: 'sidebarCommand',
  });
}

export function closeAppModalFromTitlebarNavigation(area: string): void {
  /*
   * CDXC:SettingsDismissal 2026-06-15-14:07:
   * Titlebar mode switches and titlebar action runners should dismiss the
   * workspace-scoped Settings child window before they change workarea state or
   * run commands. Send the normal app-modal close message through the native
   * bridge so Settings, if open, closes without adding titlebar-specific state.
   */
  window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({ area, type: 'close' });
}

export function appendTitlebarActionCrashDebugLog(
  diagnosticLogging: DiagnosticLoggingSettings,
  event: string,
  details?: unknown
): void {
  /**
   * CDXC:TitlebarActions 2026-05-15-17:23:
   * Terminal action button crashes need a breadcrumb from the isolated React
   * titlebar before the native-sidebar command runner receives the click.
   * Persist this trace outside the normal debug-toggle filter so a repro that
   * exits the app still leaves the selected action id and project context.
   *
   * CDXC:GxserverLogs 2026-06-15-20:39:
   * actionCrashTrace is a breadcrumb namespace, not severity. Keep the first
   * titlebar hop only while the native.terminal.focus scenario is enabled so
   * routine action clicks do not persist as normal-mode crash warnings.
   */
  if (!isDiagnosticLoggingScenarioEnabled(diagnosticLogging, 'native.terminal.focus')) {
    return;
  }
  postNative({
    details: details === undefined ? undefined : JSON.stringify(details),
    event,
    type: 'appendTerminalFocusDebugLog',
  });
}

export function appendTitlebarModeSwitchDebugLog(
  diagnosticLogging: DiagnosticLoggingSettings,
  event: string,
  details: Record<string, unknown> = {}
): void {
  /**
   * CDXC:ModeSwitcherDiagnostics 2026-06-15-00:21:
   * Agents, Source, Browser, Kanban, and Manage titlebar clicks need the same first-hop
   * timing breadcrumbs. Send only enum-like mode state, booleans, safe ids,
   * and monotonic timestamps while the native.mode.switcher scenario is enabled; never
   * include project names, paths, URLs, titles, commands, or user text.
   *
   * CDXC:DiagnosticsSettings 2026-06-27-22:07:
   * First-hop titlebar mode-switch breadcrumbs must follow the same exact
   * scenario allowlist as the native writer so Debugging Mode can show debug UI
   * without enabling routine persistent logs.
   */
  if (!isDiagnosticLoggingScenarioEnabled(diagnosticLogging, 'native.mode.switcher')) {
    return;
  }
  postNative({
    details: JSON.stringify({
      ...details,
      performanceNowMs: performance.now(),
      source: 'titlebar',
      wallTimeMs: Date.now(),
    }),
    event,
    type: 'appendModeSwitcherDebugLog',
  });
}

export function appendTitlebarChromeResponsivenessDebugLog(
  diagnosticLogging: DiagnosticLoggingSettings,
  event: string,
  details: Record<string, unknown> = {}
): void {
  /*
   * CDXC:ChromeResponsivenessDiagnostics 2026-06-30-23:52:
   * Heavy lag and blank chrome repros need first-hop titlebar timing from the
   * isolated React titlebar. Gate routine breadcrumbs behind the targeted
   * native.chrome.responsiveness scenario and send only counts, timings,
   * booleans, and enum-like phases to the native sanitized writer.
   */
  if (!isDiagnosticLoggingScenarioEnabled(diagnosticLogging, 'native.chrome.responsiveness')) {
    return;
  }
  postNative({
    details: JSON.stringify({
      ...details,
      performanceNowMs: Math.round(performance.now()),
      source: 'titlebar',
      wallTimeMs: Date.now(),
    }),
    event,
    type: 'appendNativeChromeResponsivenessDebugLog',
  });
}

export function titlebarModeSwitchLogDetails(input: {
  optimisticMode: TitlebarMode | undefined;
  projectState: TitlebarProjectState;
  targetMode: TitlebarMode;
}): Record<string, unknown> {
  return {
    activeMode: input.projectState.activeMode,
    editorIsOpen: input.projectState.editorIsOpen,
    editorIsSleeping: input.projectState.editorIsSleeping,
    editorStatus: input.projectState.editorStatus,
    hasOptimisticMode: input.optimisticMode !== undefined,
    optimisticMode: input.optimisticMode ?? 'none',
    projectId: input.projectState.projectId ?? 'none',
    projectIsQuick: input.projectState.projectIsQuick,
    targetMode: input.targetMode,
  };
}

export function runNativeProcess(
  executable: string,
  args: string[],
  options: { cwd?: string; env?: Record<string, string>; timeoutMs?: number } = {}
): Promise<NativeProcessResult> {
  const requestId = `titlebar-process-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  postNative({
    args,
    cwd: options.cwd,
    env: options.env,
    executable,
    requestId,
    type: 'runProcess',
  });
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      pendingProcessResults.delete(requestId);
      reject(new Error(`${executable} ${args.join(' ')} timed out`));
    }, options.timeoutMs ?? 30_000);
    pendingProcessResults.set(requestId, { reject, resolve, timeout });
  });
}

export function runNativeKeepAwakeLidSleepPrevention(
  enabled: boolean,
  options: { installIfNeeded?: boolean; timeoutMs?: number } = {}
): Promise<NativeProcessResult> {
  const requestId = `titlebar-lid-sleep-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  postNative({
    enabled,
    installIfNeeded: options.installIfNeeded,
    requestId,
    type: 'setKeepAwakeLidSleepPrevention',
  });
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      pendingProcessResults.delete(requestId);
      reject(new Error(`setKeepAwakeLidSleepPrevention ${enabled} timed out`));
    }, options.timeoutMs ?? KEEP_AWAKE_ADMIN_PROCESS_TIMEOUT_MS);
    pendingProcessResults.set(requestId, { reject, resolve, timeout });
  });
}

export function syncKeepAwakeRuntimeToMainTitlebar(syncState: KeepAwakeRuntimeSyncState): void {
  /*
   * CDXC:TitlebarKeepAwake 2026-06-23-19:36:
   * Keep Awake menu actions run inside a native child WKWebView. Relay the committed runtime state back to the main titlebar explicitly so the titlebar icon changes immediately instead of depending on cross-webview localStorage events.
   */
  postNative({
    runtime: syncState.runtime,
    suppressAutoStart: syncState.suppressAutoStart,
    type: 'syncTitlebarKeepAwakeRuntime',
  });
}
