/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
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

export const TRUSTED_REPO = "maddada/Ghostex";

/*
 * Pinned toolchains. These mirror the workflow pins; `product-inputs.test.mjs`
 * asserts they still match the workflow files, so drift fails a test instead of
 * silently reusing an artifact built with a different compiler.
 */
export const TOOLCHAIN = Object.freeze({
  androidBuildTools: "36.0.0",
  androidNdk: "29.0.14206865",
  androidPlatform: "android-36",
  bun: "1.3.10",
  dotnet: "8.0.x",
  goVersionFile: "build/pinned-beads-source/go.mod",
  java: "17.0.19+10",
  node: "24.13.1",
  ripgrepPackageVersion: "1.17.1",
  ripgrepSha256: "2fa16464fd8638588a67c7fc172d3c4b57fbdc65dff366e10b0b0e90734628a6",
  ripgrepVersion: "v15.0.1",
  vpk: "1.2.0",
  zig015: "0.15.2",
  zig016: "0.16.0",
});

/* Mirrors scripts/beads-release.mjs; asserted equal in product-inputs.test.mjs. */
export const BEADS_PINS = Object.freeze({
  packageId: "1.1.0-672d942083a1-schema54",
  schemaVersion: "54",
  sourceRevision: "672d942083a1fd0c8603fa1e77620c58ba9d47c8",
  version: "1.1.0",
});

/* Mirrors CODE_SERVER_COMPONENT_IDENTITY_REVISION; asserted equal in the tests. */
export const CODE_SERVER_IDENTITY_REVISION = "p2";

export const SPARKLE_FEED_URL = "https://raw.githubusercontent.com/maddada/Ghostex/main/appcast.xml";

/*
 * Top-level tracked paths that deliberately contribute to no product.
 * `product-inputs.test.mjs` fails when a new top-level entry appears that is
 * neither claimed by a product pathspec nor listed here, so a new directory
 * cannot silently escape the fingerprint.
 */
export const IGNORED_FOR_RELEASE = Object.freeze([
  { path: ".beads", why: "Project board database; never compiled into an artifact." },
  { path: ".editorconfig", why: "Editor configuration only." },
  { path: ".gitattributes", why: "Git checkout behavior only; release jobs use fresh clones." },
  { path: ".gitignore", why: "Ignore rules only; tracked inputs are enumerated explicitly." },
  { path: "AGENTS.md", why: "Agent instructions; metadata only." },
  { path: "CHANGELOG.md", why: "Release notes source; metadata only (§4.11 rule 8)." },
  { path: "CLAUDE.md", why: "Agent instructions; metadata only." },
  { path: "LICENSE", why: "Metadata only." },
  { path: "README.md", why: "Metadata only." },
  { path: "appcast.xml", why: "Sparkle feed output written by the publisher, not a build input." },
  { path: "claude-code-codex-keybindings.json", why: "Developer keybindings; not packaged." },
  { path: "favicon.png", why: "Web asset for local tooling; not packaged by any release job." },
  { path: "apps/history-cli", why: "Local history CLI; not packaged by any release job." },
  {
    path: ".dependencies/ghostty-patches",
    why: "Source-sync overlay only; release jobs compile the already-patched tracked ghostty tree.",
  },
  { path: "apps/web", why: "Web app; released separately, never part of a GPUI release artifact." },
  { path: "apps/mobile/views/chat", why: "Mobile chat bundle source; consumed by the mobile submodule build, not by release jobs." },
  {
    path: "apps/mobile/views/find",
    why: "Mobile Find Prompts bundle source; consumed by the mobile submodule build, not by release jobs.",
  },
  {
    path: ".dependencies/zehn",
    why: "Retired Zig prompt-history source; kept as reference only. The shipped implementation is the packages/find crate compiled into gxserver.",
  },
  {
    path: "vitest.config.ts",
    why: "Test runner configuration; release:test uses scripts/release-gpui/vitest.release.config.ts.",
  },
]);

/*
 * Added to every product. `package.json` is hashed through the `package-json`
 * projection so a lone version bump cannot invalidate products that are not
 * version-stamped (§4.11 rule 8).
 */
