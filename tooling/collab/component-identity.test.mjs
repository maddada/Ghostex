import { describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { collabComponentIdentity } from './component-identity.mjs';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (file) => readFileSync(path.join(root, file), 'utf8');
const PLATFORMS = ['darwin-arm64', 'linux-x64', 'linux-arm64', 'windows-x64', 'windows-arm64'];

describe('the Collab component', () => {
  test('has a stable content-addressed identity', () => {
    const first = collabComponentIdentity('linux-x64');
    expect(first.componentVersion).toMatch(/^\d+\.\d+\.\d+-[0-9a-f]{12}$/);
    expect(collabComponentIdentity('linux-x64')).toEqual(first);
    expect(first.downloadTag).toBe(`collab-${first.componentVersion}`);
    expect(first.archiveName).toBe(`collab-${first.componentVersion}-linux-x64.tar.gz`);
  });

  test('is built and published for every platform the app ships for', () => {
    const workflow = read('.github/workflows/release-gpui-collab.yml');
    for (const platform of PLATFORMS) {
      expect(workflow).toContain(`platform: ${platform}`);
      expect(workflow).toContain(platform);
    }
    expect(workflow).toContain(`COLLAB_PLATFORMS: ${PLATFORMS.join(' ')}`);
    expect(read('.github/workflows/release-gpui.yml')).toContain('uses: ./.github/workflows/release-gpui-collab.yml');
  });

  test('is sealed by the macOS, Linux and Windows release builds, never bundled', () => {
    expect(read('apps/desktop/scripts/build-macos-app.sh')).toContain('tooling/collab/seal-component.sh');
    expect(read('apps/desktop/scripts/build-linux-app.sh')).toContain('tooling/collab/seal-component.sh');
    expect(read('apps/desktop/scripts/build-windows-app.ps1')).toContain('tooling/collab/seal-component.sh');
    expect(read('.github/workflows/release-gpui-macos.yml')).toContain('GHOSTEX_COLLAB_SEAL=1');
    expect(read('.github/workflows/release-gpui-linux.yml')).toContain("GHOSTEX_COLLAB_SEAL: '1'");
    expect(read('.github/workflows/release-gpui-windows.yml')).toContain("GHOSTEX_COLLAB_SEAL: '1'");
    expect(read('tooling/validate-macos-app-bundle.mjs')).toContain('validateCollabComponentAbsence');
  });
});
