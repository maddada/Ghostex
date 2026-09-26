import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, readFileSync, symlinkSync, unlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const root = import.meta.dirname;
const repo = path.resolve(root, '../..');
const release = process.argv.includes('--release');
// Cargo can use response files for web-sys's long feature list; sccache's Windows process invocation exceeds the command-line limit.
const environment = { ...process.env, ...(process.platform === 'win32' ? { RUSTC_WRAPPER: '' } : {}) };
function run(command, args, cwd = root, capture = false) {
  const result = spawnSync(command, args, { cwd, env: environment, encoding: 'utf8', stdio: capture ? 'pipe' : 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with ${result.status}: ${result.stderr ?? ''}`);
  return result.stdout;
}

// CDXC:WebGpui 2026-09-25 WHY: Windows checkouts with core.symlinks=false contain path text where Rust needs the shared desktop sources. Restore actual links so the browser still compiles the same files as desktop.
if (process.platform === 'win32') {
  const entries = run('git', ['ls-files', '-s', '-z', 'apps/gpui-web'], repo, true).split('\0');
  for (const entry of entries) {
    if (!entry.startsWith('120000 ')) continue;
    const file = path.join(repo, entry.slice(entry.indexOf('\t') + 1));
    if (lstatSync(file).isSymbolicLink()) continue;
    const contents = readFileSync(file, 'utf8');
    const target = contents.trim();
    const resolved = path.resolve(path.dirname(file), target);
    if (path.isAbsolute(target) || !resolved.startsWith(repo + path.sep) || !existsSync(resolved)) {
      throw new Error(`Cannot restore shared-source link ${file}: ${target}`);
    }
    unlinkSync(file);
    try {
      symlinkSync(target, file, lstatSync(resolved).isDirectory() ? 'dir' : 'file');
    } catch (error) {
      writeFileSync(file, contents);
      throw new Error(`Enable Windows Developer Mode or run with symlink privileges to build GPUI web: ${error.message}`);
    }
  }
}

const prefix = path.join(root, 'target/libghostty-vt-wasm');
if (!existsSync(path.join(prefix, 'lib/libghostty-vt.a'))) {
  const ghostty = path.join(repo, '.dependencies/ghostty');
  const version = readFileSync(path.join(ghostty, 'build.zig.zon'), 'utf8').match(/\.version\s*=\s*"([^"]+)"/)[1];
  const homebrewZig = '/opt/homebrew/opt/zig@0.16/bin/zig';
  const zig = process.env.GHOSTEX_ZIG || (existsSync(homebrewZig) ? homebrewZig : 'zig');
  run(zig, ['build', `-Dversion-string=${version}`, '-Demit-lib-vt=true', '-Demit-lib-vt-shared=false',
    '-Demit-xcframework=false', '-Doptimize=ReleaseSmall', '-Dtarget=wasm32-freestanding', '--prefix', prefix], ghostty);
}
run('cargo', ['build', '--target', 'wasm32-unknown-unknown', ...(release ? ['--release'] : [])]);
run('bun', ['tooling/build-chat-runtime.mjs', 'apps/gpui-web/www/public/chat-runtime.js'], repo);
run('wasm-bindgen', [`target/wasm32-unknown-unknown/${release ? 'release' : 'debug'}/ghostex_gpui_web.wasm`,
  '--out-dir', 'www/src/wasm', '--target', 'web', '--no-typescript']);
