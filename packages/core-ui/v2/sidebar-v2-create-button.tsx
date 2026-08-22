import {
  IconChevronDown,
  IconFolderPlus,
  IconGitBranch,
  IconPlus,
  IconTerminal2,
  IconWorld,
} from "@tabler/icons-react";
import { useState } from "react";
import type { SidebarNewSessionEnvMode } from "../../shared/ghostex-settings";
import type { SidebarAgentButton } from "../../shared/sidebar-agents";
import { AppTooltip } from "../app-tooltip";
import { AgentMenuChatIndicator } from "../agent-menu-chat-indicator";
import { ProjectAgentLauncherIcon } from "../project-agent-launcher-icon";
import { SidebarContextMenuPortal } from "../sidebar-context-menu-portal";
import type { WebviewApi } from "../webview-api";

/*
 * CDXC:SidebarV2Worktree 2026-07-29:
 * V2's creation control: a split button whose PLAIN half is the unchanged
 * instant-session path (it posts exactly the message the classic sidebar's
 * agent button posts) and whose chevron opens the worktree entry point.
 *
 * Two rules this component enforces so the flow can never regress V1 behavior:
 * - Without the `worktreeSessions` capability there are NO worktree items. The
 *   control keeps only what its caller supplied, which is what an un-upgraded
 *   daemon (or a remote machine) must see.
 * - The "default to worktree" preference only ever changes what the PLAIN half
 *   opens. It never changes the message the instant path posts, and it is
 *   ignored entirely when the capability is missing.
 *
 * CDXC:SidebarV2SingleCreateControl 2026-07-30:
 * This split button is now the ONLY create control in V2's header, so the
 * chevron carries everything the shared V1 header used to spread across three
 * buttons: the agent picker, and the two explicitly-labelled Quick entries.
 * Each of those halves is opt-in per call site — the toolbar passes them, the
 * per-project group headers do not — so a group header renders exactly the
 * chevron it rendered before this change (worktree items only, and therefore
 * no chevron at all without the capability).
 */

export type SidebarV2CreateButtonPosition = {
  clientX: number;
  clientY: number;
};

export type SidebarV2CreateButtonProps = {
  /**
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * The configured agents the chevron's picker offers. Absent (or empty)
   * together with `onCreateAgentSession` means this control has no picker,
   * which is what the per-project group headers want.
   */
  agents?: readonly SidebarAgentButton[];
  /** gxserver for this button's project can serve the worktree flow. */
  canCreateWorktree: boolean;
  /** Global default for the plain half: instant local session, or the popover. */
  defaultEnvMode: SidebarNewSessionEnvMode;
  /** Accessible name for the plain half, e.g. "New session in ghostex". */
  label: string;
  /**
   * Launch `agent` in the SAME target the plain half resolves. Picking an agent
   * here is a create action, not a preference write.
   */
  onCreateAgentSession?: (agent: SidebarAgentButton) => void;
  onCreateInstantSession: () => void;
  /**
   * CDXC:AddProject 2026-07-30:
   * Opens the shared add-project dialog. V2's header has no separate Projects
   * section header to hang Add Project off, so it lives in this menu next to
   * the other "make something new" entries. Absent callers (the per-project
   * group headers) show no such item — adding a project is not a per-project
   * action.
   */
  onAddProject?: () => void;
  /**
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * The two Quick entries. They are the ONLY way V2 creates in the Quick
   * collection now, which is why they are explicitly labelled "Quick …" and
   * why every other path in this control targets a real project.
   */
  onCreateQuickBrowserTab?: () => void;
  onCreateQuickTerminal?: () => void;
  onOpenWorktreePopover: (position: SidebarV2CreateButtonPosition) => void;
  onSetDefaultEnvMode?: (mode: SidebarNewSessionEnvMode) => void;
  /** Last-used agent id, for the picker's checkmark. */
  primaryAgentId?: string;
  vscode: WebviewApi;
};

