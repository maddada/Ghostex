/*
 * CDXC:Release 2026-08-13:
 * The per-product provenance writer is the contract between every build job and
 * the publisher, so these tests pin the shape it produces and, more importantly,
 * every case where it must refuse: a manifest whose bytes moved, a product the
 * plan skipped, a version that disagrees with the plan, and a reuse without
 * verified evidence.
 */

import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, test } from 'vitest';

import {
  parseWriteProvenanceArgs,
  productProvenanceForDirectory,
  readReleasePlan,
  renderProvenanceMarkdown,
  verifyManifestArtifacts,
} from './write-provenance.mjs';
import { REUSE_CHECKS } from './provenance.mjs';
import { computePlan } from './plan.mjs';
import { createFixtureRepo, releaseBaselineFromPlan } from './plan-test-fixtures.mjs';
import { defaultScope } from './product-inputs.mjs';

const scratchRoots = [];
afterEach(() => {
  for (const root of scratchRoots.splice(0)) rmSync(root, { force: true, recursive: true });
});

function scratch() {
  const root = mkdtempSync(path.join(tmpdir(), 'ghostex-provenance-'));
  scratchRoots.push(root);
  return root;
}

function planFor(repo, overrides = {}) {
  return computePlan({
    entries: repo.reader.listTree(repo.head),
    isAncestor: () => true,
    readObject: repo.reader.readObject,
    scope: defaultScope(),
    sourceSha: repo.head,
    version: '7.8.0',
    ...overrides,
  });
}

/* A plausible on-disk artifact directory, exactly as a build job leaves it. */
function stageArtifactDirectory({ artifacts, platform, version = '7.8.0', extra = {} }) {
  const directory = path.join(scratch(), platform);
  mkdirSync(directory, { recursive: true });
  const entries = artifacts.map((name, index) => {
    const bytes = Buffer.from(`${platform}:${name}:${index}`);
    writeFileSync(path.join(directory, name), bytes);
    return { name, sha256: createHash('sha256').update(bytes).digest('hex'), size: bytes.length };
  });
  writeFileSync(
    path.join(directory, 'manifest.json'),
    `${JSON.stringify({ artifacts: entries, platform, schemaVersion: 1, version, ...extra }, null, 2)}\n`
  );
  return { directory, entries };
}

describe('release plan input', () => {
  test('reads an inline plan and a plan file, and refuses neither', () => {
    const repo = createFixtureRepo();
    const plan = planFor(repo);
    expect(readReleasePlan({ GHOSTEX_RELEASE_PLAN: JSON.stringify(plan) }).version).toBe('7.8.0');
    const file = path.join(scratch(), 'plan.json');
    writeFileSync(file, JSON.stringify(plan));
    expect(readReleasePlan({ GHOSTEX_RELEASE_PLAN_FILE: file }).version).toBe('7.8.0');
    expect(() => readReleasePlan({})).toThrow(/GHOSTEX_RELEASE_PLAN/u);
  });
});

describe('manifest artifact verification', () => {
  test('re-derives digests from the bytes and rejects a mismatch', () => {
    const { directory, entries } = stageArtifactDirectory({
      artifacts: ['gxserver-linux-x64.tar.gz'],
      platform: 'gxserver-linux-x64',
    });
    expect(verifyManifestArtifacts({ directory, manifest: { artifacts: entries } })).toEqual(entries);
    const tampered = [{ ...entries[0], sha256: '0'.repeat(64) }];
    expect(() => verifyManifestArtifacts({ directory, manifest: { artifacts: tampered } })).toThrow(/SHA256 mismatch/u);
    const resized = [{ ...entries[0], size: entries[0].size + 1 }];
    expect(() => verifyManifestArtifacts({ directory, manifest: { artifacts: resized } })).toThrow(/Size mismatch/u);
  });

  test('refuses a path-traversing artifact name and an empty artifact list', () => {
    const directory = scratch();
    expect(() =>
      verifyManifestArtifacts({ directory, manifest: { artifacts: [{ name: '../escape.tar.gz' }] } })
    ).toThrow(/Unsafe artifact name/u);
    expect(() => verifyManifestArtifacts({ directory, manifest: { artifacts: [] } })).toThrow(/no artifacts/u);
  });
});

