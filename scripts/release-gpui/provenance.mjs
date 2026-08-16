/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
 * Provenance records for release products, and the four independent checks that
 * must all pass before any bytes are reused.
 *
 * The provenance record is only an index. Nothing in it is trusted on its own:
 * the fingerprint is recomputed from the current checkout, the digests are
 * re-derived from the bytes and from GitHub's own asset metadata, the origin is
 * re-checked against repository, workflow, and commit ancestry, and the
 * attestation is verified cryptographically. A record that fails any check
 * downgrades the product to `build`; it never weakens a check.
 */

import { PRODUCTS, TRUSTED_REPO, productDefinition } from "./product-inputs.mjs";
import { FINGERPRINT_ALGORITHM_REVISION } from "./fingerprint.mjs";

export const PROVENANCE_SCHEMA_VERSION = 1;
export const RELEASE_PROVENANCE_SCHEMA_VERSION = 1;
export const REUSE_CHECKS = Object.freeze(["fingerprint", "digest", "origin", "attestation"]);
export const RELEASE_WORKFLOW_NAME = "Release Ghostex";
export const AMEND_EXISTING_WORKFLOW_NAME = "Amend existing Ghostex release";
export const ALLOWED_RELEASE_WORKFLOW_NAMES = Object.freeze([
  RELEASE_WORKFLOW_NAME,
  AMEND_EXISTING_WORKFLOW_NAME,
]);

export function isAllowedReleaseWorkflowName(name) {
  return ALLOWED_RELEASE_WORKFLOW_NAMES.includes(name);
}

const sha256Pattern = /^[0-9a-f]{64}$/u;
const commitPattern = /^[0-9a-f]{40}$/u;
const versionPattern = /^\d+\.\d+\.\d+$/u;
const releaseTagPattern = /^v\d+\.\d+\.\d+$/u;
const signingModes = new Set(["developer-id+notarized", "authenticode", "unsigned", "android-keystore"]);

function fail(message) {
  throw new Error(`Invalid release provenance: ${message}`);
}

function requireString(value, label, pattern) {
  if (typeof value !== "string" || value.length === 0) fail(`${label} must be a non-empty string`);
  if (pattern && !pattern.test(value)) fail(`${label} must match ${pattern}`);
  return value;
}

function requireInteger(value, label) {
  if (!Number.isSafeInteger(value) || value < 0) fail(`${label} must be a non-negative integer`);
  return value;
}

function requireBoolean(value, label) {
  if (typeof value !== "boolean") fail(`${label} must be a boolean`);
  return value;
}

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail(`${label} must be an object`);
  return value;
}

export function releaseProvenanceAssetName(version) {
  requireString(version, "version", versionPattern);
  return `release-provenance-${version}.json`;
}

export function normalizedAssetDigest(digest) {
  if (typeof digest !== "string") return "";
  return digest.startsWith("sha256:") ? digest.slice("sha256:".length) : digest;
}

export function buildProductProvenance({
  action,
  algorithmRevision = FINGERPRINT_ALGORITHM_REVISION,
  artifacts,
  fingerprint,
  inputs,
  originRunId,
  originSourceSha,
  originTag,
  product,
  productVersion,
  releaseVersion,
  reusedFrom = null,
  signing,
  sourceSha,
}) {
  const definition = productDefinition(product);
  const record = {
    action,
    algorithmRevision,
    artifacts: artifacts.map((artifact) => ({
      name: artifact.name,
      sha256: artifact.sha256,
      size: artifact.size,
    })),
    fingerprint,
    inputs,
    originRunId,
    originSourceSha,
    originTag,
    platform: { ...definition.platform },
    product,
    productVersion,
    releaseVersion,
    reusedFrom,
    schemaVersion: PROVENANCE_SCHEMA_VERSION,
    sourceSha,
    versionStamped: Boolean(definition.versionStamped),
  };
  const mode = signing ?? (typeof definition.signing?.mode === "string" ? definition.signing.mode : null);
  if (mode) record.signing = { mode };
  return validateProductProvenance(record);
}

