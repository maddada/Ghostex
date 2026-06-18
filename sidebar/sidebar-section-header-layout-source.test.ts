import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const sidebarAppSource = readFileSync(new URL("./sidebar-app.tsx", import.meta.url), "utf8");
const groupPanelsSource = readFileSync(new URL("./styles/group-panels.css", import.meta.url), "utf8");

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("reference sidebar section header layout source", () => {
  test("collapses Quick and Projects labels when hover actions become visible", () => {
    /*
     * CDXC:SidebarHeaderActions 2026-06-17-23:21:
     * Quick and Projects section labels should shorten like Search instead of
     * painting underneath their hover action buttons in the narrow native
     * sidebar.
     */
    expect(sidebarAppSource).toContain('<span className="reference-sidebar-section-title">{title}</span>');

    const sectionRowRule = sourceBetween(
      groupPanelsSource,
      ".reference-sidebar-section-row {",
      ".reference-sidebar-section-row[data-reference-section=\"projects\"]",
    );
    expect(sectionRowRule).toContain("--reference-sidebar-section-actions-max-width: 132px;");
    expect(sectionRowRule).toContain("CDXC:SidebarHeaderActions 2026-06-17-23:21");

    const titleRule = sourceBetween(
      groupPanelsSource,
      ".reference-sidebar-section-title {",
      ".reference-sidebar-section-chevron",
    );
    expect(titleRule).toContain("min-width: 0;");
    expect(titleRule).toContain("overflow: hidden;");
    expect(titleRule).toContain("text-overflow: ellipsis;");
    expect(titleRule).toContain("white-space: nowrap;");

    const hiddenActionsRule = sourceBetween(
      groupPanelsSource,
      ".reference-sidebar-section-actions {",
      ".sidebar-reference-layout[data-reference-sidebar=\"true\"]\n  .reference-sidebar-section-actions",
    );
    expect(hiddenActionsRule).toContain("max-width: 0;");
    expect(hiddenActionsRule).toContain("overflow: hidden;");

    const visibleActionsRule = sourceBetween(
      groupPanelsSource,
      ".reference-sidebar-section-row:hover .reference-sidebar-section-actions,",
      "\n\n.reference-sidebar-section-action {",
    );
    expect(visibleActionsRule).toContain(
      "max-width: var(--reference-sidebar-section-actions-max-width);",
    );
    expect(visibleActionsRule).toContain("overflow: visible;");
  });
});
