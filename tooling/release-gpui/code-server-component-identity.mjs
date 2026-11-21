import { createHash } from 'node:crypto';
import { lstat, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

export const CODE_SERVER_COMPONENT_IDENTITY_REVISION = 'p2';

export const CODE_SERVER_NODE_PAYLOAD_INPUTS = [
  'ci/build/build-code-server.sh',
  'src/common',
  'src/node',
  'typings',
  'package.json',
  'pnpm-lock.yaml',
  'pnpm-workspace.yaml',
  '.node-version',
  'tsconfig.json',
];

/*
 * CDXC:Release 2026-09-16 WHY:
 * The native Windows editor (VS Code REH for win32, platform windows-native-<arch>)
 * is produced by build-windows-code-server.ps1, so that script is part of the
 * payload recipe the same way ci/build/build-code-server.sh is for every platform.
 * It is folded into the one shared component identity rather than into a
 * Windows-only identity: every platform asset of the component lives under the
 * single immutable tag code-server-<componentVersion>, and a second tag would need
 * a second manifest record and a second planner entry. A recipe change therefore
 * re-keys the whole component (the Linux and Darwin assets rebuild once too, in
 * parallel and off the critical path), and an unchanged recipe reuses every asset.
 * The pinned Node version is already covered through .node-version above. The
 * inputs are read from the Ghostex checkout's HEAD, like the code-server inputs.
 */
export const CODE_SERVER_RECIPE_INPUTS = ['apps/desktop/scripts/build-windows-code-server.ps1'];

const ghostexRepoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

async function payloadFiles(codeServerRoot) {
  const files = [];

  async function visit(relativePath) {
    const absolutePath = path.join(codeServerRoot, relativePath);
    const stats = await lstat(absolutePath);
    if (stats.isDirectory()) {
      const entries = await readdir(absolutePath);
      for (const entry of entries.sort()) {
        await visit(path.posix.join(relativePath.replaceAll(path.sep, '/'), entry));
      }
      return;
    }
    if (!stats.isFile()) {
      throw new Error(`Unsupported code-server payload input: ${relativePath}`);
    }
    files.push(relativePath.replaceAll(path.sep, '/'));
  }

  for (const input of CODE_SERVER_NODE_PAYLOAD_INPUTS) {
    await visit(input);
  }
  return files.sort();
}

function canonicalInput(root, relativePath, label) {
  const result = spawnSync('git', ['-C', root, 'show', `HEAD:${relativePath}`], {
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.status !== 0 || !Buffer.isBuffer(result.stdout)) {
    const detail = Buffer.isBuffer(result.stderr) ? result.stderr.toString('utf8').trim() : '';
    throw new Error(`Could not read canonical ${label} ${relativePath} from HEAD${detail ? `: ${detail}` : ''}`);
  }
  return result.stdout;
}

export async function codeServerNodePayloadFingerprint(codeServerRoot, { recipeRoot = ghostexRepoRoot } = {}) {
  const root = path.resolve(codeServerRoot);
  const digest = createHash('sha256');
  for (const relativePath of await payloadFiles(root)) {
    const contents = canonicalInput(root, relativePath, 'code-server payload input');
    digest.update(`file\0${relativePath}\0${contents.byteLength}\0`);
    digest.update(contents);
    digest.update('\0');
  }
  for (const relativePath of CODE_SERVER_RECIPE_INPUTS) {
    const contents = canonicalInput(path.resolve(recipeRoot), relativePath, 'code-server recipe input');
    digest.update(`recipe\0${relativePath}\0${contents.byteLength}\0`);
    digest.update(contents);
    digest.update('\0');
  }
  return digest.digest('hex');
}

function resolveSourceRevision(codeServerRoot) {
  const result = spawnSync('git', ['-C', codeServerRoot, 'rev-parse', '--short=12', 'HEAD'], {
    encoding: 'utf8',
  });
  const revision = result.status === 0 ? result.stdout.trim() : '';
  if (!/^[0-9a-f]{12}$/.test(revision)) {
    throw new Error(`Could not resolve the code-server source revision from ${codeServerRoot}`);
  }
  return revision;
}

/** @param {{ codeServerRoot: string, recipeRoot?: string, sourceRevision?: string }} options */
export async function codeServerComponentIdentity({ codeServerRoot, recipeRoot, sourceRevision }) {
  const revision = sourceRevision ?? resolveSourceRevision(codeServerRoot);
  if (!/^[0-9a-f]{12}$/.test(revision)) {
    throw new Error(`Invalid code-server source revision: ${revision}`);
  }
  const payloadFingerprint = await codeServerNodePayloadFingerprint(codeServerRoot, { recipeRoot });
  return {
    componentVersion: `${revision}-${CODE_SERVER_COMPONENT_IDENTITY_REVISION}-${payloadFingerprint}`,
    payloadFingerprint,
    sourceRevision: revision,
  };
}

export function codeServerComponentNames(componentVersion, platform) {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(componentVersion)) {
    throw new Error(`Invalid code-server component version: ${componentVersion}`);
  }
  /* windows-<arch> is the WSL wrapper around the Linux archive; windows-native-<arch> is the native editor. */
  if (!/^(darwin-arm64|linux-(x64|arm64)|windows-(native-)?(x64|arm64))$/.test(platform)) {
    throw new Error(`Invalid code-server component platform: ${platform}`);
  }
  return {
    archiveName: `code-server-${componentVersion}-${platform}.tar.gz`,
    artifactName: `release-code-server-${componentVersion}-${platform}`,
    downloadTag: `code-server-${componentVersion}`,
  };
}

async function main() {
  const args = process.argv.slice(2);
  let codeServerRoot = '.dependencies/code-server';
  let githubOutput = false;
  let platform;
  for (let index = 0; index < args.length; index += 1) {
    switch (args[index]) {
      case '--root':
        codeServerRoot = args[++index];
        break;
      case '--platform':
        platform = args[++index];
        break;
      case '--github-output':
        githubOutput = true;
        break;
      default:
        throw new Error(`Unknown argument: ${args[index]}`);
    }
  }
  const identity = await codeServerComponentIdentity({ codeServerRoot });
  if (!githubOutput) {
    process.stdout.write(`${identity.componentVersion}\n`);
    return;
  }
  if (!platform) {
    throw new Error('--github-output requires --platform');
  }
  const names = codeServerComponentNames(identity.componentVersion, platform);
  process.stdout.write(
    [
      `component_version=${identity.componentVersion}`,
      `payload_fingerprint=${identity.payloadFingerprint}`,
      `archive_name=${names.archiveName}`,
      `artifact_name=${names.artifactName}`,
      `download_tag=${names.downloadTag}`,
    ].join('\n') + '\n'
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
