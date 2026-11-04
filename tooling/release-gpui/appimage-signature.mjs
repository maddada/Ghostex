#!/usr/bin/env node
/*
 * CDXC:Release 2026-10-27 WHY:
 * The AppImage signature format of `appimagetool --sign`, which AppImageUpdate
 * checks (its `validate` tool, and before every update: the new image must be
 * signed by the same key as the installed one). The signed data is the
 * lowercase hex SHA-256 of the whole image with the `.sha256_sig` and
 * `.sig_key` sections read as zero bytes; an ASCII-armored detached signature
 * of that text goes into `.sha256_sig`, the armored public key into `.sig_key`.
 * gpg makes the signature (linux-appimage.sh); this file only hashes and embeds,
 * so the release needs no rolling appimagetool build.
 * SEE-ALSO: tooling/release-gpui/linux-appimage.sh, tooling/packaging/appimage/ghostex-appimage-signing.asc.
 *
 * Usage: node appimage-signature.mjs digest <appimage>
 *        node appimage-signature.mjs embed <appimage> <signature.asc> <public-key.asc>
 */
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { findElfSection } from './appimage-update-info.mjs';

export const SIGNATURE_SECTION = '.sha256_sig';
export const KEY_SECTION = '.sig_key';

/* The hex digest that is signed: the image with both signature sections read as zeros. */
export function signedDigest(bytes) {
  const copy = Buffer.from(bytes);
  for (const name of [SIGNATURE_SECTION, KEY_SECTION]) {
    const { offset, size } = findElfSection(copy, name);
    copy.fill(0, offset, offset + size);
  }
  return createHash('sha256').update(copy).digest('hex');
}

/* A copy of `bytes` with the signature and public key in their sections, zero-padded. */
export function withSignature(bytes, signature, publicKey) {
  const patched = Buffer.from(bytes);
  for (const [name, text] of [
    [SIGNATURE_SECTION, signature],
    [KEY_SECTION, publicKey],
  ]) {
    const content = Buffer.from(text, 'utf8');
    const { offset, size } = findElfSection(patched, name);
    /* Both are read as C strings, so one zero byte must remain. */
    if (content.length >= size) throw new Error(`${name} holds ${size - 1} bytes; got ${content.length}`);
    patched.fill(0, offset, offset + size);
    content.copy(patched, offset);
  }
  return patched;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [command, file, signatureFile, publicKeyFile] = process.argv.slice(2);
  if (command === 'digest' && file) {
    process.stdout.write(signedDigest(readFileSync(file)));
  } else if (command === 'embed' && file && signatureFile && publicKeyFile) {
    const signature = readFileSync(signatureFile, 'utf8');
    const publicKey = readFileSync(publicKeyFile, 'utf8');
    writeFileSync(file, withSignature(readFileSync(file), signature, publicKey));
  } else {
    console.error('Usage: node appimage-signature.mjs digest <appimage>');
    console.error('       node appimage-signature.mjs embed <appimage> <signature.asc> <public-key.asc>');
    process.exit(2);
  }
}
