// Formats the files an agent just edited, from a PostToolUse hook in your own untracked agent settings (.claude/settings.local.json for Claude Code, .codex/hooks.json for Codex; ai/formatting-and-file-size.md).
// It reads the hook's JSON on stdin (Claude's `tool_input.file_path`, or the paths in Codex's `apply_patch` text); file paths given as arguments are formatted instead, for a manual run.
// Only app-owned files are touched, by tooling/format/scope.json: Rust anywhere outside `neverFormat`, and prettier only for the files its globs cover.
// It never fails the edit: every outcome exits 0, and a formatter error is one short line on stderr.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = realpathSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..'));
const scope = JSON.parse(readFileSync(path.join(root, 'tooling/format/scope.json'), 'utf8'));

function note(message) {
  process.stderr.write(`format-edited-file: ${message}\n`);
}

function readStdin() {
  try {
    return readFileSync(0, 'utf8');
  } catch {
    return '';
  }
}

/** The paths an edit tool wrote, from the hook input of Claude Code (Edit, Write, MultiEdit) or Codex (apply_patch). */
export function editedPaths(input) {
  const toolInput = input?.tool_input ?? {};
  const base = typeof input?.cwd === 'string' ? input.cwd : process.cwd();
  const paths = [];
  if (typeof toolInput.file_path === 'string') paths.push(toolInput.file_path);
  if (typeof toolInput.command === 'string') {
    for (const match of toolInput.command.matchAll(/^\*\*\* (?:Add File|Update File|Move to): (.+)$/gm)) {
      paths.push(match[1].trim());
    }
  }
  return paths.map((file) => path.resolve(base, file));
}

/** The repository-relative path when the file is app-owned and formattable, otherwise null. */
export function ownedPath(file) {
  let real;
  try {
    real = realpathSync(file);
    if (!statSync(real).isFile()) return null;
  } catch {
    return null;
  }
  const relative = path.relative(root, real).split(path.sep).join('/');
  if (relative.startsWith('../') || path.isAbsolute(relative)) return null;
  if (scope.neverFormat.some((glob) => path.matchesGlob(relative, glob))) return null;
  return relative;
}

function gitIgnored(relative) {
  return spawnSync('git', ['check-ignore', '-q', relative], { cwd: root }).status === 0;
}

/** The `edition` keys of a Cargo.toml by table: `package` and `workspace.package` hold "2021" and the like, or "workspace" for `edition.workspace = true`. */
export function manifestEditions(text) {
  const editions = {};
  let table = '';
  for (const line of text.split('\n')) {
    const header = line.match(/^\s*\[([^\]]+)\]\s*(#.*)?$/);
    if (header) {
      table = header[1].trim();
      editions[table] ??= null;
      continue;
    }
    const value = line.match(/^\s*edition\s*=\s*"(\d{4})"/);
    if (value) editions[table] = value[1];
    else if (/^\s*edition\.workspace\s*=\s*true/.test(line)) editions[table] = 'workspace';
  }
  return editions;
}

/** The edition of the crate that owns the file: the nearest Cargo.toml with a [package], following `edition.workspace = true` up to the workspace. */
export function crateEdition(file) {
  let dir = path.dirname(file);
  let inherits = false;
  while (dir.startsWith(root)) {
    const manifest = path.join(dir, 'Cargo.toml');
    if (existsSync(manifest)) {
      const editions = manifestEditions(readFileSync(manifest, 'utf8'));
      if (inherits && editions['workspace.package']) return editions['workspace.package'];
      if (!inherits && 'package' in editions) {
        if (editions.package !== 'workspace') return editions.package ?? '2015';
        if (editions['workspace.package']) return editions['workspace.package'];
        inherits = true;
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function writeIfChanged(file, before, after) {
  if (after !== before) writeFileSync(file, after);
}

// Through stdin, so rustfmt formats this file alone and not the out-of-line modules it declares. Running in the file's
// folder lets rustup pick the crate's pinned toolchain and rustfmt find any rustfmt.toml above it.
function formatRust(file) {
  const edition = crateEdition(file);
  if (!edition) return;
  const before = readFileSync(file, 'utf8');
  const result = spawnSync('rustfmt', ['--edition', edition, '--emit', 'stdout'], {
    cwd: path.dirname(file),
    input: before,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) return note(`rustfmt could not run: ${result.error.message}`);
  if (result.status !== 0) {
    return note(`rustfmt left ${path.relative(root, file)} as it was: ${result.stderr.trim().split('\n')[0]}`);
  }
  writeIfChanged(file, before, result.stdout);
}

async function formatPrettier(file) {
  const entry = path.join(root, 'node_modules/prettier/index.mjs');
  if (!existsSync(entry)) return note('prettier is not installed (run bun install)');
  const prettier = await import(pathToFileURL(entry).href);
  const before = readFileSync(file, 'utf8');
  const options = (await prettier.resolveConfig(file)) ?? {};
  try {
    writeIfChanged(file, before, await prettier.format(before, { ...options, filepath: file }));
  } catch (error) {
    note(`prettier left ${path.relative(root, file)} as it was: ${String(error.message).split('\n')[0]}`);
  }
}

export async function formatFile(file) {
  const relative = ownedPath(file);
  if (!relative || gitIgnored(relative)) return;
  const absolute = path.join(root, relative);
  if (relative.endsWith('.rs')) return formatRust(absolute);
  if (scope.prettierGlobs.some((glob) => path.matchesGlob(relative, glob))) return formatPrettier(absolute);
}

async function main() {
  let files = process.argv.slice(2).map((file) => path.resolve(file));
  if (files.length === 0) {
    try {
      files = editedPaths(JSON.parse(readStdin() || '{}'));
    } catch {
      return;
    }
  }
  for (const file of new Set(files)) await formatFile(file);
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => note(String(error?.message ?? error)));
}
