import { type CSSProperties, type ReactNode } from 'react';
import { IconCircleCheck, IconHelpCircle, IconTestPipe } from '@tabler/icons-react';
import {
  MANAGE_ANNOTATION_IMAGE_MAX_BYTES,
  MANAGE_ANNOTATION_MAX_IMAGES,
  MANAGE_ANNOTATION_SCHEMA_VERSION,
  MANAGE_COMMENT_ANNOTATION_COLOR,
  MANAGE_QUICK_LABELS,
  MANAGE_REDLINE_ANNOTATION_COLOR,
  MANAGE_SELECTION_MAX_LENGTH,
  MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN,
  MANAGE_SELECTION_TOOLBAR_WIDTH_ESTIMATE,
} from './constants';
import {
  ManageAnnotation,
  ManageAnnotationImage,
  ManageAnnotationStore,
  ManageMeoSelectionState,
  ManageQuickLabelId,
  ManageSelectionAnchor,
  isRecord,
} from './types';

export function annotationPersistenceLabel(state: 'idle' | 'loading' | 'ready' | 'saving' | 'saved' | 'error'): string {
  switch (state) {
    case 'error':
      return 'Not saved';
    case 'loading':
      return 'Loading';
    case 'saved':
      return 'Saved';
    case 'saving':
      return 'Saving';
    case 'idle':
    case 'ready':
      return 'Local';
  }
}

export function annotationTypeLabel(annotation: ManageAnnotation): string {
  if (annotation.type === 'redline') {
    return 'Redline';
  }
  if (annotation.labelId) {
    return quickLabelText(annotation.labelId);
  }
  return annotation.scope === 'global' ? 'Global comment' : 'Comment';
}

export function annotationDisplayNote(annotation: ManageAnnotation): string {
  const note = annotation.note.trim();
  if (!note) {
    return '';
  }
  return annotation.labelId && note === quickLabelText(annotation.labelId) ? '' : note;
}

export function quickLabelText(labelId: ManageQuickLabelId): string {
  return MANAGE_QUICK_LABELS.find((label) => label.id === labelId)?.text ?? labelId;
}

export function quickLabelColor(labelId: ManageQuickLabelId | undefined): string {
  return MANAGE_QUICK_LABELS.find((label) => label.id === labelId)?.color ?? MANAGE_COMMENT_ANNOTATION_COLOR;
}

export function manageAnnotationColor(annotation: Pick<ManageAnnotation, 'labelId' | 'type'>): string {
  return annotation.type === 'redline' ? MANAGE_REDLINE_ANNOTATION_COLOR : quickLabelColor(annotation.labelId);
}

export function manageToolbarActionStyle(color: string): CSSProperties {
  return { '--manage-toolbar-action-color': color } as CSSProperties;
}

export function clampManageSelectionToolbarLeft(left: number): number {
  const halfToolbarWidth = Math.min(
    MANAGE_SELECTION_TOOLBAR_WIDTH_ESTIMATE / 2,
    Math.max(0, window.innerWidth / 2 - MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN)
  );
  const minLeft = MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN + halfToolbarWidth;
  const maxLeft = Math.max(minLeft, window.innerWidth - MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN - halfToolbarWidth);
  return Math.min(Math.max(left, minLeft), maxLeft);
}

export function meoSelectionToolbarPosition(
  selectionState: ManageMeoSelectionState,
  fallbackAnchor: ManageSelectionAnchor
): { isBelow: boolean; left: number; top: number } {
  const margin = 8;
  const estimatedWidth = 236;
  const estimatedHeight = 34;
  const anchorX =
    typeof selectionState.anchorX === 'number' && Number.isFinite(selectionState.anchorX)
      ? selectionState.anchorX
      : fallbackAnchor.left;
  const anchorY =
    typeof selectionState.anchorY === 'number' && Number.isFinite(selectionState.anchorY)
      ? selectionState.anchorY
      : fallbackAnchor.top;
  const anchorBottomY =
    typeof selectionState.anchorBottomY === 'number' && Number.isFinite(selectionState.anchorBottomY)
      ? selectionState.anchorBottomY
      : fallbackAnchor.top;
  const rawLeft = selectionState.align === 'start' ? anchorX : anchorX - estimatedWidth / 2;
  const maxLeft = Math.max(margin, window.innerWidth - estimatedWidth - margin);
  const toolbarBottom =
    (document.querySelector('.manage-meo-markdown-editor .mode-toolbar') as HTMLElement | null)?.getBoundingClientRect()
      .bottom ?? 0;
  const aboveTop = anchorY - margin - estimatedHeight;
  const isBelow = aboveTop < toolbarBottom + margin;
  return {
    isBelow,
    left: Math.min(maxLeft, Math.max(margin, rawLeft)),
    top: Math.max(margin, isBelow ? anchorBottomY + margin : anchorY - margin),
  };
}

