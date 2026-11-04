#!/usr/bin/env node
/*
 * CDXC:Release 2026-10-27 WHY:
 * AppImage managers (AppManager, AppImageUpdate, Gear Lever) find an image's
 * updates through the update information embedded in the runtime's reserved
 * `.upd_info` ELF section, and AppManager shows it as the Update Link
 * (GitHub issue #320). appimagetool writes that section with `-u`, but it
 * ships only as a rolling "continuous" build; the runtime we embed is pinned by
 * digest, so this writes the string into that same section of our own copy
 * instead, zero-padded, exactly as appimagetool does.
 * SEE-ALSO: tooling/release-gpui/linux-appimage.sh (the caller and the zsync file).
 *
 * Usage: node appimage-update-info.mjs write <runtime-file> <update-information>
 *        node appimage-update-info.mjs read <appimage-or-runtime>
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const UPDATE_INFORMATION_SECTION = '.upd_info';

/* The section's file offset and size in a little-endian ELF64 file. */
export function findElfSection(bytes, name) {
  const buffer = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (buffer.readUInt32BE(0) !== 0x7f454c46 || buffer[4] !== 2 || buffer[5] !== 1) {
    throw new Error('Not a little-endian ELF64 file');
  }
  const sectionHeaders = Number(buffer.readBigUInt64LE(0x28));
  const entrySize = buffer.readUInt16LE(0x3a);
  const count = buffer.readUInt16LE(0x3c);
  const namesIndex = buffer.readUInt16LE(0x3e);
  const header = (index) => sectionHeaders + index * entrySize;
  const namesOffset = Number(buffer.readBigUInt64LE(header(namesIndex) + 0x18));
  for (let index = 0; index < count; index += 1) {
    const nameStart = namesOffset + buffer.readUInt32LE(header(index));
    const nameEnd = buffer.indexOf(0, nameStart);
    if (buffer.toString('latin1', nameStart, nameEnd) !== name) continue;
    return {
      offset: Number(buffer.readBigUInt64LE(header(index) + 0x18)),
      size: Number(buffer.readBigUInt64LE(header(index) + 0x20)),
    };
  }
  throw new Error(`The ELF file has no ${name} section`);
}

/* The update information an image or runtime carries ('' when the section is empty). */
export function readUpdateInformation(bytes) {
  const { offset, size } = findElfSection(bytes, UPDATE_INFORMATION_SECTION);
  const section = Buffer.from(bytes.buffer, bytes.byteOffset + offset, size);
  const end = section.indexOf(0);
  return section.toString('utf8', 0, end < 0 ? size : end);
}

/* A copy of `bytes` whose `.upd_info` section holds `updateInformation`, zero-padded. */
export function withUpdateInformation(bytes, updateInformation) {
  const text = Buffer.from(updateInformation, 'utf8');
  const { offset, size } = findElfSection(bytes, UPDATE_INFORMATION_SECTION);
  /* The runtime reads the section as a C string, so one zero byte must remain. */
  if (text.length >= size) {
    throw new Error(`Update information is ${text.length} bytes; ${UPDATE_INFORMATION_SECTION} holds ${size - 1}`);
  }
  const patched = Buffer.from(bytes);
  patched.fill(0, offset, offset + size);
  text.copy(patched, offset);
  return patched;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [command, file, updateInformation] = process.argv.slice(2);
  if (command === 'write' && file && updateInformation) {
    writeFileSync(file, withUpdateInformation(readFileSync(file), updateInformation));
  } else if (command === 'read' && file) {
    process.stdout.write(`${readUpdateInformation(readFileSync(file))}\n`);
  } else {
    console.error('Usage: node appimage-update-info.mjs write <runtime-file> <update-information>');
    console.error('       node appimage-update-info.mjs read <appimage-or-runtime>');
    process.exit(2);
  }
}
