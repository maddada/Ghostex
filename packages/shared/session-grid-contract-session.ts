import {
  DEFAULT_AGENT_MANAGER_ZOOM_PERCENT,
  DEFAULT_MAIN_GROUP_ID,
  DEFAULT_MAIN_GROUP_TITLE,
  GRID_COLUMN_COUNT,
  MAX_AGENT_MANAGER_ZOOM_PERCENT,
  MAX_SESSION_DISPLAY_ID_COUNT,
  MIN_AGENT_MANAGER_ZOOM_PERCENT,
  type BaseSessionRecord,
  type CreateSessionRecordOptions,
  type GroupedSessionWorkspaceSnapshot,
  type SessionGridSnapshot,
  type SessionRecord,
  type SessionTitleSource,
  type SidebarTheme,
  type SidebarThemeSetting,
  type SidebarThemeVariant,
  type TerminalEngine,
  type TerminalSurface,
  type TerminalSessionPersistenceProvider,
  type TerminalSessionRecord,
  type BrowserSessionRecord,
  type TerminalViewMode,
  type VisibleSessionCount,
} from './session-grid-contract-core';

/**
 * CDXC:SessionStatus 2026-04-25-08:29
 * Visible session titles should remove agent status glyphs, including Claude
 * Code's animated star markers, while the activity parser uses them for
 * working/done indicators.
 * CDXC:RepoStructure 2026-05-09-15:53
 * Use working for agent work status. Reserve running for live runtime state.
 *
 * CDXC:AgentProviders 2026-05-19-15:22:
 * Cursor CLI appends ` - ⏳ Working <spinner>` and ` - ✅ Ready` to terminal titles,
 * including the startup title `Cursor Agent - ✅ Ready`. Strip those trailing status
 * suffixes for visible sidebar titles. Keep the gxserver session-status title
 * classifier aligned with these strip rules.
 *
 * CDXC:AgentProviders 2026-05-19-18:45:
 * Antigravity CLI uses `agy` while running and `🔔 agy` when finished. Strip the
 * bell attention prefix for visible sidebar titles while gxserver keeps the raw
 * terminal title for attention detection.
 */
/*
 * CDXC:AgentProviders 2026-06-01-12:59:
 * The sidebar status marker uses the mathematical asterisk `∗`, not only ASCII `*`.
 * Strip it before title trust checks so display markers cannot make placeholder titles look like real restore lookup titles.
 */
const LEADING_TERMINAL_TITLE_STATUS_MARKER_PATTERN = /^[\s\u2800-\u28ff·•⋅◦✳*∗✶✻✽✸✹✺✷✴◐◑◒◓✦◇🤖🔔]+/u;
const ANTIGRAVITY_ATTENTION_TITLE_PATTERN = /^🔔\s*agy$/iu;
const ANTIGRAVITY_IDLE_TITLE_PATTERN = /^agy$/iu;
const LEADING_TERMINAL_TITLE_PREFIX_PATTERN = /^(?:OC\s*\|\s*)+/iu;
const CURSOR_CLI_WORKING_TITLE_SUFFIX_PATTERN = /⏳ Working [.·]+$/u;
const CURSOR_CLI_READY_TITLE_SUFFIX_PATTERN = /✅ Ready$/u;
const CURSOR_CLI_AGENT_READY_TITLE_PATTERN = /^Cursor Agent\s*-\s*✅ Ready$/iu;
const CURSOR_CLI_AGENT_TITLE_PATTERN = /^Cursor Agent$/iu;
const CURSOR_CLI_WORKING_TITLE_STRIP_PATTERN = /\s*-\s*⏳ Working [.·]+$/u;
const CURSOR_CLI_READY_TITLE_STRIP_PATTERN = /\s*-\s*✅ Ready$/u;
export const DEFAULT_TERMINAL_SESSION_TITLE = 'Terminal Session';
/**
 * CDXC:CommandPane 2026-05-16-07:36:
 * Users need the bottom command pane to shrink to 5% of the window height.
 *
 * CDXC:CommandPane 2026-05-30-09:20:
 * The command pane may grow up to 90% of the workspace height. Opening the pane and double-clicking the top resize rail restore a pixel default height from Settings, not a percentage of the window.
 *
 * CDXC:CommandPane 2026-05-30-09:45:
 * The built-in default height is 125px until the user changes Command Pane Default Height in Workspace settings.
 */
export const MIN_COMMANDS_PANEL_HEIGHT_RATIO = 0.05;
export const MAX_COMMANDS_PANEL_HEIGHT_RATIO = 0.9;
export const DEFAULT_COMMANDS_PANEL_HEIGHT_PX = 125;
/**
 * CDXC:CommandPane 2026-05-30-09:20:
 * Persisted command-pane ratios are normalized before the native workspace reports its height. Seed defaults from this reference height so first paint matches the 125px target on typical windows.
 */
export const DEFAULT_COMMANDS_PANEL_REFERENCE_WORKSPACE_HEIGHT_PX = 900;

