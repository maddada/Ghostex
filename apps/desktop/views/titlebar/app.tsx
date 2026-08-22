import {
  IconLayoutSidebar,
  IconLayoutSidebarRight,
} from "@tabler/icons-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { cn } from "@/packages/components/utils";
import { TooltipProvider } from "@/packages/core-ui/app-tooltip";
import { openQuickAccess } from "@/packages/core-ui/app-modal-host-bridge";
import {
  isSidebarCommandConfigured,
  type SidebarCommandButton,
} from "@/packages/shared/sidebar-commands";
import {
  getSidebarTitlebarGradientColors,
  isDiagnosticLoggingScenarioEnabled,
  type KeepAwakeDurationMinutes,
  type SessionPersistenceProvider,
  type WebLinkOpenTarget,
} from "@/packages/shared/ghostex-settings";
import { parseRemoteProjectId } from "@/packages/shared/remote-terminal-selection";
import {
  buildSidebarGitMenuItems,
  hasSidebarGitRemoteCommitDelta,
  resolveSidebarGitPrimaryActionState,
  type SidebarGitAction,
} from "@/packages/shared/sidebar-git";
import type { SidebarTheme } from "@/packages/shared/session-grid-contract";
import {
  GHOSTEX_CHANGELOG_URL,
  GHOSTEX_DISCORD_URL,
  GHOSTEX_DOCS_URL,
  KEEP_AWAKE_LID_SLEEP_STORAGE_KEY,
  KEEP_AWAKE_POWER_CHECK_INTERVAL_MS,
  KEEP_AWAKE_RUNTIME_CHANGED_EVENT,
  KEEP_AWAKE_RUNTIME_STORAGE_KEY,
  KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY,
  KEEP_AWAKE_WORKING_SESSION_GRACE_MS,
  LAST_OPEN_TARGET_STORAGE_KEY,
  RESOURCE_POLL_INTERVAL_MS,
  TITLEBAR_EVENT_LOOP_STALL_LOG_THROTTLE_MS,
  TITLEBAR_EVENT_LOOP_STALL_THRESHOLD_MS,
  TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS,
  TITLEBAR_GRADIENT_BLEND_START_PERCENT,
  createTitlebarDropdownPanelPreferredSize,
  initialTitlebarDropdownPanelKind,
} from "./constants";
import {
  TITLEBAR_DEBUGGING_MODE_NOTICE,
  TITLEBAR_PERSISTENCE_OFF_NOTICE,
  TITLEBAR_TIPS,
  createTitlebarGhostexCliNotice,
  createTitlebarMissingAgentHooksNotice,
} from "./tips-data";
import {
  appendTitlebarActionCrashDebugLog,
  appendTitlebarChromeResponsivenessDebugLog,
  appendTitlebarModeSwitchDebugLog,
  closeAppModalFromTitlebarNavigation,
  enableTitlebarTooltipsFromDom,
  pendingProcessResults,
  postNative,
  postTitlebarSidebarCommand,
  runNativeProcess,
  setTitlebarNativePointerInside,
  setTitlebarWindowFocused,
  suppressTitlebarTooltipsFromDom,
  syncKeepAwakeRuntimeToMainTitlebar,
  titlebarModeSwitchLogDetails,
} from "./native-bridge";
import {
  EMPTY_RESOURCE_GROUP_VIEWS,
  EMPTY_RESOURCE_PROCESS_TOTALS,
  applyResourceItemCollapsedState,
  createGhostexResourceProcessTotals,
  createInactiveTerminalSleepSessionIds,
  createResourceGroupViews,
  createResourceServerBundles,
  createResourceViewItemCollapseTargets,
  isResourceBundleActionable,
  readResourceListeningServers,
  readResourceProcesses,
  resourceBundleProjectEditorIds,
  resourceBundleSidebarSessionIds,
  terminateResourceProcesses,
  uniqueResourceBundles,
} from "./resource-processes";
import {
  applyKeepAwakeLidSleepPrevention,
  cacheTitlebarGitState,
  createInitialProjectState,
  mergeTitlebarProjectState,
  publishKeepAwakeRuntimeSync,
  readKeepAwakePowerSnapshot,
  readKeepAwakeRuntimeSyncState,
  readStoredKeepAwakeRuntime,
  readStoredTitlebarTipIds,
  writeStoredTitlebarTipIds,
} from "./project-state";
import { TitlebarTipsMenu } from "./tips-panel";
import { TitlebarResourcesMenu } from "./resources-panel";
import {
  createConfiguredOpenTargets,
  isRecord,
  persistLastActionCommandId,
  readLastActionCommandId,
  readLastOpenTargetId,
  resolveVisibleOpenTargets,
} from "./settings-io";
import type {
  KeepAwakeRuntimeState,
  KeepAwakeRuntimeSyncState,
  NativeHostEvent,
  ResolvedOpenTarget,
  ResourceGroupView,
  ResourceItemCollapseTarget,
  ResourceListeningServer,
  ResourceProcess,
  ResourceProcessBundle,
  ResourceProcessTotals,
  TitlebarDropdownPanelKind,
  TitlebarDropdownPanelSize,
  TitlebarGxserverDaemonStatus,
  TitlebarKeepAwakeCommand,
  TitlebarMode,
  TitlebarNotice,
  TitlebarProjectState,
  TitlebarRgbColor,
  TitlebarTip,
} from "./types";

export function GhostexTitlebarHost() {
  return <App />;
}

