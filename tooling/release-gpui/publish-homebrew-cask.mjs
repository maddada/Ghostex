#!/usr/bin/env node
/*
 * Manual fallback for the official Homebrew cask of a published macOS release.
 *
 * With `--official`, bumps `version` and `sha256` in `Casks/g/ghostex.rb` of
 * Homebrew/homebrew-cask, touching nothing else, through a version-bump pull request
 * opened from a fork under the token's account. `--render <path>` only writes the
 * bumped cask to a file (for `brew style` / `brew audit`). The release workflow runs
 * neither: Homebrew's autobump bot opens the bump itself (see the 2026-10-04 DECISION).
 *
 * Everything goes through `gh` and the GitHub REST API. No Homebrew command runs,
 * so the script behaves identically on a macOS workstation and on a Linux runner.
 *
 * CDXC:Release 2026-10-04 DECISION:
 * User: Homebrew's autobump bot (BrewTestBot) updates the official `ghostex` cask on
 * every release, so our own pull request is opt-in (`--official`) and the release
 * workflow no longer runs it. Supersedes the 2026-09-15 decision that CI must open the
 * official-cask PR on every release. `brew install ghostex` stays the advertised macOS
 * path.
 *
 * CDXC:Release 2026-10-16 DECISION:
 * User: Ghostex leaves the personal tap maddada/homebrew-tap. Its `Casks/ghostex.rb`
 * was bumped one last time to 10.24.0 and deprecated in favour of
 * `brew install --cask ghostex`; existing tap installs keep updating through Sparkle.
 * Releases no longer push the tap, so a normal release needs no Homebrew token.
 *
 * CDXC:Release 2026-09-15 WHY:
 * `brew bump-cask-pr` was considered and rejected for CI: it needs the whole
 * homebrew-cask tap as a git clone (~620 MB), runs `brew audit`/`brew style` for a
 * cask on Linux, and decides fork and branch handling itself. The bump is two lines
 * of Ruby with a checksum the release already recorded, so the PR is opened with the
 * contents API instead and the maintainers' own CI performs the audit. An existing
 * open PR for the version (the bot's, or a manual rerun) is a success here.
 * From 10.0.1 on this path failed with HTTP 404 on `POST <fork>/git/refs` (the
 * token has no write access to the fork) while BrewTestBot opened every bump PR
 * itself, which is why it is now opt-in.
 */
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { releaseProvenanceAssetName, validateReleaseProvenance } from './provenance.mjs';
import { withRetryProfile } from './retry.mjs';

const defaultRepo = 'maddada/Ghostex';
export const officialCask = Object.freeze({
  label: 'Homebrew/homebrew-cask',
  repo: 'Homebrew/homebrew-cask',
  base: 'main',
  path: 'Casks/g/ghostex.rb',
});
const sha256Pattern = /^[0-9a-f]{64}$/u;

export function assertVersion(version) {
  if (!/^\d+\.\d+\.\d+$/u.test(version ?? '')) {
    throw new Error(`--version must be MAJOR.MINOR.PATCH, got ${version || 'nothing'}`);
  }
  return version;
}

export function assertSha256(value, label) {
  if (!sha256Pattern.test(value ?? '')) {
    throw new Error(`${label} must be a 64-character lowercase sha256, got ${value || 'nothing'}`);
  }
  return value;
}

export function releaseAssetName(version) {
  return `ghostex-${assertVersion(version)}-arm64.dmg`;
}

export function releaseAssetUrl(version, repo = defaultRepo) {
  return `https://github.com/${repo}/releases/download/v${version}/${releaseAssetName(version)}`;
}

export function branchName(version) {
  return `ghostex-${assertVersion(version)}`;
}

export function pullRequestTitle(version) {
  return `ghostex ${assertVersion(version)}`;
}

