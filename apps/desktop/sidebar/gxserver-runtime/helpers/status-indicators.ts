/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_ACTIVE_WORKSPACE_TAB_SESSION_TITLE_MAX_CHARS,
  GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_TYPE,
  GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_VERSION,
  GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_TYPE,
  GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_VERSION,
  GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE,
  GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION,
  GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE,
  GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION,
  GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_TYPE,
  GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_VERSION,
  GPUI_STATUS_INDICATOR_ID_MAX_CHARS,
  GPUI_STATUS_INDICATOR_MAX_CANDIDATES,
  GPUI_STATUS_INDICATOR_MAX_PROJECTS,
  GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT,
  GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS,
} from "../constants";
import type {
  GpuiMenuBarProjectActivationPayload,
  GpuiMenuBarSessionActivationPayload,
  GpuiPetOverlayStatePayload,
  GpuiSessionStatusIndicatorCandidate,
  GpuiSessionStatusIndicatorProject,
  GpuiSessionStatusIndicatorStatus,
  GpuiSessionStatusIndicatorsPayload,
  GpuiStatusPetActivationPayload,
} from "../types-and-protocol";
import { normalizeNonEmptyString } from "./records";
import {
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from "./remote-presentation";
import { createDisplaySessionLayout } from "@/packages/shared/active-sessions-sort";
import type { ghostexSettings } from "@/packages/shared/ghostex-settings";
import {
  createGxserverPresentationProjectSessionId,
  parseGxserverPresentationProjectSessionId,
} from "@/packages/shared/gxserver-presentation-sidebar-projection";
import type {
  SidebarSessionGroup,
  SidebarSessionItem,
} from "@/packages/shared/session-grid-contract";
import { DEFAULT_TERMINAL_SESSION_TITLE } from "@/packages/shared/session-grid-contract";
import {
  normalizeWorkspaceProjectIconDataUrl,
} from "@/packages/shared/workspace-project-appearance";

export function createGpuiSessionStatusIndicatorCandidatesFromSidebarGroups(
  groups: readonly SidebarSessionGroup[],
): GpuiSessionStatusIndicatorCandidate[] {
  /*
  CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
  GPUI derives status/pet candidates from the live gxserver SidebarApp groups because the GPUI sidebar entry mounts SidebarApp directly and never runs native-sidebar.tsx. Preserve the same project/session order semantics as macOS by reusing shared display layout, but keep the bridge payload bounded and route with ids only rather than paths, commands, terminal text, external URLs, or daemon bodies. Project icon parity may carry only an already-normalized image data URL for notification attachments.
  */
  const candidates: GpuiSessionStatusIndicatorCandidate[] = [];
  let order = 0;
  for (const group of groups) {
    if (candidates.length >= GPUI_STATUS_INDICATOR_MAX_CANDIDATES) {
      break;
    }
    const groupProjectId = group.projectContext?.editor.projectId;
    const groupIconDataUrl = normalizeWorkspaceProjectIconDataUrl(
      group.projectContext?.iconDataUrl,
    );
    const sessionsById = Object.fromEntries(
      group.sessions.map((session) => [session.sessionId, session]),
    );
    const manualSessionIds = group.sessions.map((session) => session.sessionId);
    const displayLayout = createDisplaySessionLayout({
      sessionIdsByGroup: { [group.groupId]: manualSessionIds },
      sessionsById,
      sortMode: "lastActivity",
      workspaceGroupIds: [group.groupId],
    });
    const visualSessionIds = displayLayout.sessionIdsByGroup[group.groupId] ?? manualSessionIds;
    for (const sessionId of visualSessionIds) {
      if (candidates.length >= GPUI_STATUS_INDICATOR_MAX_CANDIDATES) {
        break;
      }
      const session = sessionsById[sessionId];
      if (!session) {
        continue;
      }
      const combinedReference = parseGxserverPresentationProjectSessionId(session.sessionId);
      const candidateProjectId = groupProjectId ?? combinedReference?.projectId;
      if (!candidateProjectId) {
        continue;
      }
      candidates.push({
        hasRunningZmxBacking: hasRunningZmxBackingForGpuiIdleIndicator(session),
        ...(groupIconDataUrl ? { iconDataUrl: groupIconDataUrl } : {}),
        lastInteractionAt: session.lastInteractionAt,
        order,
        projectId: candidateProjectId,
        projectTitle: boundedGpuiStatusIndicatorTitle(
          group.title || candidateProjectId,
          candidateProjectId,
        ),
        sessionId: session.sessionId,
        status: getGpuiSessionStatusIndicatorStatus(session),
        title: getGpuiPetOverlaySessionTitle(session),
      });
      order += 1;
    }
  }
  return candidates;
}

export function createGpuiSessionStatusIndicatorsPayload(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
  settings: ghostexSettings,
): GpuiSessionStatusIndicatorsPayload {
  const counts = countGpuiSessionStatusIndicatorCandidates(candidates);
  return {
    attentionCount: counts.attention,
    availableCount: counts.available,
    hideMenuBarIndicators: settings.hideMenuBarSessionStatusIndicators,
    projects: createGpuiSessionStatusIndicatorProjects(candidates),
    type: GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE,
    version: GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION,
    workingCount: counts.working,
  };
}

export function createGpuiPetOverlayStatePayload(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
  settings: ghostexSettings,
): GpuiPetOverlayStatePayload {
  const actionableActivityCandidates = candidates.filter(
    (candidate) => candidate.status === "attention" || candidate.status === "working",
  );
  const shownActivityCandidates =
    actionableActivityCandidates.length > 0
      ? [...actionableActivityCandidates].sort(compareGpuiPetOverlayActivityCandidates).slice(0, 3)
      : [...candidates].sort(compareGpuiSessionStatusIndicatorCandidates).slice(0, 2);
  return {
    activities: shownActivityCandidates.map((candidate) => ({
      id: candidate.sessionId,
      projectId: candidate.projectId,
      state: candidate.status,
      title: candidate.title,
    })),
    enabled: settings.petOverlayEnabled,
    selectedPetId: boundedGpuiStatusIndicatorTitle(settings.selectedPetId, "cat"),
    statusItems: createGpuiPetOverlayStatusItems(candidates),
    type: GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE,
    version: GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION,
  };
}

export function createGpuiSessionStatusIndicatorProjects(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
): GpuiSessionStatusIndicatorProject[] {
  const projects: GpuiSessionStatusIndicatorProject[] = [];
  const projectsById = new Map<string, GpuiSessionStatusIndicatorProject>();
  for (const candidate of candidates) {
    if (!shouldCountGpuiSessionStatusIndicatorCandidate(candidate)) {
      continue;
    }
    let project = projectsById.get(candidate.projectId);
    if (!project) {
      if (projects.length >= GPUI_STATUS_INDICATOR_MAX_PROJECTS) {
        continue;
      }
      project = {
        ...(candidate.iconDataUrl ? { iconDataUrl: candidate.iconDataUrl } : {}),
        projectId: candidate.projectId,
        sessions: [],
        title: candidate.projectTitle,
      };
      projectsById.set(candidate.projectId, project);
      projects.push(project);
    }
    if (project.sessions.length >= GPUI_STATUS_INDICATOR_MAX_SESSIONS_PER_PROJECT) {
      continue;
    }
    project.sessions.push({
      lastActiveAt: candidate.lastInteractionAt,
      sessionId: candidate.sessionId,
      sidebarOrder: candidate.order,
      status: candidate.status,
      title: candidate.title,
    });
  }
  return projects;
}

export function countGpuiSessionStatusIndicatorCandidates(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
): Record<GpuiSessionStatusIndicatorStatus, number> {
  const counts = {
    attention: 0,
    available: 0,
    working: 0,
  };
  for (const candidate of candidates) {
    if (shouldCountGpuiSessionStatusIndicatorCandidate(candidate)) {
      counts[candidate.status] += 1;
    }
  }
  return counts;
}

export function createGpuiPetOverlayStatusItems(
  candidates: readonly GpuiSessionStatusIndicatorCandidate[],
): Array<{ count: number; status: GpuiSessionStatusIndicatorStatus }> {
  const counts = countGpuiSessionStatusIndicatorCandidates(candidates);
  if (counts.attention > 0 || counts.working > 0) {
    const items: Array<{ count: number; status: GpuiSessionStatusIndicatorStatus }> = [];
    if (counts.attention > 0) {
      items.push({ count: counts.attention, status: "attention" });
    }
    if (counts.working > 0) {
      items.push({ count: counts.working, status: "working" });
    }
    return items;
  }
  return counts.available > 0 ? [{ count: counts.available, status: "available" }] : [];
}

export function getGpuiSessionStatusIndicatorStatus(
  session: SidebarSessionItem,
): GpuiSessionStatusIndicatorStatus {
  if (session.activity === "attention") {
    return "attention";
  }
  if (session.activity === "working") {
    return "working";
  }
  return "available";
}

export function hasRunningZmxBackingForGpuiIdleIndicator(session: SidebarSessionItem): boolean {
  if (session.sessionKind !== "terminal") {
    return false;
  }
  if (
    session.sessionPersistenceProvider !== "zmx" ||
    !normalizeNonEmptyString(session.sessionPersistenceName)
  ) {
    return false;
  }
  return (
    session.providerSessionState === "exists" ||
    session.nativePaneState === "mounted" ||
    session.nativePaneState === "mounting" ||
    session.isLive === true
  );
}

export function shouldCountGpuiSessionStatusIndicatorCandidate(
  candidate: GpuiSessionStatusIndicatorCandidate,
): boolean {
  return candidate.status !== "available" || candidate.hasRunningZmxBacking;
}

export function compareGpuiSessionStatusIndicatorCandidates(
  left: GpuiSessionStatusIndicatorCandidate,
  right: GpuiSessionStatusIndicatorCandidate,
): number {
  const timeDelta =
    getGpuiIndicatorTimestamp(right.lastInteractionAt) -
    getGpuiIndicatorTimestamp(left.lastInteractionAt);
  if (timeDelta !== 0) {
    return timeDelta;
  }
  return left.order - right.order;
}

export function compareGpuiPetOverlayActivityCandidates(
  left: GpuiSessionStatusIndicatorCandidate,
  right: GpuiSessionStatusIndicatorCandidate,
): number {
  const statusDelta =
    getGpuiPetOverlayActivityStatusPriority(right.status) -
    getGpuiPetOverlayActivityStatusPriority(left.status);
  if (statusDelta !== 0) {
    return statusDelta;
  }
  return left.order - right.order;
}

export function getGpuiPetOverlayActivityStatusPriority(status: GpuiSessionStatusIndicatorStatus): number {
  switch (status) {
    case "attention":
      return 2;
    case "working":
      return 1;
    case "available":
      return 0;
  }
}

export function getGpuiIndicatorTimestamp(value: string | undefined): number {
  if (!value) {
    return 0;
  }
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) ? timestamp : 0;
}

