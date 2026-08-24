#!/usr/bin/env node
/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
 * The planner CLI. It gathers the three inputs `computePlan()` cannot gather
 * itself — the provenance of recent releases, an optional nominated source run,
 * and the live component tag state — and then calls the same pure planner the
 * unit tests use.
 *
 * The same code path serves the operator's local `--dry-run` preview and the
 * authoritative plan the `prepare` job publishes, so the preview cannot drift
 * from reality. Nothing here decides anything: every build/reuse/skip decision
 * lives in plan.mjs, and every reuse check lives in provenance.mjs.
 *
 * Network access is confined to `gh`. `--offline` skips it entirely, which
 * yields an empty reuse index and therefore a full-build plan: degrading toward
 * building is always safe, degrading toward reuse never is.
 */

import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  COMPONENT_IDS,
  PRODUCT_IDS,
  TRUSTED_REPO,
  defaultScope,
  productDefinition,
} from "./product-inputs.mjs";
import {
  FINGERPRINT_ALGORITHM_REVISION,
  computeFingerprints,
  createGitTreeReader,
  shortFingerprint,
} from "./fingerprint.mjs";
import { releaseProvenanceAssetName, validateReleaseProvenance } from "./provenance.mjs";
import { DEFAULT_BASELINE_COUNT, computePlan, planSummaryLine, renderPlanText, scopeFromEnv } from "./plan.mjs";
import { withRetryProfile } from "./retry.mjs";

const versionPattern = /^\d+\.\d+\.\d+$/u;
const releaseTagPattern = /^v(\d+\.\d+\.\d+)$/u;

export function planCliUsage() {
  return `
Usage:
  node tooling/release-gpui/plan-cli.mjs <version> [options]

Options:
  --scope-json <json|@file>   Release scope flags; defaults to the GHOSTEX_RELEASE_* env, else everything
  --force-all                 Rebuild every in-scope product (no reuse at all)
  --force <a,b>               Rebuild these products even when their inputs are unchanged
  --reuse-from-run <id>       Also consider the product artifacts of this Release Ghostex run (Tier 2)
  --baseline-count <n>        How many recent releases to inspect for provenance (default ${DEFAULT_BASELINE_COUNT})
  --format <text|json>        Output format (default text)
  --output <file>             Also write the plan JSON to this file
  --emit-github-output        Append plan/expected_platforms/summary to $GITHUB_OUTPUT
  --emit-step-summary         Append the plan table to $GITHUB_STEP_SUMMARY
  --repo <owner/name>         Release repository (default ${TRUSTED_REPO})
  --repo-root <dir>           Git checkout to plan from (default the current directory)
  --source-sha <sha>          Source commit to plan at (default HEAD)
  --cef-version <v>           Override the resolved cef component version
  --code-server-version <v>   Override the resolved code-server component version
  --offline                   Never call gh; produces a full-build plan
`.trim();
}

