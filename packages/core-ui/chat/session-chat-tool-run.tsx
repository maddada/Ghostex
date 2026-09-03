// Compact work rows for tool calls, results, and file edits.

import {
  IconChevronRight,
  IconFileText,
  IconPencil,
  IconTerminal2,
  IconTool,
  IconWorldSearch,
} from '@tabler/icons-react';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import type { SessionChatToolCallBlock, SessionChatToolResultBlock } from '../../shared/session-chat';
import { cn } from '@/packages/components/utils';
import { diffFromSessionChatText, diffFromSessionChatToolCall, type SessionChatDiffLine } from './session-chat-diff';
import { centerSessionChatExpansion, SessionChatExpansion } from './session-chat-expansion';
import { answeredSessionChatQuestionExchange, SessionChatQuestionExchangeCard } from './session-chat-question-exchange';
import { pairSessionChatToolBlocks } from './session-chat-tool-fold';
import {
  formatSessionChatToolInput,
  summarizeSessionChatCommandInput,
  summarizeSessionChatToolInput,
} from './session-chat-tool-summary';

export const SESSION_CHAT_MAX_TOOL_RESULT_CHARS = 4000;

type ToolBlock = SessionChatToolCallBlock | SessionChatToolResultBlock;

export interface SessionChatToolRunProps {
  blocks: readonly ToolBlock[];
  /** Global expand toggle; expands the run and each row's detail. */
  expandSignal?: boolean;
  /** The parent disclosure already owns collapsing, so render every row. */
  showAllRows?: boolean;
  /**
   * Keep answered question pairs as plain tool rows. Set by contexts where a
   * hoisted SessionChatQuestionExchangeCard already shows the exchange (the
   * expanded completed-work log), so it is not rendered twice.
   */
  questionPairsAsRows?: boolean;
}

function clipBody(text: string): string {
  return text.length > SESSION_CHAT_MAX_TOOL_RESULT_CHARS
    ? `${text.slice(0, SESSION_CHAT_MAX_TOOL_RESULT_CHARS)}…`
    : text;
}

/*
 * The one glyph on this surface that says WHAT ran rather than "this expands":
 * semantic tier (see CDXC:SessionChat in chat.css). It shares the
 * control tier's size because it stands in the same marker slot on the same
 * vertical axis as the chevrons; only its stroke weight and its shape set it
 * apart, and it must never be flattened into a chevron.
 */
function toolIcon(name: string): ReactNode {
  const normalized = name.toLowerCase();
  const className = 'ghostex-chat-glyph-semantic';
  if (/edit|write|patch|replace/.test(normalized)) {
    return <IconPencil aria-hidden='true' className={className} />;
  }
  if (/read|file|glob|list/.test(normalized)) {
    return <IconFileText aria-hidden='true' className={className} />;
  }
  if (/exec|command|shell|terminal|bash/.test(normalized)) {
    return <IconTerminal2 aria-hidden='true' className={className} />;
  }
  if (/web|search|browser|fetch|url/.test(normalized)) {
    return <IconWorldSearch aria-hidden='true' className={className} />;
  }
  return <IconTool aria-hidden='true' className={className} />;
}

function isCommandTool(name: string): boolean {
  return /exec|command|shell|terminal|bash/.test(name.toLowerCase());
}

