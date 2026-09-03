#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { lookup } from 'node:dns/promises';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { validateMacosAppBundle } from './validate-macos-app-bundle.mjs';

const repoRoot = path.resolve(new URL('..', import.meta.url).pathname);

const config = {
  githubRepo: 'maddada/Ghostex',
  tapRepo: 'https://github.com/maddada/homebrew-tap.git',
  caskPath: 'Casks/ghostex.rb',
  caskName: 'ghostex',
  appName: 'Ghostex',
  stagedAppName: 'ghostex.app',
  bundleId: 'com.madda.ghostex.host',
  signingIdentity: 'Developer ID Application: Mohamad Youssef (KTKP595G3B)',
  teamId: 'KTKP595G3B',
  notaryProfile: 'notarytool-profile',
  sparklePublicKey: 'AGWDPeMqfhmbjt8Pbk+VTC9fDfXAYq+cZoLGCYuGn70=',
  armFeed: 'appcast.xml',
  /*
   The GPUI app releases as a separate
   product (own bundle id, DMG asset, and Sparkle feed) behind the opt-in
   --gpui flag, riding the same version/tag as the macOS app. It signs with
   the same Developer ID certificate (user decision 2026-07-03) and the same
   Sparkle EdDSA key; GHOSTEX_GPUI_SIGN_IDENTITY overrides the identity.
   */
  gpuiAppName: 'Ghostex',
  gpuiStagedAppName: 'Ghostex.app',
  gpuiBundleId: 'com.madda.ghostex.gpui',
  gpuiFeed: 'appcast-gpui.xml',
  installCommand: 'brew install --cask maddada/tap/ghostex',
  androidSigningEnvFile: '/Users/madda/.config/ghostex/android/release-signing.env',
  androidApkAssetName: 'ghostex-android.apk',
};

/*
 CDXC:ReleaseAutomation 2026-05-29-19:12:
 Public releases can spend many minutes in Xcode builds and Apple notarization.
 Use explicit step timeouts and heartbeat logs so release operators see progress
 instead of waiting on a silent shell for twenty-plus minutes.
 */
const releaseTimeouts = {
  typecheckMs: 8 * 60 * 1000,
  testMs: 12 * 60 * 1000,
  buildArchMs: 50 * 60 * 1000,
  notaryArchMs: 45 * 60 * 1000,
  brewFetchMs: 15 * 60 * 1000,
  androidMs: 45 * 60 * 1000,
  overallMs: 150 * 60 * 1000,
  heartbeatMs: 60 * 1000,
};

/*
 CDXC:MacRelease 2026-06-10-09:47:
 Future Ghostex macOS releases are Apple Silicon only. Keep the arm64 build,
 signing, notarization, Sparkle, GitHub, and Homebrew path intact, but stop
 generating new Intel DMGs or appcast entries. Existing v4.1.0 and older Intel
 tags, GitHub assets, appcast history, and Homebrew git history must remain
 untouched.
 */
const releaseArchitectures = [
  {
    arch: 'arm64',
    brewArch: 'arm',
    feed: config.armFeed,
    feedUrl: 'https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml',
  },
];

function gpuiReleaseEntry(version) {
  return {
    arch: 'arm64',
    kind: 'gpui',
    appName: config.gpuiAppName,
    stagedAppName: config.gpuiStagedAppName,
    bundleId: config.gpuiBundleId,
    dmgName: `ghostex-gpui-${version}-arm64.dmg`,
    volumeName: 'ghostex-gpui',
    releaseNotesTitle: 'Ghostex',
    feed: config.gpuiFeed,
    feedUrl: `https://raw.githubusercontent.com/${config.githubRepo}/main/${config.gpuiFeed}`,
  };
}

/*
 CDXC:RemoteUbuntuPackaging 2026-06-29-19:45:
 Public Ghostex releases must bundle first-run server packages for Ubuntu
 x64 and arm64. Validate the deterministic Linux CI outputs before mutating
 release metadata and force native app packaging to stage both resources so the
 remote installer cannot silently ship without Ubuntu x64 support.
 */
const remoteGxserverLinuxPackageConfigs = [
  {
    arch: 'x64',
    defaultPackageDir: path.join(repoRoot, 'build', 'remote-gxserver-linux', 'x64', 'package'),
    packageLabel: 'LINUX_X64',
    resourceName: 'gxserver-linux-x64',
  },
  {
    arch: 'arm64',
    defaultPackageDir: path.join(repoRoot, 'build', 'remote-gxserver-linux', 'arm64', 'package'),
    packageLabel: 'LINUX_ARM64',
    resourceName: 'gxserver-linux-arm64',
  },
];

/*
 CDXC:RemoteMinimalDeps 2026-07-13:
 The Linux remote package no longer ships portless (macOS launchd-only), the
 npm-style package.json manifest, or dist/protocol exports; nothing on the
 remote host consumes them and version identity lives in build-identity.json.
 */
const remoteGxserverLinuxRequiredPackageResources = ['bin/gxserver', 'bin/zmx', 'bin/ghostex', 'build-identity.json'];

/*
 CDXC:OnDemandAssets 2026-07-02-14:10:
 Public releases stop embedding the two Ubuntu remote gxserver payloads inside
 the DMG. The app build stages a sealed checksum manifest instead, and the
 release publishes the two tarballs as version-pinned GitHub release assets the
 app downloads on first use. Remote installs keep the Mac-then-scp flow.
 */
const onDemandAssetNames = ['gxserver-linux-x64.tar.gz', 'gxserver-linux-arm64.tar.gz'];

/*
 CDXC:ReleaseAutomation 2026-07-02-14:10:
 The release runs as named, individually resumable phases. Each phase records
 its outputs and duration in build/release-state/v<version>.json so a failed
 release can continue with --from <phase> instead of improvised recovery, and
 so every run ends with a phase timing table.
 */
const releasePhaseNames = [
  'preflight',
  'prepare-remote-linux',
  'publish-macos',
  'publish-android',
  'publish-homebrew',
  'verify-live',
];

class ReleaseError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ReleaseError';
  }
}

function usage() {
  return `
Usage:
  bun run release:local -- <version> [options]
  node tooling/release-ghostex.mjs <version> [options]

Options:
  --with-tests        Run release-safe Vitest checks before building.
  --skip-typecheck   Skip bun run typecheck.
  --skip-brew-fetch  Skip final brew fetch checks.
  --skip-sparkle     Do not update or validate Sparkle appcasts.
  --skip-android     Do not build/upload the signed Android APK.
  --gpui             Retired 2026-08-23: fails fast with a ReleaseError. Used to
                     also build, sign, notarize, and publish the GPUI app
                     (ghostex-gpui-<version>-arm64.dmg + appcast-gpui.xml) on the
                     same version/tag. The current pipeline is tooling/release-gpui.
  --resume [version] Resume an already-created release and finish missing Homebrew/Android/notes steps.
  --from <phase>     Resume from a phase using build/release-state/v<version>.json.
                     Phases: preflight, prepare-remote-linux, publish-macos,
                     publish-android, publish-homebrew, verify-live.
  --only <phase>     Run a single phase, trusting recorded state for the rest.
  --github-prerelease
                     Mark the GitHub release as a prerelease.
  --release-branch <branch>
                     Branch that receives the release commit. Defaults to main.
  --no-push          Commit release metadata but do not push, tag, publish GitHub, or update Homebrew.
  --no-terminal-delegate
                     Fail instead of handing off to Terminal.app when the agent shell cannot see signing/notary credentials.
  --help             Show this help.

Expected state:
  Run this only after the agent/user has split-committed feature changes,
  updated CHANGELOG.md (and any local product notes you keep outside the repo), and pushed the release branch.

Timeouts and progress:
  Build steps log heartbeat updates about every minute.
  arm64 build timeout: 50 minutes.
  arm64 notarization timeout: 45 minutes.
  Overall release timeout: 150 minutes.
`;
}

/*
CDXC:LocalDocs 2026-08-09:
Product and review documentation is local-only under docs/ (gitignored) and is
no longer a release-branch requirement. CHANGELOG.md remains the tracked release
notes surface in the public repository.
*/

/*
CDXC:BetaDistribution 2026-06-05-22:26:
Nightly beta releases must be installable from GitHub Releases and Homebrew without
advancing the Sparkle feeds that production users poll for automatic updates.
Keep beta controls explicit so the public release path still updates appcasts by
default while prerelease tags can target nightly and skip Sparkle entirely.
*/
function parseArgs(argv) {
  const options = {
    withTests: false,
    skipTypecheck: false,
    skipBrewFetch: false,
    skipSparkle: false,
    skipAndroid: false,
    gpui: false,
    resume: false,
    fromPhase: null,
    onlyPhase: null,
    githubPrerelease: false,
    releaseBranch: 'main',
    noPush: false,
    noTerminalDelegate: false,
  };
  const positional = [];

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (arg === '--with-tests') {
      options.withTests = true;
    } else if (arg === '--skip-typecheck') {
      options.skipTypecheck = true;
    } else if (arg === '--skip-brew-fetch') {
      options.skipBrewFetch = true;
    } else if (arg === '--skip-sparkle') {
      options.skipSparkle = true;
    } else if (arg === '--skip-android') {
      options.skipAndroid = true;
    } else if (arg === '--gpui') {
      options.gpui = true;
    } else if (arg === '--resume') {
      options.resume = true;
      const maybeVersion = argv[index + 1]?.trim();
      if (maybeVersion && !maybeVersion.startsWith('-')) {
        positional.push(maybeVersion);
        index += 1;
      }
    } else if (arg === '--from' || arg === '--only') {
      const phase = argv[index + 1]?.trim();
      if (!phase || !releasePhaseNames.includes(phase)) {
        throw new ReleaseError(`${arg} requires one of: ${releasePhaseNames.join(', ')}`);
      }
      if (arg === '--from') {
        options.fromPhase = phase;
      } else {
        options.onlyPhase = phase;
      }
      index += 1;
    } else if (arg === '--github-prerelease') {
      options.githubPrerelease = true;
    } else if (arg === '--release-branch') {
      const branch = argv[index + 1]?.trim();
      if (!branch || branch.startsWith('-')) {
        throw new ReleaseError('--release-branch requires a branch name.');
      }
      options.releaseBranch = branch;
      index += 1;
    } else if (arg === '--no-push') {
      options.noPush = true;
    } else if (arg === '--no-terminal-delegate') {
      options.noTerminalDelegate = true;
    } else if (arg.startsWith('-')) {
      throw new ReleaseError(`Unknown option: ${arg}`);
    } else {
      positional.push(arg);
    }
  }

  if (options.help) {
    return { ...options, version: null };
  }

  if (positional.length !== 1) {
    throw new ReleaseError('Pass exactly one version, for example 3.9.2.');
  }

  const version = positional[0];
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new ReleaseError(`Version must be semver-like x.y.z or x.y.z-prerelease. Received: ${version}`);
  }

  return { ...options, version };
}

function releaseBuildVersion(version) {
  const [major, minor, patch] = version
    .split('-')[0]
    .split('.')
    .map((part) => Number.parseInt(part, 10));
  return major * 10000 + minor * 100 + patch;
}

function isPrereleaseVersion(version) {
  return version.includes('-');
}

function missingRemoteGxserverLinuxPackageResources(packageDir, exists = existsSync) {
  return remoteGxserverLinuxRequiredPackageResources.filter((relativePath) => {
    return !exists(path.join(packageDir, relativePath));
  });
}

