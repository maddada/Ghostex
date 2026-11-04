/*
 * CDXC:Release 2026-09-15 DECISION:
 * User: every platform goes live the moment its own build finishes, without
 * waiting for any other platform. The publish order is whatever the build
 * order happens to be; nothing waits for macOS and macOS waits for nothing.
 * So one release run publishes in independent stages (macos, android, linux,
 * linux-arm64, windows-x64, windows-arm64). The first stage to finish creates the tag
 * and the release with its own products; every later stage amends that release
 * with its own products. A stage that loses the creation race simply amends.
 * Supersedes the 2026-09-10 design where the macOS stage always created the
 * release and the others waited for it.
 *
 * WHY: the stages share only three things: the tag, the provenance asset, and
 * the release notes. Tag creation is atomic on GitHub's side (the second push
 * of the same tag is rejected), which is the whole race arbiter. Provenance and
 * notes are merged with a read-merge-verify loop rather than a lock, because a
 * GitHub concurrency group keeps one pending run and cancels the others, which
 * would silently drop a product's publication.
 *
 * A stage's product set is its own products plus the gxserver runtimes those
 * products embed, because a shipped app of this version resolves the runtime
 * from `v<version>` and must never find it missing. The runtimes therefore
 * belong to several stages; whichever publishes first uploads them, and later
 * stages verify the live digest instead of uploading again.
 * SEE-ALSO: .github/workflows/release-gpui.yml (the stage jobs),
 * .github/workflows/release-gpui-publish.yml (one call per stage),
 * publish-staged.mjs (create-or-amend), assemble.mjs, amend-existing.mjs.
 */

import { PRODUCT_IDS, productDefinition } from './product-inputs.mjs';
import { packDependencies } from './amend-existing-lib.mjs';
import { releaseProvenanceAssetName } from './provenance.mjs';

/* Own products per stage; pack dependencies are added by resolvePublishStage. */
export const PUBLISH_STAGES = Object.freeze({
  macos: Object.freeze(['macos-arm64']),
  android: Object.freeze(['android']),
  linux: Object.freeze(['linux-deb-x64', 'linux-rpm-x64', 'linux-tar-x64', 'linux-appimage-x64']),
  'linux-arm64': Object.freeze(['linux-deb-arm64', 'linux-rpm-arm64', 'linux-tar-arm64', 'linux-appimage-arm64']),
  'windows-x64': Object.freeze(['windows-x64', 'gxserver-wsl-windows-x64']),
  'windows-arm64': Object.freeze(['windows-arm64', 'gxserver-wsl-windows-arm64']),
});

export const PUBLISH_STAGE_NAMES = Object.freeze(Object.keys(PUBLISH_STAGES));

/* assemble.mjs exits with this when the remote tag appeared under it; publish-staged.mjs then amends. */
export const LOST_CREATION_RACE_EXIT_CODE = 75;

/* The stages that ship at least one product a plan expects. */
export function stagesForPlan(plan) {
  const expected = new Set(plan.expectedPlatforms ?? []);
  return PUBLISH_STAGE_NAMES.filter((stage) => PUBLISH_STAGES[stage].some((productId) => expected.has(productId)));
}

/*
 * Every expected product must be published by some stage. Own products cover
 * the customer packages; the gxserver runtimes are only ever published as a
 * dependency of a package that embeds them, so an enabled runtime with no
 * enabled consumer would silently never ship. That is a scope error, reported
 * at planning time rather than discovered on the release page.
 */
export function assertStagesCoverPlan(plan) {
  const expected = plan.expectedPlatforms ?? [];
  const covered = new Set();
  for (const stage of stagesForPlan(plan)) {
    for (const productId of resolvePublishStage({ plan, stage }).products) covered.add(productId);
  }
  const orphaned = expected.filter((productId) => !covered.has(productId));
  if (orphaned.length > 0) {
    throw new Error(
      `${orphaned.join(', ')} would never be published: no enabled package embeds it. ` +
        'Enable a package that consumes it or skip it from the scope.'
    );
  }
}

/*
 * The stage's products that this plan actually ships (built or reused), plus
 * the gxserver runtimes they embed. Skipped products are not this stage's
 * business.
 */
export function resolvePublishStage({ plan, stage }) {
  const own = PUBLISH_STAGES[stage];
  if (!own) throw new Error(`Unknown publish stage: ${stage} (expected one of ${PUBLISH_STAGE_NAMES.join(', ')})`);
  const expected = new Set(plan.expectedPlatforms ?? []);
  const ownProducts = own.filter((productId) => expected.has(productId));
  const dependencies = [
    ...new Set(ownProducts.flatMap((productId) => packDependencies(productId)).filter((id) => expected.has(id))),
  ];
  const products = PRODUCT_IDS.filter(
    (productId) => ownProducts.includes(productId) || dependencies.includes(productId)
  );
  return { dependencies, ownProducts, products, stage };
}

/* Every asset name this run's plan may put on the release, so a stage can tell a sibling stage's upload from foreign interference. */
export function plannedAssetNames({ plan, version }) {
  const names = new Set([releaseProvenanceAssetName(version)]);
  for (const productId of plan.expectedPlatforms ?? []) {
    const product = productDefinition(productId);
    for (const name of [...product.artifacts(version), ...(product.optionalArtifacts?.(version) ?? [])]) {
      names.add(name);
    }
  }
  return names;
}

/*
 * Actions artifact names a stage's publisher needs: its products, the plan,
 * and the provenance record of every product in the plan, siblings' included
 * (they are small). Its own records validate what it mutates; a sibling's
 * record lets it restore that sibling's entry when a concurrent write dropped
 * it (see the merge loop in amend-existing.mjs).
 */
export function stageArtifactNames({ plan, stage }) {
  const { products } = resolvePublishStage({ plan, stage });
  return [
    'release-plan',
    ...products.map((productId) => `release-${productId}`),
    ...(plan.expectedPlatforms ?? []).map((productId) => `release-provenance-${productId}`),
  ];
}

/* A download-artifact glob for the stage's artifacts. */
export function stageArtifactPattern({ plan, stage }) {
  return `{${stageArtifactNames({ plan, stage }).join(',')}}`;
}
