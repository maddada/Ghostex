import type { SessionChatDraftVersion, SessionChatRecoveryDraft } from '@/packages/shared/session-chat-queue';
import { reportDraftStorageFailure } from './session-chat-draft-outbox';
import { SessionChatStorageIndex } from './session-chat-storage-index';

const PREFIX = 'ghostex.sessionChat.recovery.';
export type LocalRecoveryDraft = {
  sessionKey: string;
  text: string;
  updatedAt: number;
  version?: SessionChatDraftVersion;
  dismissed?: boolean;
};
const recoveryIndex = new SessionChatStorageIndex<LocalRecoveryDraft>(
  PREFIX,
  (raw) => {
    const entry = JSON.parse(raw) as LocalRecoveryDraft;
    return typeof entry.text === 'string' && !entry.dismissed ? entry : null;
  },
  (entry) => entry.sessionKey
);
function identity(entry: LocalRecoveryDraft): string {
  return `${entry.sessionKey}:${entry.version ? `${entry.version.draftId}:${entry.version.revision}` : entry.updatedAt}`;
}
export function preserveDraftRevision(entry: LocalRecoveryDraft): void {
  if (entry.text === '') return;
  try {
    const name = PREFIX + identity(entry);
    if (localStorage.getItem(name) === null) recoveryIndex.set(name, entry);
  } catch {
    reportDraftStorageFailure(entry.sessionKey);
  }
}
export function retireDraftRecovery(sessionKey: string, receipts: readonly SessionChatDraftVersion[]): void {
  if (receipts.length === 0) return;
  const consumed = new Map<string, number>();
  for (const receipt of receipts)
    consumed.set(receipt.draftId, Math.max(consumed.get(receipt.draftId) ?? 0, receipt.revision));
  for (const [id, entry] of recoveryDraftEntries(sessionKey)) {
    if (
      entry.sessionKey === sessionKey &&
      entry.version &&
      (consumed.get(entry.version.draftId) ?? 0) >= entry.version.revision
    )
      dismissDraftRecovery(id);
  }
}
export function recoveryDraftEntries(sessionKey?: string): [string, LocalRecoveryDraft][] {
  try {
    return recoveryIndex.entries(sessionKey).map(([name, entry]) => [name.slice(PREFIX.length), entry]);
  } catch {
    /* Storage errors are reported by the editing path. */
  }
  return [];
}
export function dismissDraftRecovery(id: string): void {
  const raw = localStorage.getItem(PREFIX + id);
  if (raw) recoveryIndex.set(PREFIX + id, { ...JSON.parse(raw), text: '', dismissed: true });
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
