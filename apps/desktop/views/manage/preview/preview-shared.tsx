import { type ReactNode } from "react";

export function ManagePreviewMessage({ icon, title }: { icon: ReactNode; title: string }) {
  return (
    <div className="manage-preview-message">
      {icon}
      <span>{title}</span>
    </div>
  );
}

export function isEditableEventTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }
  if (target.matches("input, textarea, select, [contenteditable='true']")) {
    return true;
  }
  return Boolean(target.closest("input, textarea, select, [contenteditable='true']"));
}