export function resolveDefaultCommandsPanelHeightRatio(
  workspaceHeightPx?: number,
  defaultHeightPx: number = DEFAULT_COMMANDS_PANEL_HEIGHT_PX
): number {
  const workspaceHeight =
    typeof workspaceHeightPx === 'number' && Number.isFinite(workspaceHeightPx) && workspaceHeightPx > 0
      ? workspaceHeightPx
      : DEFAULT_COMMANDS_PANEL_REFERENCE_WORKSPACE_HEIGHT_PX;
  const resolvedDefaultHeightPx =
    Number.isFinite(defaultHeightPx) && defaultHeightPx > 0 ? defaultHeightPx : DEFAULT_COMMANDS_PANEL_HEIGHT_PX;
  return Math.max(
    MIN_COMMANDS_PANEL_HEIGHT_RATIO,
    Math.min(MAX_COMMANDS_PANEL_HEIGHT_RATIO, resolvedDefaultHeightPx / workspaceHeight)
  );
}

export const DEFAULT_COMMANDS_PANEL_HEIGHT_RATIO = resolveDefaultCommandsPanelHeightRatio();
const DEFAULT_TERMINAL_ENGINE: TerminalEngine = 'ghostty-native';
const IGNORED_GENERIC_TERMINAL_TITLES = new Set([
  'amp',
  'amp cli',
  'agy',
  'antigravity',
  'antigravity cli',
  'claude',
  'claude code',
  'codex',
  'codex cli',
  'cursor',
  'cursor agent',
  'cursor cli',
  'cursor-agent',
  'droid',
  'factory droid',
  'grok',
  'grok build',
  'openai codex',
  'kiro',
  'kiro cli',
  'kiro-cli',
  'omp',
  'pi',
  'π',
  'ghostex',
]);
const IGNORED_PLACEHOLDER_SESSION_TITLES = new Set([
  DEFAULT_TERMINAL_SESSION_TITLE.toLowerCase(),
  'amp cli session',
  'amp session',
  'antigravity cli session',
  'antigravity session',
  'campfire session',
  'claude session',
  'claude code session',
  'codebuddy session',
  'code buddy session',
  'codex session',
  'codex cli session',
  'command code session',
  'commandcode session',
  'copilot session',
  'cursor agent session',
  'cursor cli session',
  'cursor session',
  'mastra session',
  'mastra code session',
  'devin session',
  'droid session',
  'factory droid session',
  'gemini session',
  'grok session',
  'grok build session',
  'hermes session',
  'hermes agent session',
  'kimi session',
  'kimi code session',
  'kiro session',
  'kiro cli session',
  'omp session',
  'openclaude session',
  'open claude session',
  'opencode session',
  'open code session',
  'openai codex session',
  'pi session',
  'qoder session',
  'qodercli session',
  'rovo session',
  'rovo dev session',
  'rovodev session',
]);
const DEFAULT_SESSION_AGENT_TITLE_NAMES = new Map<string, string>([
  ['agy', 'Antigravity CLI'],
  ['amp', 'Amp CLI'],
  ['amp-cli', 'Amp CLI'],
  ['antigravity', 'Antigravity CLI'],
  ['antigravity-cli', 'Antigravity CLI'],
  ['campfire', 'Campfire'],
  ['claude', 'Claude'],
  ['claude-code', 'Claude'],
  ['codebuddy', 'CodeBuddy'],
  ['code-buddy', 'CodeBuddy'],
  ['codex', 'Codex'],
  ['codex-cli', 'Codex'],
  ['command-code', 'Command Code'],
  ['commandcode', 'Command Code'],
  ['copilot', 'Copilot'],
  ['cursor', 'Cursor CLI'],
  ['cursor-cli', 'Cursor CLI'],
  ['mastra', 'Mastra Code'],
  ['mastracode', 'Mastra Code'],
  ['devin', 'Devin'],
  ['droid', 'Factory Droid'],
  ['factory-droid', 'Factory Droid'],
  ['gemini', 'Gemini'],
  ['grok', 'Grok Build'],
  ['grok-build', 'Grok Build'],
  ['hermes', 'Hermes Agent'],
  ['hermes-agent', 'Hermes Agent'],
  ['kimi', 'Kimi Code'],
  ['kimi-code', 'Kimi Code'],
  ['kiro', 'Kiro CLI'],
  ['kiro-cli', 'Kiro CLI'],
  ['omp', 'OMP'],
  ['openclaude', 'OpenClaude'],
  ['open-claude', 'OpenClaude'],
  ['opencode', 'OpenCode'],
  ['open-code', 'OpenCode'],
  ['pi', 'Pi'],
  ['qoder', 'Qoder'],
  ['qodercli', 'Qoder'],
  ['rovo', 'Rovo Dev'],
  ['rovo-dev', 'Rovo Dev'],
  ['rovodev', 'Rovo Dev'],
  ['π', 'Pi'],
]);
const DEFAULT_SESSION_SEARCH_PLACEHOLDER_TITLES = createDefaultSessionSearchPlaceholderTitles();
const ELLIPSIZED_PATH_TITLE_PATTERN = /^(?:…|\.\.\.)[\\/]/u;
const WINDOWS_DEFAULT_POWERSHELL_TITLE_PATTERN =
  /^[a-z]:\\windows\\system32\\windowspowershell\\v1\.0\\powershell\.exe(?:\s+\.)?$/iu;
