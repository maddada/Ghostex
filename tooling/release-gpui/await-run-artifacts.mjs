#!/usr/bin/env node
/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
 * Just-in-time wait for artifacts produced by *this* workflow run.
 *
 * This is what replaces the artificial `needs: gxserver_*` edges (§Q5). macOS,
 * Linux, Windows, and the WSL packagers never needed gxserver to *finish before
 * they start compiling* — they needed its tarball at the moment they stage a
 * package. Waiting here instead of in the job graph lets every platform's
 * compiler errors surface at t≈2 min instead of t≈22 min.
 *
 * The wait is bounded and fails with a named artifact list: a job that hangs to
 * its 150-minute timeout tells the operator nothing, and blocks a runner the
 * whole time.
 */

import { spawnSync } from 'node:child_process';
import { appendFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { classifyError } from './failure-classification.mjs';
import { withRetryProfile } from './retry.mjs';

export const DEFAULT_TIMEOUT_MINUTES = 45;
export const DEFAULT_POLL_SECONDS = 15;

/*
 * One transient `gh api` failure at minute 25 of a 45-minute poll must not kill a
 * platform job that is otherwise healthy. The listing is a pure observation, so a
 * failed poll is simply not an observation: the loop keeps going until either the
 * artifacts appear, the deadline passes, or the API has been unreachable this many
 * polls in a row — at which point something structural is wrong and failing loudly
 * beats waiting silently.
 */
export const MAX_CONSECUTIVE_LIST_FAILURES = 5;

/*
 * Producer-aware fast fail. Release 7.8.0 lost a 45-minute timeout (twice, one
 * per Windows packager) because the code-server producer job had *finished* but
 * had uploaded its artifact under a different identity name; nothing was ever
 * going to satisfy the wait. When `--producer-pattern` names the jobs that can
 * upload the awaited artifacts, the wait fails within one jobs-poll of the last
 * matching job completing, and prints the run's actual artifact names so a
 * naming mismatch is a one-glance diagnosis instead of a timeout autopsy.
 */
export const JOBS_CHECK_EVERY_POLLS = 4;

export function parseAwaitArgs(argv) {
  const options = {
    dest: null,
    names: [],
    pollSeconds: DEFAULT_POLL_SECONDS,
    producerPattern: null,
    repo: process.env.GITHUB_REPOSITORY ?? 'maddada/Ghostex',
    runId: process.env.GITHUB_RUN_ID ?? '',
    timeoutMinutes: DEFAULT_TIMEOUT_MINUTES,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = argv[index + 1];
    if (argument === '--names') {
      options.names = String(value ?? '')
        .split(',')
        .map((name) => name.trim())
        .filter(Boolean);
    } else if (argument === '--dest') options.dest = value;
    else if (argument === '--run-id') options.runId = value;
    else if (argument === '--repo') options.repo = value;
    else if (argument === '--timeout-minutes') options.timeoutMinutes = Number(value);
    else if (argument === '--poll-seconds') options.pollSeconds = Number(value);
    else if (argument === '--producer-pattern') options.producerPattern = new RegExp(String(value ?? ''), 'u');
    else throw new Error(`Unknown option: ${argument}`);
    index += 1;
  }
  if (options.names.length === 0) throw new Error('--names <a,b> is required');
  if (!/^\d+$/u.test(String(options.runId))) throw new Error('--run-id (or GITHUB_RUN_ID) is required');
  if (!Number.isFinite(options.timeoutMinutes) || options.timeoutMinutes <= 0) {
    throw new Error('--timeout-minutes must be a positive number');
  }
  return options;
}

/* Names of every non-expired artifact currently uploaded by the run. */
export function listRunArtifacts({ repo, runId, run = spawnSync }) {
  const result = run('gh', ['api', `repos/${repo}/actions/runs/${runId}/artifacts?per_page=100`], {
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`gh api artifacts failed: ${(result.stderr || result.stdout || '').trim()}`);
  }
  const payload = JSON.parse(result.stdout);
  return new Set((payload.artifacts ?? []).filter((artifact) => !artifact.expired).map((artifact) => artifact.name));
}

export function missingArtifacts(names, available) {
  return names.filter((name) => !available.has(name));
}

/* Name/status/conclusion of every job in the run (latest attempt). */
export function listRunJobs({ repo, runId, run = spawnSync }) {
  const result = run('gh', ['api', `repos/${repo}/actions/runs/${runId}/jobs?filter=latest&per_page=100`], {
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`gh api jobs failed: ${(result.stderr || result.stdout || '').trim()}`);
  }
  const payload = JSON.parse(result.stdout);
  return (payload.jobs ?? []).map((job) => ({
    conclusion: job.conclusion ?? null,
    name: job.name ?? '',
    status: job.status ?? '',
  }));
}

function describeAvailable(available) {
  const names = [...available].sort();
  return names.length > 0 ? names.join(', ') : '(none)';
}

function sleepSeconds(seconds) {
  const shared = new Int32Array(new SharedArrayBuffer(4));
  Atomics.wait(shared, 0, 0, seconds * 1000);
}

export async function awaitRunArtifacts(
  options,
  { list = listRunArtifacts, listJobs = listRunJobs, sleep = sleepSeconds } = {}
) {
  const started = Date.now();
  const deadline = started + options.timeoutMinutes * 60_000;
  let pending = [...options.names];
  let lastAvailable = new Set();
  let announced = false;
  let warnedNoProducerMatch = false;
  let consecutiveListFailures = 0;
  let poll = 0;
  while (true) {
    let available = null;
    try {
      available = list({ repo: options.repo, runId: options.runId });
      consecutiveListFailures = 0;
    } catch (error) {
      consecutiveListFailures += 1;
      const message = error instanceof Error ? error.message : String(error);
      if (consecutiveListFailures >= MAX_CONSECUTIVE_LIST_FAILURES) {
        throw new Error(
          `Could not list run ${options.runId} artifacts ${consecutiveListFailures} times in a row ` +
            `(still waiting for ${pending.join(', ')}): ${message}`
        );
      }
      process.stdout.write(
        `::warning::Artifact listing attempt ${consecutiveListFailures}/${MAX_CONSECUTIVE_LIST_FAILURES} ` +
          `failed, retrying: ${message}\n`
      );
    }
    if (available) {
      lastAvailable = available;
      pending = missingArtifacts(options.names, available);
      if (pending.length === 0) return { waitedMs: Date.now() - started };
    }
    if (available && options.producerPattern && poll % JOBS_CHECK_EVERY_POLLS === 0) {
      /* Job listing is an observation too: a failed poll is skipped, never fatal. */
      let jobs = null;
      try {
        jobs = listJobs({ repo: options.repo, runId: options.runId });
      } catch (error) {
        process.stdout.write(
          `::warning::Producer job listing failed, will retry: ${error instanceof Error ? error.message : String(error)}\n`
        );
      }
      if (jobs) {
        const producers = jobs.filter((job) => options.producerPattern.test(job.name));
        if (producers.length === 0) {
          if (!warnedNoProducerMatch) {
            process.stdout.write(
              `::warning::No job of run ${options.runId} matches producer pattern ${options.producerPattern}; ` +
                'falling back to the plain timeout\n'
            );
            warnedNoProducerMatch = true;
          }
        } else if (producers.every((job) => job.status === 'completed')) {
          const states = producers.map((job) => `${job.name} (${job.conclusion ?? 'unknown'})`).join('; ');
          throw new Error(
            `Every producer job has completed but run ${options.runId} never uploaded: ${pending.join(', ')}. ` +
              `Producer jobs: ${states}. Artifacts the run did upload: ${describeAvailable(lastAvailable)}. ` +
              'A near-miss name in that list means the artifact identity diverged between runners.'
          );
        }
      }
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `Timed out after ${options.timeoutMinutes} minutes waiting for run ${options.runId} artifacts: ` +
          `${pending.join(', ')}. Artifacts the run did upload: ${describeAvailable(lastAvailable)}`
      );
    }
    if (!announced) {
      process.stdout.write(`::notice::Waiting for run artifacts: ${pending.join(', ')}\n`);
      announced = true;
    }
    poll += 1;
    sleep(options.pollSeconds);
  }
}

