import { createHash } from 'node:crypto';

import { describe, expect, it } from 'vitest';

import {
  bundledMainChanges,
  fetchServed,
  manifestAssetKey,
  nativeChanges,
  parseManifestResponse,
  parsePlatforms,
  planOtaRelease,
  unreachableAssetKeys,
  verifyExport,
} from './ota-release-lib.mjs';

function multipart(name, body, boundary = 'ghostex-1234') {
  return {
    contentType: `multipart/mixed; boundary=${boundary}`,
    text:
      `--${boundary}\r\ncontent-disposition: form-data; name="${name}"\r\n` +
      `content-type: application/json; charset=utf-8\r\nexpo-signature: sig="x", keyid="main"\r\n\r\n` +
      `${JSON.stringify(body)}\r\n--${boundary}--\r\n`,
  };
}

const manifest = {
  id: 'a1b2',
  createdAt: '2026-10-27T10:00:00.000Z',
  runtimeVersion: 'rt-android',
  extra: { expoClient: { version: '10.38.1' }, ghostex: { version: '10.38.1', gitSha: 'abc123' } },
};
const update = { kind: 'update', id: 'a1b2', createdAt: manifest.createdAt, version: '10.38.1', gitSha: 'abc123' };

describe('parseManifestResponse', () => {
  it('reads the served update, its version and commit', () => {
    const { contentType, text } = multipart('manifest', manifest);
    expect(parseManifestResponse(200, contentType, text)).toEqual(update);
  });

  it('reports a rollback directive and an unknown runtime', () => {
    const { contentType, text } = multipart('directive', { type: 'rollBackToEmbedded' });
    expect(parseManifestResponse(200, contentType, text)).toEqual({ kind: 'rollBackToEmbedded' });
    expect(parseManifestResponse(204, null, '')).toEqual({ kind: 'none' });
  });

  it('fails on a server error instead of reading it as "no update"', () => {
    expect(() => parseManifestResponse(500, 'text/plain', 'boom')).toThrow(/500/);
    expect(() => parseManifestResponse(200, 'application/json', '{}')).toThrow(/multipart/);
  });
});

describe('fetchServed', () => {
  it('asks the public manifest route the way a phone does', async () => {
    let seen;
    const { contentType, text } = multipart('manifest', manifest);
    const fetchImpl = async (url, init) => {
      seen = { url, headers: init.headers };
      return new Response(text, { status: 200, headers: { 'content-type': contentType } });
    };
    const served = await fetchServed({ platform: 'ios', runtime: 'rt-ios', server: 'https://ota.test', fetchImpl });
    expect(served.kind).toBe('update');
    expect(seen.url).toBe('https://ota.test/v1/manifest/production');
    expect(seen.headers).toMatchObject({
      'expo-platform': 'ios',
      'expo-runtime-version': 'rt-ios',
      'expo-protocol-version': '1',
    });
  });
});

describe('planOtaRelease', () => {
  const runtimes = { android: 'rt-android', ios: 'rt-ios' };

  it('approves when every platform runtime already has installed apps', () => {
    const plan = planOtaRelease({
      platforms: ['android', 'ios'],
      runtimes,
      served: { android: update, ios: update },
      latestRelease: '10.38.1',
    });
    expect(plan).toMatchObject({ ok: true, version: '10.38.1', lastOtaGitSha: 'abc123', refusals: [], warnings: [] });
  });

  it('refuses when the native side changed (a runtime no installed app runs)', () => {
    const plan = planOtaRelease({
      platforms: ['android', 'ios'],
      runtimes,
      served: { android: { kind: 'none' }, ios: update },
      latestRelease: '10.38.1',
    });
    expect(plan.ok).toBe(false);
    expect(plan.refusals).toHaveLength(1);
    expect(plan.refusals[0]).toMatch(/android: no installed app runs runtime rt-android.*native/);
  });

  it('checks only the platforms asked for', () => {
    const plan = planOtaRelease({
      platforms: ['ios'],
      runtimes,
      served: { android: { kind: 'none' }, ios: update },
      latestRelease: '10.38.1',
    });
    expect(plan.ok).toBe(true);
    expect(plan.runtimes).toEqual({ ios: 'rt-ios' });
  });

  it('allows replacing a rollback and warns about it', () => {
    const plan = planOtaRelease({
      platforms: ['android'],
      runtimes,
      served: { android: { kind: 'rollBackToEmbedded' } },
      latestRelease: '10.38.1',
    });
    expect(plan.ok).toBe(true);
    expect(plan.lastOtaGitSha).toBeNull();
    expect(plan.warnings[0]).toMatch(/rolled back/);
  });

  it('carries the newest release version and warns when the runtime last got an older one', () => {
    const plan = planOtaRelease({
      platforms: ['android'],
      runtimes,
      served: { android: { ...update, version: '10.37.0' } },
      latestRelease: '10.38.1',
    });
    expect(plan.version).toBe('10.38.1');
    expect(plan.warnings[0]).toMatch(/last got 10\.37\.0.*show 10\.38\.1/);
  });

  it('refuses without a usable release version', () => {
    const plan = planOtaRelease({
      platforms: ['ios'],
      runtimes,
      served: { ios: update },
      latestRelease: '10.39.0-beta.1',
    });
    expect(plan.ok).toBe(false);
    expect(plan.refusals[0]).toMatch(/MAJOR\.MINOR\.PATCH/);
  });

  it('names no single base when the platforms run updates from different commits', () => {
    const plan = planOtaRelease({
      platforms: ['android', 'ios'],
      runtimes,
      served: { android: update, ios: { ...update, gitSha: 'def456' } },
      latestRelease: '10.38.1',
    });
    expect(plan.lastOtaGitSha).toBeNull();
  });
});

