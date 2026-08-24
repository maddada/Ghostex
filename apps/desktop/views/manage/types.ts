import { type BinaryFiles } from '@excalidraw/excalidraw/types';
import { type ButtonHTMLAttributes, type ReactNode } from 'react';
import { EditorView } from '@codemirror/view';
import { type ExcalidrawElement } from '@excalidraw/excalidraw/element/types';
import {
  type ProjectDocsFileEntry as ManageFileEntry,
  type ProjectDocsGitBaseline as ManageGitBaseline,
  type ProjectDocsRequest as ManageFilesBridgeRequest,
} from '@/packages/shared/project-docs';
import { MANAGE_DOCS_ROOT_PATH } from './constants';

export type ManageAnnotationType = 'comment' | 'redline';

export type ManageAnnotationScope = 'global' | 'selection';

export type ManageQuickLabelId = 'clarify' | 'needs-tests' | 'looks-good';

export type ManageQuickLabel = {
  color: string;
  id: ManageQuickLabelId;
  text: string;
};

export type ManageTooltipButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  tooltip: ReactNode;
};

export type ManageAnnotationImage = {
  dataUrl: string;
  id: string;
  mimeType: string;
  name: string;
  size: number;
};

export type ManageAnnotation = {
  attachments: ManageAnnotationImage[];
  createdAt: string;
  id: string;
  labelId?: ManageQuickLabelId;
  note: string;
  quote: string;
  scope: ManageAnnotationScope;
  type: ManageAnnotationType;
};

export type ManageAnnotationStore = {
  annotationsByPath: Record<string, ManageAnnotation[]>;
  updatedAt: string;
  version: 1;
};

export type ManageSelectionAnchor = {
  left: number;
  top: number;
};

export type ManageCapturedSelection = {
  anchor: ManageSelectionAnchor;
  text: string;
};

export type ManageAnnotationPreview = {
  anchor: ManageSelectionAnchor;
  annotation: ManageAnnotation;
};

export type ManageCommentDraft = {
  anchor: ManageSelectionAnchor;
  attachmentError: string;
  attachments: ManageAnnotationImage[];
  note: string;
  quote: string;
};

export type ManageSidebarSide = 'left' | 'right';

export type ManageArtifactKind = 'excalidraw' | 'html' | 'markdown';

export type ManageFileContextMenuState = {
  confirmingDelete?: boolean;
  path: string;
  x: number;
  y: number;
};

export type ManageFileOperationState = {
  action:
    | 'addToSessionContext'
    | 'copyFullPath'
    | 'createFile'
    | 'createFolder'
    | 'delete'
    | 'duplicate'
    | 'move'
    | 'rename'
    | 'revealInFinder';
  path: string;
};

export type ManageDragState = {
  kind: ManageFileEntry['kind'];
  path: string;
};

export type ManageDropTarget =
  | {
      kind: 'entry';
      path: string;
      targetDirectoryPath: string;
    }
  | {
      kind: 'root';
      path: typeof MANAGE_DOCS_ROOT_PATH;
    };

export type ManageRenameDialogState = {
  error?: string;
  path: string;
  value: string;
};

export type ManageMarkdownAlertKind = 'caution' | 'important' | 'note' | 'tip' | 'warning';

export type ManageMarkdownBlock = {
  alertKind?: ManageMarkdownAlertKind;
  checked?: boolean;
  content: string;
  directiveKind?: string;
  id: string;
  language?: string;
  level?: number;
  order: number;
  ordered?: boolean;
  orderedIndex?: number;
  orderedStart?: number;
  startLine: number;
  type: 'blockquote' | 'code' | 'directive' | 'heading' | 'hr' | 'html' | 'list-item' | 'paragraph' | 'table';
};

export type ManageMeoEditor = {
  countMatches?: (query: string, options?: { caseSensitive?: boolean; wholeWord?: boolean }) => number;
  destroy: () => void;
  findNext?: (
    query: string,
    options?: { caseSensitive?: boolean; focusEditor?: boolean; wholeWord?: boolean }
  ) => { current?: number; found?: boolean; total?: number } | null;
  findPrevious?: (
    query: string,
    options?: { caseSensitive?: boolean; focusEditor?: boolean; wholeWord?: boolean }
  ) => { current?: number; found?: boolean; total?: number } | null;
  focus: () => void;
  getText: () => string;
  insertFormat: (action: string, level?: number | { cols?: number; rows?: number }) => void;
  refreshLayout?: () => void;
  replaceAll?: (
    query: string,
    replacement: string,
    options?: { caseSensitive?: boolean; wholeWord?: boolean }
  ) => { replaced?: number; total?: number };
  replaceCurrent?: (
    query: string,
    replacement: string,
    options?: { caseSensitive?: boolean; wholeWord?: boolean }
  ) => { current?: number; found?: boolean; replaced?: boolean; total?: number };
  setGitBaseline?: (snapshot?: ManageGitBaseline | null) => void;
  setGitGutterVisible?: (visible: boolean) => void;
  setLineNumbers?: (visible: boolean) => void;
  setMode?: (mode: ManageMeoMode) => void;
  setSearchQuery?: (query: string, options?: { caseSensitive?: boolean; wholeWord?: boolean }) => void;
  setText: (text: string) => void;
  view: EditorView;
};

export type ManageMeoMode = 'live' | 'source';

export type ManageMeoSelectionState = {
  align?: 'center' | 'start';
  anchorBottomY?: number;
  anchorX?: number;
  anchorY?: number;
  from?: number;
  to?: number;
  visible?: boolean;
};

export type ManageSelectionToolbarMode = 'annotations' | 'formatting';

export type ManageMeoAnnotationDecoration = {
  from: number;
  labelId?: ManageQuickLabelId;
  to: number;
  type: ManageAnnotationType;
};

export type ManageResolvedAnnotationRange = ManageMeoAnnotationDecoration & {
  annotation: ManageAnnotation;
};

export type ManageWebKitWindow = Window & {
  ghostexGpui?: {
    manageDocsResourceBaseUrl?: string;
    postManageFilesRequest?: (payload: string) => boolean;
    supportsManageFileChangePolling?: boolean;
  };
  webkit?: {
    messageHandlers?: {
      ghostexManageFiles?: {
        postMessage: (message: ManageFilesBridgeRequest) => void;
      };
    };
  };
};

export type ExcalidrawFileData = {
  appState?: Record<string, unknown>;
  elements?: readonly ExcalidrawElement[];
  files?: BinaryFiles;
  source?: string;
  type?: string;
  version?: number;
};

export type ManageDocsOpenFileWindow = Window & {
  ghostexOpenDocsFile?: (path: unknown) => void;
};

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
