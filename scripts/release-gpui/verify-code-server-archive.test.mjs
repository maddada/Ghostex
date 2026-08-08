import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { chmod, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { gunzipSync, gzipSync } from 'node:zlib';
import { afterEach, describe, expect, test } from 'vitest';

import {
  codeServerExecutableArchiveEntries,
  codeServerRequiredArchiveEntries,
  verifyCodeServerArchive,
} from './verify-code-server-archive.mjs';

const execFileAsync = promisify(execFile);
const componentVersion = `6b4cfff155c0-p2-${'a'.repeat(64)}`;
const platform = 'linux-x64';
const archiveName = `code-server-${componentVersion}-${platform}.tar.gz`;
const temporaryRoots = [];

afterEach(async () => {
  await Promise.all(temporaryRoots.splice(0).map((root) => rm(root, { force: true, recursive: true })));
});

async function createFixture({ executable = true, fixturePlatform = platform, missingEntry, readiness = true } = {}) {
  const root = await mkdtemp(path.join(tmpdir(), 'ghostex-code-server-archive-'));
  temporaryRoots.push(root);
  const payload = path.join(root, 'payload');
  const executableEntries = new Set(codeServerExecutableArchiveEntries(fixturePlatform));
  for (const entry of codeServerRequiredArchiveEntries(fixturePlatform)) {
    if (entry === missingEntry) continue;
    const target = path.join(payload, entry);
    await mkdir(path.dirname(target), { recursive: true });
    const contents =
      entry === 'out/node/routes/health.js'
        ? readiness
          ? 'exports.health = { promptEditorIpcReady: true };\n'
          : 'exports.health = { status: "alive" };\n'
        : `${entry}\n`;
    await writeFile(target, contents);
    if (executable && executableEntries.has(entry)) await chmod(target, 0o755);
  }

  const fixtureArchiveName = `code-server-${componentVersion}-${fixturePlatform}.tar.gz`;
  const archivePath = path.join(root, fixtureArchiveName);
  await execFileAsync('bash', [path.resolve('scripts/release-gpui/create-deterministic-tar.sh'), payload, archivePath]);
  const digest = createHash('sha256')
    .update(await readFile(archivePath))
    .digest('hex');
  await writeFile(`${archivePath}.sha256`, `${digest}  ${fixtureArchiveName}\n`);
  return { archiveName: fixtureArchiveName, archivePath, digest, payload };
}

function writeTarOctal(header, offset, length, value) {
  header.write(`${value.toString(8).padStart(length - 1, '0')}\0`, offset, length, 'ascii');
}

function createTarEntryHeader({ contents = '', linkName = '', name, type = '0' }) {
  const data = Buffer.from(contents);
  const header = Buffer.alloc(512);
  header.write(name, 0, 100, 'utf8');
  writeTarOctal(header, 100, 8, type === '0' ? 0o644 : 0o755);
  writeTarOctal(header, 108, 8, 0);
  writeTarOctal(header, 116, 8, 0);
  writeTarOctal(header, 124, 12, data.length);
  writeTarOctal(header, 136, 12, 0);
  header.fill(0x20, 148, 156);
  header.write(type, 156, 1, 'ascii');
  header.write(linkName, 157, 100, 'utf8');
  header.write('ustar\0', 257, 6, 'ascii');
  header.write('00', 263, 2, 'ascii');
  const checksum = header.reduce((total, byte) => total + byte, 0);
  header.write(`${checksum.toString(8).padStart(6, '0')}\0 `, 148, 8, 'ascii');
  const paddedData = Buffer.alloc(Math.ceil(data.length / 512) * 512);
  data.copy(paddedData);
  return Buffer.concat([header, paddedData]);
}

function paxRecord(key, value) {
  const payload = `${key}=${value}\n`;
  let length = Buffer.byteLength(payload) + 2;
  for (;;) {
    const record = `${length} ${payload}`;
    const actualLength = Buffer.byteLength(record);
    if (actualLength === length) return record;
    length = actualLength;
  }
}

async function injectTarEntries(fixture, entries, { corruptHeader = false } = {}) {
  const tar = gunzipSync(await readFile(fixture.archivePath));
  let terminator = -1;
  for (let offset = 0; offset + 512 <= tar.length; offset += 512) {
    if (tar.subarray(offset, offset + 512).every((byte) => byte === 0)) {
      terminator = offset;
      break;
    }
  }
  expect(terminator).toBeGreaterThan(-1);
  const injected = Buffer.concat(entries.map((entry) => createTarEntryHeader(entry)));
  if (corruptHeader) injected[0] ^= 1;
  const archiveBytes = gzipSync(Buffer.concat([tar.subarray(0, terminator), injected, tar.subarray(terminator)]));
  await writeFile(fixture.archivePath, archiveBytes);
  const digest = createHash('sha256').update(archiveBytes).digest('hex');
  await writeFile(`${fixture.archivePath}.sha256`, `${digest}  ${fixture.archiveName}\n`);
}

async function injectTarEntry(fixture, entry, options) {
  return injectTarEntries(fixture, [entry], options);
}

describe('code-server release archive verification', () => {
  test('accepts the complete producer payload with a filename-bound checksum', async () => {
    const fixture = await createFixture();
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).resolves.toMatchObject({ actualSha256: fixture.digest, expectedArchiveName: archiveName });
  });

  test('accepts the complete executable Darwin producer payload with compiled readiness', async () => {
    const fixture = await createFixture({ fixturePlatform: 'darwin-arm64' });
    await expect(
      verifyCodeServerArchive({
        archivePath: fixture.archivePath,
        componentVersion,
        platform: 'darwin-arm64',
      })
    ).resolves.toMatchObject({ actualSha256: fixture.digest, expectedArchiveName: fixture.archiveName });
  });

  test('fails closed on checksum mismatch', async () => {
    const fixture = await createFixture();
    await writeFile(`${fixture.archivePath}.sha256`, `${'b'.repeat(64)}  ${archiveName}\n`);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(/checksum mismatch/);
  });

  test.each([
    ['stale p1 identity', '6b4cfff155c0-p1'],
    ['mismatched p2 identity', `6b4cfff155c0-p2-${'b'.repeat(64)}`],
  ])('fails closed on %s before reading or repackaging the archive', async (_label, rejectedVersion) => {
    const fixture = await createFixture();
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion: rejectedVersion, platform })
    ).rejects.toThrow(/archive identity mismatch/);
  });

  test.each([
    ['malformed', `${'a'.repeat(64)}\n`, /Malformed/],
    ['extra record', `${'a'.repeat(64)}  ${archiveName}\n${'b'.repeat(64)}  ${archiveName}\n`, /Malformed/],
    ['wrong filename', `${'a'.repeat(64)}  code-server-wrong-linux-x64.tar.gz\n`, /filename mismatch/],
  ])('fails closed on a %s sidecar', async (_label, sidecar, expectedError) => {
    const fixture = await createFixture();
    await writeFile(`${fixture.archivePath}.sha256`, sidecar);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(expectedError);
  });

  test('fails closed when the sidecar is missing', async () => {
    const fixture = await createFixture();
    await rm(`${fixture.archivePath}.sha256`);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow();
  });

  test.each(['lib/node', 'lib/vscode/node_modules/node-pty/prebuilds/darwin-arm64/pty.node'])(
    'fails closed when required runtime payload %s is missing',
    async (missingEntry) => {
      const fixture = await createFixture({ fixturePlatform: 'darwin-arm64', missingEntry });
      await expect(
        verifyCodeServerArchive({
          archivePath: fixture.archivePath,
          componentVersion,
          platform: 'darwin-arm64',
        })
      ).rejects.toThrow(`Code-server archive is missing required payload: ${missingEntry}`);
    }
  );

  test('fails closed when the compiled readiness signal is missing', async () => {
    const fixture = await createFixture({ fixturePlatform: 'darwin-arm64', readiness: false });
    await expect(
      verifyCodeServerArchive({
        archivePath: fixture.archivePath,
        componentVersion,
        platform: 'darwin-arm64',
      })
    ).rejects.toThrow(/promptEditorIpcReady/);
  });

  test('fails closed when required Darwin executables lose their mode', async () => {
    const fixture = await createFixture({ executable: false, fixturePlatform: 'darwin-arm64' });
    await expect(
      verifyCodeServerArchive({
        archivePath: fixture.archivePath,
        componentVersion,
        platform: 'darwin-arm64',
      })
    ).rejects.toThrow(/not executable/);
  });

  test.each([
    ['the accepted VERIFY8 parent-traversal directory fixture', { name: '../escape/', type: '5' }, /Unsafe/],
    ['a symlink', { name: 'escape-link', type: '2', linkName: '../../escape' }, /Unsafe/],
    ['a hardlink', { name: 'escape-hardlink', type: '1', linkName: '../../escape' }, /Unsafe/],
    ['a character device', { name: 'device', type: '3' }, /Unsupported/],
    ['an otherwise unsupported FIFO', { name: 'fifo', type: '6' }, /Unsupported/],
    ['a malformed PAX header', { name: 'pax', type: 'x', contents: 'bad' }, /Malformed/],
  ])('fails closed on %s before payload reuse', async (_label, entry, expectedError) => {
    const fixture = await createFixture();
    await injectTarEntry(fixture, entry);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(expectedError);
  });

  test.each([
    ['an unknown local PAX key', paxRecord('SCHILY.xattr.user.test', 'value'), /Unsupported.*PAX field/],
    ['a PAX uid override', paxRecord('uid', '1234'), /Unsupported.*PAX field/],
  ])('fails closed on %s', async (_label, paxContents, expectedError) => {
    const fixture = await createFixture();
    await injectTarEntries(fixture, [
      { name: 'PaxHeader/metadata', type: 'x', contents: paxContents },
      { name: 'metadata', contents: 'payload' },
    ]);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(expectedError);
  });

  test('fails closed on a global PAX header', async () => {
    const fixture = await createFixture();
    await injectTarEntries(fixture, [
      { name: 'GlobalHead.0', type: 'g', contents: paxRecord('path', 'metadata') },
      { name: 'metadata', contents: 'payload' },
    ]);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(/global PAX/);
  });

  test.each([
    ['a dangling local PAX header', { name: 'PaxHeader/metadata', type: 'x', contents: paxRecord('path', 'metadata') }],
    ['a dangling GNU long-name header', { name: '././@LongLink', type: 'L', contents: 'metadata\0' }],
    ['malformed GNU long-name contents', { name: '././@LongLink', type: 'L', contents: 'metadata' }],
  ])('fails closed on %s', async (_label, metadata) => {
    const fixture = await createFixture();
    await injectTarEntry(fixture, metadata);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(/Dangling|Malformed GNU/);
  });

  test('accepts canonical GNU long-name and long-link metadata', async () => {
    const fixture = await createFixture();
    const longName = `${'long/'.repeat(24)}payload.js`;
    const longTarget = `${'segment/../'.repeat(12)}lib/node`;
    await injectTarEntries(fixture, [
      { name: '././@LongLink', type: 'L', contents: `${longName}\0` },
      { name: 'placeholder', contents: 'long payload' },
      { name: '././@LongLink', type: 'K', contents: `${longTarget}\0` },
      { name: 'gnu-long-link', type: '2' },
    ]);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).resolves.toMatchObject({ expectedArchiveName: archiveName });
  });

  test.each([
    ['a duplicate entry', { name: 'lib/node', contents: 'duplicate' }, /Duplicate/],
    ['a file that conflicts with an existing child', { name: 'lib', contents: 'conflict' }, /Conflicting/],
  ])('fails closed on %s', async (_label, entry, expectedError) => {
    const fixture = await createFixture();
    await injectTarEntry(fixture, entry);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(expectedError);
  });

  test('fails closed on a malformed tar header even when the sidecar matches', async () => {
    const fixture = await createFixture();
    await injectTarEntry(fixture, { name: 'malformed', contents: 'payload' }, { corruptHeader: true });
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).rejects.toThrow(/header checksum/);
  });

  test.each([
    ['the GNU producer root directory header', { name: './', type: '5' }],
    ['a safe directory header', { name: 'metadata/', type: '5' }],
    ['a safe symlink', { name: 'node-link', type: '2', linkName: 'lib/node' }],
    ['a safe hardlink', { name: 'node-hardlink', type: '1', linkName: 'lib/node' }],
  ])('accepts %s without weakening required payload checks', async (_label, entry) => {
    const fixture = await createFixture();
    await injectTarEntry(fixture, entry);
    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).resolves.toMatchObject({ expectedArchiveName: archiveName });
  });

  test('accepts canonical deterministic producer metadata for a safe long path and long link', async () => {
    const fixture = await createFixture();
    const longDirectory = path.join(fixture.payload, 'node_modules', 'a'.repeat(90), 'b'.repeat(90), 'c'.repeat(90));
    const longPath = path.join(longDirectory, 'payload.js');
    const longLink = path.join(longDirectory, 'safe-node-link');
    await mkdir(longDirectory, { recursive: true });
    await writeFile(longPath, 'long producer path\n');
    await symlink(`${'d'.repeat(90)}/../payload.js`, longLink);
    await execFileAsync(path.resolve('scripts/release-gpui/create-deterministic-tar.sh'), [
      fixture.payload,
      fixture.archivePath,
    ]);
    const archiveBytes = await readFile(fixture.archivePath);
    const digest = createHash('sha256').update(archiveBytes).digest('hex');
    await writeFile(`${fixture.archivePath}.sha256`, `${digest}  ${fixture.archiveName}\n`);

    await expect(
      verifyCodeServerArchive({ archivePath: fixture.archivePath, componentVersion, platform })
    ).resolves.toMatchObject({ actualSha256: digest, expectedArchiveName: archiveName });
  });
});
