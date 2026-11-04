/*
CDXC:RemotePairing 2026-09-03:
The two pairing QR payloads the desktop shows (Settings → Remote) and the
mobile app scans. Both are `<prefix><base64url(JSON)>` with no whitespace, so
a paste field can validate inline and a foreign QR is rejected by its prefix
before anything is decoded.

This file is deliberately dependency-free (no Buffer, no TextEncoder, no
base64 library) because the React Native app does not import
`packages/shared`; it mirrors this module verbatim as
`apps/mobile/app/src/machines/pairingCodes.ts`. Keep the two in sync.
*/

export const EASY_CONNECT_CODE_PREFIX = 'ghostex-ec1:';
export const TAILSCALE_CODE_PREFIX = 'ghostex-ts1:';

/*
CDXC:RemotePairing 2026-10-24 DECISION:
User: "even tho i have ghostex installed on my phone it says there aren't any
apps that can open this link when i open it (this is on android when i use
default camera, pls fix, i want same on ios also if possible)". gxserver now
wraps each code in this link (`https://ghostex.dev/pair#ghostex-ec1:…`), which
the phone app claims as an Android App Link and an iOS Universal Link, so the
system camera opens the app. The code rides in the fragment, which a browser
never sends to ghostex.dev. `readPairingCode` reads the link and the bare code
alike, so a phone and a computer of either age still pair.
*/
export const PAIRING_LINK_PREFIX = 'https://ghostex.dev/pair#';

/*
CDXC:RemotePairing 2026-10-27 WHY:
The compact Easy Connect code (gxserver's `encode_easy_connect_compact_code`):
the same fields as bytes in base32 behind an upper-case prefix, so the QR holds
everything after the link in alphanumeric mode and drops from version 19 to
about 11. Readers take it in either case. Every length is a big-endian u16:
address (the `tc` blob's base64url bytes), secret (its base64url bytes), port,
SSH port, user, name (UTF-8); bytes after the name are ignored so a later field
can be added without breaking this reader.
*/
export const EASY_CONNECT_COMPACT_CODE_PREFIX = 'GHOSTEX-EC2:';

/** Prefix of the raw Easy Connect address blob (the Advanced "Pairing address"). */
export const LEGACY_EASY_CONNECT_ADDRESS_PREFIX = 'tc';

export interface EasyConnectCode {
  v: 1;
  /** The Easy Connect address blob (`tc…`), verbatim. */
  address: string;
  /** Computer display name (hostname / ComputerName). */
  name: string;
  /** Login user on the computer (SSH user). */
  user: string;
  /** gxserver API port reachable through the tunnel (58744). */
  port: number;
  /** SSH port reachable through the tunnel (22). */
  sshPort: number;
  /** One-time pairing secret; absent when pairing registration is off. */
  secret?: string;
  /** ISO timestamp, secret validity. */
  expiresAt?: string;
}

export interface TailscaleCode {
  v: 1;
  /** Computer display name. */
  name: string;
  /** MagicDNS name, preferred over `ip`. */
  host?: string;
  /** Tailscale IP (100.x.y.z). */
  ip?: string;
  /** SSH port. */
  port: number;
  user: string;
}

export type PairingCode = { kind: 'easyConnect'; code: EasyConnectCode } | { kind: 'tailscale'; code: TailscaleCode };

export type ReadPairingCodeResult = PairingCode | { kind: 'legacyAddress'; address: string } | null;

export function encodeEasyConnectCode(code: EasyConnectCode): string {
  return EASY_CONNECT_CODE_PREFIX + encodeBase64UrlJson(code);
}

export function encodeTailscaleCode(code: TailscaleCode): string {
  return TAILSCALE_CODE_PREFIX + encodeBase64UrlJson(code);
}

/** The QR payload: an encoded code wrapped in the ghostex.dev link that opens the phone app. */
export function pairingLink(code: string): string {
  return PAIRING_LINK_PREFIX + code;
}

/**
 * Decodes a scanned or pasted payload: a pairing link or a bare code. A bare
 * `tc…` address (what the Advanced "Pairing address" row shows) is reported as
 * `legacyAddress` so it still pastes; the caller then asks for name, user and
 * password.
 */
