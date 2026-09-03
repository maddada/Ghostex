/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
 * The publisher refusal matrix (§9) as executable specification.
 *
 * These tests exercise the exact decisions `assemble.mjs` makes before it tags,
 * uploads, or pushes anything: does this artifact set describe the plan the run
 * was gated on, is every reuse claim complete, does the release page state what
 * happened, and do Sparkle/Homebrew/Windows feeds advance for the right reason.
 * A publisher that accepts a mixed set is the one failure this whole design must
 * never allow, so every rejection has a named test.
 */

import { describe, expect, test } from 'vitest';

import { computePlan, validatePlan } from './plan.mjs';
import { createFixtureRepo, productRecordsFromPlan, releaseBaselineFromPlan } from './plan-test-fixtures.mjs';
import { defaultScope, productDefinition } from './product-inputs.mjs';
import { buildProductProvenance, releaseProvenanceAssetName } from './provenance.mjs';
import {
  assertLiveProvenanceMatches,
  assertPlanMatchesScope,
  assertPlansAgree,
  assertSingleBuildOrigin,
  buildReleaseProvenanceRecord,
  collectPublishProvenance,
  crossReleaseReuseOrigins,
  isNonProductArtifactDirectory,
  readPublishPlan,
  renderBuildProvenanceNotes,
  renderReleaseProvenanceReport,
  resolveMacosFeedScope,
  resolvePublishRecoveryInputs,
  resolveWindowsFeedScope,
  verifyReleaseProvenanceAgainstAssets,
} from './publish-provenance.mjs';

const VERSION = '7.8.0';
const RUN_ID = 31700000001;

const repo = createFixtureRepo();

function planFor(overrides = {}) {
  return computePlan({
    entries: repo.reader.listTree(repo.head),
    isAncestor: () => true,
    readObject: repo.reader.readObject,
    scope: defaultScope(),
    sourceSha: repo.head,
    version: VERSION,
    ...overrides,
  });
}

/* A plan where android and both gxservers are reusable from v7.6.0. */
function reusingPlan() {
  const baselinePlan = planFor({ version: '7.6.0' });
  const baseline = releaseBaselineFromPlan({ commit: repo.head, plan: baselinePlan, version: '7.6.0' });
  return { baseline, plan: planFor({ baselines: [baseline] }) };
}

function artifactsFor(product, records) {
  return records[product].artifacts.map((artifact) => ({ ...artifact }));
}

/* The `release-<platform>` artifact set a run uploads, as the publisher sees it. */
function artifactSetFor(plan, { records }) {
  const manifests = [];
  const provenance = {};
  for (const product of plan.expectedPlatforms) {
    const directory = `/artifacts/release-${product}`;
    manifests.push({
      artifacts: artifactsFor(product, records),
      directory,
      platform: product,
      schemaVersion: 1,
      version: plan.version,
    });
    provenance[directory] = records[product];
  }
  return { manifests, provenance };
}

function reusedRecord({ entry, plan, product, tier = 'release', tag = 'v7.6.0', runId = 31501234567 }) {
  const definition = productDefinition(product);
  return buildProductProvenance({
    action: 'reused',
    algorithmRevision: plan.algorithmRevision,
    artifacts: entry.reuse.artifacts,
    fingerprint: entry.fingerprint,
    inputs: entry.inputs,
    originRunId: runId,
    originSourceSha: entry.reuse.originSourceSha,
    originTag: tier === 'release' ? tag : `v${plan.version}`,
    product,
    productVersion: definition.versionStamped ? plan.version : entry.reuse.productVersion,
    releaseVersion: plan.version,
    reusedFrom: {
      attestationSubjectDigests: entry.reuse.artifacts.map((artifact) => artifact.sha256),
      ...(tier === 'release' ? { tag } : { runId }),
      tier,
      verifiedChecks: ['fingerprint', 'digest', 'origin', 'attestation'],
    },
    sourceSha: plan.sourceSha,
  });
}

