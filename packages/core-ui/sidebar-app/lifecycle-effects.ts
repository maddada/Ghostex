import { useEffect, type Dispatch, type RefObject, type SetStateAction } from 'react';
import type { ExtensionToSidebarMessage } from '../../shared/session-grid-contract';
import type { ghostexSettings } from '../../shared/ghostex-settings';
import {
  getSidebarTitlebarForegroundForBackground,
  getSidebarTitlebarGradientColors,
} from '../../shared/ghostex-settings';
import { getWorkspaceThemeForeground, normalizeWorkspaceThemeColor } from '../../shared/workspace-project-appearance';
import { postSidebarRefreshDebugLog } from '../sidebar-refresh-debug-log';
import { useSidebarStore } from '../sidebar-store';
import type { WebviewApi } from '../webview-api';
import { readSidebarUiCollapseState, summarizeSidebarUiCollapseRead } from './collapse-state';
import type { SidebarCollapseStateLogger } from './collapse-actions';
import { getSidebarStartupElapsedMs } from './session-ordering';
import type { SidebarEventSource, SidebarSessionsById } from './types';

export const SIDEBAR_STARTUP_INTERACTION_BLOCK_MS = 1500;

type SidebarStartupReproLogger = (event: string, details: unknown) => void;
type SidebarRefreshLifecycleLogger = (event: string, details: Record<string, unknown>) => void;

export type SidebarStartupDiagnosticEffectsOptions = {
  collapsedGroupsById: Record<string, true>;
  didLogInitialUiCollapseStateReadRef: RefObject<boolean>;
  firstHydrateRevisionRef: RefObject<number | undefined>;
  groupOrder: readonly string[];
  hasAppliedHydrateRef: RefObject<boolean>;
  initialUiCollapseStateRead: ReturnType<typeof readSidebarUiCollapseState>;
  isStartupInteractionBlocked: boolean;
  lastSidebarStartupRenderStateKeyRef: RefObject<string | undefined>;
  postSidebarCollapseStateLog: SidebarCollapseStateLogger;
  postSidebarRefreshLifecycleLog: SidebarRefreshLifecycleLogger;
  postSidebarStartupReproLog: SidebarStartupReproLogger;
  refreshDebugInstanceIdRef: RefObject<string>;
  revision: number;
  sessionsById: SidebarSessionsById;
  sidebarCollapseDiagnosticLoggingEnabled: boolean;
  sidebarRefreshDiagnosticLoggingEnabled: boolean;
  sidebarStartupStartedAtRef: RefObject<number>;
  vscode: WebviewApi;
  workspaceGroupIds: readonly string[];
};

/*
 * CDXC:RepoStructure 2026-08-22:
 * SidebarApp's three startup/refresh diagnostic effects, kept together and in
 * their original relative order: app mount/unmount lifetime, the one-shot
 * initial collapse-state read, and the deduplicated render-state trace.
 */
