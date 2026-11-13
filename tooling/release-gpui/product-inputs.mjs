/*
 * CDXC:Release 2026-08-13:
 * Executable documentation of every release product's fingerprint inputs.
 *
 * This module is data only: no I/O, no git, no network. `fingerprint.mjs` turns
 * these declarations into content fingerprints and `plan.mjs` turns fingerprints
 * into build/reuse/skip decisions. Anything that can change the bytes of a
 * published artifact must appear here, because an under-declared input is the
 * one failure mode that can silently reuse a stale artifact.
 *
 * Changing this file's declarations REQUIRES bumping
 * FINGERPRINT_ALGORITHM_REVISION in fingerprint.mjs so older provenance records
 * are ignored instead of silently compared against a different input set.
 */

export const TRUSTED_REPO = 'maddada/Ghostex';

/*
 * Pinned toolchains. These mirror the workflow pins; `product-inputs.test.mjs`
 * asserts they still match the workflow files, so drift fails a test instead of
 * silently reusing an artifact built with a different compiler.
 */
export const TOOLCHAIN = Object.freeze({
  androidBuildTools: '37.0.0',
  androidNdk: '29.0.14206865',
  androidPlatform: 'android-37.0',
  bun: '1.4.4',
  dotnet: '8.0.x',
  java: '17.0.19+10',
  node: '24.22.1',
  ripgrepPackageVersion: '1.17.1',
  ripgrepSha256: '2fa16464fd8638588a67c7fc172d3c4b57fbdc65dff366e10b0b0e90734628a6',
  ripgrepVersion: 'v15.0.1',
  /*
   * The Android job builds the tailcat gomobile bridge from
   * apps/mobile/tailcat-bridge before Gradle compiles against it. Both pins are
   * asserted against that module's go.mod and against the workflow file below.
   */
  tailcatBridgeGo: '1.27.1',
  tailcatBridgeGomobile: 'v0.0.0-20260821190718-4776eadac327',
  /*
   * CDXC:CloudBoxes 2026-11-02 SEE-ALSO: the tailcat every gxserver Linux package
   * carries in bin/ (and so every Linux desktop package, at
   * /opt/ghostex/gxserver/bin/tailcat), built by server/package-remote-linux.mjs
   * with `go install` of this module version (Go's checksum database verifies the
   * module) using the Go that release-gpui-gxserver.yml installs. Asserted against
   * that script, server/src/tailcat/install.rs and the workflows.
   */
  tailcat: 'v0.6.0',
  tailcatGo: '1.27.1',
  vpk: '1.2.0',
  /*
   * One Zig toolchain for the whole repo. Ghostty and the zmx fork moved from
   * 0.16 to 0.17 together on 2026-10-26, when both upstreams required 0.17.
   */
  zig: '0.17.1',
});

/* Mirrors CODE_SERVER_COMPONENT_IDENTITY_REVISION; asserted equal in the tests. */
export const CODE_SERVER_IDENTITY_REVISION = 'p2';

export const SPARKLE_FEED_URL = 'https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml';

/*
 * Top-level tracked paths that deliberately contribute to no product.
 * `product-inputs.test.mjs` fails when a new top-level entry appears that is
 * neither claimed by a product pathspec nor listed here, so a new directory
 * cannot silently escape the fingerprint.
 */
export const IGNORED_FOR_RELEASE = Object.freeze([
  { path: '.beads', why: 'Project board database; never compiled into an artifact.' },
  { path: '.cargo', why: 'Local dev profile settings; release builds use their own profiles.' },
  { path: '.claude', why: 'Agent skills for repo maintainers (the release procedure); dev tooling only.' },
  { path: '.editorconfig', why: 'Editor configuration only.' },
  { path: '.gitattributes', why: 'Git checkout behavior only; release jobs use fresh clones.' },
  { path: '.gitignore', why: 'Ignore rules only; tracked inputs are enumerated explicitly.' },
  { path: '.oxfmtrc.json', why: 'Formatter settings for the format check and agents; changes no artifact.' },
  { path: 'AGENTS.md', why: 'Agent instructions; metadata only.' },
  { path: 'ai', why: 'Agent reference material such as the CDXC area list; metadata only.' },
  { path: 'ANALYTICS.md', why: 'Analytics policy documentation; never compiled into a release artifact.' },
  { path: 'CHANGELOG.md', why: 'Release notes source; metadata only (§4.11 rule 8).' },
  { path: 'CLAUDE.md', why: 'Agent instructions; metadata only.' },
  { path: 'CONTRIBUTING.md', why: 'Contributor guide; metadata only.' },
  { path: 'docs', why: 'Design and implementation documentation; never compiled into a release artifact.' },
  { path: 'LICENSE', why: 'Metadata only.' },
  { path: 'README.md', why: 'Metadata only.' },
  { path: 'TERMS.md', why: 'Project glossary for agents and contributors; metadata only.' },
  { path: 'appcast.xml', why: 'Sparkle feed output written by the publisher, not a build input.' },
  { path: 'claude-code-codex-keybindings.json', why: 'Developer keybindings; not packaged.' },
  { path: 'favicon.png', why: 'Web asset for local tooling; not packaged by any release job.' },
  { path: 'apps/history-cli', why: 'Local history CLI; not packaged by any release job.' },
  {
    path: '.dependencies/ghostty-patches',
    why: 'Source-sync overlay only; release jobs compile the already-patched tracked ghostty tree.',
  },
  {
    path: 'apps/gpui-web',
    why: 'GPUI web build; built from a checkout with `cargo xtask web-build`, never part of a GPUI release artifact.',
  },
  {
    path: 'vitest.config.ts',
    why: 'Test runner configuration; release:test uses tooling/release-gpui/vitest.release.config.ts.',
  },
]);

