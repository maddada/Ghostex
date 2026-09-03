/*
 * CDXC:Release 2026-08-13:
 * A reused product must be indistinguishable from a built one downstream. These
 * tests pin the reconstructed manifest/metadata shape against the publisher's
 * hard requirements (`assemble.mjs`) and against `release_gpui_write_manifest`,
 * and prove the materializer refuses to act on a product the plan did not mark
 * for reuse.
 */

import { afterAll, describe, expect, test } from 'vitest';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

import {
  MANIFEST_ARCHITECTURES,
  MANIFEST_IDENTITY_FIELDS,
  buildReusedManifest,
  buildReusedMetadata,
  materializeReuse,
} from './materialize-reuse.mjs';
import { computePlan } from './plan.mjs';
import { createFixtureRepo, releaseBaselineFromPlan, sourceRunFromPlan } from './plan-test-fixtures.mjs';
import { TRUSTED_REPO, defaultScope, productDefinition } from './product-inputs.mjs';
import { releaseProvenanceAssetName } from './provenance.mjs';

const record = {
  originRunId: 31501234567,
  originSourceSha: 'a'.repeat(40),
  productVersion: '7.6.0',
};

const scratchRoots = [];
afterAll(() => {
  for (const root of scratchRoots) rmSync(root, { force: true, recursive: true });
});

function scratchDirectory(label) {
  const root = mkdtempSync(path.join(tmpdir(), `ghostex-materialize-${label}-`));
  scratchRoots.push(root);
  return root;
}

function readJson(file) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

/* Deterministic stand-in bytes for a product's real artifacts. */
function bytesFor(productId, version) {
  const bytes = {};
  for (const name of productDefinition(productId).artifacts(version)) {
    bytes[name] = Buffer.from(`${productId}:${version}:${name}`);
  }
  return bytes;
}

/*
 * A `gh` that answers exactly the four calls the materializer makes, from the
 * same fixture provenance the planner used. It writes real files, so the byte
 * checks, the manifest reconstruction, and the side-file assertions all run
 * against a real directory.
 */
function stubGh({ baselines = [], bytes = {}, directory, sideFiles = true, sourceRun = null }) {
  const releaseByTag = new Map(baselines.map((baseline) => [baseline.tag, baseline]));
  return (args) => {
    const joined = args.join(' ');
    if (args[0] === 'api' && /\/releases\/tags\//u.test(args[1])) {
      const tag = args[1].split('/').pop();
      const baseline = releaseByTag.get(tag);
      if (!baseline) throw new Error(`stub gh: no fixture release ${tag}`);
      return {
        ok: true,
        stderr: '',
        stdout: JSON.stringify({
          assets: [
            ...baseline.assets.map((asset) => ({ ...asset })),
            { digest: null, id: 1, name: releaseProvenanceAssetName(baseline.provenance.version), size: 1 },
          ],
          draft: baseline.draft,
        }),
      };
    }
    if (args[0] === 'api' && /\/releases\/assets\//u.test(args[1])) {
      const [baseline] = baselines;
      return { ok: true, stderr: '', stdout: JSON.stringify(baseline.provenance) };
    }
    if (args[0] === 'release' && args[1] === 'download') {
      const pattern = args[args.indexOf('--pattern') + 1];
      const dir = args[args.indexOf('--dir') + 1];
      const product = Object.keys(bytes).find((id) => bytes[id][pattern] !== undefined);
      if (!product) throw new Error(`stub gh: no fixture bytes for ${pattern}`);
      mkdirSync(dir, { recursive: true });
      writeFileSync(path.join(dir, pattern), bytes[product][pattern]);
      return { ok: true, stderr: '', stdout: '' };
    }
    if (args[0] === 'run' && args[1] === 'view') {
      return {
        ok: true,
        stderr: '',
        stdout: JSON.stringify({
          conclusion: sourceRun.conclusion,
          event: sourceRun.event,
          headSha: sourceRun.headSha,
          workflowName: sourceRun.workflowName,
        }),
      };
    }
    if (args[0] === 'run' && args[1] === 'download') {
      const name = args[args.indexOf('--name') + 1];
      const dir = args[args.indexOf('--dir') + 1];
      mkdirSync(dir, { recursive: true });
      if (name.startsWith('release-provenance-')) {
        const product = name.slice('release-provenance-'.length);
        writeFileSync(path.join(dir, 'provenance.json'), JSON.stringify(sourceRun.products[product]));
        return { ok: true, stderr: '', stdout: '' };
      }
      const product = name.slice('release-'.length);
      const origin = sourceRun.products[product];
      for (const artifact of origin.artifacts) {
        writeFileSync(path.join(dir, artifact.name), bytes[product][artifact.name]);
      }
      writeFileSync(
        path.join(dir, 'manifest.json'),
        JSON.stringify({
          artifacts: origin.artifacts,
          platform: product,
          schemaVersion: 1,
          version: origin.releaseVersion,
        })
      );
      if (sideFiles) {
        for (const sideFile of productDefinition(product).sideFiles ?? []) {
          writeFileSync(path.join(dir, sideFile), '<rss/>\n');
        }
      }
      return { ok: true, stderr: '', stdout: '' };
    }
    throw new Error(`stub gh: unexpected call ${joined}`);
  };
}

