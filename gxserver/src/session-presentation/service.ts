import type {
  GxserverProjectDomainState,
  GxserverProjectId,
  GxserverSessionDomainState,
  GxserverSessionId,
  GxserverSessionStateEventParams,
  GxserverSessionStateEventResult,
  GxserverUpdateSessionParams,
} from "../../protocol/index.js";
import { projectSessionTitle } from "../session-title/projection.js";
import { getTrustedResumeTitle } from "../session-title/trust.js";
import {
  normalizeAgentId,
  normalizeCodexSessionId,
  normalizeText,
  resolveSessionIdentity,
  type GxserverResolvedSessionIdentity,
} from "./identity.js";
import { resolveSessionLaunchAgentMismatch } from "./launch-identity.js";
import { selectTrustedTitleForIdentity } from "./title-candidates.js";

export interface GxserverSessionPresentationRepository {
  getProject(projectId: GxserverSessionStateEventParams["projectId"]): GxserverProjectDomainState | undefined;
  getSession(
    projectId: GxserverSessionStateEventParams["projectId"],
    sessionId: GxserverSessionStateEventParams["sessionId"],
  ): GxserverSessionDomainState | undefined;
  listSessions(projectId?: GxserverSessionStateEventParams["projectId"]): GxserverSessionDomainState[];
  updateSession(input: GxserverUpdateSessionParams): GxserverSessionDomainState;
}

export type GxserverSessionIdentityUpdateSource = "lifecycle" | "live-process" | "passive" | "terminal-title";

export interface GxserverSessionIdentityConflict {
  agentId: string;
  currentAgentSessionId?: string;
  incomingAgentSessionId: string;
  ownerProjectId?: GxserverProjectId;
  ownerSessionId?: GxserverSessionId;
  reason: "active-agent-session-id-owned" | "passive-agent-session-id-replacement";
  source: GxserverSessionIdentityUpdateSource;
}

export type GxserverApplySessionStateEventParams = GxserverSessionStateEventParams & {
  identityUpdateSource?: GxserverSessionIdentityUpdateSource;
  onIdentityConflict?: (conflict: GxserverSessionIdentityConflict) => void;
};

export type GxserverSessionStateEventApplyResult = GxserverSessionStateEventResult & {
  identityConflict?: GxserverSessionIdentityConflict;
};