/*
 * Added to every product. `package.json` is hashed through the `package-json`
 * projection so a lone version bump cannot invalidate products that are not
 * version-stamped (§4.11 rule 8).
 */
export const SHARED_BASE_PATHSPECS = Object.freeze([
  { pathspec: 'package.json', projection: 'package-json' },
  { pathspec: 'bun.lock' },
  { pathspec: '.gitmodules' },
  { pathspec: 'tooling/release-gpui/common.sh' },
  { pathspec: 'tooling/release-gpui/product-inputs.mjs' },
  { pathspec: 'tooling/release-gpui/fingerprint.mjs' },
  { pathspec: '.github/workflows/release-gpui.yml' },
]);

/*
 * FINGERPRINT_ALGORITHM_REVISION is not repeated here: fingerprint.mjs already
 * mixes it into every digest as the leading prefix (§3.4 step 5). Keeping it out
 * of this module lets product-inputs.mjs stay import-free.
 */
export const SHARED_BASE_VALUES = Object.freeze({
  bun: TOOLCHAIN.bun,
});

/*
 * §4.11 rules 1, 2 and 4: every desktop package embeds a gxserver payload,
 * matches GXSERVER_PROTOCOL_VERSION at runtime, builds its CEF surfaces from the
 * shared React trees, and patches gpui/zed/cef-rs from .dependencies gitlinks.
 */
const DESKTOP_APP_PATHSPECS = Object.freeze([
  { pathspec: 'apps/desktop/**' },
  { pathspec: ':(exclude)apps/desktop/build' },
  { pathspec: ':(exclude)apps/desktop/target' },
  { pathspec: 'packages/paths/**' },
  { pathspec: 'packages/find/**' },
  { pathspec: ':(exclude)packages/find/target' },
  /* The Settings rows, defaults and hotkeys, compiled into the desktop app and gxserver. */
  { pathspec: 'packages/settings-catalog/**' },
  { pathspec: ':(exclude)packages/settings-catalog/target' },
  { pathspec: 'server/**' },
  { pathspec: ':(exclude)server/target' },
  { pathspec: 'packages/shared/**' },
  { pathspec: 'packages/core-ui/**' },
  { pathspec: 'packages/components/**' },
  /*
   * CDXC:Dependencies 2026-11-06: components.json (the shadcn CLI's config) was deleted with the shadcn package. The
   * pathspec string is part of every desktop digest (`path\0<pathspec>\0…`), so dropping it would need a
   * FINGERPRINT_ALGORITHM_REVISION bump and a full rebuild; `allowMissing` is not hashed, so it stays declared and
   * matches nothing. Remove it together with the next revision bump.
   */
  { pathspec: 'components.json', allowMissing: true },
  /*
   * Compiled in with include_str! by server/src/agent_model_catalog.rs and
   * packages/gx-chat-core/src/menus/picker/settle.rs, so the snapshot is baked
   * into gxserver and every chat. The runtime refresh from
   * raw.githubusercontent.com does not remove it as a build input.
   */
  { pathspec: 'agent-model-catalog.json' },
  /*
   * The theme presets' terminal palettes, embedded by apps/desktop/build.rs, and the licence
   * notices every desktop package ships beside the app. Purely additive and desktop-only, so
   * FINGERPRINT_ALGORITHM_REVISION is not bumped (same reasoning as the android product's note below).
   */
  { pathspec: '.dependencies/theme-presets/**' },
  { pathspec: 'THIRD_PARTY_NOTICES.md' },
  { pathspec: '.dependencies/ghostty/**' },
  { pathspec: ':(exclude).dependencies/ghostty/.zig-cache' },
  { pathspec: ':(exclude).dependencies/ghostty/zig-out' },
  { pathspec: '.dependencies/zed' },
  { pathspec: '.dependencies/cef-rs' },
  { pathspec: '.dependencies/gpui-component' },
  { pathspec: 'tooling/release-gpui/prepare-references.sh' },
  { pathspec: 'tooling/release-gpui/create-deterministic-tar.sh' },
  { pathspec: 'tooling/release-gpui/publish-component.mjs' },
  { pathspec: 'tooling/release-gpui/component-signing.mjs' },
  { pathspec: 'tooling/release-gpui/on-demand-manifest.mjs' },
  { pathspec: 'tooling/release-gpui/patches' },
  { pathspec: 'tsconfig.json' },
]);

