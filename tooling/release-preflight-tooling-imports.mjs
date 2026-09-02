#!/usr/bin/env node
import { readFile, readdir } from 'node:fs/promises';
import { builtinModules } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

/*
 CDXC:ReleaseAutomation 2026-09-02-12:40:
 Release 8.5.0's first Actions run (33462783752) failed only in the Android job:
 `bun run build:mobile-chat` died with ERR_MODULE_NOT_FOUND for `esbuild`, which
 tooling/build-mobile-chat.mjs imports directly but nothing declared in the root
 package.json. It resolved locally, and on every runner that happened to hoist it
 the same way, purely as a transitive of vite. That cost a full 21-minute recovery
 run. This check makes the rule explicit: every bare package specifier imported by
 a script under tooling/ must be declared in the root package.json, so the
 lockfile - not hoisting luck - decides whether a release script can load.

 Approach: comments are stripped and template-literal and regex-literal bodies
 are blanked with a small string-aware tokenizer, then the remaining source is
 matched for static imports,
 re-exports, dynamic import(), require(), and createRequire(...)() call sites.
 Specifiers are reduced to their package root before the package.json lookup.

 Template bodies are blanked on purpose: tooling/generate-mobile-tabler-icons.mjs
 emits a file whose `import ... from 'react-native-svg'` only resolves inside the
 apps/mobile/app submodule, and reporting it here would be a false positive.
 The cost is that source assembled from template strings and handed to a
 bundler - tooling/shiki-classic-assets.mjs builds an esbuild stdin entry that
 imports `@shikijs/themes` - is outside this scan; that package is declared in
 the root package.json by hand for the same reason esbuild is.
*/

const repoRoot = path.resolve(new URL('..', import.meta.url).pathname);
const toolingRoot = path.join(repoRoot, 'tooling');
const scannedExtensions = new Set(['.mjs', '.js', '.ts']);
const skippedDirectories = new Set(['node_modules', 'patches']);
const nodeBuiltins = new Set([...builtinModules, ...builtinModules.map((name) => `node:${name}`)]);

/*
 Comments are removed while quoted strings are kept intact, so a quoted specifier
 inside a comment cannot register as an import and a string that merely contains
 `//` cannot truncate the line. Template literals keep their backticks but lose
 their body: import specifiers are never backtick-quoted, and template bodies are
 where generated source (with its own imports) lives.
*/
function stripComments(source) {
  let output = '';
  let index = 0;
  let quote = null;
  while (index < source.length) {
    const char = source[index];
    const next = source[index + 1];
    if (quote) {
      const keep = quote !== '`';
      if (char === '\\') {
        output += keep ? char + (next ?? '') : '';
        index += 2;
        continue;
      }
      if (char === quote) {
        quote = null;
        output += char;
        index += 1;
        continue;
      }
      output += keep ? char : '';
      index += 1;
      continue;
    }
    if (char === '"' || char === "'" || char === '`') {
      quote = char;
      output += char;
      index += 1;
      continue;
    }
    if (char === '/' && next === '/') {
      const end = source.indexOf('\n', index);
      index = end === -1 ? source.length : end;
      continue;
    }
    if (char === '/' && next === '*') {
      const end = source.indexOf('*/', index + 2);
      index = end === -1 ? source.length : end + 2;
      continue;
    }
    if (char === '/' && startsRegexLiteral(output)) {
      index = skipRegexLiteral(source, index);
      output += '/re/';
      continue;
    }
    output += char;
    index += 1;
  }
  return output;
}

