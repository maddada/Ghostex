/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_TYPE,
  GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_VERSION,
  GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_TYPE,
  GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_VERSION,
  GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_TYPE,
  GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_VERSION,
} from '../constants';
import type { GpuiWorkspaceTabSessionSelectionPayload } from '../types-and-protocol';
import { normalizeNonEmptyString, uniqueNonEmptyStrings } from './records';
import { parseGpuiRemotePresentationSessionId } from './remote-presentation';
import { gpuiStatusPetActivationSessionIdAllowed } from './status-indicators';
import { parseGxserverPresentationProjectSessionId } from '@/packages/shared/gxserver-presentation-sidebar-projection';
import type { SidebarToExtensionMessage } from '@/packages/shared/session-grid-contract';
import type { SidebarCommandScope } from '@/packages/shared/sidebar-commands';
import { isSidebarCommandRunMode } from '@/packages/shared/sidebar-commands';

export function normalizeGpuiCommandPaletteSessionFocus(value: unknown): string | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !['sessionId', 'type', 'version'].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_COMMAND_PALETTE_SESSION_FOCUS_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (!sessionId) {
    return undefined;
  }
  // Palette rows only ever carry projected sidebar ids: combined local
  // project-session ids or remote presentation ids. Raw daemon ids are not
  // routable from the palette and are rejected.
  if (!parseGpuiRemotePresentationSessionId(sessionId) && !parseGxserverPresentationProjectSessionId(sessionId)) {
    return undefined;
  }
  return sessionId;
}

/*
 * CDXC:AgentLauncher 2026-08-01:
 * The tab strip runs Global Actions and the Command Palette runs Project
 * Actions, and both send only an id. Without a scope on the selector the two
 * id spaces are indistinguishable, so a Global Action whose id also exists as a
 * project action would launch the project one. Scope is optional and absent
 * means project, which keeps every existing palette sender unchanged.
 */
export function normalizeGpuiCommandPaletteRunSidebarCommand(value: unknown):
  | {
      message: Extract<SidebarToExtensionMessage, { type: 'runSidebarCommand' }>;
      scope: SidebarCommandScope;
    }
  | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !['commandId', 'runMode', 'scope', 'type', 'version'].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_COMMAND_PALETTE_RUN_COMMAND_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const commandId = normalizeNonEmptyString(record.commandId)?.trim();
  if (!commandId) {
    return undefined;
  }
  let scope: SidebarCommandScope = 'project';
  if (Object.prototype.hasOwnProperty.call(record, 'scope')) {
    if (record.scope !== 'global' && record.scope !== 'project') {
      return undefined;
    }
    scope = record.scope;
  }
  if (!Object.prototype.hasOwnProperty.call(record, 'runMode')) {
    return {
      message: {
        commandId,
        type: 'runSidebarCommand',
      },
      scope,
    };
  }
  if (!isSidebarCommandRunMode(record.runMode)) {
    return undefined;
  }
  return {
    message: {
      commandId,
      runMode: record.runMode,
      type: 'runSidebarCommand',
    },
    scope,
  };
}

export function normalizeGpuiWorkspaceTabSessionSelection(
  value: unknown
): GpuiWorkspaceTabSessionSelectionPayload | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some(
      (key) =>
        ![
          'localRuntimeMissing',
          'localWasSleeping',
          'projectId',
          'sessionId',
          'type',
          'version',
          'visibleSessionIds',
        ].includes(key)
    )
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_WORKSPACE_TAB_SESSION_SELECTED_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiStatusPetActivationSessionIdAllowed(projectId) ||
    !gpuiStatusPetActivationSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  if (record.localWasSleeping !== undefined && record.localWasSleeping !== true) {
    return undefined;
  }
  if (record.localRuntimeMissing !== undefined && record.localRuntimeMissing !== true) {
    return undefined;
  }
  const visibleSessionIds = Array.isArray(record.visibleSessionIds)
    ? uniqueNonEmptyStrings(record.visibleSessionIds)?.filter((visibleSessionId) =>
        gpuiStatusPetActivationSessionIdAllowed(visibleSessionId)
      )
    : undefined;
  if (
    record.visibleSessionIds !== undefined &&
    (!Array.isArray(record.visibleSessionIds) ||
      visibleSessionIds?.length !== record.visibleSessionIds.length ||
      visibleSessionIds.length > 64)
  ) {
    return undefined;
  }
  return {
    ...(record.localRuntimeMissing === true ? { localRuntimeMissing: true } : {}),
    ...(record.localWasSleeping === true ? { localWasSleeping: true } : {}),
    projectId,
    sessionId,
    ...(visibleSessionIds ? { visibleSessionIds } : {}),
  };
}
