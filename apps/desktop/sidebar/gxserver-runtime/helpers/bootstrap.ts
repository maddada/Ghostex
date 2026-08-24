/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import { GPUI_SIDEBAR_DEFAULT_CLIENT_ID, GPUI_SIDEBAR_THEME_VALUES } from '../constants';
import type {
  GpuiGxserverBootstrap,
  GpuiSidebarRuntimeSettings,
  GpuiSidebarRuntimeSettingsSnapshot,
  GpuiValidatedGxserverBootstrap,
} from '../types-and-protocol';
import { normalizeNonEmptyString, uniqueNonEmptyStrings } from './records';
import { createGpuiRemotePresentationGroupId, parseGpuiRemotePresentationProjectId } from './remote-presentation';
import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import { normalizeghostexSettings } from '@/packages/shared/ghostex-settings';
import { createGxserverPresentationProjectGroupId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { GxserverAppUserData } from '@/packages/shared/gxserver-protocol';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import { resolveActiveTerminalSelection } from '@/packages/shared/remote-terminal-selection';
import type { SidebarTheme } from '@/packages/shared/session-grid-contract';

export function createEmptyGpuiAppUserData(): GxserverAppUserData {
  return {
    pinnedPrompts: [],
    scratchPadContent: '',
  };
}

export function validateGpuiGxserverBootstrap(
  bootstrap: GpuiGxserverBootstrap
): GpuiValidatedGxserverBootstrap | undefined {
  if (bootstrap.protocolVersion !== undefined && bootstrap.protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
    return undefined;
  }
  if (typeof bootstrap.baseUrl !== 'string' || bootstrap.baseUrl.trim().length === 0) {
    return undefined;
  }
  if (typeof bootstrap.authToken !== 'string' || bootstrap.authToken.trim().length === 0) {
    return undefined;
  }
  try {
    const baseUrl = new URL(bootstrap.baseUrl);
    return {
      authToken: bootstrap.authToken,
      baseUrl: baseUrl.toString().replace(/\/$/u, ''),
      clientId: normalizeNonEmptyString(bootstrap.clientId) ?? GPUI_SIDEBAR_DEFAULT_CLIENT_ID,
      focusedSessionId: normalizeNonEmptyString(bootstrap.focusedSessionId),
      initialActiveProjectId: normalizeNonEmptyString(bootstrap.initialActiveProjectId),
      visibleSessionIds: uniqueNonEmptyStrings(bootstrap.visibleSessionIds),
    };
  } catch {
    return undefined;
  }
}

export function hasSameGpuiGxserverBootstrapTransport(
  left: GpuiValidatedGxserverBootstrap,
  right: GpuiValidatedGxserverBootstrap
): boolean {
  return left.authToken === right.authToken && left.baseUrl === right.baseUrl && left.clientId === right.clientId;
}

export function activeGroupIdForGpuiGxserverBootstrapPresentationState({
  focusedSessionId,
  initialActiveProjectId,
}: Pick<GpuiValidatedGxserverBootstrap, 'focusedSessionId' | 'initialActiveProjectId'>): string | undefined {
  const activeTerminal = resolveActiveTerminalSelection({
    activeProjectId: initialActiveProjectId,
    focusedSessionId,
  });
  if (activeTerminal?.remote) {
    return createGpuiRemotePresentationGroupId(activeTerminal.machineId, activeTerminal.projectId);
  }
  const remoteProject = initialActiveProjectId
    ? parseGpuiRemotePresentationProjectId(initialActiveProjectId)
    : undefined;
  if (remoteProject) {
    return createGpuiRemotePresentationGroupId(remoteProject.machineId, remoteProject.projectId);
  }
  return initialActiveProjectId ? createGxserverPresentationProjectGroupId(initialActiveProjectId) : undefined;
}

export function createGpuiSidebarSettings(runtimeSettings?: GpuiSidebarRuntimeSettings): ghostexSettings {
  /*
  CDXC:GPUISettingsSidebarHandoff 2026-06-24-11:22:
  GPUI SidebarApp must receive the real saved shared Settings object, normalized through the same TypeScript settings schema as macOS, instead of hardcoded bootstrap defaults. Keep debuggingMode/showBetaFeatures pinned to strict CEF-provided booleans so string-like or numeric truthy values cannot alter the Settings/HUD projection.
  */
  const settings = normalizeghostexSettings(runtimeSettings?.settings);
  return {
    ...settings,
    debuggingMode: runtimeSettings?.debuggingMode === true,
    showBetaFeatures: runtimeSettings?.showBetaFeatures === true,
  };
}

export function normalizeGpuiSidebarTheme(value: unknown): SidebarTheme | undefined {
  if (value === 'plain-dark') {
    return 'dark-2';
  }
  return GPUI_SIDEBAR_THEME_VALUES.has(value as SidebarTheme) ? (value as SidebarTheme) : undefined;
}

export function currentGpuiRuntimeSettings(): GpuiSidebarRuntimeSettings | undefined {
  return window.ghostexGpui?.runtimeSettings;
}

export function hasSameGpuiRuntimeSettings(
  previous: GpuiSidebarRuntimeSettings | undefined,
  next: GpuiSidebarRuntimeSettingsSnapshot
): boolean {
  return (
    previous?.debuggingMode === next.debuggingMode &&
    previous?.showBetaFeatures === next.showBetaFeatures &&
    previous?.settings === next.settings
  );
}
