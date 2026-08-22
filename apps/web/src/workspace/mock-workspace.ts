import type { WorkspaceSession } from "./workspace-model";

export function createMockWorkspaceSessions(): WorkspaceSession[] {
  const base = {
    machineId: "local",
    projectId: "ghostex-web-debug",
  };
  return [
    {
      ...base,
      sessionId: "debug-agent",
      title: "Agent",
      agentIcon: "codex",
      presentationState: "running",
      activity: "idle",
    },
    {
      ...base,
      sessionId: "debug-build",
      title: "Build",
      agentIcon: "codex",
      presentationState: "running",
      activity: "working",
    },
    {
      ...base,
      sessionId: "debug-review",
      title: "Review",
      agentIcon: "claude",
      presentationState: "running",
      activity: "attention",
    },
    {
      ...base,
      sessionId: "debug-sleeping",
      title: "Sleeping",
      presentationState: "sleeping",
      activity: "idle",
    },
    {
      ...base,
      sessionId: "debug-mounting",
      title: "Mounting",
      presentationState: "mounting",
      activity: "idle",
    },
    {
      ...base,
      sessionId: "debug-failed",
      title: "Failed startup",
      presentationState: "startup-failed",
      activity: "idle",
    },
    {
      ...base,
      sessionId: "debug-restored",
      title: "Restored",
      presentationState: "restored-unmounted",
      activity: "idle",
    },
  ];
}
