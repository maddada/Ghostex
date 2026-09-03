import { AppTooltip } from '@/packages/core-ui/app-tooltip';
import {
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  useRef,
} from 'react';
import { IconMessagePlus, IconX } from '@tabler/icons-react';
import { Bold as MeoBoldIcon } from 'lucide-react';
import { createPortal } from 'react-dom';
import {
  MANAGE_COMMENT_ANNOTATION_COLOR,
  MANAGE_DISMISS_TOOLBAR_COLOR,
  MANAGE_MEO_HEADING_COLOR,
  MANAGE_QUICK_LABELS,
} from '../constants';
import {
  ManageAnnotation,
  ManageAnnotationPreview,
  ManageCommentDraft,
  ManageQuickLabel,
  ManageSelectionAnchor,
} from '../types';
import { ManageTooltipButton } from '../manage-tooltip-button';
import {
  annotationDisplayNote,
  annotationPreviewCardStyle,
  annotationPreviewText,
  annotationTypeLabel,
  clampManageSelectionToolbarLeft,
  commentPopoverStyle,
  manageAnnotationColor,
  manageToolbarActionStyle,
  renderManageQuickLabelIcon,
} from '../annotation-store';

export function ManageAnnotationToolbar({
  anchor,
  onComment,
  onDismiss,
  onFormatting,
  onQuickLabel,
}: {
  anchor: ManageSelectionAnchor;
  onComment: () => void;
  onDismiss: () => void;
  onFormatting: () => void;
  onQuickLabel: (label: ManageQuickLabel) => void;
}) {
  return createPortal(
    <div
      className='manage-markdown-selection-toolbar'
      style={{
        left: clampManageSelectionToolbarLeft(anchor.left),
        top: Math.max(8, anchor.top - 46),
      }}
    >
      <AppTooltip content='Comment' side='top'>
        <button
          aria-label='Comment'
          onClick={onComment}
          style={manageToolbarActionStyle(MANAGE_COMMENT_ANNOTATION_COLOR)}
          type='button'
        >
          <IconMessagePlus aria-hidden='true' size={15} />
        </button>
      </AppTooltip>
      <AppTooltip content='Formatting' side='top'>
        <button
          aria-label='Formatting'
          onClick={onFormatting}
          style={manageToolbarActionStyle(MANAGE_MEO_HEADING_COLOR)}
          type='button'
        >
          <MeoBoldIcon aria-hidden='true' size={15} />
        </button>
      </AppTooltip>
      {MANAGE_QUICK_LABELS.map((label) => (
        <AppTooltip content={label.text} key={label.id} side='top'>
          <button
            aria-label={label.text}
            onClick={() => onQuickLabel(label)}
            style={manageToolbarActionStyle(label.color)}
            type='button'
          >
            {renderManageQuickLabelIcon(label.id)}
          </button>
        </AppTooltip>
      ))}
      <AppTooltip content='Dismiss' side='top'>
        <button
          aria-label='Dismiss'
          onClick={onDismiss}
          style={manageToolbarActionStyle(MANAGE_DISMISS_TOOLBAR_COLOR)}
          type='button'
        >
          <IconX aria-hidden='true' size={15} />
        </button>
      </AppTooltip>
    </div>,
    document.body
  );
}

export function ManageAnnotationPreviewCard({
  onRemoveAnnotation,
  preview,
}: {
  onRemoveAnnotation: (annotationId: string) => void;
  preview: ManageAnnotationPreview;
}) {
  const annotation = preview.annotation;
  const note = annotationPreviewText(annotation);
  return createPortal(
    <aside
      className='manage-annotation-preview-card'
      data-label-id={annotation.labelId}
      data-type={annotation.type}
      style={
        {
          ...annotationPreviewCardStyle(preview.anchor),
          '--manage-annotation-color': manageAnnotationColor(annotation),
        } as CSSProperties
      }
    >
      <header>
        <span>{annotationTypeLabel(annotation)}</span>
        {annotation.attachments.length > 0 ? (
          <span>
            {annotation.attachments.length} {annotation.attachments.length === 1 ? 'image' : 'images'}
          </span>
        ) : null}
      </header>
      <ManageTooltipButton
        aria-label='Remove annotation'
        className='manage-annotation-preview-remove-button manage-icon-button'
        onClick={(event: ReactMouseEvent<HTMLButtonElement>) => {
          event.stopPropagation();
          onRemoveAnnotation(annotation.id);
        }}
        onPointerDown={(event: ReactPointerEvent<HTMLButtonElement>) => {
          event.preventDefault();
          event.stopPropagation();
        }}
        tooltip='Remove annotation'
        type='button'
      >
        <IconX aria-hidden='true' size={14} />
      </ManageTooltipButton>
      <p>{note}</p>
    </aside>,
    document.body
  );
}

