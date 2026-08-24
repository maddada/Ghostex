export type WorkspaceOpenTargetLaunchStyle = 'direct-path' | 'goto' | 'line-column';
export type WorkspaceIdeTargetApp = 'zed' | 'zed-preview' | 'vscode' | 'vscode-insiders';

export type BuiltInWorkspaceOpenTargetId =
  | 'cursor'
  | 'trae'
  | 'kiro'
  | 'vscode'
  | 'vscode-insiders'
  | 'vscodium'
  | 'zed'
  | 'antigravity'
  | 'idea'
  | 'aqua'
  | 'clion'
  | 'datagrip'
  | 'dataspell'
  | 'goland'
  | 'phpstorm'
  | 'pycharm'
  | 'rider'
  | 'rubymine'
  | 'rustrover'
  | 'webstorm'
  | 'finder';

export type WorkspaceOpenTargetDefinition = {
  baseArgs?: readonly string[];
  commands: readonly [string, ...string[]] | null;
  id: BuiltInWorkspaceOpenTargetId;
  label: string;
  launchStyle: WorkspaceOpenTargetLaunchStyle;
  macOSAppNames?: readonly [string, ...string[]];
  targetApp?: WorkspaceIdeTargetApp;
};

export type CustomWorkspaceOpenTarget = {
  args: string[];
  command: string;
  id: string;
  label: string;
};

export const CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX = 'custom:';
export const ALWAYS_AVAILABLE_WORKSPACE_OPEN_TARGET_IDS: readonly BuiltInWorkspaceOpenTargetId[] = ['finder'];

export type WorkspaceOpenTargetAvailability = {
  availableTargetIds: BuiltInWorkspaceOpenTargetId[];
  checkedAtMs: number;
  resolvedAppNames: Record<string, string>;
  resolvedCommands: Record<string, string>;
};

export const DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY: WorkspaceOpenTargetAvailability = {
  availableTargetIds: [...ALWAYS_AVAILABLE_WORKSPACE_OPEN_TARGET_IDS],
  checkedAtMs: 0,
  resolvedAppNames: {},
  resolvedCommands: {},
};

/**
 * CDXC:TitlebarOpenIn 2026-05-11-00:22
 * The titlebar Open In menu uses the shared editor command catalog so
 * installed local IDEs appear without maintaining a second, smaller
 * ghostex-only list.
 *
 * CDXC:TitlebarOpenIn 2026-05-16-23:02
 * Embedded Editor is a first-party Code surface reached from the titlebar Code
 * mode, not an external Open In selection. Keep the Open In catalog focused on
 * installed editors and the first-party folder opener so the dropdown does not duplicate Code mode.
 *
 * CDXC:TitlebarOpenIn 2026-06-04-13:39:
 * The built-in filesystem target remains internally keyed as finder for protocol compatibility, but user-facing labels must say Open Folder so the macOS app copy is OS-agnostic.
 */
