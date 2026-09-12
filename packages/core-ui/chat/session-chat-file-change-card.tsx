import { createContext, useContext, useEffect, useId, useRef, useState } from 'react';
import { cn } from '@/packages/components/utils';
import { AppTooltip } from '../app-tooltip';
import type { SessionChatFileChange } from './session-chat-file-changes';
import { useSessionChatHostLinks } from './session-chat-links';
import { SESSION_CHAT_FILE_PATH_ATTRIBUTE } from './session-chat-file-paths';
import { revealSessionChatFileChangeHeader } from './session-chat-file-change-scroll';
import './session-chat-file-change-card.css';

export const SessionChatFileChangePreviewContext = createContext(false);
export const SessionChatFileChangeInteractionContext = createContext<((messageId: string) => void) | null>(null);

/** CDXC:SessionChat 2026-09-10 DECISION:
 * User: clicking anywhere on a file-change path uses the reference pill's host Editor/Docs action, and right-clicking uses the same reference menu; this replaces the separate folder-copy action.
 * User: edits default to one collapsed row; Settings > Chat can enable seven-line previews. Counts beside the path toggle the full diff, replacing the chevron and bottom line-count label.
 * User: after either expanding or collapsing a diff, keep its header visible, scrolling to it when necessary.
 * User: the circle toggles the diff and its center turns white on hover; the left rail also toggles the open code.
 * User: clicking the card itself toggles the diff; only the path/name text opens the file, not the empty space beside it.
 * User: the header is a single line containing the file path, truncated from the start when necessary; this replaces the stacked filename and folder.
 * User: use styled action tooltips; the unified path now shares one tooltip instead of separate folder and filename tooltips.
 */
