import { useEffect, useState } from 'react';

import { SessionNoteModal } from '@/packages/core-ui/session-note-modal';
import type { OpenSessionNoteModalDetail } from './action-events';
import type { WebSidebarRuntime } from '../sidebar-runtime/sidebar-runtime';

/*
 * CDXC:SessionAgentNotes 2026-08-24:
 * The web half of the session-note editor. The shared sidebar opens it through
 * the app-modal bridge, exactly as it does in the desktop app; the web shim
 * turns that into the window event this host listens for.
 *
 * The save leaves through `runtime.vscode.postMessage` rather than a direct
 * daemon call, so the web sidebar runtime's `setSessionNote` handler is the one
 * place that resolves machine/project/session and talks to gxserver — the same
 * split gpui has, and the reason the two hosts cannot drift.
 */
export function SessionNoteModalHost({ runtime }: { runtime: WebSidebarRuntime }) {
  const [detail, setDetail] = useState<OpenSessionNoteModalDetail>();

  useEffect(() => {
    const open = (event: WindowEventMap['ghostex-web:openSessionNoteModal']) => setDetail(event.detail);
    const close = () => setDetail(undefined);
    window.addEventListener('ghostex-web:openSessionNoteModal', open);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:openSessionNoteModal', open);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, []);

  return (
    <SessionNoteModal
      initialNote={detail?.initialNote ?? ''}
      isOpen={detail !== undefined}
      onCancel={() => setDetail(undefined)}
      onConfirm={(note) => {
        if (detail) {
          runtime.vscode.postMessage({
            note,
            ...(detail.projectId ? { projectId: detail.projectId } : {}),
            sessionId: detail.sessionId,
            type: 'setSessionNote',
          });
        }
        setDetail(undefined);
      }}
      sessionTitle={detail?.sessionTitle}
    />
  );
}