export function parsePlanCliArgs(argv) {
  const options = {
    baselineCount: DEFAULT_BASELINE_COUNT,
    cefComponentVersion: null,
    codeServerComponentVersion: null,
    emitGithubOutput: false,
    emitStepSummary: false,
    forceAll: false,
    forcedProducts: [],
    format: "text",
    offline: false,
    output: null,
    repo: TRUSTED_REPO,
    repoRoot: process.cwd(),
    reuseFromRunId: null,
    scopeJson: null,
    sourceSha: null,
    version: null,
  };
  const takeValue = (argv_, index, name) => {
    const value = argv_[index + 1];
    if (value === undefined || value.startsWith("--")) throw new Error(`${name} requires a value`);
    return value;
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help" || argument === "-h") return { ...options, help: true };
    if (!argument.startsWith("--")) {
      if (options.version !== null) throw new Error(`Unexpected argument: ${argument}`);
      options.version = argument;
      continue;
    }
    switch (argument) {
      case "--force-all":
        options.forceAll = true;
        break;
      case "--offline":
        options.offline = true;
        break;
      case "--emit-github-output":
        options.emitGithubOutput = true;
        break;
      case "--emit-step-summary":
        options.emitStepSummary = true;
        break;
      case "--version":
        options.version = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--force":
        options.forcedProducts = takeValue(argv, index, argument)
          .split(",")
          .map((entry) => entry.trim())
          .filter(Boolean);
        index += 1;
        break;
      case "--scope-json":
        options.scopeJson = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--reuse-from-run":
        options.reuseFromRunId = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--baseline-count":
        options.baselineCount = Number(takeValue(argv, index, argument));
        index += 1;
        break;
      case "--format":
        options.format = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--output":
        options.output = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--repo":
        options.repo = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--repo-root":
        options.repoRoot = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--source-sha":
        options.sourceSha = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--cef-version":
        options.cefComponentVersion = takeValue(argv, index, argument);
        index += 1;
        break;
      case "--code-server-version":
        options.codeServerComponentVersion = takeValue(argv, index, argument);
        index += 1;
        break;
      default:
        throw new Error(`Unknown option: ${argument}`);
    }
  }
  if (!["text", "json"].includes(options.format)) throw new Error("--format must be text or json");
  if (!Number.isInteger(options.baselineCount) || options.baselineCount < 1) {
    throw new Error("--baseline-count must be a positive integer");
  }
  if (options.reuseFromRunId !== null && !/^\d+$/u.test(options.reuseFromRunId)) {
    throw new Error("--reuse-from-run must be a GitHub Actions run id");
  }
  for (const productId of options.forcedProducts) productDefinition(productId);
  if (options.forceAll && options.forcedProducts.length > 0) {
    throw new Error("--force-all already rebuilds everything; drop --force");
  }
  return options;
}

/*
 * Scope precedence: an explicit `--scope-json` (what the dispatcher passes),
 * then the GHOSTEX_RELEASE_* environment the workflow sets, then "everything".
 * Falling back to `scopeFromEnv` when no flag is set would silently produce an
 * empty scope for a local preview, so the presence check matters.
 */
export function resolvePlanScope({ env = process.env, scopeJson = null } = {}) {
  if (scopeJson) {
    const text = scopeJson.startsWith("@") ? readFileSync(scopeJson.slice(1), "utf8") : scopeJson;
    return defaultScope(JSON.parse(text));
  }
  const hasEnvScope = Object.keys(env).some((key) => key.startsWith("GHOSTEX_RELEASE_") && env[key] !== "");
  return hasEnvScope ? scopeFromEnv(env) : defaultScope();
}

function ghCapture(args, { allowFailure = false, maxBuffer = 96 * 1024 * 1024 } = {}) {
  const result = spawnSync("gh", args, { encoding: "utf8", maxBuffer });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    if (allowFailure) return { ok: false, stderr: result.stderr ?? "", stdout: result.stdout ?? "" };
    throw new Error(`gh ${args.join(" ")} failed: ${(result.stderr || result.stdout || "").trim()}`);
  }
  return { ok: true, stderr: result.stderr ?? "", stdout: result.stdout ?? "" };
}

function ghJson(args, options) {
  return withRetryProfile(async () => JSON.parse(ghCapture(args, options).stdout), "github", {
    label: `gh ${args.slice(0, 2).join(" ")}`,
  });
}

function notice(message) {
  process.stderr.write(`${message}\n`);
}

/*
 * Tier 1 of the reuse index: the `release-provenance-<version>.json` asset of
 * recent non-draft releases. A release without that asset (everything published
 * before this feature) is simply not a reuse candidate.
 */
