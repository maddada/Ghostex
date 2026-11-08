/**
 * Signs the bundled agent skills gxserver downloads between releases (GitHub #495 follow-up,
 * "pin skills"): the release half of server/src/agent_skills_remote.rs.
 *
 * The statement names the commit the files are fetched from and every skill file's SHA-256, size
 * and git mode, one per line, sorted:
 *
 *   ghostex-skills-v1
 *   <commit, 40 hex>
 *   <issuedAtMs>
 *   <skill>/<path> <sha256> <size> <mode>
 *
 * It is signed with Ghostex's component signing key (GHOSTEX_COMPONENT_SIGNING_KEY, the same
 * Ed25519 key components are signed with), checked against the public key built into gxserver,
 * and written as `{ statement, signature }`. .github/workflows/publish-agent-skills.yml publishes
 * it as the `agent-skills` release's ghostex-skills.json. gxserver installs a downloaded skill
 * file only when it matches this statement, so a change on main reaches installed apps only once
 * a new statement is signed.
 *
 * Usage: node tooling/release-gpui/sign-agent-skills.mjs <output.json> [--commit <sha>]
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { writeFileSync } from 'node:fs';

import {
  COMPONENT_SIGNING_KEY_ENV,
  componentSigningKey,
  expectedSigningPublicKeyHex,
  signComponentStatement,
  verifyComponentSignature,
} from './component-signing.mjs';

/** Must equal AGENT_SKILLS_SIGNING_DOMAIN in server/src/agent_skills_remote.rs. */
export const SKILLS_SIGNING_DOMAIN = 'ghostex-skills-v1';
const SKILLS_ROOT = 'skills';
/** gxserver reads only plain path segments (agent_skills_remote.rs `is_safe_relative_path`). */
const SAFE_PATH = /^[A-Za-z0-9._-]+(\/[A-Za-z0-9._-]+)+$/;
const EXCLUDED_SEGMENTS = new Set(['.git', 'node_modules', '.DS_Store', '.', '..']);

function git(args) {
  return execFileSync('git', args, { encoding: 'buffer', maxBuffer: 256 * 1024 * 1024 });
}

/** Every skill file at `commit`: `{ path, sha256, size, mode }`, path relative to skills/. */
export function skillFiles(commit) {
  const listing = git(['ls-tree', '-r', '-z', commit, '--', `${SKILLS_ROOT}/`]).toString('utf8');
  const files = [];
  for (const entry of listing.split('\0')) {
    if (!entry) continue;
    const [meta, fullPath] = entry.split('\t');
    const [mode, type, blob] = meta.split(' ');
    if (type !== 'blob' || (mode !== '100644' && mode !== '100755')) continue;
    const path = fullPath.slice(SKILLS_ROOT.length + 1);
    if (!SAFE_PATH.test(path) || path.split('/').some((segment) => EXCLUDED_SEGMENTS.has(segment))) continue;
    const bytes = git(['cat-file', 'blob', blob]);
    files.push({ path, sha256: createHash('sha256').update(bytes).digest('hex'), size: bytes.length, mode });
  }
  return files.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

export function skillsStatement({ commit, issuedAtMs, files }) {
  return [
    SKILLS_SIGNING_DOMAIN,
    commit,
    String(issuedAtMs),
    ...files.map((file) => `${file.path} ${file.sha256} ${file.size} ${file.mode}`),
  ].join('\n');
}

function main() {
  const args = process.argv.slice(2);
  const output = args[0];
  if (!output || output.startsWith('--')) {
    throw new Error('Usage: node tooling/release-gpui/sign-agent-skills.mjs <output.json> [--commit <sha>]');
  }
  const commitFlag = args.indexOf('--commit');
  const commit = git(['rev-parse', commitFlag >= 0 ? args[commitFlag + 1] : 'HEAD'])
    .toString('utf8')
    .trim()
    .toLowerCase();
  if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error(`Not a commit: ${commit}`);
  const key = componentSigningKey();
  if (!key) throw new Error(`${COMPONENT_SIGNING_KEY_ENV} is not set; the skills can't be signed.`);
  const files = skillFiles(commit);
  if (!files.some((file) => file.path.endsWith('/SKILL.md'))) throw new Error('No skill found under skills/.');
  const statement = skillsStatement({ commit, issuedAtMs: Date.now(), files });
  const signature = signComponentStatement(statement, key);
  if (!verifyComponentSignature(statement, signature, expectedSigningPublicKeyHex())) {
    throw new Error(`${COMPONENT_SIGNING_KEY_ENV} does not match the component signing public key built into Ghostex.`);
  }
  writeFileSync(output, `${JSON.stringify({ statement, signature }, null, 2)}\n`);
  console.log(`Signed ${files.length} skill files at ${commit} into ${output}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}
