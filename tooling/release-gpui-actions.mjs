#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

import { resolvePublishRecoveryInputs } from './release-gpui/publish-provenance.mjs';
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
  --skip-macos
  --skip-linux | --skip-linux-deb | --skip-linux-rpm | --skip-linux-tar
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
  --windows-signing <auto|required|off>  Default: auto
  --source-run-id <id>                   Required by publish
  --dry-run

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
    dryRun: false,
    forceAll: false,
    forceProducts: '',
    skipLocalTests: false,
    reuseFromRunId: '',
    gxserverLinuxArm64: !amendDefaults,
    gxserverLinuxX64: !amendDefaults,
    gxserverWslWindowsArm64: !amendDefaults,
    gxserverWslWindowsX64: !amendDefaults,
    linuxDeb: !amendDefaults,
    linuxRpm: !amendDefaults,
    linuxTar: !amendDefaults,
    macos: !amendDefaults,
    prerelease: false,
    sourceRunId: '',
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
        linuxDeb: false,
        linuxRpm: false,
        linuxTar: false,
        macos: true,
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
      options.linuxDeb = options.linuxRpm = options.linuxTar = false;
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
    } else {
      throw new Error(`Unknown option: ${arg}`);
    }
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
    options.linuxDeb,
    options.linuxRpm,
    options.linuxTar,
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
  if (
    (options.linuxDeb || options.linuxRpm || options.linuxTar || options.windowsX64 || options.gxserverWslWindowsX64) &&
    !options.gxserverLinuxX64
  ) {
    throw new Error('Enabled x64 packages require gxserver Linux x64');
  }
  if ((options.windowsArm64 || options.gxserverWslWindowsArm64) && !options.gxserverLinuxArm64) {
    throw new Error('Enabled ARM64 packages require gxserver Linux ARM64');
  }
}

function requiresGpuiReferenceContract(options) {
  return (
    options.macos ||
    options.linuxDeb ||
    options.linuxRpm ||
    options.linuxTar ||
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

function resolveWindowsSigning(options, secrets) {
  if (!options.windowsX64 && !options.windowsArm64) return false;
  const names = ['WINDOWS_CODE_SIGN_PFX_BASE64', 'WINDOWS_CODE_SIGN_PFX_PASSWORD'];
  const available = names.every((name) => secrets.has(name));
  if (options.windowsSigning === 'required' && !available) {
    requireSecrets(secrets, 'Windows signing', names);
  }
  if (options.windowsSigning === 'off') return false;
  return available;
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
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
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
    linuxDeb: options.linuxDeb,
    linuxRpm: options.linuxRpm,
    linuxTar: options.linuxTar,
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
const head = validateLocalSource(version, {
  allowExistingTag: command === 'publish' || command === 'amend',
  requireExistingTag: command === 'amend',
});
if ((command === 'start' || command === 'amend') && requiresGpuiReferenceContract(options)) {
  run('node', ['tooling/release-gpui/verify-reference-contract.mjs'], { capture: false });
  /* Costs milliseconds here; discovering it on a runner cost 7.8.0 a 14-minute round. */
  run('node', ['tooling/release-gpui/check-ghostty-zig-pin.mjs'], { capture: false });
}
/*
 * CDXC:ReleaseLocalTestGate 2026-08-19:
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
      force_all: options.forceAll,
      force_products: options.forceProducts,
      reuse_from_run_id: options.reuseFromRunId,
      gxserver_linux_arm64: options.gxserverLinuxArm64,
      gxserver_linux_x64: options.gxserverLinuxX64,
      gxserver_wsl_windows_arm64: options.gxserverWslWindowsArm64,
      gxserver_wsl_windows_x64: options.gxserverWslWindowsX64,
      linux_deb: options.linuxDeb,
      linux_rpm: options.linuxRpm,
      linux_tar: options.linuxTar,
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
  dispatch(
    'release-gpui-publish.yml',
    {
      expected_platforms: recordedPlatforms.join(','),
      prerelease: recovery.prerelease,
      source_run_id: options.sourceRunId,
      update_sparkle: recovery.updateSparkle,
      version,
      windows_signed: recovery.windowsSigned,
    },
    options.dryRun
  );
}
