/*
 * CDXC:Release 2026-08-13:
 * The release planner. Given the git tree at the source commit, the release
 * scope flags, and the provenance of recent releases (and optionally a nominated
 * source run), it decides for every product whether the release must `build` it,
 * may `reuse` byte-identical published bytes, or should `skip` it because it is
 * out of scope.
 *
 * `computePlan()` is pure: no network, no `gh`, no filesystem beyond the injected
 * git tree reader. The CLI wrapper fetches the inputs and hands them in, so the
 * same code produces the local dry-run preview and the authoritative plan the
 * `prepare` job publishes.
 *
 * Enabled is not the same as built. A scope flag expresses intent; the plan
 * decides the action. Every in-scope product still ships real bytes on the
 * versioned release, because reuse means byte-identical re-publication and never
 * omission.
 */

import {
  COMPONENT_IDS,
  PRODUCT_IDS,
  TRUSTED_REPO,
  componentPlatformRequirements,
  LINUX_DESKTOP_FORMATS,
  defaultScope,
  isProductRequested,
  linuxDesktopProductId,
  nodeDefinition,
  productDefinition,
  validateProductGraph,
} from './product-inputs.mjs';
import {
  FINGERPRINT_ALGORITHM_REVISION,
  computeFingerprints,
  describeFingerprintDifference,
  explainFingerprintDifference,
  identityRevisionInputsDigest,
  shortFingerprint,
} from './fingerprint.mjs';
import { buildReuseIndex, reuseDescriptor, verifyReuseCandidate } from './provenance.mjs';

export const PLAN_SCHEMA_VERSION = 1;
export const PRODUCT_ACTIONS = Object.freeze(['build', 'reuse', 'skip']);
export const DEFAULT_BASELINE_COUNT = 12;

/* Measured job durations (release 7.7, run 31648691822); used only for reporting. */
const PRODUCT_RUNNER_MINUTES = Object.freeze({
  android: 15,
  'gxserver-linux-arm64': 18,
  'gxserver-linux-x64': 22,
  'gxserver-wsl-windows-arm64': 1,
  'gxserver-wsl-windows-x64': 1,
  'linux-appimage-arm64': 14,
  'linux-appimage-x64': 14,
  'linux-deb-arm64': 14,
  'linux-deb-x64': 14,
  'linux-rpm-arm64': 14,
  'linux-rpm-x64': 14,
  'linux-tar-arm64': 14,
  'linux-tar-x64': 14,
  'macos-arm64': 34,
  'windows-arm64': 27,
  'windows-x64': 25,
});

const REUSE_JOB_MINUTES = 1;

function versionPatternOk(version) {
  return /^\d+\.\d+\.\d+$/u.test(version ?? '');
}

export function scopeFromEnv(env = process.env) {
  const flag = (name, fallback = false) => {
    const value = env[name];
    if (value === undefined || value === '') return fallback;
    return value === 'true' || value === '1';
  };
  return defaultScope({
    android: flag('GHOSTEX_RELEASE_ANDROID'),
    gxserverLinuxArm64: flag('GHOSTEX_RELEASE_GXSERVER_LINUX_ARM64'),
    gxserverLinuxX64: flag('GHOSTEX_RELEASE_GXSERVER_LINUX_X64'),
    gxserverWslWindowsArm64: flag('GHOSTEX_RELEASE_GXSERVER_WSL_WINDOWS_ARM64'),
    gxserverWslWindowsX64: flag('GHOSTEX_RELEASE_GXSERVER_WSL_WINDOWS_X64'),
    linuxAppimage: flag('GHOSTEX_RELEASE_LINUX_APPIMAGE'),
    linuxAppimageArm64: flag('GHOSTEX_RELEASE_LINUX_APPIMAGE_ARM64'),
    linuxDeb: flag('GHOSTEX_RELEASE_LINUX_DEB'),
    linuxDebArm64: flag('GHOSTEX_RELEASE_LINUX_DEB_ARM64'),
    linuxRpm: flag('GHOSTEX_RELEASE_LINUX_RPM'),
    linuxRpmArm64: flag('GHOSTEX_RELEASE_LINUX_RPM_ARM64'),
    linuxTar: flag('GHOSTEX_RELEASE_LINUX_TAR'),
    linuxTarArm64: flag('GHOSTEX_RELEASE_LINUX_TAR_ARM64'),
    macos: flag('GHOSTEX_RELEASE_MACOS'),
    prerelease: flag('GHOSTEX_RELEASE_PRERELEASE'),
    signWindows: flag('GHOSTEX_RELEASE_SIGN_WINDOWS'),
    updateSparkle: flag('GHOSTEX_RELEASE_UPDATE_SPARKLE'),
    windowsArm64: flag('GHOSTEX_RELEASE_WINDOWS_ARM64'),
    windowsX64: flag('GHOSTEX_RELEASE_WINDOWS_X64'),
  });
}

