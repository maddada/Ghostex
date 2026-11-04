#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { validateOnDemandManifestV2 } from './on-demand-manifest.mjs';
import { inspectRelease, verifyPublishedComponent } from './publish-component.mjs';
import { APP_RELEASE_GITHUB_REPO, COMPONENTS_GITHUB_TOKEN_ENV, componentsGithubRepo } from './components-repo.mjs';
import { validatePlan } from './plan.mjs';
import { PRODUCT_IDS, isProductRequested, productDefinition } from './product-inputs.mjs';
import { assertStagesCoverPlan } from './publish-stage.mjs';

/*
 * CDXC:Release 2026-08-13:
 * `--plan <file>` cross-checks the resolved plan against the scope the run was
 * dispatched with. `--only-plan` runs that check alone, so the workflow can keep
 * the expensive immutable-input validation first (it must fail within seconds)
 * and still validate the plan once it exists.
 */
const argv = process.argv.slice(2);
const version = argv.find((argument) => !argument.startsWith('--'));
const planPath = argv.includes('--plan') ? argv[argv.indexOf('--plan') + 1] : null;
const onlyPlan = argv.includes('--only-plan');
if (!/^\d+\.\d+\.\d+$/u.test(version ?? '')) {
  throw new Error(`Version must be MAJOR.MINOR.PATCH, got ${version ?? 'nothing'}`);
}
if (onlyPlan && !planPath) throw new Error('--only-plan requires --plan <file>');

const enabled = (name) => process.env[name] === 'true' || process.env[name] === '1';
const requireValues = (label, names) => {
  const missing = names.filter((name) => !process.env[name]);
  if (missing.length > 0) {
    throw new Error(`${label} requires configured secrets: ${missing.join(', ')}`);
  }
};

const platforms = {
  android: enabled('GHOSTEX_RELEASE_ANDROID'),
  gxserverLinuxArm64: enabled('GHOSTEX_RELEASE_GXSERVER_LINUX_ARM64'),
  gxserverLinuxX64: enabled('GHOSTEX_RELEASE_GXSERVER_LINUX_X64'),
  gxserverWslWindowsArm64: enabled('GHOSTEX_RELEASE_GXSERVER_WSL_WINDOWS_ARM64'),
  gxserverWslWindowsX64: enabled('GHOSTEX_RELEASE_GXSERVER_WSL_WINDOWS_X64'),
  linuxAppimage: enabled('GHOSTEX_RELEASE_LINUX_APPIMAGE'),
  linuxAppimageArm64: enabled('GHOSTEX_RELEASE_LINUX_APPIMAGE_ARM64'),
  linuxDeb: enabled('GHOSTEX_RELEASE_LINUX_DEB'),
  linuxDebArm64: enabled('GHOSTEX_RELEASE_LINUX_DEB_ARM64'),
  linuxRpm: enabled('GHOSTEX_RELEASE_LINUX_RPM'),
  linuxRpmArm64: enabled('GHOSTEX_RELEASE_LINUX_RPM_ARM64'),
  linuxTar: enabled('GHOSTEX_RELEASE_LINUX_TAR'),
  linuxTarArm64: enabled('GHOSTEX_RELEASE_LINUX_TAR_ARM64'),
  macos: enabled('GHOSTEX_RELEASE_MACOS'),
  windowsArm64: enabled('GHOSTEX_RELEASE_WINDOWS_ARM64'),
  windowsX64: enabled('GHOSTEX_RELEASE_WINDOWS_X64'),
};
const linuxX64Enabled = platforms.linuxDeb || platforms.linuxRpm || platforms.linuxTar || platforms.linuxAppimage;
const linuxArm64Enabled =
  platforms.linuxDebArm64 || platforms.linuxRpmArm64 || platforms.linuxTarArm64 || platforms.linuxAppimageArm64;
const buildOnly = enabled('GHOSTEX_RELEASE_BUILD_ONLY');
const prerelease = enabled('GHOSTEX_RELEASE_PRERELEASE');