/*
 * CDXC:Release 2026-09-15 WHY:
 * Homebrew's bot closes any pull request whose body lacks the repository's
 * pull request template ("perhaps because this was written by an AI") and
 * reopens it only once the template is filled in. 9.6.0's PR was closed that
 * way one minute after it opened. The body is therefore the template itself.
 * Boxes are ticked honestly: the audit and style boxes only when the caller
 * passed --audited / --styled after really running those commands (the
 * release workflow does so on a macOS runner), and the AI box carries a
 * disclosure that the change comes from this deterministic automation.
 */
/** @param {{ version: string, repo?: string, checks?: { audited?: boolean, styled?: boolean } }} options */
export function pullRequestBody({ version, repo = defaultRepo, checks = {} }) {
  const box = (done) => (done ? '[x]' : '[ ]');
  const script = `https://github.com/${repo}/blob/main/tooling/release-gpui/publish-homebrew-cask.mjs`;
  const verified = [];
  if (checks.styled) verified.push('`brew style`');
  if (checks.audited) verified.push('`brew audit --cask --online`');
  return [
    `Version bump for the [Ghostex ${version}](https://github.com/${repo}/releases/tag/v${version}) release: \`version\` and \`sha256\` only, the rest of the cask is unchanged. The \`sha256\` is the digest GitHub records for \`${releaseAssetName(version)}\` and matches the release provenance record.`,
    '',
    '-----',
    '',
    'After making any changes to a cask, existing or new, verify:',
    '',
    '- [x] The submission is for [a stable version](https://docs.brew.sh/Acceptable-Casks#stable-versions) or [documented exception](https://docs.brew.sh/Acceptable-Casks#but-there-is-no-stable-version).',
    `- ${box(checks.audited)} \`brew audit --cask --online <cask>\` is error-free.`,
    `- ${box(checks.styled)} \`brew style --fix <cask>\` reports no offenses.`,
    '',
    'Additionally, if adding a new cask:',
    '',
    '- [ ] Named the cask according to the [token reference](https://docs.brew.sh/Cask-Cookbook#token-reference).',
    "- [ ] Checked the cask was not [already refused](https://github.com/search?q=repo%3AHomebrew%2Fhomebrew-cask+is%3Aclosed+is%3Aunmerged+&type=pullrequests) (add your cask's name to the end of the search field).",
    '- [ ] `brew audit --cask --new <cask>` worked successfully.',
    '- [ ] `HOMEBREW_NO_INSTALL_FROM_API=1 brew install --cask <cask>` worked successfully.',
    '- [ ] `brew uninstall --cask <cask>` worked successfully.',
    '',
    '-----',
    '',
    '- [x] I did not use AI/LLM to create this PR, or I disclosed the tool/model below and reviewed its output, including [`zap` stanza](https://docs.brew.sh/Cask-Cookbook#stanza-zap) paths; I did not attribute commits to AI and will answer maintainer questions and review comments myself without AI/LLM.',
    '',
    `Disclosure: this pull request is opened by the Ghostex release automation (${script}), a deterministic script that rewrites the \`version\` and \`sha256\` stanzas from the published, signed release; no language model generates the change.` +
      (verified.length > 0
        ? ` The release workflow ran ${verified.join(' and ')} on this exact file before opening it.`
        : '') +
      ' The Ghostex maintainer reviews the diff and answers questions here personally.',
    '',
    '-----',
    '',
  ].join('\n');
}

export function caskVersion(cask) {
  const match = /^\s*version "([^"]+)"/mu.exec(cask);
  if (!match) throw new Error('The cask declares no version');
  return match[1];
}

/*
 * The two stanzas are rewritten in place. Every other byte, including the
 * indentation of the rewritten lines, has to survive untouched: the diff a
 * Homebrew maintainer sees must be the two lines and nothing else.
 */
