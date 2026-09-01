/*
CDXC:SessionChatWorkingStrip 2026-08-30:
Always-visible "agent is working" indicator, pinned directly above the composer
and OUTSIDE the transcript scroller so the working state stays visible at any
scroll position.

When a terminal activity exists (compaction, background shells, a monitor, a
⏺ status), the strip renders the SAME activity card the transcript used to
show — the card lives here now, and the transcript no longer duplicates it.
Without an activity it shows the pulsing spark plus a whimsical working word.
*/

import { useEffect, useState } from 'react';
import type { SessionChatTerminalActivity } from '../../shared/session-chat';
import { SessionChatActivityRow } from './session-chat-activity-row';
import { pickSessionChatWorkingWord } from './session-chat-working-words';

export interface SessionChatWorkingStripProps {
  working: boolean;
  activity: SessionChatTerminalActivity | null;
}

export function SessionChatWorkingStrip({ working, activity }: SessionChatWorkingStripProps) {
  // One whimsical word per working stint: re-picked on each false→true edge,
  // stable for the whole stint so the label doesn't churn mid-turn.
  const [word, setWord] = useState(pickSessionChatWorkingWord);
  useEffect(() => {
    if (working) {
      setWord(pickSessionChatWorkingWord());
    }
  }, [working]);

  // An activity keeps the strip visible even when the session isn't "working"
  // in the sidebar sense, and it replaces the whimsical word entirely.
  if (activity) {
    return <SessionChatActivityRow activity={activity} className='my-0' />;
  }
  if (!working) {
    return null;
  }

  return (
    <div aria-live='polite' className='ghostex-chat-working-strip' role='status'>
      <div className='ghostex-chat-working-strip-row'>
        <span aria-hidden='true' className='ghostex-chat-working-strip-spark'>
          <svg viewBox='0 0 24 24'>
            <path d='M12 0.8c.5 4.6 1.8 7.4 3.6 9.1 1.6 1.6 4.2 2.5 7.6 2.1-3.4-.4-6 .5-7.6 2.1-1.8 1.7-3.1 4.5-3.6 9.1-.5-4.6-1.8-7.4-3.6-9.1C6.8 12.5 4.2 11.6.8 12c3.4.4 6-.5 7.6-2.1C10.2 8.2 11.5 5.4 12 .8z' />
          </svg>
        </span>
        <span className='ghostex-chat-working-strip-text'>{word}…</span>
      </div>
    </div>
  );
}
