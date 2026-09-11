import { useEffect, useState, type ComponentProps, type ComponentType } from 'react';

function deferredEditor<P extends object>(load: () => Promise<ComponentType<P>>, label: string) {
  let pending: Promise<ComponentType<P>> | undefined;
  return function DeferredEditor(props: P) {
    const [Editor, setEditor] = useState<ComponentType<P>>();
    const [error, setError] = useState<string>();
    const [attempt, setAttempt] = useState(0);
    useEffect(() => {
      let cancelled = false;
      setError(undefined);
      pending ??= load().catch((error) => {
        pending = undefined;
        throw error;
      });
      void pending
        .then((component) => {
          if (!cancelled) setEditor(() => component);
        })
        .catch((error) => {
          if (!cancelled) setError(error instanceof Error ? error.message : `Could not load ${label}.`);
        });
      return () => {
        cancelled = true;
      };
    }, [attempt]);
    if (Editor) return <Editor {...props} />;
    return (
      <div className='manage-editor-loading' role='status'>
        {error ? (
          <>
            Could not load {label}.{' '}
            <button type='button' onClick={() => setAttempt((value) => value + 1)}>
              Retry
            </button>
          </>
        ) : (
          `Loading ${label}…`
        )}
      </div>
    );
  };
}

type MarkdownProps = ComponentProps<typeof import('./markdown-review-viewer').ManageMarkdownReviewViewer>;
type DrawingProps = ComponentProps<typeof import('./excalidraw-editor').ManageExcalidrawEditor>;

export const DeferredMarkdownEditor = deferredEditor<MarkdownProps>(
  () => import('./markdown-review-viewer').then((module) => module.ManageMarkdownReviewViewer),
  'Markdown editor'
);
export const DeferredDrawingEditor = deferredEditor<DrawingProps>(
  () => import('./excalidraw-editor').then((module) => module.ManageExcalidrawEditor),
  'drawing editor'
);
