#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

import { COMPONENTS_GITHUB_TOKEN_ENV, componentsGithubRepo } from './release-gpui/components-repo.mjs';
import { resolvePublishRecoveryInputs } from './release-gpui/publish-provenance.mjs';
import { PUBLISH_STAGE_NAMES } from './release-gpui/publish-stage.mjs';
import { isAllowedReleaseWorkflowName } from './release-gpui/provenance.mjs';

const repo = 'maddada/Ghostex';

function run(command, args, options = {}) {
  const output = execFileSync(command, args, {
    cwd: new URL('..', import.meta.url),
    encoding: 'utf8',
    stdio: options.capture === false ? 'inherit' : 'pipe',
  });
  return typeof output === 'string' ? output.trim() : '';
}

function sleep(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

function usage() {
  return `
Usage:
  node tooling/release-gpui-actions.mjs start <version> [options]
  node tooling/release-gpui-actions.mjs publish <version> --source-run-id <id> [options]
  node tooling/release-gpui-actions.mjs amend <version> [product flags] [options]

Scope options for start/publish (all are enabled by default):
  --only-macos
  --only-linux-arm64                     Linux ARM64 packages only (plus their gxserver runtime); implies --skip-sparkle
  --skip-macos
  --skip-linux                           Both architectures
  --skip-linux-x64 | --skip-linux-deb | --skip-linux-rpm | --skip-linux-tar | --skip-linux-appimage   (x64)
  --skip-linux-arm64 | --skip-linux-deb-arm64 | --skip-linux-rpm-arm64 | --skip-linux-tar-arm64
                     | --skip-linux-appimage-arm64
  --skip-windows | --skip-windows-x64 | --skip-windows-arm64
  --skip-android
  --skip-gxserver-linux-x64 | --skip-gxserver-linux-arm64
  --skip-gxserver-wsl | --skip-gxserver-wsl-x64 | --skip-gxserver-wsl-arm64

Product flags for amend (all are disabled by default; opt-in):
  --macos
  --linux | --linux-deb | --linux-rpm | --linux-tar
  --windows | --windows-x64 | --windows-arm64
  --android
  --gxserver | --gxserver-linux-x64 | --gxserver-linux-arm64
  --wsl | --gxserver-wsl | --gxserver-wsl-x64 | --gxserver-wsl-arm64

Release options:
  --skip-sparkle
  --prerelease
  --windows-signing <auto|required|off>  Default: auto (signs only when the SIGNPATH_SIGNING_POLICY_SLUG
                                         repository variable is release-signing; required signs with the
                                         configured policy, test-signing by default)
  --source-run-id <id>                   Required by publish
  --stage <name>                         publish only: one stage of a staged release
                                         (macos, android, linux, linux-arm64, windows-x64, windows-arm64)
  --dry-run
  --build-only                           start only: build and upload Actions artifacts, publish nothing
                                         (no tag, no release, no component uploads). Linux packages and
                                         gxserver runtimes only; an existing tag is allowed.

Planning options (scope flags express intent; the plan decides build/reuse/skip):
  --force-all                            Rebuild every in-scope product, ignoring verified reuse
  --force <a,b>                          Rebuild these products even when their inputs are unchanged
  --reuse-from-run <id>                  Also reuse the successful products of this failed run
`.trim();
}

function parseArgs(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(usage());
    process.exit(0);
  }
  const [command = 'start', version, ...rest] = argv;
  if (!['start', 'publish', 'amend'].includes(command)) throw new Error(`Unknown command: ${command}`);
  if (!/^\d+\.\d+\.\d+$/u.test(version ?? '')) throw new Error('Pass a MAJOR.MINOR.PATCH version');
  const amendDefaults = command === 'amend';
  const options = {
    android: !amendDefaults,
    buildOnly: false,
    dryRun: false,
    forceAll: false,
    forceProducts: '',
    skipLocalTests: false,
    reuseFromRunId: '',
    gxserverLinuxArm64: !amendDefaults,
    gxserverLinuxX64: !amendDefaults,
    gxserverWslWindowsArm64: !amendDefaults,
    gxserverWslWindowsX64: !amendDefaults,
    /* release-amend-existing.yml has no AppImage inputs either; an amend never selects them. */
    linuxAppimage: !amendDefaults,
    linuxAppimageArm64: !amendDefaults,
    linuxDeb: !amendDefaults,
    /* release-amend-existing.yml has no Linux ARM64 inputs; an amend never selects them. */
    linuxDebArm64: !amendDefaults,
    linuxRpm: !amendDefaults,
    linuxRpmArm64: !amendDefaults,
    linuxTar: !amendDefaults,
    linuxTarArm64: !amendDefaults,
    macos: !amendDefaults,
    prerelease: false,
    sourceRunId: '',
    stage: '',
    updateSparkle: true,
    windowsArm64: !amendDefaults,
    windowsSigning: 'auto',
    windowsX64: !amendDefaults,
  };
  const rejectSkipOnAmend = (flag) => {
    if (command === 'amend') {
      throw new Error(`amend is opt-in; use the matching --windows/--macos/--gxserver flag instead of ${flag}`);
    }
  };
  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index];
    if (arg === '--only-macos') {
      rejectSkipOnAmend(arg);
      Object.assign(options, {
        android: false,
        gxserverLinuxArm64: true,
        gxserverLinuxX64: true,
        gxserverWslWindowsArm64: false,
        gxserverWslWindowsX64: false,
        linuxAppimage: false,
        linuxAppimageArm64: false,
        linuxDeb: false,
        linuxDebArm64: false,
        linuxRpm: false,
        linuxRpmArm64: false,
        linuxTar: false,
        linuxTarArm64: false,
        macos: true,
        windowsArm64: false,
        windowsX64: false,
      });
    } else if (arg === '--only-linux-arm64') {
      rejectSkipOnAmend(arg);
      Object.assign(options, {
        android: false,
        gxserverLinuxArm64: true,
        gxserverLinuxX64: false,
        gxserverWslWindowsArm64: false,
        gxserverWslWindowsX64: false,
        linuxAppimage: false,
        linuxAppimageArm64: true,
        linuxDeb: false,
        linuxDebArm64: true,
        linuxRpm: false,
        linuxRpmArm64: true,
        linuxTar: false,
        linuxTarArm64: true,
        macos: false,
        updateSparkle: false,
        windowsArm64: false,
        windowsX64: false,
      });
    } else if (arg === '--skip-macos') {
      rejectSkipOnAmend(arg);
      options.macos = false;
    } else if (arg === '--macos') options.macos = true;
    else if (arg === '--linux') options.linuxDeb = options.linuxRpm = options.linuxTar = true;
    else if (arg === '--linux-deb') options.linuxDeb = true;
    else if (arg === '--linux-rpm') options.linuxRpm = true;
    else if (arg === '--linux-tar') options.linuxTar = true;
    else if (arg === '--windows') options.windowsX64 = options.windowsArm64 = true;
    else if (arg === '--windows-x64') options.windowsX64 = true;
    else if (arg === '--windows-arm64') options.windowsArm64 = true;
    else if (arg === '--android') options.android = true;
    else if (arg === '--gxserver') options.gxserverLinuxX64 = options.gxserverLinuxArm64 = true;
    else if (arg === '--gxserver-linux-x64') options.gxserverLinuxX64 = true;
    else if (arg === '--gxserver-linux-arm64') options.gxserverLinuxArm64 = true;
    else if (arg === '--wsl' || arg === '--gxserver-wsl') {
      options.gxserverWslWindowsX64 = options.gxserverWslWindowsArm64 = true;
    } else if (arg === '--gxserver-wsl-x64') options.gxserverWslWindowsX64 = true;
    else if (arg === '--gxserver-wsl-arm64') options.gxserverWslWindowsArm64 = true;
    else if (arg === '--skip-linux') {
      rejectSkipOnAmend(arg);
      options.linuxDeb = options.linuxRpm = options.linuxTar = options.linuxAppimage = false;
      options.linuxDebArm64 = options.linuxRpmArm64 = options.linuxTarArm64 = options.linuxAppimageArm64 = false;
    } else if (arg === '--skip-linux-x64') {
      rejectSkipOnAmend(arg);
      options.linuxDeb = options.linuxRpm = options.linuxTar = options.linuxAppimage = false;
    } else if (arg === '--skip-linux-arm64') {
      rejectSkipOnAmend(arg);
      options.linuxDebArm64 = options.linuxRpmArm64 = options.linuxTarArm64 = options.linuxAppimageArm64 = false;
    } else if (arg === '--skip-linux-appimage-arm64') {
      rejectSkipOnAmend(arg);
      options.linuxAppimageArm64 = false;
    } else if (arg === '--skip-linux-appimage') {
      rejectSkipOnAmend(arg);
      options.linuxAppimage = false;
    } else if (arg === '--skip-linux-deb-arm64') {
      rejectSkipOnAmend(arg);
      options.linuxDebArm64 = false;
    } else if (arg === '--skip-linux-rpm-arm64') {
      rejectSkipOnAmend(arg);
      options.linuxRpmArm64 = false;
    } else if (arg === '--skip-linux-tar-arm64') {
      rejectSkipOnAmend(arg);
      options.linuxTarArm64 = false;
    } else if (arg === '--skip-linux-deb') {
      rejectSkipOnAmend(arg);
      options.linuxDeb = false;
    } else if (arg === '--skip-linux-rpm') {
      rejectSkipOnAmend(arg);
      options.linuxRpm = false;
    } else if (arg === '--skip-linux-tar') {
      rejectSkipOnAmend(arg);
      options.linuxTar = false;
    } else if (arg === '--skip-windows') {
      rejectSkipOnAmend(arg);
      options.windowsX64 = options.windowsArm64 = false;
    } else if (arg === '--skip-windows-x64') {
      rejectSkipOnAmend(arg);
      options.windowsX64 = false;
    } else if (arg === '--skip-windows-arm64') {
      rejectSkipOnAmend(arg);
      options.windowsArm64 = false;
    } else if (arg === '--skip-android') {
      rejectSkipOnAmend(arg);
      options.android = false;
    } else if (arg === '--skip-gxserver-linux-x64') {
      rejectSkipOnAmend(arg);
      options.gxserverLinuxX64 = false;
    } else if (arg === '--skip-gxserver-linux-arm64') {
      rejectSkipOnAmend(arg);
      options.gxserverLinuxArm64 = false;
    } else if (arg === '--skip-gxserver-wsl') {
      rejectSkipOnAmend(arg);
      options.gxserverWslWindowsX64 = false;
      options.gxserverWslWindowsArm64 = false;
    } else if (arg === '--skip-gxserver-wsl-x64') {
      rejectSkipOnAmend(arg);
      options.gxserverWslWindowsX64 = false;
    } else if (arg === '--skip-gxserver-wsl-arm64') {
      rejectSkipOnAmend(arg);
      options.gxserverWslWindowsArm64 = false;
    } else if (arg === '--skip-sparkle') options.updateSparkle = false;
    else if (arg === '--prerelease') options.prerelease = true;
    else if (arg === '--dry-run') options.dryRun = true;
    else if (arg === '--build-only') options.buildOnly = true;
    else if (arg === '--skip-local-tests') options.skipLocalTests = true;
    else if (arg === '--force-all') options.forceAll = true;
    else if (arg === '--force') {
      options.forceProducts = rest[index + 1] ?? '';
      index += 1;
    } else if (arg === '--reuse-from-run') {
      options.reuseFromRunId = rest[index + 1] ?? '';
      index += 1;
    } else if (arg === '--windows-signing') {
      options.windowsSigning = rest[index + 1] ?? '';
      index += 1;
    } else if (arg === '--source-run-id') {
      options.sourceRunId = rest[index + 1] ?? '';
      index += 1;
    } else if (arg === '--stage') {
      options.stage = rest[index + 1] ?? '';
      index += 1;
    } else {
      throw new Error(`Unknown option: ${arg}`);
    }
  }
  if (options.stage && command !== 'publish') throw new Error('--stage is only valid with publish');
  if (options.buildOnly && command !== 'start') throw new Error('--build-only is only valid with start');
  if (options.stage && !PUBLISH_STAGE_NAMES.includes(options.stage)) {
    throw new Error(`--stage must be one of ${PUBLISH_STAGE_NAMES.join(', ')}`);
  }
  if (!['auto', 'required', 'off'].includes(options.windowsSigning)) {
    throw new Error('--windows-signing must be auto, required, or off');
  }
  if (command === 'publish' && !/^\d+$/u.test(options.sourceRunId)) {
    throw new Error('publish requires --source-run-id <GitHub Actions run id>');
  }
  if (options.forceAll && options.forceProducts) {
    throw new Error('--force-all already rebuilds everything; drop --force');
  }
  if (options.reuseFromRunId && !/^\d+$/u.test(options.reuseFromRunId)) {
    throw new Error('--reuse-from-run must be a GitHub Actions run id');
  }
  return { command, options, version };
}

