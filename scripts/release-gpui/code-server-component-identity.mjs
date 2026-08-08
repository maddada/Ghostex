import { createHash } from 'node:crypto';
import { lstat, readFile, readdir } from 'node:fs/promises';
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
  'package-lock.json',
  '.node-version',
  'tsconfig.json',
];

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

export async function codeServerNodePayloadFingerprint(codeServerRoot) {
  const root = path.resolve(codeServerRoot);
  const digest = createHash('sha256');
  for (const relativePath of await payloadFiles(root)) {
    const contents = await readFile(path.join(root, relativePath));
    digest.update(`file\0${relativePath}\0${contents.byteLength}\0`);
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

export async function codeServerComponentIdentity({ codeServerRoot, sourceRevision }) {
  const revision = sourceRevision ?? resolveSourceRevision(codeServerRoot);
  if (!/^[0-9a-f]{12}$/.test(revision)) {
    throw new Error(`Invalid code-server source revision: ${revision}`);
  }
  const payloadFingerprint = await codeServerNodePayloadFingerprint(codeServerRoot);
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
  if (!/^(darwin-arm64|linux-(x64|arm64)|windows-(x64|arm64))$/.test(platform)) {
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
  let codeServerRoot = 'code-server';
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
