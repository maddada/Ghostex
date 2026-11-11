#!/usr/bin/env node
/**
 * Publishes the phone app's over-the-air updates to the Ghostex update server
 * (tooling/release-mobile/ota-server/), signing each manifest or directive with the private key.
 *
 *   ota-publish.mjs publish <bundle-dir> [--channel production] [--dry-run]
 *       Publishes every platform folder ota-export.sh wrote into <bundle-dir>.
 *   ota-publish.mjs rollback-to-embedded --platform <android|ios> --runtime <version> [--channel production]
 *       Phones on that runtime version drop their downloaded update and run the code inside the app.
 *   ota-publish.mjs republish --platform <p> --runtime <r> --from <history-name> [--channel production]
 *       Serves an earlier update again (new id and date, so phones on a later update take it).
 *   ota-publish.mjs history --platform <p> --runtime <r> [--channel production]
 *
 * Environment: GHOSTEX_OTA_SIGNING_KEY (the PEM private key, or GHOSTEX_OTA_SIGNING_KEY_FILE),
 * GHOSTEX_OTA_UPLOAD_TOKEN, and optionally GHOSTEX_OTA_SERVER (default https://ota.ghostex.dev).
 * Every signature is checked against the certificate the app ships before anything is uploaded,
 * so a wrong key in the secrets fails here instead of on phones.
 */

import { createHash, createSign, createVerify, randomUUID, X509Certificate } from 'node:crypto';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { manifestAssetKey, unreachableAssetKeys } from './ota-release-lib.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SERVER = (process.env.GHOSTEX_OTA_SERVER || 'https://ota.ghostex.dev').replace(/\/+$/, '');
const KEY_ID = 'main';

const CONTENT_TYPES = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  svg: 'image/svg+xml',
  ttf: 'font/ttf',
  otf: 'font/otf',
  woff: 'font/woff',
  woff2: 'font/woff2',
  json: 'application/json',
  mp3: 'audio/mpeg',
  wav: 'audio/wav',
  mp4: 'video/mp4',
  html: 'text/html',
  js: 'application/javascript',
};

