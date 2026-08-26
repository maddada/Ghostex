#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { access, chmod, cp, mkdir, mkdtemp, readFile, readdir, realpath, rm, stat, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { BEADS_PACKAGE_ID, stageBeadsRelease } from '../tooling/beads-release.mjs';
import { smokeTestPackagedBeads } from '../tooling/smoke-test-packaged-beads.mjs';
import { execFile, spawn } from 'node:child_process';

const execFileAsync = promisify(execFile);
const scriptPath = fileURLToPath(import.meta.url);
const gxserverRoot = path.dirname(scriptPath);
const repoRoot = path.resolve(gxserverRoot, '..');

/*
 * CDXC:RemoteMinimalDeps 2026-07-13:
 * Remote hosts must not need a specific glibc/libstdc++ floor, so the Rust
 * binaries (gxserver, ghostex) build against musl and link
 * statically, matching the already-static zmx (Zig musl). bd is the
 * checksum-verified schema-compatible Beads binary with embedded Dolt support.
 */
const archConfigs = {
  x64: {
    elfMachine: 0x3e,
    rustTarget: 'x86_64-unknown-linux-musl',
    zigTarget: 'x86_64-linux-musl',
  },
  arm64: {
    elfMachine: 0xb7,
    rustTarget: 'aarch64-unknown-linux-musl',
    zigTarget: 'aarch64-linux-musl',
  },
};

const helpText = `
Usage: node gxserver-rs/package-remote-linux.mjs [--arch x64|arm64|all] [--out <dir>]

Builds the self-contained Linux remote gxserver package that the macOS app
stages as Web/gxserver-linux-<arch> and uploads to Ubuntu after the user clicks
Install gxserver.

Run this on Ubuntu or in Linux CI. The default output is:
  build/remote-gxserver-linux/<arch>/package

Inputs can be overridden with:
  --zmx-root <dir>       default: zmx
  --out-root <dir>       default for --arch all: build/remote-gxserver-linux
  --rust-target <triple> default: arch-specific Linux musl target (static)
  --zig-target <triple>  default: arch-specific Linux musl target
  --zmx-zig-bin <path>   default: ZMX_ZIG, ZIG, or zig
  --allow-cross          allow running outside Linux when cross toolchains are configured
`;

main().catch((error) => {
  console.error(error?.message || String(error));
  process.exit(1);
});

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(helpText.trimStart());
    return;
  }

  const requestedArch = normalizeArch(options.arch || process.arch);
  const arches = requestedArch === 'all' ? ['x64', 'arm64'] : [requestedArch];
  if (arches.length > 1 && options.out) {
    throw new Error(
      '--out can only be used with one --arch value. Use --out-root or omit --out when building all Linux arches.'
    );
  }

  /*
   * CDXC:RemoteUbuntuPackaging 2026-06-29-18:58:
   * Ubuntu remote installs need x64 and arm64 server packages from the same packaging entry point. Keep the default host-arch build for existing single-arch CI, and add explicit --arch all so release builders can produce both deterministic build/remote-gxserver-linux/<arch>/package outputs before macOS and GPUI staging.
   */
  for (const arch of arches) {
    await buildLinuxPackageForArch({ arch, options });
  }
}