export function applySessionStateEvent(
  repository: GxserverSessionPresentationRepository,
  params: GxserverApplySessionStateEventParams,
): GxserverSessionStateEventApplyResult {
  const session = repository.getSession(params.projectId, params.sessionId);
  if (!session) {
    throw new Error(`Session ${params.projectId}/${params.sessionId} does not exist.`);
  }
  const project = repository.getProject(params.projectId);
  if (!project) {
    throw new Error(`Project ${params.projectId} does not exist.`);
  }

  const projectSessions = repository.listSessions(params.projectId);
  const observedIdentity = resolveSessionIdentity({
    agentName: params.agentName,
    agentSessionId: params.agentSessionId,
    agentSessionPath: params.agentSessionPath,
    startupText: params.startupText,
  });
  const identityUpdateSource = params.identityUpdateSource ?? "passive";
  const launchAgentMismatch =
    identityUpdateSource === "live-process"
      ? undefined
      : resolveSessionLaunchAgentMismatch(session, observedIdentity.agentId);
  if (launchAgentMismatch) {
    /*
    CDXC:GxserverSessionIdentity 2026-06-09-21:59:
    Passive identity repair is only valid when the session was not launched by gxserver as a different agent. Preserve the existing row when a global hook reports another CLI so every renderer sees the launch-owned identity instead of a cross-wired hook result.
    */
    return {
      changed: false,
      projection: projectSessionTitle(session),
      reason: "launch-agent-mismatch",
      session,
    };
  }
  const currentIdentity = resolveStoredSessionIdentity(session);
  const resolvedIdentity = mergeObservedSessionIdentity(observedIdentity, currentIdentity);
  let identityConflict: GxserverSessionIdentityConflict | undefined;
  const identity = resolveAllowedSessionIdentity({
    currentIdentity,
    currentSession: session,
    observedIdentity,
    onIdentityConflict: (conflict) => {
      identityConflict = conflict;
      params.onIdentityConflict?.(conflict);
    },
    resolvedIdentity,
    sessions: projectSessions,
    source: identityUpdateSource,
  });
  if (identityConflict && identityUpdateSource === "passive") {
    /*
    CDXC:GxserverSessionIdentity 2026-06-09-22:30:
    A passive Codex hook or state-file replay that reports a different thread is not only an identity no-op; its activity, title, and first-prompt payload belong to another terminal row. Reject the whole observation before callers can turn the clicked session green or play completion audio.
    */
    return {
      changed: false,
      identityConflict,
      projection: projectSessionTitle(session),
      reason: "passive-session-identity-conflict",
      session,
    };
  }
  const nextAgentId = identity.agentId ?? session.agentId;
  let runtimeSettings = applySessionIdentityRuntimeSettings({
    currentIdentity,
    identity,
    runtimeSettings: session.runtimeSettings,
    source: identityUpdateSource,
  });
  runtimeSettings = {
    ...runtimeSettings,
    ...("firstPromptTitleGenerationAgent" in params && params.firstPromptTitleGenerationAgent
      ? { firstPromptTitleGenerationAgent: params.firstPromptTitleGenerationAgent }
      : {}),
    ...("firstPromptTitleGenerationCommand" in params
      ? { firstPromptTitleGenerationCommand: params.firstPromptTitleGenerationCommand }
      : {}),
    ...(params.firstUserMessage ? { firstUserMessage: params.firstUserMessage } : {}),
  };
  const shouldPromoteAgentKind = Boolean(nextAgentId || identity.agentSessionId || identity.agentSessionPath);
  let title = session.title;
  let reason = "identity-updated";
  const currentWithIdentity: GxserverSessionDomainState = {
    ...session,
    ...(nextAgentId ? { agentId: nextAgentId } : {}),
    kind: shouldPromoteAgentKind ? "agent" : session.kind,
    runtimeSettings,
  };

  if (getTrustedResumeTitle(currentWithIdentity).title === undefined) {
    const candidate = selectTrustedTitleForIdentity({
      currentSession: currentWithIdentity,
      eventTitle: params.title,
      eventTitleSource: params.titleSource,
      identity,
      project,
      sessions: projectSessions,
    });
    if (candidate) {
      title = candidate.title;
      runtimeSettings = {
        ...runtimeSettings,
        titleSource: candidate.titleSource,
      };
      reason = candidate.reason;
    }
  } else {
    reason = "current-title-already-trusted";
  }

  const needsUpdate =
    title !== session.title ||
    nextAgentId !== session.agentId ||
    (shouldPromoteAgentKind && session.kind !== "agent") ||
    runtimeSettings.agentName !== session.runtimeSettings.agentName ||
    runtimeSettings.agentId !== session.runtimeSettings.agentId ||
    runtimeSettings.agentSessionId !== session.runtimeSettings.agentSessionId ||
    runtimeSettings.agentSessionPath !== session.runtimeSettings.agentSessionPath ||
    runtimeSettings.launchAgentId !== session.runtimeSettings.launchAgentId ||
    runtimeSettings.firstPromptTitleGenerationAgent !==
      session.runtimeSettings.firstPromptTitleGenerationAgent ||
    runtimeSettings.firstPromptTitleGenerationCommand !==
      session.runtimeSettings.firstPromptTitleGenerationCommand ||
    runtimeSettings.firstUserMessage !== session.runtimeSettings.firstUserMessage ||
    runtimeSettings.agentActivity !== session.runtimeSettings.agentActivity ||
    runtimeSettings.titleSource !== session.runtimeSettings.titleSource;

  if (!needsUpdate) {
    return {
      changed: false,
      ...(identityConflict ? { identityConflict } : {}),
      projection: projectSessionTitle(currentWithIdentity),
      reason: "unchanged",
      session: currentWithIdentity,
    };
  }

  /*
  CDXC:GxserverSessionPresentation 2026-05-31-21:10:
  macOS main previously coupled hook-captured agent identity, trusted title provenance, previous-session history, and restore protection in the sidebar. gxserver now performs that reduction so every client sees the same Codex/Claude/Cursor/OpenCode/Pi identity and title instead of rendering placeholder `Terminal Session` rows after resume.
  */
  const updated = repository.updateSession({
    ...(nextAgentId ? { agentId: nextAgentId } : {}),
    kind: shouldPromoteAgentKind ? "agent" : session.kind,
    projectId: params.projectId,
    runtimeSettings,
    sessionId: params.sessionId,
    title,
  });
  return {
    changed: true,
    ...(identityConflict ? { identityConflict } : {}),
    projection: projectSessionTitle(updated),
    reason,
    session: updated,
  };
}

