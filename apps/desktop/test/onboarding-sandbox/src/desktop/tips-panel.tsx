/*
 * Tips & Tricks dropdown anchored under the fake titlebar's info button.
 *
 * This is a faithful VISUAL RE-MOCK of the real panel (header actions +
 * Notices/Unread/Read sections, verbatim tip copy), not the production
 * component. Mounting the real one was evaluated and rejected:
 * apps/desktop/views/titlebar-host.tsx does not export TitlebarTipsMenu — only
 * GhostexTitlebarHost — and importing that module runs three module-scope side
 * effects in whatever document imports it: it reads
 * __ghostex_TITLEBAR_PANEL_KIND__ at import time, it appends a global <style>
 * element to document.head, and it imports packages/core-ui/styles.css (generated
 * Tailwind + preflight) which would restyle the whole sandbox page, including
 * the control panel another agent owns. It also self-mounts into #root when one
 * exists. An iframe would isolate that, but the iframe HTML entry lives outside
 * this agent's directory, so the re-mock is the safe choice here.
 */
import { useEffect, useRef } from "react";
import { useSandboxStore } from "../state/store";
import {
  BookGlyph,
  HistoryGlyph,
  InfoCircleGlyph,
  StarGlyph,
  ToolGlyph,
  WarningGlyph,
} from "./icons";
import { SANDBOX_TIPS } from "./tips-content";
import "./tips-panel.css";

export function TipsPanel({ anchorRef }: { anchorRef: React.RefObject<HTMLElement | null> }) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const notices = useSandboxStore((s) => s.tipsNotices);
  const badgeCount = useSandboxStore((s) => s.tipsBadgeCount);
  const setTipsPanelOpen = useSandboxStore((s) => s.setTipsPanelOpen);

  useEffect(() => {
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (panelRef.current?.contains(target)) return;
      if (anchorRef.current?.contains(target)) return;
      setTipsPanelOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => document.removeEventListener("mousedown", onPointerDown, true);
  }, [anchorRef, setTipsPanelOpen]);

  /*
   * The store exposes only the badge count, so split the catalog the way the
   * real panel does: the badge counts unread tips plus notices.
   */
  const unreadTipCount = Math.max(0, Math.min(SANDBOX_TIPS.length, badgeCount - notices.length));
  const unreadTips = SANDBOX_TIPS.slice(0, unreadTipCount);
  const readTips = SANDBOX_TIPS.slice(unreadTipCount);

  return (
    <div className="sbx-tips-panel" ref={panelRef}>
      <div className="sbx-tips-header">
        <div className="sbx-tips-title">
          <InfoCircleGlyph size={18} />
          <span>Tips</span>
        </div>
        <div className="sbx-tips-actions">
          <button className="sbx-tips-action" type="button">
            <BookGlyph />
            <span>Docs</span>
          </button>
          <button className="sbx-tips-action" type="button">
            <StarGlyph />
            <span>Video</span>
          </button>
          <button className="sbx-tips-action" type="button">
            <ToolGlyph />
            <span>Setup</span>
          </button>
          <button className="sbx-tips-action" type="button">
            <HistoryGlyph />
            <span>Updates</span>
          </button>
        </div>
      </div>
      <div className="sbx-tips-scroll">
        {notices.length > 0 ? (
          <section className="sbx-tips-section">
            <div className="sbx-tips-section-heading">Notices</div>
            <div className="sbx-tips-list">
              {notices.map((notice) => (
                <div
                  className="sbx-tip-row sbx-tip-row-notice"
                  data-severity={notice.severity}
                  key={notice.id}
                >
                  <span className="sbx-tip-icon">
                    <WarningGlyph />
                  </span>
                  <span className="sbx-tip-text">
                    <span className="sbx-tip-title">{notice.title}</span>
                    <span className="sbx-tip-body">{notice.body}</span>
                  </span>
                </div>
              ))}
            </div>
          </section>
        ) : null}
        {unreadTips.length > 0 ? (
          <section className="sbx-tips-section">
            <div className="sbx-tips-section-heading">Unread</div>
            <div className="sbx-tips-list">
              {unreadTips.map((tip) => (
                <div className="sbx-tip-row" data-read="false" key={tip.id}>
                  <span className="sbx-tip-unread-dot" />
                  <span className="sbx-tip-text">
                    <span className="sbx-tip-title">{tip.title}</span>
                    <span className="sbx-tip-body">{tip.body}</span>
                  </span>
                </div>
              ))}
            </div>
          </section>
        ) : null}
        <section className="sbx-tips-section">
          <div className="sbx-tips-section-heading">Read</div>
          <div className="sbx-tips-list">
            {readTips.length > 0 ? (
              readTips.map((tip) => (
                <div className="sbx-tip-row" data-read="true" key={tip.id}>
                  <span className="sbx-tip-unread-dot" data-hidden="true" />
                  <span className="sbx-tip-text">
                    <span className="sbx-tip-title">{tip.title}</span>
                    <span className="sbx-tip-body">{tip.body}</span>
                  </span>
                </div>
              ))
            ) : (
              <div className="sbx-tips-empty">No read tips yet.</div>
            )}
          </div>
        </section>
      </div>
      <div className="sbx-tips-footer">
        Visual re-mock of the production Tips panel — notices come from the simulation engine.
      </div>
    </div>
  );
}