/* Inputs shared by both gxserver Linux architectures. */
const GXSERVER_PATHSPECS = Object.freeze([
  { pathspec: 'server/**' },
  { pathspec: ':(exclude)server/target' },
  { pathspec: 'packages/paths/**' },
  { pathspec: 'packages/find/**' },
  { pathspec: ':(exclude)packages/find/target' },
  { pathspec: 'packages/settings-catalog/**' },
  { pathspec: ':(exclude)packages/settings-catalog/target' },
  /* The chat core's Files and links rules, which `readSessionFilesLinks` runs. */
  { pathspec: 'packages/gx-chat-core/**' },
  { pathspec: ':(exclude)packages/gx-chat-core/target' },
  { pathspec: 'packages/gx-protocol/**' },
  { pathspec: ':(exclude)packages/gx-protocol/target' },
  { pathspec: '.dependencies/zmx' },
  /* server/package-remote-linux.mjs stages the bundled skills catalog into the package. */
  { pathspec: 'skills/**' },
  { pathspec: 'tooling/build-remote-gxserver-linux-release.sh' },
  { pathspec: 'tooling/release-shared.mjs' },
  { pathspec: 'tooling/release-gpui/prepare-references.sh' },
  /*
   * release-gpui-runtime.yml was split into the gxserver package workflow and
   * the immutable code-server component workflow; only the package half builds
   * these bytes.
   */
  { pathspec: '.github/workflows/release-gpui-gxserver.yml' },
]);

function windowsArtifacts(version, arch) {
  const channel = `win-${arch}-stable`;
  return [
    `ghostex-${version}-windows-${arch}.exe`,
    `releases.${channel}.json`,
    `Ghostex-${version}-${channel}-full.nupkg`,
  ];
}

function windowsOptionalArtifacts(version, arch) {
  const channel = `win-${arch}-stable`;
  return [`assets.${channel}.json`, `RELEASES-${channel}`, `Ghostex-${version}-${channel}-delta.nupkg`];
}

function gxserverProduct(arch) {
  return {
    artifacts: () => [`gxserver-linux-${arch}.tar.gz`],
    composedFrom: [],
    id: `gxserver-linux-${arch}`,
    kind: 'product',
    pathspecs: GXSERVER_PATHSPECS,
    platform: { arch, os: 'linux', runnerLabel: arch === 'arm64' ? 'ubuntu-24.04-arm' : 'ubuntu-24.04' },
    scopeFlag: arch === 'arm64' ? 'gxserverLinuxArm64' : 'gxserverLinuxX64',
    sideFiles: [],
    signing: { mode: 'unsigned' },
    values: {
      arch,
      tailcat: TOOLCHAIN.tailcat,
      tailcatGo: TOOLCHAIN.tailcatGo,
      zig: TOOLCHAIN.zig,
    },
    versionStamped: false,
  };
}

/*
 * The four Linux desktop packages of each architecture are one compile and
 * four wrappers over the same staged package root (§7): .deb, .rpm, the
 * prefix-preserving `.tar.zst` that Arch, ubi, and mise install straight from
 * the release page, and the AppImage (GitHub issue #320). ARM64 builds natively
 * on ubuntu-22.04-arm with the same scripts; its names use each format's own
 * spelling of the architecture. Both build on Ubuntu 22.04 for its glibc 2.35
 * floor (CDXC:Release 2026-10-31 in release-gpui-linux.yml).
 */