export function useSidebarStartupDiagnosticEffects({
  collapsedGroupsById,
  didLogInitialUiCollapseStateReadRef,
  firstHydrateRevisionRef,
  groupOrder,
  hasAppliedHydrateRef,
  initialUiCollapseStateRead,
  isStartupInteractionBlocked,
  lastSidebarStartupRenderStateKeyRef,
  postSidebarCollapseStateLog,
  postSidebarRefreshLifecycleLog,
  postSidebarStartupReproLog,
  refreshDebugInstanceIdRef,
  revision,
  sessionsById,
  sidebarCollapseDiagnosticLoggingEnabled,
  sidebarRefreshDiagnosticLoggingEnabled,
  sidebarStartupStartedAtRef,
  vscode,
  workspaceGroupIds,
}: SidebarStartupDiagnosticEffectsOptions) {
  useEffect(() => {
    /*
    CDXC:Diagnostics 2026-06-06-23:18:
    The mount/unmount diagnostic must describe the React app lifetime only. Including effect-event callbacks in this dependency list made every hydrate render look like an app remount in persistent logs, hiding the real refresh cadence and adding avoidable Debugging Mode noise.
    */
    const instanceId = refreshDebugInstanceIdRef.current;
    postSidebarStartupReproLog('appMounted', {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      startupInteractionBlockMs: SIDEBAR_STARTUP_INTERACTION_BLOCK_MS,
    });
    postSidebarRefreshLifecycleLog('appMounted', {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      instanceId,
      revision: useSidebarStore.getState().revision,
      sessionCount: Object.keys(useSidebarStore.getState().sessionsById).length,
    });

    return () => {
      postSidebarStartupReproLog('appUnmounted', {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        finalRevision: useSidebarStore.getState().revision,
      });
      postSidebarRefreshLifecycleLog('appUnmounted', {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        finalRevision: useSidebarStore.getState().revision,
        instanceId,
        sessionCount: Object.keys(useSidebarStore.getState().sessionsById).length,
      });
    };
  }, []);

  useEffect(() => {
    if (!sidebarCollapseDiagnosticLoggingEnabled || didLogInitialUiCollapseStateReadRef.current) {
      return;
    }

    didLogInitialUiCollapseStateReadRef.current = true;
    postSidebarCollapseStateLog('initialRead', {
      ...summarizeSidebarUiCollapseRead(initialUiCollapseStateRead),
      currentCollapsedGroupCount: Object.keys(collapsedGroupsById).length,
      groupCount: groupOrder.length,
      sessionCount: Object.keys(sessionsById).length,
      workspaceGroupCount: workspaceGroupIds.length,
    });
  }, [
    collapsedGroupsById,
    groupOrder,
    initialUiCollapseStateRead,
    sidebarCollapseDiagnosticLoggingEnabled,
    sessionsById,
    workspaceGroupIds,
  ]);

  useEffect(() => {
    const renderState = {
      elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
      firstHydrateRevision: firstHydrateRevisionRef.current,
      groupCount: groupOrder.length,
      hasHydrate: hasAppliedHydrateRef.current,
      revision,
      sessionCount: Object.keys(sessionsById).length,
      startupInteractionBlocked: isStartupInteractionBlocked,
      workspaceGroupCount: workspaceGroupIds.length,
    };
    const renderStateKey = JSON.stringify(renderState);
    if (lastSidebarStartupRenderStateKeyRef.current === renderStateKey) {
      return;
    }

    lastSidebarStartupRenderStateKeyRef.current = renderStateKey;
    postSidebarStartupReproLog('renderState', renderState);
    postSidebarRefreshDebugLog(sidebarRefreshDiagnosticLoggingEnabled, vscode, 'renderStateChanged', {
      ...renderState,
      instanceId: refreshDebugInstanceIdRef.current,
    });
    if (hasAppliedHydrateRef.current && renderState.sessionCount === 0) {
      postSidebarStartupReproLog('emptyStateAfterHydrate', renderState);
      postSidebarRefreshDebugLog(sidebarRefreshDiagnosticLoggingEnabled, vscode, 'emptyStateAfterHydrate', {
        ...renderState,
        instanceId: refreshDebugInstanceIdRef.current,
      });
    }
  }, [
    groupOrder,
    isStartupInteractionBlocked,
    postSidebarStartupReproLog,
    revision,
    sidebarRefreshDiagnosticLoggingEnabled,
    sessionsById,
    vscode,
    workspaceGroupIds,
  ]);
}

export type SidebarHostMessageListenersOptions = {
  handleWindowMessage: (event: MessageEvent<ExtensionToSidebarMessage>) => void;
  messageSource: SidebarEventSource;
  nativeHostEventSource: SidebarEventSource | null;
};

/*
 * CDXC:RepoStructure 2026-08-22:
 * Both inbound host channels — extension-style postMessage and the Ghostex
 * native host custom event — feed the SAME handler, so they stay one hook.
 */
export function useSidebarHostMessageListeners({
  handleWindowMessage,
  messageSource,
  nativeHostEventSource,
}: SidebarHostMessageListenersOptions) {
  useEffect(() => {
    const handleMessage = (event: Event) => {
      if (event instanceof MessageEvent) {
        handleWindowMessage(event);
      }
    };

    messageSource.addEventListener('message', handleMessage);

    return () => {
      messageSource.removeEventListener('message', handleMessage);
    };
  }, [handleWindowMessage, messageSource]);

  useEffect(() => {
    if (!nativeHostEventSource) {
      return;
    }

    const handleNativeHostEvent = (event: Event) => {
      if (!(event instanceof CustomEvent)) {
        return;
      }

      handleWindowMessage(
        new MessageEvent<ExtensionToSidebarMessage>('message', {
          data: event.detail,
        })
      );
    };

    /**
     * CDXC:Hotkeys 2026-06-05-21:17:
     * Native macOS shortcuts arrive through the Ghostex host custom event, while extension-style traffic arrives through postMessage. Route both into the same sidebar action handler so Cmd+number uses the visible-row slot resolver consistently.
     *
     * CDXC:Hotkeys 2026-06-12-12:33:
     * The native sidebar wrapper owns typed nativeHotkey host events. Allow that wrapper to disable this shared listener so Cmd+T creates one terminal tab instead of running both the wrapper action and the shared SidebarApp createSession bridge.
     */
    nativeHostEventSource.addEventListener('ghostex-native-host-event', handleNativeHostEvent);

    return () => {
      nativeHostEventSource.removeEventListener('ghostex-native-host-event', handleNativeHostEvent);
    };
  }, [handleWindowMessage, nativeHostEventSource]);
}

export type SidebarTimeoutCleanupOptions = {
  completionFlashTimeoutBySessionIdRef: RefObject<Map<string, number>>;
};

/* Clears the completion-flash timers when the app unmounts. */
export function useSidebarTimeoutCleanup({ completionFlashTimeoutBySessionIdRef }: SidebarTimeoutCleanupOptions) {
  useEffect(() => {
    return () => {
      for (const timeout of completionFlashTimeoutBySessionIdRef.current.values()) {
        window.clearTimeout(timeout);
      }
      completionFlashTimeoutBySessionIdRef.current.clear();
    };
  }, []);
}