/* Records for a plan, using the reuse descriptors the plan itself authorized. */
function recordsForPlan(plan) {
  const built = productRecordsFromPlan({
    plan,
    runId: RUN_ID,
    sourceSha: plan.sourceSha,
    tag: `v${plan.version}`,
    version: plan.version,
  });
  const records = {};
  for (const product of plan.expectedPlatforms) {
    const entry = plan.products[product];
    records[product] =
      entry.action === 'reuse'
        ? reusedRecord({ entry, plan, product, runId: entry.reuse.runId, tag: entry.reuse.tag })
        : built[product];
  }
  return records;
}

function collect(plan, set) {
  return collectPublishProvenance({
    manifests: set.manifests,
    plan,
    readProvenance: (directory) => set.provenance[directory] ?? null,
    version: plan.version,
  });
}

describe('plan intake', () => {
  const plan = planFor();
  const inline = JSON.stringify(plan);

  test('accepts the inline plan alone', () => {
    const resolved = readPublishPlan({
      artifactsRoot: '/artifacts',
      env: { GHOSTEX_RELEASE_PLAN: inline },
      fileExists: () => false,
      readTextFile: () => '',
    });
    expect(resolved.version).toBe(VERSION);
  });

  test('accepts the uploaded release-plan artifact alone (publish-only recovery)', () => {
    const resolved = readPublishPlan({
      artifactsRoot: '/artifacts',
      env: {},
      fileExists: (file) => file === '/artifacts/release-plan/release-plan.json',
      readTextFile: () => inline,
    });
    expect(resolved.sourceSha).toBe(plan.sourceSha);
  });

  test('refuses when neither source is present', () => {
    expect(() =>
      readPublishPlan({ artifactsRoot: '/artifacts', env: {}, fileExists: () => false, readTextFile: () => '' })
    ).toThrow(/no resolved release plan/u);
  });

  test("refuses when the dispatched plan disagrees with the run's recorded plan", () => {
    const drifted = JSON.parse(inline);
    drifted.products['macos-arm64'].fingerprint = '9'.repeat(64);
    expect(() =>
      readPublishPlan({
        artifactsRoot: '/artifacts',
        env: { GHOSTEX_RELEASE_PLAN: JSON.stringify(drifted) },
        fileExists: () => true,
        readTextFile: () => inline,
      })
    ).toThrow(/Refusing to publish/u);
  });

  test('refuses a plan whose expected platforms differ from the recorded run', () => {
    const other = planFor({ scope: defaultScope({ android: false }) });
    expect(() => assertPlansAgree(plan, other)).toThrow(/expected platforms|expects/u);
  });

  test('refuses when the workflow input scope does not equal the plan', () => {
    expect(() => assertPlanMatchesScope({ expectedPlatforms: ['macos-arm64'], plan, version: VERSION })).toThrow(
      /does not equal the plan/u
    );
    expect(() => assertPlanMatchesScope({ expectedPlatforms: plan.expectedPlatforms, plan, version: '7.9.0' })).toThrow(
      /the plan releases 7\.8\.0/u
    );
    expect(
      assertPlanMatchesScope({ expectedPlatforms: [...plan.expectedPlatforms].reverse(), plan, version: VERSION })
    ).toEqual(plan.expectedPlatforms);
  });

  test('tolerates the non-product artifact directories the run also uploads', () => {
    expect(isNonProductArtifactDirectory('release-plan')).toBe(true);
    expect(isNonProductArtifactDirectory('release-provenance-android')).toBe(true);
    expect(isNonProductArtifactDirectory('release-code-server-390f119a145e-p2-linux-x64')).toBe(true);
    expect(isNonProductArtifactDirectory('release-android')).toBe(false);
  });
});

