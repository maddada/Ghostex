#!/usr/bin/env node
/*
 * Same-version amend publisher. Adds or replaces the selected products on an
 * existing public tag, merges provenance, and proves every unrelated asset
 * digest is unchanged. It never creates a tag and never rewrites the original
 * changelog body.
 */

import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

import { validateOnDemandManifestV2 } from './on-demand-manifest.mjs';
import { validateWindowsUpdateFeed } from './windows-update-feed.mjs';
import { releaseProvenanceAssetName, validateReleaseProvenance } from './provenance.mjs';
import {
  PRODUCT_PROVENANCE_FILE,
  assertPlanMatchesScope,
  assertSingleBuildOrigin,
  collectPublishProvenance,
  isNonProductArtifactDirectory,
  readPublishPlan,
  renderReleaseProvenanceReport,
} from './publish-provenance.mjs';
import {
  assertLiveDependencyAlignment,
  assertUnrelatedAssetsUnchanged,
  mergeAmendProvenance,
  mergeReleaseNotes,
  mutateArtifactNames,
  packDependencies,
} from './amend-existing-lib.mjs';
import { productDefinition } from './product-inputs.mjs';
import { plannedAssetNames, resolvePublishStage } from './publish-stage.mjs';

const [version, artifactsRoot] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/u.test(version ?? '')) throw new Error('Version must be MAJOR.MINOR.PATCH');
if (!artifactsRoot || !existsSync(artifactsRoot)) throw new Error(`Artifact root is missing: ${artifactsRoot}`);

const expected = new Set(
  (process.env.GHOSTEX_RELEASE_EXPECTED_PLATFORMS ?? '')
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)
);
if (expected.size === 0) throw new Error('GHOSTEX_RELEASE_EXPECTED_PLATFORMS is empty');

const plan = readPublishPlan({
  artifactsRoot,
  env: process.env,
  fileExists: existsSync,
  readTextFile: (file) => readFileSync(file, 'utf8'),
});
assertPlanMatchesScope({ expectedPlatforms: [...expected], plan, version });

/*
 * CDXC:Release 2026-09-10 WHY:
 * Two callers. The standalone amend workflow names its mutate set explicitly
 * (GHOSTEX_RELEASE_AMEND_PRODUCTS) after resolve-amend-scope.mjs expanded it
 * against the live provenance. A stage of a staged release (GHOSTEX_RELEASE_STAGE)
 * mutates exactly its own stage's in-scope products, runs concurrently with its
 * sibling stages, and only needs the artifacts of those products plus the gxserver
 * runtimes they embed. See publish-stage.mjs.
 */
const stage = (process.env.GHOSTEX_RELEASE_STAGE ?? '').trim();
const publishStage = stage ? resolvePublishStage({ plan, stage }) : null;
if (publishStage?.initial) throw new Error(`the ${stage} stage creates the release; it is published by assemble.mjs`);
const mutate = publishStage
  ? publishStage.products
  : (process.env.GHOSTEX_RELEASE_AMEND_PRODUCTS ?? '')
      .split(',')
      .map((value) => value.trim())
      .filter(Boolean);
if (publishStage && mutate.length === 0) {
  console.log(`Publish stage ${stage}: none of its products is in this release's scope; nothing to publish.`);
  process.exit(0);
}
if (mutate.length === 0) throw new Error('GHOSTEX_RELEASE_AMEND_PRODUCTS is empty');
for (const productId of mutate) productDefinition(productId);
const needed = publishStage
  ? new Set([...mutate, ...mutate.flatMap(packDependencies).filter((productId) => expected.has(productId))])
  : expected;
if (publishStage) console.log(`Publish stage ${stage}: ${mutate.join(', ')}`);

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: options.capture ? 'pipe' : 'inherit',
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} failed${result.stderr ? `\n${result.stderr}` : ''}`);
  }
  return result.stdout?.trim() ?? '';
}

function sha256(file) {
  return createHash('sha256').update(readFileSync(file)).digest('hex');
}

function appcastReferencesRelease(xml, buildNumber, version) {
  const build = String(buildNumber).replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
  const hasBuildElement = new RegExp(`<sparkle:version>\\s*${build}\\s*</sparkle:version>`, 'u').test(xml);
  const hasBuildAttribute = new RegExp(`sparkle:version\\s*=\\s*["']${build}["']`, 'u').test(xml);
  return (hasBuildElement || hasBuildAttribute) && xml.includes(`ghostex-${version}-arm64.dmg`);
}