export const SHARED_BASE_PATHSPECS = Object.freeze([
  { pathspec: "package.json", projection: "package-json" },
  { pathspec: "bun.lock" },
  { pathspec: ".gitmodules" },
  { pathspec: "scripts/release-gpui/common.sh" },
  { pathspec: "scripts/release-gpui/product-inputs.mjs" },
  { pathspec: "scripts/release-gpui/fingerprint.mjs" },
  { pathspec: ".github/workflows/release-gpui.yml" },
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
  { pathspec: "apps/desktop/**" },
  { pathspec: ":(exclude)apps/desktop/build" },
  { pathspec: ":(exclude)apps/desktop/target" },
  { pathspec: "packages/paths/**" },
  { pathspec: "packages/find/**" },
  { pathspec: ":(exclude)packages/find/target" },
  { pathspec: "server/**" },
  { pathspec: ":(exclude)server/target" },
  { pathspec: "packages/shared/**" },
  { pathspec: "packages/core-ui/**" },
  { pathspec: "packages/components/**" },
  { pathspec: "components.json" },
  { pathspec: ".dependencies/ghostty/**" },
  { pathspec: ":(exclude).dependencies/ghostty/.zig-cache" },
  { pathspec: ":(exclude).dependencies/ghostty/zig-out" },
  { pathspec: ".dependencies/zed" },
  { pathspec: ".dependencies/cef-rs" },
  { pathspec: ".dependencies/gpui-component" },
  { pathspec: "scripts/release-gpui/prepare-references.sh" },
  { pathspec: "scripts/release-gpui/create-deterministic-tar.sh" },
  { pathspec: "scripts/release-gpui/publish-component.mjs" },
  { pathspec: "scripts/release-gpui/on-demand-manifest.mjs" },
  { pathspec: "scripts/release-gpui/patches" },
  { pathspec: "tsconfig.json" },
]);

/* Inputs shared by both gxserver Linux architectures. */
const GXSERVER_PATHSPECS = Object.freeze([
  { pathspec: "server/**" },
  { pathspec: ":(exclude)server/target" },
  { pathspec: "packages/paths/**" },
  { pathspec: "packages/find/**" },
  { pathspec: ":(exclude)packages/find/target" },
  { pathspec: ".dependencies/tui2/**" },
  { pathspec: ":(exclude).dependencies/tui2/target" },
  { pathspec: ".dependencies/zmx" },
  { pathspec: "scripts/build-remote-gxserver-linux-release.sh" },
  { pathspec: "scripts/beads-release.mjs" },
  { pathspec: "scripts/build-pinned-beads-release.mjs" },
  { pathspec: "scripts/smoke-test-packaged-beads.mjs" },
  { pathspec: "scripts/release-ghostex.mjs" },
  { pathspec: "scripts/release-gpui/prepare-references.sh" },
  /*
   * release-gpui-runtime.yml was split into the gxserver package workflow and
   * the immutable code-server component workflow; only the package half builds
   * these bytes.
   */
  { pathspec: ".github/workflows/release-gpui-gxserver.yml" },
]);

/* §4.4: the pinned Beads payload, shared by gxserver Linux and the macOS bd tarball. */
const BEADS_PATHSPECS = Object.freeze([
  { pathspec: "scripts/beads-release.mjs" },
  { pathspec: "scripts/build-pinned-beads-release.mjs" },
  { pathspec: "scripts/smoke-test-packaged-beads.mjs" },
]);

function windowsArtifacts(version, arch) {
  const channel = `win-${arch}-stable`;
  return [
    `ghostex-${version}-windows-${arch}.exe`,
    `ghostex-${version}-windows-${arch}-portable.zip`,
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
    kind: "product",
    pathspecs: GXSERVER_PATHSPECS,
    platform: { arch, os: "linux", runnerLabel: arch === "arm64" ? "ubuntu-24.04-arm" : "ubuntu-24.04" },
    scopeFlag: arch === "arm64" ? "gxserverLinuxArm64" : "gxserverLinuxX64",
    sideFiles: [],
    signing: { mode: "unsigned" },
    values: {
      arch,
      beadsPackageId: BEADS_PINS.packageId,
      beadsPinnedRef: BEADS_PINS.sourceRevision,
      goVersionFile: TOOLCHAIN.goVersionFile,
      zig015: TOOLCHAIN.zig015,
      zig016: TOOLCHAIN.zig016,
    },
    versionStamped: false,
  };
}

