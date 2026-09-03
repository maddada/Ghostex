#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import {
  extractChangelogSectionFromText,
  releaseBuildVersion,
  validateMajorMinorReleaseNotes,
} from './release-ghostex.mjs';
import { RELEASE_BUILD_SCRIPTS, runReleaseBuildScripts } from './release-preflight-build-scripts.mjs';
import { formatToolingImportViolation, scanToolingImports } from './release-preflight-tooling-imports.mjs';
import {
  SPARKLE_KEY_SOURCE,
  evaluatePreflightLiteralProbes,
  evaluateWorkflowAssertions,
  extractSparklePublicKey,
  formatRegressions,
  formatStale,
} from './release-workflow-assertions.mjs';

/*
 CDXC:Release 2026-07-02-14:10:
 The 5.4.0 release spent minutes on a root `bun run test` that discovers
 bundled code-server trees, and it discovered late source edits only after
 expensive package builds had started. This remains an optional deep local
 audit for historical/local flows. The canonical Actions release runs these
 gates once in its remote prepare job.
*/

const repoRoot = path.resolve(new URL('..', import.meta.url).pathname);
const githubRepo = 'maddada/Ghostex';
const signingIdentity = 'Developer ID Application: Mohamad Youssef (KTKP595G3B)';
const subrepoCandidates = ['apps/mobile/app', '.dependencies/zmx'];

/*
 Both the remote-linux gate check and the assertion-freshness meta-check read
 the same parsed workflows, and preflight runs its checks concurrently. Evaluate
 once and share the promise so the two never disagree about what they saw.
*/
let workflowAssertionEvaluation = null;
function evaluateWorkflowGates() {
  workflowAssertionEvaluation ??= evaluateWorkflowAssertions({ repoRoot });
  return workflowAssertionEvaluation;
}

/*
 The Sparkle EdDSA public key is owned by apps/desktop/scripts/build-macos-app.sh,
 which stamps it into Info.plist as SUPublicEDKey. Preflight used to carry its own
 copy of the base64, so rotating the key in the build script would have left this
 check failing against a value nothing ships. Read the shipped value instead, and
 make an unreadable assignment a stale-check failure rather than a key mismatch.
*/
let sparklePublicKeyPromise = null;
function expectedSparklePublicKey() {
  sparklePublicKeyPromise ??= (async () => {
    const source = path.join(repoRoot, SPARKLE_KEY_SOURCE);
    if (!existsSync(source)) {
      throw new Error(
        `STALE CHECK, not a product regression - ${SPARKLE_KEY_SOURCE} does not exist, so preflight cannot learn which Sparkle public key the app ships.`
      );
    }
    const key = extractSparklePublicKey(await readFile(source, 'utf8'));
    if (!key) {
      throw new Error(
        `STALE CHECK, not a product regression - GHOSTEX_GPUI_SPARKLE_PUBLIC_ED_KEY could not be read out of ${SPARKLE_KEY_SOURCE}. Update extractSparklePublicKey in tooling/release-workflow-assertions.mjs.`
      );
    }
    return key;
  })();
  return sparklePublicKeyPromise;
}

const highConfidenceSecretPatterns = [
  { label: 'private key block', regex: /-----BEGIN [A-Z ]*PRIVATE KEY-----/ },
  { label: 'GitHub token', regex: /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b/ },
  { label: 'GitHub fine-grained token', regex: /\bgithub_pat_[A-Za-z0-9_]{22,}\b/ },
  { label: 'AWS access key id', regex: /\bAKIA[0-9A-Z]{16}\b/ },
  { label: 'Slack token', regex: /\bxox[baprs]-[A-Za-z0-9-]{10,}\b/ },
  { label: 'Anthropic key', regex: /\bsk-ant-[A-Za-z0-9-]{20,}\b/ },
  { label: 'Google API key', regex: /\bAIza[0-9A-Za-z_-]{35}\b/ },
];

const genericSecretPattern = /(?:password|passwd|secret|api[_-]?key|auth[_-]?token)\s*[:=]\s*["'][^"']{12,}["']/i;

