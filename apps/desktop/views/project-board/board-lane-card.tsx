import {
  IconExternalLink,
  IconLink,
  IconMessageCircle,
  IconPlayerPlay,
  IconPlus,
  IconTrash,
  IconUser,
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
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/packages/components/ui/card";
import { Separator } from "@/packages/components/ui/separator";
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
  getPrimaryUsableConversationLink,
  ConversationLinkName,
} from "./ticket-detail";

export const PROJECT_BOARD_CONTEXT_MENU_VIEWPORT_MARGIN_PX = 12;

export function BoardLane({
  column,
  conversationAction,
  linksByBeadKey,
  onAddTicket,
  onJumpToConversation,
  onOpenContextMenu,
  onOpenTicket,
  tickets,
}: {
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
    <section
      className="project-board-lane"
      data-drop-target={String(isDropTarget)}
      data-tone={column.tone}
      ref={ref}
    >
      <header className="project-board-lane-header">
        <div>
          <span className="project-board-lane-dot" />
          <h2>{column.label}</h2>
        </div>
        <div className="project-board-lane-header-action">
          <span className="project-board-lane-count">{tickets.length}</span>
          <Button
            aria-label={`Add ticket to ${column.label}`}
            className="project-board-lane-add"
            onClick={() => onAddTicket(column.key)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <IconPlus aria-hidden="true" />
          </Button>
        </div>
      </header>
      <div className="project-board-lane-scroll vertical-scroll-fade-mask">
        <div className="project-board-card-stack">
          {visibleTickets.map((ticket) => (
            <TicketCard
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
            <div className="project-board-lane-limit" role="status">
              Showing {visibleTickets.length} of {tickets.length}. Use search or status filters to narrow this lane.
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

export function TicketCard({
  conversationAction,
  links,
  onJumpToConversation,
  onOpenContextMenu,
  onOpenTicket,
  ticket,
}: {
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

  return (
    <Card
      className="project-board-card"
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
      size="sm"
      tabIndex={0}
    >
      <CardHeader className="project-board-card-header">
        <CardTitle>{ticket.title}</CardTitle>
        <CardDescription>{ticket.displayId}</CardDescription>
      </CardHeader>
      <CardContent className="project-board-card-content">
        <p>{ticket.description || "No prompt yet."}</p>
        {ticket.labels?.length ? (
          <div className="project-board-card-labels">
            {ticket.labels.map((label) => (
              <span className="project-board-card-label" key={label}>
                {label}
              </span>
            ))}
          </div>
        ) : null}
        <Separator />
        <div className="project-board-card-meta">
          <span className="project-board-priority">{priorityLabel(ticket.priority)}</span>
          {estimateToTshirt(ticket.estimate) ? (
            <span>{estimateToTshirt(ticket.estimate)}</span>
          ) : null}
          {blockedByCount > 0 ? <span>{blockedByCount} blocked</span> : null}
          {blockingCount > 0 ? <span>{blockingCount} blocking</span> : null}
          {creator ? (
            <span className="project-board-card-creator" title={`Created by ${creator}`}>
              by {creator}
            </span>
          ) : null}
          {ticket.assignee ? (
            <span className="project-board-card-assignee" title={`Assigned to ${ticket.assignee}`}>
              <IconUser />
              <span className="project-board-card-assignee-name">{ticket.assignee}</span>
            </span>
          ) : null}
          <span className="project-board-comments">
            <IconMessageCircle />
            {ticket.comment_count ?? ticket.comments?.length ?? 0}
          </span>
        </div>
        {primaryLink ? (
          <div className="project-board-card-conversation">
            <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
              <span className="project-board-card-conversation-label">
                <IconLink />
                <ConversationLinkName
                  className="project-board-card-conversation-name"
                  label={primaryLinkLabel}
                />
                {additionalLinkCount > 0 ? (
                  <span className="project-board-card-conversation-extra">
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
      </CardContent>
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
        className="project-board-context-menu-backdrop"
        onClick={onDismiss}
        onContextMenu={(event) => {
          event.preventDefault();
          onDismiss();
        }}
        type="button"
      />
      <div
        className="project-board-ticket-context-menu"
        onClick={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.preventDefault()}
        ref={menuRef}
        role="menu"
        style={menuStyle}
      >
        <button
          className="project-board-ticket-context-menu-item"
          disabled={primaryActionDisabled}
          onClick={onPrimaryAction}
          role="menuitem"
          type="button"
        >
          <IconPlayerPlay aria-hidden="true" />
          {primaryActionLabel}
        </button>
        <button
          className="project-board-ticket-context-menu-item project-board-ticket-context-menu-item-danger"
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