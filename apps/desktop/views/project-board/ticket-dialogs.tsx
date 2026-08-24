/**
 * CDXC:ProjectBoardDialogRedesign 2026-08-24:
 * The New Ticket and Edit ticket dialogs moved out of project-board-app.tsx so
 * the Codex-style redesign can be rendered from Storybook with mock props. The
 * components are pure presentation over the board's existing state and
 * callbacks: no bd calls, no bridge state, and no form logic moved with them.
 */
import { IconExternalLink, IconLink, IconPlayerPlay, IconTrash } from '@tabler/icons-react';
import type { ClipboardEvent, ComponentProps, KeyboardEvent, RefObject } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { Input } from '@/packages/components/ui/input';
import { ScrollArea } from '@/packages/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { Textarea } from '@/packages/components/ui/textarea';
import {
  appendImageMarkdownToDescription,
  boardStatusLabel,
  formatShortDate,
  parseProjectBoardCommentText,
  removeDescriptionImageReference,
  type BoardColumn,
  type BoardStatusKey,
  type BoardTicket,
  type TshirtSize,
} from '../project-board-shared';
import type {
  ProjectBoardConversationLinkView,
  ProjectBoardConversationState,
  ProjectBoardStartLocation,
} from '@/packages/shared/bead-conversation-links';
import { PROJECT_BOARD_START_LOCATION_SELECT_ITEMS } from './constants';
import { hasProjectBoardImagePastePayload } from './board-state';
import { sendProjectBoardImageRequest } from './bridge';
import {
  ConversationSection,
  DependencySummary,
  ImagePreviewStrip,
  TicketMetaFields,
  projectBoardCommentMetadataFromLink,
} from './ticket-detail';
import type { ConversationActionState, DetailDraft, TicketFormDraft } from './types';

type SelectItems = ComponentProps<typeof Select>['items'];

export function handleCmdEnter(event: KeyboardEvent, action: () => void) {
  if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
    event.preventDefault();
    action();
  }
}

function pasteImageIntoDescription(
  event: ClipboardEvent<HTMLTextAreaElement>,
  applyDescription: (update: (description: string) => string) => void,
  onError: (message: string) => void
) {
  if (!hasProjectBoardImagePastePayload(event.clipboardData)) {
    return;
  }
  event.preventDefault();
  const selectionStart = event.currentTarget.selectionStart;
  const selectionEnd = event.currentTarget.selectionEnd;
  void sendProjectBoardImageRequest({ action: 'pasteImage' })
    .then((response) => {
      if (!response.imagePath) {
        onError(response.error || 'Clipboard image could not be converted to a path.');
        return;
      }
      applyDescription((description) =>
        appendImageMarkdownToDescription(description, response.imagePath ?? '', selectionStart, selectionEnd)
      );
    })
    .catch((error) => {
      onError(error instanceof Error ? error.message : 'Clipboard image paste failed.');
    });
}

