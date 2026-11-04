import { describe, expect, test } from 'vitest';

import {
  artifactNamesForProduct,
  assertLiveDependencyAlignment,
  assertUnrelatedAssetsUnchanged,
  companionProducts,
  mergeAmendProvenance,
  mergeReleaseNotes,
  mutateArtifactNames,
  packDependencies,
  resolveAmendIntent,
} from './amend-existing-lib.mjs';
import {
  customerDownloadUrl,
  DOWNLOADS_START,
  IOS_DISCORD_URL,
  renderIosAvailabilityNotes,
} from './customer-downloads.mjs';
import { releaseProvenanceAssetName } from './provenance.mjs';

const VERSION = '7.7.1';

describe('resolveAmendIntent', () => {
  test('adding Windows to a macOS+gxserver release also publishes missing WSL zips', () => {
    const intent = resolveAmendIntent({
      liveProductIds: ['macos-arm64', 'gxserver-linux-x64', 'gxserver-linux-arm64'],
      selected: ['windows-x64', 'windows-arm64'],
    });
    expect(intent.mutate).toEqual([
      'windows-x64',
      'windows-arm64',
      'gxserver-wsl-windows-x64',
      'gxserver-wsl-windows-arm64',
    ]);
    expect(intent.scope).toEqual([
      'gxserver-linux-x64',
      'gxserver-linux-arm64',
      'windows-x64',
      'windows-arm64',
      'gxserver-wsl-windows-x64',
      'gxserver-wsl-windows-arm64',
    ]);
    expect(intent.scopeFlags.macos).toBe(false);
    expect(intent.scopeFlags.windowsX64).toBe(true);
    expect(intent.scopeFlags.gxserverLinuxX64).toBe(true);
    expect(intent.forceProducts).toEqual(intent.mutate);
  });

  test('selecting only Windows x64 does not pull in ARM64', () => {
    const intent = resolveAmendIntent({
      liveProductIds: ['macos-arm64', 'gxserver-linux-x64', 'gxserver-linux-arm64'],
      selected: ['windows-x64'],
    });
    expect(intent.mutate).toEqual(['windows-x64', 'gxserver-wsl-windows-x64']);
    expect(intent.scopeFlags.windowsArm64).toBe(false);
    expect(intent.scopeFlags.gxserverLinuxArm64).toBe(false);
  });

  test('a missing pack dependency is mutated so it can be published', () => {
    const intent = resolveAmendIntent({
      liveProductIds: ['macos-arm64'],
      selected: ['windows-x64'],
    });
    expect(intent.mutate).toContain('gxserver-linux-x64');
    expect(intent.mutate).toContain('gxserver-wsl-windows-x64');
    expect(intent.mutate).toContain('windows-x64');
  });

  test('refuses to replace gxserver while a live Windows installer still embeds it', () => {
    expect(() =>
      resolveAmendIntent({
        liveProductIds: ['windows-x64', 'gxserver-linux-x64', 'gxserver-wsl-windows-x64'],
        selected: ['gxserver-linux-x64'],
      })
    ).toThrow(/desynchronize live windows-x64/u);
  });

  test('allows replacing gxserver when its consumers are also selected', () => {
    const intent = resolveAmendIntent({
      liveProductIds: ['windows-x64', 'gxserver-linux-x64', 'gxserver-wsl-windows-x64'],
      selected: ['gxserver-linux-x64', 'windows-x64', 'gxserver-wsl-windows-x64'],
    });
    expect(intent.mutate).toEqual(['gxserver-linux-x64', 'windows-x64', 'gxserver-wsl-windows-x64']);
  });

  test('refuses an empty selection', () => {
    expect(() => resolveAmendIntent({ liveProductIds: ['macos-arm64'], selected: [] })).toThrow(
      /at least one product/u
    );
  });
});

