import fs from 'node:fs/promises';
import path from 'node:path';
import * as esbuild from 'esbuild';

/**
 * CDXC:Docs 2026-09-11 WHY:
 * Docs shipped a 13.5 MB HTML entry even when opening a small Markdown file.
 * Split its editor graph into classic-script modules because CEF file origins cannot load external ES modules; a single module registry preserves shared React and editor contexts.
 */
export async function writeDocsClassicAssets(outDir: string, options: esbuild.BuildOptions): Promise<string> {
  const runtimeDir = path.join(outDir, 'docs-runtime');
  const result = await esbuild.build({
    ...options,
    outdir: runtimeDir,
    entryNames: '[name]-[hash]',
    chunkNames: 'chunks/[name]-[hash]',
    splitting: true,
    metafile: true,
    format: 'esm',
    write: false,
  });
  const root = options.absWorkingDir!;
  const entryPoint = (options.entryPoints as string[])[0];
  let entryId: string | undefined;
  for (const output of result.outputFiles ?? []) {
    if (!output.path.endsWith('.js')) continue;
    const metadata = result.metafile!.outputs[path.relative(root, output.path).replaceAll('\\', '/')];
    if (!metadata) throw new Error(`Missing Docs module metadata: ${output.path}`);
    const id = path.relative(outDir, output.path).replaceAll('\\', '/');
    if (metadata.entryPoint && path.resolve(root, metadata.entryPoint) === entryPoint) entryId = id;
    const dependencies = metadata.imports
      .filter((entry) => !entry.external && entry.kind !== 'dynamic-import')
      .map((entry) => path.relative(outDir, path.resolve(root, entry.path)).replaceAll('\\', '/'));
    const transformed = await esbuild.transform(output.text.replaceAll('import.meta.url', '__moduleUrl'), {
      format: 'cjs',
      target: 'chrome120',
      minify: true,
    });
    const code = transformed.code.replace(/\bimport\(("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')\)/g, '__load($1)');
    const script = `globalThis.__ghostexDocsModules.define(${JSON.stringify(id)},${JSON.stringify(dependencies)},function(module,exports,require,__load,__moduleUrl){${code}\n});`;
    await fs.mkdir(path.dirname(output.path), { recursive: true });
    await fs.writeFile(output.path, script);
  }
  if (!entryId) throw new Error('Docs did not emit its entry module.');
  return `(${installDocsModuleLoader.toString()})();globalThis.__ghostexDocsModules.import(${JSON.stringify(entryId)}).catch(error=>{document.getElementById('root').textContent='Could not load Docs: '+error.message;});`;
}

function installDocsModuleLoader() {
  const definitions = new Map<string, { dependencies: string[]; factory: Function }>();
  const modules = new Map<string, { exports: unknown }>();
  const scripts = new Map<string, Promise<void>>();
  const resolve = (id: string, from = document.baseURI) => new URL(id, from).href;
  const loadScript = (id: string) => {
    if (definitions.has(id)) return Promise.resolve();
    const pending = scripts.get(id);
    if (pending) return pending;
    const promise = new Promise<void>((done, reject) => {
      const script = document.createElement('script');
      script.src = id;
      script.onload = () => {
        script.remove();
        if (definitions.has(id)) done();
        else {
          scripts.delete(id);
          reject(new Error('Docs module did not register.'));
        }
      };
      script.onerror = () => {
        script.remove();
        scripts.delete(id);
        reject(new Error('Could not load a Docs editor module.'));
      };
      document.head.append(script);
    });
    scripts.set(id, promise);
    return promise;
  };
  const prepare = async (id: string, seen = new Set<string>()) => {
    if (seen.has(id)) return;
    seen.add(id);
    await loadScript(id);
    await Promise.all(definitions.get(id)!.dependencies.map((dependency) => prepare(resolve(dependency), seen)));
  };
  const execute = (id: string): unknown => {
    const cached = modules.get(id);
    if (cached) return cached.exports;
    const definition = definitions.get(id);
    if (!definition) throw new Error('Docs module is unavailable.');
    const module = { exports: {} as unknown };
    modules.set(id, module);
    try {
      definition.factory(
        module,
        module.exports,
        (dependency: string) => execute(resolve(dependency, id)),
        (dependency: string) => importModule(resolve(dependency, id)),
        id
      );
    } catch (error) {
      modules.delete(id);
      throw error;
    }
    return module.exports;
  };
  const importModule = async (id: string) => {
    const absolute = resolve(id);
    await prepare(absolute);
    return execute(absolute);
  };
  Object.assign(globalThis, {
    __ghostexDocsModules: {
      define(id: string, dependencies: string[], factory: Function) {
        definitions.set(resolve(id), { dependencies, factory });
      },
      import: importModule,
    },
  });
}
