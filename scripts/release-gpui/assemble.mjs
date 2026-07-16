#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const [version, artifactsRoot] = process.argv.slice(2);
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) throw new Error("Version must be MAJOR.MINOR.PATCH");
if (!artifactsRoot || !existsSync(artifactsRoot)) throw new Error(`Artifact root is missing: ${artifactsRoot}`);

const expected = new Set(
  (process.env.GHOSTEX_RELEASE_EXPECTED_PLATFORMS ?? "")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean),
);
if (expected.size === 0) throw new Error("GHOSTEX_RELEASE_EXPECTED_PLATFORMS is empty");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", stdio: options.capture ? "pipe" : "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed${result.stderr ? `\n${result.stderr}` : ""}`);
  return result.stdout?.trim() ?? "";
}

function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

const artifactContracts = new Map([
  ["macos-arm64", [`ghostex-${version}-arm64.dmg`, "bd-darwin-arm64.tar.gz"]],
  ["linux-deb-x64", [`ghostex_${version}_amd64.deb`]],
  ["linux-rpm-x64", [`ghostex-${version}-1.x86_64.rpm`]],
  ["windows-x64", [`ghostex-${version}-windows-x64.exe`, `ghostex-${version}-windows-x64-portable.zip`]],
  ["windows-arm64", [`ghostex-${version}-windows-arm64.exe`, `ghostex-${version}-windows-arm64-portable.zip`]],
  ["android", ["ghostex-android.apk"]],
  ["gxserver-linux-x64", ["gxserver-linux-x64.tar.gz"]],
  ["gxserver-linux-arm64", ["gxserver-linux-arm64.tar.gz"]],
  ["gxserver-wsl-windows-x64", ["gxserver-wsl-windows-x64.zip"]],
  ["gxserver-wsl-windows-arm64", ["gxserver-wsl-windows-arm64.zip"]],
]);

const sourceCommit = run("git", ["rev-parse", "HEAD"], { capture: true });
const updateSparkle = process.env.GHOSTEX_RELEASE_UPDATE_SPARKLE !== "0";
run("git", ["config", "user.name", "github-actions[bot]"]);
run("git", ["config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"]);

const manifests = [];
for (const artifactDirectory of readdirSync(artifactsRoot, { withFileTypes: true })) {
  if (!artifactDirectory.isDirectory()) continue;
  const directory = path.join(artifactsRoot, artifactDirectory.name);
  const manifestPath = path.join(directory, "manifest.json");
  if (!existsSync(manifestPath)) continue;
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8").replace(/^\uFEFF/, ""));
  if (manifest.schemaVersion !== 1 || manifest.version !== version || !expected.has(manifest.platform)) {
    throw new Error(`Unexpected manifest ${manifestPath}: ${JSON.stringify(manifest)}`);
  }
  const contract = artifactContracts.get(manifest.platform);
  if (!contract) throw new Error(`No release artifact contract is defined for ${manifest.platform}`);
  const names = (manifest.artifacts ?? []).map((artifact) => artifact.name).sort();
  if (JSON.stringify(names) !== JSON.stringify([...contract].sort())) {
    throw new Error(`${manifest.platform} artifacts ${JSON.stringify(names)} do not match ${JSON.stringify(contract)}`);
  }
  for (const artifact of manifest.artifacts ?? []) {
    if (path.basename(artifact.name) !== artifact.name) throw new Error(`Unsafe artifact name: ${artifact.name}`);
    const file = path.join(directory, artifact.name);
    if (!existsSync(file) || !statSync(file).isFile()) throw new Error(`Manifest artifact is missing: ${file}`);
    const actual = sha256(file);
    if (actual !== artifact.sha256) throw new Error(`SHA256 mismatch for ${artifact.name}: ${actual} != ${artifact.sha256}`);
    if (statSync(file).size !== artifact.size) throw new Error(`Size mismatch for ${artifact.name}`);
    artifact.path = file;
  }
  manifests.push({ directory, ...manifest });
}
const received = new Set(manifests.map((manifest) => manifest.platform));
for (const platform of expected) {
  if (!received.has(platform)) throw new Error(`Enabled platform produced no validated manifest: ${platform}`);
}
if (received.size !== expected.size || manifests.length !== expected.size) {
  throw new Error("Received duplicate or unexpected platform manifests");
}

const byPlatform = new Map(manifests.map((manifest) => [manifest.platform, manifest]));
function artifactPath(platform, name) {
  const manifest = byPlatform.get(platform);
  const artifact = manifest?.artifacts.find((candidate) => candidate.name === name);
  if (!artifact) throw new Error(`${platform} is missing ${name}`);
  return artifact.path;
}

function validateZipEntrySha(zipPath, expectedEntry, expectedSha) {
  const entries = run("unzip", ["-Z1", zipPath], { capture: true }).split(/\r?\n/u).filter(Boolean);
  for (const entry of entries) {
    if (entry.startsWith("/") || entry.split("/").includes("..")) throw new Error(`Unsafe ZIP entry in ${zipPath}: ${entry}`);
  }
  if (!entries.includes(expectedEntry)) throw new Error(`${path.basename(zipPath)} is missing ${expectedEntry}`);
  const temporary = mkdtempSync(path.join(os.tmpdir(), "ghostex-release-zip-"));
  try {
    run("unzip", ["-q", zipPath, expectedEntry, "-d", temporary]);
    const extracted = path.join(temporary, ...expectedEntry.split("/"));
    const actual = sha256(extracted);
    if (actual !== expectedSha) throw new Error(`${path.basename(zipPath)} embeds ${expectedEntry} with SHA256 ${actual}; expected ${expectedSha}`);
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
}

function readZipEntryText(zipPath, expectedEntry) {
  const entries = run("unzip", ["-Z1", zipPath], { capture: true }).split(/\r?\n/u).filter(Boolean);
  if (!entries.includes(expectedEntry)) throw new Error(`${path.basename(zipPath)} is missing ${expectedEntry}`);
  const result = spawnSync("unzip", ["-p", zipPath, expectedEntry], { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Could not read ${expectedEntry} from ${path.basename(zipPath)}: ${result.stderr}`);
  return result.stdout;
}

