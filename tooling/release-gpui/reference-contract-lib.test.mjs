import { describe, expect, test } from "vitest";
import {
  extractManagedTooltipPlacements,
  extractManagedTooltipPlacementUsages,
  extractPublicRustMethods,
  missingManagedTooltipPlacements,
  missingRequiredRustMethods,
} from "./reference-contract-lib.mjs";

describe("release GPUI reference contract", () => {
  test("extracts defined and used managed tooltip placements", () => {
    const library = `
pub enum ManagedTooltipPlacement {
    #[default]
    Auto,
    Left,
    BelowLeft,
}
`;
    const application = `
let first = ManagedTooltipPlacement::Left;
let second = ManagedTooltipPlacement::Below;
`;

    expect([...extractManagedTooltipPlacements(library)]).toEqual([
      "Auto",
      "Left",
      "BelowLeft",
    ]);
    expect([...extractManagedTooltipPlacementUsages(application)]).toEqual([
      "Left",
      "Below",
    ]);

    const { missing } = missingManagedTooltipPlacements(library, [
      { path: "gpui/src/main.rs", source: application },
    ]);
    expect([...missing]).toEqual([["Below", ["gpui/src/main.rs"]]]);
  });

  test("detects missing patched builder methods", () => {
    const library = `
impl PopupMenu {
    pub fn items_padding_bottom(self, value: Pixels) -> Self { self }
    pub fn scrollbar_show(self, value: ScrollbarShow) -> Self { self }
}
`;
    expect([...extractPublicRustMethods(library)]).toEqual(["items_padding_bottom", "scrollbar_show"]);
    expect(
      missingRequiredRustMethods(library, ["items_padding_bottom", "scrollbar_show", "scrollbar_thickness"])
        .missing,
    ).toEqual(["scrollbar_thickness"]);
  });
});
