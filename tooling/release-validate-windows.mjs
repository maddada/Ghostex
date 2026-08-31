#!/usr/bin/env node
/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-30:
 * Pre-dispatch Windows compile gate.
 *
 * `release-gpui-validate.yml` already runs inside the release workflow, but only
 * as a sibling job: when it fails, the dispatch is dead and the fix has to be
 * redispatched from scratch. In a checkout this busy, `origin/main` will usually
 * have moved by then, which invalidates every product fingerprint the cancelled
 * run had already earned. That is exactly how 8.3.0 lost its first dispatch to
 * one `#[cfg(windows)]` compile error.
 *
 * Running the same workflow standalone first costs ~23 minutes against a full
 * matrix that costs ~3 hours, and it is the only gate that compiles the
 * Windows-configured tree. `cargo check` on macOS cannot see that code at any
 * flag setting.
 *
 * Usage:
 *   bun run release:validate:windows
 *   bun run release:validate:windows -- --no-arm64
 */
import { spawnSync } from 'node:child_process';

const repo = 'maddada/Ghostex';
const workflow = 'release-gpui-validate.yml';
const pollIntervalMs = 20_000;
const discoveryAttempts = 30;

function run(command, args, { allowFailure = false } = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0 && !allowFailure) {
    throw new Error(`${command} ${args.join(' ')} failed\n${result.stderr ?? ''}`);
  }
  return result.stdout?.trim() ?? '';
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/*
 * The workflow is dispatched with `--ref main`, so validating anything other
 * than the exact commit Actions will check out would be a false signal.
 */
function resolveValidatedCommit() {
  const branch = run('git', ['rev-parse', '--abbrev-ref', 'HEAD']);
  if (branch !== 'main') throw new Error(`Release validation runs against main; this worktree is on ${branch}`);
  const head = run('git', ['rev-parse', 'HEAD']);
  const remoteMain = run('git', ['ls-remote', 'origin', 'refs/heads/main']).split(/\s+/u)[0];
  if (!remoteMain) throw new Error('Could not resolve origin/main');
  if (head !== remoteMain) {
    throw new Error(`HEAD ${head.slice(0, 10)} != origin/main ${remoteMain.slice(0, 10)}. Push first, then validate.`);
  }
  return head;
}

function listRuns() {
  const output = run('gh', [
    'run',
    'list',
    '--workflow',
    workflow,
    '--repo',
    repo,
    '--limit',
    '20',
    '--json',
    'databaseId,headSha,status,conclusion,createdAt,event,url',
  ]);
  return JSON.parse(output || '[]');
}

async function discoverRun({ head, since }) {
  for (let attempt = 1; attempt <= discoveryAttempts; attempt += 1) {
    const candidate = listRuns()
      .filter((entry) => entry.headSha === head && new Date(entry.createdAt).getTime() >= since)
      .sort((left, right) => new Date(right.createdAt) - new Date(left.createdAt))[0];
    if (candidate) return candidate;
    await sleep(4000);
  }
  throw new Error(`Dispatched ${workflow} but no run appeared for ${head.slice(0, 10)}`);
}

async function watchRun(runId) {
  for (;;) {
    const entry = JSON.parse(
      run('gh', ['run', 'view', String(runId), '--repo', repo, '--json', 'status,conclusion,url'])
    );
    if (entry.status === 'completed') return entry;
    await sleep(pollIntervalMs);
  }
}

async function main() {
  const args = process.argv.slice(2);
  const arm64 = !args.includes('--no-arm64');
  for (const arg of args) {
    if (arg !== '--no-arm64') throw new Error(`Unknown option: ${arg}`);
  }

  const head = resolveValidatedCommit();
  const since = Date.now() - 60_000;
  console.log(`Validating Windows compilation of ${head.slice(0, 10)} (arm64: ${arm64 ? 'yes' : 'no'})...`);
  run('gh', ['workflow', 'run', workflow, '--repo', repo, '--ref', 'main', '-f', `arm64_target=${arm64}`]);

  const discovered = await discoverRun({ head, since });
  console.log(`Run: ${discovered.url}`);
  console.log('Waiting for the Windows compile gate (typically ~20-25 minutes)...');

  const startedAt = Date.now();
  const finished = await watchRun(discovered.databaseId);
  const minutes = Math.round((Date.now() - startedAt) / 60_000);

  if (finished.conclusion !== 'success') {
    console.error(`\nWindows validation ${finished.conclusion} after ${minutes}m: ${finished.url}`);
    console.error('Fix the Windows-configured tree before dispatching the release.');
    process.exitCode = 1;
    return;
  }
  console.log(`\nWindows validation PASSED in ${minutes}m for ${head.slice(0, 10)}.`);
  console.log('Safe to dispatch: bun run release:actions -- <version>');
}

main().catch((error) => {
  console.error('');
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