function timestampForComment(date = new Date()) {
  const pad = (value) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}-${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function appleScriptString(value) {
  return `"${String(value).replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

function logStep(message) {
  console.log(`\n==> ${message}`);
}

function run(command, options = {}) {
  const cwd = options.cwd ?? repoRoot;
  const env = { ...process.env, ...(options.env ?? {}) };
  const stdio = options.stdio ?? 'inherit';
  const timeoutMs = options.timeoutMs;

  console.log(`$ ${command}`);

  return new Promise((resolve, reject) => {
    let settled = false;
    const child = spawn(command, {
      cwd,
      env,
      shell: true,
      stdio,
    });
    const timeout = timeoutMs
      ? setTimeout(() => {
          if (settled) {
            return;
          }
          settled = true;
          child.kill('SIGTERM');
          reject(new ReleaseError(`Command timed out after ${timeoutMs}ms: ${command}`));
        }, timeoutMs)
      : null;

    let stdout = '';
    let stderr = '';

    if (stdio === 'pipe') {
      child.stdout.on('data', (chunk) => {
        stdout += chunk.toString();
      });
      child.stderr.on('data', (chunk) => {
        stderr += chunk.toString();
      });
    }

    child.on('error', (error) => {
      if (settled) {
        return;
      }
      settled = true;
      if (timeout) {
        clearTimeout(timeout);
      }
      reject(error);
    });
    child.on('close', (code) => {
      if (settled) {
        return;
      }
      settled = true;
      if (timeout) {
        clearTimeout(timeout);
      }
      if (code === 0) {
        resolve({ stdout, stderr });
      } else {
        const detail = stderr || stdout;
        reject(new ReleaseError(`Command failed (${code}): ${command}${detail ? `\n${detail}` : ''}`));
      }
    });
  });
}

async function capture(command, options = {}) {
  const result = await run(command, { ...options, stdio: 'pipe' });
  return result.stdout.trim();
}

function formatElapsedSeconds(startedAt) {
  const elapsedSeconds = Math.max(0, Math.round((Date.now() - startedAt) / 1000));
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

async function runWithHeartbeat(command, options = {}) {
  const {
    label = 'command',
    timeoutMs = releaseTimeouts.notaryArchMs,
    heartbeatMs = releaseTimeouts.heartbeatMs,
    cwd = repoRoot,
    env = process.env,
  } = options;
  const startedAt = Date.now();
  const timeoutMinutes = Math.max(1, Math.round(timeoutMs / 60_000));
  console.log(`${label}: starting (timeout ${timeoutMinutes} min)`);

  return new Promise((resolve, reject) => {
    let settled = false;
    let stdout = '';
    let stderr = '';
    const child = spawn(command, {
      cwd,
      env: { ...process.env, ...env },
      shell: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const heartbeat = setInterval(() => {
      console.log(`${label}: still running (${formatElapsedSeconds(startedAt)} elapsed)...`);
    }, heartbeatMs);
    const timeout = setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      child.kill('SIGTERM');
      clearInterval(heartbeat);
      reject(
        new ReleaseError(`${label} timed out after ${formatElapsedSeconds(startedAt)} (${timeoutMinutes} min limit).`)
      );
    }, timeoutMs);

    child.stdout.on('data', (chunk) => {
      const text = chunk.toString();
      stdout += text;
      process.stdout.write(text);
    });
    child.stderr.on('data', (chunk) => {
      const text = chunk.toString();
      stderr += text;
      process.stderr.write(text);
    });
    child.on('error', (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearInterval(heartbeat);
      clearTimeout(timeout);
      reject(error);
    });
    child.on('close', (code) => {
      if (settled) {
        return;
      }
      settled = true;
      clearInterval(heartbeat);
      clearTimeout(timeout);
      if (code === 0) {
        console.log(`${label}: finished in ${formatElapsedSeconds(startedAt)}`);
        resolve(stdout.trim());
      } else {
        const detail = stderr || stdout;
        reject(
          new ReleaseError(
            `${label} failed (${code}) after ${formatElapsedSeconds(startedAt)}: ${command}${detail ? `\n${detail}` : ''}`
          )
        );
      }
    });
  });
}

function assertReleaseWithinOverallBudget(startedAt, stepLabel) {
  const elapsedMs = Date.now() - startedAt;
  if (elapsedMs > releaseTimeouts.overallMs) {
    throw new ReleaseError(
      `Release exceeded the overall ${Math.round(releaseTimeouts.overallMs / 60_000)} minute budget during ${stepLabel}.`
    );
  }
}

function releaseStatePath(version) {
  return path.join(repoRoot, 'build', 'release-state', `v${version}.json`);
}

async function loadReleaseState(version) {
  try {
    const state = JSON.parse(await readFile(releaseStatePath(version), 'utf8'));
    return state && typeof state === 'object' && state.version === version ? state : null;
  } catch {
    return null;
  }
}

async function saveReleaseState(state) {
  const statePath = releaseStatePath(state.version);
  await mkdir(path.dirname(statePath), { recursive: true });
  await writeFile(statePath, `${JSON.stringify(state, null, 2)}\n`);
}

async function loadOrCreateReleaseState(version, buildVersion, options) {
  const existing = await loadReleaseState(version);
  if (options.fromPhase || options.onlyPhase) {
    if (!existing) {
      throw new ReleaseError(
        `--from/--only need an existing state file at ${releaseStatePath(version)}, but none was found for ${version}.`
      );
    }
    const targetPhase = options.fromPhase ?? options.onlyPhase;
    const targetIndex = releasePhaseNames.indexOf(targetPhase);
    for (const phaseName of releasePhaseNames.slice(0, targetIndex)) {
      if (existing.phases?.[phaseName]?.status !== 'completed') {
        throw new ReleaseError(
          `Cannot resume from ${targetPhase}: earlier phase ${phaseName} is not recorded as completed in ${releaseStatePath(version)}.`
        );
      }
    }
    // Re-run the target phase (and later phases for --from) even if a prior
    // attempt recorded them, so a failed publication can be repaired.
    const resetFrom = options.onlyPhase ? [targetPhase] : releasePhaseNames.slice(targetIndex);
    for (const phaseName of resetFrom) {
      if (existing.phases?.[phaseName]) {
        delete existing.phases[phaseName];
      }
    }
    return existing;
  }
  if (existing && Object.values(existing.phases ?? {}).some((phase) => phase.status === 'completed')) {
    console.warn(
      `Found prior release state at ${releaseStatePath(version)}; completed phases will be skipped. Use --from <phase> to redo a phase.`
    );
    return existing;
  }
  return { buildVersion, phases: {}, startedAt: new Date().toISOString(), version };
}

async function runReleasePhase(state, name, options, fn) {
  const recorded = state.phases[name];
  if (recorded?.status === 'completed') {
    logStep(`Phase ${name} (skipped: already completed in ${formatPhaseDuration(recorded.durationMs)})`);
    return recorded.outputs ?? {};
  }
  if (options.onlyPhase && options.onlyPhase !== name) {
    if (recorded?.status !== 'completed') {
      logStep(`Phase ${name} (skipped by --only ${options.onlyPhase})`);
    }
    return recorded?.outputs ?? {};
  }
  logStep(`Phase ${name}`);
  const startedAt = Date.now();
  state.phases[name] = { startedAt: new Date(startedAt).toISOString(), status: 'running' };
  await saveReleaseState(state);
  try {
    const outputs = (await fn()) ?? {};
    state.phases[name] = {
      durationMs: Date.now() - startedAt,
      outputs,
      startedAt: new Date(startedAt).toISOString(),
      status: 'completed',
    };
    await saveReleaseState(state);
    console.log(`Phase ${name} finished in ${formatPhaseDuration(Date.now() - startedAt)}.`);
    return outputs;
  } catch (error) {
    state.phases[name] = {
      durationMs: Date.now() - startedAt,
      error: String(error?.message ?? error).slice(0, 4000),
      startedAt: new Date(startedAt).toISOString(),
      status: 'failed',
    };
    await saveReleaseState(state);
    throw error;
  }
}

function formatPhaseDuration(durationMs) {
  if (!Number.isFinite(durationMs)) {
    return '-';
  }
  const totalSeconds = Math.round(durationMs / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

function printPhaseTimingSummary(state) {
  const rows = releasePhaseNames.map((name) => ({ name, phase: state.phases[name] })).filter((row) => row.phase);
  if (rows.length === 0) {
    return;
  }
  logStep('Phase timing summary');
  let totalMs = 0;
  for (const { name, phase } of rows) {
    totalMs += phase.durationMs ?? 0;
    console.log(
      `  ${name.padEnd(22)} ${formatPhaseDuration(phase.durationMs).padStart(9)}  ${phase.status}${phase.error ? ` (${phase.error.split('\n')[0].slice(0, 100)})` : ''}`
    );
  }
  console.log(`  ${'total'.padEnd(22)} ${formatPhaseDuration(totalMs).padStart(9)}`);
  console.log(`  State file: ${releaseStatePath(state.version)}`);
}

function isGitNetworkResolutionError(error) {
  const message = String(error?.message ?? error);
  return /Could not resolve host|unable to access/i.test(message);
}

function isHomebrewHostToolchainVersionError(error) {
  const message = String(error?.message ?? error);
  return /Your Xcode .*too outdated/i.test(message) || /Your Command Line Tools are too outdated/i.test(message);
}

async function runOptionalHomebrewHostValidation(command, options = {}) {
  /*
   * CDXC:ReleaseAutomation 2026-06-16-20:32:
   * Homebrew can reject local audit/style/fetch commands on macOS beta hosts
   * when Xcode/CLT lags Homebrew's newest minimum, even though the Ghostex cask
   * can still be rendered, syntax-checked, pushed, and validated from the tap.
   * Treat only that host-toolchain diagnostic as a skippable local validation
   * gap; cask syntax, canonical cask validation, git push, GitHub assets, and
   * raw live-cask validation remain mandatory.
   *
   * CDXC:ReleaseAutomation 2026-06-16-20:39:
   * Homebrew host-toolchain diagnostics are written to child stderr. Capture
   * optional Homebrew validation output so the classifier can distinguish that
   * local host issue from real cask failures before rethrowing.
   */
  try {
    const result = await run(command, { ...options, stdio: 'pipe' });
    if (result.stdout.trim()) {
      console.log(result.stdout.trim());
    }
    if (result.stderr.trim()) {
      console.error(result.stderr.trim());
    }
    return true;
  } catch (error) {
    if (!isHomebrewHostToolchainVersionError(error)) {
      throw error;
    }
    console.warn(
      [
        "Warning: skipping local Homebrew validation because this host's Xcode/CLT is below Homebrew's current minimum.",
        `Skipped: ${command}`,
      ].join('\n')
    );
    return false;
  }
}

async function readGitHubHttpsCredentials() {
  const creds = await capture("printf 'protocol=https\\nhost=github.com\\n\\n' | git credential fill");
  const username = creds.match(/^username=(.+)$/m)?.[1];
  const password = creds.match(/^password=(.+)$/m)?.[1];
  if (!username || !password) {
    throw new ReleaseError('Could not read GitHub HTTPS credentials for git network commands.');
  }
  return { username, password };
}

/**
 * CDXC:Distribution 2026-05-23-12:55:
 * Some release environments resolve github.com for curl but not for git's libcurl.
 * Retry origin fetch/push/ls-remote through a Host-header HTTPS URL when DNS fails.
 */
async function resolveGitHubAddress() {
  try {
    return (await lookup('github.com', { family: 4 })).address;
  } catch {
    const output = await capture("nslookup github.com 2>/dev/null | awk '/^Address: / { print $2; exit }'");
    if (!output) {
      throw new ReleaseError('Could not resolve github.com for git network commands.');
    }
    return output;
  }
}

async function githubHttpsRemoteUrl(repoPath = config.githubRepo) {
  const { username, password } = await readGitHubHttpsCredentials();
  const address = await resolveGitHubAddress();
  return `https://${username}:${encodeURIComponent(password)}@${address}/${repoPath}.git`;
}

async function runGitNetwork(command, options = {}) {
  try {
    await run(command, options);
    return;
  } catch (error) {
    if (!isGitNetworkResolutionError(error)) {
      throw error;
    }
  }

  const remoteUrl = await githubHttpsRemoteUrl();
  const translated = command.replace(/\borigin\b/g, shellQuote(remoteUrl));
  await run(
    `git -c http.sslVerify=false -c http.extraHeader=${shellQuote('Host: github.com')} ${translated.replace(/^git\s+/, '')}`,
    options
  );
}

async function ensureGhAuth() {
  const { password } = await readGitHubHttpsCredentials();
  if (!process.env.GH_TOKEN) {
    process.env.GH_TOKEN = password;
  }
  if (!process.env.GITHUB_TOKEN) {
    process.env.GITHUB_TOKEN = password;
  }
}

/**
 * CDXC:Distribution 2026-05-23-13:25:
 * Agent shells often inject stale GH_TOKEN values. Prefer the user's real gh
 * login-session auth before falling back to git credential fill for git push.
 */
async function ensureGhAuthForRelease() {
  try {
    await run('env -u GH_TOKEN -u GITHUB_TOKEN gh auth status -h github.com');
    return;
  } catch {
    await run('env -u GH_TOKEN -u GITHUB_TOKEN gh auth setup-git -h github.com || true');
  }
  await ensureGhAuth();
}

async function recoverKeychainVisibility() {
  logStep('Recover keychain visibility for signing');
  const keychains = await releaseKeychainSearchList();
  if (keychains.length > 0) {
    await run(`security list-keychains -d user -s ${keychains.map(shellQuote).join(' ')} 2>/dev/null || true`);
  }
  const loginKeychain = path.join(process.env.HOME ?? '', 'Library/Keychains/login.keychain-db');
  await run(`security default-keychain -d user -s ${shellQuote(loginKeychain)} 2>/dev/null || true`);
  if (existsSync(loginKeychain)) {
    await run(`security unlock-keychain ${shellQuote(loginKeychain)} 2>/dev/null`, { timeoutMs: 3000 }).catch(() => {});
  }
}

async function configuredUserKeychains() {
  try {
    const output = await capture('security list-keychains -d user 2>/dev/null');
    return output
      .split('\n')
      .map((line) => line.trim().replace(/^"|"$/g, ''))
      .filter(Boolean);
  } catch {
    return [];
  }
}

async function releaseKeychainSearchList() {
  const home = process.env.HOME ?? '';
  const candidates = [
    ...(await configuredUserKeychains()),
    path.join(home, 'Library/Keychains/login.keychain-db'),
    path.join(home, 'Library/Keychains/iCloud.keychain-db'),
    '/Library/Keychains/System.keychain',
  ];
  return [...new Set(candidates.filter((keychain) => keychain && existsSync(keychain)))];
}

function releaseSigningIdentity() {
  return process.env.GHOSTEX_CODE_SIGN_IDENTITY?.trim() || config.signingIdentity;
}

function gpuiReleaseSigningIdentity() {
  return process.env.GHOSTEX_GPUI_SIGN_IDENTITY?.trim() || releaseSigningIdentity();
}

/**
 * CDXC:Distribution 2026-05-23-13:10:
 * Release builds must use the Developer ID identity from ghostex-release-to-brew,
 * not ad-hoc detection, and preflight should fail with actionable keychain guidance
 * when the login keychain is locked or the certificate is missing.
 */
async function listCodeSigningIdentities() {
  /*
   CDXC:Distribution 2026-05-23-16:01:
   Developer ID certificates are not guaranteed to live only in login.keychain-db.
   Release preflight must inspect the aggregate keychain view and configured user keychains before deciding signing is unavailable.
   */
  const chunks = [];
  try {
    chunks.push(`== aggregate ==\n${await capture('security find-identity -v -p codesigning 2>/dev/null')}`);
  } catch (error) {
    chunks.push(`== aggregate failed ==\n${String(error.message ?? error)}`);
  }
  for (const keychain of await releaseKeychainSearchList()) {
    try {
      chunks.push(
        `== ${keychain} ==\n${await capture(`security find-identity -v -p codesigning ${shellQuote(keychain)} 2>/dev/null`)}`
      );
    } catch (error) {
      chunks.push(`== ${keychain} failed ==\n${String(error.message ?? error)}`);
    }
  }
  return chunks.join('\n');
}

function signingIdentityIsVisible(identities) {
  return Boolean(matchingSigningIdentityLine(identities));
}

function matchingSigningIdentityLine(identities) {
  const identity = releaseSigningIdentity();
  return identities.split('\n').find((line) => line.includes(`"${identity}"`) || line.includes(identity));
}

async function ensureSigningIdentity() {
  await recoverKeychainVisibility();
  const identity = releaseSigningIdentity();
  const identities = await listCodeSigningIdentities();
  if (!signingIdentityIsVisible(identities)) {
    throw new ReleaseError(
      [
        `No valid code signing identity found for release: ${identity}`,
        '',
        identities.trim() || '(security find-identity returned no valid identities)',
      ].join('\n')
    );
  }
  await ensureSigningIdentityCanSign(identity);
}

/**
 * CDXC:Distribution 2026-05-25-18:22:
 * `security find-identity` can see a Developer ID certificate even when the
 * embedded agent shell cannot use the private key. Probe `codesign` directly so
 * the release delegates to Terminal.app before a long build fails on CEF files
 * with errSecInternalComponent.
 */
async function ensureSigningIdentityCanSign(identity) {
  const probeDir = await mkdtemp(path.join(tmpdir(), 'ghostex-codesign-probe-'));
  const probePath = path.join(probeDir, 'probe.sh');
  try {
    await writeFile(probePath, '#!/bin/sh\nexit 0\n');
    await run(`chmod +x ${shellQuote(probePath)}`);
    await run(`/usr/bin/codesign --force --sign ${shellQuote(identity)} --timestamp=none ${shellQuote(probePath)}`);
  } finally {
    await rm(probeDir, { recursive: true, force: true });
  }
}

function terminalReleasePaths(version) {
  return {
    logPath: `/tmp/ghostex-release-${version}.log`,
    runnerPath: `/tmp/ghostex-release-${version}.command`,
    startedPath: `/tmp/ghostex-release-${version}.started`,
    donePath: `/tmp/ghostex-release-${version}.done`,
    exitPath: `/tmp/ghostex-release-${version}.exit`,
  };
}

function releaseCommandArgs(version, options, extraArgs = []) {
  const args = [version, ...extraArgs];
  if (options.withTests) {
    args.push('--with-tests');
  }
  if (options.skipTypecheck) {
    args.push('--skip-typecheck');
  }
  if (options.skipBrewFetch) {
    args.push('--skip-brew-fetch');
  }
  if (options.skipSparkle) {
    args.push('--skip-sparkle');
  }
  if (options.skipAndroid) {
    args.push('--skip-android');
  }
  if (options.gpui) {
    args.push('--gpui');
  }
  if (options.resume) {
    args.push('--resume');
  }
  if (options.fromPhase) {
    args.push('--from', options.fromPhase);
  }
  if (options.onlyPhase) {
    args.push('--only', options.onlyPhase);
  }
  if (options.githubPrerelease) {
    args.push('--github-prerelease');
  }
  if (options.releaseBranch !== 'main') {
    args.push('--release-branch', options.releaseBranch);
  }
  if (options.noPush) {
    args.push('--no-push');
  }
  return args.map(shellQuote).join(' ');
}

async function writeTerminalReleaseRunner(version, options) {
  const { logPath, runnerPath, startedPath, donePath, exitPath } = terminalReleasePaths(version);
  const identity = releaseSigningIdentity();
  await rm(logPath, { force: true });
  await rm(startedPath, { force: true });
  await rm(donePath, { force: true });
  await rm(exitPath, { force: true });
  await writeFile(
    logPath,
    [
      `Ghostex release ${version} prepared for Terminal.app at ${new Date().toISOString()}`,
      `Runner: ${runnerPath}`,
      '',
    ].join('\n')
  );
  const runner = `#!/bin/zsh -l
set -uo pipefail
cd ${shellQuote(repoRoot)}
unset GH_TOKEN GITHUB_TOKEN
export GHOSTEX_CODE_SIGN_IDENTITY=${shellQuote(identity)}
export GHOSTEX_CODE_SIGN_TIMESTAMP_FLAG=--timestamp
export GHOSTEX_RELEASE_TERMINAL_DELEGATED=1
exec > >(tee -a ${shellQuote(logPath)}) 2>&1
echo "Ghostex release ${version} Terminal runner started at $(date)"
touch ${shellQuote(startedPath)}
release_status=0
{
  security list-keychains -d user -s "$HOME/Library/Keychains/login.keychain-db" "$HOME/Library/Keychains/iCloud.keychain-db" /Library/Keychains/System.keychain 2>/dev/null || true
  security default-keychain -d user -s "$HOME/Library/Keychains/login.keychain-db" 2>/dev/null || true
  perl -e 'alarm 3; exec @ARGV' security unlock-keychain "$HOME/Library/Keychains/login.keychain-db" 2>/dev/null || true
  security find-identity -v -p codesigning | rg ${shellQuote('Developer ID Application: Mohamad Youssef \\(KTKP595G3B\\)')} || true
  xcrun notarytool history --keychain-profile ${shellQuote(config.notaryProfile)} | head -n 8
  gh auth status -h github.com
  bun run release:local -- ${releaseCommandArgs(version, options, ['--no-terminal-delegate'])}
} || {
  release_status=$?
}
if [ "$release_status" -eq 0 ]; then
  echo "Ghostex release ${version} finished at $(date)"
else
  echo "Ghostex release ${version} failed with status $release_status at $(date)"
fi
echo "$release_status" > ${shellQuote(exitPath)}
touch ${shellQuote(donePath)}
exit "$release_status"
`;
  await writeFile(runnerPath, runner, { mode: 0o755 });
  return { logPath, runnerPath, startedPath, donePath, exitPath };
}

async function launchTerminalReleaseRunner(runnerPath) {
  logStep('Launch release through login-session Terminal');
  /**
   * CDXC:Distribution 2026-05-23-14:05:
   * AppleScript's `do script` breaks when generated through osascript -e because
   * `script` is reserved. Opening the .command file in Terminal.app is the
   * reliable login-session handoff for release builds.
   */
  const attempts = [
    `open -a /System/Applications/Utilities/Terminal.app ${shellQuote(runnerPath)}`,
    `open -a Terminal ${shellQuote(runnerPath)}`,
    '/Applications/OpenInTerminal.app/Contents/MacOS/OpenInTerminal-Lite',
    '/Applications/OpenInTerminal.app/Contents/MacOS/OpenInTerminal',
  ].map((command) =>
    command.endsWith('OpenInTerminal-Lite') || command.endsWith('OpenInTerminal')
      ? `${shellQuote(command)} ${shellQuote(runnerPath)}`
      : command
  );

  let lastError;
  for (const attempt of attempts) {
    if (attempt.includes('OpenInTerminal') && !existsSync(attempt.split(' ')[0].replaceAll("'", ''))) {
      continue;
    }
    try {
      await run(attempt);
      return;
    } catch (error) {
      lastError = error;
    }
  }
  console.warn('Could not open Terminal.app from this environment; running the login-shell release runner directly.');
  await run(`nohup /bin/zsh -l ${shellQuote(runnerPath)} </dev/null >/dev/null 2>&1 &`);
}

async function waitForReleaseStart(startedPath, logPath, runnerPath, timeoutMs = 60 * 1000) {
  const startedAt = Date.now();
  while (!existsSync(startedPath)) {
    if (Date.now() - startedAt > timeoutMs) {
      const log = existsSync(logPath) ? await readFile(logPath, 'utf8') : '(log file was not created)';
      throw new ReleaseError(
        [
          'Timed out waiting for Terminal.app to start the release runner.',
          `Runner: ${runnerPath}`,
          `Log: ${logPath}`,
          '',
          log.trim(),
        ].join('\n')
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

async function monitorTerminalReleaseLog(version, paths) {
  const { logPath, runnerPath, startedPath, donePath, exitPath } = paths;
  logStep(`Monitor Terminal release log (${logPath})`);
  await waitForReleaseStart(startedPath, logPath, runnerPath);
  let lastLength = 0;
  let stableFor = 0;
  while (true) {
    const log = existsSync(logPath) ? await readFile(logPath, 'utf8') : '';
    const nextChunk = log.slice(lastLength);
    if (nextChunk) {
      process.stdout.write(nextChunk);
      lastLength = log.length;
    }
    if (existsSync(donePath)) {
      const exitCode = existsSync(exitPath) ? (await readFile(exitPath, 'utf8')).trim() : 'unknown';
      if (exitCode === '0') {
        return;
      }
      throw new ReleaseError(log.trim() || `Terminal release failed with status ${exitCode}. See ${logPath}`);
    }
    stableFor = nextChunk ? 0 : stableFor + 1;
    if (stableFor >= 180) {
      throw new ReleaseError(`Terminal release appears stalled. See ${logPath}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 5000));
  }
}

async function agentShellCredentialsReady() {
  try {
    await ensureSigningIdentity();
    await ensureNotaryProfile();
  } catch {
    return false;
  }
  return true;
}

async function delegateReleaseToTerminal(version, options) {
  const paths = await writeTerminalReleaseRunner(version, options);
  await launchTerminalReleaseRunner(paths.runnerPath);
  await monitorTerminalReleaseLog(version, paths);
}

async function ensureNotaryProfile() {
  try {
    await run(
      `/bin/zsh -lc ${shellQuote(`set -o pipefail; xcrun notarytool history --keychain-profile ${shellQuote(config.notaryProfile)} | head -n 8`)}`
    );
  } catch (error) {
    throw new ReleaseError(
      [
        `Notary profile ${config.notaryProfile} is unavailable.`,
        '',
        'Fix:',
        '1. Store Apple notarization credentials:',
        '   xcrun notarytool store-credentials notarytool-profile --key <AuthKey.p8> --key-id <KEY_ID> --issuer <ISSUER_ID>',
        '2. Unlock the login keychain, then re-run bun run release:local -- <version>.',
        '',
        String(error.message ?? error),
      ].join('\n')
    );
  }
}

async function ensureCleanWorktree() {
  const status = await capture('git status --porcelain --untracked-files=all');
  if (status) {
    throw new ReleaseError(
      ['Working tree is not clean. Commit the agent/user changes before running the release script.', '', status].join(
        '\n'
      )
    );
  }
}

async function ensureReleaseBranchSynced(releaseBranch) {
  const branch = await capture('git branch --show-current');
  const head = await capture('git rev-parse HEAD');
  await runGitNetwork(`git fetch origin ${shellQuote(releaseBranch)} --tags`);
  const originBranch = await capture(`git rev-parse ${shellQuote(`origin/${releaseBranch}`)}`);
  try {
    await run(`git merge-base --is-ancestor ${shellQuote(originBranch)} ${shellQuote(head)}`);
    if (head === originBranch) {
      console.log(`Local HEAD matches origin/${releaseBranch}.`);
    } else {
      console.log(`Local HEAD is ahead of origin/${releaseBranch}; continuing.`);
    }
    return;
  } catch {
    throw new ReleaseError(
      `Current HEAD on ${branch || '(detached HEAD)'} must include origin/${releaseBranch} before the script creates the release commit.`
    );
  }
}

async function captureGitNetwork(command, options = {}) {
  try {
    return await capture(command, options);
  } catch (error) {
    if (!isGitNetworkResolutionError(error)) {
      throw error;
    }
    const remoteUrl = await githubHttpsRemoteUrl();
    const translated = command.replace(/\borigin\b/g, shellQuote(remoteUrl));
    return await capture(
      `git -c http.sslVerify=false -c http.extraHeader=${shellQuote('Host: github.com')} ${translated.replace(/^git\s+/, '')}`,
      options
    );
  }
}

async function ensureTagMissing(version) {
  const localTag = await capture(`git tag --list ${shellQuote(`v${version}`)}`);
  const remoteTag = await captureGitNetwork(`git ls-remote --tags origin ${shellQuote(`v${version}*`)}`);
  if (localTag || remoteTag) {
    throw new ReleaseError(`Tag v${version} already exists locally or remotely.`);
  }
}

async function ensureReleaseMissing(version) {
  const command = `gh release view ${shellQuote(`v${version}`)} --repo ${shellQuote(config.githubRepo)}`;
  try {
    await capture(command);
  } catch {
    return;
  }
  throw new ReleaseError(`GitHub release v${version} already exists.`);
}

async function ensureReleaseExists(version) {
  const command = `gh release view ${shellQuote(`v${version}`)} --repo ${shellQuote(config.githubRepo)}`;
  try {
    await capture(command);
  } catch {
    throw new ReleaseError(`GitHub release v${version} does not exist; cannot resume release repair.`);
  }
}

async function ensureTagExists(version) {
  const localTag = await capture(`git tag --list ${shellQuote(`v${version}`)}`);
  const remoteTag = await captureGitNetwork(`git ls-remote --tags origin ${shellQuote(`refs/tags/v${version}`)}`);
  if (!localTag && !remoteTag) {
    throw new ReleaseError(`Tag v${version} does not exist locally or remotely; cannot resume release repair.`);
  }
}

async function verifyHomebrewReleaseReadiness(version) {
  logStep('Preflight Homebrew cask rendering');
  const tapDir = await mkdtemp(path.join(tmpdir(), `ghostex-${version}-homebrew-preflight-`));
  try {
    await run(`git clone ${shellQuote(config.tapRepo)} ${shellQuote(tapDir)}`);
    const caskFile = path.join(tapDir, config.caskPath);
    const currentCask = await readFile(caskFile, 'utf8');
    const placeholderSha = currentCask.match(/sha256\s+"([0-9a-f]{64})"/)?.[1] ?? 'a'.repeat(64);
    const renderedCask = renderGhostexCaskForTap(currentCask, {
      sha256: placeholderSha,
      version,
    });
    await writeFile(caskFile, renderedCask);
    await run(`ruby -c ${shellQuote(config.caskPath)}`, { cwd: tapDir });
    /*
     * CDXC:ReleaseAutomation 2026-06-14-09:07:
     * Homebrew readiness must run before GitHub/Sparkle publication so a host
     * with an unusable Homebrew/Xcode/CLT setup fails while the release is still
     * reversible. Use a rendered placeholder cask and syntax/audit probes that
     * do not depend on the future DMG URL already existing.
     */
    await runOptionalHomebrewHostValidation(
      `HOMEBREW_NO_INSTALL_FROM_API=1 brew audit --cask --skip-style ${shellQuote(config.caskPath)}`,
      {
        cwd: tapDir,
        timeoutMs: releaseTimeouts.brewFetchMs,
      }
    );
  } finally {
    await rm(tapDir, { recursive: true, force: true });
  }
}

async function ensureAndroidReleaseReadiness(version, buildVersion, options) {
  if (options.noPush || options.skipAndroid) {
    return;
  }
  if (!existsSync(config.androidSigningEnvFile)) {
    throw new ReleaseError(`Android signing env file is missing: ${config.androidSigningEnvFile}`);
  }
  const script = `
set -euo pipefail
set -a
source ${shellQuote(config.androidSigningEnvFile)}
set +a
export GHOSTEX_ANDROID_VERSION_NAME=${shellQuote(version)}
export GHOSTEX_ANDROID_VERSION_CODE=${shellQuote(String(buildVersion))}
export GHOSTEX_ANDROID_APK_VERSION_TAG=${shellQuote(`v${version}`)}
export GHOSTEX_ANDROID_REQUIRE_RELEASE_SIGNING=1
: "\${GHOSTEX_ANDROID_SIGNING_STORE_FILE:?}"
: "\${GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD:?}"
: "\${GHOSTEX_ANDROID_SIGNING_KEY_ALIAS:?}"
: "\${GHOSTEX_ANDROID_SIGNING_KEY_PASSWORD:?}"
test -f "$GHOSTEX_ANDROID_SIGNING_STORE_FILE"
test -f "$PWD/mobile/package.json"
case "$GHOSTEX_ANDROID_SIGNING_STORE_FILE" in
  "$PWD/mobile"|"$PWD/mobile"/*) exit 42 ;;
esac
android_tool_found() {
  local tool="$1"
  local root
  for root in "\${ANDROID_HOME:-$HOME/Library/Android/sdk}" "$HOME/Library/Android/sdk" /opt/homebrew/share/android-commandlinetools; do
    [[ -d "$root" ]] || continue
    [[ -n "$(find "$root" -path "*/build-tools/*/$tool" -print -quit 2>/dev/null)" ]] && return 0
  done
  return 1
}
android_tool_found apksigner
android_tool_found aapt
`;
  /*
   * CDXC:AndroidRelease 2026-06-14-09:07:
   * Android upload is now part of the standard release flow. Validate signing
   * material and build-tool availability before macOS publication so a missing
   * keystore or SDK tool does not leave the release needing manual APK repair.
   */
  try {
    await run(`/bin/zsh -lc ${shellQuote(script)}`, { timeoutMs: 30_000 });
  } catch (error) {
    const message = String(error.message ?? error);
    if (message.includes('exit 42')) {
      throw new ReleaseError('Android signing keystore must live outside the React Native mobile checkout.');
    }
    throw error;
  }
}

function remoteGxserverLinuxPackageBuildHint() {
  return [
    'Build the Ubuntu remote gxserver packages on Ubuntu/Linux CI with:',
    '  bun run gxserver:remote-linux',
    'Then run the release again from macOS so build/remote-gxserver-linux/x64/package and build/remote-gxserver-linux/arm64/package can be bundled.',
  ].join('\n');
}

async function remoteGxserverLinuxPackageIdentity(packageDir) {
  try {
    return JSON.parse(await readFile(path.join(packageDir, 'build-identity.json'), 'utf8'));
  } catch {
    return null;
  }
}

async function ensureRemoteGxserverLinuxPackagesReady() {
  const expectedRevision = await capture('git rev-parse HEAD');
  const failures = [];
  for (const packageConfig of remoteGxserverLinuxPackageConfigs) {
    const missingResources = missingRemoteGxserverLinuxPackageResources(packageConfig.defaultPackageDir);
    if (missingResources.length > 0) {
      failures.push(
        `${packageConfig.packageLabel} (${packageConfig.arch}) at ${packageConfig.defaultPackageDir} is missing: ${missingResources.join(', ')}`
      );
      continue;
    }
    const identity = await remoteGxserverLinuxPackageIdentity(packageConfig.defaultPackageDir);
    if (!identity?.sourceRevision) {
      failures.push(
        `${packageConfig.packageLabel} (${packageConfig.arch}) at ${packageConfig.defaultPackageDir} was built without sourceRevision metadata. Rebuild it with the current package script.`
      );
      continue;
    }
    if (identity.sourceRevision !== expectedRevision) {
      failures.push(
        `${packageConfig.packageLabel} (${packageConfig.arch}) at ${packageConfig.defaultPackageDir} was built from ${identity.sourceRevision}, but the release source is ${expectedRevision}.`
      );
    }
    if (identity.sourceDirty) {
      failures.push(
        `${packageConfig.packageLabel} (${packageConfig.arch}) at ${packageConfig.defaultPackageDir} was built from a dirty worktree.`
      );
    }
  }
  if (failures.length > 0) {
    throw new ReleaseError(
      [
        'Remote Ubuntu gxserver packages are required for Ghostex releases.',
        ...failures,
        remoteGxserverLinuxPackageBuildHint(),
      ].join('\n')
    );
  }
}

async function latestSparkleVersion() {
  let maxVersion = 0;
  const xml = await readFile(path.join(repoRoot, config.armFeed), 'utf8');
  for (const match of xml.matchAll(/<sparkle:version>(\d+)<\/sparkle:version>/g)) {
    maxVersion = Math.max(maxVersion, Number.parseInt(match[1], 10));
  }
  return maxVersion;
}

async function findSparkleBinDir() {
  /*
   CDXC:ReleaseAutomation 2026-06-10-09:47:
   New macOS releases are arm64-only, so Sparkle appcast generation should first
   use the arm64 SwiftPM artifact directory. Keep older fallback paths only so
   already-cached local tooling can still be found without rebuilding.
   */
  const searchRoots = [
    path.join(repoRoot, 'build/arm64/SourcePackages/artifacts/sparkle'),
    path.join(repoRoot, 'build/SourcePackages/artifacts/sparkle'),
    '/tmp/ghostex-xcodebuild/SourcePackages/artifacts/sparkle',
    path.join(process.env.HOME ?? '', 'Library/Developer/Xcode/DerivedData'),
  ];
  const command = [
    'find',
    ...searchRoots.map((root) => shellQuote(root)),
    "-path '*/Sparkle/bin/generate_appcast' -print -quit 2>/dev/null | xargs dirname",
  ].join(' ');
  const sparkleBinDir = await capture(command);
  if (!sparkleBinDir) {
    throw new ReleaseError('Could not find Sparkle generate_appcast. Build once so SwiftPM downloads Sparkle.');
  }
  for (const tool of ['generate_appcast', 'sign_update', 'generate_keys']) {
    const toolPath = path.join(sparkleBinDir, tool);
    if (!existsSync(toolPath)) {
      throw new ReleaseError(`Missing Sparkle tool: ${toolPath}`);
    }
  }
  return sparkleBinDir;
}

async function findAndVerifySparkleBinDir() {
  const sparkleBinDir = await findSparkleBinDir();
  const publicKey = await capture(`${shellQuote(path.join(sparkleBinDir, 'generate_keys'))} -p`);
  if (!publicKey.includes(config.sparklePublicKey)) {
    throw new ReleaseError('Sparkle public key does not match the expected app SUPublicEDKey.');
  }
  return sparkleBinDir;
}

async function preflight(version, buildVersion, options) {
  logStep('Preflight');
  await ensureGhAuthForRelease();
  await ensureCleanWorktree();
  await ensureReleaseBranchSynced(options.releaseBranch);
  await ensureTagMissing(version);
  await extractChangelogSection(version);
  if (!options.noPush) {
    await ensureReleaseMissing(version);
    await verifyHomebrewReleaseReadiness(version);
    await ensureAndroidReleaseReadiness(version, buildVersion, options);
  }

  if (!options.skipSparkle) {
    const previousBuild = await latestSparkleVersion();
    if (buildVersion <= previousBuild) {
      throw new ReleaseError(
        `Build version ${buildVersion} must be greater than the latest Sparkle build ${previousBuild}.`
      );
    }
    /*
     * CDXC:ReleaseAutomation 2026-08-23:
     * --gpui used to merge into repository-root appcast-gpui.xml, a separate
     * Sparkle feed for the GPUI app's own bundle id. That feed was retired
     * (deleted with appcast-x86_64.xml); the current pipeline publishes the
     * single appcast.xml through tooling/release-gpui instead. This whole
     * legacy --gpui publish path has no successor in this file, so fail fast
     * and explicitly rather than trying to merge into a feed that no longer
     * exists (see buildArch, which fails the same way for the same reason).
     */
    if (options.gpui) {
      throw new ReleaseError(
        `--gpui is unreachable: it published a separate ${config.gpuiFeed} Sparkle feed, retired on 2026-08-23. ` +
          "This script's macOS release pipeline has no successor for that feed; the current pipeline publishes " +
          'appcast.xml through tooling/release-gpui.'
      );
    }
  }

  try {
    await run('env -u GH_TOKEN -u GITHUB_TOKEN gh auth status -h github.com');
  } catch (error) {
    console.warn(
      `Warning: gh auth status failed in this shell; Terminal delegation may still succeed.\n${String(error.message ?? error)}`
    );
  }
  await ensureSigningIdentity();
  await ensureNotaryProfile();

  console.log(
    `Release timeouts: build ${Math.round(releaseTimeouts.buildArchMs / 60_000)}m/arch, notary ${Math.round(releaseTimeouts.notaryArchMs / 60_000)}m/arch, overall ${Math.round(releaseTimeouts.overallMs / 60_000)}m.`
  );

  if (!options.skipTypecheck) {
    await run('bun run typecheck', { timeoutMs: releaseTimeouts.typecheckMs });
  }
  if (options.withTests) {
    await run('bun run release:test', { timeoutMs: releaseTimeouts.testMs });
  }

  return {};
}

async function updatePackageJson(version) {
  const packagePath = path.join(repoRoot, 'package.json');
  const packageJson = JSON.parse(await readFile(packagePath, 'utf8'));
  packageJson.version = version;
  await writeFile(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

async function bumpReleaseMetadata(version, buildVersion, options) {
  logStep(`Bump release metadata to ${version} (${buildVersion})`);
  await updatePackageJson(version);
}

/*
 * CDXC:ReleaseAutomation 2026-08-22:
 * buildArch used to invoke native/macos/ghostexHost/build-ghostex-host.sh, the
 * Xcode build for the Swift/AppKit macOS app. That app (and its ghostexHost
 * Xcode project) was removed on 2026-08-20; this legacy pipeline has no
 * successor in this file. macOS builds now run through tooling/release-gpui/macos.sh.
 * Fail fast and explicitly instead of leaving a stale path that would only
 * surface as an opaque "no such file" error from the shell.
 */
async function buildArch(version, entry) {
  throw new ReleaseError(
    `buildArch(${entry.arch}) for ${version} is unreachable: it built the deprecated Swift macOS app via ` +
      'native/macos/ghostexHost/build-ghostex-host.sh, removed with that app on 2026-08-20. ' +
      "This script's macOS build pipeline has no successor here; macOS builds run through tooling/release-gpui/macos.sh."
  );
}

async function validateBuiltApp(version, buildVersion, entry) {
  logStep(`Validate built ${entry.arch} app`);
  const infoCommand = [
    `plutil -p ${shellQuote(path.join(entry.appPath, 'Contents/Info.plist'))}`,
    '|',
    "rg 'CFBundleShortVersionString|CFBundleVersion|CFBundleIdentifier|SUFeedURL|SUPublicEDKey|GHOSTEX'",
  ].join(' ');
  await run(infoCommand);
  await run(
    `codesign -dv --verbose=4 ${shellQuote(entry.appPath)} 2>&1 | rg 'Authority|TeamIdentifier|Identifier|Timestamp|Runtime|Format'`
  );
  await run(`codesign --verify --deep --strict --verbose=2 ${shellQuote(entry.appPath)}`);
  await run(
    `lipo -archs ${shellQuote(path.join(entry.appPath, 'Contents/MacOS', config.appName))} | grep -Fx ${shellQuote(entry.arch)}`
  );
  await run(
    `lipo -archs ${shellQuote(path.join(entry.appPath, 'Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework'))} | grep -Fx ${shellQuote(entry.arch)}`
  );
  try {
    await validateMacosAppBundle({
      allowLegacyBundleShape: true,
      appName: config.appName,
      appPath: entry.appPath,
      arch: entry.arch,
    });
  } catch (error) {
    throw new ReleaseError(error instanceof Error ? error.message : String(error));
  }
  const onDemandAssets = await validateOnDemandAssetStaging(version, entry);

  const info = await capture(
    `plutil -extract CFBundleShortVersionString raw ${shellQuote(path.join(entry.appPath, 'Contents/Info.plist'))}`
  );
  const bundleVersion = await capture(
    `plutil -extract CFBundleVersion raw ${shellQuote(path.join(entry.appPath, 'Contents/Info.plist'))}`
  );
  const feedUrl = await capture(
    `plutil -extract SUFeedURL raw ${shellQuote(path.join(entry.appPath, 'Contents/Info.plist'))}`
  );
  const publicKey = await capture(
    `plutil -extract SUPublicEDKey raw ${shellQuote(path.join(entry.appPath, 'Contents/Info.plist'))}`
  );

  if (info !== version || bundleVersion !== String(buildVersion)) {
    throw new ReleaseError(`${entry.arch} Info.plist version mismatch: ${info} (${bundleVersion})`);
  }
  if (feedUrl !== entry.feedUrl) {
    throw new ReleaseError(`${entry.arch} SUFeedURL mismatch: ${feedUrl}`);
  }
  if (publicKey !== config.sparklePublicKey) {
    throw new ReleaseError(`${entry.arch} SUPublicEDKey mismatch.`);
  }

  await validateLidSleepHelperSigning(entry);
  return onDemandAssets;
}

function onDemandAssetsDir(version) {
  return path.join(repoRoot, 'build', 'on-demand-assets', version);
}

async function readOnDemandBuildManifest(version) {
  const manifestPath = path.join(onDemandAssetsDir(version), 'assets.json');
  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  } catch (error) {
    throw new ReleaseError(
      `On-demand asset build manifest is missing or unreadable: ${manifestPath}. The app build with GHOSTEX_ON_DEMAND_ASSETS=1 writes it.\n${String(error?.message ?? error)}`
    );
  }
  if (manifest.version !== version || !Array.isArray(manifest.assets)) {
    throw new ReleaseError(`On-demand asset build manifest at ${manifestPath} does not describe version ${version}.`);
  }
  return manifest;
}

async function validateOnDemandAssetStaging(version, entry) {
  const webRoot = path.join(entry.appPath, 'Contents', 'Resources', 'Web');

  for (const packageConfig of remoteGxserverLinuxPackageConfigs) {
    const staleDir = path.join(webRoot, packageConfig.resourceName);
    if (existsSync(staleDir)) {
      throw new ReleaseError(
        `${entry.arch} app bundle still embeds ${packageConfig.resourceName}; on-demand releases must ship it as a GitHub release asset instead.`
      );
    }
  }

  const bundleManifestPath = path.join(webRoot, 'on-demand-resources.json');
  let bundleManifest;
  try {
    bundleManifest = JSON.parse(await readFile(bundleManifestPath, 'utf8'));
  } catch (error) {
    throw new ReleaseError(
      `${entry.arch} app bundle is missing the sealed on-demand manifest at ${bundleManifestPath}.\n${String(error?.message ?? error)}`
    );
  }
  if (bundleManifest.version !== version) {
    throw new ReleaseError(
      `${entry.arch} on-demand manifest records version ${bundleManifest.version}, expected ${version}.`
    );
  }

  const buildManifest = await readOnDemandBuildManifest(version);
  const uploads = [];
  for (const assetName of onDemandAssetNames) {
    const buildAsset = buildManifest.assets.find((asset) => asset.name === assetName);
    if (!buildAsset?.path || !existsSync(buildAsset.path)) {
      throw new ReleaseError(
        `On-demand asset tarball is missing for ${assetName} under ${onDemandAssetsDir(version)}.`
      );
    }
    const bundleAsset = Object.values(bundleManifest.assets ?? {}).find((asset) => asset?.name === assetName);
    if (!bundleAsset) {
      throw new ReleaseError(`${entry.arch} sealed on-demand manifest does not list ${assetName}.`);
    }
    if (!/^[0-9a-f]{64}$/.test(buildAsset.sha256 ?? '') || bundleAsset.sha256 !== buildAsset.sha256) {
      throw new ReleaseError(
        `On-demand asset checksum mismatch for ${assetName}: sealed ${bundleAsset.sha256} vs built ${buildAsset.sha256}.`
      );
    }
    const actualSha = await capture(`shasum -a 256 ${shellQuote(buildAsset.path)} | awk '{print $1}'`);
    if (actualSha !== buildAsset.sha256) {
      throw new ReleaseError(
        `On-demand asset file changed after manifest sealing for ${assetName}: manifest ${buildAsset.sha256}, file ${actualSha}.`
      );
    }
    uploads.push({ name: assetName, path: buildAsset.path, sha256: buildAsset.sha256 });
  }
  return uploads;
}

async function uploadOnDemandAssets(version, onDemandAssets) {
  if (onDemandAssets.length === 0) {
    return;
  }
  logStep('Upload on-demand release assets');
  const assetPaths = onDemandAssets.map((asset) => shellQuote(asset.path)).join(' ');
  await run(
    `gh release upload ${shellQuote(`v${version}`)} ${assetPaths} --repo ${shellQuote(config.githubRepo)} --clobber`,
    { timeoutMs: releaseTimeouts.brewFetchMs }
  );
}

async function onDemandAssetsFromReleaseAssets(assets) {
  const found = [];
  for (const assetName of onDemandAssetNames) {
    const asset = assets.find((entry) => entry.name === assetName);
    if (!asset) {
      return null;
    }
    const sha256 = parseGithubAssetSha(asset);
    found.push({ name: assetName, sha256 });
  }
  return found;
}

async function validateLidSleepHelperSigning(entry) {
  const helperName = `${entry.bundleId ?? config.bundleId}.LidSleepHelper`;
  const launchServicesHelper = path.join(entry.appPath, 'Contents/Library/LaunchServices', helperName);
  const resourcesHelper = path.join(entry.appPath, 'Contents/Resources', helperName);

  if (existsSync(resourcesHelper)) {
    throw new ReleaseError(
      `${entry.arch} still contains an unsigned Resources copy of ${helperName}. Release builds must ship only Contents/Library/LaunchServices/${helperName}.`
    );
  }
  if (!existsSync(launchServicesHelper)) {
    throw new ReleaseError(`${entry.arch} is missing bundled lid sleep helper: ${launchServicesHelper}`);
  }

  const signingDetails = await capture(`codesign -dv --verbose=4 ${shellQuote(launchServicesHelper)} 2>&1`);
  if (!/Developer ID Application:/.test(signingDetails)) {
    throw new ReleaseError(
      `${entry.arch} lid sleep helper is not Developer ID signed:\n${launchServicesHelper}\n${signingDetails}`
    );
  }
  if (!/Timestamp=/.test(signingDetails)) {
    throw new ReleaseError(`${entry.arch} lid sleep helper is missing a secure timestamp: ${launchServicesHelper}`);
  }
  if (!/flags=.*runtime/.test(signingDetails)) {
    throw new ReleaseError(`${entry.arch} lid sleep helper is missing hardened runtime: ${launchServicesHelper}`);
  }

  const entitlements = await capture(
    `codesign -d --entitlements :- ${shellQuote(launchServicesHelper)} 2>/dev/null | plutil -p - 2>/dev/null || true`
  );
  if (/get-task-allow/.test(entitlements)) {
    throw new ReleaseError(
      `${entry.arch} lid sleep helper still has get-task-allow and cannot be notarized: ${launchServicesHelper}`
    );
  }
}

async function buildGpuiArch(version, buildVersion, entry) {
  logStep('Build GPUI arm64 release app');
  /*
   The GPUI leg reuses the already-prepared
   remote Linux gxserver packages (prepare-remote-linux phase) and the Sparkle
   framework SwiftPM already downloaded for the macOS build, so it must run
   after both. GPUI still embeds the Linux packages in its bundle; the macOS
   app moved them to on-demand GitHub assets.
   */
  const env = {
    GHOSTEX_MACOS_ARCH: entry.arch,
    GHOSTEX_GPUI_SIGN_IDENTITY: gpuiReleaseSigningIdentity(),
    GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG: '--timestamp',
    GHOSTEX_GPUI_MARKETING_VERSION: version,
    GHOSTEX_GPUI_BUILD_VERSION: String(buildVersion),
    GHOSTEX_GPUI_SPARKLE_FEED_URL: entry.feedUrl,
    GHOSTEX_REQUIRE_REMOTE_GXSERVER_LINUX_PACKAGES: '1',
    GHOSTEX_REQUIRE_SPARKLE: '1',
  };
  await runWithHeartbeat('/bin/bash apps/desktop/scripts/build-macos-app.sh', {
    label: 'GPUI app build',
    env,
    timeoutMs: releaseTimeouts.buildArchMs,
  });
  const appPath = path.join(repoRoot, 'apps', 'desktop', 'build', 'macos.noindex', `${config.gpuiAppName}.app`);
  if (!existsSync(appPath)) {
    throw new ReleaseError(`GPUI build did not produce an app bundle at ${appPath}`);
  }
  return { ...entry, appPath };
}

async function validateGpuiBuiltApp(version, buildVersion, entry) {
  logStep('Validate built GPUI app');
  const infoPlist = path.join(entry.appPath, 'Contents/Info.plist');
  await run(
    `plutil -p ${shellQuote(infoPlist)} | rg 'CFBundleShortVersionString|CFBundleVersion|CFBundleIdentifier|SUFeedURL|SUPublicEDKey|GHOSTEX'`
  );
  await run(
    `codesign -dv --verbose=4 ${shellQuote(entry.appPath)} 2>&1 | rg 'Authority|TeamIdentifier|Identifier|Timestamp|Runtime|Format'`
  );
  await run(`codesign --verify --deep --strict --verbose=2 ${shellQuote(entry.appPath)}`);
  await run(
    `lipo -archs ${shellQuote(path.join(entry.appPath, 'Contents/MacOS', entry.appName))} | grep -Fx ${shellQuote(entry.arch)}`
  );
  // The cef-rs CEF distribution can be single-arch or universal; require the
  // release arch to be present without forbidding a fat framework.
  await run(
    `lipo -archs ${shellQuote(path.join(entry.appPath, 'Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework'))} | grep -w ${shellQuote(entry.arch)}`
  );
  if (!existsSync(path.join(entry.appPath, 'Contents/Frameworks/Sparkle.framework'))) {
    throw new ReleaseError('GPUI release app is missing Sparkle.framework; auto-update would be dead.');
  }

  const info = await capture(`plutil -extract CFBundleShortVersionString raw ${shellQuote(infoPlist)}`);
  const bundleVersion = await capture(`plutil -extract CFBundleVersion raw ${shellQuote(infoPlist)}`);
  const bundleId = await capture(`plutil -extract CFBundleIdentifier raw ${shellQuote(infoPlist)}`);
  const feedUrl = await capture(`plutil -extract SUFeedURL raw ${shellQuote(infoPlist)}`);
  const publicKey = await capture(`plutil -extract SUPublicEDKey raw ${shellQuote(infoPlist)}`);
  if (info !== version || bundleVersion !== String(buildVersion)) {
    throw new ReleaseError(`GPUI Info.plist version mismatch: ${info} (${bundleVersion})`);
  }
  if (bundleId !== config.gpuiBundleId) {
    throw new ReleaseError(`GPUI bundle identifier mismatch: ${bundleId}`);
  }
  if (feedUrl !== entry.feedUrl) {
    throw new ReleaseError(`GPUI SUFeedURL mismatch: ${feedUrl}`);
  }
  if (publicKey !== config.sparklePublicKey) {
    throw new ReleaseError('GPUI SUPublicEDKey mismatch.');
  }

  await validateLidSleepHelperSigning(entry);
}

async function packageReleaseDmg(version, artifactDir, entry) {
  logStep(`Package ${entry.kind === 'gpui' ? 'GPUI ' : ''}${entry.arch} DMG`);
  const stagingRoot = path.join(tmpdir(), 'ghostex-release-staging.noindex');
  await mkdir(stagingRoot, { recursive: true });
  const stagingDir = await mkdtemp(
    path.join(stagingRoot, `ghostex-${version}-${entry.kind ?? 'macos'}-${entry.arch}-stage-`)
  );
  const finalDmg = path.join(artifactDir, entry.dmgName ?? `ghostex-${version}-${entry.arch}.dmg`);
  const stagedApp = path.join(stagingDir, entry.stagedAppName ?? config.stagedAppName);

  /*
   CDXC:ReleaseAutomation 2026-06-17-09:54:
   Release DMG staging copies the signed app bundle after code-server and nested
   native payloads have been materialized. Use ditto for the bundle copy so a
   transient filesystem entry under a large packaged runtime cannot make cp -R
   abort after validation has already passed.
   */
  await run(`ditto ${shellQuote(entry.appPath)} ${shellQuote(stagedApp)}`);
  await run(`ln -s /Applications ${shellQuote(path.join(stagingDir, 'Applications'))}`);
  await run(
    `hdiutil create -volname ${shellQuote(entry.volumeName ?? 'ghostex')} -srcfolder ${shellQuote(stagingDir)} -format UDZO ${shellQuote(finalDmg)}`
  );
  const preStapleSha = await capture(`shasum -a 256 ${shellQuote(finalDmg)} | awk '{print $1}'`);
  await rm(stagingDir, { recursive: true, force: true });

  return {
    ...entry,
    finalDmg,
    preStapleSha,
  };
}

async function notarizeReleaseDmg(version, artifactDir, entry) {
  logStep(`Notarize ${entry.arch}`);
  const notaryLogPath = path.join(artifactDir, `ghostex-${version}-${entry.arch}-notary.log`);
  const notaryOutput = await runWithHeartbeat(
    `xcrun notarytool submit ${shellQuote(entry.finalDmg)} --keychain-profile ${shellQuote(config.notaryProfile)} --wait | tee ${shellQuote(notaryLogPath)}`,
    {
      label: `${entry.arch} notarization`,
      timeoutMs: releaseTimeouts.notaryArchMs,
    }
  );
  const submissionId = notaryOutput.match(/id:\s*([0-9a-f-]+)/)?.[1] ?? 'unknown';
  /*
   CDXC:ReleaseAutomation 2026-05-23-13:58:
   `notarytool --wait` prints repeated `Current status: In Progress` lines
   before the final `status: Accepted`; parse the last status-like token so
   accepted submissions are not rejected after a long notarization wait.
   */
  const statusMatches = [...notaryOutput.matchAll(/(?:Current status:|status:)\s*([A-Za-z ]+)/g)];
  const status = statusMatches.at(-1)?.[1]?.trim() ?? 'unknown';
  if (status !== 'Accepted') {
    throw new ReleaseError(`${entry.arch} notarization did not finish Accepted. Status: ${status}`);
  }

  await run(`xcrun stapler staple ${shellQuote(entry.finalDmg)}`);
  await run(`xcrun stapler validate ${shellQuote(entry.finalDmg)}`);
  const sha256 = await capture(`shasum -a 256 ${shellQuote(entry.finalDmg)} | awk '{print $1}'`);
  const artifactKey = entry.kind === 'gpui' ? `gpui-${entry.arch}` : entry.arch;
  await writeFile(`/tmp/ghostex-${version.replaceAll('.', '')}-${artifactKey}-sha256`, `${sha256}\n`);
  await writeFile(`/tmp/ghostex-${version.replaceAll('.', '')}-${artifactKey}-final-dmg`, `${entry.finalDmg}\n`);

  return {
    ...entry,
    sha256,
    notaryLogPath,
    notarySubmissionId: submissionId,
    notaryStatus: status,
  };
}

async function validateMountedDmg(version, buildVersion, entry) {
  logStep(`Validate mounted ${entry.arch} DMG`);
  const mountRoot = path.join(tmpdir(), 'ghostex-dmg-mounts.noindex');
  await mkdir(mountRoot, { recursive: true });
  const mountPoint = await mkdtemp(path.join(mountRoot, `validate-${version}-${entry.arch}-`));
  await capture(
    `hdiutil attach -nobrowse -readonly -mountpoint ${shellQuote(mountPoint)} ${shellQuote(entry.finalDmg)}`
  );

  try {
    const appPath = path.join(mountPoint, entry.stagedAppName ?? config.stagedAppName);
    try {
      await run(`spctl --assess --type execute --verbose ${shellQuote(appPath)}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!message.includes('Too many open files')) {
        throw error;
      }
      /*
       CDXC:MacRelease 2026-06-12-12:35:
       Apple's spctl can fail with "Too many open files" while walking the
       mounted Ghostex app bundle even after notarytool accepts the DMG and
       stapler validates its ticket. Treat only that descriptor-exhaustion case
       as an assessment tool limitation, revalidate the stapled DMG ticket, and
       continue into the mounted app's codesign, architecture, and version
       checks so distribution validation still proves the shipped artifact.
      */
      console.warn(
        `Warning: spctl could not assess mounted ${entry.arch} app because it exhausted open files; validating the stapled DMG ticket and mounted app signature instead.`
      );
      await run(`xcrun stapler validate ${shellQuote(entry.finalDmg)}`);
    }
    await run(`codesign --verify --deep --strict --verbose=2 ${shellQuote(appPath)}`);
    await run(
      `lipo -archs ${shellQuote(path.join(appPath, 'Contents/MacOS', entry.appName ?? config.appName))} | grep -Fx ${shellQuote(entry.arch)}`
    );
    await run(
      `plutil -p ${shellQuote(path.join(appPath, 'Contents/Info.plist'))} | rg 'CFBundleShortVersionString|CFBundleVersion|CFBundleIdentifier|SUFeedURL|SUPublicEDKey'`
    );
    const shortVersion = await capture(
      `plutil -extract CFBundleShortVersionString raw ${shellQuote(path.join(appPath, 'Contents/Info.plist'))}`
    );
    const bundleVersion = await capture(
      `plutil -extract CFBundleVersion raw ${shellQuote(path.join(appPath, 'Contents/Info.plist'))}`
    );
    if (shortVersion !== version || bundleVersion !== String(buildVersion)) {
      throw new ReleaseError(`Mounted ${entry.arch} app version mismatch: ${shortVersion} (${bundleVersion})`);
    }
  } finally {
    await run(`hdiutil detach ${shellQuote(mountPoint)}`);
    await rm(mountPoint, { recursive: true, force: true });
  }
}

async function buildAndPackage(version, buildVersion, options = {}) {
  logStep('Build arm64 release app');
  /*
   CDXC:MacRelease 2026-06-10-09:47:
   Release automation intentionally builds only the Apple Silicon app. Do not
   add Intel release legs back here; old Intel artifacts remain available from
   their existing GitHub releases. The historical appcast-x86_64.xml feed that
   tracked them was retired on 2026-08-23.
   */
  const built = [];
  for (const entry of releaseArchitectures) {
    built.push(await buildArch(version, entry));
  }

  let onDemandAssets = [];
  for (const entry of built) {
    onDemandAssets = await validateBuiltApp(version, buildVersion, entry);
  }

  /*
   The optional GPUI leg builds after the
   macOS app because its Sparkle framework staging reuses the SwiftPM
   artifacts the macOS Xcode build downloads. GPUI artifacts stay in a
   separate array so arch-keyed macOS consumers (Homebrew, resume, notes
   arm-lookup) never accidentally select the GPUI DMG.
   */
  const gpuiBuilt = [];
  if (options.gpui) {
    const gpuiEntry = await buildGpuiArch(version, buildVersion, gpuiReleaseEntry(version));
    await validateGpuiBuiltApp(version, buildVersion, gpuiEntry);
    gpuiBuilt.push(gpuiEntry);
  }

  const artifactDir = await mkdtemp(path.join(tmpdir(), `ghostex-${version}-release-`));
  console.log(`Artifact directory: ${artifactDir}`);

  /*
   CDXC:ReleaseAutomation 2026-06-10-09:47:
   The public macOS release artifact set contains one arm64 DMG. Keep the same
   signing, packaging, notarization, stapling, and mounted-DMG validation steps
   for that artifact so Apple Silicon update safety stays unchanged.
   */
  logStep('Package arm64 DMG');
  const packagedDmgs = [];
  for (const entry of built) {
    packagedDmgs.push(await packageReleaseDmg(version, artifactDir, entry));
  }
  const packagedGpuiDmgs = [];
  for (const entry of gpuiBuilt) {
    packagedGpuiDmgs.push(await packageReleaseDmg(version, artifactDir, entry));
  }

  logStep('Notarize arm64 DMG');
  const packaged = await Promise.all(packagedDmgs.map((entry) => notarizeReleaseDmg(version, artifactDir, entry)));
  const packagedGpui = await Promise.all(
    packagedGpuiDmgs.map((entry) => notarizeReleaseDmg(version, artifactDir, entry))
  );

  for (const entry of [...packaged, ...packagedGpui]) {
    await validateMountedDmg(version, buildVersion, entry);
  }

  return { artifactDir, artifacts: packaged, gpuiArtifacts: packagedGpui, onDemandAssets };
}

async function generateAppcast(version, buildVersion, sparkleBinDir, artifact) {
  logStep(`Generate Sparkle feed ${artifact.feed}`);
  const workDir = await mkdtemp(path.join(tmpdir(), `ghostex-${version}-${artifact.arch}-appcast-`));
  const appcastPath = path.join(repoRoot, artifact.feed);
  const workAppcast = path.join(workDir, 'appcast.xml');
  const workDmg = path.join(workDir, path.basename(artifact.finalDmg));

  await run(`cp ${shellQuote(appcastPath)} ${shellQuote(workAppcast)}`);
  await run(`cp ${shellQuote(artifact.finalDmg)} ${shellQuote(workDmg)}`);
  const changelogNotes = await writeSparkleReleaseNotes(version, workDmg, artifact.releaseNotesTitle ?? 'Ghostex');
  await run(
    [
      shellQuote(path.join(sparkleBinDir, 'generate_appcast')),
      '--download-url-prefix',
      shellQuote(`https://github.com/${config.githubRepo}/releases/download/v${version}/`),
      '--full-release-notes-url',
      shellQuote(`https://github.com/${config.githubRepo}/releases/tag/v${version}`),
      '--embed-release-notes',
      '--maximum-versions 6',
      '-o',
      shellQuote(workAppcast),
      shellQuote(workDir),
    ].join(' ')
  );
  await run(`cp ${shellQuote(workAppcast)} ${shellQuote(appcastPath)}`);
  await run(`xmllint --noout ${shellQuote(appcastPath)}`);
  await run(`${shellQuote(path.join(sparkleBinDir, 'sign_update'))} ${shellQuote(appcastPath)}`);
  /*
   CDXC:ReleaseAutomation 2026-06-03-20:28:
   Sparkle verification must prove the appcast enclosure signature validates
   the generated DMG artifact, not merely that the XML feed can be signed.
   Use namespace-agnostic XPath because appcast namespace prefixes can vary
   across generated feeds.
   */
  const enclosureSignature = await capture(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@*[local-name()='edSignature'])[1])" ${shellQuote(appcastPath)}`
  );
  await run(
    `${shellQuote(path.join(sparkleBinDir, 'sign_update'))} --verify ${shellQuote(artifact.finalDmg)} ${shellQuote(enclosureSignature)}`
  );
  await run(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" ${shellQuote(appcastPath)} | grep -Fx ${shellQuote(String(buildVersion))}`
  );
  await run(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" ${shellQuote(appcastPath)} | grep -Fx ${shellQuote(version)}`
  );
  const embeddedReleaseNotesFormat = await capture(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='description']/@*[local-name()='format'])[1])" ${shellQuote(appcastPath)}`
  );
  if (embeddedReleaseNotesFormat.trim() !== 'markdown') {
    throw new ReleaseError(`Sparkle feed ${artifact.feed} did not embed markdown release notes for ${version}.`);
  }
  const embeddedReleaseNotes = await capture(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='description'])[1])" ${shellQuote(appcastPath)}`
  );
  if (!embeddedReleaseNotes.includes(changelogNotes.trim())) {
    throw new ReleaseError(`Sparkle feed ${artifact.feed} is missing the CHANGELOG.md notes for ${version}.`);
  }
  await run(
    `rg ${shellQuote(`${path.basename(artifact.finalDmg)}|sparkle:version|sparkle:shortVersionString|sparkle:edSignature|sparkle-signatures`)} ${shellQuote(appcastPath)} -g '!node_modules/**' -g '!dist/**' -g '!build/**' -g '!coverage/**' -g '!.git/**'`
  );

  await rm(workDir, { recursive: true, force: true });
}

async function writeSparkleReleaseNotes(version, workDmg, releaseNotesTitle = 'Ghostex') {
  const changelogNotes = await extractChangelogSection(version);
  const parsedDmg = path.parse(workDmg);
  const releaseNotesPath = path.join(parsedDmg.dir, `${parsedDmg.name}.md`);
  /*
   CDXC:AutoUpdate 2026-06-08-10:07:
   Sparkle's update dialog does not render `sparkle:fullReleaseNotesLink`; it
   shows changelog text only from a per-item description or releaseNotesLink.
   Write same-basename markdown beside each DMG and force embedding so the
   update menu shows CHANGELOG.md notes without depending on a separate notes
   asset or browser fallback.
   */
  const releaseNotes = [
    `# ${releaseNotesTitle} ${version}`,
    '',
    changelogNotes,
    '',
    `[Full release notes](https://github.com/${config.githubRepo}/releases/tag/v${version})`,
    '',
  ].join('\n');
  await writeFile(releaseNotesPath, releaseNotes, 'utf8');
  return changelogNotes;
}

async function updateSparkleFeeds(version, buildVersion, sparkleBinDir, artifacts) {
  for (const artifact of artifacts) {
    await generateAppcast(version, buildVersion, sparkleBinDir, artifact);
  }
}

async function commitReleaseMetadata(version, options) {
  logStep('Commit release metadata');
  const metadataFiles = ['package.json'];
  if (!options.skipSparkle) {
    metadataFiles.push(config.armFeed);
    if (options.gpui) {
      metadataFiles.push(config.gpuiFeed);
    }
  }
  await run(`git add ${metadataFiles.map(shellQuote).join(' ')}`);
  await run(`git commit -m ${shellQuote(`chore: release ${version}`)}`);

  if (!options.noPush) {
    await runGitNetwork(`git push origin HEAD:${shellQuote(options.releaseBranch)}`);
    await run(`git tag -a ${shellQuote(`v${version}`)} -m ${shellQuote(`Release v${version}`)}`);
    await runGitNetwork(`git push origin ${shellQuote(`v${version}`)}`);
  }

  return capture('git rev-parse HEAD');
}

async function extractChangelogSection(version) {
  const changelog = await readFile(path.join(repoRoot, 'CHANGELOG.md'), 'utf8');
  return extractChangelogSectionFromText(changelog, version);
}

function extractChangelogSectionFromText(changelog, version) {
  /*
   CDXC:ReleaseAutomation 2026-05-23-14:03:
   Do not use a multiline regex with `$` here: in JS multiline mode it can stop
   at the blank line after the heading and make valid release notes look empty.
   */
  const lines = changelog.split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith(`## ${version} - `));
  if (start === -1) {
    throw new ReleaseError(`CHANGELOG.md does not contain a top-level section for ${version}.`);
  }
  const section = [];
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith('## ')) {
      break;
    }
    section.push(line);
  }
  const notes = section.join('\n').trim();
  if (!notes || notes.includes('CDXC:') || notes.includes('<!--')) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} is empty or contains comments.`);
  }
  validateMajorMinorReleaseNotes(notes, version);
  return notes;
}

function validateMajorMinorReleaseNotes(notes, version) {
  /*
   * CDXC:ReleaseNotes 2026-06-14-09:18:
   * Public changelog sections must keep release notes scannable by using
   * Major and Minor top-level bullets, with an optional GPUI section after
   * Minor for cross-platform app work.
   * Enforce this before publishing so GitHub and Sparkle notes stay consistent.
   */
  const lines = notes.split(/\r?\n/).filter((line) => line.trim().length > 0);
  const majorIndex = lines.findIndex((line) => line === '- Major');
  const minorIndex = lines.findIndex((line) => line === '- Minor');
  const gpuiIndex = lines.findIndex((line) => line === '- GPUI');
  if (majorIndex !== 0 || minorIndex === -1 || majorIndex > minorIndex) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} must use Major and Minor top-level bullets.`);
  }
  if (gpuiIndex !== -1 && gpuiIndex < minorIndex) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} must place GPUI after Minor.`);
  }
  const topLevelBullets = lines.filter((line) => line.startsWith('- '));
  if (topLevelBullets.some((line) => line !== '- Major' && line !== '- Minor' && line !== '- GPUI')) {
    throw new ReleaseError(
      `CHANGELOG.md section for ${version} must keep Major, Minor, and optional GPUI as the only top-level bullets.`
    );
  }
  const sectionHeadings = new Set(['- Major', '- Minor', '- GPUI']);
  const invalidItemLines = lines.filter(
    (line) => !sectionHeadings.has(line) && (!line.startsWith('  - ') || line.slice(4).trim().length === 0)
  );
  if (invalidItemLines.length > 0) {
    throw new ReleaseError(
      `CHANGELOG.md section for ${version} must keep every change item on one physical \`  - \` line.`
    );
  }
  if (
    lines.filter((line) => line === '- Major').length !== 1 ||
    lines.filter((line) => line === '- Minor').length !== 1 ||
    lines.filter((line) => line === '- GPUI').length > 1
  ) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} must not repeat release-note headings.`);
  }
  const majorSubBullets = lines.slice(majorIndex + 1, minorIndex).filter((line) => line.startsWith('  - '));
  const minorEndIndex = gpuiIndex === -1 ? lines.length : gpuiIndex;
  const minorSubBullets = lines.slice(minorIndex + 1, minorEndIndex).filter((line) => line.startsWith('  - '));
  if (majorSubBullets.length === 0 || minorSubBullets.length === 0) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} must include sub-bullets under both Major and Minor.`);
  }
  if (gpuiIndex !== -1 && !lines.slice(gpuiIndex + 1).some((line) => line.startsWith('  - '))) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} must include sub-bullets under GPUI when present.`);
  }
}

async function buildGithubReleaseNotes(
  version,
  artifacts,
  { androidArtifact = null, onDemandAssets = null, gpuiArtifact = null } = {}
) {
  const changelogNotes = await extractChangelogSection(version);
  const arm = artifacts.find((entry) => entry.arch === 'arm64' && entry.kind !== 'gpui');
  if (!arm) {
    throw new ReleaseError('arm64 release artifact is required.');
  }
  const downloads = ['- Apple Silicon', `  - \`${path.basename(arm.finalDmg)}\``, `  - SHA256: \`${arm.sha256}\``];
  if (gpuiArtifact) {
    downloads.push(
      '- Ghostex (Apple Silicon)',
      `  - \`${path.basename(gpuiArtifact.finalDmg)}\``,
      `  - SHA256: \`${gpuiArtifact.sha256}\``
    );
  }
  if (androidArtifact) {
    downloads.push('- Android', `  - \`${androidArtifact.name}\``, `  - SHA256: \`${androidArtifact.sha256}\``);
  }

  const onDemandSection = [];
  if (onDemandAssets?.length) {
    onDemandSection.push(
      '## On-demand components',
      '',
      'The app downloads these automatically when first needed (remote Linux machines, Project board) and verifies them against checksums sealed inside the signed app:',
      '',
      ...onDemandAssets.map((asset) => `- \`${asset.name}\` — SHA256: \`${asset.sha256}\``),
      ''
    );
  }

  return [
    '## Changes',
    '',
    changelogNotes,
    '',
    '## Downloads',
    '',
    ...downloads,
    '',
    ...onDemandSection,
    '## Install',
    '',
    '```sh',
    config.installCommand,
    '```',
    '',
  ].join('\n');
}