function usage() {
  return `
Usage:
  node tooling/release-preflight-fast.mjs <version> [options]

Options:
  --release-branch <branch>  Branch being released. Defaults to main.
  --cargo                    Also run cargo check for server and gpui.
  --skip-tests               Skip bun run release:test.
  --skip-typecheck           Skip bun run typecheck.
  --skip-build-scripts       Skip running the release JS build scripts locally.
  --skip-credentials         Skip gh/signing/notary credential probes.
  --allow-concurrent-sessions
                             Release even though another agent session is
                             actively working in this worktree.
  --freeze-seconds <n>       Freeze window length after checks pass. Default 45.
  --skip-freeze              Skip the freeze window.
  --help                     Show this help.

Exit code 0 means every optional local audit passed and the worktree stayed
stable through the freeze window. New public releases use
bun run release:actions -- <version>.
`;
}

function parseArgs(argv) {
  const options = {
    allowConcurrentSessions: false,
    cargo: false,
    freezeSeconds: 45,
    releaseBranch: 'main',
    skipBuildScripts: false,
    skipCredentials: false,
    skipFreeze: false,
    skipTests: false,
    skipTypecheck: false,
    version: null,
  };
  const positional = [];
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (arg === '--cargo') {
      options.cargo = true;
    } else if (arg === '--skip-tests') {
      options.skipTests = true;
    } else if (arg === '--skip-typecheck') {
      options.skipTypecheck = true;
    } else if (arg === '--skip-build-scripts') {
      options.skipBuildScripts = true;
    } else if (arg === '--skip-credentials') {
      options.skipCredentials = true;
    } else if (arg === '--allow-concurrent-sessions') {
      options.allowConcurrentSessions = true;
    } else if (arg === '--skip-freeze') {
      options.skipFreeze = true;
    } else if (arg === '--freeze-seconds') {
      options.freezeSeconds = Number.parseInt(argv[index + 1] ?? '', 10);
      if (!Number.isFinite(options.freezeSeconds) || options.freezeSeconds < 0) {
        throw new Error('--freeze-seconds requires a non-negative integer.');
      }
      index += 1;
    } else if (arg === '--release-branch') {
      options.releaseBranch = argv[index + 1]?.trim();
      if (!options.releaseBranch) {
        throw new Error('--release-branch requires a branch name.');
      }
      index += 1;
    } else if (arg.startsWith('-')) {
      throw new Error(`Unknown option: ${arg}`);
    } else {
      positional.push(arg);
    }
  }
  if (options.help) {
    return options;
  }
  if (positional.length !== 1 || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(positional[0] ?? '')) {
    throw new Error('Pass exactly one semver version, for example 5.5.0.');
  }
  options.version = positional[0];
  return options;
}

function runCommand(command, { timeoutMs = 60_000, cwd = repoRoot, env } = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, {
      cwd,
      env: { ...process.env, ...(env ?? {}) },
      shell: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    let settled = false;
    const timeout = setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      child.kill('SIGTERM');
      resolve({ code: 124, stdout, stderr: `${stderr}\n(timed out after ${timeoutMs}ms)` });
    }, timeoutMs);
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
    });
    child.on('error', (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolve({ code: 127, stdout, stderr: String(error.message ?? error) });
    });
    child.on('close', (code) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolve({ code: code ?? 1, stdout, stderr });
    });
  });
}

async function capture(command, options = {}) {
  const result = await runCommand(command, options);
  if (result.code !== 0) {
    throw new Error(`${command} failed (${result.code}): ${(result.stderr || result.stdout).trim()}`);
  }
  return result.stdout.trim();
}

function pass(detail = '') {
  return { detail, status: 'PASS' };
}

function warn(detail) {
  return { detail, status: 'WARN' };
}

function fail(detail) {
  return { detail, status: 'FAIL' };
}

