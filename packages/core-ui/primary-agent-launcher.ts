const PRIMARY_AGENT_LAUNCHER_STORAGE_KEY = "ghostex-sidebar-project-terminal-launcher";

export const PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT = "ghostex-sidebar-primary-agent-launcher-changed";

export type PrimaryAgentLauncherChangedEvent = CustomEvent<{
  agentId: string;
}>;

export function readPrimaryAgentLauncherId(): string | undefined {
  /**
   * CDXC:QuickAgents 2026-06-08-18:25:
   * Quick and project headers share one selected sidebar agent so the section-level Quick picker behaves like the existing project-header picker instead of maintaining a second default. Keep the historic storage key so existing project agent choices carry forward.
   */
  return localStorage.getItem(PRIMARY_AGENT_LAUNCHER_STORAGE_KEY)?.trim() || undefined;
}

export function writePrimaryAgentLauncherId(agentId: string): void {
  localStorage.setItem(PRIMARY_AGENT_LAUNCHER_STORAGE_KEY, agentId);
  window.dispatchEvent(
    new CustomEvent(PRIMARY_AGENT_LAUNCHER_CHANGED_EVENT, {
      detail: { agentId },
    }),
  );
}
