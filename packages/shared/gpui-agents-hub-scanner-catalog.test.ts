import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

/**
 * The Agents Hub filesystem catalog is scanned by the GPUI native Rust port
 * (GpuiAgentsHubCatalogBuilder in
 * apps/desktop/src/app/helpers/agents_hub/catalog_builder.rs). Repo policy
 * forbids tests inside apps/desktop/, so this shared source test extracts every home-relative
 * catalog root/file path from that scanner and asserts the known provider
 * roots are all still present. Removing or renaming a provider root fails here.
 *
 * CDXC:AgentsHubCatalog 2026-08-20-13:05:
 * This was a two-sided parity test against the macOS `native-sidebar.tsx`
 * helper script. That host is gone, so only the GPUI side is asserted now.
 */

const gpuiMainSource = readFileSync(
  new URL('../../apps/desktop/src/app/helpers/agents_hub/catalog_builder.rs', import.meta.url),
  'utf8'
);

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function quotedSegments(argList: string): string[] {
  return [...argList.matchAll(/"([^"]*)"/g)].map((match) => match[1]!);
}

function gpuiCatalogHomePaths(): Set<string> {
  const rustScanner = sourceBetween(
    gpuiMainSource,
    'struct GpuiAgentsHubCatalogBuilder',
    'fn gpui_empty_agents_hub_catalog_build'
  );
  const paths = new Set<string>();
  // builder.home_path(&["segment", ...]) — the Rust home-relative helper.
  for (const match of rustScanner.matchAll(/home_path\(&\[([^\]]*)\]/g)) {
    paths.add(quotedSegments(match[1]!).join('/'));
  }
  // home.join("agents").join("skills") — the non-dot shared agents trees.
  for (const match of rustScanner.matchAll(/home\s*\.join\("([^"]+)"\)((?:\s*\.join\("[^"]+"\))*)/g)) {
    const chain = [match[1]!, ...[...match[2]!.matchAll(/"([^"]+)"/g)].map((segment) => segment[1]!)];
    paths.add(chain.join('/'));
  }
  return paths;
}

describe('GPUI Agents Hub scanner catalog roots', () => {
  test('the scanner references every known home-relative catalog path', () => {
    const gpuiPaths = gpuiCatalogHomePaths();

    // Guard the extraction itself: a regex or boundary regression that
    // extracts nothing must fail loudly instead of passing on empty sets.
    expect(gpuiPaths.size).toBeGreaterThanOrEqual(20);
    for (const anchor of [
      '.agents/skills',
      '.claude/CLAUDE.md',
      '.claude/skills',
      '.codex/AGENTS.md',
      '.codex/skills',
      '.config/agents/skills',
      '.config/opencode/opencode.json',
      '.config/opencode/skills',
      '.copilot/skills',
      '.cursor/skills',
      '.factory/skills',
      '.gemini/antigravity-cli/skills',
      '.gemini/antigravity/skills',
      '.gemini/skills',
      '.hermes/skills',
      '.kiro/skills',
      '.pi/agent/settings.json',
      '.pi/agent/skills',
      '.qoder/skills',
      '.rovodev/skills',
      'agents/skills',
      'agents/hooks',
    ]) {
      expect(gpuiPaths.has(anchor), `GPUI scanner lost ${anchor}`).toBe(true);
    }
  });
});
