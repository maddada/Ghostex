import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  closeSync,
  existsSync,
  fstatSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, test } from 'vitest';

import { signedDigest, withSignature } from './appimage-signature.mjs';
import { findElfSection, readUpdateInformation, withUpdateInformation } from './appimage-update-info.mjs';
import { customerDownloadEntries, renderCustomerDownloadNotes } from './customer-downloads.mjs';
import { productDefinition } from './product-inputs.mjs';
import { PUBLISH_STAGES, stageArtifactNames } from './publish-stage.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const appRun = path.join(repoRoot, 'tooling/packaging/appimage/AppRun');

describe('the Linux AppImage release product', () => {
  test('ships in each architecture stage and the Linux line of the download block', () => {
    expect(PUBLISH_STAGES.linux).toContain('linux-appimage-x64');
    expect(PUBLISH_STAGES['linux-arm64']).toContain('linux-appimage-arm64');
    const plan = { expectedPlatforms: ['gxserver-linux-arm64', 'linux-appimage-arm64'] };
    /* The names the Linux job uploads (release-gpui-linux.yml). */
    expect(stageArtifactNames({ plan, stage: 'linux-arm64' })).toEqual([
      'release-plan',
      'release-gxserver-linux-arm64',
      'release-linux-appimage-arm64',
      'release-provenance-gxserver-linux-arm64',
      'release-provenance-linux-appimage-arm64',
    ]);

    const assets = ['linux-appimage-x64', 'linux-appimage-arm64'].flatMap((id) =>
      productDefinition(id).artifacts('7.8.0')
    );
    const groups = customerDownloadEntries('7.8.0', assets);
    expect(groups.map((group) => [group.title, group.line, group.downloads.map((d) => d.label)])).toEqual([
      ['Linux x64', 2, ['AppImage', 'AUR (ghostex-bin)']],
      ['Linux ARM64', 2, ['AppImage']],
    ]);
    /* The zsync files are published for AppImage managers, not linked for people. */
    expect(assets.filter((name) => name.endsWith('.zsync'))).toHaveLength(2);
    expect(renderCustomerDownloadNotes('7.8.0', assets)).not.toContain('.zsync');
  });

  test('names each architecture its own zsync file in the update information', () => {
    const script = readFileSync(path.join(repoRoot, 'tooling/release-gpui/linux-appimage.sh'), 'utf8');
    expect(script).toContain(
      'APPIMAGE_UPDATE_INFORMATION="gh-releases-zsync|maddada|Ghostex|latest|ghostex-*-linux-${LINUX_ARCH}.AppImage.zsync"'
    );
    expect(script).toContain('X-AppImage-Homepage=');
  });

  test('pins the type2 runtime by digest for both architectures', () => {
    const script = readFileSync(path.join(repoRoot, 'tooling/release-gpui/linux-appimage.sh'), 'utf8');
    expect(script).toMatch(/^APPIMAGE_RUNTIME_RELEASE="\d{8}"$/mu);
    for (const name of ['runtime-x86_64', 'runtime-aarch64']) {
      expect(script).toMatch(
        new RegExp(`APPIMAGE_RUNTIME_NAME="${name}"\\n\\s*APPIMAGE_RUNTIME_SHA256="[0-9a-f]{64}"`, 'u')
      );
    }
  });
});

/* A minimal little-endian ELF64 file: a header, `.upd_info` (1024 zero bytes) and the section name table. */
function elfWithUpdateInformationSection() {
  const names = Buffer.from('\0.upd_info\0.shstrtab\0', 'latin1');
  const updInfoOffset = 64;
  const namesOffset = updInfoOffset + 1024;
  const headersOffset = namesOffset + names.length;
  const file = Buffer.alloc(headersOffset + 3 * 64);
  Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]).copy(file, 0);
  file.writeBigUInt64LE(BigInt(headersOffset), 0x28);
  file.writeUInt16LE(64, 0x3a);
  file.writeUInt16LE(3, 0x3c);
  file.writeUInt16LE(2, 0x3e);
  const section = (index, nameOffset, offset, size) => {
    const at = headersOffset + index * 64;
    file.writeUInt32LE(nameOffset, at);
    file.writeBigUInt64LE(BigInt(offset), at + 0x18);
    file.writeBigUInt64LE(BigInt(size), at + 0x20);
  };
  section(1, 1, updInfoOffset, 1024);
  section(2, 11, namesOffset, names.length);
  names.copy(file, namesOffset);
  return file;
}