function planFor(repo, overrides = {}) {
  return computePlan({
    entries: repo.reader.listTree(repo.head),
    isAncestor: () => true,
    readObject: repo.reader.readObject,
    repo: TRUSTED_REPO,
    scope: defaultScope(),
    sourceSha: repo.head,
    version: '7.8.0',
    ...overrides,
  });
}

describe('reconstructed manifests for reused products', () => {
  const artifacts = [{ name: 'ghostex-android.apk', sha256: 'b'.repeat(64), size: 91234567 }];

  test('carries the identity fields the publisher enforces for Android', () => {
    const manifest = buildReusedManifest({
      artifacts,
      product: 'android',
      record,
      runId: 31700000000,
      version: '7.8.0',
      workflowSha: 'c'.repeat(40),
    });
    /* assemble.mjs:111-118 rejects an Android manifest without these two. */
    expect(manifest.source_kind).toBe('react-native-mobile');
    expect(manifest.application_id).toBe('io.ghostex');
    expect(manifest.platform).toBe('android');
    expect(manifest.schemaVersion).toBe(1);
    /* The release version is the NEW one; the source sha stays truthful. */
    expect(manifest.version).toBe('7.8.0');
    expect(manifest.source_sha).toBe(record.originSourceSha);
    expect(manifest.workflow_run_id).toBe(31700000000);
  });

  test('omits identity fields for products that have none', () => {
    const manifest = buildReusedManifest({
      artifacts: [{ name: 'gxserver-linux-x64.tar.gz', sha256: 'd'.repeat(64), size: 10 }],
      product: 'gxserver-linux-x64',
      record,
      runId: 1,
      version: '7.8.0',
      workflowSha: '',
    });
    expect(manifest.source_kind).toBeUndefined();
    expect(manifest.application_id).toBeUndefined();
    expect(manifest.workflow_sha).toBeUndefined();
  });

  test('mirrors the architecture map and single-artifact spread of common.sh', () => {
    const metadata = buildReusedMetadata({
      artifacts,
      product: 'android',
      record,
      runId: 1,
      version: '7.8.0',
      workflowSha: '',
    });
    expect(metadata.architecture).toBe(MANIFEST_ARCHITECTURES.android);
    expect(metadata.package).toBe('android');
    expect(metadata.name).toBe('ghostex-android.apk');
    expect(metadata.sha256).toBe(artifacts[0].sha256);
    const unknown = buildReusedMetadata({
      artifacts,
      product: 'linux-deb-x64',
      record,
      runId: 1,
      version: '7.8.0',
      workflowSha: '',
    });
    expect(unknown.architecture).toBe('unknown');
  });

  test('declares identity fields only for products the publisher checks', () => {
    expect(Object.keys(MANIFEST_IDENTITY_FIELDS)).toEqual(['android']);
  });
});

