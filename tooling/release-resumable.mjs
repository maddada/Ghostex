#!/usr/bin/env node
import { readFileSync } from "node:fs";
import {
  advanceAfterStaging,
  assertSha,
  assertVersion,
  dispatchMissing,
  dispatchWorkflow,
  ensureDraftRelease,
  getRelease,
  printStatus,
  readJsonAsset,
  rebaseDraftSource,
  recordMacosSubmission,
  recordMacosSigned,
  RELEASE_REPO,
  replaceStagedAsset,
  reuseGxserverPackagesFromRelease,
  run,
  selectedReleaseContracts,
  stagePackage,
  STATE_ASSET,
  validateStagedRelease,
} from "./release-state-lib.mjs";

const usage = `
Usage:
  bun run release:start:resumable -- <version> [--source-sha <sha>] [--channel stable|prerelease|test] [--skip-sparkle] [--only-macos] [--reuse-gxserver-from <version>]
  bun run release:status -- <version>
  bun run release:resume -- <version> [--dry-run]
  node tooling/release-resumable.mjs rebase-draft-source <version> --new-source <sha> --rebuild <package[,package...]>
  bun run release:retry -- <version> android|gxserver-linux-x64|gxserver-linux-arm64|macos|macos-submit|macos-notarization
  bun run release:assemble -- <version>
  bun run release:verify -- <version>
  bun run release:replace -- <version> <asset> --expect-sha <old-sha256> --confirm-replace

CI-only commands:
  node tooling/release-resumable.mjs stage-package <version> <source_sha> <package> <artifact-dir> <workflow_sha> <channel> <update-sparkle>
  node tooling/release-resumable.mjs stage-package-and-advance <version> <source_sha> <package> <artifact-dir> <workflow_sha> <channel> <update-sparkle>
  node tooling/release-resumable.mjs record-macos-submission <version> <source_sha> <submission-id> <dmg-sha256> <signed-dmg-run-id>
  node tooling/release-resumable.mjs record-macos-signed <version> <source_sha> <dmg-sha256> <signed-dmg-run-id> <workflow-sha> <channel> <update-sparkle>
  node tooling/release-resumable.mjs validate-source <version> <source_sha>
`;

const args = process.argv.slice(2);
const command = args.shift();
if (!command || command === "--help" || command === "help") {
  console.log(usage.trim());
  process.exit(command ? 0 : 2);
}

function takeOption(name, defaultValue = null) {
  const index = args.indexOf(name);
  if (index < 0) return defaultValue;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) throw new Error(`${name} requires a value`);
  args.splice(index, 2);
  return value;
}

function takeFlag(name) {
  const index = args.indexOf(name);
  if (index < 0) return false;
  args.splice(index, 1);
  return true;
}

function localSourceSha() {
  return run("git", ["rev-parse", "HEAD"], { capture: true }).stdout;
}

function assertSourceVersion(sourceSha, version) {
  const packageJson = run("git", ["show", `${sourceSha}:package.json`], { capture: true }).stdout;
  const actual = JSON.parse(packageJson).version;
  if (actual !== version) throw new Error(`package.json at ${sourceSha} is ${actual}; expected ${version}`);
}

