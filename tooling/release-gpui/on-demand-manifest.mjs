import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const sha256Pattern = /^[0-9a-f]{64}$/;
const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const githubRepoPattern = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

function fail(message) {
  throw new Error(`Invalid on-demand resources manifest: ${message}`);
}

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail(`${label} must be an object`);
  return value;
}

function requireIdentifier(value, label) {
  if (typeof value !== "string" || !identifierPattern.test(value)) {
    fail(`${label} must be a non-empty identifier`);
  }
  return value;
}

function requireAssetName(value, label) {
  if (
    typeof value !== "string" ||
    !value ||
    value.includes("/") ||
    value.includes("\\") ||
    value.includes("..")
  ) {
    fail(`${label} must be a plain file name`);
  }
  return value;
}

function requireSha256(value, label) {
  if (typeof value !== "string" || !sha256Pattern.test(value)) fail(`${label} must be 64 lowercase hex characters`);
  return value;
}

function requireSize(value, label) {
  if (!Number.isSafeInteger(value) || value < 0) fail(`${label} must be a non-negative integer`);
  return value;
}

export function authenticateComponentChecksumSidecar(contents, expectedAssetName, expectedSha256) {
  if (typeof contents !== "string") fail("component checksum sidecar must be UTF-8 text");
  const match = /^([0-9a-f]{64})  ([^\r\n]+)\r?\n?$/.exec(contents);
  if (!match) fail("component checksum sidecar must contain one filename-bound SHA-256 record");
  if (match[2] !== expectedAssetName) {
    fail(`component checksum sidecar filename must equal ${expectedAssetName}`);
  }
  if (match[1] !== expectedSha256) {
    fail(`component checksum sidecar digest must equal the sealed digest for ${expectedAssetName}`);
  }
  return match[1];
}

export function validateOnDemandManifestV2(input) {
  const manifest = requireObject(input, "root");
  if (manifest.schemaVersion !== 2) fail("schemaVersion must equal 2");
  if (typeof manifest.version !== "string" || !manifest.version.trim()) fail("version must be a non-empty string");
  if (typeof manifest.githubRepo !== "string" || !githubRepoPattern.test(manifest.githubRepo)) {
    fail("githubRepo must have owner/repository form");
  }

  const assets = requireObject(manifest.assets, "assets");
  for (const [key, rawAsset] of Object.entries(assets)) {
    requireIdentifier(key, `assets key ${JSON.stringify(key)}`);
    const asset = requireObject(rawAsset, `assets.${key}`);
    requireAssetName(asset.name, `assets.${key}.name`);
    requireSha256(asset.sha256, `assets.${key}.sha256`);
    requireSize(asset.bytes, `assets.${key}.bytes`);
  }

  const components = requireObject(manifest.components, "components");
  for (const [key, rawComponent] of Object.entries(components)) {
    requireIdentifier(key, `components key ${JSON.stringify(key)}`);
    const component = requireObject(rawComponent, `components.${key}`);
    const name = requireIdentifier(component.name, `components.${key}.name`);
    if (name !== key) fail(`components.${key}.name must equal its map key`);
    const componentVersion = requireIdentifier(component.componentVersion, `components.${key}.componentVersion`);
    const downloadTag = requireIdentifier(component.downloadTag, `components.${key}.downloadTag`);
    if (downloadTag !== `${name}-${componentVersion}`) {
      fail(`components.${key}.downloadTag must equal ${name}-${componentVersion}`);
    }
    const platforms = requireObject(component.platforms, `components.${key}.platforms`);
    if (Object.keys(platforms).length === 0) fail(`components.${key}.platforms must not be empty`);
    for (const [platformKey, rawPlatformAsset] of Object.entries(platforms)) {
      requireIdentifier(platformKey, `components.${key}.platforms key ${JSON.stringify(platformKey)}`);
      const platformAsset = requireObject(rawPlatformAsset, `components.${key}.platforms.${platformKey}`);
      const assetName = requireAssetName(
        platformAsset.assetName,
        `components.${key}.platforms.${platformKey}.assetName`,
      );
      const expectedAssetName = `${name}-${componentVersion}-${platformKey}.tar.gz`;
      if (assetName !== expectedAssetName) {
        fail(`components.${key}.platforms.${platformKey}.assetName must equal ${expectedAssetName}`);
      }
      const expectedSidecarName = `${assetName}.sha256`;
      if (name === "code-server" || platformAsset.sha256SidecarName !== undefined) {
        const sidecarName = requireAssetName(
          platformAsset.sha256SidecarName,
          `components.${key}.platforms.${platformKey}.sha256SidecarName`,
        );
        if (sidecarName !== expectedSidecarName) {
          fail(
            `components.${key}.platforms.${platformKey}.sha256SidecarName must equal ${expectedSidecarName}`,
          );
        }
      }
      requireSha256(platformAsset.sha256, `components.${key}.platforms.${platformKey}.sha256`);
      requireSize(platformAsset.sizeBytes, `components.${key}.platforms.${platformKey}.sizeBytes`);
    }
  }
  return manifest;
}

