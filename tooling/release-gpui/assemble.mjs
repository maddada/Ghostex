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

function resolveRemoteMain() {
  const remoteMain = run('git', ['ls-remote', 'origin', 'refs/heads/main'], { capture: true }).split(/\s+/)[0];
  if (!remoteMain) throw new Error('Could not resolve origin/main');
  return remoteMain;
}

/*
 * CDXC:Release 2026-08-30:
 * `origin/main` moves while a multi-hour build runs, because several agents
 * share this checkout. 8.3.0 lost an entire dispatch to that: all eleven
 * products had built and publication was refused because an unrelated mobile
 * submodule bump landed while the runners worked.
 *
 * Drift is only safe to absorb when it is a pure fast-forward -- the commit we
 * built is still in main's history, so nothing that went into these artifacts
 * was rewritten or rolled back. Divergence stays fatal, because then the
 * artifacts describe source that main no longer has.
 */
function classifyMainDrift(builtCommit) {
  const remoteMain = resolveRemoteMain();
  if (remoteMain === builtCommit) return { kind: 'unchanged', remoteMain };
  run('git', ['fetch', '--no-tags', 'origin', 'main']);
  if (spawnSync('git', ['merge-base', '--is-ancestor', builtCommit, remoteMain]).status !== 0) {
    throw new Error(
      `origin/main diverged from the built source during the build (${builtCommit} is not an ancestor of ${remoteMain}); refusing partial publication`
    );
  }
  return { kind: 'advanced', remoteMain };
}

/*
 * Build `origin/main`-plus-this-appcast without touching the runner's working
 * tree or index. Plumbing rather than cherry-pick: the final content of
 * appcast.xml is already known exactly, so there is nothing to merge and no
 * conflict to resolve, and every other path keeps whatever landed during the
 * build byte-for-byte.
 */
function commitAppcastOnto({ appcastXml, message, parent }) {
  const blob = run('git', ['hash-object', '-w', '--stdin'], { capture: true, input: appcastXml });
  const indexDirectory = mkdtempSync(path.join(os.tmpdir(), 'ghostex-release-index-'));
  const env = { ...process.env, GIT_INDEX_FILE: path.join(indexDirectory, 'index') };
  try {
    run('git', ['read-tree', parent], { capture: true, env });
    run('git', ['update-index', '--add', '--cacheinfo', `100644,${blob},appcast.xml`], { capture: true, env });
    const tree = run('git', ['write-tree'], { capture: true, env });
    return run('git', ['commit-tree', tree, '-p', parent, '-m', message], { capture: true });
  } finally {
    rmSync(indexDirectory, { force: true, recursive: true });
  }
}

/*
 * Advance the Sparkle feed on `main`. The tag is never moved to accommodate
 * drift: it keeps pointing at the exact commit that was built and signed. When
 * main has moved on, the appcast bump is replayed on top of it instead, so the
 * advance can never revert work that landed during the build.
 *
 * `main` can move again between resolving it and pushing, so a rejected push is
 * retried against the newer tip rather than treated as a failure.
 */
function advanceMainWithAppcast({ appcastCommit, appcastXml, builtCommit, message }) {
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    const drift = classifyMainDrift(builtCommit);
    if (drift.kind === 'unchanged') {
      run('git', ['push', 'origin', `${appcastCommit}:main`]);
      return 'fast-forward';
    }
    const replayed = commitAppcastOnto({ appcastXml, message, parent: drift.remoteMain });
    const push = spawnSync('git', ['push', 'origin', `${replayed}:main`], { encoding: 'utf8', stdio: 'pipe' });
    if (push.status === 0) {
      console.log(
        `origin/main advanced during the build; replayed the ${version} appcast bump onto ${drift.remoteMain.slice(0, 10)} as ${replayed.slice(0, 10)}.`
      );
      return 'replayed';
    }
    console.log(`origin/main moved again while advancing Sparkle; retrying (attempt ${attempt} of 3).`);
  }
  throw new Error(`Could not advance origin/main with the ${version} appcast bump after 3 attempts`);
}

