import {
  getSidebarSessionLifecycleState,
  type SidebarSessionGroup,
  type SidebarSessionItem,
} from './session-grid-contract-sidebar';

type SidebarSessionDetailsGroup = Pick<SidebarSessionGroup, 'projectContext' | 'remoteMachineContext' | 'title'>;

const AGENT_LABELS: Record<string, string> = {
  browser: 'Browser',
  claude: 'Claude',
  codex: 'Codex',
  copilot: 'Copilot',
  'cursor-cli': 'Cursor CLI',
  gemini: 'Gemini',
  opencode: 'OpenCode',
  pi: 'Pi',
  terminal: 'Terminal',
};

/**
 * CDXC:ContextMenus 2026-06-11-23:08:
 * The Copy details action should copy stable session metadata from the rendered
 * sidebar row, not terminal output or the saved first user message. This keeps
 * the context-menu action useful for support/debug handoffs without silently
 * adding prompt text to the clipboard.
 */
export function buildSidebarSessionDetailsClipboardText(
  session: SidebarSessionItem,
  group?: SidebarSessionDetailsGroup
): string {
  const title = pickFirstNonEmpty(session.displayTitle, session.primaryTitle, session.alias);
  const lines: string[] = ['Ghostex Session'];

  appendLine(lines, 'Title', title);
  appendLine(lines, 'Alias', session.alias === title ? undefined : session.alias);
  appendLine(lines, 'Session ID', session.sessionId);
  appendLine(
    lines,
    'Routing ID',
    session.sessionRoutingId && session.sessionRoutingId !== session.sessionId ? session.sessionRoutingId : undefined
  );
  appendLine(lines, 'Kind', formatSessionKind(session));
  appendLine(lines, 'Status', getSidebarSessionLifecycleState(session));
  appendLine(lines, 'Activity', session.activityLabel ?? session.activity);
  appendLine(lines, 'Agent', session.agentIcon ? formatIdentifier(session.agentIcon) : undefined);
  appendLine(lines, 'Agent Session ID', session.agentSessionId);
  appendLine(
    lines,
    'Terminal Title',
    session.terminalTitle && session.terminalTitle !== title ? session.terminalTitle : undefined
  );
  appendLine(lines, 'Detail', session.detail);
  appendLine(lines, 'Persistence', formatPersistence(session));
  appendLine(lines, 'Remote Machine', group?.remoteMachineContext?.machineName);
  appendLine(lines, 'Project', group?.title);
  appendLine(lines, 'Project Path', group?.projectContext?.path);
  appendLine(lines, 'Worktree', group?.projectContext?.worktree?.name);
  appendLine(lines, 'Worktree Branch', group?.projectContext?.worktree?.branch);
  appendLine(lines, 'Parent Project', group?.projectContext?.worktree?.parentProjectName);
  appendLine(lines, 'Last Active', session.lastInteractionAt);

  return lines.join('\n');
}

function appendLine(lines: string[], label: string, value: string | undefined): void {
  const normalizedValue = normalizeClipboardField(value);
  if (!normalizedValue) {
    return;
  }
  lines.push(`${label}: ${normalizedValue}`);
}

function normalizeClipboardField(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

function pickFirstNonEmpty(...values: Array<string | undefined>): string {
  return values.map(normalizeClipboardField).find((value): value is string => Boolean(value)) ?? 'Session';
}

function formatSessionKind(session: SidebarSessionItem): string {
  return formatIdentifier(session.sessionKind ?? session.kind ?? 'terminal');
}

function formatPersistence(session: SidebarSessionItem): string | undefined {
  if (!session.sessionPersistenceProvider && !session.sessionPersistenceName) {
    return undefined;
  }
  if (session.sessionPersistenceProvider && session.sessionPersistenceName) {
    return `${session.sessionPersistenceProvider} (${session.sessionPersistenceName})`;
  }
  return session.sessionPersistenceProvider ?? session.sessionPersistenceName;
}

function formatIdentifier(value: string): string {
  const knownLabel = AGENT_LABELS[value];
  if (knownLabel) {
    return knownLabel;
  }
  return value
    .split(/[-_]/u)
    .filter(Boolean)
    .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(' ');
}
