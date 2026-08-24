import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const watchModalSource = readFileSync(new URL('./watch-ghostex-video-modal.tsx', import.meta.url), 'utf8');
const sidebarStylesSource = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('watch ghostex video modal source', () => {
  test('copies the highlighted-feature shell but renders one filling walkthrough video', () => {
    /*
     * CDXC:GhostexTutorialVideo 2026-06-18-04:49:
     * The tutorial video modal should be a one-page copy of the Highlighted
     * Features shell. It must show the supplied video walkthrough, fill the
     * modal below the required title, and remove screenshot carousel behavior.
     *
     * CDXC:GhostexTutorialVideo 2026-08-08:
     * YouTube owns the encoded walkthrough so the app bundle does not carry a
     * large duplicate video asset.
     */
    expect(watchModalSource).toContain('Ghostex Features Walkthrough');
    expect(watchModalSource).toContain('https://www.youtube.com/embed/APdP-j5n4Mw?playsinline=1&rel=0');
    expect(watchModalSource).toContain('https://www.youtube.com/watch?v=APdP-j5n4Mw');
    expect(watchModalSource).toContain('<iframe');
    expect(watchModalSource).toContain("frameBorder='0'");
    expect(watchModalSource).toContain("referrerPolicy='strict-origin-when-cross-origin'");
    expect(watchModalSource).toContain('allowFullScreen');
    expect(watchModalSource).not.toContain('<video');
    expect(watchModalSource).not.toContain('ghostex-features-walkthrough.webm');
    expect(watchModalSource).not.toContain('loom.com');
    expect(watchModalSource).toContain('disablePointerDismissal');
    expect(watchModalSource).toContain('showCloseButton={false}');
    expect(watchModalSource).toContain('watch-ghostex-video-modal-dialog');
    expect(watchModalSource).toContain('watch-ghostex-video-frame');
    expect(watchModalSource).not.toContain('<img');
    expect(watchModalSource).not.toContain('IconChevronLeft');
    expect(watchModalSource).not.toContain('IconChevronRight');
    expect(watchModalSource).not.toContain('DISCOVER_GHOSTEX_FEATURES');
    expect(watchModalSource).not.toContain('ghostex-rich-prompt-editor-ctrl-g.png');

    const videoVisualStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .watch-ghostex-video-visual {',
      '.ghostex-settings-shadcn .watch-ghostex-video-frame {'
    );
    expect(videoVisualStyles).toContain('align-items: stretch;');
    expect(videoVisualStyles).toContain('grid-template-columns: minmax(0, 1fr);');

    const videoFrameStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn .watch-ghostex-video-frame {',
      '.ghostex-settings-shadcn .discover-ghostex-feature-image {'
    );
    expect(videoFrameStyles).toContain('height: calc(100% - 1px);');
    expect(videoFrameStyles).toContain('width: calc(100% - 1px);');
    expect(videoFrameStyles).toContain('border-radius: var(--settings-radius-section);');
    expect(videoFrameStyles).toContain('box-shadow: 0 0 0 0.5px');
  });
});