export function validateMacosReleaseOnDemandManifest(input) {
  const manifest = validateOnDemandManifestV2(input);
  const codeServer = manifest.components["code-server"];
  if (!codeServer) {
    fail("macOS releases require the code-server component");
  }
  for (const platform of ["darwin-arm64", "linux-x64", "linux-arm64"]) {
    if (!codeServer.platforms[platform]) {
      fail(`macOS releases require components.code-server.platforms.${platform}`);
    }
  }
  return manifest;
}

export function buildOnDemandManifestV2({ version, githubRepo = "maddada/Ghostex", assets, components = {} }) {
  const manifest = { schemaVersion: 2, version, githubRepo, assets, components };
  return validateOnDemandManifestV2(manifest);
}

export function legacyAssetsFromBuildManifest(buildManifest) {
  const assets = {};
  for (const entry of requireObject(buildManifest, "build manifest").assets ?? []) {
    if (!entry || typeof entry !== "object") fail("build manifest asset entries must be objects");
    const key = requireIdentifier(entry.key, "build manifest asset key");
    assets[key] = {
      bytes: requireSize(Number(entry.bytes), `build manifest asset ${key} bytes`),
      name: requireAssetName(entry.name, `build manifest asset ${key} name`),
      sha256: requireSha256(entry.sha256, `build manifest asset ${key} sha256`),
    };
  }
  return assets;
}

function componentMap(input) {
  if (!input) return {};
  const parsed = requireObject(input, "component manifest");
  return parsed.components ? requireObject(parsed.components, "component manifest components") : parsed;
}

export async function sealOnDemandManifest({ buildManifestPath, componentManifestPath, outputPath, githubRepo }) {
  const buildManifest = JSON.parse(await readFile(buildManifestPath, "utf8"));
  let components = {};
  if (componentManifestPath) {
    components = componentMap(JSON.parse(await readFile(componentManifestPath, "utf8")));
  }
  const manifest = buildOnDemandManifestV2({
    version: buildManifest.version,
    githubRepo,
    assets: legacyAssetsFromBuildManifest(buildManifest),
    components,
  });
  await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

function parseCliArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (!argument.startsWith("--")) throw new Error(`Unexpected argument: ${argument}`);
    const key = argument.slice(2);
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`Missing value for ${argument}`);
    options[key] = value;
    index += 1;
  }
  return options;
}

async function main() {
  const [command, ...argv] = process.argv.slice(2);
  if (command === "validate") {
    const options = parseCliArgs(argv);
    validateOnDemandManifestV2(JSON.parse(await readFile(options.manifest, "utf8")));
    process.stdout.write(`Validated on-demand manifest v2: ${options.manifest}\n`);
    return;
  }
  if (command === "validate-macos") {
    const options = parseCliArgs(argv);
    validateMacosReleaseOnDemandManifest(JSON.parse(await readFile(options.manifest, "utf8")));
    process.stdout.write(`Validated macOS on-demand manifest v2: ${options.manifest}\n`);
    return;
  }
  if (command === "seal") {
    const options = parseCliArgs(argv);
    const manifest = await sealOnDemandManifest({
      buildManifestPath: options["build-manifest"],
      componentManifestPath: options["component-manifest"],
      outputPath: options.output,
      githubRepo: options.repo ?? "maddada/Ghostex",
    });
    process.stdout.write(
      `Sealed on-demand manifest v2 with ${Object.keys(manifest.components).length} component(s): ${options.output}\n`,
    );
    return;
  }
  throw new Error("Usage: on-demand-manifest.mjs validate --manifest PATH | validate-macos --manifest PATH | seal --build-manifest PATH --output PATH [--component-manifest PATH] [--repo OWNER/REPO]");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