const AGENT_STATUS_WORD_TITLE_PATTERN =
  /^(?:[\s.:[\](){}!|/\\_-]*)(?:done|error|idle|thinking|working)(?:[\s.:[\](){}!|/\\_-]*)$/iu;
const GHOST_PLACEHOLDER_SESSION_TITLE_PATTERN = /^👻(?:\s+Terminal Session)?$/u;
const CODEX_SESSION_ID_TITLE_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/iu;

export function clampVisibleSessionCount(value: number): VisibleSessionCount {
  /**
   * CDXC:Workarea 2026-05-11-17:14
   * Workspace visibility no longer has a fixed pane cap. Clamp only to a
   * positive integer so tab groups and native pane layouts can keep every
   * session the user opens.
   */
  if (!Number.isFinite(value)) {
    return 1;
  }

  return Math.max(1, Math.round(value));
}

export function clampTerminalViewMode(value: string | undefined): TerminalViewMode {
  switch (value) {
    case 'horizontal':
    case 'vertical':
    case 'grid':
      return value;
    default:
      return 'grid';
  }
}

export function clampAgentManagerZoomPercent(value: number | undefined): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return DEFAULT_AGENT_MANAGER_ZOOM_PERCENT;
  }

  return Math.min(MAX_AGENT_MANAGER_ZOOM_PERCENT, Math.max(MIN_AGENT_MANAGER_ZOOM_PERCENT, Math.round(value)));
}

export function clampSidebarThemeSetting(value: string | undefined): SidebarThemeSetting {
  switch (value) {
    case 'auto':
      /**
       * CDXC:Theming 2026-06-15-02:29:
       * Theme selection is disabled again while themes are coming soon, so
       * legacy Auto should resolve to the active Dark Gray/Dark 2 chrome.
       */
      return 'dark-2';
    case 'plain':
      /**
       * CDXC:Theming 2026-06-15-01:43:
       * The old persisted "plain" setting is the current shipped dark surface.
       * Keep it as the Dark 2 snapshot so existing users do not silently move
       * off the exact #0e0e0e chrome they selected before Dark 1 became the
       * default.
       */
      return 'dark-2';
    case 'dark-2':
      return 'dark-2';
    case 'dark-1':
    case 'plain-light':
    case 'dark-modern':
    case 'dark-green':
      /**
       * CDXC:Theming 2026-06-15-02:29:
       * Hidden theme values may exist from prior builds or settings files, but
       * the disabled Settings control must keep the app on Dark Gray/Dark 2.
       */
      return 'dark-2';
    case 'dark-plus':
    case 'dark-blue':
    case 'dark-red':
    case 'dark-pink':
    case 'dark-orange':
    case 'light-plus':
    case 'light-blue':
    case 'light-green':
    case 'light-pink':
    case 'light-orange':
    case 'monokai':
    case 'solarized-dark':
      return 'dark-2';
    default:
      return 'dark-2';
  }
}

export function resolveSidebarTheme(themeSetting: SidebarThemeSetting, variant: SidebarThemeVariant): SidebarTheme {
  if (themeSetting === 'auto') {
    /**
     * CDXC:Theming 2026-06-15-02:29:
     * Theme selection is disabled again; direct Auto callers should use the
     * same active Dark Gray/Dark 2 chrome as normalized settings.
     */
    return 'dark-2';
  }

  if (themeSetting === 'plain') {
    /**
     * CDXC:Theming 2026-06-15-01:43:
     * Preserve old callers that pass the legacy "plain" value directly by
     * resolving it to Dark 2, the snapshot of the previous #0e0e0e app chrome.
     */
    return 'dark-2';
  }

  return themeSetting;
}

export function createDefaultSessionGridSnapshot(): SessionGridSnapshot {
  return {
    focusedSessionId: undefined,
    fullscreenRestoreVisibleCount: undefined,
    sessions: [],
    visibleCount: 1,
    visibleSessionIds: [],
    viewMode: 'grid',
  };
}

export function createDefaultCommandsPanelState(options?: { defaultHeightPx?: number; workspaceHeightPx?: number }) {
  return {
    activeSessionId: undefined,
    heightRatio: resolveDefaultCommandsPanelHeightRatio(options?.workspaceHeightPx, options?.defaultHeightPx),
    isVisible: false,
    mode: 'pinned' as const,
    paneLayout: undefined,
    sessions: [],
  };
}

export function isSessionGridFocusModeActive(
  snapshot: Pick<SessionGridSnapshot, 'fullscreenRestoreVisibleCount' | 'visibleCount'>
): boolean {
  return snapshot.visibleCount === 1 && snapshot.fullscreenRestoreVisibleCount !== undefined;
}

export function getSessionGridLayoutVisibleCount(
  snapshot: Pick<SessionGridSnapshot, 'fullscreenRestoreVisibleCount' | 'visibleCount'>
): VisibleSessionCount {
  return snapshot.fullscreenRestoreVisibleCount ?? snapshot.visibleCount;
}