const artifactContracts = new Map([
  ['macos-arm64', [`ghostex-${version}-arm64.dmg`]],
  ['linux-deb-x64', [`ghostex_${version}_amd64.deb`]],
  ['linux-rpm-x64', [`ghostex-${version}-1.x86_64.rpm`]],
  ['linux-tar-x64', [`ghostex-${version}-linux-x64.tar.zst`]],
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
    `ghostex-${version}-windows-${arch}-portable.zip`,
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
const updateSparkle = macosFeedScope.sparkle;
const windowsFeedScope = resolveWindowsFeedScope({ plan });
console.log(
  `Plan: ${plan.expectedPlatforms.join(', ')} (macOS ${macosFeedScope.macosAction}; sparkle=${
    updateSparkle ? 'advance' : 'hold'
  }; homebrew=${macosFeedScope.homebrew ? 'eligible' : 'hold'}; windows feeds regenerated=${
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
for (const platform of expected) {
  if (!received.has(platform)) throw new Error(`Enabled platform produced no validated manifest: ${platform}`);
}
if (received.size !== expected.size || manifests.length !== expected.size) {
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
    const portable = artifactPath(windowsPlatform, `ghostex-${version}-windows-${arch}-portable.zip`);
    const velopackPayloadRoot = 'current';
    validateZipEntrySha(portable, `${velopackPayloadRoot}/resources/wsl/${linuxName}`, linuxSha);
    const sidecarEntry = `${velopackPayloadRoot}/resources/wsl/${linuxName}.sha256`;
    const sidecar = readZipEntryText(portable, sidecarEntry);
    if (sidecar !== `${linuxSha}\n`) {
      throw new Error(`${path.basename(portable)} has an invalid ${sidecarEntry}`);
    }
    const entries = zipEntries(portable);
    const forbidden = entries.find(
      (entry) =>
        entry === 'libcef.dll' ||
        entry.endsWith('/libcef.dll') ||
        (entry.startsWith(`${velopackPayloadRoot}/resources/wsl/code-server-`) &&
          (entry.endsWith(`-linux-${arch}.tar.gz`) || entry.endsWith(`-linux-${arch}.tar.gz.sha256`)))
    );
    if (forbidden) throw new Error(`${path.basename(portable)} still embeds release-excluded payload ${forbidden}`);
    const componentManifestEntry = `${velopackPayloadRoot}/resources/on-demand-resources.json`;
    const componentManifest = validateOnDemandManifestV2(
      JSON.parse(readZipEntryText(portable, componentManifestEntry))
    );
    for (const componentName of ['cef', 'code-server']) {
      const component = componentManifest.components[componentName];
      const asset = component?.platforms?.[`windows-${arch}`];
      if (!asset) {
        throw new Error(`${path.basename(portable)} manifest v2 is missing ${componentName} for windows-${arch}`);
      }
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
if (customerDownloads) releaseNotes.push(customerDownloads, '');
const notesPath = path.join(artifactsRoot, `release-notes-${version}.md`);
writeFileSync(notesPath, `${releaseNotes.join('\n').trim()}\n`);
const expectedAssets = new Map([
  ...manifests.flatMap((manifest) => manifest.artifacts).map((artifact) => [artifact.name, artifact.sha256]),
  [provenanceAssetName, provenanceAssetSha],
]);
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
  if (liveRelease.assets?.length !== expectedAssets.size) {
    throw new Error(`Live release has ${liveRelease.assets?.length ?? 0} assets; expected ${expectedAssets.size}`);
  }
  for (const asset of liveRelease.assets) {
    if (asset.name === provenanceAssetName && !verifyProvenanceDigest) continue;
    const expectedSha = expectedAssets.get(asset.name);
    const liveSha =
      typeof asset.digest === 'string' && asset.digest.startsWith('sha256:')
        ? asset.digest.slice('sha256:'.length)
        : null;
    if (!expectedSha || liveSha !== expectedSha) {
      throw new Error(
        `Live asset digest mismatch for ${asset.name}: ${liveSha ?? 'missing'} != ${expectedSha ?? 'unexpected asset'}`
      );
    }
  }
}

const [major, minor, patch] = version.split('.').map(Number);
const buildNumber = major * 10000 + minor * 100 + patch;
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

if (macos && updateSparkle) {
  writeFileSync('appcast.xml', generatedAppcastXml);
  run('git', ['add', 'appcast.xml']);
  run('git', ['commit', '-m', `chore: release ${version}`]);
}

// Fail before creating the tag if main diverged, rather than after uploading
// every asset. Pure fast-forward drift is absorbed when Sparkle advances below.
classifyMainDrift(sourceCommit);
run('git', ['tag', '-a', tag, '-m', `Release ${tag}`]);
run('git', ['push', 'origin', tag]);
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
run('gh', ['release', 'edit', tag, '--repo', 'maddada/Ghostex', '--draft=false']);

// Keep the Sparkle feed as the final public mutation. Existing users cannot
// observe an appcast entry until the matching signed DMG is already live.
if (macos && updateSparkle) {
  advanceMainWithAppcast({
    appcastCommit: run('git', ['rev-parse', 'HEAD'], { capture: true }),
    appcastXml: generatedAppcastXml,
    builtCommit: sourceCommit,
    message: `chore: release ${version}`,
  });
}

const liveRelease = JSON.parse(run('gh', ['api', `repos/maddada/Ghostex/releases/tags/${tag}`], { capture: true }));
validateLiveRelease(liveRelease);

if (macos && updateSparkle) {
  let liveAppcast = '';
  for (let attempt = 0; attempt < 12; attempt += 1) {
    const xml = readLiveAppcast();
    if (appcastReferencesRelease(xml, buildNumber, version)) {
      liveAppcast = xml;
      break;
    }
    spawnSync('sleep', ['5']);
  }
  if (!liveAppcast) throw new Error(`Live appcast did not advance to ${version} (${buildNumber})`);
}

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
