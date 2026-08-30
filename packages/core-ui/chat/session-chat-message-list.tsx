// Session chat message list (upstream chat spec §11.2 pipeline on shadcn chat
// components).
// Pipeline: drop the never-surfaced harness records → sort → fold tool-only
// messages into the preceding assistant turn. Harness-injected turns the
// terminal DOES print (task notifications, local command output, interrupts,
// continuation summaries, messages from other sessions) survive as collapsed
// markers that expand to their full text — hiding them is what reads as
// "messages are missing".
//
// Scrolling is owned by the shadcn MessageScroller: autoScroll follows live
// growth, preserveScrollOnPrepend anchors history loads, and the scroller
// button replaces the hand-rolled "Jump to latest" control. The viewport is
// flipped to RTL (content back to LTR) so the scrollbar renders on the left
// edge of the conversation.

import {
  IconAlertTriangle,
  IconCheck,
  IconChevronRight,
  IconCopy,
  IconFile,
  IconGitBranch,
  IconInfoCircle,
  IconPhoto,
  IconSparkles,
} from '@tabler/icons-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  SESSION_CHAT_FORK_BOUNDARY_ID_PREFIX,
  type SessionChatMessage,
  type SessionChatTerminalActivity,
  type SessionChatTheme,
} from '../../shared/session-chat';
import { cn } from '@/packages/components/utils';
import { Button } from '../../components/ui/button';
import { Separator } from '../../components/ui/separator';
import {
  Attachment,
  AttachmentContent,
  AttachmentMedia,
  AttachmentTitle,
  AttachmentTrigger,
} from '../../components/ui/attachment';
import {
  SessionChatImageReference,
  SessionChatInlineImage,
  useSessionChatImageViewer,
} from './session-chat-image-viewer';
import { normalizeSessionChatImageTranscriptMessages } from './session-chat-image-transcript-markers';
import { Bubble, BubbleContent } from '../../components/ui/bubble';
import { Marker, MarkerContent, MarkerIcon } from '../../components/ui/marker';
import { Message, MessageContent, MessageFooter } from '../../components/ui/message';
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
  useMessageScroller,
} from '../../components/ui/message-scroller';
import { SessionChatActivityRow } from './session-chat-activity-row';
import { orderSessionChatMessages } from './session-chat-assembler';
import {
  anchorSessionChatExpansionTop,
  centerSessionChatExpansion,
  SessionChatDisclosure,
  SessionChatExpansion,
} from './session-chat-expansion';
import { SessionChatMarkdown } from './session-chat-markdown';
import { SessionChatScrollCap } from './session-chat-scroll-cap';
import {
  SessionChatSaveMarkdownDialog,
  type ListSessionMessageMarkdownPaths,
  type SaveSessionMessageMarkdown,
} from './session-chat-save-markdown-dialog';
import { isSessionChatPendingMessageId } from './session-chat-pending';
import {
  dropSessionChatHiddenMessages,
  sessionChatSuppressedTurnLabel,
  sessionChatSuppressedTurnPresentation,
  type SessionChatStatusRow,
  type SessionChatStatusTone,
} from './session-chat-noise';
import { SESSION_CHAT_STREAMING_ID } from './session-chat-streaming';
import {
  answeredSessionChatQuestionExchange,
  isSessionChatQuestionToolName,
  SessionChatQuestionExchangeCard,
  type SessionChatQuestionExchange,
} from './session-chat-question-exchange';
import {
  foldSessionChatToolMessages,
  pairSessionChatToolBlocks,
  splitSessionChatBlocks,
} from './session-chat-tool-fold';
import { SessionChatToolRun } from './session-chat-tool-run';
import { countSessionChatToolCalls, summarizeSessionChatToolRun } from './session-chat-tool-summary';

const LOAD_EARLIER_SCROLL_TOP_PX = 320;
const AUTO_SCROLL_EDGE_THRESHOLD_PX = 10;
const PASTED_IMAGE_NAME = /^ghostex-paste-.+\.png$/i;
/** Terminal-pane parity: the conversation scrollbar fades out this long after
 * the last scroll (chat.css keys on the data-user-scrolling attribute). */
const SCROLLBAR_FADE_MS = 2000;

export interface SessionChatMessageListProps {
  messages: readonly SessionChatMessage[];
  isWorking: boolean;
  /**
   * CDXC:SessionChatTerminalActivity 2026-08-22: live on-screen progress
   * (compaction). Shown INSTEAD of the typing indicator: it says the same
   * "still working" thing with the detail the indicator cannot carry.
   */
  terminalActivity?: SessionChatTerminalActivity | null;
  hasMore: boolean;
  loadingEarlier: boolean;
  onLoadEarlier: () => void;
  /** Saves a settled assistant response inside this session project's Docs tree. */
  saveMessageMarkdown?: SaveSessionMessageMarkdown;
  /** Reads existing project Markdown paths before generating the next file name. */
  listMessageMarkdownPaths?: ListSessionMessageMarkdownPaths;
  /** Current session title used to prefill a useful Markdown file name. */
  sessionTitle?: string;
  /** Matches the portaled save dialog and toast to this chat surface. */
  theme?: SessionChatTheme;
  /** Reveal reasoning-owned tool activity by default. */
  verboseMode?: boolean;
  /** Show only user prompts with each completed final reply collapsed beneath it. */
  summaryMode?: boolean;
}

function isPastedImagePath(path: string | undefined): boolean {
  if (!path) {
    return false;
  }
  const segment = path.split(/[\\/]/).at(-1) ?? '';
  return PASTED_IMAGE_NAME.test(segment);
}

function imageChipLabel(block: { alt?: string; path?: string; url?: string }): string {
  if (isPastedImagePath(block.path)) {
    return 'Pasted image';
  }
  if (block.path) {
    return block.path.split(/[\\/]/).at(-1) ?? block.path;
  }
  return block.alt ?? block.url ?? 'Image';
}

function ImageAttachments({
  blocks,
  className,
}: {
  blocks: readonly { alt?: string; path?: string; url?: string }[];
  className?: string;
}) {
  const viewer = useSessionChatImageViewer();
  if (blocks.length === 0) {
    return null;
  }
  /*
  A picture shared in the conversation shows as the picture. The named chip
  stays as the honest stand-in for one that cannot be read here — a host with
  no image transport, or a file that has since gone — so a turn never renders
  a broken image well.
  */
  return (
    <div className={cn('flex min-w-0 flex-wrap gap-2 py-1', className)}>
      {blocks.map((block, index) => {
        const target = {
          ...(block.path !== undefined ? { path: block.path } : {}),
          ...(block.url !== undefined ? { url: block.url } : {}),
          ...(block.alt !== undefined ? { alt: block.alt } : {}),
        };
        const label = imageChipLabel(block);
        const chip = (
          <Attachment size='xs'>
            <AttachmentMedia>
              <IconPhoto aria-hidden='true' stroke={1.8} />
            </AttachmentMedia>
            <AttachmentContent>
              <AttachmentTitle>{label}</AttachmentTitle>
            </AttachmentContent>
            {viewer?.canOpen(target) === true ? (
              <AttachmentTrigger
                aria-label={`View ${label}`}
                className='cursor-zoom-in'
                onClick={() => viewer?.open(target)}
              />
            ) : null}
          </Attachment>
        );
        return <SessionChatInlineImage fallback={chip} key={index} target={{ ...target, alt: target.alt ?? label }} />;
      })}
    </div>
  );
}