export const BUILT_IN_WORKSPACE_OPEN_TARGETS: readonly WorkspaceOpenTargetDefinition[] = [
  {
    commands: ['cursor'],
    id: 'cursor',
    label: 'Cursor',
    launchStyle: 'goto',
    macOSAppNames: ['Cursor'],
  },
  { commands: ['trae'], id: 'trae', label: 'Trae', launchStyle: 'goto', macOSAppNames: ['Trae'] },
  {
    baseArgs: ['ide'],
    commands: ['kiro'],
    id: 'kiro',
    label: 'Kiro',
    launchStyle: 'goto',
    macOSAppNames: ['Kiro'],
  },
  {
    commands: ['code'],
    id: 'vscode',
    label: 'VS Code',
    launchStyle: 'goto',
    macOSAppNames: ['Visual Studio Code'],
    targetApp: 'vscode',
  },
  {
    commands: ['code-insiders'],
    id: 'vscode-insiders',
    label: 'VS Code Insiders',
    launchStyle: 'goto',
    macOSAppNames: ['Visual Studio Code - Insiders'],
    targetApp: 'vscode-insiders',
  },
  {
    commands: ['codium'],
    id: 'vscodium',
    label: 'VSCodium',
    launchStyle: 'goto',
    macOSAppNames: ['VSCodium'],
  },
  {
    commands: ['zed', 'zeditor'],
    id: 'zed',
    label: 'Zed',
    launchStyle: 'direct-path',
    macOSAppNames: ['Zed', 'Zed Preview'],
    targetApp: 'zed',
  },
  {
    commands: ['agy-ide'],
    id: 'antigravity',
    label: 'Antigravity',
    launchStyle: 'goto',
    macOSAppNames: ['Antigravity'],
  },
  {
    commands: ['idea'],
    id: 'idea',
    label: 'IntelliJ IDEA',
    launchStyle: 'line-column',
    macOSAppNames: ['IntelliJ IDEA'],
  },
  { commands: ['aqua'], id: 'aqua', label: 'Aqua', launchStyle: 'line-column', macOSAppNames: ['Aqua'] },
  { commands: ['clion'], id: 'clion', label: 'CLion', launchStyle: 'line-column', macOSAppNames: ['CLion'] },
  {
    commands: ['datagrip'],
    id: 'datagrip',
    label: 'DataGrip',
    launchStyle: 'line-column',
    macOSAppNames: ['DataGrip'],
  },
  {
    commands: ['dataspell'],
    id: 'dataspell',
    label: 'DataSpell',
    launchStyle: 'line-column',
    macOSAppNames: ['DataSpell'],
  },
  {
    commands: ['goland'],
    id: 'goland',
    label: 'GoLand',
    launchStyle: 'line-column',
    macOSAppNames: ['GoLand'],
  },
  {
    commands: ['phpstorm'],
    id: 'phpstorm',
    label: 'PhpStorm',
    launchStyle: 'line-column',
    macOSAppNames: ['PhpStorm'],
  },
  {
    commands: ['pycharm'],
    id: 'pycharm',
    label: 'PyCharm',
    launchStyle: 'line-column',
    macOSAppNames: ['PyCharm'],
  },
  { commands: ['rider'], id: 'rider', label: 'Rider', launchStyle: 'line-column', macOSAppNames: ['Rider'] },
  {
    commands: ['rubymine'],
    id: 'rubymine',
    label: 'RubyMine',
    launchStyle: 'line-column',
    macOSAppNames: ['RubyMine'],
  },
  {
    commands: ['rustrover'],
    id: 'rustrover',
    label: 'RustRover',
    launchStyle: 'line-column',
    macOSAppNames: ['RustRover'],
  },
  {
    commands: ['webstorm'],
    id: 'webstorm',
    label: 'WebStorm',
    launchStyle: 'line-column',
    macOSAppNames: ['WebStorm'],
  },
  { commands: null, id: 'finder', label: 'Open Folder', launchStyle: 'direct-path' },
];

const BUILT_IN_WORKSPACE_OPEN_TARGET_IDS = new Set<string>(BUILT_IN_WORKSPACE_OPEN_TARGETS.map((target) => target.id));

export function normalizeWorkspaceOpenTargetHiddenIds(candidate: unknown): string[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  return Array.from(
    new Set(
      candidate.filter(
        (id): id is BuiltInWorkspaceOpenTargetId => typeof id === 'string' && BUILT_IN_WORKSPACE_OPEN_TARGET_IDS.has(id)
      )
    )
  );
}

