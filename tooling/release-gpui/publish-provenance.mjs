/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
 * The publisher side of the change-aware release: plan <-> manifest <->
 * provenance cross-checks, the `release-provenance-<version>.json` asset, the
 * internal build-provenance operator summary, and the scope of the update feeds.
 *
 * `assemble.mjs` is a linear script with real side effects (it tags, uploads,
 * and pushes), so every decision it has to *refuse* on lives here as a pure
 * function instead. That keeps the refusal matrix unit-testable against fixture
 * artifact sets, and keeps the publisher itself a thin sequence of verified
 * steps.
 *
 * Three rules drive this module:
 *
 * 1. The plan is authoritative. A product may only appear in the release with
 *    the action the plan resolved for it, the fingerprint the plan computed, and
 *    the artifacts its own manifest declares. Anything else is a mixed or
 *    ambiguous artifact set and is refused, never published.
 * 2. Reuse is byte-identical re-publication. Every in-scope product ships real
 *    bytes on `v<version>`; nothing is omitted and nothing points at another tag.
 * 3. Feed scope is keyed on "macOS is in this release", not on "macOS was
 *    rebuilt". A same-version recovery legitimately reuses a macOS DMG from a run
 *    that never published, so no appcast entry and no cask update exist yet and
 *    both must still advance.
 */

import path from 'node:path';

import { PRODUCT_IDS, productDefinition } from './product-inputs.mjs';
import {
  buildReleaseProvenance,
  releaseProvenanceAssetName,
  validateProductProvenance,
  validateReleaseProvenance,
} from './provenance.mjs';
import { validatePlan } from './plan.mjs';

export const RELEASE_PLAN_ARTIFACT_DIRECTORY = 'release-plan';
export const RELEASE_PLAN_ARTIFACT_FILE = 'release-plan.json';
export const PRODUCT_PROVENANCE_FILE = 'provenance.json';

/*
 * Artifact directories the publisher downloads that legitimately carry no
 * `manifest.json`. They are inputs to planning and observability, never release
 * products, so the publisher tolerates them instead of treating them as an
 * unexpected platform.
 */
export function isNonProductArtifactDirectory(name) {
  return (
    name === RELEASE_PLAN_ARTIFACT_DIRECTORY ||
    name.startsWith('release-provenance-') ||
    name.startsWith('release-code-server-')
  );
}

/*
 * The APK embeds `versionName`/`versionCode` even though its payload is not
 * version-stamped for reuse purposes (§4.5), so a reused APK keeps the version
 * of the release in which mobile last changed and the release page must say so.
 * Every other non-version-stamped payload carries no marketing version at all.
 */
const PRODUCTS_WITH_EMBEDDED_VERSION = new Set(['android']);

const PLAN_ACTION_TO_RECORD_ACTION = Object.freeze({ build: 'built', reuse: 'reused' });

function refuse(message) {
  throw new Error(`Refusing to publish: ${message}`);
}

function refuseProduct(product, message) {
  throw new Error(`Refusing to publish ${product}: ${message}`);
}

export function planActionToRecordAction(action) {
  const mapped = PLAN_ACTION_TO_RECORD_ACTION[action];
  if (!mapped) refuse(`unknown plan action ${action}`);
  return mapped;
}

/*
 * The plan reaches the publisher two ways: inline through the workflow input
 * (`GHOSTEX_RELEASE_PLAN`) and as the `release-plan` artifact of the run being
 * published. Both are produced by the same `prepare` job, so when both are
 * present they must agree exactly; a disagreement means someone hand-dispatched
 * a plan that does not describe these artifacts.
 */
export function readPublishPlan({ artifactsRoot, env = process.env, fileExists, readTextFile }) {
  const inline = (env.GHOSTEX_RELEASE_PLAN ?? '').trim();
  const uploadedPath = path.join(artifactsRoot, RELEASE_PLAN_ARTIFACT_DIRECTORY, RELEASE_PLAN_ARTIFACT_FILE);
  const uploadedText = fileExists(uploadedPath) ? readTextFile(uploadedPath).trim() : '';
  if (!inline && !uploadedText) {
    refuse(
      'no resolved release plan was supplied. Pass GHOSTEX_RELEASE_PLAN, or publish a run whose ' +
        `prepare job uploaded ${RELEASE_PLAN_ARTIFACT_DIRECTORY}/${RELEASE_PLAN_ARTIFACT_FILE}`
    );
  }
  const plan = validatePlan(JSON.parse(inline || uploadedText));
  if (inline && uploadedText) assertPlansAgree(plan, validatePlan(JSON.parse(uploadedText)));
  return plan;
}