/* An ELF64 file with a 64-byte `.sha256_sig`, a 128-byte `.sig_key` and payload bytes around them. */
function elfWithSignatureSections() {
  const names = Buffer.from('\0.sha256_sig\0.sig_key\0.shstrtab\0', 'latin1');
  const sigOffset = 64;
  const keyOffset = sigOffset + 64;
  const namesOffset = keyOffset + 128;
  const headersOffset = namesOffset + names.length;
  const file = Buffer.alloc(headersOffset + 4 * 64 + 256, 0x5a);
  file.fill(0, 0, 64);
  Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]).copy(file, 0);
  file.writeBigUInt64LE(BigInt(headersOffset), 0x28);
  file.writeUInt16LE(64, 0x3a);
  file.writeUInt16LE(4, 0x3c);
  file.writeUInt16LE(3, 0x3e);
  file.fill(0, headersOffset, headersOffset + 4 * 64);
  const section = (index, nameOffset, offset, size) => {
    const at = headersOffset + index * 64;
    file.writeUInt32LE(nameOffset, at);
    file.writeBigUInt64LE(BigInt(offset), at + 0x18);
    file.writeBigUInt64LE(BigInt(size), at + 0x20);
  };
  file.fill(0, sigOffset, namesOffset);
  section(1, 1, sigOffset, 64);
  section(2, 13, keyOffset, 128);
  section(3, 22, namesOffset, names.length);
  names.copy(file, namesOffset);
  return file;
}

/* The v4 OpenPGP fingerprint of the first public-key packet in an armored key. */
function armoredKeyFingerprint(armored) {
  const body = armored
    .split('\n')
    .filter((line) => line && !line.startsWith('-----') && !line.startsWith('=') && !line.includes(':'))
    .join('');
  const packets = Buffer.from(body, 'base64');
  const tag = packets[0];
  /* New-format packet (0xC6) or old-format tag 6 with a 1- or 2-byte length. */
  let header;
  let length;
  if ((tag & 0x40) !== 0) {
    expect(tag & 0x3f).toBe(6);
    const first = packets[1];
    if (first < 192) [header, length] = [2, first];
    else [header, length] = [3, ((first - 192) << 8) + packets[2] + 192];
  } else {
    expect((tag >> 2) & 0x0f).toBe(6);
    if ((tag & 3) === 0) [header, length] = [2, packets[1]];
    else [header, length] = [3, packets.readUInt16BE(1)];
  }
  const key = packets.subarray(header, header + length);
  const prefix = Buffer.from([0x99, length >> 8, length & 0xff]);
  return createHash('sha1').update(prefix).update(key).digest('hex').toUpperCase();
}

