import { describe, expect, test } from "vitest";
import {
  REUSE_CHECKS,
  buildProductProvenance,
  buildReleaseProvenance,
  buildReuseIndex,
  releaseProvenanceAssetName,
  validateProductProvenance,
  validateReleaseProvenance,
  verifyReuseCandidate,
} from "./provenance.mjs";
import { FINGERPRINT_ALGORITHM_REVISION } from "./fingerprint.mjs";
import { TRUSTED_REPO } from "./product-inputs.mjs";

const digestA = "a".repeat(64);
const digestB = "b".repeat(64);
const commitOld = "1".repeat(40);
const commitNew = "2".repeat(40);

const inputs = {
  composed: {},
  paths: [{ digest: digestA, entryCount: 1, pathspec: "mobile" }],
  values: { bun: "1.4.0" },
};

function androidRecord(overrides = {}) {
  return buildProductProvenance({
    action: "built",
    artifacts: [{ name: "ghostex-android.apk", sha256: digestA, size: 91234567 }],
    fingerprint: digestB,
    inputs,
    originRunId: 31501234567,
    originSourceSha: commitOld,
    originTag: "v7.6.0",
    product: "android",
    productVersion: "7.6.0",
    releaseVersion: "7.6.0",
    sourceSha: commitOld,
    ...overrides,
  });
}

function macosRecord(overrides = {}) {
  return buildProductProvenance({
    action: "built",
    artifacts: [
      { name: "ghostex-7.8.0-arm64.dmg", sha256: digestA, size: 268435456 },
      { name: "bd-darwin-arm64.tar.gz", sha256: digestB, size: 4096 },
    ],
    fingerprint: digestA,
    inputs,
    originRunId: 31644067583,
    originSourceSha: commitOld,
    originTag: "v7.8.0",
    product: "macos-arm64",
    productVersion: "7.8.0",
    releaseVersion: "7.8.0",
    sourceSha: commitOld,
    ...overrides,
  });
}

function releaseCandidate(record, overrides = {}) {
  return {
    assets: [],
    commit: commitOld,
    draft: false,
    record,
    repo: TRUSTED_REPO,
    runId: record.originRunId,
    tag: record.originTag,
    tier: "release",
    ...overrides,
  };
}

function runCandidate(record, overrides = {}) {
  return {
    artifactExpired: false,
    conclusion: "success",
    commit: commitOld,
    draft: false,
    event: "workflow_dispatch",
    record,
    repo: TRUSTED_REPO,
    runId: record.originRunId,
    tag: null,
    tier: "run",
    workflowName: "Release Ghostex",
    ...overrides,
  };
}

const alwaysAncestor = { isAncestor: () => true };

describe("product provenance records", () => {
  test("round-trips a built product record", () => {
    const record = androidRecord();
    expect(validateProductProvenance(record)).toBe(record);
    expect(record.schemaVersion).toBe(1);
    expect(record.algorithmRevision).toBe(FINGERPRINT_ALGORITHM_REVISION);
    expect(record.versionStamped).toBe(false);
    expect(record.platform).toEqual({ arch: "arm64", os: "android", runnerLabel: "ubuntu-24.04" });
    expect(record.signing).toEqual({ mode: "android-keystore" });
    expect(record.reusedFrom).toBeNull();
  });

  test("rejects malformed digests, sizes, and artifact names", () => {
    expect(() => androidRecord({ fingerprint: "nope" })).toThrow(/fingerprint/u);
    expect(() =>
      androidRecord({ artifacts: [{ name: "ghostex-android.apk", sha256: "short", size: 1 }] }),
    ).toThrow(/sha256/u);
    expect(() =>
      androidRecord({ artifacts: [{ name: "ghostex-android.apk", sha256: digestA, size: -1 }] }),
    ).toThrow(/size/u);
    expect(() =>
      androidRecord({ artifacts: [{ name: "../escape.apk", sha256: digestA, size: 1 }] }),
    ).toThrow(/plain file name/u);
    expect(() => validateProductProvenance({ ...androidRecord(), schemaVersion: 2 })).toThrow(/schemaVersion/u);
    expect(() => validateProductProvenance({ ...androidRecord(), product: "not-a-product" })).toThrow(
      /is not a known release product/u,
    );
  });

  test("enforces the built-product invariants", () => {
    expect(() => androidRecord({ productVersion: "7.5.0" })).toThrow(/must equal the release version/u);
    expect(() => androidRecord({ originTag: "v1.0.0" })).toThrow(/must originate from this release tag/u);
    expect(() => androidRecord({ reusedFrom: { tier: "release" } })).toThrow(/must not carry reusedFrom/u);
  });

  test("requires all four verified checks on a reused product", () => {
    const reused = () =>
      androidRecord({
        action: "reused",
        releaseVersion: "7.8.0",
        reusedFrom: {
          attestationSubjectDigests: [digestA],
          tag: "v7.6.0",
          tier: "release",
          verifiedChecks: ["fingerprint", "digest", "origin"],
        },
      });
    expect(reused).toThrow(/verifiedChecks is missing attestation/u);
    expect(REUSE_CHECKS).toEqual(["fingerprint", "digest", "origin", "attestation"]);
  });

  test("refuses to reuse a version-stamped product across releases", () => {
    expect(() =>
      macosRecord({
        action: "reused",
        productVersion: "7.7.0",
        releaseVersion: "7.8.0",
        reusedFrom: { tag: "v7.7.0", tier: "release", verifiedChecks: [...REUSE_CHECKS] },
      }),
    ).toThrow(/version-stamped and may never be reused across releases/u);
  });

  test("cross-checks a record against the manifest artifact list", () => {
    const record = androidRecord();
    expect(
      validateProductProvenance(record, {
        expect: { manifestArtifacts: [{ name: "ghostex-android.apk", sha256: digestA, size: 91234567 }] },
      }),
    ).toBe(record);
    expect(() =>
      validateProductProvenance(record, {
        expect: { manifestArtifacts: [{ name: "ghostex-android.apk", sha256: digestB, size: 91234567 }] },
      }),
    ).toThrow(/must equal the manifest artifacts/u);
  });
});

