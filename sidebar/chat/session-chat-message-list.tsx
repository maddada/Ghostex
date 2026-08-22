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
  IconInfoCircle,
  IconPhoto,
} from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  SessionChatMessage,
  SessionChatTerminalActivity,
} from "../../shared/session-chat";
import { cn } from "../../lib/utils";
import { Button } from "../../components/ui/button";
import { Separator } from "../../components/ui/separator";
import {
  Attachment,
  AttachmentContent,
  AttachmentMedia,
  AttachmentTitle,
  AttachmentTrigger,
} from "../../components/ui/attachment";
import {
  SessionChatInlineImage,
  useSessionChatImageViewer,
} from "./session-chat-image-viewer";
import { normalizeSessionChatImageTranscriptMessages } from "./session-chat-image-transcript-markers";
import { Bubble, BubbleContent } from "../../components/ui/bubble";
import { Marker, MarkerContent } from "../../components/ui/marker";
import {
  Message,
  MessageContent,
  MessageFooter,
} from "../../components/ui/message";
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
  useMessageScroller,
} from "../../components/ui/message-scroller";
import { SessionChatActivityRow } from "./session-chat-activity-row";
import { orderSessionChatMessages } from "./session-chat-assembler";
import {
  centerSessionChatExpansion,
  SessionChatExpansion,
} from "./session-chat-expansion";
import { SessionChatMarkdown } from "./session-chat-markdown";
import { SessionChatScrollCap } from "./session-chat-scroll-cap";
import { isSessionChatPendingMessageId } from "./session-chat-pending";
import {
  dropSessionChatHiddenMessages,
  sessionChatSuppressedTurnLabel,
  sessionChatSuppressedTurnPresentation,
  type SessionChatStatusRow,
  type SessionChatStatusTone,
} from "./session-chat-noise";
import { SESSION_CHAT_STREAMING_ID } from "./session-chat-streaming";
import {
  foldSessionChatToolMessages,
  splitSessionChatBlocks,
} from "./session-chat-tool-fold";
import { SessionChatToolRun } from "./session-chat-tool-run";

const LOAD_EARLIER_SCROLL_TOP_PX = 80;
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
  /** Reveal reasoning-owned tool activity by default. */
  verboseMode?: boolean;
}

function isPastedImagePath(path: string | undefined): boolean {
  if (!path) {
    return false;
  }
  const segment = path.split(/[\\/]/).at(-1) ?? "";
  return PASTED_IMAGE_NAME.test(segment);
}

function imageChipLabel(block: {
  alt?: string;
  path?: string;
  url?: string;
}): string {
  if (isPastedImagePath(block.path)) {
    return "Pasted image";
  }
  if (block.path) {
    return block.path.split(/[\\/]/).at(-1) ?? block.path;
  }
  return block.alt ?? block.url ?? "Image";
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
    <div className={cn("flex min-w-0 flex-wrap gap-2 py-1", className)}>
      {blocks.map((block, index) => {
        const target = {
          ...(block.path !== undefined ? { path: block.path } : {}),
          ...(block.url !== undefined ? { url: block.url } : {}),
          ...(block.alt !== undefined ? { alt: block.alt } : {}),
        };
        const label = imageChipLabel(block);
        const chip = (
          <Attachment size="xs">
            <AttachmentMedia>
              <IconPhoto aria-hidden="true" stroke={1.8} />
            </AttachmentMedia>
            <AttachmentContent>
              <AttachmentTitle>{label}</AttachmentTitle>
            </AttachmentContent>
            {viewer?.canOpen(target) === true ? (
              <AttachmentTrigger
                aria-label={`View ${label}`}
                className="cursor-zoom-in"
                onClick={() => viewer?.open(target)}
              />
            ) : null}
          </Attachment>
        );
        return (
          <SessionChatInlineImage
            fallback={chip}
            key={index}
            target={{ ...target, alt: target.alt ?? label }}
          />
        );
      })}
    </div>
  );
}

function CopyFooter({ markdown }: { markdown: string }) {
  return (
    <MessageFooter className="px-0 opacity-0 transition-opacity group-hover/message:opacity-100 group-focus-within/message:opacity-100">
      <Button
        aria-label="Copy message"
        onClick={() => {
          void navigator.clipboard.writeText(markdown);
        }}
        size="icon-xs"
        variant="ghost"
      >
        <IconCopy aria-hidden="true" stroke={1.9} />
      </Button>
    </MessageFooter>
  );
}