export function validateProductProvenance(input, { expect = {} } = {}) {
  const record = requireObject(input, "product record");
  if (record.schemaVersion !== PROVENANCE_SCHEMA_VERSION) fail("schemaVersion must equal 1");
  requireString(record.algorithmRevision, "algorithmRevision");
  const product = requireString(record.product, "product");
  if (!PRODUCTS[product]) fail(`product ${product} is not a known release product`);
  const definition = productDefinition(product);
  requireString(record.fingerprint, "fingerprint", sha256Pattern);
  if (record.action !== "built" && record.action !== "reused") fail('action must be "built" or "reused"');
  requireBoolean(record.versionStamped, "versionStamped");
  if (record.versionStamped !== Boolean(definition.versionStamped)) {
    fail(`versionStamped must equal the declared value for ${product}`);
  }
  requireString(record.releaseVersion, "releaseVersion", versionPattern);
  requireString(record.productVersion, "productVersion", versionPattern);
  requireString(record.sourceSha, "sourceSha", commitPattern);
  requireString(record.originTag, "originTag", releaseTagPattern);
  requireInteger(record.originRunId, "originRunId");
  requireString(record.originSourceSha, "originSourceSha", commitPattern);

  const platform = requireObject(record.platform, "platform");
  requireString(platform.os, "platform.os");
  requireString(platform.arch, "platform.arch");
  requireString(platform.runnerLabel, "platform.runnerLabel");
  if (platform.os !== definition.platform.os || platform.arch !== definition.platform.arch) {
    fail(`platform must equal ${definition.platform.os}/${definition.platform.arch} for ${product}`);
  }

  if (record.signing !== undefined) {
    const signing = requireObject(record.signing, "signing");
    if (!signingModes.has(signing.mode)) fail(`signing.mode ${signing.mode} is not a known signing mode`);
  }

  const inputs = requireObject(record.inputs, "inputs");
  if (!Array.isArray(inputs.paths)) fail("inputs.paths must be an array");
  for (const entry of inputs.paths) {
    const pathEntry = requireObject(entry, "inputs.paths entry");
    requireString(pathEntry.pathspec, "inputs.paths[].pathspec");
    requireString(pathEntry.digest, "inputs.paths[].digest", sha256Pattern);
    requireInteger(pathEntry.entryCount, "inputs.paths[].entryCount");
  }
  requireObject(inputs.values, "inputs.values");
  requireObject(inputs.composed, "inputs.composed");

  if (!Array.isArray(record.artifacts) || record.artifacts.length === 0) {
    fail("artifacts must be a non-empty array");
  }
  for (const artifact of record.artifacts) {
    const entry = requireObject(artifact, "artifacts entry");
    requireString(entry.name, "artifacts[].name");
    if (entry.name.includes("/") || entry.name.includes("\\") || entry.name.includes("..")) {
      fail(`artifacts[].name must be a plain file name, got ${entry.name}`);
    }
    requireString(entry.sha256, "artifacts[].sha256", sha256Pattern);
    requireInteger(entry.size, "artifacts[].size");
  }

  if (record.action === "built") {
    if (record.productVersion !== record.releaseVersion) fail("a built product version must equal the release version");
    if (record.originTag !== `v${record.releaseVersion}`) fail("a built product must originate from this release tag");
    if (record.reusedFrom !== null) fail("a built product must not carry reusedFrom");
  } else {
    const reusedFrom = requireObject(record.reusedFrom, "reusedFrom");
    if (reusedFrom.tier !== "release" && reusedFrom.tier !== "run") fail('reusedFrom.tier must be "release" or "run"');
    if (reusedFrom.tier === "release") requireString(reusedFrom.tag, "reusedFrom.tag", releaseTagPattern);
    if (reusedFrom.tier === "run") requireInteger(reusedFrom.runId, "reusedFrom.runId");
    if (!Array.isArray(reusedFrom.verifiedChecks)) fail("reusedFrom.verifiedChecks must be an array");
    for (const check of REUSE_CHECKS) {
      if (!reusedFrom.verifiedChecks.includes(check)) fail(`reusedFrom.verifiedChecks is missing ${check}`);
    }
    if (record.versionStamped && record.productVersion !== record.releaseVersion) {
      fail(`${product} is version-stamped and may never be reused across releases`);
    }
  }

  if (expect.product !== undefined && expect.product !== product) {
    fail(`product must equal ${expect.product}`);
  }
  if (expect.action !== undefined && expect.action !== record.action) {
    fail(`action must equal ${expect.action} for ${product}`);
  }
  if (expect.fingerprint !== undefined && expect.fingerprint !== record.fingerprint) {
    fail(`fingerprint must equal ${expect.fingerprint} for ${product}`);
  }
  if (expect.releaseVersion !== undefined && expect.releaseVersion !== record.releaseVersion) {
    fail(`releaseVersion must equal ${expect.releaseVersion} for ${product}`);
  }
  if (expect.algorithmRevision !== undefined && expect.algorithmRevision !== record.algorithmRevision) {
    fail(`algorithmRevision must equal ${expect.algorithmRevision} for ${product}`);
  }
  if (expect.manifestArtifacts !== undefined) {
    const asKey = (list) =>
      [...list]
        .map((artifact) => `${artifact.name}\0${artifact.sha256}\0${artifact.size}`)
        .sort()
        .join("|");
    if (asKey(expect.manifestArtifacts) !== asKey(record.artifacts)) {
      fail(`artifacts must equal the manifest artifacts for ${product}`);
    }
  }
  return record;
}