describe('publisher refusal matrix', () => {
  test('accepts an all-built set that matches the plan', () => {
    const plan = planFor();
    const records = recordsForPlan(plan);
    const accepted = collect(plan, artifactSetFor(plan, { records }));
    expect(Object.keys(accepted).sort()).toEqual([...plan.expectedPlatforms].sort());
    expect(Object.values(accepted).every((record) => record.action === 'built')).toBe(true);
  });

  test('accepts a mixed built/reused set that matches the plan', () => {
    const { plan } = reusingPlan();
    expect(plan.products.android.action).toBe('reuse');
    const accepted = collect(plan, artifactSetFor(plan, { records: recordsForPlan(plan) }));
    expect(accepted.android.action).toBe('reused');
    expect(accepted['macos-arm64'].action).toBe('built');
  });

  test('refuses a missing product', () => {
    const plan = planFor();
    const set = artifactSetFor(plan, { records: recordsForPlan(plan) });
    set.manifests = set.manifests.filter((manifest) => manifest.platform !== 'android');
    expect(() => collect(plan, set)).toThrow(/Refusing to publish android: the plan expects it/u);
  });

  test('refuses a product the plan skipped', () => {
    const plan = planFor({ scope: defaultScope({ android: false }) });
    const records = recordsForPlan(planFor());
    const set = artifactSetFor(plan, { records });
    set.manifests.push({
      artifacts: artifactsFor('android', records),
      directory: '/artifacts/release-android',
      platform: 'android',
      schemaVersion: 1,
      version: plan.version,
    });
    set.provenance['/artifacts/release-android'] = records.android;
    expect(() => collect(plan, set)).toThrow(/the plan skipped it/u);
  });

  test('refuses a product whose artifact carries no provenance record', () => {
    const plan = planFor();
    const set = artifactSetFor(plan, { records: recordsForPlan(plan) });
    delete set.provenance['/artifacts/release-android'];
    expect(() => collect(plan, set)).toThrow(/carries no provenance\.json/u);
  });

  test('refuses a fingerprint that does not match the plan', () => {
    const plan = planFor();
    const records = recordsForPlan(plan);
    records.android = { ...records.android, fingerprint: '0'.repeat(64) };
    expect(() => collect(plan, artifactSetFor(plan, { records }))).toThrow(/fingerprint must equal/u);
  });

  test('refuses a record whose action contradicts the plan', () => {
    const { plan } = reusingPlan();
    const records = recordsForPlan(plan);
    /* The plan says reuse; the artifact claims it was freshly built. */
    records.android = productRecordsFromPlan({
      plan,
      runId: RUN_ID,
      sourceSha: plan.sourceSha,
      tag: `v${plan.version}`,
      version: plan.version,
    }).android;
    expect(() => collect(plan, artifactSetFor(plan, { records }))).toThrow(/action must equal reused/u);
  });

  test('refuses a reused product missing any of the four verified checks', () => {
    const { plan } = reusingPlan();
    const records = recordsForPlan(plan);
    records.android = {
      ...records.android,
      reusedFrom: { ...records.android.reusedFrom, verifiedChecks: ['fingerprint', 'digest', 'origin'] },
    };
    expect(() => collect(plan, artifactSetFor(plan, { records }))).toThrow(/verifiedChecks is missing attestation/u);
  });

  /*
   * The invariant that keeps the DMG, deb, rpm, Windows set, and WSL zip honest.
   * The plan entry below is deliberately a *valid* same-version reuse, so the
   * refusal cannot come from the plan/record action mismatch — it has to come
   * from the version-stamped rule inside the record itself.
   */
  test('refuses a version-stamped product whose record claims an older product version', () => {
    const plan = planFor();
    const reuse = {
      artifacts: [{ name: `ghostex-${VERSION}-arm64.dmg`, sha256: 'a'.repeat(64), size: 10 }],
      originSourceSha: repo.head,
      productVersion: VERSION,
      runId: 31501234567,
      tier: 'run',
    };
    const entry = { ...plan.products['macos-arm64'], action: 'reuse', reuse };
    plan.products['macos-arm64'] = entry;

    const honest = reusedRecord({ entry, plan, product: 'macos-arm64', runId: reuse.runId, tier: 'run' });
    expect(honest.productVersion).toBe(VERSION);

    const records = recordsForPlan(plan);
    records['macos-arm64'] = { ...honest, productVersion: '7.6.0' };
    expect(() => collect(plan, artifactSetFor(plan, { records }))).toThrow(
      /macos-arm64 is version-stamped and may never be reused across releases/u
    );
  });

  test('refuses a version-stamped product the plan authorized from another release tag', () => {
    const plan = planFor();
    const reuse = {
      artifacts: [{ name: `ghostex-${VERSION}-arm64.dmg`, sha256: 'a'.repeat(64), size: 10 }],
      originSourceSha: repo.head,
      productVersion: '7.6.0',
      runId: 31501234567,
      tag: 'v7.6.0',
      tier: 'release',
    };
    plan.products['macos-arm64'] = { ...plan.products['macos-arm64'], action: 'reuse', reuse };
    /* validatePlan alone already refuses this plan, before any artifact arrives. */
    expect(() => validatePlan(plan)).toThrow(/version-stamped and may never be reused across releases/u);
  });

  test("refuses when the record's artifacts do not equal the manifest's", () => {
    const plan = planFor();
    const records = recordsForPlan(plan);
    const set = artifactSetFor(plan, { records });
    const manifest = set.manifests.find((entry) => entry.platform === 'android');
    manifest.artifacts = [{ ...manifest.artifacts[0], sha256: 'c'.repeat(64) }];
    expect(() => collect(plan, set)).toThrow(/artifacts must equal the manifest artifacts/u);
  });

  test('refuses a record produced at a different source commit', () => {
    const plan = planFor();
    const records = recordsForPlan(plan);
    records.android = { ...records.android, sourceSha: 'f'.repeat(40) };
    expect(() => collect(plan, artifactSetFor(plan, { records }))).toThrow(/but the plan was computed at/u);
  });

  test('refuses a reuse whose origin is not the one the plan authorized', () => {
    const { plan } = reusingPlan();
    const records = recordsForPlan(plan);
    records.android = {
      ...records.android,
      reusedFrom: { ...records.android.reusedFrom, tag: 'v7.5.0' },
      originTag: 'v7.5.0',
    };
    expect(() => collect(plan, artifactSetFor(plan, { records }))).toThrow(/but the plan authorized v7\.6\.0/u);
  });

  test('refuses built artifacts that come from more than one Actions run', () => {
    const plan = planFor();
    const records = recordsForPlan(plan);
    const accepted = collect(plan, artifactSetFor(plan, { records }));
    expect(assertSingleBuildOrigin({ records: accepted })).toBe(RUN_ID);
    accepted.android = { ...accepted.android, originRunId: 42 };
    expect(() => assertSingleBuildOrigin({ records: accepted })).toThrow(/more than one Actions run/u);
  });

  test('refuses built artifacts from a run other than the nominated source run', () => {
    const plan = planFor();
    const accepted = collect(plan, artifactSetFor(plan, { records: recordsForPlan(plan) }));
    expect(() => assertSingleBuildOrigin({ expectedRunId: 999, records: accepted })).toThrow(
      /not the nominated source run 999/u
    );
    expect(assertSingleBuildOrigin({ expectedRunId: String(RUN_ID), records: accepted })).toBe(RUN_ID);
  });

  test('does not compare origin runs for reused products', () => {
    const { plan } = reusingPlan();
    const accepted = collect(plan, artifactSetFor(plan, { records: recordsForPlan(plan) }));
    expect(accepted.android.originRunId).not.toBe(RUN_ID);
    expect(assertSingleBuildOrigin({ expectedRunId: RUN_ID, records: accepted })).toBe(RUN_ID);
  });
});

