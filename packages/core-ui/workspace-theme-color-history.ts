import {
  normalizeWorkspaceThemeColorHistory,
} from "../shared/workspace-project-appearance";

const WORKSPACE_THEME_COLOR_HISTORY_STORAGE_KEY = "ghostex-workspace-theme-color-history";

/*
CDXC:ProjectSidebarOwnership 2026-06-02-13:58:
Shared helpers must stay side-effect free after the gxserver/native ownership split. Recent workspace theme colors are macOS/sidebar UI-local persistence, so localStorage access belongs in the sidebar layer while shared/workspace-project-appearance keeps only pure color normalization.
*/
export function readWorkspaceThemeColorHistory(): string[] {
  try {
    return normalizeWorkspaceThemeColorHistory(
      JSON.parse(localStorage.getItem(WORKSPACE_THEME_COLOR_HISTORY_STORAGE_KEY) ?? "[]"),
    );
  } catch {
    return [];
  }
}

export function writeWorkspaceThemeColorHistory(history: readonly string[]): void {
  try {
    localStorage.setItem(
      WORKSPACE_THEME_COLOR_HISTORY_STORAGE_KEY,
      JSON.stringify(normalizeWorkspaceThemeColorHistory([...history])),
    );
  } catch {
    // Ignore storage failures; the chosen project theme color still persists on the project.
  }
}
