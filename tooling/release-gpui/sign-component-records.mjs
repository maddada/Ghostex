/**
 * Signs the component archive records a release build wrote unsigned
 * (`GHOSTEX_COMPONENT_SIGNATURES_DEFERRED=1`, publish-component.mjs): every record in a components
 * manifest (`build/on-demand-components/components.json`) and in every sealed copy of the app's
 * `on-demand-resources.json` named on the command line.
 *
 * CDXC:Release 2026-11-14 DECISION:
 * User (Q3121): no step that runs build tooling (vite, npm, cargo) holds GHOSTEX_COMPONENT_SIGNING_KEY.
 * The build steps record and seal archives unsigned; this script is the only thing the signing step
 * runs. It signs the same statement publish-component.mjs signs (component-signing.mjs), keeps a
 * signature that is already there only when it is Ghostex's, and fails unless every record of every
 * file ends up signed, so an app never ships a manifest that clients refuse.
 *
 * Usage: node sign-component-records.mjs <components.json | on-demand-resources.json>...
 */
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  COMPONENT_SIGNING_KEY_ENV,
  componentSigningKey,
  componentSigningStatement,
  expectedSigningPublicKeyHex,
  signComponentAsset,
  verifyComponentSignature,
} from './component-signing.mjs';
import { validateOnDemandManifestV2 } from './on-demand-manifest.mjs';

/** The component map of a components manifest (`{ components }`) or of a sealed manifest. */
function componentsOf(document, file) {
  const components = document?.components;
  if (!components || typeof components !== 'object' || Array.isArray(components)) {
    throw new Error(`${file} has no components to sign.`);
  }
  return components;
}

/**
 * Signs every unsigned record of `document` in place and checks every signature it ends with.
 * Returns how many records it signed.
 */
export function signComponentRecords(
  document,
  privateKey,
  { file = 'manifest', publicKeyHex = expectedSigningPublicKeyHex() } = {}
) {
  let signed = 0;
  for (const [key, component] of Object.entries(componentsOf(document, file))) {
    const name = component?.name ?? key;
    const platforms = component?.platforms;
    if (!platforms || typeof platforms !== 'object' || Object.keys(platforms).length === 0) {
      throw new Error(`${file}: components.${key} has no platform records.`);
    }
    for (const [platform, record] of Object.entries(platforms)) {
      const fields = {
        name,
        componentVersion: component.componentVersion,
        platform,
        assetName: record.assetName,
        sha256: record.sha256,
        sizeBytes: record.sizeBytes,
      };
      const where = `${file}: components.${key}.platforms.${platform}`;
      if (record.signature === undefined) {
        record.signature = signComponentAsset(fields, privateKey, publicKeyHex);
        signed += 1;
      } else if (!verifyComponentSignature(componentSigningStatement(fields), record.signature, publicKeyHex)) {
        throw new Error(`${where} carries a signature that is not Ghostex's.`);
      }
    }
  }
  return signed;
}

export function signComponentRecordFiles(files, { env = process.env } = {}) {
  if (files.length === 0) throw new Error('Name at least one components.json or on-demand-resources.json.');
  const privateKey = componentSigningKey(env);
  if (!privateKey)
    throw new Error(`${COMPONENT_SIGNING_KEY_ENV} is not configured: component archives must be signed.`);
  const publicKeyHex = expectedSigningPublicKeyHex(env);
  const results = [];
  for (const file of files) {
    const document = JSON.parse(readFileSync(file, 'utf8'));
    const signed = signComponentRecords(document, privateKey, { file, publicKeyHex });
    // A sealed manifest is checked as the app reads it, now with every signature required.
    if (document.schemaVersion !== undefined) {
      validateOnDemandManifestV2(document, { requireSignatures: true, signingPublicKeyHex: publicKeyHex });
    }
    writeFileSync(file, `${JSON.stringify(document, null, 2)}\n`);
    results.push({ file, signed });
  }
  return results;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    for (const { file, signed } of signComponentRecordFiles(process.argv.slice(2))) {
      process.stdout.write(`Signed ${signed} component record(s) in ${file}\n`);
    }
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