export function buildReleaseProvenance({
  algorithmRevision = FINGERPRINT_ALGORITHM_REVISION,
  components = {},
  plan,
  products,
  publishedAt,
  sourceSha,
  version,
  workflowRunId,
}) {
  const record = {
    algorithmRevision,
    components,
    plan,
    products,
    publishedAt,
    schemaVersion: RELEASE_PROVENANCE_SCHEMA_VERSION,
    sourceSha,
    tag: `v${version}`,
    version,
    workflowRunId,
  };
  return validateReleaseProvenance(record);
}

export function validateReleaseProvenance(input) {
  const record = requireObject(input, "release provenance");
  if (record.schemaVersion !== RELEASE_PROVENANCE_SCHEMA_VERSION) fail("schemaVersion must equal 1");
  requireString(record.algorithmRevision, "algorithmRevision");
  const version = requireString(record.version, "version", versionPattern);
  if (record.tag !== `v${version}`) fail(`tag must equal v${version}`);
  requireString(record.sourceSha, "sourceSha", commitPattern);
  requireInteger(record.workflowRunId, "workflowRunId");
  requireString(record.publishedAt, "publishedAt");
  const products = requireObject(record.products, "products");
  for (const [product, productRecord] of Object.entries(products)) {
    validateProductProvenance(productRecord, { expect: { product, releaseVersion: version } });
  }
  requireObject(record.components, "components");
  return record;
}

/*
 * Build the reuse candidate index.
 *
 * Tier 1 (durable, cross-release): `release-provenance-<version>.json` assets of
 * recent non-draft releases. Tier 2 (ephemeral, same-version recovery): the
 * product provenance records of a nominated source run. Releases published
 * before this feature carry no provenance and are simply not candidates.
 */
export function buildReuseIndex({ baselines = [], sourceRun = null } = {}) {
  const index = new Map();
  const push = (product, candidate) => {
    if (!PRODUCTS[product]) return;
    if (!index.has(product)) index.set(product, []);
    index.get(product).push(candidate);
  };

  const ordered = [...baselines].sort((left, right) => {
    const leftTime = Date.parse(left.publishedAt ?? "") || 0;
    const rightTime = Date.parse(right.publishedAt ?? "") || 0;
    return rightTime - leftTime;
  });
  for (const baseline of ordered) {
    const provenance = baseline.provenance;
    if (!provenance || typeof provenance !== "object") continue;
    for (const [product, record] of Object.entries(provenance.products ?? {})) {
      push(product, {
        assets: baseline.assets ?? [],
        commit: baseline.commit ?? provenance.sourceSha ?? null,
        draft: Boolean(baseline.draft),
        publishedAt: baseline.publishedAt ?? null,
        record,
        repo: baseline.repo ?? TRUSTED_REPO,
        runId: record?.originRunId ?? null,
        tag: baseline.tag ?? provenance.tag ?? null,
        tier: "release",
      });
    }
  }

  if (sourceRun) {
    const expired = new Set(sourceRun.expiredArtifacts ?? []);
    const available = Array.isArray(sourceRun.availableArtifacts) ? new Set(sourceRun.availableArtifacts) : null;
    for (const [product, record] of Object.entries(sourceRun.products ?? {})) {
      push(product, {
        artifactExpired: expired.has(`release-${product}`),
        /*
         * A job can upload its provenance record and then die before uploading
         * the package artifact itself. When the caller supplies the run's live
         * artifact listing, require the package artifact to actually exist;
         * without a listing (the materializer's path, where the download itself
         * is the existence proof) the check is left to the download.
         */
        artifactMissing: available ? !available.has(`release-${product}`) : false,
        assets: [],
        commit: sourceRun.headSha ?? null,
        conclusion: sourceRun.conclusion ?? null,
        draft: false,
        event: sourceRun.event ?? null,
        record,
        repo: sourceRun.repo ?? TRUSTED_REPO,
        runId: sourceRun.runId ?? null,
        tag: null,
        tier: "run",
        workflowName: sourceRun.workflowName ?? null,
      });
    }
  }
  return index;
}

