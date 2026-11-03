/*
 The helpers the current release tooling still uses from the retired local
 release driver (`release-ghostex*.mjs`, deleted 2026-10-01; see git history):
 build-number math, the on-demand asset names, the remote gxserver package
 check, CHANGELOG parsing and validation, and the published cask check.
*/
import { existsSync } from 'node:fs';
import path from 'node:path';

export class ReleaseError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ReleaseError';
  }
}

/*
 CDXC:RemotePairing 2026-07-13:
 The Linux remote package no longer ships the npm-style package.json
 manifest or dist/protocol exports; nothing on the
 remote host consumes them and version identity lives in build-identity.json.
 */
const remoteGxserverLinuxRequiredPackageResources = [
  'bin/gxserver',
  'bin/zmx',
  'bin/ghostex',
  'bin/tailcat',
  'build-identity.json',
];

/*
 CDXC:Release 2026-07-02-14:10:
 Public releases stop embedding the two Ubuntu remote gxserver payloads inside
 the DMG. The app build stages a sealed checksum manifest instead, and the
 release publishes the two tarballs as version-pinned GitHub release assets the
 app downloads on first use. Remote installs keep the Mac-then-scp flow.
 */
export const onDemandAssetNames = ['gxserver-linux-x64.tar.gz', 'gxserver-linux-arm64.tar.gz'];

export function releaseBuildVersion(version) {
  const [major, minor, patch] = version
    .split('-')[0]
    .split('.')
    .map((part) => Number.parseInt(part, 10));
  return major * 10000 + minor * 100 + patch;
}

export function missingRemoteGxserverLinuxPackageResources(packageDir, exists = existsSync) {
  return remoteGxserverLinuxRequiredPackageResources.filter((relativePath) => {
    return !exists(path.join(packageDir, relativePath));
  });
}

export function extractChangelogSectionFromText(changelog, version) {
  /*
   CDXC:Release 2026-05-23-14:03:
   Do not use a multiline regex with `$` here: in JS multiline mode it can stop
   at the blank line after the heading and make valid release notes look empty.
   */
  const lines = changelog.split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith(`## ${version} - `));
  if (start === -1) {
    throw new ReleaseError(`CHANGELOG.md does not contain a top-level section for ${version}.`);
  }
  const section = [];
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith('## ')) {
      break;
    }
    section.push(line);
  }
  const notes = section.join('\n').trim();
  if (!notes || notes.includes('CDXC:') || notes.includes('<!--')) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} is empty or contains comments.`);
  }
  validateMajorMinorReleaseNotes(notes, version);
  return notes;
}

const LEGACY_RELEASE_NOTE_HEADINGS = ['- Major', '- Minor', '- GPUI'];
const RELEASE_NOTE_HEADINGS = ['- New Features', '- Major Improvements', '- Minor Improvements', '- Stabilization'];
const BULLET_RELEASE_NOTE_HEADINGS = [...LEGACY_RELEASE_NOTE_HEADINGS, ...RELEASE_NOTE_HEADINGS];
/* Category names of the retired formats; a headed theme heading may not reuse them. */
const RETIRED_RELEASE_NOTE_CATEGORY_NAMES = [
  'New Features',
  'Major Improvements',
  'Minor Improvements',
  'Stabilization',
  'Major',
  'Minor',
  'GPUI',
];
/* `### <one emoji, optional VS16> <theme text>`; digits, `#` and `*` are not Extended_Pictographic so a bare number never passes as the emoji. */
const HEADED_RELEASE_NOTE_HEADING_PATTERN = /^### (\p{Extended_Pictographic}️?) (\S.*)$/u;

function changelogNotesLines(notes) {
  return notes.split(/\r?\n/).filter((line) => line.trim().length > 0);
}

/**
 * Which of the three accepted section shapes `notes` is written in: `headed`
 * (an optional bold intro line, then emoji `###` theme headings with flat `- `
 * items, 9.8.0 and later), `bullets` (`- New Features` top-level bullets with
 * `  - ` items), or `legacy` (`- Major` / `- Minor` / `- GPUI`). Any `#` line
 * makes the section headed so a headed section that also carries bullet
 * headings is reported as mixed instead of as a malformed bullet section.
 */
export function changelogNotesFormat(notes) {
  const lines = changelogNotesLines(notes);
  if (lines.some((line) => line.startsWith('#'))) {
    return 'headed';
  }
  if (lines.some((line) => LEGACY_RELEASE_NOTE_HEADINGS.includes(line))) {
    return 'legacy';
  }
  return 'bullets';
}

/** The change items of a section in document order, without their bullet markers, in every accepted format. Never the intro line or a heading. */
export function changelogNotesItems(notes) {
  const lines = changelogNotesLines(notes);
  if (changelogNotesFormat(notes) === 'headed') {
    return lines.filter((line) => line.startsWith('- ')).map((line) => line.slice(2).trim());
  }
  return lines.filter((line) => line.startsWith('  - ')).map((line) => line.slice(4).trim());
}

