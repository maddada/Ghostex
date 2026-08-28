import { useEffect, useState } from 'react';

import { SpaceEditorModal } from '@/packages/core-ui/space-editor-modal';
import type { OpenSidebarSpaceEditorModalDetail } from './action-events';
import type { WebSidebarRuntime } from '../sidebar-runtime/sidebar-runtime';

/*
 * CDXC:SidebarSpaces 2026-08-27:
 * The web half of the New/Edit Space dialog. The shared sidebar opens it through
 * the app-modal bridge exactly as it does in the desktop app; the web shim turns
 * that into the window event this host listens for.
 *
 * The confirm leaves through `runtime.vscode.postMessage` rather than mutating
 * anything here: the runtime bounces it back to SidebarApp, which owns the Space
 * document and applies the edit to the CURRENT one. That is the same split gpui
 * has, and the reason the two hosts cannot drift.
 */
export function SpaceEditorModalHost({ runtime }: { runtime: WebSidebarRuntime }) {
  const [detail, setDetail] = useState<OpenSidebarSpaceEditorModalDetail>();

  useEffect(() => {
    const open = (event: WindowEventMap['ghostex-web:openSidebarSpaceEditorModal']) => setDetail(event.detail);
    const close = () => setDetail(undefined);
    window.addEventListener('ghostex-web:openSidebarSpaceEditorModal', open);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:openSidebarSpaceEditorModal', open);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, []);

  return (
    <SpaceEditorModal
      initialColor={detail?.spaceColor}
      initialIcon={detail?.spaceIcon}
      initialName={detail?.spaceName}
      isOpen={detail !== undefined}
      mode={detail?.mode ?? 'create'}
      onCancel={() => setDetail(undefined)}
      onDelete={() => {
        if (detail?.spaceId) {
          runtime.vscode.postMessage({
            mode: 'delete',
            ...(detail.remoteMachineId ? { remoteMachineId: detail.remoteMachineId } : {}),
            spaceId: detail.spaceId,
            type: 'sidebarSpaceEditorResult',
          });
        }
        setDetail(undefined);
      }}
      onSubmit={(space) => {
        if (detail) {
          runtime.vscode.postMessage({
            color: space.color,
            icon: space.icon,
            ...(detail.memberCollectionId ? { memberCollectionId: detail.memberCollectionId } : {}),
            ...(detail.memberProjectId ? { memberProjectId: detail.memberProjectId } : {}),
            mode: detail.mode,
            name: space.name,
            ...(detail.remoteMachineId ? { remoteMachineId: detail.remoteMachineId } : {}),
            ...(detail.mode === 'edit' && detail.spaceId ? { spaceId: detail.spaceId } : {}),
            type: 'sidebarSpaceEditorResult',
          });
        }
        setDetail(undefined);
      }}
    />
  );
}