describe('feed scope', () => {
  test('advances Sparkle and Homebrew when macOS was built', () => {
    const plan = planFor();
    const scope = resolveMacosFeedScope({ plan, updateSparkleRequested: true });
    expect(scope).toMatchObject({ homebrew: true, macosAction: 'build', sparkle: true });
  });

  test('still advances Sparkle and Homebrew when macOS is reused into the same version', () => {
    /*
     * The design's §4.10 rule ("only when macOS action == build") is wrong for a
     * same-version recovery: the reused DMG comes from a run that never
     * published, so no appcast entry and no cask update exist yet.
     */
    const plan = planFor();
    plan.products['macos-arm64'].action = 'reuse';
    const scope = resolveMacosFeedScope({ plan, updateSparkleRequested: true });
    expect(scope.sparkle).toBe(true);
    expect(scope.homebrew).toBe(true);
    expect(scope.reason).toMatch(/same-version/u);
  });

  test('the plan document agrees with the publisher about a reused macOS', () => {
    const plan = planFor();
    plan.products['macos-arm64'].action = 'reuse';
    plan.products['macos-arm64'].reuse = {
      artifacts: [{ name: `ghostex-${VERSION}-arm64.dmg`, sha256: 'a'.repeat(64), size: 10 }],
      originSourceSha: repo.head,
      productVersion: VERSION,
      runId: 31644067583,
      tier: 'run',
    };
    plan.feeds = { homebrew: true, sparkle: true, windowsFeeds: ['x64', 'arm64'] };
    expect(() => validatePlan(plan)).not.toThrow();
  });

  test('holds both feeds when macOS is not in the release', () => {
    const plan = planFor({ scope: defaultScope({ macos: false, updateSparkle: false }) });
    const scope = resolveMacosFeedScope({ plan, updateSparkleRequested: false });
    expect(scope).toMatchObject({ homebrew: false, macosAction: 'skip', sparkle: false });
  });

  test('refuses a Sparkle advance without macOS in the release', () => {
    const plan = planFor({ scope: defaultScope({ macos: false, updateSparkle: false }) });
    expect(() => resolveMacosFeedScope({ plan, updateSparkleRequested: true })).toThrow(
      /macOS is not part of this release/u
    );
  });

  test('reports which Windows channels are regenerated versus carried forward', () => {
    const plan = planFor();
    expect(resolveWindowsFeedScope({ plan })).toEqual({ carriedForward: [], regenerated: ['x64', 'arm64'] });
    plan.products['windows-arm64'].action = 'reuse';
    plan.products['windows-x64'].action = 'skip';
    expect(resolveWindowsFeedScope({ plan })).toEqual({ carriedForward: ['arm64'], regenerated: [] });
  });
});

