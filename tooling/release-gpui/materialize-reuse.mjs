#!/usr/bin/env node
/*
 * CDXC:Release 2026-08-13:
 * Materializes one plan-marked `reuse` product into exactly the same
 * `release-<platform>` artifact shape a build job produces, so nothing
 * downstream — publisher, cross-payload checks, final verifier — has to know
 * how the bytes were obtained.
 *
 * The planner's decision is an *invitation*, never an authorization: this job
 * re-derives all four checks (§Q3) from the bytes it just downloaded, GitHub's
 * own metadata, commit ancestry, and a cryptographic build attestation. If any
 * of them fails the job fails loudly. It never falls back to "publish it
 * anyway", and it never rewrites the payload — only the release-scoped manifest
 * fields a new version legitimately changes.
 */

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { TRUSTED_REPO, productDefinition } from './product-inputs.mjs';
import { releaseProvenanceAssetName, validateReleaseProvenance, verifyReuseCandidate } from './provenance.mjs';
import { readReleasePlan, writeProductProvenance } from './write-provenance.mjs';

/*
 * Manifest identity fields the publisher enforces for specific products
 * (`assemble.mjs:111-118`). They are properties of the product, not of a
 * particular build, so a reconstructed manifest must carry them verbatim.
 */
export const MANIFEST_IDENTITY_FIELDS = Object.freeze({
  android: { application_id: 'io.ghostex', source_kind: 'react-native-mobile' },
});

/* Mirrors the architecture map in `release_gpui_write_manifest` (common.sh). */
export const MANIFEST_ARCHITECTURES = Object.freeze({
  android: 'universal',
  'gxserver-linux-arm64': 'aarch64',
  'gxserver-linux-x64': 'x86_64',
  'macos-arm64': 'arm64',
});

function sha256File(filePath) {
  return createHash('sha256').update(readFileSync(filePath)).digest('hex');
}

/*
 * The single `gh` seam. Every network read this module performs goes through it,
 * so the fixture tests can drive the whole accept/refuse path — download,
 * re-verification, manifest reconstruction, side-file checks — against a stubbed
 * GitHub instead of only exercising the pure helpers.
 */
export function defaultGh(args, { allowFailure = false } = {}) {
  const result = spawnSync('gh', args, { encoding: 'utf8', maxBuffer: 96 * 1024 * 1024, stdio: 'pipe' });
  if (result.error) throw result.error;
  if (result.status !== 0 && !allowFailure) {
    throw new Error(`gh ${args.join(' ')} failed: ${(result.stderr || result.stdout || '').trim()}`);
  }
  return { ok: result.status === 0, stderr: result.stderr ?? '', stdout: result.stdout ?? '' };
}

export function buildReusedManifest({ artifacts, product, record, runId, version, workflowSha }) {
  return {
    artifacts,
    application_id: MANIFEST_IDENTITY_FIELDS[product]?.application_id,
    platform: product,
    schemaVersion: 1,
    source_kind: MANIFEST_IDENTITY_FIELDS[product]?.source_kind,
    source_sha: record.originSourceSha,
    version,
    workflow_run_id: runId || undefined,
    workflow_sha: workflowSha || undefined,
  };
}

export function buildReusedMetadata({ artifacts, product, record, runId, version, workflowSha }) {
  const primary = artifacts.length === 1 ? artifacts[0] : {};
  return {
    architecture: MANIFEST_ARCHITECTURES[product] ?? 'unknown',
    artifacts,
    application_id: MANIFEST_IDENTITY_FIELDS[product]?.application_id,
    created_at: new Date().toISOString(),
    package: product,
    schemaVersion: 1,
    source_kind: MANIFEST_IDENTITY_FIELDS[product]?.source_kind,
    ...primary,
    source_sha: record.originSourceSha,
    version,
    workflow_run_id: runId || undefined,
    workflow_sha: workflowSha || undefined,
  };
}

/* Tier 1: the durable store. Bytes come from the release where they were first published. */
function downloadFromRelease({ artifacts, directory, gh, repo, tag }) {
  mkdirSync(directory, { recursive: true });
  for (const artifact of artifacts) {
    gh(['release', 'download', tag, '--repo', repo, '--pattern', artifact.name, '--dir', directory, '--clobber']);
    const file = path.join(directory, artifact.name);
    if (!existsSync(file)) throw new Error(`${tag} did not yield ${artifact.name}`);
  }
}

