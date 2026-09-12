import type { SessionChatMessage } from '@/packages/shared/session-chat';

function sameValue(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (!left || !right || typeof left !== 'object' || typeof right !== 'object') return false;
  if (Array.isArray(left) !== Array.isArray(right)) return false;
  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const keys = Object.keys(leftRecord);
  return (
    keys.length === Object.keys(rightRecord).length &&
    keys.every((key) => Object.hasOwn(rightRecord, key) && sameValue(leftRecord[key], rightRecord[key]))
  );
}

/** CDXC:SessionChat 2026-09-12 WHY:
 * Transcript re-reads and tool folding recreate otherwise settled message objects.
 * Compare their JSON data before React re-enters markdown, diff and tool rendering; content-visibility alone only skips browser layout and paint.
 * Keep the text DOM mounted so transcript search ranges, selection and the minimap retain their targets.
 */
export function sameSessionChatMessage(left: SessionChatMessage, right: SessionChatMessage): boolean {
  return sameValue(left, right);
}