function candidateLabel(candidate) {
  if (candidate.tier === 'release') return candidate.tag ?? 'an earlier release';
  return `run ${candidate.runId ?? '(unknown)'}`;
}

/*
 * Candidate preference: version-stamped products may only be reused inside the
 * same version, which in practice means the nominated source run, so those
 * candidates come first. Everything else prefers the durable release tier, newest
 * first, because Actions artifacts expire and release assets do not.
 */
function orderCandidates(candidates, { versionStamped }) {
  const releases = candidates.filter((candidate) => candidate.tier === 'release');
  const runs = candidates.filter((candidate) => candidate.tier === 'run');
  return versionStamped ? [...runs, ...releases] : [...releases, ...runs];
}

function buildReason({ algorithmRevision, candidates, currentInputs, definition, version }) {
  if (candidates.length === 0) return 'no provenance baseline for this product; building';
  const comparable = candidates.find((candidate) => candidate.record?.algorithmRevision === algorithmRevision);
  if (!comparable) {
    const revisions = [...new Set(candidates.map((candidate) => candidate.record?.algorithmRevision ?? '(none)'))];
    return `provenance algorithm revision ${revisions.join(', ')} != ${algorithmRevision}; building`;
  }
  const difference = explainFingerprintDifference(currentInputs, comparable.record.inputs);
  const described = describeFingerprintDifference(difference);
  const origin = candidateLabel(comparable);
  const digests = `fingerprint ${shortFingerprint(comparable.record.fingerprint)} != ${shortFingerprint(currentInputs.fingerprint)}`;
  if (described) return `${described} changed since ${origin} (${digests})`;
  /*
   * No declared input moved: the only difference is the marketing version baked
   * into the bytes. Say so, instead of implying a source change.
   */
  if (definition.versionStamped && comparable.record.releaseVersion !== version) {
    return `version-stamped payload; rebuilt for ${version} with inputs unchanged since ${origin}`;
  }
  return `inputs differ from ${origin} (${digests})`;
}