describe('product provenance records', () => {
  const repo = createFixtureRepo();

  test('records a built product against this run and this release tag', () => {
    const plan = planFor(repo);
    const { directory, entries } = stageArtifactDirectory({
      artifacts: ['gxserver-linux-x64.tar.gz'],
      platform: 'gxserver-linux-x64',
    });
    const record = productProvenanceForDirectory({
      directory,
      env: { GHOSTEX_RELEASE_SOURCE_SHA: repo.head, GITHUB_RUN_ID: '31648691822' },
      plan,
    });
    expect(record.action).toBe('built');
    expect(record.product).toBe('gxserver-linux-x64');
    expect(record.fingerprint).toBe(plan.products['gxserver-linux-x64'].fingerprint);
    expect(record.originTag).toBe('v7.8.0');
    expect(record.originRunId).toBe(31648691822);
    expect(record.productVersion).toBe('7.8.0');
    expect(record.reusedFrom).toBeNull();
    expect(record.artifacts).toEqual(entries);
  });

  test('records a reused product against its origin and the verified checks', () => {
    const baseline = releaseBaselineFromPlan({ commit: repo.head, plan: planFor(repo), version: '7.7.0' });
    const plan = planFor(repo, { baselines: [baseline] });
    expect(plan.products.android.action).toBe('reuse');
    const reuse = plan.products.android.reuse;
    const { directory } = stageArtifactDirectory({ artifacts: ['ghostex-android.apk'], platform: 'android' });
    const record = productProvenanceForDirectory({
      directory,
      env: { GHOSTEX_RELEASE_SOURCE_SHA: repo.head, GITHUB_RUN_ID: '31700000000' },
      plan,
      reusedFrom: { attestationSubjectDigests: [], tag: reuse.tag, tier: 'release', verifiedChecks: [...REUSE_CHECKS] },
    });
    expect(record.action).toBe('reused');
    expect(record.originTag).toBe('v7.7.0');
    expect(record.productVersion).toBe(reuse.productVersion);
    expect(record.releaseVersion).toBe('7.8.0');
    expect(record.reusedFrom.verifiedChecks).toEqual([...REUSE_CHECKS]);
  });

  test('refuses a reuse without verified evidence', () => {
    const baseline = releaseBaselineFromPlan({ commit: repo.head, plan: planFor(repo), version: '7.7.0' });
    const plan = planFor(repo, { baselines: [baseline] });
    const { directory } = stageArtifactDirectory({ artifacts: ['ghostex-android.apk'], platform: 'android' });
    expect(() => productProvenanceForDirectory({ directory, env: {}, plan })).toThrow(/verified reuse evidence/u);
  });

  test('refuses a product the plan skipped and a manifest from another version', () => {
    const plan = planFor(repo, { scope: defaultScope({ android: false }) });
    const skipped = stageArtifactDirectory({ artifacts: ['ghostex-android.apk'], platform: 'android' });
    expect(() => productProvenanceForDirectory({ directory: skipped.directory, env: {}, plan })).toThrow(
      /skipped by the plan/u
    );
    const stale = stageArtifactDirectory({
      artifacts: ['gxserver-linux-x64.tar.gz'],
      platform: 'gxserver-linux-x64',
      version: '7.7.0',
    });
    expect(() => productProvenanceForDirectory({ directory: stale.directory, env: {}, plan })).toThrow(
      /manifest is version 7\.7\.0/u
    );
  });

  test('refuses a platform the plan does not know at all', () => {
    const plan = planFor(repo);
    const { directory } = stageArtifactDirectory({ artifacts: ['thing.tar.gz'], platform: 'macos-arm64-signed' });
    expect(() => productProvenanceForDirectory({ directory, env: {}, plan })).toThrow(/no entry for/u);
  });
});

describe('write-provenance CLI surface', () => {
  test('parses its options and requires a directory', () => {
    expect(parseWriteProvenanceArgs(['--dir', 'out', '--if-planned'])).toEqual({
      directory: 'out',
      ifPlanned: true,
      reusedFromFile: null,
    });
    expect(parseWriteProvenanceArgs(['--dir', 'out', '--reused-from', 'r.json']).reusedFromFile).toBe('r.json');
    expect(() => parseWriteProvenanceArgs([])).toThrow(/--dir/u);
    expect(() => parseWriteProvenanceArgs(['--wat'])).toThrow(/Unknown option/u);
  });

  test('renders a job summary naming the action, fingerprint, and origin', () => {
    const repo = createFixtureRepo();
    const plan = planFor(repo);
    const { directory } = stageArtifactDirectory({
      artifacts: ['gxserver-linux-x64.tar.gz'],
      platform: 'gxserver-linux-x64',
    });
    const record = productProvenanceForDirectory({ directory, env: { GITHUB_RUN_ID: '1' }, plan });
    const markdown = renderProvenanceMarkdown(record, {
      cacheHits: [{ hit: true, name: 'cargo' }],
      retries: [{ attempts: 2, label: 'zig', rule: 'zig-http-close' }],
      timings: [{ name: 'await', seconds: 3 }],
    });
    expect(markdown).toContain('### gxserver-linux-x64 — BUILT');
    expect(markdown).toContain('Cache `cargo`: hit');
    expect(markdown).toContain('Retry `zig-http-close` x2');
    expect(markdown).toContain('await: 3s');
  });
});