export function createDefaultGroupedSessionWorkspaceSnapshot(): GroupedSessionWorkspaceSnapshot {
  return {
    activeGroupId: DEFAULT_MAIN_GROUP_ID,
    groups: [
      {
        groupId: DEFAULT_MAIN_GROUP_ID,
        snapshot: createDefaultSessionGridSnapshot(),
        title: DEFAULT_MAIN_GROUP_TITLE,
      },
    ],
    nextGroupNumber: 2,
    nextSessionDisplayId: 0,
    nextSessionNumber: 1,
  };
}

/**
 * CDXC:SessionIdentity 2026-04-26-20:54
 * New workspace sessions use one opaque, timestamped ID for sessionId,
 * displayId, and the generated alias so daemon state, socket routing, and
 * sidebar labels do not reuse numeric identities from closed sessions.
 *
 * CDXC:SessionIdentity 2026-05-15-17:33
 * Session IDs and provider-backed default names should stay short enough to
 * read in tmux, zmx, zellij, pane overlays, and sidebar metadata. Generate
 * `g-MMDD-HHMMSS` as the canonical identity anywhere a new session ID is used.
 */
export function createTimestampedSessionId(
  usedSessionIds: Iterable<string>,
  now = new Date(),
  _random = Math.random
): string {
  const usedSessionIdSet = new Set(usedSessionIds);

  for (let secondOffset = 0; secondOffset < 24 * 60 * 60; secondOffset += 1) {
    const sessionId = `g-${formatSessionIdTimestamp(addSeconds(now, secondOffset))}`;
    if (!usedSessionIdSet.has(sessionId)) {
      return sessionId;
    }
  }

  throw new Error('Unable to allocate a unique ghostex session ID for this timestamp.');
}

export function formatSessionDisplayId(displayId: number | string): string {
  if (typeof displayId === 'string') {
    const trimmedDisplayId = displayId.trim();
    if (/^(?:g|s)-[a-z0-9-]+$/i.test(trimmedDisplayId)) {
      return trimmedDisplayId;
    }

    if (/^\d{2}$/.test(trimmedDisplayId)) {
      return trimmedDisplayId;
    }

    const parsedDisplayId = Number.parseInt(trimmedDisplayId, 10);
    if (Number.isInteger(parsedDisplayId)) {
      return formatSessionDisplayId(parsedDisplayId);
    }
  }

  if (!Number.isFinite(Number(displayId))) {
    return '00';
  }

  const normalizedDisplayId =
    ((Math.floor(Number(displayId)) % MAX_SESSION_DISPLAY_ID_COUNT) + MAX_SESSION_DISPLAY_ID_COUNT) %
    MAX_SESSION_DISPLAY_ID_COUNT;
  return String(normalizedDisplayId).padStart(2, '0');
}

export function getSlotPosition(slotIndex: number): Pick<SessionRecord, 'column' | 'row'> {
  const normalizedSlotIndex = Math.max(0, Math.floor(slotIndex));
  return {
    column: normalizedSlotIndex % GRID_COLUMN_COUNT,
    row: Math.floor(normalizedSlotIndex / GRID_COLUMN_COUNT),
  };
}

export function getSlotLabel(row: number, column: number): string {
  return `R${row + 1}C${column + 1}`;
}

export function getSessionShortcutLabel(slotIndex: number, platform: 'default' | 'mac'): string {
  const shortcutNumber = Math.max(1, Math.floor(slotIndex) + 1);
  return platform === 'mac' ? `⌘⌥${shortcutNumber}` : `⌃⌥${shortcutNumber}`;
}

export function createSessionAlias(sessionNumber: number, slotIndex: number, displayId?: string | number): string {
  void slotIndex;
  return formatSessionDisplayId(displayId ?? sessionNumber - 1);
}

/**
 * CDXC:SessionTitles 2026-04-27-03:58
 * Newly created sessions should not expose their routing/session id as the
 * primary title. Creation source owns the default title until the live terminal
 * emits a meaningful title or the user explicitly renames the session.
 */
export function createAgentSessionDefaultTitle(agentName: string | undefined): string {
  const normalizedAgentName = agentName?.replace(/\s+/g, ' ').trim();
  const defaultAgentTitleName = normalizedAgentName
    ? (DEFAULT_SESSION_AGENT_TITLE_NAMES.get(normalizedAgentName.toLowerCase()) ?? normalizedAgentName)
    : undefined;
  return defaultAgentTitleName ? `${defaultAgentTitleName} Session` : DEFAULT_TERMINAL_SESSION_TITLE;
}

function createDefaultSessionSearchPlaceholderTitles(): Set<string> {
  const titles = new Set(IGNORED_PLACEHOLDER_SESSION_TITLES);
  for (const title of IGNORED_PLACEHOLDER_SESSION_TITLES) {
    if (title === DEFAULT_TERMINAL_SESSION_TITLE.toLowerCase() || !title.endsWith(' session')) {
      continue;
    }
    titles.add(title.replace(/\s+session$/u, ' agent session'));
  }
  for (const agentTitleName of new Set(DEFAULT_SESSION_AGENT_TITLE_NAMES.values())) {
    const normalizedAgentTitleName = agentTitleName.trim().replace(/\s+/g, ' ').toLowerCase();
    if (!normalizedAgentTitleName) {
      continue;
    }
    titles.add(`${normalizedAgentTitleName} session`);
    titles.add(`${normalizedAgentTitleName} agent session`);
    if (normalizedAgentTitleName.endsWith(' cli')) {
      titles.add(`${normalizedAgentTitleName.slice(0, -' cli'.length)} agent session`);
    }
  }
  return titles;
}

