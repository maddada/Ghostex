// Send classification (upstream chat spec §11.6 port, catalog-driven).
// firstToken is NOT trimmed — a leading space means prose, because the agent
// TUIs only treat LINE-LEADING tokens as commands.

export type SessionChatSendClassification = 'chat' | 'command' | 'unknown-token';

/**
 * Default verified catalog used for local "Ran /x" markers. Only catalog
 * commands get a marker (and skip the optimistic echo); an unknown /token is
 * sent as-is with no echo and no marker.
 */
export const SESSION_CHAT_DEFAULT_COMMAND_CATALOG: readonly string[] = ['clear', 'compact', 'exit', 'help', 'model'];

export function classifySessionChatSend(
  draft: string,
  catalogCommandNames: readonly string[] = SESSION_CHAT_DEFAULT_COMMAND_CATALOG,
  skillPrefix?: string
): SessionChatSendClassification {
  const firstToken = draft.split(/\s/, 1)[0] ?? '';
  if (catalogCommandNames.some((name) => firstToken === `/${name}`)) {
    return 'command';
  }
  if (firstToken.startsWith('/')) {
    return 'unknown-token';
  }
  if (skillPrefix === '$' && firstToken.startsWith('$')) {
    // `$` is Codex grammar only; elsewhere `$PATH` is prose.
    return 'unknown-token';
  }
  return 'chat';
}
