#!/usr/bin/env node
import { spawn } from 'node:child_process';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { validateMajorMinorReleaseNotes } from './release-ghostex.mjs';

/*
 CDXC:Release 2026-09-01-12:05:
 Writing the CHANGELOG.md section for 8.4.0 meant reading 24 commits by hand and
 privately deciding which were user-facing - about fifteen minutes, with the
 exclusions kept in a scratch ledger nobody else could review. The raw material
 was already good: this repo writes commit bodies in user-facing prose, and the
 shipped entries tracked them closely.

 This script turns that pass into a reviewable artifact. It prints a FIRST DRAFT
 of the section, structurally valid against validateMajorMinorReleaseNotes from
 the first keystroke, plus an explicit "omitted - confirm" list so every
 exclusion is signed off rather than silently dropped. It never writes to
 CHANGELOG.md: drafting and committing are separate acts, and the operator
 reviews in between.

 It is not an oracle and must not be trusted blindly. Major vs Minor here is a
 guess from the conventional-commit type; the real judgement is the operator's.
*/

const repoRoot = path.resolve(new URL('..', import.meta.url).pathname);

/* ASCII record/field separators: git subjects and bodies never contain them. */
const RECORD_SEPARATOR = '\u001e';
const FIELD_SEPARATOR = '\u001f';

/*
 Types and scopes that are release engineering rather than product change. These
 only decide which list a commit lands in - omitted commits are still printed in
 full so the operator can pull one back.
*/
const internalTypes = new Set(['build', 'chore', 'ci', 'docs', 'refactor', 'release', 'revert', 'style', 'test']);
const internalScopes = new Set(['ci', 'deps', 'release', 'skills', 'tooling']);
const majorTypes = new Set(['feat']);
const minorTypes = new Set(['fix', 'perf']);

/* Co-author trailers written by agent tooling, never a human to credit. */
const toolAuthorPattern = /(cursor|claude|codex|copilot|github-actions|dependabot|renovate|\[bot\])/i;

/* Paths whose changes alone never justify a user-facing changelog entry. */
const internalPathPattern = /^(?:\.github\/|\.agents\/|docs\/|skills\/|tooling\/|[^/]*\.md$)/;

function usage() {
  return `
Usage:
  node tooling/release-changelog-draft.mjs <version> [options]

Options:
  --base <ref>            Range start. Defaults to the highest release tag below
                          <version>.
  --head <ref>            Range end. Defaults to HEAD.
  --date <YYYY-MM-DD>     Heading date. Defaults to today.
  --primary-author <email>
                          Treat this address as the maintainer, so only other
                          authors get a "thanks to @handle" suffix. Defaults to
                          the most frequent human author in the range.
  --section-only          Print only the markdown section, with no guidance,
                          omissions, or commit detail.
  --help                  Show this help.

Prints a FIRST DRAFT of the CHANGELOG.md section for <version> to stdout. It
never edits CHANGELOG.md; paste and rewrite the section yourself.
`;
}

function parseArgs(argv) {
  const options = {
    base: null,
    date: null,
    head: 'HEAD',
    primaryAuthor: null,
    sectionOnly: false,
    version: null,
  };
  const positional = [];
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (arg === '--section-only') {
      options.sectionOnly = true;
    } else if (arg === '--base') {
      options.base = argv[index + 1]?.trim();
      if (!options.base) {
        throw new Error('--base requires a git ref.');
      }
      index += 1;
    } else if (arg === '--head') {
      options.head = argv[index + 1]?.trim();
      if (!options.head) {
        throw new Error('--head requires a git ref.');
      }
      index += 1;
    } else if (arg === '--date') {
      options.date = argv[index + 1]?.trim();
      if (!/^\d{4}-\d{2}-\d{2}$/.test(options.date ?? '')) {
        throw new Error('--date requires a YYYY-MM-DD date.');
      }
      index += 1;
    } else if (arg === '--primary-author') {
      options.primaryAuthor = argv[index + 1]?.trim();
      if (!options.primaryAuthor) {
        throw new Error('--primary-author requires an email address.');
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
    throw new Error('Pass exactly one semver version, for example 8.5.0.');
  }
  options.version = positional[0];
  return options;
}

function runGit(args) {
  return new Promise((resolve, reject) => {
    const child = spawn('git', args, { cwd: repoRoot, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
    });
    child.on('error', (error) => reject(error));
    child.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(`git ${args.join(' ')} failed (${code}): ${(stderr || stdout).trim()}`));
      } else {
        resolve(stdout);
      }
    });
  });
}