const LINUX_DESKTOP_ARTIFACT_NAMES = Object.freeze({
  x64: Object.freeze({
    deb: (version) => [`ghostex_${version}_amd64.deb`],
    rpm: (version) => [`ghostex-${version}-1.x86_64.rpm`],
    tar: (version) => [`ghostex-${version}-linux-x64.tar.zst`],
    appimage: (version) => [`ghostex-${version}-linux-x64.AppImage`, `ghostex-${version}-linux-x64.AppImage.zsync`],
  }),
  arm64: Object.freeze({
    deb: (version) => [`ghostex_${version}_arm64.deb`],
    rpm: (version) => [`ghostex-${version}-1.aarch64.rpm`],
    tar: (version) => [`ghostex-${version}-linux-arm64.tar.zst`],
    appimage: (version) => [`ghostex-${version}-linux-arm64.AppImage`, `ghostex-${version}-linux-arm64.AppImage.zsync`],
  }),
});

export const LINUX_DESKTOP_FORMATS = Object.freeze(['deb', 'rpm', 'tar', 'appimage']);

/* x64 keeps its original flags so existing scopes and recorded plans read unchanged. */
const LINUX_DESKTOP_SCOPE_FLAGS = Object.freeze({
  x64: Object.freeze({ deb: 'linuxDeb', rpm: 'linuxRpm', tar: 'linuxTar', appimage: 'linuxAppimage' }),
  arm64: Object.freeze({
    deb: 'linuxDebArm64',
    rpm: 'linuxRpmArm64',
    tar: 'linuxTarArm64',
    appimage: 'linuxAppimageArm64',
  }),
});

export function linuxDesktopProductId(format, arch) {
  return `linux-${format}-${arch}`;
}

function linuxDesktopProduct(format, arch = 'x64') {
  return {
    artifacts: LINUX_DESKTOP_ARTIFACT_NAMES[arch][format],
    composedFrom: [`gxserver-linux-${arch}`, 'code-server', 'cef'],
    id: linuxDesktopProductId(format, arch),
    kind: 'product',
    pathspecs: [
      ...DESKTOP_APP_PATHSPECS,
      /* Linux and Windows stage the completion and copy sounds in resources/sounds. */
      { pathspec: 'media/sounds/**' },
      { pathspec: 'tooling/release-gpui/linux-stage.sh' },
      { pathspec: `tooling/release-gpui/linux-${format}.sh` },
      ...(format === 'appimage'
        ? [
            { pathspec: 'tooling/packaging/appimage/**' },
            { pathspec: 'tooling/release-gpui/appimage-update-info.mjs' },
            { pathspec: 'tooling/release-gpui/appimage-signature.mjs' },
            { pathspec: 'tooling/release-gpui/appimage-bundle-libraries.mjs' },
            { pathspec: 'tooling/release-gpui/linux-appimage-smoke.sh' },
          ]
        : []),
      { pathspec: '.github/workflows/release-gpui-linux.yml' },
    ],
    platform: { arch, os: 'linux', runnerLabel: arch === 'arm64' ? 'ubuntu-22.04-arm' : 'ubuntu-22.04' },
    scopeFlag: LINUX_DESKTOP_SCOPE_FLAGS[arch][format],
    sideFiles: [],
    signing: { mode: 'unsigned' },
    /* `arch` is added for ARM64 only, so the x64 input sets (and their fingerprints' meaning) stay as they were. */
    values: {
      ...(arch === 'arm64' ? { arch } : {}),
      packageFormat: format,
      /* The package ships the gxserver package's tailcat, and the release job checks its version. */
      tailcat: TOOLCHAIN.tailcat,
      zig: TOOLCHAIN.zig,
    },
    versionStamped: true,
  };
}