function linuxDesktopProduct(format) {
  return {
    artifacts: (version) =>
      format === "deb" ? [`ghostex_${version}_amd64.deb`] : [`ghostex-${version}-1.x86_64.rpm`],
    composedFrom: ["gxserver-linux-x64", "cef"],
    id: `linux-${format}-x64`,
    kind: "product",
    pathspecs: [
      ...DESKTOP_APP_PATHSPECS,
      { pathspec: "scripts/release-gpui/linux-stage.sh" },
      { pathspec: `scripts/release-gpui/linux-${format}.sh` },
      { pathspec: ".github/workflows/release-gpui-linux.yml" },
    ],
    platform: { arch: "x64", os: "linux", runnerLabel: "ubuntu-24.04" },
    scopeFlag: format === "deb" ? "linuxDeb" : "linuxRpm",
    sideFiles: [],
    signing: { mode: "unsigned" },
    values: {
      packageFormat: format,
      zig015: TOOLCHAIN.zig015,
      zig016: TOOLCHAIN.zig016,
    },
    versionStamped: true,
  };
}

function windowsProduct(arch) {
  return {
    artifacts: (version) => windowsArtifacts(version, arch),
    composedFrom: [`gxserver-linux-${arch}`, "code-server", "cef"],
    id: `windows-${arch}`,
    kind: "product",
    optionalArtifacts: (version) => windowsOptionalArtifacts(version, arch),
    pathspecs: [
      ...DESKTOP_APP_PATHSPECS,
      { pathspec: "scripts/release-gpui/windows.ps1" },
      { pathspec: "scripts/release-gpui/prepare-zig.ps1" },
      { pathspec: "scripts/release-gpui/windows-update-feed.mjs" },
      { pathspec: "scripts/release-gpui/verify-code-server-archive.mjs" },
      { pathspec: ".github/workflows/release-gpui-windows.yml" },
      { pathspec: ".github/workflows/release-gpui-validate.yml" },
    ],
    platform: { arch, os: "windows", runnerLabel: arch === "arm64" ? "windows-11-vs2026-arm" : "windows-2025" },
    scopeFlag: arch === "arm64" ? "windowsArm64" : "windowsX64",
    sideFiles: [],
    signing: { mode: (context) => (context.scope.signWindows ? "authenticode" : "unsigned") },
    values: {
      arch,
      dotnet: TOOLCHAIN.dotnet,
      /* sign_windows changes the produced bytes and the release notes. */
      signingMode: (context) => (context.scope.signWindows ? "authenticode" : "unsigned"),
      vpk: TOOLCHAIN.vpk,
      zigPin: TOOLCHAIN.zig016,
    },
    versionStamped: true,
  };
}

function wslProduct(arch) {
  return {
    artifacts: () => [`gxserver-wsl-windows-${arch}.zip`],
    composedFrom: [`gxserver-linux-${arch}`],
    id: `gxserver-wsl-windows-${arch}`,
    kind: "product",
    pathspecs: [
      { pathspec: "scripts/release-gpui/wsl-runtime.sh" },
      { pathspec: "scripts/release-gpui/install-gxserver-wsl.ps1" },
      { pathspec: ".github/workflows/release-gpui-wsl-runtime.yml" },
    ],
    platform: { arch, os: "windows", runnerLabel: "ubuntu-24.04" },
    scopeFlag: arch === "arm64" ? "gxserverWslWindowsArm64" : "gxserverWslWindowsX64",
    sideFiles: [],
    signing: { mode: "unsigned" },
    values: { arch },
    versionStamped: true,
  };
}

/*
 * Fingerprint nodes that are not published products. They exist so their inputs
 * compose into the products that embed or seal them (§3.4 part 3).
 */
