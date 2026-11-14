/**
 * Ghostex's Ed25519 signature on every on-demand component archive (GitHub #495, Q1731): the
 * release half. gxserver and the desktop check it (packages/gx-protocol/src/component_signing.rs,
 * server/src/components/mod.rs, apps/desktop/src/component_store/signature.rs).
 *
 * The private key exists only as the `GHOSTEX_COMPONENT_SIGNING_KEY` Actions secret (a PKCS#8
 * PEM). `publish-component.mjs` signs each archive it records, whether this run built it or reused
 * a published one (whose GitHub build attestation it has verified first), and the signature is
 * sealed into on-demand-resources.json as `platforms.<platform>.signature`.
 */
import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';

export const COMPONENT_SIGNING_DOMAIN = 'ghostex-component-v1';
/** Must equal COMPONENT_SIGNING_PUBLIC_KEY_HEX in packages/gx-protocol/src/component_signing.rs. */
export const COMPONENT_SIGNING_PUBLIC_KEY_HEX = '0a0c966a4663b7787a9b60248a0a2527a14ab9b904a2fcd3f5dd4fa958f665df';
export const COMPONENT_SIGNING_KEY_ENV = 'GHOSTEX_COMPONENT_SIGNING_KEY';

/**
 * The public key the release checks its own signatures with: Ghostex's, or
 * `GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY` for a test signing key (debug gxserver and desktop builds
 * read the same variable). Clients always check against the key built into them, so this only
 * changes what the release's self-check expects.
 */
export function expectedSigningPublicKeyHex(env = process.env) {
  return env.GHOSTEX_COMPONENT_SIGNING_PUBLIC_KEY?.trim() || COMPONENT_SIGNING_PUBLIC_KEY_HEX;
}

/** The bytes signed for one archive: one field per line, no trailing newline. */
export function componentSigningStatement({ name, componentVersion, platform, assetName, sha256, sizeBytes }) {
  return [
    COMPONENT_SIGNING_DOMAIN,
    name,
    componentVersion,
    platform,
    assetName,
    String(sha256).toLowerCase(),
    String(sizeBytes),
  ].join('\n');
}

/**
 * Whether this step records archives unsigned for a later signing step
 * (`GHOSTEX_COMPONENT_SIGNATURES_DEFERRED=1`, sign-component-records.mjs).
 */
export function signaturesDeferred(env = process.env) {
  return env.GHOSTEX_COMPONENT_SIGNATURES_DEFERRED === '1';
}

/** The signing key from the environment, or null. */
export function componentSigningKey(env = process.env) {
  const pem = env[COMPONENT_SIGNING_KEY_ENV];
  if (!pem || !pem.trim()) return null;
  return createPrivateKey(pem.replace(/\\n/g, '\n'));
}

export function signComponentStatement(statement, privateKey) {
  return sign(null, Buffer.from(statement, 'utf8'), privateKey).toString('base64');
}

function publicKeyFromHex(hex) {
  const raw = Buffer.from(hex, 'hex');
  if (raw.length !== 32) throw new Error('A component signing public key is 32 bytes of hex.');
  return createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: raw.toString('base64url') }, format: 'jwk' });
}

export function verifyComponentSignature(statement, signature, publicKeyHex = COMPONENT_SIGNING_PUBLIC_KEY_HEX) {
  const bytes = Buffer.from(String(signature), 'base64');
  if (bytes.length !== 64) return false;
  return verify(null, Buffer.from(statement, 'utf8'), publicKeyFromHex(publicKeyHex), bytes);
}

/**
 * The signature for one archive record, checked against the public key gxserver and the desktop
 * carry, so a wrong secret stops the release here instead of shipping archives every client refuses.
 */
export function signComponentAsset(
  { name, componentVersion, platform, assetName, sha256, sizeBytes },
  privateKey,
  publicKeyHex = expectedSigningPublicKeyHex()
) {
  const statement = componentSigningStatement({ name, componentVersion, platform, assetName, sha256, sizeBytes });
  const signature = signComponentStatement(statement, privateKey);
  if (!verifyComponentSignature(statement, signature, publicKeyHex)) {
    throw new Error(`${COMPONENT_SIGNING_KEY_ENV} does not match the component signing public key built into Ghostex.`);
  }
  return signature;
}
