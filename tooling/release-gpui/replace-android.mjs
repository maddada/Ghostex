#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { customerDownloadUrl, mergeCustomerDownloadNotes } from './customer-downloads.mjs';

const [version, artifactDirectory] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/u.test(version ?? '')) {
  throw new Error(`Version must be MAJOR.MINOR.PATCH, got ${version ?? 'nothing'}`);
}
if (!artifactDirectory || !existsSync(artifactDirectory)) {
  throw new Error(`Android artifact directory is missing: ${artifactDirectory ?? 'nothing'}`);
}

const repo = 'maddada/Ghostex';
const tag = `v${version}`;
const apkName = 'ghostex-android.apk';
const manifestPath = path.join(artifactDirectory, 'manifest.json');
const apkPath = path.join(artifactDirectory, apkName);

function run(command, args, options = {}) {
  const output = execFileSync(command, args, {
    encoding: 'utf8',
    stdio: options.capture === false ? 'inherit' : 'pipe',
  });
  return typeof output === 'string' ? output.trim() : '';
}

function sha256(file) {
  return createHash('sha256').update(readFileSync(file)).digest('hex');
}

if (!existsSync(manifestPath) || !existsSync(apkPath)) {
  throw new Error('React Native Android artifact is missing manifest.json or ghostex-android.apk');
}
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/u, ''));
if (
  manifest.schemaVersion !== 1 ||
  manifest.platform !== 'android' ||
  manifest.version !== version ||
  manifest.source_kind !== 'react-native-mobile' ||
  manifest.application_id !== 'io.ghostex'
) {
  throw new Error(`Invalid React Native Android manifest: ${JSON.stringify(manifest)}`);
}
if (process.env.GITHUB_SHA && manifest.source_sha !== process.env.GITHUB_SHA) {
  throw new Error(
    `Android artifact source ${manifest.source_sha ?? 'missing'} does not match ${process.env.GITHUB_SHA}`
  );
}
if (manifest.artifacts?.length !== 1 || manifest.artifacts[0]?.name !== apkName) {
  throw new Error('Android replacement manifest must contain only ghostex-android.apk');
}
const expectedSha = manifest.artifacts[0].sha256;
const actualSha = sha256(apkPath);
if (expectedSha !== actualSha || manifest.artifacts[0].size !== statSync(apkPath).size) {
  throw new Error('Android replacement bytes do not match their immutable manifest');
}

const release = JSON.parse(
  run('gh', ['release', 'view', tag, '--repo', repo, '--json', 'assets,body,isDraft,isPrerelease,url'])
);
if (release.isDraft || release.isPrerelease) {
  throw new Error(`${tag} must be an existing public stable release`);
}
const currentApk = release.assets.find((asset) => asset.name === apkName);
if (!currentApk) throw new Error(`${release.url} does not contain ${apkName}`);
const nonAndroidDigests = new Map(
  release.assets.filter((asset) => asset.name !== apkName).map((asset) => [asset.name, asset.digest])
);

const updatedBody = mergeCustomerDownloadNotes(
  release.body,
  version,
  release.assets.map((asset) => asset.name)
);

const currentSha =
  typeof currentApk.digest === 'string' && currentApk.digest.startsWith('sha256:')
    ? currentApk.digest.slice('sha256:'.length)
    : '';
if (currentSha !== actualSha) {
  run('gh', ['release', 'upload', tag, '--repo', repo, apkPath, '--clobber'], {
    capture: false,
  });
}
if (updatedBody !== release.body) {
  const temporary = mkdtempSync(path.join(os.tmpdir(), `ghostex-${version}-android-notes-`));
  const notesPath = path.join(temporary, 'release-notes.md');
  writeFileSync(notesPath, updatedBody);
  run('gh', ['release', 'edit', tag, '--repo', repo, '--notes-file', notesPath], {
    capture: false,
  });
}

let verified;
for (let attempt = 0; attempt < 12; attempt += 1) {
  verified = JSON.parse(run('gh', ['release', 'view', tag, '--repo', repo, '--json', 'assets,body,url']));
  const candidate = verified.assets.find((asset) => asset.name === apkName);
  if (candidate?.digest === `sha256:${actualSha}`) break;
  execFileSync('sleep', ['2']);
}
const verifiedApk = verified.assets.find((asset) => asset.name === apkName);
if (verifiedApk?.digest !== `sha256:${actualSha}`) {
  throw new Error(`Live Android digest is ${verifiedApk?.digest ?? 'missing'}; expected sha256:${actualSha}`);
}
if (!verified.body.includes(customerDownloadUrl(version, apkName))) {
  throw new Error('Live release notes do not contain the Android download link');
}
for (const [name, digest] of nonAndroidDigests) {
  const asset = verified.assets.find((candidate) => candidate.name === name);
  if (asset?.digest !== digest) {
    throw new Error(`Unrelated release asset changed during Android replacement: ${name}`);
  }
}
const unexpectedAssets = verified.assets.filter(
  (asset) => asset.name !== apkName && !nonAndroidDigests.has(asset.name)
);
if (unexpectedAssets.length > 0) {
  throw new Error(
    `Unexpected assets appeared during Android replacement: ${unexpectedAssets.map((asset) => asset.name).join(', ')}`
  );
}
if (verified.assets.length !== release.assets.length) {
  throw new Error(`Release asset count changed from ${release.assets.length} to ${verified.assets.length}`);
}

console.log(`Replaced and verified only ${apkName} on ${verified.url}.`);
console.log(`SHA256=${actualSha}`);