async function createGithubRelease(version, artifacts, options, { onDemandAssets = [], gpuiArtifact = null } = {}) {
  logStep('Create GitHub release');
  const notesPath = path.join(await mkdtemp(path.join(tmpdir(), `ghostex-${version}-notes-`)), 'notes.md');
  const notes = await buildGithubReleaseNotes(version, artifacts, { onDemandAssets, gpuiArtifact });

  await writeFile(notesPath, notes);
  const assets = artifacts.map((entry) => shellQuote(entry.finalDmg)).join(' ');
  await run(
    [
      'gh release create',
      shellQuote(`v${version}`),
      assets,
      '--repo',
      shellQuote(config.githubRepo),
      '--title',
      shellQuote(`Ghostex ${version}`),
      '--notes-file',
      shellQuote(notesPath),
      ...(options.githubPrerelease || isPrereleaseVersion(version) ? ['--prerelease'] : []),
    ].join(' ')
  );
  await uploadOnDemandAssets(version, onDemandAssets);

  return `https://github.com/${config.githubRepo}/releases/tag/v${version}`;
}

async function updateGithubReleaseNotes(version, artifacts, releaseAssets) {
  logStep('Update GitHub release notes');
  const notesPath = path.join(await mkdtemp(path.join(tmpdir(), `ghostex-${version}-final-notes-`)), 'notes.md');
  await writeFile(notesPath, await buildGithubReleaseNotes(version, artifacts, releaseAssets));
  await run(
    [
      'gh release edit',
      shellQuote(`v${version}`),
      '--repo',
      shellQuote(config.githubRepo),
      '--notes-file',
      shellQuote(notesPath),
    ].join(' ')
  );
}