describe('product contracts', () => {
  test('Windows required artifacts include the Velopack feed', () => {
    expect(artifactNamesForProduct('windows-x64', VERSION).required).toContain(`ghostex-${VERSION}-windows-x64.exe`);
    expect(artifactNamesForProduct('windows-x64', VERSION).required).toContain('releases.win-x64-stable.json');
    expect(packDependencies('windows-arm64')).toEqual(['gxserver-linux-arm64']);
    expect(companionProducts('windows-arm64')).toEqual(['gxserver-wsl-windows-arm64']);
  });

  test('each Linux package embeds the gxserver runtime of its own architecture', () => {
    expect(packDependencies('linux-deb-x64')).toEqual(['gxserver-linux-x64']);
    expect(packDependencies('linux-tar-arm64')).toEqual(['gxserver-linux-arm64']);
    expect(artifactNamesForProduct('linux-rpm-arm64', VERSION).required).toEqual([`ghostex-${VERSION}-1.aarch64.rpm`]);
  });
});

describe('unrelated asset protection', () => {
  const dmg = {
    digest: 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    name: 'ghostex-7.7.1-arm64.dmg',
    size: 10,
  };
  const gx = {
    digest: 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
    name: 'gxserver-linux-x64.tar.gz',
    size: 20,
  };

  test('accepts new mutated assets and a rewritten provenance file', () => {
    const mutateNames = mutateArtifactNames({ mutate: ['windows-x64'], version: VERSION });
    expect(mutateNames.has(releaseProvenanceAssetName(VERSION))).toBe(true);
    assertUnrelatedAssetsUnchanged({
      afterAssets: [
        dmg,
        gx,
        {
          digest: 'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
          name: `ghostex-${VERSION}-windows-x64.exe`,
          size: 30,
        },
        {
          digest: 'sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd',
          name: releaseProvenanceAssetName(VERSION),
          size: 4,
        },
      ],
      beforeAssets: [
        dmg,
        gx,
        {
          digest: 'sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
          name: releaseProvenanceAssetName(VERSION),
          size: 3,
        },
      ],
      mutateNames,
    });
  });

  test('refuses a digest change on an unrelated live asset', () => {
    expect(() =>
      assertUnrelatedAssetsUnchanged({
        afterAssets: [
          { ...dmg, digest: 'sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff' },
          gx,
        ],
        beforeAssets: [dmg, gx],
        mutateNames: mutateArtifactNames({ mutate: ['windows-x64'], version: VERSION }),
      })
    ).toThrow(/Unrelated release asset changed/u);
  });
});

describe('live gxserver alignment', () => {
  test('requires the packed tarball to match the live archive when gxserver is not mutated', () => {
    const sha = '9e85b4f7ed8315094c0deb7e7f869fb39f1e445930d826e88f65d2650a964ece';
    assertLiveDependencyAlignment({
      liveAssets: [{ digest: `sha256:${sha}`, name: 'gxserver-linux-x64.tar.gz', size: 1 }],
      mutate: ['windows-x64'],
      packedShaByName: { 'gxserver-linux-x64.tar.gz': sha },
    });
    expect(() =>
      assertLiveDependencyAlignment({
        liveAssets: [{ digest: `sha256:${sha}`, name: 'gxserver-linux-x64.tar.gz', size: 1 }],
        mutate: ['windows-x64'],
        packedShaByName: { 'gxserver-linux-x64.tar.gz': '0'.repeat(64) },
      })
    ).toThrow(/does not match the live release/u);
  });
});

