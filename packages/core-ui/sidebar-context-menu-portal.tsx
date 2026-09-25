import { AppMenuPanel } from '@/packages/components/ui/app-menu-panel';
import { createPortal } from 'react-dom';
import {
  useLayoutEffect,
  useRef,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  type RefObject,
} from 'react';
import type {
  NativePortlessAdminAction,
  NativePortlessAdminResult,
  NativePortlessProtocol,
} from '../shared/native-ghostty-host-protocol';
import { setSidebarTooltipsSuppressed } from './app-tooltip';
import type { WebviewApi } from './webview-api';

/**
 * CDXC:ContextMenus 2026-07-30:
 * The one margin every sidebar context menu and submenu panel keeps from the
 * webview edges. Exported so stacked submenu portals clamp against the same
 * number this portal does.
 */
export const SIDEBAR_CONTEXT_MENU_VIEWPORT_MARGIN_PX = 12;

const CONTEXT_MENU_VIEWPORT_MARGIN_PX = SIDEBAR_CONTEXT_MENU_VIEWPORT_MARGIN_PX;

type SidebarContextMenuPortalProps = {
  children: ReactNode;
  menuClassName?: string;
  menuRef?: RefObject<HTMLDivElement | null>;
  menuStyle?: CSSProperties;
  onDismiss: () => void;
  vscode?: WebviewApi;
};

type GhostexNativeSidebarBridge = {
  dismissSidebarContextMenu: () => void;
  notifySidebarContextMenuClosed: () => void;
  notifySidebarContextMenuOpened: () => void;
  setNativePointerInside: (isInside: boolean) => void;
  openActiveProjectEditorFromTitlebar: () => void;
  exitFocusModeFromTitlebar: () => void;
  focusSessionFromPromptEditorClose: (nativeSessionId: string) => void;
  focusResourceSessionFromTitlebar: (sessionId: string) => void;
  openAgentsModeFromTitlebar: () => void;
  openGitHubProjectFromTitlebar: () => void;
  /*
   * CDXC:Automations 2026-06-30-11:05:
   * Titlebar Automate is a first-class project/Quick workarea forwarded through the same native sidebar bridge as Source, Browser, Kanban, and Docs.
   */
  openAutomateFromTitlebar: () => void;
  quitResourcesFromTitlebar: (sessionIds: string[], projectIds: string[]) => void;
  toggleProjectEditorCompanionFromTitlebar: () => void;
  sleepInactiveSessionsFromTitlebar: (sessionIds: string[]) => void;
  openTasksPlaceholderFromTitlebar: () => void;
  openManageFromTitlebar: () => void;
  refreshWorkspaceOpenTargetAvailabilityFromTitlebar: () => void;
  rotateActivePaneLayoutClockwiseFromTitlebar: () => void;
  sleepPetOverlayFromPet: () => void;
  togglePetOverlayFromTitlebar: () => void;
  toggleCommandsPanelFromTitlebar: () => void;
  runSidebarCommandFromTitlebar: (commandId: string) => void;
  runSidebarGitActionFromTitlebar: (action: 'commit' | 'push' | 'pr' | 'syncMain' | 'multiRelease' | 'release') => void;
  runPortlessAdminAction: (
    action: NativePortlessAdminAction,
    options?: { protocol?: NativePortlessProtocol; requestId?: string; timeoutMs?: number }
  ) => Promise<NativePortlessAdminResult>;
};

const activeDismissHandlers = new Set<() => void>();
let openSidebarContextMenuCount = 0;

declare global {
  interface Window {
    __ghostex_NATIVE_SIDEBAR__?: GhostexNativeSidebarBridge;
  }
}

/**
 * CDXC:ContextMenus 2026-05-20-13:05:
 * Native AppKit surfaces dismiss open sidebar context menus through this hook
 * while leaving the user's original click intact.
 */
export function dismissAllSidebarContextMenus(): void {
  for (const dismiss of [...activeDismissHandlers]) {
    dismiss();
  }
}

