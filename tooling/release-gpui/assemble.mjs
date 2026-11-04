#!/usr/bin/env node
import { createHash } from 'node:crypto';
import {
  appendFileSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { validateOnDemandManifestV2 } from './on-demand-manifest.mjs';
import { validateWindowsUpdateFeed } from './windows-update-feed.mjs';
import { renderCustomerDownloadNotes } from './customer-downloads.mjs';
import { releaseProvenanceAssetName } from './provenance.mjs';
import {
  advanceMainWithAppcast,
  appcastReferencesRelease,
  classifyMainDrift,
  readLiveAppcast,
  releaseBuildNumber,
  waitForLiveAppcast,
} from './appcast-advance.mjs';
import { LOST_CREATION_RACE_EXIT_CODE, resolvePublishStage } from './publish-stage.mjs';
import {
  PRODUCT_PROVENANCE_FILE,
  assertLiveProvenanceMatches,
  assertPlanMatchesScope,
  assertSingleBuildOrigin,
  buildReleaseProvenanceRecord,
  collectPublishProvenance,
  isNonProductArtifactDirectory,
  readPublishPlan,
  renderBuildProvenanceNotes,
  renderReleaseProvenanceReport,
  resolveMacosFeedScope,
  resolveWindowsFeedScope,
} from './publish-provenance.mjs';

const [version, artifactsRoot] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? '')) throw new Error('Version must be MAJOR.MINOR.PATCH');
if (!artifactsRoot || !existsSync(artifactsRoot)) throw new Error(`Artifact root is missing: ${artifactsRoot}`);

const expected = new Set(
  (process.env.GHOSTEX_RELEASE_EXPECTED_PLATFORMS ?? '')
    .split(',')
    .map((value) => value.trim())
    .filter(Boolean)
);
if (expected.size === 0) throw new Error('GHOSTEX_RELEASE_EXPECTED_PLATFORMS is empty');

/*
 * CDXC:Release 2026-08-13:
 * The resolved plan decides what this release contains. It arrives inline from
 * the parent workflow and, for a publish-only recovery, also as the source run's
 * `release-plan` artifact; when both are present they must agree exactly.
 * Everything below validates plan <-> manifest <-> provenance three ways before
 * a single byte is uploaded.
 */
const plan = readPublishPlan({
  artifactsRoot,
  env: process.env,
  fileExists: existsSync,
  readTextFile: (file) => readFileSync(file, 'utf8'),
});
assertPlanMatchesScope({ expectedPlatforms: [...expected], plan, version });

/*
 * CDXC:Release 2026-09-15 WHY:
 * In a staged release (GHOSTEX_RELEASE_STAGE set) this script runs for the
 * stage that arrives first: it creates the tag and the release with that
 * stage's products only, and every later stage amends the release
 * (amend-existing.mjs). `expected` stays the whole scope so the plan check
 * above is unchanged; `publishing` is what this invocation actually uploads.
 * When the remote tag appears under this script, another stage won the race
 * and this one exits LOST_CREATION_RACE_EXIT_CODE so publish-staged.mjs can
 * amend instead. See publish-stage.mjs.
 */
const stage = (process.env.GHOSTEX_RELEASE_STAGE ?? '').trim();
const publishStage = stage ? resolvePublishStage({ plan, stage }) : null;
const publishing = new Set(publishStage ? publishStage.products : expected);
if (publishStage) console.log(`Publish stage ${stage} creates the release with ${[...publishing].join(', ')}`);

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: options.capture ? 'pipe' : 'inherit',
    ...(options.env ? { env: options.env } : {}),
    ...(options.input === undefined ? {} : { input: options.input }),
  });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(`${command} ${args.join(' ')} failed${result.stderr ? `\n${result.stderr}` : ''}`);
  return result.stdout?.trim() ?? '';
}

function sha256(file) {
  return createHash('sha256').update(readFileSync(file)).digest('hex');
}

function remoteTagExists(tagName) {
  const result = spawnSync('git', ['ls-remote', '--tags', 'origin', `refs/tags/${tagName}`], { encoding: 'utf8' });
  return result.status === 0 && result.stdout.trim().length > 0;
}

/*
 * 9.6.0's Linux stage got an HTTP 500 from the draft flip that had in fact
 * succeeded, and the stage failed after the release was public. Verify the
 * live state instead of trusting one API reply.
 */
