import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export function formatCommand(command, args) {
  return [command, ...args].map(shellQuote).join(' ');
}

function shellQuote(value) {
  const text = String(value);
  if (/^[A-Za-z0-9_./:=@%+-]+$/.test(text)) {
    return text;
  }
  return `'${text.replaceAll("'", "'\\''")}'`;
}

export function resolveLocalStartCodeSignIdentity(environment) {
  if (Object.hasOwn(environment, 'GHOSTEX_GPUI_SIGN_IDENTITY')) {
    return environment.GHOSTEX_GPUI_SIGN_IDENTITY ?? '';
  }
  const identities = listCodeSigningIdentities(environment);
  const preferredIdentity = preferredLocalStartCodeSignIdentities(identities).find((identity) =>
    identityCanSign(identity.name, environment)
  );
  if (preferredIdentity) {
    return preferredIdentity.name;
  }
  console.warn(
    identities.length > 0
      ? 'Found code-signing identities, but none could sign; falling back to ad-hoc GPUI signing. macOS may ask for permissions again after GPUI rebuilds.'
      : 'No Apple code-signing identity was found; falling back to ad-hoc GPUI signing. macOS may ask for permissions again after GPUI rebuilds.'
  );
  return '-';
}

function preferredLocalStartCodeSignIdentities(identities) {
  const prefixes = ['Apple Development: ', 'Mac Developer: ', 'Developer ID Application: ', 'Apple Distribution: '];
  const seen = new Set();
  const preferred = [];
  for (const prefix of prefixes) {
    for (const identity of identities) {
      if (!identity.name.startsWith(prefix) || seen.has(identity.name)) {
        continue;
      }
      seen.add(identity.name);
      preferred.push(identity);
    }
  }
  return preferred;
}

function identityCanSign(identityName, environment) {
  /*
  CDXC:Build 2026-08-25:
  `security find-identity -v -p codesigning` can list Apple Development certs
  that `codesign --sign` then rejects with "no identity found" (missing private
  key, locked keychain, or a stale listing). Probe with a throwaway file so
  local start falls back to ad-hoc instead of failing after the full rebuild.
  */
  const probeDir = mkdtempSync(path.join(tmpdir(), 'ghostex-gpui-sign-'));
  const probePath = path.join(probeDir, 'probe');
  try {
    writeFileSync(probePath, 'ghostex-gpui-codesign-probe\n');
    const result = spawnSync('codesign', ['--force', '--sign', identityName, '--timestamp=none', probePath], {
      cwd: repoRoot,
      encoding: 'utf8',
      env: environment,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return result.status === 0;
  } finally {
    rmSync(probeDir, { force: true, recursive: true });
  }
}

export function resolveLocalStartCodeSignTimestampFlag(environment) {
  if (Object.hasOwn(environment, 'GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG')) {
    return environment.GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG ?? '';
  }
  return '--timestamp=none';
}

function listCodeSigningIdentities(environment) {
  const result = spawnSync('security', ['find-identity', '-v', '-p', 'codesigning'], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: environment,
    stdio: ['ignore', 'pipe', 'ignore'],
  });
  if (result.error || result.status !== 0) {
    return [];
  }
  const identities = [];
  for (const line of result.stdout.split(/\r?\n/)) {
    const match = line.match(/^\s*\d+\)\s+([A-Fa-f0-9]+)\s+"([^"]+)"/);
    if (match) {
      identities.push({ hash: match[1], name: match[2] });
    }
  }
  return identities;
}

export function withoutColorDisablingEnvironment(environment) {
  const sanitized = { ...environment };
  for (const key of ['ANSI_COLORS_DISABLED', 'NO_COLOR', 'NODE_DISABLE_COLORS']) {
    delete sanitized[key];
  }
  if (isColorDisablingForceColor(sanitized.FORCE_COLOR)) {
    delete sanitized.FORCE_COLOR;
  }
  return sanitized;
}

function isColorDisablingForceColor(value) {
  return typeof value === 'string' && ['0', 'false'].includes(value.trim().toLowerCase());
}
