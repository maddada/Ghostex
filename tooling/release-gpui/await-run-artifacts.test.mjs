/*
 * CDXC:Release 2026-08-13:
 * The just-in-time artifact wait is what replaced the artificial `needs:` edges,
 * so the two behaviours that matter are: it returns as soon as every named
 * artifact exists, and it fails with a bounded, named error instead of hanging a
 * scarce runner to its job timeout.
 */

import { describe, expect, test } from 'vitest';

import {
  MAX_CONSECUTIVE_LIST_FAILURES,
  awaitRunArtifacts,
  classifyArtifactDownloadFailure,
  downloadRunArtifacts,
  missingArtifacts,
  parseAwaitArgs,
} from './await-run-artifacts.mjs';

describe('await-run-artifacts arguments', () => {
  test('parses names, destination, and bounds', () => {
    const options = parseAwaitArgs([
      '--names',
      'release-gxserver-linux-x64, release-code-server-x',
      '--run-id',
      '31648691822',
      '--repo',
      'maddada/Ghostex',
      '--timeout-minutes',
      '20',
      '--poll-seconds',
      '5',
      '--dest',
      'build/runtime-artifacts',
    ]);
    expect(options.names).toEqual(['release-gxserver-linux-x64', 'release-code-server-x']);
    expect(options.runId).toBe('31648691822');
    expect(options.timeoutMinutes).toBe(20);
    expect(options.pollSeconds).toBe(5);
    expect(options.dest).toBe('build/runtime-artifacts');
  });

  test('requires names, a run id, and a positive timeout', () => {
    expect(() => parseAwaitArgs([])).toThrow(/--names/u);
    expect(() => parseAwaitArgs(['--names', 'a', '--run-id', 'nope'])).toThrow(/--run-id/u);
    expect(() => parseAwaitArgs(['--names', 'a', '--run-id', '1', '--timeout-minutes', '0'])).toThrow(/positive/u);
    expect(() => parseAwaitArgs(['--names', 'a', '--run-id', '1', '--wat', 'x'])).toThrow(/Unknown option/u);
  });
});

