import { PointerActivationConstraints } from "@dnd-kit/dom";

const SIDEBAR_REORDER_HOLD_DELAY_MS = 250;
const SIDEBAR_REORDER_HOLD_TOLERANCE_PX = 12;
export const SIDEBAR_REORDER_DISTANCE_PX = 8;
const TOUCH_SIDEBAR_REORDER_HOLD_DELAY_MS = 320;
const TOUCH_SIDEBAR_REORDER_HOLD_TOLERANCE_PX = 12;

/**
 * One pointer gesture for every reorderable sidebar surface.
 *
 * Mouse users can either hold briefly or move decisively. Touch stays
 * hold-only so scrolling cannot turn into a reorder gesture.
 */
export function getSidebarReorderActivationConstraints(
  event: Pick<PointerEvent, "pointerType">,
) {
  if (event.pointerType === "touch") {
    return [
      new PointerActivationConstraints.Delay({
        tolerance: TOUCH_SIDEBAR_REORDER_HOLD_TOLERANCE_PX,
        value: TOUCH_SIDEBAR_REORDER_HOLD_DELAY_MS,
      }),
    ];
  }

  return [
    new PointerActivationConstraints.Delay({
      tolerance: SIDEBAR_REORDER_HOLD_TOLERANCE_PX,
      value: SIDEBAR_REORDER_HOLD_DELAY_MS,
    }),
    new PointerActivationConstraints.Distance({
      value: SIDEBAR_REORDER_DISTANCE_PX,
    }),
  ];
}