/* Tier 2: same-version recovery. The artifact directory is copied verbatim, side files included. */
function downloadFromRun({ directory, gh, product, repo, runId }) {
  mkdirSync(directory, { recursive: true });
  gh(['run', 'download', String(runId), '--repo', repo, '--name', `release-${product}`, '--dir', directory]);
}

export function originProvenanceFromRelease({ gh = defaultGh, product, repo, tag }) {
  const release = JSON.parse(gh(['api', `repos/${repo}/releases/tags/${tag}`]).stdout);
  const version = tag.replace(/^v/u, '');
  const assetName = releaseProvenanceAssetName(version);
  const asset = (release.assets ?? []).find((entry) => entry.name === assetName);
  if (!asset) throw new Error(`${tag} carries no ${assetName}; it cannot be a reuse source`);
  const payload = gh([
    'api',
    `repos/${repo}/releases/assets/${asset.id}`,
    '-H',
    'Accept: application/octet-stream',
  ]).stdout;
  const provenance = validateReleaseProvenance(JSON.parse(payload));
  const record = provenance.products?.[product];
  if (!record) throw new Error(`${tag} provenance has no record for ${product}`);
  return {
    assets: (release.assets ?? []).map((entry) => ({
      digest: entry.digest ?? null,
      name: entry.name,
      size: entry.size,
    })),
    draft: Boolean(release.draft),
    provenance,
    record,
  };
}

export function originProvenanceFromRun({ gh = defaultGh, product, repo, runId }) {
  const scratch = mkdtempSync(path.join(tmpdir(), 'ghostex-reuse-origin-'));
  gh(['run', 'download', String(runId), '--repo', repo, '--name', `release-provenance-${product}`, '--dir', scratch]);
  const recordPath = path.join(scratch, 'provenance.json');
  if (!existsSync(recordPath)) throw new Error(`run ${runId} carries no provenance record for ${product}`);
  return JSON.parse(readFileSync(recordPath, 'utf8'));
}

function defaultAttestationVerifier({ file, gh = defaultGh, repo }) {
  return gh(['attestation', 'verify', file, '--repo', repo], { allowFailure: true }).ok;
}

