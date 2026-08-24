import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { DEFAULT_ghostex_HOTKEYS } from './ghostex-hotkeys';

/**
 * GPUI mirrors the shared default hotkey table (DEFAULT_ghostex_HOTKEYS in
 * packages/shared/ghostex-hotkeys.ts) as the Rust constant GPUI_DEFAULT_GHOSTEX_HOTKEYS
 * in apps/desktop/src/app/hotkeys.rs, because macOS overlays defaults at read time and never
 * persists them, so fresh GPUI installs must resolve the same defaults
 * natively. Repo policy forbids tests inside apps/desktop/, so this shared test loads
 * the TypeScript truth directly (the module is importable, unlike the
 * string-embedded scripts the scanner-parity precedent has to regex) and
 * extracts the Rust table from source, then asserts the id → default-chord
 * maps are identical. Adding, removing, renaming, or rebinding a default on
 * one side without the other fails here.
 */

const gpuiMainSource = readFileSync(new URL('../../apps/desktop/src/app/hotkeys.rs', import.meta.url), 'utf8');

function gpuiDefaultHotkeys(): Record<string, string> {
  const start = gpuiMainSource.indexOf('const GPUI_DEFAULT_GHOSTEX_HOTKEYS');
  expect(start).toBeGreaterThanOrEqual(0);
  const end = gpuiMainSource.indexOf('];', start);
  expect(end).toBeGreaterThan(start);
  const table = gpuiMainSource.slice(start, end);
  const entries: Record<string, string> = {};
  for (const match of table.matchAll(/\("([^"]*)",\s*"([^"]*)"\)/g)) {
    const [, actionId, defaultKey] = match;
    expect(entries, `duplicate Rust default for ${actionId}`).not.toHaveProperty(actionId!);
    entries[actionId!] = defaultKey!;
  }
  return entries;
}

describe('GPUI default hotkey table parity with packages/shared/ghostex-hotkeys.ts', () => {
  test('Rust GPUI_DEFAULT_GHOSTEX_HOTKEYS matches DEFAULT_ghostex_HOTKEYS', () => {
    const gpuiDefaults = gpuiDefaultHotkeys();

    // Guard the extraction itself: a marker or regex regression that extracts
    // nothing must fail loudly instead of passing on an empty object.
    expect(Object.keys(gpuiDefaults).length).toBeGreaterThanOrEqual(40);

    expect(gpuiDefaults).toEqual({ ...DEFAULT_ghostex_HOTKEYS });
  });
});
