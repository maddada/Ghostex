import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  authenticateComponentChecksumSidecar,
  validateOnDemandManifestV2,
} from "./on-demand-manifest.mjs";

const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

function requireIdentifier(value, label) {
  if (!value || !identifierPattern.test(value)) throw new Error(`${label} must match ${identifierPattern}`);
  return value;
}

export function sha256File(filePath) {
  return createHash("sha256").update(readFileSync(filePath)).digest("hex");
}

export function componentAssetsFromDirectory({ assetDir, component, componentVersion }) {
  const prefix = `${component}-${componentVersion}-`;
  const assets = readdirSync(assetDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.startsWith(prefix) && (entry.name.endsWith(".tar.gz") || entry.name.endsWith(".tgz")))
    .map((entry) => {
      const suffixLength = entry.name.endsWith(".tar.gz") ? ".tar.gz".length : ".tgz".length;
      const platform = entry.name.slice(prefix.length, -suffixLength);
      requireIdentifier(platform, `platform parsed from ${entry.name}`);
      const filePath = path.resolve(assetDir, entry.name);
      return { assetName: entry.name, filePath, platform, sha256: sha256File(filePath), sizeBytes: statSync(filePath).size };
    })
    .sort((left, right) => left.platform.localeCompare(right.platform));
  if (assets.length === 0) {
    throw new Error(`No ${prefix}<platform>.tar.gz assets found in ${assetDir}`);
  }
  return assets;
}