function publishDraftRelease(tagName) {
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    const edit = spawnSync('gh', ['release', 'edit', tagName, '--repo', 'maddada/Ghostex', '--draft=false'], {
      encoding: 'utf8',
    });
    const live = spawnSync('gh', ['api', `repos/maddada/Ghostex/releases/tags/${tagName}`], { encoding: 'utf8' });
    if (live.status === 0 && JSON.parse(live.stdout).draft === false) return;
    console.log(
      `Draft flip attempt ${attempt} of 5 did not settle${edit.status !== 0 ? `: ${(edit.stderr || '').trim()}` : ''}; retrying.`
    );
    spawnSync('sleep', ['5']);
  }
  throw new Error(`Could not publish ${tagName}: the release is still a draft after 5 attempts`);
}

function loseCreationRace(reason) {
  if (!publishStage) throw new Error(reason);
  console.log(`${reason}; another stage created the release first.`);
  process.exit(LOST_CREATION_RACE_EXIT_CODE);
}

const artifactContracts = new Map([
  ['macos-arm64', [`ghostex-${version}-arm64.dmg`]],
  ['linux-deb-x64', [`ghostex_${version}_amd64.deb`]],
  ['linux-rpm-x64', [`ghostex-${version}-1.x86_64.rpm`]],
  ['linux-tar-x64', [`ghostex-${version}-linux-x64.tar.zst`]],
  ['linux-appimage-x64', [`ghostex-${version}-linux-x64.AppImage`, `ghostex-${version}-linux-x64.AppImage.zsync`]],
  ['linux-deb-arm64', [`ghostex_${version}_arm64.deb`]],
  ['linux-rpm-arm64', [`ghostex-${version}-1.aarch64.rpm`]],
  ['linux-tar-arm64', [`ghostex-${version}-linux-arm64.tar.zst`]],
  [
    'linux-appimage-arm64',
    [`ghostex-${version}-linux-arm64.AppImage`, `ghostex-${version}-linux-arm64.AppImage.zsync`],
  ],
  ['windows-x64', null],
  ['windows-arm64', null],
  ['android', ['ghostex-android.apk']],
  ['gxserver-linux-x64', ['gxserver-linux-x64.tar.gz']],
  ['gxserver-linux-arm64', ['gxserver-linux-arm64.tar.gz']],
  ['gxserver-wsl-windows-x64', ['gxserver-wsl-windows-x64.zip']],
  ['gxserver-wsl-windows-arm64', ['gxserver-wsl-windows-arm64.zip']],
]);

function validateArtifactContract(platform, names, contract) {
  if (!platform.startsWith('windows-')) {
    if (JSON.stringify(names) !== JSON.stringify([...contract].sort())) {
      throw new Error(`${platform} artifacts ${JSON.stringify(names)} do not match ${JSON.stringify(contract)}`);
    }
    return;
  }
  const arch = platform.slice('windows-'.length);
  const channel = `win-${arch}-stable`;
  const required = new Set([
    `ghostex-${version}-windows-${arch}.exe`,
    `releases.${channel}.json`,
    `Ghostex-${version}-${channel}-full.nupkg`,
  ]);
  const optional = new Set([
    `assets.${channel}.json`,
    `RELEASES-${channel}`,
    `Ghostex-${version}-${channel}-delta.nupkg`,
  ]);
  for (const name of required) {
    if (!names.includes(name)) throw new Error(`${platform} is missing Velopack artifact ${name}`);
  }
  for (const name of names) {
    if (!required.has(name) && !optional.has(name)) {
      throw new Error(`${platform} has unexpected Velopack artifact ${name}`);
    }
  }
}

const sourceCommit = run('git', ['rev-parse', 'HEAD'], { capture: true });
/*
 * The artifacts were produced at the plan's source commit; this job tags the
 * commit it checked out. For a normal run those are the same commit, and for a
 * publish-only recovery the plan's commit must be an ancestor — otherwise the
 * release would carry artifacts built from a commit this tag does not contain.
 */
