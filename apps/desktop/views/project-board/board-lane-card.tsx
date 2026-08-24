import {
  IconCircleMinus,
  IconExclamationCircle,
  IconExternalLink,
  IconLink,
  IconMessageCircle,
  IconPlayerPlay,
  IconPlus,
  IconRuler2,
  IconTrash,
} from "@tabler/icons-react";
import {
  useDraggable,
  useDroppable,
} from "@dnd-kit/react";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { Button } from "@/packages/components/ui/button";
import { Card } from "@/packages/components/ui/card";
import {
  TOOLTIP_DELAY_MS,
  TooltipProvider,
} from "@/packages/components/ui/tooltip";
import {
  conversationLinkActionKind,
  conversationLinkLabel,
  getBlockedByIds,
  priorityLabel,
  ticketCreatorName,
  estimateToTshirt,
  type BoardColumn,
  type BoardStatusKey,
  type BoardTicket,
} from "../project-board-shared";
import {
  selectBeadConversationLinks,
  type ProjectBoardConversationLinkView,
} from "@/packages/shared/bead-conversation-links";
import {
  type ConversationActionState,
  type TicketContextMenuState,
} from "./types";
import { PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN } from "./constants";
import {
  BOARD_CARD_VIEW_DEFAULTS,
  type BoardCardViewOptions,
} from "./card-view-options";
import {
  getPrimaryUsableConversationLink,
  ConversationLinkName,
} from "./ticket-detail";

export const PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX = 12;

/*
 * CDXC:ProjectBoardRedesign 2026-08-23:
 * Kanban shares the Codex-style Automate language: flat rounded panels, quiet
 * regular-weight text on one scale, default shadcn tokens, all styling in
 * Tailwind instead of the bespoke `.project-board-*` CSS. Lane tone dots keep
 * their original colors.
 */
const LANE_TONE_COLORS: Record<string, string> = {
  muted: "#8f9aa7",
  blue: "#5ea4ff",
  amber: "#e7b85b",
  violet: "#b18cff",
  green: "#95d7f6",
};

function laneToneColor(tone: string): string {
  return LANE_TONE_COLORS[tone] ?? "rgba(244, 244, 245, 0.42)";
}

/*
 * CDXC:ProjectBoardRedesign 2026-08-24:
 * Linear-style card accents: labels and assignee avatars get a stable color
 * derived from their text so the same tag looks the same on every card.
 */
const CHIP_TONE_COLORS = [
  "#5ea4ff",
  "#e7b85b",
  "#b18cff",
  "#6fd19c",
  "#f28b8b",
  "#95d7f6",
  "#e79ad0",
];

function chipToneColor(seed: string): string {
  let hash = 0;
  for (const char of seed) {
    hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  }
  return CHIP_TONE_COLORS[hash % CHIP_TONE_COLORS.length];
}

const CARD_CHIP_CLASS =
  "inline-flex max-w-full min-w-0 items-center gap-1.5 rounded-full border border-border/80 bg-white/[0.02] px-2 py-[3px] text-[11px] font-normal leading-4 text-muted-foreground [&_svg]:size-3 [&_svg]:shrink-0";

function TicketPriorityIcon({ priority }: { priority: number | undefined }) {
  const value = priority ?? 2;
  if (value <= 0) {
    return (
      <svg aria-hidden="true" className="size-4 shrink-0 text-orange-400/90" viewBox="0 0 16 16">
        <rect fill="currentColor" height="13" rx="3.5" width="13" x="1.5" y="1.5" />
        <path d="M8 4.5v4" stroke="#0e0e0e" strokeLinecap="round" strokeWidth="1.6" />
        <circle cx="8" cy="11.2" fill="#0e0e0e" r="1" />
      </svg>
    );
  }
  const filledBars = value === 1 ? 3 : value === 2 ? 2 : 1;
  const tone =
    value === 1
      ? "text-amber-400/90"
      : value === 2
        ? "text-sky-400/80"
        : "text-muted-foreground/70";
  return (
    <svg aria-hidden="true" className={`size-4 shrink-0 ${tone}`} viewBox="0 0 16 16">
      {[0, 1, 2].map((bar) => (
        <rect
          fill="currentColor"
          height={4 + bar * 3.5}
          key={bar}
          opacity={bar < filledBars ? 1 : 0.25}
          rx="1"
          width="3"
          x={2 + bar * 4.5}
          y={10 - bar * 3.5}
        />
      ))}
    </svg>
  );
}

