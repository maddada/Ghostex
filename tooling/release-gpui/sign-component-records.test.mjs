import { execFileSync } from 'node:child_process';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';
import { buildOnDemandManifestV2, validateOnDemandManifestV2 } from './on-demand-manifest.mjs';
import { componentManifestRecord } from './publish-component.mjs';
import { signComponentRecordFiles } from './sign-component-records.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const digest = 'b'.repeat(64);

function testKey() {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const publicKeyHex = Buffer.from(publicKey.export({ format: 'jwk' }).x, 'base64url').toString('hex');
  return { pem: privateKey.export({ format: 'pem', type: 'pkcs8' }).toString(), publicKeyHex };
}

function unsignedRecord() {
  return componentManifestRecord({
    assets: [{ platform: 'linux-x64', assetName: 'cef-1.0.0-linux-x64.tar.gz', sha256: digest, sizeBytes: 42 }],
    component: 'cef',
    componentVersion: '1.0.0',
    downloadTag: 'cef-1.0.0',
    githubRepo: 'maddada/ghostex-components',
  });
}

function files() {
  const folder = mkdtempSync(path.join(tmpdir(), 'sign-records-'));
  const components = path.join(folder, 'components.json');
  const sealed = path.join(folder, 'on-demand-resources.json');
  writeFileSync(components, `${JSON.stringify({ components: { cef: unsignedRecord() } }, null, 2)}\n`);
  const manifest = buildOnDemandManifestV2({ version: '1.0.0', assets: {}, components: { cef: unsignedRecord() } });
  writeFileSync(sealed, `${JSON.stringify(manifest, null, 2)}\n`);
  return { components, sealed };
}

describe('sign-component-records', () => {
  test('signs every unsigned record of a components manifest and a sealed manifest', () => {
    const key = testKey();
    const { components, sealed } = files();
    const env = { GHOSTEX_COMPONENT_SIGNING_KEY: key.pem, GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY: key.publicKeyHex };
    const results = signComponentRecordFiles([components, sealed], { env });
    expect(results.map((result) => result.signed)).toEqual([1, 1]);
    const signedSeal = JSON.parse(readFileSync(sealed, 'utf8'));
    expect(() =>
      validateOnDemandManifestV2(signedSeal, { requireSignatures: true, signingPublicKeyHex: key.publicKeyHex })
    ).not.toThrow();
    const signedRecord = JSON.parse(readFileSync(components, 'utf8')).components.cef.platforms['linux-x64'];
    expect(signedRecord.signature).toBe(signedSeal.components.cef.platforms['linux-x64'].signature);
    // Signing again keeps the signatures it made.
    expect(signComponentRecordFiles([components, sealed], { env }).map((result) => result.signed)).toEqual([0, 0]);
  });

  test('refuses a signature that is not made with the key', () => {
    const key = testKey();
    const other = testKey();
    const { components } = files();
    signComponentRecordFiles([components], {
      env: { GHOSTEX_COMPONENT_SIGNING_KEY: other.pem, GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY: other.publicKeyHex },
    });
    expect(() =>
      signComponentRecordFiles([components], {
        env: { GHOSTEX_COMPONENT_SIGNING_KEY: key.pem, GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY: key.publicKeyHex },
      })
    ).toThrow(/not Ghostex's/u);
  });

  test('needs the key', () => {
    const { components } = files();
    expect(() => signComponentRecordFiles([components], { env: {} })).toThrow(/GHOSTEX_COMPONENT_SIGNING_KEY/u);
  });
});

describe('deferred component signatures', () => {
  test('a deferred publish refuses to run while the signing key is present', () => {
    const key = testKey();
    const folder = mkdtempSync(path.join(tmpdir(), 'deferred-publish-'));
    let failure = '';
    try {
      execFileSync(
        process.execPath,
        [
          path.join(here, 'publish-component.mjs'),
          '--metadata-only',
          '--component',
          'cef',
          '--version',
          '1.0.0',
          '--asset-dir',
          folder,
          '--output',
          path.join(folder, 'components.json'),
        ],
        {
          env: {
            ...process.env,
            GHOSTEX_COMPONENT_SIGNATURES_DEFERRED: '1',
            GHOSTEX_COMPONENT_SIGNING_KEY: key.pem,
            GHOSTEX_REQUIRE_COMPONENT_SIGNATURES: '1',
          },
          stdio: 'pipe',
        }
      );
    } catch (error) {
      failure = String(error.stderr ?? error.message);
    }
    expect(failure).toMatch(/must not reach a step that defers/u);
  });

  test('a deferred seal accepts unsigned records and a release seal does not', async () => {
    const folder = mkdtempSync(path.join(tmpdir(), 'deferred-seal-'));
    const build = path.join(folder, 'assets.json');
    const components = path.join(folder, 'components.json');
    writeFileSync(build, JSON.stringify({ version: '1.0.0', assets: [] }));
    writeFileSync(components, JSON.stringify({ components: { cef: unsignedRecord() } }));
    const seal = (env) =>
      execFileSync(
        process.execPath,
        [
          path.join(here, 'on-demand-manifest.mjs'),
          'seal',
          '--build-manifest',
          build,
          '--component-manifest',
          components,
          '--output',
          path.join(folder, 'sealed.json'),
        ],
        { env: { ...process.env, GHOSTEX_REQUIRE_COMPONENT_SIGNATURES: '1', ...env }, stdio: 'pipe' }
      );
    expect(() => seal({ GHOSTEX_COMPONENT_SIGNATURES_DEFERRED: '1' })).not.toThrow();
    expect(() => seal({ GHOSTEX_COMPONENT_SIGNATURES_DEFERRED: '0' })).toThrow();
  });
});
