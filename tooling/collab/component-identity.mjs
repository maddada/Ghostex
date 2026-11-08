#!/usr/bin/env node
// The Collab component's immutable identity: `<crate version>-<12 hex of its build inputs>`, so a
// release rebuilds `ghostex-collab` only when its source, its lockfile, its toolchain, the wire
// contract it shares with gxserver, the pinned guest page or the build script change, and
// otherwise reuses the published archives (like the code-server component).
// Usage: component-identity.mjs [--platform <os-arch>] [--github-output]
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const INPUTS = [
  'server/collab/Cargo.toml',
  'server/collab/Cargo.lock',
  'server/collab/rust-toolchain.toml',
  'server/collab/src',
  'packages/gx-protocol/Cargo.toml',
  'packages/gx-protocol/src',
  'tooling/collab/build-component.sh',
  'tooling/collab/collab.entitlements',
];

function files(path) {
  const full = join(root, path);
  if (statSync(full).isDirectory()) {
    return readdirSync(full)
      .filter((name) => !name.startsWith('.'))
      .sort()
      .flatMap((name) => files(join(path, name)));
  }
  return [path];
}

export function collabComponentIdentity(platform) {
  const hash = createHash('sha256');
  for (const file of INPUTS.flatMap(files)) {
    hash.update(`${relative(root, join(root, file)).split('\\').join('/')}\0`);
    hash.update(readFileSync(join(root, file)));
    hash.update('\0');
  }
  const cargo = readFileSync(join(root, 'server/collab/Cargo.toml'), 'utf8');
  const crateVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1];
  if (!crateVersion) throw new Error('server/collab/Cargo.toml has no version');
  const componentVersion = `${crateVersion}-${hash.digest('hex').slice(0, 12)}`;
  return {
    componentVersion,
    downloadTag: `collab-${componentVersion}`,
    archiveName: platform ? `collab-${componentVersion}-${platform}.tar.gz` : undefined,
    artifactName: platform ? `collab-component-${platform}` : undefined,
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const args = process.argv.slice(2);
  const platformIndex = args.indexOf('--platform');
  const platform = platformIndex === -1 ? undefined : args[platformIndex + 1];
  const identity = collabComponentIdentity(platform);
  if (args.includes('--github-output')) {
    process.stdout.write(`component_version=${identity.componentVersion}\n`);
    process.stdout.write(`download_tag=${identity.downloadTag}\n`);
    if (platform) {
      process.stdout.write(`archive_name=${identity.archiveName}\n`);
      process.stdout.write(`artifact_name=${identity.artifactName}\n`);
    }
  } else {
    process.stdout.write(`${identity.componentVersion}\n`);
  }
}
