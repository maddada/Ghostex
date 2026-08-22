import {
  IconAlertTriangle,
  IconCheck,
  IconCopy,
  IconEdit,
  IconFileText,
  IconMessagePlus,
  IconMessages,
  IconRefresh,
  IconX,
} from "@tabler/icons-react";
import { type ProjectDocsFilePreview as ManageFilePreview } from "@/packages/shared/project-docs";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { MANAGE_ANNOTATION_IMAGE_MAX_BYTES, MANAGE_ANNOTATION_MAX_IMAGES, MANAGE_QUICK_LABELS } from "../constants";
import { ManageAnnotation, ManageAnnotationImage, ManageAnnotationPreview, ManageAnnotationType, ManageCapturedSelection, ManageCommentDraft, ManageQuickLabel, ManageQuickLabelId, ManageSelectionAnchor, ManageSelectionToolbarMode } from "../types";
import { ManageAnnotationDropdown, ManageAnnotationPreviewCard, ManageAnnotationToolbar, ManageCommentPopover } from "./annotation-overlays";
import { ManageExcalidrawEditor } from "./excalidraw-editor";
import { ManageHtmlRenderViewer } from "./html-viewer";
import { ManageMarkdownReviewViewer } from "./markdown-review-viewer";
import { ManagePreviewMessage, isEditableEventTarget } from "./preview-shared";
import { ManageTextEditor } from "./text-editor";
import { ManageTooltipButton } from "../manage-tooltip-button";
import { annotationPersistenceLabel, defaultManageSelectionAnchor, formatManageAnnotationsAsMarkdown, normalizeAnnotationQuote, normalizeAttachmentName, selectionAnchorFromRect, writeTextToClipboard } from "../annotation-store";
import { formatFileSize, isExcalidrawPath, isHtmlPath, isMarkdownPath, languageLabelForPath } from "../file-tree-utils";