function validateScope(options, command) {
  const enabled = [
    options.macos,
    linuxX64Enabled(options),
    linuxArm64Enabled(options),
    options.windowsX64,
    options.windowsArm64,
    options.android,
    options.gxserverLinuxX64,
    options.gxserverLinuxArm64,
    options.gxserverWslWindowsX64,
    options.gxserverWslWindowsArm64,
  ];
  if (!enabled.some(Boolean)) throw new Error('At least one platform must be enabled');
  if (options.prerelease && options.updateSparkle) {
    throw new Error('A prerelease requires --skip-sparkle');
  }
  if (command === 'amend') {
    if (options.prerelease) throw new Error('amend requires an existing public stable release');
    return;
  }
  if (options.updateSparkle && !options.macos) throw new Error('--skip-macos requires --skip-sparkle');
  if (options.macos && (!options.gxserverLinuxX64 || !options.gxserverLinuxArm64)) {
    throw new Error('macOS requires both gxserver Linux runtimes');
  }
  if ((linuxX64Enabled(options) || options.windowsX64 || options.gxserverWslWindowsX64) && !options.gxserverLinuxX64) {
    throw new Error('Enabled x64 packages require gxserver Linux x64');
  }
  if ((options.windowsArm64 || options.gxserverWslWindowsArm64) && !options.gxserverLinuxArm64) {
    throw new Error('Enabled ARM64 packages require gxserver Linux ARM64');
  }
  /*
   * Staged publishing ships a gxserver runtime only as a dependency of a
   * package that embeds it, so a runtime with no enabled consumer would never
   * reach the release page. Refuse the scope here rather than on the runner.
   */
  const x64Consumers = [options.macos, linuxX64Enabled(options), options.windowsX64, options.gxserverWslWindowsX64];
  if (options.gxserverLinuxX64 && !x64Consumers.some(Boolean)) {
    throw new Error(
      'gxserver Linux x64 is enabled but no enabled package embeds it; add --skip-gxserver-linux-x64 or enable a consumer'
    );
  }
  const arm64Consumers = [
    options.macos,
    linuxArm64Enabled(options),
    options.windowsArm64,
    options.gxserverWslWindowsArm64,
  ];
  if (options.gxserverLinuxArm64 && !arm64Consumers.some(Boolean)) {
    throw new Error(
      'gxserver Linux ARM64 is enabled but no enabled package embeds it; add --skip-gxserver-linux-arm64 or enable a consumer'
    );
  }
}