function FileChangeCard({ change, messageId }: { change: SessionChatFileChange; messageId?: string }) {
  const previewEnabled = useContext(SessionChatFileChangePreviewContext);
  const reportInteraction = useContext(SessionChatFileChangeInteractionContext);
  const [expanded, setExpanded] = useState(false);
  const [copyStatus, setCopyStatus] = useState<string | null>(null);
  const openFile = useSessionChatHostLinks()?.openFile;
  const bodyId = useId();
  const headerRef = useRef<HTMLDivElement>(null);
  const filename = change.path.split(/[\\/]/).at(-1) || change.path;
  const parentPath = change.path.slice(0, -filename.length);
  useEffect(() => {
    if (copyStatus === null) return;
    const timeout = window.setTimeout(() => setCopyStatus(null), 1500);
    return () => window.clearTimeout(timeout);
  }, [copyStatus]);
  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(change.path);
      setCopyStatus('Path copied');
    } catch (error) {
      console.error('[session-chat] file change path copy failed', error);
      setCopyStatus('Could not copy path');
    }
  };
  const code = change.lines.filter((line) => line.kind !== 'meta');
  const added = code.filter((line) => line.kind === 'add').length;
  const removed = code.filter((line) => line.kind === 'del').length;
  const canExpand = !previewEnabled || code.length > 7 || Boolean(change.result?.isError);
  const showBody = expanded || previewEnabled;
  const lines = expanded ? change.lines : code.slice(0, 7);
  const toggle = () => {
    if (!canExpand) return;
    if (messageId) reportInteraction?.(messageId);
    revealSessionChatFileChangeHeader(headerRef.current);
    setExpanded((value) => !value);
  };
  return (
    <section
      className={cn('ghostex-chat-file-change-card', showBody && 'has-preview', canExpand && 'is-expandable')}
      aria-label={`${change.action} ${change.path}`}
      onClick={(event) => {
        if (!(event.target instanceof Element) || event.target.closest('button')) return;
        toggle();
      }}
    >
      <div className='ghostex-chat-file-change-header' ref={headerRef}>
        <button
          type='button'
          className='ghostex-chat-file-change-marker'
          onClick={toggle}
          disabled={!canExpand}
          aria-expanded={expanded}
          aria-controls={bodyId}
          aria-label={`${expanded ? 'Collapse' : 'Show'} changes for ${filename}`}
        />
        <AppTooltip content={copyStatus ?? (openFile ? 'Open path' : 'Copy file path')} side='top'>
          <button
            type='button'
            className='ghostex-chat-file-change-name'
            onClick={() => {
              if (openFile) openFile(change.path);
              else void copyPath();
            }}
            aria-label={`${openFile ? 'Open path' : 'Copy file path'}: ${change.path}`}
            {...{ [SESSION_CHAT_FILE_PATH_ATTRIBUTE]: change.path }}
          >
            {parentPath ? (
              <span className='ghostex-chat-file-change-parent'>
                <bdi dir='ltr'>{parentPath}</bdi>
              </span>
            ) : null}
            <span className='ghostex-chat-file-change-filename'>
              <bdi dir='ltr'>{filename}</bdi>
            </span>
          </button>
        </AppTooltip>
        <span className='sr-only' role='status'>
          {copyStatus}
        </span>
        {change.result?.isError ? <span className='ghostex-chat-file-change-action is-error'>Failed</span> : null}
        <AppTooltip content={expanded ? 'Collapse changes' : 'Show changes'} side='top'>
          <button
            type='button'
            className='ghostex-chat-file-change-counts'
            onClick={toggle}
            disabled={!canExpand}
            aria-expanded={expanded}
            aria-controls={bodyId}
            aria-label={`${expanded ? 'Collapse' : 'Show'} changes for ${filename}: ${added} lines added, ${removed} lines removed`}
          >
            <span className='is-add'>+{added}</span>
            <span className='is-del'>−{removed}</span>
          </button>
        </AppTooltip>
      </div>
      {showBody && canExpand ? (
        <button
          type='button'
          className='ghostex-chat-file-change-rail'
          onClick={toggle}
          aria-expanded={expanded}
          aria-controls={bodyId}
          aria-label={`${expanded ? 'Collapse' : 'Show all'} changes for ${filename} using rail`}
        />
      ) : null}
      {showBody ? (
        <div className='ghostex-chat-file-change-body' id={bodyId}>
          <button
            type='button'
            className='ghostex-chat-file-change-code'
            onClick={toggle}
            disabled={!canExpand}
            aria-expanded={canExpand ? expanded : undefined}
            aria-label={`${expanded ? 'Collapse' : 'Expand'} code for ${filename}`}
          >
            <code>
              {lines.map((line, index) => (
                <span className={cn('ghostex-chat-file-change-line', `is-${line.kind}`)} key={index}>
                  <span aria-hidden='true'>{line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '}</span>
                  <span>{line.text || ' '}</span>
                </span>
              ))}
              {lines.length === 0 ? (
                <span className='ghostex-chat-file-change-empty'>
                  {change.action === 'Delete' ? 'File removed' : 'Empty file'}
                </span>
              ) : null}
            </code>
          </button>
          {expanded && change.result?.isError ? (
            <pre className='ghostex-chat-file-change-error'>{change.result.output}</pre>
          ) : null}
        </div>
      ) : null}
      {showBody && canExpand ? (
        <div className='ghostex-chat-file-change-footer'>
          <button
            type='button'
            className='ghostex-chat-file-change-toggle'
            onClick={toggle}
            aria-expanded={expanded}
            aria-controls={bodyId}
          >
            {expanded ? 'Collapse changes' : 'Show all changes'}
          </button>
        </div>
      ) : null}
    </section>
  );
}

export function SessionChatFileChangeCards({
  changes,
  messageId,
}: {
  changes: readonly SessionChatFileChange[];
  messageId?: string;
}) {
  return changes.length ? (
    <div className='ghostex-chat-file-changes'>
      {changes.map((change, index) => (
        <FileChangeCard change={change} messageId={messageId} key={`${index}:${change.path}`} />
      ))}
    </div>
  ) : null;
}