function readLiveAppcast() {
  const response = spawnSync('gh', ['api', 'repos/maddada/Ghostex/contents/appcast.xml?ref=main'], {
    encoding: 'utf8',
  });
  if (response.status !== 0) return '';
  const encoded = JSON.parse(response.stdout).content?.replace(/\s/gu, '') ?? '';
  return Buffer.from(encoded, 'base64').toString('utf8');
}

const sourceCommit = run('git', ['rev-parse', 'HEAD'], { capture: true });
if (spawnSync('git', ['merge-base', '--is-ancestor', plan.sourceSha, sourceCommit]).status !== 0) {
  throw new Error(`Refusing to amend: the plan's source ${plan.sourceSha} is not an ancestor of ${sourceCommit}`);
}

const manifests = [];
for (const artifactDirectory of readdirSync(artifactsRoot, { withFileTypes: true })) {
  if (!artifactDirectory.isDirectory()) continue;
  const directory = path.join(artifactsRoot, artifactDirectory.name);
  const manifestPath = path.join(directory, 'manifest.json');
  if (!existsSync(manifestPath)) {
    if (!isNonProductArtifactDirectory(artifactDirectory.name)) {
      console.log(`::warning::Ignoring artifact directory without a manifest: ${artifactDirectory.name}`);
    }
    continue;
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/u, ''));
  if (manifest.schemaVersion !== 1 || manifest.version !== version || !expected.has(manifest.platform)) {
    throw new Error(`Unexpected manifest ${manifestPath}: ${JSON.stringify(manifest)}`);
  }
  // A sibling stage's artifact is that stage's to publish.
  if (!needed.has(manifest.platform)) continue;
  if (
    manifest.platform === 'android' &&
    (manifest.source_kind !== 'react-native-mobile' || manifest.application_id !== 'io.ghostex')
  ) {
    throw new Error(
      `Android manifest must identify the React Native mobile app (got ${manifest.source_kind ?? 'unknown'} / ${manifest.application_id ?? 'unknown'})`
    );
  }
  const { required, optional } = {
    required: productDefinition(manifest.platform).artifacts(version),
    optional: productDefinition(manifest.platform).optionalArtifacts?.(version) ?? [],
  };
  const names = (manifest.artifacts ?? []).map((artifact) => artifact.name);
  for (const name of required) {
    if (!names.includes(name)) throw new Error(`${manifest.platform} is missing ${name}`);
  }
  for (const name of names) {
    if (!required.includes(name) && !optional.includes(name)) {
      throw new Error(`${manifest.platform} has unexpected artifact ${name}`);
    }
  }
  for (const artifact of manifest.artifacts ?? []) {
    if (path.basename(artifact.name) !== artifact.name) throw new Error(`Unsafe artifact name: ${artifact.name}`);
    const file = path.join(directory, artifact.name);
    if (!existsSync(file) || !statSync(file).isFile()) throw new Error(`Manifest artifact is missing: ${file}`);
    const actual = sha256(file);
    if (actual !== artifact.sha256) throw new Error(`SHA256 mismatch for ${artifact.name}`);
    if (statSync(file).size !== artifact.size) throw new Error(`Size mismatch for ${artifact.name}`);
    artifact.path = file;
  }
  manifests.push({ directory, ...manifest });
}

const received = new Set(manifests.map((manifest) => manifest.platform));
for (const platform of needed) {
  if (!received.has(platform)) throw new Error(`Enabled platform produced no validated manifest: ${platform}`);
}
for (const productId of mutate) {
  if (!received.has(productId)) throw new Error(`Amend product ${productId} produced no validated manifest`);
}

const productProvenance = collectPublishProvenance({
  manifests,
  plan,
  readProvenance: (directory) => {
    const file = path.join(directory, PRODUCT_PROVENANCE_FILE);
    if (!existsSync(file)) return null;
    return JSON.parse(readFileSync(file, 'utf8').replace(/^\uFEFF/u, ''));
  },
  version,
});
assertSingleBuildOrigin({
  expectedRunId: process.env.GHOSTEX_RELEASE_SOURCE_RUN_ID || null,
  records: Object.fromEntries(mutate.map((productId) => [productId, productProvenance[productId]])),
});

