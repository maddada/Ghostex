import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const sidebarAppSource = readFileSync(new URL("./sidebar-app.tsx", import.meta.url), "utf8");
const groupPanelsCssSource = readFileSync(
  new URL("./styles/group-panels.css", import.meta.url),
  "utf8",
);

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("sidebar search source", () => {
  test("keeps top Search as an inline nav-row input when active", () => {
    /*
     * CDXC:SidebarSearch 2026-06-19-13:52:
     * The top Search button should become an in-place text input using the
     * Search label as the placeholder instead of swapping to the boxed shared
     * session search field.
     *
     * CDXC:SidebarSearch 2026-06-19-13:59:
     * The inline placeholder fades down after activation, while closing empty
     * search restores the plain Search row without a re-entry animation.
     *
     * The Search icon stays mounted in both inactive and active states so the
     * row keeps the same visual identity while its label becomes an input.
     */
    const searchItemSource = sourceBetween(
      sidebarAppSource,
      "function SidebarReferenceSearchNavItem({",
      "function SidebarReferenceNavButton({",
    );

    expect(searchItemSource).toContain("reference-sidebar-inline-search-row");
    expect(searchItemSource).toContain("reference-sidebar-inline-search-input");
    expect(searchItemSource).toContain('<span className="reference-sidebar-nav-label">Search</span>');
    expect(searchItemSource).toContain('placeholder="Search"');
    expect(searchItemSource).toContain("<IconSearch");
    expect(searchItemSource).toContain("reference-sidebar-search-icon");
    expect(searchItemSource).not.toContain("<SidebarSessionSearchField");
    expect(groupPanelsCssSource).toContain(".reference-sidebar-inline-search-input");
    expect(groupPanelsCssSource).toContain("background: transparent;");
    expect(groupPanelsCssSource).toContain("reference-sidebar-inline-search-placeholder-fade");
    expect(groupPanelsCssSource).not.toContain("reference-sidebar-search-button-enter");
    expect(groupPanelsCssSource).not.toContain(".reference-sidebar-search-field");
  });
});
