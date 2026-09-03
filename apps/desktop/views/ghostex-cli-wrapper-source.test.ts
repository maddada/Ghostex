import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const releaseGhostexSource = readFileSync(new URL('../../../tooling/release-ghostex.mjs', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('Ghostex CLI command wrappers', () => {
  test('Homebrew cask generation installs wrappers instead of CLI binary aliases', () => {
    /*
     * CDXC:Cli 2026-06-12-09:31:
     * Release automation must not reintroduce Homebrew binary stanzas for
     * ghostex/gx, because those stanzas create symlinks back into Ghostex.app.
     */
    const renderer = sourceBetween(releaseGhostexSource, 'function renderGhostexCaskForTap', 'async function main');

    expect(renderer).toContain('function renderGhostexCask');
    expect(renderer).toContain('function validateGhostexCask');
    expect(renderer).toContain('postflight do');
    expect(renderer).toContain('command_path.write <<~EOS');
    expect(renderer).toContain('exec "#{cli_binary}" "$@"');
    expect(renderer).toContain('system "/usr/bin/xattr", "-d", attribute, command_path.to_s');
    expect(renderer).toContain('uninstall_preflight do');
    expect(renderer).toContain('Ghostex cask must install wrapper files, not Homebrew binary aliases.');
    expect(releaseGhostexSource).not.toContain('--except-cops Homebrew/OSDependsOn');
    expect(releaseGhostexSource).toContain('HOMEBREW_NO_INSTALL_FROM_API=1 brew style --fix');
    expect(releaseGhostexSource).toContain('depends_on macos: :ventura');
    expect(releaseGhostexSource).not.toContain('depends_on macos: ">= :ventura"');
    expect(renderer).not.toContain('const ghostexBinary');
    expect(renderer).not.toContain('const gxBinary');
  });
});