export function normalizeWorkspaceOpenTargetAvailability(candidate: unknown): WorkspaceOpenTargetAvailability {
  if (!isRecord(candidate)) {
    return DEFAULT_WORKSPACE_OPEN_TARGET_AVAILABILITY;
  }
  const availableTargetIds = normalizeAvailableWorkspaceOpenTargetIds(candidate.availableTargetIds);
  return {
    availableTargetIds,
    checkedAtMs:
      typeof candidate.checkedAtMs === 'number' && Number.isFinite(candidate.checkedAtMs)
        ? Math.max(0, candidate.checkedAtMs)
        : 0,
    resolvedAppNames: normalizeTargetResolutionMap(candidate.resolvedAppNames, availableTargetIds),
    resolvedCommands: normalizeTargetResolutionMap(candidate.resolvedCommands, availableTargetIds),
  };
}

export function normalizeCustomWorkspaceOpenTargets(candidate: unknown): CustomWorkspaceOpenTarget[] {
  if (!Array.isArray(candidate)) {
    return [];
  }
  const seenIds = new Set<string>();
  const normalized: CustomWorkspaceOpenTarget[] = [];
  for (const entry of candidate) {
    if (!isRecord(entry)) {
      continue;
    }
    const label = typeof entry.label === 'string' ? entry.label.trim() : '';
    const command = typeof entry.command === 'string' ? entry.command.trim() : '';
    if (!label || !command) {
      continue;
    }
    const requestedId = typeof entry.id === 'string' ? entry.id.trim() : '';
    const baseId = requestedId.startsWith(CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX)
      ? requestedId
      : `${CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX}${createWorkspaceOpenTargetSlug(label)}`;
    let id = baseId;
    for (let suffix = 2; seenIds.has(id); suffix += 1) {
      id = `${baseId}-${suffix}`;
    }
    if (seenIds.has(id)) {
      continue;
    }
    seenIds.add(id);
    normalized.push({
      args: Array.isArray(entry.args) ? entry.args.filter((arg): arg is string => typeof arg === 'string') : [],
      command,
      id,
      label,
    });
  }
  return normalized;
}

export function createWorkspaceOpenTargetSlug(label: string): string {
  const slug = label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return slug || 'target';
}

export function isAlwaysAvailableWorkspaceOpenTarget(targetId: string): targetId is BuiltInWorkspaceOpenTargetId {
  return (ALWAYS_AVAILABLE_WORKSPACE_OPEN_TARGET_IDS as readonly string[]).includes(targetId);
}

export function getBuiltInWorkspaceOpenTargetById(targetId: string): WorkspaceOpenTargetDefinition | undefined {
  return BUILT_IN_WORKSPACE_OPEN_TARGETS.find((target) => target.id === targetId);
}

export function normalizeAvailableWorkspaceOpenTargetIds(candidate: unknown): BuiltInWorkspaceOpenTargetId[] {
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-02:03
   * The Open In menu defaults to only locally installed built-ins. Persist the
   * detected available ids separately from user-hidden ids so detection can
   * refresh once at startup without re-enabling targets the user disabled.
   */
  const ids = new Set<BuiltInWorkspaceOpenTargetId>(ALWAYS_AVAILABLE_WORKSPACE_OPEN_TARGET_IDS);
  if (Array.isArray(candidate)) {
    for (const id of candidate) {
      if (typeof id === 'string' && BUILT_IN_WORKSPACE_OPEN_TARGET_IDS.has(id)) {
        ids.add(id as BuiltInWorkspaceOpenTargetId);
      }
    }
  }
  return BUILT_IN_WORKSPACE_OPEN_TARGETS.filter((target) => ids.has(target.id)).map((target) => target.id);
}

function normalizeTargetResolutionMap(
  candidate: unknown,
  availableTargetIds: readonly BuiltInWorkspaceOpenTargetId[]
): Record<string, string> {
  if (!isRecord(candidate)) {
    return {};
  }
  const availableIds = new Set<string>(availableTargetIds);
  const normalized: Record<string, string> = {};
  for (const [targetId, value] of Object.entries(candidate)) {
    if (availableIds.has(targetId) && typeof value === 'string' && value.trim()) {
      normalized[targetId] = value.trim();
    }
  }
  return normalized;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}
