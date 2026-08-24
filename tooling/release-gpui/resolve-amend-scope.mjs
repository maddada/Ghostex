#!/usr/bin/env node
/*
 * Resolve the mutate set and build scope for a same-version amend, then emit
 * GitHub Actions outputs the prepare job threads into preflight and the planner.
 */

import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";

import {
  githubOutputsForIntent,
  productIdsFromScopeFlags,
  resolveAmendIntent,
  scopeEnvFromFlags,
} from "./amend-existing-lib.mjs";
import { releaseProvenanceAssetName, validateReleaseProvenance } from "./provenance.mjs";

const repo = "maddada/Ghostex";

function run(command, args) {
  return execFileSync(command, args, { encoding: "utf8" }).trim();
}

function parseArgs(argv) {
  const options = {
    android: false,
    gxserverLinuxArm64: false,
    gxserverLinuxX64: false,
    gxserverWslWindowsArm64: false,
    gxserverWslWindowsX64: false,
    linuxDeb: false,
    linuxRpm: false,
    linuxTar: false,
    macos: false,
    updateSparkle: false,
    version: null,
    windowsArm64: false,
    windowsX64: false,
  };
  const truthy = new Set(["1", "true", "yes"]);
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = argv[index + 1];
    const flag = (name) => {
      options[name] = truthy.has(String(value ?? "true").toLowerCase());
      if (value !== undefined && !String(value).startsWith("--")) index += 1;
    };
    if (argument === "--version") {
      options.version = value;
      index += 1;
    } else if (argument === "--macos") flag("macos");
    else if (argument === "--linux-deb") flag("linuxDeb");
    else if (argument === "--linux-rpm") flag("linuxRpm");
    else if (argument === "--linux-tar") flag("linuxTar");
    else if (argument === "--windows-x64") flag("windowsX64");
    else if (argument === "--windows-arm64") flag("windowsArm64");
    else if (argument === "--android") flag("android");
    else if (argument === "--gxserver-linux-x64") flag("gxserverLinuxX64");
    else if (argument === "--gxserver-linux-arm64") flag("gxserverLinuxArm64");
    else if (argument === "--gxserver-wsl-windows-x64") flag("gxserverWslWindowsX64");
    else if (argument === "--gxserver-wsl-windows-arm64") flag("gxserverWslWindowsArm64");
    else if (argument === "--update-sparkle") flag("updateSparkle");
    else throw new Error(`Unknown option: ${argument}`);
  }
  if (!/^\d+\.\d+\.\d+$/u.test(options.version ?? "")) {
    throw new Error("Pass --version MAJOR.MINOR.PATCH");
  }
  return options;
}

function liveProductIds(version) {
  const tag = `v${version}`;
  const release = JSON.parse(
    run("gh", ["release", "view", tag, "--repo", repo, "--json", "assets,isDraft,isPrerelease,url"]),
  );
  if (release.isDraft || release.isPrerelease) {
    throw new Error(`${tag} must be an existing public stable release`);
  }
  const provenanceName = releaseProvenanceAssetName(version);
  const asset = (release.assets ?? []).find((entry) => entry.name === provenanceName);
  if (!asset) {
    throw new Error(
      `${release.url} has no ${provenanceName}; same-version amend requires a change-aware provenance record`,
    );
  }
  const raw = run("gh", [
    "api",
    `repos/${repo}/releases/assets/${asset.id}`,
    "-H",
    "Accept: application/octet-stream",
  ]);
  const provenance = validateReleaseProvenance(JSON.parse(raw));
  return Object.keys(provenance.products);
}

const options = parseArgs(process.argv.slice(2));
const selected = productIdsFromScopeFlags(options);
const intent = resolveAmendIntent({
  liveProductIds: liveProductIds(options.version),
  selected,
});
const outputs = githubOutputsForIntent(intent, {
  updateSparkle: options.updateSparkle,
  version: options.version,
});
const env = scopeEnvFromFlags(intent.scopeFlags);
const githubOutput = process.env.GITHUB_OUTPUT;
if (!githubOutput) throw new Error("GITHUB_OUTPUT is required");
const lines = [
  ...Object.entries(outputs).map(([key, value]) => `${key}=${value}`),
  ...Object.entries(env).map(([key, value]) => `${key}=${value}`),
];
appendFileSync(githubOutput, `${lines.join("\n")}\n`);
console.log(`Amend ${options.version}: mutate ${intent.mutate.join(", ")}`);
console.log(`Build scope: ${intent.scope.join(", ")}`);