function planProduct({
  algorithmRevision,
  assetMetadata,
  attestationVerified,
  fingerprints,
  forceAll,
  forcedProducts,
  isAncestor,
  productId,
  reuseIndex,
  scope,
  version,
}) {
  const definition = productDefinition(productId);
  const computed = fingerprints.get(productId);
  const entry = {
    action: 'skip',
    fingerprint: computed.fingerprint,
    inputs: computed.inputs,
    reason: '',
    requested: isProductRequested(productId, scope),
    reuse: null,
    rejectedReuse: [],
  };

  if (!entry.requested) {
    entry.reason = 'not in the requested release scope';
    return entry;
  }
  if (forceAll) {
    entry.action = 'build';
    entry.reason = 'force-all requested; rebuilding every in-scope product';
    return entry;
  }
  if (forcedProducts.includes(productId)) {
    entry.action = 'build';
    entry.reason = 'explicitly forced by the operator';
    return entry;
  }

  const candidates = orderCandidates(reuseIndex.get(productId) ?? [], {
    versionStamped: Boolean(definition.versionStamped),
  });
  for (const candidate of candidates) {
    const verification = verifyReuseCandidate({
      algorithmRevision,
      candidate,
      evidence: {
        assetMetadata: assetMetadata ? (name, record) => assetMetadata({ candidate, name, record }) : undefined,
        attestationVerified: attestationVerified
          ? (name, record) => attestationVerified({ candidate, name, record })
          : undefined,
        isAncestor,
      },
      fingerprint: computed.fingerprint,
      productId,
      releaseVersion: version,
    });
    if (verification.ok) {
      entry.action = 'reuse';
      entry.reuse = reuseDescriptor({ candidate, verification });
      const digest = shortFingerprint(entry.reuse.artifacts[0]?.sha256 ?? '');
      entry.reason = `all relevant inputs match ${candidateLabel(candidate)}${digest ? ` (sha256 ${digest}…)` : ''}`;
      return entry;
    }
    entry.rejectedReuse.push({ origin: candidateLabel(candidate), reasons: verification.failures });
  }

  entry.action = 'build';
  entry.reason = buildReason({
    algorithmRevision,
    candidates,
    currentInputs: { ...computed.inputs, fingerprint: computed.fingerprint },
    definition,
    version,
  });
  const fingerprintMatched = entry.rejectedReuse.find((rejected) =>
    rejected.reasons.every((reason) => !reason.startsWith('fingerprint '))
  );
  if (fingerprintMatched) {
    entry.reason = `reuse candidate ${fingerprintMatched.origin} rejected: ${fingerprintMatched.reasons.join('; ')}`;
  }
  return entry;
}

function planComponents({ componentIdentities, componentTagState, entries, products, readObject }) {
  const required = new Map(COMPONENT_IDS.map((component) => [component, new Set()]));
  for (const [productId, entry] of Object.entries(products)) {
    if (entry.action !== 'build') continue;
    for (const [component, platforms] of Object.entries(componentPlatformRequirements(productId))) {
      for (const platform of platforms) required.get(component)?.add(platform);
    }
  }

  const plan = {};
  for (const component of COMPONENT_IDS) {
    const requiredPlatforms = [...required.get(component)].sort();
    const state = componentTagState[component] ?? {};
    const componentVersion = componentIdentities[component] ?? state.componentVersion ?? null;
    const currentIdentityDigest = nodeDefinition(component).identityRevisionPathspecs
      ? identityRevisionInputsDigest({ entries, nodeId: component, readObject })
      : null;
    const entry = {
      action: 'skip',
      componentVersion,
      downloadTag: componentVersion ? `${component}-${componentVersion}` : null,
      identityRevisionInputsDigest: currentIdentityDigest,
      reason: 'no building product requires this component',
      requiredPlatforms,
    };
    if (requiredPlatforms.length === 0) {
      plan[component] = entry;
      continue;
    }
    const published = new Set(Object.keys(state.platforms ?? {}));
    const missing = requiredPlatforms.filter((platform) => !published.has(platform));
    if (
      currentIdentityDigest &&
      state.identityRevisionInputsDigest &&
      state.identityRevisionInputsDigest !== currentIdentityDigest
    ) {
      entry.action = 'build';
      entry.reason = 'component identity revision inputs changed since the published component';
    } else if (!componentVersion) {
      entry.action = 'build';
      entry.reason = 'component identity is unknown at planning time';
    } else if (missing.length === 0) {
      entry.action = 'reuse';
      entry.reason = `${entry.downloadTag} already has ${requiredPlatforms.join(', ')}`;
    } else {
      entry.action = 'build';
      entry.reason = `component tag missing ${missing.join(', ')}`;
    }
    plan[component] = entry;
  }
  return plan;
}

