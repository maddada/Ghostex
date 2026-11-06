/*
 * CDXC:WindowsInstall 2026-11-06 WHY: the user picked a PowerShell one-liner
 * (irm https://ghostex.dev/install.ps1 | iex) so first installs skip SmartScreen
 * (a PowerShell download carries no Mark of the Web). The script must refuse to
 * run anything whose SHA-256 differs from the digest GitHub publishes, and must
 * never weaken the PC's security. These tests run the real script in pwsh with
 * GitHub, the download and Start-Process replaced by local functions (functions
 * win over cmdlets in PowerShell's command lookup), so they need no network.
 * Skipped where pwsh is not installed. The script is served from the site repo
 * (maddada-com/public/install.ps1), which keeps a copy of this file.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';

const scriptPath = path.join(path.dirname(fileURLToPath(import.meta.url)), 'install.ps1');
const script = readFileSync(scriptPath, 'utf8');
const hasPwsh = spawnSync('pwsh', ['-NoProfile', '-Command', 'exit 0']).status === 0;

const payload = 'fake ghostex installer bytes';
const payloadDigest = `sha256:${createHash('sha256').update(payload).digest('hex')}`;

function release(digests = {}) {
  const asset = (arch) => ({
    name: `ghostex-10.47.0-windows-${arch}.exe`,
    size: payload.length,
    browser_download_url: `https://github.com/maddada/Ghostex/releases/download/v10.47.0/ghostex-10.47.0-windows-${arch}.exe`,
    ...(digests[arch] === null ? {} : { digest: digests[arch] ?? payloadDigest }),
  });
  return { tag_name: 'v10.47.0', assets: [asset('x64'), asset('arm64'), { name: 'other.zip', digest: payloadDigest }] };
}

const mocks = String.raw`
function Invoke-RestMethod { param($Uri, [switch]$UseBasicParsing)
  Add-Content $env:MOCK_LOG "api $Uri"
  $env:MOCK_RELEASE | ConvertFrom-Json }
function Invoke-WebRequest { param($Uri, $OutFile, [switch]$UseBasicParsing)
  Add-Content $env:MOCK_LOG "download $Uri $OutFile"
  [IO.File]::WriteAllText($OutFile, $env:MOCK_PAYLOAD) }
function Start-Process { param($FilePath, [switch]$PassThru)
  Add-Content $env:MOCK_LOG "start $FilePath"
  $p = [pscustomobject]@{ Handle = 1; ExitCode = [int]$env:MOCK_EXIT_CODE }
  $p | Add-Member ScriptMethod WaitForExit { }
  $p }
& ([scriptblock]::Create([IO.File]::ReadAllText($env:MOCK_SCRIPT)))
`;

let workDir;
afterEach(() => workDir && rmSync(workDir, { recursive: true, force: true }));

function run({ env = {}, rel = release(), payloadText = payload, exitCode = 0 } = {}) {
  workDir = mkdtempSync(path.join(tmpdir(), 'ghostex-install-test-'));
  const log = path.join(workDir, 'log.txt');
  const result = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command', mocks], {
    encoding: 'utf8',
    env: {
      ...process.env,
      OS: 'Windows_NT',
      PROCESSOR_ARCHITECTURE: 'AMD64',
      PROCESSOR_ARCHITEW6432: '',
      GHOSTEX_INSTALL_ARCH: 'x64',
      GHOSTEX_INSTALL_DRY_RUN: '',
      MOCK_LOG: log,
      MOCK_SCRIPT: scriptPath,
      MOCK_RELEASE: JSON.stringify(rel),
      MOCK_PAYLOAD: payloadText,
      MOCK_EXIT_CODE: String(exitCode),
      ...env,
    },
  });
  const lines = existsSync(log) ? readFileSync(log, 'utf8').trim().split(/\r?\n/) : [];
  const download = lines.find((line) => line.startsWith('download '));
  return {
    status: result.status,
    output: `${result.stdout}\n${result.stderr}`,
    lines,
    downloadUrl: download?.split(' ')[1],
    downloadFolder: download && path.dirname(download.split(' ').slice(2).join(' ')),
    started: lines.filter((line) => line.startsWith('start ')),
  };
}

describe('install.ps1 safety rules', () => {
  it('never changes Defender, execution policy or SmartScreen, and is not encoded', () => {
    for (const forbidden of [
      /Add-MpPreference/i,
      /Set-MpPreference/i,
      /Set-ExecutionPolicy/i,
      /Unblock-File/i,
      /EncodedCommand/i,
      /FromBase64String/i,
      /SmartScreen\s*=/i,
    ]) {
      expect(script).not.toMatch(forbidden);
    }
  });
});

describe.skipIf(!hasPwsh)('install.ps1', () => {
  it('downloads the release asset, checks it and runs it, then deletes the download', () => {
    const r = run();
    expect(r.status, r.output).toBe(0);
    expect(r.lines[0]).toBe('api https://api.github.com/repos/maddada/Ghostex/releases/latest');
    expect(r.downloadUrl).toMatch(/\/v10\.47\.0\/ghostex-10\.47\.0-windows-x64\.exe$/);
    expect(r.started).toHaveLength(1);
    expect(r.started[0]).toContain(path.join(r.downloadFolder, 'ghostex-10.47.0-windows-x64.exe'));
    expect(existsSync(r.downloadFolder)).toBe(false);
  });

  it('refuses and deletes a download whose SHA-256 differs from GitHub digest', () => {
    const r = run({ payloadText: 'tampered bytes' });
    expect(r.status).not.toBe(0);
    expect(r.output).toContain('Checksum mismatch');
    expect(r.started).toHaveLength(0);
    expect(existsSync(r.downloadFolder)).toBe(false);
  });

  it('refuses before downloading when GitHub publishes no digest', () => {
    const r = run({ rel: release({ x64: null }) });
    expect(r.status).not.toBe(0);
    expect(r.output).toContain('no SHA-256 digest');
    expect(r.downloadUrl).toBeUndefined();
    expect(r.started).toHaveLength(0);
  });

  it('refuses a digest that is not SHA-256', () => {
    const r = run({ rel: release({ x64: 'md5:0123' }) });
    expect(r.status).not.toBe(0);
    expect(r.downloadUrl).toBeUndefined();
  });

  it('dry run downloads and checks, then stops before the installer', () => {
    const r = run({ env: { GHOSTEX_INSTALL_DRY_RUN: '1' } });
    expect(r.status, r.output).toBe(0);
    expect(r.output).toContain('SHA-256 matches');
    expect(r.output).toContain('Dry run');
    expect(r.started).toHaveLength(0);
    expect(existsSync(r.downloadFolder)).toBe(false);
  });

  it('picks arm64 when an x64 PowerShell runs on ARM64 Windows', () => {
    const r = run({ env: { GHOSTEX_INSTALL_ARCH: '', PROCESSOR_ARCHITEW6432: 'ARM64', GHOSTEX_INSTALL_DRY_RUN: '1' } });
    expect(r.status, r.output).toBe(0);
    expect(r.downloadUrl).toMatch(/windows-arm64\.exe$/);
  });

  it('rejects an unknown architecture override', () => {
    const r = run({ env: { GHOSTEX_INSTALL_ARCH: 'x86' } });
    expect(r.status).not.toBe(0);
    expect(r.output).toContain('must be x64 or arm64');
    expect(r.lines).toHaveLength(0);
  });

  it('fails when the release has no installer for this architecture', () => {
    const rel = release();
    rel.assets = rel.assets.filter((asset) => !asset.name.includes('arm64'));
    const r = run({ rel, env: { GHOSTEX_INSTALL_ARCH: 'arm64' } });
    expect(r.status).not.toBe(0);
    expect(r.output).toContain('has no ghostex-10.47.0-windows-arm64.exe');
  });

  it('reports a failed installer and still deletes the download', () => {
    const r = run({ exitCode: 1 });
    expect(r.status).not.toBe(0);
    expect(r.output).toContain('exited with code 1');
    expect(existsSync(r.downloadFolder)).toBe(false);
  });

  it('refuses to install on other operating systems', () => {
    const r = run({ env: { OS: '' } });
    expect(r.status).not.toBe(0);
    expect(r.output).toContain('This installer is for Windows');
  });
});
