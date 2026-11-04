#!/usr/bin/env node
/*
 * CDXC:Release 2026-10-31 WHY:
 * The AppImage catalog test (AppImage/appimage.github.io PR #11084) ran the
 * 10.41.0 image on a clean Ubuntu 22.04 and it died before any window with
 * "libxkbcommon-x11.so.0: cannot open shared object file": the image carried
 * none of the libraries the app links, so it ran only where a distribution
 * package happened to have installed them. This copies every library the app
 * links that the AppImage community's excludelist does not promise on every
 * host (glibc, libGL/EGL, X11/xcb, ALSA, fontconfig, freetype and the like)
 * beside the executables, whose RUNPATH is $ORIGIN (apps/desktop/build.rs),
 * and gives each copy the same RUNPATH so its own dependencies resolve there
 * too. AppRun copies opt/ghostex whole into the user's data folder, so the
 * libraries travel with the executables, and the desktop entry, which starts
 * that copy without AppRun, needs no LD_LIBRARY_PATH.
 *
 * The bundled set must equal EXPECTED_BUNDLE, and every library name the
 * executables mention must be accounted for (DLOPEN_DECISIONS covers the ones
 * they open at runtime, which ldd never shows), so a new dependency fails the
 * release instead of the next user's first launch.
 * SEE-ALSO: tooling/packaging/appimage/excludelist (vendored),
 * tooling/release-gpui/linux-appimage.sh (the caller),
 * tooling/release-gpui/linux-appimage-smoke.sh (the clean offline launch).
 *
 * Usage: node appimage-bundle-libraries.mjs --payload <AppDir>/opt/ghostex --arch x64|arm64
 */
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
export const EXCLUDELIST_PATH = path.join(repoRoot, 'tooling/packaging/appimage/excludelist');

/* The ELF executables in opt/ghostex; gxserver, ghostex and zmx are static. */
export const ENTRY_POINTS = ['Ghostex', 'ghostex-gpui-runtime', 'ghostex-gpui-cef-helper'];

/* Installed later as the verified CEF component, never bundled (linux-appimage.sh checks). */
export const ON_DEMAND_LIBRARIES = new Set(['libcef.so']);

/* glibc pieces the vendored excludelist does not name for every architecture. */
export const HOST_LIBC_LIBRARIES = new Set(['ld-linux-aarch64.so.1']);

/*
 * Libraries the executables open with dlopen. Keyed by the name without its
 * version, since the code tries both (wgpu: libEGL.so.1, then libEGL.so).
 */
export const DLOPEN_DECISIONS = {
  libEGL: 'host: the GPU driver (excludelist)',
  'libwayland-client': 'host: excludelist; only used on a native Wayland session',
  'libwayland-egl':
    'host: only used on a native Wayland session (Ghostex runs as an X11 client whenever DISPLAY is set), where it ships with libwayland-client',
  libvulkan:
    'host: the Vulkan loader has to find the host drivers; without it wgpu renders through its GL backend (libEGL)',
};

/* What a release bundles today; review any change before adding it here. */
export const EXPECTED_BUNDLE = {
  arm64: ['libxcb-xkb.so.1', 'libxkbcommon-x11.so.0', 'libxkbcommon.so.0'],
  x64: ['libxcb-xkb.so.1', 'libxkbcommon-x11.so.0', 'libxkbcommon.so.0'],
};

