import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { GHOSTEX_ANDROID_APK_URL } from '../shared/sidebar-commands';

const firstLaunchSetupModalSource = readFileSync(new URL('./first-launch-setup-modal.tsx', import.meta.url), 'utf8');
const sidebarStylesSource = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('first launch setup modal source', () => {
  test('shows the redesigned six-step sequence and keeps dormant pages out of it', () => {
    /*
    CDXC:FirstLaunchSetup 2026-08-24:
    The onboarding redesign flow is Welcome -> Extensions -> Agents -> Connect
    (hooks) -> Skills -> Get started (project). Older page components stay in
    source for future reuse but never enter the visible sequence.
    */
    const visiblePages = sourceBetween(
      firstLaunchSetupModalSource,
      'const FIRST_LAUNCH_SETUP_PAGES',
      'const FIRST_LAUNCH_STEP_LABELS'
    );

    expect(visiblePages).toMatch(/'welcome',\s*'extensions',\s*'agents',\s*'hooks',\s*'skills',\s*'project',/u);
    expect(visiblePages).not.toContain("'preferences'");
    expect(visiblePages).not.toContain("'cli'");
    expect(visiblePages).not.toContain("'video'");
    expect(visiblePages).not.toContain("'ready'");
    expect(visiblePages).not.toContain("'browserControl'");
    expect(visiblePages).not.toContain("'desktopCua'");
    expect(visiblePages).not.toContain("'agentsSessions'");
    expect(visiblePages).not.toContain("'remoteAccess'");
    expect(firstLaunchSetupModalSource).toContain('function getVisibleFirstLaunchSetupPage');
    expect(firstLaunchSetupModalSource).toContain('const FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS = DEFAULT_SIDEBAR_AGENTS;');
    expect(firstLaunchSetupModalSource).toContain('function FirstLaunchPreferencesPage');
    expect(firstLaunchSetupModalSource).toContain('function FirstLaunchCliPage');
    expect(firstLaunchSetupModalSource).toContain('function FirstLaunchGuidePageView');
  });

  test('skips the Connect step entirely when no agent CLI is installed', () => {
    /*
    CDXC:FirstLaunchSetup 2026-08-24:
    The Connect (hooks) step can only connect agents that exist on the machine,
    so with zero installed agent CLIs the step disappears from the sequence
    instead of nagging with a skip warning. The old continue-warning overlay is
    gone with it.
    */
    const pagesHelper = sourceBetween(
      firstLaunchSetupModalSource,
      'function getFirstLaunchSetupPages',
      'type FirstLaunchUseCase'
    );
    expect(pagesHelper).toContain("FIRST_LAUNCH_SETUP_PAGES.filter((page) => page !== 'hooks')");
    expect(firstLaunchSetupModalSource).not.toContain('FirstLaunchContinueWarning');
    expect(firstLaunchSetupModalSource).not.toContain('first-launch-setup-warning-backdrop');
  });

  test('only offers visible bundled skills, recommended checked by default', () => {
    /*
    CDXC:AgentSkills 2026-08-24:
    The skills step renders the shared visible catalog (hiddenFromUi filtered
    out) with the recommended tier preselected, and installs the Trycua driver
    once when a selected skill needs it.
    */
    expect(firstLaunchSetupModalSource).toContain('VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS');
    expect(firstLaunchSetupModalSource).toContain('FIRST_LAUNCH_RECOMMENDED_SKILL_IDS');
    expect(firstLaunchSetupModalSource).toContain('requiresCuaDriver === true');
  });

  test('shows Recommended as the leftmost default sidebar-style preset', () => {
    /*
    CDXC:FirstLaunchPreferences 2026-06-13-03:28:
    The (currently dormant) defaults page keeps Recommended to the left of
    Minimal, Codex, and Detailed.
    */
    const presetOrder = sourceBetween(
      firstLaunchSetupModalSource,
      'const FIRST_LAUNCH_SIDEBAR_PRESET_ORDER',
      'const FIRST_LAUNCH_SIDEBAR_PRESETS'
    );

    expect(presetOrder).toMatch(/'recommended',\s*'minimal',\s*'codex',\s*'detailed',/u);
  });

  test('keeps defaults-page checkbox controls square', () => {
    /*
    CDXC:FirstLaunchPreferences 2026-06-13-05:27:
    The dormant defaults page should keep square checkbox controls scoped to
    the onboarding preference tiles.
    */
    const checkboxStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .first-launch-setup-checkbox {',
      '.ghostex-settings-shadcn .first-launch-setup-benefit + .first-launch-setup-benefit {'
    );

    expect(checkboxStyles).toContain('appearance: none;');
    expect(checkboxStyles).toContain('border-radius: var(--settings-radius-adaptive);');
    expect(checkboxStyles).toContain('.ghostex-settings-shadcn .first-launch-setup-checkbox:checked::after');
  });

  test('uses the latest-release redirect for Android APK downloads', () => {
    /*
    CDXC:FirstLaunchSetup 2026-06-16-01:04:
    First-launch Android APK buttons must use a latest-release redirect instead
    of a tagged APK URL so release updates do not leave onboarding pointed at an
    older Android package.
    */
    const androidDownloadUrlDefinition = sourceBetween(
      firstLaunchSetupModalSource,
      'const FIRST_LAUNCH_ANDROID_APK_URL',
      'const FIRST_LAUNCH_DISCORD_URL'
    );

    expect(androidDownloadUrlDefinition).toContain('GHOSTEX_ANDROID_APK_URL');
    expect(GHOSTEX_ANDROID_APK_URL).toBe(
      'https://github.com/maddada/Ghostex/releases/latest/download/ghostex-android.apk'
    );
    expect(GHOSTEX_ANDROID_APK_URL).not.toMatch(/releases\/download\/v\d/u);
  });

  test('preserves unavailable gxserver-owned default prompt agents on the preferences page', () => {
    /*
    CDXC:GxserverAgentSettings 2026-06-19-08:58:
    First-launch preferences can save unrelated defaults, so the default-agent
    select must display an unavailable saved gxserver id instead of visually
    falling back to Codex and inviting accidental overwrite.
    */
    const preferencesPage = sourceBetween(
      firstLaunchSetupModalSource,
      'function FirstLaunchPreferencesPage',
      'function FirstLaunchCheckboxSetting'
    );

    expect(preferencesPage).toContain('const firstLaunchPromptAgentOptions = firstLaunchPromptAgentHasSavedDefault');
    expect(preferencesPage).toContain('Unavailable (${normalizedDefaultPromptAgentId})');
    expect(preferencesPage).toContain('const selectedDefaultPromptAgentId = normalizedDefaultPromptAgentId;');
  });
});
