#!/usr/bin/env node
import { createHash } from 'node:crypto';
import {
  access,
  chmod,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  stat,
  symlink,
  writeFile,
} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { execFile, spawn } from 'node:child_process';

const execFileAsync = promisify(execFile);
const scriptPath = fileURLToPath(import.meta.url);
const gxserverRoot = path.dirname(scriptPath);
const repoRoot = path.resolve(gxserverRoot, '..');

/*
 * CDXC:RemotePairing 2026-07-13:
 * Remote hosts must not need a specific glibc/libstdc++ floor, so the Rust
 * binaries (gxserver, ghostex) build against musl and link
 * statically, matching the already-static zmx (Zig musl).
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
  --zmx-zig-bin <path>   default: ZMX_ZIG, GHOSTEX_ZIG, ZIG, then the Zig the build requires
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
   * CDXC:RemotePairing 2026-06-29-18:58:
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
      explicit: [
        ['--zmx-zig-bin', options.zmxZigBin],
        ['ZMX_ZIG', process.env.ZMX_ZIG],
      ],
    });
    const config = {
      ...archConfig,
      arch,
      packageVersion: options.packageVersion || (await gxserverPackageVersion()),
      rustTarget: options.rustTarget || archConfig.rustTarget,
      sourceDirty: await gitSourceDirty(repoRoot),
      sourceRevision: await gitOutput(repoRoot, ['rev-parse', 'HEAD'], 'unknown'),
      zmxRoot: path.resolve(repoRoot, options.zmxRoot || '.dependencies/zmx'),
      zmxSourceDirty: await gitSourceDirty(path.resolve(repoRoot, options.zmxRoot || '.dependencies/zmx')),
      zmxSourceRevision: await gitOutput(
        path.resolve(repoRoot, options.zmxRoot || '.dependencies/zmx'),
        ['rev-parse', 'HEAD'],
        'unknown'
      ),
      zmxZigBin,
      zigTarget: options.zigTarget || archConfig.zigTarget,
    };

    /*
     * CDXC:RemoteMachines 2026-06-23-10:07:
     * Ubuntu install must be a first-run package, not an on-host source build.
     * Build server and zmx into one package directory so the macOS app
     * can upload it over SSH and start the same Rust control plane without PATH
     * fallbacks.
     *
     * CDXC:RemotePairing 2026-06-29-19:45:
     * Release automation must reject stale prebuilt Linux packages. Record the
     * source git revision and dirty-state in build-identity.json so macOS
     * releases can prove x64 and arm64 Ubuntu payloads were built from the
     * commit being released before staging them into the app bundle.
     *
     * CDXC:RemotePairing 2026-08-23:
     * The vendored ghostex-tui terminal app was removed from the repository, so
     * the remote package no longer builds or stages `bin/ghostex-tui`. A herdr
     * plugin replaces it (spec in docs/2026-08-23/tui2-herdr-plugin/).
     *
     * CDXC:Zmx 2026-09-03:
     * zmx is a submodule, so a zmx-only change (wire tags, grid policy) leaves
     * the superproject HEAD untouched until the gitlink bump lands. Record the
     * submodule's own revision and dirty state too so the Linux freshness check
     * cannot accept a package built from stale zmx sources.
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

  const { gxserverBin } = await buildGxserver(config);
  /*
   * CDXC:PromptSearch 2026-08-20:
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

  await copyExecutable(gxserverBin, path.join(binsDir, 'gxserver'), 'gxserver');
  await linkCli(binsDir);
  await copyStrippedExecutable(zmxBin, path.join(binsDir, 'zmx'), 'zmx');
  const tailcatBin = await buildTailcat({ arch: config.arch, workRoot });
  await copyExecutable(tailcatBin, path.join(binsDir, 'tailcat'), 'tailcat');

  /*
   * CDXC:AgentSkills 2026-09-11 WHY:
   * `ghostex <skill> install-skill` resolves the bundled skill next to the CLI
   * that runs it, and the Linux desktop package, the WSL runtime and remote
   * Ubuntu hosts all run bin/ghostex from this package. Only the macOS bundle
   * carried the skills, so first-run capability setup on Linux failed with
   * "Could not find ghostex-cli" (GitHub issue #124). Stage the repository's
   * skills/ catalog as <package>/skills so the install works offline wherever
   * this package is used; gxserver still prefers the GitHub main copy when it
   * can download and verify it.
   * SEE-ALSO: server/src/ghostex_cli/skills.rs (gxserver_package_skills_dir),
   * apps/desktop/scripts/build-macos-app.sh (bundled_cli_skill_assets).
   */
  await stageBundledSkills(stageDir);

  /*
   * CDXC:RemotePairing 2026-07-13:
   * The remote package used to ship an npm-style package.json manifest,
   * dist/protocol JS/type exports, and the Node ghostex CLI under CLI/.
   * Nothing on the remote host consumes the manifest or protocol exports (version identity lives in build-identity.json), and the public
   * `ghostex`/`gx` CLI is now the native Rust bin/ghostex built from the
   * same gxserver crate, so none of them are staged anymore. `gx` is
   * created as a symlink by `gxserver setup` at install time.
   */
  await validateLinuxPackage(stageDir, config);
  await writeBuildIdentity(stageDir, config.packageVersion, config);

  await rm(outputDir, { force: true, recursive: true });
  await mkdir(path.dirname(outputDir), { recursive: true });
  await cp(stageDir, outputDir, { recursive: true });
  // cp rewrites a relative link to an absolute one into the stage folder (Node) or copies its target (Bun).
  await linkCli(path.join(outputDir, 'bin'));
}