/* The sonames the excludelist keeps on the host (one per line; '#' starts a comment). */
export function parseExcludelist(text) {
  return new Set(
    text
      .split('\n')
      .map((line) => line.replace(/#.*/u, '').trim())
      .filter(Boolean)
  );
}

/* `readelf -d` output: the file's DT_NEEDED sonames, in order. */
export function parseNeeded(readelfOutput) {
  return [...readelfOutput.matchAll(/\(NEEDED\)\s+Shared library: \[([^\]]+)\]/gu)].map((match) => match[1]);
}

/* `ldd` output: soname -> resolved path, or null when the loader cannot find it. */
export function parseLdd(lddOutput) {
  const resolved = new Map();
  for (const line of lddOutput.split('\n')) {
    const match = /^\s*(\S+) => (?:(\/\S+)|not found)/u.exec(line);
    if (match) resolved.set(match[1], match[2] ?? null);
  }
  return resolved;
}

export function isHostLibrary(soname, excluded) {
  return excluded.has(soname) || HOST_LIBC_LIBRARIES.has(soname);
}

/*
 * Walks DT_NEEDED from the entry points. Host and on-demand libraries end the
 * walk: their own dependencies are the host's business. Everything else is
 * bundled and walked in turn.
 * `neededOf(soname)` gives a bundled library's DT_NEEDED; `resolve(soname)` its path or null.
 */
export function planBundle({ entryNeeded, neededOf, resolve, excluded }) {
  const bundle = new Map();
  const missing = new Set();
  const queue = [...entryNeeded];
  while (queue.length > 0) {
    const soname = queue.shift();
    if (bundle.has(soname) || missing.has(soname)) continue;
    if (isHostLibrary(soname, excluded) || ON_DEMAND_LIBRARIES.has(soname)) continue;
    const resolved = resolve(soname);
    if (!resolved) {
      missing.add(soname);
      continue;
    }
    bundle.set(soname, resolved);
    queue.push(...neededOf(soname));
  }
  return { bundle, missing: [...missing].sort() };
}

/* The library names an executable mentions, the dlopen targets among them. */
export function libraryNamesIn(bytes) {
  const text = Buffer.from(bytes).toString('latin1');
  return [...new Set([...text.matchAll(/lib[A-Za-z0-9_+-]+\.so(?:\.\d+)*/gu)].map((match) => match[0]))].sort();
}

/* Names that are neither linked, host, on demand, bundled nor a reviewed dlopen target. */
export function unreviewedLibraryNames({ names, linked, bundled, excluded }) {
  return names.filter((name) => {
    if (linked.has(name) || bundled.has(name) || ON_DEMAND_LIBRARIES.has(name)) return false;
    if (isHostLibrary(name, excluded)) return false;
    return !Object.hasOwn(DLOPEN_DECISIONS, name.replace(/\.so(?:\.\d+)*$/u, ''));
  });
}

export function compareWithExpected(arch, bundled) {
  const expected = EXPECTED_BUNDLE[arch];
  if (!expected) throw new Error(`No expected AppImage library list for ${arch}`);
  const actual = [...bundled].sort();
  if (actual.join('\n') === [...expected].sort().join('\n')) return null;
  return (
    `The AppImage would bundle [${actual.join(', ')}], not the reviewed [${expected.join(', ')}] for ${arch}. ` +
    'Check each new library against tooling/packaging/appimage/excludelist and update EXPECTED_BUNDLE in ' +
    'tooling/release-gpui/appimage-bundle-libraries.mjs.'
  );
}

function run(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
}

function bundleLibraries(payload, arch) {
  const excluded = parseExcludelist(readFileSync(EXCLUDELIST_PATH, 'utf8'));
  const entries = ENTRY_POINTS.map((name) => path.join(payload, name));
  for (const entry of entries) {
    if (!existsSync(entry)) throw new Error(`The AppImage payload has no ${path.basename(entry)}`);
  }
  /* Every entry's ldd closure, so a library deep in the tree resolves the way the loader would. */
  const resolved = new Map();
  for (const entry of entries) {
    for (const [soname, file] of parseLdd(run('ldd', [entry]))) {
      if (file || !resolved.has(soname)) resolved.set(soname, file);
    }
  }
  const entryNeeded = entries.flatMap((entry) => parseNeeded(run('readelf', ['-d', entry])));
  const { bundle, missing } = planBundle({
    entryNeeded,
    excluded,
    neededOf: (soname) => parseNeeded(run('readelf', ['-d', resolved.get(soname)])),
    resolve: (soname) => resolved.get(soname) ?? null,
  });
  if (missing.length > 0) {
    throw new Error(`The build host cannot resolve ${missing.join(', ')}; install its -dev package first`);
  }
  const mismatch = compareWithExpected(arch, bundle.keys());
  if (mismatch) throw new Error(mismatch);
  const linked = new Set(resolved.keys());
  for (const entry of entries) {
    const unreviewed = unreviewedLibraryNames({
      bundled: new Set(bundle.keys()),
      excluded,
      linked,
      names: libraryNamesIn(readFileSync(entry)),
    });
    if (unreviewed.length > 0) {
      throw new Error(
        `${path.basename(entry)} mentions ${unreviewed.join(', ')}, which is neither linked nor a reviewed dlopen ` +
          'target; decide whether the AppImage bundles it (DLOPEN_DECISIONS in appimage-bundle-libraries.mjs)'
      );
    }
  }
  for (const [soname, file] of bundle) {
    const target = path.join(payload, soname);
    copyFileSync(file, target);
    chmodSync(target, 0o644);
    run('patchelf', ['--set-rpath', '$ORIGIN', target]);
    process.stdout.write(`Bundled ${soname} from ${file}\n`);
  }
  return [...bundle.keys()];
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const option = (name) => {
    const index = args.indexOf(name);
    return index >= 0 ? args[index + 1] : undefined;
  };
  const payload = option('--payload');
  const arch = option('--arch');
  if (!payload || !arch) {
    console.error('Usage: node appimage-bundle-libraries.mjs --payload <AppDir>/opt/ghostex --arch x64|arm64');
    process.exit(2);
  }
  try {
    bundleLibraries(path.resolve(payload), arch);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