export function bumpCask(cask, { sha256, version }) {
  assertVersion(version);
  assertSha256(sha256, 'sha256');
  const updated = cask
    .replace(/^(\s*)version "[^"]+"/mu, `$1version "${version}"`)
    .replace(/^(\s*)sha256 "[0-9a-f]+"/mu, `$1sha256 "${sha256}"`);
  if (!updated.includes(`version "${version}"`) || !updated.includes(`sha256 "${sha256}"`)) {
    throw new Error('Could not rewrite the cask version and sha256 stanzas');
  }
  const before = cask.split('\n');
  const after = updated.split('\n');
  if (before.length !== after.length) throw new Error('The cask bump changed the line count');
  const changed = before.map((line, index) => index).filter((index) => before[index] !== after[index]);
  const allowed = changed.every((index) => /^\s*(version|sha256) "/u.test(before[index]));
  if (!allowed || changed.length > 2) {
    throw new Error(
      `The cask bump touched lines other than version and sha256: ${changed.map((i) => i + 1).join(', ')}`
    );
  }
  return updated;
}

export function unifiedDiff(before, after, filePath, context = 3) {
  const a = before.split('\n');
  const b = after.split('\n');
  if (a.length !== b.length) throw new Error('unifiedDiff expects a line-for-line rewrite');
  const changed = a.map((line, index) => index).filter((index) => a[index] !== b[index]);
  if (changed.length === 0) return '';
  const lines = [`--- a/${filePath}`, `+++ b/${filePath}`];
  let index = 0;
  while (index < changed.length) {
    const start = Math.max(0, changed[index] - context);
    let end = changed[index];
    while (index + 1 < changed.length && changed[index + 1] - end <= context * 2) {
      index += 1;
      end = changed[index];
    }
    end = Math.min(a.length - 1, end + context);
    lines.push(`@@ -${start + 1},${end - start + 1} +${start + 1},${end - start + 1} @@`);
    for (let line = start; line <= end; line += 1) {
      if (a[line] === b[line]) lines.push(` ${a[line]}`);
      else lines.push(`-${a[line]}`, `+${b[line]}`);
    }
    index += 1;
  }
  return `${lines.join('\n')}\n`;
}

function gh(args, options = {}) {
  const env = { ...process.env };
  if (options.token) {
    env.GH_TOKEN = options.token;
    env.GITHUB_TOKEN = options.token;
  }
  const result = spawnSync('gh', args, { encoding: 'utf8', env, input: options.input });
  if (result.error) throw new Error(`gh ${args.join(' ')} could not start: ${result.error.message}`);
  if (result.status !== 0 && !options.allowFailure) {
    throw new Error(`gh ${args.join(' ')} failed: ${(result.stderr || result.stdout || 'unknown error').trim()}`);
  }
  return result;
}

/**
 * @param {string} endpoint
 * @param {{ allowNotFound?: boolean, body?: unknown, method?: string, token?: string }} [options]
 */
function ghApi(endpoint, { allowNotFound = false, body, method = 'GET', token } = {}) {
  const args = ['api', '--method', method, endpoint];
  if (body !== undefined) args.push('--input', '-');
  return withRetryProfile(
    async () => {
      const result = gh(args, {
        allowFailure: true,
        input: body === undefined ? undefined : JSON.stringify(body),
        token,
      });
      if (result.status === 0) return result.stdout ? JSON.parse(result.stdout) : null;
      const message = (result.stderr || result.stdout || '').trim();
      if (allowNotFound && /HTTP 404/u.test(message)) return null;
      const error = new Error(`gh api ${method} ${endpoint} failed: ${message}`);
      error.ghostexHttpStatus = Number(/HTTP (\d{3})/u.exec(message)?.[1] ?? 0);
      throw error;
    },
    'github',
    { label: `gh api ${method} ${endpoint}` }
  );
}

function decodeContent(entry) {
  if (!entry || entry.type !== 'file' || typeof entry.content !== 'string') {
    throw new Error(`Unexpected contents API payload for ${entry?.path ?? 'unknown path'}`);
  }
  return Buffer.from(entry.content.replace(/\n/gu, ''), 'base64').toString('utf8');
}

/** @param {{ repo: string, path: string, ref: string, token?: string, allowNotFound?: boolean }} options */
async function readFile({ repo, path: filePath, ref, token, allowNotFound = false }) {
  const entry = await ghApi(`repos/${repo}/contents/${filePath}?ref=${encodeURIComponent(ref)}`, {
    allowNotFound,
    token,
  });
  if (!entry) return null;
  return { content: decodeContent(entry), sha: entry.sha };
}

async function writeFile({ branch, content, message, path: filePath, repo, sha, token }) {
  return ghApi(`repos/${repo}/contents/${filePath}`, {
    body: { branch, content: Buffer.from(content, 'utf8').toString('base64'), message, sha },
    method: 'PUT',
    token,
  });
}

function sha256FromGitHubMetadata(version, repo) {
  const result = gh(['release', 'view', `v${version}`, '--repo', repo, '--json', 'assets,isDraft,url'], {
    allowFailure: true,
  });
  if (result.status !== 0) {
    throw new Error(`v${version} is not a published release of ${repo}: ${(result.stderr || '').trim()}`);
  }
  const release = JSON.parse(result.stdout);
  if (release.isDraft) throw new Error(`v${version} is still a draft; refusing to publish it to Homebrew`);
  const asset = (release.assets ?? []).find((entry) => entry.name === releaseAssetName(version));
  if (!asset) {
    throw new Error(`v${version} carries no ${releaseAssetName(version)}; refusing to publish it to Homebrew`);
  }
  const digest =
    typeof asset.digest === 'string' && asset.digest.startsWith('sha256:') ? asset.digest.slice('sha256:'.length) : '';
  return sha256Pattern.test(digest) ? digest : null;
}

async function sha256FromDownload(version, repo) {
  const url = releaseAssetUrl(version, repo);
  process.stdout.write(`Hashing ${url}\n`);
  const response = await fetch(url, { redirect: 'follow' });
  if (!response.ok || !response.body) {
    throw new Error(`Could not download ${url}: HTTP ${response.status} ${response.statusText}`);
  }
  const hash = createHash('sha256');
  for await (const chunk of response.body) hash.update(chunk);
  return hash.digest('hex');
}

export async function resolveSha256({ repo, sha256, version }) {
  if (sha256) return { source: '--sha256', value: assertSha256(sha256.toLowerCase(), '--sha256') };
  const fromGitHub = sha256FromGitHubMetadata(version, repo);
  if (fromGitHub) return { source: 'GitHub release asset digest', value: fromGitHub };
  return { source: 'downloaded release asset', value: await sha256FromDownload(version, repo) };
}

/*
 * CDXC:Release 2026-08-13:
 * The cask may only advance when macOS is actually part of this release. A reused
 * DMG can only come from a same-version recovery whose cask update never happened,
 * so what matters is that these exact bytes are the ones the release recorded: the
 * published sha256 is cross-checked against the provenance record rather than
 * against GitHub's metadata alone.
 */
function verifyProvenance({ repo, sha256, version }) {
  const assetName = releaseProvenanceAssetName(version);
  const scratch = mkdtempSync(path.join(os.tmpdir(), `ghostex-${version}-provenance-`));
  try {
    const result = gh(
      ['release', 'download', `v${version}`, '--repo', repo, '--pattern', assetName, '--dir', scratch],
      {
        allowFailure: true,
      }
    );
    const file = path.join(scratch, assetName);
    if (result.status !== 0 || !existsSync(file)) {
      process.stdout.write(
        `v${version} carries no ${assetName} (released before change-aware planning); using the live DMG digest only.\n`
      );
      return;
    }
    const provenance = validateReleaseProvenance(JSON.parse(readFileSync(file, 'utf8')));
    const macos = provenance.products['macos-arm64'];
    if (!macos) {
      throw new Error(
        `Refusing to update Homebrew: v${version} published no macOS product, so the cask must not advance`
      );
    }
    const recorded = macos.artifacts.find((artifact) => artifact.name === releaseAssetName(version));
    if (!recorded) {
      throw new Error(`Refusing to update Homebrew: v${version} provenance records no ${releaseAssetName(version)}`);
    }
    if (recorded.sha256 !== sha256) {
      throw new Error(
        `Refusing to update Homebrew: live ${releaseAssetName(version)} digest ${sha256} does not match the recorded ${recorded.sha256}`
      );
    }
    process.stdout.write(
      `macOS was ${macos.action} for ${version} (${
        macos.action === 'built' ? 'this release' : `from ${macos.reusedFrom.tag ?? `run ${macos.reusedFrom.runId}`}`
      }); cask update authorized.\n`
    );
  } finally {
    rmSync(scratch, { force: true, recursive: true });
  }
}

/*
 * CDXC:Release 2026-09-15 WHY:
 * The official cask's livecheck reads the Sparkle appcast through
 * raw.githubusercontent.com, which caches for five minutes. 9.6.0's PR opened
 * a minute after the appcast advanced, Homebrew CI ran livecheck inside that
 * window, saw the previous version, and failed the PR. Wait until the raw feed
 * really serves the new version before opening the pull request.
 */
export const LIVECHECK_APPCAST_URL = 'https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml';

export async function waitForLivecheckAppcast({ version, timeoutMs = 8 * 60 * 1000, intervalMs = 20 * 1000 }) {
  const deadline = Date.now() + timeoutMs;
  const needle = `<sparkle:shortVersionString>${version}</sparkle:shortVersionString>`;
  for (;;) {
    const response = await fetch(LIVECHECK_APPCAST_URL, { cache: 'no-store' }).catch(() => null);
    const text = response && response.ok ? await response.text() : '';
    if (text.includes(needle)) {
      process.stdout.write(`The Sparkle appcast serves ${version} through the raw CDN; livecheck will agree.\n`);
      return;
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `The raw appcast at ${LIVECHECK_APPCAST_URL} does not serve ${version} yet; Homebrew's livecheck would fail. ` +
          'Advance Sparkle first, or rerun once the CDN has refreshed.'
      );
    }
    process.stdout.write(`Waiting for the raw appcast CDN to serve ${version} (livecheck source)...\n`);
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

// Shared with tooling/release-final-verify.mjs, which treats an open bump pull
// request as the official cask being on its way rather than missing.
/** @param {{ version: string, token?: string }} options */
export async function findOpenPullRequest({ version, token }) {
  const query = `repo:${officialCask.repo} is:pr is:open ghostex in:title`;
  const search = await ghApi(`search/issues?q=${encodeURIComponent(query)}&per_page=50`, { token });
  const pattern = new RegExp(`\\bghostex\\b.*\\b${version.replaceAll('.', '\\.')}\\b`, 'iu');
  return (search?.items ?? []).find((item) => pattern.test(item.title)) ?? null;
}

async function tokenLogin(token) {
  const user = await ghApi('user', { token });
  if (!user?.login) throw new Error('HOMEBREW_GITHUB_API_TOKEN does not identify a GitHub user');
  return user.login;
}

async function ensureFork({ owner, token }) {
  const fork = `${owner}/homebrew-cask`;
  const existing = await ghApi(`repos/${fork}`, { allowNotFound: true, token });
  if (existing) {
    if (!existing.fork || existing.parent?.full_name !== officialCask.repo) {
      throw new Error(`${fork} exists but is not a fork of ${officialCask.repo}`);
    }
    return fork;
  }
  process.stdout.write(`Forking ${officialCask.repo} to ${fork}.\n`);
  gh(['repo', 'fork', officialCask.repo, '--clone=false'], { token });
  // GitHub creates forks asynchronously; a large repository can take a minute.
  for (let attempt = 0; attempt < 12; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 5000));
    const created = await ghApi(`repos/${fork}`, { allowNotFound: true, token });
    if (created) return fork;
  }
  throw new Error(`${fork} is still not visible a minute after forking; rerun once GitHub finishes the fork`);
}