function checkOrigin(candidate, evidence) {
  const failures = [];
  if (candidate.repo !== TRUSTED_REPO) {
    failures.push(`origin repository ${candidate.repo} is not ${TRUSTED_REPO}`);
  }
  if (candidate.tier === "release") {
    if (candidate.draft) failures.push("origin release is a draft");
    if (!candidate.tag || !releaseTagPattern.test(candidate.tag)) {
      failures.push(`origin tag ${candidate.tag ?? "(missing)"} is not a published release tag`);
    }
  } else if (candidate.tier === "run") {
    if (!isAllowedReleaseWorkflowName(candidate.workflowName)) {
      failures.push(
        `origin run workflow ${candidate.workflowName ?? "(unknown)"} is not a Ghostex release workflow`,
      );
    }
    if (candidate.event !== "workflow_dispatch") {
      failures.push(`origin run event ${candidate.event ?? "(unknown)"} is not workflow_dispatch`);
    }
    /*
     * The run-level conclusion only proves the run *finished*; it is not
     * required to be "success". Trust in a run-tier candidate is entirely
     * product-scoped: the product's own provenance artifact exists, its package
     * artifact exists unexpired, the digests match the downloaded bytes, the
     * attestation verifies, and the commit is an ancestor. Requiring a green
     * run made the 7.8.0 release rebuild every already-successful product on
     * each retry because *other* jobs in the same run had failed.
     */
    const conclusion = candidate.conclusion ?? null;
    if (!["success", "failure", "cancelled"].includes(conclusion)) {
      failures.push(`origin run conclusion ${conclusion ?? "(unknown)"} is not a completed release run`);
    }
    if (candidate.artifactExpired) failures.push("origin run artifacts have expired");
    if (candidate.artifactMissing) {
      failures.push("origin run never uploaded this product's package artifact");
    }
  } else {
    failures.push(`unknown reuse tier ${candidate.tier}`);
  }
  const originCommit = candidate.commit ?? candidate.record?.originSourceSha ?? null;
  if (!originCommit || !commitPattern.test(originCommit)) {
    failures.push("origin commit is missing");
  } else if (typeof evidence.isAncestor !== "function") {
    failures.push("no ancestry oracle was provided");
  } else if (!evidence.isAncestor(originCommit)) {
    failures.push(`origin commit ${originCommit.slice(0, 12)} is not an ancestor of the source commit`);
  }
  return failures;
}

/*
 * Byte equality must hold against BOTH the downloaded bytes and GitHub's own
 * asset metadata, for every artifact. Anything short of that leaves the check
 * pending, so a partially-observed set can never be reported as verified.
 */
function checkDigest(record, evidence, { requireBothSources }) {
  const failures = [];
  let unobserved = 0;
  for (const artifact of record.artifacts) {
    const metadata = evidence.assetMetadata?.(artifact.name, record) ?? null;
    if (metadata) {
      if (normalizedAssetDigest(metadata.digest) !== artifact.sha256) {
        failures.push(`${artifact.name} published digest does not match the provenance record`);
      }
      if (metadata.size !== undefined && Number(metadata.size) !== artifact.size) {
        failures.push(`${artifact.name} published size does not match the provenance record`);
      }
    }
    const local = evidence.localArtifact?.(artifact.name, record) ?? null;
    if (local) {
      if (local.sha256 !== artifact.sha256) failures.push(`${artifact.name} bytes do not match the provenance record`);
      if (local.size !== undefined && Number(local.size) !== artifact.size) {
        failures.push(`${artifact.name} byte length does not match the provenance record`);
      }
    }
    const observed = requireBothSources ? Boolean(metadata && local) : Boolean(metadata || local);
    if (!observed) unobserved += 1;
  }
  return { failures, pending: unobserved > 0 && failures.length === 0 };
}

