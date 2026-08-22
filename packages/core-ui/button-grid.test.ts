import { expect, test } from "vitest";
import { getSidebarButtonGridColumnCount } from "./button-grid";

test("should keep at least one sidebar button grid column", () => {
  expect(getSidebarButtonGridColumnCount(0)).toBe(1);
});

test("should match the number of buttons until the shared max is reached", () => {
  expect(getSidebarButtonGridColumnCount(1)).toBe(1);
  expect(getSidebarButtonGridColumnCount(3)).toBe(3);
  expect(getSidebarButtonGridColumnCount(5)).toBe(5);
});

test("should cap sidebar button grids at five columns", () => {
  expect(getSidebarButtonGridColumnCount(6)).toBe(5);
  expect(getSidebarButtonGridColumnCount(10)).toBe(5);
});
