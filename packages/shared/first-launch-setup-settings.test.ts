import { describe, expect, test } from 'vitest';
import {
  HIGHLIGHTED_FEATURES_CURRENT_REVISION,
  HIGHLIGHTED_FEATURES_SEEN_STORAGE_KEY,
  FIRST_LAUNCH_PREFERENCES_MAIN_SETTING_KEYS,
  FIRST_LAUNCH_SETUP_CURRENT_REVISION,
  FIRST_LAUNCH_SETUP_SEEN_STORAGE_KEY,
  FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS,
  hasSeenCurrentHighlightedFeatures,
  hasSeenCurrentFirstLaunchSetup,
  isFirstLaunchSetupMainSettingVisible,
  markCurrentHighlightedFeaturesSeen,
  markCurrentFirstLaunchSetupSeen,
} from './first-launch-setup-settings';

function createFirstLaunchSetupStorage(initialValue?: string, highlightedFeaturesValue?: string) {
  const values = new Map<string, string>();
  if (initialValue !== undefined) {
    values.set(FIRST_LAUNCH_SETUP_SEEN_STORAGE_KEY, initialValue);
  }
  if (highlightedFeaturesValue !== undefined) {
    values.set(HIGHLIGHTED_FEATURES_SEEN_STORAGE_KEY, highlightedFeaturesValue);
  }
  return {
    getItem: (key: string) => values.get(key) ?? null,
    read: (key: string) => values.get(key),
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
  };
}

describe('first launch setup visible settings', () => {
  test('exposes keep awake when lid closed on the first-launch defaults step', () => {
    /**
     * CDXC:FirstLaunchPreferences 2026-05-31-07:10:
     * ZMU-71 requires the lid-close keep-awake preference on the first-time
     * defaults page so new installs can opt in before using Keep Awake.
     */
    expect(
      isFirstLaunchSetupMainSettingVisible('keepAwakePreventLidSleep', FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS)
    ).toBe(true);
    expect(FIRST_LAUNCH_PREFERENCES_MAIN_SETTING_KEYS).toContain('keepAwakePreventLidSleep');
  });

  test('exposes title generation agent settings on the first-launch defaults step', () => {
    /**
     * CDXC:FirstLaunchPreferences 2026-06-04-21:02:
     * The first-time modal must expose the same first-prompt title-generation
     * agent selector as Settings so new installs can choose Codex, Cursor,
     * Claude, Grok Build, or Custom before automatic session naming runs.
     */
    expect(
      isFirstLaunchSetupMainSettingVisible('sessionTitleGenerationAgent', FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS)
    ).toBe(true);
    expect(FIRST_LAUNCH_PREFERENCES_MAIN_SETTING_KEYS).toContain('sessionTitleGenerationAgent');
    expect(FIRST_LAUNCH_PREFERENCES_MAIN_SETTING_KEYS).toContain('customSessionTitleGenerationCommand');
  });

  test('treats legacy boolean seen markers as older first-launch setup revisions', () => {
    /**
     * CDXC:FirstLaunchSetup 2026-06-07-12:32:
     * Users who saw the pre-refresh first-time modal stored the old boolean
     * marker. The refreshed modal must open once for those installs, then write
     * the current revision so the setup flow does not repeat every launch.
     */
    const storage = createFirstLaunchSetupStorage('true');

    expect(hasSeenCurrentFirstLaunchSetup(storage)).toBe(false);

    markCurrentFirstLaunchSetupSeen(storage);

    expect(storage.read(FIRST_LAUNCH_SETUP_SEEN_STORAGE_KEY)).toBe(FIRST_LAUNCH_SETUP_CURRENT_REVISION);
    expect(hasSeenCurrentFirstLaunchSetup(storage)).toBe(true);
  });

  test('skips first-launch setup only after the current revision has been seen', () => {
    const storage = createFirstLaunchSetupStorage(FIRST_LAUNCH_SETUP_CURRENT_REVISION);

    expect(hasSeenCurrentFirstLaunchSetup(storage)).toBe(true);
  });

  test('treats the pre-shortened setup revision as stale', () => {
    /**
     * CDXC:FirstLaunchSetup 2026-06-18-02:29:
     * The shortened Welcome / Agent Hooks / Bundled Agent Skills flow should
     * open once even for installs that stored the previous setup revision.
     */
    const storage = createFirstLaunchSetupStorage('2026-06-07-first-launch-refresh');

    expect(hasSeenCurrentFirstLaunchSetup(storage)).toBe(false);
  });

  test('tracks highlighted features as a separate update announcement revision', () => {
    /**
     * CDXC:HighlightedFeatures 2026-06-16-18:55:
     * Existing users may already have the current first-launch setup marker, so
     * Highlighted Features needs its own revision marker to show once after
     * update without reopening the setup flow on later launches.
     */
    const storage = createFirstLaunchSetupStorage(FIRST_LAUNCH_SETUP_CURRENT_REVISION);

    expect(hasSeenCurrentFirstLaunchSetup(storage)).toBe(true);
    expect(hasSeenCurrentHighlightedFeatures(storage)).toBe(false);

    markCurrentHighlightedFeaturesSeen(storage);

    expect(storage.read(HIGHLIGHTED_FEATURES_SEEN_STORAGE_KEY)).toBe(HIGHLIGHTED_FEATURES_CURRENT_REVISION);
    expect(hasSeenCurrentHighlightedFeatures(storage)).toBe(true);
  });
});
