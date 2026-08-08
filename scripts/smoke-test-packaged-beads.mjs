#!/usr/bin/env node

import { execFile } from "node:child_process";
import { access, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";
import {
  BEADS_PACKAGE_ID,
  BEADS_SOURCE_REVISION_SHORT,
  BEADS_VERSION,
} from "./beads-release.mjs";

const execFileAsync = promisify(execFile);

function combinedOutput(error) {
  return [error?.stdout, error?.stderr, error?.message].filter(Boolean).join("\n");
}

async function run(binaryPath, args, options = {}) {
  try {
    return await execFileAsync(binaryPath, args, {
      ...options,
      encoding: "utf8",
      maxBuffer: 8 * 1024 * 1024,
    });
  } catch (error) {
    const output = combinedOutput(error);
    if (/embedded Dolt requires a CGO build|proxied-server|CGO_ENABLED=0/iu.test(output)) {
      throw new Error(
        `Packaged Beads binary lacks embedded-Dolt/CGO support. Command: bd ${args.join(" ")}\n${output}`,
      );
    }
    throw new Error(`Packaged Beads smoke command failed: bd ${args.join(" ")}\n${output}`);
  }
}

export async function smokeTestPackagedBeads(binaryValue) {
  const binaryPath = path.resolve(binaryValue);
  await access(binaryPath);

  const versionResult = await run(binaryPath, ["version"]);
  const versionOutput = `${versionResult.stdout}\n${versionResult.stderr}`.trim();
  const expectedVersion = new RegExp(
    `^bd version ${BEADS_VERSION.replaceAll(".", "\\.")} .*\\b${BEADS_SOURCE_REVISION_SHORT}\\b`,
    "mu",
  );
  if (!expectedVersion.test(versionOutput)) {
    throw new Error(
      `Packaged Beads identity mismatch: expected ${BEADS_PACKAGE_ID}, ` +
        `got ${JSON.stringify(versionOutput)}`,
    );
  }

  const smokeRoot = await mkdtemp(path.join(os.tmpdir(), "ghostex-packaged-beads-smoke-"));
  const isolatedHome = path.join(smokeRoot, "home");
  const repository = path.join(smokeRoot, "repository");
  try {
    await mkdir(isolatedHome, { recursive: true });
    await mkdir(repository, { recursive: true });
    await execFileAsync("git", ["init", "--quiet", repository]);
    const env = { ...process.env, HOME: isolatedHome };
    const initResult = await run(
      binaryPath,
      ["init", "--non-interactive", "--skip-agents", "--skip-hooks"],
      { cwd: repository, env },
    );
    const initOutput = `${initResult.stdout}\n${initResult.stderr}`;
    if (!/Backend:\s*dolt/iu.test(initOutput) || !/Mode:\s*embedded/iu.test(initOutput)) {
      throw new Error(
        "Packaged Beads init did not report the required embedded Dolt backend. " +
          `Output:\n${initOutput.trim()}`,
      );
    }

    const metadataPath = path.join(repository, ".beads", "metadata.json");
    const metadata = JSON.parse(await readFile(metadataPath, "utf8"));
    if (
      metadata.backend !== "dolt" ||
      metadata.database !== "dolt" ||
      metadata.dolt_mode !== "embedded"
    ) {
      throw new Error(
        `Packaged Beads workspace is not embedded Dolt: ${JSON.stringify(metadata)}`,
      );
    }

    const statusResult = await run(binaryPath, ["status", "--json"], { cwd: repository, env });
    const status = JSON.parse(statusResult.stdout);
    if (status?.schema_version !== 1 || typeof status?.summary !== "object") {
      throw new Error(`Packaged Beads status returned an unexpected payload: ${statusResult.stdout}`);
    }

    const createResult = await run(
      binaryPath,
      ["create", "Packaged Beads schema smoke", "--description", "Verify native database access", "--silent"],
      { cwd: repository, env },
    );
    const issueId = createResult.stdout.trim();
    if (!/^[a-z0-9_-]+$/iu.test(issueId)) {
      throw new Error(`Packaged Beads create returned an unexpected issue ID: ${createResult.stdout}`);
    }
    await run(binaryPath, ["update", issueId, "--status", "in_progress"], {
      cwd: repository,
      env,
    });
    const listResult = await run(binaryPath, ["list", "--all", "--json"], {
      cwd: repository,
      env,
    });
    const listPayload = JSON.parse(listResult.stdout);
    const issues = Array.isArray(listPayload)
      ? listPayload
      : Array.isArray(listPayload?.data)
        ? listPayload.data
        : listPayload?.issues;
    const createdIssue = Array.isArray(issues)
      ? issues.find((issue) => issue?.id === issueId)
      : undefined;
    if (createdIssue?.status !== "in_progress") {
      throw new Error(`Packaged Beads list could not read the updated issue: ${listResult.stdout}`);
    }
  } finally {
    await rm(smokeRoot, { force: true, recursive: true });
  }

  console.log(`Packaged Beads ${BEADS_PACKAGE_ID} embedded-Dolt smoke test passed: ${binaryPath}`);
}

async function main() {
  const binaryPath = process.argv[2];
  if (!binaryPath) {
    throw new Error("Usage: node scripts/smoke-test-packaged-beads.mjs <packaged-bd-path>");
  }
  await smokeTestPackagedBeads(binaryPath);
}

if (import.meta.url === pathToFileURL(process.argv[1] || "").href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
