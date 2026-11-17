#!/usr/bin/env node
/*
 * The release's "safe to go back" check: every gxserver database migration added since the
 * previous release must be one the previous version can live with, unless this release marks
 * itself "can't go back" in release-go-back.json.
 *
 * CDXC:Release 2026-11-17 DECISION:
 * User: "for db changes we need to keep those backwards compatible so we can rollback easily, i
 * believe we don't do heavy changes on db usually. but if we need to do a change and rolling back
 * isn't possible for that specific release then we can skip showing the easy rollback option in
 * that case", then Q3730: an automatic check in the release. Going back runs the older gxserver on
 * the newer database; it skips migrations it doesn't know (storage/setup.rs), so only changes it
 * can't read break it. Allowed: new tables, new indexes, new columns (SQLite makes a NOT NULL
 * column carry a default), and data changes (INSERT, UPDATE, DELETE). Anything else (dropping or
 * renaming a table, column or index, a unique index, a trigger or view, a table rebuild) fails the
 * check until `noGoBackBelow` in release-go-back.json is this release's version, which hides Go
 * back to anything older in this release (apps/desktop/src/go_back/mod.rs).
 * SEE-ALSO: ai/database-rules.md, server/src/storage/schema_migrations.rs.
 *
 * Usage: node tooling/release-gpui/go-back-gate.mjs [--version X.Y.Z] [--since <tag>] [--json]
 * Runs from release:actions before dispatch, in release-gpui.yml's gates job and in
 * release:preflight.
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const MIGRATIONS_FILE = 'server/src/storage/schema_migrations.rs';
const MIGRATIONS_DIR = 'server/src/storage';
const MARKER_FILE = 'release-go-back.json';

function git(args) {
  const result = spawnSync('git', args, { cwd: repoRoot, encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`git ${args.join(' ')} failed: ${(result.stderr || '').trim()}`);
  }
  return result.stdout;
}

function parseVersion(text) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/u.exec(String(text).trim());
  return match ? match.slice(1, 4).map(Number) : null;
}

function compareVersions(a, b) {
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return 0;
}

/** The newest release tag older than `version`. */
export function previousReleaseTag(version) {
  const current = parseVersion(version);
  const tags = git(['tag', '--list', 'v*'])
    .split(/\r?\n/u)
    .map((tag) => ({ tag, parsed: parseVersion(tag) }))
    .filter(({ parsed }) => parsed && (!current || compareVersions(parsed, current) < 0))
    .sort((a, b) => compareVersions(b.parsed, a.parsed));
  return tags[0]?.tag ?? null;
}

/** Each migration entry's id and where its SQL is: `{ id, file }` or `{ id, inline: true }`. */
export function migrationEntries(source) {
  const start = source.indexOf('GXSERVER_STORAGE_MIGRATIONS');
  if (start < 0) throw new Error(`${MIGRATIONS_FILE} no longer defines GXSERVER_STORAGE_MIGRATIONS`);
  const body = source.slice(start);
  const ids = [...body.matchAll(/\bid:\s*"([^"]+)"/gu)];
  return ids.map((match, index) => {
    const chunk = body.slice(match.index, ids[index + 1]?.index ?? body.length);
    const file = /\bsql:\s*include_str!\(\s*"([^"]+)"\s*\)/u.exec(chunk)?.[1];
    return file ? { id: match[1], file } : { id: match[1], inline: true };
  });
}