export function readPairingCode(payload: string): ReadPairingCodeResult {
  const trimmed = payload.trim();
  if (trimmed.length === 0 || /\s/u.test(trimmed)) return null;
  const fragment = readPairingLinkFragment(trimmed);
  if (fragment !== undefined) {
    // A link carries a whole code, never a bare address.
    if (fragment === null) return null;
    if (
      !fragment.startsWith(EASY_CONNECT_CODE_PREFIX) &&
      !fragment.startsWith(TAILSCALE_CODE_PREFIX) &&
      !hasCompactPrefix(fragment)
    ) {
      return null;
    }
    return readPairingCode(fragment);
  }
  if (hasCompactPrefix(trimmed)) {
    const code = readEasyConnectCompactCode(trimmed.slice(EASY_CONNECT_COMPACT_CODE_PREFIX.length));
    return code ? { kind: 'easyConnect', code } : null;
  }
  if (trimmed.startsWith(EASY_CONNECT_CODE_PREFIX)) {
    const code = readEasyConnectCode(decodeBase64UrlJson(trimmed.slice(EASY_CONNECT_CODE_PREFIX.length)));
    return code ? { kind: 'easyConnect', code } : null;
  }
  if (trimmed.startsWith(TAILSCALE_CODE_PREFIX)) {
    const code = readTailscaleCode(decodeBase64UrlJson(trimmed.slice(TAILSCALE_CODE_PREFIX.length)));
    return code ? { kind: 'tailscale', code } : null;
  }
  if (
    trimmed.length > LEGACY_EASY_CONNECT_ADDRESS_PREFIX.length &&
    trimmed.startsWith(LEGACY_EASY_CONNECT_ADDRESS_PREFIX)
  ) {
    return { kind: 'legacyAddress', address: trimmed };
  }
  return null;
}

/**
 * Whether `payload` is meant as a Ghostex pairing code (a ghostex.dev pairing
 * link or a `ghostex-` prefix) even when `readPairingCode` cannot read it, so
 * a scanner can say "update the app" instead of "not a Ghostex code" when a
 * newer computer shows a format this build does not know.
 */
export function isGhostexPairingPayload(payload: string): boolean {
  const trimmed = payload.trim();
  return readPairingLinkFragment(trimmed) !== undefined || /^ghostex-[a-z]+\d+:/iu.test(trimmed);
}

function hasCompactPrefix(text: string): boolean {
  return text.slice(0, EASY_CONNECT_COMPACT_CODE_PREFIX.length).toUpperCase() === EASY_CONNECT_COMPACT_CODE_PREFIX;
}

function readEasyConnectCompactCode(body: string): EasyConnectCode | null {
  const bytes = decodeBase32(body);
  if (!bytes) return null;
  let offset = 0;
  const readU16 = (): number | null => {
    if (offset + 2 > bytes.length) return null;
    const value = (bytes[offset]! << 8) | bytes[offset + 1]!;
    offset += 2;
    return value;
  };
  const readField = (): number[] | null => {
    const length = readU16();
    if (length === null || offset + length > bytes.length) return null;
    const field = bytes.slice(offset, offset + length);
    offset += length;
    return field;
  };
  const address = readField();
  const secret = readField();
  const port = readU16();
  const sshPort = readU16();
  const user = readField();
  const name = readField();
  if (!address || !secret || port === null || sshPort === null || !user || !name) return null;
  return readEasyConnectCode({
    v: 1,
    address: LEGACY_EASY_CONNECT_ADDRESS_PREFIX + encodeBase64Url(address),
    name: utf8Decode(name),
    user: utf8Decode(user),
    port,
    sshPort,
    secret: encodeBase64Url(secret),
  });
}

/**
 * The fragment of a ghostex.dev pairing link (`null` when it does not decode),
 * or `undefined` when `text` is not one. Lenient about what a camera or a
 * browser may change on the way: `http`, `www.`, the host's case, a trailing
 * slash and a query.
 */
function readPairingLinkFragment(text: string): string | null | undefined {
  const match = /^https?:\/\/(?:www\.)?ghostex\.dev\/pair\/?(?:\?[^#]*)?#(.*)$/iu.exec(text);
  if (!match) return undefined;
  try {
    return decodeURIComponent(match[1]!);
  } catch {
    return null;
  }
}

function readEasyConnectCode(value: unknown): EasyConnectCode | null {
  if (!isRecord(value) || value.v !== 1) return null;
  const address = readNonEmptyString(value.address);
  const name = readNonEmptyString(value.name);
  const user = readNonEmptyString(value.user);
  const port = readPort(value.port);
  const sshPort = readPort(value.sshPort);
  if (!address || !name || !user || port === null || sshPort === null) return null;
  const code: EasyConnectCode = { v: 1, address, name, user, port, sshPort };
  const secret = readNonEmptyString(value.secret);
  if (secret) code.secret = secret;
  const expiresAt = readNonEmptyString(value.expiresAt);
  if (expiresAt) code.expiresAt = expiresAt;
  return code;
}

function readTailscaleCode(value: unknown): TailscaleCode | null {
  if (!isRecord(value) || value.v !== 1) return null;
  const name = readNonEmptyString(value.name);
  const user = readNonEmptyString(value.user);
  const port = readPort(value.port);
  const host = readNonEmptyString(value.host);
  const ip = readNonEmptyString(value.ip);
  if (!name || !user || port === null || (!host && !ip)) return null;
  // Wire order (v, name, host, ip, port, user) so re-encoding a decoded code
  // reproduces the original payload byte for byte.
  return { v: 1, name, ...(host ? { host } : {}), ...(ip ? { ip } : {}), port, user };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readNonEmptyString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPort(value: unknown): number | null {
  return typeof value === 'number' && Number.isInteger(value) && value > 0 && value <= 65535 ? value : null;
}

// ---------------------------------------------------------------------------
// base64url over UTF-8 JSON, implemented by hand so the same source runs in
// browsers, Node, and Hermes without Buffer/TextEncoder/atob.
// ---------------------------------------------------------------------------

const BASE64URL_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';

function encodeBase64UrlJson(value: unknown): string {
  return encodeBase64Url(utf8Encode(JSON.stringify(value)));
}

function decodeBase64UrlJson(text: string): unknown {
  const bytes = decodeBase64Url(text);
  if (!bytes) return null;
  const json = utf8Decode(bytes);
  if (json === null) return null;
  try {
    return JSON.parse(json) as unknown;
  } catch {
    return null;
  }
}

function encodeBase64Url(bytes: number[]): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i]!;
    const b1 = i + 1 < bytes.length ? bytes[i + 1]! : 0;
    const b2 = i + 2 < bytes.length ? bytes[i + 2]! : 0;
    const triple = (b0 << 16) | (b1 << 8) | b2;
    out += BASE64URL_ALPHABET[(triple >> 18) & 63]!;
    out += BASE64URL_ALPHABET[(triple >> 12) & 63]!;
    if (i + 1 < bytes.length) out += BASE64URL_ALPHABET[(triple >> 6) & 63]!;
    if (i + 2 < bytes.length) out += BASE64URL_ALPHABET[triple & 63]!;
  }
  return out;
}

