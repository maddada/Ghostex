/*
CDXC:SessionChatWorkingStrip 2026-08-30:
Always-visible "agent is working" indicator, pinned directly above the composer
and OUTSIDE the transcript scroller. The in-transcript typing indicator only
reads when the user is scrolled to the bottom; this strip is the guarantee the
working state stays visible at any scroll position.

Detail text comes from the same live source as the transcript's activity row
(`terminalActivity`: label + locally-ticking elapsed clock). Without an
activity it says just "Working". When the activity row is on screen the two
say the same thing, which is fine — the strip is chrome, the row is context.
*/

import { useEffect, useState } from 'react';
import type { SessionChatTerminalActivity } from '../../shared/session-chat';
import { formatSessionChatActivityElapsed, sessionChatActivityElapsedSeconds } from './session-chat-activity-row';
import { pickSessionChatWorkingWord } from './session-chat-working-words';

const WORKING_STRIP_CLOCK_TICK_MS = 1_000;

export interface SessionChatWorkingStripProps {
  working: boolean;
  activity: SessionChatTerminalActivity | null;
}

export function SessionChatWorkingStrip({ working, activity }: SessionChatWorkingStripProps) {
  const [now, setNow] = useState(() => Date.now());
  // One whimsical word per working stint: re-picked on each false→true edge,
  // stable for the whole stint so the label doesn't churn mid-turn.
  const [word, setWord] = useState(pickSessionChatWorkingWord);
  useEffect(() => {
    if (working) {
      setWord(pickSessionChatWorkingWord());
    }
  }, [working]);
  const hasClock = activity?.elapsedSeconds !== undefined;
  useEffect(() => {
    if (!hasClock) {
      return;
    }
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), WORKING_STRIP_CLOCK_TICK_MS);
    return () => clearInterval(timer);
  }, [activity?.detectedAt, hasClock]);

  // An activity (compaction, background shells, a monitor) keeps the strip
  // visible even when the session isn't "working" in the sidebar sense, and
  // it REPLACES the whimsical word: "Compacting conversation…" already says
  // what's happening, so prefixing "Pondering · " would just be noise.
  if (!working && !activity) {
    return null;
  }

  const elapsed = activity ? sessionChatActivityElapsedSeconds(activity, now) : null;
  const percent = activity?.percent === undefined ? null : Math.min(100, Math.max(0, Math.round(activity.percent)));

  return (
    <div aria-live='polite' className='ghostex-chat-working-strip' role='status'>
      <div className='ghostex-chat-working-strip-row'>
        <span aria-hidden='true' className='ghostex-chat-working-strip-spark'>
          <svg viewBox='0 0 24 24'>
            <path d='M12 0.8c.5 4.6 1.8 7.4 3.6 9.1 1.6 1.6 4.2 2.5 7.6 2.1-3.4-.4-6 .5-7.6 2.1-1.8 1.7-3.1 4.5-3.6 9.1-.5-4.6-1.8-7.4-3.6-9.1C6.8 12.5 4.2 11.6.8 12c3.4.4 6-.5 7.6-2.1C10.2 8.2 11.5 5.4 12 .8z' />
          </svg>
        </span>
        <span className='ghostex-chat-working-strip-text'>
          {activity ? <span className='ghostex-chat-working-strip-detail'>{activity.label}…</span> : <>{word}…</>}
          {elapsed !== null ? (
            <span className='ghostex-chat-working-strip-elapsed'> {formatSessionChatActivityElapsed(elapsed)}</span>
          ) : null}
          {percent !== null ? <span className='ghostex-chat-working-strip-elapsed'> {percent}%</span> : null}
        </span>
      </div>
      {percent !== null ? (
        <div
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={percent}
          className='ghostex-chat-working-strip-bar'
          role='progressbar'
        >
          <div className='ghostex-chat-working-strip-bar-fill' style={{ width: `${percent}%` }} />
        </div>
      ) : null}
    </div>
  );
}