function remoteReleaseTag() {
  return execFileSync('git', ['ls-remote', '--tags', 'origin', `refs/tags/v${version}`], { encoding: 'utf8' }).trim();
}

/*
 * CDXC:Release 2026-10-19 WHY:
 * A build-only run (release-gpui.yml `build_only`) must not publish anything:
 * every publish stage is skipped, and the Linux job seals its CEF component
 * without uploading it. The other desktop jobs and a code-server component
 * build publish components from inside the build job, so build-only is limited
 * to the Linux packages and the gxserver runtimes, with the code-server
 * component reused from its live tag.
 */
const BUILD_ONLY_PRODUCTS = new Set([
  'gxserver-linux-x64',
  'gxserver-linux-arm64',
  'linux-deb-x64',
  'linux-rpm-x64',
  'linux-tar-x64',
  'linux-appimage-x64',
  'linux-deb-arm64',
  'linux-rpm-arm64',
  'linux-tar-arm64',
  'linux-appimage-arm64',
]);

function validateBuildOnlyPlan(plan) {
  const outside = (plan.expectedPlatforms ?? []).filter((productId) => !BUILD_ONLY_PRODUCTS.has(productId));
  if (outside.length > 0) {
    throw new Error(
      `A build-only run covers the Linux packages and gxserver runtimes only; this scope also has ${outside.join(', ')}`
    );
  }
  const componentBuilds = Object.entries(plan.jobs ?? {})
    .filter(([job, action]) => job.startsWith('code_server_') && action === 'build')
    .map(([job]) => job);
  if (componentBuilds.length > 0) {
    throw new Error(
      `A build-only run must not publish components, but ${componentBuilds.join(', ')} would build and publish the code-server component`
    );
  }
}
const signWindows = enabled('GHOSTEX_RELEASE_SIGN_WINDOWS');
const updateSparkle = enabled('GHOSTEX_RELEASE_UPDATE_SPARKLE');

/*
 * The plan may reduce work through verified reuse, but it may never act on a
 * product the operator did not request, never skip one they did, and never
 * disagree with the run's version or source commit. Any of those would mean the
 * plan and the dispatch describe different releases.
 */
function validatePlanAgainstScope(plan) {
  validatePlan(plan);
  if (plan.version !== version) {
    throw new Error(`The resolved plan releases ${plan.version}, not ${version}`);
  }
  if (process.env.GITHUB_SHA && plan.sourceSha !== process.env.GITHUB_SHA) {
    throw new Error(`The resolved plan was computed at ${plan.sourceSha}, not ${process.env.GITHUB_SHA}`);
  }
  const scope = {
    android: platforms.android,
    gxserverLinuxArm64: platforms.gxserverLinuxArm64,
    gxserverLinuxX64: platforms.gxserverLinuxX64,
    gxserverWslWindowsArm64: platforms.gxserverWslWindowsArm64,
    gxserverWslWindowsX64: platforms.gxserverWslWindowsX64,
    linuxAppimage: platforms.linuxAppimage,
    linuxAppimageArm64: platforms.linuxAppimageArm64,
    linuxDeb: platforms.linuxDeb,
    linuxDebArm64: platforms.linuxDebArm64,
    linuxRpm: platforms.linuxRpm,
    linuxRpmArm64: platforms.linuxRpmArm64,
    linuxTar: platforms.linuxTar,
    linuxTarArm64: platforms.linuxTarArm64,
    macos: platforms.macos,
    windowsArm64: platforms.windowsArm64,
    windowsX64: platforms.windowsX64,
  };
  const mismatches = [];
  for (const productId of PRODUCT_IDS) {
    const requested = isProductRequested(productId, scope);
    const entry = plan.products[productId];
    if (requested && entry.action === 'skip') mismatches.push(`${productId} is requested but skipped by the plan`);
    if (!requested && entry.action !== 'skip') {
      mismatches.push(`${productId} is not requested but planned as ${entry.action}`);
    }
    if (entry.action === 'reuse' && productDefinition(productId).versionStamped) {
      if (entry.reuse?.productVersion !== version) {
        mismatches.push(`${productId} is version-stamped and cannot be reused from ${entry.reuse?.productVersion}`);
      }
    }
  }
  if (mismatches.length > 0) {
    throw new Error(`The resolved plan disagrees with the requested scope:\n- ${mismatches.join('\n- ')}`);
  }
  /* Staged publishing: every expected product must belong to a publish stage. */
  assertStagesCoverPlan(plan);
  if (
    plan.jobs?.linux_arm64 === 'build' &&
    plan.jobs?.linux_arm64_gxserver_source === 'release' &&
    !remoteReleaseTag()
  ) {
    throw new Error(
      `Linux ARM64 packages without gxserver_linux_arm64 embed the live gxserver-linux-arm64.tar.gz of v${version}, ` +
        'which does not exist yet; enable gxserver_linux_arm64'
    );
  }
  if (buildOnly) validateBuildOnlyPlan(plan);
  const built = PRODUCT_IDS.filter((id) => plan.products[id].action === 'build');
  const reused = PRODUCT_IDS.filter((id) => plan.products[id].action === 'reuse');
  console.log(
    `Plan validated for ${version}: ${built.length} built (${built.join(', ') || 'none'}), ` +
      `${reused.length} reused (${reused.join(', ') || 'none'}).`
  );
}