function stripComments(sql) {
  return sql.replace(/\/\*[\s\S]*?\*\//gu, ' ').replace(/--[^\n]*/gu, ' ');
}

/** The statements of `sql`, a trigger's BEGIN ... END body kept whole. */
function statements(sql) {
  const parts = stripComments(sql)
    .split(';')
    .map((part) => part.replace(/\s+/gu, ' ').trim())
    .filter(Boolean);
  const merged = [];
  for (let index = 0; index < parts.length; index += 1) {
    let statement = parts[index];
    if (/^CREATE (TEMP |TEMPORARY )?TRIGGER\b/iu.test(statement)) {
      while (!/\bEND$/iu.test(statement) && index + 1 < parts.length) {
        index += 1;
        statement = `${statement}; ${parts[index]}`;
      }
    }
    merged.push(statement);
  }
  return merged;
}

/** Tables a migration creates. */
export function createdTables(sql) {
  return statements(sql)
    .map((statement) => /^CREATE (?:TEMP |TEMPORARY )?TABLE (?:IF NOT EXISTS )?["`]?(\w+)/iu.exec(statement)?.[1])
    .filter(Boolean)
    .map((name) => name.toLowerCase());
}

/**
 * Statements the version before can't live with, as `{ statement, why }`. `newTables` are tables
 * created in this release, which the version before never writes.
 */
export function breakingStatements(sql, newTables = new Set()) {
  const found = [];
  for (const statement of statements(sql)) {
    const upper = statement.toUpperCase();
    let why = null;
    if (/^CREATE (TEMP |TEMPORARY )?TABLE\b/u.test(upper)) why = null;
    else if (/^CREATE UNIQUE INDEX\b/u.test(upper)) {
      const table = /\bON ["`]?(\w+)/iu.exec(statement)?.[1]?.toLowerCase();
      why = table && newTables.has(table) ? null : 'a unique index can refuse rows the older version writes';
    } else if (/^CREATE INDEX\b/u.test(upper)) why = null;
    else if (/^ALTER TABLE \S+ ADD (COLUMN )?/u.test(upper)) why = null;
    else if (/^ALTER TABLE \S+ RENAME\b/u.test(upper)) why = 'renames a table or column the older version reads';
    else if (/^ALTER TABLE \S+ DROP\b/u.test(upper)) why = 'drops a column the older version reads';
    else if (/^DROP (TABLE|INDEX|VIEW|TRIGGER)\b/u.test(upper)) {
      const name = /^DROP \w+ (?:IF EXISTS )?["`]?(\w+)/iu.exec(statement)?.[1]?.toLowerCase();
      why = name && newTables.has(name) ? null : 'drops something the older version uses';
    } else if (/^(INSERT|REPLACE|UPDATE|DELETE)\b/u.test(upper)) why = null;
    else if (/^WITH\b/u.test(upper) && /\b(INSERT|UPDATE|DELETE)\b/u.test(upper)) why = null;
    else if (/^PRAGMA\b/u.test(upper)) why = null;
    else if (/^CREATE (TEMP |TEMPORARY )?(TRIGGER|VIEW)\b/u.test(upper)) {
      why = 'a trigger or view can change what the older version reads or writes';
    } else why = 'a statement this check does not know is safe';
    if (why) found.push({ statement: statement.length > 160 ? `${statement.slice(0, 157)}...` : statement, why });
  }
  return found;
}

/** A migration someone reviewed: `-- go-back-safe: <why>` anywhere in its SQL. */
export function reviewedSafe(sql) {
  return /^\s*--\s*go-back-safe:\s*\S/imu.test(sql);
}

function readMarker() {
  try {
    return JSON.parse(readFileSync(path.join(repoRoot, MARKER_FILE), 'utf8'));
  } catch (error) {
    throw new Error(`${MARKER_FILE} could not be read: ${error.message}`);
  }
}

/**
 * The check. Returns `{ status: 'pass' | 'fail', detail }`; `since` overrides the previous tag.
 */
export function evaluateGoBackGate({ version, since } = {}) {
  const releaseVersion = version ?? JSON.parse(readFileSync(path.join(repoRoot, 'package.json'), 'utf8')).version;
  const previousTag = since ?? previousReleaseTag(releaseVersion);
  if (!previousTag) {
    return { status: 'pass', detail: 'no earlier release tag to compare with' };
  }
  const before = new Set(migrationEntries(git(['show', `${previousTag}:${MIGRATIONS_FILE}`])).map((entry) => entry.id));
  const added = migrationEntries(readFileSync(path.join(repoRoot, MIGRATIONS_FILE), 'utf8')).filter(
    (entry) => !before.has(entry.id)
  );
  if (added.length === 0) {
    return { status: 'pass', detail: `no database changes since ${previousTag}` };
  }
  const problems = [];
  const sqlOf = (entry) => readFileSync(path.join(repoRoot, MIGRATIONS_DIR, entry.file), 'utf8');
  const newTables = new Set(added.filter((entry) => entry.file).flatMap((entry) => createdTables(sqlOf(entry))));
  for (const entry of added) {
    if (entry.inline) {
      problems.push(
        `${entry.id}: its SQL is written inline; put it in ${MIGRATIONS_DIR}/migrations/<id>.sql (include_str!) so this check can read it`
      );
      continue;
    }
    const sql = sqlOf(entry);
    if (reviewedSafe(sql)) continue;
    for (const { statement, why } of breakingStatements(sql, newTables)) {
      problems.push(`${entry.id}: ${why}: ${statement}`);
    }
  }
  if (problems.length === 0) {
    return {
      status: 'pass',
      detail: `${added.length} database change(s) since ${previousTag}, all safe to go back from: ${added.map((entry) => entry.id).join(', ')}`,
    };
  }
  const marker = readMarker();
  if (marker.noGoBackBelow === releaseVersion) {
    return {
      status: 'pass',
      detail: `${releaseVersion} is marked "can't go back" (${marker.reason || 'no reason given'}); Go back to versions before it is hidden. Changes: ${problems.join('; ')}`,
    };
  }
  return {
    status: 'fail',
    detail: [
      `Database changes since ${previousTag} that ${previousTag.slice(1)} can't read after going back:`,
      ...problems.map((problem) => `  - ${problem}`),
      'Make each change additive (ai/database-rules.md), or, if this release really must break going back, set',
      `"noGoBackBelow": "${releaseVersion}" and a "reason" in ${MARKER_FILE}, commit, and run the release again.`,
    ].join('\n'),
  };
}

function main(argv) {
  const value = (flag) => {
    const index = argv.indexOf(flag);
    return index >= 0 ? argv[index + 1] : undefined;
  };
  const result = evaluateGoBackGate({ version: value('--version'), since: value('--since') });
  if (argv.includes('--json')) {
    console.log(JSON.stringify(result));
  } else {
    console.log(`Go back check: ${result.status.toUpperCase()}\n${result.detail}`);
  }
  process.exitCode = result.status === 'fail' ? 1 : 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`Go back check could not run: ${error.message}`);
    process.exitCode = 1;
  }
}
