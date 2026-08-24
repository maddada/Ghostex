import { startTransition, useEffect, useRef, useState } from 'react';
import type { SidebarHydrateMessage, SidebarToExtensionMessage } from '../shared/session-grid-contract';
import { SidebarApp } from './sidebar-app';
import { logSidebarDebug } from './sidebar-debug';
import {
  createSidebarStoryMessage,
  createSidebarStoryWorkspace,
  reduceSidebarStoryWorkspace,
  type SidebarStoryWorkspace,
} from './sidebar-story-workspace';
import type { WebviewApi } from './webview-api';

export type SidebarStoryHarnessProps = {
  message: SidebarHydrateMessage;
  onWorkspaceChange?: (workspace: SidebarStoryWorkspace) => void;
};

const sidebarStoryMessages: SidebarToExtensionMessage[] = [];
const STORYBOOK_DRAG_SETTLE_DELAY_MS = 900;

export function getSidebarStoryMessages() {
  return [...sidebarStoryMessages];
}

export function resetSidebarStoryMessages() {
  sidebarStoryMessages.length = 0;
}

export function SidebarStoryHarness({ message, onWorkspaceChange }: SidebarStoryHarnessProps) {
  const [workspace, setWorkspace] = useState(() => createSidebarStoryWorkspace(message));
  const workspaceRef = useRef(workspace);
  const vscode = useRef<WebviewApi>({
    postMessage(nextMessage) {
      sidebarStoryMessages.push(nextMessage);

      if (nextMessage.type === 'sidebarDebugLog') {
        logSidebarDebug(true, `storybook ${nextMessage.event}`, nextMessage.details);
      }

      const nextWorkspace = reduceSidebarStoryWorkspace(workspaceRef.current, nextMessage);
      if (!nextWorkspace) {
        return;
      }

      scheduleStoryWorkspaceUpdate(() => {
        startTransition(() => {
          setWorkspace(nextWorkspace);
        });
      });
    },
  }).current;

  useEffect(() => {
    /*
     * CDXC:GPUIProjectSidebarBridge 2026-06-22-20:02:
     * Storybook owns the current explicit sidebar workspace state. Let embeds observe that state directly so GPUI can post active-project changes without deriving project identity from fixture names, sidebar labels alone, paths, or logs.
     */
    workspaceRef.current = workspace;
    onWorkspaceChange?.(workspace);
  }, [onWorkspaceChange, workspace]);

  useEffect(() => {
    startTransition(() => {
      setWorkspace(createSidebarStoryWorkspace(message));
    });
  }, [message]);

  useEffect(() => {
    const nextMessage = createSidebarStoryMessage(workspace);
    const timeoutId = window.setTimeout(() => {
      window.postMessage(nextMessage, '*');
    }, 0);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [workspace]);

  return (
    <div
      /*
       * CDXC:SidebarStorybook 2026-05-05-05:29
       * Native sidebar stories must not insert an extra block between
       * .native-sidebar-main and SidebarApp. The real app renders the project
       * header and stack as direct flex children, and scroll/overflow bugs only
       * reproduce accurately when Storybook keeps that layout contract.
       */
      style={{ display: 'contents' }}
    >
      <SidebarApp enableProjectCollections={true} vscode={vscode} windowScopeId='storybook' />
    </div>
  );
}

function scheduleStoryWorkspaceUpdate(callback: () => void) {
  window.setTimeout(() => {
    if (typeof window.requestAnimationFrame !== 'function') {
      callback();
      return;
    }

    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        callback();
      });
    });
  }, STORYBOOK_DRAG_SETTLE_DELAY_MS);
}