async function upstreamHead({ token }) {
  const repository = await ghApi(`repos/${officialCask.repo}`, { token });
  const branch = repository?.default_branch;
  if (!branch) throw new Error(`${officialCask.repo} reports no default branch`);
  const ref = await ghApi(`repos/${officialCask.repo}/git/ref/heads/${branch}`, { token });
  const sha = ref?.object?.sha;
  if (!/^[0-9a-f]{40}$/u.test(sha ?? '')) {
    throw new Error(`${officialCask.repo} ${branch} resolves to no commit: ${JSON.stringify(ref)}`);
  }
  return { branch, sha };
}

/*
 * CDXC:Release 2026-09-16 WHY:
 * The bump branch is created at upstream's own head commit, never from the fork's
 * `main`, and the fork is never synced. Release 9.7.0 failed one step earlier, in
 * `POST repos/maddada/homebrew-cask/merge-upstream`, with HTTP 422 "refusing to
 * allow a Personal Access Token to create or update workflow
 * `.github/workflows/check-issues.yml` without `workflow` scope": the sync writes
 * every upstream commit into the fork, Homebrew edits its workflow files often, and
 * a token limited to contents and pull requests may not write those. Widening
 * HOMEBREW_GITHUB_API_TOKEN to the `workflow` scope just to fast-forward a branch
 * nobody reads was rejected. A fork shares upstream's object network, so a ref in
 * the fork can point straight at an upstream commit; the fork's `main` may stay
 * arbitrarily far behind and the PR never depends on it. An existing branch is
 * force-reset to the same commit so a retry always rebases the bump on current
 * upstream instead of failing on a stale base.
 */
