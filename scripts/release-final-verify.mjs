#!/usr/bin/env node
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import {
  extractChangelogSectionFromText,
  onDemandAssetNames,
  releaseBuildVersion,
  validateGhostexCask,
} from "./release-ghostex.mjs";
import { validateMacosAppBundle } from "./validate-macos-app-bundle.mjs";
import { validateOnDemandManifestV2 } from "./release-gpui/on-demand-manifest.mjs";
import { inspectRelease, verifyPublishedComponent } from "./release-gpui/publish-component.mjs";
import {
  validateWindowsUpdateFeed,
  windowsUpdateArtifactNames,
} from "./release-gpui/windows-update-feed.mjs";
import {
  releaseProvenanceAssetName,
  validateReleaseProvenance,
} from "./release-gpui/provenance.mjs";
import {
  customerDownloadEntries,
  renderIosAvailabilityNotes,
} from "./release-gpui/customer-downloads.mjs";
import {
  crossReleaseReuseOrigins,
  productOriginLabel,
  renderReleaseProvenanceReport,
  verifyReleaseProvenanceAgainstAssets,
} from "./release-gpui/publish-provenance.mjs";

/*
 CDXC:ReleaseAutomation 2026-07-02-14:10:
 Final live verification previously lived as a long manual checklist in the
 release skill and re-downloaded the ~800 MB DMG it had already fetched twice.
 This script codifies the whole checklist as one command with a PASS/FAIL
 table, accepts --dmg to reuse an already-verified local artifact, and
 downloads the live DMG only when no verified local copy exists.
*/

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);
const githubRepo = "maddada/Ghostex";
export const MAX_RELEASE_DMG_BYTES = 300 * 1024 * 1024;
const subrepoCandidates = [
  "apps/mobile/app", "crossplatform", ".dependencies/zmx",
];

function usage() {
  return `
Usage:
  node scripts/release-final-verify.mjs <version> [options]

Options:
  --dmg <path>       Reuse an already-downloaded DMG (for example Homebrew's
                     fetch cache) instead of downloading the live asset again.
  --skip-repo        Skip local repo checks (clean worktree, tag at HEAD).
  --skip-brew        Skip all Homebrew checks.
  --skip-brew-fetch  Run brew info/cat but skip the large forced DMG fetch.
                     Supplying --dmg also skips that redundant fetch.
  --skip-android     Skip Android APK checks.
  --skip-sparkle     Skip live appcast checks.
  --skip-dmg         Skip DMG download/mount/bundle validation.
  --skip-subrepos    Skip subrepo cleanliness checks.
  --help             Show this help.
`;
}

function parseArgs(argv) {
  const options = {
    dmg: null,
    skipAndroid: false,
    skipBrew: false,
    skipBrewFetch: false,
    skipDmg: false,
    skipRepo: false,
    skipSparkle: false,
    skipSubrepos: false,
    version: null,
  };
  const positional = [];
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") {
      options.help = true;
    } else if (arg === "--dmg") {
      options.dmg = argv[index + 1];
      if (!options.dmg) {
        throw new Error("--dmg requires a path.");
      }
      index += 1;
    } else if (arg === "--skip-repo") {
      options.skipRepo = true;
    } else if (arg === "--skip-brew") {
      options.skipBrew = true;
    } else if (arg === "--skip-brew-fetch") {
      options.skipBrewFetch = true;
    } else if (arg === "--skip-android") {
      options.skipAndroid = true;
    } else if (arg === "--skip-sparkle") {
      options.skipSparkle = true;
    } else if (arg === "--skip-dmg") {
      options.skipDmg = true;
    } else if (arg === "--skip-subrepos") {
      options.skipSubrepos = true;
    } else if (arg.startsWith("-")) {
      throw new Error(`Unknown option: ${arg}`);
    } else {
      positional.push(arg);
    }
  }
  if (options.help) {
    return options;
  }
  if (positional.length !== 1 || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(positional[0] ?? "")) {
    throw new Error("Pass exactly one semver version, for example 5.5.0.");
  }
  options.version = positional[0];
  return options;
}