export async function collectReleaseBaselines({ count, repo, resolveTagCommit }) {
  const perPage = Math.min(100, Math.max(count * 2, count));
  const releases = await ghJson(["api", `repos/${repo}/releases?per_page=${perPage}`]);
  const baselines = [];
  for (const release of releases) {
    if (baselines.length >= count) break;
    if (release.draft) continue;
    const match = releaseTagPattern.exec(release.tag_name ?? "");
    if (!match) continue;
    const assetName = releaseProvenanceAssetName(match[1]);
    const asset = (release.assets ?? []).find((candidate) => candidate.name === assetName);
    if (!asset) continue;
    const download = ghCapture(
      ["api", `repos/${repo}/releases/assets/${asset.id}`, "-H", "Accept: application/octet-stream"],
      { allowFailure: true },
    );
    if (!download.ok) {
      notice(`::warning::Skipping ${release.tag_name}: ${assetName} could not be downloaded`);
      continue;
    }
    let provenance;
    try {
      provenance = validateReleaseProvenance(JSON.parse(download.stdout));
    } catch (error) {
      notice(`::warning::Skipping ${release.tag_name}: ${error instanceof Error ? error.message : String(error)}`);
      continue;
    }
    baselines.push({
      assets: (release.assets ?? []).map((entry) => ({
        digest: entry.digest ?? null,
        name: entry.name,
        size: entry.size,
      })),
      commit: resolveTagCommit?.(release.tag_name) ?? provenance.sourceSha,
      draft: false,
      provenance,
      publishedAt: release.published_at ?? null,
      repo,
      tag: release.tag_name,
    });
  }
  return baselines;
}

/*
 * Tier 2: a nominated Release Ghostex run whose product artifacts are still
 * alive. The per-product provenance records travel in their own tiny
 * `release-provenance-<product>` artifacts precisely so planning never has to
 * download multi-gigabyte packages to learn a fingerprint.
 */
export async function collectSourceRun({ repo, runId }) {
  const run = await ghJson([
    "run",
    "view",
    String(runId),
    "--repo",
    repo,
    "--json",
    "conclusion,event,headSha,status,url,workflowName",
  ]);
  const listing = await ghJson(["api", `repos/${repo}/actions/runs/${runId}/artifacts?per_page=100`]);
  const artifacts = listing.artifacts ?? [];
  const expiredArtifacts = artifacts.filter((artifact) => artifact.expired).map((artifact) => artifact.name);
  const available = new Set(artifacts.filter((artifact) => !artifact.expired).map((artifact) => artifact.name));

  const products = {};
  const scratch = mkdtempSync(path.join(tmpdir(), "ghostex-plan-run-"));
  try {
    for (const productId of PRODUCT_IDS) {
      const artifactName = `release-provenance-${productId}`;
      if (!available.has(artifactName)) continue;
      const destination = path.join(scratch, productId);
      const download = ghCapture(
        ["run", "download", String(runId), "--repo", repo, "--name", artifactName, "--dir", destination],
        { allowFailure: true },
      );
      if (!download.ok) {
        notice(`::warning::${artifactName} could not be downloaded from run ${runId}`);
        continue;
      }
      const recordPath = path.join(destination, "provenance.json");
      if (!existsSync(recordPath)) continue;
      products[productId] = JSON.parse(readFileSync(recordPath, "utf8"));
    }
  } finally {
    rmSync(scratch, { force: true, recursive: true });
  }

  if (run.conclusion && run.conclusion !== "success") {
    const survivors = Object.keys(products);
    notice(
      `::notice::Run ${runId} concluded ${run.conclusion}; its ${survivors.length} product(s) with ` +
        `surviving artifacts (${survivors.join(", ") || "none"}) remain reuse candidates`,
    );
  }

  return {
    availableArtifacts: [...available],
    conclusion: run.conclusion ?? null,
    event: run.event ?? null,
    expiredArtifacts,
    headSha: run.headSha ?? null,
    products,
    repo,
    runId: Number(runId),
    status: run.status ?? null,
    url: run.url ?? null,
    workflowName: run.workflowName ?? null,
  };
}

/*
 * Component identities are not fingerprints (§4.2, §4.3), so they are resolved
 * from the authoritative source when it is available and otherwise inferred:
 * a component whose fingerprint node still matches a baseline's recorded
 * composition must still carry that baseline's component version.
 */