function UserImageThumbnails({ blocks }: { blocks: readonly { alt?: string; path?: string; url?: string }[] }) {
  if (blocks.length === 0) {
    return null;
  }
  return (
    <div className='flex min-w-0 flex-wrap justify-end gap-1.5 py-1'>
      {blocks.map((block, index) => (
        <SessionChatImageReference
          key={block.path ?? block.url ?? index}
          label={block.alt?.trim() || `Image #${index + 1}`}
          target={{
            ...(block.path !== undefined ? { path: block.path } : {}),
            ...(block.url !== undefined ? { url: block.url } : {}),
            ...(block.alt !== undefined ? { alt: block.alt } : {}),
          }}
        />
      ))}
    </div>
  );
}

function CopyFooter({
  anchoredToAssistantMarker = false,
  markdown,
  onSaveMarkdown,
}: {
  anchoredToAssistantMarker?: boolean;
  markdown: string;
  onSaveMarkdown?: (markdown: string) => void;
}) {
  const canSaveMarkdown = markdown.split(/\r?\n/u).filter((line) => line.trim().length > 0).length > 1;
  return (
    <MessageFooter
      className={cn(
        'px-0',
        anchoredToAssistantMarker
          ? 'ghostex-chat-final-actions'
          : 'opacity-0 transition-opacity group-hover/message:opacity-100 group-focus-within/message:opacity-100'
      )}
    >
      <Button
        aria-label='Copy message'
        className={anchoredToAssistantMarker ? 'ghostex-chat-final-action ghostex-chat-final-action-copy' : undefined}
        onClick={() => {
          void navigator.clipboard.writeText(markdown);
        }}
        size='icon-xs'
        title='Copy message'
        variant='ghost'
      >
        <IconCopy aria-hidden='true' data-icon='inline-start' stroke={1.9} />
      </Button>
      {anchoredToAssistantMarker && onSaveMarkdown && canSaveMarkdown ? (
        <Button
          aria-label='Save message to Markdown'
          className='ghostex-chat-final-action ghostex-chat-final-action-save'
          onClick={() => onSaveMarkdown(markdown)}
          size='icon-xs'
          title='Save to md'
          variant='ghost'
        >
          <IconFile aria-hidden='true' data-icon='inline-start' stroke={1.9} />
        </Button>
      ) : null}
    </MessageFooter>
  );
}

/** Marks a prompt the agent has accepted but not started on yet. */
function QueuedLabel() {
  return (
    <div className='ghostex-chat-queued-label self-end' data-queued='true'>
      Queued
    </div>
  );
}

/**
 * A harness-injected turn the terminal prints too: one muted line that expands
 * to the verbatim text. Collapsed by default so orchestration chatter never
 * buries the conversation, present so it is never silently missing.
 */
function SuppressedTurn({ label, text }: { label: string; text: string }) {
  const [expanded, setExpanded] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  return (
    <div className='flex w-full min-w-0 flex-col gap-1.5 pb-2'>
      <button
        aria-expanded={expanded}
        className='ghostex-chat-suppressed-trigger self-start'
        // Opts out of the sidebar's legacy `button:where(:not([data-slot]))`
        // base, which otherwise paints a 1px app border around the marker.
        data-slot='session-chat-suppressed-trigger'
        onClick={() => {
          if (!expanded) {
            centerSessionChatExpansion(triggerRef.current);
          }
          setExpanded((value) => !value);
        }}
        ref={triggerRef}
        type='button'
      >
        <span className='ghostex-chat-marker-slot'>
          <IconChevronRight
            aria-hidden='true'
            className={cn('ghostex-chat-disclosure-chevron', expanded && 'is-open')}
            stroke={2}
          />
        </span>
        <span className='truncate'>{label}</span>
      </button>
      {expanded ? (
        <SessionChatExpansion label={`Collapse ${label}`} onCollapse={() => setExpanded(false)}>
          <div className='min-w-0 whitespace-pre-wrap break-words rounded-md border border-border/60 bg-muted/30 px-2.5 py-2 font-mono text-[11px] leading-relaxed text-muted-foreground'>
            {text}
          </div>
        </SessionChatExpansion>
      ) : null}
    </div>
  );
}

/**
 * A harness turn short enough to read in place: one muted line of prose with
 * the marker's label as its lead-in, styled like a reasoning line. Beats a
 * chevron the reader has to click to learn the task exited 0.
 */
function InlineSuppressedTurn({ label, text }: { label: string; text: string }) {
  return (
    <div className='ghostex-chat-suppressed-inline'>
      <div>
        <span className='ghostex-chat-suppressed-inline-label'>{label}</span>
        {text}
      </div>
    </div>
  );
}

const STATUS_TONE_ICON: Record<SessionChatStatusTone, { Icon: typeof IconCheck; className: string }> = {
  ok: { Icon: IconCheck, className: 'bg-emerald-500/15 text-emerald-400' },
  error: {
    Icon: IconAlertTriangle,
    className: 'bg-destructive/15 text-destructive',
  },
  neutral: { Icon: IconInfoCircle, className: 'bg-muted text-muted-foreground' },
};

/**
 * The one durable row for a completed action — a model/effort change, a
 * compaction, a background task reporting back. Non-expandable on purpose:
 * the label already says everything the row is for.
 */
function StatusRow({ label, tone = 'ok' }: { label: string; tone?: SessionChatStatusTone }) {
  const { Icon, className } = STATUS_TONE_ICON[tone];
  return (
    <div className='inline-flex max-w-full min-w-0 items-center gap-2 rounded-full border border-border/60 bg-muted/35 px-3 py-1.5 text-xs font-medium text-muted-foreground'>
      {/* Tone badge: a badge-tier glyph in a tinted round. This round is off
          the marker axis, so it takes the ramp's smallest tier — the semantic
          size only made the pill taller without aligning it to anything. */}
      <span className={cn('flex size-4 shrink-0 items-center justify-center rounded-full', className)}>
        <Icon aria-hidden='true' className='ghostex-chat-glyph-badge' />
      </span>
      <span className='min-w-0 [overflow-wrap:anywhere]'>{label}</span>
    </div>
  );
}

/** One row per status; a turn carrying several reports each of them. */
function StatusRows({ statuses }: { statuses: readonly SessionChatStatusRow[] }) {
  return (
    <div className='flex w-full min-w-0 flex-col items-start gap-1.5 pb-3'>
      {statuses.map((status, index) => (
        <StatusRow key={index} label={status.label} tone={status.tone} />
      ))}
    </div>
  );
}

