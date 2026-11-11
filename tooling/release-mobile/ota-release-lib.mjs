/**
 * The decisions behind an OTA-only release of the phone app (tooling/release-mobile/ota-release.mjs):
 * which runtime versions the installed apps run, whether this checkout's JavaScript may go to them,
 * and which version it carries. Pure functions plus thin git and HTTP readers, so the rules are unit
 * tested (ota-release-lib.test.mjs) and the dispatcher and the workflow's check share them.
 *
 * CDXC:Release 2026-10-28 DECISION: a phone-only JavaScript change ships over the air without a
 * Ghostex release: no desktop builds, no Sparkle appcast, no tag and no GitHub release (user, after
 * 10.38.1 went out as a full release only to carry a phone JavaScript fix: "Yes, add it"). It is
 * refused when the phone's native side changed, because an update can't carry native code.
 *
 * CDXC:Release 2026-10-28 WHY: "the runtime the installed apps run" is read from the update server,
 * not recomputed from an old tag. A full release publishes its update to exactly the runtime its
 * APK was built with, so a runtime with a record on ota.ghostex.dev is one installed apps run, and a
 * runtime with none means the native inputs changed since the last app build. The update carries the
 * newest GitHub release's version (app.config.js only takes MAJOR.MINOR.PATCH, and the APK updater in
 * apps/mobile/app/src/updates/appUpdateStore.ts offers a new app unless the server's version is at
 * or above the newest release), so phones read it as "that release, plus a fix".
 */

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const MOBILE_APP = 'apps/mobile/app';
export const OTA_PLATFORMS = ['android', 'ios'];
export const DEFAULT_CHANNEL = 'production';

export function otaServer(env = process.env) {
  return (env.GHOSTEX_OTA_SERVER || 'https://ota.ghostex.dev').replace(/\/+$/, '');
}

/** `both` | `android` | `ios` (the workflow's choice input) → the platform list. */
export function parsePlatforms(value) {
  if (value === undefined || value === '' || value === 'both') return [...OTA_PLATFORMS];
  if (OTA_PLATFORMS.includes(value)) return [value];
  throw new Error(`platforms must be both, android or ios, got ${value}`);
}

/**
 * The record a phone on `runtime` is served, read from an Expo Updates v1 manifest response.
 * @returns {{ kind: 'none' } | { kind: 'rollBackToEmbedded' } |
 *   { kind: 'update', id: string, createdAt: string, version: string | null, gitSha: string | null }}
 */
export function parseManifestResponse(status, contentType, text) {
  if (status === 204 || status === 404) return { kind: 'none' };
  if (status !== 200) throw new Error(`the update server answered ${status}: ${text.slice(0, 200)}`);
  const boundary = /boundary=("?)([^";]+)\1/i.exec(contentType ?? '')?.[2];
  if (!boundary) throw new Error(`the update server sent ${contentType}, not a multipart manifest`);
  for (const part of text.split(`--${boundary}`)) {
    const split = part.indexOf('\r\n\r\n');
    if (split === -1) continue;
    const headers = part.slice(0, split).toLowerCase();
    const body = part.slice(split + 4).replace(/\r\n$/, '');
    if (headers.includes('name="directive"')) return { kind: 'rollBackToEmbedded' };
    if (!headers.includes('name="manifest"')) continue;
    const manifest = JSON.parse(body);
    const version = manifest.extra?.ghostex?.version ?? manifest.extra?.expoClient?.version ?? null;
    return {
      kind: 'update',
      id: manifest.id,
      createdAt: manifest.createdAt,
      version: typeof version === 'string' ? version : null,
      gitSha: typeof manifest.extra?.ghostex?.gitSha === 'string' ? manifest.extra.ghostex.gitSha : null,
    };
  }
  throw new Error('the update server sent a multipart response with neither a manifest nor a directive');
}

/** What the update server serves phones on `runtime` (the public route phones use; no token). */
export async function fetchServed({
  platform,
  runtime,
  channel = DEFAULT_CHANNEL,
  server = otaServer(),
  fetchImpl = fetch,
}) {
  const response = await fetchImpl(`${server}/v1/manifest/${channel}`, {
    headers: {
      accept: 'multipart/mixed',
      'expo-protocol-version': '1',
      'expo-platform': platform,
      'expo-runtime-version': runtime,
    },
  });
  return parseManifestResponse(response.status, response.headers.get('content-type'), await response.text());
}

