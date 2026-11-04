#!/usr/bin/env node
/*
 * Keep the repository's GitHub Actions cache under budget so the release
 * caches survive between on-demand releases.
 *
 * CDXC:Release 2026-09-16 WHY: the repository cache is capped at 10 GB and
 * GitHub evicts whatever it likes once the cap is crossed. On 2026-09-15 the
 * 9.6.0 Android build saved a 1.77 GB Gradle cache at 10:35 UTC; the warm
 * Rust runs later that day pushed the total over the cap and GitHub evicted
 * that Gradle entry by the evening, so the next release built Android cold
 * again (22 minutes instead of 7 to 10). sccache's GitHub Actions backend
 * writes one cache entry per compiled object (about 4,200 entries, 4 GB, for
 * the six compile targets) and never deletes the objects it superseded, so
 * without a janitor the stale objects crowd out the entries that matter.
 * This script makes the eviction choice deliberate: stale sccache objects and
 * superseded hash-keyed entries go first, the newest entry of every
 * release-critical family is never touched, and nothing created in the last
 * 24 hours is deleted.
 *
 * CDXC:ReleaseKache 2026-11-04: the Rust builds moved from sccache to kache.
 * Each warm run saves one kache store per build set as
 * kache-<KACHE_STORE_SET>-<commit sha>, and release jobs restore the newest.
 * The kache family is protected, so the newest store of every set survives
 * and the older ones go through the superseded tier. The sccache rules stay
 * until the last sccache objects (no longer read by any run) have aged out.
 *
 * Usage:
 *   node tooling/release-gpui/cache-janitor.mjs [--apply] [--repo maddada/Ghostex]
 *     [--budget-gb 9] [--sccache-unused-days 6] [--min-age-hours 24]
 *
 * Without --apply it is a dry run: it prints the inventory, the table of
 * entries it would delete, and the totals, and deletes nothing. Requires gh
 * authenticated with `actions: write` on the repository.
 */

import { execFile } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);
const GB = 1_000_000_000;
const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;
const DELETE_INTERVAL_MS = 1000;
const MAX_RATE_LIMIT_RETRIES = 4;
const WARM_WORKFLOW_FILE = 'warm-rust-build-cache.yml';

/*
 * CDXC:Release 2026-09-16 WHY: the sccache-unused window must exceed the
 * longest gap between warm runs. warm-rust-build-cache.yml runs Monday and
 * Thursday 04:00 UTC, so every object it reads legitimately sits untouched
 * for up to four days; with a 3-day window the Sunday and Wednesday janitor
 * runs would delete the whole sccache set right before the next warm run,
 * which would then compile cold. Six days stays under GitHub's own 7-day
 * eviction.
 */
export const DEFAULT_OPTIONS = Object.freeze({
  apply: false,
  budgetGb: 9,
  minAgeHours: 24,
  repo: 'maddada/Ghostex',
  sccacheUnusedDays: 6,
});