function DiffView({ lines }: { lines: readonly SessionChatDiffLine[] }) {
  return (
    <div className='ghostex-chat-file-edit'>
      <div className='ghostex-chat-diff'>
        {lines.map((line, index) => (
          <div className={cn('ghostex-chat-diff-line', `is-${line.kind}`)} key={index}>
            <span className='ghostex-chat-diff-sign'>
              {line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : ' '}
            </span>
            <span>{line.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function ToolBody({ error, label, text }: { error?: boolean; label?: string; text: string }) {
  return (
    <div className='ghostex-chat-tool-body-group'>
      {label ? <div className='ghostex-chat-tool-body-label'>{label}</div> : null}
      <pre className={cn('ghostex-chat-tool-body', error && 'is-error')}>{clipBody(text)}</pre>
    </div>
  );
}

function ToolLine({
  call,
  expandSignal,
  result,
}: {
  call?: SessionChatToolCallBlock;
  expandSignal: boolean;
  result?: SessionChatToolResultBlock;
}) {
  const [open, setOpen] = useState(expandSignal);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(expandSignal), [expandSignal]);

  const name = call?.name ?? 'Result';
  const commandTool = isCommandTool(name);
  const inputPreview = call ? summarizeSessionChatToolInput(call.input) : '';
  const commandPreview = call ? summarizeSessionChatCommandInput(call.input) : '';
  const resultPreview = result?.output.split('\n')[0]?.trim().slice(0, 120) ?? '';
  const preview = commandTool ? commandPreview : inputPreview || resultPreview;
  const callDiff = call ? diffFromSessionChatToolCall(call.name, call.input) : null;
  const resultDiff = result ? diffFromSessionChatText(result.output) : null;
  const diff = callDiff ?? resultDiff;
  const inputDetail = call ? formatSessionChatToolInput(call.input) : '';
  const inputAddsInfo = Boolean(call && (commandTool || inputDetail.replace(/\s+/g, ' ').trim() !== preview));
  const hasResultBody = Boolean(result?.output && resultDiff === null);
  const hasDetail = diff !== null || inputAddsInfo || hasResultBody;

  return (
    <div className={cn('ghostex-chat-work-row', result?.isError && 'is-error')} data-open={open}>
      <button
        aria-expanded={hasDetail ? open : undefined}
        className='ghostex-chat-work-trigger'
        disabled={!hasDetail}
        onClick={() => {
          if (hasDetail) {
            if (!open) {
              centerSessionChatExpansion(triggerRef.current);
            }
            setOpen((current) => !current);
          }
        }}
        ref={triggerRef}
        type='button'
      >
        <span className='ghostex-chat-work-icon'>{toolIcon(name)}</span>
        <span className='ghostex-chat-work-heading'>{name}</span>
        {preview ? <span className='ghostex-chat-work-preview'>{preview}</span> : null}
        {hasDetail ? (
          <IconChevronRight aria-hidden='true' className={cn('ghostex-chat-disclosure-chevron', open && 'is-open')} />
        ) : null}
      </button>
      {hasDetail && open ? (
        <SessionChatExpansion
          className='ghostex-chat-work-detail'
          label={`Collapse ${name}`}
          onCollapse={() => setOpen(false)}
        >
          {inputAddsInfo && (!diff || commandTool) ? (
            <ToolBody label={commandTool ? 'Command' : result ? 'Input' : undefined} text={inputDetail} />
          ) : null}
          {diff ? <DiffView lines={diff} /> : null}
          {!diff && hasResultBody && result ? (
            <ToolBody error={result.isError} label={call ? 'Result' : undefined} text={result.output} />
          ) : null}
        </SessionChatExpansion>
      ) : null}
    </div>
  );
}

export function SessionChatToolRun({
  blocks,
  expandSignal = false,
  showAllRows = false,
  questionPairsAsRows = false,
}: SessionChatToolRunProps) {
  const pairs = pairSessionChatToolBlocks(blocks);
  const [expanded, setExpanded] = useState(showAllRows || expandSignal);
  useEffect(() => setExpanded(showAllRows || expandSignal), [expandSignal, showAllRows]);

  const exchanges = pairs.map((pair) => (questionPairsAsRows ? null : answeredSessionChatQuestionExchange(pair)));
  const renderItem = (index: number) => {
    const pair = pairs[index];
    if (!pair) {
      return null;
    }
    const exchange = exchanges[index];
    if (exchange) {
      return (
        <div className='ghostex-chat-question-exchange-item py-1' key={index}>
          <SessionChatQuestionExchangeCard exchange={exchange} />
        </div>
      );
    }
    return <ToolLine call={pair.call} expandSignal={expandSignal} key={index} result={pair.result} />;
  };

  // An answered question is conversation, not work: its card never folds
  // behind the "+N previous tool calls" toggle. The fold hides only the
  // ordinary tool rows before the last pair, exactly as before.
  const collapsedVisible = pairs.map((_, index) => index === pairs.length - 1 || exchanges[index] !== null);
  const hiddenCount = collapsedVisible.filter((visible) => !visible).length;
  const toggle = (
    <button
      aria-expanded={expanded}
      className='ghostex-chat-tool-run-toggle'
      onClick={() => setExpanded((current) => !current)}
      type='button'
    >
      <span className='ghostex-chat-work-icon'>
        <IconChevronRight aria-hidden='true' className={cn('ghostex-chat-disclosure-chevron', expanded && 'is-open')} />
      </span>
      <span>
        {expanded ? 'Show fewer tool calls' : `+${hiddenCount} previous tool ${hiddenCount === 1 ? 'call' : 'calls'}`}
      </span>
    </button>
  );

  const allRows = pairs.map((_, index) => renderItem(index));
  const collapsedRows = pairs.map((_, index) => (collapsedVisible[index] ? renderItem(index) : null));

  return (
    <div className='ghostex-chat-tool-run'>
      {hiddenCount === 0 || showAllRows ? (
        allRows
      ) : expanded ? (
        <SessionChatExpansion
          bodyClassName='ghostex-chat-tool-run-expanded'
          label='Show fewer tool calls'
          onCollapse={() => setExpanded(false)}
        >
          {allRows}
          {toggle}
        </SessionChatExpansion>
      ) : (
        <>
          {collapsedRows}
          {toggle}
        </>
      )}
    </div>
  );
}
