import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

/*
 * CDXC:Release 2026-11-18 WHY: crash #653: every macOS release through 11.9.0 linked a Debug
 * GhosttyKit, because the release's `zig build` passed no -Doptimize (Zig's default is Debug) while
 * every other Ghostty build path passes ReleaseSafe. The kit is built in three places and cached
 * under a key; these tests keep all of them on ReleaseSafe, and keep the cache key naming the mode
 * so a cached kit of another mode is never restored.
 */

const repoFile = (relativePath) => readFileSync(new URL(`../${relativePath}`, import.meta.url), 'utf8');

/* Each `zig build ... -Demit-xcframework ...` command, joined across `\` line continuations. */
function ghosttyKitBuildCommands(source) {
  const joined = source.replace(/\\\r?\n\s*/gu, ' ');
  return joined
    .split(/\r?\n/u)
    .filter((line) => / build .*-Demit-xcframework(?!=false)/u.test(line) && !/^\s*#/u.test(line));
}

const BUILD_SITES = [
  'tooling/release-gpui/macos.sh',
  'tooling/release-gpui/macos-prerequisite.sh',
  '.github/workflows/warm-rust-build-cache.yml',
];

describe('GhosttyKit release builds', () => {
  test.each(BUILD_SITES)('%s builds GhosttyKit with -Doptimize=ReleaseSafe', (file) => {
    const commands = ghosttyKitBuildCommands(repoFile(file));
    expect(commands.length).toBeGreaterThan(0);
    for (const command of commands) {
      expect(command).toContain('-Doptimize=ReleaseSafe');
    }
  });

  test('the GhosttyKit cache key names the optimize mode, the same in the release and warm jobs', () => {
    const keys = ['.github/workflows/release-gpui-macos.yml', '.github/workflows/warm-rust-build-cache.yml'].map(
      (file) => repoFile(file).match(/key: (warm-ghosttykit-[^\n]*)/u)?.[1]
    );
    expect(keys[0]).toContain('-releasesafe-');
    expect(keys[1]).toBe(keys[0]);
  });
});

describe('release symbol map', () => {
  test('the macOS job keeps the link map as an artifact named by version and build id', () => {
    const workflow = repoFile('.github/workflows/release-gpui-macos.yml');
    expect(workflow).toContain('run: tooling/release-gpui/macos.sh --phase symbols "${{ inputs.version }}"');
    expect(workflow).toContain(
      'name: ghostex-symbols-${{ inputs.version }}-macos-arm64-${{ steps.symbols.outputs.build_id }}'
    );
    // The symbols are taken after signing, from the bundle that ships.
    expect(workflow.indexOf('--phase symbols')).toBeGreaterThan(workflow.indexOf('--phase sign-app'));
    expect(workflow.indexOf('--phase symbols')).toBeLessThan(workflow.indexOf('--phase dmg'));
  });

  test('every macOS release phase links with the link map variable set', () => {
    const script = repoFile('tooling/release-gpui/macos.sh');
    expect(script).toMatch(/^export GHOSTEX_LINK_MAP_DIR=/mu);
    expect(script).toMatch(/^symbols\) phase_symbols ;;$/mu);
    expect(repoFile('apps/desktop/build.rs')).toContain('"cargo:rustc-link-arg-bin=ghostex-gpui=-Wl,-map,{}"');
  });
});
