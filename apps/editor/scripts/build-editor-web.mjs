#!/usr/bin/env bun
import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const editorRoot = path.resolve(scriptDir, '..');
const repoRoot = path.resolve(editorRoot, '..', '..');
const webRoot = path.join(editorRoot, 'web');
const distRoot = path.join(editorRoot, 'dist', 'web');
const monacoSource = path.join(repoRoot, 'node_modules', 'monaco-editor', 'min', 'vs');
const monacoDest = path.join(distRoot, 'monaco', 'vs');
const bundleMarker = '__GHOSTEX_EDITOR_BUNDLE__';

const buildResult = await Bun.build({
  entrypoints: [path.join(webRoot, 'editor.ts')],
  format: 'iife',
  target: 'browser',
  write: false,
});

if (!buildResult.success) {
  for (const log of buildResult.logs) {
    console.error(log);
  }
  process.exit(1);
}

const bundle = await outputTextForJavaScript(buildResult.outputs);
const sourceHtml = await readFile(path.join(webRoot, 'index.html'), 'utf8');
if (!sourceHtml.includes(bundleMarker)) {
  throw new Error(`Missing ${bundleMarker} marker in apps/editor/web/index.html`);
}

const html = sourceHtml.replace(bundleMarker, bundle);

await rm(distRoot, { force: true, recursive: true });
await mkdir(path.dirname(monacoDest), { recursive: true });
await cp(monacoSource, monacoDest, { recursive: true });
await writeFile(path.join(distRoot, 'index.html'), html);

console.log(`Built ${path.relative(repoRoot, path.join(distRoot, 'index.html'))}`);
console.log(`Staged ${path.relative(repoRoot, monacoDest)}`);

async function outputTextForJavaScript(outputs) {
  const output = outputs.find((candidate) => candidate.path.endsWith('.js'));
  if (!output) {
    throw new Error('Bun.build did not produce a JavaScript bundle');
  }
  return output.text();
}