async function ensureBranch({ fork, branch, sha, token }) {
  const existing = await ghApi(`repos/${fork}/git/ref/heads/${branch}`, { allowNotFound: true, token });
  if (existing?.object?.sha === sha) {
    process.stdout.write(`${fork}:${branch} is already at upstream ${sha.slice(0, 12)}.\n`);
    return;
  }
  if (existing) {
    await ghApi(`repos/${fork}/git/refs/heads/${branch}`, { body: { force: true, sha }, method: 'PATCH', token });
    process.stdout.write(`Reset ${fork}:${branch} to upstream ${sha.slice(0, 12)}.\n`);
    return;
  }
  await ghApi(`repos/${fork}/git/refs`, { body: { ref: `refs/heads/${branch}`, sha }, method: 'POST', token });
  process.stdout.write(`Created ${fork}:${branch} at upstream ${sha.slice(0, 12)}.\n`);
}

function alreadyShips({ content, sha256, version, where }) {
  if (caskVersion(content) !== version) return false;
  const same = content.includes(`sha256 "${sha256}"`);
  process.stdout.write(
    `${where} already ships ghostex ${version}${same ? '' : ' with a different sha256'}; nothing to do.\n`
  );
  if (!same) {
    throw new Error(
      `${where} ghostex ${version} carries a different sha256 than ${releaseAssetName(version)}; ` +
        'a release must never republish the same version with different bytes'
    );
  }
  return true;
}

