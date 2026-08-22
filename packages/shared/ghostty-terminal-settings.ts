import type { ghostexSettings } from "./ghostex-settings";

export type GhosttyTerminalConfigValues = {
  adjustCellHeightPercent: number;
  adjustCellWidth: number;
  cursorStyle: string;
  fontFamily: string;
  fontSize: number;
  fontVariationWeight: number | null;
  clipboardPasteProtection: boolean;
  clipboardTrimTrailingSpaces: boolean;
  pastePreviewableImages: boolean;
  confirmCloseSurface: string;
  copyOnSelect: string;
  cursorStyleBlink: boolean;
  ghosttyTheme: string;
  mouseHideWhileTyping: boolean;
  mouseScrollMultiplierDiscrete: number;
  mouseScrollMultiplierPrecision: number;
  scrollbackLimitBytes: number;
  scrollbar: string;
};

/**
 * CDXC:TerminalSettings 2026-04-26-19:02
 * Ghostty does not expose a CSS-style font-weight setting. ghostex maps the
 * weight slider to the documented variable-font `font-variation = wght=...`
 * key, and writes line height through adjust-cell-height.
 *
 * CDXC:TerminalScrollSettings 2026-04-29-08:56
 * Mouse wheel speed is a native Ghostty setting, not a ghostex event transform.
 * Emit precision and discrete mouse-scroll-multiplier values so Ghostty starts
 * in the requested scroll mode and external Ghostty windows share the setting.
 *
 * CDXC:TerminalBehaviorSettings 2026-04-29-09:32
 * High-use Ghostty preferences are mapped one-to-one into documented Ghostty
 * config keys. Scrollback is stored in MB in ghostex settings but written as
 * bytes because Ghostty's scrollback-limit key is byte-based.
 *
 * CDXC:TerminalTypographySettings 2026-04-29-09:32
 * Font family is already normalized as a Ghostty font-family string. An empty
 * value intentionally means the native config merge should not manage
 * font-family, preserving the user's current Ghostty config or platform
 * default.
 *
 * CDXC:GhosttyDefaults 2026-05-22-12:29:
 * New Ghostex terminals should generate the requested GitHub Dark baseline with
 * black/white colors, a bar cursor, JetBrains Mono 13pt at wght=300, 20% cell
 * height expansion, 15 MB scrollback, protected clipboard behavior, no
 * copy-on-select, and one-to-one precision/discrete mouse scroll multipliers.
 *
 * CDXC:TerminalImagePaste 2026-06-08-13:32:
 * The native terminal settings sync also carries app runtime flags. Paste
 * previewable images must reach macOS immediately, but it must not be emitted as
 * a Ghostty config key.
 */
export function getGhosttyTerminalConfigValues(
  settings: ghostexSettings,
): GhosttyTerminalConfigValues {
  return {
    adjustCellHeightPercent: settings.terminalLineHeight - 1,
    adjustCellWidth: settings.terminalLetterSpacing,
    cursorStyle: settings.terminalCursorStyle,
    fontFamily: settings.terminalFontFamily.trim(),
    fontSize: settings.terminalFontSize,
    fontVariationWeight: settings.terminalFontWeight === 400 ? null : settings.terminalFontWeight,
    clipboardPasteProtection: settings.terminalClipboardPasteProtection,
    clipboardTrimTrailingSpaces: settings.terminalClipboardTrimTrailingSpaces,
    pastePreviewableImages: settings.terminalPastePreviewableImages,
    confirmCloseSurface: settings.terminalConfirmCloseSurface,
    copyOnSelect: settings.terminalCopyOnSelect,
    cursorStyleBlink: settings.terminalCursorStyleBlink,
    ghosttyTheme: settings.terminalGhosttyTheme,
    mouseHideWhileTyping: settings.terminalMouseHideWhileTyping,
    mouseScrollMultiplierDiscrete: settings.terminalMouseScrollMultiplierDiscrete,
    mouseScrollMultiplierPrecision: settings.terminalMouseScrollMultiplierPrecision,
    scrollbackLimitBytes: Math.round(settings.terminalScrollbackLimitMb * 1_000_000),
    scrollbar: settings.terminalScrollbar,
  };
}