function linuxX64Enabled(options) {
  return options.linuxDeb || options.linuxRpm || options.linuxTar || options.linuxAppimage;
}

function linuxArm64Enabled(options) {
  return options.linuxDebArm64 || options.linuxRpmArm64 || options.linuxTarArm64 || options.linuxAppimageArm64;
}

/*
 * Linux ARM64 normally embeds this run's gxserver-linux-arm64. Added to a
 * release that is already public (10.27.0), it embeds that release's live
 * runtime instead, because rebuilding the runtime would publish different
 * bytes under a name the live macOS app already sealed (plan.mjs,
 * linux_arm64_gxserver_source). So the runtime may be out of scope only when
 * the tag exists.
 */
function validateLinuxArm64Runtime(options, tagExists) {
  if (linuxArm64Enabled(options) && !options.gxserverLinuxArm64 && !tagExists) {
    throw new Error(
      'Enabled ARM64 Linux packages require gxserver Linux ARM64; only a release that already exists may ' +
        'skip it (the packages then embed its live gxserver-linux-arm64.tar.gz)'
    );
  }
}

/* release-gpui.yml publishes nothing in a build-only run; the runner's preflight enforces the same scope on the plan. */
function validateBuildOnly(options) {
  if (!options.buildOnly) return;
  const outside = [
    options.macos && 'macOS',
    (options.windowsX64 || options.windowsArm64) && 'Windows',
    options.android && 'Android',
    (options.gxserverWslWindowsX64 || options.gxserverWslWindowsArm64) && 'the WSL runtimes',
  ].filter(Boolean);
  if (outside.length > 0) {
    throw new Error(`--build-only covers the Linux packages and gxserver runtimes only; skip ${outside.join(', ')}`);
  }
  if (options.updateSparkle) throw new Error('--build-only requires --skip-sparkle');
}