export function assertPlansAgree(dispatched, uploaded) {
  for (const field of ['version', 'sourceSha', 'algorithmRevision']) {
    if (dispatched[field] !== uploaded[field]) {
      refuse(
        `the dispatched plan ${field} ${dispatched[field]} does not match the run's recorded plan ${uploaded[field]}`
      );
    }
  }
  if (JSON.stringify(dispatched.expectedPlatforms) !== JSON.stringify(uploaded.expectedPlatforms)) {
    refuse(
      `the dispatched plan expects ${dispatched.expectedPlatforms.join(', ')} but the run's recorded plan ` +
        `expects ${uploaded.expectedPlatforms.join(', ')}`
    );
  }
  for (const productId of PRODUCT_IDS) {
    const left = dispatched.products[productId];
    const right = uploaded.products[productId];
    if (left.action !== right.action || left.fingerprint !== right.fingerprint) {
      refuse(
        `the dispatched plan resolves ${productId} as ${left.action}/${left.fingerprint.slice(0, 12)} but the run's ` +
          `recorded plan resolves ${right.action}/${right.fingerprint.slice(0, 12)}`
      );
    }
  }
  return true;
}

/*
 * Scenario H — publish-only recovery.
 *
 * The source run built, signed, and gated a specific artifact set under a
 * specific set of release switches. Re-typing those switches on the recovery
 * command line is how a prerelease gets published as stable, or a Sparkle feed
 * gets advanced for a release the operator meant to hold: the flags are typed
 * again by hand, hours later, from memory. The recorded plan carries what the run
 * actually resolved, so it is authoritative here exactly as `expectedPlatforms`
 * already is, and any disagreement is reported loudly rather than obeyed.
 */
export function resolvePublishRecoveryInputs({ flags = {}, plan }) {
  const scope = plan?.scope;
  if (!scope || typeof scope !== 'object' || Array.isArray(scope)) {
    refuse("the source run's recorded plan carries no resolved scope; refusing publish-only recovery");
  }
  const macosAction = plan.products?.['macos-arm64']?.action ?? 'skip';
  const recorded = {
    prerelease: Boolean(scope.prerelease),
    /* Mirrors the parent workflow: Sparkle needs macOS to actually ship. */
    updateSparkle: Boolean(scope.updateSparkle) && macosAction !== 'skip',
    windowsSigned: Boolean(scope.signWindows),
  };
  const labels = {
    prerelease: '--prerelease',
    updateSparkle: 'Sparkle (--skip-sparkle)',
    windowsSigned: '--windows-signing',
  };
  const conflicts = [];
  for (const key of ['prerelease', 'updateSparkle', 'windowsSigned']) {
    if (flags[key] === undefined) continue;
    if (Boolean(flags[key]) !== recorded[key]) {
      conflicts.push(
        `${labels[key]}: the command line says ${Boolean(flags[key])} but the source run recorded ${recorded[key]}`
      );
    }
  }
  return { conflicts, expectedPlatforms: [...plan.expectedPlatforms], macosAction, ...recorded };
}

/*
 * `expected_platforms` is a workflow input; the plan is computed on the runner.
 * They must describe the same release or the atomicity gate in the parent
 * workflow was evaluated against a different set than the one being published.
 */
export function assertPlanMatchesScope({ expectedPlatforms, plan, version }) {
  if (plan.version !== version) {
    refuse(`the plan releases ${plan.version}, not ${version}`);
  }
  const requested = [...expectedPlatforms].sort();
  const planned = [...plan.expectedPlatforms].sort();
  if (JSON.stringify(requested) !== JSON.stringify(planned)) {
    refuse(
      `GHOSTEX_RELEASE_EXPECTED_PLATFORMS (${requested.join(', ') || 'empty'}) does not equal the plan's ` +
        `expected platforms (${planned.join(', ') || 'empty'})`
    );
  }
  return plan.expectedPlatforms;
}

/*
 * One product: the record must describe this plan entry, this manifest, and this
 * release. `validateProductProvenance` owns the record's internal invariants
 * (including "a version-stamped product may never be reused across versions" and
 * "a reused product carries all four verified checks"); this adds the
 * plan-relative ones.
 */