function windowsProduct(arch) {
  return {
    artifacts: (version) => windowsArtifacts(version, arch),
    composedFrom: [`gxserver-linux-${arch}`, 'code-server', 'cef'],
    id: `windows-${arch}`,
    kind: 'product',
    optionalArtifacts: (version) => windowsOptionalArtifacts(version, arch),
    pathspecs: [
      ...DESKTOP_APP_PATHSPECS,
      { pathspec: 'media/sounds/**' },
      { pathspec: 'tooling/release-gpui/windows.ps1' },
      { pathspec: 'tooling/release-gpui/windows-build-watchdog.ps1' },
      { pathspec: 'tooling/release-gpui/vpk-signpath-hook.ps1' },
      { pathspec: 'tooling/release-gpui/certum-sign.ps1' },
      { pathspec: 'tooling/release-gpui/certum-simplysign-login.ps1' },
      { pathspec: 'tooling/release-gpui/certum-verify-package.ps1' },
      { pathspec: '.signpath/artifact-configuration.xml' },
      { pathspec: 'tooling/release-gpui/prepare-zig.ps1' },
      { pathspec: 'tooling/release-gpui/windows-update-feed.mjs' },
      { pathspec: 'tooling/release-gpui/verify-code-server-archive.mjs' },
      { pathspec: '.github/workflows/release-gpui-windows.yml' },
      /*
       * CDXC:Release 2026-09-01:
       * `.github/workflows/release-gpui-validate.yml` is deliberately NOT an input.
       * It was one while it gated the Windows packaging jobs; now that it is an
       * opt-in workflow_dispatch check outside every release run, it cannot affect
       * a single byte of the Windows output, and keeping it here would rebuild
       * Windows for a comment edit in a workflow the release never invokes.
       */
    ],
    platform: { arch, os: 'windows', runnerLabel: arch === 'arm64' ? 'windows-11-vs2026-arm' : 'windows-2025' },
    scopeFlag: arch === 'arm64' ? 'windowsArm64' : 'windowsX64',
    sideFiles: [],
    signing: { mode: (context) => (context.scope.signWindows ? 'authenticode' : 'unsigned') },
    values: {
      arch,
      dotnet: TOOLCHAIN.dotnet,
      /* sign_windows changes the produced bytes and the release notes. */
      signingMode: (context) => (context.scope.signWindows ? 'authenticode' : 'unsigned'),
      vpk: TOOLCHAIN.vpk,
      zigPin: TOOLCHAIN.zig,
    },
    versionStamped: true,
  };
}

function wslProduct(arch) {
  return {
    artifacts: () => [`gxserver-wsl-windows-${arch}.zip`],
    composedFrom: [`gxserver-linux-${arch}`],
    id: `gxserver-wsl-windows-${arch}`,
    kind: 'product',
    pathspecs: [
      { pathspec: 'tooling/release-gpui/wsl-runtime.sh' },
      { pathspec: 'tooling/release-gpui/install-gxserver-wsl.ps1' },
      { pathspec: '.github/workflows/release-gpui-wsl-runtime.yml' },
    ],
    platform: { arch, os: 'windows', runnerLabel: 'ubuntu-24.04' },
    scopeFlag: arch === 'arm64' ? 'gxserverWslWindowsArm64' : 'gxserverWslWindowsX64',
    sideFiles: [],
    signing: { mode: 'unsigned' },
    values: { arch },
    versionStamped: true,
  };
}

/*
 * Fingerprint nodes that are not published products. They exist so their inputs
 * compose into the products that embed or seal them (§3.4 part 3).
 */
const COMPOSED_NODES = {
  cef: {
    /*
     * §4.3: the component identity is CEF_VERSION, which is a pure function of
     * the pinned cef-rs checkout. Fingerprinting the gitlink keeps planning
     * offline and submodule-checkout free while tracking the identity exactly.
     */
    composedFrom: [],
    id: 'cef',
    kind: 'component',
    pathspecs: [{ pathspec: '.dependencies/cef-rs' }],
    values: {},
    versionStamped: false,
  },
  'code-server': {
    /*
     * §4.2: the component identity is
     * `<12-hex code-server HEAD>-p2-<payload fingerprint>`. Both halves are
     * determined by the code-server gitlink and the identity revision, so the
     * gitlink plus the identity-revision inputs is an exact, offline stand-in.
     * The identity-revision inputs below change the produced archive without
     * changing the upstream payload, so they must invalidate the build.
     */
    composedFrom: [],
    id: 'code-server',
    identityRevisionPathspecs: [
      { pathspec: 'tooling/release-gpui/patches/code-server-ripgrep-target-validation.patch' },
      { pathspec: '.github/workflows/release-gpui-code-server.yml' },
      { pathspec: '.github/workflows/release-gpui-code-server-windows.yml' },
      /* Lives inside the code-server gitlink, so it never appears in this tree. */
      { pathspec: '.dependencies/code-server/.node-version', allowMissing: true },
    ],
    kind: 'component',
    pathspecs: [
      { pathspec: '.dependencies/code-server' },
      /* The native Windows editor recipe is part of the component identity (CODE_SERVER_RECIPE_INPUTS). */
      { pathspec: 'apps/desktop/scripts/build-windows-code-server.ps1' },
      { pathspec: 'tooling/release-gpui/code-server-component-identity.mjs' },
      { pathspec: 'tooling/release-gpui/patches/code-server-ripgrep-target-validation.patch' },
      { pathspec: 'tooling/release-gpui/verify-code-server-archive.mjs' },
      { pathspec: 'tooling/release-gpui/verify-windows-native-code-server-archive.mjs' },
      { pathspec: '.github/workflows/release-gpui-code-server.yml' },
      { pathspec: '.github/workflows/release-gpui-code-server-windows.yml' },
    ],
    values: { identityRevision: CODE_SERVER_IDENTITY_REVISION, node: TOOLCHAIN.node },
    versionStamped: false,
  },
};

