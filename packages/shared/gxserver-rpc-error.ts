// The typed rejection a gxserver RPC client throws, so callers can branch on
// WHY the daemon refused instead of pattern-matching an English message.
//
// The wire shape is `GxserverRpcErrorResponse`: `{ ok: false, error: <code>,
// message }`. Each host's fetch helper (the desktop chat bridge's `rpc`, the
// web app's `GxserverConnection.rpc`) builds one of these from that envelope,
// and shared UI reads the code back with `gxserverRpcErrorCode`. `instanceof`
// is exact here because every host bundles packages/shared into the same module
// graph as the UI that reads it.
//
// First consumer: `composerNotReady` from `/api/sendSessionChatMessage`, which
// the chat composer renders as its own notice (agent input box not painted yet)
// instead of the generic "message could not be sent".

import type { GxserverRpcErrorCode } from './gxserver-protocol';

export class GxserverRpcError extends Error {
  /** The daemon's refusal code, verbatim from the response envelope. */
  readonly code: GxserverRpcErrorCode;
  /** The endpoint that was refused, so a log line identifies the call. */
  readonly endpoint: string;

  constructor(code: GxserverRpcErrorCode, message: string, endpoint: string) {
    super(message);
    this.name = 'GxserverRpcError';
    this.code = code;
    this.endpoint = endpoint;
  }
}

/**
 * Builds the typed error for a gxserver error envelope, or returns null when
 * the body is not one — a proxy's HTML page, a truncated response, or a
 * connection that never reached the daemon. The caller throws its own
 * transport-level Error in that case, because there is no daemon verdict to
 * report.
 */
export function gxserverRpcErrorFromResponseBody(endpoint: string, body: unknown): GxserverRpcError | null {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) {
    return null;
  }
  const envelope = body as { error?: unknown; message?: unknown; ok?: unknown };
  if (envelope.ok !== false || typeof envelope.error !== 'string' || envelope.error === '') {
    return null;
  }
  const message =
    typeof envelope.message === 'string' && envelope.message !== ''
      ? envelope.message
      : `gxserver refused ${endpoint} (${envelope.error}).`;
  // The daemon owns this vocabulary, and it may ship a code before this client
  // is rebuilt. Carrying the string through unchanged keeps that case readable
  // in logs; readers compare against the codes they know and fall through.
  return new GxserverRpcError(envelope.error as GxserverRpcErrorCode, message, endpoint);
}

/** The daemon's refusal code, or null for a transport-level failure. */
export function gxserverRpcErrorCode(error: unknown): GxserverRpcErrorCode | null {
  return error instanceof GxserverRpcError ? error.code : null;
}