switch (command) {
  case "start": {
    const version = args.shift();
    assertVersion(version);
    const sourceSha = takeOption("--source-sha", localSourceSha());
    const channel = takeOption("--channel", "stable");
    const updateSparkle = !takeFlag("--skip-sparkle");
    const onlyMacos = takeFlag("--only-macos");
    const reuseGxserverFrom = takeOption("--reuse-gxserver-from");
    if (args.length > 0) throw new Error(`Unknown arguments: ${args.join(" ")}`);
    assertSha(sourceSha);
    if (reuseGxserverFrom) assertVersion(reuseGxserverFrom);
    assertSourceVersion(sourceSha, version);
    run("gh", ["api", `repos/${RELEASE_REPO}/commits/${sourceSha}`], { capture: true });
    const workflowSha = run("git", ["rev-parse", "origin/main"], { capture: true }).stdout;
    const packages = onlyMacos
      ? ["gxserver-linux-x64", "gxserver-linux-arm64", "macos-arm64"]
      : null;
    ensureDraftRelease({ channel, packages, sourceSha, updateSparkle, version, workflowSha });
    console.log(`Created/resumed draft v${version} for source ${sourceSha}.`);
    if (reuseGxserverFrom) {
      await reuseGxserverPackagesFromRelease(version, { fromVersion: reuseGxserverFrom });
    }
    dispatchMissing(version);
    break;
  }
  case "status": {
    const version = args.shift();
    assertVersion(version);
    if (args.length > 0) throw new Error(`Unknown arguments: ${args.join(" ")}`);
    printStatus(version);
    break;
  }
  case "resume": {
    const version = args.shift();
    assertVersion(version);
    const dryRun = takeFlag("--dry-run");
    if (args.length > 0) throw new Error(`Unknown arguments: ${args.join(" ")}`);
    dispatchMissing(version, { dryRun });
    break;
  }
  case "rebase-draft-source": {
    const version = args.shift();
    assertVersion(version);
    const newSourceSha = takeOption("--new-source");
    const rebuildPackages = (takeOption("--rebuild", "") ?? "").split(",").filter(Boolean);
    if (!newSourceSha || rebuildPackages.length === 0) {
      throw new Error("rebase-draft-source requires --new-source and at least one --rebuild package");
    }
    if (args.length > 0) throw new Error(`Unknown arguments: ${args.join(" ")}`);
    rebaseDraftSource(version, { newSourceSha, rebuildPackages });
    break;
  }
  case "retry": {
    const version = args.shift();
    const target = args.shift();
    assertVersion(version);
    if (args.length > 0) throw new Error(`Unknown arguments: ${args.join(" ")}`);
    const { completed, state } = validateStagedRelease(version);
    const normalized = target === "macos" ? "macos-arm64" : target;
    if (target === "macos-submit") {
      const notarization = state.macos_notarization ?? {};
      if (!notarization.signed_dmg_run_id) throw new Error("No preserved signed macOS run is recorded");
      console.log(`macOS notarization: submit preserved signed run ${notarization.signed_dmg_run_id}; no compilation will run`);
      dispatchWorkflow("release-build-macos.yml", {
        channel: state.channel,
        macos_stage: "submit",
        prerequisite_run_id: notarization.signed_dmg_run_id,
        source_sha: state.source_sha,
        update_sparkle: state.sparkle.requested,
        version,
      });
      break;
    }
    if (target === "macos-notarization") {
      const notarization = state.macos_notarization ?? {};
      if (!notarization.submission_id || !notarization.signed_dmg_run_id) {
        throw new Error("No saved macOS submission ID and signed-DMG run ID are available to resume");
      }
      console.log(`macOS notarization: resume submission ${notarization.submission_id}; no compilation will run`);
      dispatchWorkflow("release-build-macos.yml", {
        channel: state.channel,
        macos_stage: "poll-staple",
        prerequisite_run_id: notarization.signed_dmg_run_id,
        source_sha: state.source_sha,
        submission_id: notarization.submission_id,
        update_sparkle: state.sparkle.requested,
        version,
      });
      break;
    }
    const contract = selectedReleaseContracts(version, state).get(normalized);
    if (!contract) throw new Error(`Unknown retry target: ${target}`);
    const fields = {
      channel: state.channel,
      source_sha: state.source_sha,
      update_sparkle: state.sparkle.requested,
      version,
    };
    if (normalized === "macos-arm64") {
      if (!completed["gxserver-linux-x64"] || !completed["gxserver-linux-arm64"]) {
        throw new Error("macOS retry requires both staged gxserver packages");
      }
      fields.gxserver_x64_run_id = completed["gxserver-linux-x64"].run_id;
      fields.gxserver_arm64_run_id = completed["gxserver-linux-arm64"].run_id;
      fields.macos_stage = "all";
    }
    console.log(`${contract.label}: explicit retry requested; staged assets will still be checksum-protected`);
    dispatchWorkflow(contract.workflow, fields);
    break;
  }
  case "assemble": {
    const version = args.shift();
    assertVersion(version);
    const { state } = validateStagedRelease(version, { requireComplete: true });
    console.log(`All staged packages for ${version} are valid; dispatching build-free assembly.`);
    dispatchWorkflow("release-assemble.yml", {
      channel: state.channel,
      source_sha: state.source_sha,
      update_sparkle: state.sparkle.requested,
      version,
    });
    break;
  }
  case "verify": {
    const version = args.shift();
    assertVersion(version);
    const { release, state } = validateStagedRelease(version, { requireComplete: true });
    if (release.draft) throw new Error(`v${version} is still a draft`);
    const tagLines = run("git", ["ls-remote", "origin", `refs/tags/v${version}`, `refs/tags/v${version}^{}`], { capture: true }).stdout.split(/\r?\n/);
    const peeled = tagLines.find((line) => line.endsWith(`refs/tags/v${version}^{}`));
    const direct = tagLines.find((line) => line.endsWith(`refs/tags/v${version}`));
    if ((peeled ?? direct)?.split(/\s+/)[0] !== state.source_sha) throw new Error(`v${version} does not resolve to ${state.source_sha}`);
    if (state.sparkle.requested && !state.sparkle.published) throw new Error(`Sparkle has not been published for ${version}`);
    console.log(`v${version} is public with the exact verified asset allowlist${state.sparkle.requested ? " and Sparkle is published" : ""}.`);
    break;
  }
  case "replace": {
    const version = args.shift();
    const assetName = args.shift();
    const expectedOldSha = takeOption("--expect-sha");
    const confirmed = takeFlag("--confirm-replace");
    assertVersion(version);
    if (!assetName || !/^[0-9a-f]{64}$/.test(expectedOldSha ?? "")) throw new Error("An asset and full --expect-sha are required");
    if (!confirmed) throw new Error(`Refusing to remove ${assetName} without --confirm-replace`);
    if (args.length > 0) throw new Error(`Unknown arguments: ${args.join(" ")}`);
    replaceStagedAsset(version, { assetName, expectedOldSha });
    break;
  }
  case "stage-package":
  case "stage-package-and-advance": {
    const [version, sourceSha, packageName, artifactDirectory, workflowSha, channel, updateSparkleRaw] = args;
    if (!updateSparkleRaw) throw new Error(usage.trim());
    const staged = stagePackage({
      artifactDirectory,
      channel,
      packageName,
      sourceSha,
      updateSparkle: updateSparkleRaw === "true",
      version,
      workflowRunId: process.env.GITHUB_RUN_ID,
      workflowSha,
    });
    if (command === "stage-package-and-advance") {
      if (staged.newlyCompleted) advanceAfterStaging(version, packageName);
      else console.log(`${packageName}: skipping automatic advance because this package was already completed`);
    }
    break;
  }
  case "record-macos-submission": {
    const [version, sourceSha, submissionId, dmgSha256, signedDmgRunId] = args;
    recordMacosSubmission(version, { dmgSha256, signedDmgRunId, sourceSha, submissionId });
    break;
  }
  case "record-macos-signed": {
    const [version, sourceSha, dmgSha256, signedDmgRunId, workflowSha, channel, updateSparkleRaw] = args;
    recordMacosSigned(version, {
      channel,
      dmgSha256,
      signedDmgRunId,
      sourceSha,
      updateSparkle: updateSparkleRaw === "true",
      workflowSha,
    });
    break;
  }
  case "validate-source": {
    const [version, sourceSha] = args;
    assertVersion(version);
    assertSha(sourceSha);
    const actualSha = run("git", ["rev-parse", "HEAD"], { capture: true }).stdout;
    if (actualSha !== sourceSha) throw new Error(`Checked-out source ${actualSha} does not match ${sourceSha}`);
    const packageVersion = JSON.parse(readFileSync("package.json", "utf8")).version;
    if (packageVersion !== version) throw new Error(`package.json is ${packageVersion}; expected ${version}`);
    const changelog = readFileSync("CHANGELOG.md", "utf8");
    if (!changelog.includes(`## ${version} -`)) throw new Error(`CHANGELOG.md has no ${version} section`);
    console.log(`Validated immutable Ghostex ${version} source ${sourceSha}.`);
    break;
  }
  case "show-state": {
    const version = args.shift();
    console.log(JSON.stringify(readJsonAsset(getRelease(version), STATE_ASSET), null, 2));
    break;
  }
  default:
    throw new Error(`Unknown command ${command}\n\n${usage.trim()}`);
}