async function publishOfficialCask({ checks, dryRun, render, sha256, token, version }) {
  const live = await readFile({ path: officialCask.path, ref: officialCask.base, repo: officialCask.repo });
  const liveVersion = caskVersion(live.content);
  if (alreadyShips({ content: live.content, sha256, version, where: officialCask.label })) return;
  const bumped = bumpCask(live.content, { sha256, version });
  const diff = unifiedDiff(live.content, bumped, officialCask.path);
  process.stdout.write(`${officialCask.label}: ${liveVersion} -> ${version}\n${diff}`);
  if (render) {
    mkdirSync(path.dirname(path.resolve(render)), { recursive: true });
    writeFileSync(render, bumped);
    process.stdout.write(`Rendered the bumped cask to ${render} for brew style / brew audit.\n`);
    return;
  }

  const open = await findOpenPullRequest({ version, token: dryRun ? undefined : token });
  if (open) {
    process.stdout.write(`An open pull request for ghostex ${version} already exists: ${open.html_url}\n`);
    return;
  }
  if (dryRun) {
    process.stdout.write(`Dry run: would open "${pullRequestTitle(version)}" against ${officialCask.repo}.\n`);
    return;
  }

  await waitForLivecheckAppcast({ version });
  const owner = await tokenLogin(token);
  const fork = await ensureFork({ owner, token });
  // The livecheck wait can last minutes, so the cask is re-read at the exact
  // upstream commit the branch is based on: the contents API needs that blob's
  // sha, and a maintainer edit to another stanza in between must not be lost.
  const head = await upstreamHead({ token });
  const base = await readFile({ path: officialCask.path, ref: head.sha, repo: officialCask.repo, token });
  const where = `${officialCask.label} ${head.branch} at ${head.sha.slice(0, 12)}`;
  if (alreadyShips({ content: base.content, sha256, version, where })) return;
  const commit = bumpCask(base.content, { sha256, version });
  if (base.content !== live.content) {
    process.stdout.write(`Upstream moved during the wait; the bump below is rebased on ${head.sha.slice(0, 12)}.\n`);
    process.stdout.write(unifiedDiff(base.content, commit, officialCask.path));
  }
  const branch = branchName(version);
  await ensureBranch({ fork, branch, sha: head.sha, token });
  await writeFile({
    branch,
    content: commit,
    message: pullRequestTitle(version),
    path: officialCask.path,
    repo: fork,
    sha: base.sha,
    token,
  });
  process.stdout.write(`Committed the bump to ${fork}:${branch} on top of upstream ${head.sha.slice(0, 12)}.\n`);
  const pull = await ghApi(`repos/${officialCask.repo}/pulls`, {
    body: {
      base: head.branch,
      body: pullRequestBody({ checks, version }),
      head: `${owner}:${branch}`,
      maintainer_can_modify: true,
      title: pullRequestTitle(version),
    },
    method: 'POST',
    token,
  });
  process.stdout.write(`Opened ${pull.html_url}\n`);
}