function runCommand(command, { timeoutMs = 120_000, cwd = repoRoot } = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, { cwd, env: process.env, shell: true, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    let settled = false;
    const timeout = setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      child.kill("SIGTERM");
      resolve({ code: 124, stderr: `${stderr}\n(timed out after ${timeoutMs}ms)`, stdout });
    }, timeoutMs);
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolve({ code: 127, stderr: String(error.message ?? error), stdout });
    });
    child.on("close", (code) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolve({ code: code ?? 1, stderr, stdout });
    });
  });
}

async function capture(command, options = {}) {
  const result = await runCommand(command, options);
  if (result.code !== 0) {
    throw new Error(`${command} failed (${result.code}): ${(result.stderr || result.stdout).trim().slice(0, 800)}`);
  }
  return result.stdout.trim();
}

async function githubContent(repo, filePath, ref = "main") {
  const response = await capture(
    `env -u GH_TOKEN -u GITHUB_TOKEN gh api ${shellQuote(`repos/${repo}/contents/${filePath}?ref=${ref}`)}`,
  );
  const encoded = JSON.parse(response).content?.replace(/\s/gu, "") ?? "";
  if (!encoded) throw new Error(`GitHub returned no content for ${repo}/${filePath}@${ref}`);
  return Buffer.from(encoded, "base64").toString("utf8");
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function parseAssetSha(asset) {
  const digest = asset?.digest;
  return typeof digest === "string" && digest.startsWith("sha256:") ? digest.slice("sha256:".length) : null;
}

function formatMiB(bytes) {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

export function enforceReleaseDmgBudget(bytes) {
  if (bytes > MAX_RELEASE_DMG_BYTES) {
    throw new Error(
      `New-shape DMG is ${formatMiB(bytes)}, exceeding the ${formatMiB(MAX_RELEASE_DMG_BYTES)} release budget.`,
    );
  }
  return bytes;
}

const results = [];

async function check(name, fn) {
  const startedAt = Date.now();
  try {
    const detail = await fn();
    if (detail === SKIPPED) {
      results.push({ detail: "", durationMs: Date.now() - startedAt, name, status: "SKIP" });
    } else if (detail && typeof detail === "object" && detail.warn) {
      results.push({ detail: detail.warn, durationMs: Date.now() - startedAt, name, status: "WARN" });
    } else {
      results.push({ detail: detail ?? "", durationMs: Date.now() - startedAt, name, status: "PASS" });
    }
  } catch (error) {
    results.push({
      detail: String(error?.message ?? error).split("\n").slice(0, 3).join(" | "),
      durationMs: Date.now() - startedAt,
      name,
      status: "FAIL",
    });
  }
}

const SKIPPED = Symbol("skipped");

function formatDuration(durationMs) {
  const seconds = durationMs / 1000;
  return seconds < 60 ? `${seconds.toFixed(1)}s` : `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
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
  const startedAt = Date.now();
  console.log(`Ghostex final live verification for ${version} (build ${buildVersion})`);

  await check("repo-clean", async () => {
    if (options.skipRepo) {
      return SKIPPED;
    }
    const status = await capture("git status --porcelain --untracked-files=all");
    if (status) {
      throw new Error(`Worktree is dirty:\n${status.split(/\r?\n/).slice(0, 6).join(", ")}`);
    }
    return "clean";
  });

  await check("tag-at-head", async () => {
    if (options.skipRepo) {
      return SKIPPED;
    }
    const tags = await capture("git tag --points-at HEAD");
    if (!tags.split(/\r?\n/).includes(`v${version}`)) {
      throw new Error(`v${version} does not point at HEAD (tags at HEAD: ${tags || "none"}).`);
    }
    return `v${version} at HEAD`;
  });

  let releaseAssets = [];
  let releaseBody = "";
  await check("github-release", async () => {
    const json = await capture(
      `env -u GH_TOKEN -u GITHUB_TOKEN gh release view ${shellQuote(`v${version}`)} --repo ${shellQuote(githubRepo)} --json tagName,url,assets,body`,
    );
    const release = JSON.parse(json);
    releaseAssets = release.assets ?? [];
    releaseBody = release.body ?? "";
    const dmgName = `ghostex-${version}-arm64.dmg`;
    if (!releaseAssets.some((asset) => asset.name === dmgName)) {
      throw new Error(`Release is missing ${dmgName}.`);
    }
    return `${releaseAssets.length} assets at ${release.url}`;
  });

  const dmgAsset = releaseAssets.find((asset) => asset.name === `ghostex-${version}-arm64.dmg`);
  const dmgDigest = parseAssetSha(dmgAsset);

  await check("customer-downloads", async () => {
    const groups = customerDownloadEntries(version, releaseAssets.map((asset) => asset.name));
    const downloads = groups.flatMap((group) => group.downloads);
    if (downloads.length === 0) throw new Error("Release has no customer-facing download assets.");
    const missing = downloads.filter((download) => !releaseBody.includes(download.url));
    if (missing.length > 0) {
      throw new Error(`Release notes are missing customer download links: ${missing.map((item) => item.label).join(", ")}`);
    }
    const hasAndroidDownload = groups.some((group) => group.title === "Android");
    if (hasAndroidDownload && !releaseBody.includes(renderIosAvailabilityNotes())) {
      throw new Error("Release notes are missing the iOS TestFlight Discord instructions.");
    }
    return `${downloads.length} direct customer download links${hasAndroidDownload ? " plus iOS TestFlight instructions" : ""}`;
  });

  /*
   CDXC:ReleaseChangeAwarePlanning 2026-08-13:
   The release records, per product, whether its bytes were built here or reused
   from an earlier verified origin. This check re-derives every one of those
   claims from public data only: the live asset digests, and — for a
   cross-release reuse — the origin release's own asset digests, which is what
   "byte-identical re-publication" has to mean to a user.
  */
  let releaseProvenance = null;
  await check("provenance", async () => {
    const assetName = releaseProvenanceAssetName(version);
    if (!releaseAssets.some((asset) => asset.name === assetName)) {
      return {
        warn: `Expected difference: ${version} carries no ${assetName} (released before change-aware planning).`,
      };
    }
    const temporary = await mkdtemp(path.join(tmpdir(), `ghostex-provenance-verify-${version}-`));
    await capture(
      `env -u GH_TOKEN -u GITHUB_TOKEN gh release download ${shellQuote(`v${version}`)} ` +
        `--repo ${shellQuote(githubRepo)} --pattern ${shellQuote(assetName)} --dir ${shellQuote(temporary)}`,
    );
    releaseProvenance = validateReleaseProvenance(
      JSON.parse(await readFile(path.join(temporary, assetName), "utf8")),
    );
    if (releaseProvenance.version !== version || releaseProvenance.tag !== `v${version}`) {
      throw new Error(`${assetName} records ${releaseProvenance.tag}, not v${version}.`);
    }
    const failures = verifyReleaseProvenanceAgainstAssets({
      liveAssets: releaseAssets.map((asset) => ({
        name: asset.name,
        sha256: parseAssetSha(asset),
        size: asset.size,
      })),
      releaseProvenance,
      version,
    });
    if (failures.length > 0) throw new Error(failures.slice(0, 4).join(" | "));

    /* A cross-release reuse must byte-match the release it names. */
    const origins = crossReleaseReuseOrigins(releaseProvenance);
    for (const origin of origins) {
      if (origin.versionStamped) {
        throw new Error(`${origin.product} is version-stamped but claims reuse from ${origin.tag}.`);
      }
      const originRelease = JSON.parse(
        await capture(
          `env -u GH_TOKEN -u GITHUB_TOKEN gh release view ${shellQuote(origin.tag)} ` +
            `--repo ${shellQuote(githubRepo)} --json assets`,
        ),
      );
      for (const artifact of origin.artifacts) {
        const originAsset = (originRelease.assets ?? []).find((asset) => asset.name === artifact.name);
        const originSha = parseAssetSha(originAsset);
        if (!originAsset || originSha !== artifact.sha256) {
          throw new Error(
            `${origin.product} claims ${artifact.name} is unchanged since ${origin.tag}, but that release publishes ` +
              `${originSha ?? "no such asset"}.`,
          );
        }
      }
    }
    const counts = Object.values(releaseProvenance.products).reduce(
      (totals, record) => ({
        built: totals.built + (record.action === "built" ? 1 : 0),
        reused: totals.reused + (record.action === "reused" ? 1 : 0),
      }),
      { built: 0, reused: 0 },
    );
    return (
      `${counts.built} built, ${counts.reused} reused (${origins.length} cross-release origin(s) byte-matched), ` +
      `${releaseAssets.length} assets recorded`
    );
  });

  const provenanceFor = (product) => releaseProvenance?.products?.[product] ?? null;
  const describeProduct = (product) => {
    const record = provenanceFor(product);
    if (!record) return "";
    return ` [${record.action}, ${productOriginLabel(record)}]`;
  };
  const describeArtifact = (assetName) => {
    const record = Object.values(releaseProvenance?.products ?? {}).find((candidate) =>
      candidate.artifacts.some((artifact) => artifact.name === assetName),
    );
    return record ? describeProduct(record.product) : "";
  };

  await check("windows-update-feeds", async () => {
    const arches = ["x64", "arm64"].filter((arch) => {
      const names = windowsUpdateArtifactNames(version, arch);
      return [names.installer, names.portable, names.feed, names.fullPackage]
        .some((name) => releaseAssets.some((asset) => asset.name === name));
    });
    if (arches.length === 0) {
      return { warn: "Release has no Velopack Windows update channels (legacy or Windows-excluded release)." };
    }
    const temporary = await mkdtemp(path.join(tmpdir(), `ghostex-windows-update-verify-${version}-`));
    const validated = [];
    for (const arch of arches) {
      const names = windowsUpdateArtifactNames(version, arch);
      const channelNames = new Set([
        names.installer,
        names.portable,
        names.feed,
        names.fullPackage,
        names.deltaPackage,
      ]);
      const channelAssets = releaseAssets
        .filter((asset) => channelNames.has(asset.name))
        .map((asset) => ({ name: asset.name, sha256: parseAssetSha(asset), size: asset.size }));
      for (const required of [names.installer, names.portable, names.feed, names.fullPackage]) {
        const asset = channelAssets.find((candidate) => candidate.name === required);
        if (!asset) throw new Error(`Windows ${arch} release is missing ${required}.`);
        if (!asset.sha256) throw new Error(`GitHub reports no digest for ${required}.`);
      }
      await capture(
        `env -u GH_TOKEN -u GITHUB_TOKEN gh release download ${shellQuote(`v${version}`)} ` +
          `--repo ${shellQuote(githubRepo)} --pattern ${shellQuote(names.feed)} --dir ${shellQuote(temporary)}`,
      );
      const result = validateWindowsUpdateFeed({
        arch,
        artifacts: channelAssets,
        feedText: await readFile(path.join(temporary, names.feed), "utf8"),
        version,
      });
      validated.push(
        `${result.channel}${result.delta ? " with delta" : " full"}${describeProduct(`windows-${arch}`)}`,
      );
    }
    return validated.join(", ");
  });

  const onDemandReleaseAssets = onDemandAssetNames
    .map((name) => releaseAssets.find((asset) => asset.name === name))
    .filter(Boolean);
  const expectOnDemand = onDemandReleaseAssets.length === onDemandAssetNames.length;

  await check("on-demand-assets", async () => {
    if (releaseAssets.length === 0) {
      throw new Error("GitHub release assets were not readable.");
    }
    if (!expectOnDemand) {
      if (onDemandReleaseAssets.length > 0) {
        throw new Error(
          `Release has only ${onDemandReleaseAssets.length}/${onDemandAssetNames.length} on-demand assets: ${onDemandReleaseAssets.map((asset) => asset.name).join(", ")}.`,
        );
      }
      return { warn: "No on-demand assets on this release (legacy bundled-payload release)." };
    }
    for (const asset of onDemandReleaseAssets) {
      if (!parseAssetSha(asset)) {
        throw new Error(`GitHub reports no digest for ${asset.name}.`);
      }
    }
    /*
     Every shipped app of this version resolves these from `v<version>`, so a
     reused product is not a reason to skip a check: it is a reason to say where
     the bytes came from. The digests themselves were already matched against the
     provenance record and against the origin release above.
    */
    return onDemandReleaseAssets.map((asset) => `${asset.name}${describeArtifact(asset.name)}`).join(", ");
  });

  let changelogNotes = null;
  await check("changelog-section", async () => {
    const changelog = await readFile(path.join(repoRoot, "CHANGELOG.md"), "utf8");
    changelogNotes = extractChangelogSectionFromText(changelog, version);
    return "present with Major/Minor bullets";
  });

  let liveSignature = null;
  await check("live-appcast", async () => {
    if (options.skipSparkle) {
      return SKIPPED;
    }
    const appcastPath = path.join(await mkdtemp(path.join(tmpdir(), `ghostex-verify-${version}-`)), "appcast.xml");
    await writeFile(appcastPath, await githubContent(githubRepo, "appcast.xml"));
    await capture(`xmllint --noout ${shellQuote(appcastPath)}`);
    const topVersion = await capture(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='version'])[1])" ${shellQuote(appcastPath)}`,
    );
    const topShortVersion = await capture(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='shortVersionString'])[1])" ${shellQuote(appcastPath)}`,
    );
    const topUrl = await capture(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@url)[1])" ${shellQuote(appcastPath)}`,
    );
    liveSignature = await capture(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='enclosure']/@*[local-name()='edSignature'])[1])" ${shellQuote(appcastPath)}`,
    );
    const embeddedNotes = await capture(
      `xmllint --xpath "string((//*[local-name()='item'][1]/*[local-name()='description'])[1])" ${shellQuote(appcastPath)}`,
    );
    const expectedUrl = `https://github.com/${githubRepo}/releases/download/v${version}/ghostex-${version}-arm64.dmg`;
    if (topVersion !== String(buildVersion) || topShortVersion !== version) {
      throw new Error(`Top item is ${topShortVersion} (${topVersion}); expected ${version} (${buildVersion}).`);
    }
    if (topUrl !== expectedUrl) {
      throw new Error(`Top enclosure URL is ${topUrl}.`);
    }
    if (!liveSignature) {
      throw new Error("Top enclosure has no EdDSA signature.");
    }
    const notesProbe = changelogNotes
      ?.split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line.startsWith("- ") && line !== "- Major" && line !== "- Minor" && line !== "- GPUI")
      ?.slice(2);
    if (!embeddedNotes.trim()) {
      throw new Error("Top item has empty embedded release notes.");
    }
    if (notesProbe && !embeddedNotes.includes(notesProbe)) {
      throw new Error(`Embedded notes do not contain expected changelog text: ${notesProbe}`);
    }
    return `top item ${version} (${buildVersion}) with embedded notes`;
  });

  await check("homebrew-cask", async () => {
    if (options.skipBrew) {
      return SKIPPED;
    }
    if (!dmgDigest) {
      throw new Error("GitHub reported no DMG digest to validate the cask sha256 against.");
    }
    const liveCask = await githubContent("maddada/homebrew-tap", "Casks/ghostex.rb");
    validateGhostexCask(liveCask, { sha256: dmgDigest, version });
    return `live cask at ${version}, arm64-only, :ventura`;
  });

  let dmgPath = options.dmg ?? null;
  let dmgBytes = 0;
  if (dmgPath && !existsSync(dmgPath)) {
    throw new Error(`--dmg does not exist: ${dmgPath}`);
  }
  await check("homebrew-commands", async () => {
    if (options.skipBrew) {
      return SKIPPED;
    }
    await capture("HOMEBREW_NO_INSTALL_FROM_API=1 brew info --cask maddada/tap/ghostex", { timeoutMs: 300_000 });
    const catOutput = await capture("HOMEBREW_NO_INSTALL_FROM_API=1 brew cat --cask maddada/tap/ghostex", {
      timeoutMs: 300_000,
    });
    validateGhostexCask(catOutput, { sha256: dmgDigest, version });
    if (options.skipBrewFetch || dmgPath) {
      return dmgPath ? "brew info/cat validated; supplied DMG reused" : "brew info/cat validated; fetch skipped";
    }
    await capture("HOMEBREW_NO_INSTALL_FROM_API=1 brew fetch --force --cask --arch=arm maddada/tap/ghostex", {
      timeoutMs: 900_000,
    });
    const brewCache = await capture("brew --cache --cask maddada/tap/ghostex");
    if (existsSync(brewCache)) dmgPath = brewCache;
    return dmgPath ? "brew info/cat/fetch validated; cached DMG reused" : "brew info/cat/fetch validated";
  });

  await check("dmg-artifact", async () => {
    if (options.skipDmg) {
      return SKIPPED;
    }
    if (!dmgPath) {
      const downloadPath = path.join(tmpdir(), `ghostex-${version}-final-verify.dmg`);
      await capture(
        `curl -fsSL ${shellQuote(`https://github.com/${githubRepo}/releases/download/v${version}/ghostex-${version}-arm64.dmg`)} -o ${shellQuote(downloadPath)}`,
        { timeoutMs: 1_800_000 },
      );
      dmgPath = downloadPath;
    }
    const sha = await capture(`shasum -a 256 ${shellQuote(dmgPath)} | awk '{print $1}'`);
    if (dmgDigest && sha !== dmgDigest) {
      throw new Error(`DMG SHA256 ${sha} does not match GitHub digest ${dmgDigest}.`);
    }
    dmgBytes = (await stat(dmgPath)).size;
    return `${path.basename(dmgPath)} ${formatMiB(dmgBytes)} (${sha.slice(0, 12)}...)`;
  });

  await check("sparkle-signature", async () => {
    if (options.skipSparkle || options.skipDmg) {
      return SKIPPED;
    }
    if (!dmgPath || !liveSignature) {
      throw new Error("DMG path or live signature unavailable.");
    }
    const findCommand = [
      "find",
      shellQuote(path.join(repoRoot, "build/arm64/SourcePackages/artifacts/sparkle")),
      shellQuote(path.join(repoRoot, "build/SourcePackages/artifacts/sparkle")),
      "'/tmp/ghostex-xcodebuild/SourcePackages/artifacts/sparkle'",
      "-path '*/Sparkle/bin/sign_update' -print -quit 2>/dev/null",
    ].join(" ");
    const signUpdate = (await runCommand(findCommand)).stdout.trim();
    if (!signUpdate) {
      return { warn: "Sparkle sign_update tool not found locally; signature not re-verified against the DMG." };
    }
    await capture(`${shellQuote(signUpdate)} --verify ${shellQuote(dmgPath)} ${shellQuote(liveSignature)}`);
    return "live EdDSA signature verifies the DMG bytes";
  });

  let sealedManifestV2 = null;
  await check("dmg-bundle-validation", async () => {
    if (options.skipDmg) {
      return SKIPPED;
    }
    if (!dmgPath) {
      throw new Error("No DMG available to mount.");
    }
    const attachOutput = await capture(`hdiutil attach -nobrowse -readonly ${shellQuote(dmgPath)}`);
    const mountPoint = attachOutput.split("\n").filter(Boolean).at(-1)?.split(/\t+/).at(-1)?.trim();
    if (!mountPoint || !mountPoint.startsWith("/Volumes/")) {
      throw new Error(`Could not parse mount point from hdiutil output.`);
    }
    try {
      const appPath = path.join(mountPoint, "ghostex.app");
      await capture(`codesign --verify --deep --strict --verbose=2 ${shellQuote(appPath)}`, { timeoutMs: 600_000 });
      const shortVersion = await capture(
        `plutil -extract CFBundleShortVersionString raw ${shellQuote(path.join(appPath, "Contents/Info.plist"))}`,
      );
      const bundleVersion = await capture(
        `plutil -extract CFBundleVersion raw ${shellQuote(path.join(appPath, "Contents/Info.plist"))}`,
      );
      if (shortVersion !== version || bundleVersion !== String(buildVersion)) {
        throw new Error(`Mounted app is ${shortVersion} (${bundleVersion}); expected ${version} (${buildVersion}).`);
      }

      const manifestPath = path.join(appPath, "Contents/Resources/Web/on-demand-resources.json");
      const manifest = existsSync(manifestPath) ? JSON.parse(await readFile(manifestPath, "utf8")) : null;
      const isNewBundleShape = manifest?.schemaVersion === 2;
      await validateMacosAppBundle({
        allowLegacyBundleShape: !isNewBundleShape,
        appName: "Ghostex",
        appPath,
        arch: "arm64",
      });
      const installedKiB = Number.parseInt(
        (await capture(`du -sk ${shellQuote(appPath)} | awk '{print $1}'`)).trim(),
        10,
      );
      const installedBytes = installedKiB * 1024;
      if (expectOnDemand) {
        if (!manifest) throw new Error("Mounted app has no sealed on-demand manifest.");
        if (manifest.version !== version) {
          throw new Error(`Sealed on-demand manifest records ${manifest.version}; expected ${version}.`);
        }
        for (const asset of onDemandReleaseAssets) {
          const sealed = Object.values(manifest.assets ?? {}).find((entry) => entry?.name === asset.name);
          const liveSha = parseAssetSha(asset);
          if (!sealed || sealed.sha256 !== liveSha) {
            throw new Error(
              `Sealed checksum for ${asset.name} (${sealed?.sha256 ?? "missing"}) does not match the live asset digest (${liveSha}).`,
            );
          }
        }
      } else if (existsSync(manifestPath)) {
        throw new Error("Mounted app declares on-demand assets but the release has none.");
      }
      if (!isNewBundleShape) {
        return {
          warn:
            `Expected difference: ${version} uses the legacy bundled-runtime shape; ` +
            `installed app ${formatMiB(installedBytes)}, DMG ${formatMiB(dmgBytes)}. ` +
            "The 300 MiB DMG budget starts with manifest v2 releases.",
        };
      }
      sealedManifestV2 = validateOnDemandManifestV2(manifest);
      enforceReleaseDmgBudget(dmgBytes);
      return (
        `manifest v2 bundle valid; installed app ${formatMiB(installedBytes)}, ` +
        `DMG ${formatMiB(dmgBytes)} / ${formatMiB(MAX_RELEASE_DMG_BYTES)} budget`
      );
    } finally {
      await runCommand(`hdiutil detach ${shellQuote(mountPoint)}`);
    }
  });

  await check("component-tags", async () => {
    if (options.skipDmg) return SKIPPED;
    if (!sealedManifestV2) {
      return { warn: "Expected difference: legacy release has no manifest v2 component tags to verify." };
    }
    for (const component of Object.values(sealedManifestV2.components)) {
      verifyPublishedComponent({
        component,
        release: inspectRelease({ repo: githubRepo, tag: component.downloadTag }),
      });
    }
    return `${Object.keys(sealedManifestV2.components).length} component tag(s) match sealed digests and sizes`;
  });

  await check("component-download-unpack", async () => {
    if (options.skipDmg) return SKIPPED;
    if (!sealedManifestV2) {
      return { warn: "Expected difference: legacy release has no component tarball to spot-check." };
    }
    const candidates = Object.values(sealedManifestV2.components).flatMap((component) =>
      Object.values(component.platforms).map((asset) => ({ component, asset })),
    );
    const selected = candidates.sort((left, right) => left.asset.sizeBytes - right.asset.sizeBytes)[0];
    if (!selected) throw new Error("Manifest v2 contains no component asset to spot-check.");
    const temporary = await mkdtemp(path.join(tmpdir(), `ghostex-component-verify-${version}-`));
    const archivePath = path.join(temporary, selected.asset.assetName);
    const extractPath = path.join(temporary, "unpacked");
    await capture(
      `env -u GH_TOKEN -u GITHUB_TOKEN gh release download ${shellQuote(selected.component.downloadTag)} ` +
        `--repo ${shellQuote(githubRepo)} --pattern ${shellQuote(selected.asset.assetName)} --dir ${shellQuote(temporary)}`,
      { timeoutMs: 1_800_000 },
    );
    const downloadedSha = await capture(`shasum -a 256 ${shellQuote(archivePath)} | awk '{print $1}'`);
    if (downloadedSha !== selected.asset.sha256) {
      throw new Error(`Downloaded ${selected.asset.assetName} digest ${downloadedSha} does not match the sealed manifest.`);
    }
    const listing = await capture(`tar -tzf ${shellQuote(archivePath)}`);
    const unsafe = listing.split(/\r?\n/u).find((entry) => entry.startsWith("/") || entry.split("/").includes(".."));
    if (unsafe) throw new Error(`Unsafe component archive entry: ${unsafe}`);
    await mkdir(extractPath);
    await capture(`tar -xzf ${shellQuote(archivePath)} -C ${shellQuote(extractPath)}`);
    if ((await readdir(extractPath)).length === 0) throw new Error(`${selected.asset.assetName} unpacked to an empty directory.`);
    return `${selected.asset.assetName} downloaded, SHA-verified, and unpacked`;
  });

  await check("android-apk", async () => {
    if (options.skipAndroid) {
      return SKIPPED;
    }
    const apkAsset = releaseAssets.find((asset) => asset.name === "ghostex-android.apk");
    if (!apkAsset) {
      throw new Error("Release is missing ghostex-android.apk.");
    }
    const apkSha = parseAssetSha(apkAsset);
    if (!apkSha) {
      return { warn: "GitHub reported no digest for ghostex-android.apk; checksum not cross-checked." };
    }
    /* A reused APK is still checked through live asset metadata and provenance. */
    return `APK digest ${apkSha.slice(0, 12)}... verified from the live asset${describeProduct("android")}`;
  });

  await check("subrepos-clean", async () => {
    if (options.skipSubrepos) {
      return SKIPPED;
    }
    const problems = [];
    for (const repo of subrepoCandidates) {
      const repoPath = path.join(repoRoot, repo);
      if (!existsSync(repoPath)) {
        continue;
      }
      const topLevelResult = await runCommand(
        `git -C ${shellQuote(repoPath)} rev-parse --show-toplevel`,
        { timeoutMs: 10_000 },
      );
      if (
        topLevelResult.code !== 0 ||
        path.resolve(topLevelResult.stdout.trim()) !== path.resolve(repoPath)
      ) {
        continue;
      }
      const status = await capture(`git -C ${shellQuote(repoPath)} status --porcelain --untracked-files=all`);
      if (status) {
        problems.push(repo);
      }
    }
    if (problems.length > 0) {
      throw new Error(`Dirty subrepos: ${problems.join(", ")}`);
    }
    return "all clean";
  });

  console.log("");
  const nameWidth = Math.max(...results.map((result) => result.name.length)) + 2;
  for (const result of results) {
    console.log(
      `${result.status.padEnd(4)}  ${result.name.padEnd(nameWidth)} ${formatDuration(result.durationMs).padStart(8)}  ${result.detail}`,
    );
  }
  if (releaseProvenance) {
    console.log("");
    console.log(renderReleaseProvenanceReport(releaseProvenance));
  }
  const failed = results.filter((result) => result.status === "FAIL");
  if (failed.length > 0) {
    console.error(`\nFinal verification FAILED (${failed.map((result) => result.name).join(", ")}) in ${formatDuration(Date.now() - startedAt)}.`);
    process.exitCode = 1;
    return;
  }
  const warned = results.filter((result) => result.status === "WARN");
  console.log(
    `\nFinal verification PASSED in ${formatDuration(Date.now() - startedAt)}${warned.length > 0 ? ` with ${warned.length} warning(s)` : ""}.`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error("");
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