if (onlyPlan) {
  validatePlanAgainstScope(JSON.parse(readFileSync(planPath, 'utf8')));
  process.exit(0);
}

if (process.env.GITHUB_REF && process.env.GITHUB_REF !== 'refs/heads/main') {
  throw new Error(`Public releases must be dispatched from main, got ${process.env.GITHUB_REF}`);
}
const packageVersion = JSON.parse(readFileSync('package.json', 'utf8')).version;
if (packageVersion !== version) {
  throw new Error(`package.json is ${packageVersion}; expected ${version}`);
}
const changelog = readFileSync('CHANGELOG.md', 'utf8');
if (!changelog.includes(`## ${version} -`)) {
  throw new Error(`CHANGELOG.md has no ${version} section`);
}
if (!Object.values(platforms).some(Boolean)) {
  throw new Error('At least one release platform must be enabled');
}
if (updateSparkle && !platforms.macos) {
  throw new Error('Sparkle can only be updated when the macOS package is enabled');
}
if (prerelease && updateSparkle) {
  throw new Error('A prerelease cannot advance the production Sparkle feed');
}
if (platforms.macos && (!platforms.gxserverLinuxX64 || !platforms.gxserverLinuxArm64)) {
  throw new Error('macOS requires both gxserver Linux runtimes for its sealed on-demand manifest');
}
if ((linuxX64Enabled || platforms.windowsX64 || platforms.gxserverWslWindowsX64) && !platforms.gxserverLinuxX64) {
  throw new Error('Enabled x64 Linux/Windows packages require gxserver_linux_x64');
}
if ((platforms.windowsArm64 || platforms.gxserverWslWindowsArm64) && !platforms.gxserverLinuxArm64) {
  throw new Error('Enabled ARM64 Windows packages require gxserver_linux_arm64');
}
if (linuxArm64Enabled && !platforms.gxserverLinuxArm64 && !remoteReleaseTag()) {
  throw new Error(
    'Enabled ARM64 Linux packages require gxserver_linux_arm64, unless they are added to an existing release whose live runtime they embed'
  );
}