export function componentChecksumSidecars(assets) {
  return assets.map((asset) => {
    const filePath = `${asset.filePath}.sha256`;
    if (!existsSync(filePath)) {
      throw new Error(`Missing filename-bound checksum sidecar for ${asset.assetName}`);
    }
    const contents = readFileSync(filePath, "utf8");
    try {
      authenticateComponentChecksumSidecar(contents, asset.assetName, asset.sha256);
    } catch (error) {
      throw new Error(
        `Invalid filename-bound checksum sidecar for ${asset.assetName}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    return {
      assetName: `${asset.assetName}.sha256`,
      filePath,
      platform: asset.platform,
      sha256: sha256File(filePath),
      sizeBytes: statSync(filePath).size,
    };
  });
}

export function normalizedRemoteDigest(asset) {
  const digest = typeof asset.digest === "string" ? asset.digest : "";
  return digest.startsWith("sha256:") ? digest.slice("sha256:".length) : digest;
}

export function componentPublishCommand({
  assetDir = "build/on-demand-components/assets",
  component,
  componentVersion,
  output = "build/on-demand-components/components.json",
}) {
  return `bun run release:component -- --component ${component} --version ${componentVersion} --asset-dir ${assetDir} --output ${output}`;
}

export function verifyPublishedComponent({ component, release }) {
  const tag = component.downloadTag;
  const fix = componentPublishCommand({
    component: component.name,
    componentVersion: component.componentVersion,
  });
  if (!release?.exists) {
    throw new Error(`Component tag ${tag} is missing. Fix: ${fix}`);
  }
  const remoteByName = new Map((release.assets ?? []).map((asset) => [asset.name, asset]));
  for (const [platform, sealed] of Object.entries(component.platforms ?? {})) {
    const remote = remoteByName.get(sealed.assetName);
    if (!remote) {
      throw new Error(`Component tag ${tag} is missing ${sealed.assetName} for ${platform}. Fix: ${fix}`);
    }
    const remoteDigest = normalizedRemoteDigest(remote);
    if (remoteDigest !== sealed.sha256 || Number(remote.size) !== sealed.sizeBytes) {
      throw new Error(
        `Component tag ${tag} has mismatched size/digest for ${sealed.assetName}: ` +
          `${remote.size ?? "unknown"}/${remoteDigest || "unavailable"}; sealed manifest expects ` +
          `${sealed.sizeBytes}/${sealed.sha256}. Fix: publish a newly versioned component, then run: ${fix}`,
      );
    }
    if (sealed.sha256SidecarName && !remoteByName.has(sealed.sha256SidecarName)) {
      throw new Error(
        `Component tag ${tag} is missing ${sealed.sha256SidecarName} for ${platform}. Fix: ${fix}`,
      );
    }
  }
  return component;
}

export function planComponentRelease({ assets, release }) {
  if (!release.exists) {
    return { createRelease: true, uploads: assets, noops: [] };
  }
  const remoteByName = new Map((release.assets ?? []).map((asset) => [asset.name, asset]));
  const uploads = [];
  const noops = [];
  for (const asset of assets) {
    const remote = remoteByName.get(asset.assetName);
    if (!remote) {
      uploads.push(asset);
      continue;
    }
    const remoteDigest = normalizedRemoteDigest(remote);
    if (remoteDigest !== asset.sha256 || Number(remote.size) !== asset.sizeBytes) {
      throw new Error(
        `Refusing to replace ${asset.assetName}: component tag already has size/digest ${remote.size ?? "unknown"}/${remoteDigest || "unavailable"}, local asset is ${asset.sizeBytes}/${asset.sha256}`,
      );
    }
    noops.push(asset);
  }
  return { createRelease: false, uploads, noops };
}

function runGh(args, { allowFailure = false } = {}) {
  const result = spawnSync("gh", args, { encoding: "utf8" });
  if (result.status !== 0 && !allowFailure) {
    throw new Error(`gh ${args.join(" ")} failed: ${(result.stderr || result.stdout || "unknown error").trim()}`);
  }
  return result;
}

function reusePublishedAsset({ assetDir, component, componentVersion, platform, repo, requireSha256Sidecar = false }) {
  const tag = `${component}-${componentVersion}`;
  const assetName = `${component}-${componentVersion}-${platform}.tar.gz`;
  const sidecarName = `${assetName}.sha256`;
  const release = inspectRelease({ repo, tag });
  if (!release.exists) return false;
  const remote = release.assets.find((asset) => asset.name === assetName);
  if (!remote) return false;
  const remoteSidecar = release.assets.find((asset) => asset.name === sidecarName);
  if (requireSha256Sidecar && !remoteSidecar) {
    throw new Error(`Component tag ${tag} is missing ${sidecarName}`);
  }

  mkdirSync(assetDir, { recursive: true });
  const downloadArgs = [
    "release",
    "download",
    tag,
    "--repo",
    repo,
    "--pattern",
    assetName,
  ];
  if (requireSha256Sidecar) downloadArgs.push("--pattern", sidecarName);
  downloadArgs.push(
    "--dir",
    assetDir,
    "--clobber",
  );
  runGh(downloadArgs);
  const localPath = path.resolve(assetDir, assetName);
  const localSize = statSync(localPath).size;
  const localDigest = sha256File(localPath);
  const remoteDigest = normalizedRemoteDigest(remote);
  if (Number(remote.size) !== localSize || (remoteDigest && remoteDigest !== localDigest)) {
    throw new Error(
      `Downloaded component asset ${assetName} does not match GitHub metadata: ` +
        `${localSize}/${localDigest}; expected ${remote.size ?? "unknown"}/${remoteDigest || "unavailable"}`,
    );
  }
  console.log(`Reused published component asset ${tag}/${assetName}`);
  return true;
}

export function inspectRelease({ repo, tag }) {
  const result = runGh(["release", "view", tag, "--repo", repo, "--json", "assets"], { allowFailure: true });
  if (result.status !== 0) {
    const diagnostic = `${result.stderr ?? ""}\n${result.stdout ?? ""}`.toLowerCase();
    if (diagnostic.includes("release not found") || diagnostic.includes("not found") || diagnostic.includes("404")) {
      return { exists: false, assets: [] };
    }
    throw new Error(`Could not inspect component release ${tag}: ${diagnostic.trim() || `gh exited ${result.status}`}`);
  }
  const payload = JSON.parse(result.stdout);
  return { exists: true, assets: payload.assets ?? [] };
}

function createReleaseIfMissing({ component, componentVersion, repo, tag }) {
  const result = runGh(
    ["release", "create", tag, "--repo", repo, "--title", `${component} ${componentVersion}`, "--notes", `Ghostex component ${component} ${componentVersion}.`],
    { allowFailure: true },
  );
  if (result.status === 0) return;
  const release = inspectRelease({ repo, tag });
  if (!release.exists) {
    throw new Error(`Could not create component release ${tag}: ${(result.stderr || result.stdout || "unknown error").trim()}`);
  }
}

function uploadAssetIdempotently({ asset, repo, tag }) {
  const result = runGh(["release", "upload", tag, asset.filePath, "--repo", repo], { allowFailure: true });
  if (result.status === 0) return;
  const release = inspectRelease({ repo, tag });
  const remote = release.assets.find((candidate) => candidate.name === asset.assetName);
  if (
    remote &&
    normalizedRemoteDigest(remote) === asset.sha256 &&
    Number(remote.size) === asset.sizeBytes
  ) {
    return;
  }
  throw new Error(
    `Could not upload ${asset.assetName} to ${tag}: ${(result.stderr || result.stdout || "unknown error").trim()}`,
  );
}

function parseArguments(argv) {
  const options = { dryRun: false, metadataOnly: false, reusePublished: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--dry-run") {
      options.dryRun = true;
      continue;
    }
    if (argument === "--metadata-only") {
      options.metadataOnly = true;
      continue;
    }
    if (argument === "--reuse-published") {
      options.reusePublished = true;
      continue;
    }
    if (argument === "--require-sha256-sidecars") {
      options.requireSha256Sidecars = true;
      continue;
    }
    if (!argument.startsWith("--")) throw new Error(`Unexpected argument: ${argument}`);
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`Missing value for ${argument}`);
    options[argument.slice(2)] = value;
    index += 1;
  }
  return options;
}

export function componentManifestRecord({ assets, checksumSidecars = [], component, componentVersion, downloadTag }) {
  const sidecarByPlatform = new Map(checksumSidecars.map((sidecar) => [sidecar.platform, sidecar]));
  return {
    name: component,
    componentVersion,
    downloadTag,
    platforms: Object.fromEntries(
      assets.map((asset) => [
        asset.platform,
        {
          assetName: asset.assetName,
          ...(sidecarByPlatform.has(asset.platform)
            ? { sha256SidecarName: sidecarByPlatform.get(asset.platform).assetName }
            : {}),
          sha256: asset.sha256,
          sizeBytes: asset.sizeBytes,
        },
      ]),
    ),
  };
}

function writeAggregateManifest(outputPath, record) {
  let components = {};
  if (existsSync(outputPath)) {
    const previous = JSON.parse(readFileSync(outputPath, "utf8"));
    components = previous.components ?? previous;
  }
  components = { ...components, [record.name]: record };
  // Reuse the sealed schema validator by supplying inert-but-valid release fields.
  validateOnDemandManifestV2({ schemaVersion: 2, version: "component-publisher", githubRepo: "maddada/Ghostex", assets: {}, components });
  mkdirSync(path.dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify({ components }, null, 2)}\n`);
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const component = requireIdentifier(options.component, "component");
  const componentVersion = requireIdentifier(options.version, "version");
  const repo = options.repo ?? "maddada/Ghostex";
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repo)) throw new Error("repo must have owner/repository form");
  const tag = options.tag ?? `${component}-${componentVersion}`;
  requireIdentifier(tag, "tag");
  if (!options["asset-dir"]) throw new Error("--asset-dir is required");
  if (options.reusePublished) {
    const platform = requireIdentifier(options.platform, "platform");
    reusePublishedAsset({
      assetDir: options["asset-dir"],
      component,
      componentVersion,
      platform,
      repo,
      requireSha256Sidecar: options.requireSha256Sidecars,
    });
  }
  const assets = componentAssetsFromDirectory({ assetDir: options["asset-dir"], component, componentVersion });
  const checksumSidecars = options.requireSha256Sidecars ? componentChecksumSidecars(assets) : [];
  const publishedAssets = options.requireSha256Sidecars
    ? [...assets, ...checksumSidecars].sort((left, right) => left.assetName.localeCompare(right.assetName))
    : assets;
  if (options["require-platforms"]) {
    const availablePlatforms = new Set(assets.map((asset) => asset.platform));
    for (const platform of options["require-platforms"].split(",")) {
      requireIdentifier(platform, "required platform");
      if (!availablePlatforms.has(platform)) {
        throw new Error(`Missing required ${component} component asset for ${platform}`);
      }
    }
  }
  if (options["release-state"] && !options.dryRun) throw new Error("--release-state is test-only and requires --dry-run");
  const release = options.metadataOnly
    ? { exists: false, assets: [] }
    : options["release-state"]
      ? JSON.parse(readFileSync(options["release-state"], "utf8"))
      : inspectRelease({ repo, tag });
  const plan = options.metadataOnly
    ? { createRelease: false, uploads: [], noops: [] }
    : planComponentRelease({ assets: publishedAssets, release });

  if (plan.createRelease) {
    process.stdout.write(`${options.dryRun ? "DRY-RUN " : ""}CREATE ${repo} release ${tag}\n`);
    if (!options.dryRun) {
      createReleaseIfMissing({ component, componentVersion, repo, tag });
    }
  }
  for (const asset of plan.uploads) {
    process.stdout.write(`${options.dryRun ? "DRY-RUN " : ""}UPLOAD ${asset.assetName} sha256=${asset.sha256}\n`);
    if (!options.dryRun) uploadAssetIdempotently({ asset, repo, tag });
  }
  for (const asset of plan.noops) process.stdout.write(`NO-OP ${asset.assetName} sha256=${asset.sha256}\n`);

  const record = componentManifestRecord({
    assets,
    checksumSidecars,
    component,
    componentVersion,
    downloadTag: tag,
  });
  const outputPath = options.output
    ? path.resolve(options.output)
    : options.dryRun
      ? null
      : path.resolve("build/on-demand-components/components.json");
  if (outputPath) {
    writeAggregateManifest(outputPath, record);
    process.stdout.write(`SEALED-METADATA ${outputPath}\n`);
  }
  if (options.metadataOnly) process.stdout.write(`METADATA-ONLY ${component} ${componentVersion}\n`);
  process.stdout.write(`${JSON.stringify({ components: { [component]: record } }, null, 2)}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