/*
 * The await step has already proved every named artifact exists, so the only ways
 * the download can fail are transport ones. Classification still owns the fatal
 * signatures (an integrity or Ghostex refusal is never retried); everything it
 * cannot name is retried here, because re-downloading an artifact that provably
 * exists is idempotent and the alternative is losing a whole platform job.
 */
export function classifyArtifactDownloadFailure(error) {
  const classification = classifyError(error);
  if (classification.category === 'fatal') return classification;
  return { ...classification, retryable: true };
}

export async function downloadRunArtifacts({ dest, names, repo, retryOverrides = {}, run = spawnSync, runId }) {
  mkdirSync(dest, { recursive: true });
  for (const name of names) {
    await withRetryProfile(
      () => {
        const result = run(
          'gh',
          ['run', 'download', String(runId), '--repo', repo, '--name', name, '--dir', path.join(dest, name)],
          { encoding: 'utf8', stdio: ['ignore', 'inherit', 'pipe'] }
        );
        if (result.error) throw result.error;
        if (result.status !== 0) {
          throw new Error(`gh run download ${name} failed: ${(result.stderr ?? '').toString().trim()}`);
        }
        return result;
      },
      'github',
      { classify: classifyArtifactDownloadFailure, label: `gh run download ${name}`, ...retryOverrides }
    );
  }
}

async function main() {
  const options = parseAwaitArgs(process.argv.slice(2));
  const { waitedMs } = await awaitRunArtifacts(options);
  const seconds = Math.round(waitedMs / 1000);
  process.stdout.write(`Run artifacts available after ${seconds}s: ${options.names.join(', ')}\n`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(
      process.env.GITHUB_STEP_SUMMARY,
      `- Awaited run artifacts (${options.names.join(', ')}): ${seconds}s\n`
    );
  }
  if (options.dest) {
    await downloadRunArtifacts({
      dest: options.dest,
      names: options.names,
      repo: options.repo,
      runId: options.runId,
    });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