describe('materialization guards', () => {
  const repo = createFixtureRepo();

  test('refuses a product the plan did not mark for reuse', () => {
    const plan = planFor(repo);
    expect(() =>
      materializeReuse({ directory: '/tmp/does-not-matter', isAncestor: () => true, plan, product: 'macos-arm64' })
    ).toThrow(/planned as build, not reuse/u);
  });

  test('refuses a product the plan does not contain', () => {
    const plan = planFor(repo);
    expect(() =>
      materializeReuse({ directory: '/tmp/does-not-matter', isAncestor: () => true, plan, product: 'android' })
    ).toThrow(/planned as build, not reuse/u);
    plan.products.android = undefined;
    expect(() =>
      materializeReuse({ directory: '/tmp/does-not-matter', isAncestor: () => true, plan, product: 'android' })
    ).toThrow(/no entry for android/u);
  });

  test('the planner never offers a version-stamped product from another release', () => {
    const baseline = releaseBaselineFromPlan({ commit: repo.head, plan: planFor(repo), version: '7.7.0' });
    const plan = planFor(repo, { baselines: [baseline] });
    for (const productId of ['macos-arm64', 'windows-x64', 'linux-deb-x64', 'gxserver-wsl-windows-x64']) {
      expect(plan.products[productId].action).toBe('build');
    }
    expect(plan.products.android.action).toBe('reuse');
    expect(plan.products['gxserver-linux-x64'].action).toBe('reuse');
  });

  /*
   * ...and if a hand-edited plan ever did offer one, the materializer itself
   * refuses it. This is the check that actually protects the DMG, so it runs the
   * full download/verify path rather than only asserting the planner's output.
   */
  test('materialization refuses a forged cross-release reuse of a version-stamped product', () => {
    const directory = scratchDirectory('forged');
    const baselinePlan = planFor(repo, { version: '7.6.0' });
    const baseline = releaseBaselineFromPlan({
      artifactBytes: { 'macos-arm64': bytesFor('macos-arm64', '7.6.0') },
      commit: repo.head,
      plan: baselinePlan,
      version: '7.6.0',
    });
    const plan = planFor(repo);
    const originRecord = baseline.provenance.products['macos-arm64'];
    plan.products['macos-arm64'] = {
      ...plan.products['macos-arm64'],
      action: 'reuse',
      reuse: {
        artifacts: originRecord.artifacts,
        originSourceSha: repo.head,
        productVersion: '7.6.0',
        runId: originRecord.originRunId,
        tag: 'v7.6.0',
        tier: 'release',
      },
    };
    expect(() =>
      materializeReuse({
        attestationVerifier: () => true,
        directory,
        env: {},
        gh: stubGh({ baselines: [baseline], bytes: { 'macos-arm64': bytesFor('macos-arm64', '7.6.0') }, directory }),
        isAncestor: () => true,
        plan,
        product: 'macos-arm64',
      })
    ).toThrow(/Refusing to reuse macos-arm64/u);
  });
});

/*
 * The accept path — download, four-check re-verification, manifest/metadata
 * reconstruction, side files, provenance emission — is what actually decides
 * whether stale bytes reach a public release, so it runs here against fixture
 * directories and a stubbed `gh` rather than only in CI.
 */
