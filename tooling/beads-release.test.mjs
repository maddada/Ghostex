import { chmod, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';
import {
  BEADS_RELEASE_ARTIFACTS,
  BEADS_RELEASE_BASE_URL,
  BEADS_RELEASE_TAG,
  BEADS_PACKAGE_ID,
  BEADS_SCHEMA_VERSION,
  BEADS_SOURCE_REVISION,
  BEADS_SOURCE_REVISION_SHORT,
  BEADS_VERSION,
  beadsReleaseArtifact,
  stageBeadsRelease,
} from './beads-release.mjs';
import { smokeTestPackagedBeads } from './smoke-test-packaged-beads.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

describe('schema-compatible Beads release packaging', () => {
  test('pins the published schema-v54 artifacts and source identity for every release platform', () => {
    expect(BEADS_VERSION).toBe('1.1.0');
    expect(BEADS_SOURCE_REVISION).toBe('672d942083a1fd0c8603fa1e77620c58ba9d47c8');
    expect(BEADS_SOURCE_REVISION_SHORT).toBe('672d942083a1');
    expect(BEADS_SCHEMA_VERSION).toBe(54);
    expect(BEADS_PACKAGE_ID).toBe('1.1.0-672d942083a1-schema54');
    expect(BEADS_RELEASE_TAG).toBe('v7.2.0');
    expect(BEADS_RELEASE_BASE_URL).toBe('https://github.com/maddada/Ghostex/releases/download/v7.2.0');
    expect(BEADS_RELEASE_ARTIFACTS).toEqual({
      darwin: {
        arm64: {
          binaryPath: 'bd',
          name: 'bd-darwin-arm64.tar.gz',
          sha256: '2ea04cfd8d5081950019c745d880c17c8b5eba99d1ac5f88d769bde25e77f00b',
        },
      },
      linux: {
        arm64: {
          binaryPath: 'bin/bd',
          name: 'gxserver-linux-arm64.tar.gz',
          sha256: '106a402e7a743acfe7f235ceb10d8a907c81f65323d1b37266f166d577246e65',
        },
        x64: {
          binaryPath: 'bin/bd',
          name: 'gxserver-linux-x64.tar.gz',
          sha256: '4aab77429f5ca43d64f6f3096ff5ab33a70eee84a3cee1c043107df1773a8204',
        },
      },
    });
    expect(() => beadsReleaseArtifact('macos', 'x86_64')).toThrow(/Unsupported Beads release platform/u);
    expect(beadsReleaseArtifact('linux', 'aarch64').name).toBe('gxserver-linux-arm64.tar.gz');
  });

  test('rejects platforms that Ghostex does not package as bd binaries', () => {
    expect(() => beadsReleaseArtifact('win32', 'x64')).toThrow(/Unsupported Beads release platform/u);
    expect(() => beadsReleaseArtifact('darwin', 'riscv64')).toThrow(/Unsupported Beads release platform/u);
  });

  test('rejects an archive that does not match the published checksum', async () => {
    const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'ghostex-beads-checksum-test-'));
    try {
      const archivePath = path.join(temporaryRoot, 'bd-darwin-arm64.tar.gz');
      await writeFile(archivePath, 'tampered archive', 'utf8');
      await expect(
        stageBeadsRelease({
          arch: 'arm64',
          archivePath,
          outputPath: path.join(temporaryRoot, 'bd'),
          platform: 'darwin',
        })
      ).rejects.toThrow(/checksum mismatch/iu);
    } finally {
      await rm(temporaryRoot, { force: true, recursive: true });
    }
  });

  test('fails clearly when the pinned binary lacks embedded Dolt support', async () => {
    const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), 'ghostex-beads-cgo-test-'));
    try {
      const fakeBd = path.join(temporaryRoot, 'bd');
      await writeFile(
        fakeBd,
        `#!/bin/sh
if [ "\${1}" = version ]; then
  echo "bd version 1.1.0 (672d942083a1: fake)"
  exit 0
fi
echo "embedded Dolt requires a CGO build" >&2
exit 1
`,
        'utf8'
      );
      await chmod(fakeBd, 0o755);
      await expect(smokeTestPackagedBeads(fakeBd)).rejects.toThrow(/lacks embedded-Dolt\/CGO support/iu);
    } finally {
      await rm(temporaryRoot, { force: true, recursive: true });
    }
  });

  test('the macOS and Linux packagers stage and smoke-test the packaged binary', async () => {
    const [beadsBuilder, macosPackager, linuxPackager, macosWorkflow, runtimeWorkflow] = await Promise.all([
      readFile(path.join(repoRoot, 'tooling', 'build-pinned-beads-release.mjs'), 'utf8'),
      readFile(path.join(repoRoot, 'apps', 'desktop', 'scripts', 'prepare-macos-runtime.sh'), 'utf8'),
      readFile(path.join(repoRoot, 'server', 'package-remote-linux.mjs'), 'utf8'),
      readFile(path.join(repoRoot, '.github', 'workflows', 'release-gpui-macos.yml'), 'utf8'),
      readFile(path.join(repoRoot, '.github', 'workflows', 'release-gpui-gxserver.yml'), 'utf8'),
    ]);
    expect(beadsBuilder).toContain("CGO_ENABLED: '1'");
    expect(beadsBuilder).toContain("'gms_pure_go'");
    expect(beadsBuilder).toContain('smokeTestPackagedBeads(outputPath)');
    expect(macosPackager).toContain('tooling/beads-release.mjs');
    expect(macosPackager).toContain('tooling/smoke-test-packaged-beads.mjs');
    expect(macosPackager).toContain('GHOSTEX_BEADS_PREBUILT_BINARY');
    expect(linuxPackager).toContain('stageBeadsRelease');
    expect(linuxPackager).toContain('smokeTestPackagedBeads');
    expect(linuxPackager).toContain('GHOSTEX_BEADS_PREBUILT_BINARY');
    expect(linuxPackager).not.toContain('buildBeads(');
    for (const workflow of [macosWorkflow, runtimeWorkflow]) {
      expect(workflow).toContain('ref: 672d942083a1fd0c8603fa1e77620c58ba9d47c8');
      expect(workflow).toContain('Build pinned embedded-Dolt Beads');
      expect(workflow).toContain('GHOSTEX_BEADS_PREBUILT_BINARY');
    }
  });
});
