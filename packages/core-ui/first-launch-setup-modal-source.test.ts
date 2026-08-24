import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

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
  test('limits the visible first-launch sequence to welcome, hooks, and bundled skills', () => {
    /*
    CDXC:FirstLaunchSetup 2026-06-18-02:29:
    The first-time launch modal should only show Welcome, Agent Hooks, and
    Bundled Agent Skills while retaining the older page components in source for
    future reuse.
    */
    const visiblePages = sourceBetween(
      firstLaunchSetupModalSource,
      'const FIRST_LAUNCH_SETUP_PAGES',
      'const FIRST_LAUNCH_GUIDE_PAGES'
    );

    expect(visiblePages).toMatch(/"welcome",\s*"hooks",\s*"skills"/u);
    expect(visiblePages).not.toContain('"preferences"');
    expect(visiblePages).not.toContain('"cli"');
    expect(visiblePages).not.toContain('"browserControl"');
    expect(visiblePages).not.toContain('"desktopCua"');
    expect(visiblePages).not.toContain('"agentsSessions"');
    expect(visiblePages).not.toContain('"remoteAccess"');
    expect(firstLaunchSetupModalSource).toContain('function getVisibleFirstLaunchSetupPage');
    expect(firstLaunchSetupModalSource).toContain(
      'return FIRST_LAUNCH_SETUP_PAGES.includes(page) ? page : FIRST_LAUNCH_SETUP_PAGES[0];'
    );
    expect(firstLaunchSetupModalSource).toContain('const FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS = DEFAULT_SIDEBAR_AGENTS;');
    expect(firstLaunchSetupModalSource).not.toContain('const FIRST_LAUNCH_HOOK_AGENT_IDS');
    expect(firstLaunchSetupModalSource).toContain('function FirstLaunchPreferencesPage');
    expect(firstLaunchSetupModalSource).toContain('function FirstLaunchCliPage');
    expect(firstLaunchSetupModalSource).toContain('function FirstLaunchGuidePageView');
  });

  test('warns before skipping first-launch hooks or bundled skills', () => {
    /*
    CDXC:FirstLaunchSetup 2026-06-18-02:38:
    Continuing from the first-launch Hook or Bundled Agent Skills steps without
    installing should open a warning overlay. Install actions should live in the
    overlay's bottom-right action row, and first-launch setup pages should not
    expose manual refresh buttons.
    */
    expect(firstLaunchSetupModalSource).toContain('type FirstLaunchContinueWarning = "hooks" | "skills"');
    expect(firstLaunchSetupModalSource).toContain('areFirstLaunchAgentHooksReady(agentHookStatus)');
    expect(firstLaunchSetupModalSource).toContain('isAnyFirstLaunchBundledSkillInstalled(ghostexCliStatus)');
    expect(firstLaunchSetupModalSource).toContain('ghostexCliStatus?.embeddedBrowserSkillInstalled === true');
    expect(firstLaunchSetupModalSource).toContain('title: "Continue without bundled agent skills?"');
    expect(firstLaunchSetupModalSource).toContain('className="first-launch-setup-warning-backdrop"');
    expect(firstLaunchSetupModalSource).toContain('role="alertdialog"');
    expect(firstLaunchSetupModalSource).toContain('onInstallMissingSkills={installMissingBundledSkills}');
    expect(firstLaunchSetupModalSource).not.toContain('title="Refresh agent hook status"');
    expect(firstLaunchSetupModalSource).not.toContain('onRefreshStatus={onRequestGhostexCliStatus}');
    expect(sidebarStylesSource).toContain('.ghostex-settings-shadcn .first-launch-setup-warning-backdrop');
    expect(sidebarStylesSource).toContain('justify-content: flex-end;');
  });

  test('skips the hook warning when a primary hook provider is ready', () => {
    /*
    CDXC:FirstLaunchSetup 2026-06-19-08:42:
    The Hook step should not show the continue-warning overlay when Claude,
    Codex, OpenCode, or Pi already has a current Ghostex hook. Missing secondary
    providers should remain visible in the status list without blocking
    Continue.
    */
    expect(firstLaunchSetupModalSource).toContain(
      'const FIRST_LAUNCH_HOOK_SKIP_WARNING_AGENT_IDS = ["claude", "codex", "opencode", "pi"] as const;'
    );
    const hookReadiness = sourceBetween(
      firstLaunchSetupModalSource,
      'function areFirstLaunchAgentHooksReady',
      'function isAnyFirstLaunchBundledSkillInstalled'
    );

    expect(hookReadiness).toContain('FIRST_LAUNCH_HOOK_SKIP_WARNING_AGENT_IDS.some');
    expect(hookReadiness).toContain('isFirstLaunchAgentHookReadyStatus');
    expect(hookReadiness).not.toContain('FIRST_LAUNCH_HOOK_SUPPORTED_AGENTS.every');

    const warningStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .first-launch-setup-warning-backdrop {',
      '.ghostex-settings-shadcn .first-launch-setup-warning-actions {'
    );
    /*
    CDXC:ModalRedesign 2026-08-24:
    The alert now uses the shared Codex surface tokens — raised card tone, one
    hairline, section radius — instead of the primary-tinted gradient panel.
    */
    expect(warningStyles).toContain('background: var(--settings-raised);');
    expect(warningStyles).toContain('border: 1px solid var(--settings-hairline);');
    expect(warningStyles).toContain('border-radius: var(--settings-radius-section);');
    expect(warningStyles).not.toContain('var(--popover) 96%');
    expect(warningStyles).not.toContain('#f59e0b');
    expect(warningStyles).not.toContain('#fcd34d');
  });

  test('shows Recommended as the leftmost default sidebar-style preset', () => {
    /*
    CDXC:FirstLaunchPreferences 2026-06-13-03:28:
    The first-launch defaults page must add Recommended to the left of Minimal,
    Codex, and Detailed, and the shared default settings should keep that
    preset selected on new installs.
    */
    const presetOrder = sourceBetween(
      firstLaunchSetupModalSource,
      'const FIRST_LAUNCH_SIDEBAR_PRESET_ORDER',
      'const FIRST_LAUNCH_SIDEBAR_PRESETS'
    );

    expect(presetOrder).toMatch(/"recommended",\s*"minimal",\s*"codex",\s*"detailed"/u);

    const presetOptionsStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .first-launch-setup-preset-options {',
      '.ghostex-settings-shadcn .first-launch-setup-preset-button {'
    );
    expect(presetOptionsStyles).toContain('grid-template-columns: repeat(4, minmax(0, 1fr));');
  });

  test('keeps defaults-page checkbox controls square', () => {
    /*
    CDXC:FirstLaunchPreferences 2026-06-13-05:27:
    The first-time defaults modal should show square checkbox controls instead
    of native macOS rounded checkboxes while keeping the shape scoped to the
    onboarding preference tiles.

    CDXC:ModalRedesign 2026-08-24:
    The control stays a small square affordance rather than the native rounded
    checkbox, but takes the redesign's adaptive corner so it belongs to the same
    family as the 8px controls beside it.
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

    expect(androidDownloadUrlDefinition).toContain(
      'https://github.com/maddada/Ghostex/releases/latest/download/ghostex-android.apk'
    );
    expect(androidDownloadUrlDefinition).not.toMatch(/releases\/download\/v\d/u);
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
      'function FirstLaunchHooksPage'
    );

    expect(preferencesPage).toContain('const firstLaunchPromptAgentOptions = firstLaunchPromptAgentHasSavedDefault');
    expect(preferencesPage).toContain('Unavailable (${normalizedDefaultPromptAgentId})');
    expect(preferencesPage).toContain('const selectedDefaultPromptAgentId = normalizedDefaultPromptAgentId;');
  });
});