export function SidebarV2CreateButton({
  agents,
  canCreateWorktree,
  defaultEnvMode,
  label,
  onAddProject,
  onCreateAgentSession,
  onCreateInstantSession,
  onCreateQuickBrowserTab,
  onCreateQuickTerminal,
  onOpenWorktreePopover,
  onSetDefaultEnvMode,
  primaryAgentId,
  vscode,
}: SidebarV2CreateButtonProps) {
  const [menuPosition, setMenuPosition] = useState<SidebarV2CreateButtonPosition>();

  const popoverPositionFrom = (element: HTMLElement): SidebarV2CreateButtonPosition => {
    const rect = element.getBoundingClientRect();
    return { clientX: rect.left, clientY: rect.bottom + 4 };
  };

  const worktreeIsDefault = canCreateWorktree && defaultEnvMode === "worktree";

  /*
   * CDXC:SidebarV2SingleCreateControl 2026-07-30:
   * The chevron exists for whatever the caller actually supplied. The toolbar
   * always has the picker and the Quick entries, so its chevron is always
   * there — including on daemons with no worktree support, which used to hide
   * it. A group header supplies neither, so its chevron still appears only with
   * the worktree capability, exactly as before.
   *
   * `onSetDefaultEnvMode` deliberately does NOT count: the worktree preference
   * is meaningless without the capability, so it can never be the only reason a
   * menu opens.
   */
  const pickableAgents = onCreateAgentSession ? (agents ?? []) : [];
  const hasQuickItems =
    onCreateQuickTerminal !== undefined || onCreateQuickBrowserTab !== undefined;
  const hasMenu =
    canCreateWorktree ||
    pickableAgents.length > 0 ||
    hasQuickItems ||
    onAddProject !== undefined;

  return (
    <div className="sidebar-v2-create-split" data-can-worktree={String(canCreateWorktree)}>
      <AppTooltip content={worktreeIsDefault ? "New worktree session" : label}>
        <button
          aria-label={label}
          className="sidebar-v2-create-button"
          onClick={(event) => {
            event.stopPropagation();
            if (worktreeIsDefault) {
              onOpenWorktreePopover(popoverPositionFrom(event.currentTarget));
              return;
            }
            onCreateInstantSession();
          }}
          type="button"
        >
          <IconPlus aria-hidden="true" size={14} stroke={2} />
        </button>
      </AppTooltip>
      {hasMenu ? (
        <button
          aria-expanded={menuPosition !== undefined}
          aria-haspopup="menu"
          aria-label="New session options"
          className="sidebar-v2-create-chevron"
          onClick={(event) => {
            event.stopPropagation();
            setMenuPosition(popoverPositionFrom(event.currentTarget));
          }}
          type="button"
        >
          <IconChevronDown aria-hidden="true" size={12} stroke={2} />
        </button>
      ) : null}
      {menuPosition ? (
        <SidebarContextMenuPortal
          menuClassName="session-context-menu sidebar-v2-create-menu"
          menuStyle={{ left: `${menuPosition.clientX}px`, top: `${menuPosition.clientY}px` }}
          onDismiss={() => setMenuPosition(undefined)}
          vscode={vscode}
        >
          {/*
            * CDXC:SidebarV2SingleCreateControl 2026-07-30:
            * Section one is "create in the resolved target project": every
            * agent this host has configured, then the worktree entry point.
            * Picking an agent here launches it the same way the plain half
            * launches the primary one — same target, same message — so the
            * picker is a create action and not a preference editor. (It does
            * update the last-used agent, because it launches through the
            * caller's own agent path, which is the behavior V1's picker has.)
            */}
          {pickableAgents.length > 0 || canCreateWorktree ? (
            <div className="session-context-menu-section">
              {pickableAgents.map((agent) => (
                <button
                  aria-label={agent.name}
                  aria-pressed={agent.agentId === primaryAgentId}
                  className="session-context-menu-item group-agent-menu-item sidebar-v2-create-menu-agent"
                  data-agent-id={agent.agentId}
                  data-selected={String(agent.agentId === primaryAgentId)}
                  key={agent.agentId}
                  onClick={() => {
                    setMenuPosition(undefined);
                    onCreateAgentSession?.(agent);
                  }}
                  role="menuitem"
                  type="button"
                >
                  <ProjectAgentLauncherIcon agent={agent} colorMode="brand" />
                  <span className="group-agent-menu-label">{agent.name}</span>
                  <AgentMenuChatIndicator agent={agent} />
                </button>
              ))}
              {canCreateWorktree ? (
                <button
                  className="session-context-menu-item"
                  onClick={() => {
                    const position = menuPosition;
                    setMenuPosition(undefined);
                    onOpenWorktreePopover(position);
                  }}
                  role="menuitem"
                  type="button"
                >
                  <IconGitBranch
                    aria-hidden="true"
                    className="session-context-menu-icon"
                    size={16}
                    stroke={1.8}
                  />
                  New worktree session…
                </button>
              ) : null}
            </div>
          ) : null}
          {/*
            * CDXC:SidebarV2SingleCreateControl 2026-07-30:
            * The Quick entries are the ONLY create paths in V2 that land in the
            * Quick collection, and they say so in their labels. Everything
            * above targets a real project, so a projectless session is now
            * always something the user asked for by name.
            */}
          {hasQuickItems ? (
            <>
              <div className="session-context-menu-divider" role="separator" />
              <div className="session-context-menu-section">
                {onCreateQuickTerminal ? (
                  <button
                    className="session-context-menu-item"
                    onClick={() => {
                      setMenuPosition(undefined);
                      onCreateQuickTerminal();
                    }}
                    role="menuitem"
                    type="button"
                  >
                    <IconTerminal2
                      aria-hidden="true"
                      className="session-context-menu-icon"
                      size={16}
                      stroke={1.8}
                    />
                    Quick Terminal
                  </button>
                ) : null}
                {onCreateQuickBrowserTab ? (
                  <button
                    className="session-context-menu-item"
                    onClick={() => {
                      setMenuPosition(undefined);
                      onCreateQuickBrowserTab();
                    }}
                    role="menuitem"
                    type="button"
                  >
                    <IconWorld
                      aria-hidden="true"
                      className="session-context-menu-icon"
                      size={16}
                      stroke={1.8}
                    />
                    Quick Browser Tab
                  </button>
                ) : null}
              </div>
            </>
          ) : null}
          {/*
            * CDXC:AddProject 2026-07-30:
            * Adding a project is not creating a session, so it gets its own
            * section below the create paths. It opens the shared add-project
            * dialog, which owns machine selection, browsing, and cloning.
            */}
          {onAddProject ? (
            <>
              <div className="session-context-menu-divider" role="separator" />
              <div className="session-context-menu-section">
                <button
                  className="session-context-menu-item"
                  data-sidebar-v2-create-menu-item="addProject"
                  onClick={() => {
                    setMenuPosition(undefined);
                    onAddProject();
                  }}
                  role="menuitem"
                  type="button"
                >
                  <IconFolderPlus
                    aria-hidden="true"
                    className="session-context-menu-icon"
                    size={16}
                    stroke={1.8}
                  />
                  Add project…
                </button>
              </div>
            </>
          ) : null}
          {canCreateWorktree && onSetDefaultEnvMode ? (
            <>
              <div className="session-context-menu-divider" role="separator" />
              <div className="session-context-menu-section">
                <button
                  aria-checked={defaultEnvMode === "worktree"}
                  className="session-context-menu-item"
                  onClick={() => {
                    setMenuPosition(undefined);
                    onSetDefaultEnvMode(defaultEnvMode === "worktree" ? "local" : "worktree");
                  }}
                  role="menuitemcheckbox"
                  type="button"
                >
                  <span className="session-context-menu-icon" aria-hidden="true">
                    {defaultEnvMode === "worktree" ? "✓" : ""}
                  </span>
                  Default new sessions to worktree
                </button>
              </div>
            </>
          ) : null}
        </SidebarContextMenuPortal>
      ) : null}
    </div>
  );
}