export function BoardLane({
  cardView = BOARD_CARD_VIEW_DEFAULTS,
  column,
  conversationAction,
  linksByBeadKey,
  onAddTicket,
  onJumpToConversation,
  onOpenContextMenu,
  onOpenTicket,
  tickets,
}: {
  cardView?: BoardCardViewOptions;
  column: BoardColumn;
  conversationAction: ConversationActionState;
  linksByBeadKey: Map<string, ProjectBoardConversationLinkView[]>;
  onAddTicket: (status: BoardStatusKey) => void;
  onJumpToConversation: (link: ProjectBoardConversationLinkView) => void;
  onOpenContextMenu: (ticket: BoardTicket, point: { x: number; y: number }) => void;
  onOpenTicket: (ticket: BoardTicket) => void;
  tickets: BoardTicket[];
}) {
  const { isDropTarget, ref } = useDroppable({
    accept: "ticket",
    data: { statusKey: column.key },
    id: column.key,
  });
  const visibleTickets = tickets.slice(0, PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN);
  const hiddenTicketCount = tickets.length - visibleTickets.length;
  return (
    /* `project-board-lane` carries only the scrollbar hover-reveal rules in styles.ts now. */
    <section
      className="project-board-lane group/lane flex min-h-0 min-w-[220px] flex-col rounded-xl border border-border/80 bg-white/[0.02] transition-colors data-[drop-target=true]:border-border data-[drop-target=true]:bg-white/[0.04]"
      data-drop-target={String(isDropTarget)}
      data-tone={column.tone}
      ref={ref}
    >
      <header className="flex h-11 shrink-0 items-center justify-between px-3">
        <div className="flex min-w-0 items-center gap-2">
          <span
            aria-hidden="true"
            className="size-1.5 shrink-0 rounded-full"
            style={{ background: laneToneColor(column.tone) }}
          />
          <h2 className="m-0 truncate text-[13px] font-normal text-foreground/90">{column.label}</h2>
        </div>
        <div className="relative flex h-7 w-7 shrink-0 items-center justify-end">
          <span className="block min-w-full text-right text-xs font-normal text-muted-foreground transition-opacity group-hover/lane:opacity-0 group-focus-within/lane:opacity-0">
            {tickets.length}
          </span>
          <Button
            aria-label={`Add ticket to ${column.label}`}
            className="pointer-events-none absolute -right-1 top-0 opacity-0 transition-opacity group-hover/lane:pointer-events-auto group-hover/lane:opacity-100 group-focus-within/lane:pointer-events-auto group-focus-within/lane:opacity-100"
            onClick={() => onAddTicket(column.key)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconPlus aria-hidden="true" />
          </Button>
        </div>
      </header>
      {/*
       * CDXC:ProjectBoardRedesign 2026-08-24:
       * Fade only the bottom edge. The scroll-linked top fade kicked in at the
       * first scrolled pixel and visibly cut off the top border of the first
       * card, so scrolled-under cards now get a clean hard edge at the lane
       * header instead.
       */}
      {/*
       * CDXC:ProjectBoardRedesign 2026-08-24:
       * pt-0.5 keeps the first card's top border off the scroller's clip
       * boundary (it rendered half-clipped at the exact edge), and pr-0.5
       * plus the reserved scrollbar gutter (styles.ts) adds up to the same
       * 10px the left side gets, so the cards sit centered in the lane.
       */}
      <div className="project-board-lane-scroll vertical-scroll-fade-mask-bottom min-h-0 flex-1 overflow-x-hidden overflow-y-auto [--edge-fade-distance:18px]">
        <div className="flex min-w-0 flex-col gap-2 pt-0.5 pl-2.5 pr-0.5 pb-2.5">
          {visibleTickets.map((ticket) => (
            <TicketCard
              cardView={cardView}
              conversationAction={conversationAction}
              key={ticket.id}
              links={selectBeadConversationLinks(linksByBeadKey, ticket.id)}
              onJumpToConversation={onJumpToConversation}
              onOpenContextMenu={onOpenContextMenu}
              onOpenTicket={onOpenTicket}
              ticket={ticket}
            />
          ))}
          {hiddenTicketCount > 0 ? (
            <div
              className="rounded-lg border border-dashed border-border px-3 py-2.5 text-xs font-normal leading-normal text-muted-foreground"
              role="status"
            >
              Showing {visibleTickets.length} of {tickets.length}. Use search or status filters to narrow this lane.
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

export function TicketCard({
  cardView = BOARD_CARD_VIEW_DEFAULTS,
  conversationAction,
  links,
  onJumpToConversation,
  onOpenContextMenu,
  onOpenTicket,
  ticket,
}: {
  cardView?: BoardCardViewOptions;
  conversationAction: ConversationActionState;
  links: ProjectBoardConversationLinkView[];
  onJumpToConversation: (link: ProjectBoardConversationLinkView) => void;
  onOpenContextMenu: (ticket: BoardTicket, point: { x: number; y: number }) => void;
  onOpenTicket: (ticket: BoardTicket) => void;
  ticket: BoardTicket;
}) {
  const { isDragging, ref } = useDraggable({
    data: { ticketId: ticket.id },
    id: ticket.id,
    type: "ticket",
  });
  const blockedByCount = ticket.dependency_count ?? getBlockedByIds(ticket).length;
  const blockingCount = ticket.dependent_count ?? 0;
  const creator = ticketCreatorName(ticket.created_by, ticket.assignee);
  const primaryLink = getPrimaryUsableConversationLink(links) ?? links[0];
  const additionalLinkCount = primaryLink ? links.length - 1 : 0;
  const primaryLinkLabel = primaryLink ? conversationLinkLabel(primaryLink) : "";
  const primaryLinkActionKind = conversationLinkActionKind(primaryLink);
  const jumpDisabled = primaryLinkActionKind === "none" || Boolean(conversationAction);
  const commentCount = ticket.comment_count ?? ticket.comments?.length ?? 0;
  const tshirt = estimateToTshirt(ticket.estimate);
  const assigneeTone = ticket.assignee ? chipToneColor(ticket.assignee) : "";
  const showTopRow = cardView.showId || (cardView.showAssignee && Boolean(ticket.assignee));
  const showChips =
    (cardView.showLabels && Boolean(ticket.labels?.length)) ||
    (cardView.showDetails &&
      (Boolean(tshirt) || blockedByCount > 0 || blockingCount > 0 || commentCount > 0));

  return (
    <Card
      className="w-full min-w-0 max-w-full cursor-default select-none gap-1.5 rounded-lg border-border/80 bg-white/[0.04] p-3 shadow-none transition-colors hover:bg-white/[0.06] data-[dragging=true]:opacity-55"
      data-dragging={String(isDragging)}
      onClick={() => onOpenTicket(ticket)}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onOpenContextMenu(ticket, { x: event.clientX, y: event.clientY });
      }}
      onKeyDown={(event) => {
        if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) {
          return;
        }
        event.preventDefault();
        const bounds = event.currentTarget.getBoundingClientRect();
        onOpenContextMenu(ticket, {
          x: bounds.left + Math.min(32, bounds.width - 12),
          y: bounds.top + Math.min(28, bounds.height - 12),
        });
      }}
      ref={ref}
      role="button"
      tabIndex={0}
    >
      {showTopRow ? (
        <div className="flex min-w-0 items-center justify-between gap-2">
          <span className="truncate text-[11px] font-normal text-muted-foreground/70">
            {cardView.showId ? ticket.displayId : ""}
          </span>
          {cardView.showAssignee && ticket.assignee ? (
            <span
              className="flex size-[18px] shrink-0 items-center justify-center rounded-full text-[10px] font-medium uppercase leading-none"
              style={{ backgroundColor: `${assigneeTone}2e`, color: assigneeTone }}
              title={`Assigned to ${ticket.assignee}`}
            >
              {ticket.assignee.slice(0, 1)}
            </span>
          ) : null}
        </div>
      ) : null}
      <div className="flex min-w-0 items-start gap-1.5">
        {cardView.showPriority ? (
          <span className="mt-0.5 flex shrink-0" title={priorityLabel(ticket.priority)}>
            <TicketPriorityIcon priority={ticket.priority} />
          </span>
        ) : null}
        <span className="min-w-0 break-words text-[13px] font-normal leading-snug text-foreground/95">
          {ticket.title}
        </span>
      </div>
      {cardView.showDescription && ticket.description ? (
        <p className="m-0 line-clamp-2 break-words text-xs font-normal leading-relaxed text-muted-foreground">
          {ticket.description}
        </p>
      ) : null}
      {showChips ? (
        <div className="mt-0.5 flex flex-wrap items-center gap-1">
          {cardView.showLabels
            ? (ticket.labels ?? []).map((label) => (
                <span className={CARD_CHIP_CLASS} key={label}>
                  <span
                    aria-hidden="true"
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: chipToneColor(label) }}
                  />
                  <span className="truncate">{label}</span>
                </span>
              ))
            : null}
          {cardView.showDetails && tshirt ? (
            <span className={CARD_CHIP_CLASS} title="Estimate">
              <IconRuler2 aria-hidden="true" className="text-muted-foreground/70" />
              {tshirt}
            </span>
          ) : null}
          {cardView.showDetails && blockedByCount > 0 ? (
            <span className={CARD_CHIP_CLASS}>
              <IconCircleMinus aria-hidden="true" className="text-red-400/80" />
              {blockedByCount} blocked
            </span>
          ) : null}
          {cardView.showDetails && blockingCount > 0 ? (
            <span className={CARD_CHIP_CLASS}>
              <IconExclamationCircle aria-hidden="true" className="text-amber-400/80" />
              {blockingCount} blocking
            </span>
          ) : null}
          {cardView.showDetails && commentCount > 0 ? (
            <span className={CARD_CHIP_CLASS}>
              <IconMessageCircle aria-hidden="true" className="text-sky-400/70" />
              {commentCount}
            </span>
          ) : null}
        </div>
      ) : null}
      {cardView.showLinks && primaryLink ? (
        <div className="flex items-center justify-between gap-2 text-xs font-normal text-muted-foreground [&_svg]:size-3.5">
          <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
            <span className="flex min-w-0 items-center gap-1.5">
              <IconLink className="text-emerald-400/80" />
              <ConversationLinkName
                className="truncate"
                label={primaryLinkLabel}
              />
              {additionalLinkCount > 0 ? (
                <span className="shrink-0 text-muted-foreground/70">
                  +{additionalLinkCount}
                </span>
              ) : null}
            </span>
          </TooltipProvider>
          <Button
            aria-label={
              primaryLinkActionKind === "resume"
                ? "Resume linked conversation"
                : "Jump to linked conversation"
            }
            disabled={jumpDisabled}
            onClick={(event) => {
              event.stopPropagation();
              onJumpToConversation(primaryLink);
            }}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            {primaryLinkActionKind === "resume" ? <IconPlayerPlay /> : <IconExternalLink />}
          </Button>
        </div>
      ) : null}
      {cardView.showDetails && creator ? (
        <div className="truncate text-[11px] font-normal text-muted-foreground/60">
          Created by {creator}
        </div>
      ) : null}
    </Card>
  );
}

