import { describe, expect, test } from "vitest";
import { getSidebarContextMenuBackdropRetarget } from "./sidebar-context-menu-portal";

function createBackdrop() {
  const containedTarget = { role: "contained" } as Element;
  const backdrop = {
    contains: (target: Element) => target === containedTarget,
    style: {
      pointerEvents: "auto",
    },
  } as unknown as HTMLElement;

  return { backdrop, containedTarget };
}

describe("getSidebarContextMenuBackdropRetarget", () => {
  test("temporarily removes backdrop pointer targeting while finding the row underneath", () => {
    const { backdrop } = createBackdrop();
    const rowTarget = { role: "session-row" } as Element;
    const pointerEventsDuringLookup: string[] = [];

    const retarget = getSidebarContextMenuBackdropRetarget({
      backdrop,
      clientX: 42,
      clientY: 96,
      elementFromPoint: (x, y) => {
        pointerEventsDuringLookup.push(backdrop.style.pointerEvents);
        expect({ x, y }).toEqual({ x: 42, y: 96 });
        return rowTarget;
      },
    });

    expect(retarget).toBe(rowTarget);
    expect(pointerEventsDuringLookup).toEqual(["none"]);
    expect(backdrop.style.pointerEvents).toBe("auto");
  });

  test("does not retarget context menus back into the existing backdrop", () => {
    const { backdrop, containedTarget } = createBackdrop();

    expect(
      getSidebarContextMenuBackdropRetarget({
        backdrop,
        clientX: 1,
        clientY: 2,
        elementFromPoint: () => backdrop,
      }),
    ).toBeUndefined();

    expect(
      getSidebarContextMenuBackdropRetarget({
        backdrop,
        clientX: 1,
        clientY: 2,
        elementFromPoint: () => containedTarget,
      }),
    ).toBeUndefined();
  });
});