function componentJobAction({ arch, components, products }) {
  const consumers = [
    'macos-arm64',
    `windows-${arch}`,
    ...LINUX_DESKTOP_FORMATS.map((format) => linuxDesktopProductId(format, arch)),
  ];
  const needed = consumers.some((productId) => products[productId]?.action === 'build');
  if (!needed) return 'skip';
  const component = components['code-server'];
  if (!component) return 'build';
  const published = new Set(Object.keys(component.publishedPlatforms ?? {}));
  if (component.action === 'reuse' || published.has(`linux-${arch}`)) return 'reuse';
  return 'build';
}

/*
 * CDXC:Release 2026-09-15 WHY:
 * The darwin-arm64 code-server component has its own job
 * (release-gpui-code-server-macos.yml) so the macOS build no longer builds VS
 * Code on its critical path. Its only consumer is the macOS product.
 */
function darwinComponentJobAction({ components, products }) {
  if (products['macos-arm64']?.action !== 'build') return 'skip';
  const component = components['code-server'];
  if (!component) return 'build';
  const published = new Set(Object.keys(component.publishedPlatforms ?? {}));
  if (component.action === 'reuse' || published.has('darwin-arm64')) return 'reuse';
  return 'build';
}

/*
 * CDXC:Release 2026-09-16 WHY:
 * The native Windows editor (VS Code REH for win32, platform windows-native-<arch>)
 * has its own job per architecture (release-gpui-code-server-windows.yml) so the
 * Windows packaging job no longer compiles it: 28 minutes on x64 and 50 on ARM64
 * in 9.6.0, the largest cost of the release. Its only consumer is the matching
 * Windows product, which awaits the artifact just in time before packaging.
 */
function windowsNativeComponentJobAction({ arch, components, products }) {
  if (products[`windows-${arch}`]?.action !== 'build') return 'skip';
  const component = components['code-server'];
  if (!component) return 'build';
  const published = new Set(Object.keys(component.publishedPlatforms ?? {}));
  if (component.action === 'reuse' || published.has(`windows-native-${arch}`)) return 'reuse';
  return 'build';
}

function planJobs({ components, products }) {
  const action = (productId) => products[productId]?.action ?? 'skip';
  const linuxPackages = (arch) =>
    LINUX_DESKTOP_FORMATS.filter((format) => action(linuxDesktopProductId(format, arch)) === 'build');
  const linuxX64Packages = linuxPackages('x64');
  const linuxArm64Packages = linuxPackages('arm64');
  return {
    android: action('android'),
    code_server_arm64: componentJobAction({ arch: 'arm64', components, products }),
    code_server_darwin_arm64: darwinComponentJobAction({ components, products }),
    code_server_windows_arm64: windowsNativeComponentJobAction({ arch: 'arm64', components, products }),
    code_server_windows_x64: windowsNativeComponentJobAction({ arch: 'x64', components, products }),
    code_server_x64: componentJobAction({ arch: 'x64', components, products }),
    gxserver_arm64: action('gxserver-linux-arm64'),
    gxserver_x64: action('gxserver-linux-x64'),
    linux_arm64: linuxArm64Packages.length > 0 ? 'build' : 'skip',
    /*
     * CDXC:Release 2026-10-19 WHY:
     * Linux ARM64 was added to 10.27.0 after that release was public, and its
     * live gxserver-linux-arm64.tar.gz is already sealed into the 10.27.0 macOS
     * app. Rebuilding the runtime would publish different bytes under the same
     * name, so a run that ships the ARM64 packages without the runtime in scope
     * embeds the release's live runtime instead (`release`); otherwise the job
     * awaits this run's runtime artifact (`run`). preflight.mjs allows the
     * `release` source only when v<version> already exists.
     */
    linux_arm64_gxserver_source: action('gxserver-linux-arm64') === 'skip' ? 'release' : 'run',
    linux_arm64_packages: linuxArm64Packages,
    linux_packages: linuxX64Packages,
    linux_x64: linuxX64Packages.length > 0 ? 'build' : 'skip',
    macos: action('macos-arm64'),
    reuse_matrix: PRODUCT_IDS.filter((productId) => action(productId) === 'reuse'),
    /*
     * CDXC:Release 2026-09-01:
     * There is deliberately no `validate_windows` flag here. Both release-gpui.yml
     * and release-amend-existing.yml deleted that job — the Windows packaging jobs
     * compile the same tree natively and ARE the Windows compile validation — so
     * nothing in the graph reads it. release-gpui-validate.yml is opt-in and
     * dispatched by hand; it needs no plan input. Do not reintroduce the flag as a
     * conditional `needs:` gate: a skipped `needs:` skips its dependents, which is
     * how a release silently ships no Windows package.
     */
    windows_arm64: action('windows-arm64'),
    windows_x64: action('windows-x64'),
    wsl_arm64: action('gxserver-wsl-windows-arm64'),
    wsl_x64: action('gxserver-wsl-windows-x64'),
  };
}