/* Key families, matched in order; the first match wins. */
/** @type {Array<[string, RegExp]>} */
const FAMILY_RULES = [
  ['sccache', /^sccache\//u],
  ['kache', /^kache-/u],
  ['cargo-reg', /^cargo-reg-/u],
  ['cargo-check', /^cargo-check-/u],
  ['cef', /^cef-/u],
  ['zig', /^zig-/u],
  ['ghosttykit', /^warm-ghosttykit-/u],
  ['gradle', /^(?:react-native-android-gradle|android-gradle)-/u],
  ['ccache', /^react-native-android-ccache-/u],
  ['tailcat-bridge', /^tailcat-bridge-/u],
  ['go', /^setup-go-/u],
  ['bun', /^bun-/u],
  ['node', /^(?:node-cache|setup-node)-/u],
];

/*
 * Families a release needs warm. Their newest entry per key prefix is never
 * deleted, not even to reach the budget; only older siblings with the same
 * prefix (an entry restore-keys would no longer pick) are candidates.
 */
export const PROTECTED_FAMILIES = new Set([
  'kache',
  'cargo-reg',
  'cargo-check',
  'cef',
  'zig',
  'ghosttykit',
  'gradle',
  'ccache',
  'tailcat-bridge',
  'go',
]);

function usage() {
  return (
    'Usage: node tooling/release-gpui/cache-janitor.mjs [--apply] [--repo maddada/Ghostex] ' +
    '[--budget-gb 9] [--sccache-unused-days 6] [--min-age-hours 24]'
  );
}

export function parseArgs(argv) {
  /** @type {{ apply: boolean, budgetGb: number, help?: boolean, minAgeHours: number, repo: string, sccacheUnusedDays: number }} */
  const options = { ...DEFAULT_OPTIONS };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      const value = argv[index + 1];
      if (value === undefined) throw new Error(`${arg} requires a value`);
      index += 1;
      return value;
    };
    if (arg === '--help' || arg === '-h') return { ...options, help: true };
    if (arg === '--apply') options.apply = true;
    else if (arg === '--repo') options.repo = next();
    else if (arg === '--budget-gb') options.budgetGb = Number(next());
    else if (arg === '--sccache-unused-days') options.sccacheUnusedDays = Number(next());
    else if (arg === '--min-age-hours') options.minAgeHours = Number(next());
    else throw new Error(`Unknown option: ${arg}\n${usage()}`);
  }
  if (!Number.isFinite(options.budgetGb) || options.budgetGb <= 0) throw new Error('--budget-gb must be > 0');
  if (!Number.isFinite(options.sccacheUnusedDays) || options.sccacheUnusedDays < 1) {
    throw new Error('--sccache-unused-days must be >= 1');
  }
  if (!Number.isFinite(options.minAgeHours) || options.minAgeHours < 0) throw new Error('--min-age-hours must be >= 0');
  return options;
}

export function familyOf(key) {
  for (const [family, pattern] of FAMILY_RULES) if (pattern.test(key)) return family;
  return 'other';
}

/*
 * The key with its trailing content hash removed (`cargo-reg-linux-<sha256>`
 * becomes `cargo-reg-linux`), or null for keys without one. Entries sharing a
 * prefix are the versions of one actions/cache slot; restore-keys only ever
 * pick the newest, so the older ones are dead weight.
 */
export function keyPrefix(key) {
  const match = /^(.*)-[0-9a-f]{32,}$/u.exec(key);
  return match ? match[1] : null;
}

function isProtected(entry) {
  return PROTECTED_FAMILIES.has(entry.family);
}

function normalize(raw) {
  return {
    createdAt: Date.parse(raw.created_at),
    family: familyOf(raw.key),
    id: raw.id,
    key: raw.key,
    lastAccessedAt: Date.parse(raw.last_accessed_at),
    prefix: familyOf(raw.key) === 'sccache' ? null : keyPrefix(raw.key),
    ref: raw.ref,
    sizeInBytes: raw.size_in_bytes,
  };
}

function refIsLive(ref, liveRefs) {
  const branch = /^refs\/heads\/(.+)$/u.exec(ref);
  if (branch) return liveRefs.branches.has(branch[1]);
  const pull = /^refs\/pull\/(\d+)\/merge$/u.exec(ref);
  if (pull) return liveRefs.openPulls.has(Number(pull[1]));
  /* Tags and anything else are never deleted on ref grounds. */
  return true;
}

/*
 * Decide what to delete. Pure: takes the normalized inventory plus the facts
 * it needs and returns the selection, so a dry run and a real run agree.
 *
 * CDXC:Release 2026-09-16 WHY: tiers, in order, each skipping entries younger
 * than minAgeMs (a release or warm run that just wrote an entry must never
 * lose it to the janitor running in parallel):
 *   1. dead-ref: the branch was deleted or the PR is closed; main-branch runs
 *      cannot read those entries at all.
 *   2. sccache-unused: sccache objects nobody read for sccacheUnusedMs.
 *   3. sccache-stale: sccache objects the latest successful warm run did not
 *      read. That run compiles every release target with the release env, so
 *      an object it did not touch is not in the current crate graph (it is a
 *      superseded object, or a release-run workspace object built with the
 *      real marketing version that no later run can hit).
 *   4. superseded: hash-keyed entries that are not the newest of their key
 *      prefix (an older Cargo.lock's registry cache, an older NDK's ccache).
 *      This tier also applies to protected families, because restore-keys
 *      never fall back to those older siblings.
 *   5. budget: if the projected total is still above budgetBytes, the least
 *      recently read sccache objects until it fits. This is exactly what
 *      GitHub's own eviction would do, minus the collateral damage: it is what
 *      evicted the 9.6.0 Gradle cache.
 * The newest entry of a protected family is never selected by any tier.
 */
