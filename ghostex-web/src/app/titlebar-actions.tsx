import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { AppTooltip } from "@/sidebar/app-tooltip";
import type {
  GxserverSidebarHudCommandButton,
  GxserverSidebarHudResponse,
} from "@/shared/gxserver-protocol";
import {
  getConnectionStates,
  rpcForMachine,
  subscribeConnectionStates,
} from "../connections/connection-registry";
import {
  getActiveSidebarProject,
  subscribeActiveSidebarProject,
} from "../sidebar-runtime/active-project-store";
import "./action-events";

function BoltIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path d="M13 3v7h6L11 21v-7H5L13 3Z" />
    </svg>
  );
}

export function TitlebarActions() {
  const activeProject = useSyncExternalStore(
    subscribeActiveSidebarProject,
    getActiveSidebarProject,
    getActiveSidebarProject,
  );
  const connections = useSyncExternalStore(
    subscribeConnectionStates,
    getConnectionStates,
    getConnectionStates,
  );
  const [open, setOpen] = useState(false);
  const [commands, setCommands] = useState<readonly GxserverSidebarHudCommandButton[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const menuRef = useRef<HTMLDivElement>(null);
  const requestId = useRef(0);
  const activeConnection = connections.find(
    (state) => state.machine.machineId === activeProject?.machineId,
  );
  const commandSessionsExist = Boolean(
    activeProject
    && activeConnection?.presentation?.sessions.some((session) =>
      session.projectId === activeProject.projectId
      && session.surface === "commands"
    ),
  );

  useEffect(() => {
    if (!open) return;
    const close = (event: globalThis.PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  useEffect(() => {
    if (!open || !activeProject) {
      setCommands([]);
      setError(undefined);
      return;
    }
    const nextRequestId = ++requestId.current;
    setLoading(true);
    setError(undefined);
    void rpcForMachine<GxserverSidebarHudResponse>(
      activeProject.machineId,
      "/api/readSidebarHud",
      { activeProjectId: activeProject.projectId },
    ).then((response) => {
      if (requestId.current === nextRequestId) setCommands(response.commands);
    }).catch((nextError: unknown) => {
      if (requestId.current === nextRequestId) {
        setCommands([]);
        setError(nextError instanceof Error ? nextError.message : String(nextError));
      }
    }).finally(() => {
      if (requestId.current === nextRequestId) setLoading(false);
    });
  }, [activeProject, open]);

  const runAction = (action: GxserverSidebarHudCommandButton) => {
    if (!activeProject) return;
    if (action.actionType === "browser") {
      if (!action.url?.trim()) {
        setError(`${action.name} does not have a URL.`);
        return;
      }
      window.open(action.url, "_blank", "noopener,noreferrer");
      setOpen(false);
      return;
    }
    if (!action.command?.trim()) {
      setError(`${action.name} does not have a command.`);
      return;
    }
    /*
     * CDXC:ProjectActions 2026-07-31-12:00:
     * Terminal actions can carry saved links that open alongside the command
     * run. The web app has no integrated browser pane, so both link targets
     * open as regular browser tabs here.
     */
    for (const link of action.links ?? []) {
      if (link.url.trim()) {
        window.open(link.url, "_blank", "noopener,noreferrer");
      }
    }
    window.dispatchEvent(new CustomEvent("ghostex-web:runTitlebarAction", {
      detail: {
        action,
        machineId: activeProject.machineId,
        projectId: activeProject.projectId,
      },
    }));
    setOpen(false);
  };

  return (
    <div className="web-titlebar-actions" ref={menuRef}>
      <AppTooltip content="Actions">
        <button
          aria-expanded={open}
          aria-haspopup="menu"
          aria-label="Actions"
          className="web-titlebar__icon-button web-titlebar__action"
          onClick={() => setOpen((current) => !current)}
          type="button"
        >
          <BoltIcon />
        </button>
      </AppTooltip>
      {open && (
        <div className="web-actions-menu" role="menu">
          <div className="web-actions-menu__heading">Actions</div>
          {!activeProject && (
            <div className="web-actions-menu__status">Select a project to view its actions.</div>
          )}
          {activeProject && loading && (
            <div className="web-actions-menu__status">Loading actions…</div>
          )}
          {activeProject && !loading && !error && commands.length === 0 && (
            <div className="web-actions-menu__status">No actions configured.</div>
          )}
          {commands.map((command) => (
            <button
              className="web-actions-menu__item"
              key={command.commandId}
              onClick={() => runAction(command)}
              role="menuitem"
              type="button"
            >
              <span>{command.name}</span>
              <small>{command.actionType === "browser" ? "Browser" : "Terminal"}</small>
            </button>
          ))}
          {error && <div className="web-actions-menu__error">{error}</div>}
          {commandSessionsExist && (
            <button
              className="web-actions-menu__reopen"
              onClick={() => {
                window.dispatchEvent(new CustomEvent("ghostex-web:openCommandPane"));
                setOpen(false);
              }}
              role="menuitem"
              type="button"
            >
              <span>Show Command Pane</span>
              <span aria-hidden="true">⌃</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}