const PRODUCT_LIST = [
  gxserverProduct('x64'),
  gxserverProduct('arm64'),
  {
    /*
     * §4.5: apps/mobile/app is a self-contained submodule with its own lockfile, so
     * packages/shared/** and packages/core-ui/** are deliberately excluded. Add them and bump the
     * algorithm revision if mobile ever imports from the parent repo.
     *
     * The one parent-repo exception is apps/mobile/tailcat-bridge: the submodule's
     * GhostexSshConnection.kt compiles against dev.ghostex.tailcatbridge, which is
     * gomobile output that is never committed. The Android job regenerates it from
     * that Go source on every build, so the Go source is a real input to the APK.
     *
     * The Rust chat core is the second exception: the Android job builds it as WebAssembly into
     * the page the app bundles (tooling/release-mobile/chat-core-page.sh, from packages/gx-chat-wasm
     * and the gx-chat-json, gx-chat-core, gx-protocol and gx-visual crates it compiles in) and
     * never commits it. The same job exports the release's over-the-air update
     * (tooling/release-mobile/ota-export.sh), and the release publishes that update only when this
     * product builds, so without these pathspecs a release that changed only the chat core would
     * reuse the previous APK and phones would never get the change. The core is not native code
     * (scripts/ota-runtime-version.js leaves it out), so phones take it over the air.
     *
     * CDXC:Release 2026-10-28: packages/gx-chat-mobile (the native UniFFI core) left with the
     * user's choice to remove it (apps/mobile/app/src/chat/rust/wasm-core/bridge.ts).
     *
     * FINGERPRINT_ALGORITHM_REVISION is deliberately NOT bumped for this purely
     * additive change. The header rule exists so a stale record can never be
     * compared against a different input set; here every product already hashes
     * this file through SHARED_BASE_PATHSPECS, so no product digest can survive
     * the edit, and the component nodes' input sets are untouched — bumping would
     * only discard the code-server/cef identity recovery in plan-cli.mjs and force
     * an unrelated component rebuild.
     */
    artifacts: () => ['ghostex-android.apk'],
    composedFrom: [],
    id: 'android',
    kind: 'product',
    pathspecs: [
      { pathspec: 'apps/mobile/app' },
      { pathspec: 'apps/mobile/tailcat-bridge/**' },
      { pathspec: ':(exclude)apps/mobile/tailcat-bridge/build' },
      { pathspec: 'packages/gx-chat-json/**' },
      { pathspec: 'packages/gx-chat-core/**' },
      { pathspec: 'packages/gx-protocol/**' },
      { pathspec: 'packages/gx-visual/**' },
      { pathspec: 'packages/gx-chat-wasm/**' },
      { pathspec: ':(exclude)packages/gx-chat-wasm/target' },
      { pathspec: 'tooling/release-mobile/chat-core-page.sh' },
      { pathspec: 'tooling/xtask/src/mobile_chat_wasm.rs' },
      { pathspec: 'tooling/release-mobile/android.sh' },
      { pathspec: 'tooling/release-mobile/ota-export.sh' },
      { pathspec: 'tooling/release-gpui/android.sh' },
      { pathspec: '.github/workflows/release-gpui-android.yml' },
    ],
    platform: { arch: 'arm64', os: 'android', runnerLabel: 'ubuntu-24.04' },
    scopeFlag: 'android',
    sideFiles: [],
    signing: { mode: 'android-keystore' },
    values: {
      androidBuildTools: TOOLCHAIN.androidBuildTools,
      androidNdk: TOOLCHAIN.androidNdk,
      androidPlatform: TOOLCHAIN.androidPlatform,
      java: TOOLCHAIN.java,
      keystoreAlias: 'ANDROID_RELEASE_KEY_ALIAS',
      node: TOOLCHAIN.node,
      signingMode: 'android-keystore',
      tailcatBridgeGo: TOOLCHAIN.tailcatBridgeGo,
      tailcatBridgeGomobile: TOOLCHAIN.tailcatBridgeGomobile,
    },
    versionStamped: false,
  },
  linuxDesktopProduct('deb'),
  linuxDesktopProduct('rpm'),
  linuxDesktopProduct('tar'),
  linuxDesktopProduct('appimage'),
  linuxDesktopProduct('deb', 'arm64'),
  linuxDesktopProduct('rpm', 'arm64'),
  linuxDesktopProduct('tar', 'arm64'),
  linuxDesktopProduct('appimage', 'arm64'),
  windowsProduct('x64'),
  windowsProduct('arm64'),
  wslProduct('x64'),
  wslProduct('arm64'),
  {
    artifacts: (version) => [`ghostex-${version}-arm64.dmg`],
    composedFrom: ['gxserver-linux-x64', 'gxserver-linux-arm64', 'code-server', 'cef'],
    id: 'macos-arm64',
    kind: 'product',
    pathspecs: [
      ...DESKTOP_APP_PATHSPECS,
      /*
       * The bundled sounds come from media/ (Linux and Windows list only
       * media/sounds/**). macOS is the only builder that copies the CLI skills
       * straight into the app bundle (the Linux packages get them through the
       * gxserver package).
       */
      { pathspec: 'media/**' },
      { pathspec: 'skills/**' },
      /*
       * The Ctrl+G Monaco prompt editor ships as Contents/Resources/
       * GhostexEditor.app, built from the Swift package plus the Monaco web
       * payload. apps/editor/desktop/** is the Linux/Windows wry variant and is
       * deliberately excluded: it cannot change the macOS artifact.
       */
      { pathspec: 'apps/editor/macos/**' },
      { pathspec: 'apps/editor/web/**' },
      { pathspec: 'apps/editor/scripts/**' },
      { pathspec: 'tooling/release-gpui/macos.sh' },
      { pathspec: 'tooling/release-gpui/macos-notary.sh' },
      { pathspec: 'tooling/release-gpui/macos-finalize.sh' },
      { pathspec: 'tooling/release-gpui/macos-prerequisite.sh' },
      { pathspec: 'tooling/release-gpui/prepare-sparkle.sh' },
      { pathspec: 'tooling/release-gpui/verify-code-server-archive.mjs' },
      { pathspec: 'tooling/validate-macos-app-bundle.mjs' },
      { pathspec: '.github/workflows/release-gpui-macos.yml' },
    ],
    platform: { arch: 'arm64', os: 'macos', runnerLabel: 'macos-15' },
    scopeFlag: 'macos',
    /* appcast.xml is uploaded beside the manifest but is not in manifest.artifacts. */
    sideFiles: ['appcast.xml'],
    signing: { mode: 'developer-id+notarized' },
    values: {
      node: TOOLCHAIN.node,
      ripgrep: `${TOOLCHAIN.ripgrepVersion}/${TOOLCHAIN.ripgrepPackageVersion}`,
      ripgrepSha256: TOOLCHAIN.ripgrepSha256,
      signingMode: 'developer-id+notarized',
      sparkleFeedUrl: SPARKLE_FEED_URL,
      updateSparkle: (context) => String(Boolean(context.scope.updateSparkle)),
      zig: TOOLCHAIN.zig,
    },
    versionStamped: true,
  },
];

