import { describe, expect, test } from 'vitest';
import { generateKeyPairSync } from 'node:crypto';
import { readFileSync } from 'node:fs';

import {
  COMPONENT_SIGNING_PUBLIC_KEY_HEX,
  componentSigningStatement,
  signComponentAsset,
  verifyComponentSignature,
} from './component-signing.mjs';
import { validateOnDemandManifestV2 } from './on-demand-manifest.mjs';
import { componentManifestRecord } from './publish-component.mjs';

function testKey() {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  return { privateKey, publicHex: publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('hex') };
}

const asset = {
  name: 'collab',
  componentVersion: '1.0.0-abc',
  platform: 'darwin-arm64',
  assetName: 'collab-1.0.0-abc-darwin-arm64.tar.gz',
  sha256: 'a'.repeat(64),
  sizeBytes: 42,
};

describe('component signatures', () => {
  test('use the statement and public key gxserver and the desktop check', () => {
    const rust = readFileSync('packages/gx-protocol/src/component_signing.rs', 'utf8');
    expect(rust).toContain(`"${COMPONENT_SIGNING_PUBLIC_KEY_HEX}"`);
    expect(componentSigningStatement(asset)).toBe(
      `ghostex-component-v1\ncollab\n1.0.0-abc\ndarwin-arm64\ncollab-1.0.0-abc-darwin-arm64.tar.gz\n${'a'.repeat(64)}\n42`
    );
    // The vector gxserver's and the desktop's tests verify.
    expect(
      verifyComponentSignature(
        componentSigningStatement(asset),
        'A/MJ7qChyfPi6XjpHxCCbF2etJ7m2YEfP5gZ+KwoptKYppGfpnAuUYUayr9yogepWp2Tn3ngvvxHs7y3eKIiAA==',
        'b388c522aec5df9f41e22e510e5c4ebfb3851662d477fee2ae872336ba128e58'
      )
    ).toBe(true);
  });

  test('a key other than the one Ghostex checks with stops the release', () => {
    const key = testKey();
    expect(() => signComponentAsset(asset, key.privateKey, COMPONENT_SIGNING_PUBLIC_KEY_HEX)).toThrow(/does not match/);
    const signature = signComponentAsset(asset, key.privateKey, key.publicHex);
    expect(verifyComponentSignature(componentSigningStatement(asset), signature, key.publicHex)).toBe(true);
    expect(
      verifyComponentSignature(componentSigningStatement({ ...asset, sizeBytes: 43 }), signature, key.publicHex)
    ).toBe(false);
  });

  test('a sealed manifest carries each archive signature, and a release refuses an unsigned one', () => {
    const key = testKey();
    const record = (signingKey) =>
      componentManifestRecord({
        assets: [
          { platform: asset.platform, assetName: asset.assetName, sha256: asset.sha256, sizeBytes: asset.sizeBytes },
        ],
        component: asset.name,
        componentVersion: asset.componentVersion,
        downloadTag: `collab-${asset.componentVersion}`,
        githubRepo: 'maddada/ghostex-components',
        signingKey,
      });
    const manifest = (component) => ({
      schemaVersion: 2,
      version: '11.0.1',
      githubRepo: 'maddada/Ghostex',
      assets: {},
      components: { collab: component },
    });
    process.env.GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY = key.publicHex;
    try {
      const signed = record(key.privateKey);
      expect(signed.platforms[asset.platform].signature).toMatch(/^[A-Za-z0-9+/]{86}==$/);
      expect(() => validateOnDemandManifestV2(manifest(signed), { requireSignatures: true })).not.toThrow();
      const tampered = structuredClone(signed);
      tampered.platforms[asset.platform].sizeBytes = 43;
      expect(() => validateOnDemandManifestV2(manifest(tampered))).toThrow(/not Ghostex's signature/);
      const unsigned = record(null);
      expect(() => validateOnDemandManifestV2(manifest(unsigned))).not.toThrow();
      expect(() => validateOnDemandManifestV2(manifest(unsigned), { requireSignatures: true })).toThrow(/not signed/);
    } finally {
      delete process.env.GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY;
    }
  });
});
