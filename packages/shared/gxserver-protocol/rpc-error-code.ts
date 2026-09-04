export type GxserverRpcErrorCode =
  | 'badRequest'
  /*
  CDXC:SessionChat 2026-08-26:
  The send was refused because the agent CLI has no input box on screen — it is
  still booting, or a trust/auth/setup screen owns the terminal. Distinct from
  `dependencyUnavailable` (the terminal refused bytes we DID write) because
  nothing was written at all: no clear burst, no paste, and never an Enter.

  The screen behind the refusal is not carried on the error. Read it from
  `/api/readSessionTerminalTail`, which answers with the same verdict plus the
  last thirty lines of the terminal.
  */
  | 'composerNotReady'
  | 'corruptState'
  | 'dependencyUnavailable'
  | 'forbidden'
  /*
  Raised when an ANSWERABLE terminal notice (Claude Code's resume-usage picker)
  owns the input line: the message would confirm a row instead of being sent.
  Emitted by `/api/sendSessionChatMessage` since 2026-08-21; mirrored here so a
  client can distinguish it from a generic internal error.
  */
  | 'invalidState'
  /*
  The send was cancelled by the user's own Escape before its Enter was written
  (`/api/interruptSessionChat` bumps the queue generation under it). Nothing
  reached the agent, so the composer restores the text silently instead of
  announcing a delivery failure.
  */
  | 'sendCancelled'
  | 'internalError'
  | 'methodNotAllowed'
  | 'notFound'
  | 'notImplemented'
  | 'protocolMismatch'
  | 'projectPathUnavailable'
  | 'unauthorized';