export function ManageCommentPopover({
  draft,
  onAddAttachmentFiles,
  onCancel,
  onDraftNoteChange,
  onRemoveDraftAttachment,
  onSubmit,
}: {
  draft: ManageCommentDraft;
  onAddAttachmentFiles: (files: FileList | File[]) => void;
  onCancel: () => void;
  onDraftNoteChange: (note: string) => void;
  onRemoveDraftAttachment: (attachmentId: string) => void;
  onSubmit: () => void;
}) {
  const attachmentInputRef = useRef<HTMLInputElement | null>(null);
  const canSubmit = Boolean(draft.note.trim()) || draft.attachments.length > 0;
  return createPortal(
    <div className='manage-comment-popover' style={commentPopoverStyle(draft.anchor)}>
      <ManageTooltipButton
        aria-label='Close comment composer'
        className='manage-comment-popover-close manage-icon-button'
        onClick={onCancel}
        tooltip='Close'
        type='button'
      >
        <IconX aria-hidden='true' size={14} />
      </ManageTooltipButton>
      <textarea
        aria-label='Annotation note'
        autoFocus
        onChange={(event) => onDraftNoteChange(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault();
            onCancel();
            return;
          }
          if ((event.metaKey || event.ctrlKey) && event.key === 'Enter' && canSubmit) {
            event.preventDefault();
            onSubmit();
          }
        }}
        placeholder={draft.quote ? 'Add a comment' : 'Add a global comment'}
        value={draft.note}
      />
      {draft.attachments.length > 0 ? (
        <div className='manage-attachment-strip'>
          {draft.attachments.map((attachment) => (
            <figure className='manage-attachment-chip' key={attachment.id}>
              <img alt='' src={attachment.dataUrl} />
              <figcaption>{attachment.name}</figcaption>
              <button
                aria-label={`Remove ${attachment.name}`}
                onClick={() => onRemoveDraftAttachment(attachment.id)}
                type='button'
              >
                <IconX aria-hidden='true' size={12} />
              </button>
            </figure>
          ))}
        </div>
      ) : null}
      {draft.attachmentError ? <div className='manage-attachment-error'>{draft.attachmentError}</div> : null}
      <div className='manage-comment-popover-actions'>
        {/*
         * CDXC:Docs 2026-06-28-08:31:
         * The Image action in the Markdown annotation comment composer is hidden because the current picker does not open from this surface. Keep the button source commented so the picker flow can be restored when it is fixed instead of deleting the intended UI.
         *
         * <button
         *   className="manage-comment-popover-image-button"
         *   onClick={() => attachmentInputRef.current?.click()}
         *   type="button"
         * >
         *   <IconPhoto aria-hidden="true" size={14} />
         *   Image
         * </button>
         */}
        <button className='manage-comment-popover-submit' disabled={!canSubmit} onClick={onSubmit} type='button'>
          <IconMessagePlus aria-hidden='true' size={14} />
          Submit
        </button>
      </div>
      <input
        accept='image/*'
        aria-label='Annotation image attachments'
        className='manage-hidden-file-input'
        multiple
        onChange={(event) => {
          if (event.currentTarget.files) {
            onAddAttachmentFiles(event.currentTarget.files);
          }
          event.currentTarget.value = '';
        }}
        ref={attachmentInputRef}
        type='file'
      />
    </div>,
    document.body
  );
}

export function ManageAnnotationDropdown({
  annotations,
  onRemoveAnnotation,
}: {
  annotations: ManageAnnotation[];
  onRemoveAnnotation: (annotationId: string) => void;
}) {
  return (
    <div
      aria-label='Annotations'
      className='manage-annotation-dropdown'
      id='manage-markdown-annotation-dropdown'
      role='dialog'
    >
      <header>
        <span>Annotations</span>
      </header>
      <div className='manage-annotation-dropdown-list'>
        {annotations.length === 0 ? <div className='manage-annotation-empty'>No annotations</div> : null}
        {annotations.map((annotation) => {
          const note = annotationDisplayNote(annotation);
          return (
            <article
              className='manage-annotation-card'
              data-label-id={annotation.labelId}
              data-type={annotation.type}
              key={annotation.id}
              style={{ '--manage-annotation-color': manageAnnotationColor(annotation) } as CSSProperties}
            >
              <div className='manage-annotation-card-header'>
                <span>{annotationTypeLabel(annotation)}</span>
                <ManageTooltipButton
                  aria-label='Remove annotation'
                  className='manage-annotation-remove-button manage-icon-button'
                  onClick={() => onRemoveAnnotation(annotation.id)}
                  tooltip='Remove annotation'
                  type='button'
                >
                  <IconX aria-hidden='true' size={14} />
                </ManageTooltipButton>
              </div>
              {annotation.scope === 'selection' ? <blockquote>{annotation.quote}</blockquote> : null}
              {note ? <p>{note}</p> : null}
              {annotation.attachments.length > 0 ? (
                <div className='manage-annotation-attachments'>
                  {annotation.attachments.map((attachment) => (
                    <a href={attachment.dataUrl} key={attachment.id} rel='noreferrer' target='_blank'>
                      <img alt='' src={attachment.dataUrl} />
                      <span>{attachment.name}</span>
                    </a>
                  ))}
                </div>
              ) : null}
            </article>
          );
        })}
      </div>
    </div>
  );
}
