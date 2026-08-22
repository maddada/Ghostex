import { describe, expect, test } from "vitest";
import { DEFAULT_ghostex_SETTINGS } from "./ghostex-settings";
import { getGhosttyTerminalConfigValues } from "./ghostty-terminal-settings";

describe("getGhosttyTerminalConfigValues", () => {
  test("maps ghostex terminal settings to documented Ghostty config values", () => {
    expect(
      getGhosttyTerminalConfigValues({
        ...DEFAULT_ghostex_SETTINGS,
        terminalFontFamily: "JetBrains Mono",
        terminalFontSize: 13,
        terminalFontWeight: 650,
        terminalClipboardPasteProtection: false,
        terminalClipboardTrimTrailingSpaces: false,
        terminalPastePreviewableImages: false,
        terminalConfirmCloseSurface: "always",
        terminalCopyOnSelect: "clipboard",
        terminalCursorStyleBlink: false,
        terminalGhosttyTheme: "GitHub Dark Default",
        terminalLetterSpacing: 0.5,
        terminalLineHeight: 1.1,
        terminalMouseHideWhileTyping: true,
        terminalMouseScrollMultiplierDiscrete: 4,
        terminalMouseScrollMultiplierPrecision: 0.75,
        terminalScrollbackLimitMb: 25,
        terminalScrollbar: "never",
      }),
    ).toEqual({
      adjustCellHeightPercent: 0.10000000000000009,
      adjustCellWidth: 0.5,
      clipboardPasteProtection: false,
      clipboardTrimTrailingSpaces: false,
      pastePreviewableImages: false,
      confirmCloseSurface: "always",
      copyOnSelect: "clipboard",
      cursorStyle: "bar",
      cursorStyleBlink: false,
      fontFamily: "JetBrains Mono",
      fontSize: 13,
      fontVariationWeight: 650,
      ghosttyTheme: "GitHub Dark Default",
      mouseHideWhileTyping: true,
      mouseScrollMultiplierDiscrete: 4,
      mouseScrollMultiplierPrecision: 0.75,
      scrollbackLimitBytes: 25000000,
      scrollbar: "never",
    });
  });

  test("leaves Ghostty font weight unmanaged at the normal slider value", () => {
    /**
     * CDXC:TerminalTypographySettings 2026-04-29-09:32
     * A 400 weight is the UI's normal-weight value, so ghostex does not write a
     * font-variation wght value unless the user moves the slider away from it.
     */
    expect(
      getGhosttyTerminalConfigValues({
        ...DEFAULT_ghostex_SETTINGS,
        terminalFontWeight: 400,
      }).fontVariationWeight,
    ).toBeNull();
  });
});
