import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
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

export function resolveLocalStartCodeSignIdentity(environment, installedAppPath) {
  if (Object.hasOwn(environment, 'GHOSTEX_GPUI_SIGN_IDENTITY')) {
    return environment.GHOSTEX_GPUI_SIGN_IDENTITY ?? '';
  }
  const listing = listCodeSigningIdentities(environment);
  const probeFailures = [];
  for (const identity of preferredLocalStartCodeSignIdentities(listing.identities)) {
    const failure = identitySigningFailure(identity.name, environment);
    if (failure === undefined) {
      rememberLocalStartCodeSignIdentity(identity.name, environment);
      return identity.name;
    }
    probeFailures.push(`  ${identity.name}: ${failure}`);
  }
  /*
  CDXC:Build 2026-09-22 WHY:
  macOS keys folder permissions (Documents, removable volumes, and the rest of TCC) to the app's designated requirement. A certificate-signed build keeps one requirement across rebuilds; an ad-hoc build's requirement is its cdhash, so every rebuild is a new app to TCC and the permission prompts come back on each restart and session switch.
  A start whose shell could not use the keychain used to warn once and install an ad-hoc build over the certificate-signed one, wiping the grants; and once the install was ad-hoc, every later keychain-less start stayed ad-hoc without a word because nothing recorded that this computer has a certificate. The identity of every successful certificate start is now remembered per computer, and ad-hoc is only for a computer that has never signed with a certificate, or an explicit GHOSTEX_GPUI_SIGN_IDENTITY=-. Every other case stops here, before the build, and prints codesign's and security's own output so the keychain problem gets fixed instead of hidden. Supersedes the 2026-08-25 fall-back-to-ad-hoc behaviour.
  */
  const installedAuthority = installedAppPath ? readCertificateAuthority(installedAppPath, environment) : undefined;
  const rememberedIdentity = rememberedLocalStartCodeSignIdentity(environment);
  if (probeFailures.length > 0 || installedAuthority || rememberedIdentity) {
    console.error(
      [
        probeFailures.length > 0
          ? `This shell lists code-signing identities but cannot sign with any of them:\n${probeFailures.join('\n')}`
          : installedAuthority
            ? `${installedAppPath} is signed with "${installedAuthority}", but this shell lists no code-signing identity.`
            : `This computer signs local Ghostex builds with "${rememberedIdentity}" (remembered in ${localStartCodeSignIdentityPath(environment)}), but this shell lists no code-signing identity.`,
        ...(listing.output ? [`security find-identity -v -p codesigning said:\n${indent(listing.output)}`] : []),
        `security list-keychains said:\n${indent(keychainSearchList(environment))}`,
        "An ad-hoc build would make macOS forget the app's folder permissions and ask again after every rebuild.",
        'Fix keychain access for this shell (unlock the login keychain, or run outside a sandboxed agent shell), or set GHOSTEX_GPUI_SIGN_IDENTITY=- to install an ad-hoc build on purpose.',
      ].join('\n')
    );
    process.exit(1);
  }
  console.warn(
    'No Apple code-signing identity was found; using ad-hoc GPUI signing. macOS will ask for permissions again after GPUI rebuilds.'
  );
  return '-';
}

function indent(text) {
  return text
    .split(/\r?\n/)
    .map((line) => `  ${line}`)
    .join('\n');
}

function localStartCodeSignIdentityPath(environment) {
  const stateHome = environment.XDG_STATE_HOME?.trim();
  const stateRoot = stateHome && path.isAbsolute(stateHome) ? stateHome : path.join(homedir(), '.local', 'state');
  return path.join(stateRoot, 'ghostex', 'local-start', 'macos-code-sign-identity');
}

function rememberedLocalStartCodeSignIdentity(environment) {
  try {
    return readFileSync(localStartCodeSignIdentityPath(environment), 'utf8').trim() || undefined;
  } catch {
    return undefined;
  }
}

function rememberLocalStartCodeSignIdentity(identityName, environment) {
  const identityPath = localStartCodeSignIdentityPath(environment);
  mkdirSync(path.dirname(identityPath), { recursive: true });
  writeFileSync(identityPath, `${identityName}\n`);
}

