import type { SessionChatDraftVersion, SessionChatRecoveryDraft } from '@/packages/shared/session-chat-queue';
import { reportDraftStorageFailure } from './session-chat-draft-outbox';

const PREFIX = 'ghostex.sessionChat.recovery.';
export type LocalRecoveryDraft = {
  sessionKey: string;
  text: string;
  updatedAt: number;
  version?: SessionChatDraftVersion;
  dismissed?: boolean;
};
function identity(entry: LocalRecoveryDraft): string {
  return `${entry.sessionKey}:${entry.version ? `${entry.version.draftId}:${entry.version.revision}` : entry.updatedAt}`;
}
export function preserveDraftRevision(entry: LocalRecoveryDraft): void {
  if (entry.text === '') return;
  try {
    const name = PREFIX + identity(entry);
    if (localStorage.getItem(name) === null) localStorage.setItem(name, JSON.stringify(entry));
  } catch {
    reportDraftStorageFailure(entry.sessionKey);
  }
}
export function retireDraftRecovery(sessionKey: string, receipts: readonly SessionChatDraftVersion[]): void {
  for (const [id, entry] of recoveryDraftEntries()) {
    if (
      entry.sessionKey === sessionKey &&
      entry.version &&
      receipts.some((v) => v.draftId === entry.version!.draftId && v.revision >= entry.version!.revision)
    )
      dismissDraftRecovery(id);
  }
}
export function recoveryDraftEntries(): [string, LocalRecoveryDraft][] {
  const entries: [string, LocalRecoveryDraft][] = [];
  try {
    for (let index = 0; index < localStorage.length; index++) {
      const name = localStorage.key(index);
      if (!name?.startsWith(PREFIX)) continue;
      const entry = JSON.parse(localStorage.getItem(name)!) as LocalRecoveryDraft;
      if (typeof entry.text === 'string' && !entry.dismissed) entries.push([name.slice(PREFIX.length), entry]);
    }
  } catch {
    /* Storage errors are reported by the editing path. */
  }
  return entries;
}
export function dismissDraftRecovery(id: string): void {
  const raw = localStorage.getItem(PREFIX + id);
  if (raw) localStorage.setItem(PREFIX + id, JSON.stringify({ ...JSON.parse(raw), text: '', dismissed: true }));
}
export function importDraftRecovery(drafts: readonly SessionChatRecoveryDraft[] = [], prefix = ''): void {
  for (const draft of drafts)
    preserveDraftRevision({
      sessionKey: `${prefix}${draft.projectId}:${draft.sessionId}`,
      text: draft.content,
      version: draft.version,
      updatedAt: Date.parse(draft.updatedAt),
    });
}
