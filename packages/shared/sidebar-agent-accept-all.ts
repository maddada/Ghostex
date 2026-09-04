import {
  getDefaultSidebarAgentByIcon,
  getDefaultSidebarAgentById,
  type DefaultSidebarAgentId,
  type SidebarAgentIcon,
} from './sidebar-agents';

/**
 * CDXC:AgentLauncher 2026-06-02-22:23:
 * The sidebar keeps only the Accept All mode type and support detection needed
 * for Settings UI. gxserver owns runtime flag insertion, stripping, and command
 * shaping so macOS, CLI, TUI, mobile, and remote clients cannot diverge.
 */
export type AgentAcceptAllMode = 'inherit' | 'enabled' | 'disabled';

/**
 * CDXC:AgentLauncher 2026-06-11-17:08:
 * Accept All selects must render the user-facing option label while collapsed,
 * not the raw mode key. Base UI needs root item metadata before the popup is
 * mounted, so keep the labels next to the mode type and reuse them in Settings
 * and standalone agent configuration.
 */
export const AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS: ReadonlyArray<{
  label: string;
  value: AgentAcceptAllMode;
}> = [
  { label: 'Use app default', value: 'inherit' },
  { label: 'Skip permissions', value: 'enabled' },
  { label: 'Keep default', value: 'disabled' },
];

export type AgentAcceptAllFlagSpec = {
  kind: 'flag';
  aliases: readonly string[];
  canonicalFlag: string;
};

export type AgentAcceptAllRuntimeConfigSpec = {
  kind: 'runtimeConfig';
};

export type AgentAcceptAllSpec = AgentAcceptAllFlagSpec | AgentAcceptAllRuntimeConfigSpec;

/**
 * CDXC:AgentLauncher 2026-05-19-10:05:
 * Flags were verified from each vendor CLI `--help` on 2026-05-19. Antigravity
 * CLI (`agy`) uses `--dangerously-skip-permissions` for Accept All. Factory Droid
 * interactive mode has no skip flag; only `droid exec` exposes
 * `--skip-permissions-unsafe`, so the default `droid` launcher stays unsupported.
 *
 * CDXC:AgentLauncher 2026-06-09-14:22:
 * OpenCode TUI does not expose a permission-bypass CLI flag. Keep it supported
 * through gxserver's runtime permission config path so macOS Settings can show
 * the same Accept All control without claiming the stored command gets a flag.
 */
export const AGENT_ACCEPT_ALL_SPECS: Readonly<Record<DefaultSidebarAgentId, AgentAcceptAllSpec | null>> = {
  antigravity: {
    kind: 'flag',
    aliases: ['--dangerously-skip-permissions'],
    canonicalFlag: '--dangerously-skip-permissions',
  },
  amp: {
    kind: 'flag',
    aliases: ['--dangerously-allow-all'],
    canonicalFlag: '--dangerously-allow-all',
  },
  campfire: null,
  claude: {
    kind: 'flag',
    aliases: ['--dangerously-skip-permissions'],
    canonicalFlag: '--dangerously-skip-permissions',
  },
  codex: {
    kind: 'flag',
    aliases: ['--yolo'],
    canonicalFlag: '--yolo',
  },
  codebuddy: null,
  'command-code': {
    kind: 'flag',
    aliases: ['--yolo'],
    canonicalFlag: '--yolo',
  },
  copilot: {
    kind: 'flag',
    aliases: ['--allow-all', '--yolo'],
    canonicalFlag: '--yolo',
  },
  cursor: {
    kind: 'flag',
    aliases: ['--force', '--yolo'],
    canonicalFlag: '--yolo',
  },
  /**
   * CDXC:AgentLauncher 2026-08-27:
   * Devin does expose a permission bypass, but it is the two-token
   * `--permission-mode bypass` form. This spec shape only describes single
   * tokens, so a spec here could be appended but never matched, deduped, or
   * stripped again when Accept All is turned back off. Leave Devin
   * unsupported until the shared spec can carry flag/value pairs.
   */
  devin: null,
  /**
   * CDXC:AgentLauncher 2026-05-19-10:05:
   * Factory Droid only documents permission bypass on `droid exec`, not the
   * interactive `droid` command used by the built-in launcher.
   */
  droid: null,
  gemini: {
    kind: 'flag',
    aliases: ['-y', '--yolo'],
    canonicalFlag: '--yolo',
  },
  grok: {
    kind: 'flag',
    aliases: ['--always-approve'],
    canonicalFlag: '--always-approve',
  },
  'hermes-agent': null,
  kimi: {
    kind: 'flag',
    aliases: ['--auto-approve', '--yes', '-y', '--yolo'],
    canonicalFlag: '--yolo',
  },
  kiro: null,
  omp: null,
  /**
   * CDXC:AgentLauncher 2026-08-27:
   * OpenClaude is a Claude-shaped fork down to its hook contract and its
   * `~/.openclaude/settings.json`, and it takes Claude's permission-bypass
   * flag verbatim, so it gets Claude's spec instead of its own dialect.
   */
  openclaude: {
    kind: 'flag',
    aliases: ['--dangerously-skip-permissions'],
    canonicalFlag: '--dangerously-skip-permissions',
  },
  opencode: { kind: 'runtimeConfig' },
  pi: null,
  qoder: null,
  rovodev: null,
};

export function normalizeAgentAcceptAllMode(candidate: unknown): AgentAcceptAllMode | undefined {
  return candidate === 'inherit' || candidate === 'enabled' || candidate === 'disabled' ? candidate : undefined;
}

export function resolveAgentAcceptAllSpec(agentId: string, icon?: SidebarAgentIcon): AgentAcceptAllSpec | undefined {
  const defaultAgent = getDefaultSidebarAgentById(agentId);
  const specFromId = defaultAgent ? AGENT_ACCEPT_ALL_SPECS[defaultAgent.agentId] : undefined;
  if (specFromId) {
    return specFromId;
  }

  if (!icon || icon === 'browser') {
    return undefined;
  }

  const iconAgentId = getDefaultSidebarAgentByIcon(icon)?.agentId;
  if (!iconAgentId) {
    return undefined;
  }

  const spec = AGENT_ACCEPT_ALL_SPECS[iconAgentId as DefaultSidebarAgentId];
  return spec ?? undefined;
}

export function resolveAgentAcceptAllFlagSpec(
  agentId: string,
  icon?: SidebarAgentIcon
): AgentAcceptAllFlagSpec | undefined {
  const spec = resolveAgentAcceptAllSpec(agentId, icon);
  return spec?.kind === 'flag' ? spec : undefined;
}

export function supportsAgentAcceptAll(agentId: string, icon?: SidebarAgentIcon): boolean {
  return resolveAgentAcceptAllSpec(agentId, icon) !== undefined;
}
