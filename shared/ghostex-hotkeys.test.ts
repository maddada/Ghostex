import { describe, expect, test } from "vitest";
import {
  DEFAULT_ghostex_HOTKEYS,
  getghostexHotkeyActionIdForKey,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
} from "./ghostex-hotkeys";

describe("normalizeghostexHotkeySettings", () => {
  test("uses default navigation hotkeys without stealing command-arrow editing", () => {
    /**
     * CDXC:Hotkeys 2026-05-15-13:31:
     * Directional pane focus must avoid plain Cmd+Arrow so terminal prompts
     * and modal prompt editors keep native text navigation.
     */
    expect(DEFAULT_ghostex_HOTKEYS.createSession).toBe("cmd+t");
    /**
     * CDXC:CommandPalette 2026-06-13-10:26:
     * Cmd+Shift+P should remain the shared default for the command palette so
     * users see the same binding in Settings, the palette, and terminal-focused
     * native dispatch.
     */
    expect(DEFAULT_ghostex_HOTKEYS.openCommandPalette).toBe("cmd+shift+p");
    /**
     * CDXC:Hotkeys 2026-05-14-08:09:
     * The Commands panel must remain bound to bare F12 so terminal-focused AppKit dispatch and sidebar-focused DOM dispatch agree on the same user shortcut.
     */
    expect(DEFAULT_ghostex_HOTKEYS.openCommandsPanel).toBe("f12");
    expect(DEFAULT_ghostex_HOTKEYS.openSettings).toBe("cmd+,");
    /**
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * Cmd+B is the complete sidebar collapse toggle. Moving the sidebar between
     * left and right remains available as an explicit command, but it starts
     * without a default shortcut.
     */
    expect(DEFAULT_ghostex_HOTKEYS.toggleSidebarCollapsed).toBe("cmd+b");
    expect(DEFAULT_ghostex_HOTKEYS.moveSidebar).toBe("");
    expect(DEFAULT_ghostex_HOTKEYS.focusPreviousSession).toBe("cmd+shift+tab");
    expect(DEFAULT_ghostex_HOTKEYS.focusNextSession).toBe("cmd+tab");
    expect(DEFAULT_ghostex_HOTKEYS.focusPreviousGroup).toBe("cmd+[");
    expect(DEFAULT_ghostex_HOTKEYS.focusNextGroup).toBe("cmd+]");
    expect(DEFAULT_ghostex_HOTKEYS.focusLeft).toBe("cmd+alt+left");
    expect(DEFAULT_ghostex_HOTKEYS.focusRight).toBe("cmd+alt+right");
    expect(DEFAULT_ghostex_HOTKEYS.focusUp).toBe("cmd+alt+up");
    expect(DEFAULT_ghostex_HOTKEYS.focusDown).toBe("cmd+alt+down");
    /**
     * CDXC:CommandPalette 2026-05-17-01:32:
     * Focused pane-menu actions should be configurable hotkeys so the command
     * palette can expose the same actions users see in pane chrome.
     */
    expect(DEFAULT_ghostex_HOTKEYS.openBrowserPane).toBe("cmd+n");
    expect(DEFAULT_ghostex_HOTKEYS.rotatePanesClockwise).toBe("ctrl+shift+l");
    expect(DEFAULT_ghostex_HOTKEYS.mergeAllTabs).toBe("ctrl+shift+m");
    expect(DEFAULT_ghostex_HOTKEYS.delayedSend).toBe("ctrl+shift+s");
    expect(DEFAULT_ghostex_HOTKEYS.forkSession).toBe("ctrl+shift+f");
    expect(DEFAULT_ghostex_HOTKEYS.reloadSession).toBe("ctrl+shift+r");
    expect(DEFAULT_ghostex_HOTKEYS.popOutPane).toBe("ctrl+shift+o");
    expect(DEFAULT_ghostex_HOTKEYS.focusGroup1).toBe("cmd+ctrl+1");
    expect(DEFAULT_ghostex_HOTKEYS.focusGroup5).toBe("cmd+ctrl+5");
    expect(DEFAULT_ghostex_HOTKEYS.focusSessionSlot1).toBe("cmd+1");
    expect(DEFAULT_ghostex_HOTKEYS.switchAgentsView).toBe("alt+1");
    expect(DEFAULT_ghostex_HOTKEYS.switchSourceView).toBe("alt+2");
    expect(DEFAULT_ghostex_HOTKEYS.switchGitHubView).toBe("alt+3");
    expect(DEFAULT_ghostex_HOTKEYS.switchKanbanView).toBe("alt+4");
    /**
     * CDXC:ActionsHotkeys 2026-05-17-01:18:
     * Action launch hotkeys are positional so Settings can bind the first five
     * Actions list rows without coupling shortcuts to command ids.
     */
    expect(DEFAULT_ghostex_HOTKEYS.runActionSlot1).toBe("ctrl+shift+1");
    expect(DEFAULT_ghostex_HOTKEYS.runActionSlot5).toBe("ctrl+shift+5");
  });

  test("matches positional action hotkeys", () => {
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "ctrl+shift+1")).toBe(
      "runActionSlot1",
    );
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "ctrl+shift+5")).toBe(
      "runActionSlot5",
    );
  });

  test("matches WebKit shifted digit characters for positional action hotkeys", () => {
    /**
     * CDXC:ActionsHotkeys 2026-05-26-13:21:
     * Ctrl+Shift+1 should launch action slot 1 even when the sidebar DOM path
     * receives KeyboardEvent.key as "!" instead of the unshifted digit AppKit
     * uses for the same physical shortcut.
     */
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "ctrl+shift+!")).toBe(
      "runActionSlot1",
    );
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "ctrl+shift+%")).toBe(
      "runActionSlot5",
    );
  });

  test("matches focused pane action hotkeys", () => {
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "cmd+n")).toBe(
      "openBrowserPane",
    );
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "ctrl+shift+o")).toBe(
      "popOutPane",
    );
  });

  test("matches Cmd+B to the sidebar collapse toggle", () => {
    /**
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * The default Cmd+B action should collapse or expand the sidebar, not switch
     * the sidebar placement side.
     */
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "cmd+b")).toBe(
      "toggleSidebarCollapsed",
    );
  });

  test("does not assign Cmd+Q to configurable app hotkeys by default", () => {
    /**
     * CDXC:MacQuit 2026-06-12-03:09:
     * Cmd+Q must remain the native app quit shortcut. No configurable Ghostex
     * hotkey should claim it by default; any previous default owner would need
     * to start unassigned so the native app command wins.
     */
    expect(Object.values(DEFAULT_ghostex_HOTKEYS)).not.toContain("cmd+q");
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "cmd+q")).toBeUndefined();
  });

  test("matches workarea view switcher hotkeys", () => {
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "alt+1")).toBe(
      "switchAgentsView",
    );
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "alt+2")).toBe(
      "switchSourceView",
    );
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "alt+3")).toBe(
      "switchGitHubView",
    );
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "alt+4")).toBe(
      "switchKanbanView",
    );
  });

  test("keeps browser bracket tab navigation as alternate defaults", () => {
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "cmd+shift+[")).toBe(
      "focusPreviousSession",
    );
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "cmd+shift+]")).toBe(
      "focusNextSession",
    );
  });

  test("does not match alternate defaults after clearing the primary hotkey", () => {
    const hotkeys = normalizeghostexHotkeySettings({
      focusNextSession: "",
      focusPreviousSession: "",
    });

    expect(getghostexHotkeyActionIdForKey(hotkeys, "cmd+shift+[")).toBeUndefined();
    expect(getghostexHotkeyActionIdForKey(hotkeys, "cmd+shift+]")).toBeUndefined();
  });

  test("migrates persisted old navigation defaults", () => {
    expect(
      normalizeghostexHotkeySettings({
        focusNextGroup: "cmd+shift+]",
        focusNextSession: "cmd+]",
        focusLeft: "cmd+left",
        focusPreviousGroup: "cmd+shift+[",
        focusPreviousSession: "cmd+[",
        focusRight: "cmd+right",
        createSession: "cmd+n",
        moveSidebar: "cmd+b",
        openCommandPalette: "cmd+k",
        openBrowserPane: "ctrl+shift+b",
      }),
    ).toMatchObject({
      createSession: "cmd+t",
      focusLeft: "cmd+alt+left",
      focusNextGroup: "cmd+]",
      focusNextSession: "cmd+tab",
      focusPreviousGroup: "cmd+[",
      focusPreviousSession: "cmd+shift+tab",
      focusRight: "cmd+alt+right",
      moveSidebar: "",
      openBrowserPane: "cmd+n",
      openCommandPalette: "cmd+shift+p",
      toggleSidebarCollapsed: "cmd+b",
    });
  });

  test("uses direct split creation defaults", () => {
    /**
     * CDXC:NativeSplits 2026-05-10-18:30
     * Cmd+D and Cmd+Shift+D create real terminal splits in the native
     * workspace.
     */
    expect(DEFAULT_ghostex_HOTKEYS.splitMore).toBe("cmd+d");
    expect(DEFAULT_ghostex_HOTKEYS.splitMoreDown).toBe("cmd+shift+d");
  });

  test("drops retired visible-count hotkeys during normalization", () => {
    expect(
      normalizeghostexHotkeySettings({
        showOne: "cmd+alt+s 1",
        showTwo: "cmd+alt+s 2",
        showThree: "cmd+alt+s 3",
      }),
    ).not.toHaveProperty("showOne");
  });

  test("keeps explicitly cleared hotkeys unassigned", () => {
    /**
     * CDXC:Hotkeys 2026-05-11-09:06
     * Clearing a binding from Settings must persist as an unassigned command,
     * while omitted settings continue to receive defaults.
     */
    expect(
      normalizeghostexHotkeySettings({
        splitMoreDown: "",
      }),
    ).toMatchObject({
      splitMore: "cmd+d",
      splitMoreDown: "",
    });
  });
});

describe("normalizeHotkeyText", () => {
  test("accepts TanStack recorder Mod output", () => {
    expect(normalizeHotkeyText("Mod+Alt+1")).toBe("cmd+alt+1");
  });

  test("normalizes shifted digit glyphs to physical digit hotkeys", () => {
    expect(normalizeHotkeyText("Ctrl+Shift+!")).toBe("ctrl+shift+1");
    expect(normalizeHotkeyText("Cmd+Ctrl+Shift+%")).toBe("cmd+ctrl+shift+5");
  });

  test("normalizes shifted bracket glyphs to physical bracket hotkeys", () => {
    expect(normalizeHotkeyText("Cmd+Shift+{")).toBe("cmd+shift+[");
    expect(normalizeHotkeyText("Cmd+Shift+}")).toBe("cmd+shift+]");
    expect(getghostexHotkeyActionIdForKey(DEFAULT_ghostex_HOTKEYS, "cmd+shift+}")).toBe(
      "focusNextSession",
    );
  });
});
