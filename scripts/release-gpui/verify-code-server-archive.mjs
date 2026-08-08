import { createHash } from 'node:crypto';
import { basename, resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import { readFile as readFileAsync } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';

import { codeServerComponentNames } from './code-server-component-identity.mjs';

export const CODE_SERVER_ARCHIVE_CONTRACT = Object.freeze(
  JSON.parse(readFileSync(new URL('../../shared/code-server-archive-contract.json', import.meta.url), 'utf8'))
);
export const CODE_SERVER_REQUIRED_ARCHIVE_ENTRIES = Object.freeze([...CODE_SERVER_ARCHIVE_CONTRACT.requiredEntries]);

const allowedLocalPaxKeys = new Set(['path', 'linkpath']);
const maximumMetadataSize = 1024 * 1024;
const sha256Pattern = /^[0-9a-f]{64}$/;
const utf8Decoder = new TextDecoder('utf-8', { fatal: true });

function platformContractEntries(field, platform) {
  const entries = CODE_SERVER_ARCHIVE_CONTRACT[field]?.[platform];
  if (!Array.isArray(entries)) {
    throw new Error(`Unsupported code-server archive platform: ${platform}`);
  }
  return entries;
}

export function codeServerRequiredArchiveEntries(platform) {
  return [
    ...CODE_SERVER_ARCHIVE_CONTRACT.requiredEntries,
    ...platformContractEntries('requiredEntriesByPlatform', platform),
  ];
}

export function codeServerExecutableArchiveEntries(platform) {
  return [
    ...CODE_SERVER_ARCHIVE_CONTRACT.executableEntries,
    ...platformContractEntries('executableEntriesByPlatform', platform),
  ];
}

function readTarString(buffer, offset, length) {
  const limit = offset + length;
  const end = buffer.indexOf(0, offset);
  const value = buffer.subarray(offset, end === -1 || end > limit ? limit : end);
  try {
    return utf8Decoder.decode(value);
  } catch {
    throw new Error('Invalid UTF-8 in code-server tar header');
  }
}

function readTarOctal(buffer, offset, length, label) {
  const value = readTarString(buffer, offset, length).trim();
  if (!/^[0-7]+$/.test(value)) throw new Error(`Invalid tar ${label}`);
  return Number.parseInt(value, 8);
}

function normalizeTarEntry(name, { allowRoot = false } = {}) {
  let normalized = name;
  while (normalized.startsWith('./')) normalized = normalized.slice(2);
  while (normalized.endsWith('/')) normalized = normalized.slice(0, -1);
  if (!normalized && allowRoot) return null;
  const segments = normalized.split('/');
  if (
    !normalized ||
    normalized.startsWith('/') ||
    /^[A-Za-z]:/.test(normalized) ||
    normalized.includes('\\') ||
    segments.some((segment) => segment === '' || segment === '.' || segment === '..') ||
    [...normalized].some((character) => character < ' ' || character === '\u007f')
  ) {
    throw new Error(`Unsafe code-server archive entry: ${JSON.stringify(name)}`);
  }
  return normalized;
}

function registerArchiveEntry(seen, normalized, kind) {
  if (seen.has(normalized)) throw new Error(`Duplicate code-server archive entry: ${normalized}`);
  const segments = normalized.split('/');
  for (let index = 1; index < segments.length; index += 1) {
    const parent = segments.slice(0, index).join('/');
    if (seen.has(parent) && seen.get(parent) !== 'directory') {
      throw new Error(`Conflicting code-server archive entries: ${parent} and ${normalized}`);
    }
  }
  for (const existing of seen.keys()) {
    if (kind !== 'directory' && existing.startsWith(`${normalized}/`)) {
      throw new Error(`Conflicting code-server archive entries: ${normalized} and ${existing}`);
    }
  }
  seen.set(normalized, kind);
}

function readTarMetadataUtf8(contents, label) {
  try {
    return utf8Decoder.decode(contents);
  } catch {
    throw new Error(`Invalid UTF-8 in tar ${label}`);
  }
}

function readGnuTarMetadataPath(contents, label) {
  if (contents.length < 2 || contents[contents.length - 1] !== 0 || contents.subarray(0, -1).includes(0)) {
    throw new Error(`Malformed GNU tar ${label}`);
  }
  return readTarMetadataUtf8(contents.subarray(0, -1), label);
}

function parsePaxMetadata(contents) {
  const values = new Map();
  for (let offset = 0; offset < contents.length;) {
    const space = contents.indexOf(0x20, offset);
    if (space === -1) throw new Error('Malformed code-server PAX header');
    const lengthText = contents.toString('ascii', offset, space);
    if (!/^[1-9][0-9]*$/.test(lengthText)) throw new Error('Malformed code-server PAX header');
    const length = Number.parseInt(lengthText, 10);
    const end = offset + length;
    if (!Number.isSafeInteger(length) || end > contents.length || contents[end - 1] !== 0x0a) {
      throw new Error('Malformed code-server PAX header');
    }
    const record = readTarMetadataUtf8(contents.subarray(space + 1, end - 1), 'PAX record');
    const separator = record.indexOf('=');
    if (separator <= 0) throw new Error('Malformed code-server PAX header');
    const key = record.slice(0, separator);
    if (!allowedLocalPaxKeys.has(key)) throw new Error(`Unsupported code-server PAX field: ${key}`);
    if (values.has(key)) throw new Error(`Duplicate code-server PAX field: ${key}`);
    const value = record.slice(separator + 1);
    if (!value) throw new Error(`Malformed empty code-server PAX field: ${key}`);
    values.set(key, value);
    offset = end;
  }
  if (values.size === 0) throw new Error('Malformed empty code-server PAX header');
  return values;
}

function normalizedLinkTarget(entryPath, linkName, hardlink) {
  if (
    !linkName ||
    linkName.startsWith('/') ||
    /^[A-Za-z]:/.test(linkName) ||
    linkName.includes('\\') ||
    [...linkName].some((character) => character < ' ' || character === '\u007f')
  ) {
    throw new Error(`Unsafe code-server archive link target: ${JSON.stringify(linkName)}`);
  }
  if (hardlink) return normalizeTarEntry(linkName);
  const resolved = entryPath.includes('/') ? entryPath.split('/').slice(0, -1) : [];
  for (const segment of linkName.split('/')) {
    if (!segment || segment === '.') continue;
    if (segment === '..') {
      if (resolved.length === 0) {
        throw new Error(`Unsafe code-server archive link target: ${JSON.stringify(linkName)}`);
      }
      resolved.pop();
    } else {
      resolved.push(segment);
    }
  }
  if (resolved.length === 0) {
    throw new Error(`Unsafe code-server archive link target: ${JSON.stringify(linkName)}`);
  }
  return normalizeTarEntry(resolved.join('/'));
}

function tarHeaderChecksum(buffer, offset) {
  let checksum = 0;
  for (let index = 0; index < 512; index += 1) {
    checksum += index >= 148 && index < 156 ? 32 : buffer[offset + index];
  }
  return checksum;
}

export function inspectCodeServerTarGz(bytes) {
  let tar;
  try {
    tar = gunzipSync(bytes);
  } catch (error) {
    throw new Error(`Invalid code-server gzip archive: ${error.message}`);
  }

  const entries = new Map();
  const seen = new Map();
  const links = new Map();
  let pendingLongName;
  let pendingLongLink;
  let pendingPax;
  for (let offset = 0; offset + 512 <= tar.length;) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;

    const storedChecksum = readTarOctal(tar, offset + 148, 8, 'header checksum');
    if (storedChecksum !== tarHeaderChecksum(tar, offset)) {
      throw new Error('Invalid code-server tar header checksum');
    }

    const name = readTarString(tar, offset, 100);
    const prefix = readTarString(tar, offset + 345, 155);
    const archiveName = prefix ? `${prefix}/${name}` : name;
    const mode = readTarOctal(tar, offset + 100, 8, 'entry mode');
    const size = readTarOctal(tar, offset + 124, 12, 'entry size');
    const type = String.fromCharCode(tar[offset + 156] || 48);
    const dataOffset = offset + 512;
    const nextOffset = dataOffset + Math.ceil(size / 512) * 512;
    if (nextOffset > tar.length) throw new Error(`Truncated code-server archive entry: ${archiveName}`);
    const contents = tar.subarray(dataOffset, dataOffset + size);
    const normalizedArchiveName = normalizeTarEntry(archiveName, { allowRoot: type === '5' });

    if (type === 'L' || type === 'K' || type === 'x') {
      if (size === 0 || size > maximumMetadataSize) {
        throw new Error(`Malformed code-server tar metadata size for ${normalizedArchiveName}`);
      }
      if (type === 'L') {
        if (pendingLongName !== undefined) throw new Error('Duplicate GNU tar long-name header');
        if (pendingPax?.has('path')) throw new Error('Conflicting code-server tar path metadata');
        if (archiveName !== '././@LongLink') throw new Error('Malformed GNU tar long-name header');
        pendingLongName = readGnuTarMetadataPath(contents, 'long name');
      } else if (type === 'K') {
        if (pendingLongLink !== undefined) throw new Error('Duplicate GNU tar long-link header');
        if (pendingPax?.has('linkpath')) throw new Error('Conflicting code-server tar link metadata');
        if (archiveName !== '././@LongLink') throw new Error('Malformed GNU tar long-link header');
        pendingLongLink = readGnuTarMetadataPath(contents, 'long link');
      } else {
        if (pendingPax !== undefined) throw new Error('Duplicate local PAX tar header');
        if (!normalizedArchiveName.split('/').includes('PaxHeader')) {
          throw new Error('Malformed local PAX code-server archive header');
        }
        pendingPax = parsePaxMetadata(contents);
        if (pendingPax.has('path') && pendingLongName !== undefined) {
          throw new Error('Conflicting code-server tar path metadata');
        }
        if (pendingPax.has('linkpath') && pendingLongLink !== undefined) {
          throw new Error('Conflicting code-server tar link metadata');
        }
      }
      offset = nextOffset;
      continue;
    }
    if (type === 'g') throw new Error('Unsupported global PAX code-server archive header');

    if ((pendingPax?.has('linkpath') || pendingLongLink !== undefined) && type !== '1' && type !== '2') {
      throw new Error('Code-server tar link metadata does not describe a link entry');
    }

    const effectiveName = pendingPax?.get('path') ?? pendingLongName ?? archiveName;
    const effectiveLinkName = pendingPax?.get('linkpath') ?? pendingLongLink ?? readTarString(tar, offset + 157, 100);
    pendingLongName = undefined;
    pendingLongLink = undefined;
    pendingPax = undefined;
    const normalized =
      effectiveName === archiveName
        ? normalizedArchiveName
        : normalizeTarEntry(effectiveName, { allowRoot: type === '5' });
    if (normalized === null) {
      if (type !== '5' || size !== 0) throw new Error('Malformed code-server archive root entry');
      offset = nextOffset;
      continue;
    }
    if (type === '0') {
      registerArchiveEntry(seen, normalized, 'file');
      entries.set(normalized, { contents, mode, size });
    } else if (type === '5') {
      if (size !== 0) throw new Error(`Malformed code-server directory entry: ${normalized}`);
      registerArchiveEntry(seen, normalized, 'directory');
    } else if (type === '1' || type === '2') {
      if (size !== 0) throw new Error(`Malformed code-server link entry: ${normalized}`);
      const kind = type === '1' ? 'hardlink' : 'symlink';
      registerArchiveEntry(seen, normalized, kind);
      links.set(normalized, {
        kind,
        target: normalizedLinkTarget(normalized, effectiveLinkName, type === '1'),
      });
    } else {
      throw new Error(`Unsupported code-server archive entry type ${JSON.stringify(type)} for ${normalized}`);
    }
    offset = nextOffset;
  }
  if (pendingLongName !== undefined || pendingLongLink !== undefined || pendingPax !== undefined) {
    throw new Error('Dangling code-server tar metadata header');
  }
  for (const [path, link] of links) {
    let target = link.target;
    const visited = new Set([path]);
    while (links.has(target)) {
      if (visited.has(target)) throw new Error(`Cyclic code-server archive link: ${path}`);
      visited.add(target);
      target = links.get(target).target;
    }
    const targetKind = seen.get(target);
    if (!targetKind || (link.kind === 'hardlink' && targetKind !== 'file')) {
      throw new Error(`Unsafe or dangling code-server archive link: ${path} -> ${link.target}`);
    }
  }
  return entries;
}

