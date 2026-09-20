/**
 * The new-session welcome: the agent mark and the "What should we build with
 * X?" headline a chat shows while it has no transcript yet.
 *
 * CDXC:SessionChat 2026-09-18 WHY:
 * React owned this decision inline in session-chat-view.tsx, so the GPUI chat
 * had no way to reach it and fell through to the `emptyState` copy — a brand
 * new session greeted the user with "Loading conversation… / Reading the agent
 * transcript." forever instead of the welcome.
 * SEE-ALSO: packages/core-ui/chat/session-chat-new-session-welcome.tsx,
 * packages/shared/session-chat-controller/native-host.ts,
 * apps/desktop/src/app/native_chat/render.rs.
 */

import { sessionChatAgentIconId } from '../session-chat';
import { getDefaultSidebarAgentById, isSidebarAgentIcon, type SidebarAgentIcon } from '../sidebar-agents';

/**
 * A new agent reports `starting` until its first transcript file exists, and
 * `empty` once the file is there but still has no turns. The welcome owns both.
 */
export function sessionChatShowsNewSessionWelcome(viewKind: string | null | undefined): boolean {
  return viewKind === 'starting' || viewKind === 'empty';
}

/** The agent's display name, e.g. `claude-code` → `Claude Code`. */
export function sessionChatWelcomeAgentName(agentLabel?: string | null): string | null {
  const normalized = agentLabel?.trim();
  if (!normalized) {
    return null;
  }
  return (
    getDefaultSidebarAgentById(normalized)?.name ??
    normalized.replace(/[-_]+/g, ' ').replace(/\b\p{L}/gu, (letter) => letter.toLocaleUpperCase())
  );
}

/**
 * A draft's own agent row wins: a project custom agent has no entry in the
 * default agent table, so only the daemon's list knows its artwork. The read
 * state's label is the transcript family id, which is not always the sidebar
 * agent id the artwork is registered under, hence the `sessionChatAgentIconId`
 * fallback.
 */
export function sessionChatWelcomeAgentIcon(
  agentLabel?: string | null,
  agentIcon?: string | null
): SidebarAgentIcon | undefined {
  const defaultAgent = agentLabel ? getDefaultSidebarAgentById(agentLabel) : undefined;
  const familyIconId = sessionChatAgentIconId(agentLabel) ?? undefined;
  const familyIcon = isSidebarAgentIcon(familyIconId) ? familyIconId : undefined;
  return (isSidebarAgentIcon(agentIcon) ? agentIcon : undefined) ?? defaultAgent?.icon ?? familyIcon;
}

export function sessionChatNewSessionWelcomeTitle(agentName: string | null | undefined): string {
  const title = agentName ? `What should we build with ${agentName}?` : 'What should we work on?';
  return wrapNewSessionWelcomeTitle(title);
}

/**
 * CDXC:SessionChat 2026-09-20 DECISION:
 * User: when chat is very narrow, the welcome title wraps, is center aligned, and the second line has 2 or 3 words, never 1.
 * A 6+ word headline keeps 3 words on the last line ("What should we" / "build with Codex?"); shorter ones keep 2 ("What should we" / "work on?").
 * SEE-ALSO: apps/desktop/src/app/native_chat/new_session_welcome.rs, packages/core-ui/chat/session-chat-new-session-welcome.tsx.
 */
export function wrapNewSessionWelcomeTitle(title: string): string {
  if (title.includes('\n')) {
    return title;
  }
  const words = title.split(/\s+/).filter(Boolean);
  if (words.length < 4) {
    return title;
  }
  const lastCount = words.length >= 6 ? 3 : 2;
  const split = words.length - lastCount;
  return `${words.slice(0, split).join(' ')}\n${words.slice(split).join(' ')}`;
}

/**
 * How long a transcript read stays blank before the skeleton appears.
 *
 * CDXC:SessionChat 2026-09-19 DECISION:
 * User: the skeleton shows the moment a transcript starts loading, in both GPUI and React chat; the pane must react at once instead of holding blank. This supersedes the 600ms blank hold from the same day.
 */
export const SESSION_CHAT_LOADING_INDICATOR_DELAY_MS = 0;
/** How long a transcript read runs before the empty region offers Retry. */
export const SESSION_CHAT_LOADING_RETRY_DELAY_MS = 12_000;

export type SessionChatLoadingStage = 'blank' | 'indicator' | 'retry';