export function isNumericSessionAlias(alias: string | undefined): boolean {
  return /^\d+$/.test(alias?.trim() ?? '');
}

export function isGeneratedSessionAlias(
  session: Pick<BaseSessionRecord, 'alias' | 'displayId' | 'sessionId' | 'slotIndex'>
): boolean {
  return session.alias.trim() === createSessionAlias(getSessionNumber(session), session.slotIndex, session.displayId);
}

export function createSessionRecord(
  sessionNumber: number,
  slotIndex: number,
  options?: CreateSessionRecordOptions
): SessionRecord {
  const position = getSlotPosition(slotIndex);
  const terminalAgentName =
    options?.kind === 'terminal' || options?.kind === undefined ? options?.agentName : undefined;
  const sessionId = options?.sessionId?.trim() || `session-${sessionNumber}`;
  const displayId = formatSessionDisplayId(
    options?.displayId ?? (isTimestampedSessionId(sessionId) ? sessionId : sessionNumber - 1)
  );
  const alias = createSessionAlias(sessionNumber, slotIndex, displayId);
  const createdAt = new Date().toISOString();
  const lastStartedAt = createdAt;
  const lastAccessedAt = createdAt;
  const title = options?.title?.trim() || DEFAULT_TERMINAL_SESSION_TITLE;
  const titleSource = normalizeSessionTitleSource(options?.titleSource, title);

  if (options?.kind === 'browser') {
    return {
      alias,
      browser: options.browser,
      column: position.column,
      createdAt,
      displayId,
      kind: 'browser',
      lastAccessedAt,
      lastStartedAt,
      row: position.row,
      sessionId,
      slotIndex,
      title,
      titleSource,
    };
  }

  return {
    alias,
    agentName: normalizeTerminalSessionAgentName(terminalAgentName),
    agentSessionId: normalizeTerminalAgentSessionIdentity(options?.agentSessionId),
    agentSessionPath: normalizeTerminalAgentSessionIdentity(options?.agentSessionPath),
    commandTitle: normalizeTerminalCommandTitle(options?.commandTitle),
    column: position.column,
    createdAt,
    closeAfterDone: options?.closeAfterDone === true ? true : undefined,
    delayedSendDeadlineAt: normalizeTerminalDelayedSendDeadlineAt(options?.delayedSendDeadlineAt),
    delayedSendRemainingMs: normalizeTerminalDelayedSendRemainingMs(options?.delayedSendRemainingMs),
    displayId,
    isFavorite: options?.sessionTag === 'favorite' ? true : undefined,
    kind: 'terminal',
    lastAccessedAt,
    lastStartedAt,
    row: position.row,
    sessionId,
    sessionTag: options?.kind === undefined || options?.kind === 'terminal' ? options?.sessionTag : undefined,
    slotIndex,
    terminalEngine: normalizeTerminalEngine(options?.terminalEngine),
    sessionPersistenceName: normalizeTerminalSessionPersistenceName(
      options?.sessionPersistenceName ?? options?.tmuxSessionName
    ),
    sessionPersistenceProvider: normalizeTerminalSessionPersistenceProvider(options?.sessionPersistenceProvider),
    surface: normalizeTerminalSurface(options?.surface),
    title,
    titleSource,
  };
}

function normalizeSessionTitleSource(source: SessionTitleSource | undefined, title: string): SessionTitleSource {
  if (
    source === 'browser-auto' ||
    source === 'generated' ||
    source === 'placeholder' ||
    source === 'terminal-auto' ||
    source === 'user'
  ) {
    return source;
  }
  return getVisiblePrimaryTitle(title) ? 'user' : 'placeholder';
}

export function normalizeTerminalSessionAgentName(value: string | undefined): string | undefined {
  const normalizedValue = value?.replace(/\s+/g, ' ').trim();
  return normalizedValue && normalizedValue.length > 0 ? normalizedValue : undefined;
}

function normalizeTerminalCommandTitle(value: string | undefined): string | undefined {
  const normalizedValue = value?.replace(/\s+/g, ' ').trim();
  return normalizedValue && normalizedValue.length > 0 ? normalizedValue : undefined;
}

function normalizeTerminalDelayedSendDeadlineAt(value: string | undefined): string | undefined {
  const normalizedValue = value?.trim();
  return normalizedValue && !Number.isNaN(Date.parse(normalizedValue)) ? normalizedValue : undefined;
}

function normalizeTerminalDelayedSendRemainingMs(value: number | undefined): number | undefined {
  if (value === undefined || !Number.isFinite(value) || value <= 0) {
    return undefined;
  }
  return Math.ceil(value);
}

export function normalizeTerminalAgentSessionIdentity(value: string | undefined): string | undefined {
  const normalizedValue = value?.trim();
  return normalizedValue && normalizedValue.length > 0 ? normalizedValue : undefined;
}