function shortOutput(text, lines = 6) {
  return String(text ?? '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, lines)
    .join(' | ');
}

async function checkBranch(options) {
  const branch = await capture('git branch --show-current');
  if (branch !== options.releaseBranch) {
    return fail(`On ${branch || '(detached HEAD)'}; release branch is ${options.releaseBranch}.`);
  }
  return pass(branch);
}

async function checkWorktreeClean() {
  const status = await capture('git status --porcelain --untracked-files=all');
  if (status) {
    return fail(`Worktree is dirty:\n${shortOutput(status, 12)}`);
  }
  return pass('clean');
}

async function checkSyncedWithOrigin(options) {
  await capture(`git fetch origin ${options.releaseBranch} --tags`, { timeoutMs: 120_000 });
  const head = await capture('git rev-parse HEAD');
  const origin = await capture(`git rev-parse origin/${options.releaseBranch}`);
  if (head !== origin) {
    return fail(
      `HEAD ${head.slice(0, 10)} != origin/${options.releaseBranch} ${origin.slice(0, 10)}. Push release-bound commits first.`
    );
  }
  return pass(head.slice(0, 10));
}

async function checkTagMissing(options) {
  const localTag = await capture(`git tag --list 'v${options.version}'`);
  const remoteTag = await capture(`git ls-remote --tags origin 'refs/tags/v${options.version}'`, { timeoutMs: 60_000 });
  if (localTag || remoteTag) {
    return fail(`Tag v${options.version} already exists ${localTag ? 'locally' : 'on origin'}.`);
  }
  return pass('absent');
}

async function checkReleaseMissing(options) {
  const result = await runCommand(
    `env -u GH_TOKEN -u GITHUB_TOKEN gh release view 'v${options.version}' --repo '${githubRepo}'`,
    { timeoutMs: 60_000 }
  );
  if (result.code === 0) {
    return fail(`GitHub release v${options.version} already exists.`);
  }
  return pass('absent');
}

async function checkChangelog(options) {
  const changelog = await readFile(path.join(repoRoot, 'CHANGELOG.md'), 'utf8');
  const notes = extractChangelogSectionFromText(changelog, options.version);
  validateMajorMinorReleaseNotes(notes, options.version);
  return pass(
    `${notes.split(/\r?\n/).filter((line) => line.trim().startsWith('- ') || line.trim().startsWith('  - ')).length} bullets`
  );
}

async function checkSparkleBuildNumber(options) {
  const buildVersion = releaseBuildVersion(options.version);
  const xml = await readFile(path.join(repoRoot, 'appcast.xml'), 'utf8');
  let maxVersion = 0;
  let seen = 0;
  for (const match of xml.matchAll(/<sparkle:version>(\d+)<\/sparkle:version>/g)) {
    seen += 1;
    maxVersion = Math.max(maxVersion, Number.parseInt(match[1], 10));
  }
  /*
   Without this, a renamed or restructured element would leave maxVersion at 0 and
   the check would PASS for every conceivable build number - the silent-staleness
   failure mode, in its worst form, because it hides rather than over-reports.
  */
  if (seen === 0) {
    return fail(
      'STALE CHECK, not a product regression - appcast.xml contains no <sparkle:version> elements, so this check would pass for any build number. Fix the assertion, not the release.'
    );
  }
  if (buildVersion <= maxVersion) {
    return fail(`Build ${buildVersion} must exceed latest Sparkle build ${maxVersion}.`);
  }
  return pass(`${buildVersion} > ${maxVersion}`);
}

async function checkSubrepos() {
  const problems = [];
  for (const repo of subrepoCandidates) {
    const repoPath = path.join(repoRoot, repo);
    if (!existsSync(repoPath)) {
      continue;
    }
    const isRepo = await runCommand(`git -C '${repoPath}' rev-parse --git-dir`, { timeoutMs: 10_000 });
    if (isRepo.code !== 0) {
      continue;
    }
    const status = await capture(`git -C '${repoPath}' status --porcelain --untracked-files=all`);
    if (status) {
      problems.push(`${repo} is dirty: ${shortOutput(status, 3)}`);
      continue;
    }
    const unpushed = await runCommand(`git -C '${repoPath}' rev-list --count @{upstream}..HEAD`, { timeoutMs: 10_000 });
    if (unpushed.code === 0 && Number.parseInt(unpushed.stdout.trim(), 10) > 0) {
      problems.push(`${repo} has ${unpushed.stdout.trim()} unpushed commit(s)`);
    }
  }
  if (problems.length > 0) {
    return fail(problems.join('; '));
  }
  return pass('clean and pushed');
}

async function checkSecretScan() {
  const previousTag = await capture(
    "git tag --sort=-version:refname | grep -E '^v[0-9]+\\.[0-9]+\\.[0-9]+$' | head -n 1"
  );
  if (!previousTag) {
    return warn('No previous release tag found; skipped changed-file secret scan.');
  }
  const changedOutput = await capture(`git diff --name-only '${previousTag}'..HEAD`);
  const changedFiles = changedOutput.split(/\r?\n/).filter(Boolean);
  const findings = [];
  const warnings = [];
  for (const file of changedFiles) {
    const filePath = path.join(repoRoot, file);
    if (!existsSync(filePath)) {
      continue;
    }
    let content;
    try {
      content = await readFile(filePath, 'utf8');
    } catch {
      continue;
    }
    for (const pattern of highConfidenceSecretPatterns) {
      if (pattern.regex.test(content)) {
        findings.push(`${file}: ${pattern.label}`);
      }
    }
    if (genericSecretPattern.test(content)) {
      warnings.push(file);
    }
  }
  if (findings.length > 0) {
    return fail(`Possible secrets in changed files: ${findings.join('; ')}`);
  }
  if (warnings.length > 0) {
    return warn(`Generic credential-like assignments (review before release): ${warnings.slice(0, 8).join(', ')}`);
  }
  return pass(`${changedFiles.length} changed files since ${previousTag}`);
}

/*
 Reports only PRODUCT regressions. Every assertion here reads a parsed workflow
 document, so quote style, key order, and line wrapping cannot change what it
 sees. When an assertion cannot be trusted the outcome is `stale`, which is
 owned by the assertion-freshness meta-check below; this check still fails, but
 it says the gate is unverified rather than claiming the gate regressed.
*/
async function checkRemoteLinuxPackages() {
  const evaluation = await evaluateWorkflowGates();
  if (evaluation.regressions.length > 0) {
    return fail(`release gate regression: ${formatRegressions(evaluation.regressions)}`);
  }
  if (evaluation.stale.length > 0) {
    return fail(
      `${evaluation.stale.length} gate(s) UNVERIFIED because their assertions are stale, not because the product regressed: ${formatStale(evaluation.stale)}. See the assertion-freshness check.`
    );
  }
  return pass(`${evaluation.ok.length} parsed workflow gates verified across x64 and ARM64`);
}

/*
 CDXC:Release 2026-09-01-11:52:
 The meta-check. `GHOSTEX_REQUIRE_BEADS_SMOKE: "1"` became `: '1'` in a prettier
 pass and the old substring assertion started failing for a reason that did not
 exist, indistinguishable from the gate actually being gone. This check exists
 so the two can never be confused again: everything it reports means "a preflight
 assertion is out of date, fix the assertion", and it names the file whose rename
 or reformat broke it. Product regressions never appear here - they appear in
 remote-linux-packages and the individual gate checks.
*/
async function checkAssertionFreshness() {
  const [workflow, literals] = await Promise.all([
    evaluateWorkflowGates(),
    evaluatePreflightLiteralProbes({
      constants: { githubRepo, signingIdentity, sparklePublicKey: await expectedSparklePublicKey() },
      repoRoot,
    }),
  ]);
  const staleEntries = [...workflow.stale, ...literals.stale];
  if (staleEntries.length > 0) {
    return fail(
      `STALE CHECK, not a product regression - ${staleEntries.length} preflight assertion(s) no longer match the files they read. Fix the assertion in tooling/release-workflow-assertions.mjs (or the preflight constant it probes), then rerun: ${formatStale(staleEntries)}`
    );
  }
  return pass(
    `${workflow.results.length} workflow assertions and ${literals.results.length} literal probes are current`
  );
}

async function checkGhAuth() {
  const result = await runCommand('env -u GH_TOKEN -u GITHUB_TOKEN gh auth status -h github.com', {
    timeoutMs: 30_000,
  });
  if (result.code !== 0) {
    return fail(`gh auth status failed: ${shortOutput(result.stderr || result.stdout)}`);
  }
  return pass('authenticated');
}

async function checkSigningIdentity() {
  const identity = process.env.GHOSTEX_CODE_SIGN_IDENTITY?.trim() || signingIdentity;
  const result = await runCommand('security find-identity -v -p codesigning', { timeoutMs: 20_000 });
  if (result.code === 0 && result.stdout.includes(identity)) {
    return pass('visible in this shell');
  }
  return warn('Not visible locally; the canonical Actions workflow uses its isolated repository secrets.');
}

async function checkNotaryProfile() {
  const result = await runCommand('xcrun notarytool history --keychain-profile notarytool-profile | head -n 4', {
    timeoutMs: 45_000,
  });
  if (result.code === 0) {
    return pass('notarytool-profile reachable');
  }
  return warn('Notary profile not reachable locally; the canonical Actions workflow uses repository secrets.');
}

async function checkSparkleKey() {
  const findCommand = [
    'find',
    `'${path.join(repoRoot, 'build/arm64/SourcePackages/artifacts/sparkle')}'`,
    `'${path.join(repoRoot, 'build/SourcePackages/artifacts/sparkle')}'`,
    "'/tmp/ghostex-xcodebuild/SourcePackages/artifacts/sparkle'",
    "-path '*/Sparkle/bin/generate_appcast' -print -quit 2>/dev/null | xargs dirname 2>/dev/null",
  ].join(' ');
  const result = await runCommand(findCommand, { timeoutMs: 20_000 });
  const sparkleBinDir = result.stdout.trim();
  if (!sparkleBinDir) {
    return warn('Sparkle tools not found locally; the canonical Actions workflow restores its pinned tools.');
  }
  const keyResult = await runCommand(`'${path.join(sparkleBinDir, 'generate_keys')}' -p`, { timeoutMs: 20_000 });
  if (keyResult.code !== 0) {
    return warn(`generate_keys -p failed: ${shortOutput(keyResult.stderr)}`);
  }
  const shippedKey = await expectedSparklePublicKey();
  if (!keyResult.stdout.includes(shippedKey)) {
    return fail(
      `Sparkle public key does not match the SUPublicEDKey ${SPARKLE_KEY_SOURCE} ships. Do not sign appcasts with this key.`
    );
  }
  return pass('EdDSA key matches the shipped SUPublicEDKey');
}

/*
 CDXC:Release 2026-09-02-12:40:
 8.5.0's first run died in the Android job on `Cannot find package 'esbuild'`:
 tooling/build-mobile-chat.mjs imported it, nothing declared it, and it only
 resolved through hoisting luck. Two stages close that gap. This one is instant
 and structural - every bare import under tooling/ must be declared in the root
 package.json - so it runs with the fast checks.
*/
async function checkToolingImports() {
  const { files, violations } = await scanToolingImports();
  if (violations.length > 0) {
    return fail(
      `${violations.length} undeclared package import(s); declare them in package.json and run bun install: ${violations.map(formatToolingImportViolation).join('; ')}`
    );
  }
  return pass(`${files.length} tooling files import only declared packages`);
}

async function checkTypecheck() {
  const result = await runCommand('bun run typecheck', { timeoutMs: 8 * 60 * 1000 });
  if (result.code !== 0) {
    return fail(shortOutput(result.stderr || result.stdout, 10));
  }
  return pass('tsc clean');
}

/*
 The second half of the esbuild lesson: actually run every JS build script the
 release workflows execute (mobile chat page, sidebar CSS, desktop CEF bundle),
 so a script that cannot load or cannot build fails here in seconds rather than
 minutes into a runner. The list, its CI callers, and the tracked-output
 safeguards live in tooling/release-preflight-build-scripts.mjs.
*/
async function checkBuildScripts() {
  const { failure, results } = await runReleaseBuildScripts();
  if (failure) {
    return fail(`${failure.command} (${failure.caller}): ${shortOutput(failure.detail, 12)}`);
  }
  const timings = results.map(
    (result) => `${result.command.replace(/^bun(?:x)? (?:run )?/, '')} ${formatDuration(result.durationMs)}`
  );
  return pass(`${RELEASE_BUILD_SCRIPTS.length} release build scripts ran clean (${timings.join(', ')})`);
}

async function checkReleaseTests() {
  const result = await runCommand('bun run release:test', { timeoutMs: 12 * 60 * 1000 });
  if (result.code !== 0) {
    return fail(shortOutput(result.stderr || result.stdout, 12));
  }
  const summary = result.stdout.match(/Tests?\s+\d+ passed[^\n]*/)?.[0] ?? 'passed';
  return pass(summary);
}

/*
`--all-targets` is not optional here. Without it cargo skips `src/bin/*`, so
`apps/desktop/src/bin/ghostex_gpui_cef_bootstrap.rs` was never compiled locally
and a broken bootstrap binary reached a release runner unopposed in 8.3.0.

Each crate is checked from inside its own directory rather than through
`--manifest-path`. `apps/desktop` pins its own toolchain in
`apps/desktop/rust-toolchain.toml`; invoking it from the repo root resolves the
root toolchain instead and fails on dependency code that needs the pin.

This still cannot see `#[cfg(windows)]` / `#[cfg(linux)]` code on macOS. That
gap is covered inside the release run by the Windows and Linux packaging jobs,
which compile their own target natively and are their own validation. It is not
covered before dispatch: release-gpui-validate.yml no longer gates anything and
is opt-in only (CDXC:Release).
*/
async function checkCargo() {
  for (const crate of ['server', 'apps/desktop']) {
    const result = await runCommand('cargo check --all-targets', {
      cwd: path.join(repoRoot, crate),
      timeoutMs: 25 * 60 * 1000,
    });
    if (result.code !== 0) {
      return fail(`${crate}: ${shortOutput(result.stderr, 8)}`);
    }
  }
  return pass('server and gpui check clean (all targets)');
}

/*
A release takes hours of runner time and the publisher must still be able to
fast-forward `origin/main` when it lands. Another agent pushing mid-build cost
8.3.0 an entire dispatch, so a session actively working in this same worktree
is a blocking condition, not a note. Read-only inspection: this never messages,
interrupts, sleeps, or focuses another session.
*/
async function checkConcurrentSessions(options) {
  const result = await runCommand('ghostex sessions --json', { timeoutMs: 30_000 });
  if (result.code !== 0) {
    return warn('ghostex CLI unavailable; confirm no other agent is working in this worktree before dispatching.');
  }
  let sessions;
  try {
    sessions = JSON.parse(result.stdout).sessions ?? [];
  } catch {
    return warn('Could not parse ghostex sessions output; confirm concurrent work manually.');
  }
  const selfId = process.env.GHOSTEX_SESSION_ID ?? '';
  const selfRef = process.env.GHOSTEX_GLOBAL_SESSION_REF ?? '';
  const busy = sessions.filter(
    (session) =>
      session.projectPath === repoRoot &&
      session.activity === 'working' &&
      session.sessionId !== selfId &&
      session.globalRef !== selfRef
  );
  if (busy.length === 0) {
    return pass('no other agent session is working in this worktree');
  }
  const listed = busy.map((session) => `${session.sessionId} (${session.agentName ?? session.agent ?? 'agent'})`);
  const detail = `${busy.length} other session(s) working here: ${listed.join(', ')}`;
  if (options.allowConcurrentSessions) {
    return warn(`${detail}. Continuing because --allow-concurrent-sessions was passed.`);
  }
  return fail(`${detail}. Wait for them to go idle, or pass --allow-concurrent-sessions.`);
}

async function runChecks(checks) {
  return Promise.all(
    checks.map(async ({ name, fn }) => {
      const startedAt = Date.now();
      let outcome;
      try {
        outcome = await fn();
      } catch (error) {
        outcome = fail(String(error?.message ?? error));
      }
      return { durationMs: Date.now() - startedAt, name, ...outcome };
    })
  );
}

function formatDuration(durationMs) {
  const seconds = durationMs / 1000;
  if (seconds < 60) {
    return `${seconds.toFixed(1)}s`;
  }
  return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
}

function printResults(results) {
  const nameWidth = Math.max(...results.map((result) => result.name.length)) + 2;
  for (const result of results) {
    const marker = result.status === 'PASS' ? 'PASS' : result.status === 'WARN' ? 'WARN' : 'FAIL';
    console.log(
      `${marker}  ${result.name.padEnd(nameWidth)} ${formatDuration(result.durationMs).padStart(8)}  ${result.detail}`
    );
  }
}

async function freezeWindow(options) {
  const headBefore = await capture('git rev-parse HEAD');
  console.log(`\n==> Freeze window: verifying the worktree stays clean for ${options.freezeSeconds}s...`);
  await new Promise((resolve) => setTimeout(resolve, options.freezeSeconds * 1000));
  const status = await capture('git status --porcelain --untracked-files=all');
  const headAfter = await capture('git rev-parse HEAD');
  if (status) {
    throw new Error(
      `Worktree changed during the freeze window:\n${shortOutput(status, 12)}\nInspect, commit, push, and rerun preflight.`
    );
  }
  if (headBefore !== headAfter) {
    throw new Error(
      `HEAD moved during the freeze window (${headBefore.slice(0, 10)} -> ${headAfter.slice(0, 10)}). Rerun preflight.`
    );
  }
  console.log('Freeze window passed: worktree stable.');
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(usage().trim());
    return;
  }
  process.chdir(repoRoot);
  const startedAt = Date.now();
  console.log(`Ghostex fast release preflight for ${options.version} (build ${releaseBuildVersion(options.version)})`);

  const fastChecks = [
    { fn: () => checkBranch(options), name: 'branch' },
    { fn: () => checkWorktreeClean(), name: 'worktree-clean' },
    { fn: () => checkToolingImports(), name: 'tooling-imports' },
    { fn: () => checkSyncedWithOrigin(options), name: 'synced-with-origin' },
    { fn: () => checkTagMissing(options), name: 'tag-missing' },
    { fn: () => checkReleaseMissing(options), name: 'github-release-missing' },
    { fn: () => checkChangelog(options), name: 'changelog-major-minor' },
    { fn: () => checkSparkleBuildNumber(options), name: 'sparkle-build-number' },
    { fn: () => checkSubrepos(), name: 'subrepos-clean' },
    { fn: () => checkConcurrentSessions(options), name: 'concurrent-sessions' },
    { fn: () => checkSecretScan(), name: 'secret-scan' },
    { fn: () => checkRemoteLinuxPackages(), name: 'remote-linux-packages' },
    { fn: () => checkAssertionFreshness(), name: 'assertion-freshness' },
    { fn: () => checkSparkleKey(), name: 'sparkle-key' },
  ];
  if (!options.skipCredentials) {
    fastChecks.push(
      { fn: () => checkGhAuth(), name: 'gh-auth' },
      { fn: () => checkSigningIdentity(), name: 'signing-identity' },
      { fn: () => checkNotaryProfile(), name: 'notary-profile' }
    );
  }

  const heavyChecks = [];
  if (!options.skipTypecheck) {
    heavyChecks.push({ fn: () => checkTypecheck(), name: 'typecheck' });
  }
  if (!options.skipBuildScripts) {
    heavyChecks.push({ fn: () => checkBuildScripts(), name: 'build-scripts' });
  }
  if (!options.skipTests) {
    heavyChecks.push({ fn: () => checkReleaseTests(), name: 'release-tests' });
  }
  if (options.cargo) {
    heavyChecks.push({ fn: () => checkCargo(), name: 'cargo-check' });
  }

  const [fastResults, heavyResults] = await Promise.all([runChecks(fastChecks), runChecks(heavyChecks)]);
  const results = [...fastResults, ...heavyResults];
  console.log('');
  printResults(results);

  const failed = results.filter((result) => result.status === 'FAIL');
  if (failed.length > 0) {
    console.error(
      `\nPreflight FAILED: ${failed.map((result) => result.name).join(', ')} (${formatDuration(Date.now() - startedAt)} total)`
    );
    process.exitCode = 1;
    return;
  }

  if (!options.skipFreeze && options.freezeSeconds > 0) {
    await freezeWindow(options);
  }

  const warned = results.filter((result) => result.status === 'WARN');
  console.log(
    `\nPreflight PASSED in ${formatDuration(Date.now() - startedAt)}${warned.length > 0 ? ` with ${warned.length} warning(s)` : ''}. Canonical dispatch: bun run release:actions -- ${options.version}.`
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error('');
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}

export { parseArgs as parsePreflightArgs };