describe("waiting for the current run's artifacts", () => {
  const options = {
    dest: null,
    names: ['release-gxserver-linux-x64', 'release-gxserver-linux-arm64'],
    pollSeconds: 0,
    repo: 'maddada/Ghostex',
    runId: '31648691822',
    timeoutMinutes: 5,
  };

  test('reports which of the named artifacts are still missing', () => {
    expect(missingArtifacts(options.names, new Set(['release-gxserver-linux-x64']))).toEqual([
      'release-gxserver-linux-arm64',
    ]);
    expect(missingArtifacts(options.names, new Set(options.names))).toEqual([]);
  });

  test('returns as soon as every artifact has been uploaded', async () => {
    let poll = 0;
    const slept = [];
    await awaitRunArtifacts(options, {
      list: () => {
        poll += 1;
        return poll < 3 ? new Set(['release-gxserver-linux-x64']) : new Set(options.names);
      },
      sleep: (seconds) => slept.push(seconds),
    });
    expect(poll).toBe(3);
    expect(slept).toEqual([0, 0]);
  });

  test('fails with a bounded, named error instead of hanging the runner', async () => {
    await expect(
      awaitRunArtifacts({ ...options, timeoutMinutes: 1 / 60_000 }, { list: () => new Set(), sleep: () => {} })
    ).rejects.toThrow(/Timed out .* release-gxserver-linux-x64, release-gxserver-linux-arm64/u);
  });

  /*
   * A single `gh api` blip 25 minutes into a 45-minute poll used to kill the whole
   * platform job. The listing is an observation, not a decision, so a failed poll
   * is simply skipped.
   */
  test('survives transient listing failures and still returns once the artifacts appear', async () => {
    let poll = 0;
    const warnings = [];
    const write = process.stdout.write.bind(process.stdout);
    process.stdout.write = (chunk) => {
      warnings.push(String(chunk));
      return true;
    };
    try {
      const result = await awaitRunArtifacts(options, {
        list: () => {
          poll += 1;
          if (poll <= 3) throw new Error('gh api artifacts failed: HTTP 503 Service Unavailable');
          return new Set(options.names);
        },
        sleep: () => {},
      });
      expect(result.waitedMs).toBeGreaterThanOrEqual(0);
    } finally {
      process.stdout.write = write;
    }
    expect(poll).toBe(4);
    expect(warnings.join('')).toMatch(/Artifact listing attempt 1\/5 failed/u);
  });

  test('gives up loudly after too many consecutive listing failures', async () => {
    let poll = 0;
    await expect(
      awaitRunArtifacts(options, {
        list: () => {
          poll += 1;
          throw new Error('gh api artifacts failed: connect ETIMEDOUT');
        },
        sleep: () => {},
      })
    ).rejects.toThrow(/Could not list run 31648691822 artifacts 5 times in a row/u);
    expect(poll).toBe(MAX_CONSECUTIVE_LIST_FAILURES);
  });

  /*
   * The 7.8.0 code-server identity mismatch: the producer job finished, but its
   * artifact carried a different name, so the old wait burned its full timeout.
   * With a producer pattern the wait fails as soon as every matching job has
   * completed, and names the artifacts the run actually uploaded.
   */
  test('fails fast once every producer job completed without uploading the artifact', async () => {
    await expect(
      awaitRunArtifacts(
        { ...options, producerPattern: /code_server_x64|reuse/u },
        {
          list: () => new Set(['code-server-otheridentity-x64']),
          listJobs: () => [
            { conclusion: 'success', name: 'code_server_x64 / build', status: 'completed' },
            { conclusion: 'success', name: 'prepare', status: 'completed' },
          ],
          sleep: () => {},
        }
      )
    ).rejects.toThrow(/Every producer job has completed .* code-server-otheridentity-x64/su);
  });

  test('keeps waiting while any producer job is still running', async () => {
    let poll = 0;
    await awaitRunArtifacts(
      { ...options, producerPattern: /gxserver_/u },
      {
        list: () => {
          poll += 1;
          return poll < 3 ? new Set() : new Set(options.names);
        },
        listJobs: () => [{ conclusion: null, name: 'gxserver_x64 / build', status: 'in_progress' }],
        sleep: () => {},
      }
    );
    expect(poll).toBe(3);
  });

  test('falls back to the plain timeout when no job matches the producer pattern', async () => {
    const write = process.stdout.write.bind(process.stdout);
    const output = [];
    process.stdout.write = (chunk) => {
      output.push(String(chunk));
      return true;
    };
    try {
      await expect(
        awaitRunArtifacts(
          { ...options, producerPattern: /never_matches/u, timeoutMinutes: 1 / 60_000 },
          {
            list: () => new Set(['something-else']),
            listJobs: () => [{ conclusion: 'success', name: 'prepare', status: 'completed' }],
            sleep: () => {},
          }
        )
      ).rejects.toThrow(/Timed out .* Artifacts the run did upload: something-else/su);
    } finally {
      process.stdout.write = write;
    }
    expect(output.join('')).toMatch(/No job of run .* matches producer pattern/u);
  });

  test('reports how long it actually waited', async () => {
    const result = await awaitRunArtifacts(options, {
      list: () => new Set(options.names),
      sleep: () => {},
    });
    expect(typeof result.waitedMs).toBe('number');
    expect(result.waitedMs).toBeLessThan(60_000);
  });
});

describe('downloading the awaited artifacts', () => {
  const base = {
    dest: null,
    names: ['release-gxserver-linux-x64'],
    repo: 'maddada/Ghostex',
    runId: '31648691822',
  };

  test('retries a transient download and succeeds', async () => {
    const attempts = [];
    const destination = `${process.env.TMPDIR ?? '/tmp'}/ghostex-await-download-${process.pid}-a`;
    await downloadRunArtifacts({
      ...base,
      dest: destination,
      retryOverrides: { sleep: async () => {} },
      run: (command, args) => {
        attempts.push(args.at(-1));
        return attempts.length < 3
          ? { status: 1, stderr: 'Unable to download artifact: 502 Bad Gateway' }
          : { status: 0, stderr: '' };
      },
    });
    expect(attempts).toHaveLength(3);
  });

  test('never retries a fatal download failure', async () => {
    let calls = 0;
    const destination = `${process.env.TMPDIR ?? '/tmp'}/ghostex-await-download-${process.pid}-b`;
    await expect(
      downloadRunArtifacts({
        ...base,
        dest: destination,
        retryOverrides: { sleep: async () => {} },
        run: () => {
          calls += 1;
          return { status: 1, stderr: 'digest mismatch for release-gxserver-linux-x64' };
        },
      })
    ).rejects.toThrow(/gh run download release-gxserver-linux-x64 failed/u);
    expect(calls).toBe(1);
  });

  test('classifies unnamed download failures as retryable and integrity failures as fatal', () => {
    expect(classifyArtifactDownloadFailure(new Error('something odd happened')).retryable).toBe(true);
    expect(classifyArtifactDownloadFailure(new Error('hash mismatch')).retryable).toBe(false);
  });
});
