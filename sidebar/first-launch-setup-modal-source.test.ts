import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const firstLaunchSetupModalSource = readFileSync(
  new URL("./first-launch-setup-modal.tsx", import.meta.url),
  "utf8",
);
const sidebarStylesSource = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("first launch setup modal source", () => {
  test("shows Recommended as the leftmost default sidebar-style preset", () => {
    /*
    CDXC:FirstLaunchPreferences 2026-06-13-03:28:
    The first-launch defaults page must add Recommended to the left of Minimal,
    Codex, and Detailed, and the shared default settings should keep that
    preset selected on new installs.
    */
    const presetOrder = sourceBetween(
      firstLaunchSetupModalSource,
      "const FIRST_LAUNCH_SIDEBAR_PRESET_ORDER",
      "const FIRST_LAUNCH_SIDEBAR_PRESETS",
    );

    expect(presetOrder).toMatch(/"recommended",\s*"minimal",\s*"codex",\s*"detailed"/u);

    const presetOptionsStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .first-launch-setup-preset-options {",
      ".ghostex-settings-shadcn .first-launch-setup-preset-button {",
    );
    expect(presetOptionsStyles).toContain("grid-template-columns: repeat(4, minmax(0, 1fr));");
  });

  test("keeps defaults-page checkbox controls square", () => {
    /*
    CDXC:FirstLaunchPreferences 2026-06-13-05:27:
    The first-time defaults modal should show square checkbox controls instead
    of native macOS rounded checkboxes while keeping the shape scoped to the
    onboarding preference tiles.
    */
    const checkboxStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-settings-shadcn .first-launch-setup-checkbox {",
      ".ghostex-settings-shadcn .first-launch-setup-benefit + .first-launch-setup-benefit {",
    );

    expect(checkboxStyles).toContain("appearance: none;");
    expect(checkboxStyles).toContain("border-radius: 0;");
    expect(checkboxStyles).toContain(
      ".ghostex-settings-shadcn .first-launch-setup-checkbox:checked::after",
    );
  });
});