async function buildLinuxPackageForArch({ arch, options }) {
  const archConfig = archConfigs[arch];
  if (!archConfig) {
    throw new Error(`Unsupported Linux package arch: ${options.arch || process.arch}`);
  }
  if (process.platform !== 'linux' && !options.allowCross) {
    throw new Error(
      'Remote gxserver Linux packages must be built on Ubuntu/Linux CI, or pass --allow-cross after configuring Rust, Zig, and C toolchains for Linux.'
    );
  }

  const outputDir = path.resolve(
    repoRoot,
    options.out ||
      (options.outRoot
        ? path.join(options.outRoot, arch, 'package')
        : path.join('build', 'remote-gxserver-linux', arch, 'package'))
  );
  await assertSafeOutputDir(outputDir);

  const workRoot = await mkdtemp(path.join(os.tmpdir(), `ghostex-remote-gxserver-${arch}-`));
  try {
    const zmxZigBin = await resolveZigBinary({
      candidates: [
        options.zmxZigBin,
        process.env.ZMX_ZIG,
        process.env.ZIG,
        path.join(os.homedir(), 'apps', `zig-${zigHostArch()}-linux-0.16.0`, 'zig'),
        'zig',
      ],
      label: 'Zig 0.16.x for zmx',
      versionMatches: (version) => /^0\.16\./u.test(version),
    });
    const config = {
      ...archConfig,
      arch,
      beadsVersion: BEADS_PACKAGE_ID,
      packageVersion: options.packageVersion || (await gxserverPackageVersion()),
      rustTarget: options.rustTarget || archConfig.rustTarget,
      sourceDirty: await gitSourceDirty(repoRoot),
      sourceRevision: await gitOutput(repoRoot, ['rev-parse', 'HEAD'], 'unknown'),
      zmxRoot: path.resolve(repoRoot, options.zmxRoot || '.dependencies/zmx'),
      zmxZigBin,
      zigTarget: options.zigTarget || archConfig.zigTarget,
    };

    /*
     * CDXC:RemoteMachines 2026-06-23-10:07:
     * Ubuntu install must be a first-run package, not an on-host source build.
     * Build server and zmx and stage the pinned
     * schema-compatible bd release artifact into one package
     * directory so the macOS app
     * can upload it over SSH and start the same Rust control plane without PATH
     * fallbacks.
     *
     * CDXC:RemoteUbuntuPackaging 2026-06-29-19:45:
     * Release automation must reject stale prebuilt Linux packages. Record the
     * source git revision and dirty-state in build-identity.json so macOS
     * releases can prove x64 and arm64 Ubuntu payloads were built from the
     * commit being released before staging them into the app bundle.
     *
     * CDXC:RemoteUbuntuTui 2026-08-23:
     * The vendored ghostex-tui terminal app was removed from the repository, so
     * the remote package no longer builds or stages `bin/ghostex-tui`. A herdr
     * plugin replaces it (spec in docs/2026-08-23/tui2-herdr-plugin/).
     */
    await buildPackage({ config, outputDir, workRoot });
    console.log(`Remote gxserver Linux ${arch} package written to ${outputDir}`);
  } finally {
    await rm(workRoot, { force: true, recursive: true });
  }
}

function parseArgs(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '--help' || arg === '-h') {
      options.help = true;
      continue;
    }
    if (arg === '--allow-cross') {
      options.allowCross = true;
      continue;
    }
    if (!arg.startsWith('--')) {
      throw new Error(`Unexpected argument: ${arg}`);
    }
    const key = arg.slice(2).replace(/-([a-z])/gu, (_, letter) => letter.toUpperCase());
    const value = args[index + 1];
    if (!value || value.startsWith('--')) {
      throw new Error(`Missing value for ${arg}`);
    }
    options[key] = value;
    index += 1;
  }
  return options;
}

function normalizeArch(value) {
  const normalized = String(value || '')
    .trim()
    .toLowerCase();
  if (normalized === 'x64' || normalized === 'amd64' || normalized === 'x86_64') {
    return 'x64';
  }
  if (normalized === 'arm64' || normalized === 'aarch64') {
    return 'arm64';
  }
  if (normalized === 'all' || normalized === 'both') {
    return 'all';
  }
  return normalized;
}

