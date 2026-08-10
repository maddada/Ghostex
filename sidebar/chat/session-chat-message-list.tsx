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

import { IconChevronRight, IconCopy, IconPhoto } from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SessionChatMessage } from "../../shared/session-chat";
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
} from "../../components/ui/message-scroller";
import { orderSessionChatMessages } from "./session-chat-assembler";
import {
  centerSessionChatExpansion,
  SessionChatExpansion,
} from "./session-chat-expansion";
import { SessionChatMarkdown } from "./session-chat-markdown";
import { isSessionChatPendingMessageId } from "./session-chat-pending";
import {
  dropSessionChatHiddenMessages,
  sessionChatSuppressedTurnLabel,
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
  hasMore: boolean;
  loadingEarlier: boolean;
  onLoadEarlier: () => void;
  /** Global tool-run expansion signal; runs start collapsed by default. */
  expandToolRuns?: boolean;
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

function ReasoningRow({ markdown }: { markdown: string }) {
  const text = markdown
    .replace(/```(?:[^\n]*)\n?([\s\S]*?)```/g, "$1")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/^\s{0,3}(?:#{1,6}|>|[-+*]|\d+[.)])\s+/gm, "")
    .replace(/(?:\*\*|__|\*|_|~~)/g, "")
    .replace(/\\([\\`*_[\]{}()#+\-.!>])/g, "$1")
    .trim();

  const lines = text
    .split(/\n+/)
    .map((line) => line.trim())
    .filter(Boolean);

  return (
    <div className="ghostex-chat-thinking-row">
      {lines.map((line, index) => (
        <div className="ghostex-chat-thinking-line" key={index}>
          {line}
        </div>
      ))}
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

function MessageRow({
  expandToolRuns,
  message,
}: {
  expandToolRuns: boolean;
  message: SessionChatMessage;
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

  const suppressedLabel = sessionChatSuppressedTurnLabel(message);
  if (suppressedLabel !== null) {
    return <SuppressedTurn label={suppressedLabel} text={markdown} />;
  }

  const isUser = message.role === "user";
  const isReasoning = message.role === "reasoning";
  const isSystem = message.role === "system";
  const showControls = !isReasoning && !isSystem && markdown.length > 0;

  if (isSystem) {
    return (
      <Marker className="pb-2">
        <MarkerContent>{markdown}</MarkerContent>
      </Marker>
    );
  }

  if (
    isReasoning &&
    markdown.length > 0 &&
    images.length === 0 &&
    tools.length === 0
  ) {
    return <ReasoningRow markdown={markdown} />;
  }

  if (isUser) {
    const userMarkdown = normalizeUserMessageMarkdown(markdown);
    // Optimistic echoes render IDENTICALLY to real turns — no muting, no
    // "Queued" label — so replacement by the transcript turn causes no
    // visible state change.
    return (
      <Message align="end" className="pb-4" data-role="user">
        <MessageContent>
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
          {showControls ? <CopyFooter markdown={userMarkdown} /> : null}
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
            <SessionChatMarkdown markdown={markdown} />
          </div>
        ) : null}
        {tools.length > 0 ? (
          <SessionChatToolRun blocks={tools} expandSignal={expandToolRuns} />
        ) : null}
        {showControls ? <CopyFooter markdown={markdown} /> : null}
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
  expandSignal,
  turn,
}: {
  expandSignal: boolean;
  turn: CompletedWorkTurn;
}) {
  const [open, setOpen] = useState(expandSignal);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => setOpen(expandSignal), [expandSignal]);
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
                expandToolRuns={expandSignal}
                key={message.id}
                message={message}
              />
            ))}
          </SessionChatExpansion>
        ) : null}
      </div>
      <MessageRow expandToolRuns={expandSignal} message={turn.final} />
    </div>
  );
}

export function SessionChatMessageList({
  expandToolRuns = false,
  hasMore,
  isWorking,
  loadingEarlier,
  messages,
  onLoadEarlier,
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

  const showTypingIndicator =
    isWorking && !messages.some((message) => message.id === SESSION_CHAT_STREAMING_ID);
  const renderItems = useMemo(
    () => completedWorkRenderItems(rendered, isWorking),
    [isWorking, rendered],
  );

  return (
    <MessageScrollerProvider
      autoScroll
      defaultScrollPosition="end"
      scrollPreviousItemPeek={64}
    >
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
            {renderItems.map((item) => (
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
                // Anchor the optimistic row exactly once when a local send is
                // appended. The authoritative transcript replaces that row
                // with a new id shortly afterwards; anchoring the replacement
                // makes message-scroller treat reconciliation as another new
                // turn and jump the viewport back to that message.
                scrollAnchor={
                  item.kind === "message" &&
                  isSessionChatPendingMessageId(item.message.id)
                }
              >
                {item.kind === "message" ? (
                  <MessageRow
                    expandToolRuns={expandToolRuns}
                    message={item.message}
                  />
                ) : (
                  <CompletedWork
                    expandSignal={expandToolRuns}
                    turn={item.turn}
                  />
                )}
              </MessageScrollerItem>
            ))}
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