function requiresGpuiReferenceContract(options) {
  return (
    options.macos ||
    linuxX64Enabled(options) ||
    linuxArm64Enabled(options) ||
    options.windowsX64 ||
    options.windowsArm64
  );
}

function expectedPlatforms(options) {
  return [
    options.macos && 'macos-arm64',
    options.linuxDeb && 'linux-deb-x64',
    options.linuxRpm && 'linux-rpm-x64',
    options.linuxTar && 'linux-tar-x64',
    options.linuxAppimage && 'linux-appimage-x64',
    options.linuxDebArm64 && 'linux-deb-arm64',
    options.linuxRpmArm64 && 'linux-rpm-arm64',
    options.linuxTarArm64 && 'linux-tar-arm64',
    options.linuxAppimageArm64 && 'linux-appimage-arm64',
    options.windowsX64 && 'windows-x64',
    options.windowsArm64 && 'windows-arm64',
    options.android && 'android',
    options.gxserverLinuxX64 && 'gxserver-linux-x64',
    options.gxserverLinuxArm64 && 'gxserver-linux-arm64',
    options.gxserverWslWindowsX64 && 'gxserver-wsl-windows-x64',
    options.gxserverWslWindowsArm64 && 'gxserver-wsl-windows-arm64',
  ].filter(Boolean);
}