export function resolveComponentIdentities({ baselines, entries, overrides = {}, readObject, scope, version }) {
  const fingerprints = computeFingerprints({ context: { scope, version }, entries, ids: [...COMPONENT_IDS], readObject });
  const identities = {};
  for (const component of COMPONENT_IDS) {
    if (overrides[component]) {
      identities[component] = overrides[component];
      continue;
    }
    const current = fingerprints.get(component)?.fingerprint;
    for (const baseline of baselines) {
      const provenance = baseline.provenance;
      if (!provenance || provenance.algorithmRevision !== FINGERPRINT_ALGORITHM_REVISION) continue;
      const recordedVersion = provenance.components?.[component]?.componentVersion;
      if (!recordedVersion) continue;
      const matches = Object.values(provenance.products ?? {}).some(
        (record) => record?.inputs?.composed?.[component] === current,
      );
      if (matches) {
        identities[component] = recordedVersion;
        break;
      }
    }
  }
  return identities;
}

/* The published platform assets of `<component>-<componentVersion>`. */
export async function collectComponentTagState({ baselines, identities, repo }) {
  const state = {};
  for (const component of COMPONENT_IDS) {
    const componentVersion = identities[component];
    if (!componentVersion) {
      state[component] = { componentVersion: null, platforms: {} };
      continue;
    }
    const tag = `${component}-${componentVersion}`;
    const view = ghCapture(["release", "view", tag, "--repo", repo, "--json", "assets"], { allowFailure: true });
    const assets = view.ok ? JSON.parse(view.stdout).assets ?? [] : [];
    const platforms = {};
    const prefix = `${tag}-`;
    for (const asset of assets) {
      if (!asset.name.startsWith(prefix) || !asset.name.endsWith(".tar.gz")) continue;
      const platform = asset.name.slice(prefix.length, -".tar.gz".length);
      if (!platform || platform.includes(".")) continue;
      platforms[platform] = {
        assetName: asset.name,
        sha256: typeof asset.digest === "string" ? asset.digest.replace(/^sha256:/u, "") : null,
        sizeBytes: asset.size ?? null,
      };
    }
    const recorded = baselines
      .map((baseline) => baseline.provenance?.components?.[component])
      .find((entry) => entry?.componentVersion === componentVersion);
    state[component] = {
      componentVersion,
      identityRevisionInputsDigest: recorded?.identityRevisionInputsDigest ?? null,
      platforms,
    };
  }
  return state;
}

/*
 * code-server's identity is authoritative only when the submodule is checked
 * out, which is exactly the state the `prepare` job arranges.
 */
export function resolveCodeServerIdentity({ repoRoot }) {
  const root = path.join(repoRoot, ".dependencies/code-server");
  if (!existsSync(path.join(root, "package.json"))) return null;
  const result = spawnSync(
    "node",
    [path.join(repoRoot, "tooling/release-gpui/code-server-component-identity.mjs"), "--root", root],
    { cwd: repoRoot, encoding: "utf8" },
  );
  if (result.status !== 0) return null;
  const identity = result.stdout.trim();
  return /^[0-9a-f]{12}-[A-Za-z0-9]+-[0-9a-f]{64}$/u.test(identity) ? identity : null;
}