export function App() {
  const bootstrap = window.__ghostex_NATIVE_HOST__ ?? {};
  const titlebarPanelKind = useMemo(() => initialTitlebarDropdownPanelKind, []);
  const isDropdownPanel = titlebarPanelKind !== undefined;
  const [projectState, setProjectState] = useState<TitlebarProjectState>(() =>
    createInitialProjectState(bootstrap),
  );
  const [selectedTargetId, setSelectedTargetId] = useState(() => readLastOpenTargetId());
  const [selectedActionCommandId, setSelectedActionCommandId] = useState(() =>
    readLastActionCommandId(createInitialProjectState(bootstrap)),
  );
  const [nativeDropdownOpen, setNativeDropdownOpen] = useState<TitlebarDropdownPanelKind | undefined>();
  const dropdownPanelSizeResolverRef = useRef<(kind: TitlebarDropdownPanelKind) => TitlebarDropdownPanelSize>(
    (kind) => createTitlebarDropdownPanelPreferredSize(kind),
  );
  const [readTipIds, setReadTipIds] = useState<Set<string>>(() => readStoredTitlebarTipIds());
  /*
   * CDXC:ReactTitlebar 2026-06-11-13:22:
   * Dropdown content now lives in native child windows, so the main titlebar
   * WKWebView must never publish a below-titlebar overlay-open state or trigger
   * the workspace interaction shield.
   */
  const titlebarOverlayOpen = false;
  const [keepAwakeRuntime, setKeepAwakeRuntime] = useState<KeepAwakeRuntimeState | undefined>(
    () => readStoredKeepAwakeRuntime(),
  );
  const [keepAwakeAutoStartSuppressed, setKeepAwakeAutoStartSuppressed] = useState(false);
  const [keepAwakeWorkingSessionGraceUntilMs, setKeepAwakeWorkingSessionGraceUntilMs] =
    useState<number | undefined>();
  const previousKeepAwakeWorkingSessionCountRef = useRef(projectState.keepAwake.workingSessionCount);
  const [resourceProcesses, setResourceProcesses] = useState<ResourceProcess[]>([]);
  /*
   * CDXC:SidebarCollapse 2026-06-20-17:10:
   * The macOS titlebar Toggle Sidebar control should be a plain Tabler sidebar
   * glyph instead of the former blue traffic-light dot. Mirror the configured
   * sidebar placement so left sidebars use IconLayoutSidebar and right sidebars
   * use IconLayoutSidebarRight.
   */
  const SidebarCollapseIcon =
    projectState.sidebarSide === "right" ? IconLayoutSidebarRight : IconLayoutSidebar;
  const keepAwakeFeatureEnabled = projectState.keepAwake.featureEnabled === true;
  const [resourceServers, setResourceServers] = useState<ResourceListeningServer[]>([]);
  /*
   * CDXC:TitlebarResources 2026-06-11-18:13:
   * The native Resources child panel should not render zero-memory or missing-session rows while the first `ps` snapshot is still loading.
   * Track first-sample readiness separately from the process array so an intentionally empty process sample can render, while AppKit keeps the child window hidden until the first real sample is committed.
   */
  const [ resourceProcessSnapshotReady, setResourceProcessSnapshotReady ] = useState(false);
  const [collapsedResourceKeys, setCollapsedResourceKeys] = useState<Set<string>>(() => {
    /**
     * CDXC:TitlebarResources 2026-06-12-23:33:
     * Resource section containers stay visible; only individual row disclosures
     * collapse. Session and browser rows encode their default collapsed state by
     * omitting their expanded keys, so the explicit override set starts empty.
     */
    return new Set();
  });
  const [quittingResourceKeys, setQuittingResourceKeys] = useState<Set<string>>(() => new Set());
  const [optimisticMode, setOptimisticMode] = useState<TitlebarMode>();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const resourceRefreshGenerationRef = useRef(0);
  const resourceRefreshInFlightRef = useRef(false);
  const resourcesOpenCollapseSeededRef = useRef(false);
  const titlebarEventLoopLastLogAtRef = useRef(0);
  /*
   * CDXC:TitlebarResources 2026-07-02-05:36:
   * refreshResources must keep a stable identity across project-state pushes.
   * When it depended on projectState.diagnosticLogging, the state hydrate that
   * arrives right after the Resources child panel loads re-ran the poll effect,
   * bumped the refresh generation, and discarded the first process snapshot —
   * so titlebarDropdownPanelReady was never posted and AppKit kept the panel
   * hidden. Diagnostics read the latest settings through this ref instead.
   */
  const diagnosticLoggingRef = useRef(projectState.diagnosticLogging);
  diagnosticLoggingRef.current = projectState.diagnosticLogging;
  const activeMode = optimisticMode ?? projectState.activeMode;
  const resourcesPanelActive = titlebarPanelKind === "resources";
  const resourceViews = useMemo(
    () =>
      resourcesPanelActive
        ? createResourceGroupViews(
            projectState.browserTabs,
            projectState.resourceGroups,
            resourceProcesses,
            resourceServers,
            projectState.codeEditorProjectIds,
          )
        : EMPTY_RESOURCE_GROUP_VIEWS,
    [
      projectState.browserTabs,
      projectState.codeEditorProjectIds,
      projectState.resourceGroups,
      resourceProcesses,
      resourceServers,
      resourcesPanelActive,
    ],
  );
  const resourceProcessTotals = useMemo(
    () =>
      resourcesPanelActive
        ? createGhostexResourceProcessTotals(resourceProcesses)
        : EMPTY_RESOURCE_PROCESS_TOTALS,
    [resourceProcesses, resourcesPanelActive],
  );
  const resourceServerBundles = useMemo(
    () =>
      resourcesPanelActive
        ? createResourceServerBundles(resourceServers, resourceViews, resourceProcesses, projectState.portless)
        : [],
    [projectState.portless, resourceProcesses, resourceServers, resourceViews, resourcesPanelActive],
  );
  const inactiveTerminalSleepSessionIds = useMemo(
    () => createInactiveTerminalSleepSessionIds(projectState.resourceGroups),
    [projectState.resourceGroups],
  );
  const unreadTips = useMemo(
    () => TITLEBAR_TIPS.filter((tip) => !readTipIds.has(tip.id)),
    [readTipIds],
  );
  const readTips = useMemo(
    () => TITLEBAR_TIPS.filter((tip) => readTipIds.has(tip.id)),
    [readTipIds],
  );
  const missingAgentHooksNotice = useMemo(
    () => createTitlebarMissingAgentHooksNotice(projectState.resourceGroups, projectState.agentHookStatus),
    [projectState.agentHookStatus, projectState.resourceGroups],
  );
  const ghostexCliNotice = useMemo(
    () => createTitlebarGhostexCliNotice(projectState.ghostexCliStatus),
    [projectState.ghostexCliStatus],
  );
  const notices = useMemo(
    () => [
      ...(ghostexCliNotice ? [ghostexCliNotice] : []),
      ...(projectState.sessionPersistenceProvider === "off"
        ? [TITLEBAR_PERSISTENCE_OFF_NOTICE]
        : []),
      ...(projectState.debuggingMode ? [TITLEBAR_DEBUGGING_MODE_NOTICE] : []),
      ...(missingAgentHooksNotice ? [missingAgentHooksNotice] : []),
    ],
    [
      ghostexCliNotice,
      missingAgentHooksNotice,
      projectState.debuggingMode,
      projectState.sessionPersistenceProvider,
    ],
  );
  const markTipRead = useCallback((tipId: string) => {
    setReadTipIds((current) => {
      if (current.has(tipId)) {
        return current;
      }
      const next = new Set(current);
      next.add(tipId);
      writeStoredTitlebarTipIds(next);
      return next;
    });
  }, []);
  const requestRuntimeStatusForTips = useCallback(() => {
    postTitlebarSidebarCommand({ type: "requestAgentHookStatus" });
    postTitlebarSidebarCommand({ type: "requestGhostexCliStatus" });
  }, []);
  const openHighlightedFeaturesFromTips = useCallback(() => {
    /*
     * CDXC:TipsAndTricks 2026-06-16-08:17:
     * The Tips & Tricks header should send users to the replayable highlighted
     * features modal instead of exposing a bulk "Read all" action.
     *
     * CDXC:GhostexTutorialVideo 2026-06-18-05:31:
     * The Tips modal Video button should open the tutorial video modal. Leave the
     * old Highlighted Features modal unused instead of deleting its implementation.
     */
    postTitlebarSidebarCommand({ type: "openGhostexTutorialVideo" });
  }, []);
  const viewGhostexGuideFromTips = useCallback(() => {
    /*
     * CDXC:TipsAndTricks 2026-06-16-10:04:
     * The Tips & Tricks header should send users to Video with a filled star
     * action and to the setup guide through a Setup action. Keep the
     * sidebar-owned workspace welcome bridge as the guide entry point because
     * that surface owns setup and onboarding repair.
     *
     * CDXC:TipsAndTricks 2026-06-18-04:53:
     * The setup action label should be the shorter "Setup" copy so the header
     * can also fit Docs, Video, and Updates without truncating action text.
     */
    postTitlebarSidebarCommand({ type: "openWorkspaceWelcome" });
  }, []);
  const openDocsFromTips = useCallback(() => {
    postTitlebarSidebarCommand({ type: "openBrowserPane", url: GHOSTEX_DOCS_URL });
  }, []);
  const openTipAction = useCallback((tip: TitlebarTip) => {
    const action = tip.action;
    if (!action) {
      return;
    }
    if (action.type === "openSettings") {
      /*
       * CDXC:TipsAndTricks 2026-06-28-08:00:
       * Clickable Ghostex skill tips should open Settings > Integrations with
       * the skill name searched so users land on the install/configure detail
       * instead of a generic setup page.
       */
      window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
        initialSearchQuery: action.settingsSearchQuery,
        initialTab: "integrations",
        modal: "settings",
        type: "open",
      });
      return;
    }
    postTitlebarSidebarCommand({ type: "openBrowserPane", url: action.url });
  }, []);
  const openChangelogFromTips = useCallback(() => {
    /*
     * CDXC:TipsAndTricks 2026-06-16-19:42:
     * The Tips & Tricks header should expose the release changelog on the far
     * right and open it as a normal current-project browser session.
     */
    postTitlebarSidebarCommand({ type: "openBrowserPane", url: GHOSTEX_CHANGELOG_URL });
  }, []);
  const syncKeepAwakeRuntimeState = useCallback(
    (syncState: KeepAwakeRuntimeSyncState | undefined) => {
      if (syncState && Object.prototype.hasOwnProperty.call(syncState, "runtime")) {
        /*
         * CDXC:TitlebarKeepAwake 2026-06-23-19:36:
         * Native child dropdowns send the committed Keep Awake runtime directly into the main titlebar bridge. Treat an explicit null runtime as a committed stop so stale localStorage in another WKWebView cannot keep the titlebar icon active.
         */
        setKeepAwakeRuntime(syncState.runtime ?? undefined);
        setKeepAwakeAutoStartSuppressed(syncState.suppressAutoStart === true);
        return;
      }
      if (syncState?.suppressAutoStart === true) {
        setKeepAwakeRuntime(undefined);
        setKeepAwakeAutoStartSuppressed(true);
        return;
      }
      const storedRuntime = readStoredKeepAwakeRuntime();
      setKeepAwakeRuntime(storedRuntime);
      if (syncState?.suppressAutoStart === false || storedRuntime) {
        setKeepAwakeAutoStartSuppressed(false);
      }
    },
    [],
  );
  const closeTitlebarDropdownPanel = useCallback(() => {
    postNative({ type: "closeTitlebarDropdownPanel" });
    setNativeDropdownOpen(undefined);
  }, []);
  useEffect(() => {
    if (!isDropdownPanel) {
      return;
    }

    const closePanelWhenNativeFocusLeaves = () => {
      /*
       * GPUI titlebar panels are native CEF siblings of the app's normal
       * workspace surfaces. A click in the sidebar, browser, terminal, or
       * GPUI shell blurs this browsing context. Close from that exact surface
       * lifecycle instead of installing a broad native mouse monitor.
       */
      closeTitlebarDropdownPanel();
    };

    window.addEventListener("blur", closePanelWhenNativeFocusLeaves);
    return () => {
      window.removeEventListener("blur", closePanelWhenNativeFocusLeaves);
    };
  }, [closeTitlebarDropdownPanel, isDropdownPanel]);
  const showTitlebarDropdownPanel = useCallback(
    (
      kind: TitlebarDropdownPanelKind,
      anchor: HTMLElement,
      options: { closeWhenAlreadyOpen?: boolean } = {},
    ) => {
      /*
       * CDXC:ReactTitlebar 2026-06-11-23:20:
       * Native child-window dropdown triggers should behave like normal menu
       * buttons: requesting the already-open panel closes it instead of
       * reopening or repositioning the same child window.
       *
       * CDXC:TitlebarKeepAwake 2026-06-15-23:25:
       * Keep Awake is a dropdown launcher, not a direct start/stop toggle.
       *
       * CDXC:TitlebarKeepAwake 2026-06-15-23:25:
       * Clicking Keep Awake again while its dropdown is open should close the
       * menu like the other titlebar dropdown triggers.
       */
      if (nativeDropdownOpen === kind && options.closeWhenAlreadyOpen !== false) {
        closeTitlebarDropdownPanel();
        return false;
      }
      const anchorElement =
        anchor.closest<HTMLElement>("[data-titlebar-dropdown-anchor]") ?? anchor;
      const rect = anchorElement.getBoundingClientRect();
      /*
       * CDXC:ReactTitlebar 2026-06-11-13:22:
       * Dropdown content must open as a native child window, not as Radix content
       * portaled below the titlebar WKWebView. Send only the titlebar-strip anchor
       * rectangle so Swift owns screen placement while React keeps rendering the
       * existing menu surface inside the child window.
       */
      setNativeDropdownOpen(kind);
      postNative({
        anchorRect: {
          height: rect.height,
          width: rect.width,
          x: rect.x,
          y: rect.y,
        },
        kind,
        preferredSize: dropdownPanelSizeResolverRef.current(kind),
        type: "showTitlebarDropdownPanel",
      });
      return true;
    },
    [closeTitlebarDropdownPanel, nativeDropdownOpen],
  );
  const openTipsMenuFromTitlebar = useCallback((event: { currentTarget: HTMLElement }) => {
    const didOpen = showTitlebarDropdownPanel("tips", event.currentTarget);
    if (didOpen) {
      requestRuntimeStatusForTips();
    }
  }, [requestRuntimeStatusForTips, showTitlebarDropdownPanel]);

  const requestTitlebarBlankMouseDown = useCallback(
    (event: ReactMouseEvent<HTMLDivElement>) => {
      if (isDropdownPanel || event.button !== 0 || event.defaultPrevented) {
        return;
      }
      const target = event.target;
      if (!(target instanceof Element)) {
        return;
      }
      /*
       * CDXC:ReactTitlebar 2026-06-13-14:08:
       * Blank titlebar drag should use normal DOM event ownership instead of
       * native coordinate hit regions. Interactive controls stop here by their
       * element semantics; passive titlebar text and empty background ask the
       * native WKWebView to drag the current mouseDown event.
       */
      if (
        target.closest(
          'button,a,input,textarea,select,[role="button"],[contenteditable="true"],[data-titlebar-dropdown-anchor]',
        )
      ) {
        return;
      }
      if (nativeDropdownOpen) {
        /*
         * CDXC:ReactTitlebar 2026-06-23-19:36:
         * Clicking blank titlebar chrome while a native child dropdown is open should dismiss that dropdown instead of starting a window drag. Keep this in the titlebar DOM mouse handler so AppKit does not need broad click rerouting or overlapping hit-test regions.
         */
        event.preventDefault();
        closeTitlebarDropdownPanel();
        return;
      }
      event.preventDefault();
      postNative({ type: "titlebarBlankMouseDown" });
    },
    [closeTitlebarDropdownPanel, isDropdownPanel, nativeDropdownOpen],
  );

  useEffect(() => {
    const suppressTitlebarWebviewContextMenu = (event: MouseEvent) => {
      /**
       * CDXC:TitlebarContextMenu 2026-05-15-18:21:
       * Right-clicking titlebar buttons, menus, labels, or project text must
       * not expose WKWebView's native Reload menu. The titlebar has no editable
       * text fields, so suppress the webview default for the whole isolated
       * titlebar document while leaving React click/keyboard behavior intact.
       */
      event.preventDefault();
    };

    document.addEventListener("contextmenu", suppressTitlebarWebviewContextMenu, true);
    return () => {
      document.removeEventListener("contextmenu", suppressTitlebarWebviewContextMenu, true);
    };
  }, []);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    const narrowTitlebarMedia = window.matchMedia("(max-width: 619.98px)");
    const closeMenusHiddenAtNarrowWidth = () => {
      /**
       * CDXC:ReactTitlebar 2026-05-29-16:05:
       * App widths below 620px hide the top-right Tips, Resources, and Keep
       * Awake controls.
       *
       * CDXC:ReactTitlebar 2026-06-11-13:22:
       * Those dropdowns are native child panels now, so close the panel when its
       * trigger leaves the visible titlebar instead of keeping an orphan window.
       */
      if (
        narrowTitlebarMedia.matches &&
        (nativeDropdownOpen === "resources" || nativeDropdownOpen === "tips")
      ) {
        closeTitlebarDropdownPanel();
      }
    };
    closeMenusHiddenAtNarrowWidth();
    narrowTitlebarMedia.addEventListener("change", closeMenusHiddenAtNarrowWidth);
    return () => {
      narrowTitlebarMedia.removeEventListener("change", closeMenusHiddenAtNarrowWidth);
    };
  }, [closeTitlebarDropdownPanel, isDropdownPanel, nativeDropdownOpen]);

  const allTargets = useMemo(
    () => createConfiguredOpenTargets(projectState.workspaceOpenTargets),
    [projectState.workspaceOpenTargets],
  );
  const visibleTargets = useMemo(
    () => resolveVisibleOpenTargets(allTargets, projectState.workspaceOpenTargets.availability),
    [allTargets, projectState.workspaceOpenTargets.availability],
  );
  const activeTarget = visibleTargets.find((target) => target.id === selectedTargetId) ?? visibleTargets[0];
  const visibleActions = useMemo(
    () => projectState.sidebarActions.commands,
    [projectState.sidebarActions.commands],
  );
  const activeAction =
    visibleActions.find((command) => command.commandId === selectedActionCommandId) ??
    visibleActions[0];
  const gitPrimaryAction = useMemo(
    () => resolveSidebarGitPrimaryActionState(projectState.git),
    [projectState.git],
  );
  const gitMenuItems = useMemo(
    () => buildSidebarGitMenuItems(projectState.git),
    [projectState.git],
  );
  const publishTitlebarStripState = useCallback(() => {
    if (isDropdownPanel) {
      return;
    }
    /**
     * CDXC:ReactTitlebar 2026-06-13-13:33:
     * Native owns the titlebar as an exact WKWebView strip, while React owns
     * controls through normal DOM layout. Do not measure DOM hit rectangles for
     * AppKit; only publish strip-level overlay lifecycle state.
     */
    postNative({
      overlayOpen: titlebarOverlayOpen,
      type: "setReactTitlebarStripState",
    });
  }, [titlebarOverlayOpen, isDropdownPanel]);

  const publishSettledTitlebarStripState = useCallback(() => {
    publishTitlebarStripState();
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      publishTitlebarStripState();
      secondFrame = window.requestAnimationFrame(publishTitlebarStripState);
    });
    const settledTimeout = window.setTimeout(publishTitlebarStripState, 120);
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame !== 0) {
        window.cancelAnimationFrame(secondFrame);
      }
      window.clearTimeout(settledTimeout);
    };
  }, [publishTitlebarStripState]);

  useLayoutEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    /**
     * CDXC:SessionFocusMode 2026-05-26-22:47:
     * The Exit focus button is conditional titlebar chrome. Publish the strip
     * lifecycle state after titlebar layout settles so native receives a fresh
     * titlebar document signal without DOM-region measuring.
     *
     * CDXC:AutoUpdate 2026-06-08-18:21:
     * The update button appears after native Sparkle appcast probes, so
     * updateAvailable must also republish the strip lifecycle state.
     */
    return publishSettledTitlebarStripState();
  }, [
    activeTarget?.id,
    activeAction?.commandId,
    keepAwakeRuntime?.pid,
    resourceProcesses.length,
    resourceServers.length,
    projectState.projectEditorCompanionPaneHidden,
    projectState.gxserverDaemon.state,
    projectState.projectIconDataUrl,
    projectState.isFocusModeActive,
    projectState.projectName,
    projectState.sidebarCollapsed,
    projectState.updateAvailable,
    projectState.updateDownloading,
    publishSettledTitlebarStripState,
    isDropdownPanel,
  ]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    window.addEventListener("resize", publishTitlebarStripState);
    return () => window.removeEventListener("resize", publishTitlebarStripState);
  }, [publishTitlebarStripState, isDropdownPanel]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    /*
     * CDXC:TooltipLifecycle 2026-06-13-02:30:
     * Native titlebar pointer-leave may hide a currently visible tooltip, but
     * DOM pointer movement inside the titlebar must immediately restore hover
     * eligibility. This keeps native tracking as cleanup, not a persistent gate
     * that waits for a titlebar ownership update.
     */
    const suppressTitlebarTooltips = () => {
      suppressTitlebarTooltipsFromDom();
    };
    const enableTitlebarTooltips = () => {
      enableTitlebarTooltipsFromDom();
    };
    const suppressWhenHidden = () => {
      if (document.visibilityState !== "visible") {
        suppressTitlebarTooltips();
      }
    };
    const suppressWhenPointerLeavesDocument = (event: MouseEvent | PointerEvent) => {
      const relatedTarget = event.relatedTarget;
      if (!(relatedTarget instanceof Node) || !document.documentElement.contains(relatedTarget)) {
        suppressTitlebarTooltips();
      }
    };

    window.addEventListener("blur", suppressTitlebarTooltips);
    window.addEventListener("pagehide", suppressTitlebarTooltips);
    document.addEventListener("visibilitychange", suppressWhenHidden);
    document.addEventListener("mouseout", suppressWhenPointerLeavesDocument, true);
    document.addEventListener("pointerout", suppressWhenPointerLeavesDocument, true);
    document.addEventListener("pointercancel", suppressTitlebarTooltips, true);
    document.addEventListener("mouseenter", enableTitlebarTooltips, true);
    document.addEventListener("pointerenter", enableTitlebarTooltips, true);
    document.addEventListener("pointermove", enableTitlebarTooltips, true);

    return () => {
      window.removeEventListener("blur", suppressTitlebarTooltips);
      window.removeEventListener("pagehide", suppressTitlebarTooltips);
      document.removeEventListener("visibilitychange", suppressWhenHidden);
      document.removeEventListener("mouseout", suppressWhenPointerLeavesDocument, true);
      document.removeEventListener("pointerout", suppressWhenPointerLeavesDocument, true);
      document.removeEventListener("pointercancel", suppressTitlebarTooltips, true);
      document.removeEventListener("mouseenter", enableTitlebarTooltips, true);
      document.removeEventListener("pointerenter", enableTitlebarTooltips, true);
      document.removeEventListener("pointermove", enableTitlebarTooltips, true);
      delete document.body.dataset.nativePointerInside;
    };
  }, [isDropdownPanel]);

  useEffect(() => {
    if (isDropdownPanel) {
      return;
    }
    return () => {
      /**
       * CDXC:ReactTitlebar 2026-05-25-10:09:
       * Native workspace shielding must clear when the titlebar host unmounts
       * or reloads. Publish an explicit closed overlay state instead of making
       * Swift infer it from stale DOM geometry.
       */
      postNative({
        overlayOpen: false,
        type: "setReactTitlebarStripState",
      });
    };
  }, [isDropdownPanel]);

  useEffect(() => {
    window.__ghostex_TITLEBAR__ = {
      closeOpenDropdowns: () => {
        /**
         * CDXC:ReactTitlebar 2026-05-16-20:01:
         * Native app content lives outside this titlebar WKWebView, so Radix
         * cannot observe normal outside clicks in the workspace/sidebar. Expose
         * one explicit close hook that AppKit can call before routing the click
         * to the real app surface behind an open dropdown.
         *
         * CDXC:ReactTitlebar 2026-06-11-13:22:
         * Titlebar dropdowns are now native child windows, so this bridge closes
         * the panel window instead of toggling in-document Radix menu state.
         */
        closeTitlebarDropdownPanel();
      },
      setNativePointerInside: setTitlebarNativePointerInside,
      setWindowFocused: setTitlebarWindowFocused,
      setNativeDropdownOpen,
      syncKeepAwakeRuntime: syncKeepAwakeRuntimeState,
      setLastActionCommandId: (commandId) => {
        /*
         * CDXC:TitlebarActions 2026-06-16-18:31:
         * Quick Actions run from the native dropdown panel must immediately
         * become the main titlebar button action. The dropdown is a separate
         * WKWebView, so native relays the chosen command id back into the main
         * titlebar bridge instead of waiting for a reload to reread localStorage.
         */
        setSelectedActionCommandId(commandId);
      },
      setActiveProjectState: (state) => {
        setProjectState((current) => {
          const next = mergeTitlebarProjectState(current, state);
          cacheTitlebarGitState(next);
          return next;
        });
      },
    };
    if (isRecord(window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__)) {
      window.__ghostex_TITLEBAR__.setActiveProjectState(window.__ghostex_PENDING_TITLEBAR_PROJECT_STATE__);
    }
    if (typeof window.__ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__ === "boolean") {
      /**
       * CDXC:AutoUpdate 2026-06-08-18:21:
       * Native may detect an app update before this React bridge exists. Apply
       * the latest pending native boolean immediately after bridge installation
       * so the titlebar download button appears during startup instead of only
       * after a later 15-minute probe.
       */
      window.__ghostex_TITLEBAR__.setActiveProjectState({
        updateAvailable: window.__ghostex_PENDING_TITLEBAR_UPDATE_AVAILABLE__,
      });
    }
    if (typeof window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOADING__ === "boolean") {
      /**
       * CDXC:AutoUpdate 2026-06-13-17:52:
       * Native may start the Sparkle download before this React bridge exists.
       * Apply the pending boolean immediately so the titlebar button begins
       * showing download state as soon as the document can render the current
       * updater state.
       *
       * CDXC:AutoUpdate 2026-06-30-22:18:
       * Apply the pending nullable progress ratio with the downloading boolean
       * so titlebar reloads preserve the circular fill and hover percent.
       */
      const pendingDownloadState: Partial<TitlebarProjectState> = {
        updateDownloading: window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOADING__,
      };
      if (
        Object.prototype.hasOwnProperty.call(
          window,
          "__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__",
        )
      ) {
        pendingDownloadState.updateDownloadProgress =
          window.__ghostex_PENDING_TITLEBAR_UPDATE_DOWNLOAD_PROGRESS__ ?? null;
      }
      window.__ghostex_TITLEBAR__.setActiveProjectState(pendingDownloadState);
    }
    if (typeof window.__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__ === "boolean") {
      setTitlebarWindowFocused(window.__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__);
    }
    return () => {
      delete window.__ghostex_TITLEBAR__;
      delete document.body.dataset.windowFocused;
    };
  }, [closeTitlebarDropdownPanel, syncKeepAwakeRuntimeState]);

  useEffect(() => {
    setSelectedActionCommandId(readLastActionCommandId(projectState));
  }, [projectState.projectId, projectState.projectPath]);

  useEffect(() => {
    setOptimisticMode(undefined);
  }, [projectState.activeMode, projectState.projectId, projectState.projectPath]);

  useEffect(() => {
    const handleHostEvent = (event: Event) => {
      const hostEvent = (event as CustomEvent<NativeHostEvent>).detail;
      if (hostEvent?.type !== "processResult") {
        return;
      }
      const pending = pendingProcessResults.get(hostEvent.requestId);
      if (!pending) {
        return;
      }
      window.clearTimeout(pending.timeout);
      pendingProcessResults.delete(hostEvent.requestId);
      pending.resolve(hostEvent);
    };
    window.addEventListener("ghostex-native-host-event", handleHostEvent);
    return () => window.removeEventListener("ghostex-native-host-event", handleHostEvent);
  }, []);

  useEffect(() => {
    if (!isDiagnosticLoggingScenarioEnabled(projectState.diagnosticLogging, "native.chrome.responsiveness")) {
      return;
    }
    /*
     * CDXC:ChromeResponsivenessDiagnostics 2026-06-30-23:52:
     * When the titlebar buttons stop responding, the isolated titlebar React
     * event loop may have stalled before WebKit terminates. Sample coarse timer
     * drift only while the targeted diagnostic scenario is enabled, and throttle
     * writes so the watchdog cannot become another source of lag.
     */
    let expectedAtMs = performance.now() + TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS;
    const interval = window.setInterval(() => {
      const nowMs = performance.now();
      const driftMs = nowMs - expectedAtMs;
      expectedAtMs = nowMs + TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS;
      if (driftMs < TITLEBAR_EVENT_LOOP_STALL_THRESHOLD_MS) {
        return;
      }
      if (nowMs - titlebarEventLoopLastLogAtRef.current < TITLEBAR_EVENT_LOOP_STALL_LOG_THROTTLE_MS) {
        return;
      }
      titlebarEventLoopLastLogAtRef.current = nowMs;
      appendTitlebarChromeResponsivenessDebugLog(
        projectState.diagnosticLogging,
        "nativeChrome.titlebar.eventLoopStall",
        {
          driftMs: Math.round(driftMs),
          intervalMs: TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS,
          resourceProcessCount: resourceProcesses.length,
          resourceRefreshInFlight: resourceRefreshInFlightRef.current,
          resourceServerCount: resourceServers.length,
          resourcesPanelActive,
          snapshotReady: resourceProcessSnapshotReady,
          titlebarPanelKind: titlebarPanelKind ?? "main",
        },
      );
    }, TITLEBAR_EVENT_LOOP_WATCHDOG_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [
    projectState.diagnosticLogging,
    resourceProcesses.length,
    resourceProcessSnapshotReady,
    resourceServers.length,
    resourcesPanelActive,
    titlebarPanelKind,
  ]);

  const refreshResources = useCallback(async (generation: number) => {
    if (resourceRefreshInFlightRef.current) {
      appendTitlebarChromeResponsivenessDebugLog(
        diagnosticLoggingRef.current,
        "nativeChrome.titlebar.resourcesRefresh.skippedInFlight",
        {
          generationCurrent: generation === resourceRefreshGenerationRef.current,
          resourcesPanelActive,
        },
      );
      return;
    }
    resourceRefreshInFlightRef.current = true;
    const startedAtMs = performance.now();
    try {
      const [processes, servers] = await Promise.all([
        readResourceProcesses(),
        readResourceListeningServers(),
      ]);
      const elapsedMs = Math.round(performance.now() - startedAtMs);
      appendTitlebarChromeResponsivenessDebugLog(
        diagnosticLoggingRef.current,
        "nativeChrome.titlebar.resourcesRefresh.finished",
        {
          elapsedMs,
          generationCurrent: generation === resourceRefreshGenerationRef.current,
          processCount: processes.length,
          resourcesPanelActive,
          serverCount: servers.length,
        },
      );
      if (generation === resourceRefreshGenerationRef.current) {
        setResourceProcesses(processes);
        setResourceServers(servers);
        setResourceProcessSnapshotReady(true);
      }
    } catch (error) {
      appendTitlebarChromeResponsivenessDebugLog(
        diagnosticLoggingRef.current,
        "nativeChrome.titlebar.resourcesRefresh.failed",
        {
          elapsedMs: Math.round(performance.now() - startedAtMs),
          errorName: error instanceof Error ? error.name : typeof error,
          generationCurrent: generation === resourceRefreshGenerationRef.current,
          resourcesPanelActive,
        },
      );
      console.warn("Failed to refresh Ghostex resources", error);
      if (generation === resourceRefreshGenerationRef.current) {
        setResourceProcessSnapshotReady(true);
      }
    } finally {
      resourceRefreshInFlightRef.current = false;
    }
  }, [resourcesPanelActive]);

  useEffect(() => {
    if (!resourcesPanelActive) {
      return;
    }
    /**
     * CDXC:TitlebarResources 2026-05-16-16:08:
     * The Resources dropdown should show live process CPU and memory without a
     * native push channel. Poll `ps` only while the wide dropdown is open so
     * the compact titlebar does not spend idle work on hidden diagnostics.
     *
     * CDXC:TitlebarResources 2026-06-07-16:20:
     * Hidden Resources UI should hold no sampled process table and should never
     * stack overlapping `ps` runs. Treat each open as a generation so slow native
     * process replies cannot repopulate closed-menu state.
     *
     * CDXC:TitlebarResources 2026-06-11-18:13:
     * Each native dropdown open clears readiness so AppKit waits for the current
     * first process sample before revealing the Resources child window.
     */
    const generation = resourceRefreshGenerationRef.current + 1;
    resourceRefreshGenerationRef.current = generation;
    setResourceProcessSnapshotReady(false);
    void refreshResources(generation);
    const interval = window.setInterval(() => {
      void refreshResources(generation);
    }, RESOURCE_POLL_INTERVAL_MS);
    return () => {
      window.clearInterval(interval);
      resourceRefreshGenerationRef.current += 1;
      setResourceProcessSnapshotReady(false);
      setResourceProcesses((current) => current.length === 0 ? current : []);
      setResourceServers((current) => current.length === 0 ? current : []);
    };
  }, [refreshResources, resourcesPanelActive]);

  useEffect(() => {
    if (titlebarPanelKind !== "resources" || !resourceProcessSnapshotReady) {
      return;
    }
    /*
     * CDXC:TitlebarResources 2026-06-11-18:13:
     * The native Resources panel is loaded offscreen until React has committed
     * the first real process snapshot. Report readiness from an effect so AppKit
     * orders the child window onscreen after the non-loading content is painted.
     */
    postNative({ kind: "resources", type: "titlebarDropdownPanelReady" });
  }, [resourceProcessSnapshotReady, titlebarPanelKind]);

  useLayoutEffect(() => {
    if (!resourcesPanelActive) {
      resourcesOpenCollapseSeededRef.current = false;
      return;
    }
    if (resourcesOpenCollapseSeededRef.current) {
      return;
    }
    const resourceItemCollapseTargets = createResourceViewItemCollapseTargets(resourceViews, resourceServerBundles);
    if (resourceItemCollapseTargets.length === 0) {
      return;
    }
    /**
     * CDXC:TitlebarResources 2026-06-13-02:02:
     * Each Resources modal open should begin with every expandable item row
     * collapsed, then show the expand action because all rows are collapsed.
     * Do this once per open in a layout effect after the dynamic process
     * snapshot has row targets, before the Resources child window receives its
     * ready signal. Keep Projects, Browser Tabs, and Orphaned / Detached visible
     * as top-level sections.
     */
    resourcesOpenCollapseSeededRef.current = true;
    setCollapsedResourceKeys((current) =>
      applyResourceItemCollapsedState(current, resourceItemCollapseTargets, true),
    );
  }, [ resourceServerBundles, resourceViews, resourcesPanelActive ]);

  const openTarget = (target: ResolvedOpenTarget | undefined) => {
    if (!target || !projectState.projectPath) {
      return;
    }
    setSelectedTargetId(target.id);
    localStorage.setItem(LAST_OPEN_TARGET_STORAGE_KEY, target.id);
    if (target.id === "finder") {
      postNative({ type: "openWorkspaceInFinder", workspacePath: projectState.projectPath });
      return;
    }
    if (target.kind === "built-in") {
      const targetApp = target.definition.targetApp;
      if (targetApp && target.resolvedCommand) {
        postNative({
          targetApp,
          type: "openWorkspaceInIde",
          workspacePath: projectState.projectPath,
        });
        return;
      }
      const command = target.resolvedCommand ?? target.definition.commands?.[0];
      if (target.resolvedCommand) {
        void runNativeProcess("/usr/bin/env", [
          target.resolvedCommand,
          ...(target.definition.baseArgs ?? []),
          projectState.projectPath,
        ]);
      } else if (target.resolvedAppName) {
        void runNativeProcess("/usr/bin/open", ["-a", target.resolvedAppName, projectState.projectPath]);
      } else if (command) {
        void runNativeProcess("/usr/bin/env", [
          command,
          ...(target.definition.baseArgs ?? []),
          projectState.projectPath,
        ]);
      }
      return;
    }
    void runNativeProcess("/usr/bin/env", [
      target.command,
      ...target.custom.args,
      projectState.projectPath,
    ]);
  };

  const openSidebarActionsSettings = () => {
    /*
    CDXC:ProjectActions 2026-06-15-15:29:
    Empty or unconfigured titlebar Actions clicks should open Settings on the Actions page instead of showing the removed standalone Configure Action modal.
    */
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarActionsSettings");
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialTab: "actions",
      modal: "settings",
      type: "open",
    });
  };

  const runSidebarAction = (command: SidebarCommandButton | undefined) => {
    if (!command) {
      openSidebarActionsSettings();
      return;
    }
    if (!isSidebarCommandConfigured(command)) {
      openSidebarActionsSettings();
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarAction");
    appendTitlebarActionCrashDebugLog(
      projectState.diagnosticLogging,
      "nativeSidebar.actionCrashTrace.titlebarClick",
      {
        actionType: command.actionType,
        closeTerminalOnExit: command.closeTerminalOnExit,
        commandId: command.commandId,
        hasCommand: Boolean(command.command?.trim()),
        hasUrl: Boolean(command.url?.trim()),
        projectId: projectState.projectId,
        projectPath: projectState.projectPath,
      },
    );
    setSelectedActionCommandId(command.commandId);
    persistLastActionCommandId(projectState, command.commandId);
    postNative({ commandId: command.commandId, type: "runSidebarCommandFromTitlebar" });
  };

  const runGitAction = (action: SidebarGitAction) => {
    /*
     * CDXC:TitlebarGit 2026-06-16-18:41:
     * If the Commits row shows no remote delta, a stale titlebar child-window
     * click should be inert instead of starting an unnecessary pull/push flow.
     */
    if (action === "syncRemote" && !hasSidebarGitRemoteCommitDelta(projectState.git)) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarGitAction");
    postNative({ action, type: "runSidebarGitActionFromTitlebar" });
  };
  const openTitlebarSettingsMenuSettings = () => {
    /*
     * CDXC:SidebarTopChrome 2026-06-29-01:43:
     * The visible Settings menu moved from the titlebar into the sidebar shortcut row. Keep this titlebar-panel compatibility route on the same app-modal host so any existing native child-window path still opens Settings as the native modal surface.
     */
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarSettingsMenu");
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      modal: "settings",
      type: "open",
    });
  };

  const openTitlebarSettingsMenuCommands = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarCommandsMenu");
    openQuickAccess("commands");
  };

  const openTitlebarSettingsMenuHotkeys = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarHotkeysMenu");
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      modal: "hotkeys",
      type: "open",
    });
  };

  const wakePetFromTitlebarSettingsMenu = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarWakePetMenu");
    postNative({ type: "togglePetOverlayFromTitlebar" });
  };

  const openTitlebarSettingsMenuDiscord = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarDiscordMenu");
    postNative({ type: "openExternalUrl", url: GHOSTEX_DISCORD_URL });
  };
  const toggleResourceCollapse = (key: string) => {
    setCollapsedResourceKeys((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const setResourceItemsCollapsed = (
    targets: readonly ResourceItemCollapseTarget[],
    collapsed: boolean,
  ) => {
    setCollapsedResourceKeys((current) => applyResourceItemCollapsedState(current, targets, collapsed));
  };

  const focusResourceSession = (sessionId: string) => {
    /**
     * CDXC:TitlebarResources 2026-05-28-10:39:
     * Resources rows need a direct Focus action so users can jump from process
     * diagnostics to the owning session without using the sidebar. Close the
     * dropdown after forwarding the durable combined session id to the sidebar
     * owner, which already handles cross-project and sleeping-session focus.
     *
     * CDXC:TitlebarResources 2026-06-13-02:13:
     * Focus must visibly leave Resources after dispatching the sidebar focus
     * command. The native child window otherwise stays open over the newly
     * focused workspace, making a successful focus request look inert.
     */
    postNative({ sessionId, type: "focusResourceSessionFromTitlebar" });
    closeTitlebarDropdownPanel();
  };

  const quitResourceBundles = (bundles: ResourceProcessBundle[]) => {
    const uniqueBundles = uniqueResourceBundles(bundles).filter(isResourceBundleActionable);
    if (uniqueBundles.length === 0) {
      return;
    }
    /**
     * CDXC:TitlebarResources 2026-05-21-16:38:
     * Any Quit action in the resource manager should immediately mark the row
     * as closing and move it below active resources. Sidebar-owned terminal
     * sessions sleep through sidebar state so their cards remain resumable;
     * non-terminal panes and detached process bundles still use their resource
     * cleanup paths.
     *
     * CDXC:TitlebarResources 2026-05-23-10:46:
     * The resource manager must not rely on sidebar sleep as the only kill
     * mechanism. It also terminates the PIDs currently shown in the dropdown so
     * row Quit, group Quit, and Sleep All actually release RAM while the
     * sidebar keeps durable terminal sessions.
     *
     * CDXC:TitlebarResources 2026-06-22-00:30:
     * Server Stop rows should interrupt only listener-backed server process trees.
     * They intentionally skip sidebar session/project close commands so the
     * terminal that launched the server remains available after the port stops.
     */
    setQuittingResourceKeys((current) => {
      const next = new Set(current);
      uniqueBundles.forEach((bundle) => next.add(bundle.key));
      return next;
    });
    const sessionIds = uniqueBundles.flatMap(resourceBundleSidebarSessionIds);
    const projectIds = uniqueBundles.flatMap(resourceBundleProjectEditorIds);
    if (sessionIds.length > 0 || projectIds.length > 0) {
      postNative({
        projectIds: Array.from(new Set(projectIds)),
        sessionIds: Array.from(new Set(sessionIds)),
        type: "quitResourcesFromTitlebar",
      });
    }
    const processByPid = new Map(resourceProcesses.map((process) => [process.pid, process]));
    const processes = Array.from(
      new Map(
        uniqueBundles
          .flatMap((bundle) => bundle.pids)
          .map((pid) => processByPid.get(pid))
          .filter((process): process is ResourceProcess => process !== undefined)
          .map((process) => [process.pid, process]),
      ).values(),
    );
    const resourceRefreshGeneration = resourceRefreshGenerationRef.current;
    if (processes.length > 0) {
      const gracefulSignal = uniqueBundles.every((bundle) => bundle.type === "server") ? "INT" : "TERM";
      void terminateResourceProcesses(processes, { gracefulSignal }).finally(() => {
        window.setTimeout(() => {
          void refreshResources(resourceRefreshGeneration);
        }, 1_800);
      });
      return;
    }
    window.setTimeout(() => {
      void refreshResources(resourceRefreshGeneration);
    }, 250);
  };

  const sleepInactiveTerminalSessions = () => {
    if (inactiveTerminalSleepSessionIds.length === 0) {
      return;
    }
    postNative({
      sessionIds: inactiveTerminalSleepSessionIds,
      type: "sleepInactiveSessionsFromTitlebar",
    });
  };

  const startGxserverDaemon = () => {
    postNative({ type: "startGxserverFromTitlebar" });
  };

  const stopGxserverDaemon = () => {
    postNative({ type: "stopGxserverFromTitlebar" });
  };

  const restartGxserverDaemon = () => {
    postNative({ type: "restartGxserverFromTitlebar" });
  };

  const setGxserverAlwaysStart = (enabled: boolean) => {
    postNative({ enabled, type: "setGxserverAlwaysStartFromTitlebar" });
  };

  const stopKeepAwake = useCallback(async (options: { suppressAutoStart?: boolean } = {}) => {
    const runtime = keepAwakeRuntime;
    setKeepAwakeRuntime(undefined);
    localStorage.removeItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY);
    if (options.suppressAutoStart !== false) {
      setKeepAwakeAutoStartSuppressed(true);
    }
    const syncState = {
      runtime: null,
      suppressAutoStart: options.suppressAutoStart !== false,
    };
    publishKeepAwakeRuntimeSync(syncState);
    syncKeepAwakeRuntimeToMainTitlebar(syncState);
    if (!runtime) {
      return;
    }
    try {
      await runNativeProcess("/bin/kill", [String(runtime.pid)]);
    } catch (error) {
      console.warn("Failed to stop keep-awake process", error);
    }
  }, [keepAwakeRuntime]);

  const startKeepAwake = useCallback(
    async (
      durationMinutes: KeepAwakeDurationMinutes = projectState.keepAwake.defaultDurationMinutes,
      options: { source?: KeepAwakeRuntimeState["source"] } = {},
    ) => {
      if (!keepAwakeFeatureEnabled) {
        setKeepAwakeAutoStartSuppressed(true);
        return;
      }
      if (keepAwakeRuntime) {
        await stopKeepAwake({ suppressAutoStart: false });
      }
      /**
       * CDXC:TitlebarKeepAwake 2026-05-28-19:28:
       * The normal keep-awake button should prevent idle sleep and AC system sleep.
       * Lid-close sleep is controlled by the separate Settings toggle because macOS does not treat it as a regular caffeinate idle-sleep assertion.
       */
      setKeepAwakeAutoStartSuppressed(false);
      const flags = projectState.keepAwake.allowDisplaySleep ? "-is" : "-dis";
      const timeout = durationMinutes > 0 ? ` -t ${durationMinutes * 60}` : "";
      const result = await runNativeProcess("/bin/sh", [
        "-lc",
        `(/usr/bin/nohup /usr/bin/caffeinate ${flags}${timeout} >/dev/null 2>&1 & echo $!)`,
      ]);
      const pid = Number(result.stdout.trim().split(/\s+/u)[0]);
      if (result.exitCode !== 0 || !Number.isFinite(pid) || pid <= 0) {
        console.warn("Failed to start keep-awake process", result.stderr || result.stdout);
        return;
      }
      const nextRuntime: KeepAwakeRuntimeState = {
        durationMinutes,
        fireAtMs: durationMinutes > 0 ? Date.now() + durationMinutes * 60_000 : undefined,
        pid,
        source: options.source ?? "manual",
        startedAtMs: Date.now(),
      };
      setKeepAwakeRuntime(nextRuntime);
      localStorage.setItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY, JSON.stringify(nextRuntime));
      const syncState = { runtime: nextRuntime, suppressAutoStart: false };
      publishKeepAwakeRuntimeSync(syncState);
      syncKeepAwakeRuntimeToMainTitlebar(syncState);
    },
    [
      keepAwakeFeatureEnabled,
      keepAwakeRuntime,
      projectState.keepAwake.allowDisplaySleep,
      projectState.keepAwake.defaultDurationMinutes,
      stopKeepAwake,
    ],
  );

  useEffect(() => {
    if (isDropdownPanel || !window.__ghostex_TITLEBAR__) {
      return undefined;
    }
    const runKeepAwakeCommand = (command: TitlebarKeepAwakeCommand) => {
      /*
       * CDXC:SidebarTopChrome 2026-06-29-01:43:
       * Keep Awake moved from the titlebar trigger strip into the sidebar shortcut row. Keep this bridge as the only sidebar entry point so the titlebar host remains the single owner of caffeinate start/stop and runtime sync.
       */
      if (command.action === "stop") {
        void stopKeepAwake();
        return;
      }
      void startKeepAwake(command.durationMinutes);
    };
    window.__ghostex_TITLEBAR__.runKeepAwakeCommand = runKeepAwakeCommand;
    return () => {
      if (window.__ghostex_TITLEBAR__?.runKeepAwakeCommand === runKeepAwakeCommand) {
        delete window.__ghostex_TITLEBAR__.runKeepAwakeCommand;
      }
    };
  }, [isDropdownPanel, startKeepAwake, stopKeepAwake]);

  const openPowerSettings = () => {
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSection: "power",
      modal: "settings",
      type: "open",
    });
  };

  const openSessionPersistenceSettings = () => {
    /**
     * CDXC:SessionPersistence 2026-06-04-02:52:
     * The persistence-off Tips notice is an actionable warning. Clicking it
     * should open the Settings page that owns Session Persistence and pre-fill
     * search with the exact setting label so users land on it immediately.
     *
     * CDXC:SettingsNavigation 2026-08-19-00:00:
     * The terminal settings moved into the General page, so this must request
     * the real `settings` tab; the retired `ghostty` id resolves to no page.
     */
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Session Persistence",
      initialTab: "settings",
      modal: "settings",
      type: "open",
    });
  };

  const openAgentHooksSettings = () => {
    /**
     * CDXC:AgentHooks 2026-06-23-05:09:
     * The missing-hook Tips warning should deep-link to Settings > Integrations
     * and search for Agent Hooks instead of installing directly from titlebar
     * chrome, so users land on the provider-specific status and install control.
     */
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Agent Hooks",
      initialTab: "integrations",
      modal: "settings",
      type: "open",
    });
  };

  const openDebuggingModeSettings = () => {
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Debug logging and UI",
      initialTab: "settings",
      modal: "settings",
      type: "open",
    });
  };

  const openGhostexCliSettings = () => {
    /**
     * CDXC:CliInstall 2026-06-07-15:26:
     * The CLI-not-accessible Tips notice should deep-link to Settings where
     * Repair CLI lives, so the notice is actionable without adding titlebar
     * install controls.
     */
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
      initialSearchQuery: "Ghostex CLI",
      initialTab: "integrations",
      modal: "settings",
      type: "open",
    });
  };

  const handleNoticeAction = (notice: TitlebarNotice) => {
    const target = notice.settingsTarget;
    if (target === "agentHooks") {
      openAgentHooksSettings();
      return;
    }
    if (target === "debuggingMode") {
      openDebuggingModeSettings();
      return;
    }
    if (target === "ghostexCli") {
      openGhostexCliSettings();
      return;
    }
    openSessionPersistenceSettings();
  };

  useEffect(() => {
    const handleStorage = (event: StorageEvent) => {
      if (
        event.key !== KEEP_AWAKE_RUNTIME_STORAGE_KEY &&
        event.key !== KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY
      ) {
        return;
      }
      if (event.key === KEEP_AWAKE_RUNTIME_STORAGE_KEY && event.newValue === null) {
        return;
      }
      syncKeepAwakeRuntimeState(
        event.key === KEEP_AWAKE_RUNTIME_SYNC_STORAGE_KEY
          ? readKeepAwakeRuntimeSyncState(event.newValue)
          : undefined,
      );
    };
    const handleLocalSync = (event: Event) => {
      syncKeepAwakeRuntimeState(
        event instanceof CustomEvent ? event.detail as KeepAwakeRuntimeSyncState : undefined,
      );
    };
    /*
     * CDXC:TitlebarKeepAwake 2026-06-15-10:12:
     * The keep-awake dropdown renders in a native child titlebar window. Runtime changes from that child must update the main titlebar immediately and explicit Don't keep awake must suppress launch/display auto-start for this app run until the user starts keep-awake again.
     */
    window.addEventListener("storage", handleStorage);
    window.addEventListener(KEEP_AWAKE_RUNTIME_CHANGED_EVENT, handleLocalSync);
    return () => {
      window.removeEventListener("storage", handleStorage);
      window.removeEventListener(KEEP_AWAKE_RUNTIME_CHANGED_EVENT, handleLocalSync);
    };
  }, [syncKeepAwakeRuntimeState]);

  useEffect(() => {
    /*
     * CDXC:ExperimentalFeatures 2026-06-28-07:41:
     * Keep Awake is gated by Enable Experimental Features. If the user turns it
     * off while caffeinate is running, stop the hidden runtime instead of
     * leaving a titlebar-invisible power assertion active.
     */
    if (keepAwakeFeatureEnabled || !keepAwakeRuntime) {
      return;
    }
    void stopKeepAwake({ suppressAutoStart: true });
  }, [keepAwakeFeatureEnabled, keepAwakeRuntime, stopKeepAwake]);

  useEffect(() => {
    if (
      !keepAwakeFeatureEnabled ||
      !projectState.keepAwake.activateOnLaunch ||
      keepAwakeRuntime ||
      keepAwakeAutoStartSuppressed
    ) {
      return;
    }
    void startKeepAwake();
  }, [
    keepAwakeFeatureEnabled,
    keepAwakeAutoStartSuppressed,
    keepAwakeRuntime,
    projectState.keepAwake.activateOnLaunch,
    startKeepAwake,
  ]);

  useEffect(() => {
    /*
     * CDXC:TitlebarKeepAwake 2026-06-23-08:20:
     * Working-session keep-awake is optional, but once enabled it should cover the active Working period plus 20 minutes afterward so users have time to reply before the Mac can sleep.
     */
    const previousWorkingSessionCount = previousKeepAwakeWorkingSessionCountRef.current;
    previousKeepAwakeWorkingSessionCountRef.current = projectState.keepAwake.workingSessionCount;
    if (!projectState.keepAwake.whileWorkingSessions) {
      setKeepAwakeWorkingSessionGraceUntilMs(undefined);
      return;
    }
    if (
      projectState.keepAwake.workingSessionCount === 0 &&
      previousWorkingSessionCount > 0
    ) {
      setKeepAwakeWorkingSessionGraceUntilMs(Date.now() + KEEP_AWAKE_WORKING_SESSION_GRACE_MS);
    }
  }, [
    projectState.keepAwake.whileWorkingSessions,
    projectState.keepAwake.workingSessionCount,
  ]);

  useEffect(() => {
    if (
      !projectState.keepAwake.whileWorkingSessions ||
      projectState.keepAwake.workingSessionCount > 0 ||
      keepAwakeWorkingSessionGraceUntilMs === undefined
    ) {
      return;
    }
    const remainingMs = keepAwakeWorkingSessionGraceUntilMs - Date.now();
    if (remainingMs <= 0) {
      setKeepAwakeWorkingSessionGraceUntilMs(undefined);
      return;
    }
    const timeout = window.setTimeout(() => {
      setKeepAwakeWorkingSessionGraceUntilMs(undefined);
    }, remainingMs);
    return () => window.clearTimeout(timeout);
  }, [
    keepAwakeWorkingSessionGraceUntilMs,
    projectState.keepAwake.whileWorkingSessions,
    projectState.keepAwake.workingSessionCount,
  ]);

  useEffect(() => {
    /*
     * CDXC:TitlebarKeepAwake 2026-06-23-08:20:
     * If no manual keep-awake period is running, active Delayed Send timers should still prevent laptop sleep so the scheduled Enter can fire. Manual Keep Awake, especially Until turned off, takes precedence because automatic holds only start when no runtime exists and only stop runtimes they started.
     */
    if (!keepAwakeFeatureEnabled) {
      return;
    }
    const delayedSendHoldActive = projectState.keepAwake.delayedSendSessionCount > 0;
    const workingSessionHoldActive =
      projectState.keepAwake.whileWorkingSessions &&
      (projectState.keepAwake.workingSessionCount > 0 ||
        (keepAwakeWorkingSessionGraceUntilMs !== undefined &&
          keepAwakeWorkingSessionGraceUntilMs > Date.now()));
    const shouldRunAutomaticKeepAwake =
      !keepAwakeAutoStartSuppressed && (delayedSendHoldActive || workingSessionHoldActive);
    if (!shouldRunAutomaticKeepAwake) {
      if (keepAwakeRuntime?.source === "automatic") {
        void stopKeepAwake({ suppressAutoStart: false });
      }
      return;
    }
    if (!keepAwakeRuntime) {
      void startKeepAwake(0, { source: "automatic" });
    }
  }, [
    keepAwakeAutoStartSuppressed,
    keepAwakeFeatureEnabled,
    keepAwakeRuntime,
    keepAwakeWorkingSessionGraceUntilMs,
    projectState.keepAwake.delayedSendSessionCount,
    projectState.keepAwake.whileWorkingSessions,
    projectState.keepAwake.workingSessionCount,
    startKeepAwake,
    stopKeepAwake,
  ]);

  useEffect(() => {
    const desired = Boolean(
      keepAwakeFeatureEnabled && keepAwakeRuntime && projectState.keepAwake.preventLidSleep,
    );
    const ghostexEnabledLidSleepPrevention =
      localStorage.getItem(KEEP_AWAKE_LID_SLEEP_STORAGE_KEY) === "enabled";
    if (!desired && !ghostexEnabledLidSleepPrevention) {
      return;
    }
    let cancelled = false;
    const needsPolicyChange = desired !== ghostexEnabledLidSleepPrevention;
    const applyPolicy = async () => {
      const applied = await applyKeepAwakeLidSleepPrevention(desired, {
        installIfNeeded: desired && needsPolicyChange,
      });
      if (!applied || cancelled) {
        return;
      }
      localStorage.setItem(KEEP_AWAKE_LID_SLEEP_STORAGE_KEY, desired ? "enabled" : "disabled");
    };
    if (needsPolicyChange) {
      void applyPolicy();
    }
    let interval: number | undefined;
    if (desired) {
      interval = window.setInterval(() => {
        void applyKeepAwakeLidSleepPrevention(true, { installIfNeeded: false }).then((applied) => {
          if (applied && !cancelled) {
            localStorage.setItem(KEEP_AWAKE_LID_SLEEP_STORAGE_KEY, "enabled");
          }
        });
      }, 10_000);
    }
    return () => {
      cancelled = true;
      if (interval !== undefined) {
        window.clearInterval(interval);
      }
    };
  }, [keepAwakeFeatureEnabled, keepAwakeRuntime, projectState.keepAwake.preventLidSleep]);

  useEffect(() => {
    if (!keepAwakeRuntime) {
      return;
    }
    const checkRuntime = async () => {
      if (keepAwakeRuntime.fireAtMs !== undefined && Date.now() >= keepAwakeRuntime.fireAtMs) {
        await stopKeepAwake();
        return;
      }
      const pidCheck = await runNativeProcess("/bin/kill", ["-0", String(keepAwakeRuntime.pid)]);
      if (pidCheck.exitCode !== 0) {
        setKeepAwakeRuntime(undefined);
        localStorage.removeItem(KEEP_AWAKE_RUNTIME_STORAGE_KEY);
        publishKeepAwakeRuntimeSync({ suppressAutoStart: false });
      }
    };
    void checkRuntime();
    const interval = window.setInterval(() => {
      void checkRuntime();
    }, KEEP_AWAKE_POWER_CHECK_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [keepAwakeRuntime, stopKeepAwake]);

  useEffect(() => {
    const shouldCheckExternalDisplay =
      keepAwakeFeatureEnabled &&
      !keepAwakeRuntime &&
      !keepAwakeAutoStartSuppressed &&
      projectState.keepAwake.activateOnExternalDisplay;
    const shouldCheckBattery =
      Boolean(keepAwakeRuntime && projectState.keepAwake.deactivateBelowBatteryThreshold);
    const shouldCheckLowPowerMode =
      Boolean(keepAwakeRuntime && projectState.keepAwake.deactivateOnLowPowerMode);
    if (!shouldCheckExternalDisplay && !shouldCheckBattery && !shouldCheckLowPowerMode) {
      return;
    }
    const checkPowerRules = async () => {
      const snapshot = await readKeepAwakePowerSnapshot({
        includeBattery: shouldCheckBattery,
        includeExternalDisplay: shouldCheckExternalDisplay,
        includeLowPowerMode: shouldCheckLowPowerMode,
      });
      if (!snapshot) {
        return;
      }
      if (
        keepAwakeRuntime &&
        projectState.keepAwake.deactivateBelowBatteryThreshold &&
        snapshot.batteryPercent !== undefined &&
        snapshot.batteryPercent <= projectState.keepAwake.batteryThresholdPercent
      ) {
        await stopKeepAwake();
        return;
      }
      if (
        keepAwakeRuntime &&
        projectState.keepAwake.deactivateOnLowPowerMode &&
        snapshot.lowPowerMode === true
      ) {
        await stopKeepAwake();
        return;
      }
      if (
        !keepAwakeRuntime &&
        !keepAwakeAutoStartSuppressed &&
        projectState.keepAwake.activateOnExternalDisplay &&
        snapshot.externalDisplayConnected
      ) {
        await startKeepAwake();
      }
    };
    void checkPowerRules();
    const interval = window.setInterval(() => {
      void checkPowerRules();
    }, KEEP_AWAKE_POWER_CHECK_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [
    keepAwakeAutoStartSuppressed,
    keepAwakeFeatureEnabled,
    keepAwakeRuntime,
    projectState.keepAwake.activateOnExternalDisplay,
    projectState.keepAwake.batteryThresholdPercent,
    projectState.keepAwake.deactivateBelowBatteryThreshold,
    projectState.keepAwake.deactivateOnLowPowerMode,
    startKeepAwake,
    stopKeepAwake,
  ]);

  const openAgentsMode = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarAgentsMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "agents" }),
    );
    setOptimisticMode("agents");
    postNative({ type: "openAgentsModeFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "agents",
    });
  };

  const codeModeDisabledReason =
    projectState.projectId && parseRemoteProjectId(projectState.projectId)
      ? "Code is currently disabled for remote projects"
      : undefined;

  const openCodeMode = () => {
    if (codeModeDisabledReason) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarSourceMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "code" }),
    );
    setOptimisticMode("code");
    postNative({ type: "openActiveProjectEditorFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "code",
    });
  };

  /**
   * CDXC:ProjectBrowserTabs 2026-06-13-00:12:
   * The top project browser mode is now user-facing Browser mode. Keep it
   * disabled only for Quick/projectless contexts; real projects without a
   * GitHub remote still open Browser mode with Google as the first tab so the
   * control is always useful without showing an app-created about:blank page.
   *
   * CDXC:ProjectBrowserTabs 2026-06-16-12:02:
   * Browser + tabs follow the same destination rule: project GitHub remote when available, otherwise Google.
   */
  const browserModeDisabledReason = projectState.projectIsQuick
    ? "Switch to a project to access this view"
    : undefined;
  /*
   * CDXC:ModeSwitcher 2026-06-08-18:39:
   * Quick sessions are projectless work areas, so Kanban should be unavailable
   * there for the same active-context reason as Browser mode. Disable the
   * titlebar tab/button before click dispatch instead of opening an empty
   * project-board surface.
   *
   * CDXC:ModeSwitcher 2026-06-16-16:00:
   * Disabled Browser and Kanban mode tabs should explain the project-context
   * requirement directly on hover. Use one shared message for Quick sessions so
   * users know switching to a project unlocks those views.
   */
  const kanbanModeDisabledReason = projectState.projectIsQuick
    ? "Switch to a project to access this view"
    : undefined;
  const manageModeDisabledReason = projectState.projectIsQuick
    ? "Switch to a project to access this view"
    : undefined;

  const openGitMode = () => {
    if (browserModeDisabledReason) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarBrowserMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "git" }),
    );
    setOptimisticMode("git");
    postNative({ type: "openGitHubProjectFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "git",
    });
  };

  const openAutomateMode = () => {
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarAutomateMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "automate" }),
    );
    setOptimisticMode("automate");
    postNative({ type: "openAutomateFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "automate",
    });
  };

  const openTasksMode = () => {
    if (kanbanModeDisabledReason) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarKanbanMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "tasks" }),
    );
    setOptimisticMode("tasks");
    postNative({ type: "openTasksPlaceholderFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "tasks",
    });
  };

  const openManageMode = () => {
    if (manageModeDisabledReason) {
      return;
    }
    closeAppModalFromTitlebarNavigation("SettingsDismissal:titlebarManageMode");
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickStart",
      titlebarModeSwitchLogDetails({ optimisticMode, projectState, targetMode: "manage" }),
    );
    setOptimisticMode("manage");
    postNative({ type: "openManageFromTitlebar" });
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.titlebarClickPostedNative",
      {
      projectId: projectState.projectId ?? "none",
      targetMode: "manage",
    });
  };

  const toggleProjectEditorCompanion = () => {
    appendTitlebarModeSwitchDebugLog(
      projectState.diagnosticLogging,
      "titlebarModeSwitch.companionToggle.dispatch",
      {
      activeMode,
      editorIsOpen: projectState.editorIsOpen,
      nextProjectEditorCompanionPaneHidden: projectState.projectEditorCompanionPaneHidden !== true,
      projectEditorCompanionPaneHidden: projectState.projectEditorCompanionPaneHidden,
      projectId: projectState.projectId,
      source: "click",
    });
    postNative({ type: "toggleProjectEditorCompanionFromTitlebar" });
  };
  const showUpdateDialog = () => {
    if (projectState.updateDownloading) {
      return;
    }
    postNative({ type: "showUpdateDialogFromTitlebar" });
  };

  const shouldShowCompanionToggleButton =
    activeMode !== "agents" &&
    projectState.editorIsOpen &&
    !projectState.editorIsSleeping;
  /*
   * CDXC:TitlebarModeTabs 2026-05-31-12:00:
   * macOS titlebar mode switcher labels use title case (Agents, Source, Browser, Kanban, Automate, Docs), not all-caps, so the segmented control reads like navigation chrome rather than shouting labels.
   *
   * CDXC:Manage 2026-06-20-04:36:
   * Manage is a project-scoped file browser workarea and should sit beside Kanban in the same titlebar segmented control instead of being hidden under a menu.
   *
   * CDXC:TitlebarManage 2026-06-28-06:16:
   * Manage is no longer beta or debugging-only chrome. Always show it in the
   * titlebar mode list and keep only the project-context disabled reason for
   * Quick sessions.
   *
   * CDXC:TitlebarDocs 2026-06-28-06:24:
   * The user-facing titlebar name for the Manage-backed project document
   * surface is Docs. Keep the stable internal "manage" mode id so persisted
   * pane state and native bridge messages remain compatible.
   *
   * CDXC:Automations 2026-06-30-11:05:
   * Automations are a first-class titlebar workarea named Automate. Opening Automate uses its own project-editor mode so project automations no longer make the titlebar look like it switched to Kanban.
   *
   * CDXC:TitlebarModeTabs 2026-06-30-12:55:
   * Kanban must appear before Automate in the macOS titlebar mode switcher, preserving the project-management flow before scheduled automation while keeping Docs last.
   */
  const configuredTitlebarModes = [
    {
      label: "Agents",
      onSelect: openAgentsMode,
      value: "agents" as const,
    },
    {
      disabled: codeModeDisabledReason !== undefined,
      disabledReason: codeModeDisabledReason,
      label: "Source",
      onSelect: openCodeMode,
      value: "code" as const,
    },
    {
      disabled: browserModeDisabledReason !== undefined,
      disabledReason: browserModeDisabledReason,
      label: "Browser",
      onSelect: openGitMode,
      value: "git" as const,
    },
    {
      disabled: kanbanModeDisabledReason !== undefined,
      disabledReason: kanbanModeDisabledReason,
      label: "Kanban",
      onSelect: openTasksMode,
      value: "tasks" as const,
    },
    {
      label: "Automate",
      onSelect: openAutomateMode,
      value: "automate" as const,
    },
    {
      disabled: manageModeDisabledReason !== undefined,
      disabledReason: manageModeDisabledReason,
      label: "Docs",
      onSelect: openManageMode,
      value: "manage" as const,
    },
  ];
  const visibleTitlebarModes = configuredTitlebarModes.filter((mode) => {
    switch (mode.value) {
      case "code":
        return !projectState.codeViewTabHidden;
      case "git":
        return !projectState.browserViewTabHidden;
      case "tasks":
        return !projectState.kanbanViewTabHidden;
      case "automate":
        return !projectState.automateViewTabHidden;
      case "manage":
        return !projectState.docsViewTabHidden;
      case "agents":
        return true;
    }
  });
  const titlebarModes =
    visibleTitlebarModes.length === 1 && visibleTitlebarModes[0]?.value === "agents"
      ? []
      : visibleTitlebarModes;
  const resolveTitlebarDropdownPanelSize = useCallback(
    (kind: TitlebarDropdownPanelKind) => createTitlebarDropdownPanelPreferredSize(kind),
    [],
  );

  useLayoutEffect(() => {
    dropdownPanelSizeResolverRef.current = resolveTitlebarDropdownPanelSize;
  }, [resolveTitlebarDropdownPanelSize]);

  useEffect(() => {
    /**
     * CDXC:SidebarTheme 2026-06-15-01:43:
     * The titlebar runs in its own WKWebView, so mirror the resolved sidebar
     * theme onto body for shared CSS tokens. This keeps the titlebar strip and
     * native child-window dropdown panels aligned with Dark 1, Dark 2, and
     * Light Settings changes.
     */
    document.body.dataset.sidebarTheme = projectState.sidebarTheme;
    return () => {
      delete document.body.dataset.sidebarTheme;
    };
  }, [projectState.sidebarTheme]);

  useEffect(() => {
    if (initialTitlebarDropdownPanelKind) {
      return;
    }

    if (projectState.customSidebarTitlebarColorsEnabled) {
      const titlebarGradientColors = getSidebarTitlebarGradientColors(
        projectState.customSidebarTitlebarBackgroundColor,
      );
      const titlebarBackground = `linear-gradient(90deg, ${titlebarGradientColors.titlebarLeft} 0%, ${titlebarGradientColors.titlebarLeft} ${TITLEBAR_GRADIENT_BLEND_START_PERCENT}%, ${titlebarGradientColors.titlebarRight} 100%)`;
      /**
       * CDXC:SidebarTitlebarColors 2026-06-15-11:24:
       * The React titlebar is a separate WKWebView from the sidebar. Apply the
       * experimental custom chrome colors only in this titlebar host; dropdown
       * panels reuse this bundle but must continue using normal dropdown/theme
       * tokens instead of the sidebar/titlebar override.
       *
       * CDXC:SidebarTitlebarColors 2026-06-15-13:22:
       * Foreground is derived from the background before it reaches this state;
       * the titlebar host should not expose or preserve a separate foreground
       * choice.
       *
       * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
       * Custom titlebar separators darken as the slider-selected background gets
       * lighter, but only inside the real titlebar host.
       *
       * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
       * The titlebar should start with the sidebar gradient's top color and
       * use a separate surface token for the gradient paint.
       *
       * CDXC:SidebarTitlebarColors 2026-06-19-13:26:
       * Keep the titlebar's left 40% on the sidebar top stop so it blends with
       * the sidebar edge, then fade to the sidebar bottom stop at the right.
       * The titlebar gradient should now darken across the strip rather than
       * brighten at the right edge.
       */
      document.body.dataset.customSidebarTitlebarColors = "true";
      document.body.style.setProperty(
        "--app-titlebar-background",
        titlebarGradientColors.titlebarLeft,
      );
      document.body.style.setProperty(
        "--app-titlebar-surface-background",
        titlebarBackground,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-background-color",
        titlebarGradientColors.titlebarLeft,
      );
      document.body.style.setProperty(
        "--custom-sidebar-titlebar-foreground-color",
        projectState.customSidebarTitlebarForegroundColor,
      );
      document.body.style.setProperty(
        "--app-foreground",
        projectState.customSidebarTitlebarForegroundColor,
      );
      document.body.style.setProperty(
        "--titlebar-button-border-color",
        getTitlebarButtonSeparatorColorForBackground(titlebarGradientColors.titlebarLeft),
      );
    } else {
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--app-titlebar-background");
      document.body.style.removeProperty("--app-titlebar-surface-background");
      document.body.style.removeProperty("--app-foreground");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--titlebar-button-border-color");
    }

    return () => {
      delete document.body.dataset.customSidebarTitlebarColors;
      document.body.style.removeProperty("--app-titlebar-background");
      document.body.style.removeProperty("--app-titlebar-surface-background");
      document.body.style.removeProperty("--app-foreground");
      document.body.style.removeProperty("--custom-sidebar-titlebar-background-color");
      document.body.style.removeProperty("--custom-sidebar-titlebar-foreground-color");
      document.body.style.removeProperty("--titlebar-button-border-color");
    };
  }, [
    projectState.customSidebarTitlebarBackgroundColor,
    projectState.customSidebarTitlebarColorsEnabled,
    projectState.customSidebarTitlebarForegroundColor,
  ]);

  const isTitlebarDarkTheme = getTitlebarThemeVariant(projectState.sidebarTheme) === "dark";

  if (titlebarPanelKind) {
    return (
      <TooltipProvider delayDuration={300}>
        <TitlebarDropdownPanelSurface
          browserBundles={resourceViews.browserBundles}
          codeIdeBundles={resourceViews.codeIdeBundles}
          collapsedResourceKeys={collapsedResourceKeys}
          daemon={projectState.gxserverDaemon}
          inactiveTerminalSleepSessionCount={inactiveTerminalSleepSessionIds.length}
          kind={titlebarPanelKind}
          notices={notices}
          onClose={closeTitlebarDropdownPanel}
          onFocusResourceSession={focusResourceSession}
          onGxserverAlwaysStartChange={setGxserverAlwaysStart}
          onGxserverRestart={restartGxserverDaemon}
          onGxserverStart={startGxserverDaemon}
          onGxserverStop={stopGxserverDaemon}
          onMarkTipRead={markTipRead}
          onOpenChangelog={openChangelogFromTips}
          onOpenDocs={openDocsFromTips}
          onOpenHighlightedFeatures={openHighlightedFeaturesFromTips}
          onOpenNoticeSettings={handleNoticeAction}
          onOpenTipAction={openTipAction}
          onQuitResources={quitResourceBundles}
          onViewGhostexGuide={viewGhostexGuideFromTips}
          onSetResourceItemsCollapsed={setResourceItemsCollapsed}
          onSleepInactiveSessions={sleepInactiveTerminalSessions}
          onToggleResourceCollapse={toggleResourceCollapse}
          orphanBundles={resourceViews.orphanBundles}
          resourceProcessSnapshotReady={resourceProcessSnapshotReady}
          resourceProcessTotals={resourceProcessTotals}
          quittingResourceKeys={quittingResourceKeys}
          readTips={readTips}
          resourceGroupViews={resourceViews.groupViews}
          serverBundles={resourceServerBundles}
          sidebarTheme={projectState.sidebarTheme}
          linkOpenTarget={projectState.webLinkOpenTarget}
          sessionPersistenceProvider={
            projectState.sessionPersistenceProvider === "off"
              ? undefined
              : projectState.sessionPersistenceProvider
          }
          unreadTips={unreadTips}
        />
      </TooltipProvider>
    );
  }

  return null;
}