async function buildPackage({ config, outputDir, workRoot }) {
  const stageDir = path.join(workRoot, 'stage');
  const binsDir = path.join(stageDir, 'bin');
  await rm(stageDir, { force: true, recursive: true });
  await mkdir(binsDir, { recursive: true });

  const { ghostexBin, gxserverBin } = await buildGxserver(config);
  /*
   * CDXC:AgentHistorySearch 2026-08-20:
   * zehn used to be built here with its own pinned Zig 0.16 toolchain. It is now
   * a Rust crate inside gxserver, so the Linux remote package needs exactly one
   * Zig toolchain. Since the zmx fork was re-ported onto upstream/main, that one
   * toolchain is 0.16.x — the same pin the vendored ghostty uses.
   */
  const zmxBin = await buildZigTool({
    binName: 'zmx',
    root: config.zmxRoot,
    target: config.zigTarget,
    workRoot,
    zigBin: config.zmxZigBin,
  });
  const bdBin = path.join(workRoot, 'bd');
  const prebuiltBeadsBinary = process.env.GHOSTEX_BEADS_PREBUILT_BINARY?.trim();
  if (prebuiltBeadsBinary) {
    await access(prebuiltBeadsBinary);
    await cp(prebuiltBeadsBinary, bdBin);
    await chmod(bdBin, 0o755);
  } else {
    await stageBeadsRelease({
      arch: config.arch,
      outputPath: bdBin,
      platform: 'linux',
    });
  }

  await copyExecutable(gxserverBin, path.join(binsDir, 'gxserver'), 'gxserver');
  await copyExecutable(ghostexBin, path.join(binsDir, 'ghostex'), 'ghostex');
  await copyExecutable(zmxBin, path.join(binsDir, 'zmx'), 'zmx');
  await copyExecutable(bdBin, path.join(binsDir, 'bd'), 'bd');

  /*
   * CDXC:RemoteMinimalDeps 2026-07-13:
   * The remote package used to ship portless, an npm-style package.json
   * manifest, dist/protocol JS/type exports, and the Node ghostex CLI under
   * CLI/. Portless is a macOS launchd-only feature the Linux daemon never
   * starts, nothing on the remote host consumes the manifest or protocol
   * exports (version identity lives in build-identity.json), and the public
   * `ghostex`/`gx` CLI is now the native Rust bin/ghostex built from the
   * same gxserver crate, so none of them are staged anymore. `gx` is
   * created as a symlink by `gxserver setup` at install time.
   */
  await validateLinuxPackage(stageDir, config);
  await writeBuildIdentity(stageDir, config.packageVersion, config);

  await rm(outputDir, { force: true, recursive: true });
  await mkdir(path.dirname(outputDir), { recursive: true });
  await cp(stageDir, outputDir, { recursive: true });
}

/*
CDXC:AnonymousAnalytics 2026-08-26:
The marketing version server/build.rs bakes into the binary. The gxserver crate's
own Cargo version is the placeholder 0.1.0 and has never tracked releases, so
without this every remote package would report the same `server_version` forever.
Resolution matches the desktop scripts: an explicit env wins, otherwise the root
package.json is the source of truth.
*/
async function resolveMarketingVersion() {
  const explicit = (process.env.GHOSTEX_GPUI_MARKETING_VERSION || '').trim();
  if (explicit) {
    return explicit;
  }
  const manifest = JSON.parse(await readFile(path.join(repoRoot, 'package.json'), 'utf8'));
  if (!manifest.version) {
    throw new Error('Could not read the marketing version from the root package.json.');
  }
  return manifest.version;
}

async function buildGxserver(config) {
  await run(
    'cargo',
    ['build', '--release', '--manifest-path', path.join(gxserverRoot, 'Cargo.toml'), '--target', config.rustTarget],
    { cwd: repoRoot, env: { GHOSTEX_GPUI_MARKETING_VERSION: await resolveMarketingVersion() } }
  );
  const releaseDir = path.join(cargoTargetRoot(gxserverRoot), config.rustTarget, 'release');
  return {
    ghostexBin: path.join(releaseDir, 'ghostex'),
    gxserverBin: path.join(releaseDir, 'gxserver'),
  };
}

function cargoTargetRoot(defaultRoot) {
  const configured = process.env.CARGO_TARGET_DIR?.trim();
  return configured ? path.resolve(repoRoot, configured) : path.join(defaultRoot, 'target');
}

