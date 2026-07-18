import { createRootRoute, Outlet } from "@tanstack/react-router";
import { useEffect, useState, type PointerEvent as ReactPointerEvent } from "react";
import { TitlebarActions } from "../app/titlebar-actions";
import { MachinesControl } from "../machines/MachinesControl";
import { WebSidebar } from "../sidebar-runtime/WebSidebar";

const WEB_TITLEBAR_HIDDEN_SECTIONS = true;
const SIDEBAR_WIDTH_STORAGE_KEY = "ghostexWeb.sidebarWidth.v1";
const DEFAULT_SIDEBAR_WIDTH = 296;
const MIN_SIDEBAR_WIDTH = 220;
const MAX_SIDEBAR_WIDTH = 520;

const VIEW_TABS = ["Agents", "Source", "Browser", "Kanban", "Automate", "Docs"] as const;

type IconName = "sidebar";

const ICON_PATHS: Record<IconName, string> = {
  sidebar: "M4 5h16v14H4zM9 5v14",
};

function clampSidebarWidth(width: number): number {
  return Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, width));
}

function readSidebarWidth(): number {
  const storedWidth = Number(window.localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY));
  return Number.isFinite(storedWidth) && storedWidth > 0
    ? clampSidebarWidth(storedWidth)
    : DEFAULT_SIDEBAR_WIDTH;
}

function ShellIcon({ name }: { name: IconName }) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path d={ICON_PATHS[name]} />
    </svg>
  );
}

function Titlebar({
  sidebarCollapsed,
  toggleSidebar,
}: {
  sidebarCollapsed: boolean;
  toggleSidebar(): void;
}) {
  return (
    <header className="web-titlebar">
      <div className="web-titlebar__left">
        <button
          aria-label={sidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
          className="web-titlebar__icon-button web-titlebar__sidebar-toggle"
          onClick={toggleSidebar}
          type="button"
        >
          <ShellIcon name="sidebar" />
        </button>
        <span className="web-titlebar__title">Ghostex</span>
        <MachinesControl />
      </div>

      <nav
        aria-label="Ghostex views"
        className="web-titlebar__center"
        hidden={WEB_TITLEBAR_HIDDEN_SECTIONS}
      >
        {VIEW_TABS.map((view) => (
          <button
            aria-current={view === "Agents" ? "page" : undefined}
            className="web-titlebar__view-tab"
            key={view}
            type="button"
          >
            {view}
          </button>
        ))}
      </nav>

      <div className="web-titlebar__right">
        <TitlebarActions />
      </div>
    </header>
  );
}

function GhostexWebShell() {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(readSidebarWidth);

  useEffect(() => {
    window.localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidth));
  }, [sidebarWidth]);

  const resizeSidebar = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.buttons !== 1) {
      return;
    }
    setSidebarWidth(clampSidebarWidth(event.clientX));
  };

  const beginSidebarResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    setSidebarWidth(clampSidebarWidth(event.clientX));
  };

  const endSidebarResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <div className="ghostex-web-shell">
      <Titlebar
        sidebarCollapsed={sidebarCollapsed}
        toggleSidebar={() => setSidebarCollapsed((collapsed) => !collapsed)}
      />
      <div
        className="ghostex-web-shell__body"
        style={{
          gridTemplateColumns: sidebarCollapsed
            ? "minmax(0, 1fr)"
            : `${sidebarWidth}px 5px minmax(0, 1fr)`,
        }}
      >
        {!sidebarCollapsed && (
          <>
            <aside aria-label="Sessions sidebar" className="web-sidebar">
              <WebSidebar />
            </aside>
            <div
              aria-label="Resize sidebar"
              className="web-sidebar-divider"
              onPointerCancel={endSidebarResize}
              onPointerDown={beginSidebarResize}
              onPointerMove={resizeSidebar}
              onPointerUp={endSidebarResize}
              role="separator"
            />
          </>
        )}
        <main className="web-workspace">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

export const Route = createRootRoute({
  component: GhostexWebShell,
});