export type SidebarStartupInteractionBlockOptions = {
  postSidebarStartupReproLog: SidebarStartupReproLogger;
  setIsStartupInteractionBlocked: Dispatch<SetStateAction<boolean>>;
  sidebarStartupStartedAtRef: RefObject<number>;
};

/* Releases the startup click shield once the first hydrate window has elapsed. */
export function useSidebarStartupInteractionBlock({
  postSidebarStartupReproLog,
  setIsStartupInteractionBlocked,
  sidebarStartupStartedAtRef,
}: SidebarStartupInteractionBlockOptions) {
  useEffect(() => {
    const timeout = window.setTimeout(() => {
      postSidebarStartupReproLog('interactionBlockReleased', {
        elapsedMs: getSidebarStartupElapsedMs(sidebarStartupStartedAtRef.current),
        revision: useSidebarStore.getState().revision,
      });
      setIsStartupInteractionBlocked(false);
    }, SIDEBAR_STARTUP_INTERACTION_BLOCK_MS);

    return () => {
      window.clearTimeout(timeout);
    };
  }, []);
}

export type SidebarDocumentChromeEffectsOptions = {
  agentManagerZoomPercent: number;
  customThemeColor: string | undefined;
  effectiveSettings: ghostexSettings;
  theme: string;
};

/*
 * CDXC:RepoStructure 2026-08-22:
 * The two effects that publish sidebar chrome onto <body>: the workspace theme
 * / custom titlebar color variables, and the Agent Manager zoom variable.
 */
export function useSidebarDocumentChromeEffects({
  agentManagerZoomPercent,
  customThemeColor,
  effectiveSettings,
  theme,
}: SidebarDocumentChromeEffectsOptions) {
  useEffect(() => {
    document.body.dataset.sidebarTheme = theme;
    const normalizedThemeColor = normalizeWorkspaceThemeColor(customThemeColor);
    const customSidebarTitlebarForegroundColor = getSidebarTitlebarForegroundForBackground(
      effectiveSettings.customSidebarTitlebarBackgroundColor
    );
    const customSidebarTitlebarGradientColors = getSidebarTitlebarGradientColors(
      effectiveSettings.customSidebarTitlebarBackgroundColor
    );
    if (normalizedThemeColor) {
      /**
       * CDXC:Theming 2026-05-05-02:58
       * Custom workspace colors are active-project sidebar theme overrides:
       * keep the preset data-sidebar-theme as fallback, but publish validated
       * CSS variables so the app-level theme surfaces derive from the color.
       */
      document.body.dataset.sidebarCustomTheme = 'true';
      document.body.style.setProperty('--workspace-sidebar-theme-color', normalizedThemeColor);
      document.body.style.setProperty(
        '--workspace-sidebar-theme-foreground',
        getWorkspaceThemeForeground(normalizedThemeColor)
      );
    } else {
      delete document.body.dataset.sidebarCustomTheme;
      document.body.style.removeProperty('--workspace-sidebar-theme-color');
      document.body.style.removeProperty('--workspace-sidebar-theme-foreground');
    }

    /**
     * CDXC:Theming 2026-08-24:
     * The accent color is a plain always-on chrome token, so publish it next to
     * the theme variables instead of gating it behind the custom chrome toggle.
     */
    document.body.style.setProperty('--ghostex-accent', effectiveSettings.accentColor);

    document.body.dataset.customSidebarTitlebarColors = 'true';
    document.body.style.setProperty('--custom-sidebar-titlebar-foreground-color', customSidebarTitlebarForegroundColor);
    document.body.style.setProperty(
      '--custom-sidebar-titlebar-background-color',
      effectiveSettings.customSidebarTitlebarBackgroundColor
    );
    document.body.style.setProperty(
      '--custom-sidebar-titlebar-gradient-top-color',
      customSidebarTitlebarGradientColors.sidebarTop
    );
    document.body.style.setProperty(
      '--custom-sidebar-titlebar-gradient-bottom-color',
      customSidebarTitlebarGradientColors.sidebarBottom
    );

    return () => {
      delete document.body.dataset.sidebarTheme;
      delete document.body.dataset.sidebarCustomTheme;
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty('--workspace-sidebar-theme-color');
      document.body.style.removeProperty('--workspace-sidebar-theme-foreground');
      document.body.style.removeProperty('--ghostex-accent');
      document.body.style.removeProperty('--custom-sidebar-titlebar-foreground-color');
      document.body.style.removeProperty('--custom-sidebar-titlebar-background-color');
      document.body.style.removeProperty('--custom-sidebar-titlebar-gradient-top-color');
      document.body.style.removeProperty('--custom-sidebar-titlebar-gradient-bottom-color');
    };
  }, [customThemeColor, effectiveSettings.accentColor, effectiveSettings.customSidebarTitlebarBackgroundColor, theme]);

  useEffect(() => {
    document.body.style.setProperty('--ghostex-agent-manager-zoom', `${agentManagerZoomPercent}%`);

    return () => {
      document.body.style.removeProperty('--ghostex-agent-manager-zoom');
    };
  }, [agentManagerZoomPercent]);
}