export function ManagePreview({
  annotations,
  annotationPersistenceState,
  draftContent,
  error,
  hasExternalChanges,
  isDirty,
  onAnnotationsChange,
  onDraftContentChange,
  onOpenDocument,
  onReload,
  preview,
  previewState,
  saveState,
  selectedPath,
}: {
  annotations: ManageAnnotation[];
  annotationPersistenceState: "idle" | "loading" | "ready" | "saving" | "saved" | "error";
  draftContent: string;
  error?: string;
  hasExternalChanges: boolean;
  isDirty: boolean;
  onAnnotationsChange: (updater: (annotations: ManageAnnotation[]) => ManageAnnotation[]) => void;
  onDraftContentChange: (content: string) => void;
  onOpenDocument: (path: string) => void;
  onReload: () => void;
  preview?: ManageFilePreview;
  previewState: "idle" | "loading" | "ready" | "error";
  saveState: "idle" | "saving" | "saved" | "error";
  selectedPath?: string;
}) {
  const [selection, setSelection] = useState<ManageCapturedSelection>();
  const [selectionToolbarMode, setSelectionToolbarMode] = useState<ManageSelectionToolbarMode>("annotations");
  const [commentDraft, setCommentDraft] = useState<ManageCommentDraft>();
  const [annotationPreview, setAnnotationPreview] = useState<ManageAnnotationPreview>();
  const [feedbackCopyState, setFeedbackCopyState] = useState<"idle" | "copied" | "error">("idle");
  const [clearAnnotationsConfirming, setClearAnnotationsConfirming] = useState(false);
  const [annotationsDropdownOpen, setAnnotationsDropdownOpen] = useState(false);
  const [htmlAnnotationEnabled, setHtmlAnnotationEnabled] = useState(true);
  const annotationsDropdownRef = useRef<HTMLDivElement | null>(null);
  const clearAnnotationsTimerRef = useRef<number | undefined>(undefined);
  const selectedPathRef = useRef<string | undefined>(selectedPath);

  const resetClearAnnotationsConfirm = useCallback(() => {
    if (clearAnnotationsTimerRef.current !== undefined) {
      window.clearTimeout(clearAnnotationsTimerRef.current);
      clearAnnotationsTimerRef.current = undefined;
    }
    setClearAnnotationsConfirming(false);
  }, []);

  useEffect(() => {
    if (selectedPathRef.current !== selectedPath) {
      selectedPathRef.current = selectedPath;
      setSelection(undefined);
      setSelectionToolbarMode("annotations");
      setCommentDraft(undefined);
      setAnnotationPreview(undefined);
      setFeedbackCopyState("idle");
      resetClearAnnotationsConfirm();
      setAnnotationsDropdownOpen(false);
    }
  }, [resetClearAnnotationsConfirm, selectedPath]);

  useEffect(() => {
    if (annotations.length === 0) {
      resetClearAnnotationsConfirm();
    }
  }, [annotations.length, resetClearAnnotationsConfirm]);

  useEffect(
    () => () => {
      if (clearAnnotationsTimerRef.current !== undefined) {
        window.clearTimeout(clearAnnotationsTimerRef.current);
      }
    },
    [],
  );

  useEffect(() => {
    if (!annotationsDropdownOpen) {
      return;
    }
    function handlePointerDown(event: PointerEvent) {
      const dropdownElement = annotationsDropdownRef.current;
      if (!dropdownElement || !event.target || dropdownElement.contains(event.target as Node)) {
        return;
      }
      setAnnotationsDropdownOpen(false);
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setAnnotationsDropdownOpen(false);
      }
    }
    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [annotationsDropdownOpen]);

  const addAnnotation = useCallback(
    ({
      attachments = [],
      labelId,
      note = "",
      quote = "",
      type,
    }: {
      attachments?: ManageAnnotationImage[];
      labelId?: ManageQuickLabelId;
      note?: string;
      quote?: string;
      type: ManageAnnotationType;
    }) => {
      const normalizedQuote = normalizeAnnotationQuote(quote);
      if (type === "redline" && !normalizedQuote) {
        return;
      }
      const normalizedNote = note.trim();
      if (type === "comment" && !normalizedQuote && !normalizedNote && attachments.length === 0) {
        return;
      }
      const nextAnnotation: ManageAnnotation = {
        attachments,
        createdAt: new Date().toISOString(),
        id: `manage-annotation-${Date.now()}-${Math.random().toString(16).slice(2)}`,
        labelId,
        note: normalizedNote,
        quote: normalizedQuote,
        scope: normalizedQuote ? "selection" : "global",
        type,
      };
      onAnnotationsChange((current) => [...current, nextAnnotation]);
      setSelection(undefined);
      setSelectionToolbarMode("annotations");
      setCommentDraft(undefined);
    },
    [onAnnotationsChange],
  );

  const captureSelectedText = useCallback((capturedSelection: ManageCapturedSelection) => {
    const normalized = normalizeAnnotationQuote(capturedSelection.text);
    if (!normalized) {
      return;
    }
    setAnnotationPreview(undefined);
    setCommentDraft(undefined);
    setSelectionToolbarMode("annotations");
    setSelection({
      anchor: capturedSelection.anchor,
      text: normalized,
    });
  }, []);

  const clearSelectedText = useCallback(() => {
    setSelection(undefined);
    setSelectionToolbarMode("annotations");
  }, []);

  const openCommentDraft = useCallback(
    (quote: string, anchor: ManageSelectionAnchor, initialNote = "") => {
      setAnnotationPreview(undefined);
      setSelection(undefined);
      setSelectionToolbarMode("annotations");
      setCommentDraft({
        anchor,
        attachmentError: "",
        attachments: [],
        note: initialNote,
        quote: normalizeAnnotationQuote(quote),
      });
    },
    [],
  );

  const addSelectedRedline = useCallback(() => {
    if (!selection) {
      return;
    }
    addAnnotation({
      quote: selection.text,
      type: "redline",
    });
  }, [addAnnotation, selection]);

  const addQuickLabel = useCallback(
    (label: ManageQuickLabel) => {
      addAnnotation({
        labelId: label.id,
        note: "",
        quote: selection?.text ?? commentDraft?.quote ?? "",
        type: "comment",
      });
    },
    [addAnnotation, commentDraft?.quote, selection?.text],
  );

  const submitCommentDraft = useCallback(() => {
    if (!commentDraft) {
      return;
    }
    addAnnotation({
      attachments: commentDraft.attachments,
      note: commentDraft.note,
      quote: commentDraft.quote,
      type: "comment",
    });
  }, [addAnnotation, commentDraft]);

  const updateCommentDraftNote = useCallback((note: string) => {
    setCommentDraft((current) => (current ? { ...current, note } : current));
  }, []);

  const addAttachmentFiles = useCallback((files: FileList | File[]) => {
    const imageFiles = Array.from(files).filter((file) => file.type.startsWith("image/"));
    if (imageFiles.length === 0) {
      return;
    }
    setCommentDraft((current) => {
      if (!current) {
        return current;
      }
      const availableSlots = Math.max(0, MANAGE_ANNOTATION_MAX_IMAGES - current.attachments.length);
      if (availableSlots === 0) {
        return {
          ...current,
          attachmentError: `Use ${MANAGE_ANNOTATION_MAX_IMAGES} images or fewer per annotation.`,
        };
      }
      let attachmentError =
        imageFiles.length > availableSlots ? `Use ${MANAGE_ANNOTATION_MAX_IMAGES} images or fewer per annotation.` : "";
      for (const file of imageFiles.slice(0, availableSlots)) {
        if (file.size > MANAGE_ANNOTATION_IMAGE_MAX_BYTES) {
          attachmentError = "Images must be 512 KB or smaller.";
          continue;
        }
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = typeof reader.result === "string" ? reader.result : "";
          if (!dataUrl) {
            return;
          }
          setCommentDraft((latest) => {
            if (!latest || latest.attachments.length >= MANAGE_ANNOTATION_MAX_IMAGES) {
              return latest;
            }
            return {
              ...latest,
              attachmentError: "",
              attachments: [
                ...latest.attachments,
                {
                  dataUrl,
                  id: `manage-annotation-image-${Date.now()}-${Math.random().toString(16).slice(2)}`,
                  mimeType: file.type,
                  name: normalizeAttachmentName(file.name),
                  size: file.size,
                },
              ],
            };
          });
        };
        reader.onerror = () => {
          setCommentDraft((latest) =>
            latest ? { ...latest, attachmentError: "Could not read image attachment." } : latest,
          );
        };
        reader.readAsDataURL(file);
      }
      return {
        ...current,
        attachmentError,
      };
    });
  }, []);

  const removeDraftAttachment = useCallback((attachmentId: string) => {
    setCommentDraft((current) =>
      current
        ? {
            ...current,
            attachments: current.attachments.filter((attachment) => attachment.id !== attachmentId),
          }
        : current,
    );
  }, []);

  const copyFeedback = useCallback(async () => {
    if (!selectedPath) {
      return;
    }
    /*
     * CDXC:DocsRootAdditive 2026-08-10:
     * This markdown is read by a human and by the agent it is pasted to, so it
     * names the file the way the tree does rather than by routing address.
     */
    const output = formatManageAnnotationsAsMarkdown(preview?.displayPath ?? selectedPath, annotations);
    try {
      await writeTextToClipboard(output);
      setFeedbackCopyState("copied");
      window.setTimeout(() => setFeedbackCopyState("idle"), 1_600);
    } catch {
      setFeedbackCopyState("error");
    }
  }, [annotations, preview?.displayPath, selectedPath]);

  const clearAllAnnotations = useCallback(() => {
    if (annotations.length === 0) {
      resetClearAnnotationsConfirm();
      return;
    }
    if (!clearAnnotationsConfirming) {
      setClearAnnotationsConfirming(true);
      if (clearAnnotationsTimerRef.current !== undefined) {
        window.clearTimeout(clearAnnotationsTimerRef.current);
      }
      clearAnnotationsTimerRef.current = window.setTimeout(() => {
        clearAnnotationsTimerRef.current = undefined;
        setClearAnnotationsConfirming(false);
      }, 3_000);
      return;
    }
    resetClearAnnotationsConfirm();
    setAnnotationsDropdownOpen(false);
    onAnnotationsChange(() => []);
  }, [annotations.length, clearAnnotationsConfirming, onAnnotationsChange, resetClearAnnotationsConfirm]);

  const openCommentForSelection = useCallback(() => {
    if (!selection) {
      return;
    }
    openCommentDraft(selection.text, selection.anchor);
  }, [openCommentDraft, selection]);

  const openGlobalComment = useCallback(
    (anchor: ManageSelectionAnchor) => {
      openCommentDraft("", anchor);
    },
    [openCommentDraft],
  );

  useEffect(() => {
    if (!selection || commentDraft) {
      return;
    }
    const activeSelection = selection;
    function handleAnnotationShortcut(event: KeyboardEvent) {
      if (event.isComposing || event.metaKey || event.ctrlKey || event.altKey || isEditableEventTarget(event.target)) {
        return;
      }
      const key = event.key.toLocaleLowerCase();
      if (key === "escape") {
        event.preventDefault();
        setSelection(undefined);
        return;
      }
      if (key === "backspace" || key === "d" || key === "delete") {
        event.preventDefault();
        addSelectedRedline();
        return;
      }
      if (key === "c") {
        event.preventDefault();
        openCommentForSelection();
        return;
      }
      if (/^[1-3]$/u.test(key)) {
        event.preventDefault();
        const label = MANAGE_QUICK_LABELS[Number(key) - 1];
        if (label) {
          addQuickLabel(label);
        }
        return;
      }
      if (event.key.length === 1) {
        event.preventDefault();
        openCommentDraft(activeSelection.text, activeSelection.anchor, event.key);
      }
    }
    window.addEventListener("keydown", handleAnnotationShortcut);
    return () => window.removeEventListener("keydown", handleAnnotationShortcut);
  }, [addQuickLabel, addSelectedRedline, commentDraft, openCommentDraft, openCommentForSelection, selection]);

  const removeAnnotation = useCallback(
    (annotationId: string) => {
      onAnnotationsChange((current) => current.filter((annotation) => annotation.id !== annotationId));
    },
    [onAnnotationsChange],
  );

  const removePreviewAnnotation = useCallback(
    (annotationId: string) => {
      removeAnnotation(annotationId);
      setAnnotationPreview(undefined);
    },
    [removeAnnotation],
  );

  if (previewState === "loading") {
    return <ManagePreviewMessage icon={<IconRefresh aria-hidden="true" size={20} />} title="Loading file" />;
  }
  if (error) {
    return (
      <ManagePreviewMessage
        icon={<IconAlertTriangle aria-hidden="true" size={21} />}
        title={error}
      />
    );
  }
  if (!selectedPath || !preview) {
    return <ManagePreviewMessage icon={<IconFileText aria-hidden="true" size={21} />} title="Select a file" />;
  }

  const language = languageLabelForPath(preview.path);
  const isMarkdown = isMarkdownPath(preview.path);
  const isDrawing = isExcalidrawPath(preview.path);
  const isHtml = isHtmlPath(preview.path);
  const usesCompactArtifactHeader = isMarkdown || isDrawing || isHtml;
  /*
   * CDXC:DocsRootAdditive 2026-08-09:
   * Show the file the way the tree names it. `preview.path` is a routing
   * address that starts with the reserved mount segment for anything under a
   * configured Docs directory, which is not a name any human asked for.
   */
  const previewDisplayPath = preview.displayPath ?? preview.path;
  const previewTitle = usesCompactArtifactHeader ? previewDisplayPath : preview.name;
  const annotationPersistenceTitle = annotationPersistenceLabel(annotationPersistenceState);

  return (
    <div
      className="manage-preview-content"
      data-compact-header={String(usesCompactArtifactHeader)}
      data-kind={isMarkdown ? "markdown" : isDrawing ? "drawing" : isHtml ? "html" : "text"}
    >
      <header className="manage-preview-header">
        <div className="manage-preview-title">
          {isDrawing ? (
            <IconEdit aria-hidden="true" size={17} stroke={1.85} />
          ) : (
            <IconFileText aria-hidden="true" size={17} stroke={1.85} />
          )}
          <span>{previewTitle}</span>
        </div>
        <div className="manage-preview-meta">
          <span>{language}</span>
          {preview.size !== undefined ? <span>{formatFileSize(preview.size)}</span> : null}
          {isDirty ? <span>Edited</span> : saveState === "saved" ? <span>Saved</span> : null}
        </div>
        {isMarkdown ? (
          <div className="manage-preview-header-actions">
            <ManageTooltipButton
              aria-label="Add global comment"
              onClick={(event) =>
                openGlobalComment(
                  selectionAnchorFromRect(event.currentTarget.getBoundingClientRect()) ?? defaultManageSelectionAnchor(),
                )
              }
              tooltip="Add global comment"
              type="button"
            >
              <IconMessagePlus aria-hidden="true" size={14} />
              <span>Comment</span>
            </ManageTooltipButton>
            <ManageTooltipButton
              aria-label="Copy feedback"
              disabled={annotations.length === 0}
              onClick={() => void copyFeedback()}
              tooltip="Copy feedback"
              type="button"
            >
              {feedbackCopyState === "copied" ? (
                <IconCheck aria-hidden="true" size={14} />
              ) : (
                <IconCopy aria-hidden="true" size={14} />
              )}
              <span>{feedbackCopyState === "copied" ? "Copied" : "Copy"}</span>
            </ManageTooltipButton>
            <ManageTooltipButton
              aria-label="Clear all annotations"
              className="manage-clear-annotations-button"
              data-confirming={String(clearAnnotationsConfirming)}
              disabled={annotations.length === 0}
              onClick={clearAllAnnotations}
              tooltip="Clear All Annotations"
              type="button"
            >
              {/*
                CDXC:DocsAnnotationToolbar 2026-06-30-04:55:
                The Markdown feedback toolbar's Clear action should use an X icon instead of a trash can because it clears review annotations rather than deleting a file.
              */}
              <IconX aria-hidden="true" size={14} />
              <span>{clearAnnotationsConfirming ? "Confirm" : "Clear"}</span>
            </ManageTooltipButton>
            <div className="manage-annotation-dropdown-shell" ref={annotationsDropdownRef}>
              <ManageTooltipButton
                aria-controls="manage-markdown-annotation-dropdown"
                aria-expanded={annotationsDropdownOpen}
                aria-haspopup="dialog"
                aria-label="Show annotations"
                className="manage-annotation-dropdown-trigger"
                onClick={() => setAnnotationsDropdownOpen((current) => !current)}
                tooltip={`Annotations (${annotations.length}) · ${annotationPersistenceTitle}`}
                type="button"
              >
                <IconMessages aria-hidden="true" size={14} />
                <span className="manage-count-badge">{annotations.length}</span>
              </ManageTooltipButton>
              {annotationsDropdownOpen ? (
                <ManageAnnotationDropdown annotations={annotations} onRemoveAnnotation={removeAnnotation} />
              ) : null}
            </div>
            <ManageTooltipButton
              aria-label={hasExternalChanges ? "Reload file with new changes" : "Reload file"}
              className="manage-file-reload-button"
              data-changes-available={String(hasExternalChanges)}
              onClick={onReload}
              tooltip={hasExternalChanges ? "Reload to show new changes" : "Reload file"}
              type="button"
            >
              <IconRefresh aria-hidden="true" size={14} />
              {hasExternalChanges ? <span aria-hidden="true" className="manage-file-change-indicator" /> : null}
            </ManageTooltipButton>
          </div>
        ) : isHtml ? (
          <div className="manage-preview-header-actions">
            <ManageTooltipButton
              aria-label="Toggle annotations"
              aria-pressed={htmlAnnotationEnabled}
              className="manage-annotation-toggle"
              onClick={() => setHtmlAnnotationEnabled((current) => !current)}
              tooltip={htmlAnnotationEnabled ? "Disable annotations" : "Enable annotations"}
              type="button"
            >
              <IconMessagePlus aria-hidden="true" size={14} />
              <span>Annotate</span>
            </ManageTooltipButton>
            <ManageTooltipButton
              aria-label="Reload HTML file"
              className="manage-file-reload-button"
              onClick={onReload}
              tooltip="Reload HTML file"
              type="button"
            >
              <IconRefresh aria-hidden="true" size={14} />
            </ManageTooltipButton>
          </div>
        ) : null}
      </header>
      {!usesCompactArtifactHeader ? (
        <div className="manage-preview-path">{previewDisplayPath}</div>
      ) : null}
      {preview.kind === "unsupported" ? (
        <ManagePreviewMessage
          icon={<IconAlertTriangle aria-hidden="true" size={21} />}
          title={preview.error ?? "Preview unavailable"}
        />
      ) : isDrawing ? (
        <ManageExcalidrawEditor
          content={draftContent}
          fileName={preview.name}
          key={preview.path}
          onChange={onDraftContentChange}
        />
      ) : isHtml ? (
        <ManageHtmlRenderViewer
          annotationsEnabled={htmlAnnotationEnabled}
          content={draftContent}
          documentKey={preview.path}
          onOpenDocument={onOpenDocument}
        />
      ) : isMarkdown ? (
        <>
          <ManageMarkdownReviewViewer
            annotations={annotations}
            content={draftContent}
            documentKey={preview.path}
            gitBaseline={preview.gitBaseline}
            onContentChange={onDraftContentChange}
            onAnnotationPreviewChange={setAnnotationPreview}
            onSelectionClear={clearSelectedText}
            onSelectionCapture={captureSelectedText}
            onSelectionToolbarModeChange={setSelectionToolbarMode}
            selection={selection}
            selectionToolbarMode={selectionToolbarMode}
          />
          {selection && selectionToolbarMode === "annotations" ? (
            <ManageAnnotationToolbar
              anchor={selection.anchor}
              onComment={openCommentForSelection}
              onDismiss={() => {
                setSelectionToolbarMode("annotations");
                setSelection(undefined);
              }}
              onFormatting={() => setSelectionToolbarMode("formatting")}
              onQuickLabel={addQuickLabel}
            />
          ) : null}
          {commentDraft ? (
            <ManageCommentPopover
              draft={commentDraft}
              onAddAttachmentFiles={addAttachmentFiles}
              onCancel={() => setCommentDraft(undefined)}
              onDraftNoteChange={updateCommentDraftNote}
              onRemoveDraftAttachment={removeDraftAttachment}
              onSubmit={submitCommentDraft}
            />
          ) : null}
          {annotationPreview && !selection && !commentDraft ? (
            <ManageAnnotationPreviewCard onRemoveAnnotation={removePreviewAnnotation} preview={annotationPreview} />
          ) : null}
        </>
      ) : (
        <ManageTextEditor
          content={draftContent}
          language={language}
          onChange={onDraftContentChange}
        />
      )}
    </div>
  );
}