async function validateLiveSparkleAndAssets(version, buildVersion, sparkleBinDir, artifacts) {
  logStep('Validate live Sparkle feed and GitHub asset');
  /*
   CDXC:ReleaseAutomation 2026-07-02-14:10:
   The 5.4.0 release downloaded the same ~800 MB DMG three times (here, brew
   fetch, final verification). This validation now proves the same facts
   without a download: the live appcast signature must verify against the
   exact local artifact bytes, and the GitHub API asset digest must equal the
   local artifact SHA256. Homebrew's arm fetch stays the single full live
   download of the release.
   */
  /*
   Live validation iterates the packaged
   artifacts themselves (macOS + optional GPUI) instead of the arch table so
   each artifact's own feed, feed URL, and DMG asset name are proven; the
   expected enclosure name comes from the artifact basename.
   */
  for (const artifact of artifacts ?? []) {
    if (!artifact?.finalDmg || !existsSync(artifact.finalDmg)) {
      throw new ReleaseError(
        `Local ${artifact?.arch ?? '(unknown)'} DMG is required for live validation: ${artifact?.finalDmg ?? '(missing)'}`
      );
    }
    const dmgAssetName = path.basename(artifact.finalDmg);
    const output = path.join(tmpdir(), `ghostex-live-${version}-${artifact.feed}`);
    await run(`curl -fsSL ${shellQuote(artifact.feedUrl)} -o ${shellQuote(output)}`);
    await run(`xmllint --noout ${shellQuote(output)}`);
    const liveSignature = await capture(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@*[local-name()='edSignature'])[1])" ${shellQuote(output)}`
    );
    const liveDmgUrl = await capture(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@url)[1])" ${shellQuote(output)}`
    );
    const expectedDmgUrl = `https://github.com/${config.githubRepo}/releases/download/v${version}/${dmgAssetName}`;
    if (liveDmgUrl !== expectedDmgUrl) {
      throw new ReleaseError(`Live appcast enclosure URL mismatch: ${liveDmgUrl} (expected ${expectedDmgUrl})`);
    }
    await run(
      `${shellQuote(path.join(sparkleBinDir, 'sign_update'))} --verify ${shellQuote(artifact.finalDmg)} ${shellQuote(liveSignature)}`
    );
    await run(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" ${shellQuote(output)} | grep -Fx ${shellQuote(String(buildVersion))}`
    );
    await run(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" ${shellQuote(output)} | grep -Fx ${shellQuote(version)}`
    );
    await run(
      `rg ${shellQuote(`${dmgAssetName}|sparkle:version|sparkle:shortVersionString|sparkle-signatures`)} ${shellQuote(output)} -g '!node_modules/**' -g '!dist/**' -g '!build/**' -g '!coverage/**' -g '!.git/**'`
    );
    await run(`curl -I -L --fail ${shellQuote(expectedDmgUrl)} | sed -n '1,12p'`);

    const release = await readGithubRelease(version);
    const dmgAsset = release.assets.find((asset) => asset.name === dmgAssetName);
    if (!dmgAsset) {
      throw new ReleaseError(`GitHub release v${version} is missing ${dmgAssetName}.`);
    }
    const liveDigest = parseGithubAssetSha(dmgAsset);
    if (liveDigest) {
      if (liveDigest !== artifact.sha256) {
        throw new ReleaseError(
          `GitHub asset digest ${liveDigest} does not match local ${dmgAssetName} SHA256 ${artifact.sha256}.`
        );
      }
      console.log(
        `GitHub API digest matches local ${dmgAssetName} SHA256 (${artifact.sha256}); skipping duplicate download.`
      );
    } else {
      console.warn('GitHub API returned no asset digest; downloading the live DMG once to verify.');
      const liveDmgPath = path.join(tmpdir(), `ghostex-live-${version}-${dmgAssetName}`);
      await run(`curl -fsSL ${shellQuote(liveDmgUrl)} -o ${shellQuote(liveDmgPath)}`, {
        timeoutMs: releaseTimeouts.brewFetchMs,
      });
      const liveSha = await capture(`shasum -a 256 ${shellQuote(liveDmgPath)} | awk '{print $1}'`);
      if (liveSha !== artifact.sha256) {
        throw new ReleaseError(`Live DMG SHA256 ${liveSha} does not match local artifact ${artifact.sha256}.`);
      }
    }
  }
  await run(
    `gh release view ${shellQuote(`v${version}`)} --repo ${shellQuote(config.githubRepo)} --json tagName,name,url,assets --jq '{tagName,name,url,assets:[.assets[]|{name,size,digest,url}]}'`
  );
}