describe('the AppImage signature', () => {
  test('signs the image with both signature sections read as zeros, like appimagetool', () => {
    const image = elfWithSignatureSections();
    const digest = signedDigest(image);
    expect(digest).toBe(createHash('sha256').update(image).digest('hex'));
    const signed = withSignature(image, 'SIGNATURE', 'PUBLIC KEY');
    /* What a verifier re-hashes after the signature is in place. */
    expect(signedDigest(signed)).toBe(digest);
    expect(signed.subarray(64, 64 + 9).toString('latin1')).toBe('SIGNATURE');
    expect(signed.subarray(128, 128 + 10).toString('latin1')).toBe('PUBLIC KEY');
    expect(signed.subarray(64 + 9, 128).every((byte) => byte === 0)).toBe(true);
    /* Any other byte changed is a different digest, so a tampered image fails verification. */
    const tampered = Buffer.from(signed);
    tampered[tampered.length - 1] ^= 1;
    expect(signedDigest(tampered)).not.toBe(digest);
    expect(() => withSignature(image, 'x'.repeat(64), 'key')).toThrow(/holds 63/u);
  });

  test('embeds the committed public key, which is the key the release pins', () => {
    const script = readFileSync(path.join(repoRoot, 'tooling/release-gpui/linux-appimage.sh'), 'utf8');
    const pinned = /^APPIMAGE_SIGNING_FINGERPRINT="([0-9A-F]{40})"$/mu.exec(script)?.[1];
    const publicKey = readFileSync(
      path.join(repoRoot, 'tooling/packaging/appimage/ghostex-appimage-signing.asc'),
      'utf8'
    );
    expect(publicKey).toMatch(/^-----BEGIN PGP PUBLIC KEY BLOCK-----$/mu);
    expect(publicKey).not.toMatch(/PRIVATE/u);
    expect(armoredKeyFingerprint(publicKey)).toBe(pinned);
    /* It has to fit the runtime's 8192-byte .sig_key section. */
    expect(Buffer.byteLength(publicKey)).toBeLessThan(8192);
  });
});

describe('the AppImage update information', () => {
  test('is written into .upd_info zero-padded and leaves every other byte alone', () => {
    const original = elfWithUpdateInformationSection();
    original.fill(0xaa, 64, 64 + 1024);
    const info = 'gh-releases-zsync|maddada|Ghostex|latest|ghostex-*-linux-x64.AppImage.zsync';
    const patched = withUpdateInformation(original, info);
    const { offset, size } = findElfSection(patched, '.upd_info');
    expect({ offset, size }).toEqual({ offset: 64, size: 1024 });
    expect(patched.subarray(offset, offset + info.length).toString('utf8')).toBe(info);
    expect(readUpdateInformation(patched)).toBe(info);
    expect(patched.subarray(offset + info.length, offset + size).every((byte) => byte === 0)).toBe(true);
    expect(patched.subarray(0, offset).equals(original.subarray(0, offset))).toBe(true);
    expect(patched.subarray(offset + size).equals(original.subarray(offset + size))).toBe(true);
  });

  test('refuses text the section cannot hold with its terminating zero', () => {
    expect(() => withUpdateInformation(elfWithUpdateInformationSection(), 'x'.repeat(1024))).toThrow(/holds 1023/u);
    expect(() => findElfSection(Buffer.alloc(64), '.upd_info')).toThrow(/ELF64/u);
  });
});

/*
 * AppRun is a Linux launcher (GNU cp, /proc); the gates job runs on macOS, so
 * these run wherever the suite runs on Linux.
 */