/*
 * Sparkle is keyed on "macOS ships in this release", not on "macOS was rebuilt".
 * macOS is version-stamped and publishes a side file, so it can only ever be
 * reused from the *same version's* failed run — exactly the recovery where the
 * DMG exists but its appcast entry never happened, and therefore still has to
 * be made. Windows feeds stay keyed on
 * `build`, because `vpk pack` only runs in a building job and a reused Windows
 * set carries its own already-correct feed.
 */
function planFeeds({ products, scope }) {
  const macosInRelease = (products['macos-arm64']?.action ?? 'skip') !== 'skip';
  return {
    sparkle: macosInRelease && Boolean(scope.updateSparkle) && !scope.prerelease,
    windowsFeeds: ['x64', 'arm64'].filter((arch) => products[`windows-${arch}`]?.action === 'build'),
  };
}

function planEstimates({ products }) {
  let built = 0;
  let saved = 0;
  for (const productId of PRODUCT_IDS) {
    const minutes = PRODUCT_RUNNER_MINUTES[productId] ?? 0;
    const action = products[productId]?.action;
    if (action === 'build') built += minutes;
    else if (action === 'reuse') saved += Math.max(0, minutes - REUSE_JOB_MINUTES);
  }
  return { builtRunnerMinutes: built, savedRunnerMinutes: saved };
}