describe('materializing a reused product end to end', () => {
  const repo = createFixtureRepo();
  afterAll(() => repo.dispose());

  function tier1({ tamper = false } = {}) {
    const directory = scratchDirectory(tamper ? 'tampered' : 'tier1');
    const bytes = { android: bytesFor('android', '7.6.0') };
    const baselinePlan = planFor(repo, { version: '7.6.0' });
    const baseline = releaseBaselineFromPlan({
      artifactBytes: bytes,
      commit: repo.head,
      plan: baselinePlan,
      version: '7.6.0',
    });
    const plan = planFor(repo, { baselines: [baseline] });
    expect(plan.products.android.action).toBe('reuse');
    const published = tamper ? { android: { 'ghostex-android.apk': Buffer.from('not the published bytes') } } : bytes;
    return {
      baseline,
      directory,
      gh: stubGh({ baselines: [baseline], bytes: published, directory }),
      plan,
    };
  }

  test("Tier 1: accepts the published bytes and reconstructs the publisher's contract", () => {
    const { directory, gh, plan } = tier1();
    const record = materializeReuse({
      attestationVerifier: () => true,
      directory,
      env: { GITHUB_RUN_ID: '31700000001', GITHUB_SHA: repo.head },
      gh,
      isAncestor: () => true,
      plan,
      product: 'android',
    });

    expect(record.action).toBe('reused');
    expect(record.reusedFrom).toMatchObject({ tag: 'v7.6.0', tier: 'release' });
    expect(record.reusedFrom.verifiedChecks.sort()).toEqual(['attestation', 'digest', 'fingerprint', 'origin']);
    expect(record.productVersion).toBe('7.6.0');
    expect(record.releaseVersion).toBe('7.8.0');

    const manifest = readJson(path.join(directory, 'manifest.json'));
    expect(manifest.platform).toBe('android');
    expect(manifest.version).toBe('7.8.0');
    expect(manifest.source_kind).toBe('react-native-mobile');
    expect(manifest.application_id).toBe('io.ghostex');
    expect(manifest.artifacts[0].sha256).toBe(record.artifacts[0].sha256);
    const metadata = readJson(path.join(directory, 'metadata.json'));
    expect(metadata.architecture).toBe('universal');
    expect(readJson(path.join(directory, 'provenance.json')).product).toBe('android');
  });

  test('Tier 1: refuses byte-tampered bytes even though the plan authorized the reuse', () => {
    const { directory, gh, plan } = tier1({ tamper: true });
    expect(() =>
      materializeReuse({
        attestationVerifier: () => true,
        directory,
        env: {},
        gh,
        isAncestor: () => true,
        plan,
        product: 'android',
      })
    ).toThrow(/bytes do not match the provenance record/u);
  });

  test('Tier 1: refuses when the build attestation cannot be verified', () => {
    const { directory, gh, plan } = tier1();
    expect(() =>
      materializeReuse({
        attestationVerifier: () => false,
        directory,
        env: {},
        gh,
        isAncestor: () => true,
        plan,
        product: 'android',
      })
    ).toThrow(/has no verifiable build attestation/u);
  });

  test("Tier 2: accepts a same-version run and requires the product's side files", () => {
    const directory = scratchDirectory('tier2');
    const bytes = { 'macos-arm64': bytesFor('macos-arm64', '7.8.0') };
    const failedPlan = planFor(repo);
    const sourceRun = sourceRunFromPlan({
      artifactBytes: bytes,
      headSha: repo.head,
      plan: failedPlan,
      version: '7.8.0',
    });
    const plan = planFor(repo, { reuseFromRunId: sourceRun.runId, sourceRun });
    expect(plan.products['macos-arm64'].action).toBe('reuse');
    expect(plan.products['macos-arm64'].reuse.tier).toBe('run');

    const gh = stubGh({ bytes, directory, sourceRun });
    const record = materializeReuse({
      attestationVerifier: () => true,
      directory,
      env: { GITHUB_RUN_ID: '31700000002' },
      gh,
      isAncestor: () => true,
      plan,
      product: 'macos-arm64',
    });
    expect(record.reusedFrom).toMatchObject({ runId: sourceRun.runId, tier: 'run' });
    expect(record.productVersion).toBe('7.8.0');
    /* The run artifact is copied verbatim, so its own manifest survives. */
    expect(readJson(path.join(directory, 'manifest.json')).platform).toBe('macos-arm64');
    expect(existsSync(path.join(directory, 'appcast.xml'))).toBe(true);
  });

  test("Tier 2: refuses a run artifact that lost the product's side file", () => {
    const directory = scratchDirectory('tier2-missing-side-file');
    const bytes = { 'macos-arm64': bytesFor('macos-arm64', '7.8.0') };
    const failedPlan = planFor(repo);
    const sourceRun = sourceRunFromPlan({
      artifactBytes: bytes,
      headSha: repo.head,
      plan: failedPlan,
      version: '7.8.0',
    });
    const plan = planFor(repo, { reuseFromRunId: sourceRun.runId, sourceRun });
    const gh = stubGh({ bytes, directory, sideFiles: false, sourceRun });
    expect(() =>
      materializeReuse({
        attestationVerifier: () => true,
        directory,
        env: {},
        gh,
        isAncestor: () => true,
        plan,
        product: 'macos-arm64',
      })
    ).toThrow(/missing its side file appcast\.xml/u);
  });
});