const COMPOSED_NODES = {
  beads: {
    composedFrom: [],
    id: "beads",
    kind: "payload",
    pathspecs: BEADS_PATHSPECS,
    values: {
      beadsPackageId: BEADS_PINS.packageId,
      beadsPinnedRef: BEADS_PINS.sourceRevision,
      beadsSchemaVersion: BEADS_PINS.schemaVersion,
      beadsVersion: BEADS_PINS.version,
      /* bd-darwin-arm64.tar.gz is Developer-ID signed inside prepare-macos-runtime.sh. */
      codesignIdentity: "developer-id",
      goVersionFile: TOOLCHAIN.goVersionFile,
    },
    versionStamped: false,
  },
  cef: {
    /*
     * §4.3: the component identity is CEF_VERSION, which is a pure function of
     * the pinned cef-rs checkout. Fingerprinting the gitlink keeps planning
     * offline and submodule-checkout free while tracking the identity exactly.
     */
    composedFrom: [],
    id: "cef",
    kind: "component",
    pathspecs: [{ pathspec: ".dependencies/cef-rs" }],
    values: {},
    versionStamped: false,
  },
  "code-server": {
    /*
     * §4.2: the component identity is
     * `<12-hex code-server HEAD>-p2-<payload fingerprint>`. Both halves are
     * determined by the code-server gitlink and the identity revision, so the
     * gitlink plus the identity-revision inputs is an exact, offline stand-in.
     * The three identity-revision inputs below change the produced archive
     * without changing the upstream payload, so they must invalidate the build.
     */
    composedFrom: [],
    id: "code-server",
    identityRevisionPathspecs: [
      { pathspec: "scripts/release-gpui/patches/code-server-ripgrep-target-validation.patch" },
      { pathspec: ".github/workflows/release-gpui-code-server.yml" },
      /* Lives inside the code-server gitlink, so it never appears in this tree. */
      { pathspec: ".dependencies/code-server/.node-version", allowMissing: true },
    ],
    kind: "component",
    pathspecs: [
      { pathspec: ".dependencies/code-server" },
      { pathspec: "scripts/release-gpui/code-server-component-identity.mjs" },
      { pathspec: "scripts/release-gpui/patches/code-server-ripgrep-target-validation.patch" },
      { pathspec: "scripts/release-gpui/verify-code-server-archive.mjs" },
      { pathspec: ".github/workflows/release-gpui-code-server.yml" },
    ],
    values: { identityRevision: CODE_SERVER_IDENTITY_REVISION, node: TOOLCHAIN.node },
    versionStamped: false,
  },
};

const PRODUCT_LIST = [
  gxserverProduct("x64"),
  gxserverProduct("arm64"),
  {
    /*
     * §4.5: apps/mobile/app is a self-contained submodule with its own lockfile, so
     * packages/shared/** and packages/core-ui/** are deliberately excluded. Add them and bump the
     * algorithm revision if mobile ever imports from the parent repo.
     */
    artifacts: () => ["ghostex-android.apk"],
    composedFrom: [],
    id: "android",
    kind: "product",
    pathspecs: [
      { pathspec: "apps/mobile/app" },
      { pathspec: "scripts/release-mobile/android.sh" },
      { pathspec: "scripts/release-gpui/android.sh" },
      { pathspec: ".github/workflows/release-gpui-android.yml" },
    ],
    platform: { arch: "arm64", os: "android", runnerLabel: "ubuntu-24.04" },
    scopeFlag: "android",
    sideFiles: [],
    signing: { mode: "android-keystore" },
    values: {
      androidBuildTools: TOOLCHAIN.androidBuildTools,
      androidNdk: TOOLCHAIN.androidNdk,
      androidPlatform: TOOLCHAIN.androidPlatform,
      java: TOOLCHAIN.java,
      keystoreAlias: "ANDROID_RELEASE_KEY_ALIAS",
      node: TOOLCHAIN.node,
      signingMode: "android-keystore",
    },
    versionStamped: false,
  },
  linuxDesktopProduct("deb"),
  linuxDesktopProduct("rpm"),
  windowsProduct("x64"),
  windowsProduct("arm64"),
  wslProduct("x64"),
  wslProduct("arm64"),
  {
    artifacts: (version) => [`ghostex-${version}-arm64.dmg`, "bd-darwin-arm64.tar.gz"],
    composedFrom: ["gxserver-linux-x64", "gxserver-linux-arm64", "code-server", "cef", "beads"],
    id: "macos-arm64",
    kind: "product",
    pathspecs: [
      ...DESKTOP_APP_PATHSPECS,
      /* macOS is the only builder that stages bundled sounds and CLI skills. */
      { pathspec: "media/**" },
      { pathspec: "skills/**" },
      /*
       * The Ctrl+G Monaco prompt editor ships as Contents/Resources/
       * GhostexEditor.app, built from the Swift package plus the Monaco web
       * payload. apps/editor/desktop/** is the Linux/Windows wry variant and is
       * deliberately excluded: it cannot change the macOS artifact.
       */
      { pathspec: "apps/editor/macos/**" },
      { pathspec: "apps/editor/web/**" },
      { pathspec: "apps/editor/scripts/**" },
      { pathspec: "scripts/release-gpui/macos.sh" },
      { pathspec: "scripts/release-gpui/macos-notary.sh" },
      { pathspec: "scripts/release-gpui/macos-finalize.sh" },
      { pathspec: "scripts/release-gpui/macos-prerequisite.sh" },
      { pathspec: "scripts/release-gpui/prepare-sparkle.sh" },
      { pathspec: "scripts/release-gpui/verify-code-server-archive.mjs" },
      { pathspec: "scripts/validate-macos-app-bundle.mjs" },
      { pathspec: ".github/workflows/release-gpui-macos.yml" },
    ],
    platform: { arch: "arm64", os: "macos", runnerLabel: "macos-15" },
    scopeFlag: "macos",
    /* appcast.xml is uploaded beside the manifest but is not in manifest.artifacts. */
    sideFiles: ["appcast.xml"],
    signing: { mode: "developer-id+notarized" },
    values: {
      node: TOOLCHAIN.node,
      ripgrep: `${TOOLCHAIN.ripgrepVersion}/${TOOLCHAIN.ripgrepPackageVersion}`,
      ripgrepSha256: TOOLCHAIN.ripgrepSha256,
      signingMode: "developer-id+notarized",
      sparkleFeedUrl: SPARKLE_FEED_URL,
      updateSparkle: (context) => String(Boolean(context.scope.updateSparkle)),
      zig015: TOOLCHAIN.zig015,
      zig016: TOOLCHAIN.zig016,
    },
    versionStamped: true,
  },
];