export function selectDeletions(rawEntries, facts) {
  const { budgetBytes, liveRefs, minAgeMs, now, sccacheUnusedMs, warmRunStartedAt } = facts;
  const entries = rawEntries.map(normalize);
  const selected = new Map();
  const fresh = (entry) => now - entry.createdAt < minAgeMs;

  const newestByPrefix = new Map();
  for (const entry of entries) {
    if (entry.prefix === null) continue;
    const slot = `${entry.ref}\n${entry.prefix}`;
    const current = newestByPrefix.get(slot);
    if (!current || entry.createdAt > current.createdAt) newestByPrefix.set(slot, entry);
  }
  const isNewestOfPrefix = (entry) =>
    entry.prefix !== null && newestByPrefix.get(`${entry.ref}\n${entry.prefix}`)?.id === entry.id;
  /* A protected family's live entry: the newest of its prefix, or its only entry when the key has no hash. */
  const untouchable = (entry) => isProtected(entry) && (entry.prefix === null || isNewestOfPrefix(entry));
  const eligible = (entry) => !selected.has(entry.id) && !fresh(entry) && !untouchable(entry);

  for (const entry of entries) {
    if (eligible(entry) && !refIsLive(entry.ref, liveRefs)) selected.set(entry.id, 'dead-ref');
  }
  for (const entry of entries) {
    if (eligible(entry) && entry.family === 'sccache' && now - entry.lastAccessedAt >= sccacheUnusedMs) {
      selected.set(entry.id, 'sccache-unused');
    }
  }
  if (warmRunStartedAt !== null) {
    for (const entry of entries) {
      if (
        eligible(entry) &&
        entry.family === 'sccache' &&
        entry.createdAt < warmRunStartedAt &&
        entry.lastAccessedAt < warmRunStartedAt
      ) {
        selected.set(entry.id, 'sccache-stale');
      }
    }
  }
  for (const entry of entries) {
    if (eligible(entry) && entry.prefix !== null && !isNewestOfPrefix(entry)) selected.set(entry.id, 'superseded');
  }

  const totalBytes = entries.reduce((sum, entry) => sum + entry.sizeInBytes, 0);
  let projectedBytes = totalBytes;
  for (const entry of entries) if (selected.has(entry.id)) projectedBytes -= entry.sizeInBytes;
  if (projectedBytes > budgetBytes) {
    const candidates = entries
      .filter((entry) => eligible(entry) && entry.family === 'sccache')
      .sort((a, b) => a.lastAccessedAt - b.lastAccessedAt);
    for (const entry of candidates) {
      if (projectedBytes <= budgetBytes) break;
      selected.set(entry.id, 'budget');
      projectedBytes -= entry.sizeInBytes;
    }
  }

  const deletions = entries
    .filter((entry) => selected.has(entry.id))
    .map((entry) => ({ ...entry, reason: selected.get(entry.id) }))
    .sort((a, b) => a.reason.localeCompare(b.reason) || b.sizeInBytes - a.sizeInBytes);
  return { budgetBytes, deletions, entries, projectedBytes, totalBytes };
}

function gb(bytes) {
  return `${(bytes / GB).toFixed(2)} GB`;
}