function normalizeTerminalSessionPersistenceName(value: string | undefined): string | undefined {
  const normalizedValue = value?.trim();
  return normalizedValue && normalizedValue.length > 0 ? normalizedValue : undefined;
}

export function normalizeTerminalSessionPersistenceProvider(
  provider: TerminalSessionPersistenceProvider | undefined
): TerminalSessionPersistenceProvider | undefined {
  /**
   * CDXC:Workarea 2026-05-07-20:32
   * Provider-backed terminal records persist the provider with the provider
   * session name. Restore, wake, reload, and sidebar badges must not reinterpret
   * a stored tmux/zmx/zellij name through the current global Settings provider.
   */
  return provider === 'tmux' || provider === 'zmx' || provider === 'zellij' ? provider : undefined;
}

export function normalizeTerminalEngine(value: string | undefined): TerminalEngine {
  return DEFAULT_TERMINAL_ENGINE;
}

export function normalizeTerminalSurface(value: TerminalSurface | undefined): TerminalSurface {
  return value === 'commands' ? 'commands' : 'workspace';
}

export function isPersistentTerminalEngine(value: TerminalEngine): boolean {
  return value === 'ghostty-native';
}

export function isXtermTerminalEngine(value: TerminalEngine): boolean {
  return false;
}

export function getTerminalSessionSurfaceTitle(
  session: Pick<BaseSessionRecord, 'alias' | 'displayId' | 'sessionId' | 'slotIndex' | 'title'>
): string {
  return formatSessionSurfaceTitle(session);
}

export function getSessionNumberFromSessionId(sessionId: string): number | undefined {
  const match = /^session-(\d+)$/.exec(sessionId);
  if (!match) {
    return undefined;
  }

  const parsedNumber = Number.parseInt(match[1], 10);
  return Number.isInteger(parsedNumber) && parsedNumber > 0 ? parsedNumber : undefined;
}

export function getVisibleSessionNumber(
  session: Pick<BaseSessionRecord, 'displayId' | 'sessionId' | 'slotIndex'>
): string {
  return formatSessionDisplayId(session.displayId ?? getSessionNumber(session) - 1);
}

export function getVisiblePrimaryTitle(title: string): string | undefined {
  const normalizedTitle = title.trim();
  if (!normalizedTitle || isIgnoredPlaceholderSessionTitle(normalizedTitle)) {
    return undefined;
  }

  return normalizedTitle;
}

export function getSessionCardPrimaryTitle(
  session: Pick<BaseSessionRecord, 'title'> & { agentName?: string }
): string | undefined {
  const normalizedTitle = session.title.trim().replace(/\s+/g, ' ');
  /**
   * CDXC:SessionTitles 2026-04-27-08:31
   * Session cards still need a human placeholder while resume/persistence code
   * treats placeholders and Ghostty cwd titles as not persisted. Show the
   * neutral or agent-aware placeholder with the card's unsynced marker instead
   * of falling through to opaque ids such as `g-0427-090032`.
   */
  if (
    !normalizedTitle ||
    /^Session \d+$/iu.test(normalizedTitle) ||
    getCodexSessionIdFromTitle(normalizedTitle) !== undefined ||
    isGhostPlaceholderSessionTitle(normalizedTitle) ||
    isIgnoredGenericAgentTerminalTitle(normalizedTitle) ||
    isPathLikeTerminalTitle(normalizedTitle)
  ) {
    return createAgentSessionDefaultTitle(session.agentName);
  }

  return normalizedTitle;
}

/**
 * CDXC:SessionTitles 2026-05-07-17:27
 * zmx reconnect can emit the Ghostty placeholder title as `👻` before the pane
 * reports a persisted session title. Treat the ghost forms as placeholders in
 * the shared title contract so they cannot outrank known stored names, render
 * as real card titles, or be persisted by terminal-title sync.
 */
export function isGhostPlaceholderSessionTitle(title: string): boolean {
  const normalizedTitle = title.trim().replace(/\s+/g, ' ');
  return GHOST_PLACEHOLDER_SESSION_TITLE_PATTERN.test(normalizedTitle);
}

export function isDefaultSessionSearchTitle(title: string | undefined): boolean {
  const normalizedTitle = title?.trim().replace(/\s+/g, ' ');
  if (!normalizedTitle) {
    return false;
  }
  /*
   * CDXC:PromptSearch 2026-06-18-00:01:
   * Session search should be a jump-to-named-work surface. Creation defaults
   * from supported agent CLIs, including `Pi Agent Session`, are placeholders
   * and must not appear as search hits until the agent or user gives the
   * session a meaningful title.
   */
  return isIgnoredPlaceholderSessionTitle(normalizedTitle);
}

