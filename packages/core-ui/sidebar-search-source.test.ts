import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const referenceChromeSource = readFileSync(new URL('./sidebar-app/reference-chrome.tsx', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('sidebar search source', () => {
  test('opens Quick Access Search from a stable nav-row button', () => {
    /*
     * CDXC:SidebarSearch 2026-06-19-13:52:
     * The top Search entry remains a stable nav-row button and opens the shared
     * Quick Access search surface. Search input ownership belongs to that
     * surface instead of being duplicated inside the sidebar row.
     */
    const searchItemSource = sourceBetween(
      referenceChromeSource,
      'function SidebarReferenceSearchNavItem({',
      'function SidebarReferenceNavButton({'
    );

    expect(searchItemSource).toContain("className='reference-sidebar-search-slot'");
    expect(searchItemSource).toContain("className='reference-sidebar-nav-button'");
    expect(searchItemSource).toContain('onClick={onSearch}');
    expect(searchItemSource).toContain("<span className='reference-sidebar-nav-label'>Search</span>");
    expect(searchItemSource).toContain('<IconSearch');
    expect(searchItemSource).toContain('reference-sidebar-search-icon');
    expect(searchItemSource).toContain('reference-sidebar-nav-shortcut');
    expect(searchItemSource).not.toContain('reference-sidebar-inline-search-input');
    expect(searchItemSource).not.toContain('<SidebarSessionSearchField');
  });
});