export function materializeReuse({
  attestationVerifier = defaultAttestationVerifier,
  directory,
  env = process.env,
  gh = defaultGh,
  isAncestor,
  plan,
  product,
  repo = TRUSTED_REPO,
}) {
  const definition = productDefinition(product);
  const entry = plan.products?.[product];
  if (!entry) throw new Error(`The release plan has no entry for ${product}`);
  if (entry.action !== 'reuse') throw new Error(`${product} is planned as ${entry.action}, not reuse`);
  const descriptor = entry.reuse;

  let candidate;
  if (descriptor.tier === 'release') {
    const origin = originProvenanceFromRelease({ gh, product, repo, tag: descriptor.tag });
    downloadFromRelease({ artifacts: origin.record.artifacts, directory, gh, repo, tag: descriptor.tag });
    /*
     * The ancestry oracle is fed the commit the *bytes* were built from, taken
     * from the origin release's own provenance record, not the commit the origin
     * tag points at. Those are the same commit for every release this pipeline
     * publishes, and re-resolving the tag would need another API round trip whose
     * answer is not independent evidence anyway: the record is already bound to
     * these exact bytes by the digest check and to this repository's build by the
     * attestation, both of which are stronger than the tag→commit mapping.
     */
    candidate = {
      assets: origin.assets,
      commit: origin.record.originSourceSha,
      draft: origin.draft,
      record: origin.record,
      repo,
      runId: origin.record.originRunId,
      tag: descriptor.tag,
      tier: 'release',
    };
  } else {
    const record = originProvenanceFromRun({ gh, product, repo, runId: descriptor.runId });
    /* Re-read the run metadata here: the planner asserted it, this job proves it. */
    const run = JSON.parse(
      gh(['run', 'view', String(descriptor.runId), '--repo', repo, '--json', 'conclusion,event,headSha,workflowName'])
        .stdout
    );
    downloadFromRun({ directory, gh, product, repo, runId: descriptor.runId });
    candidate = {
      assets: [],
      commit: run.headSha ?? null,
      conclusion: run.conclusion ?? null,
      draft: false,
      event: run.event ?? null,
      record,
      repo,
      runId: Number(descriptor.runId),
      tag: null,
      tier: 'run',
      workflowName: run.workflowName ?? null,
    };
  }

  /*
   * Tier 2's independent second digest source is the manifest the producing job
   * wrote next to the bytes; Tier 1's is GitHub's own asset metadata. Neither is
   * the provenance record being checked, which is the point.
   */
  const manifestPath = path.join(directory, 'manifest.json');
  const runManifest =
    descriptor.tier === 'run' && existsSync(manifestPath)
      ? JSON.parse(readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/u, ''))
      : null;

  const verification = verifyReuseCandidate({
    algorithmRevision: plan.algorithmRevision,
    candidate,
    evidence: {
      assetMetadata: (name) => {
        if (descriptor.tier === 'release') {
          return candidate.assets.find((asset) => asset.name === name) ?? null;
        }
        const artifact = (runManifest?.artifacts ?? []).find((item) => item.name === name);
        return artifact ? { digest: artifact.sha256, size: artifact.size } : null;
      },
      attestationVerified: (name) => attestationVerifier({ file: path.join(directory, name), gh, repo }),
      isAncestor,
      localArtifact: (name) => {
        const file = path.join(directory, name);
        if (!existsSync(file)) return null;
        return { sha256: sha256File(file), size: statSync(file).size };
      },
    },
    fingerprint: entry.fingerprint,
    productId: product,
    releaseVersion: plan.version,
    requireAll: true,
  });
  if (!verification.ok) {
    throw new Error(`Refusing to reuse ${product}: ${verification.failures.join('; ')}`);
  }

  const artifacts = candidate.record.artifacts.map((artifact) => ({
    name: artifact.name,
    sha256: artifact.sha256,
    size: artifact.size,
  }));
  const runId = Number(env.GITHUB_RUN_ID ?? 0);
  const workflowSha = env.GHOSTEX_RELEASE_WORKFLOW_SHA || env.GITHUB_SHA || '';

  if (descriptor.tier === 'release') {
    writeFileSync(
      manifestPath,
      `${JSON.stringify(
        buildReusedManifest({
          artifacts,
          product,
          record: candidate.record,
          runId,
          version: plan.version,
          workflowSha,
        }),
        null,
        2
      )}\n`
    );
    writeFileSync(
      path.join(directory, 'metadata.json'),
      `${JSON.stringify(
        buildReusedMetadata({
          artifacts,
          product,
          record: candidate.record,
          runId,
          version: plan.version,
          workflowSha,
        }),
        null,
        2
      )}\n`
    );
  } else {
    if (!runManifest) throw new Error(`run ${descriptor.runId} artifact for ${product} has no manifest.json`);
    if (runManifest.platform !== product || runManifest.version !== plan.version) {
      throw new Error(
        `run ${descriptor.runId} manifest is ${runManifest.platform}@${runManifest.version}; expected ${product}@${plan.version}`
      );
    }
    for (const sideFile of definition.sideFiles ?? []) {
      if (!existsSync(path.join(directory, sideFile))) {
        throw new Error(`${product} reuse is missing its side file ${sideFile}`);
      }
    }
  }

  const attestationSubjectDigests = artifacts.map((artifact) => sha256File(path.join(directory, artifact.name)));
  const record = writeProductProvenance({
    directory,
    env,
    plan,
    reusedFrom: {
      attestationSubjectDigests,
      ...(descriptor.tier === 'release' ? { tag: descriptor.tag } : { runId: Number(descriptor.runId) }),
      tier: descriptor.tier,
      verifiedChecks: verification.verifiedChecks,
    },
  });
  return record;
}

function parseArguments(argv) {
  const options = { directory: null, product: null, repo: TRUSTED_REPO, sourceSha: null };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = argv[index + 1];
    if (argument === '--product') options.product = value;
    else if (argument === '--dir') options.directory = value;
    else if (argument === '--repo') options.repo = value;
    else if (argument === '--source-sha') options.sourceSha = value;
    else throw new Error(`Unknown option: ${argument}`);
    index += 1;
  }
  if (!options.product) throw new Error('--product <id> is required');
  if (!options.directory) throw new Error('--dir <artifact-directory> is required');
  return options;
}

function main() {
  const options = parseArguments(process.argv.slice(2));
  const plan = readReleasePlan();
  const sourceSha = options.sourceSha ?? plan.sourceSha;
  const record = materializeReuse({
    directory: options.directory,
    isAncestor: (commit) => spawnSync('git', ['merge-base', '--is-ancestor', commit, sourceSha]).status === 0,
    plan,
    product: options.product,
    repo: options.repo,
  });
  process.stdout.write(
    `REUSED ${record.product} from ${record.reusedFrom.tag ?? `run ${record.reusedFrom.runId}`} ` +
      `(${record.reusedFrom.verifiedChecks.join(', ')})\n`
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