export function registerSidebarContextMenuDismissHandler(dismiss: () => void): () => void {
  activeDismissHandlers.add(dismiss);
  return () => {
    activeDismissHandlers.delete(dismiss);
  };
}

function notifySidebarContextMenuOpened(vscode?: WebviewApi): void {
  if (window.__ghostex_NATIVE_SIDEBAR__?.notifySidebarContextMenuOpened) {
    window.__ghostex_NATIVE_SIDEBAR__.notifySidebarContextMenuOpened();
    return;
  }
  vscode?.postMessage({ type: 'sidebarContextMenuOpened' });
}

function notifySidebarContextMenuClosed(vscode?: WebviewApi): void {
  if (window.__ghostex_NATIVE_SIDEBAR__?.notifySidebarContextMenuClosed) {
    window.__ghostex_NATIVE_SIDEBAR__.notifySidebarContextMenuClosed();
    return;
  }
  vscode?.postMessage({ type: 'sidebarContextMenuClosed' });
}

/**
 * CDXC:ContextMenus 2026-07-30:
 * Exported so submenu panels — which are separate portals stacked above this
 * one, not children of it — clamp against the viewport with the SAME margin the
 * parent menu uses. A second copy of this arithmetic is how the parent menu and
 * its flyout end up disagreeing about the sidebar's edges.
 */
export function getClampedSidebarContextMenuCoordinate(value: number, size: number, viewportSize: number): number {
  return Math.max(
    CONTEXT_MENU_VIEWPORT_MARGIN_PX,
    Math.min(value, viewportSize - size - CONTEXT_MENU_VIEWPORT_MARGIN_PX)
  );
}

export function getSidebarContextMenuBackdropRetarget({
  backdrop,
  clientX,
  clientY,
  elementFromPoint,
}: {
  backdrop: HTMLElement;
  clientX: number;
  clientY: number;
  elementFromPoint: (x: number, y: number) => Element | null;
}): Element | undefined {
  const previousPointerEvents = backdrop.style.pointerEvents;
  backdrop.style.pointerEvents = 'none';

  try {
    const target = elementFromPoint(clientX, clientY);
    if (!target || target === backdrop || backdrop.contains(target)) {
      return undefined;
    }

    return target;
  } finally {
    backdrop.style.pointerEvents = previousPointerEvents;
  }
}

function dispatchBackdropContextMenuToRetarget(event: ReactMouseEvent<HTMLButtonElement>, target: Element): void {
  target.dispatchEvent(
    new MouseEvent('contextmenu', {
      altKey: event.altKey,
      bubbles: true,
      button: event.button,
      buttons: event.buttons,
      cancelable: true,
      clientX: event.clientX,
      clientY: event.clientY,
      ctrlKey: event.ctrlKey,
      metaKey: event.metaKey,
      screenX: event.screenX,
      screenY: event.screenY,
      shiftKey: event.shiftKey,
      view: window,
    })
  );
}

/**
 * CDXC:ContextMenus 2026-05-20-12:30:
 * Session and project context menus use a transparent backdrop above sidebar
 * rows/cards and below the menu so in-sidebar clicks dismiss without activating
 * the target underneath. Native listens for clicks outside the sidebar webview
 * and calls dismissAllSidebarContextMenus() so workspace/titlebar clicks both
 * close the menu and reach their original target.
 *
 * CDXC:ContextMenus 2026-05-21-04:35:
 * Native open/close notifications must run in useLayoutEffect so the AppKit
 * outside-click monitor is armed before the user can click a terminal pane.
 *
 * CDXC:ContextMenus 2026-06-02-21:07:
 * A second right-click on a sidebar session while another context menu is open
 * must dismiss the old menu and open the menu owned by the session under the
 * pointer. Retarget backdrop contextmenu events to the underlying element so
 * session rows keep priority over the surrounding project/group menu.
 *
 * CDXC:DesignSystem 2026-06-19-14:16:
 * Viewport-clamped sidebar context menus can become internal scroll areas.
 * Apply the shared Codex-style edge fade at this portal boundary so session,
 * project, reference, and filter menus stay visually consistent when they
 * overflow.
 */