const SESSION_RENAME_UNSUPPORTED_GLYPH_PATTERN = /[^\p{L}\p{N} !"#$%&'()*+,\-.\/:;<=>?@[\\\]^_`{|}~]/gu;

/**
 * CDXC:Sessions 2026-05-15-16:15
 * Direct Rename submissions from pasted modal text must store a plain session
 * name: keep letters, numbers, spaces, and simple ASCII punctuation; drop
 * decorative glyphs such as bullet and mathematical asterisk symbols; trim the
 * result; remove line breaks; and collapse repeated whitespace to one space.
 */
export function normalizeSessionRenameTitle(title: string | undefined): string | undefined {
  const normalizedTitle = title
    ?.normalize('NFKC')
    .replace(/\s+/gu, ' ')
    .replace(SESSION_RENAME_UNSUPPORTED_GLYPH_PATTERN, '')
    .replace(/\s+/gu, ' ')
    .trim();
  return normalizedTitle || undefined;
}

export function normalizeTerminalTitle(title: string | undefined): string | undefined {
  const normalizedTitle = title?.trim();
  if (!normalizedTitle) {
    return undefined;
  }

  const sanitizedTitle = normalizedTitle
    .replace(LEADING_TERMINAL_TITLE_STATUS_MARKER_PATTERN, '')
    .replace(LEADING_TERMINAL_TITLE_PREFIX_PATTERN, '')
    .trim();
  const cursorTitle = normalizeCursorTerminalTitle(sanitizedTitle);
  if (cursorTitle !== null) {
    return cursorTitle;
  }
  const antigravityTitle = normalizeAntigravityTerminalTitle(sanitizedTitle);
  if (antigravityTitle !== null) {
    return antigravityTitle;
  }
  return normalizePiTerminalTitle(sanitizedTitle) ?? (sanitizedTitle || undefined);
}

function normalizeAntigravityTerminalTitle(title: string): string | undefined | null {
  const normalizedTitle = title.trim().replace(/\s+/g, ' ');
  if (
    ANTIGRAVITY_ATTENTION_TITLE_PATTERN.test(normalizedTitle) ||
    ANTIGRAVITY_IDLE_TITLE_PATTERN.test(normalizedTitle)
  ) {
    return 'agy';
  }

  return null;
}

function isCursorCliPlaceholderTerminalTitle(title: string): boolean {
  const normalizedTitle = title.trim().replace(/\s+/g, ' ');
  return (
    CURSOR_CLI_AGENT_READY_TITLE_PATTERN.test(normalizedTitle) ||
    CURSOR_CLI_AGENT_TITLE_PATTERN.test(normalizedTitle) ||
    ['cursor', 'cursor agent', 'cursor cli', 'cursor-agent'].includes(normalizedTitle.toLowerCase())
  );
}

function isIgnoredGenericAgentTerminalTitle(title: string): boolean {
  /**
   * CDXC:AgentProviders 2026-05-19-19:05:
   * Agent boot titles such as `Codex`, `Claude Code`, and `Cursor Agent` are CLI
   * placeholders. They may drive agent detection, but must never persist as the
   * canonical session title or render as a user-named card heading.
   */
  return IGNORED_GENERIC_TERMINAL_TITLES.has(title.trim().replace(/\s+/g, ' ').toLowerCase());
}

function normalizeCursorTerminalTitle(title: string): string | undefined | null {
  const normalizedTitle = title.trim().replace(/\s+/g, ' ');
  if (isCursorCliPlaceholderTerminalTitle(normalizedTitle)) {
    return undefined;
  }
  if (CURSOR_CLI_READY_TITLE_SUFFIX_PATTERN.test(normalizedTitle)) {
    const strippedTitle = normalizedTitle.replace(CURSOR_CLI_READY_TITLE_STRIP_PATTERN, '').trim();
    return isCursorCliPlaceholderTerminalTitle(strippedTitle) ? undefined : strippedTitle || undefined;
  }
  if (CURSOR_CLI_WORKING_TITLE_SUFFIX_PATTERN.test(normalizedTitle)) {
    const strippedTitle = normalizedTitle.replace(CURSOR_CLI_WORKING_TITLE_STRIP_PATTERN, '').trim();
    return isCursorCliPlaceholderTerminalTitle(strippedTitle) ? undefined : strippedTitle || undefined;
  }

  return null;
}

function normalizePiTerminalTitle(title: string): string | undefined {
  const normalizedTitle = title.trim();
  const ompMatch = /^π\s*(?:>|[\u2800-\u28ff])\s*(.*)$/u.exec(normalizedTitle);
  if (ompMatch) {
    /**
     * CDXC:SessionTitles 2026-08-06:
     * Omp prefixes its human title with `π >` while idle and `π <braille>`
     * while working. The braille character is an animated spinner, so strip
     * the whole status prefix and trim the remaining title for every client.
     */
    return ompMatch[1]?.trim() || 'π';
  }

  const match = /^π\s*-\s*(.+)$/u.exec(normalizedTitle);
  if (!match) {
    return undefined;
  }

  const parts = match[1]
    .split(/\s+-\s+/u)
    .map((part) => part.trim())
    .filter(Boolean);
  /**
   * CDXC:AgentProviders 2026-05-08-09:42
   * Pi emits `π - <cwd>` before a session is named and
   * `π - <session name> - <cwd>` afterward. Keep unnamed Pi titles generic
   * while surfacing the named session portion as the sidebar title, matching
   * Codex's generated-title behavior without persisting the cwd as a name.
   */
  if (parts.length < 2) {
    return 'π';
  }

  return parts.slice(0, -1).join(' - ') || 'π';
}

/**
 * CDXC:AgentProviders 2026-05-11-07:35
 * Recent Codex CLI builds can publish the underlying conversation UUID as the
 * terminal title before a human title exists. Store that UUID as agent session
 * identity, but keep it out of visible titles so unnamed Codex cards continue
 * to render as `* Codex Session`.
 */
export function getCodexSessionIdFromTitle(title: string | undefined): string | undefined {
  const normalizedTitle = normalizeTerminalTitle(title);
  if (!normalizedTitle || !CODEX_SESSION_ID_TITLE_PATTERN.test(normalizedTitle)) {
    return undefined;
  }
  return normalizedTitle.toLowerCase();
}

export function getVisibleTerminalTitle(title: string | undefined): string | undefined {
  const normalizedTitle = normalizeTerminalTitle(title);
  if (!normalizedTitle) {
    return undefined;
  }

  if (isPathLikeTerminalTitle(normalizedTitle)) {
    return undefined;
  }

  if (isIgnoredPlaceholderSessionTitle(normalizedTitle)) {
    return undefined;
  }

  if (IGNORED_GENERIC_TERMINAL_TITLES.has(normalizedTitle.trim().toLowerCase())) {
    return undefined;
  }

  if (AGENT_STATUS_WORD_TITLE_PATTERN.test(normalizedTitle)) {
    return undefined;
  }

  if (WINDOWS_DEFAULT_POWERSHELL_TITLE_PATTERN.test(normalizedTitle)) {
    return undefined;
  }

  return normalizedTitle;
}

function isIgnoredPlaceholderSessionTitle(title: string): boolean {
  const normalizedTitle = title.trim().replace(/\s+/g, ' ');
  /**
   * CDXC:SessionTitles 2026-04-27-08:20
   * Neutral titles such as `Terminal Session` and agent-aware creation titles
   * such as `Codex Session` are placeholders from session creation. Path-like
   * Ghostty titles such as `…/dev/_active/agent-tiler` are also shell context,
   * not user-persisted names, so none of them should drive resume titles.
   */
  return (
    /^Session \d+$/iu.test(normalizedTitle) ||
    getCodexSessionIdFromTitle(normalizedTitle) !== undefined ||
    isGhostPlaceholderSessionTitle(normalizedTitle) ||
    AGENT_STATUS_WORD_TITLE_PATTERN.test(normalizedTitle) ||
    DEFAULT_SESSION_SEARCH_PLACEHOLDER_TITLES.has(normalizedTitle.toLowerCase()) ||
    isPathLikeTerminalTitle(normalizedTitle)
  );
}

function isPathLikeTerminalTitle(title: string): boolean {
  return /^(~|\/)/u.test(title) || ELLIPSIZED_PATH_TITLE_PATTERN.test(title);
}

export function getPreferredSessionTitle(
  sessionTitle: string | undefined,
  terminalTitle: string | undefined
): string | undefined {
  const visibleTerminalTitle = getVisibleTerminalTitle(terminalTitle);
  if (visibleTerminalTitle) {
    return visibleTerminalTitle;
  }

  return sessionTitle ? getVisiblePrimaryTitle(sessionTitle) : undefined;
}

export function getOrderedSessions(snapshot: SessionGridSnapshot): SessionRecord[] {
  return [...snapshot.sessions].sort((left, right) => left.slotIndex - right.slotIndex);
}

export function isTerminalSession(session: SessionRecord): session is TerminalSessionRecord {
  return session.kind === 'terminal';
}

export function isBrowserSession(session: SessionRecord): session is BrowserSessionRecord {
  return session.kind === 'browser';
}

function getSessionNumber(session: Pick<BaseSessionRecord, 'sessionId' | 'slotIndex'>): number {
  return getSessionNumberFromSessionId(session.sessionId) ?? session.slotIndex + 1;
}

function formatSessionSurfaceTitle(
  session: Pick<BaseSessionRecord, 'alias' | 'displayId' | 'sessionId' | 'slotIndex' | 'title'>
): string {
  const displayId = formatSessionDisplayId(session.displayId ?? getSessionNumber(session) - 1);
  const visiblePrimaryTitle = getVisiblePrimaryTitle(session.title);
  if (visiblePrimaryTitle) {
    return `${displayId}. ${visiblePrimaryTitle}`;
  }

  return isGeneratedSessionAlias(session) ? displayId : `${displayId} ${session.alias}`;
}

function formatSessionIdTimestamp(value: Date): string {
  const month = String(value.getMonth() + 1).padStart(2, '0');
  const day = String(value.getDate()).padStart(2, '0');
  const hour = String(value.getHours()).padStart(2, '0');
  const minute = String(value.getMinutes()).padStart(2, '0');
  const second = String(value.getSeconds()).padStart(2, '0');
  return `${month}${day}-${hour}${minute}${second}`;
}

function addSeconds(value: Date, seconds: number): Date {
  return new Date(value.getTime() + seconds * 1000);
}

function isTimestampedSessionId(sessionId: string): boolean {
  return /^g-\d{4}-\d{6}$/i.test(sessionId) || /^s-\d{6}-\d{6}-[a-z0-9]{3}$/i.test(sessionId);
}
