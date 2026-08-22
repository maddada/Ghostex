import { describe, expect, test } from "vitest";
import { getSidebarFixedTooltipPosition } from "./sidebar-fixed-tooltip-button";

const tooltipRect = {
  bottom: 0,
  height: 30,
  left: 0,
  right: 120,
  top: 0,
  width: 120,
};

describe("getSidebarFixedTooltipPosition", () => {
  test("flips bottom tooltips above triggers near the lower sidebar boundary", () => {
    const position = getSidebarFixedTooltipPosition({
      align: "end",
      tooltipRect,
      triggerRect: {
        bottom: 282,
        height: 22,
        left: 456,
        right: 478,
        top: 260,
        width: 22,
      },
      viewportHeight: 300,
      viewportWidth: 500,
    });

    expect(position.side).toBe("top");
    expect(position.top).toBe(222);
    expect(position.left).toBe(358);
  });

  test("falls back from preferred left to a visible side in narrow viewports", () => {
    const position = getSidebarFixedTooltipPosition({
      preferredSide: "left",
      tooltipRect,
      triggerRect: {
        bottom: 80,
        height: 22,
        left: 16,
        right: 38,
        top: 58,
        width: 22,
      },
      viewportHeight: 240,
      viewportWidth: 150,
    });

    expect(position.side).toBe("bottom");
    expect(position.left).toBe(8);
    expect(position.maxWidth).toBe(134);
  });
});