export function SidebarContextMenuPortal({
  children,
  menuClassName = 'session-context-menu',
  menuRef,
  menuStyle,
  onDismiss,
  vscode,
}: SidebarContextMenuPortalProps) {
  const internalMenuRef = useRef<HTMLDivElement>(null);
  const activeMenuRef = menuRef ?? internalMenuRef;
  const resolvedMenuClassName = menuClassName.includes('vertical-scroll-fade-mask')
    ? menuClassName
    : `${menuClassName} vertical-scroll-fade-mask`;

  useLayoutEffect(() => {
    return registerSidebarContextMenuDismissHandler(onDismiss);
  }, [onDismiss]);

  useLayoutEffect(() => {
    /*
     * CDXC:Tooltips 2026-09-15 DECISION:
     * User: no tooltip may show while a context menu is open. Hold the shared suppression for as long as any menu portal is mounted; card submenus are raw portals that only exist while their parent menu does, so counting the menu portals is enough.
     */
    openSidebarContextMenuCount += 1;
    setSidebarTooltipsSuppressed('contextMenu', true);
    return () => {
      openSidebarContextMenuCount -= 1;
      if (openSidebarContextMenuCount === 0) {
        setSidebarTooltipsSuppressed('contextMenu', false);
      }
    };
  }, []);

  useLayoutEffect(() => {
    /**
     * CDXC:ContextMenus 2026-09-11 WHY:
     * On the desktop app these two notifications are also what makes an open menu hold native focus, so a click back into a pane blurs the page and dismisses it.
     * The QuickJS runtime counted them and merged the result with editable focus in the CEF helper (until the runtime was deleted on 2026-09-25), so the menu itself never needs DOM focus and no menu, flyout, or submenu has to be marked. Do not reintroduce a focus() here or a focus-tracking attribute on the menu: inferring the grant from the focused node is what closed menus on every submenu switch and flyout click.
     * SEE-ALSO: apps/desktop/src/bin/ghostex_gpui_cef_helper.rs. (The counting side, `gxserver-runtime/sessions-and-focus.ts`, was deleted with QuickJS.)
     */
    notifySidebarContextMenuOpened(vscode);
    return () => {
      notifySidebarContextMenuClosed(vscode);
    };
  }, [vscode]);

  useLayoutEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onDismiss();
      }
    };

    const handleWindowBlur = () => {
      /*
       * GPUI, AppKit terminal, and sibling CEF surfaces own their normal input
       * frames outside this sidebar document. When one of those surfaces takes
       * focus, the sidebar browsing context blurs; dismiss here so click-away
       * works across that native sibling boundary without a window hit-test
       * monitor or an overlay outside the sidebar.
       */
      onDismiss();
    };

    document.addEventListener('keydown', handleKeyDown);
    window.addEventListener('blur', handleWindowBlur);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('blur', handleWindowBlur);
    };
  }, [onDismiss]);

  return createPortal(
    <>
      <button
        aria-label='Close context menu'
        className='sidebar-context-menu-backdrop'
        onPointerDown={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onDismiss();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
          const retarget = getSidebarContextMenuBackdropRetarget({
            backdrop: event.currentTarget,
            clientX: event.clientX,
            clientY: event.clientY,
            elementFromPoint: (x, y) => document.elementFromPoint(x, y),
          });
          onDismiss();
          if (retarget) {
            dispatchBackdropContextMenuToRetarget(event, retarget);
          }
        }}
        type='button'
      />
      <AppMenuPanel
        className={resolvedMenuClassName}
        onClick={(event) => {
          event.stopPropagation();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
        ref={activeMenuRef}
        role='menu'
        style={menuStyle}
      >
        {children}
      </AppMenuPanel>
    </>,
    document.body
  );
}