export function parseArguments(argv) {
  const options = { dryRun: false, publish: false, repo: defaultRepo };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--publish') {
      options.publish = true;
      continue;
    }
    if (argument === '--dry-run') {
      options.dryRun = true;
      continue;
    }
    if (argument === '--audited' || argument === '--styled' || argument === '--official') {
      options[argument.slice(2)] = true;
      continue;
    }
    if (!argument.startsWith('--')) throw new Error(`Unexpected argument: ${argument}`);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for ${argument}`);
    options[argument.slice(2)] = value;
    index += 1;
  }
  return options;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const version = assertVersion(options.version);
  const dryRun = options.dryRun || !options.publish;
  if (!options.official && !options.render) {
    throw new Error(
      "Nothing to do: Homebrew's autobump bot updates the official cask and releases no longer push the personal tap. " +
        'Pass --official to open the Homebrew/homebrew-cask pull request by hand, or --render <path> to write the bumped cask.'
    );
  }
  const caskToken = process.env.HOMEBREW_GITHUB_API_TOKEN || '';
  if (!dryRun && !options.render && !caskToken) {
    throw new Error(
      'HOMEBREW_GITHUB_API_TOKEN is not set. It must be a GitHub token for the account that owns the homebrew-cask ' +
        'fork, with contents and pull-request write access. See tooling/release-gpui/homebrew-cask-setup.md.'
    );
  }
  const { source, value: sha256 } = await resolveSha256({ repo: options.repo, sha256: options.sha256, version });
  process.stdout.write(`${releaseAssetName(version)} sha256=${sha256} (from ${source})\n`);
  verifyProvenance({ repo: options.repo, sha256, version });

  await publishOfficialCask({
    checks: { audited: Boolean(options.audited), styled: Boolean(options.styled) },
    dryRun,
    render: options.render,
    sha256,
    token: caskToken,
    version,
  });
  if (dryRun && !options.publish && !options.render) process.stdout.write('Pass --publish to open the pull request.\n');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