/**
 * Whether this checkout's JavaScript may go over the air, and as which version.
 * @param {{
 *   platforms: string[],
 *   runtimes: Record<string, string>,
 *   served: Record<string, ReturnType<typeof parseManifestResponse>>,
 *   latestRelease: string,
 * }} input
 */
export function planOtaRelease({ platforms, runtimes, served, latestRelease }) {
  const refusals = [];
  const warnings = [];
  if (!/^\d+\.\d+\.\d+$/.test(latestRelease ?? '')) {
    refusals.push(`the newest GitHub release is ${latestRelease || 'missing'}, not a MAJOR.MINOR.PATCH version`);
  }
  for (const platform of platforms) {
    const runtime = runtimes[platform];
    const record = served[platform];
    if (!runtime) {
      refusals.push(`${platform}: no runtime version was computed`);
    } else if (record === undefined || record.kind === 'none') {
      refusals.push(
        `${platform}: no installed app runs runtime ${runtime}. The phone's native side changed since the last app build ` +
          '(Kotlin, Swift, config plugins, native dependencies or the tailcat bridge), and an ' +
          'over-the-air update cannot carry native code. Ship it with a full release (bun run release:actions).'
      );
    } else if (record.kind === 'rollBackToEmbedded') {
      warnings.push(
        `${platform}: runtime ${runtime} is rolled back to the code inside the app; this update replaces the rollback`
      );
    } else if (record.version !== null && record.version !== latestRelease) {
      warnings.push(
        `${platform}: runtime ${runtime} last got ${record.version} over the air, while the newest release is ${latestRelease}; ` +
          `phones on it will show ${latestRelease}`
      );
    }
  }
  const bases = [
    ...new Set(
      platforms
        .map((platform) => served[platform])
        .map((record) => (record?.kind === 'update' ? record.gitSha : null))
        .filter(Boolean)
    ),
  ];
  return {
    ok: refusals.length === 0,
    version: latestRelease,
    platforms,
    runtimes: Object.fromEntries(platforms.map((platform) => [platform, runtimes[platform]])),
    /* The main-repo commit of the update phones run now; the change list starts there. */
    lastOtaGitSha: bases.length === 1 ? bases[0] : null,
    refusals,
    warnings,
  };
}

/** The export must be exactly what the check approved: same platforms, runtimes and version. */
export function verifyExport(plan, exported) {
  const problems = [];
  const names = Object.keys(exported).sort();
  if (names.join(',') !== [...plan.platforms].sort().join(',')) {
    problems.push(`the export holds ${names.join(', ') || 'nothing'}, the check approved ${plan.platforms.join(', ')}`);
  }
  for (const [platform, info] of Object.entries(exported)) {
    if (info.platform !== platform) problems.push(`${platform}/ota.json is for ${info.platform}`);
    if (info.runtimeVersion !== plan.runtimes[platform]) {
      problems.push(
        `${platform}: exported runtime ${info.runtimeVersion}, the check approved ${plan.runtimes[platform]}`
      );
    }
    if (info.version !== plan.version)
      problems.push(`${platform}: exported version ${info.version}, expected ${plan.version}`);
  }
  return problems;
}

/**
 * The manifest `key` of one exported asset (not the launch bundle): the MD5 of its bytes, which is
 * the hash Metro registers the asset under in the JavaScript bundle and the name `expo export` gives
 * the file (`assets/<md5>`). Throws when the two disagree, so an export that names its files some
 * other way stops the publish instead of shipping assets the app cannot find.
 *
 * CDXC:Release 2026-11-02 WHY: issue #393. expo-asset finds an update's asset by
 * `ExpoUpdates.localAssets[<Metro md5>]`, and expo-updates fills that map with each manifest
 * asset's `key`. ota-publish.mjs used the SHA-256 as the key, so no OTA asset was ever found: a
 * page or picture byte-identical to the one inside the app fell back to the app's copy (the
 * embedded manifest keys by MD5), and a changed one resolved to an empty URI. On iOS expo-modules
 * turns "" into the app's working directory, `file:///`, so the chat core's WebView failed every
 * load with -1101 (NSURLErrorFileIsDirectory). Android's 10.42.1 APK and its OTA were built in one
 * job with the same page, so Android found the APK's copy and never showed it; the iOS app is built
 * on macOS, its page bytes differ, and every iPhone broke on the first OTA.
 *
 * CDXC:Release 2026-11-02 DECISION: the user wants iOS over-the-air updates back ("Is there really
 * no way to do ota for ios??"), so the fix ships without a new iOS build: this key, plus the app's
 * own fallback in apps/mobile/app/src/chat/rust/wasm-core/pageLoad.ts (pageFromUpdate).
 */