export function ProjectBoardTicketContextMenu({
  confirmingDelete,
  deleting,
  onDelete,
  onDismiss,
  onPrimaryAction,
  position,
  primaryActionDisabled,
  primaryActionLabel,
}: {
  confirmingDelete: boolean;
  deleting: boolean;
  onDelete: () => void;
  onDismiss: () => void;
  onPrimaryAction: () => void;
  position: Pick<TicketContextMenuState, "x" | "y">;
  primaryActionDisabled: boolean;
  primaryActionLabel: string;
}) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuStyle, setMenuStyle] = useState<CSSProperties>(() => ({
    left: `${position.x}px`,
    top: `${position.y}px`,
  }));

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (!menu) {
      return;
    }
    const bounds = menu.getBoundingClientRect();
    const left = Math.max(
      PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      Math.min(
        position.x,
        window.innerWidth - bounds.width - PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      ),
    );
    const top = Math.max(
      PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      Math.min(
        position.y,
        window.innerHeight - bounds.height - PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX,
      ),
    );
    setMenuStyle({
      left: `${Math.round(left)}px`,
      top: `${Math.round(top)}px`,
    });
  }, [confirmingDelete, position.x, position.y, primaryActionLabel]);

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        onDismiss();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onDismiss]);

  return createPortal(
    <>
      <button
        aria-label="Dismiss ticket context menu"
        className="fixed inset-0 z-[1190] m-0 cursor-default border-0 bg-transparent p-0"
        onClick={onDismiss}
        onContextMenu={(event) => {
          event.preventDefault();
          onDismiss();
        }}
        type="button"
      />
      <div
        className="fixed z-[1200] flex min-w-44 flex-col gap-0.5 rounded-lg border border-border bg-popover p-1 shadow-xl"
        onClick={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.preventDefault()}
        ref={menuRef}
        role="menu"
        style={menuStyle}
      >
        <button
          className="flex h-8 items-center gap-2 rounded-md border-0 bg-transparent px-2.5 text-left text-[13px] font-normal text-foreground/90 outline-none hover:bg-white/[0.06] disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4 [&_svg]:text-muted-foreground"
          disabled={primaryActionDisabled}
          onClick={onPrimaryAction}
          role="menuitem"
          type="button"
        >
          <IconPlayerPlay aria-hidden="true" />
          {primaryActionLabel}
        </button>
        <button
          className="flex h-8 items-center gap-2 rounded-md border-0 bg-transparent px-2.5 text-left text-[13px] font-normal text-red-400/90 outline-none hover:bg-red-400/10 disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4 [&_svg]:text-red-400/90"
          disabled={deleting}
          onClick={onDelete}
          role="menuitem"
          type="button"
        >
          <IconTrash aria-hidden="true" />
          {confirmingDelete ? (deleting ? "Deleting" : "Confirm delete") : "Delete"}
        </button>
      </div>
    </>,
    document.body,
  );
}