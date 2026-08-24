import { describe, expect, test } from 'vitest';
import {
  GHOSTEX_GHOSTTY_CONFIG_BLOCK_END,
  GHOSTEX_GHOSTTY_CONFIG_BLOCK_START,
  mergeGhosttyConfigLines,
  GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES,
} from './ghostty-config-actions';

describe('mergeGhosttyConfigLines', () => {
  test('applies recommended Ghostty settings without removing user config', () => {
    /**
     * CDXC:GhosttySettings 2026-04-30-01:48
     * Applying recommended settings must replace only Ghostex's marked block and
     * retain user-owned settings even when they use the same Ghostty keys.
     * CDXC:Branding 2026-05-12-07:35
     * The inserted marker is user-visible in Ghostty config, so it should use
     * Ghostex even though the managed-key constants keep their ghostex prefix.
     */
    expect(
      mergeGhosttyConfigLines(
        [
          'keybind = cmd+t=new_tab',
          'keybind = super+e=previous_value',
          'palette = 1=#ff0000',
          'palette = 6=#old',
          'theme = Dracula',
          'font-size = 18',
          'window-padding-x = 4',
        ].join('\n'),
        GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES
      )
    ).toContain(
      [
        'keybind = cmd+t=new_tab',
        'keybind = super+e=previous_value',
        'palette = 1=#ff0000',
        'palette = 6=#old',
        'theme = Dracula',
        'font-size = 18',
        'window-padding-x = 4',
        '',
        GHOSTEX_GHOSTTY_CONFIG_BLOCK_START,
        '# Applied by Ghostex:',
        'theme = GitHub Dark',
      ].join('\n')
    );
    expect(
      mergeGhosttyConfigLines(
        [
          'keybind = super+e=previous_value',
          GHOSTEX_GHOSTTY_CONFIG_BLOCK_START,
          'theme = Old Ghostex Value',
          GHOSTEX_GHOSTTY_CONFIG_BLOCK_END,
        ].join('\n'),
        GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES
      )
    ).not.toContain('Old Ghostex Value');
  });

  test('resets ghostex-managed Ghostty settings to defaults', () => {
    expect(
      mergeGhosttyConfigLines(
        [
          'theme = Dracula',
          'font-size = 18',
          GHOSTEX_GHOSTTY_CONFIG_BLOCK_START,
          'theme = GitHub Dark',
          'font-size = 13',
          GHOSTEX_GHOSTTY_CONFIG_BLOCK_END,
          'window-padding-x = 4',
        ].join('\n'),
        []
      )
    ).toBe('theme = Dracula\nfont-size = 18\nwindow-padding-x = 4\n');
  });

  test('starts a fresh config with the Ghostex block marker', () => {
    expect(
      mergeGhosttyConfigLines('', GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES).startsWith(
        `${GHOSTEX_GHOSTTY_CONFIG_BLOCK_START}\n`
      )
    ).toBe(true);
  });
});