for (const arch of ["x64", "arm64"]) {
  const linuxPlatform = `gxserver-linux-${arch}`;
  const linuxName = `gxserver-linux-${arch}.tar.gz`;
  if (!byPlatform.has(linuxPlatform)) continue;
  const linuxArchive = artifactPath(linuxPlatform, linuxName);
  const linuxSha = sha256(linuxArchive);
  const wslPlatform = `gxserver-wsl-windows-${arch}`;
  if (byPlatform.has(wslPlatform)) {
    const wslZip = artifactPath(wslPlatform, `gxserver-wsl-windows-${arch}.zip`);
    validateZipEntrySha(wslZip, `gxserver-wsl-windows-${arch}/${linuxName}`, linuxSha);
    const metadataEntry = `gxserver-wsl-windows-${arch}/wsl-package.json`;
    const temporary = mkdtempSync(path.join(os.tmpdir(), "ghostex-release-wsl-metadata-"));
    try {
      run("unzip", ["-q", wslZip, metadataEntry, "-d", temporary]);
      const metadata = JSON.parse(readFileSync(path.join(temporary, ...metadataEntry.split("/")), "utf8"));
      if (metadata.schemaVersion !== 1 || metadata.version !== version || metadata.target !== "wsl2" || metadata.targetArch !== arch || metadata.payload?.name !== linuxName || metadata.payload?.sha256 !== linuxSha) {
        throw new Error(`Invalid WSL package metadata for ${arch}: ${JSON.stringify(metadata)}`);
      }
    } finally {
      rmSync(temporary, { force: true, recursive: true });
    }
  }
  const windowsPlatform = `windows-${arch}`;
  if (byPlatform.has(windowsPlatform)) {
    const portable = artifactPath(windowsPlatform, `ghostex-${version}-windows-${arch}-portable.zip`);
    validateZipEntrySha(portable, `resources/wsl/${linuxName}`, linuxSha);
    const sidecarEntry = `resources/wsl/${linuxName}.sha256`;
    const sidecar = readZipEntryText(portable, sidecarEntry);
    if (sidecar !== `${linuxSha}\n`) {
      throw new Error(`${path.basename(portable)} has an invalid ${sidecarEntry}`);
    }
  }
}

const [major, minor, patch] = version.split(".").map(Number);
const buildNumber = major * 10000 + minor * 100 + patch;
const macos = manifests.find((manifest) => manifest.platform === "macos-arm64");
if (macos && updateSparkle) {
  const generatedAppcast = path.join(macos.directory, "appcast.xml");
  if (!existsSync(generatedAppcast)) throw new Error("macOS payload is missing appcast.xml");
  const xml = readFileSync(generatedAppcast, "utf8");
  if (!xml.includes(`sparkle:version=\"${buildNumber}\"`) || !xml.includes(`ghostex-${version}-arm64.dmg`)) {
    throw new Error("Generated appcast does not point at the new primary GPUI DMG/build");
  }
  writeFileSync("appcast.xml", xml);
  run("git", ["add", "appcast.xml"]);
  run("git", ["commit", "-m", `chore: release ${version}`]);
}

const tag = `v${version}`;
if (run("git", ["tag", "-l", tag], { capture: true })) throw new Error(`Tag already exists: ${tag}`);
const existingRelease = spawnSync("gh", ["release", "view", tag, "--repo", "maddada/Ghostex"], { stdio: "ignore" });
if (existingRelease.status === 0) throw new Error(`GitHub release already exists: ${tag}`);

