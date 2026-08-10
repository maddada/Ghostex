import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const titlebarHostSource = readFileSync(new URL("./titlebar-host.tsx", import.meta.url), "utf8");

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("titlebar mode active state source", () => {
  test("uses an instant active pill while preserving the commented Motion restore point", () => {
    /*
     * CDXC:ModeSwitcher 2026-06-15-20:07:
     * Agents/Source/Browser/Kanban/Docs clicks should mark the clicked titlebar tab
     * active immediately. Keep the old Motion spring commented in source so it
     * can be restored without re-discovering the previous tuning.
     */
    expect(titlebarHostSource).not.toMatch(/^import \{ motion \} from "motion\/react";$/m);
    expect(titlebarHostSource).toContain('Previous Motion wiring:\n* import { motion } from "motion/react";');
    expect(titlebarHostSource).not.toMatch(/^const TITLEBAR_MODE_PILL_TRANSITION =/m);
    expect(titlebarHostSource).toContain("const TITLEBAR_MODE_PILL_TRANSITION = {");
    expect(titlebarHostSource).toContain('<span aria-hidden="true" className="titlebar-mode-tab-active" />');
    expect(titlebarHostSource).not.toMatch(/^\s*<motion\.div$/m);
    expect(titlebarHostSource).toContain("*   <motion.div");
    expect(titlebarHostSource).not.toContain('transition={{ type: "spring", bounce: 0.3, duration: 0.6 }}');
  });

  test("shows right-side disabled reasons for project-only mode tabs", () => {
    /*
     * CDXC:ModeSwitcher 2026-06-16-16:00:
     * Disabled Browser, Kanban, and Docs project-only tabs need a right-side
     * AppTooltip explaining that Quick-session users must switch to a project
     * before opening those project views. The buttons must stay hoverable, so
     * do not use native disabled on the visible mode-switcher tabs.
     */
    const visibleModeSwitcherSource = sourceBetween(
      titlebarHostSource,
      "function TitlebarModeSwitcher({",
      "function parseSharedSettings(candidate: unknown): unknown",
    );

    expect(titlebarHostSource).toContain('"Switch to a project to access this view"');
    expect(visibleModeSwitcherSource).toContain(
      "content={mode.disabled ? mode.disabledReason : undefined}",
    );
    expect(visibleModeSwitcherSource).toContain('side="right"');
    expect(visibleModeSwitcherSource).toContain("if (mode.disabled) {\n                return;\n              }");
    expect(visibleModeSwitcherSource).not.toContain("disabled={mode.disabled}");
  });

  test("shows Docs in the titlebar without debugging or beta gating", () => {
    /*
     * CDXC:TitlebarManage 2026-06-28-06:16:
     * Manage is no longer marked as beta or hidden behind Debugging Mode.
     * Keep it in the macOS titlebar mode switcher and compact mode dropdown
     * while preserving the Quick-session disabled state.
     *
     * CDXC:TitlebarDocs 2026-06-28-06:24:
     * The same manage-mode slot should render as Docs in titlebar chrome while
     * retaining the internal mode id for persisted pane compatibility.
     */
    const titlebarModesSource = sourceBetween(
      titlebarHostSource,
      "const configuredTitlebarModes = [",
      "const resolveTitlebarDropdownPanelSize = useCallback",
    );

    expect(titlebarHostSource).not.toContain("const showManageTitlebarMode =");
    expect(titlebarModesSource).not.toContain("projectState.debuggingMode");
    expect(titlebarModesSource).not.toContain("projectState.showBetaFeatures");
    expect(titlebarModesSource).not.toContain("...(showManageTitlebarMode");
    expect(titlebarModesSource).toContain('label: "Docs"');
    expect(titlebarModesSource).toContain("disabled: manageModeDisabledReason !== undefined");
    expect(titlebarModesSource).toContain("value: \"manage\" as const");
  });
});