function resolveAllowedSessionIdentity(input: {
  currentIdentity: GxserverResolvedSessionIdentity;
  currentSession: GxserverSessionDomainState;
  observedIdentity: GxserverResolvedSessionIdentity;
  onIdentityConflict?: (conflict: GxserverSessionIdentityConflict) => void;
  resolvedIdentity: GxserverResolvedSessionIdentity;
  sessions: readonly GxserverSessionDomainState[];
  source: GxserverSessionIdentityUpdateSource;
}): GxserverResolvedSessionIdentity {
  const observedAgentId = normalizeAgentId(input.observedIdentity.agentId);
  const currentAgentId = normalizeAgentId(input.currentIdentity.agentId);
  const resolvedAgentId = normalizeAgentId(input.resolvedIdentity.agentId);
  const incomingAgentSessionId = normalizeCodexSessionId(input.observedIdentity.agentSessionId);
  const currentCodexSessionId =
    currentAgentId === "codex" ? normalizeCodexSessionId(input.currentIdentity.agentSessionId) : undefined;
  const isPassiveCodexObservation =
    input.source === "passive" &&
    incomingAgentSessionId !== undefined &&
    (observedAgentId === "codex" || (!observedAgentId && currentAgentId === "codex" && resolvedAgentId === "codex"));
  if (!isPassiveCodexObservation) {
    return input.resolvedIdentity;
  }
  /*
  CDXC:GxserverSessionIdentity 2026-06-12-02:44:
  Passive Codex hooks may correct stale non-Codex identity on the same surface. Protect only a current Codex-owned thread id from passive replacement; Claude and other agent ids are also UUID-shaped, and treating them as Codex ids can pin a newer Codex session to the wrong agent for every client.
  */
  if (currentCodexSessionId && currentCodexSessionId !== incomingAgentSessionId) {
    input.onIdentityConflict?.({
      agentId: "codex",
      currentAgentSessionId: currentCodexSessionId,
      incomingAgentSessionId,
      reason: "passive-agent-session-id-replacement",
      source: input.source,
    });
    /*
    CDXC:GxserverSessionIdentity 2026-06-09-08:55:
    Passive hook/session-state observations may be delayed or cross-wired between live Codex terminals. Keep Codex identity switchable through terminal-title and lifecycle evidence, but do not let a passive event replace an existing thread id because title reconciliation treats that id as canonical for every client.

    CDXC:GxserverSessionIdentity 2026-06-09-09:58:
    A Cursor hook may use a UUID-shaped session id, so only apply Codex passive-id protection when the observed event is Codex or when the current Codex row receives an otherwise anonymous UUID. Explicit Cursor agent names and Cursor transcript paths must correct stale Codex rows at the gxserver domain layer so every client receives the same icon/search/resume identity.
    */
    return keepCurrentSessionIdentity(input.resolvedIdentity, input.currentIdentity);
  }
  if (!currentCodexSessionId) {
    const owner = findActiveCodexIdentityOwner(input.sessions, input.currentSession, incomingAgentSessionId);
    if (owner) {
      input.onIdentityConflict?.({
        agentId: "codex",
        incomingAgentSessionId,
        ownerProjectId: owner.projectId,
        ownerSessionId: owner.sessionId,
        reason: "active-agent-session-id-owned",
        source: input.source,
      });
      return keepCurrentSessionIdentity(input.resolvedIdentity, input.currentIdentity);
    }
  }
  return input.resolvedIdentity;
}

function keepCurrentSessionIdentity(
  resolvedIdentity: GxserverResolvedSessionIdentity,
  currentIdentity: GxserverResolvedSessionIdentity,
): GxserverResolvedSessionIdentity {
  const currentAgentSessionId = normalizeText(currentIdentity.agentSessionId);
  const currentAgentSessionPath = normalizeText(currentIdentity.agentSessionPath);
  return {
    ...(resolvedIdentity.agentId ? { agentId: resolvedIdentity.agentId } : {}),
    ...(currentAgentSessionId ? { agentSessionId: currentAgentSessionId } : {}),
    ...(currentAgentSessionPath ? { agentSessionPath: currentAgentSessionPath } : {}),
  };
}

function mergeObservedSessionIdentity(
  observedIdentity: GxserverResolvedSessionIdentity,
  currentIdentity: GxserverResolvedSessionIdentity,
): GxserverResolvedSessionIdentity {
  const observedAgentId = normalizeAgentId(observedIdentity.agentId);
  const currentAgentId = normalizeAgentId(currentIdentity.agentId);
  const agentChanged = Boolean(observedAgentId && currentAgentId && observedAgentId !== currentAgentId);
  const agentId = observedAgentId ?? currentAgentId;
  const agentSessionId =
    normalizeText(observedIdentity.agentSessionId) ??
    (!agentChanged ? normalizeText(currentIdentity.agentSessionId) : undefined);
  const agentSessionPath =
    normalizeText(observedIdentity.agentSessionPath) ??
    (!agentChanged ? normalizeText(currentIdentity.agentSessionPath) : undefined);
  return {
    ...(agentId ? { agentId } : {}),
    ...(agentSessionId ? { agentSessionId } : {}),
    ...(agentSessionPath ? { agentSessionPath } : {}),
  };
}

