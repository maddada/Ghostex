import {
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconBox,
  IconChevronDown,
  IconCode,
  IconCpu,
  IconDeviceDesktop,
  IconFocus2,
  IconInfoCircle,
  IconLoader2,
  IconMoon,
  IconRefresh,
  IconSquareMinus,
  IconTerminal2,
  IconWorld,
  IconX,
} from "@tabler/icons-react";
import { useState, type ReactNode } from "react";
import { AppTooltip } from "@/packages/core-ui/app-tooltip";
import { AGENT_LOGO_COLORS, AGENT_LOGOS } from "@/packages/core-ui/agent-logos";
import type { SidebarAgentIcon } from "@/packages/shared/sidebar-agents";
import type { NativePortlessAdminInstallAction } from "@/packages/shared/native-ghostty-host-protocol";
import type {
  SessionPersistenceProvider,
  WebLinkOpenTarget,
} from "@/packages/shared/ghostex-settings";
import { postNative, postTitlebarSidebarCommand } from "./native-bridge";
import {
  createResourceItemCollapseTarget,
  createResourceItemCollapseTargets,
  formatResourceMemory,
  formatWholePercent,
  getBrowserProcessDisplayName,
  getProcessDisplayName,
  isResourceBundleActionable,
  isResourceItemCollapsed,
  resourceBundleFocusSessionId,
  sortResourceBundlesForDisplay,
  sumBundleCpu,
  sumBundleMemory,
} from "./resource-processes";
import type {
  ResourceGroupView,
  ResourceItemCollapseTarget,
  ResourceListeningServer,
  ResourcePortlessServerPresentation,
  ResourceProcess,
  ResourceProcessBundle,
  ResourceProcessTotals,
  TitlebarGxserverDaemonStatus,
} from "./types";

/*
 * CDXC:TitlebarTooltips 2026-06-15-13:34:
 * Titlebar hover labels should close when the pointer leaves the trigger.
 * Hovering the floating label itself must not keep it open, so titlebar-owned
 * AppTooltip roots disable Base UI's hoverable popup behavior instead of adding
 * native hit-test routing or invisible hover surfaces.
 */
export const TITLEBAR_TOOLTIP_ROOT_PROPS = {
  disableHoverablePopup: true,
} as const;