/** Marks a prompt the agent has accepted but not started on yet. */
function QueuedLabel() {
  return (
    <div className="ghostex-chat-queued-label self-end" data-queued="true">
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
    <div className="flex w-full min-w-0 flex-col gap-1.5 pb-2">
      <button
        aria-expanded={expanded}
        className="flex min-w-0 items-center gap-1 self-start text-xs text-muted-foreground transition-colors hover:text-foreground"
        // Opts out of the sidebar's legacy `button:where(:not([data-slot]))`
        // base, which otherwise paints a 1px app border around the marker.
        data-slot="session-chat-suppressed-trigger"
        onClick={() => {
          if (!expanded) {
            centerSessionChatExpansion(triggerRef.current);
          }
          setExpanded((value) => !value);
        }}
        ref={triggerRef}
        type="button"
      >
        <IconChevronRight
          aria-hidden="true"
          className={cn("size-3 shrink-0 transition-transform", expanded && "rotate-90")}
          stroke={2}
        />
        <span className="truncate">{label}</span>
      </button>
      {expanded ? (
        <SessionChatExpansion
          label={`Collapse ${label}`}
          onCollapse={() => setExpanded(false)}
        >
          <div className="min-w-0 whitespace-pre-wrap break-words rounded-md border border-border/60 bg-muted/30 px-2.5 py-2 font-mono text-[11px] leading-relaxed text-muted-foreground">
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
    <div className="ghostex-chat-suppressed-inline">
      <div>
        <span className="ghostex-chat-suppressed-inline-label">{label}</span>
        {text}
      </div>
    </div>
  );
}

const STATUS_TONE_ICON: Record<
  SessionChatStatusTone,
  { Icon: typeof IconCheck; className: string }
> = {
  ok: { Icon: IconCheck, className: "bg-emerald-500/15 text-emerald-400" },
  error: {
    Icon: IconAlertTriangle,
    className: "bg-destructive/15 text-destructive",
  },
  neutral: { Icon: IconInfoCircle, className: "bg-muted text-muted-foreground" },
};

/**
 * The one durable row for a completed action — a model/effort change, a
 * compaction, a background task reporting back. Non-expandable on purpose:
 * the label already says everything the row is for.
 */
function StatusRow({
  label,
  tone = "ok",
}: {
  label: string;
  tone?: SessionChatStatusTone;
}) {
  const { Icon, className } = STATUS_TONE_ICON[tone];
  return (
    <div className="inline-flex max-w-full min-w-0 items-center gap-2 rounded-full border border-border/60 bg-muted/35 px-3 py-1.5 text-xs font-medium text-muted-foreground">
      <span
        className={cn(
          "flex size-4 shrink-0 items-center justify-center rounded-full",
          className,
        )}
      >
        <Icon aria-hidden="true" className="size-3" stroke={2.4} />
      </span>
      <span className="min-w-0 [overflow-wrap:anywhere]">{label}</span>
    </div>
  );
}

/** One row per status; a turn carrying several reports each of them. */
function StatusRows({ statuses }: { statuses: readonly SessionChatStatusRow[] }) {
  return (
    <div className="flex w-full min-w-0 flex-col items-start gap-1.5 pb-3">
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
    .replace(/```(?:[^\n]*)\n?([\s\S]*?)```/g, "$1")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/^\s{0,3}(?:#{1,6}|>|[-+*]|\d+[.)])\s+/gm, "")
    .replace(/(?:\*\*|__|\*|_|~~)/g, "")
    .replace(/\\([\\`*_[\]{}()#+\-.!>])/g, "$1")
    .trim();
  return (
    text
      .split(/\n+/)
      .map((line) => line.trim())
      .find(Boolean) ?? ""
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
    (lines[first + 1] ?? "").trim().length === 0 &&
    !NON_HOISTABLE_REASONING_LINE.test(lines[first] ?? "");
  const headline = hoistable ? plainReasoningTeaser(lines[first] ?? "") : "";
  if (headline.length === 0) {
    return { headline: plainReasoningTeaser(markdown), body: markdown };
  }
  return {
    headline,
    body: lines
      .slice(first + 1)
      .join("\n")
      .trim(),
  };
}

function ReasoningRow({
  isStreaming,
  markdown,
  tools,
  verboseMode,
}: {
  isStreaming: boolean;
  markdown: string;
  tools: ReturnType<typeof splitSessionChatBlocks>["tools"];
  verboseMode: boolean;
}) {
  const [open, setOpen] = useState(verboseMode);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(verboseMode), [verboseMode]);

  const renderBody = (value: string) => (
    <SessionChatScrollCap className="ghostex-chat-thinking-body">
      <SessionChatMarkdown isStreaming={isStreaming} markdown={value} />
    </SessionChatScrollCap>
  );

  // With tools, the caret owns BOTH the reasoning body and the tool rows: a
  // long reasoning turn collapses to its first line instead of pushing the
  // answer it belongs to off the screen. Verbose mode still opens it by
  // default, so nothing is hidden from anyone who wants it.
  if (tools.length > 0) {
    const { headline, body: detail } = splitReasoningHeadline(markdown);
    return (
      <div className="ghostex-chat-thinking-row is-disclosure">
        <button
          aria-expanded={open}
          className="ghostex-chat-thinking-trigger"
          onClick={() => {
            if (!open) {
              centerSessionChatExpansion(triggerRef.current);
            }
            setOpen((value) => !value);
          }}
          ref={triggerRef}
          type="button"
        >
          <span className="ghostex-chat-thinking-icon">
            <span
              aria-hidden="true"
              className={cn(
                "ghostex-chat-thinking-caret",
                open && "is-open",
              )}
            />
          </span>
          <span className="ghostex-chat-thinking-text">
            {/* The reasoning's first line, open or collapsed: expanding a turn
                reveals what follows it, it does not relabel it. */}
            <span data-ghostex-thinking-text>{headline}</span>
          </span>
        </button>
        {open ? (
          <SessionChatExpansion
            className="ghostex-chat-thinking-detail"
            label="Collapse thinking"
            onCollapse={() => setOpen(false)}
          >
            {detail.length > 0 ? renderBody(detail) : null}
            <SessionChatToolRun blocks={tools} showAllRows />
          </SessionChatExpansion>
        ) : null}
      </div>
    );
  }

  return (
    <div className="ghostex-chat-thinking-row">
      <div className="ghostex-chat-thinking-line">
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
    const containingIndex = visible.findIndex((candidate) =>
      candidate.startsWith(part),
    );
    if (containingIndex < 0) {
      visible.push(part);
      continue;
    }

    const remainder =
      visible[containingIndex]?.slice(part.length).trimStart() ?? "";
    visible[containingIndex] = remainder ? `${part}\n\n${remainder}` : part;
  }
  return visible.join("\n\n");
}

/*
 * A pasted picture reaches the agent as a "[Image #N](path)" reference inside
 * the turn's own text; the chat lifts those references out into image blocks so
 * the picture renders as a picture. Copy has to hand the whole turn back — the
 * references included — or the reader loses the one thing that names the file
 * they attached, and pasting the copy into another composer attaches nothing.
 */
function userTurnCopyMarkdown(
  markdown: string,
  images: readonly { path?: string; url?: string }[],
): string {
  const references = images
    .map((block, index) => {
      const href = block.path ?? block.url;
      return href === undefined ? "" : `[Image #${index + 1}](${href})`;
    })
    .filter((reference) => reference !== "");
  return [references.join(" "), markdown]
    .filter((part) => part !== "")
    .join("\n\n");
}

function MessageRow({
  isStreaming = false,
  message,
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
  showAssistantCopy: boolean;
  verboseMode: boolean;
}) {
  const { prose, tools } = splitSessionChatBlocks(message.blocks);
  const markdown = prose
    .filter((block) => block.type === "text")
    .map((block) => (block.type === "text" ? block.text : ""))
    .join("\n\n");
  const images = prose.filter((block) => block.type === "image-ref");

  // No ghost bubbles: skip entirely when there is nothing to show.
  if (markdown.length === 0 && images.length === 0 && tools.length === 0) {
    return null;
  }

  const suppressedTurn = sessionChatSuppressedTurnPresentation(message);
  if (suppressedTurn !== null) {
    if (suppressedTurn.kind === "status") {
      return (
        <StatusRows
          statuses={
            suppressedTurn.statuses ?? [
              { label: suppressedTurn.label, tone: suppressedTurn.tone ?? "ok" },
            ]
          }
        />
      );
    }
    if (suppressedTurn.kind === "inline") {
      return (
        <InlineSuppressedTurn
          label={suppressedTurn.label}
          text={suppressedTurn.text}
        />
      );
    }
    return (
      <SuppressedTurn
        label={suppressedTurn.label}
        text={suppressedTurn.text}
      />
    );
  }

  const isUser = message.role === "user";
  const isReasoning = message.role === "reasoning";
  const isSystem = message.role === "system";
  const userMarkdown = isUser ? normalizeUserMessageMarkdown(markdown) : "";
  const userCopyMarkdown = isUser
    ? userTurnCopyMarkdown(userMarkdown, images)
    : "";
  const showCopy = isUser
    ? userCopyMarkdown.length > 0
    : markdown.length > 0 &&
      message.role === "assistant" &&
      showAssistantCopy;

  if (isSystem) {
    return (
      <Marker className="pb-2">
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
        tools={tools}
        verboseMode={verboseMode}
      />
    );
  }

  if (isUser) {
    /*
     * The "Queued" label is driven ONLY by the agent's own queue bookkeeping
     * in the transcript (`message.queued`), never by an optimistic echo:
     * an echo says "we typed it", not "the agent is holding it", and echoes
     * render IDENTICALLY to real turns so replacement by the transcript turn
     * causes no visible state change. The server retracts the queued row the
     * moment the queue releases it, so the label cannot outlive the wait.
     */
    return (
      <Message align="end" className="pb-4" data-role="user">
        <MessageContent>
          {message.queued === true ? <QueuedLabel /> : null}
          {/* justify-end keeps wrapped rows against the user's side. */}
          <ImageAttachments blocks={images} className="self-end justify-end" />
          {userMarkdown.length > 0 ? (
            <Bubble
              align="end"
              className="ghostex-chat-user-bubble"
              variant="default"
            >
              <BubbleContent>
                <SessionChatMarkdown markdown={userMarkdown} />
              </BubbleContent>
            </Bubble>
          ) : null}
          {showCopy ? <CopyFooter markdown={userCopyMarkdown} /> : null}
        </MessageContent>
      </Message>
    );
  }

  return (
    <Message align="start" className="pb-4" data-role={message.role}>
      <MessageContent>
        <ImageAttachments blocks={images} />
        {markdown.length > 0 ? (
          <div className="ghostex-chat-agent-message">
            <SessionChatMarkdown isStreaming={isStreaming} markdown={markdown} />
          </div>
        ) : null}
        {tools.length > 0 ? (
          <SessionChatToolRun blocks={tools} />
        ) : null}
        {showCopy ? <CopyFooter markdown={markdown} /> : null}
      </MessageContent>
    </Message>
  );
}

interface CompletedWorkTurn {
  final: SessionChatMessage;
  user: SessionChatMessage;
  work: SessionChatMessage[];
}

type SessionChatRenderItem =
  | { kind: "message"; message: SessionChatMessage }
  | { kind: "completed-work"; turn: CompletedWorkTurn };

function hasAgentResponseContent(message: SessionChatMessage): boolean {
  return (
    message.role === "assistant" &&
    message.id !== SESSION_CHAT_STREAMING_ID &&
    message.blocks.some(
      (block) =>
        block.type === "image-ref" ||
        (block.type === "text" && block.text.trim().length > 0),
    )
  );
}

/** One copy affordance per response: the last assistant text before the next user turn. */
function finalAssistantMessageIds(
  messages: readonly SessionChatMessage[],
  isWorking: boolean,
): ReadonlySet<string> {
  const ids = new Set<string>();
  let finalAssistantId: string | null = null;

  const commitTurn = (): void => {
    if (finalAssistantId !== null) {
      ids.add(finalAssistantId);
      finalAssistantId = null;
    }
  };

  for (const message of messages) {
    if (message.role === "user") {
      commitTurn();
      continue;
    }
    if (
      message.role === "assistant" &&
      message.blocks.some(
        (block) => block.type === "text" && block.text.trim().length > 0,
      )
    ) {
      finalAssistantId = message.id;
    }
  }
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
 * between becomes one collapsed work section. The newest interaction stays
 * expanded while the agent is still working.
 */
function completedWorkRenderItems(
  messages: readonly SessionChatMessage[],
  isWorking: boolean,
): SessionChatRenderItem[] {
  const items: SessionChatRenderItem[] = [];
  let index = 0;
  while (index < messages.length) {
    const message = messages[index];
    if (!message || message.role !== "user") {
      if (message) {
        items.push({ kind: "message", message });
      }
      index += 1;
      continue;
    }

    let nextUserIndex = index + 1;
    while (
      nextUserIndex < messages.length &&
      messages[nextUserIndex]?.role !== "user"
    ) {
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
    const isNewestTurn = nextUserIndex === messages.length;
    if (finalIndex < 0 || (isNewestTurn && isWorking)) {
      items.push({ kind: "message", message });
      for (const turnMessage of turnMessages) {
        items.push({ kind: "message", message: turnMessage });
      }
      index = nextUserIndex;
      continue;
    }

    const final = turnMessages[finalIndex];
    if (!final) {
      items.push({ kind: "message", message });
      index += 1;
      continue;
    }
    items.push({ kind: "message", message });
    items.push({
      kind: "completed-work",
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

function workedDurationLabel(
  startedAt: number | null,
  completedAt: number | null,
): string {
  if (startedAt === null || completedAt === null || completedAt < startedAt) {
    return "Worked";
  }
  const seconds = Math.max(1, Math.round((completedAt - startedAt) / 1000));
  if (seconds < 60) {
    return `Worked for ${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return `Worked for ${minutes}m${remainder > 0 ? ` ${remainder}s` : ""}`;
}

function CompletedWork({
  turn,
  verboseMode,
}: {
  turn: CompletedWorkTurn;
  verboseMode: boolean;
}) {
  const [open, setOpen] = useState(verboseMode);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(verboseMode), [verboseMode]);
  const hasWork = turn.work.length > 0;

  return (
    <div className="ghostex-chat-completed-turn">
      <div className="ghostex-chat-completed-work">
        <Button
          aria-expanded={hasWork ? open : undefined}
          className="ghostex-chat-completed-work-trigger"
          disabled={!hasWork}
          onClick={() => {
            if (hasWork) {
              if (!open) {
                centerSessionChatExpansion(triggerRef.current);
              }
              setOpen((value) => !value);
            }
          }}
          ref={triggerRef}
          size="xs"
          type="button"
          variant="ghost"
        >
          <span>{workedDurationLabel(turn.user.timestamp, turn.final.timestamp)}</span>
          {hasWork ? (
            <IconChevronRight
              aria-hidden="true"
              className={cn(open && "rotate-90")}
              data-icon="inline-end"
              stroke={2}
            />
          ) : null}
        </Button>
        <Separator />
        {hasWork && open ? (
          <SessionChatExpansion
            bodyClassName="ghostex-chat-completed-work-content"
            label="Collapse completed work"
            onCollapse={() => setOpen(false)}
          >
            {turn.work.map((message) => (
              <MessageRow
                key={message.id}
                message={message}
                showAssistantCopy={false}
                verboseMode={verboseMode}
              />
            ))}
          </SessionChatExpansion>
        ) : null}
      </div>
      <MessageRow
        message={turn.final}
        showAssistantCopy
        verboseMode={verboseMode}
      />
    </div>
  );
}

/**
 * A local send must bring the newest row back into view even when the reader
 * had scrolled up, without asking message-scroller to anchor that row to the
 * top of the viewport (top anchoring pads the transcript with a spacer and
 * leaves a scrollable empty gap above the composer).
 */
function ScrollToLatestSend({
  pendingMessageId,
}: {
  pendingMessageId: string | null;
}): null {
  const { scrollToEnd } = useMessageScroller();
  const handledRef = useRef<string | null>(null);

  useEffect(() => {
    if (pendingMessageId === null || handledRef.current === pendingMessageId) {
      return;
    }
    handledRef.current = pendingMessageId;
    scrollToEnd({ behavior: "auto" });
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
  verboseMode = false,
}: SessionChatMessageListProps) {
  const loadingEarlierRef = useRef(loadingEarlier);
  loadingEarlierRef.current = loadingEarlier;
  const hasMoreRef = useRef(hasMore);
  hasMoreRef.current = hasMore;
  const scrollbarFadeTimeoutRef = useRef<number | undefined>(undefined);

  useEffect(
    () => () => {
      if (scrollbarFadeTimeoutRef.current !== undefined) {
        window.clearTimeout(scrollbarFadeTimeoutRef.current);
      }
    },
    [],
  );

  // Auto-load older history when the reader scrolls near the top; the
  // viewport's preserveScrollOnPrepend keeps the visible rows in place when
  // the earlier page lands. Every scroll also stamps the viewport so the
  // scrollbar shows while scrolling and fades out afterwards (chat.css).
  const handleScroll = useCallback(
    (event: React.UIEvent<HTMLDivElement>): void => {
      const viewport = event.currentTarget;
      viewport.setAttribute("data-user-scrolling", "true");
      if (scrollbarFadeTimeoutRef.current !== undefined) {
        window.clearTimeout(scrollbarFadeTimeoutRef.current);
      }
      scrollbarFadeTimeoutRef.current = window.setTimeout(() => {
        viewport.removeAttribute("data-user-scrolling");
      }, SCROLLBAR_FADE_MS);
      if (
        viewport.scrollTop < LOAD_EARLIER_SCROLL_TOP_PX &&
        hasMoreRef.current &&
        !loadingEarlierRef.current
      ) {
        onLoadEarlier();
      }
    },
    [onLoadEarlier],
  );

  const rendered = useMemo(
    () =>
      foldSessionChatToolMessages(
        dropSessionChatHiddenMessages(
          normalizeSessionChatImageTranscriptMessages(
            orderSessionChatMessages(messages),
          ),
        ),
        // Collapsed markers must not break a tool-fold run.
        (message) => sessionChatSuppressedTurnLabel(message) !== null,
      ),
    [messages],
  );

  const showActivity = terminalActivity != null;
  const showTypingIndicator =
    !showActivity &&
    isWorking &&
    !messages.some((message) => message.id === SESSION_CHAT_STREAMING_ID);
  const renderItems = useMemo(
    () => completedWorkRenderItems(rendered, isWorking),
    [isWorking, rendered],
  );
  const copyableAssistantMessageIds = useMemo(
    () => finalAssistantMessageIds(rendered, isWorking),
    [isWorking, rendered],
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
    <MessageScrollerProvider autoScroll defaultScrollPosition="end">
      <ScrollToLatestSend pendingMessageId={pendingMessageId} />
      <MessageScroller className="flex-1">
        {/* RTL viewport + LTR content puts the scrollbar on the left edge. */}
        <MessageScrollerViewport
          className="[direction:rtl]"
          onScroll={handleScroll}
          preserveScrollOnPrepend
        >
          {hasMore ? (
            <div className="flex justify-center px-4 pt-2 [direction:ltr]">
              <Button
                disabled={loadingEarlier}
                onClick={onLoadEarlier}
                size="sm"
                variant="ghost"
              >
                {loadingEarlier ? "Loading…" : "Load earlier messages"}
              </Button>
            </div>
          ) : null}
          <MessageScrollerContent className="mx-auto w-full max-w-3xl gap-0 px-4 pt-8 pb-4 [direction:ltr]">
            {renderItems.map((item, index) => (
              <MessageScrollerItem
                key={
                  item.kind === "message"
                    ? item.message.id
                    : `completed-work:${item.turn.user.id}:${item.turn.final.id}`
                }
                messageId={
                  item.kind === "message"
                    ? item.message.id
                    : item.turn.final.id
                }
                // No row is a scroll anchor: anchoring a message to the top of
                // the viewport makes message-scroller pad the transcript with a
                // spacer so that message can reach the top, which leaves a
                // viewport-sized scrollable gap between the newest row and the
                // composer until the reply grows tall enough to fill it.
                // Following the bottom keeps the newest row above the composer.
              >
                {item.kind === "message" ? (
                  <MessageRow
                    /*
                     * Only the newest row can still be growing, and only while
                     * the agent is working: transcript tailing appends to the
                     * last message, and the synthetic streaming preview row is
                     * always last when it exists. Earlier rows are settled, so
                     * their code fences are safe to highlight and cache.
                     * `completedWorkRenderItems` never folds the newest turn
                     * while working, so a "completed-work" item is settled by
                     * construction and keeps the default `isStreaming={false}`.
                     */
                    isStreaming={isWorking && index === renderItems.length - 1}
                    message={item.message}
                    showAssistantCopy={copyableAssistantMessageIds.has(
                      item.message.id,
                    )}
                    verboseMode={verboseMode}
                  />
                ) : (
                  <CompletedWork
                    turn={item.turn}
                    verboseMode={verboseMode}
                  />
                )}
              </MessageScrollerItem>
            ))}
            {showActivity && terminalActivity ? (
              <SessionChatActivityRow activity={terminalActivity} />
            ) : null}
            {showTypingIndicator ? (
              <div
                aria-label="Agent is responding"
                aria-live="polite"
                className="flex h-8 items-center gap-1.5 text-muted-foreground"
                role="status"
              >
                {[0, 1, 2].map((index) => (
                  <span
                    className="size-1.5 animate-bounce rounded-full bg-muted-foreground/70"
                    key={index}
                    style={{ animationDelay: `${index * 160}ms` }}
                  />
                ))}
              </div>
            ) : null}
          </MessageScrollerContent>
        </MessageScrollerViewport>
        <MessageScrollerButton className="ghostex-chat-scroll-bottom-button" />
      </MessageScroller>
    </MessageScrollerProvider>
  );
}