function resolveStoredSessionIdentity(session: GxserverSessionDomainState): GxserverResolvedSessionIdentity {
  const storedIdentity = resolveSessionIdentity({
    agentId: session.agentId,
    agentName: session.runtimeSettings.agentName,
    agentSessionId: session.runtimeSettings.agentSessionId,
    agentSessionPath: session.runtimeSettings.agentSessionPath,
  });
  const transcriptPathIdentity = resolveSessionIdentity({
    agentSessionId: session.runtimeSettings.agentSessionId,
    agentSessionPath: session.runtimeSettings.agentSessionPath,
  });
  return mergeObservedSessionIdentity(transcriptPathIdentity, storedIdentity);
}

function applySessionIdentityRuntimeSettings(input: {
  currentIdentity: GxserverResolvedSessionIdentity;
  identity: GxserverResolvedSessionIdentity;
  runtimeSettings: Record<string, unknown>;
  source: GxserverSessionIdentityUpdateSource;
}): Record<string, unknown> {
  const runtimeSettings = { ...input.runtimeSettings };
  const currentAgentId = normalizeAgentId(input.currentIdentity.agentId);
  const nextAgentId = normalizeAgentId(input.identity.agentId);
  const agentChanged = Boolean(currentAgentId && nextAgentId && currentAgentId !== nextAgentId);
  const activityAgentId = readAgentActivityAgentId(runtimeSettings.agentActivity);
  const activityOwnerChanged = Boolean(nextAgentId && activityAgentId && activityAgentId !== nextAgentId);
  if (input.identity.agentId) {
    runtimeSettings.agentName = input.identity.agentId;
  }
  if (input.source === "live-process" && nextAgentId) {
    /*
    CDXC:GxserverSessionIdentity 2026-06-13-09:08:
    A real live process switch inside a terminal must be able to move a session from Codex to Claude or back. Once gxserver observes the actual agent executable, update the launch lock so later hooks reinforce the live process instead of resurrecting stale startup metadata.
    */
    runtimeSettings.launchAgentId = nextAgentId;
  }
  if (input.identity.agentSessionId) {
    runtimeSettings.agentSessionId = input.identity.agentSessionId;
  } else if (agentChanged) {
    delete runtimeSettings.agentSessionId;
  }
  if (input.identity.agentSessionPath) {
    runtimeSettings.agentSessionPath = input.identity.agentSessionPath;
  } else if (agentChanged) {
    delete runtimeSettings.agentSessionPath;
  }
  if (agentChanged) {
    delete runtimeSettings.agentId;
  }
  if (agentChanged || activityOwnerChanged) {
    /*
    CDXC:GxserverSessionIdentity 2026-06-12-02:44:
    Identity repair invalidates activity when its nested owner no longer matches the resolved agent. Drop that payload so a stale Claude/Cursor/etc. status blob cannot keep influencing a corrected Codex row; the next hook or title event rebuilds activity under the repaired agent identity.
    */
    delete runtimeSettings.agentActivity;
  }
  return runtimeSettings;
}

function readAgentActivityAgentId(value: unknown): string | undefined {
  if (typeof value !== "object" || value === null) {
    return undefined;
  }
  return normalizeAgentId((value as Record<string, unknown>).agentName);
}

function findActiveCodexIdentityOwner(
  sessions: readonly GxserverSessionDomainState[],
  currentSession: GxserverSessionDomainState,
  incomingAgentSessionId: string,
): GxserverSessionDomainState | undefined {
  return sessions.find((session) => {
    if (session.sessionId === currentSession.sessionId && session.projectId === currentSession.projectId) {
      return false;
    }
    if (!isActiveIdentityOwner(session)) {
      return false;
    }
    const identity = resolveSessionIdentity({
      agentId: session.agentId,
      runtimeSettings: session.runtimeSettings,
    });
    return normalizeAgentId(identity.agentId) === "codex" && normalizeCodexSessionId(identity.agentSessionId) === incomingAgentSessionId;
  });
}

function isActiveIdentityOwner(session: GxserverSessionDomainState): boolean {
  return (
    session.lifecycleState === "running" ||
    session.lifecycleState === "sleeping" ||
    (session.lifecycleState !== "stopped" && session.providerState.lifecycleState === "exists")
  );
}
