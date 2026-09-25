/**
 * The desktop's old app runtime, its Quick Access controller and the shared TypeScript only they
 * used, as they were at `FROZEN_RUNTIME_REVISION`, the last commit before step 3 of the app runtime
 * port deleted them with QuickJS. The port's gates (f2-parity.ts, f6-quick-access-parity.ts) compare
 * the Rust that replaced this code with it, so they read it from here.
 *
 * The files are extracted under the repository's ignored `tmp/` folder, so the root tsconfig still
 * applies to them: an `@/` import of a module that is still in the tree resolves to the live copy,
 * and one of a module deleted with the runtime is pointed at its frozen copy.
 */
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const FROZEN_RUNTIME_REVISION = 'd991d604d';

const root = fileURLToPath(new URL('../../', import.meta.url));

/** What step 3 deleted that a gate still reads. */
const FROZEN_PATHS = [
  'apps/desktop/sidebar/gxserver-runtime',
  'apps/desktop/sidebar/native-quick-access',
  'packages/shared/gxserver-presentation-cache.ts',
  'packages/shared/native-runtime',
];

/** The folder the frozen tree lives in; its paths mirror the repository's. */
export function frozenRuntimeRoot(): string {
  const out = join(root, 'tmp', 'frozen-runtime', FROZEN_RUNTIME_REVISION);
  if (existsSync(join(out, '.complete'))) return out;
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  const archive = spawnSync('git', ['archive', FROZEN_RUNTIME_REVISION, ...FROZEN_PATHS], {
    cwd: root,
    maxBuffer: 1 << 28,
  });
  if (archive.status !== 0) throw new Error(`git archive ${FROZEN_RUNTIME_REVISION} failed`);
  const untar = spawnSync('tar', ['-x', '-C', out], { input: archive.stdout });
  if (untar.status !== 0) throw new Error('tar failed');
  for (const file of files(out)) {
    const text = readFileSync(file, 'utf8');
    const next = text.replace(/(['"])@\/([^'"]+)\1/g, (whole, quote: string, path: string) =>
      frozenModule(out, path) ? `${quote}${join(out, path)}${quote}` : whole
    );
    if (next !== text) writeFileSync(file, next);
  }
  writeFileSync(join(out, '.complete'), '');
  return out;
}

function frozenModule(out: string, path: string): boolean {
  return ['', '.ts', '.tsx', '/index.ts'].some((suffix) => existsSync(join(out, path + suffix)));
}

function* files(directory: string): Generator<string> {
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) yield* files(path);
    else if (/\.tsx?$/.test(entry)) yield path;
  }
}