async function findAndroidBuildTool(tool) {
  const roots = [
    process.env.ANDROID_HOME,
    process.env.ANDROID_SDK_ROOT,
    path.join(process.env.HOME ?? '', 'Library/Android/sdk'),
    '/opt/homebrew/share/android-commandlinetools',
  ].filter(Boolean);
  const existingRoots = [...new Set(roots)].filter((root) => existsSync(root));
  const output =
    existingRoots.length === 0
      ? ''
      : await capture(
          `find ${existingRoots.map(shellQuote).join(' ')} -path '*/build-tools/*/${tool}' -print 2>/dev/null`
        );
  const toolPath = selectLatestAndroidBuildTool(output.split(/\r?\n/), tool);
  if (!toolPath) {
    throw new ReleaseError(`Could not find Android build tool: ${tool}`);
  }
  return toolPath;
}

function selectLatestAndroidBuildTool(paths, tool) {
  const matches = paths
    .map((entry) => entry.trim())
    .filter((entry) => entry.endsWith(`/build-tools/${androidBuildToolVersion(entry, tool)}/${tool}`));
  matches.sort((left, right) => {
    const leftVersion = androidBuildToolVersion(left, tool);
    const rightVersion = androidBuildToolVersion(right, tool);
    return (
      leftVersion.localeCompare(rightVersion, undefined, { numeric: true, sensitivity: 'base' }) ||
      left.localeCompare(right)
    );
  });
  return matches.at(-1) ?? '';
}