describe('release notes merge', () => {
  test('puts customer download links above the notes without checksums or provenance', () => {
    const installer = 'ghostex-7.7.1-windows-x64.exe';
    const merged = mergeReleaseNotes({
      assetNames: [installer],
      liveBody: '## 7.7.1 - 2026-08-13\n\n- Fixed chat view\n',
      version: VERSION,
    });
    expect(merged.startsWith(DOWNLOADS_START)).toBe(true);
    expect(merged.indexOf(customerDownloadUrl(VERSION, installer))).toBeLessThan(
      merged.indexOf('## 7.7.1 - 2026-08-13')
    );
    expect(merged).not.toContain('SHA256');
    expect(merged).not.toContain('Build provenance');
  });

  test('regenerates the top block in place as more platforms publish', () => {
    const dmg = 'ghostex-7.7.1-arm64.dmg';
    const apk = 'ghostex-android.apk';
    const deb = 'ghostex_7.7.1_amd64.deb';
    const first = mergeReleaseNotes({ assetNames: [dmg], liveBody: '## 7.7.1\n\n- Notes\n', version: VERSION });
    const second = mergeReleaseNotes({ assetNames: [dmg, apk, deb], liveBody: first, version: VERSION });
    const again = mergeReleaseNotes({ assetNames: [dmg, apk, deb], liveBody: second, version: VERSION });
    expect(again).toBe(second);
    expect(second.split(DOWNLOADS_START)).toHaveLength(2);
    expect(second).toContain(customerDownloadUrl(VERSION, apk));
    expect(second).toContain(customerDownloadUrl(VERSION, deb));
    const block = second.slice(0, second.indexOf('## 7.7.1'));
    expect(block.trim().split('\n').length).toBeLessThanOrEqual(5);
  });

  test('offers Linux x64 and ARM64 on the one Linux line', () => {
    const debX64 = 'ghostex_7.7.1_amd64.deb';
    const appImageX64 = 'ghostex-7.7.1-linux-x64.AppImage';
    const armAssets = [
      'ghostex_7.7.1_arm64.deb',
      'ghostex-7.7.1-1.aarch64.rpm',
      'ghostex-7.7.1-linux-arm64.tar.zst',
      'ghostex-7.7.1-linux-arm64.AppImage',
    ];
    const merged = mergeReleaseNotes({
      assetNames: [debX64, appImageX64, ...armAssets],
      liveBody: '## 7.7.1\n\n- Notes\n',
      version: VERSION,
    });
    const linuxLine = merged.split('\n').find((line) => line.startsWith('**Linux x64:**'));
    expect(linuxLine).toContain('**Linux ARM64:**');
    for (const name of [debX64, appImageX64, ...armAssets]) {
      expect(linuxLine).toContain(customerDownloadUrl(VERSION, name));
    }
    expect(linuxLine).toContain(`[AppImage](${customerDownloadUrl(VERSION, appImageX64)})`);
    expect(linuxLine.indexOf('AUR (ghostex-bin)')).toBeLessThan(linuxLine.indexOf('**Linux ARM64:**'));

    const armOnly = mergeReleaseNotes({ assetNames: armAssets, liveBody: '## 7.7.1\n\n- Notes\n', version: VERSION });
    expect(armOnly).toContain('**Linux ARM64:**');
    expect(armOnly).not.toContain('**Linux x64:**');
  });

  test('replaces legacy checksum, provenance and bottom download sections with the top block', () => {
    const dmg = 'ghostex-7.7.1-arm64.dmg';
    const apk = 'ghostex-android.apk';
    const liveBody = [
      '## 7.7.1',
      '',
      '## Download Ghostex 7.7.1',
      '',
      '### macOS ARM',
      '',
      `- [Download DMG](${customerDownloadUrl(VERSION, dmg)})`,
      '',
      '## Downloads',
      '',
      '### macos-arm64',
      '',
      `- \`${dmg}\` — SHA256 \`${'c'.repeat(64)}\``,
      '',
      '## Build provenance',
      '',
      '| Product | Status |',
    ].join('\n');
    const merged = mergeReleaseNotes({
      assetNames: [dmg, apk, 'release-provenance-7.7.1.json'],
      liveBody,
      version: VERSION,
    });
    expect(merged.startsWith(DOWNLOADS_START)).toBe(true);
    expect(merged).toContain(customerDownloadUrl(VERSION, dmg));
    expect(merged).toContain(customerDownloadUrl(VERSION, apk));
    expect(merged).toContain(renderIosAvailabilityNotes());
    expect(merged).toContain(`[Discord](${IOS_DISCORD_URL})`);
    expect(merged.indexOf('**Android:**')).toBeLessThan(merged.indexOf('iOS:'));
    expect(merged).not.toContain('## Download Ghostex');
    expect(merged).not.toContain('SHA256');
    expect(merged).not.toContain('Build provenance');
    expect(merged).not.toContain('release-provenance-7.7.1.json');
  });
});