function decodeBase64Url(text: string): number[] | null {
  if (text.length === 0 || text.length % 4 === 1) return null;
  const bytes: number[] = [];
  let buffer = 0;
  let bits = 0;
  for (let i = 0; i < text.length; i += 1) {
    const index = BASE64URL_ALPHABET.indexOf(text[i]!);
    if (index < 0) return null;
    buffer = (buffer << 6) | index;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      bytes.push((buffer >> bits) & 0xff);
    }
  }
  return bytes;
}

const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

/** RFC 4648 base32 without padding, either case; leftover bits are dropped. */
function decodeBase32(text: string): number[] | null {
  if (text.length === 0) return null;
  const bytes: number[] = [];
  let buffer = 0;
  let bits = 0;
  for (const char of text.toUpperCase()) {
    const index = BASE32_ALPHABET.indexOf(char);
    if (index < 0) return null;
    buffer = ((buffer << 5) | index) & 0xffff;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      bytes.push((buffer >> bits) & 0xff);
    }
  }
  return bytes;
}

function utf8Encode(text: string): number[] {
  const bytes: number[] = [];
  for (let i = 0; i < text.length; i += 1) {
    let codePoint = text.charCodeAt(i);
    if (codePoint >= 0xd800 && codePoint <= 0xdbff && i + 1 < text.length) {
      const low = text.charCodeAt(i + 1);
      if (low >= 0xdc00 && low <= 0xdfff) {
        codePoint = 0x10000 + ((codePoint - 0xd800) << 10) + (low - 0xdc00);
        i += 1;
      }
    }
    if (codePoint < 0x80) {
      bytes.push(codePoint);
    } else if (codePoint < 0x800) {
      bytes.push(0xc0 | (codePoint >> 6), 0x80 | (codePoint & 63));
    } else if (codePoint < 0x10000) {
      bytes.push(0xe0 | (codePoint >> 12), 0x80 | ((codePoint >> 6) & 63), 0x80 | (codePoint & 63));
    } else {
      bytes.push(
        0xf0 | (codePoint >> 18),
        0x80 | ((codePoint >> 12) & 63),
        0x80 | ((codePoint >> 6) & 63),
        0x80 | (codePoint & 63)
      );
    }
  }
  return bytes;
}

function utf8Decode(bytes: number[]): string | null {
  let out = '';
  let i = 0;
  while (i < bytes.length) {
    const b0 = bytes[i]!;
    let codePoint: number;
    let width: number;
    if (b0 < 0x80) {
      codePoint = b0;
      width = 1;
    } else if ((b0 & 0xe0) === 0xc0) {
      codePoint = b0 & 0x1f;
      width = 2;
    } else if ((b0 & 0xf0) === 0xe0) {
      codePoint = b0 & 0x0f;
      width = 3;
    } else if ((b0 & 0xf8) === 0xf0) {
      codePoint = b0 & 0x07;
      width = 4;
    } else {
      return null;
    }
    if (i + width > bytes.length) return null;
    for (let k = 1; k < width; k += 1) {
      const trailing = bytes[i + k]!;
      if ((trailing & 0xc0) !== 0x80) return null;
      codePoint = (codePoint << 6) | (trailing & 63);
    }
    i += width;
    if (codePoint > 0x10ffff) return null;
    if (codePoint >= 0x10000) {
      const offset = codePoint - 0x10000;
      out += String.fromCharCode(0xd800 + (offset >> 10), 0xdc00 + (offset & 0x3ff));
    } else {
      out += String.fromCharCode(codePoint);
    }
  }
  return out;
}