if (platforms.macos) {
  requireValues('macOS signing', [
    'APPLE_DEVELOPER_ID_P12_BASE64',
    'APPLE_DEVELOPER_ID_P12_PASSWORD',
    'APPLE_KEYCHAIN_PASSWORD',
  ]);
  const hasNotaryKey = ['APPLE_NOTARY_KEY_BASE64', 'APPLE_NOTARY_KEY_ID', 'APPLE_NOTARY_ISSUER_ID'].every(
    (name) => process.env[name]
  );
  const hasNotaryAppleId = ['APPLE_NOTARY_APPLE_ID', 'APPLE_NOTARY_TEAM_ID', 'APPLE_NOTARY_APP_PASSWORD'].every(
    (name) => process.env[name]
  );
  if (!hasNotaryKey && !hasNotaryAppleId) {
    throw new Error(
      'macOS notarization requires either the App Store Connect key triple or the Apple ID credential triple'
    );
  }
}
if (updateSparkle) {
  requireValues('Sparkle publication', ['SPARKLE_PRIVATE_KEY']);
}
if (platforms.android) {
  requireValues('Android signing', [
    'ANDROID_RELEASE_KEYSTORE_BASE64',
    'ANDROID_RELEASE_STORE_PASSWORD',
    'ANDROID_RELEASE_KEY_ALIAS',
    'ANDROID_RELEASE_KEY_PASSWORD',
    'GHOSTEX_MOBILE_DEPLOY_KEY',
  ]);
}
/* The Windows jobs package unsigned with a notice when the SignPath token is missing (release-gpui-windows.yml). */
if (signWindows && (platforms.windowsX64 || platforms.windowsArm64) && !process.env.SIGNPATH_API_TOKEN) {
  console.log(
    '::warning::Windows signing was requested but SIGNPATH_API_TOKEN is not set; the Windows packages will be unsigned.'
  );
}

const componentManifestPath =
  process.env.GHOSTEX_RELEASE_COMPONENT_MANIFEST || path.resolve('build/on-demand-components/components.json');
const componentPlatformsEnabled =
  platforms.macos || linuxX64Enabled || linuxArm64Enabled || platforms.windowsX64 || platforms.windowsArm64;
if (componentPlatformsEnabled) {
  /* Component tags live in a separate repository that github.token cannot write; the PAT must exist before any builder starts. */
  requireValues(`Component publishing to ${componentsGithubRepo()}`, [COMPONENTS_GITHUB_TOKEN_ENV]);
}
if (!componentPlatformsEnabled) {
  console.log('Component tag validation skipped: no desktop package is enabled.');
} else if (existsSync(componentManifestPath)) {
  const parsed = JSON.parse(readFileSync(componentManifestPath, 'utf8'));
  const components = parsed.components ?? parsed;
  validateOnDemandManifestV2({
    schemaVersion: 2,
    version,
    githubRepo: APP_RELEASE_GITHUB_REPO,
    assets: {},
    components,
  });
  for (const component of Object.values(components)) {
    const repo = component.githubRepo ?? componentsGithubRepo();
    const release = inspectRelease({ repo, tag: component.downloadTag });
    verifyPublishedComponent({ component, release });
  }
  console.log(`Validated ${Object.keys(components).length} live component tag(s) against ${componentManifestPath}.`);
} else {
  if (!existsSync(path.resolve('tooling/release-gpui/publish-component.mjs'))) {
    throw new Error('Component tags are not prepared and the component publisher is missing.');
  }
  console.log(
    `Component tag validation deferred: ${componentManifestPath} does not exist yet; ` +
      'enabled platform builders will create deterministic assets and publish them idempotently before packaging completes.'
  );
}

const remoteMain = execFileSync('git', ['ls-remote', 'origin', 'refs/heads/main'], {
  encoding: 'utf8',
})
  .trim()
  .split(/\s+/u)[0];
if (process.env.GITHUB_SHA && remoteMain !== process.env.GITHUB_SHA) {
  throw new Error(
    `origin/main moved before preflight (${process.env.GITHUB_SHA} -> ${remoteMain}); redispatch the release`
  );
}
const remoteTag = remoteReleaseTag();
/* A build-only run publishes nothing, so it may build any version, released or not. */
if (remoteTag && !enabled('GHOSTEX_RELEASE_ALLOW_EXISTING_TAG') && !buildOnly) {
  throw new Error(`v${version} already exists`);
}

if (planPath) validatePlanAgainstScope(JSON.parse(readFileSync(planPath, 'utf8')));

console.log(`Validated immutable release inputs for ${version} at ${remoteMain}.`);
