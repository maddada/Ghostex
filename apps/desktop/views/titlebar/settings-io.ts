import {
  BUILT_IN_WORKSPACE_OPEN_TARGETS,
  type WorkspaceOpenTargetAvailability,
  type WorkspaceOpenTargetDefinition,
} from '@/packages/shared/workspace-open-targets';
import { LAST_ACTION_COMMAND_STORAGE_PREFIX, LAST_OPEN_TARGET_STORAGE_KEY } from './constants';
import type { ResolvedOpenTarget, TitlebarMode, TitlebarOpenTargetsSettings, TitlebarProjectState } from './types';

export function normalizeTitlebarMode(candidate: unknown): TitlebarMode {
  /**
   * CDXC:ModeSwitcher 2026-05-15-18:20:
   * The top titlebar mode must mirror the workarea mode restored by the sidebar
   * at launch and after each mode transition. Treat the sidebar/native payload
   * as authoritative so a restored Source, Browser, Kanban, Automate, or Docs
   * pane cannot leave the segmented control highlighted on Agents.
   *
   * CDXC:ModeSwitcher 2026-05-15-18:30:
   * User clicks still need optimistic local mode selection so the shared-layout
   * pill animates immediately while slow Source/Browser/Kanban/Automate/Docs surfaces load. Clear
   * that optimistic value when sidebar state arrives so startup restore and
   * failed transitions remain synchronized with the real visible workarea.
   */
  return candidate === 'code' ||
    candidate === 'git' ||
    candidate === 'automate' ||
    candidate === 'tasks' ||
    candidate === 'manage'
    ? candidate
    : 'agents';
}

export function resolveInitialTitlebarMode(bootstrap: Record<string, unknown>): TitlebarMode {
  const explicitMode = normalizeTitlebarMode(bootstrap.activeMode);
  if (explicitMode !== 'agents') {
    return explicitMode;
  }
  /*
  CDXC:ProjectSidebarOwnership 2026-06-02-12:29:
  The titlebar must not infer startup mode from the old native-sidebar-projects.json payload. gxserver owns shared project/session inventory now, while the macOS window owns the explicit active mode passed in bootstrap state.
  */
  return 'agents';
}

export function parseSharedSettings(candidate: unknown): unknown {
  if (typeof candidate !== 'string') {
    return undefined;
  }
  try {
    return JSON.parse(candidate || 'null');
  } catch {
    return undefined;
  }
}

export function createConfiguredOpenTargets(settings: TitlebarOpenTargetsSettings): ResolvedOpenTarget[] {
  const hiddenTargetIds = new Set(settings.hiddenTargetIds);
  return [
    ...BUILT_IN_WORKSPACE_OPEN_TARGETS.filter((target) => !hiddenTargetIds.has(target.id)).map(
      (definition): ResolvedOpenTarget => ({
        definition,
        id: definition.id,
        kind: 'built-in',
        label: definition.label,
      })
    ),
    ...settings.customTargets.map((custom): ResolvedOpenTarget => ({
      command: custom.command,
      custom,
      id: custom.id,
      kind: 'custom',
      label: custom.label,
    })),
  ];
}

export function resolveVisibleOpenTargets(
  targets: ResolvedOpenTarget[],
  availability: WorkspaceOpenTargetAvailability
): ResolvedOpenTarget[] {
  const availableTargetIds = new Set(availability.availableTargetIds);
  return targets
    .map((target) => {
      if (target.id === 'finder') {
        return target;
      }
      if (target.kind === 'custom') {
        return target;
      }
      if (!availableTargetIds.has(target.id as WorkspaceOpenTargetDefinition['id'])) {
        return undefined;
      }
      /**
       * CDXC:ReactTitlebar 2026-05-11-02:03
       * The titlebar menu shows only persisted installed built-ins plus custom
       * targets. Hidden ids are applied before this step, so startup detection
       * cannot re-add an editor the user turned off in Settings.
       */
      return {
        ...target,
        resolvedAppName: availability.resolvedAppNames[target.id],
        resolvedCommand: availability.resolvedCommands[target.id],
      };
    })
    .filter((target): target is ResolvedOpenTarget => target !== undefined);
}

export function readLastOpenTargetId(): string {
  return localStorage.getItem(LAST_OPEN_TARGET_STORAGE_KEY) || 'finder';
}

export function readLastActionCommandId(
  state: Pick<TitlebarProjectState, 'projectId' | 'projectPath'>
): string | undefined {
  const storageKey = getLastActionCommandStorageKey(state);
  return storageKey ? localStorage.getItem(storageKey)?.trim() || undefined : undefined;
}

export function persistLastActionCommandId(
  state: Pick<TitlebarProjectState, 'projectId' | 'projectPath'>,
  commandId: string
): void {
  const storageKey = getLastActionCommandStorageKey(state);
  if (!storageKey) {
    return;
  }
  localStorage.setItem(storageKey, commandId);
}

export function getLastActionCommandStorageKey(
  state: Pick<TitlebarProjectState, 'projectId' | 'projectPath'>
): string | undefined {
  const projectKey = state.projectId?.trim() || state.projectPath.trim();
  if (!projectKey) {
    return undefined;
  }
  /**
   * CDXC:TitlebarActions 2026-05-11-02:46
   * Moving Actions from the sidebar header to the titlebar keeps the same
   * project-scoped primary-action behavior: the split button's left side runs
   * the last chosen action for the active project, not a global last action.
   */
  return `${LAST_ACTION_COMMAND_STORAGE_PREFIX}${projectKey}`;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}
