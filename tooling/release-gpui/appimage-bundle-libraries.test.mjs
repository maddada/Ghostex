import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import {
  compareWithExpected,
  EXCLUDELIST_PATH,
  EXPECTED_BUNDLE,
  libraryNamesIn,
  parseExcludelist,
  parseLdd,
  parseNeeded,
  planBundle,
  unreviewedLibraryNames,
} from './appimage-bundle-libraries.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const excluded = parseExcludelist(readFileSync(EXCLUDELIST_PATH, 'utf8'));

/* DT_NEEDED of the 10.41.0 binaries (readelf -d), which failed the catalog test. */
const RELEASED_NEEDED = {
  x64: {
    Ghostex: [
      'libxkbcommon.so.0',
      'libxkbcommon-x11.so.0',
      'libxcb.so.1',
      'libgcc_s.so.1',
      'libm.so.6',
      'libc.so.6',
      'ld-linux-x86-64.so.2',
    ],
    'ghostex-gpui-runtime': [
      'libxkbcommon.so.0',
      'libxkbcommon-x11.so.0',
      'libcef.so',
      'libasound.so.2',
      'libxcb.so.1',
      'libgcc_s.so.1',
      'libm.so.6',
      'libc.so.6',
      'ld-linux-x86-64.so.2',
    ],
    'ghostex-gpui-cef-helper': ['libcef.so', 'libgcc_s.so.1', 'libc.so.6', 'ld-linux-x86-64.so.2'],
  },
  arm64: {
    Ghostex: [
      'libxkbcommon.so.0',
      'libxkbcommon-x11.so.0',
      'libfreetype.so.6',
      'libxcb.so.1',
      'libgcc_s.so.1',
      'libm.so.6',
      'libc.so.6',
      'ld-linux-aarch64.so.1',
    ],
    'ghostex-gpui-runtime': [
      'libxkbcommon.so.0',
      'libxkbcommon-x11.so.0',
      'libfreetype.so.6',
      'libcef.so',
      'libasound.so.2',
      'libxcb.so.1',
      'libgcc_s.so.1',
      'libm.so.6',
      'libc.so.6',
      'ld-linux-aarch64.so.1',
    ],
    'ghostex-gpui-cef-helper': ['libcef.so', 'libgcc_s.so.1', 'libc.so.6'],
  },
};

/* DT_NEEDED of the libraries on Ubuntu 22.04, the build runner. */
const UBUNTU_2204_NEEDED = {
  'libxkbcommon.so.0': ['libc.so.6'],
  'libxkbcommon-x11.so.0': ['libxkbcommon.so.0', 'libxcb.so.1', 'libxcb-xkb.so.1', 'libc.so.6'],
  'libxcb-xkb.so.1': ['libxcb.so.1', 'libc.so.6'],
  /* Host libraries: never walked, so libXau and libXdmcp stay the host's too. */
  'libxcb.so.1': ['libXau.so.6', 'libXdmcp.so.6', 'libc.so.6'],
};

/* Every library name the 10.41.0 executables mention (their dlopen targets included). */
const RELEASED_NAMES = {
  x64: 'libEGL.so libEGL.so.1 libasound.so.2 libc.so.6 libcef.so libgcc_s.so.1 libm.so.6 libvulkan.so.1 libwayland-client.so libwayland-client.so.0 libwayland-egl.so libwayland-egl.so.1 libxcb.so.1 libxkbcommon-x11.so.0 libxkbcommon.so.0',
  arm64:
    'libEGL.so libEGL.so.1 libasound.so.2 libc.so.6 libcef.so libfreetype.so.6 libgcc_s.so.1 libm.so.6 libvulkan.so.1 libwayland-client.so libwayland-client.so.0 libwayland-egl.so libwayland-egl.so.1 libxcb.so.1 libxkbcommon-x11.so.0 libxkbcommon.so.0',
};

function plan(arch, resolvable = (soname) => `/usr/lib/${soname}`) {
  return planBundle({
    entryNeeded: Object.values(RELEASED_NEEDED[arch]).flat(),
    excluded,
    neededOf: (soname) => UBUNTU_2204_NEEDED[soname] ?? [],
    resolve: resolvable,
  });
}