export function renderPlanMarkdown(plan) {
  const lines = [];
  lines.push(`## Release plan — ${plan.version} (${plan.algorithmRevision})`);
  lines.push("");
  lines.push(`- Source \`${plan.sourceSha}\``);
  lines.push(`- Mode ${plan.forceAll ? "**force-all**" : "change-aware"}${plan.forcedProducts.length > 0 ? ` (forced: ${plan.forcedProducts.join(", ")})` : ""}`);
  lines.push(
    `- Baselines ${plan.baselineTags.length > 0 ? plan.baselineTags.join(", ") : "(none)"} — ` +
      `${plan.baselinesInspected} inspected, ${plan.baselinesWithProvenance} with provenance`,
  );
  lines.push(`- ${planSummaryLine(plan)}; ~${plan.estimates.savedRunnerMinutes} runner-minutes saved`);
  lines.push("");
  lines.push("| Product | Action | Fingerprint | Reason |");
  lines.push("|---|---|---|---|");
  for (const productId of PRODUCT_IDS) {
    const entry = plan.products[productId];
    lines.push(
      `| \`${productId}\` | ${entry.action.toUpperCase()} | \`${shortFingerprint(entry.fingerprint)}\` | ${entry.reason} |`,
    );
  }
  lines.push("");
  lines.push("| Component | Version | Action | Reason |");
  lines.push("|---|---|---|---|");
  for (const component of COMPONENT_IDS) {
    const entry = plan.components[component];
    lines.push(
      `| \`${component}\` | \`${entry.componentVersion ?? "unknown"}\` | ${entry.action.toUpperCase()} | ${entry.reason} |`,
    );
  }
  lines.push("");
  const rejected = PRODUCT_IDS.flatMap((productId) =>
    (plan.products[productId].rejectedReuse ?? []).map(
      (entry) => `- \`${productId}\` rejected ${entry.origin}: ${entry.reasons.join("; ")}`,
    ),
  );
  if (rejected.length > 0) {
    lines.push("<details><summary>Rejected reuse candidates</summary>", "");
    lines.push(...rejected);
    lines.push("", "</details>", "");
  }
  lines.push(
    `Feeds: sparkle=${plan.feeds.sparkle ? "update" : "hold"}, homebrew=${plan.feeds.homebrew ? "update" : "hold"}, ` +
      `windows-feeds=${plan.feeds.windowsFeeds.join(",") || "none"}`,
  );
  return lines.join("\n");
}

/*
 * What the jobs actually need from the plan.
 *
 * The full document is uploaded as the `release-plan` artifact and is what the
 * operator, the publisher's recovery path, and the next planner read. The copy
 * threaded through `workflow_call` inputs to eight jobs is different: it is
 * re-serialized into every job's environment, so its size is paid nine times.
 * `rejectedReuse` is the whole diagnostic history of the planner's decisions —
 * up to twelve baselines × ten products of prose — and no job consumer reads it.
 *
 * `inputs` deliberately stays: `write-provenance.mjs` copies each product's
 * per-pathspec digests straight into the provenance record it emits, so
 * stripping them would break every producing job.
 */
export function compactPlanForThreading(plan) {
  const products = {};
  for (const [productId, entry] of Object.entries(plan.products ?? {})) {
    const { rejectedReuse, ...rest } = entry;
    products[productId] = rest;
  }
  return { ...plan, products };
}

/* Workflow inputs are capped well below this; a warning beats a mid-release failure. */
export const THREADED_PLAN_WARN_BYTES = 128 * 1024;

/*
 * The parent workflow reads the plan twice: once as the whole document (passed
 * down to the jobs that write provenance) and once as flat per-job scalars.
 * The scalars exist so `if:` conditions stay readable and so GitHub does not
 * re-parse a large JSON string in twenty separate expressions.
 */
export function planGithubOutputs(plan) {
  const threaded = JSON.stringify(compactPlanForThreading(plan));
  if (threaded.length > THREADED_PLAN_WARN_BYTES) {
    notice(
      `::warning::The resolved plan threaded to the release jobs is ${threaded.length} bytes, ` +
        `above the ${THREADED_PLAN_WARN_BYTES}-byte review threshold`,
    );
  }
  return {
    expected_platforms: plan.expectedPlatforms.join(","),
    feeds_sparkle: String(Boolean(plan.feeds.sparkle)),
    feeds_windows: plan.feeds.windowsFeeds.join(","),
    job_android: plan.jobs.android,
    job_code_server_arm64: plan.jobs.code_server_arm64,
    job_code_server_x64: plan.jobs.code_server_x64,
    job_gxserver_arm64: plan.jobs.gxserver_arm64,
    job_gxserver_x64: plan.jobs.gxserver_x64,
    job_linux_x64: plan.jobs.linux_x64,
    job_macos: plan.jobs.macos,
    job_validate_windows: String(Boolean(plan.jobs.validate_windows)),
    job_windows_arm64: plan.jobs.windows_arm64,
    job_windows_x64: plan.jobs.windows_x64,
    job_wsl_arm64: plan.jobs.wsl_arm64,
    job_wsl_x64: plan.jobs.wsl_x64,
    linux_packages: plan.jobs.linux_packages.join(","),
    plan: threaded,
    reuse_count: String(plan.jobs.reuse_matrix.length),
    reuse_matrix: JSON.stringify(plan.jobs.reuse_matrix),
    summary: planSummaryLine(plan),
  };
}