function mb(bytes) {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

function stamp(ms) {
  return new Date(ms).toISOString().slice(0, 16).replace('T', ' ');
}

function shortKey(key) {
  return key.length <= 72 ? key : `${key.slice(0, 48)}...${key.slice(-21)}`;
}

function padTable(rows) {
  const widths = rows[0].map((_, column) => Math.max(...rows.map((row) => row[column].length)));
  return rows.map((row) =>
    row
      .map((cell, column) => cell.padEnd(widths[column]))
      .join('  ')
      .trimEnd()
  );
}

export function formatInventory(entries) {
  const byFamily = new Map();
  for (const entry of entries) {
    const bucket = byFamily.get(entry.family) ?? { bytes: 0, count: 0, refs: new Set() };
    bucket.count += 1;
    bucket.bytes += entry.sizeInBytes;
    bucket.refs.add(entry.ref);
    byFamily.set(entry.family, bucket);
  }
  const rows = [['family', 'entries', 'size', 'refs', 'protected']];
  for (const [family, bucket] of [...byFamily.entries()].sort((a, b) => b[1].bytes - a[1].bytes)) {
    rows.push([
      family,
      String(bucket.count),
      gb(bucket.bytes),
      String(bucket.refs.size),
      PROTECTED_FAMILIES.has(family) ? 'yes' : 'no',
    ]);
  }
  return padTable(rows);
}

export function formatDeletions(selection) {
  const { budgetBytes, deletions, entries, projectedBytes, totalBytes } = selection;
  const lines = [];
  if (deletions.length === 0) {
    lines.push('nothing to delete');
  } else {
    const rows = [['reason', 'family', 'size', 'created', 'last read', 'key']];
    for (const entry of deletions) {
      rows.push([
        entry.reason,
        entry.family,
        mb(entry.sizeInBytes),
        stamp(entry.createdAt),
        stamp(entry.lastAccessedAt),
        shortKey(entry.key),
      ]);
    }
    lines.push(...padTable(rows), '');
    const byReason = new Map();
    for (const entry of deletions) {
      const bucket = byReason.get(entry.reason) ?? { bytes: 0, count: 0 };
      bucket.count += 1;
      bucket.bytes += entry.sizeInBytes;
      byReason.set(entry.reason, bucket);
    }
    for (const [reason, bucket] of byReason) lines.push(`${reason}: ${bucket.count} entries, ${gb(bucket.bytes)}`);
  }
  const deletedBytes = totalBytes - projectedBytes;
  lines.push(
    `total ${entries.length} entries, ${gb(totalBytes)}; delete ${deletions.length} entries, ${gb(deletedBytes)}; ` +
      `after: ${gb(projectedBytes)} of the ${gb(budgetBytes)} budget`
  );
  if (projectedBytes > budgetBytes) {
    lines.push(
      `WARN  budget not reachable: every remaining entry is protected or younger than the minimum age ` +
        `(${gb(projectedBytes - budgetBytes)} over)`
    );
  }
  return lines;
}

async function gh(args, options = {}) {
  const { stdout } = await execFileAsync('gh', args, { maxBuffer: 64 * 1024 * 1024, ...options });
  return stdout;
}

/* One line per result; `--jq` prints strings bare, so only object lines are JSON. */
async function ghLines(args) {
  const stdout = await gh(args);
  return stdout.split('\n').filter((line) => line.trim() !== '');
}

async function ghJsonLines(args) {
  return (await ghLines(args)).map((line) => JSON.parse(line));
}

async function listCaches(repo) {
  return ghJsonLines(['api', '--paginate', `repos/${repo}/actions/caches?per_page=100`, '--jq', '.actions_caches[]']);
}

async function listLiveRefs(repo) {
  const branches = await ghLines(['api', '--paginate', `repos/${repo}/branches?per_page=100`, '--jq', '.[].name']);
  const openPulls = await ghLines([
    'api',
    '--paginate',
    `repos/${repo}/pulls?state=open&per_page=100`,
    '--jq',
    '.[].number',
  ]);
  return { branches: new Set(branches), openPulls: new Set(openPulls.map(Number)) };
}

/* Start time of the latest successful warm run on main, or null when none exists. */
async function latestWarmRunStart(repo) {
  const runs = await ghJsonLines([
    'run',
    'list',
    '--repo',
    repo,
    '--workflow',
    WARM_WORKFLOW_FILE,
    '--branch',
    'main',
    '--status',
    'success',
    '--limit',
    '1',
    '--json',
    'databaseId,createdAt',
    '--jq',
    '.[]',
  ]);
  return runs.length === 0 ? null : { id: runs[0].databaseId, startedAt: Date.parse(runs[0].createdAt) };
}

/*
 * CDXC:Release 2026-09-16 WHY:
 * Four parallel deletion workers caused 972 HTTP 429 failures in one janitor run.
 * GitHub requires serial mutations with a one-second pause; rate-limit responses must pause the entire queue, including until the hourly token budget resets.
 * https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api
 */
function rateLimitDelay(error, attempt) {
  const response = String(error?.stdout ?? '');
  const status = /^HTTP\/\S+\s+(\d+)/mu.exec(response)?.[1];
  const header = (name) => new RegExp(`^${name}:\\s*([^\\r\\n]+)`, 'imu').exec(response)?.[1].trim();
  const retryAfter = header('retry-after');
  const exhausted = header('x-ratelimit-remaining') === '0';
  if (
    status !== '429' &&
    !(status === '403' && (retryAfter !== undefined || exhausted || /rate limit/iu.test(response)))
  ) {
    return null;
  }
  const resetMs = exhausted ? Number(header('x-ratelimit-reset')) * 1000 - Date.now() + 1000 : 0;
  return Math.max(
    60_000 * 2 ** attempt,
    Number.isFinite(Number(retryAfter)) ? Number(retryAfter) * 1000 : 0,
    Number.isFinite(resetMs) ? resetMs : 0
  );
}

async function deleteEntries(repo, deletions, print) {
  let done = 0;
  for (const entry of deletions) {
    if (done > 0) await delay(DELETE_INTERVAL_MS);
    for (let attempt = 0; ; attempt += 1) {
      try {
        await gh(['api', '--include', '--method', 'DELETE', `repos/${repo}/actions/caches/${entry.id}`]);
        break;
      } catch (error) {
        const waitMs = rateLimitDelay(error, attempt);
        if (waitMs !== null && attempt < MAX_RATE_LIMIT_RETRIES) {
          print(
            `rate limited; waiting ${Math.ceil(waitMs / 1000)} s before retrying cache ${entry.id} (${attempt + 1}/${MAX_RATE_LIMIT_RETRIES})`
          );
          await delay(waitMs);
          continue;
        }
        const reason = String(error?.stderr || error?.message || error)
          .trim()
          .split('\n')[0];
        throw new Error(
          `Stopped after deleting ${done}/${deletions.length}; cache ${entry.id} ${entry.key}: ${reason}`
        );
      }
    }
    done += 1;
    if (done % 50 === 0 || done === deletions.length) print(`deleted ${done}/${deletions.length}`);
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(usage());
    return 0;
  }
  const print = console.log;
  const now = Date.now();
  const [rawEntries, liveRefs, warmRun] = await Promise.all([
    listCaches(options.repo),
    listLiveRefs(options.repo),
    latestWarmRunStart(options.repo),
  ]);
  print(`${options.apply ? 'APPLY' : 'DRY RUN'}  ${options.repo}  ${stamp(now)} UTC`);
  print(
    `budget ${options.budgetGb} GB, sccache unused after ${options.sccacheUnusedDays} d, ` +
      `minimum age ${options.minAgeHours} h, live branches ${liveRefs.branches.size}, open PRs ${liveRefs.openPulls.size}`
  );
  print(
    warmRun
      ? `latest successful warm run ${warmRun.id} started ${stamp(warmRun.startedAt)} UTC`
      : `no successful ${WARM_WORKFLOW_FILE} run on main; the sccache-stale tier is off`
  );
  print('');
  const selection = selectDeletions(rawEntries, {
    budgetBytes: options.budgetGb * GB,
    liveRefs,
    minAgeMs: options.minAgeHours * HOUR_MS,
    now,
    sccacheUnusedMs: options.sccacheUnusedDays * DAY_MS,
    warmRunStartedAt: warmRun ? warmRun.startedAt : null,
  });
  for (const line of formatInventory(selection.entries)) print(line);
  print('');
  for (const line of formatDeletions(selection)) print(line);
  if (!options.apply || selection.deletions.length === 0) return 0;
  print('');
  await deleteEntries(options.repo, selection.deletions, print);
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (error) => {
      console.error(error instanceof Error ? error.message : error);
      process.exitCode = 1;
    }
  );
}
