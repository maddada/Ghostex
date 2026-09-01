import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const referenceChromeSource = readFileSync(new URL('./sidebar-app/reference-chrome.tsx', import.meta.url), 'utf8');
const groupPanelsSource = readFileSync(new URL('./styles/group-panels.css', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('reference sidebar section header layout source', () => {
  test('applies native left inset to primary sidebar nav buttons', () => {
    /*
     * CDXC:SidebarReferenceBounds 2026-06-22-01:11:
     * The macOS sidebar left inset must be 5px beyond the shared reference
     * layout, and the top Agents Hub, Automations, Mobile, and Search buttons
     * must use the native primary-nav bleed so their row surfaces move with the
     * rest of the sidebar instead of staying flush to the viewport edge.
     */
    const nativeInsetRule = sourceBetween(
      groupPanelsSource,
      "body.native-sidebar-body .sidebar-reference-layout[data-reference-sidebar='true'] {",
      ".sidebar-reference-layout[data-reference-sidebar='true'],"
    );
    expect(nativeInsetRule).toContain('--reference-sidebar-primary-nav-edge-bleed-left: 9px;');
    expect(nativeInsetRule).toContain('padding-left: 14px;');

    const primaryNavBleedRule = sourceBetween(
      groupPanelsSource,
      ".sidebar-reference-layout[data-reference-sidebar='true'] .reference-sidebar-primary-nav > .reference-sidebar-nav-item,",
      '.reference-sidebar-nav-item {'
    );
    expect(primaryNavBleedRule).toContain(
      'margin-left: calc(-1 * var(--reference-sidebar-primary-nav-edge-bleed-left));'
    );

    const primaryNavButtonPaddingRule = sourceBetween(
      groupPanelsSource,
      ".sidebar-reference-layout[data-reference-sidebar='true']\n  .reference-sidebar-primary-nav\n  > :is(",
      ".sidebar-reference-layout[data-reference-sidebar='true'] .reference-sidebar-actions-button {"
    );
    expect(primaryNavButtonPaddingRule).toContain(
      'padding-left: calc(10px + var(--reference-sidebar-primary-nav-edge-bleed-left));'
    );
  });

  test('collapses Quick and Projects labels when hover actions become visible', () => {
    /*
     * CDXC:SidebarHeaderActions 2026-06-17-23:21:
     * Quick and Projects section labels should shorten like Search instead of
     * painting underneath their hover action buttons in the narrow native
     * sidebar.
     */
    expect(referenceChromeSource).toMatch(
      /<span\s+className='reference-sidebar-section-title'>\s*\{title\}\s*<\/span>/u
    );

    const sectionRowRule = sourceBetween(
      groupPanelsSource,
      '.reference-sidebar-section-row {',
      ".reference-sidebar-section-row[data-reference-section='projects']"
    );
    expect(sectionRowRule).toContain('--reference-sidebar-section-actions-max-width: 132px;');
    expect(sectionRowRule).toContain('CDXC:SidebarHeaderActions 2026-06-17-23:21');

    const titleRule = sourceBetween(
      groupPanelsSource,
      '.reference-sidebar-section-title {',
      '.reference-sidebar-section-chevron'
    );
    expect(titleRule).toContain('min-width: 0;');
    expect(titleRule).toContain('overflow: hidden;');
    expect(titleRule).toContain('text-overflow: ellipsis;');
    expect(titleRule).toContain('white-space: nowrap;');

    const hiddenActionsRule = sourceBetween(
      groupPanelsSource,
      '.reference-sidebar-section-actions {',
      ".sidebar-reference-layout[data-reference-sidebar='true']"
    );
    expect(hiddenActionsRule).toContain('max-width: 0;');
    expect(hiddenActionsRule).toContain('overflow: hidden;');

    const visibleActionsRule = sourceBetween(
      groupPanelsSource,
      '.reference-sidebar-section-row:hover .reference-sidebar-section-actions,',
      '.reference-sidebar-section-action {'
    );
    expect(visibleActionsRule).toContain('max-width: var(--reference-sidebar-section-actions-max-width);');
    expect(visibleActionsRule).toContain('overflow: visible;');
  });
});