const changelog = readFileSync("CHANGELOG.md", "utf8");
const sectionStart = changelog.indexOf(`## ${version} -`);
if (sectionStart < 0) throw new Error(`CHANGELOG.md has no ${version} section`);
const nextSection = changelog.indexOf("\n## ", sectionStart + 4);
const releaseNotes = [changelog.slice(sectionStart, nextSection < 0 ? undefined : nextSection).trim(), ""];
if (process.env.GHOSTEX_RELEASE_PRERELEASE === "1") {
  releaseNotes.push("> Nightly prerelease. Existing macOS installations will not be notified through Sparkle.", "");
}
if (process.env.GHOSTEX_RELEASE_WINDOWS_SIGNED === "0") {
  releaseNotes.push("> Windows nightly packages are not Authenticode-signed and may show a SmartScreen warning.", "");
}
releaseNotes.push("## Downloads", "");
const uploadPaths = [];
for (const manifest of manifests.sort((a, b) => a.platform.localeCompare(b.platform))) {
  releaseNotes.push(`### ${manifest.platform}`, "");
  for (const artifact of manifest.artifacts) {
    releaseNotes.push(`- \`${artifact.name}\` — SHA256 \`${artifact.sha256}\``);
    uploadPaths.push(artifact.path);
  }
  releaseNotes.push("");
}
const notesPath = path.join(artifactsRoot, `release-notes-${version}.md`);
writeFileSync(notesPath, `${releaseNotes.join("\n").trim()}\n`);

const remoteMain = run("git", ["ls-remote", "origin", "refs/heads/main"], { capture: true }).split(/\s+/)[0];
if (remoteMain !== sourceCommit) {
  throw new Error(`origin/main moved during the build (${sourceCommit} -> ${remoteMain}); refusing partial publication`);
}
run("git", ["tag", "-a", tag, "-m", `Release ${tag}`]);
run("git", ["push", "origin", tag]);
const releaseArgs = [
  "release", "create", tag,
  "--repo", "maddada/Ghostex",
  "--title", `Ghostex ${version}${process.env.GHOSTEX_RELEASE_PRERELEASE === "1" ? " Nightly" : ""}`,
  "--notes-file", notesPath,
  "--draft",
  ...uploadPaths,
];
if (process.env.GHOSTEX_RELEASE_PRERELEASE === "1") releaseArgs.push("--prerelease");
run("gh", releaseArgs);
run("gh", ["release", "edit", tag, "--repo", "maddada/Ghostex", "--draft=false"]);

// Keep the Sparkle feed as the final public mutation. Existing users cannot
// observe an appcast entry until the matching signed DMG is already live.
if (macos && updateSparkle) run("git", ["push", "origin", "HEAD:main"]);

const liveRelease = JSON.parse(run("gh", ["api", `repos/maddada/Ghostex/releases/tags/${tag}`], { capture: true }));
if (liveRelease.draft) throw new Error(`Live release ${tag} is still a draft`);
const expectedAssets = new Map(
  manifests.flatMap((manifest) => manifest.artifacts).map((artifact) => [artifact.name, artifact.sha256]),
);
if (expectedAssets.size !== uploadPaths.length) throw new Error("Release artifact names are not globally unique");
if (liveRelease.assets?.length !== expectedAssets.size) {
  throw new Error(`Live release has ${liveRelease.assets?.length ?? 0} assets; expected ${expectedAssets.size}`);
}
for (const asset of liveRelease.assets) {
  const expectedSha = expectedAssets.get(asset.name);
  const liveSha = typeof asset.digest === "string" && asset.digest.startsWith("sha256:")
    ? asset.digest.slice("sha256:".length)
    : null;
  if (!expectedSha || liveSha !== expectedSha) {
    throw new Error(`Live asset digest mismatch for ${asset.name}: ${liveSha ?? "missing"} != ${expectedSha ?? "unexpected asset"}`);
  }
}

if (macos && updateSparkle) {
  const liveAppcastUrl = `https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml?release=${version}`;
  let liveAppcast = "";
  for (let attempt = 0; attempt < 12; attempt += 1) {
    const response = spawnSync("curl", ["-fsSL", liveAppcastUrl], { encoding: "utf8" });
    if (
      response.status === 0 &&
      response.stdout.includes(`sparkle:version=\"${buildNumber}\"`) &&
      response.stdout.includes(`ghostex-${version}-arm64.dmg`)
    ) {
      liveAppcast = response.stdout;
      break;
    }
    spawnSync("sleep", ["5"]);
  }
  if (!liveAppcast) throw new Error(`Live appcast did not advance to ${version} (${buildNumber})`);
}

console.log(`Published and live-verified ${tag} with ${uploadPaths.length} assets.`);