describe('verifyExport', () => {
  const plan = { platforms: ['android', 'ios'], runtimes: { android: 'rt-a', ios: 'rt-i' }, version: '10.38.1' };
  const good = {
    android: { platform: 'android', runtimeVersion: 'rt-a', version: '10.38.1' },
    ios: { platform: 'ios', runtimeVersion: 'rt-i', version: '10.38.1' },
  };

  it('passes an export that matches the check', () => {
    expect(verifyExport(plan, good)).toEqual([]);
  });

  it('catches a drifted runtime, a wrong version and an extra platform', () => {
    expect(verifyExport(plan, { ...good, android: { ...good.android, runtimeVersion: 'rt-new' } })[0]).toMatch(
      /rt-new/
    );
    expect(verifyExport(plan, { ...good, ios: { ...good.ios, version: '10.39.0' } })[0]).toMatch(/10\.39\.0/);
    expect(verifyExport({ ...plan, platforms: ['ios'] }, good)[0]).toMatch(/holds android, ios/);
  });
});

describe('parsePlatforms and nativeChanges', () => {
  it('maps the workflow choice to platforms', () => {
    expect(parsePlatforms('both')).toEqual(['android', 'ios']);
    expect(parsePlatforms(undefined)).toEqual(['android', 'ios']);
    expect(parsePlatforms('ios')).toEqual(['ios']);
    expect(() => parsePlatforms('web')).toThrow(/both, android or ios/);
  });

  it('keeps only runtime inputs from a changed-file list', () => {
    const changed = [
      'apps/mobile/app/src/chat/Screen.tsx',
      'apps/mobile/app/modules/ghostex-native/android/Foo.kt',
      'packages/gx-chat-core/src/lib.rs',
    ];
    const inputs = [
      'apps/mobile/app/modules/ghostex-native/android/Foo.kt',
      'packages/gx-chat-core/src/lib.rs',
      'apps/mobile/app/package.json',
    ];
    expect(nativeChanges(changed, inputs)).toEqual(changed.slice(1));
  });
});

describe('bundledMainChanges', () => {
  it('counts the chat core page inputs and the agents list as phone changes', () => {
    const changed = [
      'packages/gx-chat-core/src/lib.rs',
      'packages/gx-protocol/src/wire.rs',
      'packages/gx-chat-wasm/page/chat-core.html',
      'packages/shared/session-chat-agents.ts',
      'tooling/release-mobile/chat-core-page.sh',
      'apps/desktop/src/main.rs',
      'packages/gx-core/src/lib.rs',
      'packages/gx-chat-corex/src/lib.rs',
      'packages/gx-chat-wasm/README.md',
    ];
    expect(bundledMainChanges(changed)).toEqual(changed.slice(0, 5));
  });
});

describe('manifestAssetKey', () => {
  const bytes = Buffer.from('<!doctype html><title>chat core</title>');
  const md5 = createHash('md5').update(bytes).digest('hex');

  // Issue #393: the key must be the MD5 Metro registers the asset under, or the app never finds it.
  it('is the MD5 of the bytes, which expo export also names the file', () => {
    expect(manifestAssetKey(bytes, `assets/${md5}`)).toBe(md5);
    expect(manifestAssetKey(bytes, `assets/${md5}`)).not.toBe(createHash('sha256').update(bytes).digest('hex'));
  });

  it('refuses an export whose file name is not that MD5', () => {
    expect(() => manifestAssetKey(bytes, 'assets/chat-core.html')).toThrow(/MD5/);
  });
});

describe('unreachableAssetKeys', () => {
  it('names SHA-256 keys from updates published before the issue #393 fix', () => {
    const sha = 'a'.repeat(64);
    const md5 = 'b'.repeat(32);
    expect(unreachableAssetKeys({ assets: [{ key: md5 }, { key: sha }] })).toEqual([sha]);
    expect(unreachableAssetKeys({ assets: [{ key: md5 }] })).toEqual([]);
  });
});
