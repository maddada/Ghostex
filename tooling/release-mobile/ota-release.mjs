#!/usr/bin/env node
/**
 * OTA-only release of the phone app: main's JavaScript, signed and published to ota.ghostex.dev
 * for the apps phones already have. No desktop builds, no Sparkle appcast, no tag, no GitHub release.
 *
 *   bun run release:ota [--platform android|ios] [--dry-run] [--force] [--skip-local-tests]
 *       (= ota-release.mjs start) Checks the checkout like release:actions (gh auth, main, clean,
 *       HEAD == origin/main), shows what would publish and dispatches release-mobile-ota.yml.
 *       --dry-run prints the plan and the dispatch instead. --force publishes even when nothing
 *       changed in the phone app since the last update. A dry run reports the source checks it
 *       would refuse on instead of stopping, since it dispatches nothing.
 *   ota-release.mjs check [--platform both|android|ios] [--expected-sha <sha>] [--plan-out <file>]
 *       The workflow's gate: refuses when a platform's runtime version has no installed app.
 *   ota-release.mjs verify-export <bundle-dir> --plan <file>
 *       The workflow's check that ota-export.sh exported exactly what the gate approved.
 *
 * Rules and their reasons: ota-release-lib.mjs (CDXC:Release 2026-10-28 DECISION).
 */

import { execFileSync } from 'node:child_process';
import { appendFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import {
  MOBILE_APP,
  REPO_ROOT,
  bundledMainChanges,
  changesSince,
  checkoutRuntimes,
  fetchServed,
  git,
  mobileCommitAt,
  nativeChanges,
  otaServer,
  parsePlatforms,
  planOtaRelease,
  runtimeInputFiles,
  verifyExport,
} from './ota-release-lib.mjs';

const REPO = 'maddada/Ghostex';
const WORKFLOW = 'release-mobile-ota.yml';
const SECRETS = ['GHOSTEX_OTA_SIGNING_KEY', 'GHOSTEX_OTA_UPLOAD_TOKEN', 'GHOSTEX_MOBILE_DEPLOY_KEY'];

function fail(message) {
  console.error(`release:ota: ${message}`);
  if (process.env.GITHUB_ACTIONS === 'true') console.log(`::error::${message.split('\n')[0]}`);
  process.exit(1);
}

function run(command, args, { capture = true } = {}) {
  const output = execFileSync(command, args, { cwd: REPO_ROOT, encoding: 'utf8', stdio: capture ? 'pipe' : 'inherit' });
  return typeof output === 'string' ? output.trim() : '';
}

function parseArgs(argv) {
  const [command, ...rest] = argv;
  const options = { platform: 'both', dryRun: false, force: false, skipLocalTests: false, positional: [] };
  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index];
    if (arg === '--dry-run') options.dryRun = true;
    else if (arg === '--force') options.force = true;
    else if (arg === '--skip-local-tests') options.skipLocalTests = true;
    else if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg.startsWith('--')) {
      const value = rest[index + 1];
      if (value === undefined) fail(`${arg} needs a value`);
      options[arg.slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = value;
      index += 1;
    } else options.positional.push(arg);
  }
  return { command, options };
}

function latestRelease() {
  const tag = run('gh', ['release', 'view', '--repo', REPO, '--json', 'tagName', '--jq', '.tagName']);
  return tag.replace(/^v/, '');
}

async function resolvePlan(platforms) {
  const runtimes = checkoutRuntimes(platforms);
  const served = {};
  for (const platform of platforms) served[platform] = await fetchServed({ platform, runtime: runtimes[platform] });
  return { plan: planOtaRelease({ platforms, runtimes, served, latestRelease: latestRelease() }), served };
}

function describeServed(record) {
  if (record.kind === 'none') return 'nothing (no installed app runs it)';
  if (record.kind === 'rollBackToEmbedded') return 'a rollback to the code inside the app';
  return `${record.version ?? 'unknown version'} from ${record.createdAt}, main ${record.gitSha?.slice(0, 12) ?? 'unknown'}`;
}