/**
 * Reasoning turn ("thinking"). The body is real markdown — a reasoning summary
 * can carry lists, tables, and code just like an answer, and the old regex
 * strip flattened all of it into one gapless run of lines.
 *
 * `plainReasoningTeaser` still strips, but only for the ONE line shown on the
 * collapsed trigger: markdown cannot render inside a <button> (its links and
 * the code block's copy control are interactive) and a teaser wants no block
 * structure anyway.
 */
function plainReasoningTeaser(markdown: string): string {
  const text = markdown
    .replace(/```(?:[^\n]*)\n?([\s\S]*?)```/g, '$1')
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/^\s{0,3}(?:#{1,6}|>|[-+*]|\d+[.)])\s+/gm, '')
    .replace(/(?:\*\*|__|\*|_|~~)/g, '')
    .replace(/\\([\\`*_[\]{}()#+\-.!>])/g, '$1')
    .trim();
  return (
    text
      .split(/\n+/)
      .map((line) => line.trim())
      .find(Boolean) ?? ''
  );
}

/**
 * A first line that is a list item, a table row, or a fence opener means
 * something to the markdown renderer that plain text on a heading cannot carry,
 * so it stays in the body.
 */
const NON_HOISTABLE_REASONING_LINE = /^\s{0,3}(?:[-+*]\s|\d+[.)]\s|>|\||```|~~~)/;

/**
 * The disclosure heading carries the reasoning's OWN opening line, never the
 * word "Thinking". Verbose mode opens every reasoning turn by default, so the
 * static label produced a column of identical "Thinking" rows that said nothing
 * while the sentence under each of them said everything.
 *
 * The heading therefore OWNS that line and the body renders only what follows
 * it, so it is never printed twice — and the shape verbose mode actually
 * produces (a one-line thought in front of a tool call) costs exactly one row
 * with no body at all.
 *
 * Only a first line that is a paragraph of its own is hoisted: it is the whole
 * markdown, or a blank line ends it. Anything else — a hard-wrapped paragraph
 * whose sentence continues on the next line, a list, a fenced block — would be
 * cut mid-thought, so the body keeps all of it and the heading falls back to
 * the teaser.
 */
function splitReasoningHeadline(markdown: string): {
  headline: string;
  body: string;
} {
  const lines = markdown.split(/\r?\n/);
  const first = lines.findIndex((line) => line.trim().length > 0);
  const hoistable =
    first >= 0 &&
    (lines[first + 1] ?? '').trim().length === 0 &&
    !NON_HOISTABLE_REASONING_LINE.test(lines[first] ?? '');
  const headline = hoistable ? plainReasoningTeaser(lines[first] ?? '') : '';
  if (headline.length === 0) {
    return { headline: plainReasoningTeaser(markdown), body: markdown };
  }
  return {
    headline,
    body: lines
      .slice(first + 1)
      .join('\n')
      .trim(),
  };
}

/**
 * Answered question cards carried by a turn's tool blocks. They are
 * conversation, not work, so every disclosure that collapses tool activity
 * (thinking rows, agent-message tool sections) renders them OUTSIDE its fold
 * and passes questionPairsAsRows to the run inside, keeping the raw pair as a
 * plain row there. Empty when the parent already hoists them.
 */
function questionExchangesFromTools(
  tools: ReturnType<typeof splitSessionChatBlocks>['tools']
): SessionChatQuestionExchange[] {
  const out: SessionChatQuestionExchange[] = [];
  for (const pair of pairSessionChatToolBlocks(tools)) {
    const exchange = answeredSessionChatQuestionExchange(pair);
    if (exchange) {
      out.push(exchange);
    }
  }
  return out;
}

function QuestionExchangeCards({ exchanges }: { exchanges: readonly SessionChatQuestionExchange[] }) {
  if (exchanges.length === 0) {
    return null;
  }
  return (
    <div className='grid min-w-0 gap-3 py-1.5'>
      {exchanges.map((exchange, index) => (
        <SessionChatQuestionExchangeCard exchange={exchange} key={index} />
      ))}
    </div>
  );
}

/**
 * Tool activity owned by a plain agent message, collapsed behind one summary
 * row — the same reading the thinking lane gives its tools, so a turn's answer
 * is never pushed off screen by the work that produced it.
 */
function AgentToolsDisclosure({
  questionPairsAsRows,
  tools,
  verboseMode,
}: {
  questionPairsAsRows: boolean;
  tools: ReturnType<typeof splitSessionChatBlocks>['tools'];
  verboseMode: boolean;
}) {
  const [open, setOpen] = useState(verboseMode);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(verboseMode), [verboseMode]);

  const exchanges = questionPairsAsRows ? [] : questionExchangesFromTools(tools);
  const count = countSessionChatToolCalls(tools);
  const label = count === 0 ? 'Tool output' : count === 1 ? '1 tool call' : `${count} tool calls`;
  // A question's JSON input is noise in the preview: its card carries it.
  const summary = summarizeSessionChatToolRun(
    tools.filter((block) => block.type !== 'tool-call' || !isSessionChatQuestionToolName(block.name))
  );

  return (
    <>
      <div className='ghostex-chat-tool-run'>
        <button
          aria-expanded={open}
          className='ghostex-chat-tool-run-toggle'
          onClick={() => {
            if (!open) {
              centerSessionChatExpansion(triggerRef.current);
            }
            setOpen((value) => !value);
          }}
          ref={triggerRef}
          type='button'
        >
          <span className='ghostex-chat-work-icon'>
            <IconChevronRight aria-hidden='true' className={cn('ghostex-chat-disclosure-chevron', open && 'is-open')} />
          </span>
          <span className='shrink-0'>{label}</span>
          {!open && summary ? <span className='ghostex-chat-work-preview'>{summary}</span> : null}
        </button>
        {open ? (
          <SessionChatExpansion
            bodyClassName='ghostex-chat-tool-run-expanded'
            label='Collapse tool calls'
            onCollapse={() => setOpen(false)}
          >
            <SessionChatToolRun blocks={tools} questionPairsAsRows showAllRows />
          </SessionChatExpansion>
        ) : null}
      </div>
      <QuestionExchangeCards exchanges={exchanges} />
    </>
  );
}

function ReasoningRow({
  isStreaming,
  markdown,
  questionPairsAsRows,
  tools,
  verboseMode,
}: {
  isStreaming: boolean;
  markdown: string;
  questionPairsAsRows: boolean;
  tools: ReturnType<typeof splitSessionChatBlocks>['tools'];
  verboseMode: boolean;
}) {
  const [open, setOpen] = useState(verboseMode);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(verboseMode), [verboseMode]);

  const renderBody = (value: string) => (
    <SessionChatScrollCap className='ghostex-chat-thinking-body'>
      <SessionChatMarkdown isStreaming={isStreaming} markdown={value} />
    </SessionChatScrollCap>
  );

  // With tools, the caret owns BOTH the reasoning body and the tool rows: a
  // long reasoning turn collapses to its first line instead of pushing the
  // answer it belongs to off the screen. Verbose mode still opens it by
  // default, so nothing is hidden from anyone who wants it. Answered question
  // cards escape the collapse — they are conversation, not work.
  if (tools.length > 0) {
    const { headline, body: detail } = splitReasoningHeadline(markdown);
    const exchanges = questionPairsAsRows ? [] : questionExchangesFromTools(tools);
    return (
      <>
        <div className='ghostex-chat-thinking-row is-disclosure'>
          <button
            aria-expanded={open}
            className='ghostex-chat-thinking-trigger'
            onClick={() => {
              if (!open) {
                centerSessionChatExpansion(triggerRef.current);
              }
              setOpen((value) => !value);
            }}
            ref={triggerRef}
            type='button'
          >
            {/* The reasoning disclosure used to draw a filled clip-path triangle
              here while the tool rows below it drew a stroke chevron: two
              disclosure metaphors on one column, which read as two STATES
              rather than two rows. One glyph now, on the control tier. */}
            <span className='ghostex-chat-thinking-icon'>
              <IconChevronRight
                aria-hidden='true'
                className={cn('ghostex-chat-disclosure-chevron', open && 'is-open')}
              />
            </span>
            <span className='ghostex-chat-thinking-text'>
              {/* The reasoning's first line, open or collapsed: expanding a turn
                reveals what follows it, it does not relabel it. */}
              <span data-ghostex-thinking-text>{headline}</span>
            </span>
          </button>
          {open ? (
            <SessionChatExpansion
              className='ghostex-chat-thinking-detail'
              label='Collapse thinking'
              onCollapse={() => setOpen(false)}
            >
              {detail.length > 0 ? renderBody(detail) : null}
              <SessionChatToolRun blocks={tools} questionPairsAsRows showAllRows />
            </SessionChatExpansion>
          ) : null}
        </div>
        <QuestionExchangeCards exchanges={exchanges} />
      </>
    );
  }

  return (
    <div className='ghostex-chat-thinking-row'>
      <div className='ghostex-chat-thinking-line'>
        <div data-ghostex-thinking-text>{renderBody(markdown)}</div>
      </div>
    </div>
  );
}

/*
Codex can fold rapid/steered inputs into one transcript turn with a line that
contains only "---". Rendering that transport separator as Markdown turns the
entire preceding paragraph into a Setext h2. It can also repeat an earlier
input after the separator (the repeated part is normally a prefix of the
combined part). Present those inputs as ordinary paragraphs and collapse the
repeated prefix instead of exposing transport syntax in the user's bubble.
*/
const USER_TURN_SEPARATOR = /\r?\n[\t ]*---[\t ]*(?:\r?\n|$)/;

function normalizeUserMessageMarkdown(markdown: string): string {
  const parts = markdown.split(USER_TURN_SEPARATOR).map((part) => part.trim());
  if (parts.length === 1) {
    return markdown;
  }

  const visible: string[] = [];
  for (const part of parts) {
    if (!part) {
      continue;
    }
    const containingIndex = visible.findIndex((candidate) => candidate.startsWith(part));
    if (containingIndex < 0) {
      visible.push(part);
      continue;
    }

    const remainder = visible[containingIndex]?.slice(part.length).trimStart() ?? '';
    visible[containingIndex] = remainder ? `${part}\n\n${remainder}` : part;
  }
  return visible.join('\n\n');
}

/*
 * Legacy agent transcripts carry a picture as a separate image block. Copy has
 * to restore a named reference for those blocks or the reader loses the one
 * thing that names the file they attached. Modern linked references stay in
 * the turn's text, so both their authored position and copyable path survive.
 */
function userTurnCopyMarkdown(markdown: string, images: readonly { path?: string; url?: string }[]): string {
  const references = images
    .map((block, index) => {
      const href = block.path ?? block.url;
      return href === undefined ? '' : `[Image #${index + 1}](${href})`;
    })
    .filter((reference) => reference !== '');
  return [references.join(' '), markdown].filter((part) => part !== '').join('\n\n');
}

function MessageRow({
  isStreaming = false,
  message,
  onSaveMarkdown,
  questionPairsAsRows = false,
  showAssistantCopy,
  verboseMode,
}: {
  /**
   * True while the agent is still appending to this row. Only the markdown
   * renderer's syntax highlighting reads it (a fence that is still growing must
   * not be re-tokenized per chunk, and must not enter the highlight cache).
   */
  isStreaming?: boolean;
  message: SessionChatMessage;
  onSaveMarkdown?: (markdown: string) => void;
  /** Set inside the expanded completed-work log, where the hoisted question
   * card already shows any answered question this message carries. */
  questionPairsAsRows?: boolean;
  showAssistantCopy: boolean;
  verboseMode: boolean;
}) {
  const { prose, tools } = splitSessionChatBlocks(message.blocks);
  const markdown = prose
    .filter((block) => block.type === 'text')
    .map((block) => (block.type === 'text' ? block.text : ''))
    .join('\n\n');
  const images = prose.filter((block) => block.type === 'image-ref');

  // No ghost bubbles: skip entirely when there is nothing to show.
  if (markdown.length === 0 && images.length === 0 && tools.length === 0) {
    return null;
  }

  const suppressedTurn = sessionChatSuppressedTurnPresentation(message);
  if (suppressedTurn !== null) {
    if (suppressedTurn.kind === 'status') {
      return (
        <StatusRows
          statuses={suppressedTurn.statuses ?? [{ label: suppressedTurn.label, tone: suppressedTurn.tone ?? 'ok' }]}
        />
      );
    }
    if (suppressedTurn.kind === 'inline') {
      return <InlineSuppressedTurn label={suppressedTurn.label} text={suppressedTurn.text} />;
    }
    return <SuppressedTurn label={suppressedTurn.label} text={suppressedTurn.text} />;
  }

  const isUser = message.role === 'user';
  const isReasoning = message.role === 'reasoning';
  const isSystem = message.role === 'system';
  const userMarkdown = isUser ? normalizeUserMessageMarkdown(markdown) : '';
  const userCopyMarkdown = isUser ? userTurnCopyMarkdown(userMarkdown, images) : '';
  const showCopy = isUser
    ? userCopyMarkdown.length > 0
    : markdown.length > 0 && message.role === 'assistant' && showAssistantCopy;

  const autoNamedTitle =
    message.id.startsWith('app-command:') &&
    message.blocks[0]?.type === 'text' &&
    message.blocks[0].text === 'Ghostex auto named this session' &&
    message.blocks[1]?.type === 'text'
      ? message.blocks[1].text.trim()
      : '';
  if (isSystem && autoNamedTitle) {
    return (
      <Marker className='pb-3'>
        <div className='inline-flex max-w-full items-start gap-2.5 rounded-2xl border border-border/70 bg-muted/40 px-3.5 py-2.5 shadow-sm'>
          <IconSparkles aria-hidden='true' className='mt-0.5 size-4 shrink-0 text-muted-foreground' stroke={1.8} />
          <span className='flex min-w-0 flex-col gap-0.5'>
            <span className='text-sm font-medium leading-5 text-foreground'>Ghostex auto named this session</span>
            <span className='wrap-break-word text-xs leading-4 text-muted-foreground'>
              New name: <span className='text-foreground/85'>{autoNamedTitle}</span>
            </span>
          </span>
        </div>
      </Marker>
    );
  }

  /*
  CDXC:SessionForkFamilies 2026-08-28:
  The seam where stitched scroll-back crosses from one fork ancestor into the
  next. gxserver synthesizes it as a system row, but it is not a note about the
  session: it is the boundary between two threads, so it reads as a labeled
  horizontal rule instead of another centered sentence. The text stays exactly
  as the daemon wrote it.
  */
  if (isSystem && message.id.startsWith(SESSION_CHAT_FORK_BOUNDARY_ID_PREFIX)) {
    return (
      <Marker className='pt-1 pb-3' variant='separator'>
        <MarkerContent className='inline-flex items-center gap-1.5'>
          <MarkerIcon className='size-3.5'>
            <IconGitBranch aria-hidden='true' className='size-3.5' stroke={2} />
          </MarkerIcon>
          {markdown}
        </MarkerContent>
      </Marker>
    );
  }

  if (isSystem) {
    return (
      <Marker className='pb-2'>
        <MarkerContent>{markdown}</MarkerContent>
      </Marker>
    );
  }

  /*
   * ONLY a genuine reasoning turn goes to the thinking lane. This used to also
   * catch any turn carrying a tool call, which silently demoted real answers:
   * `foldSessionChatToolMessages` folds the following tool-only rows INTO the
   * assistant turn, so a plain prose answer followed by a tool call was
   * rendered as stripped, unformatted thinking. An assistant turn now keeps
   * its markdown and shows the tools it owns beneath it.
   */
  if (isReasoning && markdown.length > 0 && images.length === 0) {
    return (
      <ReasoningRow
        isStreaming={isStreaming}
        markdown={markdown}
        questionPairsAsRows={questionPairsAsRows}
        tools={tools}
        verboseMode={verboseMode}
      />
    );
  }

  if (isUser) {
    /*
     * The "Queued" label is driven by the agent's own queue bookkeeping in
     * the transcript (`message.queued`). An optimistic echo carries the flag
     * only when the send was issued mid-response — the agent will hold that
     * prompt, so the echo pre-renders the queued row that replaces it and the
     * swap stays invisible. The server retracts the queued row the moment the
     * queue releases it, so the label cannot outlive the wait.
     */
    return (
      <Message align='end' className='pb-4' data-role='user'>
        <MessageContent>
          {message.queued === true ? <QueuedLabel /> : null}
          {/* justify-end keeps wrapped rows against the user's side. */}
          <UserImageThumbnails blocks={images} />
          {userMarkdown.length > 0 ? (
            <Bubble align='end' className='ghostex-chat-user-bubble' variant='default'>
              <BubbleContent>
                <SessionChatMarkdown chatText markdown={userMarkdown} />
              </BubbleContent>
            </Bubble>
          ) : null}
          {showCopy ? <CopyFooter markdown={userCopyMarkdown} /> : null}
        </MessageContent>
      </Message>
    );
  }

  return (
    <Message align='start' className='pb-4' data-role={message.role}>
      <MessageContent>
        <ImageAttachments blocks={images} />
        {markdown.length > 0 ? (
          <div className='ghostex-chat-agent-message'>
            <SessionChatMarkdown isStreaming={isStreaming} markdown={markdown} />
          </div>
        ) : null}
        {/*
         * Tools owned by a prose turn collapse behind a summary row, matching
         * the thinking lane's treatment of its tool activity. A tool-only
         * message keeps the open run: with no prose above it, the disclosure
         * would collapse the turn to nothing.
         */}
        {tools.length > 0 ? (
          markdown.length > 0 ? (
            <AgentToolsDisclosure questionPairsAsRows={questionPairsAsRows} tools={tools} verboseMode={verboseMode} />
          ) : (
            <SessionChatToolRun blocks={tools} questionPairsAsRows={questionPairsAsRows} />
          )
        ) : null}
        {showCopy ? <CopyFooter anchoredToAssistantMarker markdown={markdown} onSaveMarkdown={onSaveMarkdown} /> : null}
      </MessageContent>
    </Message>
  );
}

interface CompletedWorkTurn {
  final: SessionChatMessage;
  user: SessionChatMessage;
  work: SessionChatMessage[];
}

interface SummaryModeTurn {
  active: boolean;
  activeWork: SessionChatMessage[];
  final: SessionChatMessage | null;
  user: SessionChatMessage;
}

type SessionChatRenderItem =
  { kind: 'message'; message: SessionChatMessage } | { kind: 'completed-work'; turn: CompletedWorkTurn };

function hasAgentResponseContent(message: SessionChatMessage): boolean {
  return (
    message.role === 'assistant' &&
    message.id !== SESSION_CHAT_STREAMING_ID &&
    message.blocks.some(
      (block) => block.type === 'image-ref' || (block.type === 'text' && block.text.trim().length > 0)
    )
  );
}

/** One compact row per genuine user prompt, paired with its settled final reply. */
function summaryModeTurns(
  messages: readonly SessionChatMessage[],
  finalAssistantMessageIds: ReadonlySet<string>,
  isWorking: boolean
): SummaryModeTurn[] {
  const turns: SummaryModeTurn[] = [];
  let current: SummaryModeTurn | null = null;

  for (const message of messages) {
    // A held prompt (agent-CLI queue row, mid-turn send echo — `queued`) has
    // not started its own response yet: it stays inside the working turn's
    // activeWork instead of opening a turn whose reply would never come.
    const isGenuineUserMessage =
      message.role === 'user' && message.queued !== true && sessionChatSuppressedTurnLabel(message) === null;
    if (isGenuineUserMessage) {
      current = { active: false, activeWork: [], final: null, user: message };
      turns.push(current);
    } else if (current !== null) {
      current.activeWork.push(message);
      if (finalAssistantMessageIds.has(message.id)) {
        current.final = message;
      }
    }
  }
  const newest = turns.at(-1);
  if (isWorking && newest) {
    newest.active = true;
  }
  return turns;
}

function isVisibleAssistantArtifact(message: SessionChatMessage): boolean {
  return message.role === 'assistant' && message.blocks.some((block) => block.type === 'image-ref');
}

/**
 * Where the response the agent is CURRENTLY producing begins: the last user
 * row that is a genuine prompt the agent has accepted for delivery. A
 * harness-injected turn (task notification, local command output) and a
 * prompt the agent CLI is still holding in its queue (`queued`, including the
 * optimistic echo of a send made mid-turn) both land as user rows WHILE the
 * agent is mid-response — none of them starts a new response, so none of them
 * may settle the one in flight. Falls back to 0 when no such prompt exists
 * (stitched scroll-back that opens mid-conversation): the whole tail is the
 * live response then.
 */
function activeResponseStartIndex(messages: readonly SessionChatMessage[]): number {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (
      message &&
      message.role === 'user' &&
      message.queued !== true &&
      sessionChatSuppressedTurnLabel(message) === null
    ) {
      return index;
    }
  }
  return 0;
}

/** One copy affordance per response: the last assistant text before the next user turn. */
function finalAssistantMessageIds(messages: readonly SessionChatMessage[], isWorking: boolean): ReadonlySet<string> {
  const ids = new Set<string>();
  let finalAssistantId: string | null = null;
  const activeStart = isWorking ? activeResponseStartIndex(messages) : messages.length;

  const commitTurn = (): void => {
    if (finalAssistantId !== null) {
      ids.add(finalAssistantId);
      finalAssistantId = null;
    }
  };

  messages.forEach((message, index) => {
    // A harness-injected turn (a background-task notification, local command
    // output, a message from another session) is authored by the terminal, not
    // by the reader: the agent is still mid-response on both sides of it.
    // Ending the turn there put a copy affordance under commentary that the
    // agent then kept building on.
    if (sessionChatSuppressedTurnLabel(message) !== null) {
      return;
    }
    if (message.role === 'user') {
      // A user row past the active response's start is a held prompt (the
      // agent's queue, or a mid-turn send's echo): the text before it is
      // still commentary, so it must not mint a final reply.
      if (index > activeStart) {
        finalAssistantId = null;
      } else {
        commitTurn();
      }
      return;
    }
    if (
      message.role === 'assistant' &&
      message.blocks.some((block) => block.type === 'text' && block.text.trim().length > 0)
    ) {
      finalAssistantId = message.id;
    }
  });
  // The newest assistant text is only a final reply once the turn has
  // finished. While the agent is still working it is commentary, even when it
  // happens to be the most recent text block for a moment.
  if (!isWorking) {
    commitTurn();
  }
  return ids;
}

/**
 * A completed interaction keeps the user's message and the agent's final
 * response in the normal transcript flow. Everything the agent emitted in
 * between becomes one collapsed work section.
 *
 * While the agent is still working, everything from the active response's
 * start onward stays expanded. "Newest turn" is NOT enough for that guard: a
 * harness-injected user row (task notification, local command output) or a
 * held prompt (agent-CLI queue row, mid-turn send echo) lands mid-response
 * and would close the streaming turn the moment it appears — folding live
 * work into a "Worked for" row and yanking the bottom-pinned viewport onto
 * it, only for the fold to vanish again when the injected row settles.
 */
function completedWorkRenderItems(
  messages: readonly SessionChatMessage[],
  isWorking: boolean
): SessionChatRenderItem[] {
  const items: SessionChatRenderItem[] = [];
  const activeStart = isWorking ? activeResponseStartIndex(messages) : messages.length;
  let index = 0;
  while (index < messages.length) {
    const message = messages[index];
    if (!message || message.role !== 'user') {
      if (message) {
        items.push({ kind: 'message', message });
      }
      index += 1;
      continue;
    }

    let nextUserIndex = index + 1;
    while (nextUserIndex < messages.length && messages[nextUserIndex]?.role !== 'user') {
      nextUserIndex += 1;
    }
    const turnMessages = messages.slice(index + 1, nextUserIndex);
    let finalIndex = -1;
    for (let turnIndex = turnMessages.length - 1; turnIndex >= 0; turnIndex -= 1) {
      const candidate = turnMessages[turnIndex];
      if (candidate && hasAgentResponseContent(candidate)) {
        finalIndex = turnIndex;
        break;
      }
    }
    if (finalIndex < 0 || index >= activeStart) {
      items.push({ kind: 'message', message });
      for (const turnMessage of turnMessages) {
        items.push({ kind: 'message', message: turnMessage });
      }
      index = nextUserIndex;
      continue;
    }

    const final = turnMessages[finalIndex];
    if (!final) {
      items.push({ kind: 'message', message });
      index += 1;
      continue;
    }
    items.push({ kind: 'message', message });
    items.push({
      kind: 'completed-work',
      turn: {
        final,
        user: message,
        work: turnMessages.filter((_, turnIndex) => turnIndex !== finalIndex),
      },
    });
    index = nextUserIndex;
  }
  return items;
}

function workedDurationLabel(startedAt: number | null, completedAt: number | null): string {
  if (startedAt === null || completedAt === null || completedAt < startedAt) {
    return 'Worked';
  }
  const seconds = Math.max(1, Math.round((completedAt - startedAt) / 1000));
  if (seconds < 60) {
    return `Worked for ${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return `Worked for ${minutes}m${remainder > 0 ? ` ${remainder}s` : ''}`;
}

/**
 * Answered agent questions buried inside a completed turn's work. The user's
 * answer never writes a user row to the transcript — the whole ask/answer
 * exchange lives in tool blocks between two user turns — so without hoisting
 * it would vanish into the collapsed "Worked for Xs" section. The raw tool
 * rows stay in the expanded work log (questionPairsAsRows), so nothing renders
 * twice.
 */
function hoistedQuestionExchanges(
  work: readonly SessionChatMessage[]
): { exchange: SessionChatQuestionExchange; key: string }[] {
  const out: { exchange: SessionChatQuestionExchange; key: string }[] = [];
  for (const message of work) {
    const { tools } = splitSessionChatBlocks(message.blocks);
    pairSessionChatToolBlocks(tools).forEach((pair, index) => {
      const exchange = answeredSessionChatQuestionExchange(pair);
      if (exchange) {
        out.push({ exchange, key: `${message.id}:${index}` });
      }
    });
  }
  return out;
}

function CompletedWork({
  onExpand,
  onSaveMarkdown,
  showAssistantCopy,
  turn,
  verboseMode,
}: {
  onExpand: (target: HTMLElement | null) => void;
  onSaveMarkdown?: (markdown: string) => void;
  /**
   * A folded turn ends at the next user row, and a harness-injected turn
   * (task notification, local command output) is one of those rows — so this
   * turn's `final` can still be mid-response. Only the real end of the
   * response carries the copy affordance.
   */
  showAssistantCopy: boolean;
  turn: CompletedWorkTurn;
  verboseMode: boolean;
}) {
  const [open, setOpen] = useState(verboseMode);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(verboseMode), [verboseMode]);
  const visibleArtifacts = turn.work.filter(isVisibleAssistantArtifact);
  const collapsedWork = turn.work.filter((message) => !isVisibleAssistantArtifact(message));
  const hasWork = collapsedWork.length > 0;
  const questionExchanges = useMemo(() => hoistedQuestionExchanges(turn.work), [turn.work]);

  return (
    <div className='ghostex-chat-completed-turn'>
      <div className='ghostex-chat-completed-work'>
        <Button
          aria-expanded={hasWork ? open : undefined}
          className='ghostex-chat-completed-work-trigger'
          disabled={!hasWork}
          onClick={() => {
            if (hasWork) {
              if (!open) {
                onExpand(triggerRef.current);
              }
              setOpen((value) => !value);
            }
          }}
          ref={triggerRef}
          size='xs'
          type='button'
          variant='ghost'
        >
          {/* The chevron LEADS, in the transcript's marker slot, like every
              other disclosure. It used to trail the label, which left this row
              as the only expander on the surface whose glyph was not on the
              column. The slot stays even with no work to disclose, so a turn
              with nothing behind it does not shift its label left. */}
          <span className='ghostex-chat-marker-slot'>
            {hasWork ? (
              <IconChevronRight
                aria-hidden='true'
                className={cn('ghostex-chat-disclosure-chevron', open && 'is-open')}
              />
            ) : null}
          </span>
          <span>{workedDurationLabel(turn.user.timestamp, turn.final.timestamp)}</span>
        </Button>
        <Separator />
        {hasWork && open ? (
          <SessionChatExpansion
            bodyClassName='ghostex-chat-completed-work-content'
            label='Collapse completed work'
            onCollapse={() => setOpen(false)}
          >
            {collapsedWork.map((message) => (
              <MessageRow
                key={message.id}
                message={message}
                questionPairsAsRows
                showAssistantCopy={false}
                verboseMode={verboseMode}
              />
            ))}
          </SessionChatExpansion>
        ) : null}
      </div>
      {visibleArtifacts.map((message) => (
        <MessageRow key={message.id} message={message} showAssistantCopy={false} verboseMode={verboseMode} />
      ))}
      {questionExchanges.length > 0 ? (
        <Message align='start' className='pb-4' data-role='question-exchange'>
          <MessageContent>
            {questionExchanges.map(({ exchange, key }) => (
              <SessionChatQuestionExchangeCard exchange={exchange} key={key} />
            ))}
          </MessageContent>
        </Message>
      ) : null}
      <MessageRow
        message={turn.final}
        onSaveMarkdown={onSaveMarkdown}
        showAssistantCopy={showAssistantCopy}
        verboseMode={verboseMode}
      />
    </div>
  );
}

function WorkingIndicator() {
  return (
    <div
      aria-label='Agent is responding'
      aria-live='polite'
      className='flex h-8 items-center gap-1.5 text-muted-foreground'
      role='status'
    >
      {[0, 1, 2].map((index) => (
        <span
          className='size-1.5 animate-bounce rounded-full bg-muted-foreground/70'
          key={index}
          style={{ animationDelay: `${index * 160}ms` }}
        />
      ))}
    </div>
  );
}

/**
 * A local send must bring the newest row back into view even when the reader
 * had scrolled up, without asking message-scroller to anchor that row to the
 * top of the viewport (top anchoring pads the transcript with a spacer and
 * leaves a scrollable empty gap above the composer).
 */
function ScrollToLatestSend({ pendingMessageId }: { pendingMessageId: string | null }): null {
  const { scrollToEnd } = useMessageScroller();
  const handledRef = useRef<string | null>(null);

  useEffect(() => {
    if (pendingMessageId === null || handledRef.current === pendingMessageId) {
      return;
    }
    handledRef.current = pendingMessageId;
    scrollToEnd({ behavior: 'smooth' });
  }, [pendingMessageId, scrollToEnd]);

  return null;
}

export function SessionChatMessageList({
  hasMore,
  isWorking,
  terminalActivity,
  loadingEarlier,
  messages,
  onLoadEarlier,
  listMessageMarkdownPaths,
  saveMessageMarkdown,
  sessionTitle = '',
  summaryMode = false,
  theme = 'dark',
  verboseMode = false,
}: SessionChatMessageListProps) {
  const loadingEarlierRef = useRef(loadingEarlier);
  loadingEarlierRef.current = loadingEarlier;
  const hasMoreRef = useRef(hasMore);
  hasMoreRef.current = hasMore;
  const contentRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const shouldFollowBottomRef = useRef(true);
  const scrollbarFadeTimeoutRef = useRef<number | undefined>(undefined);
  const [markdownToSave, setMarkdownToSave] = useState<string | null>(null);
  const anchorExpandedAreaTop = useCallback((target: HTMLElement | null): void => {
    // Opening a disclosure is explicit navigation away from the newest row.
    // Clear bottom-follow before its resize can pin the viewport to the end.
    shouldFollowBottomRef.current = false;
    anchorSessionChatExpansionTop(target);
  }, []);

  useEffect(
    () => () => {
      if (scrollbarFadeTimeoutRef.current !== undefined) {
        window.clearTimeout(scrollbarFadeTimeoutRef.current);
      }
    },
    []
  );

  // Remember bottom-follow intent before content growth changes scrollHeight.
  useEffect(() => {
    const content = contentRef.current;
    if (!content) {
      return;
    }
    const observer = new ResizeObserver(() => {
      const viewport = viewportRef.current;
      if (viewport && shouldFollowBottomRef.current) {
        viewport.scrollTop = viewport.scrollHeight;
      }
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, []);

  const loadEarlierIfNearTop = useCallback(
    (viewport: HTMLDivElement): void => {
      if (viewport.scrollTop < LOAD_EARLIER_SCROLL_TOP_PX && hasMoreRef.current && !loadingEarlierRef.current) {
        onLoadEarlier();
      }
    },
    [onLoadEarlier]
  );

  /*
   * A scroll that reaches the top while a page is already loading cannot start
   * another request. Re-check after that page settles: prepend preservation may
   * move the reader away from the boundary, but if it remains near the top the
   * next page starts without requiring another wheel event or a manual button.
   * This also fills a viewport whose initial transcript is too short to scroll.
   */
  useEffect(() => {
    const viewport = viewportRef.current;
    if (viewport) {
      loadEarlierIfNearTop(viewport);
    }
  }, [hasMore, loadingEarlier, loadEarlierIfNearTop, messages.length]);

  // Auto-load older history before the reader reaches the top; the viewport's
  // preserveScrollOnPrepend keeps the visible rows in place when the earlier
  // page lands. Every scroll also stamps the viewport so the scrollbar shows
  // while scrolling and fades out afterwards (chat.css).
  const handleScroll = useCallback(
    (event: React.UIEvent<HTMLDivElement>): void => {
      const viewport = event.currentTarget;
      shouldFollowBottomRef.current =
        viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight <= AUTO_SCROLL_EDGE_THRESHOLD_PX;
      viewport.setAttribute('data-user-scrolling', 'true');
      if (scrollbarFadeTimeoutRef.current !== undefined) {
        window.clearTimeout(scrollbarFadeTimeoutRef.current);
      }
      scrollbarFadeTimeoutRef.current = window.setTimeout(() => {
        viewport.removeAttribute('data-user-scrolling');
      }, SCROLLBAR_FADE_MS);
      loadEarlierIfNearTop(viewport);
    },
    [loadEarlierIfNearTop]
  );

  const rendered = useMemo(
    () =>
      foldSessionChatToolMessages(
        dropSessionChatHiddenMessages(normalizeSessionChatImageTranscriptMessages(orderSessionChatMessages(messages))),
        // Collapsed markers must not break a tool-fold run.
        (message) => sessionChatSuppressedTurnLabel(message) !== null
      ),
    [messages]
  );

  const showActivity = !summaryMode && terminalActivity != null;
  const showTypingIndicator =
    !summaryMode && !showActivity && isWorking && !messages.some((message) => message.id === SESSION_CHAT_STREAMING_ID);
  const renderItems = useMemo(() => completedWorkRenderItems(rendered, isWorking), [isWorking, rendered]);
  const copyableAssistantMessageIds = useMemo(
    () => finalAssistantMessageIds(rendered, isWorking),
    [isWorking, rendered]
  );
  const summaryTurns = useMemo(
    () => summaryModeTurns(rendered, copyableAssistantMessageIds, isWorking),
    [copyableAssistantMessageIds, isWorking, rendered]
  );

  const pendingMessageId = useMemo(() => {
    for (let index = rendered.length - 1; index >= 0; index -= 1) {
      const candidate = rendered[index];
      if (candidate && isSessionChatPendingMessageId(candidate.id)) {
        return candidate.id;
      }
    }
    return null;
  }, [rendered]);

  return (
    <MessageScrollerProvider autoScroll defaultScrollPosition='end' scrollEdgeThreshold={AUTO_SCROLL_EDGE_THRESHOLD_PX}>
      <ScrollToLatestSend pendingMessageId={pendingMessageId} />
      <MessageScroller className='flex-1'>
        {/* RTL viewport + LTR content puts the scrollbar on the left edge. */}
        {/* outline-none: Chromium makes scrollers keyboard-focusable and paints
            its default focus ring on them; a transcript is not a control. */}
        <MessageScrollerViewport
          className='outline-none [direction:rtl]'
          onScroll={handleScroll}
          preserveScrollOnPrepend
          ref={viewportRef}
        >
          <MessageScrollerContent
            className='mx-auto w-full max-w-3xl gap-0 px-4 pt-8 pb-4 [direction:ltr]'
            ref={contentRef}
          >
            {summaryMode
              ? summaryTurns.map((turn) => (
                  <MessageScrollerItem key={`summary:${turn.user.id}`} messageId={turn.user.id}>
                    <MessageRow message={turn.user} showAssistantCopy={false} verboseMode={verboseMode} />
                    {turn.final ? (
                      <SessionChatDisclosure key='agent-reply' label='Agent reply' onExpand={anchorExpandedAreaTop}>
                        <MessageRow
                          message={turn.final}
                          {...(saveMessageMarkdown && listMessageMarkdownPaths
                            ? { onSaveMarkdown: setMarkdownToSave }
                            : {})}
                          showAssistantCopy={copyableAssistantMessageIds.has(turn.final.id)}
                          verboseMode={verboseMode}
                        />
                      </SessionChatDisclosure>
                    ) : turn.active ? (
                      <SessionChatDisclosure key='active-work' label='Active work' onExpand={anchorExpandedAreaTop}>
                        {turn.activeWork.map((message, index) => (
                          <MessageRow
                            isStreaming={index === turn.activeWork.length - 1}
                            key={message.id}
                            message={message}
                            showAssistantCopy={false}
                            verboseMode={verboseMode}
                          />
                        ))}
                        {terminalActivity ? <SessionChatActivityRow activity={terminalActivity} /> : null}
                        {!terminalActivity && !messages.some((message) => message.id === SESSION_CHAT_STREAMING_ID) ? (
                          <WorkingIndicator />
                        ) : null}
                      </SessionChatDisclosure>
                    ) : null}
                  </MessageScrollerItem>
                ))
              : renderItems.map((item, index) => (
                  <MessageScrollerItem
                    key={
                      item.kind === 'message'
                        ? item.message.id
                        : `completed-work:${item.turn.user.id}:${item.turn.final.id}`
                    }
                    messageId={item.kind === 'message' ? item.message.id : item.turn.final.id}
                    // No row is a scroll anchor: anchoring a message to the top of
                    // the viewport makes message-scroller pad the transcript with a
                    // spacer so that message can reach the top, which leaves a
                    // viewport-sized scrollable gap between the newest row and the
                    // composer until the reply grows tall enough to fill it.
                    // Following the bottom keeps the newest row above the composer.
                  >
                    {item.kind === 'message' ? (
                      <MessageRow
                        /*
                         * Only the newest row can still be growing, and only while
                         * the agent is working: transcript tailing appends to the
                         * last message, and the synthetic streaming preview row is
                         * always last when it exists. Earlier rows are settled, so
                         * their code fences are safe to highlight and cache.
                         * `completedWorkRenderItems` never folds the active
                         * response while working, so a "completed-work" item is
                         * settled by construction and keeps the default
                         * `isStreaming={false}`.
                         */
                        isStreaming={isWorking && index === renderItems.length - 1}
                        message={item.message}
                        {...(saveMessageMarkdown && listMessageMarkdownPaths
                          ? { onSaveMarkdown: setMarkdownToSave }
                          : {})}
                        showAssistantCopy={copyableAssistantMessageIds.has(item.message.id)}
                        verboseMode={verboseMode}
                      />
                    ) : (
                      <CompletedWork
                        onExpand={anchorExpandedAreaTop}
                        {...(saveMessageMarkdown && listMessageMarkdownPaths
                          ? { onSaveMarkdown: setMarkdownToSave }
                          : {})}
                        showAssistantCopy={copyableAssistantMessageIds.has(item.turn.final.id)}
                        turn={item.turn}
                        verboseMode={verboseMode}
                      />
                    )}
                  </MessageScrollerItem>
                ))}
            {showActivity && terminalActivity ? <SessionChatActivityRow activity={terminalActivity} /> : null}
            {showTypingIndicator ? <WorkingIndicator /> : null}
          </MessageScrollerContent>
        </MessageScrollerViewport>
        <MessageScrollerButton className='ghostex-chat-scroll-bottom-button' />
      </MessageScroller>
      {saveMessageMarkdown && listMessageMarkdownPaths ? (
        <SessionChatSaveMarkdownDialog
          listExistingPaths={listMessageMarkdownPaths}
          markdown={markdownToSave ?? ''}
          onOpenChange={(open) => {
            if (!open) {
              setMarkdownToSave(null);
            }
          }}
          open={markdownToSave !== null}
          save={saveMessageMarkdown}
          sessionTitle={sessionTitle}
          theme={theme}
        />
      ) : null}
    </MessageScrollerProvider>
  );
}