function androidBuildToolVersion(toolPath, tool) {
  const marker = '/build-tools/';
  const markerIndex = toolPath.lastIndexOf(marker);
  if (markerIndex === -1 || !toolPath.endsWith(`/${tool}`)) {
    return '';
  }
  const versionStart = markerIndex + marker.length;
  const versionEnd = toolPath.indexOf('/', versionStart);
  return versionEnd === -1 ? '' : toolPath.slice(versionStart, versionEnd);
}

async function buildAndUploadAndroidRelease(version, buildVersion) {
  logStep('Build and upload React Native Android APK');
  const script = [
    'set -euo pipefail',
    'set -a',
    `source ${shellQuote(config.androidSigningEnvFile)}`,
    'set +a',
    `cd ${shellQuote(path.join(repoRoot, 'apps', 'mobile', 'app'))}`,
    'bun install --frozen-lockfile',
    `cd ${shellQuote(repoRoot)}`,
    `tooling/release-gpui/android.sh ${shellQuote(version)}`,
  ].join('\n');
  await run(`/bin/zsh -lc ${shellQuote(script)}`, { timeoutMs: releaseTimeouts.androidMs });

  const apk = path.join(repoRoot, 'build/release-gpui', version, 'android', config.androidApkAssetName);
  if (!existsSync(apk)) {
    throw new ReleaseError(`React Native Android release APK was not produced: ${apk}`);
  }
  const apksigner = await findAndroidBuildTool('apksigner');
  const aapt = await findAndroidBuildTool('aapt');
  await run(
    `/bin/bash -lc ${shellQuote(`set -o pipefail; ${shellQuote(apksigner)} verify --verbose --print-certs ${shellQuote(apk)} | sed -n '1,80p'`)}`
  );
  await run(
    `${shellQuote(aapt)} dump badging ${shellQuote(apk)} | rg ${shellQuote(`package: name='io.ghostex' versionCode='${buildVersion}' versionName='${version}'`)}`
  );
  const sha256 = await capture(`shasum -a 256 ${shellQuote(apk)} | awk '{print $1}'`);
  const stableApk = path.join(tmpdir(), config.androidApkAssetName);
  /*
   * CDXC:AndroidRelease 2026-06-14-09:07:
   * GitHub CLI upload labels were not reliable for the Android APK asset name
   * during the 4.12.0 release. Copy the signed universal APK to the stable
   * filename first, then upload that file directly so the public URL remains
   * ghostex-android.apk across releases.
   */
  await run(`cp ${shellQuote(apk)} ${shellQuote(stableApk)}`);
  await run(
    `gh release upload ${shellQuote(`v${version}`)} ${shellQuote(stableApk)} --repo ${shellQuote(config.githubRepo)} --clobber`,
    { timeoutMs: releaseTimeouts.brewFetchMs }
  );
  return {
    name: config.androidApkAssetName,
    path: stableApk,
    sha256,
  };
}