export function validateProductAgainstPlan({ manifest, plan, record, version }) {
  const product = manifest.platform;
  const entry = plan.products?.[product];
  if (!entry) refuseProduct(product, 'the release plan has no entry for it');
  if (entry.action === 'skip') {
    refuseProduct(product, 'the plan skipped it, but the run uploaded an artifact for it');
  }
  if (!record) refuseProduct(product, `its artifact carries no ${PRODUCT_PROVENANCE_FILE}`);
  let validated;
  try {
    validated = validateProductProvenance(record, {
      expect: {
        action: planActionToRecordAction(entry.action),
        algorithmRevision: plan.algorithmRevision,
        fingerprint: entry.fingerprint,
        manifestArtifacts: manifest.artifacts ?? [],
        product,
        releaseVersion: version,
      },
    });
  } catch (error) {
    refuseProduct(product, error instanceof Error ? error.message : String(error));
  }
  if (validated.sourceSha !== plan.sourceSha) {
    refuseProduct(
      product,
      `it was produced at ${validated.sourceSha.slice(0, 12)} but the plan was computed at ${plan.sourceSha.slice(0, 12)}`
    );
  }
  if (validated.action === 'reused') {
    const reuse = entry.reuse ?? {};
    const recordOrigin = validated.reusedFrom.tag ?? `run ${validated.reusedFrom.runId}`;
    const planOrigin = reuse.tag ?? (reuse.runId ? `run ${reuse.runId}` : '(none)');
    if (recordOrigin !== planOrigin) {
      refuseProduct(product, `it was reused from ${recordOrigin} but the plan authorized ${planOrigin}`);
    }
    if (reuse.productVersion && reuse.productVersion !== validated.productVersion) {
      refuseProduct(
        product,
        `it reports product version ${validated.productVersion} but the plan authorized ${reuse.productVersion}`
      );
    }
  }
  return validated;
}

/*
 * The whole set. `readProvenance(directory)` returns the parsed record or null;
 * injecting it keeps this function pure for the fixture tests.
 */
export function collectPublishProvenance({ manifests, plan, readProvenance, version }) {
  const records = {};
  for (const manifest of manifests) {
    const record = readProvenance(manifest.directory);
    records[manifest.platform] = validateProductAgainstPlan({ manifest, plan, record, version });
  }
  for (const product of plan.expectedPlatforms) {
    if (!records[product]) refuseProduct(product, 'the plan expects it but no validated provenance record arrived');
  }
  return records;
}

/*
 * A mixed artifact set is not only "a missing platform": two products built by
 * two different Actions runs are equally ambiguous, because only one of them was
 * gated by the atomicity condition that let the publisher start. Reused products
 * legitimately name a foreign origin run, so only freshly built bytes are
 * compared.
 */
export function assertSingleBuildOrigin({ expectedRunId = null, records }) {
  const origins = new Map();
  for (const record of Object.values(records)) {
    if (record.action !== 'built') continue;
    if (!origins.has(record.originRunId)) origins.set(record.originRunId, []);
    origins.get(record.originRunId).push(record.product);
  }
  if (origins.size > 1) {
    const description = [...origins.entries()]
      .map(([runId, products]) => `${runId}: ${products.sort().join(', ')}`)
      .join(' | ');
    refuse(`the built artifacts come from more than one Actions run (${description})`);
  }
  const [originRunId] = [...origins.keys()];
  if (expectedRunId && origins.size === 1 && String(originRunId) !== String(expectedRunId)) {
    refuse(`the built artifacts come from run ${originRunId}, not the nominated source run ${expectedRunId}`);
  }
  return origins.size === 1 ? originRunId : null;
}

/*
 * Sparkle and Homebrew are keyed on "macOS ships in this release", not on "macOS
 * was rebuilt". macOS is version-stamped, so it can only ever be reused inside
 * the same version — that is the recovery case where the DMG exists, the appcast
 * entry does not, and both feeds still have to advance.
 */
export function resolveMacosFeedScope({ plan, updateSparkleRequested }) {
  const action = plan.products?.['macos-arm64']?.action ?? 'skip';
  const inRelease = action !== 'skip';
  if (updateSparkleRequested && !inRelease) {
    refuse('Sparkle was requested but macOS is not part of this release');
  }
  const reason = !inRelease
    ? 'macOS is not part of this release'
    : action === 'reuse'
      ? 'macOS is reused into this same-version release, so its feed entries do not exist yet'
      : 'macOS was built for this release';
  return {
    homebrew: inRelease,
    macosAction: action,
    reason,
    sparkle: inRelease && updateSparkleRequested,
  };
}

/* Velopack feeds are produced by `vpk pack` inside the Windows job, so this is
 * descriptive: it exists so the report and the verifier can state which channels
 * this release regenerated versus carried forward from a same-version reuse. */