/*
 * The public ghostex/gx CLI is gxserver started under the name `ghostex`
 * (CDXC:Release 2026-10-23 in server/src/main.rs), so bin/ghostex is a relative
 * link and the package carries one copy of the program.
 */
async function linkCli(binsDir) {
  const cliPath = path.join(binsDir, 'ghostex');
  await rm(cliPath, { force: true });
  await symlink('gxserver', cliPath);
}

/*
CDXC:Telemetry 2026-08-26:
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
  await run('cargo', ['build', '--release', '--target', config.rustTarget, '--bin', 'gxserver'], {
    cwd: gxserverRoot,
    env: { GHOSTEX_GPUI_MARKETING_VERSION: await resolveMarketingVersion() },
  });
  const releaseDir = path.join(cargoTargetRoot(gxserverRoot), config.rustTarget, 'release');
  return {
    gxserverBin: path.join(releaseDir, 'gxserver'),
  };
}

function cargoTargetRoot(defaultRoot) {
  const configured = process.env.CARGO_TARGET_DIR?.trim();
  return configured ? path.resolve(repoRoot, configured) : path.join(defaultRoot, 'target');
}

/*
 * CDXC:CloudBoxes 2026-11-02 WHY:
 * A Cloud Box reaches the user's devices only through tailcat (Q1021 "private only"), and the
 * Settings install path (`go install`, server/src/tailcat/install.rs) takes minutes and needs Go
 * on the box, so the package carries a prebuilt static tailcat beside gxserver (which looks there
 * first, server/src/tailcat/binary.rs). Same module and version as that install; keep them equal.
 * Go cross-compiles it without cgo from any host, the macOS release build included.
 */
const TAILCAT_MODULE = 'github.com/tailscale/tailcat/cmd/tailcat@v0.6.0';

async function buildTailcat({ arch, workRoot }) {
  const goArch = arch === 'x64' ? 'amd64' : 'arm64';
  const goPath = path.join(workRoot, 'go');
  const { stdout: modCache } = await execFileAsync('go', ['env', 'GOMODCACHE']);
  const { stdout: hostOs } = await execFileAsync('go', ['env', 'GOHOSTOS']);
  const { stdout: hostArch } = await execFileAsync('go', ['env', 'GOHOSTARCH']);
  await run('go', ['install', '-trimpath', '-ldflags=-s -w', TAILCAT_MODULE], {
    env: {
      ...process.env,
      CGO_ENABLED: '0',
      GOARCH: goArch,
      GOMODCACHE: modCache.trim(),
      GOOS: 'linux',
      GOPATH: goPath,
    },
  });
  // `go install` puts a cross-compiled binary under bin/<os>_<arch>, a native one in bin/.
  const native = hostOs.trim() === 'linux' && hostArch.trim() === goArch;
  return path.join(goPath, 'bin', native ? '' : `linux_${goArch}`, 'tailcat');
}