function checkAttestation(record, evidence) {
  if (typeof evidence.attestationVerified !== "function") return { failures: [], pending: true };
  const failures = [];
  for (const artifact of record.artifacts) {
    const verified = evidence.attestationVerified(artifact.name, record);
    if (verified === undefined || verified === null) return { failures, pending: true };
    if (!verified) failures.push(`${artifact.name} has no verifiable build attestation`);
  }
  return { failures, pending: false };
}

/*
 * All four checks (§Q3) plus record/scope compatibility. `requireAll` is used at
 * materialization time, where the bytes and their attestations are in hand; the
 * planner calls it with `requireAll: false` and treats still-pending checks as
 * work the reuse job must complete before the artifact is accepted.
 */
export function verifyReuseCandidate({
  algorithmRevision = FINGERPRINT_ALGORITHM_REVISION,
  candidate,
  evidence = {},
  fingerprint,
  productId,
  releaseVersion,
  requireAll = false,
}) {
  const definition = productDefinition(productId);
  const failures = [];
  const verifiedChecks = [];
  const pendingChecks = [];

  let record = candidate?.record;
  try {
    record = validateProductProvenance(record, { expect: { product: productId } });
  } catch (error) {
    return {
      failures: [error instanceof Error ? error.message : String(error)],
      ok: false,
      pendingChecks: [...REUSE_CHECKS],
      verifiedChecks: [],
    };
  }

  if (record.algorithmRevision !== algorithmRevision) {
    failures.push(`provenance algorithm revision ${record.algorithmRevision} != ${algorithmRevision}`);
  }
  if (record.versionStamped) {
    if (record.productVersion !== releaseVersion || record.releaseVersion !== releaseVersion) {
      failures.push(`${productId} is version-stamped and cannot be reused from ${record.releaseVersion}`);
    }
    if (candidate.tier === "release" && candidate.tag !== `v${releaseVersion}`) {
      failures.push(`${productId} is version-stamped and cannot be reused across releases`);
    }
  }
  if (definition.sideFiles?.length && candidate.tier !== "run") {
    failures.push(`${productId} publishes side files and can only be reused from the same run`);
  }

  if (record.fingerprint === fingerprint) verifiedChecks.push("fingerprint");
  else failures.push(`fingerprint ${record.fingerprint.slice(0, 12)} != ${String(fingerprint).slice(0, 12)}`);

  const originFailures = checkOrigin(candidate, evidence);
  if (originFailures.length === 0) verifiedChecks.push("origin");
  else failures.push(...originFailures);

  const digestResult = checkDigest(record, evidence, { requireBothSources: requireAll });
  if (digestResult.failures.length > 0) failures.push(...digestResult.failures);
  else if (digestResult.pending) pendingChecks.push("digest");
  else verifiedChecks.push("digest");

  const attestationResult = checkAttestation(record, evidence);
  if (attestationResult.failures.length > 0) failures.push(...attestationResult.failures);
  else if (attestationResult.pending) pendingChecks.push("attestation");
  else verifiedChecks.push("attestation");

  const ok = failures.length === 0 && (!requireAll || pendingChecks.length === 0);
  return { failures, ok, pendingChecks, record, verifiedChecks };
}

/* The reuse block written into a plan entry once a candidate is accepted. */
export function reuseDescriptor({ candidate, verification }) {
  const record = candidate.record;
  return {
    artifacts: record.artifacts.map((artifact) => ({
      name: artifact.name,
      sha256: artifact.sha256,
      size: artifact.size,
    })),
    originSourceSha: record.originSourceSha,
    pendingChecks: [...verification.pendingChecks],
    productVersion: record.productVersion,
    runId: candidate.runId ?? record.originRunId,
    tag: candidate.tag,
    tier: candidate.tier,
    verifiedChecks: [...verification.verifiedChecks],
  };
}
