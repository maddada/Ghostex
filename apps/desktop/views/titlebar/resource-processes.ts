import type { SidebarPortlessState } from "@/packages/shared/session-grid-contract-sidebar";
import type { NativePortlessAdminInstallAction } from "@/packages/shared/native-ghostty-host-protocol";
import {
  createCombinedProjectSessionId,
  parseCombinedProjectGroupId,
  parseCombinedProjectSessionId,
} from "../combined-sidebar-mode";
import { codeServerResourcePort } from "./constants";
import { runNativeProcess } from "./native-bridge";
import type {
  ResourceGroupView,
  ResourceItemCollapseTarget,
  ResourceListeningServer,
  ResourcePortlessServerPresentation,
  ResourceProcess,
  ResourceProcessBundle,
  ResourceProcessTotals,
  TitlebarBrowserTabResource,
  TitlebarResourceGroup,
  TitlebarResourceSession,
} from "./types";

export function parseResourceProcessTable(stdout: string): ResourceProcess[] {
  return stdout
    .split("\n")
    .map((line) => {
      const match = /^\s*(\d+)\s+(\d+)\s+([0-9.]+)\s+(\d+)\s+(.+?)\s*$/.exec(line);
      if (!match) {
        return undefined;
      }
      const pid = Number(match[1]);
      const ppid = Number(match[2]);
      const cpu = Number(match[3]);
      const rssKb = Number(match[4]);
      if (!Number.isFinite(pid) || !Number.isFinite(ppid) || !Number.isFinite(cpu) || !Number.isFinite(rssKb)) {
        return undefined;
      }
      return {
        command: match[5] ?? "",
        cpu,
        pid,
        ppid,
        rssMb: rssKb / 1024,
      };
    })
    .filter((process): process is ResourceProcess => process !== undefined);
}

export function parseResourceListeningServerTable(stdout: string): ResourceListeningServer[] {
  const servers: ResourceListeningServer[] = [];
  let currentPid: number | undefined;
  let currentCommandName = "";

  for (const rawLine of stdout.split("\n")) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }
    const field = line[0];
    const value = line.slice(1);
    if (field === "p") {
      const pid = Number(value);
      currentPid = Number.isFinite(pid) && pid > 0 ? pid : undefined;
      currentCommandName = "";
      continue;
    }
    if (field === "c") {
      currentCommandName = value.trim();
      continue;
    }
    if (field !== "n" || currentPid === undefined) {
      continue;
    }
    const endpoint = parseResourceListeningEndpoint(value);
    if (!endpoint) {
      continue;
    }
    servers.push({
      commandName: currentCommandName || "server",
      host: endpoint.host,
      pid: currentPid,
      port: endpoint.port,
      url: endpoint.url,
    });
  }

  return uniqueResourceListeningServers(servers);
}

export function parseResourceListeningEndpoint(endpoint: string): { host: string; port: number; url: string } | undefined {
  const trimmed = endpoint.trim();
  if (!trimmed) {
    return undefined;
  }

  let rawHost = "";
  let rawPort = "";
  if (trimmed.startsWith("[")) {
    const hostEnd = trimmed.indexOf("]:");
    if (hostEnd < 0) {
      return undefined;
    }
    rawHost = trimmed.slice(1, hostEnd);
    rawPort = trimmed.slice(hostEnd + 2);
  } else {
    const separatorIndex = trimmed.lastIndexOf(":");
    if (separatorIndex < 0) {
      return undefined;
    }
    rawHost = trimmed.slice(0, separatorIndex);
    rawPort = trimmed.slice(separatorIndex + 1);
  }

  const port = Number(rawPort);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    return undefined;
  }
  const host = resourceServerDisplayHost(rawHost);
  const formattedHost = host.includes(":") ? `[${host}]` : host;
  return {
    host,
    port,
    url: `http://${formattedHost}:${port}`,
  };
}

export function parseResourceListeningServerCwdTable(stdout: string): Map<number, string> {
  const cwdByPid = new Map<number, string>();
  let currentPid: number | undefined;

  for (const rawLine of stdout.split("\n")) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }
    const field = line[0];
    const value = line.slice(1);
    if (field === "p") {
      const pid = Number(value);
      currentPid = Number.isFinite(pid) && pid > 0 ? pid : undefined;
      continue;
    }
    if (field === "n" && currentPid !== undefined && value.trim()) {
      cwdByPid.set(currentPid, value.trim());
    }
  }

  return cwdByPid;
}

