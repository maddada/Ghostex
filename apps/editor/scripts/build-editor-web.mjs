#!/usr/bin/env bun
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const editorRoot = path.resolve(scriptDir, '..');
const repoRoot = path.resolve(editorRoot, '..', '..');
const webRoot = path.join(editorRoot, 'web');
const distRoot = path.join(editorRoot, 'dist', 'web');
const bundleMarker = '__GHOSTEX_EDITOR_BUNDLE__';

const buildResult = await Bun.build({
  entrypoints: [path.join(webRoot, 'editor.ts')],
  format: 'iife',
  target: 'browser',
  write: false,
  minify: true,
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
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

const css = await Promise.all(
  buildResult.outputs.filter((output) => output.path.endsWith('.css')).map((output) => output.text())
);
const html = sourceHtml
  .replace('__GHOSTEX_EDITOR_STYLES__', () => css.join('\n'))
  .replace(bundleMarker, () => bundle.replace(/<\/script/gi, '<\\/script'));

await rm(distRoot, { force: true, recursive: true });
await mkdir(distRoot, { recursive: true });
await writeFile(path.join(distRoot, 'index.html'), html);

console.log(`Built ${path.relative(repoRoot, path.join(distRoot, 'index.html'))}`);
console.log(`Editor JavaScript: ${Buffer.byteLength(bundle)} bytes; CSS: ${Buffer.byteLength(css.join('\n'))} bytes`);

async function outputTextForJavaScript(outputs) {
  const output = outputs.find((candidate) => candidate.path.endsWith('.js'));
  if (!output) {
    throw new Error('Bun.build did not produce a JavaScript bundle');
  }
  return output.text();
}