export function renderManageQuickLabelIcon(labelId: ManageQuickLabelId): ReactNode {
  switch (labelId) {
    case 'clarify':
      return <IconHelpCircle aria-hidden='true' size={15} />;
    case 'needs-tests':
      return <IconTestPipe aria-hidden='true' size={15} />;
    case 'looks-good':
      return <IconCircleCheck aria-hidden='true' size={15} />;
  }
}

export function normalizeAnnotationQuote(text: string): string {
  return text.replace(/\s+/g, ' ').trim().slice(0, MANAGE_SELECTION_MAX_LENGTH);
}

export function selectionAnchorFromRect(rect: DOMRect | undefined): ManageSelectionAnchor | undefined {
  if (!rect || rect.width === 0 || rect.height === 0) {
    return undefined;
  }
  const left = Math.min(Math.max(rect.left + rect.width / 2, 12), window.innerWidth - 12);
  const top = Math.min(Math.max(rect.top, 12), window.innerHeight - 12);
  return { left, top };
}

export function defaultManageSelectionAnchor(): ManageSelectionAnchor {
  return {
    left: Math.min(Math.max(window.innerWidth / 2, 12), window.innerWidth - 12),
    top: Math.min(Math.max(72, 12), window.innerHeight - 12),
  };
}

export function annotationPreviewText(annotation: ManageAnnotation): string {
  const note = annotationDisplayNote(annotation);
  if (note) {
    return truncateManageAnnotationPreviewText(note);
  }
  if (annotation.labelId) {
    return quickLabelText(annotation.labelId);
  }
  if (annotation.type === 'redline') {
    return 'Marked for deletion';
  }
  return truncateManageAnnotationPreviewText(annotation.quote);
}

export function truncateManageAnnotationPreviewText(text: string): string {
  const normalized = normalizeAnnotationQuote(text);
  return normalized.length > 150 ? `${normalized.slice(0, 147)}...` : normalized;
}

export function annotationPreviewCardStyle(anchor: ManageSelectionAnchor): CSSProperties {
  const width = Math.min(320, Math.max(240, window.innerWidth - 24));
  const halfWidth = width / 2;
  return {
    left: Math.min(Math.max(anchor.left, 12 + halfWidth), window.innerWidth - 12 - halfWidth),
    top: Math.max(12, anchor.top - 96),
    width,
  };
}

export function commentPopoverStyle(anchor: ManageSelectionAnchor): CSSProperties {
  const width = Math.min(360, Math.max(280, window.innerWidth - 24));
  const left = Math.min(Math.max(anchor.left - width / 2, 12), window.innerWidth - width - 12);
  const maxTop = Math.max(12, window.innerHeight - 260);
  const top = Math.min(Math.max(anchor.top + 12, 12), maxTop);
  return {
    left,
    top,
    width,
  };
}

export function normalizeAttachmentName(name: string): string {
  const trimmed = name.trim().replace(/\s+/g, '-');
  return trimmed ? trimmed.slice(0, 80) : 'image';
}

export function parseManageAnnotationStore(content: string): Record<string, ManageAnnotation[]> {
  if (!content.trim()) {
    return {};
  }
  try {
    const value = JSON.parse(content) as unknown;
    if (!isRecord(value)) {
      return {};
    }
    const annotationsValue = value.annotationsByPath;
    if (!isRecord(annotationsValue)) {
      return {};
    }
    const normalized: Record<string, ManageAnnotation[]> = {};
    for (const [path, annotations] of Object.entries(annotationsValue)) {
      const normalizedPath = normalizeStoredAnnotationPath(path);
      if (!normalizedPath || !Array.isArray(annotations)) {
        continue;
      }
      const normalizedAnnotations = annotations
        .map((annotation) => normalizeStoredAnnotation(annotation))
        .filter((annotation): annotation is ManageAnnotation => Boolean(annotation));
      if (normalizedAnnotations.length > 0) {
        normalized[normalizedPath] = normalizedAnnotations;
      }
    }
    return normalized;
  } catch {
    return {};
  }
}

export function serializeManageAnnotationStore(annotationsByPath: Record<string, ManageAnnotation[]>): string {
  const store: ManageAnnotationStore = {
    annotationsByPath,
    updatedAt: new Date().toISOString(),
    version: MANAGE_ANNOTATION_SCHEMA_VERSION,
  };
  return `${JSON.stringify(store, null, 2)}\n`;
}

export function stableManageAnnotationStoreKey(annotationsByPath: Record<string, ManageAnnotation[]>): string {
  return JSON.stringify(annotationsByPath);
}

export function normalizeStoredAnnotationPath(path: string): string {
  const trimmed = path.trim();
  if (!trimmed || trimmed.startsWith('/') || trimmed.includes('\0')) {
    return '';
  }
  const components = trimmed.split('/').filter(Boolean);
  if (components.includes('.') || components.includes('..')) {
    return '';
  }
  return components.join('/');
}

