import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { crateEdition, editedPaths, manifestEditions, ownedPath } from './format-edited-file.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

describe('editedPaths', () => {
  it('reads the file Claude Code edited', () => {
    expect(editedPaths({ tool_input: { file_path: '/repo/a.rs' } })).toEqual(['/repo/a.rs']);
  });

  it('reads every file a Codex patch added, updated or moved to, relative to its cwd', () => {
    const command = [
      '*** Begin Patch',
      '*** Update File: src/a.rs',
      '@@',
      '-x',
      '+y',
      '*** Add File: src/b.ts',
      '+export {};',
      '*** Update File: src/c.rs',
      '*** Move to: src/d.rs',
      '*** Delete File: src/e.rs',
      '*** End Patch',
    ].join('\n');
    expect(editedPaths({ cwd: '/repo', tool_input: { command } })).toEqual([
      '/repo/src/a.rs',
      '/repo/src/b.ts',
      '/repo/src/c.rs',
      '/repo/src/d.rs',
    ]);
  });

  it('finds nothing in a tool input without paths', () => {
    expect(editedPaths({ tool_input: { command: 'ls' } })).toEqual([]);
    expect(editedPaths({})).toEqual([]);
  });
});

describe('manifestEditions', () => {
  it('reads the package edition past arrays and other tables', () => {
    const text = '[package]\nname = "x"\nauthors = ["a"]\nedition = "2024"\n\n[dependencies]\nedition = "no"\n';
    expect(manifestEditions(text)).toMatchObject({ package: '2024' });
  });

  it('marks an inherited edition and reads the workspace one', () => {
    const text = '[workspace.package]\nedition = "2021"\n\n[package]\nedition.workspace = true\n';
    expect(manifestEditions(text)).toEqual({ 'workspace.package': '2021', package: 'workspace' });
  });
});

describe('crateEdition', () => {
  it("uses the edition of the file's own crate", () => {
    expect(crateEdition(path.join(root, 'apps/desktop/src/main.rs'))).toBe('2024');
    expect(crateEdition(path.join(root, 'server/src/main.rs'))).toBe('2021');
  });
});

describe('ownedPath', () => {
  it('accepts app-owned files', () => {
    expect(ownedPath(path.join(root, 'apps/desktop/src/main.rs'))).toBe('apps/desktop/src/main.rs');
    expect(ownedPath(path.join(root, 'tooling/format/format-edited-file.mjs'))).toBe(
      'tooling/format/format-edited-file.mjs'
    );
  });

  it('refuses dependencies, the lockfile, missing files and files outside the repository', () => {
    expect(ownedPath(path.join(root, 'node_modules/prettier/index.mjs'))).toBeNull();
    expect(ownedPath(path.join(root, 'bun.lock'))).toBeNull();
    expect(ownedPath(path.join(root, 'no-such-file.rs'))).toBeNull();
    expect(ownedPath('/etc/hosts')).toBeNull();
  });
});
