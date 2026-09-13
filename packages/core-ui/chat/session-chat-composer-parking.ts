interface ParkedComposerSelection {
  text: string;
  start: number;
  end: number;
}
const prefix = 'ghostex.sessionChat.composerSelection.';

export function saveSessionChatComposerSelection(sessionKey: string, state: ParkedComposerSelection): void {
  const serialized = JSON.stringify(state);
  localStorage.setItem(prefix + sessionKey, serialized);
  if (localStorage.getItem(prefix + sessionKey) !== serialized)
    throw new Error('The draft selection could not be saved.');
}
export function readSessionChatComposerSelection(
  sessionKey: string | undefined,
  text: string
): ParkedComposerSelection | undefined {
  if (!sessionKey) return undefined;
  try {
    const state = JSON.parse(localStorage.getItem(prefix + sessionKey) ?? 'null') as ParkedComposerSelection | null;
    if (state?.text !== text || !Number.isInteger(state.start) || !Number.isInteger(state.end)) return undefined;
    return {
      text,
      start: Math.max(0, Math.min(text.length, state.start)),
      end: Math.max(0, Math.min(text.length, state.end)),
    };
  } catch {
    return undefined;
  }
}
