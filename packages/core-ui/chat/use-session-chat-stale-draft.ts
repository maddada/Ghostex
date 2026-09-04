import { useCallback, useEffect, useEffectEvent, useRef } from 'react';
import { readStoredSessionChatDraftEntry } from './session-chat-draft-storage';
import { isSessionChatDraftStale } from './session-chat-queue';

type SilentlyRestoredSessionChatDraft = {
  text: string;
  updatedAt: number | undefined;
};

type UseSessionChatStaleDraftOptions = {
  draft: string;
  lastSentPromptAt: number | null;
  readComposerText: () => string;
  sessionKey: string | undefined;
  vacateComposer: () => void;
};

/*
CDXC:Drafts 2026-09-04 WHY:
The transcript-side half of "a sent message never comes back as a draft".
Text this composer holds only because a cache restored it (mount-time
localStorage, or the crash-restore above when it ran before the transcript
loaded) is retired the moment the transcript proves a newer prompt went
out: the field, the localStorage copy, and — through the ordinary sync —
gxserver's row. Text the user has touched is never measured; the live
value must still equal the restored text, so a single keystroke opts out.
*/
export function useSessionChatStaleDraft({
  draft,
  lastSentPromptAt,
  readComposerText,
  sessionKey,
  vacateComposer,
}: UseSessionChatStaleDraftOptions): (restored: SilentlyRestoredSessionChatDraft) => void {
  /**
   * The text this composer holds without the user having typed it — the
   * mount-time localStorage restore, or gxserver's crash-restore copy — with
   * its stamp, so it can be measured against the transcript once that has
   * loaded. Cleared once the check retires it or the user replaces it.
   */
  const silentlyRestoredDraftRef = useRef<SilentlyRestoredSessionChatDraft | null>(
    (() => {
      const stored = readStoredSessionChatDraftEntry(sessionKey);
      return stored !== null && stored.text !== '' ? stored : null;
    })(),
  );
  const readLiveComposerText = useEffectEvent(readComposerText);
  const vacateLiveComposer = useEffectEvent(vacateComposer);

  useEffect(() => {
    const restored = silentlyRestoredDraftRef.current;
    if (restored === null) {
      return;
    }
    const composerText = readLiveComposerText();
    if (composerText !== restored.text) {
      silentlyRestoredDraftRef.current = null;
      return;
    }
    if (!isSessionChatDraftStale(restored.updatedAt, lastSentPromptAt)) {
      return;
    }
    silentlyRestoredDraftRef.current = null;
    vacateLiveComposer();
  }, [draft, lastSentPromptAt]);

  return useCallback((restored: SilentlyRestoredSessionChatDraft): void => {
    silentlyRestoredDraftRef.current = restored;
  }, []);
}