function validateLocalSource(version, { allowExistingTag, requireExistingTag = false }) {
  run('gh', ['auth', 'status'], { capture: false });
  const branch = run('git', ['branch', '--show-current']);
  if (branch !== 'main') throw new Error(`Release source must be main, got ${branch}`);
  const status = run('git', ['status', '--porcelain', '--untracked-files=all']);
  if (status) throw new Error(`Release source is dirty:\n${status}`);
  run('git', ['fetch', 'origin', 'main', '--tags'], { capture: false });
  const head = run('git', ['rev-parse', 'HEAD']);
  const remoteMain = run('git', ['rev-parse', 'origin/main']);
  if (head !== remoteMain) throw new Error(`Local main ${head} differs from origin/main ${remoteMain}`);
  const packageVersion = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
  if (packageVersion !== version) throw new Error(`package.json is ${packageVersion}; expected ${version}`);
  const changelog = readFileSync(new URL('../CHANGELOG.md', import.meta.url), 'utf8');
  if (!changelog.includes(`## ${version} -`)) throw new Error(`CHANGELOG.md has no ${version} section`);
  const tag = run('git', ['ls-remote', '--tags', 'origin', `refs/tags/v${version}`]);
  if (tag && !allowExistingTag) throw new Error(`v${version} already exists`);
  if (requireExistingTag && !tag) throw new Error(`v${version} does not exist; amend requires a public tag`);
  return head;
}

function configuredSecrets() {
  const attempts = 3;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      const json = run('gh', ['secret', 'list', '--repo', repo, '--json', 'name']);
      return new Set(json ? JSON.parse(json).map(({ name }) => name) : []);
    } catch (error) {
      if (attempt === attempts) throw error;
      console.warn(`GitHub secret inventory failed (attempt ${attempt}/${attempts}); retrying...`);
      sleep(attempt * 1500);
    }
  }
  throw new Error('Unable to read GitHub repository secrets');
}

function requireSecrets(secrets, label, names) {
  const missing = names.filter((name) => !secrets.has(name));
  if (missing.length > 0) throw new Error(`${label} requires repository secrets: ${missing.join(', ')}`);
}

/* The SignPath policy the Windows jobs sign with: the repository variable, else test-signing (release-gpui-windows.yml). */
function windowsSigningPolicy() {
  const json = run('gh', ['variable', 'list', '--repo', repo, '--json', 'name,value']);
  const variable = (json ? JSON.parse(json) : []).find(({ name }) => name === 'SIGNPATH_SIGNING_POLICY_SLUG');
  return variable?.value || 'test-signing';
}

/*
 * SEE-ALSO: CDXC:Release 2026-10-19 DECISION in .github/workflows/release-gpui-windows.yml.
 * `auto` signs only with the production policy, so Windows ships unsigned until
 * SIGNPATH_SIGNING_POLICY_SLUG is release-signing; `required` signs with whatever
 * policy is configured, test-signing included.
 */
function resolveWindowsSigning(options, secrets) {
  if (!options.windowsX64 && !options.windowsArm64) return false;
  const names = ['SIGNPATH_API_TOKEN'];
  const available = names.every((name) => secrets.has(name));
  if (options.windowsSigning === 'required' && !available) {
    requireSecrets(secrets, 'Windows signing', names);
  }
  if (options.windowsSigning === 'off') return false;
  if (options.windowsSigning === 'required') return true;
  if (!available) return false;
  const policy = windowsSigningPolicy();
  if (policy !== 'release-signing') {
    console.log(
      `Windows signing stays off: SignPath policy is ${policy}, and auto signs only with release-signing ` +
        '(gh variable set SIGNPATH_SIGNING_POLICY_SLUG --body release-signing; --windows-signing required test-signs one dispatch).'
    );
    return false;
  }
  return true;
}