/*
 A `/` begins a regex literal when it cannot be a division: after an operator,
 an opening bracket, a separator, the start of the file, or a keyword such as
 `return`. Regex bodies routinely contain quotes (`/d="([^"]+)"/`), which would
 otherwise flip the string state and swallow real code.
*/
function startsRegexLiteral(precedingOutput) {
  const before = precedingOutput.trimEnd();
  if (before.length === 0) {
    return true;
  }
  if (/[(,=:[!&|?{};+\-*%<>~^]$/.test(before)) {
    return true;
  }
  return /(?:^|[^\w$])(?:return|typeof|instanceof|in|of|new|delete|void|throw|case|do|else|yield|await)$/.test(before);
}

function skipRegexLiteral(source, start) {
  let index = start + 1;
  let inClass = false;
  while (index < source.length) {
    const char = source[index];
    if (char === '\\') {
      index += 2;
      continue;
    }
    if (char === '\n') {
      return index;
    }
    if (inClass) {
      if (char === ']') {
        inClass = false;
      }
    } else if (char === '[') {
      inClass = true;
    } else if (char === '/') {
      index += 1;
      while (index < source.length && /[a-z]/.test(source[index])) {
        index += 1;
      }
      return index;
    }
    index += 1;
  }
  return index;
}

const specifierPatterns = [
  /\bimport\s*(?:[\w$*{}\s,]+?\s*from\s*)?["']([^"']+)["']/g,
  /\bexport\s*(?:[\w$*{}\s,]+?\s*from\s*)?["']([^"']+)["']/g,
  /\bimport\s*\(\s*["']([^"']+)["']/g,
  /\brequire\s*\(\s*["']([^"']+)["']/g,
  /\bcreateRequire\s*\([^)]*\)\s*\(\s*["']([^"']+)["']/g,
];

function collectSpecifiers(source) {
  const stripped = stripComments(source);
  const specifiers = new Set();
  const patterns = [...specifierPatterns];
  /*
   `const load = createRequire(import.meta.url); load('x')` binds the require
   function to an arbitrary name. Pick those names up and match their call sites.
  */
  for (const match of stripped.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*createRequire\s*\(/g)) {
    patterns.push(new RegExp(`\\b${match[1]}\\s*\\(\\s*["']([^"']+)["']`, 'g'));
  }
  for (const pattern of patterns) {
    for (const match of stripped.matchAll(pattern)) {
      specifiers.add(match[1]);
    }
  }
  return specifiers;
}

function packageRoot(specifier) {
  if (
    specifier.startsWith('.') ||
    specifier.startsWith('/') ||
    specifier.startsWith('#') ||
    specifier.startsWith('data:') ||
    specifier.startsWith('file:')
  ) {
    return null;
  }
  if (nodeBuiltins.has(specifier)) {
    return null;
  }
  const segments = specifier.split('/');
  const root = specifier.startsWith('@') ? segments.slice(0, 2).join('/') : segments[0];
  if (nodeBuiltins.has(root)) {
    return null;
  }
  return root;
}

async function listSourceFiles(directory) {
  const files = [];
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (skippedDirectories.has(entry.name)) {
        continue;
      }
      files.push(...(await listSourceFiles(entryPath)));
      continue;
    }
    if (entry.isFile() && scannedExtensions.has(path.extname(entry.name))) {
      files.push(entryPath);
    }
  }
  return files.sort();
}

async function declaredPackages() {
  const manifest = JSON.parse(await readFile(path.join(repoRoot, 'package.json'), 'utf8'));
  return new Set([...Object.keys(manifest.dependencies ?? {}), ...Object.keys(manifest.devDependencies ?? {})]);
}

/*
 Returns `{ files, violations }`. Each violation is
 `{ file, package }` with `file` relative to the repo root, so callers can print
 `<file>: imports '<pkg>' which is not declared in package.json` or fold the list
 into their own report.
*/
export async function scanToolingImports() {
  const [declared, files] = await Promise.all([declaredPackages(), listSourceFiles(toolingRoot)]);
  const violations = [];
  for (const file of files) {
    const source = await readFile(file, 'utf8');
    const seen = new Set();
    for (const specifier of collectSpecifiers(source)) {
      const root = packageRoot(specifier);
      if (!root || seen.has(root) || declared.has(root)) {
        continue;
      }
      seen.add(root);
      violations.push({ file: path.relative(repoRoot, file), package: root });
    }
  }
  return { files: files.map((file) => path.relative(repoRoot, file)), violations };
}

export function formatToolingImportViolation(violation) {
  return `${violation.file}: imports '${violation.package}' which is not declared in package.json`;
}

async function main() {
  const { files, violations } = await scanToolingImports();
  for (const violation of violations) {
    console.error(formatToolingImportViolation(violation));
  }
  if (violations.length > 0) {
    console.error(
      `\n${violations.length} undeclared package import(s) across ${files.length} tooling file(s). Declare each in the root package.json (dependencies or devDependencies) and run bun install.`
    );
    process.exitCode = 1;
    return;
  }
  console.log(`Every bare import in ${files.length} tooling file(s) is declared in package.json.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