describe.skipIf(process.platform !== 'linux')('the AppImage launcher', () => {
  let scratch = '';
  afterEach(() => {
    if (scratch) rmSync(scratch, { force: true, recursive: true });
    scratch = '';
  });

  /* A fake AppDir whose Ghostex records where it ran and with which arguments. */
  function appDir(name, payloadId, files) {
    const root = path.join(scratch, name);
    mkdirSync(path.join(root, 'opt/ghostex'), { recursive: true });
    writeFileSync(path.join(root, 'AppRun'), readFileSync(appRun));
    chmodSync(path.join(root, 'AppRun'), 0o755);
    writeFileSync(path.join(root, '.ghostex-appimage'), `${payloadId}\n`);
    writeFileSync(
      path.join(root, 'opt/ghostex/Ghostex'),
      '#!/usr/bin/env bash\nprintf "%s\\n" "$0" "$@" >"$GHOSTEX_TEST_LAUNCH"\nls /proc/$$/fd >"$GHOSTEX_TEST_FDS"\n'
    );
    chmodSync(path.join(root, 'opt/ghostex/Ghostex'), 0o755);
    for (const [relative, content] of Object.entries(files)) {
      mkdirSync(path.dirname(path.join(root, 'opt/ghostex', relative)), { recursive: true });
      writeFileSync(path.join(root, 'opt/ghostex', relative), content);
    }
    return root;
  }

  function launch(root, ...args) {
    const result = spawnSync(path.join(root, 'AppRun'), args, {
      encoding: 'utf8',
      env: {
        ...process.env,
        GHOSTEX_TEST_FDS: path.join(scratch, 'fds'),
        GHOSTEX_TEST_LAUNCH: path.join(scratch, 'launch'),
        HOME: path.join(scratch, 'home'),
        XDG_DATA_HOME: path.join(scratch, 'data'),
      },
    });
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    return readFileSync(path.join(scratch, 'launch'), 'utf8').trim().split('\n');
  }

  test('copies the payload into the data folder once and starts it from there', () => {
    scratch = mkdtempSync(path.join(tmpdir(), 'ghostex-apprun-'));
    const install = path.join(scratch, 'data/ghostex/appimage');
    const first = appDir('v1', '7.8.0-aaaa', { 'gxserver/bin/gxserver': 'v1', 'resources/old.json': 'old' });
    expect(launch(first, '--flag', 'two words')).toEqual([path.join(install, 'Ghostex'), '--flag', 'two words']);
    expect(readFileSync(path.join(install, 'gxserver/bin/gxserver'), 'utf8')).toBe('v1');
    expect(readFileSync(path.join(install, '.ghostex-appimage'), 'utf8')).toBe('7.8.0-aaaa\n');
    /* Only stdio reaches the app, so the AppImage's keep-alive pipe cannot hold the mount. */
    expect(readFileSync(path.join(scratch, 'fds'), 'utf8').trim().split('\n').sort()).toEqual(['0', '1', '2', '255']);

    const stampTime = statSync(path.join(install, '.ghostex-appimage')).mtimeMs;
    launch(first);
    expect(statSync(path.join(install, '.ghostex-appimage')).mtimeMs).toBe(stampTime);
  });

  test('a newer image replaces changed files by new inodes and drops removed ones', () => {
    scratch = mkdtempSync(path.join(tmpdir(), 'ghostex-apprun-'));
    const install = path.join(scratch, 'data/ghostex/appimage');
    launch(appDir('v1', '7.8.0-aaaa', { 'gxserver/bin/gxserver': 'v1', 'resources/old.json': 'old' }));
    /* Stands in for a gxserver still running the previous version. */
    const running = openSync(path.join(install, 'gxserver/bin/gxserver'), 'r');
    try {
      launch(appDir('v2', '7.9.0-bbbb', { 'gxserver/bin/gxserver': 'v2', 'resources/new.json': 'new' }));
      expect(readFileSync(path.join(install, 'gxserver/bin/gxserver'), 'utf8')).toBe('v2');
      /* It keeps reading its own bytes, as it does through a dpkg upgrade: the file was replaced, not rewritten. */
      expect(readFileSync(running, 'utf8')).toBe('v1');
      expect(fstatSync(running).ino).not.toBe(statSync(path.join(install, 'gxserver/bin/gxserver')).ino);
    } finally {
      closeSync(running);
    }
    expect(existsSync(path.join(install, 'resources/old.json'))).toBe(false);
    expect(readFileSync(path.join(install, 'resources/new.json'), 'utf8')).toBe('new');
    expect(readFileSync(path.join(install, '.ghostex-appimage'), 'utf8')).toBe('7.9.0-bbbb\n');
  });

  test('an interrupted copy is finished on the next launch', () => {
    scratch = mkdtempSync(path.join(tmpdir(), 'ghostex-apprun-'));
    const install = path.join(scratch, 'data/ghostex/appimage');
    mkdirSync(path.join(install, 'gxserver/bin'), { recursive: true });
    writeFileSync(path.join(install, 'gxserver/bin/gxserver'), 'half-copied');
    launch(appDir('v1', '7.8.0-aaaa', { 'gxserver/bin/gxserver': 'v1' }));
    expect(readFileSync(path.join(install, 'gxserver/bin/gxserver'), 'utf8')).toBe('v1');
  });
});