async function buildZigTool({ binName, root, target, workRoot, zigBin }) {
  await assertDirectory(root, `${binName} root`);
  const prefix = path.join(workRoot, binName);
  await run(zigBin || 'zig', ['build', '-Doptimize=ReleaseSafe', `-Dtarget=${target}`, '--prefix', prefix], {
    cwd: root,
  });
  return path.join(prefix, 'bin', binName);
}

const BUNDLED_SKILL_COPY_EXCLUDED_NAMES = new Set(['.git', 'node_modules', '.DS_Store']);

/* Every folder under skills/ with a SKILL.md is a bundled skill; the catalog is
 * the same one gxserver downloads from GitHub main at runtime. */
async function stageBundledSkills(stageDir) {
  const sourceRoot = path.join(repoRoot, 'skills');
  await assertDirectory(sourceRoot, 'bundled skills');
  const skillNames = [];
  for (const entry of await readdir(sourceRoot, { withFileTypes: true })) {
    if (!entry.isDirectory() || BUNDLED_SKILL_COPY_EXCLUDED_NAMES.has(entry.name)) {
      continue;
    }
    try {
      await access(path.join(sourceRoot, entry.name, 'SKILL.md'));
    } catch {
      continue;
    }
    skillNames.push(entry.name);
  }
  if (!skillNames.includes('ghostex-cli')) {
    throw new Error(`Bundled skills under ${sourceRoot} do not include ghostex-cli.`);
  }
  skillNames.sort();
  for (const skillName of skillNames) {
    await cp(path.join(sourceRoot, skillName), path.join(stageDir, 'skills', skillName), {
      dereference: true,
      filter: (source) => !BUNDLED_SKILL_COPY_EXCLUDED_NAMES.has(path.basename(source)),
      recursive: true,
    });
  }
}

async function validateLinuxPackage(packageDir, config) {
  const requiredFiles = ['bin/gxserver', 'bin/ghostex', 'bin/zmx', 'bin/tailcat', 'skills/ghostex-cli/SKILL.md'];
  for (const relativePath of requiredFiles) {
    await assertFile(path.join(packageDir, relativePath), relativePath);
  }
  for (const relativePath of ['bin/gxserver', 'bin/ghostex', 'bin/zmx', 'bin/tailcat']) {
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
   * CDXC:PlatformSupport 2026-07-18:
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
        packageVersion: version,
        sourceDirty: Boolean(config.sourceDirty),
        sourceRevision: config.sourceRevision || 'unknown',
        zmxSourceDirty: Boolean(config.zmxSourceDirty),
        zmxSourceRevision: config.zmxSourceRevision || 'unknown',
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
  const { stdout } = await execFileAsync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
    cwd: gxserverRoot,
  });
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

/**
 * CDXC:Build 2026-10-26 WHY: the same Zig resolution as every other build path (tooling/zig-toolchain.sh holds the full rationale): the series is the vendored Ghostty's minimum_zig_version; an explicitly named binary (this script's own --zmx-zig-bin / ZMX_ZIG first, then GHOSTEX_ZIG and ZIG) must be that series or the build fails; otherwise PATH `zig`, Homebrew, /usr/local, mise, ~/apps tarballs and build/toolchains are searched, and no match fails with an install hint instead of using another version.
 */
