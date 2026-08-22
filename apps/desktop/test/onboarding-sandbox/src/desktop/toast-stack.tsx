/*
 * Toast stack mirroring the GPUI app toasts (top-right of the app window).
 * Auto-dismiss is the engine's job (SimToast.autoDismissMs); this component
 * only renders and offers manual dismissal.
 */
import { useSandboxStore } from "../state/store";
import { CloseGlyph, InfoCircleGlyph, WarningGlyph } from "./icons";
import "./toast-stack.css";

export function ToastStack() {
  const toasts = useSandboxStore((s) => s.toasts);
  const dismissToast = useSandboxStore((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div className="sbx-toast-stack">
      {toasts.map((toast) => (
        <div className="sbx-toast" data-kind={toast.kind} key={toast.id}>
          <span className="sbx-toast-icon">
            {toast.kind === "info" ? <InfoCircleGlyph size={15} /> : <WarningGlyph size={15} />}
          </span>
          <span className="sbx-toast-text">
            <span className="sbx-toast-title">{toast.title}</span>
            {toast.message ? <span className="sbx-toast-message">{toast.message}</span> : null}
            {toast.autoDismissMs === null ? (
              <span className="sbx-toast-sticky">stays until dismissed</span>
            ) : null}
          </span>
          <button
            aria-label={`Dismiss ${toast.title}`}
            className="sbx-toast-close"
            onClick={() => dismissToast(toast.id)}
            type="button"
          >
            <CloseGlyph />
          </button>
        </div>
      ))}
    </div>
  );
}