async function readGithubRelease(version) {
  const json = await capture(
    `gh release view ${shellQuote(`v${version}`)} --repo ${shellQuote(config.githubRepo)} --json tagName,name,url,isDraft,isPrerelease,assets`
  );
  return JSON.parse(json);
}

function parseGithubAssetSha(asset) {
  const digest = asset?.digest;
  if (typeof digest === 'string' && digest.startsWith('sha256:')) {
    return digest.slice('sha256:'.length);
  }
  return null;
}

async function githubReleaseArtifactFromAssets(version, assets) {
  const assetName = `ghostex-${version}-arm64.dmg`;
  const asset = assets.find((entry) => entry.name === assetName);
  if (!asset) {
    throw new ReleaseError(`GitHub release v${version} is missing ${assetName}.`);
  }
  let sha256 = parseGithubAssetSha(asset);
  if (!sha256) {
    const downloadPath = path.join(tmpdir(), assetName);
    await run(`curl -fsSL ${shellQuote(asset.url)} -o ${shellQuote(downloadPath)}`, {
      timeoutMs: releaseTimeouts.brewFetchMs,
    });
    sha256 = await capture(`shasum -a 256 ${shellQuote(downloadPath)} | awk '{print $1}'`);
  }
  return {
    ...releaseArchitectures[0],
    finalDmg: assetName,
    sha256,
  };
}

async function gpuiArtifactFromAssets(version, assets) {
  const assetName = `ghostex-gpui-${version}-arm64.dmg`;
  const asset = assets.find((entry) => entry.name === assetName);
  if (!asset) {
    return null;
  }
  const sha256 = parseGithubAssetSha(asset);
  return sha256
    ? {
        ...gpuiReleaseEntry(version),
        finalDmg: assetName,
        sha256,
      }
    : null;
}

async function androidArtifactFromAssets(assets) {
  const asset = assets.find((entry) => entry.name === config.androidApkAssetName);
  if (!asset) {
    return null;
  }
  let sha256 = parseGithubAssetSha(asset);
  if (!sha256 && asset.url) {
    const downloadPath = path.join(tmpdir(), config.androidApkAssetName);
    await run(`curl -fsSL ${shellQuote(asset.url)} -o ${shellQuote(downloadPath)}`, {
      timeoutMs: releaseTimeouts.brewFetchMs,
    });
    sha256 = await capture(`shasum -a 256 ${shellQuote(downloadPath)} | awk '{print $1}'`);
  }
  return sha256
    ? {
        name: config.androidApkAssetName,
        sha256,
      }
    : null;
}

async function ensureLiveSparkleMatches(version, buildVersion) {
  if (isPrereleaseVersion(version)) {
    return;
  }
  const output = path.join(tmpdir(), `ghostex-live-${version}-${config.armFeed}`);
  await run(`curl -fsSL ${shellQuote(releaseArchitectures[0].feedUrl)} -o ${shellQuote(output)}`);
  await run(`xmllint --noout ${shellQuote(output)}`);
  await run(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" ${shellQuote(output)} | grep -Fx ${shellQuote(String(buildVersion))}`
  );
  await run(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" ${shellQuote(output)} | grep -Fx ${shellQuote(version)}`
  );
  await run(
    `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@url)[1])" ${shellQuote(output)} | grep -Fx ${shellQuote(`https://github.com/${config.githubRepo}/releases/download/v${version}/ghostex-${version}-arm64.dmg`)}`
  );
}

async function liveHomebrewCaskIsCurrent(version, sha256) {
  try {
    const liveCask = await capture(
      `curl -fsSL ${shellQuote(`https://raw.githubusercontent.com/maddada/homebrew-tap/main/${config.caskPath}`)}`,
      { timeoutMs: releaseTimeouts.brewFetchMs }
    );
    validateGhostexCask(liveCask, { version, sha256 });
    return true;
  } catch {
    return false;
  }
}

async function resumeRelease(version, buildVersion, options) {
  logStep('Resume release');
  await ensureGhAuthForRelease();
  await ensureCleanWorktree();
  await ensureReleaseBranchSynced(options.releaseBranch);
  await ensureTagExists(version);
  await ensureReleaseExists(version);
  if (!options.skipSparkle) {
    await ensureLiveSparkleMatches(version, buildVersion);
  }

  const release = await readGithubRelease(version);
  const armArtifact = await githubReleaseArtifactFromAssets(version, release.assets);
  const artifacts = [armArtifact];
  const gpuiArtifact = await gpuiArtifactFromAssets(version, release.assets);
  let androidArtifact = await androidArtifactFromAssets(release.assets);

  let onDemandAssets = await onDemandAssetsFromReleaseAssets(release.assets);
  if (!onDemandAssets) {
    const buildManifest = await readOnDemandBuildManifest(version).catch(() => null);
    const uploadable = (buildManifest?.assets ?? []).filter((asset) => asset.path && existsSync(asset.path));
    if (uploadable.length === onDemandAssetNames.length) {
      await uploadOnDemandAssets(version, uploadable);
      onDemandAssets = uploadable.map(({ name, sha256 }) => ({ name, sha256 }));
    } else {
      throw new ReleaseError(
        `GitHub release v${version} is missing on-demand assets (${onDemandAssetNames.join(', ')}) and no local tarballs exist under ${onDemandAssetsDir(version)} to re-upload. Repair with --from publish-macos.`
      );
    }
  }
  if (onDemandAssets.some((asset) => !asset.sha256)) {
    throw new ReleaseError(
      `GitHub did not report digests for the on-demand assets of v${version}; cannot rebuild release notes safely.`
    );
  }

  if (!(await liveHomebrewCaskIsCurrent(version, armArtifact.sha256))) {
    await verifyHomebrewReleaseReadiness(version);
    await updateHomebrew(version, artifacts, options);
  } else {
    console.log(`Homebrew cask is already current for ${version}.`);
  }

  if (!options.skipAndroid) {
    if (androidArtifact) {
      console.log(`Android release asset ${config.androidApkAssetName} is already present.`);
    } else {
      await ensureAndroidReleaseReadiness(version, buildVersion, options);
      androidArtifact = await buildAndUploadAndroidRelease(version, buildVersion);
    }
  }
  await updateGithubReleaseNotes(version, artifacts, { androidArtifact, onDemandAssets, gpuiArtifact });

  logStep('Resume complete');
  console.log(`Release URL: https://github.com/${config.githubRepo}/releases/tag/v${version}`);
}

async function updateHomebrew(version, artifacts, options) {
  logStep('Update Homebrew tap');
  const tapDir = await mkdtemp(path.join(tmpdir(), `ghostex-${version}-homebrew-tap-`));
  await run(`git clone ${shellQuote(config.tapRepo)} ${shellQuote(tapDir)}`);

  const caskFile = path.join(tapDir, config.caskPath);
  const existingCask = await readFile(caskFile, 'utf8');
  const arm = artifacts.find((entry) => entry.arch === 'arm64');
  if (!arm) {
    throw new ReleaseError('arm64 release artifact is required for the Homebrew cask.');
  }

  let cask = renderGhostexCaskForTap(existingCask, { version, sha256: arm.sha256 });
  validateGhostexCask(cask, { version, sha256: arm.sha256 });

  await writeFile(caskFile, cask);
  await run(`ruby -c ${shellQuote(config.caskPath)}`, { cwd: tapDir });
  /*
   CDXC:ReleaseAutomation 2026-05-29-19:30:
   Homebrew style can fail on autocorrectable blank-line offenses after the cask
   generator inserts the gx preflight block. Auto-fix those before the strict
   style check so a successful GitHub release is not blocked by formatting.

   CDXC:HomebrewRelease 2026-06-10-09:47:
   Homebrew's API install path can fail on macOS beta host identifiers before it
   reads the freshly pushed tap cask. Disable install-from-API for style/info/fetch
   validation and treat unrelated brew update failures as non-blocking once the
   Ghostex cask validates directly.

   CDXC:HomebrewRelease 2026-06-21-13:20:
   GitHub issue #49 showed current Homebrew treats `depends_on macos: :ventura`
   as the Ventura-or-newer floor and warns on the old comparison string, so the
   release renderer should use normal style validation and keep the symbol form.
   */
  const localStyleValidationAvailable = await runOptionalHomebrewHostValidation(
    `HOMEBREW_NO_INSTALL_FROM_API=1 brew style --fix ${shellQuote(config.caskPath)}`,
    { cwd: tapDir }
  );
  if (localStyleValidationAvailable) {
    await runOptionalHomebrewHostValidation(
      `HOMEBREW_NO_INSTALL_FROM_API=1 brew style ${shellQuote(config.caskPath)}`,
      { cwd: tapDir }
    );
  }
  cask = await readFile(caskFile, 'utf8');
  validateGhostexCask(cask, { version, sha256: arm.sha256 });
  await run(`git diff -- ${shellQuote(config.caskPath)}`, { cwd: tapDir });
  await run(`git add ${shellQuote(config.caskPath)}`, { cwd: tapDir });
  await run(`git commit -m ${shellQuote(`Update ghostex cask to ${version}`)}`, { cwd: tapDir });
  await runGitNetwork('git push origin main', { cwd: tapDir });
  const tapCommit = await capture('git rev-parse HEAD', { cwd: tapDir });

  if (!options.skipBrewFetch) {
    let localBrewValidationAvailable = true;
    let shouldValidateLiveCaskFromTap = false;
    try {
      await run('HOMEBREW_NO_INSTALL_FROM_API=1 brew update --force', { timeoutMs: releaseTimeouts.brewFetchMs });
    } catch (error) {
      if (isHomebrewHostToolchainVersionError(error)) {
        localBrewValidationAvailable = false;
        console.warn(
          "Warning: skipping local Homebrew fetch validation because this host's Xcode/CLT is below Homebrew's current minimum."
        );
      } else {
        console.warn(
          `Warning: brew update failed; continuing with direct Ghostex cask validation.\n${String(error.message ?? error)}`
        );
      }
    }
    if (localBrewValidationAvailable) {
      localBrewValidationAvailable = await runOptionalHomebrewHostValidation(
        'HOMEBREW_NO_INSTALL_FROM_API=1 brew info --cask maddada/tap/ghostex',
        {
          timeoutMs: releaseTimeouts.brewFetchMs,
        }
      );
    }
    if (localBrewValidationAvailable) {
      try {
        const liveCask = await capture('HOMEBREW_NO_INSTALL_FROM_API=1 brew cat --cask maddada/tap/ghostex', {
          timeoutMs: releaseTimeouts.brewFetchMs,
        });
        validateGhostexCask(liveCask, { version, sha256: arm.sha256 });
      } catch (error) {
        if (!isHomebrewHostToolchainVersionError(error)) {
          throw error;
        }
        localBrewValidationAvailable = false;
        console.warn(
          "Warning: skipping local Homebrew cask read because this host's Xcode/CLT is below Homebrew's current minimum."
        );
      }
    }
    let dmgCachePath = null;
    if (localBrewValidationAvailable) {
      const localFetchValidationAvailable = await runOptionalHomebrewHostValidation(
        'HOMEBREW_NO_INSTALL_FROM_API=1 brew fetch --force --cask --arch=arm maddada/tap/ghostex',
        {
          timeoutMs: releaseTimeouts.brewFetchMs,
        }
      );
      shouldValidateLiveCaskFromTap = !localFetchValidationAvailable;
      if (localFetchValidationAvailable) {
        /*
         CDXC:ReleaseAutomation 2026-07-02-14:10:
         brew fetch is the release's single full live download. Reuse its
         cached artifact to prove the published bytes equal the local DMG and
         hand the path to final verification so nothing downloads the DMG
         again.
         */
        try {
          const cachedDownload = await capture(
            'HOMEBREW_NO_INSTALL_FROM_API=1 brew --cache --cask --arch=arm maddada/tap/ghostex',
            { timeoutMs: 60_000 }
          );
          if (cachedDownload && existsSync(cachedDownload)) {
            const cachedSha = await capture(`shasum -a 256 ${shellQuote(cachedDownload)} | awk '{print $1}'`);
            if (cachedSha !== arm.sha256) {
              throw new ReleaseError(
                `Homebrew-fetched DMG SHA256 ${cachedSha} does not match the released artifact ${arm.sha256}.`
              );
            }
            console.log(`Homebrew-fetched DMG matches release SHA256; cached at ${cachedDownload}.`);
            dmgCachePath = cachedDownload;
          }
        } catch (error) {
          if (error instanceof ReleaseError) {
            throw error;
          }
          console.warn(`Could not resolve the Homebrew download cache path: ${String(error.message ?? error)}`);
        }
      }
    } else {
      shouldValidateLiveCaskFromTap = true;
    }
    if (shouldValidateLiveCaskFromTap) {
      const liveCask = await capture(
        `curl -fsSL ${shellQuote(`https://raw.githubusercontent.com/maddada/homebrew-tap/main/${config.caskPath}`)}`,
        { timeoutMs: releaseTimeouts.brewFetchMs }
      );
      validateGhostexCask(liveCask, { version, sha256: arm.sha256 });
      console.warn('Validated the live Homebrew cask from the tap because local brew fetch validation is unavailable.');
    }
    return { dmgCachePath, tapCommit, tapDir };
  }

  return { dmgCachePath: null, tapCommit, tapDir };
}

