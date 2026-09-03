#!/usr/bin/env node
/*
 * CDXC:Release 2026-08-13:
 * Emits the per-product `provenance.json` that travels inside every
 * `release-<platform>` artifact (§3.5) and, in a second tiny artifact, feeds the
 * planner's Tier-2 reuse index.
 *
 * Every producing job runs this: bash jobs get it for free through
 * `release_gpui_write_manifest`, the Windows job and the reuse job call it
 * directly. Keeping one writer is what makes the publisher's plan↔manifest↔
 * provenance cross-check meaningful — the record is derived from the manifest
 * that was actually produced and from the plan the run was dispatched with,
 * never from a job author's hand-written duplicate.
 */

import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildProductProvenance } from './provenance.mjs';
import { validatePlan } from './plan.mjs';
import { shortFingerprint } from './fingerprint.mjs';

export const PROVENANCE_FILE_NAME = 'provenance.json';

export function readReleasePlan(env = process.env) {
  const inline = env.GHOSTEX_RELEASE_PLAN;
  const file = env.GHOSTEX_RELEASE_PLAN_FILE;
  const text = inline && inline.trim() ? inline : file ? readFileSync(file, 'utf8') : '';
  if (!text.trim()) {
    throw new Error('GHOSTEX_RELEASE_PLAN (or GHOSTEX_RELEASE_PLAN_FILE) is required to write provenance');
  }
  return validatePlan(JSON.parse(text));
}

function sha256File(filePath) {
  return createHash('sha256').update(readFileSync(filePath)).digest('hex');
}

/*
 * Re-derive the digests from the bytes on disk instead of trusting the manifest
 * this job just wrote. A corrupted upload should fail in the job that produced
 * it, with the product name attached, not two hours later in the publisher.
 */
export function verifyManifestArtifacts({ directory, manifest }) {
  const artifacts = [];
  for (const artifact of manifest.artifacts ?? []) {
    if (path.basename(artifact.name) !== artifact.name) {
      throw new Error(`Unsafe artifact name in manifest: ${artifact.name}`);
    }
    const file = path.join(directory, artifact.name);
    if (!existsSync(file)) throw new Error(`Manifest artifact is missing: ${file}`);
    const size = statSync(file).size;
    const sha256 = sha256File(file);
    if (sha256 !== artifact.sha256) {
      throw new Error(`SHA256 mismatch for ${artifact.name}: ${sha256} != ${artifact.sha256}`);
    }
    if (size !== artifact.size) throw new Error(`Size mismatch for ${artifact.name}: ${size} != ${artifact.size}`);
    artifacts.push({ name: artifact.name, sha256, size });
  }
  if (artifacts.length === 0) throw new Error(`Manifest in ${directory} declares no artifacts`);
  return artifacts;
}

export function productProvenanceForDirectory({ directory, env = process.env, plan, reusedFrom = null }) {
  const manifestPath = path.join(directory, 'manifest.json');
  if (!existsSync(manifestPath)) throw new Error(`No manifest.json in ${directory}`);
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/u, ''));
  const product = manifest.platform;
  const entry = plan.products?.[product];
  if (!entry) throw new Error(`The release plan has no entry for ${product}`);
  if (entry.action === 'skip') throw new Error(`${product} is skipped by the plan but produced an artifact`);
  if (manifest.version !== plan.version) {
    throw new Error(`${product} manifest is version ${manifest.version}; the plan releases ${plan.version}`);
  }

  const artifacts = verifyManifestArtifacts({ directory, manifest });
  const runId = Number(env.GITHUB_RUN_ID ?? 0);
  const sourceSha = env.GHOSTEX_RELEASE_SOURCE_SHA || env.GITHUB_SHA || plan.sourceSha;

  if (entry.action === 'build') {
    return buildProductProvenance({
      action: 'built',
      algorithmRevision: plan.algorithmRevision,
      artifacts,
      fingerprint: entry.fingerprint,
      inputs: entry.inputs,
      originRunId: runId,
      originSourceSha: sourceSha,
      originTag: `v${plan.version}`,
      product,
      productVersion: plan.version,
      releaseVersion: plan.version,
      sourceSha,
    });
  }

  const reuse = entry.reuse;
  if (!reusedFrom) throw new Error(`${product} is reused; provenance requires the verified reuse evidence`);
  return buildProductProvenance({
    action: 'reused',
    algorithmRevision: plan.algorithmRevision,
    artifacts,
    fingerprint: entry.fingerprint,
    inputs: entry.inputs,
    originRunId: Number(reuse.runId),
    originSourceSha: reuse.originSourceSha,
    originTag: reuse.tag ?? `v${plan.version}`,
    product,
    productVersion: reuse.productVersion,
    releaseVersion: plan.version,
    reusedFrom,
    sourceSha,
  });
}

