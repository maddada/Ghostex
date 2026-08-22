import { describe, expect, test } from "vitest";

import { formatSidebarHotkeyLabel } from "./hotkey-label";

describe("formatSidebarHotkeyLabel", () => {
  test("renders compact Mac shortcut labels without plus separators", () => {
    /*
     * CDXC:Hotkeys 2026-06-14-19:40:
     * The Cmd-hold overlay should show native-feeling shortcut labels such as
     * `⌘L`, not literal-plus labels such as `⌘+L`.
     */
    expect(formatSidebarHotkeyLabel("cmd+l")).toBe("⌘L");
    expect(formatSidebarHotkeyLabel("cmd+shift+p")).toBe("⌘⇧P");
    expect(formatSidebarHotkeyLabel("ctrl+shift+tab")).toBe("⌃⇧Tab");
    expect(formatSidebarHotkeyLabel("f12")).toBe("F12");
  });
});