function keychainSearchList(environment) {
  const result = spawnSync('security', ['list-keychains'], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: environment,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return result.error?.message ?? `${result.stdout}${result.stderr}`.trim();
}

function readCertificateAuthority(codePath, environment) {
  const result = spawnSync('codesign', ['-dv', '--verbose=4', codePath], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: environment,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.error || result.status !== 0) {
    return undefined;
  }
  return `${result.stderr}\n${result.stdout}`.match(/^Authority=(.+)$/m)?.[1];
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

/**
 * CDXC:Build 2026-08-25 WHY:
 * `security find-identity -v -p codesigning` can list certificates that `codesign --sign` then rejects (missing private key, locked keychain, or a stale listing), so each identity is probed on a throwaway file before the build instead of failing after the full rebuild.
 * Returns undefined when the identity signs, otherwise codesign's error text.
 */
function identitySigningFailure(identityName, environment) {
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
    if (result.status === 0) {
      return undefined;
    }
    return (result.error?.message ?? result.stderr.trim()) || `codesign exited with status ${result.status}`;
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
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.error) {
    return { identities: [], output: result.error.message };
  }
  const output = `${result.stdout}${result.stderr}`.trim();
  if (result.status !== 0) {
    return { identities: [], output };
  }
  const identities = [];
  for (const line of result.stdout.split(/\r?\n/)) {
    const match = line.match(/^\s*\d+\)\s+([A-Fa-f0-9]+)\s+"([^"]+)"/);
    if (match) {
      identities.push({ hash: match[1], name: match[2] });
    }
  }
  return { identities, output };
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

/**
 * CDXC:PlatformSupport 2026-09-18 WHY:
 * PowerShell 7 prepends its own module directories to PSModulePath, and every child process inherits them.
 * The Windows build and install scripts run under Windows PowerShell 5.1 (powershell.exe), which then autoloads PowerShell 7's Microsoft.PowerShell.Utility 7.0 instead of its own and loses cmdlets such as Get-FileHash, so `bun run start` from a pwsh terminal failed with "Get-FileHash is not recognized".
 * Drop exactly PowerShell 7's three entries (its $PSHOME, shared and per-user module directories) so 5.1 resolves its own modules.
 * The match is anchored because other products also install under a PowerShell\Modules folder (SQL Server ships ...\Tools\PowerShell\Modules), and those must survive.
 */
export function withoutPowerShell7ModulePaths(environment) {
  const key = Object.keys(environment).find((name) => name.toLowerCase() === 'psmodulepath');
  if (process.platform !== 'win32' || !key || !environment[key]) {
    return environment;
  }
  const isPowerShell7Entry = (entry) => {
    const normalized = entry
      .trim()
      .replace(/[\\/]+$/u, '')
      .toLowerCase();
    if (
      /[\\/]powershell[\\/]7[^\\/]*[\\/]modules$/u.test(normalized) ||
      /[\\/]documents[\\/]powershell[\\/]modules$/u.test(normalized) ||
      /^[a-z]:[\\/]program files[\\/]powershell[\\/]modules$/u.test(normalized)
    ) {
      return true;
    }
    // Portable PowerShell installs can use any directory name. Their Core-only
    // built-ins must not shadow Windows PowerShell's Desktop modules.
    const manifest = path.join(entry.trim(), 'Microsoft.PowerShell.Utility', 'Microsoft.PowerShell.Utility.psd1');
    if (!existsSync(manifest)) return false;
    const editions = readFileSync(manifest, 'utf8').match(/^\s*CompatiblePSEditions\s*=\s*@\(([^)]*)\)/imu)?.[1];
    return Boolean(editions && /['"]Core['"]/iu.test(editions) && !/['"]Desktop['"]/iu.test(editions));
  };
  return {
    ...environment,
    [key]: environment[key]
      .split(';')
      .filter((entry) => entry && !isPowerShell7Entry(entry))
      .join(';'),
  };
}

function isColorDisablingForceColor(value) {
  return typeof value === 'string' && ['0', 'false'].includes(value.trim().toLowerCase());
}