function compareSemver(left, right) {
  const parse = (value) =>
    value
      .replace(/^v/, '')
      .split('.')
      .map((part) => Number.parseInt(part, 10) || 0);
  const [leftParts, rightParts] = [parse(left), parse(right)];
  for (let index = 0; index < 3; index += 1) {
    if (leftParts[index] !== rightParts[index]) {
      return leftParts[index] - rightParts[index];
    }
  }
  return 0;
}

async function previousReleaseTag(version) {
  const output = await runGit(['tag', '--list', 'v*.*.*']);
  const tags = output
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => /^v\d+\.\d+\.\d+$/.test(line))
    .filter((tag) => compareSemver(tag, version) < 0)
    .sort(compareSemver);
  if (tags.length === 0) {
    throw new Error(`No release tag below ${version} was found. Pass --base explicitly.`);
  }
  return tags[tags.length - 1];
}

function splitTrailers(body) {
  const prose = [];
  const trailers = [];
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (/^(?:co-authored-by|signed-off-by|claude-session|reviewed-by|reported-by):/i.test(trimmed)) {
      trailers.push(trimmed);
      continue;
    }
    if (/^-{3,}$/.test(trimmed)) {
      continue;
    }
    prose.push(line);
  }
  return { prose: prose.join('\n'), trailers };
}

function paragraphsOf(text) {
  return text
    .split(/\r?\n\s*\r?\n/)
    .map((paragraph) => paragraph.replace(/\s+/g, ' ').trim())
    .filter(Boolean);
}

function normalizeForComparison(text) {
  return text
    .replace(/^\*\s*/, '')
    .replace(/^[a-z]+(?:\([^)]+\))?!?:\s*/, '')
    .replace(/\s*\(#\d+\)\s*$/, '')
    .replace(/[.\s]+$/, '')
    .toLowerCase();
}

/*
 Squash-merged pull requests repeat their own subject as the first body bullet,
 so the first paragraph is often not the sentence worth keeping. Skip any
 paragraph that just restates the headline; the full body still appears under
 "commit detail" so nothing is lost.
*/
function summaryParagraph(commit) {
  const headline = normalizeForComparison(commit.headline);
  return commit.paragraphs.find((paragraph) => normalizeForComparison(paragraph) !== headline) ?? null;
}

function parseSubject(subject) {
  const match = /^(?<type>[a-z]+)(?:\((?<scope>[^)]+)\))?(?<breaking>!)?:\s*(?<rest>.+)$/.exec(subject);
  if (!match) {
    return { breaking: false, headline: subject.trim(), scope: null, type: null };
  }
  return {
    breaking: Boolean(match.groups.breaking),
    headline: match.groups.rest.trim(),
    scope: match.groups.scope?.trim() ?? null,
    type: match.groups.type,
  };
}

function coAuthorsOf(trailers) {
  return trailers
    .filter((line) => /^co-authored-by:/i.test(line))
    .map((line) => line.slice(line.indexOf(':') + 1).trim())
    .filter((entry) => !toolAuthorPattern.test(entry));
}

async function readCommits({ base, head }) {
  const raw = await runGit([
    'log',
    `--format=${RECORD_SEPARATOR}%H${FIELD_SEPARATOR}%h${FIELD_SEPARATOR}%an${FIELD_SEPARATOR}%ae${FIELD_SEPARATOR}%s${FIELD_SEPARATOR}%b`,
    `${base}..${head}`,
  ]);
  const pathsRaw = await runGit(['log', `--format=${RECORD_SEPARATOR}%H`, '--name-only', `${base}..${head}`]);
  const pathsBySha = new Map();
  for (const block of pathsRaw.split(RECORD_SEPARATOR).slice(1)) {
    const [shaLine, ...fileLines] = block.split(/\r?\n/);
    pathsBySha.set(shaLine.trim(), fileLines.map((line) => line.trim()).filter(Boolean));
  }
  return raw
    .split(RECORD_SEPARATOR)
    .slice(1)
    .map((record) => {
      const [sha, shortSha, authorName, authorEmail, subject, body = ''] = record.split(FIELD_SEPARATOR);
      const { prose, trailers } = splitTrailers(body);
      return {
        authorEmail,
        authorName,
        coAuthors: coAuthorsOf(trailers),
        files: pathsBySha.get(sha) ?? [],
        paragraphs: paragraphsOf(prose),
        sha,
        shortSha,
        subject,
        ...parseSubject(subject),
      };
    });
}

