import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, test } from 'vitest';

import { SidebarV2ProjectIcon } from './sidebar-v2-icons';

/*
 * CDXC:SidebarV2ProjectIcons 2026-07-29 (discovered icons):
 * The project-icon precedence chain, pinned where it is decided rather than only
 * through Storybook. Every case below hands the component MORE than one
 * candidate, because a test that passes a single icon proves only that the
 * variant can render, not that it outranks the others.
 *
 * Order under test: user IMAGE → discovered repository icon → typed Tabler glyph
 * → folder.
 */

const USER_IMAGE = 'data:image/png;base64,dXNlci1pbWFnZQ==';
const DISCOVERED = 'data:image/png;base64,ZGlzY292ZXJlZA==';
const TABLER = { color: '#d6e0f3', icon: 'archive', kind: 'tabler' } as const;

function render(props: Parameters<typeof SidebarV2ProjectIcon>[0]): string {
  return renderToStaticMarkup(createElement(SidebarV2ProjectIcon, props));
}

describe('SidebarV2ProjectIcon', () => {
  test('puts a user-attached image ahead of everything else', () => {
    const markup = render({
      discoveredIconDataUrl: DISCOVERED,
      icon: { dataUrl: USER_IMAGE, kind: 'image' },
      title: 'ghostex',
    });
    expect(markup).toContain('data-icon-variant="image"');
    expect(markup).toContain(USER_IMAGE);
    expect(markup).not.toContain(DISCOVERED);
  });

  test('treats the legacy iconDataUrl field as a user image too', () => {
    const markup = render({
      discoveredIconDataUrl: DISCOVERED,
      iconDataUrl: USER_IMAGE,
      title: 'ghostex',
    });
    expect(markup).toContain('data-icon-variant="image"');
    expect(markup).toContain(USER_IMAGE);
  });

  test("puts the repository's own icon ahead of a typed Tabler glyph", () => {
    /*
     * The reported bug. A typed glyph is almost never a considered choice on a
     * session row — V1 does not render them there at all, and the gpui app has
     * no picker — so a real favicon shipped by the repository is the better
     * answer whenever both exist.
     */
    const markup = render({
      discoveredIconDataUrl: DISCOVERED,
      icon: TABLER,
      title: 'ghostex',
    });
    expect(markup).toContain('data-icon-variant="discovered"');
    expect(markup).toContain(DISCOVERED);
    expect(markup).not.toContain('data-icon-variant="tabler"');
  });

  test('keeps the typed glyph as the fallback when nothing was discovered', () => {
    const markup = render({ icon: TABLER, title: 'ghostex' });
    expect(markup).toContain('data-icon-variant="tabler"');
  });

  test('falls back to the folder only when the project has no icon at all', () => {
    expect(render({ title: 'ghostex' })).toContain('data-icon-variant="glyph"');
  });

  test('refuses a discovered value that is not an image data URL', () => {
    /*
     * The value crosses a daemon boundary (possibly a remote machine's) and
     * lands in an `<img src>`, so a bad one must degrade to the next candidate
     * rather than render.
     */
    for (const hostile of [
      'https://example.invalid/icon.png',
      'data:text/html;base64,PHNjcmlwdD4=',
      'javascript:alert(1)',
    ]) {
      const markup = render({
        discoveredIconDataUrl: hostile,
        icon: TABLER,
        title: 'ghostex',
      });
      expect(markup).toContain('data-icon-variant="tabler"');
      expect(markup).not.toContain(hostile);
    }
    expect(render({ discoveredIconDataUrl: 'https://example.invalid/i.png', title: 'x' })).toContain(
      'data-icon-variant="glyph"'
    );
  });
});
