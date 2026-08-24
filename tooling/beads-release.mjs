#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { createReadStream, createWriteStream } from 'node:fs';
import { access, chmod, cp, mkdir, mkdtemp, rename, rm, stat } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);
const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(scriptPath), '..');

export const BEADS_VERSION = '1.1.0';
export const BEADS_SOURCE_REVISION = '672d942083a1fd0c8603fa1e77620c58ba9d47c8';
export const BEADS_SOURCE_REVISION_SHORT = BEADS_SOURCE_REVISION.slice(0, 12);
export const BEADS_SCHEMA_VERSION = 54;
export const BEADS_PACKAGE_ID = `${BEADS_VERSION}-${BEADS_SOURCE_REVISION.slice(0, 12)}-schema${BEADS_SCHEMA_VERSION}`;
export const BEADS_RELEASE_TAG = 'v7.2.0';
export const BEADS_RELEASE_BASE_URL = `https://github.com/maddada/Ghostex/releases/download/${BEADS_RELEASE_TAG}`;

/*
 * CDXC:ProjectBoardBeadsSchema 2026-08-08:
 * Ghostex v7.2 shipped this exact Beads revision, which creates and opens schema
 * v54 databases. Upstream v1.1.2 only understands schema v53, so replacing the
 * packaged binary with that nominally newer release stranded existing Ghostex
 * workspaces. Keep the last Ghostex-published schema-v54 artifacts pinned by
 * immutable release URL, source revision, and checksum until an upstream
 * release natively supports v54 or newer. No schema-skew compatibility flag is
 * used at runtime.
 */
export const BEADS_RELEASE_ARTIFACTS = Object.freeze({
  darwin: Object.freeze({
    arm64: Object.freeze({
      binaryPath: 'bd',
      name: 'bd-darwin-arm64.tar.gz',
      sha256: '2ea04cfd8d5081950019c745d880c17c8b5eba99d1ac5f88d769bde25e77f00b',
    }),
  }),
  linux: Object.freeze({
    arm64: Object.freeze({
      binaryPath: 'bin/bd',
      name: 'gxserver-linux-arm64.tar.gz',
      sha256: '106a402e7a743acfe7f235ceb10d8a907c81f65323d1b37266f166d577246e65',
    }),
    x64: Object.freeze({
      binaryPath: 'bin/bd',
      name: 'gxserver-linux-x64.tar.gz',
      sha256: '4aab77429f5ca43d64f6f3096ff5ab33a70eee84a3cee1c043107df1773a8204',
    }),
  }),
});

export function normalizeBeadsPlatform(value) {
  const platform = String(value || '')
    .trim()
    .toLowerCase();
  if (platform === 'macos' || platform === 'mac' || platform === 'darwin') return 'darwin';
  if (platform === 'linux') return 'linux';
  return platform;
}

export function normalizeBeadsArch(value) {
  const arch = String(value || '')
    .trim()
    .toLowerCase();
  if (arch === 'arm64' || arch === 'aarch64') return 'arm64';
  if (arch === 'x64' || arch === 'x86_64' || arch === 'amd64') return 'x64';
  return arch;
}

export function beadsReleaseArtifact(platformValue, archValue) {
  const platform = normalizeBeadsPlatform(platformValue);
  const arch = normalizeBeadsArch(archValue);
  const artifact = BEADS_RELEASE_ARTIFACTS[platform]?.[arch];
  if (!artifact) {
    throw new Error(
      `Unsupported Beads release platform: ${platformValue}/${archValue}. ` +
        'Ghostex packages this schema-v54 bd revision for darwin arm64 and linux x64/arm64.'
    );
  }
  return { ...artifact, arch, platform };
}

export async function sha256File(filePath) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest('hex');
}