async function resolveZigBinary({ explicit = [] } = {}) {
  const zon = await readFile(path.join(repoRoot, '.dependencies', 'ghostty', 'build.zig.zon'), 'utf8');
  const series = /\.minimum_zig_version\s*=\s*"(\d+\.\d+)\./u.exec(zon)?.[1];
  if (!series) throw new Error('Could not read .minimum_zig_version from .dependencies/ghostty/build.zig.zon');
  const versionOf = async (candidate) => {
    try {
      const { stdout } = await execFileAsync(candidate, ['version']);
      return stdout.trim();
    } catch {
      return null;
    }
  };
  for (const [name, value] of [...explicit, ['GHOSTEX_ZIG', process.env.GHOSTEX_ZIG], ['ZIG', process.env.ZIG]]) {
    if (!value) continue;
    const version = await versionOf(value);
    if (version?.startsWith(`${series}.`)) return value;
    throw new Error(
      `${name}=${value} is not a Zig ${series}.x binary (it reports: ${version ?? 'unavailable'}). ` +
        `Point ${name} at Zig ${series}.x, or unset it so the build finds one.`
    );
  }
  const globbed = async (dir, prefix, suffix) => {
    const entries = await readdir(dir).catch(() => []);
    return entries.filter((entry) => entry.startsWith(prefix)).map((entry) => path.join(dir, entry, ...suffix));
  };
  const miseZig = path.join(
    process.env.MISE_DATA_DIR || path.join(os.homedir(), '.local', 'share', 'mise'),
    'installs',
    'zig'
  );
  const candidates = [
    'zig',
    `/opt/homebrew/opt/zig@${series}/bin/zig`,
    '/opt/homebrew/bin/zig',
    `/usr/local/opt/zig@${series}/bin/zig`,
    '/usr/local/bin/zig',
    ...(await globbed(miseZig, `${series}.`, ['bin', 'zig'])),
    ...(await globbed(miseZig, `${series}.`, ['zig'])),
    ...(await globbed(path.join(os.homedir(), 'apps'), 'zig-', ['zig'])).filter((candidate) =>
      path.basename(path.dirname(candidate)).includes(`-${series}.`)
    ),
    ...(await globbed(path.join(repoRoot, 'build', 'toolchains'), `zig-${series}.`, ['zig'])),
  ];
  for (const candidate of candidates) {
    if ((await versionOf(candidate))?.startsWith(`${series}.`)) return candidate;
  }
  throw new Error(
    `Zig ${series}.x is required (the vendored Ghostty's minimum_zig_version), and none was found on PATH, in Homebrew, mise, ~/apps or build/toolchains. ` +
      `Install it with \`mise install zig@${series}.0\` (or Homebrew's zig when it is ${series}), or set GHOSTEX_ZIG to a Zig ${series}.x binary.`
  );
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

/*
 * CDXC:Zmx 2026-10-22 WHY:
 * `zig build -Doptimize=ReleaseSafe` leaves DWARF and a symbol table in zmx (16.1 MB, 12.9 MB of it debug info) and zmx's build.zig has no strip switch. This package ships inside every Linux install, the WSL runtime and remote hosts, so zmx is stripped here the way every Rust binary already is (no debug info, no symbol table; CDXC:Build 2026-10-11 DECISION in .cargo/config.toml), leaving about 3 MB. A zmx panic still prints its message; its stack trace shows addresses only. `zig objcopy` cannot strip ELF in Zig 0.16 ("unimplemented"); rust-objcopy (llvm-objcopy) comes with the pinned Rust toolchain that builds gxserver above and strips either architecture from any host, the macOS cross build included. Stripping changes no code or IPC, so the zmx wire generation is untouched.
 */
async function copyStrippedExecutable(source, destination, label) {
  await assertFile(source, label);
  await mkdir(path.dirname(destination), { recursive: true });
  const objcopy = await rustObjcopy();
  await run(objcopy.command, ['--strip-all', source, destination], { env: objcopy.env });
  await chmod(destination, 0o755);
}

async function rustObjcopy() {
  const [{ stdout: sysroot }, { stdout: verbose }] = await Promise.all([
    execFileAsync('rustc', ['--print', 'sysroot'], { cwd: gxserverRoot }),
    execFileAsync('rustc', ['-vV'], { cwd: gxserverRoot }),
  ]);
  const host = /^host: (\S+)$/mu.exec(verbose)?.[1];
  if (!host) {
    throw new Error('Could not read the Rust host triple from `rustc -vV`.');
  }
  const objcopy = path.join(sysroot.trim(), 'lib', 'rustlib', host, 'bin', 'rust-objcopy');
  await assertFile(objcopy, 'rust-objcopy of the pinned Rust toolchain');
  // The aarch64 Linux toolchain's rust-objcopy links libLLVM from the sysroot's lib folder, which
  // only rustup's proxies put on the loader path, so a direct call needs it (10.31.0's ARM64 build
  // failed with "libLLVM.so.22.1-rust-1.95.0-stable: cannot open shared object file").
  const libDir = path.join(sysroot.trim(), 'lib');
  const loaderPath = [libDir, process.env.LD_LIBRARY_PATH].filter(Boolean).join(path.delimiter);
  return { command: objcopy, env: { LD_LIBRARY_PATH: loaderPath } };
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
  /** @type {Promise<void>} */
  const exited = new Promise((resolve, reject) => {
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
  await exited;
}