/** @param {Record<string, any>} options */
export function computePlan({
  algorithmRevision = FINGERPRINT_ALGORITHM_REVISION,
  assetMetadata,
  attestationVerified,
  baselineCount = DEFAULT_BASELINE_COUNT,
  baselines = [],
  componentIdentities = {},
  componentTagState = {},
  entries,
  forceAll = false,
  forcedProducts = [],
  isAncestor,
  now = new Date(),
  readObject,
  reuseFromRunId = null,
  scope = defaultScope(),
  sourceRun = null,
  sourceSha,
  version,
}) {
  validateProductGraph();
  if (!versionPatternOk(version)) throw new Error('Release version must be MAJOR.MINOR.PATCH');
  if (typeof sourceSha !== 'string' || !/^[0-9a-f]{40}$/u.test(sourceSha)) {
    throw new Error('Source SHA must be a full 40-character commit id');
  }
  if (!Array.isArray(entries)) throw new Error('Plan requires the git tree entries of the source commit');
  for (const productId of forcedProducts) productDefinition(productId);
  if (reuseFromRunId !== null && sourceRun && String(sourceRun.runId) !== String(reuseFromRunId)) {
    throw new Error(`--reuse-from-run ${reuseFromRunId} does not match the supplied source run ${sourceRun.runId}`);
  }

  const usedBaselines = baselines.slice(0, baselineCount);
  const fingerprints = computeFingerprints({ context: { scope, version }, entries, readObject });
  const reuseIndex = buildReuseIndex({ baselines: usedBaselines, sourceRun });

  const products = {};
  for (const productId of PRODUCT_IDS) {
    products[productId] = planProduct({
      algorithmRevision,
      assetMetadata,
      attestationVerified,
      fingerprints,
      forceAll,
      forcedProducts,
      isAncestor,
      productId,
      reuseIndex,
      scope,
      version,
    });
  }

  const components = planComponents({ componentIdentities, componentTagState, entries, products, readObject });
  for (const [component, entry] of Object.entries(components)) {
    entry.publishedPlatforms = componentTagState[component]?.platforms ?? {};
  }

  const expectedPlatforms = PRODUCT_IDS.filter((productId) => products[productId].action !== 'skip');
  if (expectedPlatforms.length === 0) throw new Error('At least one platform must be enabled');

  const plan = {
    algorithmRevision,
    baselineTags: usedBaselines.map((baseline) => baseline.tag).filter(Boolean),
    baselinesInspected: baselines.length,
    baselinesWithProvenance: usedBaselines.filter((baseline) => Boolean(baseline.provenance)).length,
    components,
    computedAt: new Date(now).toISOString(),
    estimates: planEstimates({ products }),
    expectedPlatforms,
    feeds: planFeeds({ products, scope }),
    forceAll,
    forcedProducts: [...forcedProducts],
    jobs: planJobs({ components, products }),
    products,
    repo: TRUSTED_REPO,
    reuseFromRunId: reuseFromRunId === null ? null : Number(reuseFromRunId),
    schemaVersion: PLAN_SCHEMA_VERSION,
    scope: { ...scope },
    sourceSha,
    version,
  };
  return validatePlan(plan);
}

export function validatePlan(input) {
  const plan = input && typeof input === 'object' ? input : null;
  if (!plan) throw new Error('Invalid release plan: not an object');
  const bad = (message) => {
    throw new Error(`Invalid release plan: ${message}`);
  };
  if (plan.schemaVersion !== PLAN_SCHEMA_VERSION) bad('schemaVersion must equal 1');
  if (!versionPatternOk(plan.version)) bad('version must be MAJOR.MINOR.PATCH');
  if (typeof plan.algorithmRevision !== 'string' || !plan.algorithmRevision) bad('algorithmRevision is required');
  for (const productId of PRODUCT_IDS) {
    const entry = plan.products?.[productId];
    if (!entry) bad(`products.${productId} is missing`);
    if (!PRODUCT_ACTIONS.includes(entry.action)) bad(`products.${productId}.action is invalid`);
    if (typeof entry.reason !== 'string' || entry.reason.length === 0) bad(`products.${productId}.reason is empty`);
    if (entry.action === 'reuse') {
      if (!entry.reuse) bad(`products.${productId} is reused without a reuse descriptor`);
      const definition = productDefinition(productId);
      if (definition.versionStamped && entry.reuse.productVersion !== plan.version) {
        bad(`${productId} is version-stamped and may never be reused across releases`);
      }
      if (!Array.isArray(entry.reuse.artifacts) || entry.reuse.artifacts.length === 0) {
        bad(`products.${productId}.reuse.artifacts is empty`);
      }
    } else if (entry.reuse) {
      bad(`products.${productId} is ${entry.action} but carries a reuse descriptor`);
    }
    if (entry.action !== 'skip' && !entry.requested) bad(`products.${productId} acts on an unrequested product`);
  }
  const expected = PRODUCT_IDS.filter((productId) => plan.products[productId].action !== 'skip');
  if (JSON.stringify(expected) !== JSON.stringify(plan.expectedPlatforms ?? [])) {
    bad('expectedPlatforms does not match the resolved product actions');
  }
  if (plan.feeds?.sparkle && plan.products['macos-arm64'].action === 'skip') {
    bad('Sparkle may only advance when macOS is part of the release');
  }
  return plan;
}