function validateRequiredSecrets(options, secrets) {
  if (options.macos) {
    requireSecrets(secrets, 'macOS signing', [
      'APPLE_DEVELOPER_ID_P12_BASE64',
      'APPLE_DEVELOPER_ID_P12_PASSWORD',
      'APPLE_KEYCHAIN_PASSWORD',
    ]);
    const notaryKey = ['APPLE_NOTARY_KEY_BASE64', 'APPLE_NOTARY_KEY_ID', 'APPLE_NOTARY_ISSUER_ID'];
    const notaryAppleId = ['APPLE_NOTARY_APPLE_ID', 'APPLE_NOTARY_TEAM_ID', 'APPLE_NOTARY_APP_PASSWORD'];
    if (!notaryKey.every((name) => secrets.has(name)) && !notaryAppleId.every((name) => secrets.has(name))) {
      throw new Error('macOS notarization secrets are incomplete');
    }
  }
  if (options.updateSparkle) requireSecrets(secrets, 'Sparkle', ['SPARKLE_PRIVATE_KEY']);
  if (
    options.macos ||
    linuxX64Enabled(options) ||
    linuxArm64Enabled(options) ||
    options.windowsX64 ||
    options.windowsArm64
  ) {
    /* Component tags are published to a separate repository that github.token cannot write. */
    requireSecrets(secrets, `Component publishing to ${componentsGithubRepo()}`, [COMPONENTS_GITHUB_TOKEN_ENV]);
  }
  /*
   * The Linux job signs AppImages only when both secrets exist (linux-appimage.sh), so a fork still builds.
   * A real release must not ship them unsigned: AppImageUpdate refuses to update a signed install to an
   * unsigned image.
   */
  if (options.linuxAppimage || options.linuxAppimageArm64) {
    requireSecrets(secrets, 'AppImage signing', [
      'GHOSTEX_APPIMAGE_SIGNING_KEY',
      'GHOSTEX_APPIMAGE_SIGNING_PASSPHRASE',
    ]);
  }
  if (options.android) {
    requireSecrets(secrets, 'Android signing', [
      'ANDROID_RELEASE_KEYSTORE_BASE64',
      'ANDROID_RELEASE_STORE_PASSWORD',
      'ANDROID_RELEASE_KEY_ALIAS',
      'ANDROID_RELEASE_KEY_PASSWORD',
      'GHOSTEX_MOBILE_DEPLOY_KEY',
    ]);
  }
}

/*
 * CDXC:Release 2026-08-13:
 * Show the operator what the release will actually do before anything is
 * dispatched. This is the same planner the `prepare` job runs, with the same
 * scope, so the preview equals reality unless origin/main moves — which is
 * already a hard stop above.
 */
function previewPlan(version, options, windowsSigned, { required }) {
  const scope = {
    android: options.android,
    gxserverLinuxArm64: options.gxserverLinuxArm64,
    gxserverLinuxX64: options.gxserverLinuxX64,
    gxserverWslWindowsArm64: options.gxserverWslWindowsArm64,
    gxserverWslWindowsX64: options.gxserverWslWindowsX64,
    linuxAppimage: options.linuxAppimage,
    linuxAppimageArm64: options.linuxAppimageArm64,
    linuxDeb: options.linuxDeb,
    linuxDebArm64: options.linuxDebArm64,
    linuxRpm: options.linuxRpm,
    linuxRpmArm64: options.linuxRpmArm64,
    linuxTar: options.linuxTar,
    linuxTarArm64: options.linuxTarArm64,
    macos: options.macos,
    prerelease: options.prerelease,
    signWindows: windowsSigned,
    updateSparkle: options.updateSparkle,
    windowsArm64: options.windowsArm64,
    windowsX64: options.windowsX64,
  };
  const args = ['tooling/release-gpui/plan-cli.mjs', version, '--scope-json', JSON.stringify(scope)];
  if (options.forceAll) args.push('--force-all');
  else if (options.forceProducts) args.push('--force', options.forceProducts);
  if (options.reuseFromRunId) args.push('--reuse-from-run', options.reuseFromRunId);
  try {
    run('node', args, { capture: false });
  } catch (error) {
    if (required) throw error;
    console.warn(
      `Could not compute the local plan preview (${error instanceof Error ? error.message : String(error)}). ` +
        'The prepare job computes the authoritative plan on the runner.'
    );
  }
}

/* The publish path keys off the source run's recorded plan, never re-typed flags. */
function recordedPlanFor(sourceRunId) {
  const destination = `build/release-gpui/source-run-${sourceRunId}`;
  run('gh', ['run', 'download', sourceRunId, '--repo', repo, '--name', 'release-plan', '--dir', destination]);
  const planPath = new URL(`../${destination}/release-plan.json`, import.meta.url);
  const plan = JSON.parse(readFileSync(planPath, 'utf8'));
  if (plan.schemaVersion !== 1 || !Array.isArray(plan.expectedPlatforms) || plan.expectedPlatforms.length === 0) {
    throw new Error(`Source run ${sourceRunId} has an unreadable release plan; refusing publish-only recovery`);
  }
  return plan;
}

