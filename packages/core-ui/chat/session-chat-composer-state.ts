// Composer draft history recall (upstream chat spec §11.6 port).

export interface SessionChatComposerHistory {
  entries: readonly string[];
  index: number | null;
}

export const EMPTY_SESSION_CHAT_COMPOSER_HISTORY: SessionChatComposerHistory = {
  entries: [],
  index: null,
};

export function pushSessionChatComposerHistory(
  history: SessionChatComposerHistory,
  sent: string
): SessionChatComposerHistory {
  if (sent.trim() === '' || history.entries.at(-1) === sent) {
    return { entries: history.entries, index: null };
  }
  return { entries: [...history.entries, sent], index: null };
}

export function recallPreviousSessionChatDraft(
  history: SessionChatComposerHistory
): { history: SessionChatComposerHistory; draft: string } | null {
  if (history.entries.length === 0) {
    return null;
  }
  const index = history.index === null ? history.entries.length - 1 : Math.max(0, history.index - 1);
  return {
    draft: history.entries[index] ?? '',
    history: { entries: history.entries, index },
  };
}

export function recallNextSessionChatDraft(
  history: SessionChatComposerHistory
): { history: SessionChatComposerHistory; draft: string } | null {
  if (history.index === null) {
    return null;
  }
  const index = history.index + 1;
  if (index >= history.entries.length) {
    // Back to blank.
    return { draft: '', history: { entries: history.entries, index: null } };
  }
  return {
    draft: history.entries[index] ?? '',
    history: { entries: history.entries, index },
  };
}

/** Any manual edit resets the recall cursor (keeps entries). */
export function resetSessionChatComposerHistoryIndex(history: SessionChatComposerHistory): SessionChatComposerHistory {
  return history.index === null ? history : { entries: history.entries, index: null };
}