describe('the libraries the AppImage bundles', () => {
  test('the vendored excludelist keeps glibc, X11/xcb, GL, ALSA and freetype on the host, not xkbcommon', () => {
    for (const soname of [
      'libc.so.6',
      'libm.so.6',
      'libxcb.so.1',
      'libEGL.so.1',
      'libasound.so.2',
      'libfreetype.so.6',
      'libgcc_s.so.1',
    ]) {
      expect(excluded.has(soname)).toBe(true);
    }
    for (const soname of ['libxkbcommon.so.0', 'libxkbcommon-x11.so.0', 'libxcb-xkb.so.1']) {
      expect(excluded.has(soname)).toBe(false);
    }
  });

  test.each(['x64', 'arm64'])(
    '%s bundles libxkbcommon, libxkbcommon-x11 and libxcb-xkb, and stops at host libraries',
    (arch) => {
      const { bundle, missing } = plan(arch);
      expect(missing).toEqual([]);
      expect([...bundle.keys()].sort()).toEqual([...EXPECTED_BUNDLE[arch]].sort());
      expect(bundle.has('libXau.so.6')).toBe(false);
      expect(bundle.has('libcef.so')).toBe(false);
      expect(compareWithExpected(arch, bundle.keys())).toBeNull();
    }
  );

  test('a library the build host cannot resolve is reported instead of skipped', () => {
    const { missing } = plan('x64', (soname) => (soname === 'libxcb-xkb.so.1' ? null : `/usr/lib/${soname}`));
    expect(missing).toEqual(['libxcb-xkb.so.1']);
  });

  test('a new dependency fails against the reviewed list', () => {
    const message = compareWithExpected('x64', [...EXPECTED_BUNDLE.x64, 'libdbus-1.so.3']);
    expect(message).toContain('libdbus-1.so.3');
    expect(message).toContain('EXPECTED_BUNDLE');
  });

  test.each(['x64', 'arm64'])('every library name the %s executables mention is accounted for', (arch) => {
    const linked = new Set([...Object.values(RELEASED_NEEDED[arch]).flat(), ...Object.keys(UBUNTU_2204_NEEDED)]);
    const names = RELEASED_NAMES[arch].split(' ');
    expect(unreviewedLibraryNames({ bundled: new Set(EXPECTED_BUNDLE[arch]), excluded, linked, names })).toEqual([]);
    /* A dlopen target nobody reviewed fails the build. */
    expect(
      unreviewedLibraryNames({ bundled: new Set(), excluded, linked, names: [...names, 'libdbus-1.so.3'] })
    ).toEqual(['libdbus-1.so.3']);
  });

  test('reads names, DT_NEEDED and ldd the way the tools print them', () => {
    expect(libraryNamesIn(Buffer.from('\0libEGL.so.1libEGL.so\0Library libwayland-egl.so could not\0'))).toEqual([
      'libEGL.so',
      'libEGL.so.1',
      'libwayland-egl.so',
    ]);
    expect(
      parseNeeded(
        ' 0x0000000000000001 (NEEDED)             Shared library: [libxkbcommon.so.0]\n' +
          ' 0x0000000000000001 (NEEDED)             Shared library: [libc.so.6]\n' +
          ' 0x000000000000001d (RUNPATH)            Library runpath: [$ORIGIN]\n'
      )
    ).toEqual(['libxkbcommon.so.0', 'libc.so.6']);
    const resolved = parseLdd(
      '\tlinux-vdso.so.1 (0x00007ffd)\n' +
        '\tlibxkbcommon-x11.so.0 => /lib/x86_64-linux-gnu/libxkbcommon-x11.so.0 (0x00007f)\n' +
        '\tlibcef.so => not found\n' +
        '\t/lib64/ld-linux-x86-64.so.2 (0x00007f)\n'
    );
    expect(resolved.get('libxkbcommon-x11.so.0')).toBe('/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0');
    expect(resolved.get('libcef.so')).toBeNull();
    expect(resolved.has('linux-vdso.so.1')).toBe(false);
  });

  test('the bundle sits where the executables look: beside them, before the payload id is taken', () => {
    /* RUNPATH $ORIGIN is what finds the copies, in the AppImage and in the copy AppRun keeps. */
    expect(readFileSync(path.join(repoRoot, 'apps/desktop/build.rs'), 'utf8')).toContain(
      'cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN'
    );
    const script = readFileSync(path.join(repoRoot, 'tooling/release-gpui/linux-appimage.sh'), 'utf8');
    const bundling = script.indexOf('appimage-bundle-libraries.mjs" --payload "$APPDIR/opt/ghostex"');
    expect(bundling).toBeGreaterThan(script.indexOf('cp -a "$PACKAGE_ROOT/opt/ghostex" "$APPDIR/opt/ghostex"'));
    expect(bundling).toBeLessThan(script.indexOf('PAYLOAD_DIGEST='));
  });

  test('the Linux packages build on Ubuntu 22.04 and the AppImage is started on a clean offline host', () => {
    const workflow = readFileSync(path.join(repoRoot, '.github/workflows/release-gpui-linux.yml'), 'utf8');
    expect(workflow).toContain("runs-on: ${{ inputs.arch == 'arm64' && 'ubuntu-22.04-arm' || 'ubuntu-22.04' }}");
    expect(workflow).toMatch(/apt-get install -y [^\n]*\bpatchelf\b/u);
    expect(workflow).toContain('tooling/release-gpui/linux-appimage-smoke.sh');
    const smoke = readFileSync(path.join(repoRoot, 'tooling/release-gpui/linux-appimage-smoke.sh'), 'utf8');
    expect(smoke).toContain('FROM ubuntu:22.04');
    expect(smoke).toContain('docker run --rm --network none');
  });
});