describe("release provenance asset", () => {
  test("names and validates the published record", () => {
    expect(releaseProvenanceAssetName("7.8.0")).toBe("release-provenance-7.8.0.json");
    const record = buildReleaseProvenance({
      components: { cef: { action: "reused", componentVersion: "148.4.0-148.0.10" } },
      plan: { schemaVersion: 1 },
      products: { "macos-arm64": macosRecord() },
      publishedAt: "2026-08-13T10:11:12.000Z",
      sourceSha: commitOld,
      version: "7.8.0",
      workflowRunId: 31644067583,
    });
    expect(validateReleaseProvenance(record)).toBe(record);
    expect(record.tag).toBe("v7.8.0");
    expect(() => validateReleaseProvenance({ ...record, tag: "7.8.0" })).toThrow(/tag must equal v7\.8\.0/u);
    expect(() =>
      validateReleaseProvenance({ ...record, products: { "macos-arm64": androidRecord() } }),
    ).toThrow(/product must equal macos-arm64/u);
  });

  test("indexes recent releases newest first and ignores releases without provenance", () => {
    const index = buildReuseIndex({
      baselines: [
        {
          commit: commitOld,
          provenance: buildReleaseProvenance({
            plan: {},
            products: { android: androidRecord() },
            publishedAt: "2026-06-01T00:00:00.000Z",
            sourceSha: commitOld,
            version: "7.6.0",
            workflowRunId: 1,
          }),
          publishedAt: "2026-06-01T00:00:00.000Z",
          repo: TRUSTED_REPO,
          tag: "v7.6.0",
        },
        { publishedAt: "2026-07-01T00:00:00.000Z", repo: TRUSTED_REPO, tag: "v7.7.0" },
        {
          commit: commitOld,
          provenance: buildReleaseProvenance({
            plan: {},
            products: { android: androidRecord({ originTag: "v7.5.0", productVersion: "7.5.0", releaseVersion: "7.5.0" }) },
            publishedAt: "2026-05-01T00:00:00.000Z",
            sourceSha: commitOld,
            version: "7.5.0",
            workflowRunId: 2,
          }),
          publishedAt: "2026-05-01T00:00:00.000Z",
          repo: TRUSTED_REPO,
          tag: "v7.5.0",
        },
      ],
    });
    expect(index.get("android").map((candidate) => candidate.tag)).toEqual(["v7.6.0", "v7.5.0"]);
    expect(index.has("macos-arm64")).toBe(false);
  });

  test("indexes a nominated source run as a Tier-2 candidate", () => {
    const index = buildReuseIndex({
      sourceRun: {
        conclusion: "success",
        event: "workflow_dispatch",
        expiredArtifacts: ["release-android"],
        headSha: commitOld,
        products: { android: androidRecord() },
        repo: TRUSTED_REPO,
        runId: 31644067583,
        workflowName: "Release Ghostex",
      },
    });
    expect(index.get("android")[0]).toMatchObject({ artifactExpired: true, runId: 31644067583, tier: "run" });
  });
});