async function buildZigTool({ binName, root, target, workRoot, zigBin }) {
  await assertDirectory(root, `${binName} root`);
  const prefix = path.join(workRoot, binName);
  await run(zigBin || 'zig', ['build', '-Doptimize=ReleaseSafe', `-Dtarget=${target}`, '--prefix', prefix], {
    cwd: root,
  });
  return path.join(prefix, 'bin', binName);
}

async function validateLinuxPackage(packageDir, config) {
  const requiredFiles = ['bin/gxserver', 'bin/ghostex', 'bin/zmx', 'bin/bd'];
  for (const relativePath of requiredFiles) {
    await assertFile(path.join(packageDir, relativePath), relativePath);
  }
  for (const relativePath of ['bin/gxserver', 'bin/ghostex', 'bin/zmx', 'bin/bd']) {
    const fullPath = path.join(packageDir, relativePath);
    if (!(await isElf(fullPath))) {
      throw new Error(`Linux remote package expected an ELF binary at ${relativePath}.`);
    }
    if ((await elfMachine(fullPath)) !== config.elfMachine) {
      throw new Error(`Linux remote package expected ${config.arch} ELF architecture at ${relativePath}.`);
    }
    await chmod(fullPath, 0o755);
  }

  /*
   * CDXC:LinuxRuntimePackaging 2026-07-18:
   * gxserver-generated managed attach commands require zmx's
   * --require-existing contract. Reject a mixed package at build time instead
   * of letting an older zmx parse the flag as a session name and make Android
   * terminals exit successfully immediately after attach.
   */
  const zmxPath = path.join(packageDir, 'bin', 'zmx');
  const zmxBytes = await readFile(zmxPath);
  if (!zmxBytes.includes(Buffer.from('--require-existing'))) {
    throw new Error('Linux remote package zmx does not support the required --require-existing attach contract.');
  }

  const hostCanRunBd = process.platform === 'linux' && normalizeArch(process.arch) === config.arch;
  if (hostCanRunBd) {
    await smokeTestPackagedBeads(path.join(packageDir, 'bin', 'bd'));
  } else if (process.env.GHOSTEX_REQUIRE_BEADS_SMOKE === '1') {
    throw new Error(
      `GHOSTEX_REQUIRE_BEADS_SMOKE=1 requires a native ${config.arch} Linux runner; ` +
        `current host is ${process.platform}/${process.arch}.`
    );
  }
}

async function writeBuildIdentity(packageDir, version, config = {}) {
  const hash = createHash('sha256');
  await hashDirectory(packageDir, packageDir, hash);
  const fingerprint = `sha256:${hash.digest('hex')}`;
  await writeFile(
    path.join(packageDir, 'build-identity.json'),
    `${JSON.stringify(
      {
        buildIdentity: `gxserver:${version}:${fingerprint}`,
        fingerprint,
        beadsVersion: config.beadsVersion || BEADS_PACKAGE_ID,
        packageVersion: version,
        sourceDirty: Boolean(config.sourceDirty),
        sourceRevision: config.sourceRevision || 'unknown',
      },
      null,
      2
    )}\n`,
    'utf8'
  );
}

async function hashDirectory(root, dir, hash) {
  const entries = await readdir(dir, { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name));
  for (const entry of entries) {
    const entryPath = path.join(dir, entry.name);
    const relativePath = path.relative(root, entryPath).split(path.sep).join('/');
    if (relativePath === 'build-identity.json') {
      continue;
    }
    if (entry.isDirectory()) {
      await hashDirectory(root, entryPath, hash);
      continue;
    }
    if (!entry.isFile() && !entry.isSymbolicLink()) {
      continue;
    }
    hash.update(relativePath);
    hash.update('\0');
    hash.update(await readFile(entryPath));
    hash.update('\0');
  }
}