function inferPrimaryAuthor(commits) {
  const tally = new Map();
  for (const commit of commits) {
    if (toolAuthorPattern.test(commit.authorName) || toolAuthorPattern.test(commit.authorEmail)) {
      continue;
    }
    tally.set(commit.authorEmail, (tally.get(commit.authorEmail) ?? 0) + 1);
  }
  const ranked = [...tally.entries()].sort((left, right) => right[1] - left[1]);
  return ranked.length > 0 ? ranked[0][0] : null;
}

function classify(commit) {
  if (/^(?:release|chore)(?:\([^)]*\))?:\s*(?:prepare|release)\b/.test(commit.subject)) {
    return { bucket: 'omitted', why: 'release mechanics for this very version' };
  }
  if (commit.scope && internalScopes.has(commit.scope)) {
    return { bucket: 'omitted', why: `${commit.scope} scope is release engineering, not product` };
  }
  if (commit.type && internalTypes.has(commit.type)) {
    return { bucket: 'omitted', why: `${commit.type} commits are internal by default` };
  }
  if (commit.files.length > 0 && commit.files.every((file) => internalPathPattern.test(file))) {
    return { bucket: 'omitted', why: 'touches only docs, skills, tooling, or workflow files' };
  }
  if (commit.breaking) {
    return { bucket: 'major', why: 'marked breaking' };
  }
  if (commit.type && majorTypes.has(commit.type)) {
    return { bucket: 'major', why: `${commit.type} commits start in Major` };
  }
  if (commit.type && minorTypes.has(commit.type)) {
    return { bucket: 'minor', why: `${commit.type} commits start in Minor` };
  }
  return { bucket: 'minor', why: 'unrecognized commit type; parked in Minor for review' };
}

function attributionFor(commit, primaryAuthor) {
  const credits = [];
  if (primaryAuthor && commit.authorEmail !== primaryAuthor && !toolAuthorPattern.test(commit.authorName)) {
    credits.push(commit.authorName);
  }
  for (const coAuthor of commit.coAuthors) {
    const name = coAuthor.replace(/\s*<[^>]*>\s*$/, '').trim();
    const email = /<([^>]+)>/.exec(coAuthor)?.[1] ?? '';
    if (name && email !== primaryAuthor && !credits.includes(name)) {
      credits.push(name);
    }
  }
  return credits;
}

/* One physical `  - ` line, because validateMajorMinorReleaseNotes rejects wraps. */
function renderBullet(commit, primaryAuthor) {
  const scope = commit.scope ? `[${commit.scope}] ` : '';
  const summary = summaryParagraph(commit);
  const detail = summary ? ` ${summary}` : '';
  const credits = attributionFor(commit, primaryAuthor);
  const thanks = credits.length > 0 ? `, thanks to ${credits.map((name) => `@${name}`).join(' and ')}` : '';
  const sentence = `${scope}${commit.headline}.${detail}`.replace(/\s+/g, ' ').trim().replace(/\.+$/, '');
  return `  - ${sentence}${thanks}. (${commit.shortSha})`;
}