export function uniqueResourceListeningServers(servers: ResourceListeningServer[]): ResourceListeningServer[] {
  const seen = new Set<string>();
  return servers.filter((server) => {
    const key = `${server.pid}:${server.port}:${server.host}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

export function resourceServerDisplayHost(host: string): string {
  const normalized = host.trim().replace(/^\[|\]$/gu, "");
  return !normalized ||
    normalized === "*" ||
    normalized === "0.0.0.0" ||
    normalized === "::" ||
    normalized === "::1" ||
    normalized === "127.0.0.1"
    ? "localhost"
    : normalized;
}

export async function readResourceProcesses(): Promise<ResourceProcess[]> {
  const result = await runNativeProcess("/bin/ps", [
    "-axo",
    "pid=,ppid=,pcpu=,rss=,command=",
  ]);
  return result.exitCode === 0 ? parseResourceProcessTable(result.stdout) : [];
}

export async function readResourceListeningServers(): Promise<ResourceListeningServer[]> {
  /*
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * Resources needs a top Dev Servers section sourced from real TCP listeners,
   * not terminal text heuristics. Read lsof's structured fields while the panel
   * is open, then use cwd only for internal ownership matching without rendering
   * or logging user paths.
   */
  try {
    const listenerResult = await runNativeProcess(
      "/usr/sbin/lsof",
      ["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"],
      { timeoutMs: 10_000 },
    );
    if (listenerResult.exitCode !== 0) {
      return [];
    }

    const servers = parseResourceListeningServerTable(listenerResult.stdout);
    const pids = Array.from(new Set(servers.map((server) => server.pid)));
    if (pids.length === 0) {
      return servers;
    }

    const cwdResult = await runNativeProcess(
      "/usr/sbin/lsof",
      ["-nP", "-a", "-d", "cwd", "-F", "pn", "-p", pids.join(",")],
      { timeoutMs: 10_000 },
    );
    if (cwdResult.exitCode !== 0) {
      return servers;
    }

    const cwdByPid = parseResourceListeningServerCwdTable(cwdResult.stdout);
    return servers.map((server) => {
      const cwd = cwdByPid.get(server.pid);
      return cwd ? { ...server, cwd } : server;
    });
  } catch {
    return [];
  }
}

/**
 * CDXC:TitlebarResources 2026-05-23-10:46:
 * Resource-manager Quit is a process-manager action, so it must terminate the
 * exact processes shown in the dropdown while the sidebar separately preserves
 * terminal cards as sleeping sessions. Recheck the command before SIGKILL so a
 * delayed hard kill cannot target an unrelated process that reused the PID.
 *
 * CDXC:TitlebarResources 2026-06-22-00:30:
 * Dev Servers Stop should behave like a terminal Ctrl-C against the listener
 * process tree before escalating. Use SIGINT for server bundles and keep SIGTERM
 * for the existing close/sleep resource cleanup paths.
 */
export async function terminateResourceProcesses(
  processes: ResourceProcess[],
  options: { gracefulSignal?: "INT" | "TERM" } = {},
): Promise<void> {
  const targets = new Map(
    processes
      .filter((process) => Number.isFinite(process.pid) && process.pid > 1)
      .map((process) => [process.pid, process.command]),
  );
  if (targets.size === 0) {
    return;
  }

  const gracefulSignal = options.gracefulSignal ?? "TERM";
  await runNativeProcess("/bin/kill", [`-${gracefulSignal}`, ...Array.from(targets.keys()).map(String)]);
  window.setTimeout(() => {
    void (async () => {
      const liveProcesses = await readResourceProcesses();
      const liveTargetPids = liveProcesses
        .filter((process) => targets.get(process.pid) === process.command)
        .map((process) => process.pid);
      if (liveTargetPids.length > 0) {
        await runNativeProcess("/bin/kill", ["-KILL", ...liveTargetPids.map(String)]);
      }
    })().catch((error) => {
      console.warn("Failed to finish terminating Ghostex resources", error);
    });
  }, 1_500);
}

export function createResourceGroupViews(
  browserTabs: TitlebarBrowserTabResource[],
  resourceGroups: TitlebarResourceGroup[],
  processes: ResourceProcess[],
  servers: ResourceListeningServer[],
  codeEditorProjectIds: string[],
): {
  browserBundles: ResourceProcessBundle[];
  codeIdeBundles: ResourceProcessBundle[];
  groupViews: ResourceGroupView[];
  orphanBundles: ResourceProcessBundle[];
} {
  const claimedPids = new Set<number>();
  const childrenByParent = createProcessChildrenMap(processes);
  const groupedBrowserTabIds = new Set<string>();
  const groupViews = resourceGroups.map((group) => {
    const groupBrowserTabs = browserTabs
      .filter((tab) => isBrowserTabInResourceGroup(tab, group))
      .map((tab) => ({
        ...tab,
        projectId: tab.projectId ?? resourceGroupProjectIdForBrowserTab(tab, group),
      }));
    groupBrowserTabs.forEach((tab) => groupedBrowserTabIds.add(tab.id));
    const bundles = group.sessions
      .map((session) => createSessionResourceBundle(session, processes, childrenByParent, claimedPids))
      .filter((bundle): bundle is ResourceProcessBundle => bundle !== undefined);
    const browserBundles = createBrowserBundles(groupBrowserTabs, processes, claimedPids, {
      includeRuntimeBundles: false,
    });
    return {
      bundles: [...bundles, ...browserBundles],
      group,
    };
  });
  const codeIdeBundles = createCodeIdeResourceBundles(
    servers,
    processes,
    childrenByParent,
    claimedPids,
    codeEditorProjectIds,
  );
  claimAppRuntimeProcesses(processes, childrenByParent, claimedPids);
  const browserBundles = createBrowserBundles(
    browserTabs.filter((tab) => !groupedBrowserTabIds.has(tab.id)),
    processes,
    claimedPids,
  );
  const orphanBundles = createOrphanBundles(processes, childrenByParent, claimedPids);
  return { browserBundles, codeIdeBundles, groupViews, orphanBundles };
}

export function createGhostexResourceProcessTotals(processes: ResourceProcess[]): ResourceProcessTotals {
  /*
   * CDXC:TitlebarResources 2026-06-30-23:17:
   * The Resources header RAM/CPU total must match external monitors that group Ghostex with every owned child process, not only the rows that are visible and safe to Sleep/Close.
   * Compute the app-wide total from the raw `ps` snapshot while leaving row bundles scoped to actionable user resources.
   *
   * CDXC:TitlebarResources 2026-06-30-23:29:
   * Use Ghostex-owned executable roots plus their descendants for the header total.
   * This matches app monitors that aggregate child processes and avoids rescanning every long command line with broad ownership regexes while the Resources child window refreshes.
   *
   * CDXC:TitlebarResources 2026-06-30-23:43:
   * gxserver, zmx, and bundled helper processes can daemonize under launchd while still belonging to Ghostex's app footprint. Seed totals from any executable inside the Ghostex app bundle, then traverse descendants so orphaned helper roots and their agent children remain counted without treating arbitrary command text as ownership evidence.
   */
  const childrenByParent = createProcessChildrenMap(processes);
  const appRootProcesses = processes.filter(isGhostexAppBundleProcess);
  const ownedProcesses = collectProcessTree(appRootProcesses, childrenByParent);
  return {
    cpu: sumProcessCpu(ownedProcesses),
    memoryMb: sumProcessMemory(ownedProcesses),
    processCount: ownedProcesses.length,
  };
}

export function isGhostexAppBundleProcess(process: ResourceProcess): boolean {
  const executablePath = process.command.split(/\s+/, 1)[0] ?? "";
  return /\/Ghostex(?:-dev)?\.app\/Contents\//i.test(executablePath);
}

export function createResourceServerBundles(
  servers: ResourceListeningServer[],
  resourceViews: ReturnType<typeof createResourceGroupViews>,
  processes: ResourceProcess[],
  portless: SidebarPortlessState | undefined,
): ResourceProcessBundle[] {
  /*
   * CDXC:TitlebarResources 2026-06-22-00:30:
   * The Dev Servers section belongs above project sessions but must still be
   * owned by a visible terminal resource. Attribute listeners by process-tree
   * membership first, then by listener cwd inside the project path when a
   * provider-backed session is visible without a sampled zmx root.
   *
   * CDXC:PortlessResources 2026-06-23-15:18:
   * Resources may show Portless domains only on Ghostex-owned live server rows.
   * Join routePreviews to the existing listener-backed server bundles by
   * project id, session id, and port so assigned domains never become extra
   * rows and Stop continues to target only the live server process tree.
   */
  const portlessPreviewsByOwnerAndPort = createPortlessRoutePreviewMap(portless);
  const processByPid = new Map(processes.map((process) => [process.pid, process]));
  const childrenByParent = createProcessChildrenMap(processes);
  const terminalOwners = resourceViews.groupViews.flatMap((view) =>
    view.bundles
      .filter((bundle) => bundle.type === "session" && bundle.session?.sessionKind === "terminal")
      .map((bundle) => ({ bundle, view })),
  );
  const ownerByPid = new Map<number, { bundle: ResourceProcessBundle; view: ResourceGroupView }>();
  for (const owner of terminalOwners) {
    owner.bundle.pids.forEach((pid) => ownerByPid.set(pid, owner));
  }

  return servers
    .map((server): ResourceProcessBundle | undefined => {
      const owner =
        ownerByPid.get(server.pid) ?? findResourceServerCwdOwner(server, terminalOwners);
      if (!owner) {
        return undefined;
      }

      const process = processByPid.get(server.pid);
      const tree = process ? collectProcessTree([process], childrenByParent) : [];
      const pids = tree.length > 0 ? tree.map((treeProcess) => treeProcess.pid) : [server.pid];
      const portlessPreview = owner.bundle.session
        ? portlessPreviewsByOwnerAndPort.get(
            createPortlessRoutePreviewKeyForSession(owner.bundle.session, server.port),
          )
        : undefined;
      return {
        childProcesses: process ? tree.filter((treeProcess) => treeProcess.pid !== process.pid) : [],
        cpu: sumProcessCpu(tree),
        key: `server:${server.pid}:${server.port}`,
        label: resourceServerLabel(server),
        memoryMb: sumProcessMemory(tree),
        pids,
        ...(portlessPreview ? { portless: portlessPreview } : {}),
        process,
        server,
        session: owner.bundle.session,
        type: "server",
      };
    })
    .filter((bundle): bundle is ResourceProcessBundle => bundle !== undefined)
    .sort((left, right) => {
      const leftPort = left.server?.port ?? 0;
      const rightPort = right.server?.port ?? 0;
      return leftPort === rightPort ? left.label.localeCompare(right.label) : leftPort - rightPort;
    });
}

export function findResourceServerCwdOwner(
  server: ResourceListeningServer,
  terminalOwners: { bundle: ResourceProcessBundle; view: ResourceGroupView }[],
): { bundle: ResourceProcessBundle; view: ResourceGroupView } | undefined {
  /*
   * CDXC:TitlebarResources 2026-07-26:
   * Project paths nest, so the first group whose path contains the listener cwd
   * is not necessarily its owner: a home-directory or monorepo-root project
   * would otherwise claim every dev server started inside a project below it.
   * Attribute the listener to the deepest matching project path instead.
   */
  if (!server.cwd) {
    return undefined;
  }
  let owner: { bundle: ResourceProcessBundle; view: ResourceGroupView } | undefined;
  let ownerPathLength = -1;
  for (const candidate of terminalOwners) {
    const projectPath = normalizeResourceOwnershipPath(candidate.view.group.projectPath);
    if (
      !projectPath ||
      projectPath.length <= ownerPathLength ||
      !isResourcePathInsideOrEqualTo(server.cwd, projectPath)
    ) {
      continue;
    }
    owner = candidate;
    ownerPathLength = projectPath.length;
  }
  return owner;
}

export function createPortlessRoutePreviewMap(
  portless: SidebarPortlessState | undefined,
): Map<string, ResourcePortlessServerPresentation> {
  const previewsByOwnerAndPort = new Map<string, ResourcePortlessServerPresentation>();
  const routePreviews = portless?.presentation?.routePreviews ?? [];
  if (!portless || routePreviews.length === 0 || portless.presentation?.routePreviewStatus !== "current") {
    return previewsByOwnerAndPort;
  }
  for (const preview of routePreviews) {
    const key = createPortlessRoutePreviewKey(preview.projectId, preview.sessionId, preview.port);
    if (previewsByOwnerAndPort.has(key)) {
      continue;
    }
    previewsByOwnerAndPort.set(key, {
      hostname: preview.hostname,
      isSetupActive: isPortlessResourceSetupActive(portless),
      protocol: preview.protocol,
      setupAction: getTitlebarPortlessResourcesSetupAction(portless),
      setupActionLabel: getTitlebarPortlessResourcesSetupActionLabel(portless),
      setupStatusLabel: getTitlebarPortlessResourcesSetupStatusLabel(portless),
    });
  }
  return previewsByOwnerAndPort;
}

export function createPortlessRoutePreviewKey(projectId: string, sessionId: string, port: number): string {
  return `${projectId}:${sessionId}:${port}`;
}

export function createPortlessRoutePreviewKeyForSession(
  session: Pick<TitlebarResourceSession, "projectId" | "sessionId">,
  port: number,
): string {
  const combinedSession = parseCombinedProjectSessionId(session.sessionId);
  return createPortlessRoutePreviewKey(
    session.projectId ?? combinedSession?.projectId ?? "",
    combinedSession?.sessionId ?? session.sessionId,
    port,
  );
}

export function isPortlessResourceSetupActive(portless: SidebarPortlessState): boolean {
  const health = portless.health;
  return (
    health.enabled === true &&
    health.setupOwnership === "ghostex" &&
    health.setupStatus === "active"
  );
}

export function getTitlebarPortlessResourcesSetupAction(
  portless: SidebarPortlessState,
): NativePortlessAdminInstallAction | undefined {
  const actions: readonly NativePortlessAdminInstallAction[] = ["install", "reconfigure", "retry"];
  return actions.find((action) => portless.nativeAdmin.actions[action]?.available === true);
}

export function getTitlebarPortlessResourcesSetupActionLabel(portless: SidebarPortlessState): string {
  const action = getTitlebarPortlessResourcesSetupAction(portless);
  if (action === "retry") {
    return "Retry";
  }
  if (action === "install" || action === "reconfigure") {
    return "Set up";
  }
  return "Status";
}

export function getTitlebarPortlessResourcesSetupStatusLabel(portless: SidebarPortlessState): string {
  const health = portless.health;
  if (!health.enabled || health.setupStatus === "disabled") {
    return "Portless disabled";
  }
  if (health.setupStatus === "failed") {
    return "Portless setup failed";
  }
  if (health.setupOwnership === "standalone") {
    return "Portless needs reconfigure";
  }
  if (health.setupStatus === "needed" || health.setupOwnership === "missing") {
    return "Portless setup needed";
  }
  return "Portless status";
}

export function resourceServerLabel(server: Pick<ResourceListeningServer, "host" | "port">): string {
  return `${server.host}:${server.port}`;
}

export function isResourcePathInsideOrEqualTo(childPath: string, parentPath: string): boolean {
  const child = normalizeResourceOwnershipPath(childPath);
  const parent = normalizeResourceOwnershipPath(parentPath);
  if (!child || !parent) {
    return false;
  }
  return child === parent || child.startsWith(`${parent}/`);
}

export function normalizeResourceOwnershipPath(path: string): string {
  return path.trim().replace(/\/+$/gu, "");
}

export const EMPTY_RESOURCE_GROUP_VIEWS: ReturnType<typeof createResourceGroupViews> = {
  browserBundles: [],
  codeIdeBundles: [],
  groupViews: [],
  orphanBundles: [],
};

export const EMPTY_RESOURCE_PROCESS_TOTALS: ResourceProcessTotals = {
  cpu: 0,
  memoryMb: 0,
  processCount: 0,
};

export function createResourceItemCollapseTarget(bundle: ResourceProcessBundle): ResourceItemCollapseTarget | undefined {
  if (bundle.childProcesses.length === 0) {
    return undefined;
  }
  const collapsedByDefault = bundle.type === "session" || bundle.type === "browser" || bundle.type === "server";
  return {
    collapsedWhenKeyPresent: !collapsedByDefault,
    key: collapsedByDefault ? `expanded:${bundle.key}` : bundle.key,
  };
}

export function createResourceItemCollapseTargets(bundles: ResourceProcessBundle[]): ResourceItemCollapseTarget[] {
  return bundles
    .map((bundle) => createResourceItemCollapseTarget(bundle))
    .filter((target): target is ResourceItemCollapseTarget => target !== undefined);
}

export function isResourceItemCollapsed(target: ResourceItemCollapseTarget, collapsedKeys: Set<string>): boolean {
  return target.collapsedWhenKeyPresent
    ? collapsedKeys.has(target.key)
    : !collapsedKeys.has(target.key);
}

export function createResourceViewItemCollapseTargets(
  resourceViews: ReturnType<typeof createResourceGroupViews>,
  serverBundles: ResourceProcessBundle[] = [],
): ResourceItemCollapseTarget[] {
  /*
   * CDXC:TitlebarResources 2026-06-11-18:30:
   * Resource project/group sections no longer expose their own collapse controls
   * because per-section headers create a cramped, ambiguous Resources state.
   *
   * CDXC:TitlebarResources 2026-06-12-23:33:
   * The header expand/collapse control beside Sleep Inactive bulk-toggles
   * individual expandable resource rows inside Projects, Browser Tabs, and
   * Orphaned / Detached. It must never collapse those top-level sections.
   *
   * CDXC:TitlebarResources 2026-06-13-02:02:
   * Opening Resources should begin with every expandable row collapsed for that
   * modal instance, not just the user's first-ever Resources visit. Return
   * targets with their default-state polarity so open seeding and button clicks
   * can share the same state transition.
   */
  return createResourceItemCollapseTargets([
    ...serverBundles,
    ...resourceViews.groupViews
      .filter((view) => view.bundles.length > 0)
      .flatMap((view) => view.bundles),
    ...resourceViews.codeIdeBundles,
    ...resourceViews.browserBundles,
    ...resourceViews.orphanBundles,
  ]);
}

export function applyResourceItemCollapsedState(
  current: Set<string>,
  targets: readonly ResourceItemCollapseTarget[],
  collapsed: boolean,
): Set<string> {
  const next = new Set(current);
  let changed = false;
  for (const target of targets) {
    const shouldHaveKey = collapsed === target.collapsedWhenKeyPresent;
    if (shouldHaveKey && !next.has(target.key)) {
      next.add(target.key);
      changed = true;
    } else if (!shouldHaveKey && next.delete(target.key)) {
      changed = true;
    }
  }
  return changed ? next : current;
}

export function isBrowserTabInResourceGroup(
  tab: TitlebarBrowserTabResource,
  group: TitlebarResourceGroup,
): boolean {
  const tabSessionId = browserTabSessionId(tab);
  if (tabSessionId && group.sessions.some((session) => session.sessionId === tabSessionId)) {
    return true;
  }
  const projectId = browserTabProjectId(tab);
  return Boolean(projectId && group.projectId && projectId === group.projectId);
}

export function resourceGroupProjectIdForBrowserTab(
  tab: TitlebarBrowserTabResource,
  group: TitlebarResourceGroup,
): string | undefined {
  const tabSessionId = browserTabSessionId(tab);
  return group.projectId ?? group.sessions.find((session) => session.sessionId === tabSessionId)?.projectId;
}

export function createProcessChildrenMap(processes: ResourceProcess[]): Map<number, ResourceProcess[]> {
  const childrenByParent = new Map<number, ResourceProcess[]>();
  for (const process of processes) {
    const children = childrenByParent.get(process.ppid) ?? [];
    children.push(process);
    childrenByParent.set(process.ppid, children);
  }
  return childrenByParent;
}

export function collectProcessTree(
  seedProcesses: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
): ResourceProcess[] {
  const collected = new Map<number, ResourceProcess>();
  const queue = [...seedProcesses];
  while (queue.length > 0) {
    const process = queue.shift()!;
    if (collected.has(process.pid)) {
      continue;
    }
    collected.set(process.pid, process);
    queue.push(...(childrenByParent.get(process.pid) ?? []));
  }
  return Array.from(collected.values());
}

export function createSessionResourceBundle(
  session: TitlebarResourceSession,
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
): ResourceProcessBundle | undefined {
  const matchTokens = [
    session.sessionPersistenceName,
    session.sessionId,
    session.terminalTitle,
  ]
    .map((token) => token?.trim())
    .filter((token): token is string => Boolean(token && token.length >= 4));
  const seedProcesses = processes.filter((process) =>
    matchTokens.some((token) => process.command.includes(token)),
  );
  if (
    seedProcesses.length === 0 &&
    session.sessionKind !== "browser" &&
    !hasRunningZmxProviderForTitlebarResourceSession(session)
  ) {
    return undefined;
  }
  const tree = collectProcessTree(seedProcesses, childrenByParent);
  tree.forEach((process) => claimedPids.add(process.pid));
  return {
    childProcesses: tree.filter((process) => !seedProcesses.some((seed) => seed.pid === process.pid)),
    cpu: sumProcessCpu(tree),
    key: `session:${session.projectId ?? "active"}:${session.sessionId}`,
    label: session.title,
    memoryMb: sumProcessMemory(tree),
    pids: tree.map((process) => process.pid),
    process: seedProcesses[0],
    session,
    type: "session",
  };
}

export function hasRunningZmxProviderForTitlebarResourceSession(
  session: Pick<
    TitlebarResourceSession,
    "providerSessionState" | "sessionKind" | "sessionPersistenceName" | "sessionPersistenceProvider"
  >,
): boolean {
  /*
   * CDXC:TitlebarResources 2026-06-19-19:21:
   * Resources must list every zmx-backed terminal whose provider is running,
   * even when the macOS pane is not loaded and the sampled process command
   * does not expose the zmx session name. The sidebar labels that state as
   * "Active, not loaded"; Resources should show the same live session row and
   * attach CPU/RAM only when a sampled process tree can be matched.
   */
  return (
    session.sessionKind === "terminal" &&
    session.sessionPersistenceProvider === "zmx" &&
    Boolean(session.sessionPersistenceName?.trim()) &&
    session.providerSessionState === "exists"
  );
}

export function createCodeIdeResourceBundles(
  servers: ResourceListeningServer[],
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
  codeEditorProjectIds: string[],
): ResourceProcessBundle[] {
  /*
   * CDXC:TitlebarResources 2026-06-22-13:50:
   * Embedded Code is one shared code-server runtime, not a project child process.
   * Identify it from Ghostex's fixed localhost editor listener and render it in a Code IDE section so a root "/" project cannot claim it through path substring matching.
   *
   * CDXC:SourceRuntimeOwnership 2026-06-28-04:05:
   * Resources must use the native Source runtime port from bootstrap so legacy
   * dev bundles and GPUI-specific ports are recognized without reverting to the
   * old global 3775 assumption.
   */
  const runtimePort = codeServerResourcePort();
  const server = servers.find(
    (candidate) =>
      candidate.port === runtimePort && candidate.host === "localhost",
  );
  if (!server) {
    return [];
  }
  const processByPid = new Map(processes.map((process) => [process.pid, process]));
  const seedProcess = processByPid.get(server.pid);
  const tree = seedProcess
    ? collectProcessTree([seedProcess], childrenByParent).filter((process) => !claimedPids.has(process.pid))
    : [];
  tree.forEach((process) => claimedPids.add(process.pid));
  claimedPids.add(server.pid);
  const pids = tree.length > 0 ? tree.map((process) => process.pid) : [server.pid];
  return [
    {
      childProcesses: seedProcess
        ? tree.filter((process) => process.pid !== seedProcess.pid)
        : [],
      cpu: sumProcessCpu(tree),
      key: "code:ide",
      label: "Code",
      memoryMb: sumProcessMemory(tree),
      pids,
      process: seedProcess,
      projectEditorIds: Array.from(new Set(codeEditorProjectIds)),
      type: "code",
    },
  ];
}

export function claimAppRuntimeProcesses(
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
): void {
  const appProcesses = processes.filter(
    (process) =>
      !claimedPids.has(process.pid) &&
      /ghostexHost|Ghostex\.app|ghostex/i.test(process.command),
  );
  const appPids = new Set(appProcesses.map((process) => process.pid));
  /**
   * CDXC:TitlebarResources 2026-05-16-19:53:
   * Ghostex-owned app processes need to be claimed as one process tree, not as
   * individual helper matches, so they never leak into detached resource rows.
   *
   * CDXC:TitlebarResources 2026-05-25-16:53:
   * The Resources dropdown should hide Ghostex's own app-runtime rows. Keep
   * matching these processes only to reserve their PIDs before browser and
   * orphan resource sections are built.
   *
   * CDXC:TitlebarResources 2026-05-29-12:02:
   * Ghostex-launched zmx/tmux/zellij and agent roots are user work resources,
   * not app runtime. Do not reserve those roots here; leave them for session or
   * orphan resource tree walking so child processes such as node, npm, Codex,
   * and DevTools helpers stay counted under the Ghostex-owned session root.
   */
  appProcesses
    .filter((process) => !appPids.has(process.ppid) && !isAgentRuntimeProcess(process))
    .slice(0, 3)
    .forEach((process) => {
      const tree = collectProcessTree([process], childrenByParent).filter(
        (treeProcess) =>
          !claimedPids.has(treeProcess.pid) &&
          !isGhostexBrowserProcess(treeProcess) &&
          (treeProcess.pid === process.pid || !isAgentRuntimeProcess(treeProcess)),
      );
      tree.forEach((treeProcess) => claimedPids.add(treeProcess.pid));
    });
}

export function createBrowserBundles(
  browserTabs: TitlebarBrowserTabResource[],
  processes: ResourceProcess[],
  claimedPids: Set<number>,
  options: { includeRuntimeBundles?: boolean } = {},
): ResourceProcessBundle[] {
  /**
   * CDXC:TitlebarResources 2026-05-17-03:09:
   * Browser tab resources must only count Ghostex-owned embedded browser helper
   * processes. System-wide Chromium/Electron helpers from Chrome, VS Code,
   * Codex, Discord, or other apps can share the same `--type=renderer`
   * arguments, so ownership must be proven before a process is allowed into the
   * Browser Tabs section.
   */
  const browserProcesses = processes.filter(
    (process) => !claimedPids.has(process.pid) && isGhostexBrowserProcess(process),
  );
  const bundles: ResourceProcessBundle[] = [];
  for (const tab of browserTabs) {
    const tabProcesses = browserProcesses.filter(
      (process) => browserProcessClientId(process) === String(tab.browserId),
    );
    if (tabProcesses.length === 0) {
      continue;
    }
    tabProcesses.forEach((process) => claimedPids.add(process.pid));
    bundles.push({
      browserTab: tab,
      childProcesses: tabProcesses,
      cpu: sumProcessCpu(tabProcesses),
      key: `browser:${tab.id}`,
      label: tab.title,
      memoryMb: sumProcessMemory(tabProcesses),
      pids: tabProcesses.map((process) => process.pid),
      process: tabProcesses[0],
      type: "browser",
    });
  }
  if (options.includeRuntimeBundles === false) {
    return bundles.slice(0, 16);
  }
  const remainingProcesses = browserProcesses.filter((process) => !claimedPids.has(process.pid));
  const unmatchedRendererProcesses = remainingProcesses.filter((process) => browserProcessClientId(process));
  if (unmatchedRendererProcesses.length > 0) {
    unmatchedRendererProcesses.forEach((process) => claimedPids.add(process.pid));
    bundles.push({
      childProcesses: unmatchedRendererProcesses.slice(0, 12),
      cpu: sumProcessCpu(unmatchedRendererProcesses),
      key: "browser:unmatched-renderers",
      label: "Unmatched browser renderers",
      memoryMb: sumProcessMemory(unmatchedRendererProcesses),
      pids: unmatchedRendererProcesses.map((process) => process.pid),
      process: unmatchedRendererProcesses[0],
      type: "browser",
    });
  }
  const runtimeProcesses = remainingProcesses.filter((process) => !claimedPids.has(process.pid));
  if (runtimeProcesses.length > 0) {
    runtimeProcesses.forEach((process) => claimedPids.add(process.pid));
    bundles.push({
      childProcesses: runtimeProcesses.slice(0, 12),
      cpu: sumProcessCpu(runtimeProcesses),
      key: "browser:runtime",
      label: "Browser runtime",
      memoryMb: sumProcessMemory(runtimeProcesses),
      pids: runtimeProcesses.map((process) => process.pid),
      process: runtimeProcesses[0],
      type: "browser",
    });
  }
  return bundles.slice(0, 16);
}

export function createOrphanBundles(
  processes: ResourceProcess[],
  childrenByParent: Map<number, ResourceProcess[]>,
  claimedPids: Set<number>,
): ResourceProcessBundle[] {
  const ownedSeedProcesses = processes.filter(
    (process) =>
      !claimedPids.has(process.pid) &&
      isGhostexOwnedResourceProcess(process) &&
      isAgentRuntimeProcess(process),
  );
  const ownedSeedPids = new Set(ownedSeedProcesses.map((process) => process.pid));
  return ownedSeedProcesses
    .filter((process) => !ownedSeedPids.has(process.ppid))
    .slice(0, 16)
    .map((process) => {
      const tree = collectProcessTree([process], childrenByParent).filter(
        (treeProcess) => !claimedPids.has(treeProcess.pid),
      );
      tree.forEach((treeProcess) => claimedPids.add(treeProcess.pid));
      return {
        childProcesses: tree.filter((treeProcess) => treeProcess.pid !== process.pid),
        cpu: sumProcessCpu(tree),
        key: `orphan:${process.pid}`,
        label: getProcessDisplayName(process),
        memoryMb: sumProcessMemory(tree),
        pids: tree.map((treeProcess) => treeProcess.pid),
        process,
        type: "orphan" as const,
      };
    });
}

export function isGhostexOwnedResourceProcess(process: ResourceProcess): boolean {
  const command = process.command;
  /**
   * CDXC:TitlebarResources 2026-05-28-21:04:
   * Orphaned / Detached resources are still part of the app's CPU/RAM total, so
   * command-name matches are not enough. Only include ungrouped agent-looking
   * root processes when their command proves Ghostex ownership, then walk only
   * their descendants. External Codex, DevTools, Chrome extension, and
   * computer-use helpers from other terminals must stay out of the Resources
   * dropdown and app resource calculation.
   */
  return (
    /\/(?:Applications\/)?Ghostex(?:-dev)?\.app\b/i.test(command) ||
    /\bghostexHost\b/i.test(command) ||
    /\/\.ghostex(?:-dev)?\//i.test(command) ||
    /\bGHOSTEX_[A-Z0-9_]+=/.test(command) ||
    /\/Resources\/Web\/bin\/zmx\b/.test(command)
  );
}

export function isGhostexBrowserProcess(process: ResourceProcess): boolean {
  const command = process.command;
  const isBrowserHelper = /Chromium Embedded Framework|--type=(renderer|gpu-process|utility)\b/.test(command);
  if (!isBrowserHelper) {
    return false;
  }
  return (
    /\/Contents\/Frameworks\/[^/\s]*ghostex[^/\s]* Helper/i.test(command) ||
    /--main-bundle-path=\S*\/ghostex(?:-dev)?\.app\b/i.test(command) ||
    /--user-data-dir=\S*\/\.ghostex\/cef\b/.test(command)
  );
}

export function isAgentRuntimeProcess(process: ResourceProcess): boolean {
  return /\b(zmx|codex|code-server|computer-use|chrome-devtools-mcp|devtools)\b/i.test(process.command);
}

export function browserProcessClientId(process: ResourceProcess): string | undefined {
  return /--(?:renderer-)?client-id=(\d+)/.exec(process.command)?.[1];
}

export function browserTabSessionId(tab: TitlebarBrowserTabResource): string | undefined {
  if (tab.sessionId?.trim()) {
    return tab.sessionId.trim();
  }
  const match = /^browser:(?<sessionId>.+)$/u.exec(tab.id);
  return match?.groups?.sessionId;
}

export function browserTabProjectId(tab: TitlebarBrowserTabResource): string | undefined {
  if (tab.projectId?.trim()) {
    return tab.projectId.trim();
  }
  const match = /^project-editor:(?<projectId>.+):[^:]+$/u.exec(tab.id);
  if (!match?.groups?.projectId) {
    return undefined;
  }
  try {
    return decodeURIComponent(match.groups.projectId);
  } catch {
    return undefined;
  }
}

export function getBrowserProcessDisplayName(process: ResourceProcess): string {
  const clientId = browserProcessClientId(process);
  if (clientId) {
    return `Browser renderer client ${clientId}`;
  }
  if (process.command.includes("--type=gpu-process")) {
    return "Browser GPU";
  }
  if (process.command.includes("--type=utility")) {
    return getBrowserUtilityProcessDisplayName(process);
  }
  return "Browser renderer";
}

export function getBrowserUtilityProcessDisplayName(process: ResourceProcess): string {
  const subtype = /--utility-sub-type=([^\s]+)/.exec(process.command)?.[1];
  if (subtype?.includes("NetworkService")) {
    return "Browser network service";
  }
  if (subtype?.includes("StorageService")) {
    return "Browser storage service";
  }
  if (subtype?.includes("AudioService")) {
    return "Browser audio service";
  }
  if (subtype?.includes("VideoCaptureService")) {
    return "Browser video capture service";
  }
  return "Browser utility";
}

export function getProcessDisplayName(process: ResourceProcess): string {
  const command = process.command.split(/\s+/)[0] ?? "Process";
  return command.split("/").pop() || command;
}

export function sumProcessCpu(processes: ResourceProcess[]): number {
  return processes.reduce((sum, process) => sum + process.cpu, 0);
}

export function sumProcessMemory(processes: ResourceProcess[]): number {
  return processes.reduce((sum, process) => sum + process.rssMb, 0);
}

export function sumBundleCpu(bundles: ResourceProcessBundle[]): number {
  return bundles.reduce((sum, bundle) => sum + bundle.cpu, 0);
}

export function sumBundleMemory(bundles: ResourceProcessBundle[]): number {
  return bundles.reduce((sum, bundle) => sum + bundle.memoryMb, 0);
}

export function createInactiveTerminalSleepSessionIds(resourceGroups: TitlebarResourceGroup[]): string[] {
  /**
   * CDXC:TitlebarResources 2026-05-16-19:53:
   * The dropdown sleep shortcut is intentionally conservative: only awake,
   * idle agent terminal sessions older than seven minutes are eligible. Working
   * and attention sessions must stay awake because those states indicate active
   * output or a user-visible response waiting for review.
   *
   * CDXC:TitlebarResources 2026-05-26-17:16:
   * Sleep Inactive should sleep every awake idle terminal represented in the
   * Resources dropdown, not only old agent-detected rows. Keep working,
   * attention, and already sleeping sessions awake, but do not require agent
   * metadata or a seven-minute age gate.
   *
   * CDXC:TitlebarResources 2026-06-06-06:09:
   * Delayed Send means a terminal has a staged Enter that must fire while the
   * pane is awake. Exclude delayed-send sessions from the Resources sleep count
   * and payload so macOS and Electron do not hide pending sends behind sleep.
   */
  return resourceGroups.flatMap((group) =>
    group.sessions
      .filter((session) => {
        return !(
          session.sessionKind !== "terminal" ||
          session.isSleeping === true ||
          session.activity === "working" ||
          session.activity === "attention" ||
          hasTitlebarResourceDelayedSend(session)
        );
      })
      .map(titlebarResourceSidebarSessionId),
  );
}

export function hasTitlebarResourceDelayedSend(
  session: Pick<
    TitlebarResourceSession,
    "delayedSendDeadlineAt" | "delayedSendRemainingLabel" | "delayedSendRemainingMs"
  >,
): boolean {
  return Boolean(
    session.delayedSendRemainingLabel ||
      session.delayedSendDeadlineAt ||
      typeof session.delayedSendRemainingMs === "number",
  );
}

export function titlebarResourceSidebarSessionId(
  session: Pick<TitlebarResourceSession, "projectId" | "sessionId">,
): string {
  /*
   * CDXC:TitlebarResources 2026-06-15-15:27:
   * gxserver presentation-backed Resources rows already arrive with combined
   * project/session ids. Focus, Sleep, and Close must forward that id unchanged
   * instead of wrapping it again, or the sidebar resolves a synthetic session
   * id and the visible row action does nothing.
   */
  if (parseCombinedProjectSessionId(session.sessionId)) {
    return session.sessionId;
  }
  return session.projectId
    ? createCombinedProjectSessionId(session.projectId, session.sessionId)
    : session.sessionId;
}

export function uniqueResourceBundles(bundles: ResourceProcessBundle[]): ResourceProcessBundle[] {
  const seen = new Set<string>();
  return bundles.filter((bundle) => {
    if (seen.has(bundle.key)) {
      return false;
    }
    seen.add(bundle.key);
    return true;
  });
}

export function isResourceBundleActionable(bundle: ResourceProcessBundle): boolean {
  /**
   * CDXC:TitlebarResources 2026-06-15-13:45:
   * Resources must not expose Close for shared Chromium runtime bundles because killing GPU, network, storage, or unmatched renderer helpers can leave the app's embedded browser surfaces broken. Only user-owned browser tabs get resource Close controls; diagnostic browser helper rows stay visible for CPU/RAM accounting.
   */
  return !(bundle.type === "browser" && !bundle.browserTab);
}

export function resourceBundleSidebarSessionIds(bundle: ResourceProcessBundle): string[] {
  if (bundle.type === "server") {
    return [];
  }
  const session = bundle.session;
  if (session) {
    return [titlebarResourceSidebarSessionId(session)];
  }
  const browserSessionId = bundle.browserTab ? browserTabSessionId(bundle.browserTab) : undefined;
  if (!browserSessionId) {
    return [];
  }
  return [
    bundle.browserTab?.projectId
      ? createCombinedProjectSessionId(bundle.browserTab.projectId, browserSessionId)
      : browserSessionId,
  ];
}

export function resourceBundleFocusSessionId(bundle: ResourceProcessBundle): string | undefined {
  const session = bundle.session;
  if (session) {
    return titlebarResourceSidebarSessionId(session);
  }
  return resourceBundleSidebarSessionIds(bundle)[0];
}

export function resourceBundleProjectEditorIds(bundle: ResourceProcessBundle): string[] {
  if (bundle.projectEditorIds) {
    return bundle.projectEditorIds;
  }
  if (bundle.type === "code") {
    const match = /^code:(?<groupId>.+)$/u.exec(bundle.key);
    const projectId = match?.groups?.groupId ? parseCombinedProjectGroupId(match.groups.groupId) : undefined;
    return projectId ? [projectId] : [];
  }
  const projectId = bundle.browserTab ? browserTabProjectId(bundle.browserTab) : undefined;
  return projectId ? [projectId] : [];
}

export function sortResourceBundlesForDisplay(
  bundles: ResourceProcessBundle[],
  quittingKeys: Set<string>,
): ResourceProcessBundle[] {
  return [...bundles].sort((left, right) => {
    const leftQuitting = quittingKeys.has(left.key);
    const rightQuitting = quittingKeys.has(right.key);
    return leftQuitting === rightQuitting ? 0 : leftQuitting ? 1 : -1;
  });
}

export function formatWholePercent(value: number): string {
  return `${Math.trunc(Math.max(0, value))}%`;
}

export function formatResourceMemory(value: number): string {
  /*
   * CDXC:TitlebarResources 2026-06-30-23:17:
   * Resource memory labels must not floor GB values because that made near-2 GB totals render as 1 GB and hid real app pressure.
   * Round GB values to one decimal while keeping whole-MB labels for smaller processes.
   */
  const safeValue = Math.max(0, value);
  if (safeValue >= 1024) {
    const roundedGb = Math.round((safeValue / 1024) * 10) / 10;
    return `${Number.isInteger(roundedGb) ? roundedGb.toFixed(0) : roundedGb.toFixed(1)} GB`;
  }
  return `${Math.round(safeValue)} MB`;
}