export function manifestAssetKey(bytes, exportedPath) {
  const md5 = createHash('md5').update(bytes).digest('hex');
  const name = path.basename(exportedPath);
  if (name !== md5) {
    throw new Error(
      `${exportedPath}: expo export named it ${name}, but its MD5 (the key the app looks it up by) is ${md5}`
    );
  }
  return md5;
}

/**
 * The asset keys in a signed update manifest that the app cannot look up (anything but a Metro MD5,
 * see {@link manifestAssetKey}). ota-publish.mjs refuses to republish such an update: 10.42.1 and
 * every update before it keyed assets by SHA-256, and serving one again breaks chat on iOS.
 */
export function unreachableAssetKeys(manifest) {
  return (manifest.assets ?? []).map((asset) => asset.key).filter((key) => !/^[0-9a-f]{32}$/.test(key ?? ''));
}

/**
 * Main-repo files the phone's update is built from: the chat agents list ota-export.sh generates
 * the phone's copy from, and every input of the chat core's WebAssembly page, the same list the
 * release workflows cache the page on (tooling/release-mobile/chat-core-page.sh). A path ending in
 * `/` is a folder.
 *
 * CDXC:Release 2026-10-28 WHY: the chat core travels in the update since the native core left the
 * app (apps/mobile/app/scripts/ota-runtime-version.js), so a change to it alone is a phone change;
 * without it here, release:ota refused a core-only change as "nothing changed in the phone app".
 */
export const BUNDLED_MAIN_INPUTS = [
  'packages/shared/session-chat-agents.ts',
  'packages/gx-chat-wasm/src/',
  'packages/gx-chat-wasm/page/',
  'packages/gx-chat-wasm/Cargo.toml',
  'packages/gx-chat-wasm/Cargo.lock',
  'packages/gx-chat-wasm/rust-toolchain.toml',
  'packages/gx-chat-json/',
  'packages/gx-chat-core/',
  'packages/gx-protocol/',
  'packages/gx-visual/',
  'tooling/xtask/src/mobile_chat_wasm.rs',
  'tooling/release-mobile/chat-core-page.sh',
];

/** The files of a changed-file list that the phone's update is built from (`BUNDLED_MAIN_INPUTS`). */
export function bundledMainChanges(changedFiles) {
  return changedFiles.filter((file) =>
    BUNDLED_MAIN_INPUTS.some((input) => (input.endsWith('/') ? file.startsWith(input) : file === input))
  );
}

/** Keeps the runtime-input files (from ota-runtime-version.js) out of a changed-file list. */
export function nativeChanges(changedFiles, runtimeInputFiles) {
  const inputs = new Set(runtimeInputFiles);
  return changedFiles.filter((file) => inputs.has(file));
}

export function git(args, { cwd = REPO_ROOT } = {}) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }).trim();
}

const require = createRequire(import.meta.url);

function runtimeModule() {
  return require(path.join(REPO_ROOT, MOBILE_APP, 'scripts/ota-runtime-version.js'));
}

/**
 * This checkout's runtime version per platform, with the native choices of the GitHub APK
 * (ota-export.sh sets GHOSTEX_ANDROID_SELF_UPDATE=1; the Play bundle publishes nothing).
 */
export function checkoutRuntimes(platforms) {
  const { otaRuntimeVersion } = runtimeModule();
  return Object.fromEntries(
    platforms.map((platform) => [platform, otaRuntimeVersion(platform, { androidSelfUpdate: true })])
  );
}

/** Every git-tracked runtime input, as main-repo paths (`apps/mobile/app/...` for the app's). */
export function runtimeInputFiles(platforms) {
  const { otaRuntimeInputs } = runtimeModule();
  const files = new Set();
  for (const platform of platforms) {
    for (const group of otaRuntimeInputs(platform, { androidSelfUpdate: true }).groups) {
      for (const file of group.files) files.add(group.label === 'app' ? `${MOBILE_APP}/${file}` : file);
    }
  }
  return [...files];
}

