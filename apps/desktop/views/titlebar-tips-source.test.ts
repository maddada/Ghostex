import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const appSource = readFileSync(new URL('./titlebar/app.tsx', import.meta.url), 'utf8');
const constantsSource = readFileSync(new URL('./titlebar/constants.ts', import.meta.url), 'utf8');
const tipsDataSource = readFileSync(new URL('./titlebar/tips-data.ts', import.meta.url), 'utf8');
const tipsPanelSource = readFileSync(new URL('./titlebar/tips-panel.tsx', import.meta.url), 'utf8');
const stylesSource = readFileSync(new URL('./titlebar/styles.ts', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('native titlebar Tips & Tricks source', () => {
  test('uses video, setup, and updates actions', () => {
    /*
     * CDXC:Onboarding 2026-06-16-19:42:
     * The Tips & Tricks header should not expose a bulk Read all button.
     * It should instead open the tutorial video with a filled star, Setup with guide wording, and release updates as an in-project browser session while individual tips keep their per-row read controls.
     *
     * CDXC:Onboarding 2026-06-18-04:53:
     * The header should use the shorter Tips text, add Docs as an in-project
     * browser action, and shorten the setup label to Setup.
     *
     * CDXC:Onboarding 2026-06-18-05:31:
     * The Video button should open the tutorial video modal while leaving
     * the old Highlighted Features modal unused.
     *
     * CDXC:Onboarding 2026-06-30-04:28:
     * The header should label the tutorial-video action Video and the changelog action Updates so the widest equal-width header button is shorter.
     */
    const menuSource = sourceBetween(tipsPanelSource, 'function TitlebarTipsMenu', 'function TitlebarTipsSection');

    expect(menuSource).toContain('Docs');
    expect(menuSource).toContain('<span>Tips</span>');
    expect(menuSource).toContain('Video');
    expect(menuSource).toContain('Setup');
    expect(menuSource).toContain('Updates');
    expect(menuSource).toContain('IconStarFilled');
    expect(menuSource).toContain('IconBook2');
    expect(menuSource).toContain('IconTool');
    expect(menuSource).toContain('IconHistory');
    expect(menuSource).toContain('onOpenDocs');
    expect(menuSource).toContain('onOpenHighlightedFeatures');
    expect(menuSource).toContain('onViewGhostexGuide');
    expect(menuSource).toContain('onOpenChangelog');
    expect(menuSource).not.toContain('<span>Tips & Tricks</span>');
    expect(menuSource).not.toContain('Setup Ghostex');
    expect(menuSource).not.toContain('>Features<');
    expect(menuSource).not.toContain('>Changelog<');
    expect(menuSource).not.toContain('>Highlighted Features<');
    expect(menuSource).not.toContain('>View Ghostex Guide<');
    expect(menuSource).not.toContain('Open Highlighted Features');
    expect(menuSource).not.toContain('Read all');
    expect(menuSource).not.toContain('onMarkAllRead');
    expect(menuSource).not.toContain('Run Setup Flow');
    expect(menuSource).not.toContain('titlebar-tips-summary');
    expect(appSource).toContain("type: 'openBrowserPane', url: GHOSTEX_DOCS_URL");
    expect(appSource).toContain("type: 'openGhostexTutorialVideo'");
    expect(appSource).toContain("type: 'openWorkspaceWelcome'");
    expect(appSource).toContain("type: 'openBrowserPane', url: GHOSTEX_CHANGELOG_URL");
    expect(constantsSource).toContain('https://github.com/maddada/ghostex/releases');
  });

  test('keeps tips actions equal width, full height, connected, and right-flush', () => {
    /*
     * CDXC:Onboarding 2026-06-16-19:42:
     * The Tips & Tricks panel should make all three header actions the same width, remove the top-right unread text summary, and use pointer cursors for clickable controls.
     *
     * CDXC:Onboarding 2026-06-30-01:38:
     * The Tips header actions should fill the full header height and remove inter-button gaps, leaving only left/right borders between adjacent titlebar buttons.
     *
     * CDXC:Onboarding 2026-06-30-03:22:
     * The header actions should sit flush to the right edge, remove the idle
     * button background fill, and share the widest action width with 15px side
     * padding.
     */
    const tipsMenuStylesSource = sourceBetween(stylesSource, '.titlebar-tips-menu', '.titlebar-resources-info-button');
    const actionsStyles = sourceBetween(
      tipsMenuStylesSource,
      '.titlebar-tips-actions {',
      '  .titlebar-tips-action-button {'
    );
    const actionButtonStyles = sourceBetween(
      tipsMenuStylesSource,
      '.titlebar-tips-action-button {',
      '  .titlebar-tips-action-button:last-child'
    );

    expect(tipsMenuStylesSource).toContain('min-height: 47px;');
    expect(tipsMenuStylesSource).toContain('padding: 0 0 0 12px;');
    expect(actionsStyles).toContain('align-self: stretch;');
    expect(actionsStyles).toContain('align-items: stretch;');
    expect(actionsStyles).toContain('gap: 0;');
    expect(actionsStyles).toContain('grid-template-columns: repeat(4, minmax(max-content, 1fr));');
    expect(actionsStyles).toContain('width: max-content;');
    expect(actionsStyles).not.toContain('width: 420px;');
    expect(actionButtonStyles).toContain('background: transparent;');
    expect(actionButtonStyles).toContain('border: 0;');
    expect(actionButtonStyles).toContain('border-left: 1px solid rgba(255,255,255,0.12);');
    expect(actionButtonStyles).toContain('box-sizing: border-box;');
    expect(actionButtonStyles).toContain('height: 100%;');
    expect(actionButtonStyles).toContain('padding: 0 15px;');
    expect(tipsMenuStylesSource).toContain('.titlebar-tips-action-button:last-child');
    expect(tipsMenuStylesSource).toContain('border-right: 1px solid rgba(255,255,255,0.12);');
    expect(tipsMenuStylesSource).toContain('.titlebar-tips-panel button:not(:disabled)');
    expect(tipsMenuStylesSource).toContain('cursor: pointer;');
    expect(tipsMenuStylesSource).not.toContain('.titlebar-tips-summary');
  });

  test('opens agent skill tips to their related detail surfaces', () => {
    /*
     * CDXC:Onboarding 2026-06-28-08:00:
     * Agent-facing Browser Use and Computer Use tips should open Settings >
     * Integrations with the relevant skill searched. The read check remains
     * separate from row navigation.
     */
    const tipsSource = sourceBetween(
      tipsDataSource,
      'const TITLEBAR_TIPS: TitlebarTip[] = [',
      'export const TITLEBAR_DEBUGGING_MODE_NOTICE'
    );
    const rowSource = sourceBetween(tipsPanelSource, 'function TitlebarTipRow', 'function getTitlebarTipIcon');

    expect(tipsSource).toContain("id: 'use-ghostex-computer-use-skill'");
    expect(tipsSource).toContain('/ghostex-computer-use');
    expect(tipsSource).toContain("settingsSearchQuery: 'Ghostex Computer Use'");
    expect(tipsSource).toContain("id: 'use-ghostex-browser-use-skill'");
    expect(tipsSource).toContain('/ghostex-browser-use');
    expect(tipsSource).toContain("settingsSearchQuery: 'Ghostex Browser Use'");
    expect(tipsSource).not.toContain("id: 'recommend-faster-chrome-devtools-skill'");
    expect(appSource).toContain('initialSearchQuery: action.settingsSearchQuery');
    expect(appSource).toContain("initialTab: 'integrations'");
    expect(appSource).toContain("postTitlebarSidebarCommand({ type: 'openBrowserPane', url: action.url });");
    expect(rowSource).toContain('titlebar-tip-detail-button');
    expect(rowSource).toContain('onOpenTipAction(tip)');
    expect(rowSource).toContain('titlebar-tip-read-button');
    expect(rowSource).toContain('onMarkRead(tip.id)');
  });

  test('does not render right-aligned section counts', () => {
    /*
     * CDXC:Onboarding 2026-06-12-23:28:
     * macOS Tips & Tricks section headers should show labels only; the previous
     * right-side count looked like noisy chrome beside Read and Unread headings.
     */
    const sectionSource = sourceBetween(tipsPanelSource, 'function TitlebarTipsSection', 'function TitlebarNoticeRow');

    expect(tipsPanelSource).toContain('headers read as labels only');
    expect(sectionSource).toContain('count > 0 ? children');
    expect(tipsPanelSource).not.toContain('titlebar-tips-section-count');
    expect(stylesSource).not.toContain('titlebar-tips-section-count');
  });

  test('warns from Tips when installed agent CLIs are missing hooks', () => {
    /*
     * CDXC:AgentHooks 2026-06-18-03:08:
     * The titlebar Tips dropdown must show a non-dismissable notice when
     * installed agent CLIs are missing or using stale hooks, even before a live
     * agent session exists, and the copy must name session naming, status, and
     * sleep/resume reliability.
     *
     * CDXC:AgentHooks 2026-06-23-05:09:
     * Clicking the missing-hook notice should open Settings > Integrations with
     * Agent Hooks searched so the user sees provider status and the install
     * control instead of starting installation from titlebar chrome.
     */
    const noticeSource = sourceBetween(
      tipsDataSource,
      'function createTitlebarMissingAgentHooksNotice',
      'function isTitlebarLiveTerminalAgentSession'
    );

    expect(noticeSource).toContain('getDefaultSidebarAgentById(status.agentId)');
    expect(noticeSource).toContain('!status.cliInstalled');
    expect(noticeSource).toContain("Warning: Agent hooks aren't installed for agent CLIs");
    expect(noticeSource).toContain('Open Settings > Agents');
    expect(noticeSource).toContain('Automatic session renaming');
    expect(noticeSource).toContain('In Progress/Needs Attention status');
    expect(noticeSource).toContain('sleeping or resuming agent sessions will not work correctly');
    expect(noticeSource).toContain("action: 'openSettings'");
    expect(noticeSource).toContain("settingsTarget: 'agentHooks'");
    expect(appSource).toContain("initialAgentsSection: 'agentHooks'");
    expect(appSource).toContain("initialTab: 'agents'");
    expect(tipsPanelSource).toContain("title='Notices'");
    expect(appSource).toContain('openAgentHooksSettings');
    expect(appSource).not.toContain('installAgentHooksFromTitlebarNotice');
  });
});