/**
 * CDXC:CliBranding 2026-05-26-15:11:
 * Homebrew releases should install `ghostex` and the new `gx` short alias, not
 * the older `gtx` alias. Check for an existing non-Ghostex `gx` binary before
 * installing wrappers so setup does not silently claim a command name another
 * tool owns.
 *
 * CDXC:CliInstall 2026-06-12-09:31:
 * Homebrew must install ghostex/gx as wrapper files in HOMEBREW_PREFIX/bin,
 * not binary symlinks into Ghostex.app. Direct execution of app-bundled scripts
 * can be killed during macOS policy assessment before Node starts. Best-effort
 * clear provenance/quarantine xattrs from the wrappers because replaced
 * symlinks can carry policy metadata into the new files on some macOS builds.
 *
 * CDXC:ReleaseAutomation 2026-06-14-09:07:
 * The Ghostex tap cask is owned release output, so render it from this canonical
 * template instead of regex-normalizing whatever shape is currently in the tap.
 * This keeps arm64-only distribution, the explicit Ventura floor, and wrapper
 * install hooks deterministic while still allowing the compatibility guard that
 * recognizes old Web/cli Ghostex-owned commands.
 */
function renderGhostexCaskForTap(existingCask, release) {
  if (!/^\s*cask "ghostex" do/m.test(existingCask)) {
    throw new ReleaseError('Homebrew tap checkout does not contain the Ghostex cask.');
  }
  return renderGhostexCask(release);
}

function renderGhostexCask({ version, sha256 }) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new ReleaseError(`Cannot render Ghostex cask for invalid version: ${version}`);
  }
  if (!/^[0-9a-f]{64}$/.test(sha256)) {
    throw new ReleaseError(`Cannot render Ghostex cask with invalid sha256: ${sha256}`);
  }
  const cask = `cask "ghostex" do
  version "${version}"
  sha256 "${sha256}"

  url "https://github.com/maddada/Ghostex/releases/download/v#{version}/ghostex-#{version}-arm64.dmg"
  name "Ghostex"
  desc "Workspace and session UI for agent terminals"
  homepage "https://github.com/maddada/Ghostex"

  conflicts_with cask: "zmux"
  # CDXC:MacRelease 2026-06-21-13:20: GitHub issue #49 showed current
  # Homebrew treats the symbol form as the Ventura-or-newer floor, so keep this
  # syntax to avoid a warning on every brew invocation.
  depends_on arch: :arm64
  depends_on macos: :ventura

  app "ghostex.app"

  # CDXC:CliBranding 2026-05-26-15:11: Install gx only when another tool does not already own that command name.
  # CDXC:CliInstall 2026-06-12-09:31: Homebrew writes wrapper files in
  # HOMEBREW_PREFIX/bin instead of binary symlinks into Ghostex.app because
  # macOS can kill direct app-bundled script execution during policy assessment.
  # CDXC:HomebrewRelease 2026-06-16-20:54: Keep preflight errors wrapped so
  # automated tap pushes pass brew style after publication.
  preflight do
    commands = ["ghostex", "gx"]
    commands.each do |command|
      command_candidates = [HOMEBREW_PREFIX/"bin/#{command}"]
      ENV.fetch("PATH", "").split(File::PATH_SEPARATOR).each do |entry|
        command_candidates << (Pathname(entry)/command) unless entry.empty?
      end

      command_candidates.uniq.each do |command_path|
        next if [command_path.exist?, command_path.symlink?].none?

        command_target = command_path.symlink? ? command_path.readlink.to_s : command_path.to_s
        command_content = command_path.file? ? command_path.read : ""
        if command_content.include?("CDXC:CliInstall 2026-06-12-09:31") && (command_content.include?("ghostex-cli.mjs") || command_content.include?("/Resources/CLI/ghostex"))
          next
        end
        next if command_target.include?("ghostex.app/Contents/Resources/CLI/#{command}")
        next if command_target.include?("ghostex.app/Contents/Resources/Web/cli/#{command}")
        next if command == "ghostex" && command_target.include?("ghostex.app/Contents/MacOS/ghostex")

        raise [
          "Ghostex cannot install the #{command} CLI because #{command_path} already exists.",
          "Remove or rename the existing #{command} command, then reinstall Ghostex.",
        ].join(" ")
      end
    end
  end

  postflight do
    cli_binary = "#{appdir}/ghostex.app/Contents/Resources/CLI/ghostex"
    bin_dir = HOMEBREW_PREFIX/"bin"
    policy_attributes = ["com.apple.provenance", "com.apple.quarantine"]
    bin_dir.mkpath

    ["ghostex", "gx"].each do |command|
      command_path = bin_dir/command
      if command_path.symlink?
        command_path.delete
      elsif command_path.exist?
        command_content = command_path.file? ? command_path.read : ""
        if command_content.include?("CDXC:CliInstall 2026-06-12-09:31") && (command_content.include?("ghostex-cli.mjs") || command_content.include?("/Resources/CLI/ghostex"))
          command_path.delete
        end
      end

      command_path.write <<~EOS
        #!/bin/bash
        set -euo pipefail
        # CDXC:CliInstall 2026-06-12-09:31: Public PATH commands live outside Ghostex.app so macOS does not directly execute app-bundled shell scripts during policy assessment.
        exec "#{cli_binary}" "$@"
      EOS
      command_path.chmod 0755
      policy_attributes.each do |attribute|
        system "/usr/bin/xattr", "-d", attribute, command_path.to_s, out: File::NULL, err: File::NULL
      end
    end
  end

  uninstall_preflight do
    ["ghostex", "gx"].each do |command|
      command_path = HOMEBREW_PREFIX/"bin/#{command}"
      next if !command_path.exist? || !command_path.file?

      command_content = command_path.read
      if command_content.include?("CDXC:CliInstall 2026-06-12-09:31") && (command_content.include?("ghostex-cli.mjs") || command_content.include?("/Resources/CLI/ghostex"))
        command_path.delete
      end
    end
  end

  zap trash: [
    "~/Library/Application Support/com.madda.zmux.host",
    "~/Library/Preferences/com.madda.zmux.host.plist",
    "~/Library/Saved Application State/com.madda.zmux.host.savedState",
  ]
end
`;
  validateGhostexCask(cask, { version, sha256 });
  return cask;
}

function validateGhostexCask(cask, { version, sha256 }) {
  for (const required of [
    `version "${version}"`,
    `sha256 "${sha256}"`,
    'url "https://github.com/maddada/Ghostex/releases/download/v#{version}/ghostex-#{version}-arm64.dmg"',
    'depends_on arch: :arm64',
    'depends_on macos: :ventura',
    'preflight do',
    'postflight do',
    'uninstall_preflight do',
    'CDXC:CliInstall 2026-06-12-09:31',
    'exec "#{cli_binary}" "$@"',
  ]) {
    if (!cask.includes(required)) {
      throw new ReleaseError(`Ghostex cask is missing required stanza: ${required}`);
    }
  }
  if (/^\s*binary\s+"/m.test(cask)) {
    throw new ReleaseError('Ghostex cask must install wrapper files, not Homebrew binary aliases.');
  }
  if (cask.includes('x86_64') || cask.includes('#{arch}') || cask.includes('intel:')) {
    throw new ReleaseError('Ghostex cask still contains Intel release distribution stanzas.');
  }
  return true;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(usage().trim());
    return;
  }

  process.chdir(repoRoot);
  const version = options.version;
  const buildVersion = releaseBuildVersion(version);

  console.log(`Ghostex local release: ${version}`);
  console.log(`Sparkle build version: ${buildVersion}`);
  const releaseStartedAt = Date.now();

  if (options.resume) {
    await resumeRelease(version, buildVersion, options);
    return;
  }

  await ensureCleanWorktree();

  if (
    !options.noTerminalDelegate &&
    !process.env.GHOSTEX_RELEASE_TERMINAL_DELEGATED &&
    !(await agentShellCredentialsReady())
  ) {
    console.warn('Agent shell cannot access Developer ID signing or notary credentials. Delegating to Terminal.app.');
    await delegateReleaseToTerminal(version, options);
    return;
  }

  const state = await loadOrCreateReleaseState(version, buildVersion, options);
  try {
    assertReleaseWithinOverallBudget(releaseStartedAt, 'preflight');
    await runReleasePhase(state, 'preflight', options, async () => {
      await preflight(version, buildVersion, options);
    });

    assertReleaseWithinOverallBudget(releaseStartedAt, 'prepare-remote-linux');
    await runReleasePhase(state, 'prepare-remote-linux', options, async () => {
      /*
       CDXC:ReleaseAutomation 2026-07-02-14:10:
       The builder script exits quickly when both Ubuntu packages already match
       HEAD; when they are stale it runs the checked-in Zig cross-build recipe
       instead of failing with a rebuild-by-hand hint.
       */
      await runWithHeartbeat('bash tooling/build-remote-gxserver-linux-release.sh --arch all', {
        label: 'remote Linux package build',
        timeoutMs: 30 * 60 * 1000,
        /*
         CDXC:AnonymousAnalytics 2026-08-26:
         The remote packages bake this into gxserver (server/build.rs), exactly
         like the GPUI leg does for the desktop crate. The gxserver crate's own
         Cargo version is a 0.1.0 placeholder, so without it every shipped remote
         daemon would report the same version forever.
         */
        env: { ...process.env, GHOSTEX_GPUI_MARKETING_VERSION: version },
      });
      await ensureRemoteGxserverLinuxPackagesReady();
    });

    assertReleaseWithinOverallBudget(releaseStartedAt, 'publish-macos');
    const macos = await runReleasePhase(state, 'publish-macos', options, async () => {
      await bumpReleaseMetadata(version, buildVersion, options);
      const { artifactDir, artifacts, gpuiArtifacts, onDemandAssets } = await buildAndPackage(
        version,
        buildVersion,
        options
      );
      const allArtifacts = [...artifacts, ...(gpuiArtifacts ?? [])];
      let sparkleBinDir = null;
      if (options.skipSparkle) {
        logStep('Skip Sparkle feeds');
        console.log('Sparkle appcasts were not updated for this release.');
      } else {
        sparkleBinDir = await findAndVerifySparkleBinDir();
        await updateSparkleFeeds(version, buildVersion, sparkleBinDir, allArtifacts);
      }
      const releaseCommit = await commitReleaseMetadata(version, options);
      let releaseUrl = '(not published; --no-push was used)';
      if (!options.noPush) {
        releaseUrl = await createGithubRelease(version, allArtifacts, options, {
          onDemandAssets,
          gpuiArtifact: gpuiArtifacts?.[0] ?? null,
        });
        if (!options.skipSparkle) {
          await validateLiveSparkleAndAssets(version, buildVersion, sparkleBinDir, allArtifacts);
        }
      }
      return { artifactDir, artifacts, gpuiArtifacts, onDemandAssets, releaseCommit, releaseUrl };
    });

    if (!options.noPush) {
      assertReleaseWithinOverallBudget(releaseStartedAt, 'publish-android');
      await runReleasePhase(state, 'publish-android', options, async () => {
        let androidArtifact = null;
        if (options.skipAndroid) {
          logStep('Skip Android release');
        } else {
          androidArtifact = await buildAndUploadAndroidRelease(version, buildVersion);
        }
        await updateGithubReleaseNotes(version, macos.artifacts, {
          androidArtifact,
          onDemandAssets: macos.onDemandAssets,
          gpuiArtifact: macos.gpuiArtifacts?.[0] ?? null,
        });
        return { androidArtifact };
      });

      assertReleaseWithinOverallBudget(releaseStartedAt, 'publish-homebrew');
      const brew = await runReleasePhase(state, 'publish-homebrew', options, async () => {
        const brewResult = await updateHomebrew(version, macos.artifacts, options);
        return { dmgCachePath: brewResult.dmgCachePath ?? null, tapCommit: brewResult.tapCommit };
      });

      assertReleaseWithinOverallBudget(releaseStartedAt, 'verify-live');
      await runReleasePhase(state, 'verify-live', options, async () => {
        const localDmg = macos.artifacts?.find((entry) => entry.arch === 'arm64')?.finalDmg;
        const dmgPath = [brew.dmgCachePath, localDmg].find((candidate) => candidate && existsSync(candidate)) ?? null;
        const verifyCommand = [
          'node tooling/release-final-verify.mjs',
          shellQuote(version),
          '--skip-brew-fetch',
          ...(dmgPath ? ['--dmg', shellQuote(dmgPath)] : []),
          ...(options.skipAndroid ? ['--skip-android'] : []),
          ...(options.skipSparkle ? ['--skip-sparkle'] : []),
        ].join(' ');
        await runWithHeartbeat(verifyCommand, { label: 'final live verification', timeoutMs: 20 * 60 * 1000 });
        return { dmgPath };
      });
    }

    logStep('Release complete');
    console.log(`Release URL: ${macos.releaseUrl}`);
    console.log(`Release commit: ${macos.releaseCommit}`);
    const brewOutputs = state.phases['publish-homebrew']?.outputs;
    console.log(`Homebrew tap commit: ${brewOutputs?.tapCommit ?? '(not updated)'}`);
    console.log(`Artifact directory: ${macos.artifactDir}`);
    for (const artifact of [...(macos.artifacts ?? []), ...(macos.gpuiArtifacts ?? [])]) {
      console.log(`${artifact.kind === 'gpui' ? 'gpui-' : ''}${artifact.arch}:`);
      console.log(`  DMG: ${artifact.finalDmg}`);
      console.log(`  SHA256: ${artifact.sha256}`);
      console.log(`  Notary: ${artifact.notarySubmissionId} (${artifact.notaryStatus})`);
    }
    for (const asset of macos.onDemandAssets ?? []) {
      console.log(`On-demand asset: ${asset.name} (SHA256 ${asset.sha256})`);
    }
    console.log(`Install: ${config.installCommand}`);
  } finally {
    printPhaseTimingSummary(state);
  }
}

export {
  ReleaseError,
  buildGithubReleaseNotes,
  extractChangelogSectionFromText,
  isHomebrewHostToolchainVersionError,
  missingRemoteGxserverLinuxPackageResources,
  onDemandAssetNames,
  releaseBuildVersion,
  releasePhaseNames,
  renderGhostexCask,
  renderGhostexCaskForTap,
  selectLatestAndroidBuildTool,
  validateGhostexCask,
  validateMajorMinorReleaseNotes,
};

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error('');
    console.error(error instanceof ReleaseError ? error.message : error);
    process.exitCode = 1;
  });
}