/** The phone app submodule's commit recorded at `ref` in the main repo. */
export function mobileCommitAt(ref) {
  return git(['rev-parse', `${ref}:${MOBILE_APP}`]);
}

/**
 * What changed between main-repo commit `base` and HEAD, as main-repo paths: the main repo's files
 * plus the phone app's files inside its submodule range. Needs both commits locally.
 */
export function changesSince(base) {
  const mainFiles = git(['diff', '--name-only', base, 'HEAD']).split('\n').filter(Boolean);
  const from = mobileCommitAt(base);
  const to = mobileCommitAt('HEAD');
  const appRoot = path.join(REPO_ROOT, MOBILE_APP);
  const appFiles =
    from === to
      ? []
      : git(['diff', '--name-only', from, to], { cwd: appRoot })
          .split('\n')
          .filter(Boolean)
          .map((file) => `${MOBILE_APP}/${file}`);
  const commits =
    from === to
      ? []
      : git(['log', '--oneline', '--no-decorate', `${from}..${to}`], { cwd: appRoot })
          .split('\n')
          .filter(Boolean);
  return { from, to, commits, files: [...mainFiles.filter((file) => file !== MOBILE_APP), ...appFiles] };
}

/** The most lines an update's "What's new" carries, and the longest line. */
export const WHATS_NEW_MAX_LINES = 8;
export const WHATS_NEW_MAX_LENGTH = 120;

/** `text` without a parenthesis left open at its end (a cut inside "(…" drops the whole aside). */
function closeAsides(text) {
  let depth = 0;
  let openedAt = -1;
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] === '(') {
      if (depth === 0) openedAt = index;
      depth += 1;
    } else if (text[index] === ')' && depth > 0) depth -= 1;
  }
  return depth > 0 && openedAt > 20 ? text.slice(0, openedAt).trimEnd() : text;
}

function shortenLine(text) {
  const whole = closeAsides(text);
  if (whole.length <= WHATS_NEW_MAX_LENGTH) return whole;
  const cut = whole.slice(0, WHATS_NEW_MAX_LENGTH - 1);
  const space = cut.lastIndexOf(' ');
  return `${closeAsides(space > 40 ? cut.slice(0, space) : cut).replace(/[\s,;:.(]+$/, '')}…`;
}

/**
 * The short "What's new" an update carries (Settings > Updates on the phone shows it): one line per
 * phone commit since the update phones run now, from `git log --oneline` lines. Merges and chores
 * are left out; a subject loses its `type(scope):` prefix and everything after its first `: ` or
 * `; ` (the phone's subjects put the headline first and the details after), and starts upper case.
 *
 * CDXC:Updates 2026-11-12 WHY: the user could not tell which update the phone ran ("I can't see the
 * actual OTA update version here"); updates now say what they bring. Built by the dispatcher, which
 * has the phone's full history (the workflow checks the phone app out at depth 1).
 */
export function whatsNewFromCommits(onelineCommits) {
  const lines = [];
  for (const entry of onelineCommits) {
    const subject = entry.replace(/^[0-9a-f]{6,40}\s+/, '').trim();
    if (/^merge\b/i.test(subject) || /^chore(\([^)]*\))?!?:/i.test(subject)) continue;
    const body = subject.replace(/^[a-z]+(\([^)]*\))?!?:\s*/i, '');
    const headline = body.split(/:\s|;\s/)[0].trim();
    if (headline.length === 0) continue;
    const line = shortenLine(headline.charAt(0).toUpperCase() + headline.slice(1));
    if (!lines.includes(line)) lines.push(line);
  }
  if (lines.length <= WHATS_NEW_MAX_LINES) return lines;
  const rest = lines.length - (WHATS_NEW_MAX_LINES - 1);
  return [...lines.slice(0, WHATS_NEW_MAX_LINES - 1), `And ${rest} more changes`];
}

/** A "What's new" list handed over as JSON (the workflow input, the export's environment); throws on anything else. */
export function parseWhatsNew(json) {
  if (json === undefined || json === null || json.trim() === '') return [];
  const value = JSON.parse(json);
  if (!Array.isArray(value) || !value.every((line) => typeof line === 'string')) {
    throw new Error("what's new must be a JSON array of strings");
  }
  return value
    .map((line) => shortenLine(line.trim()))
    .filter(Boolean)
    .slice(0, WHATS_NEW_MAX_LINES);
}
