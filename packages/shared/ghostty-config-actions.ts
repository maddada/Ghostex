export const GHOSTTY_SETTINGS_DOCS_URL = 'https://ghostty.org/docs/config/reference';

/**
 * CDXC:Terminal 2026-04-30-01:48
 * Settings users need one-click Ghostty defaults, ghostex recommended settings,
 * docs, and direct config-file access. Keep the managed key list explicit for
 * older native hosts while new hosts replace only Ghostex's marked block.
 *
 * CDXC:Terminal 2026-05-22-12:29:
 * The recommended config block should match the new default Ghostex terminal
 * profile, including the requested split, SSH integration, clipboard,
 * scrollback, mouse-scroll, and JetBrains Mono variable-weight settings.
 */
export const GHOSTEX_GHOSTTY_MANAGED_CONFIG_KEYS = [
  'adjust-cell-height',
  'adjust-cell-width',
  'background',
  'clipboard-paste-protection',
  'clipboard-trim-trailing-spaces',
  'confirm-close-surface',
  'copy-on-select',
  'cursor-color',
  'cursor-style',
  'cursor-style-blink',
  'font-family',
  'font-variation',
  'font-size',
  'font-thicken',
  'font-thicken-strength',
  'foreground',
  'macos-option-as-alt',
  'mouse-hide-while-typing',
  'mouse-scroll-multiplier',
  'mouse-shift-capture',
  'scrollback-limit',
  'scrollbar',
  'selection-background',
  'shell-integration-features',
  'split-divider-color',
  'theme',
  'unfocused-split-opacity',
] as const;

export const GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES = [
  '# Applied by Ghostex:',
  'theme = GitHub Dark',
  'background = #000000',
  'foreground = #ffffff',
  'palette = 6=#39c5cf',
  'selection-background = #07284f',
  'cursor-style = bar',
  'cursor-color = #FFFFFF',
  'cursor-style-blink = true',
  '',
  'unfocused-split-opacity = 1',
  'split-divider-color = #8f8f8f',
  'mouse-shift-capture = false',
  'keybind = super+e=toggle_command_palette',
  'macos-option-as-alt = true',
  'shell-integration-features = ssh-env,ssh-terminfo',
  '',
  'font-family = "JetBrains Mono"',
  'font-size = 13',
  'adjust-cell-height = 20%',
  'adjust-cell-width = 0',
  'scrollback-limit = 15000000',
  'clipboard-trim-trailing-spaces = true',
  'clipboard-paste-protection = true',
  'copy-on-select = false',
  'confirm-close-surface = false',
  'mouse-hide-while-typing = false',
  'scrollbar = system',
  'mouse-scroll-multiplier = precision:1,discrete:1',
  'font-variation = wght=300',
] as const;

export const GHOSTEX_GHOSTTY_CONFIG_BLOCK_START = '# BEGIN Ghostex managed terminal settings';
export const GHOSTEX_GHOSTTY_CONFIG_BLOCK_END = '# END Ghostex managed terminal settings';

export function mergeGhosttyConfigLines(
  config: string,
  managedLines: readonly string[],
  managedKeys: readonly string[] = GHOSTEX_GHOSTTY_MANAGED_CONFIG_KEYS
): string {
  void managedKeys;
  const retainedLines: string[] = [];
  let insideGhostexBlock = false;
  for (const line of config.split(/\r?\n/u)) {
    const marker = line.trim();
    if (marker === GHOSTEX_GHOSTTY_CONFIG_BLOCK_START) {
      insideGhostexBlock = true;
      continue;
    }
    if (insideGhostexBlock) {
      if (marker === GHOSTEX_GHOSTTY_CONFIG_BLOCK_END) {
        insideGhostexBlock = false;
      }
      continue;
    }
    retainedLines.push(line);
  }
  trimTrailingBlankLines(retainedLines);

  if (managedLines.length === 0) {
    return retainedLines.length > 0 ? `${retainedLines.join('\n')}\n` : '';
  }

  const nextLines = [...retainedLines];
  if (nextLines.at(-1)?.trim()) {
    nextLines.push('');
  }
  nextLines.push(GHOSTEX_GHOSTTY_CONFIG_BLOCK_START, ...managedLines, GHOSTEX_GHOSTTY_CONFIG_BLOCK_END);
  return nextLines.length > 0 ? `${nextLines.join('\n')}\n` : '';
}

function trimTrailingBlankLines(lines: string[]): void {
  while (lines.at(-1)?.trim() === '') {
    lines.pop();
  }
}
