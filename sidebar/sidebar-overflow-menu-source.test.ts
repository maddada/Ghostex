import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const sidebarAppSource = readFileSync(new URL("./sidebar-app.tsx", import.meta.url), "utf8");

function sourceBetween(start: string, end: string): string {
  const startIndex = sidebarAppSource.indexOf(start);
  const endIndex = sidebarAppSource.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return sidebarAppSource.slice(startIndex, endIndex);
}

describe("sidebar overflow menu source", () => {
  test("puts Commands with the current hotkey first in the overflow menu", () => {
    /*
     * CDXC:CommandPalette 2026-06-13-10:42:
     * The sidebar overflow menu should start with Commands plus the current
     * command-palette hotkey so the app-wide command surface is discoverable
     * from the More control before secondary toggles such as Wake Pet.
     */
    const overflowMenuSource = sourceBetween(
      "function renderFloatingOverflowMenu({",
      "function resolveSessionDropTargetFromPoint",
    );

    const firstCommandPaletteIndex = overflowMenuSource.indexOf("commandPaletteMenuLabel");
    expect(sidebarAppSource).toContain("getCommandPaletteOverflowMenuLabel");
    expect(sidebarAppSource).toContain("`Commands [${hotkeyLabel}]`");
    expect(sidebarAppSource).toContain('return "CMD";');
    expect(sidebarAppSource).toContain('return "SHIFT";');
    expect(firstCommandPaletteIndex).toBeGreaterThanOrEqual(0);
    expect(firstCommandPaletteIndex).toBeLessThan(overflowMenuSource.indexOf("Wake Pet"));
    expect(firstCommandPaletteIndex).toBeLessThan(overflowMenuSource.indexOf("Pinned Prompts"));
    expect(overflowMenuSource).toContain("onOpenCommandPalette");
  });

  test("keeps the setup wizard entry stable when hooks are missing", () => {
    /*
     * CDXC:SidebarSetupWizard 2026-06-07-12:35:
     * The overflow menu's first-launch setup action is named Setup Wizard and
     * must not become a hook-install notice when agent hooks are missing.
     */
    const overflowMenuSource = sourceBetween(
      "function renderFloatingOverflowMenu({",
      "function resolveSessionDropTargetFromPoint",
    );

    expect(overflowMenuSource).toContain("Setup Wizard");
    expect(overflowMenuSource).not.toContain("hasMissingAgentHooks");
    expect(overflowMenuSource).not.toContain("sidebar-hook-warning-menu-item");
    expect(overflowMenuSource).not.toContain("Agent hooks");
  });
});