describe('provenance merge', () => {
  test('keeps live macOS and adds mutated Windows', () => {
    const fingerprint = '1'.repeat(64);
    const macosRecord = {
      action: 'built',
      algorithmRevision: 'fp1',
      artifacts: [{ name: 'ghostex-7.7.1-arm64.dmg', sha256: '2'.repeat(64), size: 10 }],
      fingerprint,
      inputs: { composed: {}, paths: [], values: {} },
      originRunId: 1,
      originSourceSha: 'a'.repeat(40),
      originTag: 'v7.7.1',
      platform: { arch: 'arm64', os: 'macos', runnerLabel: 'macos-15' },
      product: 'macos-arm64',
      productVersion: '7.7.1',
      releaseVersion: '7.7.1',
      reusedFrom: null,
      schemaVersion: 1,
      sourceSha: 'a'.repeat(40),
      versionStamped: true,
      signing: { mode: 'developer-id+notarized' },
    };
    const windowsRecord = {
      action: 'built',
      algorithmRevision: 'fp1',
      artifacts: [{ name: 'ghostex-7.7.1-windows-x64.exe', sha256: '3'.repeat(64), size: 20 }],
      fingerprint: '4'.repeat(64),
      inputs: { composed: {}, paths: [], values: {} },
      originRunId: 2,
      originSourceSha: 'b'.repeat(40),
      originTag: 'v7.7.1',
      platform: { arch: 'x64', os: 'windows', runnerLabel: 'windows-2025' },
      product: 'windows-x64',
      productVersion: '7.7.1',
      releaseVersion: '7.7.1',
      reusedFrom: null,
      schemaVersion: 1,
      sourceSha: 'b'.repeat(40),
      versionStamped: true,
      signing: { mode: 'unsigned' },
    };
    const live = {
      algorithmRevision: 'fp1',
      components: {},
      plan: {
        expectedPlatforms: ['macos-arm64'],
        products: { 'macos-arm64': { action: 'build' } },
        schemaVersion: 1,
        scope: { macos: true, windowsX64: false },
        sourceSha: 'a'.repeat(40),
        version: '7.7.1',
      },
      products: { 'macos-arm64': macosRecord },
      publishedAt: '2026-08-13T00:00:00.000Z',
      schemaVersion: 1,
      sourceSha: 'a'.repeat(40),
      tag: 'v7.7.1',
      version: '7.7.1',
      workflowRunId: 1,
    };
    const merged = mergeAmendProvenance({
      amendPlan: {
        algorithmRevision: 'fp1',
        components: {},
        expectedPlatforms: ['windows-x64', 'gxserver-linux-x64'],
        products: {
          'macos-arm64': { action: 'skip' },
          'windows-x64': { action: 'build', fingerprint: windowsRecord.fingerprint },
          'gxserver-linux-x64': { action: 'reuse' },
        },
        schemaVersion: 1,
        scope: { macos: false, windowsX64: true, gxserverLinuxX64: true },
        sourceSha: 'b'.repeat(40),
        version: '7.7.1',
      },
      live,
      mutatedRecords: { 'windows-x64': windowsRecord },
      publishedAt: '2026-08-13T18:00:00.000Z',
      sourceSha: 'b'.repeat(40),
      version: '7.7.1',
      workflowRunId: 99,
    });
    expect(Object.keys(merged.products).sort()).toEqual(['macos-arm64', 'windows-x64']);
    expect(merged.products['macos-arm64'].artifacts[0].sha256).toBe('2'.repeat(64));
    expect(merged.products['windows-x64'].originRunId).toBe(2);
    expect(merged.plan.products['macos-arm64'].action).toBe('build');
    expect(merged.plan.products['windows-x64'].action).toBe('build');
    expect(merged.workflowRunId).toBe(99);
  });
});
