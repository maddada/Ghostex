import { afterEach, describe, expect, test } from 'vitest';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

import {
  CODE_SERVER_NODE_PAYLOAD_INPUTS,
  codeServerNodePayloadFingerprint,
} from './code-server-component-identity.mjs';

const temporaryDirectories = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, { force: true, recursive: true })));
});

function git(root, ...args) {
  const result = spawnSync('git', ['-C', root, ...args], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || `git ${args.join(' ')} failed`);
  }
}

async function repositoryFixture() {
  const root = await mkdtemp(path.join(tmpdir(), 'ghostex-code-server-identity-'));
  temporaryDirectories.push(root);
  for (const input of CODE_SERVER_NODE_PAYLOAD_INPUTS) {
    const filePath = path.extname(input) ? input : path.join(input, 'fixture.ts');
    await mkdir(path.dirname(path.join(root, filePath)), { recursive: true });
    await writeFile(path.join(root, filePath), `payload for ${filePath}\n`);
  }
  git(root, 'init', '--quiet');
  git(root, 'config', 'user.email', 'release-test@ghostex.local');
  git(root, 'config', 'user.name', 'Ghostex Release Test');
  git(root, 'add', '.');
  git(root, 'commit', '--quiet', '-m', 'fixture');
  return root;
}

describe('code-server component identity', () => {
  test('hashes canonical Git blobs instead of checkout line endings', async () => {
    const root = await repositoryFixture();
    const before = await codeServerNodePayloadFingerprint(root);
    const packageJson = path.join(root, 'package.json');
    await writeFile(packageJson, 'payload for package.json\r\n');

    expect(await codeServerNodePayloadFingerprint(root)).toBe(before);
  });
});
