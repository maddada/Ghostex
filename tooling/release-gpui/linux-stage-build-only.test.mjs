import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, test } from 'vitest';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const stageScript = readFileSync(path.join(repoRoot, 'tooling/release-gpui/linux-stage.sh'), 'utf8');
const publishScript = path.join(repoRoot, 'tooling/release-gpui/publish-component.mjs');

/*
 * CDXC:Release 2026-10-19 WHY: a build-only run publishes nothing, the
 * components repository included, so every publish-component call linux-stage.sh
 * makes (CEF and code-server) must take the same metadata-only mode.
 */
const publishBlock = stageScript.slice(
  stageScript.indexOf('COMPONENT_PUBLISH_MODE=()'),
  stageScript.indexOf('rm -rf "$PACKAGE_ROOT"')
);

const temporaryRoots = [];
function temporaryRoot() {
  const root = mkdtempSync(path.join(tmpdir(), 'ghostex-linux-stage-test-'));
  temporaryRoots.push(root);
  return root;
}
afterEach(() => {
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true });
});

/* Runs the publish block with `node` replaced by a recorder; returns each publish call's arguments. */
function recordedPublishCalls(buildOnly) {
  const root = temporaryRoot();
  const log = path.join(root, 'calls.log');
  const script = [
    /* No -u: macOS bash 3.2 calls an empty array unbound; the script itself runs on Linux CI (bash 5). */
    'set -eo pipefail',
    `node() { printf '%s\\n' "$*" >> "${log}"; }`,
    'REPO_ROOT=/repo',
    'CEF_COMPONENT_VERSION=1',
    'SEALED_CODE_SERVER_COMPONENT_VERSION=2',
    'CODE_SERVER_ARCHIVE=/archives/code-server.tar.gz',
    publishBlock,
  ].join('\n');
  const env = { ...process.env };
  delete env.GHOSTEX_RELEASE_BUILD_ONLY;
  if (buildOnly !== undefined) env.GHOSTEX_RELEASE_BUILD_ONLY = buildOnly;
  const result = spawnSync('bash', ['-c', script], { encoding: 'utf8', env });
  expect(result.stderr).toBe('');
  expect(result.status).toBe(0);
  return readFileSync(log, 'utf8').trim().split('\n');
}

describe('linux-stage.sh component publishing under a build-only run', () => {
  test('publishes both components metadata-only when GHOSTEX_RELEASE_BUILD_ONLY=1', () => {
    const calls = recordedPublishCalls('1');
    expect(calls).toHaveLength(2);
    expect(calls.map((call) => /--component (\S+)/.exec(call)?.[1])).toEqual(['cef', 'code-server']);
    for (const call of calls) expect(call).toContain('--metadata-only');
  });

  test.each([undefined, '0'])(
    'publishes neither component metadata-only when GHOSTEX_RELEASE_BUILD_ONLY=%s',
    (value) => {
      const calls = recordedPublishCalls(value);
      expect(calls).toHaveLength(2);
      for (const call of calls) expect(call).not.toContain('--metadata-only');
    }
  );

  test('every publish-component call in the script goes through the shared mode', () => {
    const calls = stageScript.match(/publish-component\.mjs" \\\n[^\n]*\n/g) ?? [];
    expect(calls).toHaveLength(2);
    for (const call of calls) expect(call).toContain('"${COMPONENT_PUBLISH_MODE[@]}"');
  });
});

describe('publish-component.mjs --metadata-only with --require-sha256-sidecars', () => {
  test('seals code-server metadata without touching GitHub', () => {
    const root = temporaryRoot();
    const assetDir = path.join(root, 'assets');
    const binDir = path.join(root, 'bin');
    mkdirSync(assetDir);
    mkdirSync(binDir);
    const assetName = 'code-server-abc123-linux-x64.tar.gz';
    const archive = Buffer.from('not a real archive');
    const sha256 = createHash('sha256').update(archive).digest('hex');
    writeFileSync(path.join(assetDir, assetName), archive);
    writeFileSync(path.join(assetDir, `${assetName}.sha256`), `${sha256}  ${assetName}\n`);
    /* A gh that records any call and fails: metadata-only must never invoke it. */
    const ghLog = path.join(root, 'gh.log');
    const gh = path.join(binDir, 'gh');
    writeFileSync(gh, `#!/bin/sh\necho "$@" >> "${ghLog}"\nexit 1\n`);
    chmodSync(gh, 0o755);
    const output = path.join(root, 'components.json');
    const result = spawnSync(
      process.execPath,
      [
        publishScript,
        '--metadata-only',
        '--require-sha256-sidecars',
        '--component',
        'code-server',
        '--version',
        'abc123',
        '--asset-dir',
        assetDir,
        '--output',
        output,
      ],
      {
        encoding: 'utf8',
        env: { ...process.env, PATH: `${binDir}${path.delimiter}${process.env.PATH}`, GITHUB_ACTIONS: 'true' },
      }
    );
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    expect(result.stdout).not.toMatch(/CREATE|UPLOAD/);
    expect(result.stdout).toContain('METADATA-ONLY code-server abc123');
    expect(existsSync(ghLog)).toBe(false);
    const record = JSON.parse(readFileSync(output, 'utf8')).components['code-server'];
    expect(record.platforms['linux-x64']).toMatchObject({
      assetName,
      sha256,
      sha256SidecarName: `${assetName}.sha256`,
    });
  });
});
