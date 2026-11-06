#!/usr/bin/env node
// The GBots component's immutable identity: `<runtimeVersion>-<12 hex of its build inputs>`, so a
// release rebuilds the component only when the pinned upstream, the pinned Node or the build
// scripts change, and otherwise reuses the published archive (like the code-server component).
// Usage: component-identity.mjs [--platform <os-arch>] [--github-output]
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const INPUTS = ['upstream.json', 'build-runtime.sh', 'rename.mjs', 'build-component.sh', 'node.entitlements'];

export function gbotsComponentIdentity(platform) {
  const hash = createHash('sha256');
  for (const name of INPUTS) {
    hash.update(`${name}\0`);
    hash.update(readFileSync(join(here, name)));
    hash.update('\0');
  }
  const { runtimeVersion } = JSON.parse(readFileSync(join(here, 'upstream.json'), 'utf8'));
  const componentVersion = `${runtimeVersion}-${hash.digest('hex').slice(0, 12)}`;
  return {
    componentVersion,
    downloadTag: `gbots-${componentVersion}`,
    archiveName: platform ? `gbots-${componentVersion}-${platform}.tar.gz` : undefined,
    artifactName: platform ? `gbots-component-${platform}` : undefined,
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const args = process.argv.slice(2);
  const platformIndex = args.indexOf('--platform');
  const platform = platformIndex === -1 ? undefined : args[platformIndex + 1];
  const identity = gbotsComponentIdentity(platform);
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
