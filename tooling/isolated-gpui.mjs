import { spawnSync } from 'node:child_process';
import {
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  realpathSync,
  renameSync,
  symlinkSync,
  unlinkSync,
} from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * CDXC:Build 2026-09-13 DECISION:
 * User: ghostex-3 must start separately from the main Ghostex with separate configuration, sharing only the existing hooks via a symlink.
 * The bundle retains this environment so reopening it from Finder uses the same isolated instance.
 */
export function isolatedGpuiConfiguration() {
  if (process.platform !== 'darwin') throw new Error('The isolated Ghostex-3 launcher currently supports macOS.');
  const home = path.join(homedir(), '.local', 'share', 'ghostex-3');
  return {
    appName: 'Ghostex-3',
    bundleId: 'com.madda.ghostex.gpui.ghostex-3',
    installDir: path.join(homedir(), 'Applications'),
    environment: {
      GHOSTEX_HOME: home,
      GHOSTEX_GXSERVER_DEV_PORT: '58747',
      GHOSTEX_CODE_SERVER_PORT: '3778',
      CODE_SERVER_CONFIG: path.join(home, 'code-server-runtime-gpui/config.yaml'),
      GHOSTEX_GPUI_CEF_REMOTE_DEBUGGING_PORT: '9337',
      GHOSTEX_GXSERVER_CLI: path.join(
        homedir(),
        'Applications/Ghostex-3.app/Contents/Resources/Web/gxserver/bin/gxserver'
      ),
      GHOSTEX_GXSERVER_BIN: path.join(
        homedir(),
        'Applications/Ghostex-3.app/Contents/Resources/Web/gxserver/bin/gxserver'
      ),
    },
  };
}

export function prepareIsolatedGpui(configuration) {
  const configuredData = process.env.XDG_DATA_HOME?.trim();
  const originalData =
    configuredData && path.isAbsolute(configuredData)
      ? path.join(configuredData, 'ghostex')
      : path.join(homedir(), '.local', 'share', 'ghostex');
  const originalHooks = path.join(originalData, 'hooks');
  if (!existsSync(originalHooks)) throw new Error(`Original Ghostex hooks are missing: ${originalHooks}`);
  const home = configuration.environment.GHOSTEX_HOME;
  mkdirSync(home, { recursive: true, mode: 0o700 });
  mkdirSync(configuration.installDir, { recursive: true });
  const hooks = path.join(home, 'hooks');
  if (lstatSync(home).isSymbolicLink() || lstatSync(hooks, { throwIfNoEntry: false })?.isSymbolicLink()) {
    throw new Error('The isolated storage root and hooks directory must be private directories.');
  }
  detachIsolatedVSCodeSettings(home);
  mkdirSync(hooks, { recursive: true, mode: 0o700 });
  // Link files individually: hook upgrades use atomic rename, so they can
  // replace a local link without writing through a shared directory.
  for (const entry of readdirSync(originalHooks, { withFileTypes: true })) {
    if (!entry.isFile() && !entry.isSymbolicLink()) continue;
    const source = path.join(originalHooks, entry.name);
    const target = path.join(hooks, entry.name);
    const existing = lstatSync(target, { throwIfNoEntry: false });
    if (existing?.isSymbolicLink() && realpathSync(target) !== realpathSync(source)) {
      throw new Error(`The existing hook link points elsewhere: ${target}`);
    }
    if (!existing) symlinkSync(source, target);
  }
}

// The first isolated build linked these entries back to the user's VS Code.
// Preserve their current contents as private copies before starting the editor.
function detachIsolatedVSCodeSettings(home) {
  const user = path.join(home, 'code-server-runtime-gpui/user-data/User');
  for (const name of ['settings.json', 'keybindings.json', 'snippets', 'mcp.json', 'tasks.json']) {
    const target = path.join(user, name);
    if (!lstatSync(target, { throwIfNoEntry: false })?.isSymbolicLink()) continue;
    const temporary = `${target}.isolated-${process.pid}`;
    cpSync(realpathSync(target), temporary, { recursive: true, dereference: true, errorOnExist: true, force: false });
    // POSIX rename replaces a file symlink directly. A copied snippets
    // directory needs its old directory symlink unlinked first.
    if (lstatSync(temporary).isDirectory()) unlinkSync(target);
    renameSync(temporary, target);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const configuration = isolatedGpuiConfiguration();
  if (process.argv[2] === '--print') {
    process.stdout.write(`${JSON.stringify(configuration, null, 2)}\n`);
  } else {
    prepareIsolatedGpui(configuration);
    const cli = path.join(configuration.installDir, `${configuration.appName}.app`, 'Contents/Resources/CLI/ghostex');
    if (!existsSync(cli)) throw new Error('Build Ghostex-3 first with bun run start:isolated.');
    const result = spawnSync(cli, process.argv.slice(2), {
      env: { ...process.env, ...configuration.environment },
      stdio: 'inherit',
    });
    if (result.error) throw result.error;
    process.exitCode = result.status ?? 1;
  }
}