export function parseCodeServerChecksumSidecar(contents, expectedArchiveName) {
  const match = /^([0-9a-f]{64})  ([^\r\n]+)\r?\n?$/.exec(contents);
  if (!match || !sha256Pattern.test(match[1])) {
    throw new Error('Malformed code-server checksum sidecar');
  }
  if (match[2] !== expectedArchiveName) {
    throw new Error(`Code-server checksum filename mismatch: expected ${expectedArchiveName}, got ${match[2]}`);
  }
  return match[1];
}

export async function verifyCodeServerArchive({ archivePath, componentVersion, platform, sidecarPath }) {
  const expectedArchiveName = codeServerComponentNames(componentVersion, platform).archiveName;
  if (basename(archivePath) !== expectedArchiveName) {
    throw new Error(
      `Code-server archive identity mismatch: expected ${expectedArchiveName}, got ${basename(archivePath)}`
    );
  }

  const resolvedSidecarPath = sidecarPath ?? `${archivePath}.sha256`;
  const [archiveBytes, sidecarContents] = await Promise.all([
    readFileAsync(archivePath),
    readFileAsync(resolvedSidecarPath, 'utf8'),
  ]);
  const expectedSha256 = parseCodeServerChecksumSidecar(sidecarContents, expectedArchiveName);
  const actualSha256 = createHash('sha256').update(archiveBytes).digest('hex');
  if (actualSha256 !== expectedSha256) {
    throw new Error(`Code-server archive checksum mismatch for ${expectedArchiveName}`);
  }

  const entries = inspectCodeServerTarGz(archiveBytes);
  const requiredEntries = codeServerRequiredArchiveEntries(platform);
  const executableEntries = new Set(codeServerExecutableArchiveEntries(platform));
  for (const requiredEntry of requiredEntries) {
    const entry = entries.get(requiredEntry);
    if (!entry || entry.size === 0) {
      throw new Error(`Code-server archive is missing required payload: ${requiredEntry}`);
    }
    if (executableEntries.has(requiredEntry) && (entry.mode & 0o111) === 0) {
      throw new Error(`Code-server archive payload is not executable: ${requiredEntry}`);
    }
  }

  const readinessPayload = entries.get(CODE_SERVER_ARCHIVE_CONTRACT.readinessEntry).contents.toString('utf8');
  if (!readinessPayload.includes(CODE_SERVER_ARCHIVE_CONTRACT.readinessSignal)) {
    throw new Error(
      `Code-server archive lacks compiled ${CODE_SERVER_ARCHIVE_CONTRACT.readinessSignal} readiness signal`
    );
  }

  return { actualSha256, expectedArchiveName };
}

function readOption(args, name) {
  const index = args.indexOf(name);
  if (index === -1 || !args[index + 1]) throw new Error(`Missing required option ${name}`);
  return args[index + 1];
}

async function main() {
  const args = process.argv.slice(2);
  const result = await verifyCodeServerArchive({
    archivePath: resolve(readOption(args, '--archive')),
    componentVersion: readOption(args, '--version'),
    platform: readOption(args, '--platform'),
  });
  process.stdout.write(`Verified ${result.expectedArchiveName} (${result.actualSha256})\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
