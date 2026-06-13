import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const nativeSidebarSource = readFileSync(new URL("./native-sidebar.tsx", import.meta.url), "utf8");
const sidebarAppSource = readFileSync(new URL("../../sidebar/sidebar-app.tsx", import.meta.url), "utf8");

describe("native sidebar hotkey source", () => {
  test("keeps nativeHotkey session creation owned by the native wrapper only", () => {
    /*
     * CDXC:Hotkeys 2026-06-12-12:33:
     * Cmd+T in the macOS app should create one terminal tab. The native wrapper
     * handles typed nativeHotkey host events directly, so its embedded shared
     * SidebarApp disables the shared custom-event listener instead of issuing a
     * second createSession bridge request.
     */
    expect(nativeSidebarSource).toContain('if (hostEvent.type === "nativeHotkey") {');
    expect(nativeSidebarSource).toContain('runNativeHotkeyAction(hostEvent.actionId, "native");');
    expect(nativeSidebarSource).toContain("nativeHostEventSource={null}");
    expect(sidebarAppSource).toContain("nativeHostEventSource?: SidebarEventSource | null;");
    expect(sidebarAppSource).toContain("if (!nativeHostEventSource) {");
    expect(sidebarAppSource).toContain(
      'nativeHostEventSource.addEventListener("ghostex-native-host-event", handleNativeHostEvent);',
    );
  });

  test("resolves session slot hotkeys from the rendered sidebar list", () => {
    /*
     * CDXC:Hotkeys 2026-06-13-07:33:
     * Cmd+1..9 in the macOS app must count the visible sidebar session rows,
     * not native tab or pane chrome that also carries session IDs. This keeps
     * slot numbers aligned with the Last Active order shown in the sidebar.
     */
    expect(nativeSidebarSource).toContain(
      'document.querySelector(".native-sidebar-main .session-groups-content")',
    );
    expect(nativeSidebarSource).toContain("readRenderedSidebarSessionSlots(root)");
    expect(nativeSidebarSource).toContain(
      'logNativeHotkeyDebug("nativeHotkeys.sessionSlotRootMissing"',
    );
    expect(nativeSidebarSource).not.toContain(
      'document.querySelector(".native-sidebar-main") ?? document',
    );
  });
});
