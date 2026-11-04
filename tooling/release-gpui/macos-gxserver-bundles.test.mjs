import { spawnSync } from 'node:child_process';
import {
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, test } from 'vitest';

/*
 * CDXC:RemotePairing 2026-11-04: the macOS gxserver package keeps gxserver and zmx as the main
 * executables of their own app bundles so macOS keeps their privacy grants across builds, with
 * bin/ links every caller keeps using (apps/desktop/scripts/package-macos-gxserver-bundles.sh).
 */
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const script = path.join(repoRoot, 'apps/desktop/scripts/package-macos-gxserver-bundles.sh');
const codesignScript = path.join(repoRoot, 'apps/desktop/scripts/codesign-gpui-app.sh');
const onMac = process.platform === 'darwin';

const folders = [];
afterEach(() => {
  for (const folder of folders.splice(0)) {
    rmSync(folder, { force: true, recursive: true });
  }
});

function packageFrom(version = '10.46.0') {
  const root = mkdtempSync(path.join(tmpdir(), 'gxserver-bundles-'));
  folders.push(root);
  writeFileSync(path.join(root, 'gxserver-build'), '#!/bin/sh\necho gxserver\n', { mode: 0o755 });
  writeFileSync(path.join(root, 'zmx-build'), '#!/bin/sh\necho zmx\n', { mode: 0o755 });
  const packageDir = path.join(root, 'package');
  mkdirSync(packageDir);
  const run = spawnSync(
    '/bin/bash',
    [script, packageDir, path.join(root, 'gxserver-build'), path.join(root, 'zmx-build'), version],
    { encoding: 'utf8' }
  );
  expect(run.stderr).toBe('');
  expect(run.status).toBe(0);
  return packageDir;
}

function plistValue(plist, key) {
  const read = spawnSync('/usr/bin/plutil', ['-extract', key, 'raw', '-o', '-', plist], { encoding: 'utf8' });
  expect(read.status).toBe(0);
  return read.stdout.trim();
}

describe.skipIf(!onMac)('the macOS gxserver package', () => {
  test('runs gxserver and zmx from their own app bundles behind the bin/ links', () => {
    const packageDir = packageFrom();
    const programs = {
      ghostex: 'GhostexServer.app/Contents/MacOS/gxserver',
      gxserver: 'GhostexServer.app/Contents/MacOS/gxserver',
      zmx: 'GhostexSessions.app/Contents/MacOS/zmx',
    };
    for (const [tool, program] of Object.entries(programs)) {
      const link = path.join(packageDir, 'bin', tool);
      expect(lstatSync(link).isSymbolicLink()).toBe(true);
      // Every link stays inside the package, so the remote Mac install uploads it as a link.
      expect(path.isAbsolute(readlinkSync(link))).toBe(false);
      expect(realpathSync(link)).toBe(realpathSync(path.join(packageDir, program)));
    }
    expect(readFileSync(path.join(packageDir, programs.zmx), 'utf8')).toContain('echo zmx');
    expect(lstatSync(path.join(packageDir, programs.gxserver)).mode & 0o111).toBe(0o111);
  });

  test('gives each bundle a stable identifier, its program as main executable and the usage strings', () => {
    const packageDir = packageFrom('10.46.0');
    const bundles = [
      ['GhostexServer.app', 'com.madda.ghostex.gxserver', 'gxserver', 'Ghostex Server'],
      ['GhostexSessions.app', 'com.madda.ghostex.zmx', 'zmx', 'Ghostex Sessions'],
    ];
    for (const [bundle, identifier, executable, name] of bundles) {
      const plist = path.join(packageDir, bundle, 'Contents/Info.plist');
      expect(spawnSync('/usr/bin/plutil', ['-lint', '-s', plist]).status).toBe(0);
      expect(plistValue(plist, 'CFBundleIdentifier')).toBe(identifier);
      expect(plistValue(plist, 'CFBundleExecutable')).toBe(executable);
      expect(plistValue(plist, 'CFBundleName')).toBe(name);
      expect(plistValue(plist, 'CFBundlePackageType')).toBe('APPL');
      expect(plistValue(plist, 'CFBundleShortVersionString')).toBe('10.46.0');
      expect(plistValue(plist, 'LSUIElement')).toBe('true');
      for (const key of [
        'NSLocalNetworkUsageDescription',
        'NSDocumentsFolderUsageDescription',
        'NSDesktopFolderUsageDescription',
        'NSDownloadsFolderUsageDescription',
        'NSAppleEventsUsageDescription',
        'NSMicrophoneUsageDescription',
        'NSCameraUsageDescription',
      ]) {
        expect(plistValue(plist, key), `${bundle} ${key}`).toMatch(/Ghostex/);
      }
    }
  });

  test('a rerun over an existing package replaces the programs and keeps one layout', () => {
    const packageDir = packageFrom();
    const root = path.dirname(packageDir);
    writeFileSync(path.join(root, 'zmx-build'), '#!/bin/sh\necho zmx two\n', { mode: 0o755 });
    const rerun = spawnSync(
      '/bin/bash',
      [script, packageDir, path.join(root, 'gxserver-build'), path.join(root, 'zmx-build'), '10.47.0'],
      { encoding: 'utf8' }
    );
    expect(rerun.status).toBe(0);
    expect(readFileSync(path.join(packageDir, 'bin/zmx'), 'utf8')).toContain('zmx two');
    expect(plistValue(path.join(packageDir, 'GhostexSessions.app/Contents/Info.plist'), 'CFBundleVersion')).toBe(
      '10.47.0'
    );
  });
});

describe('the macOS signing recipe', () => {
  const recipe = readFileSync(codesignScript, 'utf8');

  test('signs the two gxserver bundles as bundles, before the outer app', () => {
    const bundles = recipe.indexOf('sign_gxserver_bundles "$APP_PATH/Contents/Resources/Web/gxserver"');
    const outer = recipe.lastIndexOf('"$APP_PATH"\n');
    expect(bundles).toBeGreaterThan(0);
    expect(outer).toBeGreaterThan(bundles);
    expect(recipe).toMatch(/for bundle in "\$package_dir\/GhostexSessions\.app" "\$package_dir\/GhostexServer\.app"/);
  });

  test('leaves the programs inside the bundles out of the loose Mach-O pass', () => {
    expect(recipe).toContain('\\( -name GhostexServer.app -o -name GhostexSessions.app \\) -prune -o');
  });
});
