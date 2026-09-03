import { DEFAULT_ghostex_SETTINGS, normalizeghostexSettings } from '@/packages/shared/ghostex-settings';
import {
  BOARD_SORT_OPTIONS,
  DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES,
  PRIORITY_OPTIONS,
  PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY,
  TSHIRT_OPTIONS,
  normalizeProjectBoardViewPreferences,
  type BoardEstimateFilter,
  type BoardPriorityFilter,
  type BoardSortOption,
  type ProjectBoardViewPreferences,
  type TshirtSize,
} from '../project-board-shared';
import { type ProjectBoardStartLocation } from '@/packages/shared/bead-conversation-links';

export const PROJECT_BOARD_COMMAND_COMPLETED_EVENT = 'ghostex-project-board-command-completed';
export const PROJECT_BOARD_AUTO_REFRESH_INTERVAL_MS = 8_000;
export const PROJECT_BOARD_GENERATED_TITLE_DELAY_MS = 2_000;
export const PROJECT_BOARD_GENERATED_TITLE_IDLE_TIMEOUT_MS = 10_000;
export const PROJECT_BOARD_DRAFT_TITLE_MAX_LENGTH = 39;
export const PROJECT_BOARD_MAX_DEPENDENCY_OPTIONS = 600;
export const PROJECT_BOARD_MAX_VISIBLE_TICKETS_PER_COLUMN = 120;
export const NATIVE_SETTINGS_STORAGE_KEY = 'ghostex-native-settings';

/*
 * CDXC:Automations 2026-07-01-03:24:
 * Experimental automation surfaces use the existing Enable Experimental
 * Features setting as their content gate. Read the native settings snapshot
 * here so disabled pages render only the coming-soon overlay and do not fetch
 * automation state.
 *
 * CDXC:Automations 2026-07-26:
 * GPUI's project-scoped Automate workarea is a released surface. Its
 * first-party URL explicitly opts out of the experimental gate, while macOS
 * Automate and the Quick Automations Overview keep their existing policy.
 */
export function readExperimentalFeaturesEnabled(searchParams: URLSearchParams): boolean {
  if (searchParams.get('automationExperimental') === 'false') {
    return true;
  }
  const storedSettingsJson = window.localStorage.getItem(NATIVE_SETTINGS_STORAGE_KEY);
  if (storedSettingsJson) {
    try {
      return normalizeghostexSettings(JSON.parse(storedSettingsJson)).showBetaFeatures;
    } catch {
      return DEFAULT_ghostex_SETTINGS.showBetaFeatures;
    }
  }
  const urlValue = searchParams.get('showBetaFeatures');
  if (urlValue === 'true') {
    return true;
  }
  if (urlValue === 'false') {
    return false;
  }
  return DEFAULT_ghostex_SETTINGS.showBetaFeatures;
}

export function readProjectBoardViewPreferences(): ProjectBoardViewPreferences {
  try {
    return normalizeProjectBoardViewPreferences(
      JSON.parse(window.localStorage.getItem(PROJECT_BOARD_VIEW_PREFERENCES_STORAGE_KEY) || 'null')
    );
  } catch {
    return DEFAULT_PROJECT_BOARD_VIEW_PREFERENCES;
  }
}

export const PROJECT_BOARD_START_LOCATION_SELECT_ITEMS: ReadonlyArray<{
  label: string;
  value: ProjectBoardStartLocation;
}> = [
  { label: 'Current project', value: 'currentProject' },
  { label: 'New worktree', value: 'newWorktree' },
];
export const PROJECT_BOARD_PRIORITY_SELECT_ITEMS = PRIORITY_OPTIONS.map((option) => ({
  label: option.label,
  value: option.value,
}));
export const PROJECT_BOARD_PRIORITY_FILTER_SELECT_ITEMS: Array<{ label: string; value: BoardPriorityFilter }> = [
  { label: 'All priorities', value: 'all' },
  ...PROJECT_BOARD_PRIORITY_SELECT_ITEMS,
];
export const PROJECT_BOARD_TSHIRT_SELECT_ITEMS: Array<{ label: string; value: TshirtSize | 'none' }> = [
  { label: 'None', value: 'none' },
  ...TSHIRT_OPTIONS.map((option) => ({ label: option.label, value: option.label })),
];
export const PROJECT_BOARD_ESTIMATE_FILTER_SELECT_ITEMS: Array<{ label: string; value: BoardEstimateFilter }> = [
  { label: 'All estimates', value: 'all' },
  ...PROJECT_BOARD_TSHIRT_SELECT_ITEMS,
];
export const PROJECT_BOARD_SORT_SELECT_ITEMS: Array<{ label: string; value: BoardSortOption }> = BOARD_SORT_OPTIONS.map(
  (option) => ({ label: option.label, value: option.value })
);
export const PROJECT_AUTOMATION_TRIAGE_RECENT_COMPLETED_LIMIT = 5;
