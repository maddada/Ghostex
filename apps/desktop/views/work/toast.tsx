import { IconX } from "@tabler/icons-react";
import { useEffect } from "react";
import { cx } from "./components";

export interface WorkToastState {
  /** A new id restarts the timer for a toast with the same text. */
  id: number;
  text: string;
  tone?: "error" | "warning";
  action?: { label: string; run: () => void };
}

/** How long a toast stays; one with an action (Undo) stays longer. */
const TOAST_MS = 6_000;
const ACTION_TOAST_MS = 10_000;

/** The page's one toast, at the bottom of the view: a link's Undo, a cloud start, an error. */
export function WorkToast({
  toast,
  onClose,
}: {
  toast: WorkToastState | null;
  onClose: () => void;
}) {
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(
      onClose,
      toast.action ? ACTION_TOAST_MS : TOAST_MS,
    );
    return () => window.clearTimeout(timer);
  }, [toast, onClose]);
  if (!toast) return null;
  return (
    <div
      className={cx("w-toast", toast.tone && `is-${toast.tone}`)}
      role="status"
    >
      <span className="w-toast-text">{toast.text}</span>
      {toast.action ? (
        <button
          type="button"
          className="w-toast-action"
          onClick={toast.action.run}
        >
          {toast.action.label}
        </button>
      ) : null}
      <button
        type="button"
        className="w-toast-close"
        title="Close"
        onClick={onClose}
      >
        <IconX size={13} />
      </button>
    </div>
  );
}