function appendOutputs(file, outputs) {
  if (!file) return;
  const lines = Object.entries(outputs).map(([key, value]) => {
    if (String(value).includes("\n")) throw new Error(`Plan output ${key} must be single-line`);
    return `${key}=${value}`;
  });
  appendFileSync(file, `${lines.join("\n")}\n`);
}

export async function buildPlanFromRepository(options) {
  const reader = createGitTreeReader({ repoRoot: options.repoRoot });
  const sourceSha = options.sourceSha ?? reader.resolve("HEAD");
  const version =
    options.version ?? JSON.parse(readFileSync(path.join(options.repoRoot, "package.json"), "utf8")).version;
  if (!versionPattern.test(version ?? "")) throw new Error("Pass a MAJOR.MINOR.PATCH release version");
  const scope = resolvePlanScope({ scopeJson: options.scopeJson });
  const entries = reader.listTree(sourceSha);

  let baselines = [];
  let sourceRun = null;
  let componentTagState = {};
  const identityOverrides = {
    cef: options.cefComponentVersion,
    "code-server": options.codeServerComponentVersion ?? resolveCodeServerIdentity({ repoRoot: options.repoRoot }),
  };

  if (options.offline) {
    notice("::notice::Planning offline: no reuse candidates will be considered");
  } else {
    baselines = await collectReleaseBaselines({
      count: options.baselineCount,
      repo: options.repo,
      resolveTagCommit: (tag) => {
        const result = spawnSync("git", ["-C", options.repoRoot, "rev-list", "-n", "1", tag], { encoding: "utf8" });
        return result.status === 0 ? result.stdout.trim() || null : null;
      },
    });
    if (options.reuseFromRunId) {
      sourceRun = await collectSourceRun({ repo: options.repo, runId: options.reuseFromRunId });
    }
  }

  const componentIdentities = resolveComponentIdentities({
    baselines,
    entries,
    overrides: Object.fromEntries(Object.entries(identityOverrides).filter(([, value]) => Boolean(value))),
    readObject: reader.readObject,
    scope,
    version,
  });
  if (!options.offline) {
    componentTagState = await collectComponentTagState({
      baselines,
      identities: componentIdentities,
      repo: options.repo,
    });
  }

  return computePlan({
    assetMetadata: ({ candidate, name }) =>
      candidate.tier === "release"
        ? (candidate.assets ?? []).find((asset) => asset.name === name) ?? null
        : null,
    baselineCount: options.baselineCount,
    baselines,
    componentIdentities,
    componentTagState,
    entries,
    forceAll: options.forceAll,
    forcedProducts: options.forcedProducts,
    isAncestor: (commit) => reader.isAncestor(commit, sourceSha),
    readObject: reader.readObject,
    reuseFromRunId: options.reuseFromRunId ? Number(options.reuseFromRunId) : null,
    scope,
    sourceRun,
    sourceSha,
    version,
  });
}

async function main() {
  const options = parsePlanCliArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(`${planCliUsage()}\n`);
    return;
  }
  const plan = await buildPlanFromRepository(options);
  if (options.format === "json") process.stdout.write(`${JSON.stringify(plan, null, 2)}\n`);
  else process.stdout.write(`${renderPlanText(plan)}\n`);
  if (options.output) writeFileSync(options.output, `${JSON.stringify(plan, null, 2)}\n`);
  if (options.emitGithubOutput) appendOutputs(process.env.GITHUB_OUTPUT, planGithubOutputs(plan));
  if (options.emitStepSummary && process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${renderPlanMarkdown(plan)}\n`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