export function getGpuiPetOverlaySessionTitle(session: SidebarSessionItem): string {
  const title =
    session.displayTitle?.trim() ||
    session.primaryTitle?.trim() ||
    session.terminalTitle?.trim() ||
    session.alias.trim() ||
    session.sessionNumber?.trim();
  return boundedGpuiStatusIndicatorTitle(title, "Untitled session");
}

export function boundedGpuiStatusIndicatorTitle(value: string | undefined, fallback: string): string {
  const normalized = normalizeNonEmptyString(value) ?? fallback;
  return normalized.length > GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS
    ? normalized.slice(0, GPUI_STATUS_INDICATOR_TITLE_MAX_CHARS)
    : normalized;
}

export function boundedGpuiActiveWorkspaceTabSessionTitle(value: string): string {
  const normalized = normalizeNonEmptyString(value) ?? DEFAULT_TERMINAL_SESSION_TITLE;
  return normalized.length > GPUI_ACTIVE_WORKSPACE_TAB_SESSION_TITLE_MAX_CHARS
    ? normalized.slice(0, GPUI_ACTIVE_WORKSPACE_TAB_SESSION_TITLE_MAX_CHARS)
    : normalized;
}

export function normalizeGpuiStatusPetActivation(
  value: unknown,
): GpuiStatusPetActivationPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !["sessionId", "type", "version"].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_STATUS_PET_ACTIVATION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (!sessionId || !gpuiStatusPetActivationSessionIdAllowed(sessionId)) {
    return undefined;
  }
  return { sessionId };
}