function padRight(value, width) {
  return value.length >= width ? value : value + ' '.repeat(width - value.length);
}

/* The operator-facing dry-run table (§11.1). */
export function renderPlanText(plan) {
  const lines = [];
  lines.push(`Ghostex release plan — ${plan.version}`);
  lines.push(`Source        ${plan.sourceSha}`);
  lines.push(
    `Baselines     ${plan.baselineTags.length > 0 ? plan.baselineTags.join(', ') : '(none)'}  ` +
      `(${plan.baselinesInspected} releases inspected, ${plan.baselinesWithProvenance} with provenance)`
  );
  const mode = plan.forceAll ? 'force-all' : 'change-aware';
  lines.push(
    `Mode          ${mode}   (force-all: ${plan.forceAll ? 'yes' : 'no'}, ` +
      `forced: ${plan.forcedProducts.length > 0 ? plan.forcedProducts.join(',') : 'none'}, ` +
      `reuse-from-run: ${plan.reuseFromRunId ?? 'none'})`
  );
  lines.push(`Algorithm     ${plan.algorithmRevision}`);
  lines.push('');
  lines.push(`${padRight('PRODUCT', 29)}${padRight('ACTION', 8)}REASON`);
  for (const productId of PRODUCT_IDS) {
    const entry = plan.products[productId];
    lines.push(`${padRight(productId, 29)}${padRight(entry.action.toUpperCase(), 8)}${entry.reason}`);
  }
  lines.push('');
  lines.push(`${padRight('COMPONENT', 14)}${padRight('VERSION', 30)}${padRight('ACTION', 8)}REASON`);
  for (const component of COMPONENT_IDS) {
    const entry = plan.components[component];
    const version = entry.componentVersion ?? '(unknown)';
    lines.push(
      `${padRight(component, 14)}${padRight(version.length > 28 ? `${version.slice(0, 27)}…` : version, 30)}` +
        `${padRight(entry.action.toUpperCase(), 8)}${entry.reason}`
    );
  }
  lines.push('');
  lines.push(
    `FEEDS         sparkle=${plan.feeds.sparkle ? 'update' : 'hold'}  ` +
      `windows-feeds=${plan.feeds.windowsFeeds.length > 0 ? plan.feeds.windowsFeeds.join(',') : 'none'}`
  );
  const jobNames = Object.entries(plan.jobs)
    .filter(
      ([name, value]) =>
        !['reuse_matrix', 'linux_packages', 'linux_x64', 'linux_arm64_packages', 'linux_arm64'].includes(name) &&
        value === 'build'
    )
    .map(([name]) => name);
  if (plan.jobs.linux_packages.length > 0) {
    jobNames.push(`linux_x64(${plan.jobs.linux_packages.join(',')})`);
  }
  if ((plan.jobs.linux_arm64_packages ?? []).length > 0) {
    jobNames.push(
      `linux_arm64(${plan.jobs.linux_arm64_packages.join(',')}; gxserver from ${plan.jobs.linux_arm64_gxserver_source})`
    );
  }
  lines.push(`JOBS          ${jobNames.join(', ') || '(none)'}`);
  lines.push(`              reuse[${plan.jobs.reuse_matrix.join(', ') || 'none'}]`);
  lines.push(
    `SAVED         ~${plan.estimates.savedRunnerMinutes} runner-minutes vs a full matrix ` +
      `(${plan.estimates.builtRunnerMinutes} minutes of build work planned)`
  );
  return lines.join('\n');
}

export function planSummaryLine(plan) {
  const counts = { build: 0, reuse: 0, skip: 0 };
  for (const productId of PRODUCT_IDS) counts[plan.products[productId].action] += 1;
  return `${counts.build} built, ${counts.reuse} reused, ${counts.skip} skipped by flag`;
}
