#!/usr/bin/env node

import { execFile } from 'node:child_process';
import { access, chmod, mkdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import {
  BEADS_PACKAGE_ID,
  BEADS_SOURCE_REVISION,
  BEADS_SOURCE_REVISION_SHORT,
  BEADS_VERSION,
  normalizeBeadsArch,
  normalizeBeadsPlatform,
} from './beads-release.mjs';
import { smokeTestPackagedBeads } from './smoke-test-packaged-beads.mjs';

const execFileAsync = promisify(execFile);

function parseArgs(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index];
    const value = args[index + 1];
    if (!name?.startsWith('--') || !value) throw new Error(`Invalid argument: ${name ?? ''}`);
    options[name.slice(2).replace(/-([a-z])/gu, (_, letter) => letter.toUpperCase())] = value;
  }
  return options;
}

async function git(sourceRoot, args) {
  const result = await execFileAsync('git', ['-C', sourceRoot, ...args], { encoding: 'utf8' });
  return result.stdout.trim();
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const sourceRoot = path.resolve(options.sourceRoot || '');
  const outputPath = path.resolve(options.output || '');
  const platform = normalizeBeadsPlatform(options.platform);
  const arch = normalizeBeadsArch(options.arch);
  if (!options.sourceRoot || !options.output || !platform || !arch) {
    throw new Error(
      'Usage: build-pinned-beads-release.mjs --source-root <path> --platform <linux|darwin> ' +
        '--arch <x64|arm64> --output <path>'
    );
  }

  const hostPlatform = normalizeBeadsPlatform(process.platform);
  const hostArch = normalizeBeadsArch(process.arch);
  if (platform !== hostPlatform || arch !== hostArch) {
    throw new Error(
      `Pinned Beads requires a native CGO build: requested ${platform}/${arch}, ` +
        `runner is ${hostPlatform}/${hostArch}`
    );
  }
  if (!new Set(['linux', 'darwin']).has(platform)) {
    throw new Error(`Unsupported pinned Beads release platform: ${platform}`);
  }

  await access(sourceRoot);
  const revision = await git(sourceRoot, ['rev-parse', 'HEAD']);
  if (revision !== BEADS_SOURCE_REVISION) {
    throw new Error(`Pinned Beads source is ${revision}; expected ${BEADS_SOURCE_REVISION}`);
  }
  const status = await git(sourceRoot, ['status', '--porcelain', '--untracked-files=all']);
  if (status) throw new Error(`Pinned Beads source is dirty:\n${status}`);

  await mkdir(path.dirname(outputPath), { recursive: true });
  await rm(outputPath, { force: true });
  const ldflags = [
    '-s',
    '-w',
    `-X main.Version=${BEADS_VERSION}`,
    `-X main.Build=${BEADS_SOURCE_REVISION_SHORT}`,
    `-X main.Commit=${BEADS_SOURCE_REVISION}`,
    '-X main.Branch=ghostex-schema54',
  ].join(' ');
  await execFileAsync('go', ['build', '-tags', 'gms_pure_go', '-ldflags', ldflags, '-o', outputPath, './cmd/bd'], {
    cwd: sourceRoot,
    env: { ...process.env, CGO_ENABLED: '1', GOTOOLCHAIN: 'auto' },
    maxBuffer: 16 * 1024 * 1024,
  });
  await chmod(outputPath, 0o755);
  await smokeTestPackagedBeads(outputPath);
  console.log(`Built pinned native-CGO Beads ${BEADS_PACKAGE_ID}: ${outputPath}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