/** The change list, best effort: a shallow CI checkout may not hold the base commits. */
function tryChanges(base) {
  try {
    return changesSince(base);
  } catch (error) {
    return { error: String(error.message ?? error).split('\n')[0] };
  }
}

function printPlan(plan, served) {
  const phone = git(['log', '-1', '--format=%h %s'], { cwd: path.join(REPO_ROOT, MOBILE_APP) });
  console.log(`Server:        ${otaServer()} (production channel)`);
  console.log(`Main commit:   ${git(['rev-parse', 'HEAD'])}`);
  console.log(`Phone commit:  ${phone}`);
  console.log(`Version:       ${plan.version} (the newest GitHub release; no new tag or release)`);
  console.log(`Platforms:     ${plan.platforms.join(', ')}`);
  for (const platform of plan.platforms) {
    console.log(
      `  ${platform.padEnd(8)} runtime ${plan.runtimes[platform]}, served now: ${describeServed(served[platform])}`
    );
  }
  for (const warning of plan.warnings) console.log(`Warning: ${warning}`);
}

function printChanges(changes, label) {
  if (changes.error) {
    console.log(`Changes since ${label}: unavailable in this checkout (${changes.error})`);
    return;
  }
  const phoneFiles = changes.files.filter((file) => file.startsWith(`${MOBILE_APP}/`));
  const bundled = bundledMainChanges(changes.files);
  console.log(`Changes in ${MOBILE_APP} since ${label} (${changes.from.slice(0, 12)}..${changes.to.slice(0, 12)}):`);
  if (changes.commits.length === 0) console.log('  (no phone commits)');
  for (const commit of changes.commits) console.log(`  ${commit}`);
  // The chat core is many files; name a few.
  const named =
    bundled.length > 3 ? `${bundled.slice(0, 3).join(', ')} and ${bundled.length - 3} more` : bundled.join(', ');
  console.log(`  ${phoneFiles.length} phone files changed${bundled.length ? `, plus ${named}` : ''}`);
  return phoneFiles.length + bundled.length;
}

function refuse(plan) {
  const lines = ['refused, nothing was published:', ...plan.refusals.map((refusal) => `  ${refusal}`)];
  const tag = `v${plan.version}`;
  const changes = /^\d+\.\d+\.\d+$/.test(plan.version) ? tryChanges(tag) : { error: 'no release tag' };
  if (!changes.error) {
    const native = nativeChanges(changes.files, runtimeInputFiles(plan.platforms));
    if (native.length > 0) {
      lines.push(`  Native inputs changed since ${tag}:`, ...native.slice(0, 40).map((file) => `    ${file}`));
      if (native.length > 40) lines.push(`    ... and ${native.length - 40} more`);
    }
  }
  fail(lines.join('\n'));
}

async function check(options) {
  const platforms = parsePlatforms(options.platform);
  const head = git(['rev-parse', 'HEAD']);
  if (options.expectedSha && options.expectedSha !== head) {
    fail(
      `main moved: the dispatcher checked ${options.expectedSha}, this run checked out ${head}. Run release:ota again.`
    );
  }
  const { plan, served } = await resolvePlan(platforms);
  printPlan(plan, served);
  if (plan.lastOtaGitSha) printChanges(tryChanges(plan.lastOtaGitSha), 'the last update');
  if (!plan.ok) refuse(plan);
  if (options.planOut) {
    mkdirSync(path.dirname(path.resolve(options.planOut)), { recursive: true });
    writeFileSync(options.planOut, `${JSON.stringify(plan, null, 2)}\n`);
  }
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `version=${plan.version}\n`);
}

function verifyExportCommand(options) {
  const dir = options.positional[0];
  if (!dir || !options.plan) fail('verify-export needs <bundle-dir> --plan <file>');
  const plan = JSON.parse(readFileSync(options.plan, 'utf8'));
  const exported = {};
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name, 'ota.json');
    if (entry.isDirectory() && existsSync(file)) exported[entry.name] = JSON.parse(readFileSync(file, 'utf8'));
  }
  const problems = verifyExport(plan, exported);
  if (problems.length > 0) fail(`the export does not match the check:\n  ${problems.join('\n  ')}`);
  console.log(`The export matches the check: ${plan.platforms.join(', ')} at ${plan.version}`);
}

