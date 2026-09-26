import { useEffect, useRef, useState } from 'react';
import { MANAGE_CONTENT_AUTOSAVE_DELAY_MS } from './constants';
import { requestManageFiles } from './manage-app';
import { ManageExcalidrawEditor } from './preview/excalidraw-editor';
import { ManageHtmlRenderViewer } from './preview/html-viewer';

/**
 * CDXC:Docs 2026-09-24 WHY:
 * The native Docs view draws everything itself except HTML files and Excalidraw drawings, which
 * need a browser engine. For those the app loads this page with `embed=1&path=…` as a normal child
 * of the document area, and it shows only that one file: no files list, no header. The native side
 * reloads it (a new `revision`) when the file changes on disk or the user presses Reload.
 * SEE-ALSO: apps/desktop/src/app/native_docs/browser_area.rs.
 */
export function ManageEmbed() {
  const params = new URLSearchParams(window.location.search);
  const projectId = params.get('projectId') ?? '';
  const projectEditorId = params.get('projectEditorId') ?? projectId;
  const path = params.get('path') ?? '';
  const annotate = params.get('annotate') !== '0';
  const drawing = /\.excalidraw$/i.test(path);
  const [content, setContent] = useState<string>();
  const [error, setError] = useState<string>();
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    requestManageFiles({ action: 'read', path, projectEditorId, projectId })
      .then((response) => {
        if (response.error || response.file?.kind !== 'text') {
          setError(response.error ?? response.file?.error ?? 'Preview unavailable');
          return;
        }
        setContent(response.file.content ?? '');
      })
      .catch((reason: unknown) => setError(reason instanceof Error ? reason.message : String(reason)));
  }, [path, projectEditorId, projectId]);

  useEffect(() => () => window.clearTimeout(saveTimer.current), []);

  if (error) {
    return <div className='manage-preview-message'>{error}</div>;
  }
  if (content === undefined) {
    return null;
  }
  if (drawing) {
    // CDXC:Docs 2026-09-15 DECISION: Excalidraw drawings keep the one-second autosave because drawing gestures have no natural save moment.
    const save = (next: string) => {
      window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        void requestManageFiles({ action: 'save', content: next, path, projectEditorId, projectId });
      }, MANAGE_CONTENT_AUTOSAVE_DELAY_MS);
    };
    return (
      <div className='manage-embed'>
        <ManageExcalidrawEditor content={content} fileName={path.split('/').pop() ?? path} onChange={save} />
      </div>
    );
  }
  return (
    <div className='manage-embed'>
      <ManageHtmlRenderViewer
        annotationsEnabled={annotate}
        content={content}
        documentKey={path}
        onOpenDocument={(next) =>
          void requestManageFiles({ action: 'openDocsFile', path: next, projectEditorId, projectId })
        }
      />
    </div>
  );
}