/* Published products, in the canonical expected_platforms order. */
export const PRODUCT_IDS = Object.freeze(PRODUCT_LIST.map((product) => product.id));

export const NODES = Object.freeze(
  Object.fromEntries([...PRODUCT_LIST.map((product) => [product.id, product]), ...Object.entries(COMPOSED_NODES)])
);

export const PRODUCTS = Object.freeze(Object.fromEntries(PRODUCT_LIST.map((product) => [product.id, product])));

export const COMPONENT_IDS = Object.freeze(
  Object.values(COMPOSED_NODES)
    .filter((node) => node.kind === 'component')
    .map((node) => node.id)
);

export const SCOPE_FLAGS = Object.freeze(PRODUCT_LIST.map((product) => product.scopeFlag));

export function nodeDefinition(id) {
  const node = NODES[id];
  if (!node) throw new Error(`Unknown release fingerprint node: ${id}`);
  return node;
}

export function productDefinition(id) {
  const product = PRODUCTS[id];
  if (!product) throw new Error(`Unknown release product: ${id}`);
  return product;
}

/* Every pathspec a node contributes, shared base first, in declaration order. */
export function nodePathspecs(id) {
  const node = nodeDefinition(id);
  const base = node.kind === 'product' ? SHARED_BASE_PATHSPECS : [];
  return [...base, ...node.pathspecs];
}