export function resolveWindowsFeedScope({ plan }) {
  const regenerated = [];
  const carriedForward = [];
  for (const arch of ['x64', 'arm64']) {
    const action = plan.products?.[`windows-${arch}`]?.action ?? 'skip';
    if (action === 'build') regenerated.push(arch);
    else if (action === 'reuse') carriedForward.push(arch);
  }
  return { carriedForward, regenerated };
}

export function releaseProvenanceComponents(plan) {
  const components = {};
  for (const [component, entry] of Object.entries(plan.components ?? {})) {
    components[component] = {
      action: entry.action,
      componentVersion: entry.componentVersion ?? null,
      downloadTag: entry.downloadTag ?? null,
      identityRevisionInputsDigest: entry.identityRevisionInputsDigest ?? null,
      /* The tag state observed while planning; the live tag is re-checked by the verifier. */
      platforms: entry.publishedPlatforms ?? {},
      requiredPlatforms: entry.requiredPlatforms ?? [],
    };
  }
  return components;
}

/*
 * The embedded plan is the human-readable "why" of the release. Each product's
 * per-pathspec input digests are dropped from it because the authoritative copy
 * of those digests already travels in that product's own provenance record — the
 * one the next planner actually reads. Keeping both roughly doubles an asset the
 * planner downloads for every baseline it inspects.
 */
export function compactPlanForRecord(plan) {
  const products = {};
  for (const [productId, entry] of Object.entries(plan.products ?? {})) {
    const { inputs, ...rest } = entry;
    products[productId] = rest;
  }
  return { ...plan, products };
}

export function buildReleaseProvenanceRecord({
  plan,
  productRecords,
  publishedAt = new Date().toISOString(),
  sourceSha,
  version,
  workflowRunId,
}) {
  return buildReleaseProvenance({
    algorithmRevision: plan.algorithmRevision,
    components: releaseProvenanceComponents(plan),
    plan: compactPlanForRecord(plan),
    products: productRecords,
    publishedAt,
    sourceSha,
    version,
    workflowRunId,
  });
}

/*
 * Idempotent re-publication. The already-published release carries a provenance
 * record written by an earlier run, whose `publishedAt` and `workflowRunId`
 * legitimately differ from the one this run just computed. What must not differ
 * is what the record actually claims: the same products, the same actions, the
 * same fingerprints, the same artifact digests. Anything else means the live
 * release is not the release this run validated.
 */
export function assertLiveProvenanceMatches({ live, record }) {
  const published = validateReleaseProvenance(live);
  if (published.version !== record.version || published.tag !== record.tag) {
    refuse(`the published ${published.tag} provenance does not describe ${record.tag}`);
  }
  const publishedProducts = Object.keys(published.products).sort();
  const computedProducts = Object.keys(record.products).sort();
  if (JSON.stringify(publishedProducts) !== JSON.stringify(computedProducts)) {
    refuse(
      `the published provenance records ${publishedProducts.join(', ')} but this run validated ` +
        `${computedProducts.join(', ')}`
    );
  }
  for (const [product, computed] of Object.entries(record.products)) {
    const live = published.products[product];
    if (live.action !== computed.action || live.fingerprint !== computed.fingerprint) {
      refuseProduct(
        product,
        `the published provenance records ${live.action}/${live.fingerprint.slice(0, 12)} but this run validated ` +
          `${computed.action}/${computed.fingerprint.slice(0, 12)}`
      );
    }
    const digests = (entries) =>
      [...entries]
        .map((artifact) => `${artifact.name}\0${artifact.sha256}\0${artifact.size}`)
        .sort()
        .join('|');
    if (digests(live.artifacts) !== digests(computed.artifacts)) {
      refuseProduct(product, 'the published provenance records different artifact digests than this run validated');
    }
  }
  return published;
}

export function productOriginLabel(record) {
  if (record.action === 'built') return 'this release';
  if (record.reusedFrom?.tier === 'release') return `unchanged since ${record.reusedFrom.tag}`;
  return `unchanged since run ${record.reusedFrom?.runId} (same version)`;
}

export function productVersionCell(record) {
  if (record.versionStamped || PRODUCTS_WITH_EMBEDDED_VERSION.has(record.product)) {
    return record.productVersion;
  }
  return '—';
}