function todayIso() {
  const now = new Date();
  const pad = (value) => String(value).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

export function renderChangelogSection({ commits, date, primaryAuthor, version }) {
  const byScope = (left, right) => (left.scope ?? '~').localeCompare(right.scope ?? '~');
  const major = commits.filter((commit) => commit.bucket === 'major').sort(byScope);
  const minor = commits.filter((commit) => commit.bucket === 'minor').sort(byScope);
  const lines = [`## ${version} - ${date}`, '', '- Major'];
  if (major.length === 0) {
    lines.push('  - TODO: no commit was classified as Major. Promote the release headline here before this ships.');
  } else {
    lines.push(...major.map((commit) => renderBullet(commit, primaryAuthor)));
  }
  lines.push('- Minor');
  if (minor.length === 0) {
    lines.push('  - TODO: no commit was classified as Minor. Move a supporting change here before this ships.');
  } else {
    lines.push(...minor.map((commit) => renderBullet(commit, primaryAuthor)));
  }
  return lines.join('\n');
}

function renderGuidance({ base, commitCount, head, primaryAuthor, version }) {
  return [
    `Ghostex changelog draft for ${version}`,
    `Range ${base}..${head} - ${commitCount} commit(s). Primary author: ${primaryAuthor ?? '(none inferred)'}`,
    '',
    'THIS IS A FIRST DRAFT, NOT AN ORACLE. Do not paste it unread.',
    '  - Every bullet is a commit subject plus its body, not release prose. Rewrite',
    '    each one the way a user would describe it, and merge the duplicates.',
    '  - Major vs Minor is guessed from the conventional-commit type (feat starts in',
    '    Major, fix and perf start in Minor). That judgement is yours, not the',
    "    script's - move bullets between the two sections freely.",
    '  - A "- GPUI" section may follow Minor for cross-platform app work. This draft',
    '    never invents one; add it by hand when the release warrants it.',
    '  - Verify every "thanks to @handle". The handle comes from the commit author',
    '    name, which is not always the GitHub login.',
    '  - Confirm every entry under "omitted - confirm" before accepting the exclusion.',
    '  - Nothing is written to CHANGELOG.md. Paste the finished section yourself.',
  ].join('\n');
}

function renderOmitted(commits) {
  if (commits.length === 0) {
    return 'omitted - confirm each exclusion (0)\n  (nothing was excluded)';
  }
  const lines = [`omitted - confirm each exclusion (${commits.length})`];
  for (const commit of commits) {
    lines.push(`  - ${commit.shortSha} ${commit.subject}`);
    lines.push(`      why omitted: ${commit.why}`);
    const summary = summaryParagraph(commit);
    if (summary) {
      lines.push(`      body: ${summary}`);
    }
  }
  return lines.join('\n');
}

function renderAttribution(commits, primaryAuthor) {
  const credited = commits
    .map((commit) => ({ commit, credits: attributionFor(commit, primaryAuthor) }))
    .filter((entry) => entry.credits.length > 0);
  if (credited.length === 0) {
    return 'attribution to preserve (0)\n  (every commit in this range is by the primary author)';
  }
  const lines = [`attribution to preserve (${credited.length})`];
  for (const { commit, credits } of credited) {
    lines.push(`  - ${commit.shortSha} ${commit.subject}`);
    lines.push(`      author: ${commit.authorName} <${commit.authorEmail}>`);
    if (commit.coAuthors.length > 0) {
      lines.push(`      co-authors: ${commit.coAuthors.join('; ')}`);
    }
    lines.push(`      suggested suffix: ", thanks to ${credits.map((name) => `@${name}`).join(' and ')}"`);
  }
  return lines.join('\n');
}

function renderDetail(commits) {
  const included = commits.filter((commit) => commit.bucket !== 'omitted');
  const lines = [`commit detail (${included.length} included)`];
  const scopes = [...new Set(included.map((commit) => commit.scope ?? '(no scope)'))].sort();
  for (const scope of scopes) {
    lines.push(`  ${scope}`);
    for (const commit of included.filter((entry) => (entry.scope ?? '(no scope)') === scope)) {
      lines.push(`    - ${commit.shortSha} [${commit.bucket}] ${commit.subject}`);
      for (const paragraph of commit.paragraphs) {
        lines.push(`        ${paragraph}`);
      }
    }
  }
  return lines.join('\n');
}

export async function buildChangelogDraft(options) {
  const base = options.base ?? (await previousReleaseTag(options.version));
  const date = options.date ?? todayIso();
  const rawCommits = await readCommits({ base, head: options.head });
  if (rawCommits.length === 0) {
    throw new Error(`No commits between ${base} and ${options.head}.`);
  }
  const primaryAuthor = options.primaryAuthor ?? inferPrimaryAuthor(rawCommits);
  const commits = rawCommits.map((commit) => ({ ...commit, ...classify(commit) }));
  const section = renderChangelogSection({ commits, date, primaryAuthor, version: options.version });
  return { base, commits, date, primaryAuthor, section };
}

function renderReport(draft, options) {
  const divider = '-'.repeat(78);
  return [
    renderGuidance({
      base: draft.base,
      commitCount: draft.commits.length,
      head: options.head,
      primaryAuthor: draft.primaryAuthor,
      version: options.version,
    }),
    '',
    `${divider}\ndraft section - edit before it goes anywhere near CHANGELOG.md\n${divider}`,
    '',
    draft.section,
    '',
    divider,
    '',
    renderOmitted(draft.commits.filter((commit) => commit.bucket === 'omitted')),
    '',
    renderAttribution(draft.commits, draft.primaryAuthor),
    '',
    renderDetail(draft.commits),
  ].join('\n');
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(usage().trim());
    return;
  }
  const draft = await buildChangelogDraft(options);
  /*
   Fail here rather than handing the operator a structurally invalid draft: the
   point of the script is that the section satisfies the release validator from
   the first keystroke, so a draft that would not is a bug in this script.
  */
  validateMajorMinorReleaseNotes(draft.section.split('\n').slice(1).join('\n').trim(), options.version);
  console.log(options.sectionOnly ? draft.section : renderReport(draft, options));
}

export { parseArgs as parseChangelogDraftArgs };

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error('');
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
