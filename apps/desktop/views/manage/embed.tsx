import { useEffect, useRef, useState } from 'react';
import { MANAGE_CONTENT_AUTOSAVE_DELAY_MS } from './constants';
import { requestManageFiles } from './files-bridge';
import { ManageExcalidrawEditor } from './preview/excalidraw-editor';
import { ManageFontViewer } from './preview/font-viewer';
import { ManageHtmlRenderViewer, manageHtmlPageName } from './preview/html-viewer';
import type { ManageWebKitWindow } from './types';

/** The Docs resource origin's URL for one file, or `undefined` when the app gave no origin. */
function manageDocsResourceUrl(path: string): string | undefined {
  const configuredBaseUrl = (window as ManageWebKitWindow).ghostexGpui?.manageDocsResourceBaseUrl;
  const components = path.split('/');
  if (!configuredBaseUrl || components.some((component) => !component || component === '.' || component === '..')) {
    return undefined;
  }
  try {
    return new URL(components.map(encodeURIComponent).join('/'), configuredBaseUrl).toString();
  } catch {
    return undefined;
  }
}

/**
 * CDXC:Docs 2026-09-27 DECISION:
 * User: "for video i think we can play videos using browser for now to keep it simple" and "audio also play in embedded browser". The Files view sends `media=video|audio` only for formats this browser can decode; the rest open in the system app (see `DocsFileKind` in apps/desktop/src/app/native_docs/state.rs). The file streams from the Docs resource origin, which answers byte ranges so the player can seek.
 */
function ManageEmbedMedia({ media, path, revision }: { media: 'audio' | 'video'; path: string; revision: string }) {
  const source = manageDocsResourceUrl(path);
  if (!source) {
    return <div className='manage-preview-message'>This file can't be played here.</div>;
  }
  return (
    <div className='manage-embed-media'>
      {media === 'video' ? (
        <video autoPlay controls key={revision} playsInline preload='metadata' src={source} />
      ) : (
        <audio autoPlay controls key={revision} preload='metadata' src={source} />
      )}
    </div>
  );
}

/**
 * CDXC:Docs 2026-10-17 DECISION:
 * User (Q222) picked the "browser viewer" for PDFs and (Q227) AVIF "in browser": the Files view sends `media=pdf` for a PDF, shown by Chromium's own PDF viewer (zoom, search, thumbnails, print) in a frame, and `media=image` for an AVIF picture, which GPUI cannot decode. Both stream from the Docs resource origin, which serves a remote project's files too.
 */
function ManageEmbedViewer({ media, path }: { media: 'image' | 'pdf'; path: string }) {
  const [failed, setFailed] = useState(false);
  const source = manageDocsResourceUrl(path);
  if (!source || failed) {
    return <div className='manage-preview-message'>This file can't be shown here.</div>;
  }
  if (media === 'image') {
    return (
      <div className='manage-embed-media'>
        <img alt='' className='manage-embed-image' onError={() => setFailed(true)} src={source} />
      </div>
    );
  }
  if (navigator.pdfViewerEnabled === false) {
    return <div className='manage-preview-message'>This browser has no PDF viewer.</div>;
  }
  return <iframe className='manage-embed-pdf' src={source} title={path.split('/').pop() ?? path} />;
}

/**
 * CDXC:Docs 2026-09-24 WHY:
 * The native Docs view draws everything itself except HTML files, Excalidraw drawings, video, audio, PDFs, AVIF
 * pictures and fonts, which need a browser engine. For those the app loads this page with `embed=1&path=…` as a normal child
 * of the document area, and it shows only that one file: no files list, no header. The native side
 * reloads it (a new `revision`) when the file changes on disk or the user presses Reload.
 * SEE-ALSO: apps/desktop/src/app/native_docs/browser_area.rs.
 */
export function ManageEmbed() {
  const params = new URLSearchParams(window.location.search);
  const media = params.get('media');
  const resourcePath = params.get('resourcePath') ?? params.get('path') ?? '';
  const revision = params.get('revision') ?? '';
  if (media === 'pdf' || media === 'image') {
    return <ManageEmbedViewer key={revision} media={media} path={resourcePath} />;
  }
  if (media === 'font') {
    const source = manageDocsResourceUrl(resourcePath);
    if (!source) {
      return <div className='manage-preview-message'>This font can't be shown here.</div>;
    }
    return (
      <ManageFontViewer fileName={resourcePath.split('/').pop() ?? resourcePath} revision={revision} source={source} />
    );
  }
  if (media === 'video' || media === 'audio') {
    return (
      <ManageEmbedMedia
        media={media}
        path={params.get('resourcePath') ?? params.get('path') ?? ''}
        revision={params.get('revision') ?? ''}
      />
    );
  }
  return <ManageEmbedDocument />;
}

function ManageEmbedDocument() {
  const params = new URLSearchParams(window.location.search);
  const projectId = params.get('projectId') ?? '';
  const projectEditorId = params.get('projectEditorId') ?? projectId;
  const path = params.get('path') ?? '';
  // An outside file is read and saved by its real path but loads its resources from its grant's mount.
  const resourcePath = params.get('resourcePath') ?? path;
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

  /**
   * CDXC:CefRuntime 2026-10-01 DECISION:
   * User: "please allow me to pinch zoom in/out in Excalidraw". The app lets a trackpad pinch reach this page only while it shows a drawing (cef/shell/browser.rs); Chromium delivers it as a Ctrl+wheel, which Excalidraw turns into its own canvas zoom. Cancelling the default here, before Excalidraw sees the event, keeps Chromium from zooming the whole page, which built-in views never do.
   */
  useEffect(() => {
    if (!drawing) {
      return;
    }
    const stopPageZoom = (event: WheelEvent) => {
      if (event.ctrlKey) {
        event.preventDefault();
      }
    };
    window.addEventListener('wheel', stopPageZoom, { capture: true, passive: false });
    return () => window.removeEventListener('wheel', stopPageZoom, { capture: true });
  }, [drawing]);

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
        documentKey={resourcePath}
        onOpenDocument={(next) =>
          void requestManageFiles({ action: 'openDocsFile', path: next, projectEditorId, projectId })
        }
        pageName={manageHtmlPageName(path, resourcePath)}
      />
    </div>
  );
}
