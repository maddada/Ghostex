import { describe, expect, test } from "vitest";
import {
  CASCADIA_CODE_TERMINAL_FONT_FAMILY,
  CROSS_PLATFORM_MONO_TERMINAL_FONT_FAMILY,
  DEFAULT_TERMINAL_FONT_PRESET,
  DROID_SANS_MONO_TERMINAL_FONT_FAMILY,
  FIRA_CODE_TERMINAL_FONT_FAMILY,
  JETBRAINS_MONO_TERMINAL_FONT_FAMILY,
  MENLO_TERMINAL_FONT_FAMILY,
  MESLO_TERMINAL_FONT_FAMILY,
  MONOSPACE_TERMINAL_FONT_FAMILY,
  TERMINAL_FONT_PRESETS,
  UI_MONOSPACE_TERMINAL_FONT_FAMILY,
  getTerminalFontFamilyForPreset,
  getTerminalFontPresetFromFontFamily,
  getGhosttyFontFamilyForPreset,
  normalizeTerminalFontPreset,
} from "./terminal-font-preset";

describe("normalizeTerminalFontPreset", () => {
  test("should keep JetBrains Mono as the default preset", () => {
    expect(normalizeTerminalFontPreset(undefined)).toBe(DEFAULT_TERMINAL_FONT_PRESET);
    expect(normalizeTerminalFontPreset("monospace")).toBe("Monospace");
    expect(normalizeTerminalFontPreset(MONOSPACE_TERMINAL_FONT_FAMILY)).toBe("Monospace");
    expect(DEFAULT_TERMINAL_FONT_PRESET).toBe("JetBrains Mono");
  });

  test("should map configured preset labels", () => {
    expect(normalizeTerminalFontPreset("UI Monospace")).toBe("UI Monospace");
    expect(normalizeTerminalFontPreset("ui-monospace")).toBe("UI Monospace");
    expect(normalizeTerminalFontPreset("Meslo")).toBe("Meslo");
    expect(normalizeTerminalFontPreset("Cross Platform Mono")).toBe("Cross Platform Mono");
    expect(normalizeTerminalFontPreset("Consolas")).toBe("Consolas (Windows Default)");
    expect(normalizeTerminalFontPreset("Monaco")).toBe("Monaco (macOS Default)");
    expect(normalizeTerminalFontPreset("Droid Sans Mono")).toBe("Droid Sans Mono (Linux Default)");
    expect(normalizeTerminalFontPreset("Fira Code")).toBe("Fira Code");
  });

  test("should map resolved font stacks back to their presets", () => {
    expect(normalizeTerminalFontPreset(UI_MONOSPACE_TERMINAL_FONT_FAMILY)).toBe("UI Monospace");
    expect(normalizeTerminalFontPreset(MESLO_TERMINAL_FONT_FAMILY)).toBe("Meslo");
    expect(normalizeTerminalFontPreset(CROSS_PLATFORM_MONO_TERMINAL_FONT_FAMILY)).toBe(
      "Cross Platform Mono",
    );
    expect(normalizeTerminalFontPreset(DROID_SANS_MONO_TERMINAL_FONT_FAMILY)).toBe(
      "Droid Sans Mono (Linux Default)",
    );
    expect(normalizeTerminalFontPreset(CASCADIA_CODE_TERMINAL_FONT_FAMILY)).toBe("Cascadia Code");
    expect(normalizeTerminalFontPreset(MENLO_TERMINAL_FONT_FAMILY)).toBe("Menlo");
  });
});

describe("terminal font preset helpers", () => {
  test("should expose only mono presets", () => {
    expect(TERMINAL_FONT_PRESETS.map((preset) => preset.preset)).toEqual([
      "Monospace",
      "UI Monospace",
      "Meslo",
      "Cross Platform Mono",
      "Consolas (Windows Default)",
      "Menlo",
      "Monaco (macOS Default)",
      "Droid Sans Mono (Linux Default)",
      "Liberation Mono",
      "DejaVu Sans Mono",
      "Courier New",
      "Cascadia Mono",
      "Cascadia Code",
      "JetBrains Mono",
      "Fira Code",
      "Source Code Pro",
      "IBM Plex Mono",
      "Roboto Mono",
      "Noto Sans Mono",
      "Ubuntu Mono",
    ]);
  });

  test("should return a fallback-backed font family for every preset", () => {
    for (const preset of TERMINAL_FONT_PRESETS) {
      const fontFamily = getTerminalFontFamilyForPreset(preset.preset);
      expect(fontFamily).toContain('"MesloLGL Nerd Font Mono"');
      expect(fontFamily).toContain("monospace");
    }
  });

  test("should detect the preset from the resolved font family", () => {
    expect(getTerminalFontPresetFromFontFamily(MONOSPACE_TERMINAL_FONT_FAMILY)).toBe("Monospace");
    expect(getTerminalFontPresetFromFontFamily(FIRA_CODE_TERMINAL_FONT_FAMILY)).toBe("Fira Code");
    expect(getTerminalFontPresetFromFontFamily(JETBRAINS_MONO_TERMINAL_FONT_FAMILY)).toBe(
      "JetBrains Mono",
    );
  });

  test("should expose a single Ghostty font family instead of a CSS stack", () => {
    expect(getGhosttyFontFamilyForPreset("JetBrains Mono")).toBe("JetBrains Mono");
    expect(getGhosttyFontFamilyForPreset("UI Monospace")).toBe("MesloLGL Nerd Font Mono");
  });
});