/* Published products, in the canonical expected_platforms order. */
export const PRODUCT_IDS = Object.freeze(PRODUCT_LIST.map((product) => product.id));

export const NODES = Object.freeze(
  Object.fromEntries([...PRODUCT_LIST.map((product) => [product.id, product]), ...Object.entries(COMPOSED_NODES)]),
);

export const PRODUCTS = Object.freeze(Object.fromEntries(PRODUCT_LIST.map((product) => [product.id, product])));

export const COMPONENT_IDS = Object.freeze(
  Object.values(COMPOSED_NODES)
    .filter((node) => node.kind === "component")
    .map((node) => node.id),
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
  const base = node.kind === "product" ? SHARED_BASE_PATHSPECS : [];
  return [...base, ...node.pathspecs];
}

export function nodeValues(id, context) {
  const node = nodeDefinition(id);
  const resolved = node.kind === "product" ? { ...SHARED_BASE_VALUES } : {};
  for (const [key, value] of Object.entries(node.values ?? {})) {
    resolved[key] = typeof value === "function" ? String(value(context)) : String(value);
  }
  return resolved;
}

export function nodeSigningMode(id, context) {
  const mode = nodeDefinition(id).signing?.mode;
  if (mode === undefined) return null;
  return typeof mode === "function" ? mode(context) : mode;
}

/* Depth-first topological order so composed children are fingerprinted first. */
export function nodeIdsInDependencyOrder(ids = Object.keys(NODES)) {
  const ordered = [];
  const visiting = new Set();
  const visited = new Set();
  const visit = (id, trail) => {
    if (visited.has(id)) return;
    if (visiting.has(id)) throw new Error(`Cyclic release fingerprint composition: ${[...trail, id].join(" -> ")}`);
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
    if (typeof product.artifacts !== "function") throw new Error(`${id} has no artifact contract`);
    if (product.sideFiles?.length && !product.versionStamped) {
      throw new Error(`${id} carries side files and must be version-stamped so it is never reused across releases`);
    }
  }
  for (const [id, node] of Object.entries(NODES)) {
    for (const child of node.composedFrom ?? []) nodeDefinition(child);
    for (const entry of node.pathspecs) {
      if (typeof entry.pathspec !== "string" || entry.pathspec.length === 0) {
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
  if (productId === "macos-arm64") {
    return { cef: ["darwin-arm64"], "code-server": ["darwin-arm64", "linux-arm64", "linux-x64"] };
  }
  if (productId === "linux-deb-x64" || productId === "linux-rpm-x64") {
    return { cef: ["linux-x64"] };
  }
  if (productId === "windows-x64" || productId === "windows-arm64") {
    const arch = productId.slice("windows-".length);
    return { cef: [`windows-${arch}`], "code-server": [`linux-${arch}`, `windows-${arch}`] };
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
    linuxRpm: true,
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