export function TitlebarDropdownPanelSurface({
  browserBundles,
  codeIdeBundles,
  collapsedResourceKeys,
  daemon,
  inactiveTerminalSleepSessionCount,
  kind,
  notices,
  onClose,
  onFocusResourceSession,
  onGxserverAlwaysStartChange,
  onGxserverRestart,
  onGxserverStart,
  onGxserverStop,
  onMarkTipRead,
  onOpenChangelog,
  onOpenDocs,
  onOpenHighlightedFeatures,
  onOpenNoticeSettings,
  onOpenTipAction,
  onQuitResources,
  onSetResourceItemsCollapsed,
  onSleepInactiveSessions,
  onViewGhostexGuide,
  onToggleResourceCollapse,
  orphanBundles,
  resourceProcessSnapshotReady,
  resourceProcessTotals,
  quittingResourceKeys,
  readTips,
  resourceGroupViews,
  serverBundles,
  sidebarTheme,
  linkOpenTarget,
  sessionPersistenceProvider,
  unreadTips,
}: {
  browserBundles: ResourceProcessBundle[];
  codeIdeBundles: ResourceProcessBundle[];
  collapsedResourceKeys: Set<string>;
  daemon: TitlebarGxserverDaemonStatus;
  inactiveTerminalSleepSessionCount: number;
  kind: TitlebarDropdownPanelKind;
  notices: TitlebarNotice[];
  onClose: () => void;
  onFocusResourceSession: (sessionId: string) => void;
  onGxserverAlwaysStartChange: (enabled: boolean) => void;
  onGxserverRestart: () => void;
  onGxserverStart: () => void;
  onGxserverStop: () => void;
  onMarkTipRead: (tipId: string) => void;
  onOpenChangelog: () => void;
  onOpenDocs: () => void;
  onOpenHighlightedFeatures: () => void;
  onOpenNoticeSettings: (notice: TitlebarNotice) => void;
  onOpenTipAction: (tip: TitlebarTip) => void;
  onQuitResources: (bundles: ResourceProcessBundle[]) => void;
  onSetResourceItemsCollapsed: (
    targets: readonly ResourceItemCollapseTarget[],
    collapsed: boolean,
  ) => void;
  onSleepInactiveSessions: () => void;
  onViewGhostexGuide: () => void;
  onToggleResourceCollapse: (key: string) => void;
  orphanBundles: ResourceProcessBundle[];
  resourceProcessSnapshotReady: boolean;
  resourceProcessTotals: ResourceProcessTotals;
  quittingResourceKeys: Set<string>;
  readTips: TitlebarTip[];
  resourceGroupViews: ResourceGroupView[];
  serverBundles: ResourceProcessBundle[];
  sidebarTheme: SidebarTheme;
  linkOpenTarget: WebLinkOpenTarget;
  sessionPersistenceProvider: Exclude<SessionPersistenceProvider, "off"> | undefined;
  unreadTips: TitlebarTip[];
}) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const closeAfter = (action: () => void | Promise<void>) => {
    void Promise.resolve()
      .then(action)
      .catch((error) => {
        console.warn("Titlebar dropdown action failed", error);
      })
      .finally(onClose);
  };
  const isPanelDarkTheme = getTitlebarThemeVariant(sidebarTheme) === "dark";

  return (
    <div
      className={cn(isPanelDarkTheme && "dark", "titlebar-dropdown-panel-root")}
      data-panel-kind={kind}
      data-sidebar-theme={sidebarTheme}
    >
      {kind === "tips" ? (
        <div className="titlebar-open-menu titlebar-tips-menu rounded-none border-border/80 p-0 text-[13px] text-foreground shadow-2xl">
          <TitlebarTipsMenu
            notices={notices}
            onMarkRead={onMarkTipRead}
            onOpenChangelog={() => closeAfter(onOpenChangelog)}
            onOpenDocs={() => closeAfter(onOpenDocs)}
            onOpenHighlightedFeatures={() => closeAfter(onOpenHighlightedFeatures)}
            onOpenNoticeSettings={(notice) => closeAfter(() => onOpenNoticeSettings(notice))}
            onOpenTipAction={(tip) => closeAfter(() => onOpenTipAction(tip))}
            onViewGhostexGuide={() => closeAfter(onViewGhostexGuide)}
            readTips={readTips}
            unreadTips={unreadTips}
          />
        </div>
      ) : null}
      {kind === "resources" ? (
        <div className="titlebar-open-menu titlebar-resources-menu rounded-none border-border/80 p-0 text-[13px] text-foreground shadow-2xl">
          <TitlebarResourcesMenu
            browserBundles={browserBundles}
            codeIdeBundles={codeIdeBundles}
            collapsedKeys={collapsedResourceKeys}
            daemon={daemon}
            groupViews={resourceGroupViews}
            inactiveTerminalSleepSessionCount={inactiveTerminalSleepSessionCount}
            onFocusSession={(sessionId) => {
              onFocusResourceSession(sessionId);
              onClose();
            }}
            onGxserverAlwaysStartChange={onGxserverAlwaysStartChange}
            onGxserverRestart={onGxserverRestart}
            onGxserverStart={onGxserverStart}
            onGxserverStop={onGxserverStop}
            onQuit={onQuitResources}
            onSetResourceItemsCollapsed={onSetResourceItemsCollapsed}
            processSnapshotReady={resourceProcessSnapshotReady}
            onSleepInactiveSessions={onSleepInactiveSessions}
            onToggle={onToggleResourceCollapse}
            orphanBundles={orphanBundles}
            processTotals={resourceProcessTotals}
            quittingKeys={quittingResourceKeys}
            serverBundles={serverBundles}
            linkOpenTarget={linkOpenTarget}
            sessionPersistenceProvider={sessionPersistenceProvider}
          />
        </div>
      ) : null}
    </div>
  );
}

