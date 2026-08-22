import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";

export const RELEASE_REPO = process.env.GHOSTEX_RELEASE_REPO ?? "maddada/Ghostex";
export const STATE_ASSET = "release-state.json";
export const METADATA_SUFFIX = ".metadata.json";

export function assertVersion(version) {
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
    throw new Error(`Version must be MAJOR.MINOR.PATCH, got ${version ?? "<empty>"}`);
  }
}

export function assertSha(sourceSha) {
  if (!/^[0-9a-f]{40}$/.test(sourceSha ?? "")) {
    throw new Error(`source_sha must be a full 40-character Git commit SHA, got ${sourceSha ?? "<empty>"}`);
  }
}

export function releaseContracts(version) {
  assertVersion(version);
  return new Map([
    ["android", {
      architecture: "universal",
      assets: ["ghostex-android.apk"],
      label: "Android (React Native)",
      workflow: "release-build-android.yml",
    }],
    ["gxserver-linux-x64", {
      architecture: "x86_64",
      assets: ["gxserver-linux-x64.tar.gz"],
      label: "gxserver Linux x64",
      workflow: "release-build-gxserver-x64.yml",
    }],
    ["gxserver-linux-arm64", {
      architecture: "aarch64",
      assets: ["gxserver-linux-arm64.tar.gz"],
      label: "gxserver Linux ARM64",
      workflow: "release-build-gxserver-arm64.yml",
    }],
    ["macos-arm64", {
      architecture: "arm64",
      assets: [`ghostex-${version}-arm64.dmg`, "bd-darwin-arm64.tar.gz"],
      dependencies: ["gxserver-linux-x64", "gxserver-linux-arm64"],
      label: "macOS",
      workflow: "release-build-macos.yml",
    }],
  ]);
}

export function releasePackageNames(version, state = null) {
  const contracts = releaseContracts(version);
  const names = state?.packages ?? [...contracts.keys()];
  if (!Array.isArray(names) || names.length === 0 || new Set(names).size !== names.length) {
    throw new Error(`Release package scope must be a non-empty list without duplicates: ${JSON.stringify(names)}`);
  }
  for (const name of names) {
    if (!contracts.has(name)) throw new Error(`Unknown release package in scope: ${name}`);
  }
  for (const name of names) {
    for (const dependency of contracts.get(name).dependencies ?? []) {
      if (!names.includes(dependency)) throw new Error(`${name} requires ${dependency} in the release package scope`);
    }
  }
  return names;
}

export function selectedReleaseContracts(version, state = null) {
  const contracts = releaseContracts(version);
  return new Map(releasePackageNames(version, state).map((name) => [name, contracts.get(name)]));
}

export function expectedAssets(version, packageNames = null) {
  const state = packageNames ? { packages: packageNames } : null;
  return [...selectedReleaseContracts(version, state).values()].flatMap((contract) => contract.assets);
}

export function run(command, args, { allowFailure = false, capture = false, cwd, env, input } = {}) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env: env ? { ...process.env, ...env } : process.env,
    input,
    maxBuffer: 64 * 1024 * 1024,
    stdio: capture || allowFailure || input !== undefined ? "pipe" : "inherit",
  });
  if (result.error) throw result.error;
  if (!allowFailure && result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed (${result.status})${result.stderr ? `\n${result.stderr.trim()}` : ""}`);
  }
  return { status: result.status ?? 1, stderr: result.stderr?.trim() ?? "", stdout: result.stdout?.trim() ?? "" };
}

function runBytes(command, args) {
  const result = spawnSync(command, args, { encoding: null, maxBuffer: 512 * 1024 * 1024, stdio: "pipe" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed (${result.status})`);
  return result.stdout;
}

function runBytesAsync(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ["ignore", "pipe", "pipe"] });
    const stdout = [];
    const stderr = [];
    let stdoutSize = 0;
    child.stdout.on("data", (chunk) => {
      stdoutSize += chunk.length;
      if (stdoutSize > 512 * 1024 * 1024) {
        child.kill();
        reject(new Error(`${command} ${args.join(" ")} exceeded the 512 MiB output limit`));
        return;
      }
      stdout.push(chunk);
    });
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.once("error", reject);
    child.once("close", (status) => {
      if (status !== 0) {
        reject(new Error(`${command} ${args.join(" ")} failed (${status})${stderr.length ? `\n${Buffer.concat(stderr).toString("utf8").trim()}` : ""}`));
        return;
      }
      resolve(Buffer.concat(stdout));
    });
  });
}

