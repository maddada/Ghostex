/**
 * CDXC:Drafts 2026-09-05 WHY:
 * Restored fragments need to be correlated across CEF caches and gxserver without recording prompt bodies.
 * FNV-1a over UTF-8 is shared with server/src/session_chat_draft_diagnostics.rs.
 */
export function sessionChatDraftFingerprint(value: string): { chars: number; bytes: number; fingerprint: string } {
  const bytes = new TextEncoder().encode(value);
  let hash = 0x811c9dc5;
  for (const byte of bytes) {
    hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  }
  return { chars: value.length, bytes: bytes.length, fingerprint: hash.toString(16).padStart(8, '0') };
}

export type SessionChatDraftDiagnosticLog = (event: string, details: Record<string, unknown>) => void;
