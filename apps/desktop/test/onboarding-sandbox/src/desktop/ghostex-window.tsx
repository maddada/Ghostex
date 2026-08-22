/*
 * Fake Ghostex main window. Visual re-mock only — the real SidebarApp is never
 * mounted here (SPEC.md "Desktop UI"). The sidebar pane reproduces the real
 * host-owned empty states from packages/core-ui/sidebar-app.tsx (gxserver unavailable →
 * "Unable to load sessions." + Load Sessions, zero projects → "No Projects
 * Added." onboarding copy) so onboarding scenarios read correctly.
 */
import { useRef } from "react";
import { useSandboxStore } from "../state/store";
import { InfoCircleGlyph, PlusGlyph, SearchGlyph, TerminalGlyph } from "./icons";
import { TipsPanel } from "./tips-panel";
import { ToastStack } from "./toast-stack";
import type { GxserverHealthScenario } from "../state/types";
import "./ghostex-window.css";

const UNHEALTHY_GXSERVER_SCENARIOS: readonly GxserverHealthScenario[] = [
  "buildMismatch",
  "protocolMismatch",
  "spawnFailure",
];

const FAKE_PROJECT_NAMES = [
  "ghostex",
  "acme-api",
  "design-system",
  "infra-scripts",
  "playground",
  "docs-site",
];

const FAKE_SESSION_TITLES = [
  "Implement onboarding sandbox",
  "Review release notes",
  "Fix sidebar diff stats",
];

function fakeProjects(count: number): { id: string; name: string; sessions: string[] }[] {
  return Array.from({ length: Math.min(count, 12) }, (_, index) => ({
    id: `sandbox-project-${index}`,
    name: FAKE_PROJECT_NAMES[index % FAKE_PROJECT_NAMES.length] + (index >= FAKE_PROJECT_NAMES.length ? `-${index}` : ""),
    sessions: index === 0 ? FAKE_SESSION_TITLES : FAKE_SESSION_TITLES.slice(0, 1),
  }));
}

export function GhostexWindow({ focused }: { focused: boolean }) {
  const tipsButtonRef = useRef<HTMLButtonElement | null>(null);
  const projectCount = useSandboxStore((s) => s.env.projectCount);
  const gxserverScenario = useSandboxStore((s) => s.env.gxserver.scenario);
  const tipsBadgeCount = useSandboxStore((s) => s.tipsBadgeCount);
  const tipsPanelOpen = useSandboxStore((s) => s.tipsPanelOpen);
  const setTipsPanelOpen = useSandboxStore((s) => s.setTipsPanelOpen);
  const quitApp = useSandboxStore((s) => s.quitApp);
  const emitEvent = useSandboxStore((s) => s.emitEvent);

  const gxserverUnavailable = UNHEALTHY_GXSERVER_SCENARIOS.includes(gxserverScenario);
  const projects = gxserverUnavailable ? [] : fakeProjects(projectCount);

  return (
    <div className="sbx-window" data-focused={focused}>
      <div className="sbx-window-titlebar">
        <div className="sbx-traffic-lights">
          <button
            aria-label="Quit Ghostex"
            className="sbx-traffic-light sbx-traffic-close"
            onClick={() => quitApp()}
            title="Quit Ghostex"
            type="button"
          />
          <span aria-hidden="true" className="sbx-traffic-light sbx-traffic-minimize" />
          <span aria-hidden="true" className="sbx-traffic-light sbx-traffic-zoom" />
        </div>
        <div className="sbx-window-title">
          <span className="sbx-window-title-app">Ghostex</span>
          {projects.length > 0 ? (
            <span className="sbx-window-title-project">— {projects[0].name}</span>
          ) : null}
        </div>
        <div className="sbx-window-titlebar-right">
          <button
            aria-label="Tips"
            className="sbx-titlebar-button"
            data-active={tipsPanelOpen}
            onClick={() => setTipsPanelOpen(!tipsPanelOpen)}
            ref={tipsButtonRef}
            type="button"
          >
            <InfoCircleGlyph />
            {tipsBadgeCount > 0 ? (
              <span className="sbx-titlebar-badge">{tipsBadgeCount > 99 ? "99+" : tipsBadgeCount}</span>
            ) : null}
          </button>
        </div>
        {tipsPanelOpen ? <TipsPanel anchorRef={tipsButtonRef} /> : null}
      </div>

      <div className="sbx-window-body">
        <div className="sbx-sidebar">
          <div className="sbx-sidebar-row sbx-sidebar-search">
            <SearchGlyph size={13} />
            <span>Search</span>
          </div>
          <div className="sbx-sidebar-section-heading">
            <span>Projects</span>
            <span className="sbx-sidebar-plus" title="Add project">
              <PlusGlyph />
            </span>
          </div>

          {gxserverUnavailable ? (
            <div className="sbx-sidebar-empty-state">
              Unable to load sessions.
              <br />
              <button
                className="sbx-sidebar-empty-state-action"
                onClick={() =>
                  emitEvent({
                    codeRef: "packages/core-ui/sidebar-app.tsx onStartGxserver",
                    detail:
                      "Sandbox desktop only reports the click; recovering gxserver is the engine's job.",
                    kind: "message",
                    label: "Sidebar empty state: Load Sessions clicked",
                  })
                }
                type="button"
              >
                Load Sessions
              </button>
            </div>
          ) : projects.length === 0 ? (
            <div className="sbx-sidebar-empty-state">
              No Projects Added.
              <br />
              <br />
              Hover over the Projects label and click on the plus button to add your first project
              and get started!
            </div>
          ) : (
            <div className="sbx-sidebar-projects">
              {projects.map((project) => (
                <div className="sbx-sidebar-project" key={project.id}>
                  <div className="sbx-sidebar-row sbx-sidebar-project-row">
                    <span className="sbx-sidebar-disclosure" />
                    <span className="sbx-sidebar-project-name">{project.name}</span>
                  </div>
                  {project.sessions.map((session) => (
                    <div className="sbx-sidebar-row sbx-sidebar-session-row" key={session}>
                      <span className="sbx-sidebar-session-dot" />
                      <span className="sbx-sidebar-session-title">{session}</span>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="sbx-main">
          <div className="sbx-main-tabstrip">
            <span className="sbx-main-tab" data-active="true">
              <TerminalGlyph size={13} />
              <span>zsh</span>
            </span>
            <span className="sbx-main-tab">
              <TerminalGlyph size={13} />
              <span>codex</span>
            </span>
          </div>
          <div className="sbx-terminal">
            <div className="sbx-terminal-line">
              <span className="sbx-terminal-path">~/dev/ghostex</span>
              <span className="sbx-terminal-prompt">&gt;</span>
              <span className="sbx-terminal-cursor" />
            </div>
            <div className="sbx-terminal-hint">
              Simulated terminal at rest. The sandbox renders app chrome and the real onboarding
              modals; it does not run a shell.
            </div>
          </div>
        </div>
      </div>

      <ToastStack />
    </div>
  );
}
