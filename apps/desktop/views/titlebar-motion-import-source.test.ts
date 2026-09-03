import { readdirSync, readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const titlebarDir = new URL('./titlebar/', import.meta.url);
const titlebarModuleSources = readdirSync(titlebarDir)
  .filter((name) => name.endsWith('.ts') || name.endsWith('.tsx'))
  .map((name) => readFileSync(new URL(name, titlebarDir), 'utf8'));
const entrySource = readFileSync(new URL('./titlebar-host.tsx', import.meta.url), 'utf8');
const allSources = [entrySource, ...titlebarModuleSources];

describe('titlebar host motion import source', () => {
  test('does not import motion/react', () => {
    /*
     * CDXC:Navigation 2026-06-15-20:07:
     * The titlebar host must not pull the Motion runtime into the titlebar
     * bundle. The animated mode pill was replaced by an instant active state,
     * so keep this bundle-weight guard even though the mode switcher itself is
     * gone from this host. The guard now spans the entry plus every module in
     * apps/desktop/views/titlebar/ since C3.3 split the host into that folder.
     */
    for (const source of allSources) {
      expect(source).not.toMatch(/^import \{ motion \} from "motion\/react";$/m);
      expect(source).not.toContain('from "motion/react"');
    }
  });
});