export function normalizeStoredAnnotation(value: unknown): ManageAnnotation | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const type = value.type === 'redline' ? 'redline' : value.type === 'comment' ? 'comment' : undefined;
  if (!type) {
    return undefined;
  }
  const quote = typeof value.quote === 'string' ? normalizeAnnotationQuote(value.quote) : '';
  const note = typeof value.note === 'string' ? value.note.slice(0, 4_000) : '';
  const attachments = Array.isArray(value.attachments)
    ? value.attachments
        .map((attachment) => normalizeStoredAttachment(attachment))
        .filter((attachment): attachment is ManageAnnotationImage => Boolean(attachment))
        .slice(0, MANAGE_ANNOTATION_MAX_IMAGES)
    : [];
  if (type === 'redline' && !quote) {
    return undefined;
  }
  if (type === 'comment' && !quote && !note.trim() && attachments.length === 0) {
    return undefined;
  }
  const labelId = normalizeQuickLabelId(value.labelId);
  return {
    attachments,
    createdAt: typeof value.createdAt === 'string' ? value.createdAt : new Date().toISOString(),
    id: typeof value.id === 'string' && value.id.trim() ? value.id : `manage-annotation-${Date.now()}`,
    ...(labelId ? { labelId } : {}),
    note,
    quote,
    scope: quote ? 'selection' : 'global',
    type,
  };
}

export function normalizeStoredAttachment(value: unknown): ManageAnnotationImage | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const dataUrl = typeof value.dataUrl === 'string' ? value.dataUrl : '';
  const mimeType = typeof value.mimeType === 'string' ? value.mimeType : '';
  const name = typeof value.name === 'string' ? normalizeAttachmentName(value.name) : 'image';
  const size = typeof value.size === 'number' && Number.isFinite(value.size) ? Math.max(0, value.size) : 0;
  if (
    !dataUrl.startsWith('data:image/') ||
    !mimeType.startsWith('image/') ||
    size > MANAGE_ANNOTATION_IMAGE_MAX_BYTES
  ) {
    return undefined;
  }
  return {
    dataUrl,
    id: typeof value.id === 'string' && value.id.trim() ? value.id : `manage-annotation-image-${Date.now()}`,
    mimeType,
    name,
    size,
  };
}

export function normalizeQuickLabelId(value: unknown): ManageQuickLabelId | undefined {
  return MANAGE_QUICK_LABELS.some((label) => label.id === value) ? (value as ManageQuickLabelId) : undefined;
}

export function formatManageAnnotationsAsMarkdown(path: string, annotations: ManageAnnotation[]): string {
  if (annotations.length === 0) {
    return `# Docs Markdown Feedback\n\nFile: \`${path}\`\n\nNo annotations.\n`;
  }
  const lines = ['# Docs Markdown Feedback', '', `File: \`${path}\``, ''];
  const redlines = annotations.filter((annotation) => annotation.type === 'redline');
  const comments = annotations.filter((annotation) => annotation.type === 'comment');
  if (redlines.length > 0) {
    lines.push('## Redlines', '');
    for (const annotation of redlines) {
      lines.push(`- Delete: ${formatMarkdownQuote(annotation.quote)}`);
      appendAnnotationDetails(lines, annotation);
    }
    lines.push('');
  }
  if (comments.length > 0) {
    lines.push('## Comments', '');
    for (const annotation of comments) {
      const prefix = annotation.scope === 'global' ? 'Global' : `On ${formatMarkdownQuote(annotation.quote)}`;
      const body =
        annotation.note.trim() || (annotation.labelId ? quickLabelText(annotation.labelId) : '(attachment only)');
      lines.push(`- ${prefix}: ${body}`);
      appendAnnotationDetails(lines, annotation);
    }
  }
  return `${lines.join('\n').trimEnd()}\n`;
}

export function appendAnnotationDetails(lines: string[], annotation: ManageAnnotation): void {
  if (annotation.labelId) {
    lines.push(`  - Label: ${quickLabelText(annotation.labelId)}`);
  }
  if (annotation.type === 'redline' && annotation.note.trim()) {
    lines.push(`  - Note: ${annotation.note.trim()}`);
  }
  if (annotation.attachments.length > 0) {
    lines.push('  - Attachments:');
    for (const attachment of annotation.attachments) {
      lines.push(`    - ${attachment.name}: ${attachment.dataUrl}`);
    }
  }
}

export function formatMarkdownQuote(text: string): string {
  return `"${text.replace(/"/gu, '\\"')}"`;
}

export async function writeTextToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.style.cssText = 'position:fixed;left:-9999px;top:-9999px';
    document.body.append(textarea);
    textarea.select();
    const didCopy = document.execCommand('copy');
    textarea.remove();
    if (!didCopy) {
      throw new Error('Clipboard copy failed.');
    }
  }
}