/** release:actions' source checks. A dry run only reports them, since it dispatches nothing. */
function validateLocalSource({ dryRun }) {
  run('gh', ['auth', 'status'], { capture: false });
  const problems = [];
  const branch = run('git', ['branch', '--show-current']);
  if (branch !== 'main') problems.push(`the release source must be main, got ${branch}`);
  const status = run('git', ['status', '--porcelain', '--untracked-files=all']);
  if (status) problems.push(`the release source is dirty:\n${status}`);
  run('git', ['fetch', 'origin', 'main', '--tags'], { capture: false });
  const head = run('git', ['rev-parse', 'HEAD']);
  const remoteMain = run('git', ['rev-parse', 'origin/main']);
  if (head !== remoteMain) problems.push(`local main ${head} differs from origin/main ${remoteMain}`);
  const checkedOut = git(['rev-parse', 'HEAD'], { cwd: path.join(REPO_ROOT, MOBILE_APP) });
  if (checkedOut !== mobileCommitAt('HEAD')) {
    problems.push(
      `${MOBILE_APP} is at ${checkedOut}, main records ${mobileCommitAt('HEAD')}; run git submodule update ${MOBILE_APP}`
    );
  }
  if (problems.length > 0 && !dryRun) fail(problems.join('\n'));
  for (const problem of problems) console.log(`[dry run] a real run would refuse: ${problem}`);
  return head;
}

function requireSecrets() {
  const names = new Set(
    JSON.parse(run('gh', ['secret', 'list', '--repo', REPO, '--json', 'name']) || '[]').map((s) => s.name)
  );
  const missing = SECRETS.filter((name) => !names.has(name));
  if (missing.length > 0) fail(`the OTA workflow needs the repository secrets ${missing.join(', ')}`);
}

async function start(options) {
  const platforms = parsePlatforms(options.platform);
  const head = validateLocalSource(options);
  const { plan, served } = await resolvePlan(platforms);
  printPlan(plan, served);
  if (!plan.ok) refuse(plan);
  const base = plan.lastOtaGitSha;
  const changed = base ? printChanges(tryChanges(base), 'the last update') : undefined;
  if (!base) console.log('Changes since the last update: unknown (the served updates name different commits or none)');
  if (changed === 0 && !options.force) {
    fail('nothing changed in the phone app since the last update; pass --force to publish it again anyway');
  }
  if (!options.skipLocalTests) run('bun', ['run', 'release:test'], { capture: false });
  requireSecrets();
  const fields = { platforms: options.platform, expected_sha: head };
  const args = ['workflow', 'run', WORKFLOW, '--repo', REPO, '--ref', 'main'];
  for (const [name, value] of Object.entries(fields)) args.push('-f', `${name}=${value}`);
  if (options.dryRun) {
    console.log(`[dry run] would dispatch: gh ${args.join(' ')}`);
    return;
  }
  const output = run('gh', args);
  console.log(output.split('\n').find((line) => /\/actions\/runs\/\d+$/.test(line.trim())) ?? output);
  console.log(
    'Watch it with: gh run watch --repo maddada/Ghostex $(gh run list --repo maddada/Ghostex --workflow release-mobile-ota.yml --limit 1 --json databaseId --jq ".[0].databaseId")'
  );
}

const { command, options } = parseArgs(process.argv.slice(2));
const commands = { start, check, 'verify-export': verifyExportCommand };
if (options.help || !commands[command]) {
  console.log(
    readFileSync(new URL(import.meta.url), 'utf8')
      .split('*/')[0]
      .replace(/^#!.*\n\/\*\*\n/, '')
      .replace(/^ \* ?/gm, '')
  );
  process.exit(options.help ? 0 : 2);
}
await commands[command](options);
