/*
CDXC:ComposerTerminalReadiness 2026-08-28:
The composer footer's Terminal View button tells the user whether the agent's
CLI is actually sitting at an input box, and shows the bottom of that screen on
hover. Both come from the same session-scoped /api/readSessionTerminalTail read
the `composerNotReady` notice uses (see session-chat-composer-not-ready.tsx),
polled on a slow timer so the tint tracks the terminal without the user having
to send a prompt first.

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

/** Slow enough to be free next to the chat stream, fast enough to feel live. */
const TAIL_POLL_INTERVAL_MS = 4000;

/** Lines kept for the hover preview, newest last. */
const TAIL_PREVIEW_LINE_COUNT = 12;

/** Hard cap so one long line cannot widen the tooltip past its max width. */
const TAIL_PREVIEW_LINE_LENGTH = 120;

/*
Terminal UIs draw separators by repeating one glyph across the full width, which
in a tooltip is pure noise and forces the surface as wide as the terminal. Any
run of four or more of the same box-drawing, block, or dash glyph collapses to a
short stub of that same glyph, so the rule still reads as a rule.
*/
// U+2010–U+2015 dashes, U+2500–U+257F box drawing, U+2580–U+259F block elements.
const RULE_GLYPH_RUN = /([‐-―─-▟])\1{3,}/g;

/** ASCII rules need a longer run before they are unambiguous (`---`, `___`). */
const ASCII_RULE_RUN = /([-_=~*#+])\1{7,}/g;

const RULE_STUB_LENGTH = 8;

/**
 * Squeezes a terminal tail into something a tooltip can show without wrapping
 * or scrolling: runs of spaces collapse, rules shrink, blank lines drop, and
 * only the newest lines survive. Returns '' when nothing is left to show.
 */
export function formatSessionTerminalTailPreview(lines: readonly string[]): string {
  const kept: string[] = [];
  for (const raw of lines) {
    const collapsed = raw
      .replace(/\t/g, ' ')
      .replace(/ {2,}/g, ' ')
      .replace(RULE_GLYPH_RUN, (_match, glyph: string) => glyph.repeat(RULE_STUB_LENGTH))
      .replace(ASCII_RULE_RUN, (_match, glyph: string) => glyph.repeat(RULE_STUB_LENGTH))
      .trim();
    if (collapsed.length === 0) {
      continue;
    }
    kept.push(
      collapsed.length > TAIL_PREVIEW_LINE_LENGTH ? `${collapsed.slice(0, TAIL_PREVIEW_LINE_LENGTH - 1)}…` : collapsed
    );
  }
  return kept.slice(-TAIL_PREVIEW_LINE_COUNT).join('\n');
}

export interface SessionTerminalTailState {
  /** The newest successful read, or null before one has landed. */
  tail: GxserverReadSessionTerminalTailResult | null;
  /** Reads again right now — used on hover so the preview is current. */
  refreshNow: () => void;
}

/**
 * Polls the session's terminal tail while mounted and the document is visible.
 * `onReadTerminalTail` is already bound to one session by the transport, and
 * the composer remounts per session, so there is no id to track here. Hosts
 * without the endpoint pass nothing and the hook stays idle.
 */
export function useSessionTerminalTail(
  onReadTerminalTail?: () => Promise<GxserverReadSessionTerminalTailResult>
): SessionTerminalTailState {
  const [tail, setTail] = useState<GxserverReadSessionTerminalTailResult | null>(null);
  const readRef = useRef<(() => Promise<GxserverReadSessionTerminalTailResult>) | undefined>(onReadTerminalTail);
  // Monotonic request id: a hover refresh racing the poll timer must not let
  // the older read paint over the newer one.
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

  const canRead = onReadTerminalTail !== undefined;
  useEffect(() => {
    if (!canRead) {
      return;
    }
    liveRef.current = true;
    let timer: ReturnType<typeof setInterval> | null = null;
    const documentHidden = (): boolean => typeof document !== 'undefined' && document.hidden;
    const stopTimer = (): void => {
      if (timer !== null) {
        clearInterval(timer);
        timer = null;
      }
    };
    const startTimer = (): void => {
      if (timer === null) {
        timer = setInterval(refreshNow, TAIL_POLL_INTERVAL_MS);
      }
    };
    const handleVisibilityChange = (): void => {
      if (documentHidden()) {
        stopTimer();
        return;
      }
      refreshNow();
      startTimer();
    };
    if (!documentHidden()) {
      refreshNow();
      startTimer();
    }
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', handleVisibilityChange);
    }
    return () => {
      liveRef.current = false;
      stopTimer();
      if (typeof document !== 'undefined') {
        document.removeEventListener('visibilitychange', handleVisibilityChange);
      }
    };
  }, [canRead, refreshNow]);

  return { tail, refreshNow };
}