if (spawnSync('git', ['merge-base', '--is-ancestor', plan.sourceSha, sourceCommit]).status !== 0) {
  throw new Error(
    `Refusing to publish: the plan's source ${plan.sourceSha} is not an ancestor of the publishing commit ${sourceCommit}`
  );
}
/*
 * Scope-aware feeds, keyed on "macOS ships in this release" rather than on
 * "macOS was rebuilt". macOS is version-stamped and therefore only reusable
 * inside its own version — precisely the recovery case where the DMG exists but
 * the appcast entry never got published, so Sparkle must still advance.
 */
const macosFeedScope = resolveMacosFeedScope({
  plan,
  updateSparkleRequested: process.env.GHOSTEX_RELEASE_UPDATE_SPARKLE !== '0',
});
/* In a staged release only the stage that ships the DMG may advance the feed. */
const updateSparkle = macosFeedScope.sparkle && publishing.has('macos-arm64');
const windowsFeedScope = resolveWindowsFeedScope({ plan });
console.log(
  `Plan: ${plan.expectedPlatforms.join(', ')} (macOS ${macosFeedScope.macosAction}; sparkle=${
    updateSparkle ? 'advance' : 'hold'
  }; windows feeds regenerated=${
    windowsFeedScope.regenerated.join(',') || 'none'
  }, carried forward=${windowsFeedScope.carriedForward.join(',') || 'none'})`
);
run('git', ['config', 'user.name', 'github-actions[bot]']);
run('git', ['config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com']);

const manifests = [];
for (const artifactDirectory of readdirSync(artifactsRoot, { withFileTypes: true })) {
  if (!artifactDirectory.isDirectory()) continue;
  const directory = path.join(artifactsRoot, artifactDirectory.name);
  const manifestPath = path.join(directory, 'manifest.json');
  if (!existsSync(manifestPath)) {
    /*
     * The run also uploads the plan, the per-product provenance records, and the
     * immutable code-server component archives. None of them is a release
     * product, so they carry no manifest; anything else without one is worth
     * saying out loud without failing an otherwise complete release.
     */
    if (!isNonProductArtifactDirectory(artifactDirectory.name)) {
      console.log(`::warning::Ignoring artifact directory without a manifest: ${artifactDirectory.name}`);
    }
    continue;
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/, ''));
  if (manifest.schemaVersion !== 1 || manifest.version !== version || !expected.has(manifest.platform)) {
    throw new Error(`Unexpected manifest ${manifestPath}: ${JSON.stringify(manifest)}`);
  }
  // A sibling stage's build may already have finished; its artifact is that stage's to publish.
  if (!publishing.has(manifest.platform)) continue;
  const contract = artifactContracts.get(manifest.platform);
  if (contract === undefined) throw new Error(`No release artifact contract is defined for ${manifest.platform}`);
  if (
    manifest.platform === 'android' &&
    (manifest.source_kind !== 'react-native-mobile' || manifest.application_id !== 'io.ghostex')
  ) {
    throw new Error(
      `Android manifest must identify the React Native mobile app (got ${manifest.source_kind ?? 'unknown'} / ${manifest.application_id ?? 'unknown'})`
    );
  }
  const names = (manifest.artifacts ?? []).map((artifact) => artifact.name).sort();
  validateArtifactContract(manifest.platform, names, contract);
  for (const artifact of manifest.artifacts ?? []) {
    if (path.basename(artifact.name) !== artifact.name) throw new Error(`Unsafe artifact name: ${artifact.name}`);
    const file = path.join(directory, artifact.name);
    if (!existsSync(file) || !statSync(file).isFile()) throw new Error(`Manifest artifact is missing: ${file}`);
    const actual = sha256(file);
    if (actual !== artifact.sha256)
      throw new Error(`SHA256 mismatch for ${artifact.name}: ${actual} != ${artifact.sha256}`);
    if (statSync(file).size !== artifact.size) throw new Error(`Size mismatch for ${artifact.name}`);
    artifact.path = file;
  }
  manifests.push({ directory, ...manifest });
}
const received = new Set(manifests.map((manifest) => manifest.platform));
for (const platform of publishing) {
  if (!received.has(platform)) throw new Error(`Enabled platform produced no validated manifest: ${platform}`);
}
if (received.size !== publishing.size || manifests.length !== publishing.size) {
  throw new Error('Received duplicate or unexpected platform manifests');
}

