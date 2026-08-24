import {
  IconExternalLink,
  IconLink,
  IconPlayerPlay,
  IconUnlink,
  IconUser,
  IconX,
} from "@tabler/icons-react";
import {
  useEffect,
  useMemo,
  useState,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { Button } from "@/packages/components/ui/button";
import { Input } from "@/packages/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/packages/components/ui/select";
import {
  TOOLTIP_DELAY_MS,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/packages/components/ui/tooltip";
import {
  PRIORITY_OPTIONS,
  TSHIRT_OPTIONS,
  conversationLinkActionKind,
  conversationLinkLabel,
  conversationLinkStatusText,
  extractPreviewableDescriptionImageReferences,
  isUsableConversationLink,
  ticketCreatorName,
  type BoardColumn,
  type ProjectBoardCommentMetadata,
  type BoardStatusKey,
  type BoardTicket,
  type DescriptionImageReference,
  type TshirtSize,
} from "../project-board-shared";
import {
  type ProjectBoardAgentOption,
  type ProjectBoardConversationLinkView,
} from "@/packages/shared/bead-conversation-links";
import { type ConversationActionState } from "./types";
import {
  PROJECT_BOARD_PRIORITY_SELECT_ITEMS,
  PROJECT_BOARD_TSHIRT_SELECT_ITEMS,
} from "./constants";

export function TicketMetaFields({
  assignee,
  blockedByIds,
  blockingIds,
  boardColumns,
  createdBy,
  knownLabels,
  labels,
  onBlockedByChange,
  onBlockingChange,
  onLabelsChange,
  onPriorityChange,
  onStatusChange,
  onTshirtChange,
  priority,
  showStatus = true,
  status,
  ticketOptions,
  tshirt,
}: {
  assignee?: string;
  blockedByIds: string[];
  blockingIds: string[];
  boardColumns: ReadonlyArray<BoardColumn>;
  createdBy?: string;
  knownLabels: string[];
  labels: string[];
  onBlockedByChange: (ids: string[]) => void;
  onBlockingChange: (ids: string[]) => void;
  onLabelsChange: (labels: string[]) => void;
  onPriorityChange: (priority: string) => void;
  onStatusChange: (status: BoardStatusKey) => void;
  onTshirtChange: (size: TshirtSize | undefined) => void;
  priority: string;
  showStatus?: boolean;
  status: BoardStatusKey;
  ticketOptions: Array<{ id: string; label: string }>;
  tshirt?: TshirtSize;
}) {
  const [labelDraft, setLabelDraft] = useState("");
  const labelSuggestions = knownLabels.filter((label) => !labels.includes(label));
  const creator = ticketCreatorName(createdBy, assignee);
  const statusSelectItems = useMemo(
    () => boardColumns.map((column) => ({ label: column.label, value: column.key })),
    [boardColumns],
  );

  return (
    <div className="project-ticket-meta-grid">
      {showStatus ? (
        <label className="project-ticket-field project-ticket-field-inline">
          <span>Status</span>
          <Select
            items={statusSelectItems}
            onValueChange={(value) => onStatusChange(value as BoardStatusKey)}
            value={status}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {boardColumns.map((column) => (
                <SelectItem key={column.key} value={column.key}>
                  {column.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
      ) : null}
      <label className="project-ticket-field project-ticket-field-inline">
        <span>Priority</span>
        <Select
          items={PROJECT_BOARD_PRIORITY_SELECT_ITEMS}
          onValueChange={onPriorityChange}
          value={priority}
        >
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PRIORITY_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <label className="project-ticket-field project-ticket-field-inline">
        <span>T-shirt</span>
        <Select
          items={PROJECT_BOARD_TSHIRT_SELECT_ITEMS}
          onValueChange={(value) => onTshirtChange(value === "none" ? undefined : (value as TshirtSize))}
          value={tshirt ?? "none"}
        >
          <SelectTrigger>
            <SelectValue placeholder="None" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="none">None</SelectItem>
            {TSHIRT_OPTIONS.map((option) => (
              <SelectItem key={option.label} value={option.label}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      {/*
        CDXC:ProjectBoardTicketMetadata 2026-05-30-08:31:
        Ticket metadata should put Labels where Blocked by was, keep every metadata control's label-to-element spacing consistent, and show T-shirt select values as friendly labels.
      */}
      <div className="project-ticket-field project-ticket-field-inline project-ticket-labels-field">
        <span>Labels</span>
        {labels.length > 0 ? (
          <div className="project-ticket-label-list">
            {labels.map((label) => (
              <button
                className="project-ticket-label-chip"
                key={label}
                onClick={() => onLabelsChange(labels.filter((candidate) => candidate !== label))}
                type="button"
              >
                {label}
                <IconX aria-hidden="true" />
              </button>
            ))}
          </div>
        ) : null}
        <div className="project-ticket-label-editor">
          <Input
            aria-label="Add label"
            list="project-board-label-suggestions"
            onChange={(event) => setLabelDraft(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                const next = labelDraft.trim();
                if (next && !labels.includes(next)) {
                  onLabelsChange([...labels, next]);
                }
                setLabelDraft("");
              }
            }}
            placeholder="Add label"
            value={labelDraft}
          />
          <datalist id="project-board-label-suggestions">
            {labelSuggestions.map((label) => (
              <option key={label} value={label} />
            ))}
          </datalist>
          <Button
            onClick={() => {
              const next = labelDraft.trim();
              if (next && !labels.includes(next)) {
                onLabelsChange([...labels, next]);
              }
              setLabelDraft("");
            }}
            type="button"
            variant="outline"
          >
            Add
          </Button>
        </div>
      </div>
      <DependencyPicker
        label="Blocking"
        onChange={onBlockingChange}
        selectedIds={blockingIds}
        ticketOptions={ticketOptions}
      />
      <DependencyPicker
        label="Blocked by"
        onChange={onBlockedByChange}
        selectedIds={blockedByIds}
        ticketOptions={ticketOptions}
      />
      {creator ? (
        <div className="project-ticket-field project-ticket-field-inline">
          <span>Created by</span>
          <div className="project-ticket-creator-value" title={creator}>
            {creator}
          </div>
        </div>
      ) : null}
      {assignee ? (
        <div className="project-ticket-field project-ticket-field-inline">
          <span>Assignee</span>
          <div className="project-ticket-assignee-value" title={assignee}>
            <IconUser aria-hidden="true" />
            <span className="project-ticket-assignee-name">{assignee}</span>
          </div>
        </div>
      ) : null}
    </div>
  );
}

export function DependencyPicker({
  label,
  onChange,
  selectedIds,
  ticketOptions,
}: {
  label: string;
  onChange: (ids: string[]) => void;
  selectedIds: string[];
  ticketOptions: Array<{ id: string; label: string }>;
}) {
  const [draft, setDraft] = useState("");
  const available = ticketOptions.filter((option) => !selectedIds.includes(option.id));
  return (
    <div className="project-ticket-field project-ticket-field-inline">
      <span>{label}</span>
      {selectedIds.length > 0 ? (
        <div className="project-ticket-label-list">
          {selectedIds.map((id) => {
            const ticket = ticketOptions.find((option) => option.id === id);
            return (
              <button
                className="project-ticket-label-chip"
                key={id}
                onClick={() => onChange(selectedIds.filter((candidate) => candidate !== id))}
                type="button"
              >
                {ticket?.label ?? id}
                <IconX aria-hidden="true" />
              </button>
            );
          })}
        </div>
      ) : null}
      <Select
        onValueChange={(value) => {
          if (value && !selectedIds.includes(value)) {
            onChange([...selectedIds, value]);
          }
          setDraft("");
        }}
        value={draft}
      >
        <SelectTrigger>
          <SelectValue placeholder={`Add ${label.toLowerCase()} ticket`} />
        </SelectTrigger>
        <SelectContent>
          {available.map((option) => (
            <SelectItem key={option.id} value={option.id}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

export function DependencySummary({
  blockedByIds,
  blockingIds,
  tickets,
}: {
  blockedByIds: string[];
  blockingIds: string[];
  tickets: BoardTicket[];
}) {
  if (blockedByIds.length === 0 && blockingIds.length === 0) {
    return null;
  }
  const labelFor = (id: string) => tickets.find((ticket) => ticket.id === id)?.displayId ?? id;
  return (
    <div className="flex flex-col gap-1 text-xs font-normal text-muted-foreground">
      {blockedByIds.length > 0 ? (
        <p className="m-0">
          <span className="text-foreground/80">Blocked by:</span>{" "}
          {blockedByIds.map(labelFor).join(", ")}
        </p>
      ) : null}
      {blockingIds.length > 0 ? (
        <p className="m-0">
          <span className="text-foreground/80">Blocking:</span>{" "}
          {blockingIds.map(labelFor).join(", ")}
        </p>
      ) : null}
    </div>
  );
}

export function ImagePreviewStrip({
  description,
  imagePreviewDataUrls,
  onRemove,
}: {
  description: string;
  imagePreviewDataUrls: Record<string, string>;
  onRemove?: (image: DescriptionImageReference) => void;
}) {
  const [openImage, setOpenImage] = useState<DescriptionImageReference | undefined>();
  const images = extractPreviewableDescriptionImageReferences(description);
  const openPreviewSrc = openImage ? imagePreviewDataUrls[openImage.src] : undefined;

  useEffect(() => {
    if (!openImage) {
      return;
    }
    if (!images.some((image) => image.id === openImage.id)) {
      setOpenImage(undefined);
      return;
    }
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenImage(undefined);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [images, openImage]);

  if (images.length === 0) {
    return null;
  }

  return (
    <>
      <div className="project-ticket-image-strip" aria-label="Image previews">
        {images.map((image) => {
          const previewSrc = imagePreviewDataUrls[image.src];
          return (
            <div
              aria-label={previewSrc ? `Open image preview ${image.src}` : undefined}
              className="project-ticket-image-thumb"
              key={image.id}
              onClick={() => {
                if (previewSrc) {
                  setOpenImage(image);
                }
              }}
              onKeyDown={(event) => {
                if (!previewSrc) {
                  return;
                }
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  setOpenImage(image);
                }
              }}
              role={previewSrc ? "button" : undefined}
              tabIndex={previewSrc ? 0 : undefined}
            >
              {previewSrc ? <img alt="" src={previewSrc} /> : <span aria-hidden="true" />}
              {onRemove ? (
                <button
                  aria-label="Remove pasted image"
                  className="project-ticket-image-remove"
                  onClick={(event) => {
                    event.stopPropagation();
                    onRemove(image);
                    if (openImage?.id === image.id) {
                      setOpenImage(undefined);
                    }
                  }}
                  type="button"
                >
                  <IconX aria-hidden="true" />
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
      {openImage && openPreviewSrc
        ? createPortal(
            <div
              className="project-ticket-image-popup"
              onClick={() => setOpenImage(undefined)}
              role="presentation"
            >
              <img alt="" src={openPreviewSrc} />
            </div>,
            document.body,
          )
        : null}
    </>
  );
}

export function ConversationSection({
  action,
  agents,
  focusedSessionId,
  links,
  onAssociateFocusedSession,
  onJumpToConversation,
  onSelectedAgentChange,
  onUnlinkConversation,
  selectedAgentId,
}: {
  action: ConversationActionState;
  agents: ProjectBoardAgentOption[];
  focusedSessionId?: string;
  links: ProjectBoardConversationLinkView[];
  onAssociateFocusedSession: () => void;
  onJumpToConversation: (link: ProjectBoardConversationLinkView) => void;
  onSelectedAgentChange: (agentId: string) => void;
  onUnlinkConversation: (link: ProjectBoardConversationLinkView) => void;
  selectedAgentId: string;
}) {
  const isAssociating = action?.kind === "associate";
  const hasActiveConversationAction = Boolean(action);
  const agentSelectItems = useMemo(
    () =>
      agents.map((agent) => ({
        label: agent.label,
        value: agent.agentId,
      })),
    [agents],
  );
  return (
    <section className="project-ticket-conversations" aria-label="Linked conversations">
      <div className="project-ticket-section-title">Start work with</div>
      <div className="project-ticket-conversation-controls">
        <Select
          disabled={agents.length === 0}
          items={agentSelectItems}
          onValueChange={onSelectedAgentChange}
          value={selectedAgentId}
        >
          <SelectTrigger aria-label="Agent for Start work">
            <SelectValue placeholder="Choose agent" />
          </SelectTrigger>
          <SelectContent>
            {agents.map((agent) => (
              <SelectItem key={agent.agentId} value={agent.agentId}>
                {agent.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          disabled={!focusedSessionId || hasActiveConversationAction}
          onClick={onAssociateFocusedSession}
          type="button"
          variant="outline"
        >
          <IconLink data-icon="inline-start" />
          {isAssociating ? "Associating" : "Associate focused"}
        </Button>
      </div>
      {links.length > 0 ? (
        <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
          <div className="project-ticket-conversation-list">
            {links.map((link) => {
              const label = conversationLinkLabel(link);
              const actionKind = conversationLinkActionKind(link);
              return (
                <div className="project-ticket-conversation-row" key={link.id}>
                  <div className="project-ticket-conversation-main">
                    <ConversationLinkName
                      className="project-ticket-conversation-name"
                      label={label}
                    />
                    <span className="project-ticket-conversation-status">
                      {conversationLinkStatusText(link)}
                    </span>
                  </div>
                  <div className="project-ticket-conversation-actions">
                    <Button
                      aria-label={
                        actionKind === "resume"
                          ? "Resume linked conversation"
                          : "Jump to linked conversation"
                      }
                      disabled={actionKind === "none" || hasActiveConversationAction}
                      onClick={() => onJumpToConversation(link)}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      {actionKind === "resume" ? <IconPlayerPlay /> : <IconExternalLink />}
                    </Button>
                    <Button
                      aria-label="Unlink conversation"
                      disabled={hasActiveConversationAction}
                      onClick={() => onUnlinkConversation(link)}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <IconUnlink />
                    </Button>
                  </div>
                </div>
              );
            })}
          </div>
        </TooltipProvider>
      ) : (
        <p className="project-ticket-empty">No linked conversation yet.</p>
      )}
    </section>
  );
}

export function getPrimaryUsableConversationLink(
  links: ProjectBoardConversationLinkView[],
): ProjectBoardConversationLinkView | undefined {
  return links.find(isUsableConversationLink);
}

export function ConversationLinkName({
  className,
  label,
}: {
  className: string;
  label: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger render={<span className={className}>{label}</span>} />
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
}

export function projectBoardCommentMetadataFromLink(
  link: ProjectBoardConversationLinkView | undefined,
): ProjectBoardCommentMetadata {
  /*
   * CDXC:ProjectBoardComments 2026-06-05-06:43:
   * UI-added comments should use the linked agent conversation as their metadata source so the rendered author line can show the agent beside the Beads user and the footer can show the resumable agent CLI session id instead of the truncated status preview.
   *
   * CDXC:ProjectBoardComments 2026-06-05-06:55:
   * The comment Session footer must be the saved session id from the agent CLI, not the Ghostex pane id. If the linked conversation has not reported an agent session id yet, omit the footer rather than displaying the wrong id as resumable.
   */
  if (!link) {
    return {};
  }
  return {
    agentName: link.agentName || link.agentId,
    sessionId: link.agentSessionId,
  };
}

export function compareConversationLinksNewestFirst(
  left: ProjectBoardConversationLinkView,
  right: ProjectBoardConversationLinkView,
): number {
  const leftTime = Date.parse(left.updatedAt);
  const rightTime = Date.parse(right.updatedAt);
  return (Number.isNaN(rightTime) ? 0 : rightTime) - (Number.isNaN(leftTime) ? 0 : leftTime);
}