describe("reuse verification", () => {
  const accept = (overrides = {}) =>
    verifyReuseCandidate({
      candidate: releaseCandidate(androidRecord()),
      evidence: alwaysAncestor,
      fingerprint: digestB,
      productId: "android",
      releaseVersion: "7.8.0",
      ...overrides,
    });

  test("accepts a matching candidate and reports the still-pending checks", () => {
    const result = accept();
    expect(result.ok).toBe(true);
    expect(result.verifiedChecks.sort()).toEqual(["fingerprint", "origin"]);
    expect(result.pendingChecks.sort()).toEqual(["attestation", "digest"]);
    expect(result.failures).toEqual([]);
  });

  test("rejects a wrong fingerprint", () => {
    const result = accept({ fingerprint: "c".repeat(64) });
    expect(result.ok).toBe(false);
    expect(result.failures.join(" ")).toMatch(/fingerprint bbbbbbbbbbbb != cccccccccccc/u);
  });

  test("rejects bytes whose published digest does not match the record", () => {
    const result = accept({
      evidence: {
        ...alwaysAncestor,
        assetMetadata: () => ({ digest: `sha256:${digestB}`, size: 91234567 }),
      },
    });
    expect(result.ok).toBe(false);
    expect(result.failures.join(" ")).toMatch(/published digest does not match/u);
  });

  test("rejects downloaded bytes whose size does not match the record", () => {
    const result = accept({
      evidence: { ...alwaysAncestor, localArtifact: () => ({ sha256: digestA, size: 4 }) },
    });
    expect(result.ok).toBe(false);
    expect(result.failures.join(" ")).toMatch(/byte length does not match/u);
  });

  /*
   * The size-only perturbation above can be caused by a truncated download; this
   * is the case that matters — same length, different content. It is the last
   * line of defence against substituted bytes, so it gets its own test.
   */
  test("rejects downloaded bytes whose sha256 does not match the record at identical size", () => {
    const result = accept({
      evidence: {
        ...alwaysAncestor,
        localArtifact: () => ({ sha256: "e".repeat(64), size: 91234567 }),
      },
    });
    expect(result.ok).toBe(false);
    expect(result.failures.join(" ")).toMatch(/ghostex-android\.apk bytes do not match the provenance record/u);
    expect(result.failures.join(" ")).not.toMatch(/byte length/u);
  });

  test("requires both digest sources before reporting the digest check verified", () => {
    const bothSources = {
      ...alwaysAncestor,
      assetMetadata: () => ({ digest: `sha256:${digestA}`, size: 91234567 }),
      localArtifact: () => ({ sha256: digestA, size: 91234567 }),
    };
    expect(accept({ evidence: bothSources, requireAll: true }).verifiedChecks).toContain("digest");
    /* One source alone leaves the check pending, so `requireAll` refuses it. */
    const oneSource = { ...alwaysAncestor, localArtifact: () => ({ sha256: digestA, size: 91234567 }) };
    const partial = accept({ evidence: oneSource, requireAll: true });
    expect(partial.ok).toBe(false);
    expect(partial.pendingChecks).toContain("digest");
  });

  test("rejects an untrusted origin repository", () => {
    const result = accept({ candidate: releaseCandidate(androidRecord(), { repo: "attacker/Ghostex" }) });
    expect(result.ok).toBe(false);
    expect(result.failures.join(" ")).toMatch(/is not maddada\/Ghostex/u);
  });

  test("rejects a draft release and a non-ancestor origin commit", () => {
    expect(accept({ candidate: releaseCandidate(androidRecord(), { draft: true }) }).failures.join(" ")).toMatch(
      /origin release is a draft/u,
    );
    expect(
      accept({ evidence: { isAncestor: () => false } }).failures.join(" "),
    ).toMatch(/is not an ancestor of the source commit/u);
  });

  test("rejects a run that is not a completed dispatched Release Ghostex run", () => {
    const cases = [
      { conclusion: null },
      { conclusion: "action_required" },
      { event: "push" },
      { workflowName: "Some Other Workflow" },
      { artifactExpired: true },
      { artifactMissing: true },
    ];
    for (const override of cases) {
      const result = accept({ candidate: runCandidate(androidRecord(), override) });
      expect(result.ok).toBe(false);
    }
  });

  /*
   * The 7.8.0 regression guard: a product whose job succeeded — provenance
   * record uploaded, package artifact alive — stays reusable even when *other*
   * jobs made the run's overall conclusion "failure" or "cancelled". Trust is
   * product-scoped (digest, attestation, ancestry), never run-scoped.
   */
  test("accepts surviving products of a completed run that overall failed", () => {
    for (const conclusion of ["failure", "cancelled"]) {
      const result = accept({ candidate: runCandidate(androidRecord(), { conclusion }) });
      expect(result.ok).toBe(true);
      expect(result.verifiedChecks).toContain("origin");
    }
  });

  test("accepts an amend-existing workflow run as a trusted origin", () => {
    const result = accept({
      candidate: runCandidate(androidRecord(), { workflowName: "Amend existing Ghostex release" }),
    });
    expect(result.ok).toBe(true);
  });

  test("rejects an incompatible algorithm revision", () => {
    const stale = { ...androidRecord(), algorithmRevision: "fp0" };
    expect(accept({ candidate: releaseCandidate(stale) }).failures.join(" ")).toMatch(
      /algorithm revision fp0 != fp3/u,
    );
  });

  test("rejects an incompatible scope: a version-stamped product from another release", () => {
    const result = verifyReuseCandidate({
      candidate: releaseCandidate(macosRecord({ originTag: "v7.7.0", productVersion: "7.7.0", releaseVersion: "7.7.0" })),
      evidence: alwaysAncestor,
      fingerprint: digestA,
      productId: "macos-arm64",
      releaseVersion: "7.8.0",
    });
    expect(result.ok).toBe(false);
    expect(result.failures.join(" ")).toMatch(/version-stamped and cannot be reused/u);
  });

  test("only reuses a product with side files from the same run", () => {
    const sameVersionRelease = verifyReuseCandidate({
      candidate: releaseCandidate(macosRecord()),
      evidence: alwaysAncestor,
      fingerprint: digestA,
      productId: "macos-arm64",
      releaseVersion: "7.8.0",
    });
    expect(sameVersionRelease.ok).toBe(false);
    expect(sameVersionRelease.failures.join(" ")).toMatch(/publishes side files/u);

    const sameRun = verifyReuseCandidate({
      candidate: runCandidate(macosRecord()),
      evidence: alwaysAncestor,
      fingerprint: digestA,
      productId: "macos-arm64",
      releaseVersion: "7.8.0",
    });
    expect(sameRun.ok).toBe(true);
  });

  test("requireAll refuses a candidate whose bytes or attestation are unverified", () => {
    expect(accept({ requireAll: true }).ok).toBe(false);
    const fullyVerified = accept({
      evidence: {
        assetMetadata: () => ({ digest: `sha256:${digestA}`, size: 91234567 }),
        attestationVerified: () => true,
        isAncestor: () => true,
        localArtifact: () => ({ sha256: digestA, size: 91234567 }),
      },
      requireAll: true,
    });
    expect(fullyVerified.ok).toBe(true);
    expect(fullyVerified.verifiedChecks.sort()).toEqual([...REUSE_CHECKS].sort());

    /* Byte equality must hold against both the bytes and GitHub's metadata. */
    const metadataOnly = accept({
      evidence: {
        assetMetadata: () => ({ digest: `sha256:${digestA}`, size: 91234567 }),
        attestationVerified: () => true,
        isAncestor: () => true,
      },
      requireAll: true,
    });
    expect(metadataOnly.ok).toBe(false);
    expect(metadataOnly.pendingChecks).toEqual(["digest"]);
    expect(metadataOnly.failures).toEqual([]);

    const missingAttestation = accept({
      evidence: {
        assetMetadata: () => ({ digest: `sha256:${digestA}`, size: 91234567 }),
        attestationVerified: () => false,
        isAncestor: () => true,
      },
      requireAll: true,
    });
    expect(missingAttestation.ok).toBe(false);
    expect(missingAttestation.failures.join(" ")).toMatch(/no verifiable build attestation/u);
  });

  test("rejects a record that does not describe the requested product", () => {
    const result = verifyReuseCandidate({
      candidate: releaseCandidate(androidRecord()),
      evidence: alwaysAncestor,
      fingerprint: digestB,
      productId: "macos-arm64",
      releaseVersion: "7.8.0",
    });
    expect(result.ok).toBe(false);
    expect(result.pendingChecks).toEqual([...REUSE_CHECKS]);
  });
});