function dispatch(workflow, fields, dryRun) {
  const args = ['workflow', 'run', workflow, '--repo', repo, '--ref', 'main'];
  for (const [name, value] of Object.entries(fields)) args.push('-f', `${name}=${value}`);
  if (dryRun) {
    console.log(JSON.stringify({ fields, workflow }, null, 2));
    return;
  }
  const output = run('gh', args);
  const url = output.split(/\r?\n/u).find((line) => /\/actions\/runs\/\d+$/u.test(line.trim()));
  console.log(url ?? output);
}

const { command, options, version } = parseArgs(process.argv.slice(2));
validateScope(options, command);
validateBuildOnly(options);
/*
 * A staged release may already have created the tag when one platform failed:
 * the recovery redispatch (`start --reuse-from-run`) then targets an existing
 * tag on purpose, and its stages amend the live release. A plain `start`
 * against an existing tag stays refused.
 */
const head = validateLocalSource(version, {
  allowExistingTag:
    command === 'publish' ||
    command === 'amend' ||
    (command === 'start' && (Boolean(options.reuseFromRunId) || options.buildOnly)),
  requireExistingTag: command === 'amend',
});
if (command === 'start') {
  validateLinuxArm64Runtime(options, Boolean(run('git', ['ls-remote', '--tags', 'origin', `refs/tags/v${version}`])));
}
if ((command === 'start' || command === 'amend') && requiresGpuiReferenceContract(options)) {
  run('node', ['tooling/release-gpui/verify-reference-contract.mjs'], { capture: false });
  /* Costs milliseconds here; discovering it on a runner cost 7.8.0 a 14-minute round. */
  run('node', ['tooling/release-gpui/check-ghostty-zig-pin.mjs'], { capture: false });
}
/*
 * CDXC:Release 2026-08-19:
 * `release:test` is ~11 seconds of pure source assertions on an already
 * installed tree, and it is the single gate most likely to trip on a normal
 * release: it pins CSS selectors, component source, and workflow shapes that
 * routine UI work moves underneath. 7.11.0 spent a whole dispatch plus a
 * redispatch discovering one stale selector assertion on the runner. Run it
 * here against the exact clean commit that is about to be dispatched.
 *
 * Deliberately outside the GPUI reference-contract branch above: that branch is
 * scope-conditional, while this suite also covers the planner, fingerprint, and
 * publisher scripts that every scope depends on.
 *
 * The frozen install, typecheck, and the remote `release:test` run stay
 * remote-only on purpose: those are the expensive gates, and prepare still runs
 * each of them exactly once.
 */
if ((command === 'start' || command === 'amend') && !options.skipLocalTests) {
  run('bun', ['run', 'release:test'], { capture: false });
}
const secrets = configuredSecrets();
validateRequiredSecrets(options, secrets);
const windowsSigned = resolveWindowsSigning(options, secrets);
const platforms = expectedPlatforms(options);
console.log(`Source: ${head}`);
console.log(`Platforms: ${platforms.join(', ')}`);
console.log(`Windows signing: ${windowsSigned ? 'enabled' : 'disabled'}`);

