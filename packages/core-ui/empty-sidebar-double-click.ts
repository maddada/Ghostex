export const SIDEBAR_EMPTY_SPACE_BLOCKER_SELECTOR = [
  "button",
  "input",
  "select",
  "textarea",
  "a",
  "[role='button']",
  "[role='menu']",
  "[role='menuitem']",
  "[data-empty-space-blocking='true']",
].join(", ");

export type EmptySidebarDoubleClickEvent = {
  clientX: number;
  clientY: number;
  currentTarget: HTMLElement;
  target: EventTarget | null;
};

export function isEmptySidebarDoubleClick(event: EmptySidebarDoubleClickEvent): boolean {
  const { clientX, clientY, currentTarget, target } = event;

  if (typeof document.elementsFromPoint === "function") {
    const elementsAtPoint = document.elementsFromPoint(clientX, clientY);
    let sawSidebarRoot = false;

    for (const element of elementsAtPoint) {
      if (!currentTarget.contains(element) && element !== currentTarget) {
        continue;
      }

      sawSidebarRoot = sawSidebarRoot || element === currentTarget;

      if (element.closest(SIDEBAR_EMPTY_SPACE_BLOCKER_SELECTOR) !== null) {
        return false;
      }

      if (element === currentTarget) {
        return true;
      }
    }

    if (sawSidebarRoot) {
      return true;
    }
  }

  if (target === currentTarget) {
    return true;
  }

  const targetNode = target instanceof Node ? target : undefined;
  const targetElement =
    targetNode instanceof Element ? targetNode : (targetNode?.parentElement ?? undefined);
  if (!targetElement || !currentTarget.contains(targetElement)) {
    return false;
  }

  return targetElement.closest(SIDEBAR_EMPTY_SPACE_BLOCKER_SELECTOR) === null;
}
