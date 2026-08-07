#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { createReadStream, createWriteStream } from "node:fs";
import {
  access,
  chmod,
  cp,
  mkdir,
  mkdtemp,
  rename,
  rm,
  stat,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(scriptPath), "..");

export const BEADS_VERSION = "1.1.2";
export const BEADS_RELEASE_TAG = `v${BEADS_VERSION}`;
export const BEADS_RELEASE_BASE_URL =
  `https://github.com/gastownhall/beads/releases/download/${BEADS_RELEASE_TAG}`;

// Published by the upstream v1.1.2 release in checksums.txt. Ghostex verifies
// the selected archive before it can enter an app or gxserver package.
export const BEADS_RELEASE_ARTIFACTS = Object.freeze({
  darwin: Object.freeze({
    arm64: Object.freeze({
      name: "beads_1.1.2_darwin_arm64.tar.gz",
      sha256: "9b0137a83a2afd343e2abd2a506be72ea032721000f76669c2cf81729e78501d",
    }),
    x64: Object.freeze({
      name: "beads_1.1.2_darwin_amd64.tar.gz",
      sha256: "0e94de9319c9d66cb7e0038bb17ebaf5dd2fe669e366a4b9153528b474a1a8f6",
    }),
  }),
  linux: Object.freeze({
    arm64: Object.freeze({
      name: "beads_1.1.2_linux_arm64.tar.gz",
      sha256: "a134015faf4be0a43f8681a8d602eaf0b7c255c957f09d3c933257c8c92fdd10",
    }),
    x64: Object.freeze({
      name: "beads_1.1.2_linux_amd64.tar.gz",
      sha256: "a72d71ed374955dc9f83a0f90b54bd7b6a0016709dd1676ae2e368651ed401c2",
    }),
  }),
});

export function normalizeBeadsPlatform(value) {
  const platform = String(value || "").trim().toLowerCase();
  if (platform === "macos" || platform === "mac" || platform === "darwin") return "darwin";
  if (platform === "linux") return "linux";
  return platform;
}

export function normalizeBeadsArch(value) {
  const arch = String(value || "").trim().toLowerCase();
  if (arch === "arm64" || arch === "aarch64") return "arm64";
  if (arch === "x64" || arch === "x86_64" || arch === "amd64") return "x64";
  return arch;
}

export function beadsReleaseArtifact(platformValue, archValue) {
  const platform = normalizeBeadsPlatform(platformValue);
  const arch = normalizeBeadsArch(archValue);
  const artifact = BEADS_RELEASE_ARTIFACTS[platform]?.[arch];
  if (!artifact) {
    throw new Error(
      `Unsupported Beads release platform: ${platformValue}/${archValue}. ` +
        "Ghostex packages bd for darwin x64/arm64 and linux x64/arm64.",
    );
  }
  return { ...artifact, arch, platform };
}

export async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
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
      path.join(repoRoot, "build", "downloads", "beads", BEADS_RELEASE_TAG),
  );
  const cachedArchive = path.join(resolvedCacheDir, artifact.name);
  if (await fileExists(cachedArchive)) {
    const actual = await sha256File(cachedArchive);
    if (actual === artifact.sha256) return { archivePath: cachedArchive, artifact };
    await rm(cachedArchive, { force: true });
  }

  await mkdir(resolvedCacheDir, { recursive: true });
  const temporaryArchive = path.join(
    resolvedCacheDir,
    `.${artifact.name}.${process.pid}.${Date.now()}.download`,
  );
  const releaseBaseUrl = (
    process.env.GHOSTEX_BEADS_RELEASE_BASE_URL || BEADS_RELEASE_BASE_URL
  ).replace(/\/$/u, "");
  const url = `${releaseBaseUrl}/${artifact.name}`;
  try {
    const response = await fetch(url, { redirect: "follow" });
    if (!response.ok || !response.body) {
      throw new Error(`download returned HTTP ${response.status} ${response.statusText}`);
    }
    await pipeline(Readable.fromWeb(response.body), createWriteStream(temporaryArchive, { mode: 0o600 }));
    await assertPublishedChecksum(temporaryArchive, artifact);
    await rename(temporaryArchive, cachedArchive);
  } catch (error) {
    await rm(temporaryArchive, { force: true });
    throw new Error(
      `Could not download checksum-verified Beads ${BEADS_RELEASE_TAG} artifact ${url}: ` +
        `${error instanceof Error ? error.message : String(error)}`,
    );
  }
  return { archivePath: cachedArchive, artifact };
}

async function assertPublishedChecksum(archivePath, artifact) {
  const actual = await sha256File(archivePath);
  if (actual !== artifact.sha256) {
    throw new Error(
      `Beads ${BEADS_RELEASE_TAG} checksum mismatch for ${artifact.name}: ` +
        `expected ${artifact.sha256}, got ${actual}`,
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
  if (!outputPath) throw new Error("stageBeadsRelease requires outputPath");
  const destination = path.resolve(outputPath);
  const verified = await verifiedArchive({ arch, archivePath, cacheDir, platform });
  const extractRoot = await mkdtemp(path.join(os.tmpdir(), "ghostex-beads-release-"));
  try {
    await execFileAsync("tar", ["-xzf", verified.archivePath, "-C", extractRoot]);
    const extractedBd = path.join(extractRoot, "bd");
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
    if (arg === "--help" || arg === "-h") {
      options.help = true;
      continue;
    }
    if (!arg.startsWith("--")) throw new Error(`Unexpected argument: ${arg}`);
    const value = args[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`Missing value for ${arg}`);
    options[arg.slice(2).replace(/-([a-z])/gu, (_, letter) => letter.toUpperCase())] = value;
    index += 1;
  }
  return options;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(
      "Usage: node scripts/beads-release.mjs --platform darwin|linux " +
        "--arch x64|arm64 --output <path> [--archive <path>] [--cache-dir <path>]\n",
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
    `Staged Beads ${BEADS_RELEASE_TAG} ${staged.platform}/${staged.arch} from ` +
      `${staged.name} (${staged.sha256}) at ${staged.outputPath}`,
  );
}

if (import.meta.url === pathToFileURL(process.argv[1] || "").href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
