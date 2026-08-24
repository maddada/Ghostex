import { type CSSProperties, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { EditorView } from '@codemirror/view';
import { type Extension } from '@codemirror/state';
import { type ProjectDocsGitBaseline as ManageGitBaseline } from '@/packages/shared/project-docs';
import { MANAGE_MEO_CONTENT_MAX_WIDTH } from '../constants';
import {
  ManageAnnotation,
  ManageAnnotationPreview,
  ManageCapturedSelection,
  ManageMeoEditor,
  ManageMeoMode,
  ManageMeoSelectionState,
  ManageSelectionToolbarMode,
} from '../types';
import {
  ManageMeoSelectionFormatToolbar,
  ManageMeoTopToolbar,
  applyManageMeoTheme,
  createManageMeoAnnotationDecorations,
  manageMeoAnnotationEffect,
  manageMeoAnnotationField,
  syncManageMeoAnnotationReviewState,
} from '../meo-toolbar';
import { sanitizeManageHref } from '../html-sanitize';
import { createEditor as createMeoEditor } from '../../meo/editor';
import '../../meo/styles.css';

export function ManageMarkdownReviewViewer({
  annotations,
  content,
  documentKey,
  gitBaseline,
  onAnnotationPreviewChange,
  onContentChange,
  onSelectionClear,
  onSelectionCapture,
  onSelectionToolbarModeChange,
  selection,
  selectionToolbarMode,
}: {
  annotations: ManageAnnotation[];
  content: string;
  documentKey: string;
  gitBaseline?: ManageGitBaseline;
  onAnnotationPreviewChange: (preview: ManageAnnotationPreview | undefined) => void;
  onContentChange: (content: string) => void;
  onSelectionClear: () => void;
  onSelectionCapture: (selection: ManageCapturedSelection) => void;
  onSelectionToolbarModeChange: (mode: ManageSelectionToolbarMode) => void;
  selection?: ManageCapturedSelection;
  selectionToolbarMode: ManageSelectionToolbarMode;
}) {
  const editorHostRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<ManageMeoEditor | null>(null);
  const latestContentRef = useRef(content);
  const annotationsRef = useRef(annotations);
  const [contentMaxWidthEnabled, setContentMaxWidthEnabled] = useState(false);
  const [currentMode, setCurrentMode] = useState<ManageMeoMode>('live');
  const [findCaseSensitive, setFindCaseSensitive] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState('');
  const [findReplacement, setFindReplacement] = useState('');
  const [findStatus, setFindStatus] = useState('');
  const [findStatusIsError, setFindStatusIsError] = useState(false);
  const [findWholeWord, setFindWholeWord] = useState(false);
  const [gitGutterVisible, setGitGutterVisible] = useState(true);
  const [lineNumbersVisible, setLineNumbersVisible] = useState(true);
  const [meoSelectionState, setMeoSelectionState] = useState<ManageMeoSelectionState>({ visible: false });
  const onAnnotationPreviewChangeRef = useRef(onAnnotationPreviewChange);
  const onContentChangeRef = useRef(onContentChange);
  const onSelectionClearRef = useRef(onSelectionClear);
  const onSelectionCaptureRef = useRef(onSelectionCapture);

  useEffect(() => {
    annotationsRef.current = annotations;
  }, [annotations]);

  useEffect(() => {
    onAnnotationPreviewChangeRef.current = onAnnotationPreviewChange;
  }, [onAnnotationPreviewChange]);

  useEffect(() => {
    onContentChangeRef.current = onContentChange;
  }, [onContentChange]);

  useEffect(() => {
    onSelectionClearRef.current = onSelectionClear;
  }, [onSelectionClear]);

  useEffect(() => {
    onSelectionCaptureRef.current = onSelectionCapture;
  }, [onSelectionCapture]);

  const applyMeoFormat = useCallback((action: string, level?: number | { cols?: number; rows?: number }) => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    editor.insertFormat(action, level);
    editor.focus();
  }, []);

  const applyMeoMode = useCallback((mode: ManageMeoMode) => {
    const editor = editorRef.current;
    setCurrentMode(mode);
    editor?.setMode?.(mode);
    editor?.refreshLayout?.();
    editor?.focus();
  }, []);

  const toggleMeoLineNumbers = useCallback(() => {
    setLineNumbersVisible((current) => {
      const next = !current;
      editorRef.current?.setLineNumbers?.(next);
      editorRef.current?.refreshLayout?.();
      return next;
    });
  }, []);

  const toggleMeoGitGutter = useCallback(() => {
    setGitGutterVisible((current) => {
      const next = !current;
      editorRef.current?.setGitGutterVisible?.(next);
      editorRef.current?.refreshLayout?.();
      return next;
    });
  }, []);

  const toggleMeoContentMaxWidth = useCallback(() => {
    setContentMaxWidthEnabled((current) => {
      const next = !current;
      window.requestAnimationFrame(() => editorRef.current?.refreshLayout?.());
      return next;
    });
  }, []);

  const findOptions = useMemo(
    () => ({
      caseSensitive: findCaseSensitive,
      wholeWord: findWholeWord,
    }),
    [findCaseSensitive, findWholeWord]
  );

  const setFindStatusText = useCallback((text: string, isError = false) => {
    setFindStatus(text);
    setFindStatusIsError(isError);
  }, []);

  const updateFindStatusSummary = useCallback(() => {
    const editor = editorRef.current;
    if (!editor || !findOpen) {
      return;
    }
    editor.setSearchQuery?.(findQuery, findOptions);
    if (!findQuery) {
      setFindStatusText('');
      return;
    }
    const total = editor.countMatches?.(findQuery, findOptions) ?? 0;
    if (total === 0) {
      setFindStatusText('No matches', true);
      return;
    }
    setFindStatusText(`${total} matches`);
  }, [findOpen, findOptions, findQuery, setFindStatusText]);

  const runFind = useCallback(
    (backward = false) => {
      const editor = editorRef.current;
      if (!editor) {
        return;
      }
      if (!findQuery) {
        setFindStatusText('Enter text', true);
        return;
      }
      const result = backward
        ? editor.findPrevious?.(findQuery, findOptions)
        : editor.findNext?.(findQuery, findOptions);
      if (!result?.found) {
        setFindStatusText('No matches', true);
        return;
      }
      setFindStatusText(`${result.current}/${result.total}`);
    },
    [findOptions, findQuery, setFindStatusText]
  );

  const replaceCurrentFindMatch = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    if (!findQuery) {
      setFindStatusText('Enter text', true);
      return;
    }
    const result = editor.replaceCurrent?.(findQuery, findReplacement, findOptions);
    if (!result?.replaced) {
      if (result?.found) {
        setFindStatusText(`${result.current}/${result.total}`);
      } else {
        setFindStatusText('No matches', true);
      }
      return;
    }
    if (result.found) {
      setFindStatusText(`Replaced - ${result.current}/${result.total}`);
      return;
    }
    setFindStatusText(result.total ? `Replaced - ${result.total} remaining` : 'Replaced');
  }, [findOptions, findQuery, findReplacement, setFindStatusText]);

  const replaceAllFindMatches = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    if (!findQuery) {
      setFindStatusText('Enter text', true);
      return;
    }
    const result = editor.replaceAll?.(findQuery, findReplacement, findOptions);
    if (!result?.replaced) {
      setFindStatusText('No matches', true);
      return;
    }
    setFindStatusText(`Replaced ${result.replaced} matches`);
  }, [findOptions, findQuery, findReplacement, setFindStatusText]);

  const closeFind = useCallback(() => {
    setFindOpen(false);
    setFindQuery('');
    setFindReplacement('');
    setFindStatusText('');
    editorRef.current?.setSearchQuery?.('', findOptions);
    editorRef.current?.focus();
  }, [findOptions, setFindStatusText]);

  useEffect(() => {
    if (!findOpen) {
      editorRef.current?.setSearchQuery?.('', findOptions);
      return;
    }
    updateFindStatusSummary();
  }, [findOpen, findOptions, findQuery, updateFindStatusSummary]);

  useEffect(() => {
    setMeoSelectionState({ visible: false });
    setFindOpen(false);
    setFindQuery('');
    setFindReplacement('');
    setFindStatusText('');
  }, [documentKey, setFindStatusText]);

  useEffect(() => {
    const editor = editorRef.current;
    if (!editor || content === latestContentRef.current) {
      return;
    }
    latestContentRef.current = content;
    editor.setText(content);
    editor.refreshLayout?.();
  }, [content]);

  useEffect(() => {
    editorRef.current?.setGitBaseline?.(gitBaseline ?? null);
  }, [documentKey, gitBaseline]);

  useEffect(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    editor.view.dispatch({
      effects: manageMeoAnnotationEffect.of(createManageMeoAnnotationDecorations(editor.getText(), annotations)),
    });
    syncManageMeoAnnotationReviewState(
      editor.view,
      annotations,
      onSelectionCaptureRef.current,
      onSelectionClearRef.current,
      onAnnotationPreviewChangeRef.current
    );
  }, [annotations, content]);

  useEffect(() => {
    const host = editorHostRef.current;
    if (!host) {
      return;
    }
    latestContentRef.current = content;
    host.replaceChildren();
    applyManageMeoTheme();
    let mountedEditor: ManageMeoEditor | null = null;
    const editor = createMeoEditor({
      externalExtensions: [
        manageMeoAnnotationField,
        EditorView.updateListener.of((update) => {
          if (!update.selectionSet && !update.docChanged && !update.viewportChanged) {
            return;
          }
          syncManageMeoAnnotationReviewState(
            update.view,
            annotationsRef.current,
            onSelectionCaptureRef.current,
            onSelectionClearRef.current,
            onAnnotationPreviewChangeRef.current
          );
        }),
      ] satisfies Extension[],
      initialGitGutter: gitGutterVisible,
      initialLineNumbers: lineNumbersVisible,
      initialMode: currentMode,
      initialVimKeybindings: [],
      parent: host,
      text: content,
      onSelectionChange: (state: ManageMeoSelectionState) => {
        setMeoSelectionState(state?.visible ? state : { visible: false });
      },
      onApplyChanges: (nextContent: string) => {
        latestContentRef.current = nextContent;
        onContentChangeRef.current(nextContent);
        mountedEditor?.view.dispatch({
          effects: manageMeoAnnotationEffect.of(
            createManageMeoAnnotationDecorations(nextContent, annotationsRef.current)
          ),
        });
      },
      onOpenLink: (href: string) => {
        const safeHref = sanitizeManageHref(href);
        if (safeHref) {
          window.open(safeHref, '_blank', 'noopener,noreferrer');
        }
      },
    }) as ManageMeoEditor;
    mountedEditor = editor;
    editorRef.current = editor;
    editor.setGitBaseline?.(gitBaseline ?? null);
    editor.view.dispatch({
      effects: manageMeoAnnotationEffect.of(createManageMeoAnnotationDecorations(content, annotationsRef.current)),
    });
    syncManageMeoAnnotationReviewState(
      editor.view,
      annotationsRef.current,
      onSelectionCaptureRef.current,
      onSelectionClearRef.current,
      onAnnotationPreviewChangeRef.current
    );
    window.requestAnimationFrame(() => editor.refreshLayout?.());
    return () => {
      editor.destroy();
      if (editorRef.current === editor) {
        editorRef.current = null;
      }
    };
  }, [documentKey]);

  return (
    <div className='manage-markdown-review manage-markdown-meo-review'>
      <section className='manage-markdown-review-main'>
        <div
          className={`manage-meo-markdown-editor editor-root${contentMaxWidthEnabled ? ' meo-content-max-width-enabled' : ''}`}
          style={
            {
              '--meo-content-max-width': contentMaxWidthEnabled ? MANAGE_MEO_CONTENT_MAX_WIDTH : '100%',
            } as CSSProperties
          }
        >
          <ManageMeoTopToolbar
            contentMaxWidthEnabled={contentMaxWidthEnabled}
            currentMode={currentMode}
            findCaseSensitive={findCaseSensitive}
            findOpen={findOpen}
            findQuery={findQuery}
            findReplacement={findReplacement}
            findStatus={findStatus}
            findStatusIsError={findStatusIsError}
            findWholeWord={findWholeWord}
            gitGutterVisible={gitGutterVisible}
            lineNumbersVisible={lineNumbersVisible}
            onCloseFind={closeFind}
            onFindCaseSensitiveChange={setFindCaseSensitive}
            onFindOpenChange={setFindOpen}
            onFindQueryChange={setFindQuery}
            onFindReplacementChange={setFindReplacement}
            onFindWholeWordChange={setFindWholeWord}
            onFormat={applyMeoFormat}
            onModeChange={applyMeoMode}
            onReplaceAll={replaceAllFindMatches}
            onReplaceCurrent={replaceCurrentFindMatch}
            onRunFind={runFind}
            onToggleContentMaxWidth={toggleMeoContentMaxWidth}
            onToggleGitGutter={toggleMeoGitGutter}
            onToggleLineNumbers={toggleMeoLineNumbers}
          />
          <div className='editor-wrapper' data-outline-position='right'>
            <div className='editor-host' ref={editorHostRef} />
          </div>
          {selectionToolbarMode === 'formatting' && selection ? (
            <ManageMeoSelectionFormatToolbar
              anchor={selection.anchor}
              onAnnotate={() => onSelectionToolbarModeChange('annotations')}
              onFormat={applyMeoFormat}
              selectionState={meoSelectionState}
            />
          ) : null}
        </div>
      </section>
    </div>
  );
}