function fail(message) {
  console.error(`ota-publish: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const [command, ...rest] = argv;
  const options = { channel: 'production', dryRun: false, positional: [] };
  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index];
    if (arg === '--dry-run') options.dryRun = true;
    else if (arg.startsWith('--')) {
      const value = rest[index + 1];
      if (value === undefined) fail(`${arg} needs a value`);
      options[arg.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
      index += 1;
    } else options.positional.push(arg);
  }
  return { command, options };
}

function signingKey() {
  const file = process.env.GHOSTEX_OTA_SIGNING_KEY_FILE;
  const key = file ? readFileSync(file, 'utf8') : process.env.GHOSTEX_OTA_SIGNING_KEY;
  if (!key) fail('GHOSTEX_OTA_SIGNING_KEY (or GHOSTEX_OTA_SIGNING_KEY_FILE) is not set');
  return key;
}

function certificatePath(bundleDir) {
  const inBundle = bundleDir === undefined ? null : path.join(bundleDir, 'certificate.pem');
  if (inBundle !== null && existsSync(inBundle)) return inBundle;
  return path.join(REPO_ROOT, 'apps/mobile/app/certs/certificate.pem');
}

/** Signs `text` and proves the signature verifies against the certificate inside the app. */
function sign(text, certificateFile) {
  const signer = createSign('RSA-SHA256');
  signer.update(text, 'utf8');
  const signature = signer.sign(signingKey(), 'base64');
  const certificate = new X509Certificate(readFileSync(certificateFile));
  const verifier = createVerify('RSA-SHA256');
  verifier.update(text, 'utf8');
  if (!verifier.verify(certificate.publicKey, signature, 'base64')) {
    fail(`the signing key does not match ${certificateFile}; phones would refuse this update`);
  }
  // An Expo structured-field dictionary; base64 never contains a quote or backslash.
  return `sig="${signature}", keyid="${KEY_ID}"`;
}

async function api(method, route, { body, headers = {}, auth = true, expect = [200, 201] } = {}) {
  const token = process.env.GHOSTEX_OTA_UPLOAD_TOKEN;
  if (auth && !token) fail('GHOSTEX_OTA_UPLOAD_TOKEN is not set');
  const response = await fetch(`${SERVER}${route}`, {
    method,
    body,
    headers: { ...(auth ? { authorization: `Bearer ${token}` } : {}), ...headers },
  });
  if (!expect.includes(response.status)) {
    fail(`${method} ${route} answered ${response.status}: ${await response.text()}`);
  }
  return response;
}

function sha256(bytes) {
  const digest = createHash('sha256').update(bytes).digest();
  return { hex: digest.toString('hex'), base64url: digest.toString('base64url') };
}

/**
 * What the update says about itself in `extra.ghostex`, read by the phone's Settings > Updates
 * (apps/mobile/app/src/updates/overTheAir.ts `updateDetails`): the release version and main commit
 * (every update), the phone commit and the short "What's new" (exports from 2026-11-12 on).
 */
function ghostexDetails(info) {
  return {
    version: info.version,
    gitSha: info.gitSha,
    ...(typeof info.mobileSha === 'string' ? { mobileSha: info.mobileSha } : {}),
    ...(Array.isArray(info.whatsNew) && info.whatsNew.length > 0 ? { whatsNew: info.whatsNew } : {}),
  };
}

/** One platform folder from ota-export.sh: export/ (expo export), expo-config.json, ota.json. */
function buildUpdate(platformDir) {
  const info = JSON.parse(readFileSync(path.join(platformDir, 'ota.json'), 'utf8'));
  const exportDir = path.join(platformDir, 'export');
  const metadata = JSON.parse(readFileSync(path.join(exportDir, 'metadata.json'), 'utf8'));
  const files = metadata.fileMetadata?.[info.platform];
  if (!files?.bundle) fail(`${exportDir}/metadata.json has no ${info.platform} bundle`);
  const expoConfig = JSON.parse(readFileSync(path.join(platformDir, 'expo-config.json'), 'utf8'));
  if (expoConfig.version !== info.version) {
    fail(`${platformDir}: the app config says ${expoConfig.version}, the export says ${info.version}`);
  }

  const uploads = [];
  const entry = (relative, ext, launch) => {
    const bytes = readFileSync(path.join(exportDir, relative));
    const { hex, base64url } = sha256(bytes);
    const contentType = launch ? 'application/javascript' : CONTENT_TYPES[ext] || 'application/octet-stream';
    uploads.push({ hex, bytes, contentType });
    let key = hex;
    if (!launch) {
      // The app finds an asset by Metro's MD5, not by this SHA-256 (issue #393, see manifestAssetKey).
      try {
        key = manifestAssetKey(bytes, relative);
      } catch (error) {
        fail(error.message);
      }
    }
    return {
      hash: base64url,
      key,
      contentType,
      ...(launch ? {} : { fileExtension: `.${ext}` }),
      url: `${SERVER}/v1/assets/${hex}`,
    };
  };
  const manifest = {
    id: randomUUID(),
    createdAt: new Date().toISOString(),
    runtimeVersion: info.runtimeVersion,
    launchAsset: entry(files.bundle, null, true),
    assets: (files.assets ?? []).map((item) => entry(item.path, item.ext, false)),
    metadata: {},
    extra: { expoClient: expoConfig, ghostex: ghostexDetails(info) },
  };
  return { info, manifest, uploads };
}

async function postRecord({ channel, platform, runtime, record, dryRun }) {
  const route = `/v1/admin/updates/${channel}/${platform}/${encodeURIComponent(runtime)}`;
  if (dryRun) {
    console.log(`[dry run] POST ${route} (${record.kind}, ${record.version ?? 'no version'})`);
    return;
  }
  const response = await api('POST', route, {
    body: JSON.stringify(record),
    headers: { 'content-type': 'application/json' },
  });
  console.log(`Published ${platform} ${record.kind} for runtime ${runtime}: ${JSON.stringify(await response.json())}`);
}

async function publish(options) {
  const bundleDir = options.positional[0];
  if (!bundleDir) fail('publish needs the bundle folder ota-export.sh wrote');
  const platforms = readdirSync(bundleDir, { withFileTypes: true })
    .filter((item) => item.isDirectory() && existsSync(path.join(bundleDir, item.name, 'ota.json')))
    .map((item) => item.name);
  if (platforms.length === 0) fail(`${bundleDir} holds no platform folder with an ota.json`);

  for (const platform of platforms) {
    const { info, manifest, uploads } = buildUpdate(path.join(bundleDir, platform));
    if (info.platform !== platform) fail(`${platform}/ota.json is for ${info.platform}`);
    const body = JSON.stringify(manifest);
    const signature = sign(body, certificatePath(bundleDir));
    console.log(
      `${platform}: Ghostex ${info.version}, runtime ${info.runtimeVersion}, ${uploads.length} files, update ${manifest.id}`
    );
    for (const upload of uploads) {
      if (options.dryRun) continue;
      const head = await api('HEAD', `/v1/assets/${upload.hex}`, { auth: false, expect: [200, 404] });
      if (head.status === 200) continue;
      await api('PUT', `/v1/admin/assets/${upload.hex}`, {
        body: upload.bytes,
        headers: { 'content-type': upload.contentType },
      });
    }
    await postRecord({
      channel: options.channel,
      platform,
      runtime: info.runtimeVersion,
      record: { kind: 'update', version: info.version, createdAt: manifest.createdAt, body, signature },
      dryRun: options.dryRun,
    });
  }
}

function requireTarget(options) {
  if (options.platform !== 'android' && options.platform !== 'ios') fail('--platform must be android or ios');
  if (!options.runtime) fail('--runtime is required');
}

async function rollbackToEmbedded(options) {
  requireTarget(options);
  const createdAt = new Date().toISOString();
  const body = JSON.stringify({ type: 'rollBackToEmbedded', parameters: { commitTime: createdAt } });
  await postRecord({
    channel: options.channel,
    platform: options.platform,
    runtime: options.runtime,
    record: { kind: 'rollBackToEmbedded', version: null, createdAt, body, signature: sign(body, certificatePath()) },
    dryRun: options.dryRun,
  });
}

async function history(options) {
  requireTarget(options);
  const response = await api(
    'GET',
    `/v1/admin/updates/${options.channel}/${options.platform}/${encodeURIComponent(options.runtime)}/history`
  );
  for (const name of (await response.json()).history) console.log(name);
}

async function republish(options) {
  requireTarget(options);
  if (!options.from) fail('--from <history-name> is required (see the history command)');
  const route = `/v1/admin/updates/${options.channel}/${options.platform}/${encodeURIComponent(options.runtime)}`;
  const old = await (await api('GET', `${route}/history/${options.from}`)).json();
  if (old.kind !== 'update') fail(`${options.from} is a ${old.kind}, not an update`);
  const oldManifest = JSON.parse(old.body);
  const unreachable = unreachableAssetKeys(oldManifest);
  if (unreachable.length > 0) {
    fail(
      `${options.from} keys ${unreachable.length} assets by SHA-256 (published before the issue #393 fix); ` +
        'the app cannot find them, and on iOS chat never starts. Publish a fresh export instead.'
    );
  }
  const manifest = { ...oldManifest, id: randomUUID(), createdAt: new Date().toISOString() };
  const body = JSON.stringify(manifest);
  await postRecord({
    channel: options.channel,
    platform: options.platform,
    runtime: options.runtime,
    record: {
      kind: 'update',
      version: old.version,
      createdAt: manifest.createdAt,
      body,
      signature: sign(body, certificatePath()),
    },
    dryRun: options.dryRun,
  });
}

const { command, options } = parseArgs(process.argv.slice(2));
const commands = { publish, 'rollback-to-embedded': rollbackToEmbedded, republish, history };
if (!commands[command]) {
  console.error(readFileSync(fileURLToPath(import.meta.url), 'utf8').split('*/')[0]);
  process.exit(2);
}
await commands[command](options);