if (command === 'start') {
  previewPlan(version, options, windowsSigned, { required: options.dryRun });
  dispatch(
    'release-gpui.yml',
    {
      android: options.android,
      build_only: options.buildOnly,
      force_all: options.forceAll,
      force_products: options.forceProducts,
      reuse_from_run_id: options.reuseFromRunId,
      gxserver_linux_arm64: options.gxserverLinuxArm64,
      gxserver_linux_x64: options.gxserverLinuxX64,
      gxserver_wsl_windows_arm64: options.gxserverWslWindowsArm64,
      gxserver_wsl_windows_x64: options.gxserverWslWindowsX64,
      linux_appimage: options.linuxAppimage,
      linux_appimage_arm64: options.linuxAppimageArm64,
      linux_deb: options.linuxDeb,
      linux_deb_arm64: options.linuxDebArm64,
      linux_rpm: options.linuxRpm,
      linux_rpm_arm64: options.linuxRpmArm64,
      linux_tar: options.linuxTar,
      linux_tar_arm64: options.linuxTarArm64,
      macos: options.macos,
      prerelease: options.prerelease,
      sign_windows: windowsSigned,
      update_sparkle: options.updateSparkle,
      version,
      windows_arm64: options.windowsArm64,
      windows_x64: options.windowsX64,
    },
    options.dryRun
  );
} else if (command === 'amend') {
  console.log('Amend is opt-in; the runner expands pack/companion dependencies against the live provenance.');
  dispatch(
    'release-amend-existing.yml',
    {
      android: options.android,
      gxserver_linux_arm64: options.gxserverLinuxArm64,
      gxserver_linux_x64: options.gxserverLinuxX64,
      gxserver_wsl_windows_arm64: options.gxserverWslWindowsArm64,
      gxserver_wsl_windows_x64: options.gxserverWslWindowsX64,
      linux_deb: options.linuxDeb,
      linux_rpm: options.linuxRpm,
      linux_tar: options.linuxTar,
      macos: options.macos,
      sign_windows: windowsSigned,
      update_sparkle: options.updateSparkle,
      version,
      windows_arm64: options.windowsArm64,
      windows_x64: options.windowsX64,
    },
    options.dryRun
  );
} else {
  const sourceRun = JSON.parse(
    run('gh', ['run', 'view', options.sourceRunId, '--repo', repo, '--json', 'event,headSha,status,url,workflowName'])
  );
  if (sourceRun.status !== 'completed') throw new Error(`Source run is ${sourceRun.status}: ${sourceRun.url}`);
  if (!isAllowedReleaseWorkflowName(sourceRun.workflowName) || sourceRun.event !== 'workflow_dispatch') {
    throw new Error(`Source run is not a dispatched Ghostex release workflow: ${sourceRun.url}`);
  }
  run('git', ['merge-base', '--is-ancestor', sourceRun.headSha, head]);
  /*
   * Publish-only recovery reuses exactly what the source run resolved. Re-typing
   * the scope flags could silently publish a different artifact set than the one
   * that was built and verified, so the recorded plan is authoritative and an
   * unreadable plan is a hard stop.
   */
  const recordedPlan = recordedPlanFor(options.sourceRunId);
  if (recordedPlan.version !== version) {
    throw new Error(`Source run ${options.sourceRunId} released ${recordedPlan.version}, not ${version}`);
  }
  const recordedPlatforms = recordedPlan.expectedPlatforms;
  const scopeDrift = recordedPlatforms.filter((platform) => !platforms.includes(platform));
  const flagDrift = platforms.filter((platform) => !recordedPlatforms.includes(platform));
  if (scopeDrift.length > 0 || flagDrift.length > 0) {
    console.warn(
      `Scope flags describe ${platforms.join(', ')}, but the source run's plan resolved ` +
        `${recordedPlatforms.join(', ')}. Publishing the recorded plan.`
    );
  }
  const sourceArtifacts =
    JSON.parse(run('gh', ['api', `repos/${repo}/actions/runs/${options.sourceRunId}/artifacts?per_page=100`]))
      .artifacts ?? [];
  const availableArtifacts = new Set(
    sourceArtifacts.filter((artifact) => !artifact.expired).map((artifact) => artifact.name)
  );
  const missingArtifacts = recordedPlatforms
    .map((platform) => `release-${platform}`)
    .filter((name) => !availableArtifacts.has(name));
  if (missingArtifacts.length > 0) {
    throw new Error(`Source run is missing non-expired artifacts: ${missingArtifacts.join(', ')}`);
  }
  console.log(`Recorded plan: ${recordedPlatforms.join(', ')}`);
  /*
   * Not only the platform list: prerelease, Sparkle, and Windows signing also
   * come from the recorded plan. Re-typed flags describe what the operator
   * remembers; the plan describes what was built, signed, and gated.
   */
  const recovery = resolvePublishRecoveryInputs({
    flags: {
      prerelease: options.prerelease,
      updateSparkle: options.updateSparkle,
      windowsSigned,
    },
    plan: recordedPlan,
  });
  for (const conflict of recovery.conflicts) {
    console.warn(`Ignoring a re-typed flag that disagrees with the source run's plan — ${conflict}.`);
  }
  console.log(
    `Recorded switches: prerelease=${recovery.prerelease}, sparkle=${recovery.updateSparkle}, ` +
      `windows-signed=${recovery.windowsSigned}`
  );
  /*
   * The plan itself is deliberately not re-uploaded as a dispatch input: the
   * publisher downloads the source run's own `release-plan` artifact along with
   * every other artifact and reads it from there. One authoritative copy, no
   * ~40 KB workflow input, and no way for a hand-edited plan to describe a
   * different artifact set than the one being published.
   */
  if (options.stage) console.log(`Publishing stage ${options.stage} only (create-or-amend against the live release).`);
  dispatch(
    'release-gpui-publish.yml',
    {
      expected_platforms: recordedPlatforms.join(','),
      prerelease: recovery.prerelease,
      source_run_id: options.sourceRunId,
      stage: options.stage,
      update_sparkle: recovery.updateSparkle,
      version,
      windows_signed: recovery.windowsSigned,
    },
    options.dryRun
  );
}