async function gxserverPackageVersion() {
  const { stdout } = await execFileAsync(
    'cargo',
    ['metadata', '--format-version', '1', '--no-deps', '--manifest-path', path.join(gxserverRoot, 'Cargo.toml')],
    { cwd: repoRoot }
  );
  const metadata = JSON.parse(stdout);
  const rootPackageId = metadata.root_package_id || metadata.resolve?.root;
  const rootPackage =
    metadata.packages.find((pkg) => pkg.id === rootPackageId) ||
    metadata.packages.find((pkg) => pkg.name === 'gxserver') ||
    metadata.packages[0];
  if (!rootPackage?.version) {
    throw new Error('Could not read gxserver-rs package version from Cargo metadata.');
  }
  return rootPackage.version;
}

async function resolveZigBinary({ candidates, label, versionMatches }) {
  const tried = [];
  for (const candidate of [...new Set(candidates.filter(Boolean))]) {
    try {
      const { stdout } = await execFileAsync(candidate, ['version']);
      const version = stdout.trim();
      tried.push(`${candidate} (${version || 'unknown'})`);
      if (versionMatches(version)) return candidate;
    } catch {
      tried.push(`${candidate} (unavailable)`);
    }
  }
  throw new Error(`Could not find ${label}. Tried: ${tried.join(', ')}`);
}

function zigHostArch() {
  return process.arch === 'arm64' ? 'aarch64' : 'x86_64';
}

async function assertSafeOutputDir(outputDir) {
  const resolvedRepo = await realpath(repoRoot);
  const resolvedParent = await realpath(path.dirname(outputDir)).catch(() => path.dirname(outputDir));
  const unsafe = new Set([path.parse(outputDir).root, os.homedir(), resolvedRepo, path.dirname(resolvedRepo)]);
  if (unsafe.has(outputDir) || unsafe.has(resolvedParent)) {
    throw new Error(`Refusing to use unsafe package output directory: ${outputDir}`);
  }
}

async function copyExecutable(source, destination, label) {
  await assertFile(source, label);
  await mkdir(path.dirname(destination), { recursive: true });
  await cp(source, destination);
  await chmod(destination, 0o755);
}

async function assertDirectory(candidate, label) {
  const info = await stat(candidate).catch(() => undefined);
  if (!info?.isDirectory()) {
    throw new Error(`${label} is missing or not a directory: ${candidate}`);
  }
}

async function assertFile(candidate, label) {
  const info = await stat(candidate).catch(() => undefined);
  if (!info?.isFile()) {
    throw new Error(`${label} is missing or not a file: ${candidate}`);
  }
}

async function fileExists(candidate) {
  try {
    await access(candidate);
    return true;
  } catch {
    return false;
  }
}

async function isElf(candidate) {
  const data = await readFile(candidate).catch(() => Buffer.alloc(0));
  return data.length >= 4 && data[0] === 0x7f && data[1] === 0x45 && data[2] === 0x4c && data[3] === 0x46;
}

async function elfMachine(candidate) {
  const data = await readFile(candidate).catch(() => Buffer.alloc(0));
  if (data.length < 20 || !(await isElf(candidate))) {
    return undefined;
  }
  if (data[5] === 1) {
    return data.readUInt16LE(18);
  }
  if (data[5] === 2) {
    return data.readUInt16BE(18);
  }
  return undefined;
}

async function gitOutput(cwd, args, fallback) {
  try {
    const { stdout } = await execFileAsync('git', args, { cwd });
    return stdout.trim() || fallback;
  } catch {
    return fallback;
  }
}

async function gitSourceDirty(cwd) {
  try {
    const { stdout } = await execFileAsync('git', ['status', '--porcelain', '--untracked-files=all'], { cwd });
    return stdout.trim().length > 0;
  } catch {
    return true;
  }
}

async function run(command, args, options = {}) {
  console.log(`$ ${command} ${args.join(' ')}`);
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: { ...process.env, ...(options.env || {}) },
      stdio: 'inherit',
    });
    child.on('error', reject);
    child.on('exit', (code, signal) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`${command} exited with ${signal || code}`));
    });
  });
}
