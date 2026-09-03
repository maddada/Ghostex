import type { SettingsModalNavigationTab } from '../shared/ghostex-settings';

export type SettingsModalTab = SettingsModalNavigationTab;

export type SettingsModalTabVisibilityOptions = {
  showOSIntegrationSettingsTab: boolean;
};

export function shouldShowOSIntegrationSettingsTab({
  isFirstLaunchSetup,
  showBetaFeatures,
}: {
  isFirstLaunchSetup: boolean;
  showBetaFeatures: boolean;
}): boolean {
  /*
   * CDXC:Settings 2026-06-28-07:41:
   * OS Integration is currently an experimental Settings tab. Hide it during
   * ordinary Settings use and first-launch setup until the user enables Enable
   * Experimental Features from the advanced Experimental section.
   */
  return showBetaFeatures && !isFirstLaunchSetup;
}

export function resolveSettingsModalTabForVisibility(
  tab: SettingsModalTab,
  { showOSIntegrationSettingsTab }: SettingsModalTabVisibilityOptions
): SettingsModalTab {
  if (tab === 'osIntegration' && !showOSIntegrationSettingsTab) {
    return 'settings';
  }
  return tab;
}