export function sha256(file) {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

export function getRelease(version, { required = true } = {}) {
  assertVersion(version);
  let response = run("gh", ["api", `repos/${RELEASE_REPO}/releases/tags/v${version}`], { allowFailure: true, capture: true });
  if (response.status !== 0) {
    const releases = run("gh", ["api", `repos/${RELEASE_REPO}/releases?per_page=100`], { allowFailure: true, capture: true });
    if (releases.status === 0) {
      const match = JSON.parse(releases.stdout).find((candidate) => candidate.tag_name === `v${version}`);
      if (match) return match;
    }
    if (!required && /HTTP 404|Not Found/i.test(response.stderr)) return null;
    throw new Error(`Could not read staged release v${version}: ${response.stderr || response.stdout}`);
  }
  return JSON.parse(response.stdout);
}

function waitForCreatedRelease(version) {
  const waitBuffer = new Int32Array(new SharedArrayBuffer(4));
  for (let attempt = 1; attempt <= 12; attempt += 1) {
    const release = getRelease(version, { required: false });
    if (release) return release;
    if (attempt < 12) Atomics.wait(waitBuffer, 0, 0, 500);
  }
  throw new Error(`GitHub created draft v${version}, but it did not become readable through the Releases API`);
}

export function findAsset(release, name) {
  return (release.assets ?? []).find((asset) => asset.name === name) ?? null;
}

export function downloadAsset(asset) {
  return runBytes("gh", [
    "api",
    "-H", "Accept: application/octet-stream",
    `repos/${RELEASE_REPO}/releases/assets/${asset.id}`,
  ]);
}

function downloadAssetAsync(asset) {
  return runBytesAsync("gh", [
    "api",
    "-H", "Accept: application/octet-stream",
    `repos/${RELEASE_REPO}/releases/assets/${asset.id}`,
  ]);
}

export function assetSha256(asset) {
  if (typeof asset.digest === "string" && asset.digest.startsWith("sha256:")) {
    return asset.digest.slice("sha256:".length);
  }
  return createHash("sha256").update(downloadAsset(asset)).digest("hex");
}

export function readJsonAsset(release, name, { required = true } = {}) {
  const asset = findAsset(release, name);
  if (!asset) {
    if (!required) return null;
    throw new Error(`Release v${release.tag_name?.replace(/^v/, "")} is missing ${name}`);
  }
  return JSON.parse(downloadAsset(asset).toString("utf8"));
}

function uploadFile(tag, file, name = path.basename(file)) {
  const temporary = mkdtempSync(path.join(os.tmpdir(), "ghostex-release-upload-"));
  try {
    const uploadPath = path.join(temporary, name);
    writeFileSync(uploadPath, readFileSync(file));
    run("gh", ["release", "upload", tag, uploadPath, "--repo", RELEASE_REPO]);
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
}

async function uploadFileAsync(tag, file, name = path.basename(file)) {
  const temporary = mkdtempSync(path.join(os.tmpdir(), "ghostex-release-upload-"));
  try {
    const uploadPath = path.join(temporary, name);
    writeFileSync(uploadPath, readFileSync(file));
    await runBytesAsync("gh", ["release", "upload", tag, uploadPath, "--repo", RELEASE_REPO]);
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
}

export function uploadImmutableAsset(version, file, name = path.basename(file)) {
  if (!existsSync(file) || !statSync(file).isFile()) throw new Error(`Staged file is missing: ${file}`);
  let release = getRelease(version);
  const expectedSha = sha256(file);
  const existing = findAsset(release, name);
  if (existing) {
    const existingSha = assetSha256(existing);
    if (existingSha !== expectedSha) {
      throw new Error(
        `Refusing to overwrite staged asset ${name}: existing SHA256 ${existingSha}, new SHA256 ${expectedSha}. ` +
        "Use an explicit replacement procedure after auditing the release state.",
      );
    }
    console.log(`${name}: already staged with expected checksum; reusing it`);
    return { asset: existing, reused: true, sha256: expectedSha };
  }
  uploadFile(`v${version}`, file, name);
  release = getRelease(version);
  const uploaded = findAsset(release, name);
  if (!uploaded || assetSha256(uploaded) !== expectedSha) throw new Error(`Upload verification failed for ${name}`);
  console.log(`${name}: staged and verified`);
  return { asset: uploaded, reused: false, sha256: expectedSha };
}

async function uploadImmutableAssetAsync(version, file, name = path.basename(file)) {
  if (!existsSync(file) || !statSync(file).isFile()) throw new Error(`Staged file is missing: ${file}`);
  let release = getRelease(version);
  const expectedSha = sha256(file);
  const existing = findAsset(release, name);
  if (existing) {
    const existingSha = assetSha256(existing);
    if (existingSha !== expectedSha) {
      throw new Error(
        `Refusing to overwrite staged asset ${name}: existing SHA256 ${existingSha}, new SHA256 ${expectedSha}. ` +
        "Use an explicit replacement procedure after auditing the release state.",
      );
    }
    console.log(`${name}: already staged with expected checksum; reusing it`);
    return { asset: existing, reused: true, sha256: expectedSha };
  }
  await uploadFileAsync(`v${version}`, file, name);
  release = getRelease(version);
  const uploaded = findAsset(release, name);
  if (!uploaded || assetSha256(uploaded) !== expectedSha) throw new Error(`Upload verification failed for ${name}`);
  console.log(`${name}: staged and verified`);
  return { asset: uploaded, reused: false, sha256: expectedSha };
}

export function replaceReleaseState(version, state) {
  const release = getRelease(version);
  const existing = findAsset(release, STATE_ASSET);
  const temporary = mkdtempSync(path.join(os.tmpdir(), "ghostex-release-state-"));
  const statePath = path.join(temporary, STATE_ASSET);
  try {
    writeFileSync(statePath, `${JSON.stringify(state, null, 2)}\n`);
    // release-state.json is the mutable orchestration record. Deliverables and
    // their metadata never use this replacement path.
    if (existing) run("gh", ["api", "--method", "DELETE", `repos/${RELEASE_REPO}/releases/assets/${existing.id}`]);
    uploadFile(`v${version}`, statePath, STATE_ASSET);
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
  return state;
}

export function createInitialState({ channel = "stable", packages = null, sourceSha, updateSparkle = true, version, workflowSha = null }) {
  assertVersion(version);
  assertSha(sourceSha);
  if (!new Set(["stable", "prerelease", "test"]).has(channel)) throw new Error(`Unsupported release channel: ${channel}`);
  if (channel !== "stable" && updateSparkle) throw new Error(`${channel} releases cannot update the production Sparkle feed`);
  const packageScope = releasePackageNames(version, packages ? { packages } : null);
  if (updateSparkle && !packageScope.includes("macos-arm64")) {
    throw new Error("Sparkle publication requires macos-arm64 in the release package scope");
  }
  return {
    channel,
    completed: {},
    created_at: new Date().toISOString(),
    expected: expectedAssets(version, packageScope),
    github_release: { published: false },
    macos_notarization: {},
    packages: packageScope,
    schemaVersion: 2,
    source_sha: sourceSha,
    sparkle: { published: false, requested: Boolean(updateSparkle) },
    version,
    workflow_sha: workflowSha,
  };
}

export function ensureDraftRelease({ channel = "stable", packages = null, sourceSha, updateSparkle = true, version, workflowSha = null }) {
  let release = getRelease(version, { required: false });
  if (!release) {
    const notes = `Durable staging release for Ghostex ${version}. Assets remain draft until release-assemble verifies the complete manifest.`;
    const args = [
      "release", "create", `v${version}`,
      "--repo", RELEASE_REPO,
      "--target", sourceSha,
      "--title", `Ghostex ${version}`,
      "--notes", notes,
      "--draft",
    ];
    if (channel !== "stable") args.push("--prerelease");
    run("gh", args);
    // GitHub may print the new draft URL before the tag lookup and releases
    // collection expose it. Wait for that short consistency window instead of
    // leaving a valid but uninitialized draft that needs a manual rerun.
    release = waitForCreatedRelease(version);
  }
  let state = readJsonAsset(release, STATE_ASSET, { required: false });
  if (!state) {
    if (!release.draft) throw new Error(`Refusing to initialize resumable state on already-public release v${version}`);
    state = createInitialState({ channel, packages, sourceSha, updateSparkle, version, workflowSha });
    replaceReleaseState(version, state);
  }
  validateStateIdentity(state, { sourceSha, version });
  if (packages && JSON.stringify(releasePackageNames(version, state)) !== JSON.stringify(packages)) {
    throw new Error(`Release package scope ${JSON.stringify(releasePackageNames(version, state))} does not match ${JSON.stringify(packages)}`);
  }
  if (release.draft && release.target_commitish !== state.source_sha) {
    throw new Error(`Draft target ${release.target_commitish} does not match immutable source_sha ${state.source_sha}`);
  }
  if (state.channel !== channel) {
    throw new Error(`Release state channel ${state.channel} does not match ${channel}`);
  }
  if (Boolean(state.sparkle?.requested) !== Boolean(updateSparkle)) {
    throw new Error(`Release state Sparkle setting ${state.sparkle?.requested} does not match ${updateSparkle}`);
  }
  if (!release.draft && !state.github_release?.published) {
    state.github_release = { published: true, published_at: release.published_at ?? null };
    replaceReleaseState(version, state);
  }
  return { release: getRelease(version), state };
}

export function validateStateIdentity(state, { sourceSha, version }) {
  if (![1, 2].includes(state.schemaVersion)) throw new Error(`Unsupported release-state schema: ${state.schemaVersion}`);
  if (state.version !== version) throw new Error(`Release state version ${state.version} does not match ${version}`);
  if (sourceSha && state.source_sha !== sourceSha) {
    throw new Error(`Release state source_sha ${state.source_sha} does not match ${sourceSha}`);
  }
  assertSha(state.source_sha);
  const exactExpected = expectedAssets(version, releasePackageNames(version, state));
  if (JSON.stringify(state.expected) !== JSON.stringify(exactExpected)) {
    throw new Error(`Release expected allowlist is invalid: ${JSON.stringify(state.expected)} != ${JSON.stringify(exactExpected)}`);
  }
  if (state.source_compatibility !== undefined) {
    if (!state.source_compatibility || typeof state.source_compatibility !== "object" || Array.isArray(state.source_compatibility)) {
      throw new Error("Release source compatibility must be an object");
    }
    const enabledPackages = new Set(releasePackageNames(version, state));
    for (const [packageName, compatibility] of Object.entries(state.source_compatibility)) {
      if (!enabledPackages.has(packageName) || !state.completed?.[packageName]) {
        throw new Error(`Release source compatibility references an incomplete or disabled package: ${packageName}`);
      }
      assertSha(compatibility?.built_source_sha);
      if (compatibility.release_source_sha !== state.source_sha) {
        throw new Error(`Release source compatibility for ${packageName} does not target ${state.source_sha}`);
      }
      if (
        !Array.isArray(compatibility.audited_changes) ||
        compatibility.audited_changes.length === 0 ||
        compatibility.audited_changes.some((file) => typeof file !== "string" || !file)
      ) {
        throw new Error(`Release source compatibility for ${packageName} has no audited change list`);
      }
    }
  }
}

function sourcePathAffectsPackage(file, packageName) {
  if (
    file === "CHANGELOG.md" ||
    file.startsWith(".agents/skills/ghostex-release-operator/") ||
    file === "scripts/release-assemble-resumable.mjs" ||
    file === "scripts/release-resumable.mjs" ||
    file === "scripts/release-state-lib.mjs"
  ) {
    return false;
  }
  if (file === "apps/mobile/app") {
    return packageName === "android";
  }
  if (file.startsWith("apps/desktop/") || file.startsWith("sidebar/")) {
    return packageName === "macos-arm64";
  }
  return true;
}

export function rebaseDraftSource(version, { newSourceSha, rebuildPackages }) {
  assertVersion(version);
  assertSha(newSourceSha);
  const release = getRelease(version);
  if (!release.draft) throw new Error(`v${version} must still be a draft before its source can be updated`);
  const state = readJsonAsset(release, STATE_ASSET);
  validateStateIdentity(state, { version });
  const oldSourceSha = state.source_sha;
  if (oldSourceSha === newSourceSha) {
    console.log(`v${version}: draft already targets ${newSourceSha}`);
    return { affectedPackages: [], changedPaths: [], state };
  }
  if (run("git", ["merge-base", "--is-ancestor", oldSourceSha, newSourceSha], { allowFailure: true }).status !== 0) {
    throw new Error(`New source ${newSourceSha} must descend from existing release source ${oldSourceSha}`);
  }
  run("gh", ["api", `repos/${RELEASE_REPO}/commits/${newSourceSha}`], { capture: true });
  const packageVersion = JSON.parse(run("git", ["show", `${newSourceSha}:package.json`], { capture: true }).stdout).version;
  if (packageVersion !== version) throw new Error(`package.json at ${newSourceSha} is ${packageVersion}; expected ${version}`);

  const changedPaths = run("git", ["diff", "--name-only", oldSourceSha, newSourceSha], { capture: true }).stdout
    .split(/\r?\n/)
    .filter(Boolean);
  if (changedPaths.length === 0) throw new Error(`No source changes exist between ${oldSourceSha} and ${newSourceSha}`);
  const contracts = selectedReleaseContracts(version, state);
  const requestedRebuilds = new Set(rebuildPackages);
  for (const packageName of requestedRebuilds) {
    if (!contracts.has(packageName)) throw new Error(`Cannot rebuild disabled package ${packageName}`);
  }
  const affectedPackages = [...contracts.keys()].filter((packageName) =>
    changedPaths.some((file) => sourcePathAffectsPackage(file, packageName)));
  const missingRebuilds = affectedPackages.filter((packageName) => !requestedRebuilds.has(packageName));
  if (missingRebuilds.length > 0) {
    throw new Error(
      `Source update affects packages not selected for rebuild: ${missingRebuilds.join(", ")}. ` +
      `Audited paths: ${changedPaths.join(", ")}`,
    );
  }

  for (const [packageName, contract] of contracts) {
    if (requestedRebuilds.has(packageName)) {
      if (state.completed?.[packageName]) {
        throw new Error(`Refusing to rebase with already-completed rebuild package ${packageName}`);
      }
      for (const assetName of contract.assets) {
        if (findAsset(release, assetName) || findAsset(release, `${assetName}${METADATA_SUFFIX}`)) {
          throw new Error(`Refusing to rebase while rebuild package ${packageName} already has staged asset ${assetName}`);
        }
      }
      continue;
    }
    if (!state.completed?.[packageName]) {
      throw new Error(`Wait for unaffected package ${packageName} to complete before rebasing the draft source`);
    }
  }

  const sourceCompatibility = { ...(state.source_compatibility ?? {}) };
  for (const packageName of contracts.keys()) {
    if (requestedRebuilds.has(packageName)) {
      delete sourceCompatibility[packageName];
      continue;
    }
    const priorCompatibility = sourceCompatibility[packageName];
    sourceCompatibility[packageName] = {
      audited_changes: [...new Set([
        ...(priorCompatibility?.audited_changes ?? []),
        ...changedPaths,
      ])],
      built_source_sha: priorCompatibility?.built_source_sha ?? oldSourceSha,
      release_source_sha: newSourceSha,
    };
  }
  state.source_compatibility = sourceCompatibility;
  state.source_sha = newSourceSha;
  state.workflow_sha = newSourceSha;
  run("gh", [
    "api",
    "--method", "PATCH",
    `repos/${RELEASE_REPO}/releases/${release.id}`,
    "-f", `tag_name=v${version}`,
    "-f", `target_commitish=${newSourceSha}`,
  ], { capture: true });
  replaceReleaseState(version, state);
  const rebased = validateStagedRelease(version);
  console.log(
    `v${version}: rebased ${oldSourceSha} -> ${newSourceSha}; rebuilding ${[...requestedRebuilds].join(", ")}; ` +
    `carried packages retain audited build-source metadata`,
  );
  return { affectedPackages, changedPaths, state: rebased.state };
}

export function createMetadata({ architecture, asset, packageName, reusedFrom = null, sourceSha, version, workflowRunId, workflowSha }) {
  assertVersion(version);
  assertSha(sourceSha);
  return {
    architecture,
    asset: path.basename(asset),
    created_at: new Date().toISOString(),
    package: packageName,
    schemaVersion: 1,
    sha256: sha256(asset),
    size: statSync(asset).size,
    source_sha: sourceSha,
    version,
    workflow_run_id: Number(workflowRunId || 0),
    workflow_sha: workflowSha || null,
    ...(reusedFrom ? { reused_from: reusedFrom } : {}),
  };
}

export function stagePackage({ artifactDirectory, channel, packageName, reusedFrom = null, sourceSha, updateSparkle, version, workflowRunId, workflowSha }) {
  ensureDraftRelease({ channel, sourceSha, updateSparkle, version, workflowSha });
  const releaseState = readJsonAsset(getRelease(version), STATE_ASSET);
  const contract = selectedReleaseContracts(version, releaseState).get(packageName);
  if (!contract) throw new Error(`Package ${packageName} is not enabled for this release`);
  const metadata = [];
  for (const assetName of contract.assets) {
    const assetPath = path.join(artifactDirectory, assetName);
    if (!existsSync(assetPath)) throw new Error(`${packageName} is missing required asset ${assetName}`);
    let entry = createMetadata({
      architecture: contract.architecture,
      asset: assetPath,
      packageName,
      reusedFrom,
      sourceSha,
      version,
      workflowRunId,
      workflowSha,
    });
    uploadImmutableAsset(version, assetPath, assetName);
    const metadataName = `${assetName}${METADATA_SUFFIX}`;
    const currentRelease = getRelease(version);
    const existingMetadataAsset = findAsset(currentRelease, metadataName);
    if (existingMetadataAsset) {
      const existing = JSON.parse(downloadAsset(existingMetadataAsset).toString("utf8"));
      if (
        existing.schemaVersion !== 1 || existing.version !== version || existing.source_sha !== sourceSha ||
        existing.package !== packageName || existing.architecture !== contract.architecture ||
        existing.asset !== assetName || existing.sha256 !== entry.sha256 || Number(existing.size) !== entry.size ||
        JSON.stringify(existing.reused_from ?? null) !== JSON.stringify(reusedFrom)
      ) {
        throw new Error(`Refusing to overwrite mismatched staged metadata ${metadataName}`);
      }
      console.log(`${metadataName}: already staged for the same immutable asset; reusing it`);
      entry = existing;
      metadata.push(entry);
      continue;
    }
    const temporary = mkdtempSync(path.join(os.tmpdir(), "ghostex-release-metadata-"));
    try {
      const metadataPath = path.join(temporary, metadataName);
      writeFileSync(metadataPath, `${JSON.stringify(entry, null, 2)}\n`);
      uploadImmutableAsset(version, metadataPath, metadataName);
    } finally {
      rmSync(temporary, { force: true, recursive: true });
    }
    metadata.push(entry);
  }
  const nextState = readJsonAsset(getRelease(version), STATE_ASSET);
  validateStateIdentity(nextState, { sourceSha, version });
  const newlyCompleted = !nextState.completed?.[packageName];
  if (newlyCompleted) {
    nextState.completed[packageName] = {
      assets: Object.fromEntries(metadata.map((entry) => [entry.asset, entry.sha256])),
      completed_at: new Date().toISOString(),
      run_id: Number(workflowRunId || 0),
      workflow_sha: workflowSha || null,
      ...(reusedFrom ? { reused_from: reusedFrom } : {}),
    };
  }
  if (packageName === "macos-arm64") {
    nextState.macos_notarization = {
      ...nextState.macos_notarization,
      accepted_at: new Date().toISOString(),
      stapled: true,
      status: "accepted",
    };
  }
  if (newlyCompleted || packageName === "macos-arm64") replaceReleaseState(version, nextState);
  else console.log(`${contract.label}: release state already records this package; no state transition needed`);
  return { metadata, newlyCompleted, state: nextState };
}

async function prepareReusablePackage(source, temporaryRoot, packageName, contract) {
  const packageDirectory = path.join(temporaryRoot, packageName);
  const prepared = [];
  for (const assetName of contract.assets) {
    const sourceAsset = findAsset(source.release, assetName);
    if (!sourceAsset) throw new Error(`v${source.state.version} is missing ${assetName}`);
    prepared.push((async () => {
      const bytes = await downloadAssetAsync(sourceAsset);
      const assetPath = path.join(packageDirectory, assetName);
      writeFileSync(assetPath, bytes);
      const downloadedSha = sha256(assetPath);
      const expectedSha = source.completed[packageName].assets[assetName];
      if (downloadedSha !== expectedSha) {
        throw new Error(`Downloaded ${assetName} checksum ${downloadedSha} does not match v${source.state.version} digest ${expectedSha}`);
      }
    })());
  }
  await Promise.all(prepared);
  return { artifactDirectory: packageDirectory, packageName };
}

async function stageReusablePackageAssets({ artifactDirectory, contract, packageName, reusedFrom, sourceSha, version, workflowSha }) {
  const metadata = [];
  for (const assetName of contract.assets) {
    const assetPath = path.join(artifactDirectory, assetName);
    const entry = createMetadata({
      architecture: contract.architecture,
      asset: assetPath,
      packageName,
      reusedFrom,
      sourceSha,
      version,
      workflowRunId: 0,
      workflowSha,
    });
    await uploadImmutableAssetAsync(version, assetPath, assetName);
    const metadataName = `${assetName}${METADATA_SUFFIX}`;
    const currentRelease = getRelease(version);
    const existingMetadataAsset = findAsset(currentRelease, metadataName);
    if (existingMetadataAsset) {
      const existing = JSON.parse(downloadAsset(existingMetadataAsset).toString("utf8"));
      if (
        existing.schemaVersion !== 1 || existing.version !== version || existing.source_sha !== sourceSha ||
        existing.package !== packageName || existing.architecture !== contract.architecture ||
        existing.asset !== assetName || existing.sha256 !== entry.sha256 || Number(existing.size) !== entry.size ||
        JSON.stringify(existing.reused_from ?? null) !== JSON.stringify(reusedFrom)
      ) {
        throw new Error(`Refusing to overwrite mismatched staged metadata ${metadataName}`);
      }
    } else {
      const metadataDirectory = mkdtempSync(path.join(os.tmpdir(), "ghostex-release-metadata-"));
      try {
        const metadataPath = path.join(metadataDirectory, metadataName);
        writeFileSync(metadataPath, `${JSON.stringify(entry, null, 2)}\n`);
        await uploadImmutableAssetAsync(version, metadataPath, metadataName);
      } finally {
        rmSync(metadataDirectory, { force: true, recursive: true });
      }
    }
    metadata.push(entry);
  }
  return metadata;
}

export async function reuseGxserverPackagesFromRelease(version, { fromVersion }) {
  assertVersion(version);
  assertVersion(fromVersion);
  if (fromVersion === version) throw new Error("A release cannot reuse packages from itself");

  const packageNames = ["gxserver-linux-x64", "gxserver-linux-arm64"];
  const targetRelease = getRelease(version);
  if (!targetRelease.draft) throw new Error(`v${version} must still be a draft before packages can be reused`);
  const targetState = readJsonAsset(targetRelease, STATE_ASSET);
  validateStateIdentity(targetState, { version });
  const targetContracts = selectedReleaseContracts(version, targetState);
  for (const packageName of packageNames) {
    if (!targetContracts.has(packageName)) throw new Error(`Package ${packageName} is not enabled for v${version}`);
  }

  const source = validateStagedRelease(fromVersion, { requireComplete: true });
  if (source.release.draft) throw new Error(`Refusing to reuse packages from draft release v${fromVersion}`);
  for (const packageName of packageNames) {
    if (!source.completed[packageName]) throw new Error(`v${fromVersion} has no verified ${packageName} package`);
  }

  const temporary = mkdtempSync(path.join(os.tmpdir(), "ghostex-reuse-gxserver-"));
  try {
    // Network copies and checksum calculations are independent. Keep those in
    // parallel, then serialize staging because release-state.json is mutable.
    for (const packageName of packageNames) {
      const packageDirectory = path.join(temporary, packageName);
      mkdirSync(packageDirectory, { recursive: true });
    }
    const prepared = await Promise.all(packageNames.map((packageName) =>
      prepareReusablePackage(source, temporary, packageName, targetContracts.get(packageName))));
    const reusedFrom = {
      source_sha: source.state.source_sha,
      tag: `v${fromVersion}`,
      version: fromVersion,
    };
    // Deliverable and metadata asset names are disjoint and immutable, so both
    // copies can safely upload and verify together. State is recorded below in
    // a serial loop because release-state.json is the one mutable release asset.
    await Promise.all(prepared.map((item) => stageReusablePackageAssets({
      artifactDirectory: item.artifactDirectory,
      contract: targetContracts.get(item.packageName),
      packageName: item.packageName,
      reusedFrom,
      sourceSha: targetState.source_sha,
      version,
      workflowSha: targetState.workflow_sha,
    })));
    const staged = [];
    for (const item of prepared) {
      staged.push(stagePackage({
        artifactDirectory: item.artifactDirectory,
        channel: targetState.channel,
        packageName: item.packageName,
        reusedFrom,
        sourceSha: targetState.source_sha,
        updateSparkle: targetState.sparkle.requested,
        version,
        workflowRunId: 0,
        workflowSha: targetState.workflow_sha,
      }));
      console.log(`${targetContracts.get(item.packageName).label}: reused byte-for-byte from public v${fromVersion} with verified checksums`);
    }
    return staged;
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
}

export function reusePackageFromRelease(version, { fromVersion, packageName }) {
  assertVersion(version);
  assertVersion(fromVersion);
  if (fromVersion === version) throw new Error("A release cannot reuse a package from itself");
  if (!packageName.startsWith("gxserver-linux-")) {
    throw new Error(`Only immutable gxserver runtime packages may be reused, got ${packageName}`);
  }

  const targetRelease = getRelease(version);
  if (!targetRelease.draft) throw new Error(`v${version} must still be a draft before packages can be reused`);
  const targetState = readJsonAsset(targetRelease, STATE_ASSET);
  validateStateIdentity(targetState, { version });
  const contract = selectedReleaseContracts(version, targetState).get(packageName);
  if (!contract) throw new Error(`Package ${packageName} is not enabled for v${version}`);

  const source = validateStagedRelease(fromVersion, { requireComplete: true });
  if (source.release.draft) throw new Error(`Refusing to reuse packages from draft release v${fromVersion}`);
  if (!source.completed[packageName]) throw new Error(`v${fromVersion} has no verified ${packageName} package`);

  const temporary = mkdtempSync(path.join(os.tmpdir(), `ghostex-reuse-${packageName}-`));
  try {
    for (const assetName of contract.assets) {
      const sourceAsset = findAsset(source.release, assetName);
      if (!sourceAsset) throw new Error(`v${fromVersion} is missing ${assetName}`);
      const assetPath = path.join(temporary, assetName);
      writeFileSync(assetPath, downloadAsset(sourceAsset));
      const downloadedSha = sha256(assetPath);
      const publishedSha = assetSha256(sourceAsset);
      if (downloadedSha !== publishedSha) {
        throw new Error(`Downloaded ${assetName} checksum ${downloadedSha} does not match v${fromVersion} digest ${publishedSha}`);
      }
    }
    const reusedFrom = {
      source_sha: source.state.source_sha,
      tag: `v${fromVersion}`,
      version: fromVersion,
    };
    const staged = stagePackage({
      artifactDirectory: temporary,
      channel: targetState.channel,
      packageName,
      reusedFrom,
      sourceSha: targetState.source_sha,
      updateSparkle: targetState.sparkle.requested,
      version,
      workflowRunId: 0,
      workflowSha: targetState.workflow_sha,
    });
    console.log(`${contract.label}: reused byte-for-byte from public v${fromVersion} with verified checksums`);
    return staged;
  } finally {
    rmSync(temporary, { force: true, recursive: true });
  }
}

export function validateStagedRelease(version, { requireComplete = false, sourceSha = null } = {}) {
  const release = getRelease(version);
  const state = readJsonAsset(release, STATE_ASSET);
  validateStateIdentity(state, { sourceSha, version });
  if (release.draft && release.target_commitish !== state.source_sha) {
    throw new Error(`Draft target ${release.target_commitish} does not match immutable source_sha ${state.source_sha}`);
  }
  const contracts = selectedReleaseContracts(version, state);
  const allowed = new Set([STATE_ASSET]);
  for (const name of state.expected) {
    allowed.add(name);
    allowed.add(`${name}${METADATA_SUFFIX}`);
  }
  const unexpected = (release.assets ?? []).map((asset) => asset.name).filter((name) => !allowed.has(name));
  if (unexpected.length > 0) throw new Error(`Draft contains unexpected or disabled assets: ${unexpected.join(", ")}`);

  const completed = {};
  const errors = [];
  for (const [packageName, contract] of contracts) {
    const entries = [];
    for (const name of contract.assets) {
      const asset = findAsset(release, name);
      const metadataAsset = findAsset(release, `${name}${METADATA_SUFFIX}`);
      if (!asset || !metadataAsset) {
        errors.push(`${packageName}: missing ${!asset ? name : `${name}${METADATA_SUFFIX}`}`);
        continue;
      }
      const metadata = JSON.parse(downloadAsset(metadataAsset).toString("utf8"));
      const actualSha = assetSha256(asset);
      const actualSize = Number(asset.size);
      const compatibleSource = state.source_compatibility?.[packageName]?.built_source_sha;
      if (
        metadata.schemaVersion !== 1 || metadata.version !== version ||
        (metadata.source_sha !== state.source_sha && metadata.source_sha !== compatibleSource) ||
        metadata.package !== packageName || metadata.architecture !== contract.architecture ||
        metadata.asset !== name || metadata.sha256 !== actualSha || Number(metadata.size) !== actualSize
      ) {
        errors.push(`${packageName}: invalid metadata/checksum for ${name}`);
        continue;
      }
      entries.push(metadata);
    }
    if (entries.length === contract.assets.length) {
      const recorded = state.completed?.[packageName];
      const recordedAssetsMatch = recorded?.assets && entries.every((entry) => recorded.assets[entry.asset] === entry.sha256);
      completed[packageName] = {
        assets: Object.fromEntries(entries.map((entry) => [entry.asset, entry.sha256])),
        compatible_from: state.source_compatibility?.[packageName]?.built_source_sha,
        reused_from: recordedAssetsMatch ? recorded.reused_from : entries[0].reused_from,
        run_id: recordedAssetsMatch ? recorded.run_id : entries[0].workflow_run_id,
        workflow_sha: recordedAssetsMatch ? recorded.workflow_sha : entries[0].workflow_sha,
      };
    }
  }
  if (requireComplete && errors.length > 0) throw new Error(`Release staging is incomplete or invalid:\n- ${errors.join("\n- ")}`);
  return { completed, errors, release, state };
}

export function printStatus(version) {
  const result = validateStagedRelease(version);
  const lines = [];
  const contracts = selectedReleaseContracts(version, result.state);
  for (const [packageName, contract] of contracts) {
    const ready = result.completed[packageName];
    if (ready) {
      const origin = ready.compatible_from
        ? `carried from audited source ${ready.compatible_from.slice(0, 10)}`
        : ready.reused_from?.tag
        ? `reused from ${ready.reused_from.tag}`
        : `run ${ready.run_id || "unknown"}`;
      lines.push(`${contract.label.padEnd(26)} ready — ${origin}`);
    } else if (packageName === "macos-arm64" && result.state.macos_notarization?.submission_id) {
      lines.push(`${contract.label.padEnd(26)} failed — resume notarization ${result.state.macos_notarization.submission_id}`);
    } else if (packageName === "macos-arm64" && result.state.macos_notarization?.signed_dmg_run_id) {
      lines.push(`${contract.label.padEnd(26)} signed — submit preserved run ${result.state.macos_notarization.signed_dmg_run_id}`);
    } else {
      const reason = result.errors.find((entry) => entry.startsWith(`${packageName}:`))?.split(": ").slice(1).join(": ") ?? "not built";
      lines.push(`${contract.label.padEnd(26)} missing — ${reason}`);
    }
  }
  const complete = Object.keys(result.completed).length === contracts.size;
  lines.push(`${"GitHub release".padEnd(26)} ${result.release.draft ? (complete ? "ready to assemble" : "waiting for packages") : "published"}`);
  lines.push(`${"Sparkle".padEnd(26)} ${result.state.sparkle?.published ? "published" : result.state.sparkle?.requested ? "not published" : "disabled"}`);
  console.log(lines.join("\n"));
  return result;
}

export function dispatchWorkflow(workflow, fields) {
  const args = ["workflow", "run", workflow, "--repo", RELEASE_REPO, "--ref", "main"];
  for (const [name, value] of Object.entries(fields)) {
    if (value === undefined || value === null || value === "") continue;
    args.push("-f", `${name}=${value}`);
  }
  run("gh", args);
}

function workflowFields(state, completed, packageName) {
  const fields = {
    channel: state.channel,
    source_sha: state.source_sha,
    update_sparkle: state.sparkle.requested,
    version: state.version,
  };
  if (packageName === "macos-arm64") {
    fields.gxserver_x64_run_id = completed["gxserver-linux-x64"].run_id;
    fields.gxserver_arm64_run_id = completed["gxserver-linux-arm64"].run_id;
    const notarization = state.macos_notarization ?? {};
    if (notarization.submission_id) {
      fields.macos_stage = "poll-staple";
      fields.prerequisite_run_id = notarization.signed_dmg_run_id;
      fields.submission_id = notarization.submission_id;
    } else if (notarization.signed_dmg_run_id) {
      fields.macos_stage = "submit";
      fields.prerequisite_run_id = notarization.signed_dmg_run_id;
    }
  }
  return fields;
}

export function planAdvanceAfterStaging(version, result, stagedPackage) {
  const contracts = selectedReleaseContracts(version, result.state);
  if (!contracts.has(stagedPackage)) throw new Error(`Package ${stagedPackage} is not enabled for this release`);
  if (!result.completed[stagedPackage]) throw new Error(`${stagedPackage} is not validly staged and cannot advance the release`);
  const decisions = [];
  for (const [packageName, contract] of contracts) {
    if (result.completed[packageName]) continue;
    const dependencies = contract.dependencies ?? [];
    if (!dependencies.includes(stagedPackage)) continue;
    if (!dependencies.every((dependency) => result.completed[dependency])) continue;
    decisions.push({
      fields: workflowFields(result.state, result.completed, packageName),
      label: contract.label,
      workflow: contract.workflow,
    });
  }
  const complete = Object.keys(result.completed).length === contracts.size;
  const assemblyNeeded = complete && (
    result.release.draft || !result.state.github_release?.published ||
    (result.state.sparkle?.requested && !result.state.sparkle?.published)
  );
  return { assemblyNeeded, decisions };
}

export function advanceAfterStaging(version, stagedPackage, { dryRun = false } = {}) {
  const result = printStatus(version);
  const { assemblyNeeded, decisions } = planAdvanceAfterStaging(version, result, stagedPackage);
  if (decisions.length > 0) {
    console.log("\nNewly eligible dispatches:");
    for (const decision of decisions) console.log(`  ${decision.label}: build/resume via ${decision.workflow}`);
    if (!dryRun) for (const decision of decisions) dispatchWorkflow(decision.workflow, decision.fields);
    return decisions;
  }
  if (assemblyNeeded) {
    console.log(`All packages are ready; dispatching ${result.release.draft ? "assembly" : "publication recovery"}.`);
    if (!dryRun) dispatchWorkflow("release-assemble.yml", {
      channel: result.state.channel,
      source_sha: result.state.source_sha,
      update_sparkle: result.state.sparkle.requested,
      version,
    });
  } else {
    console.log(`No package became eligible after staging ${stagedPackage}.`);
  }
  return decisions;
}

export function dispatchMissing(version, { dryRun = false } = {}) {
  const result = printStatus(version);
  const decisions = [];
  const contracts = selectedReleaseContracts(version, result.state);
  for (const [packageName, contract] of contracts) {
    if (result.completed[packageName]) continue;
    const dependenciesReady = (contract.dependencies ?? []).every((dependency) => result.completed[dependency]);
    if (!dependenciesReady) continue;
    const fields = workflowFields(result.state, result.completed, packageName);
    decisions.push({ fields, label: contract.label, workflow: contract.workflow });
  }
  if (decisions.length === 0) {
    const complete = Object.keys(result.completed).length === contracts.size;
    const assemblyNeeded = result.release.draft || !result.state.github_release?.published ||
      (result.state.sparkle?.requested && !result.state.sparkle?.published);
    if (complete && assemblyNeeded) {
      console.log(`All packages are ready; dispatching ${result.release.draft ? "assembly" : "publication recovery"}.`);
      if (!dryRun) dispatchWorkflow("release-assemble.yml", {
        channel: result.state.channel,
        source_sha: result.state.source_sha,
        update_sparkle: result.state.sparkle.requested,
        version,
      });
    } else {
      console.log("No package workflow is currently dispatchable.");
    }
    return decisions;
  }
  console.log("\nDispatch plan:");
  for (const decision of decisions) console.log(`  ${decision.label}: build/resume via ${decision.workflow}`);
  if (!dryRun) for (const decision of decisions) dispatchWorkflow(decision.workflow, decision.fields);
  return decisions;
}

export function recordMacosSubmission(version, { dmgSha256, signedDmgRunId, sourceSha, submissionId }) {
  const release = getRelease(version);
  const state = readJsonAsset(release, STATE_ASSET);
  validateStateIdentity(state, { sourceSha, version });
  state.macos_notarization = {
    dmg_sha256: dmgSha256,
    signed_dmg_run_id: Number(signedDmgRunId),
    status: "submitted",
    submission_id: submissionId,
    submitted_at: new Date().toISOString(),
  };
  replaceReleaseState(version, state);
}

export function recordMacosSigned(version, { channel, dmgSha256, signedDmgRunId, sourceSha, updateSparkle, workflowSha }) {
  ensureDraftRelease({ channel, sourceSha, updateSparkle, version, workflowSha });
  const release = getRelease(version);
  const state = readJsonAsset(release, STATE_ASSET);
  validateStateIdentity(state, { sourceSha, version });
  state.macos_notarization = {
    dmg_sha256: dmgSha256,
    signed_at: new Date().toISOString(),
    signed_dmg_run_id: Number(signedDmgRunId),
    status: "signed",
  };
  replaceReleaseState(version, state);
}

export function markPublished(version, { githubPublished = false, sparklePublished = false } = {}) {
  const release = getRelease(version);
  const state = readJsonAsset(release, STATE_ASSET);
  if (githubPublished) state.github_release = { published: true, published_at: new Date().toISOString() };
  if (sparklePublished) state.sparkle = { ...state.sparkle, published: true, published_at: new Date().toISOString() };
  replaceReleaseState(version, state);
  return state;
}

export function replaceStagedAsset(version, { assetName, expectedOldSha }) {
  const release = getRelease(version);
  if (!release.draft) throw new Error("Staged assets can only be explicitly replaced while the release is a draft");
  const state = readJsonAsset(release, STATE_ASSET);
  validateStateIdentity(state, { version });
  if (!state.expected.includes(assetName)) throw new Error(`${assetName} is not in the release deliverable allowlist`);
  const asset = findAsset(release, assetName);
  if (!asset) throw new Error(`Staged asset is already absent: ${assetName}`);
  const actual = assetSha256(asset);
  if (actual !== expectedOldSha) throw new Error(`Expected old SHA256 ${expectedOldSha}, but ${assetName} is ${actual}`);
  const metadata = findAsset(release, `${assetName}${METADATA_SUFFIX}`);
  run("gh", ["api", "--method", "DELETE", `repos/${RELEASE_REPO}/releases/assets/${asset.id}`]);
  if (metadata) run("gh", ["api", "--method", "DELETE", `repos/${RELEASE_REPO}/releases/assets/${metadata.id}`]);
  const packageName = [...selectedReleaseContracts(version, state)].find(([, contract]) => contract.assets.includes(assetName))?.[0];
  if (packageName) delete state.completed[packageName];
  if (packageName === "macos-arm64") state.macos_notarization = {};
  replaceReleaseState(version, state);
  console.log(`Removed explicitly authorized draft asset ${assetName} and its metadata; the package is now missing.`);
}
