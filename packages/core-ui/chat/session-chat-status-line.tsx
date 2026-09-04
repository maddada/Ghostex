// The text status line under the chat box: the starred context detail rows
// (session-chat-context-details.ts), values only, wrapping as the chat
// narrows. Hovering a value names the row it came from. A diamond separates
// items because the middle dot already separates the parts inside one value.

import { Fragment } from 'react';
import { AppTooltip } from '../app-tooltip';
import type { SessionChatContextDetailItem } from './session-chat-context-details';

export function SessionChatStatusLine({ items }: { items: readonly SessionChatContextDetailItem[] }) {
  if (items.length === 0) {
    return null;
  }
  return (
    <div aria-label='Session status' className='ghostex-chat-status-line' role='status'>
      {items.map((item, index) => (
        <Fragment key={item.id}>
          {index > 0 ? (
            <span aria-hidden='true' className='ghostex-chat-status-line-separator'>
              ◆
            </span>
          ) : null}
          <AppTooltip content={item.label} side='top'>
            <span className='ghostex-chat-status-line-item'>{item.value}</span>
          </AppTooltip>
        </Fragment>
      ))}
    </div>
  );
}
