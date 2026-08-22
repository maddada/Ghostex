import {
  detectghostexHotkeyPlatform,
  normalizeHotkeyText,
  type ghostexHotkeyPlatform,
} from "../shared/ghostex-hotkeys";

/**
 * CDXC:Hotkeys 2026-07-30:
 * Settings, menus, command discovery, and terminal controls must describe the
 * same saved chord using the current OS convention. macOS gets compact native
 * glyphs (`⌘⌥S`); Windows and Linux get textual labels (`Ctrl+Alt+S`).
 * Stored `cmd` remains the cross-platform primary modifier.
 */
export function formatSidebarHotkeyLabel(
  hotkey: string,
  platform: ghostexHotkeyPlatform = detectghostexHotkeyPlatform(),
): string {
  return normalizeHotkeyText(hotkey)
    .split(" ")
    .map((chord) => formatSidebarHotkeyChord(chord, platform))
    .join(" ");
}

function formatSidebarHotkeyChord(chord: string, platform: ghostexHotkeyPlatform): string {
  const parts = chord.split("+");
  const hasPrimaryModifier = parts.includes("cmd");
  const separator = platform === "mac" ? "" : "+";
  return parts
    .map((part) => formatSidebarHotkeyPart(part, platform, hasPrimaryModifier))
    .filter((part, index, formattedParts) => part !== formattedParts[index - 1])
    .join(separator);
}

function formatSidebarHotkeyPart(
  part: string,
  platform: ghostexHotkeyPlatform,
  hasPrimaryModifier: boolean,
): string {
  if (platform !== "mac") {
    switch (part) {
      case "cmd":
        return "Ctrl";
      case "ctrl":
        return hasPrimaryModifier ? "Alt" : "Ctrl";
      case "alt":
        return "Alt";
      case "shift":
        return "Shift";
      default:
        break;
    }
  }
  switch (part) {
    case "cmd":
      return "⌘";
    case "ctrl":
      return "⌃";
    case "alt":
      return "⌥";
    case "shift":
      return "⇧";
    case "up":
      return "↑";
    case "right":
      return "→";
    case "down":
      return "↓";
    case "left":
      return "←";
    case "tab":
      return "Tab";
    default:
      if (/^f\d+$/u.test(part)) {
        return part.toUpperCase();
      }
      return part.length === 1 ? part.toUpperCase() : part;
  }
}
