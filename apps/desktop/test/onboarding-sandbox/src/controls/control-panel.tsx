/*
 * Inspector drawer: the sandbox's control surface. Everything here drives the
 * store (never the engine directly) so the desktop, the modal windows and this
 * panel always agree on one simulation state. See SPEC.md "Control panel".
 */
import { EnvironmentSection } from "./environment-section";
import { EventLogSection } from "./event-log-section";
import { LifecycleSection } from "./lifecycle-section";
import { ModalGallerySection } from "./modal-gallery-section";
import { PresetsSection } from "./presets-section";
import { StateFileSection } from "./state-file-section";
import { usePersistedState } from "./controls-storage";
import "./controls.css";
import "./event-log.css";

export function ControlPanel() {
  const [collapsed, setCollapsed] = usePersistedState("panel.collapsed", false);

  if (collapsed) {
    return (
      <aside className="cp cp--collapsed">
        <button
          className="cp-expand"
          onClick={() => setCollapsed(false)}
          title="Show the inspector"
          type="button"
        >
          <span className="cp-expand-caret">‹</span>
          <span className="cp-expand-text">Inspector</span>
        </button>
      </aside>
    );
  }

  return (
    <aside className="cp">
      <header className="cp-header">
        <span className="cp-header-title">Onboarding inspector</span>
        <button
          className="cp-collapse"
          onClick={() => setCollapsed(true)}
          title="Collapse the inspector"
          type="button"
        >
          ›
        </button>
      </header>
      <div className="cp-scroll">
        <LifecycleSection />
        <PresetsSection />
        <EnvironmentSection />
        <StateFileSection />
        <ModalGallerySection />
      </div>
      <EventLogSection />
    </aside>
  );
}
