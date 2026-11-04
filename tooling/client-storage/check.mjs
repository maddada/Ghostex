import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
/**
 * CDXC:Build 2026-10-26 WHY:
 * TypeScript 7 (the `typescript` package, which runs every tsc gate) ships no JavaScript compiler API, so this parser-only lint loads Microsoft's TypeScript 6 API package instead. It is the repo's only importer of the compiler API; move it to the TypeScript 7.1 API (or drop the package) once that is stable.
 */
import ts from '@typescript/typescript6';

const root = fileURLToPath(new URL('../../', import.meta.url));
const roots = [
  'packages/shared',
  'packages/core-ui',
  'packages/components',
  'packages/client-storage',
  'apps/desktop/sidebar',
  'apps/desktop/views',
  'apps/desktop/src',
  'apps/desktop/native',
  'apps/editor/web',
];
const excluded = new Set([
  'node_modules',
  'bower_components',
  'vendor',
  '.git',
  '.hg',
  '.svn',
  'dist',
  'build',
  'out',
  '.next',
  '.nuxt',
  '.svelte-kit',
  '.astro',
  '.vite',
  '.turbo',
  '.cache',
  '.parcel-cache',
  'coverage',
  '.nyc_output',
  '.pytest_cache',
  '.mypy_cache',
  '.ruff_cache',
  '.tox',
  '.venv',
  'venv',
  'target',
  'DerivedData',
  '.gradle',
  '.idea',
  '.vscode',
  '.dependencies',
]);
const adapters = new Set([
  'packages/client-storage/adapters/browser.ts',
  'packages/client-storage/adapters/database.ts',
]);
// JavaScript that Ghostex injects into third-party pages, where no client-storage handle exists.
// Such a script may write only keys of stores marked `external` in packages/client-storage/catalog.ts.
const externalStoreScripts = new Set(['apps/desktop/src/app/helpers/browser.rs']);
const forbidden = new Set(['localStorage', 'sessionStorage', 'indexedDB', 'webkitIndexedDB', 'mozIndexedDB']);
function* sources(directory) {
  if (!fs.existsSync(directory)) return;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (excluded.has(entry.name)) continue;
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) yield* sources(file);
    else if (/\.(?:[cm]?[jt]sx?|rs|mm|m|html)$/.test(entry.name) && !/\.(?:test|generated)\./.test(entry.name))
      yield file;
  }
}
function literal(node) {
  if (ts.isStringLiteralLike(node)) return node.text;
  if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.PlusToken) {
    const left = literal(node.left),
      right = literal(node.right);
    return left !== undefined && right !== undefined ? left + right : undefined;
  }
  return undefined;
}
export function checkClientStorage() {
  const errors = [];
  for (const directory of roots)
    for (const file of sources(path.join(root, directory))) {
      const relative = path.relative(root, file).split(path.sep).join('/');
      if (adapters.has(relative) || externalStoreScripts.has(relative)) continue;
      const source = fs.readFileSync(file, 'utf8');
      const report = (offset) =>
        errors.push(
          `${relative}:${source.slice(0, offset).split('\n').length}: use a registered handle from packages/client-storage; direct browser storage is forbidden.`
        );
      if (/\.(?:rs|mm|m|html)$/.test(file)) {
        // Native code can inject JavaScript too. Ignore ordinary prose comments.
        const pattern =
          /\b(?:localStorage|sessionStorage|indexedDB)\s*(?:\.\s*(?:getItem|setItem|removeItem|clear|key|length|open|deleteDatabase)\b|\[)/g;
        for (const match of source.matchAll(pattern))
          if (!/^\s*(?:\/\/|\*|<!--)/.test(source.slice(source.lastIndexOf('\n', match.index) + 1, match.index)))
            report(match.index);
        continue;
      }
      const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
      function visit(node) {
        if (
          ts.isImportDeclaration(node) &&
          ts.isStringLiteral(node.moduleSpecifier) &&
          !relative.startsWith('packages/client-storage/') &&
          /client-storage\/(?:adapters|service|migration|handles|budgets|codecs)/.test(node.moduleSpecifier.text)
        ) {
          errors.push(
            `${relative}: import the public client-storage handle API; adapters and persistence internals are private.`
          );
        }
        if ((ts.isIdentifier(node) && forbidden.has(node.text)) || forbidden.has(literal(node))) {
          report(node.getStart(ast));
          return;
        }
        if (
          ts.isIdentifier(node) &&
          ['Storage', 'IDBFactory'].includes(node.text) &&
          !ts.isTypeReferenceNode(node.parent)
        ) {
          report(node.getStart(ast));
          return;
        }
        ts.forEachChild(node, visit);
      }
      visit(ast);
    }
  if (errors.length) throw new Error(`Unmanaged browser storage access:\n${errors.join('\n')}`);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  checkClientStorage();
  console.log('Client storage access check passed.');
}
