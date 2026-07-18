import type {
  GxserverPresentationSession,
  GxserverSessionDomainState,
} from "@/shared/gxserver-protocol";
import type {
  WorkspacePresentationState,
  WorkspaceSession,
} from "../workspace/workspace-model";

export interface SessionReference {
  machineId: string;
  projectId: string;
  sessionId: string;
}

export function createWorkspaceSessionId(reference: SessionReference): string {
  return [reference.machineId, reference.projectId, reference.sessionId]
    .map((part) => encodeURIComponent(part))
    .join("/");
}

export function presentationSessionToWorkspaceSession(
  machineId: string,
  session: GxserverPresentationSession,
): WorkspaceSession {
  const reference = { machineId, projectId: session.projectId, sessionId: session.sessionId };
  return {
    ...reference,
    activity: session.activity,
    ...(session.agentIcon || session.agentName || session.agentId
      ? { agentIcon: session.agentIcon ?? session.agentName ?? session.agentId }
      : {}),
    presentationState: presentationStateForSession(session),
    title: session.displayTitle ?? session.title,
    workspaceId: createWorkspaceSessionId(reference),
  };
}

export function domainSessionToWorkspaceSession(
  machineId: string,
  session: GxserverSessionDomainState,
  presentationState: WorkspacePresentationState,
  statusMessage?: string,
): WorkspaceSession {
  const reference = { machineId, projectId: session.projectId, sessionId: session.sessionId };
  return {
    ...reference,
    activity: "idle",
    ...(session.agentId ? { agentIcon: session.agentId } : {}),
    presentationState,
    ...(statusMessage ? { statusMessage } : {}),
    title: session.title || "Terminal",
    workspaceId: createWorkspaceSessionId(reference),
  };
}

function presentationStateForSession(
  session: GxserverPresentationSession,
): WorkspacePresentationState {
  if (session.lifecycleState === "sleeping") {
    return "sleeping";
  }
  if (session.lifecycleState === "running" && session.providerSessionState === "exists") {
    return "running";
  }
  if (session.lifecycleState === "running" && session.providerSessionState === "missing") {
    return "restored-unmounted";
  }
  if (session.lifecycleState === "unknown" || session.providerSessionState === "unknown") {
    return "mounting";
  }
  return "startup-failed";
}
