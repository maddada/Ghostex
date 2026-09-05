/*
CDXC:SessionChat 2026-08-28:
The composer footer's Terminal View button tells the user whether the agent's
CLI is actually sitting at an input box, and shows the bottom of that screen on
hover. Both come from the same session-scoped /api/readSessionTerminalTail read
the `composerNotReady` notice uses (see session-chat-composer-not-ready.tsx).
The read happens only when the user hovers or focuses the Terminal View button;
the chat does not keep capturing an unseen terminal in the background.

Two rules this hook exists to keep:
  * `unknown` is not "not ready". The daemon only measured composer signatures
    for a handful of agent CLIs and answers `unknown` for the rest; the caller
    must render the neutral color for it (gxserver-protocol.ts, CDXC
    :SessionChatComposerReady).
  * A failed read keeps the last verdict. A dropped socket or a sleeping daemon
    must not paint the button red — it means "we don't know", which is the same
    as `unknown`, and flickering the tint on every transport hiccup would be
    worse than a slightly stale one.
*/

import { useCallback, useEffect, useRef, useState } from 'react';
import type { GxserverReadSessionTerminalTailResult } from '@/packages/shared/gxserver-protocol';

/*
 * CDXC:SessionChat 2026-09-05 DECISION:
 * User: the agent CLI's horizontal rules must be truncated in the Terminal View
 * hover preview instead of being copied verbatim. A zmx session is 200 columns
 * wide, so a rule row carried nothing but width: it pinned the tooltip to its
 * max width and each rule wrapped onto a second line, framing the real content
 * in four ragged bars. Supersedes the earlier "keep full-width rules verbatim"
 * rule of this formatter; indentation, blank rows, and text content are still
 * verbatim.
 */
const RULE_RUN_PATTERN = '[─━═╌╍┄┅┈┉▔‾⎯–—_-]{8,}';
const RULE_RUN = new RegExp(RULE_RUN_PATTERN, 'u');
const RULE_RUNS = new RegExp(RULE_RUN_PATTERN, 'gu');
/** Narrow enough to stay a frame, wide enough to still read as a rule. */
const MIN_RULE_COLUMNS = 24;
const MAX_RULE_COLUMNS = 72;

function previewColumns(line: string): number {
  return Array.from(line.trimEnd()).length;
}

/**
 * Rules are shortened to the widest row that is not itself a rule, so the box
 * still frames the content it was drawn around instead of dictating the
 * tooltip's width.
 */
function ruleColumnsFor(lines: readonly string[]): number {
  let widest = 0;
  for (const line of lines) {
    if (RULE_RUN.test(line)) {
      continue;
    }
    widest = Math.max(widest, previewColumns(line));
  }
  return Math.min(MAX_RULE_COLUMNS, Math.max(MIN_RULE_COLUMNS, widest));
}

/**
 * Keeps the captured terminal rows for the hover preview: indentation, blank
 * rows, and text content verbatim, with every long run of rule characters cut
 * down to `columns`. The server owns the bounded screen-tail size.
 */
export function formatSessionTerminalTailPreview(lines: readonly string[]): string {
  const columns = ruleColumnsFor(lines);
  return lines
    .map((line) =>
      line.replace(RULE_RUNS, (run) => {
        const chars = Array.from(run);
        return chars.length <= columns ? run : chars.slice(0, columns).join('');
      })
    )
    .join('\n');
}

export interface SessionTerminalTailState {
  /** The newest successful read, or null before one has landed. */
  tail: GxserverReadSessionTerminalTailResult | null;
  /** Reads again right now — used on hover so the preview is current. */
  refreshNow: () => void;
}

/**
 * Reads the session's terminal tail only when `refreshNow` is invoked by the
 * Terminal View trigger. `onReadTerminalTail` is already bound to one session
 * by the transport, and the composer remounts per session, so there is no id to
 * track here. Hosts without the endpoint pass nothing and the hook stays idle.
 */
export function useSessionTerminalTail(
  onReadTerminalTail?: () => Promise<GxserverReadSessionTerminalTailResult>
): SessionTerminalTailState {
  const [tail, setTail] = useState<GxserverReadSessionTerminalTailResult | null>(null);
  const readRef = useRef<(() => Promise<GxserverReadSessionTerminalTailResult>) | undefined>(onReadTerminalTail);
  // Monotonic request id: focus and hover can overlap, and an older read must
  // not paint over a newer one.
  const requestRef = useRef(0);
  const liveRef = useRef(false);

  useEffect(() => {
    readRef.current = onReadTerminalTail;
  }, [onReadTerminalTail]);

  const refreshNow = useCallback(() => {
    const read = readRef.current;
    if (!read) {
      return;
    }
    const request = requestRef.current + 1;
    requestRef.current = request;
    void read()
      .then((result) => {
        if (!liveRef.current || requestRef.current !== request) {
          return;
        }
        setTail(result);
      })
      .catch(() => {
        // Keep the last verdict; a failed read is "unknown", never "not ready".
      });
  }, []);

  useEffect(() => {
    liveRef.current = true;
    return () => {
      liveRef.current = false;
    };
  }, []);

  return { tail, refreshNow };
}
