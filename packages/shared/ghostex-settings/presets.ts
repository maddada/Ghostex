import {
  SIDEBAR_SETTINGS_PRESET_KEYS,
  type SidebarSettingsPresetId,
  type SidebarSettingsPresetKey,
  type SidebarSettingsPresetSettings,
  type ghostexSettings,
} from "./types";

/**
 * CDXC:SidebarSettingsPresets 2026-05-16-10:11:
 * The Settings top row exposes Codex, Minimal, Detailed, and Recommended sidebar UI presets as toggle buttons.
 * Preset state is derived from the controlled sidebar settings instead of persisted separately, so manual deviations show Custom without adding another source of truth.
 *
 * CDXC:SidebarSettingsPresets 2026-06-12-07:10:
 * Superseded by CDXC:SidebarSettingsPresets 2026-06-30-22:29.
 *
 * CDXC:SidebarSettingsPresets 2026-06-13-01:06:
 * Superseded by CDXC:SidebarSettingsPresets 2026-06-30-22:29.
 *
 * CDXC:SidebarSettingsPresets 2026-06-13-15:42:
 * Recommended should keep the sidebar quieter by hiding session-card Last Active timestamps while preserving the rest of the detailed status chrome.
 *
 * CDXC:SessionStatusIndicators 2026-06-15-14:00:
 * Sidebar presets did not control the legacy macOS floating status indicator.
 *
 * CDXC:SessionStatusIndicators 2026-06-27-20:11:
 * The standalone floating status indicator was removed from macOS and GPUI.
 * Presets now tune only sidebar density and the menu-bar indicator; legacy
 * floating keys are normalized separately for old settings files only.
 *
 * CDXC:SidebarSettingsPresets 2026-06-23-08:20:
 * Every sidebar preset must show session-card close buttons on hover. Presets may still tune density, icons, timestamps, project stats, and menu-bar indicators, but they should not remove the primary per-session close affordance.
 *
 * CDXC:SidebarSettingsPresets 2026-06-30-22:29:
 * Recommended should match the user's current preset-controlled sidebar configuration: visible session agent icons, visible browser favicons, close button on hover, hidden Last Active timestamps, visible project git stats, hidden changed-file counts, and visible menu-bar session indicators.
 */
export const SIDEBAR_SETTINGS_PRESET_SETTINGS = {
  codex: {
    showProjectIcons: true,
    hideSessionAgentIconUntilHover: true,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: true,
  },
  minimal: {
    showProjectIcons: false,
    hideSessionAgentIconUntilHover: true,
    hideBrowserFaviconUntilHover: true,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: true,
    hideProjectHeaderDiffStats: true,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: true,
  },
  detailed: {
    showProjectIcons: true,
    hideSessionAgentIconUntilHover: false,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: false,
    hideProjectHeaderDiffStats: false,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: false,
  },
  recommended: {
    showProjectIcons: true,
    hideSessionAgentIconUntilHover: false,
    hideBrowserFaviconUntilHover: false,
    showCloseButtonOnSessionCards: true,
    hideLastActiveTimeOnSessionCards: true,
    hideProjectHeaderDiffStats: false,
    showProjectEditorDiffFileCount: false,
    hideMenuBarSessionStatusIndicators: false,
  },
} as const satisfies Record<SidebarSettingsPresetId, SidebarSettingsPresetSettings>;

export const SIDEBAR_SETTINGS_PRESETS: ReadonlyArray<{
  id: SidebarSettingsPresetId;
  label: string;
  settings: SidebarSettingsPresetSettings;
}> = [
  {
    id: "recommended",
    label: "Recommended",
    settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.recommended,
  },
  { id: "codex", label: "Codex", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.codex },
  { id: "minimal", label: "Minimal", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.minimal },
  { id: "detailed", label: "Detailed", settings: SIDEBAR_SETTINGS_PRESET_SETTINGS.detailed },
];

export function getSidebarSettingsPresetId(
  settings: Pick<ghostexSettings, SidebarSettingsPresetKey>,
): SidebarSettingsPresetId | undefined {
  return SIDEBAR_SETTINGS_PRESETS.find((preset) =>
    SIDEBAR_SETTINGS_PRESET_KEYS.every((key) => Object.is(settings[key], preset.settings[key])),
  )?.id;
}