export function writeProductProvenance({ directory, env = process.env, plan, reusedFrom = null }) {
  const record = productProvenanceForDirectory({ directory, env, plan, reusedFrom });
  writeFileSync(path.join(directory, PROVENANCE_FILE_NAME), `${JSON.stringify(record, null, 2)}\n`);
  return record;
}

export function renderProvenanceMarkdown(record, { cacheHits = [], retries = [], timings = [] } = {}) {
  const lines = [];
  lines.push(`### ${record.product} — ${record.action.toUpperCase()}`);
  lines.push('');
  lines.push(`- Fingerprint \`${shortFingerprint(record.fingerprint)}\` (${record.algorithmRevision})`);
  lines.push(`- Release version \`${record.releaseVersion}\`, product version \`${record.productVersion}\``);
  if (record.reusedFrom) {
    const origin = record.reusedFrom.tier === 'release' ? record.reusedFrom.tag : `run ${record.reusedFrom.runId}`;
    lines.push(`- Reused from ${origin}; verified ${record.reusedFrom.verifiedChecks.join(', ')}`);
  } else {
    lines.push(`- Built by run \`${record.originRunId}\` from \`${record.originSourceSha.slice(0, 12)}\``);
  }
  for (const artifact of record.artifacts) {
    lines.push(`- \`${artifact.name}\` — ${artifact.size} bytes, sha256 \`${shortFingerprint(artifact.sha256)}…\``);
  }
  for (const cache of cacheHits) lines.push(`- Cache \`${cache.name}\`: ${cache.hit ? 'hit' : 'miss'}`);
  for (const timing of timings) lines.push(`- ${timing.name}: ${timing.seconds}s`);
  for (const retry of retries) lines.push(`- Retry \`${retry.rule}\` x${retry.attempts} (${retry.label})`);
  return lines.join('\n');
}

export function parseWriteProvenanceArgs(argv) {
  const options = { directory: null, ifPlanned: false, reusedFromFile: null };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--if-planned') {
      options.ifPlanned = true;
    } else if (argument === '--dir') {
      options.directory = argv[index + 1];
      index += 1;
    } else if (argument === '--reused-from') {
      options.reusedFromFile = argv[index + 1];
      index += 1;
    } else {
      throw new Error(`Unknown option: ${argument}`);
    }
  }
  if (!options.directory) throw new Error('--dir <artifact-directory> is required');
  return options;
}

function main() {
  const options = parseWriteProvenanceArgs(process.argv.slice(2));
  const plan = readReleasePlan();
  /*
   * `--if-planned` exists for the shared manifest writer, which also produces
   * intermediate manifests (macos-arm64-signed) that are not release products.
   * Direct callers omit it and get a hard error for an unplanned platform.
   */
  if (options.ifPlanned) {
    const manifestPath = path.join(options.directory, 'manifest.json');
    if (!existsSync(manifestPath)) return;
    const platform = JSON.parse(readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/u, '')).platform;
    if (!plan.products?.[platform] || plan.products[platform].action === 'skip') {
      process.stdout.write(`PROVENANCE skipped: ${platform} is not a planned release product\n`);
      return;
    }
  }
  const reusedFrom = options.reusedFromFile ? JSON.parse(readFileSync(options.reusedFromFile, 'utf8')) : null;
  const record = writeProductProvenance({ directory: options.directory, plan, reusedFrom });
  process.stdout.write(`PROVENANCE ${record.product} ${record.action} ${record.fingerprint}\n`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${renderProvenanceMarkdown(record)}\n\n`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