export function normalizeGpuiMenuBarProjectActivation(
  value: unknown,
): GpuiMenuBarProjectActivationPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some((key) => !["projectId", "type", "version"].includes(key))) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_MENU_BAR_PROJECT_ACTIVATION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  if (!projectId || !gpuiStatusPetActivationSessionIdAllowed(projectId)) {
    return undefined;
  }
  return { projectId };
}

export function normalizeGpuiMenuBarSessionActivation(
  value: unknown,
): GpuiMenuBarSessionActivationPayload | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !["projectId", "sessionId", "type", "version"].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_MENU_BAR_SESSION_ACTIVATION_MESSAGE_VERSION
  ) {
    return undefined;
  }
  const projectId = normalizeNonEmptyString(record.projectId)?.trim();
  const sessionId = normalizeNonEmptyString(record.sessionId)?.trim();
  if (
    !projectId ||
    !sessionId ||
    !gpuiStatusPetActivationSessionIdAllowed(projectId) ||
    !gpuiStatusPetActivationSessionIdAllowed(sessionId)
  ) {
    return undefined;
  }
  return { projectId, sessionId };
}

export function gpuiMenuBarStatusSessionFocusRoutingId(projectId: string, sessionId: string): string {
  if (
    parseGpuiRemotePresentationSessionId(sessionId) ||
    parseGxserverPresentationProjectSessionId(sessionId)
  ) {
    return sessionId;
  }
  const remoteProject = parseGpuiRemotePresentationProjectId(projectId);
  if (remoteProject) {
    return createGpuiRemotePresentationSessionId(
      remoteProject.machineId,
      remoteProject.projectId,
      sessionId,
    );
  }
  return createGxserverPresentationProjectSessionId(projectId, sessionId);
}

export function gpuiStatusPetActivationSessionIdAllowed(value: string): boolean {
  return (
    value.length <= GPUI_STATUS_INDICATOR_ID_MAX_CHARS &&
    !value.includes("/") &&
    !value.includes("\\") &&
    !/[\u0000-\u001f\u007f]/u.test(value)
  );
}