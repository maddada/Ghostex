/*
 * CDXC:Release 2026-09-10 DECISION:
 * User: macOS goes live first, as soon as its build is done; after that every
 * remaining product goes live the moment its own build finishes, in whatever
 * order that happens. So one release run publishes in stages: the macOS stage
 * creates the tag and the release, and each later stage amends that release
 * independently as soon as its own builds succeed.
 *
 * WHY: the amend stages run concurrently and share only the provenance asset and
 * the release notes. They write those with a read-merge-verify loop rather than
 * a lock, because GitHub keeps one pending run per concurrency group and cancels
 * the others, which would silently drop a product's publication.
 * SEE-ALSO: .github/workflows/release-gpui.yml (the stage jobs), assemble.mjs
 * (initial stage), amend-existing.mjs (later stages).
 */

import { PRODUCT_IDS, productDefinition } from "./product-inputs.mjs";
import { releaseProvenanceAssetName } from "./provenance.mjs";

export const INITIAL_PUBLISH_STAGE = "macos";

export const PUBLISH_STAGES = Object.freeze({
  macos: Object.freeze([
    "macos-arm64",
    "gxserver-linux-x64",
    "gxserver-linux-arm64",
  ]),
  linux: Object.freeze(["linux-deb-x64", "linux-rpm-x64", "linux-tar-x64"]),
  android: Object.freeze(["android"]),
  "windows-x64": Object.freeze(["windows-x64", "gxserver-wsl-windows-x64"]),
  "windows-arm64": Object.freeze([
    "windows-arm64",
    "gxserver-wsl-windows-arm64",
  ]),
});

/* Every product belongs to exactly one stage; a product outside the map would never be published. */
{
  const covered = Object.values(PUBLISH_STAGES).flat().sort();
  const all = [...PRODUCT_IDS].sort();
  if (JSON.stringify(covered) !== JSON.stringify(all)) {
    throw new Error(
      `PUBLISH_STAGES covers ${covered.join(", ")} but the products are ${all.join(", ")}`,
    );
  }
}

/* The stage's products that the plan actually ships (built or reused); skipped ones are not this stage's business. */
export function resolvePublishStage({ plan, stage }) {
  const products = PUBLISH_STAGES[stage];
  if (!products) throw new Error(`Unknown publish stage: ${stage}`);
  const expected = new Set(plan.expectedPlatforms ?? []);
  return {
    initial: stage === INITIAL_PUBLISH_STAGE,
    products: products.filter((productId) => expected.has(productId)),
    stage,
  };
}

/* Every asset name this run's plan may put on the release, so a stage can tell a sibling stage's upload from foreign interference. */
export function plannedAssetNames({ plan, version }) {
  const names = new Set([releaseProvenanceAssetName(version)]);
  for (const productId of plan.expectedPlatforms ?? []) {
    const product = productDefinition(productId);
    for (const name of [
      ...product.artifacts(version),
      ...(product.optionalArtifacts?.(version) ?? []),
    ]) {
      names.add(name);
    }
  }
  return names;
}