/* §Q12: the release page states, per product, built versus reused and from where. */
export function renderBuildProvenanceNotes(releaseProvenance) {
  const lines = ['## Build provenance', ''];
  lines.push('| Product | Status | Product version | Source |');
  lines.push('|---|---|---|---|');
  for (const product of PRODUCT_IDS) {
    const record = releaseProvenance.products[product];
    if (!record) continue;
    lines.push(`| ${product} | ${record.action} | ${productVersionCell(record)} | ${productOriginLabel(record)} |`);
  }
  const componentLines = [];
  for (const [component, entry] of Object.entries(releaseProvenance.components ?? {})) {
    if (!entry.componentVersion) continue;
    componentLines.push(`\`${component}\` ${entry.componentVersion} (${entry.action})`);
  }
  lines.push('');
  if (componentLines.length > 0) lines.push(`Components: ${componentLines.join(' · ')}`, '');
  lines.push('Reused artifacts are byte-identical to the release named above; their inputs did not change.');
  lines.push(`Full machine-readable record: \`${releaseProvenanceAssetName(releaseProvenance.version)}\`.`);
  return lines.join('\n');
}

/* The four-way status vocabulary (§11.3) used by the publisher and the verifier. */
export function summarizeReleaseProvenance(releaseProvenance, { plan = releaseProvenance.plan } = {}) {
  const summary = { built: [], reused: [], skippedAsUnchanged: [], skippedByFlag: [] };
  for (const product of PRODUCT_IDS) {
    const record = releaseProvenance.products[product];
    if (record) {
      const entry = {
        fingerprint: record.fingerprint,
        origin: productOriginLabel(record),
        product,
        productVersion: productVersionCell(record),
      };
      if (record.action === 'built') summary.built.push(entry);
      else summary.reused.push(entry);
      continue;
    }
    const planned = plan?.products?.[product];
    if (planned?.action === 'skip') {
      summary.skippedByFlag.push({ product, reason: planned.reason });
    }
  }
  return summary;
}

export function renderReleaseProvenanceReport(releaseProvenance, { plan } = {}) {
  const summary = summarizeReleaseProvenance(releaseProvenance, { plan });
  const lines = [];
  const describe = (entry) => `${entry.product} (${entry.fingerprint.slice(0, 12)}, ${entry.origin})`;
  lines.push(`BUILT     ${summary.built.map(describe).join(' · ') || '(none)'}`);
  lines.push(`REUSED    ${summary.reused.map(describe).join(' · ') || '(none)'}`);
  lines.push(
    `SKIPPED   by flag: ${summary.skippedByFlag.map((entry) => entry.product).join(', ') || '(none)'}; ` +
      `as unchanged: ${summary.skippedAsUnchanged.map((entry) => entry.product).join(', ') || '(none)'}`
  );
  return lines.join('\n');
}

/*
 * The verifier's public-data view of a reuse claim. Everything here is derived
 * from GitHub metadata, never from the record being checked: the live release's
 * asset digests, and the origin release's asset digests for a cross-release
 * reuse. A version-stamped product can only be reused inside its own version, so
 * its origin is an Actions run and there is no earlier release to compare with.
 */
export function verifyReleaseProvenanceAgainstAssets({ liveAssets, releaseProvenance, version }) {
  const failures = [];
  const byName = new Map(liveAssets.map((asset) => [asset.name, asset]));
  const recorded = new Set([releaseProvenanceAssetName(version)]);
  for (const record of Object.values(releaseProvenance.products)) {
    for (const artifact of record.artifacts) {
      recorded.add(artifact.name);
      const asset = byName.get(artifact.name);
      if (!asset) {
        failures.push(`${artifact.name} is recorded for ${record.product} but missing from the release`);
        continue;
      }
      if (asset.sha256 && asset.sha256 !== artifact.sha256) {
        failures.push(`${artifact.name} live digest ${asset.sha256.slice(0, 12)} does not match the provenance record`);
      }
      if (asset.size !== undefined && asset.size !== null && Number(asset.size) !== artifact.size) {
        failures.push(`${artifact.name} live size ${asset.size} does not match the provenance record`);
      }
    }
  }
  for (const asset of liveAssets) {
    if (!recorded.has(asset.name)) failures.push(`${asset.name} is published but recorded in no provenance record`);
  }
  return failures;
}

export function crossReleaseReuseOrigins(releaseProvenance) {
  const origins = [];
  for (const record of Object.values(releaseProvenance.products)) {
    if (record.action !== 'reused' || record.reusedFrom?.tier !== 'release') continue;
    origins.push({
      artifacts: record.artifacts.map((artifact) => ({ name: artifact.name, sha256: artifact.sha256 })),
      product: record.product,
      tag: record.reusedFrom.tag,
      versionStamped: Boolean(productDefinition(record.product).versionStamped),
    });
  }
  return origins;
}