function validateHeadedReleaseNotes(lines, version) {
  const prefix = `CHANGELOG.md section for ${version}`;
  const mixed = lines.find((line) => BULLET_RELEASE_NOTE_HEADINGS.includes(line));
  if (mixed) {
    throw new ReleaseError(
      `${prefix} must not mix \`###\` theme headings with the \`${mixed}\` bullet heading of the older formats.`
    );
  }
  const firstHeading = lines.findIndex((line) => line.startsWith('#'));
  const intro = lines.slice(0, firstHeading);
  if (intro.length > 1) {
    throw new ReleaseError(
      `${prefix} may have at most one intro line before its first heading; found a second one: \`${intro[1]}\`.`
    );
  }
  if (intro.length === 1 && !intro[0].startsWith('**')) {
    throw new ReleaseError(
      `${prefix} may only carry one bold \`**...**\` intro line before its first heading; found \`${intro[0]}\`.`
    );
  }
  const themes = [];
  let currentHeading = null;
  let itemsInGroup = 0;
  for (const line of lines.slice(firstHeading)) {
    if (line.startsWith('#')) {
      if (!/^### /u.test(line)) {
        throw new ReleaseError(`${prefix} must write every group heading at level 3 (\`### \`); found \`${line}\`.`);
      }
      const match = HEADED_RELEASE_NOTE_HEADING_PATTERN.exec(line);
      if (!match) {
        throw new ReleaseError(
          `${prefix} heading \`${line}\` must be \`### <one emoji> <what changed>\`, with exactly one emoji and non-empty text after it.`
        );
      }
      const theme = match[2].trim();
      if (RETIRED_RELEASE_NOTE_CATEGORY_NAMES.some((name) => name.toLowerCase() === theme.toLowerCase())) {
        throw new ReleaseError(
          `${prefix} heading \`${line}\` names a category; headings must describe what changed, not a category.`
        );
      }
      if (currentHeading && itemsInGroup === 0) {
        throw new ReleaseError(`${prefix} has no items under \`${currentHeading}\`.`);
      }
      if (themes.includes(theme.toLowerCase())) {
        throw new ReleaseError(
          `${prefix} repeats the heading text \`${theme}\`; every heading must name a different theme.`
        );
      }
      themes.push(theme.toLowerCase());
      currentHeading = line;
      itemsInGroup = 0;
      continue;
    }
    if (!line.startsWith('- ') || line.slice(2).trim().length === 0) {
      throw new ReleaseError(
        `${prefix} must keep every change item on one physical \`- \` line at column 0 under \`${currentHeading}\`; found \`${line}\`.`
      );
    }
    itemsInGroup += 1;
  }
  if (itemsInGroup === 0) {
    throw new ReleaseError(`${prefix} has no items under \`${currentHeading}\`.`);
  }
}

export function validateMajorMinorReleaseNotes(notes, version) {
  /*
   * CDXC:Release 2026-09-16 DECISION:
   * User: "we add emojis and headings dividing the changelog items into
   * different groups", and each heading "should actually describe what
   * changed in high level with the points under it". So a section is an
   * optional bold intro line, then `### <emoji> <theme>` headings that each
   * name what changed (never one of the retired category names such as New
   * Features or Stabilization), each with flat `- ` items and no nesting or
   * wrapping, in any order the author picks. This supersedes the earlier
   * 2026-09-16 draft of fixed emoji category headings and the 2026-09-07
   * decision that wrote `- New Features` top-level bullets with `  - ` items;
   * that shape and the older Major/Minor/GPUI bullets stay valid because
   * sections up to 9.7.0 are already published, and a section may not mix formats.
   * Enforce this before publishing so GitHub, Sparkle, and Velopack notes stay consistent.
   */
  const lines = changelogNotesLines(notes);
  if (changelogNotesFormat(notes) === 'headed') {
    validateHeadedReleaseNotes(lines, version);
    return;
  }
  const legacy = lines.some((line) => LEGACY_RELEASE_NOTE_HEADINGS.includes(line));
  const current = lines.some((line) => RELEASE_NOTE_HEADINGS.includes(line));
  if (legacy && current) {
    throw new ReleaseError(
      `CHANGELOG.md section for ${version} must not mix Major/Minor headings with the New Features headings.`
    );
  }
  const headings = legacy ? LEGACY_RELEASE_NOTE_HEADINGS : RELEASE_NOTE_HEADINGS;
  const required = legacy ? ['- Major', '- Minor'] : [];
  const used = lines.filter((line) => headings.includes(line));
  const invalidTopLevel = lines.filter((line) => line.startsWith('- ') && !headings.includes(line));
  if (invalidTopLevel.length > 0 || used.length === 0) {
    throw new ReleaseError(
      `CHANGELOG.md section for ${version} must keep ${headings.join(', ')} as the only top-level bullets.`
    );
  }
  if (new Set(used).size !== used.length) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} must not repeat release-note headings.`);
  }
  const missing = required.filter((heading) => !used.includes(heading));
  if (missing.length > 0 || !headings.includes(lines[0])) {
    throw new ReleaseError(`CHANGELOG.md section for ${version} must use ${headings.join(', ')} top-level bullets.`);
  }
  const order = used.map((heading) => headings.indexOf(heading));
  if (order.some((index, position) => position > 0 && index < order[position - 1])) {
    throw new ReleaseError(
      `CHANGELOG.md section for ${version} must keep its headings in ${headings.join(', ')} order.`
    );
  }
  const invalidItemLines = lines.filter(
    (line) => !headings.includes(line) && (!line.startsWith('  - ') || line.slice(4).trim().length === 0)
  );
  if (invalidItemLines.length > 0) {
    throw new ReleaseError(
      `CHANGELOG.md section for ${version} must keep every change item on one physical \`  - \` line.`
    );
  }
  const headingPositions = lines.flatMap((line, index) => (headings.includes(line) ? [index] : []));
  const emptySections = headingPositions.filter((index, position) => {
    const nextIndex = headingPositions[position + 1] ?? lines.length;
    return !lines.slice(index + 1, nextIndex).some((line) => line.startsWith('  - '));
  });
  if (emptySections.length > 0) {
    throw new ReleaseError(
      `CHANGELOG.md section for ${version} must include sub-bullets under ${lines[emptySections[0]].slice(2)}.`
    );
  }
}
