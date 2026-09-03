import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const commandIconPickerSource = readFileSync(new URL('./command-icon-picker.tsx', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('command icon picker source', () => {
  test('closes the popover before parent editor state updates', () => {
    /*
     * CDXC:AgentLauncher 2026-06-19-19:52:
     * Settings action icon selection must close the portaled Popover before
     * updating parent editor state so re-renders cannot leave the picker owning
     * focus.
     */
    const optionSelect = sourceBetween(commandIconPickerSource, 'onSelect={() => {', 'value={option.label}');

    expect(commandIconPickerSource).toContain("import { flushSync } from 'react-dom';");
    expect(optionSelect).toContain('flushSync(() => {');
    expect(optionSelect).toContain('setIsOpen(false);');
    expect(optionSelect.indexOf('setIsOpen(false);')).toBeLessThan(optionSelect.indexOf('onIconChange(option.icon);'));
  });
});
