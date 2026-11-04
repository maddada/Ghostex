import { describe, expect, test } from 'vitest';
import {
  PAIRING_LINK_PREFIX,
  encodeEasyConnectCode,
  isGhostexPairingPayload,
  encodeTailscaleCode,
  pairingLink,
  readPairingCode,
  type EasyConnectCode,
  type TailscaleCode,
} from './ghostex-remote-pairing';

const easyConnect: EasyConnectCode = {
  v: 1,
  address: 'tc1qexampleaddress',
  name: 'Studio',
  user: 'madda',
  port: 58744,
  sshPort: 22,
  secret: 's3cr3t',
  expiresAt: '2026-10-24T07:15:00.000Z',
};

const tailscale: TailscaleCode = { v: 1, name: 'Studio', host: 'studio.tail1234.ts.net', port: 22, user: 'madda' };

describe('pairingLink', () => {
  test('keeps the code after # so no web server sees it', () => {
    const code = encodeEasyConnectCode(easyConnect);
    const link = pairingLink(code);
    expect(link).toBe(`https://ghostex.dev/pair#${code}`);
    const url = new URL(link);
    expect(url.origin + url.pathname + url.search).toBe('https://ghostex.dev/pair');
    expect(url.hash).toBe(`#${code}`);
  });
});

describe('readPairingCode', () => {
  test('reads the bare codes older computers show', () => {
    expect(readPairingCode(encodeEasyConnectCode(easyConnect))).toEqual({ kind: 'easyConnect', code: easyConnect });
    expect(readPairingCode(encodeTailscaleCode(tailscale))).toEqual({ kind: 'tailscale', code: tailscale });
    expect(readPairingCode('tc1qexampleaddress')).toEqual({ kind: 'legacyAddress', address: 'tc1qexampleaddress' });
  });

  test('reads the links newer computers show', () => {
    const code = encodeEasyConnectCode(easyConnect);
    for (const link of [
      pairingLink(code),
      `  ${PAIRING_LINK_PREFIX}${code}\n`,
      `HTTPS://Ghostex.Dev/pair/#${code}`,
      `https://www.ghostex.dev/pair?from=camera#${code}`,
      `https://ghostex.dev/pair#${code.replace(':', '%3A')}`,
    ]) {
      expect(readPairingCode(link), link).toEqual({ kind: 'easyConnect', code: easyConnect });
    }
    expect(readPairingCode(pairingLink(encodeTailscaleCode(tailscale)))).toEqual({
      kind: 'tailscale',
      code: tailscale,
    });
  });

  test('refuses anything that is not a Ghostex pairing link', () => {
    const code = encodeEasyConnectCode(easyConnect);
    for (const link of [
      `https://ghostex.dev/pairs#${code}`,
      `https://ghostex.dev/pair/x#${code}`,
      `https://evil.example/pair#${code}`,
      `https://ghostex.dev.evil.example/pair#${code}`,
      'https://ghostex.dev/pair',
      'https://ghostex.dev/pair#tc1qexampleaddress',
      'https://ghostex.dev/pair#ghostex-ec1:not-base64!',
      'https://ghostex.dev/pair#%E0%A4%A',
    ]) {
      expect(readPairingCode(link), link).toBeNull();
    }
  });
});

/** gxserver's `COMPACT_VECTOR` (server/src/remote_access/pairing_code.rs). */
const COMPACT_VECTOR =
  'GHOSTEX-EC2:AAUAAAICAMCAKBQHBAEQUCYMBUHA6EARCIJRIFIWC4MBSGQ3DQOR4HZAEERCGJBFEYTQAEDEMVTGO2DJNJVWY3LON5YHC4TT4V4AAFQAAVWWCZDEMEAA4U3UOVSGS37CQCMXGICNMFRQ';
const compactVectorCode: EasyConnectCode = {
  v: 1,
  address: 'tcAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJw',
  name: 'Studio\u2019s Mac',
  user: 'madda',
  port: 58744,
  sshPort: 22,
  secret: 'ZGVmZ2hpamtsbW5vcHFycw',
};

describe('every code a computer since 10.30.0 has shown', () => {
  // 10.30.0 to 10.32.0: the bare code, a 43-character secret and its expiry.
  const bare = encodeEasyConnectCode({ ...easyConnect, secret: 'Iujwpx9qYo-HS_aPhRb64lJqjy08AbWo0_Z6yRCcBOc' });
  // 2026-10-27 on: a 22-character secret and no expiry.
  const trimmed: EasyConnectCode = { ...easyConnect, secret: 'Iujwpx9qYo-HS_aPhRb64l' };
  delete trimmed.expiresAt;

  test.each([
    ['10.30.0 bare code', bare],
    ['10.33.0 link', pairingLink(bare)],
    ['2026-10-27 link', pairingLink(encodeEasyConnectCode(trimmed))],
    ['2026-10-27 Copy as text', encodeEasyConnectCode(trimmed)],
  ])('%s', (_label, payload) => {
    expect(readPairingCode(payload)?.kind).toBe('easyConnect');
  });

  test('Tailscale codes, bare and linked', () => {
    expect(readPairingCode(encodeTailscaleCode(tailscale))?.kind).toBe('tailscale');
    expect(readPairingCode(pairingLink(encodeTailscaleCode(tailscale)))?.kind).toBe('tailscale');
  });

  test('the compact code, bare, linked and in lower case', () => {
    for (const payload of [COMPACT_VECTOR, pairingLink(COMPACT_VECTOR), COMPACT_VECTOR.toLowerCase()]) {
      expect(readPairingCode(payload), payload).toEqual({ kind: 'easyConnect', code: compactVectorCode });
    }
  });

  test('a broken compact code is refused', () => {
    for (const payload of ['GHOSTEX-EC2:', 'GHOSTEX-EC2:AAUAAAICAMCA', 'GHOSTEX-EC2:AAUA1AAICAMCA']) {
      expect(readPairingCode(payload), payload).toBeNull();
    }
  });
});

describe('isGhostexPairingPayload', () => {
  test('knows a Ghostex code it cannot read from a foreign one', () => {
    expect(isGhostexPairingPayload('ghostex-ec9:AAAA')).toBe(true);
    expect(isGhostexPairingPayload('https://ghostex.dev/pair#ghostex-ec9:AAAA')).toBe(true);
    expect(isGhostexPairingPayload('GHOSTEX-EC3:AAAA')).toBe(true);
    expect(isGhostexPairingPayload('https://example.com/pair#ghostex-ec1:AAAA')).toBe(false);
    expect(isGhostexPairingPayload('WIFI:S:home;;')).toBe(false);
  });
});