export function getTitlebarThemeVariant(theme: SidebarTheme): "dark" | "light" {
  return theme.startsWith("light-") || theme === "plain-light" ? "light" : "dark";
}

export function parseTitlebarHexRgbColor(color: string): TitlebarRgbColor | undefined {
  const normalized = color.trim().toLowerCase();
  const match = /^#([0-9a-f]{6})$/u.exec(normalized);
  if (!match) {
    return undefined;
  }

  const hex = match[1];
  return {
    red: Number.parseInt(hex.slice(0, 2), 16),
    green: Number.parseInt(hex.slice(2, 4), 16),
    blue: Number.parseInt(hex.slice(4, 6), 16),
  };
}

export function getTitlebarButtonSeparatorColorForBackground(backgroundColor: string): string {
  /**
   * CDXC:SidebarTitlebarColors 2026-06-15-15:01:
   * When the experimental sidebar/titlebar background gets lighter, titlebar
   * button separators should get darker so the chrome reads as deliberate lines
   * instead of faint raised rows.
   *
   * CDXC:SidebarTitlebarColors 2026-06-15-16:03:
   * A 90 contrast background made the previous separator curve nearly match
   * the background. Darken separators much faster as the background lightens so
   * the button dividers stay visible throughout the 85-100 slider range.
   *
   * CDXC:SidebarTitlebarColors 2026-06-16-15:52:
   * The 93 contrast + white tint default computes to #141414. The previous
   * curve crossed over there and returned #151515, making 1px titlebar button
   * separators disappear. Keep very dark backgrounds on the subtle lighter
   * separator curve, then switch to the dark divider floor once the background
   * reaches the new default range.
   *
   * CDXC:SidebarTitlebarColors 2026-06-19-12:33:
   * The titlebar now paints a horizontal gradient but separators use the solid
   * left stop. Keep very dark left stops on a lighter divider floor so
   * separators between titlebar items stay visible instead of blending into the
   * chrome.
   */
  const color = parseTitlebarHexRgbColor(backgroundColor);
  if (!color) {
    return "#252525";
  }

  const averageChannel = Math.round((color.red + color.green + color.blue) / 3);
  const separatorChannel =
    averageChannel <= 22
      ? 37
      : averageChannel <= 34
        ? Math.max(18, Math.min(37, Math.round(37 - (averageChannel - 22) * 1.5)))
        : 6;
  const separatorHex = separatorChannel.toString(16).padStart(2, "0");
  return `#${separatorHex}${separatorHex}${separatorHex}`;
}