/*
 * Plan <-> manifest <-> provenance. Every expected platform must carry exactly
 * one provenance record whose product, action, fingerprint, algorithm revision,
 * release version, source commit, and artifact digests agree with both the plan
 * and the manifest that arrived beside it. A reused product additionally has to
 * name the origin the plan authorized and carry all four verified checks.
 */
const productProvenance = collectPublishProvenance({
  manifests,
  plan,
  products: [...publishing],
  readProvenance: (directory) => {
    const file = path.join(directory, PRODUCT_PROVENANCE_FILE);
    if (!existsSync(file)) return null;
    return JSON.parse(readFileSync(file, 'utf8').replace(/^\uFEFF/u, ''));
  },
  version,
});
assertSingleBuildOrigin({
  expectedRunId: process.env.GHOSTEX_RELEASE_SOURCE_RUN_ID || null,
  records: productProvenance,
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
    if (entry.startsWith('/') || entry.split('/').includes('..'))
      throw new Error(`Unsafe ZIP entry in ${zipPath}: ${entry}`);
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
    if (actual !== expectedSha)
      throw new Error(
        `${path.basename(zipPath)} embeds ${expectedEntry} with SHA256 ${actual}; expected ${expectedSha}`
      );
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
}

function readZipEntryText(zipPath, expectedEntry) {
  const entries = run('unzip', ['-Z1', zipPath], { capture: true }).split(/\r?\n/u).filter(Boolean);
  if (!entries.includes(expectedEntry)) throw new Error(`${path.basename(zipPath)} is missing ${expectedEntry}`);
  const result = spawnSync('unzip', ['-p', zipPath, expectedEntry], { encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(`Could not read ${expectedEntry} from ${path.basename(zipPath)}: ${result.stderr}`);
  return result.stdout;
}

for (const arch of ['x64', 'arm64']) {
  const linuxPlatform = `gxserver-linux-${arch}`;
  const linuxName = `gxserver-linux-${arch}.tar.gz`;
  if (!byPlatform.has(linuxPlatform)) continue;
  const linuxArchive = artifactPath(linuxPlatform, linuxName);
  const linuxSha = sha256(linuxArchive);
  const wslPlatform = `gxserver-wsl-windows-${arch}`;
  if (byPlatform.has(wslPlatform)) {
    const wslZip = artifactPath(wslPlatform, `gxserver-wsl-windows-${arch}.zip`);
    validateZipEntrySha(wslZip, `gxserver-wsl-windows-${arch}/${linuxName}`, linuxSha);
    const metadataEntry = `gxserver-wsl-windows-${arch}/wsl-package.json`;
    const temporary = mkdtempSync(path.join(os.tmpdir(), 'ghostex-release-wsl-metadata-'));
    try {
      run('unzip', ['-q', wslZip, metadataEntry, '-d', temporary]);
      const metadata = JSON.parse(readFileSync(path.join(temporary, ...metadataEntry.split('/')), 'utf8'));
      if (
        metadata.schemaVersion !== 1 ||
        metadata.version !== version ||
        metadata.target !== 'wsl2' ||
        metadata.targetArch !== arch ||
        metadata.payload?.name !== linuxName ||
        metadata.payload?.sha256 !== linuxSha
      ) {
        throw new Error(`Invalid WSL package metadata for ${arch}: ${JSON.stringify(metadata)}`);
      }
    } finally {
      rmSync(temporary, { force: true, recursive: true });
    }
  }
  const windowsPlatform = `windows-${arch}`;
  if (byPlatform.has(windowsPlatform)) {
    // The full update package holds the app under lib/app (Velopack's PackageBuilder).
    const fullPackage = artifactPath(windowsPlatform, `Ghostex-${version}-win-${arch}-stable-full.nupkg`);
    const velopackPayloadRoot = 'lib/app';
    const entries = zipEntries(fullPackage);
    const forbidden = entries.find(
      (entry) =>
        entry === 'libcef.dll' ||
        entry.endsWith('/libcef.dll') ||
        entry.startsWith(`${velopackPayloadRoot}/resources/wsl/gxserver-`) ||
        entry.startsWith(`${velopackPayloadRoot}/code-server/`) ||
        (entry.startsWith(`${velopackPayloadRoot}/resources/wsl/code-server-`) &&
          (entry.endsWith(`-linux-${arch}.tar.gz`) || entry.endsWith(`-linux-${arch}.tar.gz.sha256`)))
    );
    if (forbidden) throw new Error(`${path.basename(fullPackage)} still embeds release-excluded payload ${forbidden}`);
    const componentManifestEntry = `${velopackPayloadRoot}/resources/on-demand-resources.json`;
    const componentManifest = validateOnDemandManifestV2(
      JSON.parse(readZipEntryText(fullPackage, componentManifestEntry))
    );
    for (const componentName of ['cef', 'code-server']) {
      const component = componentManifest.components[componentName];
      const asset = component?.platforms?.[`windows-${arch}`];
      if (!asset) {
        throw new Error(`${path.basename(fullPackage)} manifest v2 is missing ${componentName} for windows-${arch}`);
      }
    }
    // The native editor is downloaded on first use (build-windows-app.ps1).
    if (!componentManifest.components['code-server'].platforms[`windows-native-${arch}`]) {
      throw new Error(`${path.basename(fullPackage)} manifest v2 is missing code-server for windows-native-${arch}`);
    }
    // The WSL gxserver is downloaded on demand: the sealed entry must name this release's Linux archive.
    if (componentManifest.assets[linuxPlatform]?.sha256 !== linuxSha) {
      throw new Error(`${path.basename(fullPackage)} manifest v2 ${linuxPlatform} sha256 does not match ${linuxName}`);
    }
  }
}

const tag = `v${version}`;

/*
 * The durable reuse index. Every release carries one small asset recording, per
 * product, the fingerprint, the digests, the origin, and the product version
 * behind its bytes. That asset is what makes the *next* release able to reuse
 * anything at all, and what lets the final verifier re-derive every reuse claim
 * from public data alone. Actions artifact retention is irrelevant to it.
 */
const releaseProvenance = buildReleaseProvenanceRecord({
  plan,
  productRecords: productProvenance,
  publishedAt: new Date().toISOString(),
  sourceSha: sourceCommit,
  version,
  workflowRunId: Number(process.env.GITHUB_RUN_ID ?? 0),
});
const provenanceAssetName = releaseProvenanceAssetName(version);
const provenanceAssetPath = path.join(artifactsRoot, provenanceAssetName);
writeFileSync(provenanceAssetPath, `${JSON.stringify(releaseProvenance, null, 2)}\n`);

const changelog = readFileSync('CHANGELOG.md', 'utf8');
const sectionStart = changelog.indexOf(`## ${version} -`);
if (sectionStart < 0) throw new Error(`CHANGELOG.md has no ${version} section`);
const nextSection = changelog.indexOf('\n## ', sectionStart + 4);
const releaseNotes = [changelog.slice(sectionStart, nextSection < 0 ? undefined : nextSection).trim(), ''];
if (process.env.GHOSTEX_RELEASE_PRERELEASE === '1') {
  releaseNotes.push('> Nightly prerelease. Existing macOS installations will not be notified through Sparkle.', '');
}
if (process.env.GHOSTEX_RELEASE_WINDOWS_SIGNED === '0') {
  releaseNotes.push('> Windows beta packages are not Authenticode-signed and may show a SmartScreen warning.', '');
}
const uploadPaths = [];
for (const manifest of manifests.sort((a, b) => a.platform.localeCompare(b.platform))) {
  for (const artifact of manifest.artifacts) {
    uploadPaths.push(artifact.path);
  }
}
const provenanceAssetSha = sha256(provenanceAssetPath);
uploadPaths.push(provenanceAssetPath);
const customerDownloads = renderCustomerDownloadNotes(
  version,
  manifests.flatMap((manifest) => manifest.artifacts.map((artifact) => artifact.name))
);
if (customerDownloads) releaseNotes.unshift(customerDownloads, '');
const notesPath = path.join(artifactsRoot, `release-notes-${version}.md`);
writeFileSync(notesPath, `${releaseNotes.join('\n').trim()}\n`);
const expectedAssets = new Map(
  /** @type {Array<[string, string]>} */ ([
    ...manifests.flatMap((manifest) => manifest.artifacts).map((artifact) => [artifact.name, artifact.sha256]),
    [provenanceAssetName, provenanceAssetSha],
  ])
);
if (expectedAssets.size !== uploadPaths.length) throw new Error('Release artifact names are not globally unique');

/*
 * `verifyProvenanceDigest` is false only when re-validating a release this run
 * did not upload. The provenance record carries a `publishedAt` timestamp, so a
 * second run produces different bytes for the same facts; the already-published
 * path therefore compares the live record's *content* (below) instead of its
 * digest. Everything else is still matched byte for byte.
 */
function validateLiveRelease(liveRelease, { verifyProvenanceDigest = true } = {}) {
  if (liveRelease.draft) throw new Error(`Live release ${tag} is still a draft`);
  const expectedPrerelease = process.env.GHOSTEX_RELEASE_PRERELEASE === '1';
  if (Boolean(liveRelease.prerelease) !== expectedPrerelease) {
    throw new Error(`Live release prerelease=${liveRelease.prerelease}; expected ${expectedPrerelease}`);
  }
  // A staged release grows as later stages amend it, so a stage only checks its own assets.
  if (!publishStage && liveRelease.assets?.length !== expectedAssets.size) {
    throw new Error(`Live release has ${liveRelease.assets?.length ?? 0} assets; expected ${expectedAssets.size}`);
  }
  const liveByName = new Map((liveRelease.assets ?? []).map((asset) => [asset.name, asset]));
  for (const [name, expectedSha] of expectedAssets) {
    if (name === provenanceAssetName && !verifyProvenanceDigest) continue;
    const asset = liveByName.get(name);
    const liveSha =
      typeof asset?.digest === 'string' && asset.digest.startsWith('sha256:')
        ? asset.digest.slice('sha256:'.length)
        : null;
    if (liveSha !== expectedSha) {
      throw new Error(`Live asset digest mismatch for ${name}: ${liveSha ?? 'missing'} != ${expectedSha}`);
    }
  }
  if (!publishStage) {
    for (const asset of liveRelease.assets ?? []) {
      if (!expectedAssets.has(asset.name)) throw new Error(`Live release carries an unexpected asset: ${asset.name}`);
    }
  }
}

const buildNumber = releaseBuildNumber(version);
const macos = manifests.find((manifest) => manifest.platform === 'macos-arm64');
const generatedAppcast = macos && updateSparkle ? path.join(macos.directory, 'appcast.xml') : null;
let generatedAppcastXml = '';
if (macos && updateSparkle) {
  if (!existsSync(generatedAppcast)) throw new Error('macOS payload is missing appcast.xml');
  generatedAppcastXml = readFileSync(generatedAppcast, 'utf8');
  if (!appcastReferencesRelease(generatedAppcastXml, buildNumber, version)) {
    throw new Error('Generated appcast does not point at the new primary GPUI DMG/build');
  }
}

const existingReleaseResult = spawnSync('gh', ['api', `repos/maddada/Ghostex/releases/tags/${tag}`], {
  encoding: 'utf8',
});
if (existingReleaseResult.status === 0 && publishStage) {
  loseCreationRace(`${tag} is already published`);
}
if (existingReleaseResult.status === 0) {
  if (!run('git', ['tag', '-l', tag], { capture: true })) {
    throw new Error(`GitHub release ${tag} exists without a fetched local tag`);
  }
  const tagCommit = run('git', ['rev-list', '-n', '1', tag], { capture: true });
  const ancestor = spawnSync('git', ['merge-base', '--is-ancestor', tagCommit, sourceCommit]);
  if (ancestor.status !== 0) {
    throw new Error(`Existing ${tag} commit ${tagCommit} is not an ancestor of source ${sourceCommit}`);
  }
  const existingRelease = JSON.parse(existingReleaseResult.stdout);
  validateLiveRelease(existingRelease, { verifyProvenanceDigest: false });
  const publishedProvenanceAsset = (existingRelease.assets ?? []).find((asset) => asset.name === provenanceAssetName);
  if (!publishedProvenanceAsset) throw new Error(`Existing ${tag} carries no ${provenanceAssetName}`);
  assertLiveProvenanceMatches({
    live: JSON.parse(
      run(
        'gh',
        [
          'api',
          `repos/maddada/Ghostex/releases/assets/${publishedProvenanceAsset.id}`,
          '-H',
          'Accept: application/octet-stream',
        ],
        { capture: true }
      )
    ),
    record: releaseProvenance,
  });
  if (macos && updateSparkle && !appcastReferencesRelease(readLiveAppcast(), buildNumber, version)) {
    const taggedAppcast = run('git', ['show', `${tag}:appcast.xml`], { capture: true });
    if (taggedAppcast.trim() !== generatedAppcastXml.trim()) {
      throw new Error(`Existing ${tag} contains an appcast that differs from the validated macOS artifact`);
    }
    advanceMainWithAppcast({
      appcastCommit: tagCommit,
      appcastXml: generatedAppcastXml,
      builtCommit: run('git', ['rev-parse', `${tagCommit}^`], { capture: true }),
      message: `chore: release ${version}`,
      version,
    });
  }
  if (macos && updateSparkle && !appcastReferencesRelease(readLiveAppcast(), buildNumber, version)) {
    throw new Error(`Live appcast did not advance to ${version} (${buildNumber})`);
  }
  console.log(`Already published and live-verified ${tag} with ${uploadPaths.length} assets.`);
  console.log(renderReleaseProvenanceReport(releaseProvenance, { plan }));
  process.exit(0);
}
if (run('git', ['tag', '-l', tag], { capture: true }))
  throw new Error(`Tag already exists without a public release: ${tag}`);
if (publishStage && remoteTagExists(tag)) loseCreationRace(`${tag} already exists on origin`);

if (macos && updateSparkle) {
  writeFileSync('appcast.xml', generatedAppcastXml);
  run('git', ['add', 'appcast.xml']);
  run('git', ['commit', '-m', `chore: release ${version}`]);
}

// Fail before creating the tag if main diverged, rather than after uploading
// every asset. Pure fast-forward drift is absorbed when Sparkle advances below.
classifyMainDrift(sourceCommit);
run('git', ['tag', '-a', tag, '-m', `Release ${tag}`]);
/*
 * The tag push is the creation race's arbiter: origin rejects a second push of
 * the same tag, so exactly one stage gets past this line as the creator.
 */
const tagPush = spawnSync('git', ['push', 'origin', tag], { encoding: 'utf8' });
if (tagPush.status !== 0) {
  process.stdout.write(tagPush.stdout ?? '');
  process.stderr.write(tagPush.stderr ?? '');
  if (publishStage && remoteTagExists(tag)) {
    run('git', ['tag', '-d', tag]);
    loseCreationRace(`${tag} appeared on origin while this stage was tagging`);
  }
  throw new Error(`git push origin ${tag} failed`);
}
const releaseArgs = [
  'release',
  'create',
  tag,
  '--repo',
  'maddada/Ghostex',
  '--title',
  `Ghostex ${version}${process.env.GHOSTEX_RELEASE_PRERELEASE === '1' ? ' Nightly' : ''}`,
  '--notes-file',
  notesPath,
  '--draft',
  ...uploadPaths,
];
if (process.env.GHOSTEX_RELEASE_PRERELEASE === '1') releaseArgs.push('--prerelease');
run('gh', releaseArgs);
publishDraftRelease(tag);

// Keep the Sparkle feed as the final public mutation. Existing users cannot
// observe an appcast entry until the matching signed DMG is already live.
if (macos && updateSparkle) {
  advanceMainWithAppcast({
    appcastCommit: run('git', ['rev-parse', 'HEAD'], { capture: true }),
    appcastXml: generatedAppcastXml,
    builtCommit: sourceCommit,
    message: `chore: release ${version}`,
    version,
  });
}

const liveRelease = JSON.parse(run('gh', ['api', `repos/maddada/Ghostex/releases/tags/${tag}`], { capture: true }));
validateLiveRelease(liveRelease);

if (macos && updateSparkle) waitForLiveAppcast({ buildNumber, version });

console.log(`Published and live-verified ${tag} with ${uploadPaths.length} assets.`);
console.log(renderReleaseProvenanceReport(releaseProvenance, { plan }));
if (process.env.GITHUB_STEP_SUMMARY) {
  appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    `${renderBuildProvenanceNotes(releaseProvenance)}\n\n\`\`\`\n${renderReleaseProvenanceReport(releaseProvenance, {
      plan,
    })}\n\`\`\`\n\n`
  );
}