describe('release provenance asset and notes', () => {
  const { plan } = reusingPlan();
  const records = recordsForPlan(plan);
  const accepted = collect(plan, artifactSetFor(plan, { records }));
  const releaseProvenance = buildReleaseProvenanceRecord({
    plan,
    productRecords: accepted,
    publishedAt: '2026-08-13T10:11:12.000Z',
    sourceSha: plan.sourceSha,
    version: VERSION,
    workflowRunId: RUN_ID,
  });

  test('records every published product, the plan, and the component state', () => {
    expect(releaseProvenance.tag).toBe(`v${VERSION}`);
    expect(Object.keys(releaseProvenance.products).sort()).toEqual([...plan.expectedPlatforms].sort());
    expect(releaseProvenance.plan.version).toBe(VERSION);
    expect(Object.keys(releaseProvenance.components).sort()).toEqual(['cef', 'code-server']);
    for (const component of Object.values(releaseProvenance.components)) {
      expect(component).toHaveProperty('identityRevisionInputsDigest');
      expect(component).toHaveProperty('requiredPlatforms');
    }
  });

  test('the notes state built versus reused per product with the origin', () => {
    const notes = renderBuildProvenanceNotes(releaseProvenance);
    expect(notes).toContain('## Build provenance');
    expect(notes).toContain(`| macos-arm64 | built | ${VERSION} | this release |`);
    expect(notes).toContain('| android | reused | 7.6.0 | unchanged since v7.6.0 |');
    /* A payload with no marketing version must not pretend to have one. */
    expect(notes).toContain('| gxserver-linux-x64 | reused | — | unchanged since v7.6.0 |');
    expect(notes).toContain(releaseProvenanceAssetName(VERSION));
  });

  test('the operator report separates built, reused, and skipped', () => {
    const report = renderReleaseProvenanceReport(releaseProvenance);
    expect(report).toMatch(/^BUILT {5}.*macos-arm64 \(/mu);
    expect(report).toMatch(/^REUSED {4}.*android \(/mu);
    expect(report).toMatch(/^SKIPPED {3}by flag: \(none\)/mu);

    /* A product excluded by a scope flag is reported as skipped, never as absent. */
    const partialPlan = planFor({ scope: defaultScope({ android: false }) });
    const partial = buildReleaseProvenanceRecord({
      plan: partialPlan,
      productRecords: collect(partialPlan, artifactSetFor(partialPlan, { records: recordsForPlan(partialPlan) })),
      publishedAt: '2026-08-13T10:11:12.000Z',
      sourceSha: partialPlan.sourceSha,
      version: VERSION,
      workflowRunId: RUN_ID,
    });
    expect(renderReleaseProvenanceReport(partial)).toMatch(/^SKIPPED {3}by flag: android/mu);
  });

  test('verifies the recorded digests against the live release assets', () => {
    const liveAssets = [
      ...Object.values(releaseProvenance.products).flatMap((record) =>
        record.artifacts.map((artifact) => ({ name: artifact.name, sha256: artifact.sha256, size: artifact.size }))
      ),
      { name: releaseProvenanceAssetName(VERSION), sha256: 'e'.repeat(64), size: 4096 },
    ];
    expect(verifyReleaseProvenanceAgainstAssets({ liveAssets, releaseProvenance, version: VERSION })).toEqual([]);

    const missing = liveAssets.filter((asset) => asset.name !== 'ghostex-android.apk');
    expect(
      verifyReleaseProvenanceAgainstAssets({ liveAssets: missing, releaseProvenance, version: VERSION })
    ).toContainEqual(expect.stringContaining('ghostex-android.apk is recorded for android'));

    const tampered = liveAssets.map((asset) =>
      asset.name === 'ghostex-android.apk' ? { ...asset, sha256: 'd'.repeat(64) } : asset
    );
    expect(
      verifyReleaseProvenanceAgainstAssets({ liveAssets: tampered, releaseProvenance, version: VERSION })
    ).toContainEqual(expect.stringContaining('live digest'));

    const extra = [...liveAssets, { name: 'surprise.zip', sha256: 'f'.repeat(64), size: 1 }];
    expect(
      verifyReleaseProvenanceAgainstAssets({ liveAssets: extra, releaseProvenance, version: VERSION })
    ).toContainEqual(expect.stringContaining('surprise.zip is published but recorded in no provenance record'));
  });

  test('idempotent re-publication compares what the record claims, not its bytes', () => {
    /*
     * A second publisher run recomputes the record, so `publishedAt` and
     * `workflowRunId` legitimately differ. Everything the record asserts about
     * the release must not.
     */
    const republished = buildReleaseProvenanceRecord({
      plan,
      productRecords: accepted,
      publishedAt: '2026-08-14T00:00:00.000Z',
      sourceSha: plan.sourceSha,
      version: VERSION,
      workflowRunId: RUN_ID + 1,
    });
    expect(() => assertLiveProvenanceMatches({ live: releaseProvenance, record: republished })).not.toThrow();

    const forkedAction = JSON.parse(JSON.stringify(releaseProvenance));
    forkedAction.products.android.fingerprint = '9'.repeat(64);
    expect(() => assertLiveProvenanceMatches({ live: forkedAction, record: republished })).toThrow(
      /Refusing to publish android/u
    );

    const forkedDigest = JSON.parse(JSON.stringify(releaseProvenance));
    forkedDigest.products.android.artifacts[0].sha256 = '8'.repeat(64);
    expect(() => assertLiveProvenanceMatches({ live: forkedDigest, record: republished })).toThrow(
      /different artifact digests/u
    );

    const missingProduct = JSON.parse(JSON.stringify(releaseProvenance));
    delete missingProduct.products.android;
    expect(() => assertLiveProvenanceMatches({ live: missingProduct, record: republished })).toThrow(
      /but this run validated/u
    );
  });

  test('names every cross-release reuse so the verifier can byte-match its origin', () => {
    const origins = crossReleaseReuseOrigins(releaseProvenance);
    expect(origins.map((origin) => origin.product).sort()).toEqual([
      'android',
      'gxserver-linux-arm64',
      'gxserver-linux-x64',
    ]);
    for (const origin of origins) {
      expect(origin.tag).toBe('v7.6.0');
      expect(origin.versionStamped).toBe(false);
      expect(origin.artifacts.length).toBeGreaterThan(0);
    }
  });
});

/*
 * Scenario H — publish-only recovery.
 *
 * The dispatcher re-types the scope and signing flags hours after the run that
 * built the artifacts. Everything that changes what gets published must come
 * from the run's own recorded plan instead, or a prerelease quietly ships as
 * stable the first time someone forgets a flag.
 */
describe('Scenario H — publish-only recovery reads the recorded plan', () => {
  const prereleasePlan = () =>
    planFor({ scope: defaultScope({ prerelease: true, signWindows: true, updateSparkle: false }) });

  test('takes prerelease, Sparkle, and Windows signing from the recorded plan', () => {
    const plan = prereleasePlan();
    const resolved = resolvePublishRecoveryInputs({ flags: {}, plan });
    expect(resolved.prerelease).toBe(true);
    expect(resolved.updateSparkle).toBe(false);
    expect(resolved.windowsSigned).toBe(true);
    expect(resolved.expectedPlatforms).toEqual(plan.expectedPlatforms);
    expect(resolved.conflicts).toEqual([]);
  });

  test('publishes the recorded switches and reports every re-typed flag that disagrees', () => {
    const plan = prereleasePlan();
    const resolved = resolvePublishRecoveryInputs({
      /* Exactly the "forgot --prerelease, forgot --skip-sparkle" recovery. */
      flags: { prerelease: false, updateSparkle: true, windowsSigned: false },
      plan,
    });
    expect(resolved.prerelease).toBe(true);
    expect(resolved.updateSparkle).toBe(false);
    expect(resolved.windowsSigned).toBe(true);
    expect(resolved.conflicts).toHaveLength(3);
    expect(resolved.conflicts.join(' | ')).toMatch(/--prerelease: the command line says false/u);
    expect(resolved.conflicts.join(' | ')).toMatch(/Sparkle \(--skip-sparkle\)/u);
    expect(resolved.conflicts.join(' | ')).toMatch(/--windows-signing/u);
  });

  test('never advances Sparkle for a release the plan did not ship macOS in', () => {
    const plan = planFor({ scope: defaultScope({ macos: false, updateSparkle: true }) });
    expect(plan.products['macos-arm64'].action).toBe('skip');
    const resolved = resolvePublishRecoveryInputs({ flags: { updateSparkle: true }, plan });
    expect(resolved.updateSparkle).toBe(false);
    expect(resolved.macosAction).toBe('skip');
  });

  test('refuses a recorded plan that carries no resolved scope', () => {
    const plan = planFor();
    delete plan.scope;
    expect(() => resolvePublishRecoveryInputs({ flags: {}, plan })).toThrow(/carries no resolved scope/u);
  });

  test('the same-version recovery still keeps Sparkle and Homebrew for a reused macOS product', () => {
    const { plan } = reusingPlan();
    const recovery = resolvePublishRecoveryInputs({ flags: {}, plan });
    expect(recovery.updateSparkle).toBe(true);
    expect(resolveMacosFeedScope({ plan, updateSparkleRequested: recovery.updateSparkle })).toMatchObject({
      homebrew: true,
      sparkle: true,
    });
  });
});