export function TitlebarResourcesMenu({
  browserBundles,
  codeIdeBundles,
  collapsedKeys,
  daemon,
  groupViews,
  inactiveTerminalSleepSessionCount,
  onFocusSession,
  onGxserverAlwaysStartChange,
  onGxserverRestart,
  onGxserverStart,
  onGxserverStop,
  onQuit,
  onSetResourceItemsCollapsed,
  processSnapshotReady,
  processTotals,
  onSleepInactiveSessions,
  onToggle,
  orphanBundles,
  quittingKeys,
  serverBundles,
  linkOpenTarget,
  sessionPersistenceProvider,
}: {
  browserBundles: ResourceProcessBundle[];
  codeIdeBundles: ResourceProcessBundle[];
  collapsedKeys: Set<string>;
  daemon: TitlebarGxserverDaemonStatus;
  groupViews: ResourceGroupView[];
  inactiveTerminalSleepSessionCount: number;
  onFocusSession: (sessionId: string) => void;
  onGxserverAlwaysStartChange: (enabled: boolean) => void;
  onGxserverRestart: () => void;
  onGxserverStart: () => void;
  onGxserverStop: () => void;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onSetResourceItemsCollapsed: (
    targets: readonly ResourceItemCollapseTarget[],
    collapsed: boolean,
  ) => void;
  processSnapshotReady: boolean;
  processTotals: ResourceProcessTotals;
  onSleepInactiveSessions: () => void;
  onToggle: (key: string) => void;
  orphanBundles: ResourceProcessBundle[];
  quittingKeys: Set<string>;
  serverBundles: ResourceProcessBundle[];
  linkOpenTarget: WebLinkOpenTarget;
  sessionPersistenceProvider?: Exclude<SessionPersistenceProvider, "off">;
}) {
  const visibleGroupViews = processSnapshotReady
    ? groupViews.filter((view) => view.bundles.length > 0)
    : [];
  const metricBundles = processSnapshotReady
    ? [
        ...visibleGroupViews.flatMap((view) => view.bundles),
        ...codeIdeBundles,
        ...browserBundles,
        ...orphanBundles,
      ]
    : [];
  /*
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Dev-server rows intentionally duplicate process ownership for discovery,
   * so bundle lists used for row controls avoid folding those duplicates into
   * Sleep/Close targets while row and section metrics still show each listener's
   * current process usage.
   */
  const allBundles = processSnapshotReady ? [...serverBundles, ...metricBundles] : [];
  /**
   * CDXC:TitlebarResources 2026-05-23-10:52:
   * Header actions should be two matching resource controls: one for sleeping
   * only inactive terminal sessions, and one for sleeping all terminal session
   * resources without targeting the app runtime.
   *
   * CDXC:TitlebarResources 2026-06-12-23:37:
   * Header Sleep actions should rely on visible labels and normal button hover
   * instead of tooltip wrappers. Sleep releases live CPU/RAM while preserving
   * the sidebar card, but clickability is more important than hover copy here.
   *
   * CDXC:TitlebarResources 2026-05-25-16:53:
   * The Resources dropdown should manage user-owned work resources, not expose
   * Ghostex's own app-runtime process rows. Keep app process matching available
   * for internal PID ownership, but exclude App Runtime bundles from visible
   * sections and bulk resource actions.
   *
   * CDXC:TitlebarResources 2026-06-30-23:17:
   * The header total is different from row actions: it reports Ghostex's full
   * owned process footprint so it matches external app monitors, while Sleep
   * and Close stay scoped to visible user-resource bundles.
   *
   * CDXC:TitlebarResources 2026-05-25-16:59:
   * The old yellow zmx warning duplicated the action wording and made the menu
   * noisier than the controls themselves. Remove that note and expose the bulk
   * terminal action as Sleep All only when session persistence is active through
   * tmux, zmx, or zellij.
   */
  const persistentSessionMode =
    sessionPersistenceProvider === "tmux" ||
    sessionPersistenceProvider === "zmx" ||
    sessionPersistenceProvider === "zellij";
  const sleepAllSessionBundles = visibleGroupViews
    .flatMap((view) => view.bundles)
    .filter((bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal");
  /**
   * CDXC:TitlebarResources 2026-05-24-20:58:
   * Resource summary and row-action tooltips must stay compact enough for the titlebar area.
   * Keep explanatory copy short and apply the width cap inline because the
   * shared TooltipContent sets its viewport cap with inline styles.
   *
   * CDXC:TitlebarResources 2026-05-25-09:37:
   * Resource summary tooltips need the same compact width cap as action
   * tooltips so Live CPU and Live memory do not stretch across the toolbar.
   *
   * CDXC:TitlebarResources 2026-06-11-18:13:
   * Keep the fixed-size native Resources dropdown stable while the first process table loads.
   * The native child window stays hidden until this view commits with real snapshot data; the loading copy is only an internal fallback.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The header bulk control targets individual expandable resource rows, not
   * the top-level Projects, Browser Tabs, or Orphaned / Detached sections.
   * Keep section containers expanded while the button toggles the same row
   * disclosure state as the per-item chevrons.
   *
   * CDXC:TitlebarResources 2026-06-12-23:37:
   * Header Sleep actions should behave like normal buttons: always visible,
   * always hit-testable, and styled by ordinary CSS :hover/:disabled states.
   * Avoid React hover gates and native-pointer body flags because they made the
   * child-panel buttons appear visible while still rejecting clicks.
  */
  const resourceTooltipStyle = { maxWidth: 220 };
  const liveCpuLabel = processSnapshotReady ? formatWholePercent(processTotals.cpu) : "--";
  const liveMemoryLabel = processSnapshotReady ? formatResourceMemory(processTotals.memoryMb) : "--";
  const resourceItemCollapseTargets = createResourceItemCollapseTargets(allBundles);
  const allResourceItemsCollapsed =
    resourceItemCollapseTargets.length > 0 &&
    resourceItemCollapseTargets.every((target) => isResourceItemCollapsed(target, collapsedKeys));
  const resourceItemToggleLabel = allResourceItemsCollapsed
    ? "Expand all resource items"
    : "Collapse all resource items";
  const [resourcesInfoOpen, setResourcesInfoOpen] = useState(false);
  return (
    <div className="titlebar-resources-panel">
      <div className="titlebar-resources-header">
        <div className="titlebar-resources-title">
          <IconDeviceDesktop aria-hidden="true" size={18} />
          <span>Resources</span>
        </div>
        <div className="titlebar-resources-actions">
          <div className="titlebar-resources-info-control">
            <button
              aria-expanded={resourcesInfoOpen}
              aria-label="Resources information"
              className="titlebar-resources-info-button"
              onClick={() => setResourcesInfoOpen((open) => !open)}
              type="button"
            >
              {/*
               * CDXC:TitlebarResources 2026-06-16-01:08:
               * Resources explanatory copy belongs behind a click-only info
               * affordance beside the bulk expand/collapse control. Keep the
               * dropdown 400px wide and separate each note line with whitespace
               * so the header stays compact while the copy remains available.
               *
               * CDXC:TitlebarResources 2026-06-16-01:54:
               * The info dropdown must fit inside the Resources panel and draw
               * only one background/border surface. Position it from the full
               * header instead of the small icon wrapper so the 400px text area
               * is not clipped.
               *
               * CDXC:TitlebarResources 2026-06-16-02:02:
               * Make the info dropdown wider and lighter than the Resources
               * panel so the explanatory sentences can fit without looking
               * like the same dark layer as the modal behind it.
               */}
              <IconInfoCircle aria-hidden="true" size={14} stroke={1.9} />
            </button>
            {resourcesInfoOpen ? (
              <div className="titlebar-resources-info-popover" role="dialog">
                <div className="titlebar-resources-info-note">
                  <p>This app uses native Ghostty terminals as they're lighter on CPU & RAM than electron/web terminals.</p>
                  <p>The RAM use you see here is the lowest possible for the Agent CLI that you're using.</p>
                  <p>Keep in mind that each CLI uses more/less RAM based on a lot of factors.</p>
                  <p>You can easily sleep all inactive terminals here (Auto-sleep can be configured in settings).</p>
                </div>
              </div>
            ) : null}
          </div>
          <button
            aria-label={resourceItemToggleLabel}
            className="titlebar-resources-collapse-all-button"
            disabled={resourceItemCollapseTargets.length === 0}
            onClick={() =>
              onSetResourceItemsCollapsed(resourceItemCollapseTargets, !allResourceItemsCollapsed)
            }
            type="button"
          >
            {/*
             * CDXC:TitlebarResources 2026-06-12-23:33:
             * The header expand/collapse control belongs to Resources itself:
             * it sits immediately before Sleep Inactive and toggles individual
             * expandable resource items inside each group. It must not collapse
             * Projects, Browser Tabs, or Orphaned / Detached as sections.
             *
             * CDXC:TitlebarResources 2026-06-13-01:54:
             * Match the sidebar Projects bulk-control icon language: the
             * collapse action uses IconArrowsDiagonalMinimize, while the expand
             * action uses IconArrowsDiagonal2.
             */}
            {allResourceItemsCollapsed ? (
              <IconArrowsDiagonal2 aria-hidden="true" size={14} stroke={1.9} />
            ) : (
              <IconArrowsDiagonalMinimize aria-hidden="true" size={14} stroke={1.9} />
            )}
          </button>
          <button
            className="titlebar-resources-action-button"
            data-enabled={String(inactiveTerminalSleepSessionCount > 0)}
            data-variant="sleep"
            disabled={inactiveTerminalSleepSessionCount === 0}
            onClick={onSleepInactiveSessions}
            type="button"
          >
            <IconMoon aria-hidden="true" size={14} stroke={1.8} />
            <span>Sleep Inactive</span>
          </button>
          {persistentSessionMode ? (
            <>
              <button
                className="titlebar-resources-action-button"
                data-variant="sleep"
                disabled={sleepAllSessionBundles.length === 0}
                onClick={() => onQuit(sleepAllSessionBundles)}
                type="button"
              >
                <IconMoon aria-hidden="true" size={14} stroke={1.9} />
                <span>Sleep All</span>
              </button>
            </>
          ) : null}
          <div className="titlebar-resources-summary">
            <AppTooltip
              {...TITLEBAR_TOOLTIP_ROOT_PROPS}
              content={
                <>
                  <span className="titlebar-resource-tooltip-title">Live CPU</span>
                  <span>CPU used by Ghostex and owned child processes.</span>
                </>
              }
              contentClassName="titlebar-resource-tooltip"
              contentStyle={resourceTooltipStyle}
            >
              <span>
                <IconCpu aria-hidden="true" size={13} stroke={1.8} />
                {liveCpuLabel}
              </span>
            </AppTooltip>
            <AppTooltip
              {...TITLEBAR_TOOLTIP_ROOT_PROPS}
              content={
                <>
                  <span className="titlebar-resource-tooltip-title">Live memory</span>
                  <span>RAM used by Ghostex and owned child processes, including app runtime and helper processes.</span>
                </>
              }
              contentClassName="titlebar-resource-tooltip"
              contentStyle={resourceTooltipStyle}
            >
              <span>
                <IconDeviceDesktop aria-hidden="true" size={13} stroke={1.8} />
                {liveMemoryLabel}
              </span>
            </AppTooltip>
          </div>
        </div>
      </div>
      <div className="titlebar-resources-scroll" data-loading={String(!processSnapshotReady)}>
        <TitlebarGxserverDaemonSection
          daemon={daemon}
          onAlwaysStartChange={onGxserverAlwaysStartChange}
          onRestart={onGxserverRestart}
          onStart={onGxserverStart}
          onStop={onGxserverStop}
        />
        {processSnapshotReady ? (
          <>
            {/*
             * CDXC:TitlebarResources 2026-06-22-00:30:
             * Running dev servers should be the first Resources body section,
             * above project session resource groups, so localhost ports are
             * discoverable before users scan terminal/session rows.
             */}
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              linkOpenTarget={linkOpenTarget}
              title="Dev Servers"
              bundles={serverBundles}
            />
            {visibleGroupViews.length > 0 ? (
              visibleGroupViews.map((view) => (
                <TitlebarResourceSection
                  collapsedKeys={collapsedKeys}
                  key={view.group.groupId}
                  onQuit={onQuit}
                  onFocusSession={onFocusSession}
                  onToggle={onToggle}
                  quittingKeys={quittingKeys}
                  title={view.group.title}
                  bundles={view.bundles}
                />
              ))
            ) : (
              <div className="titlebar-resources-empty">No grouped sessions matched running processes.</div>
            )}
            {/*
             * CDXC:TitlebarResources 2026-06-22-13:50:
             * The shared embedded Code runtime belongs after project-owned session groups and before Browser Tabs, where users expect app-wide IDE infrastructure rather than a specific project process.
             */}
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              title="Code IDE"
              bundles={codeIdeBundles}
            />
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              title="Browser Tabs"
              bundles={browserBundles}
            />
            <TitlebarResourceSection
              collapsedKeys={collapsedKeys}
              onQuit={onQuit}
              onFocusSession={onFocusSession}
              onToggle={onToggle}
              quittingKeys={quittingKeys}
              title="Orphaned / Detached"
              bundles={orphanBundles}
            />
          </>
        ) : (
          <div className="titlebar-resources-loading" role="status" aria-live="polite">
            <IconLoader2 aria-hidden="true" className="titlebar-resources-loading-icon" size={16} stroke={1.9} />
            <span>Loading resources...</span>
          </div>
        )}
      </div>
    </div>
  );
}

export function TitlebarGxserverDaemonSection({
  daemon,
  onAlwaysStartChange,
  onRestart,
  onStart,
  onStop,
}: {
  daemon: TitlebarGxserverDaemonStatus;
  onAlwaysStartChange: (enabled: boolean) => void;
  onRestart: () => void;
  onStart: () => void;
  onStop: () => void;
}) {
  const isRunning = daemon.state === "running";
  const isStarting = daemon.state === "starting";
  const shouldShowReloadApp = !isRunning;
  const statusLabel = daemon.version
    ? `${daemon.state} - v${daemon.version}`
    : daemon.state;
  return (
    <section className="titlebar-gxserver-daemon">
      <div className="titlebar-gxserver-daemon-main">
        <span className="titlebar-gxserver-daemon-dot" data-state={daemon.ok === false ? "error" : daemon.state} />
        <div className="titlebar-gxserver-daemon-copy">
          {daemon.message ? <span>{daemon.message}</span> : null}
          <span>{statusLabel}</span>
        </div>
      </div>
      <div className="titlebar-gxserver-daemon-controls">
        {/*
         * CDXC:TitlebarDaemonControls 2026-06-12-11:51:
         * The Resources dropdown should expose Restart as the primary daemon action. Hide manual Start/Stop controls so users do not manage daemon lifecycle from this compact status row.
         */}
        <AppTooltip
          {...TITLEBAR_TOOLTIP_ROOT_PROPS}
          content="Restart daemon"
          contentClassName="titlebar-resource-tooltip"
        >
          <button
            aria-label="Restart gxserver"
            className="titlebar-gxserver-daemon-icon-button"
            disabled={isStarting}
            onClick={onRestart}
            type="button"
          >
            <IconRefresh aria-hidden="true" size={14} stroke={1.9} />
          </button>
        </AppTooltip>
        {shouldShowReloadApp ? (
          <AppTooltip
            {...TITLEBAR_TOOLTIP_ROOT_PROPS}
            content="Reload app"
            contentClassName="titlebar-resource-tooltip"
          >
            <button
              aria-label="Reload Ghostex"
              className="titlebar-gxserver-daemon-icon-button"
              onClick={() => {
                window.location.reload();
              }}
              type="button"
            >
              <IconRefresh aria-hidden="true" size={14} stroke={1.9} />
            </button>
          </AppTooltip>
        ) : null}
        {/*
         * CDXC:TitlebarDaemonControls 2026-06-12-11:51:
         * If gxserver is off unexpectedly, show Reload App as the recovery action so the webview can rehydrate and reconnect instead of asking users to manually start the daemon here.
         */}
        {/*
         * CDXC:TitlebarDaemonControls 2026-06-12-11:56:
         * Hide the Always start checkbox from the compact Resources daemon row; this status surface should only offer Restart, plus Reload App when gxserver is off.
         */}
        {/* <label className="titlebar-gxserver-daemon-checkbox">
          <input
            checked={daemon.alwaysStart}
            onChange={(event) => onAlwaysStartChange(event.currentTarget.checked)}
            type="checkbox"
          />
          <span>Always start</span>
        </label> */}
      </div>
    </section>
  );
}

export function TitlebarResourceSection({
  bundles,
  collapsedKeys,
  onQuit,
  onFocusSession,
  onToggle,
  quittingKeys,
  linkOpenTarget,
  title,
}: {
  bundles: ResourceProcessBundle[];
  collapsedKeys: Set<string>;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onFocusSession: (sessionId: string) => void;
  onToggle: (key: string) => void;
  quittingKeys: Set<string>;
  linkOpenTarget?: WebLinkOpenTarget;
  title: string;
}) {
  if (bundles.length === 0) {
    return null;
  }
  const sectionCpu = sumBundleCpu(bundles);
  const sectionMemory = sumBundleMemory(bundles);
  const sortedBundles = sortResourceBundlesForDisplay(bundles, quittingKeys);
  const actionableBundles = bundles.filter(isResourceBundleActionable);
  const hasTerminalSession = actionableBundles.some(
    (bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal",
  );
  const hasServer = actionableBundles.some((bundle) => bundle.type === "server");
  const sectionActionBundles = hasTerminalSession
    ? actionableBundles.filter((bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal")
    : actionableBundles;
  const sectionActionLabel = hasTerminalSession ? "Sleep Project" : hasServer ? "Stop Servers" : "Quit";
  const sectionActionTooltipTitle = hasTerminalSession
    ? "Sleep project"
    : hasServer
      ? "Stop servers"
      : "Quit this group";
  const sectionActionTooltipBody = hasTerminalSession
    ? "Sleeps this project's terminal sessions and keeps them restorable in the sidebar."
    : hasServer
      ? "Stops the listener-backed server processes without sleeping the owning terminal sessions."
      : "Stops user-owned live processes and closes related surfaces.";
  /**
   * CDXC:TitlebarResources 2026-05-25-14:21:
   * Resource action tooltips share the compact width cap used by header and
   * summary tooltips, including Quit group, so long process-management copy
   * wraps near the hovered control instead of spanning the window.
   *
   * CDXC:TitlebarResources 2026-05-26-13:11:
   * Project resource groups that include terminal sessions should expose the
   * group action as Sleep Project, not Quit. Limit that action to terminal
   * session bundles so browser/code resources are not closed by a sleep-labeled
   * control.
   *
   * CDXC:TitlebarResources 2026-06-11-18:30:
   * Resource section headers are static labels now: no per-section chevron and
   * no click target, so the fixed native dropdown avoids visually noisy
   * competing collapse controls.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The single header button controls the individual resource rows in bulk,
   * not this section container. Always render section bodies so Projects,
   * Browser Tabs, and Orphaned / Detached remain visible grouping labels.
   *
   * CDXC:TitlebarResources 2026-06-15-13:45:
   * Section-level Quit must target the same action-eligible resources as row
   * Close. Keep shared browser helper bundles visible for diagnostics, but do
   * not let a bulk action close infrastructure that embedded browser panes need
   * to keep working.
   *
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Dev Servers rows use Stop language because the action targets only the
   * listener process tree. Do not route those rows through session sleep or
   * project close semantics.
   */
  const resourceTooltipStyle = { maxWidth: 220 };
  return (
    <section className="titlebar-resource-section">
      <div className="titlebar-resource-section-heading">
        <div className="titlebar-resource-section-label">
          <span>{title}</span>
          <span className="titlebar-resource-section-summary">
            <span>
              <IconCpu aria-hidden="true" size={12} stroke={1.8} />
              {formatWholePercent(sectionCpu)}
            </span>
            <span>
              <IconDeviceDesktop aria-hidden="true" size={12} stroke={1.8} />
              {formatResourceMemory(sectionMemory)}
            </span>
            <span className="titlebar-resource-section-count">{bundles.length}</span>
          </span>
        </div>
        {sectionActionBundles.length > 0 ? (
          <AppTooltip
            {...TITLEBAR_TOOLTIP_ROOT_PROPS}
            content={
              <>
                <span className="titlebar-resource-tooltip-title">{sectionActionTooltipTitle}</span>
                <span>{sectionActionTooltipBody}</span>
              </>
            }
            contentClassName="titlebar-resource-tooltip"
            contentStyle={resourceTooltipStyle}
          >
            <button
              className="titlebar-resource-section-quit-button"
              data-action={hasTerminalSession ? "sleep" : hasServer ? "stop" : "quit"}
              onClick={() => onQuit(sectionActionBundles)}
              type="button"
            >
              {sectionActionLabel}
            </button>
          </AppTooltip>
        ) : null}
      </div>
      <div className="titlebar-resource-section-body">
        {sortedBundles.map((bundle) => (
          <TitlebarResourceBundle
            bundle={bundle}
            collapsedKeys={collapsedKeys}
            isQuitting={quittingKeys.has(bundle.key)}
            key={bundle.key}
            onFocusSession={onFocusSession}
            onQuit={onQuit}
            onToggle={onToggle}
            linkOpenTarget={linkOpenTarget}
          />
        ))}
      </div>
    </section>
  );
}

export function TitlebarResourceBundle({
  bundle,
  collapsedKeys,
  isQuitting,
  onQuit,
  onFocusSession,
  onToggle,
  linkOpenTarget,
}: {
  bundle: ResourceProcessBundle;
  collapsedKeys: Set<string>;
  isQuitting: boolean;
  onQuit: (bundles: ResourceProcessBundle[]) => void;
  onFocusSession: (sessionId: string) => void;
  onToggle: (key: string) => void;
  linkOpenTarget?: WebLinkOpenTarget;
}) {
  const hasChildren = bundle.childProcesses.length > 0;
  /**
   * CDXC:TitlebarResources 2026-05-16-18:28:
   * Sessions often own several agent/runtime child processes, so their rows
   * should start collapsed to keep the Resources menu scannable. Store only
   * explicit user expansions for session bundles while section rows and other
   * bundle types keep the existing collapsed-key behavior.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The Resources header bulk toggle uses the same target helper as row
   * chevrons so it collapses individual items inside groups, not the group
   * sections themselves.
   */
  const bundleCollapseTarget = createResourceItemCollapseTarget(bundle);
  const bundleToggleKey = bundleCollapseTarget?.key ?? bundle.key;
  const isCollapsed = bundleCollapseTarget
    ? isResourceItemCollapsed(bundleCollapseTarget, collapsedKeys)
    : false;
  /**
   * CDXC:TitlebarResources 2026-05-23-10:52:
   * Terminal-session Quit from Resources terminates the live process tree but
   * intentionally keeps the session card in the sidebar as sleeping. Use the
   * sleep affordance for those rows; keep the quit affordance for browser,
   * code, and detached process rows that are actually removed or closed.
   */
  const preservesSidebarSession =
    bundle.type === "session" && bundle.session?.sessionKind === "terminal";
  const isServer = bundle.type === "server";
  const serverPortless = bundle.portless;
  const mainLabel = getResourceBundleMainLabel(bundle);
  const mainUrl = getResourceBundleMainUrl(bundle);
  const showPortlessSetupAction =
    isServer && serverPortless !== undefined && !serverPortless.isSetupActive;
  const focusSessionId = resourceBundleFocusSessionId(bundle);
  const isActionable = isResourceBundleActionable(bundle);
  const actionLabel = preservesSidebarSession
    ? `Sleep ${bundle.label}`
    : isServer
      ? `Stop server ${bundle.label}`
      : `Close ${bundle.label}`;
  /**
   * CDXC:TitlebarResources 2026-05-28-10:39:
   * Session resource rows expose Focus beside Sleep/Close. Focus uses the same
   * sidebar session id as Sleep so cross-project Resources rows activate the
   * exact owning session.
   *
   * CDXC:TitlebarResources 2026-06-13-00:56:
   * Per-item resource action buttons should behave like normal visible controls,
   * not hover-revealed overlays. Keep metrics visible, keep actions in stable
   * grid columns, and avoid tooltip trigger wrappers or native-pointer hover
   * gates that can make visible row buttons reject clicks.
   *
   * CDXC:TitlebarResources 2026-06-15-13:45:
   * Row-level Close should disappear for app-critical shared browser helper
   * bundles instead of disabling the button or letting the click reach process
   * termination. Users should only be able to close resource rows that map to a
   * restorable terminal session or an owned browser/code/orphan surface.
   *
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Dev-server Focus may jump to the owning terminal, but Stop must only signal
   * the listener process tree and must not sleep the terminal session.
   */
  return (
    <div className="titlebar-resource-bundle" data-quitting={String(isQuitting)}>
      <div
        className="titlebar-resource-row"
        data-expandable={String(hasChildren)}
        onClick={() => {
          if (hasChildren) {
            onToggle(bundleToggleKey);
          }
        }}
      >
        <div className="titlebar-resource-main">
          {hasChildren ? (
            <button
              className="titlebar-resource-collapse-button"
              onClick={(event) => {
                event.stopPropagation();
                onToggle(bundleToggleKey);
              }}
              type="button"
            >
              <IconChevronDown aria-hidden="true" data-collapsed={String(isCollapsed)} size={14} stroke={1.8} />
            </button>
          ) : (
            <span className="titlebar-resource-collapse-spacer" />
          )}
          <span className="titlebar-resource-avatar">{getResourceBundleAvatar(bundle)}</span>
          <span className="titlebar-resource-text">
            {mainUrl ? (
              <a
                className="titlebar-resource-name titlebar-resource-main-link"
                href={mainUrl}
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  openResourceBundleMainUrl(bundle, mainUrl, linkOpenTarget);
                }}
              >
                {mainLabel}
              </a>
            ) : (
              <span className="titlebar-resource-name">{mainLabel}</span>
            )}
            <span className="titlebar-resource-meta">
              {isQuitting ? (
                preservesSidebarSession ? (
                  "Sleeping..."
                ) : isServer ? (
                  "Stopping..."
                ) : (
                  "Quitting..."
                )
              ) : (
                <>
                  <span className="titlebar-resource-meta-text">{getResourceBundleMeta(bundle)}</span>
                  {showPortlessSetupAction && serverPortless ? (
                    <button
                      className="titlebar-resource-portless-action"
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        runPortlessResourcesSetupAction(serverPortless);
                      }}
                      type="button"
                    >
                      {serverPortless.setupActionLabel}
                    </button>
                  ) : null}
                </>
              )}
            </span>
          </span>
        </div>
        <div className="titlebar-resource-metrics" aria-label="Resource usage">
          <span className="titlebar-resource-metric">
            <IconCpu aria-hidden="true" size={13} stroke={1.8} />
            {formatWholePercent(bundle.cpu)}
          </span>
          <span className="titlebar-resource-metric">
            <IconDeviceDesktop aria-hidden="true" size={13} stroke={1.8} />
            {formatResourceMemory(bundle.memoryMb)}
          </span>
        </div>
        {focusSessionId ? (
          <button
            aria-label={`Focus ${bundle.label}`}
            className="titlebar-resource-focus-button"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onFocusSession(focusSessionId);
            }}
            type="button"
          >
            <IconFocus2 aria-hidden="true" size={13} stroke={1.9} />
          </button>
        ) : null}
        {isActionable ? (
          <button
            aria-label={actionLabel}
            className="titlebar-resource-kill-button"
            data-action={preservesSidebarSession ? "sleep" : isServer ? "stop" : "quit"}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onQuit([ bundle ]);
            }}
            type="button"
          >
            {preservesSidebarSession ? (
              <IconMoon aria-hidden="true" size={13} stroke={1.9} />
            ) : isServer ? (
              <IconSquareMinus aria-hidden="true" size={13} stroke={1.9} />
            ) : (
              <IconX aria-hidden="true" size={13} stroke={2} />
            )}
          </button>
        ) : null}
      </div>
      {hasChildren && !isCollapsed ? (
        <div className="titlebar-resource-children">
          {bundle.childProcesses.slice(0, 8).map((process) => (
            <div className="titlebar-resource-child-row" key={process.pid}>
              <span className="titlebar-resource-child-name">
                {getResourceChildProcessName(bundle, process)} pid {process.pid}
              </span>
              <div className="titlebar-resource-child-metrics" aria-label="Child process resource usage">
                <span className="titlebar-resource-metric">
                  <IconCpu aria-hidden="true" size={12} stroke={1.8} />
                  {formatWholePercent(process.cpu)}
                </span>
                <span className="titlebar-resource-metric">
                  <IconDeviceDesktop aria-hidden="true" size={12} stroke={1.8} />
                  {formatResourceMemory(process.rssMb)}
                </span>
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function getResourceChildProcessName(
  bundle: ResourceProcessBundle,
  process: ResourceProcess,
): string {
  return bundle.type === "browser" ? getBrowserProcessDisplayName(process) : getProcessDisplayName(process);
}

export function getResourceBundleAvatar(bundle: ResourceProcessBundle): ReactNode {
  const agentIcon = bundle.session?.agentIcon;
  if (isSidebarAgentIcon(agentIcon)) {
    /**
     * CDXC:TitlebarResources 2026-05-26-13:24:
     * Resource rows should use the same shared agent-logo mask assets as Agents
     * Hub profile chips instead of two-letter text abbreviations. This keeps
     * Codex, Claude, browser, and other agent identities visually aligned
     * across the sidebar and resource manager.
     */
    return (
      <span
        aria-hidden="true"
        className="titlebar-resource-avatar-logo"
        data-agent-icon={agentIcon}
        style={{
          backgroundColor: AGENT_LOGO_COLORS[agentIcon],
          maskImage: `url("${AGENT_LOGOS[agentIcon]}")`,
          WebkitMaskImage: `url("${AGENT_LOGOS[agentIcon]}")`,
        }}
      />
    );
  }
  if (bundle.type === "code") {
    return <IconCode aria-hidden="true" size={15} stroke={1.9} />;
  }
  if (bundle.type === "browser") {
    return <IconWorld aria-hidden="true" size={15} stroke={1.9} />;
  }
  if (bundle.type === "server") {
    return <IconWorld aria-hidden="true" size={15} stroke={1.9} />;
  }
  if (bundle.session?.sessionKind === "terminal") {
    return <IconTerminal2 aria-hidden="true" size={15} stroke={1.9} />;
  }
  return <IconBox aria-hidden="true" size={15} stroke={1.9} />;
}

export function isSidebarAgentIcon(candidate: unknown): candidate is SidebarAgentIcon {
  return typeof candidate === "string" && Object.prototype.hasOwnProperty.call(AGENT_LOGOS, candidate);
}

export function getResourceBundleMainLabel(bundle: ResourceProcessBundle): string {
  if (bundle.server && bundle.portless?.isSetupActive) {
    return bundle.portless.hostname;
  }
  if (bundle.server && bundle.portless) {
    return resourceServerLocalhostLabel(bundle.server);
  }
  return bundle.label;
}

export function getResourceBundleMainUrl(bundle: ResourceProcessBundle): string | undefined {
  if (!bundle.server) {
    return undefined;
  }
  if (bundle.portless?.isSetupActive) {
    return resourcePortlessUrl(bundle.portless);
  }
  return resourceServerLocalhostUrl(bundle.server);
}

export function openResourceBundleMainUrl(
  bundle: ResourceProcessBundle,
  url: string,
  linkOpenTarget: WebLinkOpenTarget | undefined,
): void {
  /*
   * CDXC:TerminalDevServers 2026-06-23-19:22:
   * Resources dev-server links should open either in the user's system default browser or the internal browser. Do not expose a per-browser target list here; only server bundles should read this setting so future resource links keep their existing route.
   *
   * CDXC:WebLinkOpenTarget 2026-08-19:
   * That choice is now the app-wide webLinkOpenTarget shared with terminal and session chat links, so these rows stop disagreeing with the Browser setting.
   */
  if (bundle.type === "server" && linkOpenTarget === "system-default-browser") {
    postNative({ type: "openExternalUrl", url });
    return;
  }
  postTitlebarSidebarCommand({ type: "openBrowserPane", url });
}

export function getResourceBundleMeta(bundle: ResourceProcessBundle): string {
  if (bundle.server) {
    const pid = bundle.process?.pid ?? bundle.server.pid;
    if (bundle.portless) {
      const processMeta = `${bundle.server.commandName} pid ${pid}`;
      return bundle.portless.isSetupActive
        ? `${resourceServerLocalhostLabel(bundle.server)} - ${processMeta}`
        : `${bundle.portless.setupStatusLabel} - ${processMeta}`;
    }
    return `${bundle.server.commandName} pid ${pid}`;
  }
  if (bundle.session) {
    const provider = bundle.session.sessionPersistenceProvider
      ? `${bundle.session.sessionPersistenceProvider} terminal`
      : bundle.session.sessionKind ?? "session";
    const pid = bundle.process?.pid ? ` pid ${bundle.process.pid}` : "";
    return `${provider}${pid}`;
  }
  if (bundle.browserTab) {
    return bundle.browserTab.url?.trim() || "Browser tab";
  }
  if (bundle.type === "browser") {
    if (bundle.key === "browser:runtime") {
      return "Shared GPU, network, and storage helpers";
    }
    if (bundle.key === "browser:unmatched-renderers") {
      return "No visible Browser tab matched these helpers";
    }
    return "Browser helper processes";
  }
  if (bundle.process?.pid) {
    return `pid ${bundle.process.pid}`;
  }
  return bundle.type;
}

export function resourceServerLocalhostLabel(server: Pick<ResourceListeningServer, "port">): string {
  return `localhost:${server.port}`;
}

export function resourceServerLocalhostUrl(server: Pick<ResourceListeningServer, "port">): string {
  return `http://${resourceServerLocalhostLabel(server)}`;
}

export function resourcePortlessUrl(portless: Pick<ResourcePortlessServerPresentation, "hostname" | "protocol">): string {
  return `${portless.protocol}://${portless.hostname}`;
}

export function runPortlessResourcesSetupAction(portless: ResourcePortlessServerPresentation): void {
  if (!portless.setupAction) {
    openPortlessResourcesSettings();
    return;
  }
  postTitlebarSidebarCommand({
    action: portless.setupAction,
    protocol: portless.protocol,
    requestId: createPortlessResourcesAdminRequestId(portless.setupAction),
    type: "runPortlessSettingsAdminAction",
  });
}

export function openPortlessResourcesSettings(): void {
  window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage({
    initialSearchQuery: "Portless",
    initialTab: "projects",
    modal: "settings",
    type: "open",
  });
}

export function createPortlessResourcesAdminRequestId(action: NativePortlessAdminInstallAction): string {
  return `portless-resources-${action}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