const byPlatform = new Map(manifests.map((manifest) => [manifest.platform, manifest]));
for (const arch of ['x64', 'arm64']) {
  const manifest = byPlatform.get(`windows-${arch}`);
  if (!manifest) continue;
  const feedArtifact = manifest.artifacts.find((artifact) => artifact.name === `releases.win-${arch}-stable.json`);
  validateWindowsUpdateFeed({
    arch,
    artifacts: manifest.artifacts,
    feedText: readFileSync(feedArtifact.path, 'utf8').replace(/^\uFEFF/u, ''),
    version,
  });
}

function artifactPath(platform, name) {
  const manifest = byPlatform.get(platform);
  const artifact = manifest?.artifacts.find((candidate) => candidate.name === name);
  if (!artifact) throw new Error(`${platform} is missing ${name}`);
  return artifact.path;
}

function zipEntries(zipPath) {
  const entries = run('unzip', ['-Z1', zipPath], { capture: true }).split(/\r?\n/u).filter(Boolean);
  for (const entry of entries) {
    if (entry.startsWith('/') || entry.split('/').includes('..')) {
      throw new Error(`Unsafe ZIP entry in ${zipPath}: ${entry}`);
    }
  }
  return entries;
}

function validateZipEntrySha(zipPath, expectedEntry, expectedSha) {
  const entries = zipEntries(zipPath);
  if (!entries.includes(expectedEntry)) throw new Error(`${path.basename(zipPath)} is missing ${expectedEntry}`);
  const temporary = mkdtempSync(path.join(os.tmpdir(), 'ghostex-release-zip-'));
  try {
    run('unzip', ['-q', zipPath, expectedEntry, '-d', temporary]);
    const extracted = path.join(temporary, ...expectedEntry.split('/'));
    const actual = sha256(extracted);
    if (actual !== expectedSha) {
      throw new Error(
        `${path.basename(zipPath)} embeds ${expectedEntry} with SHA256 ${actual}; expected ${expectedSha}`
      );
    }
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
}

function readZipEntryText(zipPath, expectedEntry) {
  const entries = zipEntries(zipPath);
  if (!entries.includes(expectedEntry)) throw new Error(`${path.basename(zipPath)} is missing ${expectedEntry}`);
  const result = spawnSync('unzip', ['-p', zipPath, expectedEntry], { encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`Could not read ${expectedEntry} from ${path.basename(zipPath)}: ${result.stderr}`);
  }
  return result.stdout;
}

const packedShaByName = {};
for (const arch of ['x64', 'arm64']) {
  const linuxPlatform = `gxserver-linux-${arch}`;
  const linuxName = `gxserver-linux-${arch}.tar.gz`;
  if (!byPlatform.has(linuxPlatform)) continue;
  const linuxSha = sha256(artifactPath(linuxPlatform, linuxName));
  packedShaByName[linuxName] = linuxSha;
  const wslPlatform = `gxserver-wsl-windows-${arch}`;
  if (byPlatform.has(wslPlatform)) {
    const wslZip = artifactPath(wslPlatform, `gxserver-wsl-windows-${arch}.zip`);
    validateZipEntrySha(wslZip, `gxserver-wsl-windows-${arch}/${linuxName}`, linuxSha);
  }
  const windowsPlatform = `windows-${arch}`;
  if (byPlatform.has(windowsPlatform)) {
    const portable = artifactPath(windowsPlatform, `ghostex-${version}-windows-${arch}-portable.zip`);
    const velopackPayloadRoot = 'current';
    validateZipEntrySha(portable, `${velopackPayloadRoot}/resources/wsl/${linuxName}`, linuxSha);
    const componentManifest = validateOnDemandManifestV2(
      JSON.parse(readZipEntryText(portable, `${velopackPayloadRoot}/resources/on-demand-resources.json`))
    );
    for (const componentName of ['cef', 'code-server']) {
      if (!componentManifest.components[componentName]?.platforms?.[`windows-${arch}`]) {
        throw new Error(`${path.basename(portable)} manifest v2 is missing ${componentName} for windows-${arch}`);
      }
    }
  }
}

const tag = `v${version}`;
const provenanceName = releaseProvenanceAssetName(version);

/* A sibling stage's `--clobber` removes the provenance asset for a moment; reads and writes retry through that window. */
function readLiveRelease() {
  let lastError;
  for (let attempt = 0; attempt < 6; attempt += 1) {
    try {
      const release = JSON.parse(
        run('gh', ['release', 'view', tag, '--repo', 'maddada/Ghostex', '--json', 'assets,body,isDraft,isPrerelease,url'], {
          capture: true,
        })
      );
      if (!(release.assets ?? []).some((candidate) => candidate.name === provenanceName)) {
        throw new Error(`${release.url} carries no ${provenanceName}`);
      }
      // `gh release view --json assets` reports GraphQL node ids, which the REST asset endpoint rejects; download by name.
      const provenance = validateReleaseProvenance(
        JSON.parse(
          run('gh', ['release', 'download', tag, '--repo', 'maddada/Ghostex', '--pattern', provenanceName, '--output', '-'], {
            capture: true,
          })
        )
      );
      return { provenance, release };
    } catch (error) {
      lastError = error;
      spawnSync('sleep', ['3']);
    }
  }
  throw lastError;
}

function uploadProvenanceAsset(file) {
  let lastError;
  for (let attempt = 0; attempt < 4; attempt += 1) {
    try {
      run('gh', ['release', 'upload', tag, '--repo', 'maddada/Ghostex', file, '--clobber']);
      return;
    } catch (error) {
      lastError = error;
      spawnSync('sleep', ['3']);
    }
  }
  throw lastError;
}

const first = readLiveRelease();
const liveRelease = first.release;
if (liveRelease.isDraft) throw new Error(`${tag} is still a draft`);
const expectPrerelease = publishStage ? process.env.GHOSTEX_RELEASE_PRERELEASE === '1' : false;
if (Boolean(liveRelease.isPrerelease) !== expectPrerelease) {
  throw new Error(
    publishStage
      ? `${tag} prerelease=${liveRelease.isPrerelease}; this stage expected ${expectPrerelease}`
      : `${tag} must be an existing public stable release`
  );
}
if (!run('git', ['tag', '-l', tag], { capture: true })) {
  throw new Error(`GitHub release ${tag} exists without a fetched local tag`);
}
const tagCommit = run('git', ['rev-list', '-n', '1', tag], { capture: true });
/*
 * A standalone amend runs from a later main that already contains the tag. A
 * stage of a staged release runs from the commit the release was built from,
 * and it is the tag (with the appcast commit the first stage made) that
 * contains that commit.
 */
const [ancestor, descendant] = publishStage ? [sourceCommit, tagCommit] : [tagCommit, sourceCommit];
if (spawnSync('git', ['merge-base', '--is-ancestor', ancestor, descendant]).status !== 0) {
  throw new Error(`Existing ${tag} commit ${tagCommit} and source ${sourceCommit} do not contain each other`);
}

assertLiveDependencyAlignment({
  liveAssets: liveRelease.assets,
  mutate,
  packedShaByName,
});

const mutatedRecords = Object.fromEntries(mutate.map((productId) => [productId, productProvenance[productId]]));
const mutatedManifests = manifests.filter((manifest) => mutate.includes(manifest.platform));
for (const manifest of mutatedManifests) {
  for (const artifact of manifest.artifacts) {
    run('gh', ['release', 'upload', tag, '--repo', 'maddada/Ghostex', artifact.path, '--clobber']);
  }
}

function sameProductRecord(left, right) {
  const digests = (record) =>
    [...record.artifacts]
      .map((artifact) => `${artifact.name}\0${artifact.sha256}\0${artifact.size}`)
      .sort()
      .join('|');
  return (
    Boolean(left && right) &&
    left.action === right.action &&
    left.fingerprint === right.fingerprint &&
    digests(left) === digests(right)
  );
}
const normalizeBody = (body) => String(body ?? '').replaceAll('\r\n', '\n').trimEnd();

/*
 * CDXC:Release 2026-09-10 WHY:
 * Sibling stages of a staged release amend this release at the same time, and
 * the provenance record and the notes are the only state they share. Each stage
 * merges its products into whatever is live, writes, then re-reads: when a
 * sibling's write raced past ours and dropped our products, we merge again from
 * its state. Every write is "live plus mine", so the loop converges as soon as
 * the last writer has seen everyone else's products. A standalone amend has no
 * siblings and settles on the first pass.
 */
const provenanceAssetPath = path.join(artifactsRoot, provenanceName);
const notesPath = path.join(artifactsRoot, `amend-notes-${version}.md`);
let mergedProvenance;
let verified;
for (let attempt = 0; ; attempt += 1) {
  const live = attempt === 0 ? first : readLiveRelease();
  mergedProvenance = mergeAmendProvenance({
    amendPlan: plan,
    live: live.provenance,
    mutatedRecords,
    publishedAt: new Date().toISOString(),
    sourceSha: sourceCommit,
    version,
    workflowRunId: Number(process.env.GITHUB_RUN_ID ?? 0),
  });
  writeFileSync(provenanceAssetPath, `${JSON.stringify(mergedProvenance, null, 2)}\n`);
  const provenanceSha = sha256(provenanceAssetPath);
  const releaseAssetNames = new Set((live.release.assets ?? []).map((asset) => asset.name));
  for (const manifest of mutatedManifests) {
    for (const artifact of manifest.artifacts) releaseAssetNames.add(artifact.name);
  }
  writeFileSync(notesPath, mergeReleaseNotes({ assetNames: [...releaseAssetNames], liveBody: live.release.body, version }));
  uploadProvenanceAsset(provenanceAssetPath);
  run('gh', ['release', 'edit', tag, '--repo', 'maddada/Ghostex', '--notes-file', notesPath]);

  let after;
  for (let poll = 0; poll < 12; poll += 1) {
    after = readLiveRelease();
    const provenanceAsset = after.release.assets.find((asset) => asset.name === provenanceName);
    if (provenanceAsset?.digest === `sha256:${provenanceSha}`) break;
    spawnSync('sleep', ['2']);
  }
  const liveNames = after.release.assets.map((asset) => asset.name);
  const provenanceSettled = mutate.every((productId) =>
    sameProductRecord(after.provenance.products[productId], mutatedRecords[productId])
  );
  const notesSettled =
    normalizeBody(mergeReleaseNotes({ assetNames: liveNames, liveBody: after.release.body, version })) ===
    normalizeBody(after.release.body);
  if (provenanceSettled && notesSettled) {
    verified = after.release;
    break;
  }
  if (attempt >= 8) {
    throw new Error(`${tag} did not settle after ${attempt + 1} merge attempts (provenance ${provenanceSettled}, notes ${notesSettled})`);
  }
  console.log(`A sibling publish raced this stage's write to ${tag}; merging again (attempt ${attempt + 2}).`);
}

/* Nothing outside this run's plan may change; in a staged release, sibling stages' assets are that plan's. */
assertUnrelatedAssetsUnchanged({
  afterAssets: verified.assets,
  beforeAssets: liveRelease.assets,
  mutateNames: publishStage ? plannedAssetNames({ plan, version }) : mutateArtifactNames({ mutate, version }),
});
for (const manifest of mutatedManifests) {
  for (const artifact of manifest.artifacts) {
    const live = verified.assets.find((asset) => asset.name === artifact.name);
    if (live?.digest !== `sha256:${artifact.sha256}`) {
      throw new Error(`Live digest for ${artifact.name} is ${live?.digest ?? 'missing'}`);
    }
  }
}

const updateSparkle = process.env.GHOSTEX_RELEASE_UPDATE_SPARKLE !== '0' && mutate.includes('macos-arm64');
const macos = byPlatform.get('macos-arm64');
if (updateSparkle) {
  run('git', ['config', 'user.name', 'github-actions[bot]']);
  run('git', ['config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com']);
  const [major, minor, patch] = version.split('.').map(Number);
  const buildNumber = major * 10000 + minor * 100 + patch;
  const generatedAppcast = path.join(macos.directory, 'appcast.xml');
  if (!existsSync(generatedAppcast)) throw new Error('macOS payload is missing appcast.xml');
  const generatedAppcastXml = readFileSync(generatedAppcast, 'utf8');
  if (!appcastReferencesRelease(generatedAppcastXml, buildNumber, version)) {
    throw new Error('Generated appcast does not point at the amended GPUI DMG/build');
  }
  writeFileSync('appcast.xml', generatedAppcastXml);
  run('git', ['add', 'appcast.xml']);
  run('git', ['commit', '-m', `chore: amend ${version} sparkle`]);
  const remoteMain = run('git', ['ls-remote', 'origin', 'refs/heads/main'], { capture: true }).split(/\s+/)[0];
  if (remoteMain !== sourceCommit) {
    throw new Error(`origin/main moved during the amend (${sourceCommit} -> ${remoteMain})`);
  }
  run('git', ['push', 'origin', 'HEAD:main']);
  if (!appcastReferencesRelease(readLiveAppcast(), buildNumber, version)) {
    throw new Error(`Live appcast did not advance to ${version} (${buildNumber})`);
  }
}

console.log(`Amended ${tag} with ${mutate.join(', ')} at ${verified.url}.`);
console.log(renderReleaseProvenanceReport(mergedProvenance, { plan: mergedProvenance.plan }));
