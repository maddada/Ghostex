import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { withoutPowerShell7ModulePaths } from './local-start-utils.mjs';

if (process.platform !== 'win32') throw new Error('Run setup:windows from a native Windows shell.');

const result = spawnSync(
  'powershell.exe',
  [
    '-NoProfile',
    '-NonInteractive',
    '-ExecutionPolicy',
    'Bypass',
    '-File',
    fileURLToPath(new URL('./prepare-windows-build.ps1', import.meta.url)),
    '-Install',
  ],
  {
    env: withoutPowerShell7ModulePaths(process.env),
    stdio: 'inherit',
    windowsHide: true,
  }
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

const preparation = spawnSync(
  process.execPath,
  [fileURLToPath(new URL('./start-gpui.mjs', import.meta.url)), '--prepare-only'],
  { stdio: 'inherit', windowsHide: true }
);
if (preparation.error) throw preparation.error;
process.exit(preparation.status ?? 1);