async function verifiedArchive({ arch, archivePath, cacheDir, platform }) {
  const artifact = beadsReleaseArtifact(platform, arch);
  if (archivePath) {
    const explicitArchive = path.resolve(archivePath);
    await assertPublishedChecksum(explicitArchive, artifact);
    return { archivePath: explicitArchive, artifact };
  }

  const resolvedCacheDir = path.resolve(
    cacheDir ||
      process.env.GHOSTEX_BEADS_DOWNLOAD_CACHE ||
      path.join(repoRoot, 'build', 'downloads', 'beads', BEADS_PACKAGE_ID)
  );
  const cachedArchive = path.join(resolvedCacheDir, artifact.name);
  if (await fileExists(cachedArchive)) {
    const actual = await sha256File(cachedArchive);
    if (actual === artifact.sha256) return { archivePath: cachedArchive, artifact };
    await rm(cachedArchive, { force: true });
  }

  await mkdir(resolvedCacheDir, { recursive: true });
  const temporaryArchive = path.join(resolvedCacheDir, `.${artifact.name}.${process.pid}.${Date.now()}.download`);
  const releaseBaseUrl = (process.env.GHOSTEX_BEADS_RELEASE_BASE_URL || BEADS_RELEASE_BASE_URL).replace(/\/$/u, '');
  const url = `${releaseBaseUrl}/${artifact.name}`;
  try {
    const response = await fetch(url, { redirect: 'follow' });
    if (!response.ok || !response.body) {
      throw new Error(`download returned HTTP ${response.status} ${response.statusText}`);
    }
    await pipeline(Readable.fromWeb(response.body), createWriteStream(temporaryArchive, { mode: 0o600 }));
    await assertPublishedChecksum(temporaryArchive, artifact);
    await rename(temporaryArchive, cachedArchive);
  } catch (error) {
    await rm(temporaryArchive, { force: true });
    throw new Error(
      `Could not download checksum-verified Beads ${BEADS_PACKAGE_ID} artifact ${url}: ` +
        `${error instanceof Error ? error.message : String(error)}`
    );
  }
  return { archivePath: cachedArchive, artifact };
}

async function assertPublishedChecksum(archivePath, artifact) {
  const actual = await sha256File(archivePath);
  if (actual !== artifact.sha256) {
    throw new Error(
      `Beads ${BEADS_PACKAGE_ID} checksum mismatch for ${artifact.name}: ` +
        `expected ${artifact.sha256}, got ${actual}`
    );
  }
}

export async function stageBeadsRelease({
  arch = process.arch,
  archivePath,
  cacheDir,
  outputPath,
  platform = process.platform,
} = {}) {
  if (!outputPath) throw new Error('stageBeadsRelease requires outputPath');
  const destination = path.resolve(outputPath);
  const verified = await verifiedArchive({ arch, archivePath, cacheDir, platform });
  const extractRoot = await mkdtemp(path.join(os.tmpdir(), 'ghostex-beads-release-'));
  try {
    await execFileAsync('tar', ['-xzf', verified.archivePath, '-C', extractRoot]);
    const extractedBd = path.join(extractRoot, verified.artifact.binaryPath);
    await access(extractedBd);
    const extractedMode = (await stat(extractedBd)).mode;
    if ((extractedMode & 0o111) === 0) {
      throw new Error(`${verified.artifact.name} did not preserve executable permissions on bd`);
    }
    await mkdir(path.dirname(destination), { recursive: true });
    await cp(extractedBd, destination);
    await chmod(destination, 0o755);
  } finally {
    await rm(extractRoot, { force: true, recursive: true });
  }
  return { ...verified.artifact, outputPath: destination };
}

async function fileExists(filePath) {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
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
    if (!arg.startsWith('--')) throw new Error(`Unexpected argument: ${arg}`);
    const value = args[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for ${arg}`);
    options[arg.slice(2).replace(/-([a-z])/gu, (_, letter) => letter.toUpperCase())] = value;
    index += 1;
  }
  return options;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(
      'Usage: node tooling/beads-release.mjs --platform darwin|linux ' +
        '--arch x64|arm64 --output <path> [--archive <path>] [--cache-dir <path>]\n'
    );
    return;
  }
  const staged = await stageBeadsRelease({
    arch: options.arch,
    archivePath: options.archive,
    cacheDir: options.cacheDir,
    outputPath: options.output,
    platform: options.platform,
  });
  console.log(
    `Staged Beads ${BEADS_PACKAGE_ID} ${staged.platform}/${staged.arch} from ` +
      `Ghostex ${BEADS_RELEASE_TAG} asset ${staged.name} (${staged.sha256}) at ${staged.outputPath}`
  );
}

if (import.meta.url === pathToFileURL(process.argv[1] || '').href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
