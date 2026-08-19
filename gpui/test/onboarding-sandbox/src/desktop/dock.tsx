/*
 * Fake macOS dock. Only the Ghostex tile is wired: clicking it launches the
 * simulated app (or re-focuses the fake window when it is already running).
 * Decoy tiles exist purely to make the desktop read as a desktop.
 */
import type { ReactNode } from "react";
import ghostexIconUrl from "@/gpui/resources/AppIcon.appiconset/icon_256x256.png";
import { useSandboxStore } from "../state/store";
import {
  CompassGlyph,
  FinderGlyph,
  MailGlyph,
  MusicGlyph,
  NotesGlyph,
  TerminalGlyph,
  TrashGlyph,
} from "./icons";
import "./dock.css";

interface DecoyApp {
  glyph: ReactNode;
  id: string;
  label: string;
  running?: boolean;
  tint: string;
}

const DECOY_APPS: DecoyApp[] = [
  {
    glyph: <FinderGlyph />,
    id: "finder",
    label: "Finder",
    running: true,
    tint: "linear-gradient(180deg, #4fb2ff 0%, #1f7fe0 100%)",
  },
  {
    glyph: <CompassGlyph />,
    id: "safari",
    label: "Safari",
    tint: "linear-gradient(180deg, #f4f7fb 0%, #c9d6e6 100%)",
  },
  {
    glyph: <MailGlyph />,
    id: "mail",
    label: "Mail",
    tint: "linear-gradient(180deg, #61c3ff 0%, #2b7de0 100%)",
  },
  {
    glyph: <NotesGlyph />,
    id: "notes",
    label: "Notes",
    tint: "linear-gradient(180deg, #ffe27a 0%, #f5c518 100%)",
  },
  {
    glyph: <MusicGlyph />,
    id: "music",
    label: "Music",
    tint: "linear-gradient(180deg, #ff6a75 0%, #e0293e 100%)",
  },
  {
    glyph: <TerminalGlyph size={24} />,
    id: "terminal",
    label: "Terminal",
    tint: "linear-gradient(180deg, #3a3a42 0%, #17171b 100%)",
  },
];

export function Dock({ onFocusGhostex }: { onFocusGhostex: () => void }) {
  const appPhase = useSandboxStore((s) => s.appPhase);
  const launchApp = useSandboxStore((s) => s.launchApp);
  const isRunning = appPhase !== "notRunning";

  return (
    <div className="sbx-dock-area">
      <div className="sbx-dock">
        {DECOY_APPS.map((app) => (
          <div className="sbx-dock-item" key={app.id}>
            <div className="sbx-dock-tooltip">{app.label}</div>
            <div
              aria-label={app.label}
              className="sbx-dock-tile sbx-dock-tile-decoy"
              role="img"
              style={{ background: app.tint }}
            >
              <span className="sbx-dock-tile-glyph" data-dark={app.id === "notes" || app.id === "safari"}>
                {app.glyph}
              </span>
            </div>
            <span className="sbx-dock-running" data-on={app.running === true} />
          </div>
        ))}

        <span className="sbx-dock-separator" />

        <div className="sbx-dock-item">
          <div className="sbx-dock-tooltip">Ghostex</div>
          <button
            aria-label={isRunning ? "Focus Ghostex" : "Launch Ghostex"}
            className="sbx-dock-tile sbx-dock-tile-ghostex"
            data-launching={appPhase === "launching"}
            onClick={() => {
              if (isRunning) {
                onFocusGhostex();
                return;
              }
              launchApp();
            }}
            type="button"
          >
            <img alt="" className="sbx-dock-ghostex-icon" src={ghostexIconUrl} />
          </button>
          <span className="sbx-dock-running" data-on={isRunning} />
        </div>

        <span className="sbx-dock-separator" />

        <div className="sbx-dock-item">
          <div className="sbx-dock-tooltip">Trash</div>
          <div
            aria-label="Trash"
            className="sbx-dock-tile sbx-dock-tile-decoy"
            role="img"
            style={{ background: "linear-gradient(180deg, #9aa3ae 0%, #5f6874 100%)" }}
          >
            <span className="sbx-dock-tile-glyph">
              <TrashGlyph />
            </span>
          </div>
          <span className="sbx-dock-running" data-on={false} />
        </div>
      </div>
    </div>
  );
}