export function nodeValues(id, context) {
  const node = nodeDefinition(id);
  const resolved = node.kind === 'product' ? { ...SHARED_BASE_VALUES } : {};
  for (const [key, value] of Object.entries(node.values ?? {})) {
    resolved[key] = typeof value === 'function' ? String(value(context)) : String(value);
  }
  return resolved;
}

export function nodeSigningMode(id, context) {
  const mode = nodeDefinition(id).signing?.mode;
  if (mode === undefined) return null;
  return typeof mode === 'function' ? mode(context) : mode;
}

/* Depth-first topological order so composed children are fingerprinted first. */
export function nodeIdsInDependencyOrder(ids = Object.keys(NODES)) {
  const ordered = [];
  const visiting = new Set();
  const visited = new Set();
  const visit = (id, trail) => {
    if (visited.has(id)) return;
    if (visiting.has(id)) throw new Error(`Cyclic release fingerprint composition: ${[...trail, id].join(' -> ')}`);
    visiting.add(id);
    for (const child of nodeDefinition(id).composedFrom ?? []) visit(child, [...trail, id]);
    visiting.delete(id);
    visited.add(id);
    ordered.push(id);
  };
  for (const id of ids) visit(id, []);
  return ordered;
}

export function validateProductGraph() {
  nodeIdsInDependencyOrder();
  const seenScopeFlags = new Set();
  for (const id of PRODUCT_IDS) {
    const product = productDefinition(id);
    if (!product.scopeFlag) throw new Error(`${id} has no scope flag`);
    if (seenScopeFlags.has(product.scopeFlag)) throw new Error(`Duplicate scope flag ${product.scopeFlag}`);
    seenScopeFlags.add(product.scopeFlag);
    if (typeof product.artifacts !== 'function') throw new Error(`${id} has no artifact contract`);
    if (product.sideFiles?.length && !product.versionStamped) {
      throw new Error(`${id} carries side files and must be version-stamped so it is never reused across releases`);
    }
  }
  for (const [id, node] of Object.entries(NODES)) {
    for (const child of node.composedFrom ?? []) nodeDefinition(child);
    for (const entry of node.pathspecs) {
      if (typeof entry.pathspec !== 'string' || entry.pathspec.length === 0) {
        throw new Error(`${id} has an empty pathspec`);
      }
    }
  }
  return true;
}

/*
 * Which component platform assets a build of `productId` requires. Used by the
 * planner to decide whether a component tag is already complete (§8).
 *
 * macOS ships the CEF framework inside the signed app bundle, so its users never
 * download the darwin-arm64 CEF asset at runtime — but `macos.sh` still publishes
 * it to the `cef-<version>` tag as part of the sealed manifest, so it is a real
 * required platform of a macOS build. Leaving it out made a macOS-only release
 * report `cef … SKIP — no building product requires this component`, which is a
 * false statement in the provenance record even though nothing else keyed on it.
 */
export function componentPlatformRequirements(productId) {
  if (productId === 'macos-arm64') {
    return { cef: ['darwin-arm64'], 'code-server': ['darwin-arm64', 'linux-arm64', 'linux-x64'] };
  }
  for (const arch of ['x64', 'arm64']) {
    if (LINUX_DESKTOP_FORMATS.some((format) => productId === linuxDesktopProductId(format, arch))) {
      return { cef: [`linux-${arch}`], 'code-server': [`linux-${arch}`] };
    }
  }
  if (productId === 'windows-x64' || productId === 'windows-arm64') {
    const arch = productId.slice('windows-'.length);
    /* windows-<arch> is the WSL editor and windows-native-<arch> the native editor; the app installs either on demand. */
    return {
      cef: [`windows-${arch}`],
      'code-server': [`linux-${arch}`, `windows-${arch}`, `windows-native-${arch}`],
    };
  }
  return {};
}

export function defaultScope(overrides = {}) {
  return {
    android: true,
    gxserverLinuxArm64: true,
    gxserverLinuxX64: true,
    gxserverWslWindowsArm64: true,
    gxserverWslWindowsX64: true,
    linuxDeb: true,
    linuxDebArm64: true,
    linuxRpm: true,
    linuxRpmArm64: true,
    linuxTar: true,
    linuxTarArm64: true,
    linuxAppimage: true,
    linuxAppimageArm64: true,
    macos: true,
    prerelease: false,
    signWindows: false,
    updateSparkle: true,
    windowsArm64: true,
    windowsX64: true,
    ...overrides,
  };
}

export function isProductRequested(productId, scope) {
  return Boolean(scope?.[productDefinition(productId).scopeFlag]);
}
