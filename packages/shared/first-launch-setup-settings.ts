import type { ghostexSettings } from './ghostex-settings';

export const FIRST_LAUNCH_SETUP_SEEN_STORAGE_KEY = 'ghostex-native-first-launch-setup-seen';
export const FIRST_LAUNCH_SETUP_CURRENT_REVISION = '2026-08-24-onboarding-redesign';
export const HIGHLIGHTED_FEATURES_SEEN_STORAGE_KEY = 'ghostex-native-highlighted-features-seen';
export const HIGHLIGHTED_FEATURES_CURRENT_REVISION = '2026-06-16-highlighted-features-launch';

type FirstLaunchSetupSeenStorage = Pick<Storage, 'getItem' | 'setItem'>;

/**
 * CDXC:FirstLaunchSetup 2026-05-19-11:20:
 * First launch setup reuses the Settings main tab with a filtered subset of
 * controls. Add setting keys to this list as product requirements specify which
 * options appear in the post-tips onboarding modal.
 *
 * CDXC:FirstLaunchPreferences 2026-05-31-07:10:
 * ZMU-71: the first-time defaults step must include keepAwakePreventLidSleep
 * ("Keep awake when lid is closed") alongside the other high-impact toggles on
 * FirstLaunchPreferencesPage. Keep this list aligned with that page.
 *
 * CDXC:FirstLaunchPreferences 2026-06-04-21:02:
 * The first-time defaults modal must also expose the first-prompt title
 * generation agent selector so new installs can choose Codex, Cursor, Claude,
 * Grok Build, or Custom before automatic session naming runs.
 *
 * CDXC:FirstLaunchSetup 2026-06-07-12:32:
 * The first-time modal changed after the last release, so the seen marker is a
 * revision string instead of a forever boolean. Legacy `true` values should
 * reopen the refreshed setup once, then store this revision to avoid repeated
 * prompts on later launches.
 *
 * CDXC:HighlightedFeatures 2026-06-16-18:55:
 * Highlighted Features is a product-update announcement as well as first-run
 * intro. Store a separate revision marker so existing users who already
 * completed first-launch setup still see the new feature tour once after
 * updating, while future launches and manual replays remain independent.
 *
 * CDXC:FirstLaunchSetup 2026-06-18-02:29:
 * The visible first-time setup flow is shortened to Welcome, Agent Hooks, and
 * Bundled Agent Skills. Bump the setup revision so installs that were silently
 * marked seen by the temporary no-startup-flow behavior get this shorter setup
 * once.
 */
export type FirstLaunchSetupMainSettingKey =
  | keyof ghostexSettings
  | 'accessibilityPermission'
  | 'attentionNotificationActions'
  | 'ghostexFolderStats'
  | 'ghosttySettingsActions'
  | 'sidebarSettingsPreset';

export const FIRST_LAUNCH_PREFERENCES_MAIN_SETTING_KEYS = [
  'sidebarSettingsPreset',
  'defaultPromptAgentId',
  'sessionTitleGenerationAgent',
  'customSessionTitleGenerationCommand',
  'keepAwakePreventLidSleep',
  'agentAcceptAllEnabled',
  'showMacOSAttentionNotifications',
  'completionBellEnabled',
] as const satisfies readonly FirstLaunchSetupMainSettingKey[];

export const FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS = new Set<FirstLaunchSetupMainSettingKey>(
  FIRST_LAUNCH_PREFERENCES_MAIN_SETTING_KEYS
);

export function isFirstLaunchSetupMainSettingVisible(
  settingKey: FirstLaunchSetupMainSettingKey,
  visibleSettings: ReadonlySet<FirstLaunchSetupMainSettingKey> = FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS
): boolean {
  return visibleSettings.has(settingKey);
}

export function hasSeenCurrentFirstLaunchSetup(
  storage: FirstLaunchSetupSeenStorage,
  revision = FIRST_LAUNCH_SETUP_CURRENT_REVISION
): boolean {
  return storage.getItem(FIRST_LAUNCH_SETUP_SEEN_STORAGE_KEY) === revision;
}

export function markCurrentFirstLaunchSetupSeen(
  storage: FirstLaunchSetupSeenStorage,
  revision = FIRST_LAUNCH_SETUP_CURRENT_REVISION
): void {
  storage.setItem(FIRST_LAUNCH_SETUP_SEEN_STORAGE_KEY, revision);
}

export function hasSeenCurrentHighlightedFeatures(
  storage: FirstLaunchSetupSeenStorage,
  revision = HIGHLIGHTED_FEATURES_CURRENT_REVISION
): boolean {
  return storage.getItem(HIGHLIGHTED_FEATURES_SEEN_STORAGE_KEY) === revision;
}

export function markCurrentHighlightedFeaturesSeen(
  storage: FirstLaunchSetupSeenStorage,
  revision = HIGHLIGHTED_FEATURES_CURRENT_REVISION
): void {
  storage.setItem(HIGHLIGHTED_FEATURES_SEEN_STORAGE_KEY, revision);
}