export function EditTicketDialog({
  boardColumns,
  conversationAction,
  conversationState,
  deleteConfirmingTicketId,
  detail,
  detailCommentMetadataLink,
  detailConversationLinks,
  detailPrimaryActionDisabled,
  detailPrimaryActionKind,
  detailPrimaryActionLabel,
  detailPrimaryConversationLink,
  imagePreviewDataUrls,
  knownLabels,
  onAssociateFocusedSession,
  onClose,
  onDeleteTicket,
  onJumpToConversation,
  onSaveTicketDetail,
  onSelectedAgentChange,
  onStartTicketWork,
  onUnlinkConversation,
  selectedAgentId,
  setDeleteConfirmingTicketId,
  setDetail,
  setErrorMessage,
  ticketOptions,
  tickets,
}: {
  boardColumns: ReadonlyArray<BoardColumn>;
  conversationAction: ConversationActionState;
  conversationState: ProjectBoardConversationState;
  deleteConfirmingTicketId: string;
  detail: DetailDraft;
  detailCommentMetadataLink: ProjectBoardConversationLinkView | undefined;
  detailConversationLinks: ProjectBoardConversationLinkView[];
  detailPrimaryActionDisabled: boolean;
  detailPrimaryActionKind: string;
  detailPrimaryActionLabel: string;
  detailPrimaryConversationLink: ProjectBoardConversationLinkView | undefined;
  imagePreviewDataUrls: Record<string, string>;
  knownLabels: string[];
  onAssociateFocusedSession: () => void;
  onClose: () => void;
  onDeleteTicket: () => void;
  onJumpToConversation: (link: ProjectBoardConversationLinkView) => void;
  onSaveTicketDetail: () => void;
  onSelectedAgentChange: (agentId: string) => void;
  onStartTicketWork: () => void;
  onUnlinkConversation: (link: ProjectBoardConversationLinkView) => void;
  selectedAgentId: string;
  setDeleteConfirmingTicketId: (ticketId: string) => void;
  setDetail: (update: (current: DetailDraft) => DetailDraft) => void;
  setErrorMessage: (message: string) => void;
  ticketOptions: Array<{ id: string; label: string }>;
  tickets: BoardTicket[];
}) {
  const isConfirmingDelete = deleteConfirmingTicketId === detail.ticket?.id;
  return (
    <Dialog
      open={Boolean(detail.ticket)}
      onOpenChange={(open) => {
        if (!open) {
          onClose();
        }
      }}
    >
      <DialogContent className='project-ticket-dialog gap-4 p-5'>
        <DialogHeader className='gap-1'>
          <DialogTitle className='text-[15px] font-normal'>Edit ticket</DialogTitle>
          <DialogDescription className='text-xs font-normal text-muted-foreground'>
            {detail.ticket?.displayId} · {detail.ticket?.id}
          </DialogDescription>
        </DialogHeader>
        <div
          className='project-ticket-dialog-body vertical-scroll-fade-mask'
          onKeyDown={(event) => handleCmdEnter(event, () => onSaveTicketDetail())}
        >
          <TicketMetaFields
            assignee={detail.ticket?.assignee}
            blockedByIds={detail.blockedByIds}
            blockingIds={detail.blockingIds}
            boardColumns={boardColumns}
            createdBy={detail.ticket?.created_by}
            knownLabels={knownLabels}
            labels={detail.labels}
            onBlockedByChange={(blockedByIds) => setDetail((current) => ({ ...current, blockedByIds }))}
            onBlockingChange={(blockingIds) => setDetail((current) => ({ ...current, blockingIds }))}
            onLabelsChange={(labels) => setDetail((current) => ({ ...current, labels }))}
            onPriorityChange={(priority) => setDetail((current) => ({ ...current, priority }))}
            onStatusChange={(status: BoardStatusKey) => setDetail((current) => ({ ...current, status }))}
            onTshirtChange={(tshirt: TshirtSize | undefined) => setDetail((current) => ({ ...current, tshirt }))}
            priority={detail.priority}
            status={detail.status}
            ticketOptions={ticketOptions.filter((option) => option.id !== detail.ticket?.id)}
            tshirt={detail.tshirt}
          />
          <label className='project-ticket-field'>
            <span>Title</span>
            <Input
              className='project-ticket-title-input'
              onChange={(event) => {
                const title = event.currentTarget.value;
                setDetail((current) => ({ ...current, title }));
              }}
              value={detail.title}
            />
          </label>
          <label className='project-ticket-field'>
            <span>Prompt</span>
            <Textarea
              className='project-ticket-prompt-input'
              onChange={(event) => {
                const description = event.currentTarget.value;
                setDetail((current) => ({
                  ...current,
                  description,
                }));
              }}
              onPaste={(event) =>
                pasteImageIntoDescription(
                  event,
                  (update) =>
                    setDetail((current) => ({
                      ...current,
                      description: update(current.description),
                    })),
                  setErrorMessage
                )
              }
              placeholder='Write the full prompt for this ticket.'
              value={detail.description}
            />
          </label>
          <ImagePreviewStrip
            description={detail.description}
            imagePreviewDataUrls={imagePreviewDataUrls}
            onRemove={(image) =>
              setDetail((current) => ({
                ...current,
                description: removeDescriptionImageReference(current.description, image.id),
              }))
            }
          />
          <DependencySummary blockedByIds={detail.blockedByIds} blockingIds={detail.blockingIds} tickets={tickets} />
          {detail.ticket ? (
            <ConversationSection
              agents={conversationState.agents}
              action={conversationAction}
              focusedSessionId={conversationState.focusedTerminalSessionId}
              links={detailConversationLinks}
              onAssociateFocusedSession={onAssociateFocusedSession}
              onJumpToConversation={onJumpToConversation}
              onSelectedAgentChange={onSelectedAgentChange}
              onUnlinkConversation={onUnlinkConversation}
              selectedAgentId={selectedAgentId}
            />
          ) : null}
          <section className='flex flex-col gap-2' aria-label='Comments'>
            <div className='project-ticket-section-title'>Comments</div>
            <ScrollArea className='project-ticket-comment-list'>
              {detail.ticket?.comments?.length ? (
                detail.ticket.comments.map((comment, index) => {
                  const parsedComment = parseProjectBoardCommentText(comment.text);
                  const fallbackMetadata = projectBoardCommentMetadataFromLink(detailCommentMetadataLink);
                  const agentName = parsedComment.agentName ?? fallbackMetadata.agentName;
                  const sessionId = parsedComment.sessionId ?? fallbackMetadata.sessionId;
                  const createdAtLabel = formatShortDate(comment.created_at);
                  return (
                    <article className='project-ticket-comment' key={`${comment.created_at}-${index}`}>
                      <div className='flex min-w-0 items-baseline justify-between gap-2.5'>
                        <div className='flex min-w-0 items-baseline gap-1.5'>
                          <span className='min-w-0 truncate text-[13px] font-normal text-foreground/90'>
                            {comment.author || 'Comment'}
                          </span>
                          {agentName ? <span className='project-ticket-comment-agent'>({agentName})</span> : null}
                        </div>
                        {createdAtLabel ? (
                          <time
                            dateTime={comment.created_at}
                            className='shrink-0 text-[11px] font-normal text-muted-foreground'
                          >
                            {createdAtLabel}
                          </time>
                        ) : null}
                      </div>
                      <p>{parsedComment.body || comment.text}</p>
                      {sessionId ? (
                        <footer className='project-ticket-comment-session'>
                          <span>Session</span>
                          <code>{sessionId}</code>
                        </footer>
                      ) : null}
                    </article>
                  );
                })
              ) : (
                <p className='project-ticket-empty'>No comments yet.</p>
              )}
            </ScrollArea>
          </section>
          <label className='project-ticket-field'>
            <span>Add comment</span>
            <Textarea
              onChange={(event) => {
                const comment = event.currentTarget.value;
                setDetail((current) => ({ ...current, comment }));
              }}
              placeholder='Add a note for the team.'
              value={detail.comment}
            />
          </label>
        </div>
        <DialogFooter className='project-ticket-dialog-footer'>
          <Button
            disabled={detail.isDeleting || detail.isSaving}
            onClick={() => {
              if (isConfirmingDelete) {
                onDeleteTicket();
                return;
              }
              setDeleteConfirmingTicketId(detail.ticket?.id ?? '');
            }}
            type='button'
            variant='destructive'
          >
            <IconTrash data-icon='inline-start' />
            {isConfirmingDelete ? (detail.isDeleting ? 'Deleting' : 'Confirm delete') : 'Delete'}
          </Button>
          <div className='ml-auto flex flex-wrap items-center justify-end gap-2'>
            <Button
              disabled={detailPrimaryActionDisabled}
              onClick={() => {
                if (detailPrimaryConversationLink) {
                  onJumpToConversation(detailPrimaryConversationLink);
                  return;
                }
                onStartTicketWork();
              }}
              type='button'
              variant='outline'
            >
              {detailPrimaryConversationLink ? (
                detailPrimaryActionKind === 'resume' ? (
                  <IconPlayerPlay data-icon='inline-start' />
                ) : (
                  <IconExternalLink data-icon='inline-start' />
                )
              ) : (
                <IconLink data-icon='inline-start' />
              )}
              {detailPrimaryActionLabel}
            </Button>
            <Button disabled={detail.isDeleting || detail.isSaving} onClick={() => onSaveTicketDetail()}>
              {detail.isSaving ? 'Saving' : 'Save'}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function NewTicketDialog({
  agentSelectItems,
  boardColumns,
  conversationAction,
  conversationState,
  imagePreviewDataUrls,
  knownLabels,
  newPromptRef,
  newTicket,
  newTicketStartLocation,
  onCreateTicket,
  onOpenChange,
  onSelectedAgentChange,
  open,
  selectedAgentId,
  setErrorMessage,
  setNewTicket,
  setNewTicketStartLocation,
  ticketOptions,
}: {
  agentSelectItems: SelectItems;
  boardColumns: ReadonlyArray<BoardColumn>;
  conversationAction: ConversationActionState;
  conversationState: ProjectBoardConversationState;
  imagePreviewDataUrls: Record<string, string>;
  knownLabels: string[];
  newPromptRef?: RefObject<HTMLTextAreaElement | null>;
  newTicket: TicketFormDraft;
  newTicketStartLocation: ProjectBoardStartLocation;
  onCreateTicket: (options?: { startAfterCreate?: boolean }) => void;
  onOpenChange: (open: boolean) => void;
  onSelectedAgentChange: (agentId: string) => void;
  open: boolean;
  selectedAgentId: string;
  setErrorMessage: (message: string) => void;
  setNewTicket: (update: (current: TicketFormDraft) => TicketFormDraft) => void;
  setNewTicketStartLocation: (location: ProjectBoardStartLocation) => void;
  ticketOptions: Array<{ id: string; label: string }>;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='project-ticket-dialog gap-4 p-5'>
        <DialogHeader className='gap-1'>
          <DialogTitle className='text-[15px] font-normal'>New Ticket</DialogTitle>
          <DialogDescription className='text-xs font-normal text-muted-foreground'>
            Leave the title empty to auto-generate it from the prompt. Creates in{' '}
            {boardStatusLabel(newTicket.status, boardColumns)}.
          </DialogDescription>
        </DialogHeader>
        <div
          className='project-ticket-dialog-body vertical-scroll-fade-mask'
          onKeyDown={(event) => handleCmdEnter(event, () => onCreateTicket())}
        >
          <TicketMetaFields
            blockedByIds={newTicket.blockedByIds}
            blockingIds={newTicket.blockingIds}
            boardColumns={boardColumns}
            knownLabels={knownLabels}
            labels={newTicket.labels}
            onBlockedByChange={(blockedByIds) => setNewTicket((current) => ({ ...current, blockedByIds }))}
            onBlockingChange={(blockingIds) => setNewTicket((current) => ({ ...current, blockingIds }))}
            onLabelsChange={(labels) => setNewTicket((current) => ({ ...current, labels }))}
            onPriorityChange={(priority) => setNewTicket((current) => ({ ...current, priority }))}
            onStatusChange={() => undefined}
            onTshirtChange={(tshirt: TshirtSize | undefined) => setNewTicket((current) => ({ ...current, tshirt }))}
            priority={newTicket.priority}
            status='todo'
            showStatus={false}
            ticketOptions={ticketOptions}
            tshirt={newTicket.tshirt}
          />
          <label className='project-ticket-field'>
            <span>Title</span>
            <Input
              className='project-ticket-title-input'
              onChange={(event) => {
                const title = event.currentTarget.value;
                setNewTicket((current) => ({ ...current, title }));
              }}
              placeholder='Auto-generated from prompt when left empty'
              value={newTicket.title}
            />
          </label>
          <label className='project-ticket-field'>
            <span>Prompt</span>
            <Textarea
              className='project-ticket-prompt-input'
              onChange={(event) => {
                const description = event.currentTarget.value;
                setNewTicket((current) => ({
                  ...current,
                  description,
                }));
              }}
              onPaste={(event) =>
                pasteImageIntoDescription(
                  event,
                  (update) =>
                    setNewTicket((current) => ({
                      ...current,
                      description: update(current.description),
                    })),
                  setErrorMessage
                )
              }
              placeholder='Write the full prompt for this ticket.'
              ref={newPromptRef}
              value={newTicket.description}
            />
          </label>
          <ImagePreviewStrip
            description={newTicket.description}
            imagePreviewDataUrls={imagePreviewDataUrls}
            onRemove={(image) =>
              setNewTicket((current) => ({
                ...current,
                description: removeDescriptionImageReference(current.description, image.id),
              }))
            }
          />
        </div>
        <DialogFooter className='project-ticket-create-footer'>
          <section className='flex min-w-0 flex-col gap-2' aria-label='Create and start options'>
            <div className='project-ticket-section-title'>Start work</div>
            <div className='project-ticket-create-start-controls'>
              <Select
                disabled={conversationState.agents.length === 0}
                items={agentSelectItems}
                onValueChange={onSelectedAgentChange}
                value={selectedAgentId}
              >
                <SelectTrigger aria-label='Agent for Create and Start' className='project-ticket-footer-select'>
                  <SelectValue placeholder='Choose agent' />
                </SelectTrigger>
                <SelectContent>
                  {conversationState.agents.map((agent) => (
                    <SelectItem key={agent.agentId} value={agent.agentId}>
                      {agent.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Select
                items={PROJECT_BOARD_START_LOCATION_SELECT_ITEMS}
                onValueChange={(value) => setNewTicketStartLocation(value as ProjectBoardStartLocation)}
                value={newTicketStartLocation}
              >
                <SelectTrigger aria-label='Start location' className='project-ticket-footer-select'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {PROJECT_BOARD_START_LOCATION_SELECT_ITEMS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </section>
          <div className='project-ticket-create-actions'>
            <Button
              disabled={!newTicket.description.trim()}
              onClick={() => onCreateTicket()}
              type='button'
              variant='outline'
            >
              Create
            </Button>
            <Button
              disabled={
                !newTicket.description.trim() || conversationState.agents.length === 0 || Boolean(conversationAction)
              }
              onClick={() => onCreateTicket({ startAfterCreate: true })}
              type='button'
            >
              <IconLink data-icon='inline-start' />
              Create & Start
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
