import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";
import {
  BEADS_PINS,
  CODE_SERVER_IDENTITY_REVISION,
  COMPONENT_IDS,
  IGNORED_FOR_RELEASE,
  NODES,
  PRODUCT_IDS,
  TOOLCHAIN,
  componentPlatformRequirements,
  nodeIdsInDependencyOrder,
  nodePathspecs,
  nodeValues,
  productDefinition,
  validateProductGraph,
} from "./product-inputs.mjs";
import { createGitTreeReader, normalizePathspec } from "./fingerprint.mjs";
import { defaultScope } from "./product-inputs.mjs";
import { BEADS_PACKAGE_ID, BEADS_SCHEMA_VERSION, BEADS_SOURCE_REVISION, BEADS_VERSION } from "../beads-release.mjs";
import { CODE_SERVER_COMPONENT_IDENTITY_REVISION } from "./code-server-component-identity.mjs";

const reader = createGitTreeReader({ repoRoot: process.cwd() });
const entries = reader.listTree("HEAD");
const trackedPaths = new Set(entries.map((entry) => entry.path));
const topLevelPaths = execFileSync("git", ["ls-tree", "HEAD", "--name-only"], { encoding: "utf8" })
  .split("\n")
  .map((line) => line.trim())
  .filter(Boolean);

function positivePathspecs() {
  const result = [];
  for (const nodeId of Object.keys(NODES)) {
    const declarations = [...nodePathspecs(nodeId), ...(NODES[nodeId].identityRevisionPathspecs ?? [])];
    for (const declaration of declarations) {
      const { negative, prefix } = normalizePathspec(declaration.pathspec);
      if (!negative) result.push({ declaration, nodeId, prefix });
    }
  }
  return result;
}

/*
 * Resolution is checked against the working tree rather than `git ls-tree HEAD`
 * so a pathspec for a file that is added in this same change still resolves.
 * Coverage of tracked top-level paths still uses the committed tree.
 */
function resolves(prefix) {
  if (existsSync(prefix)) return true;
  for (const path of trackedPaths) {
    if (path === prefix || path.startsWith(`${prefix}/`)) return true;
  }
  return false;
}

describe("release product input map", () => {
  test("declares an acyclic composition graph with one scope flag per product", () => {
    expect(validateProductGraph()).toBe(true);
    const order = nodeIdsInDependencyOrder();
    for (const nodeId of order) {
      for (const child of NODES[nodeId].composedFrom ?? []) {
        expect(order.indexOf(child)).toBeLessThan(order.indexOf(nodeId));
      }
    }
  });

  test("covers exactly the published release platforms", () => {
    expect(PRODUCT_IDS).toEqual([
      "gxserver-linux-x64",
      "gxserver-linux-arm64",
      "android",
      "linux-deb-x64",
      "linux-rpm-x64",
      "windows-x64",
      "windows-arm64",
      "gxserver-wsl-windows-x64",
      "gxserver-wsl-windows-arm64",
      "macos-arm64",
    ]);
    expect([...COMPONENT_IDS].sort()).toEqual(["cef", "code-server"]);
  });

  test("matches the publisher artifact-name contract", () => {
    const version = "7.8.0";
    expect(productDefinition("macos-arm64").artifacts(version)).toEqual([
      "ghostex-7.8.0-arm64.dmg",
      "bd-darwin-arm64.tar.gz",
    ]);
    expect(productDefinition("linux-deb-x64").artifacts(version)).toEqual(["ghostex_7.8.0_amd64.deb"]);
    expect(productDefinition("linux-rpm-x64").artifacts(version)).toEqual(["ghostex-7.8.0-1.x86_64.rpm"]);
    expect(productDefinition("android").artifacts(version)).toEqual(["ghostex-android.apk"]);
    expect(productDefinition("gxserver-linux-arm64").artifacts(version)).toEqual(["gxserver-linux-arm64.tar.gz"]);
    expect(productDefinition("gxserver-wsl-windows-x64").artifacts(version)).toEqual([
      "gxserver-wsl-windows-x64.zip",
    ]);
    expect(productDefinition("windows-x64").artifacts(version)).toEqual([
      "ghostex-7.8.0-windows-x64.exe",
      "ghostex-7.8.0-windows-x64-portable.zip",
      "releases.win-x64-stable.json",
      "Ghostex-7.8.0-win-x64-stable-full.nupkg",
    ]);
  });

  test("every declared pathspec resolves in the current checkout", () => {
    const unresolved = positivePathspecs()
      .filter(({ declaration, prefix }) => !declaration.allowMissing && !resolves(prefix))
      .map(({ nodeId, declaration }) => `${nodeId}: ${declaration.pathspec}`);
    expect(unresolved).toEqual([]);
  });

  test("documents every pathspec that is allowed to be missing", () => {
    const allowed = positivePathspecs()
      .filter(({ declaration }) => declaration.allowMissing)
      .map(({ declaration }) => declaration.pathspec)
      .sort();
    /*
     * The only legitimately-absent pathspec lives inside the code-server
     * gitlink, so it can never appear in this repository's tree. The workflow
     * files the rewiring workstream created now exist and are required.
     */
    expect([...new Set(allowed)]).toEqual([".dependencies/code-server/.node-version"]);
  });

  test("uses only supported pathspec syntax", () => {
    for (const nodeId of Object.keys(NODES)) {
      for (const declaration of nodePathspecs(nodeId)) {
        expect(() => normalizePathspec(declaration.pathspec)).not.toThrow();
      }
    }
  });

  test("claims or explicitly ignores every tracked top-level path", () => {
    const prefixes = positivePathspecs().map(({ prefix }) => prefix);
    const ignored = new Set(IGNORED_FOR_RELEASE.map((entry) => entry.path));
    const unclassified = topLevelPaths.filter((topLevel) => {
      if (ignored.has(topLevel)) return false;
      return !prefixes.some((prefix) => prefix === topLevel || prefix.startsWith(`${topLevel}/`));
    });
    expect(unclassified).toEqual([]);
    for (const entry of IGNORED_FOR_RELEASE) {
      expect(entry.why.length).toBeGreaterThan(10);
    }
  });

  test("keeps every ignored path genuinely tracked and unclaimed", () => {
    const prefixes = positivePathspecs().map(({ prefix }) => prefix);
    for (const entry of IGNORED_FOR_RELEASE) {
      const claimed = prefixes.some((prefix) => prefix === entry.path || prefix.startsWith(`${entry.path}/`));
      expect({ claimed, path: entry.path }).toEqual({ claimed: false, path: entry.path });
    }
  });

  test("encodes the cross-cutting invalidation rules", () => {
    const specs = (nodeId) => nodePathspecs(nodeId).map((declaration) => declaration.pathspec);
    for (const desktop of ["macos-arm64", "linux-deb-x64", "linux-rpm-x64", "windows-x64", "windows-arm64"]) {
      /* Rule 1: protocol coupling and the embedded gxserver payload. */
      expect(specs(desktop)).toContain("server/**");
      expect(specs(desktop)).toContain("packages/paths/**");
      /* Rule 2: CEF surfaces are built from the shared React trees. */
      expect(specs(desktop)).toContain("packages/shared/**");
      expect(specs(desktop)).toContain("packages/core-ui/**");
      expect(specs(desktop)).toContain("packages/components/**");
      expect(specs(desktop)).toContain("apps/desktop/views/**");
      /* Rule 4: patched dependency gitlinks. */
      expect(specs(desktop)).toContain(".dependencies/cef-rs");
      /* Rule 10: the graph and manifest definitions invalidate everything. */
      expect(specs(desktop)).toContain(".github/workflows/release-gpui.yml");
      expect(specs(desktop)).toContain("scripts/release-gpui/common.sh");
    }
    /* Rule 3: packages/shared/** is deliberately absent from the remote Linux package. */
    expect(specs("gxserver-linux-x64")).not.toContain("packages/shared/**");
    expect(specs("gxserver-linux-arm64")).not.toContain("packages/shared/**");
    /* Rule 5: a gxserver rebuild forces every desktop rebuild through composition. */
    expect(productDefinition("macos-arm64").composedFrom).toEqual([
      "gxserver-linux-x64",
      "gxserver-linux-arm64",
      "code-server",
      "cef",
      "beads",
    ]);
    expect(productDefinition("linux-deb-x64").composedFrom).toEqual(["gxserver-linux-x64", "cef"]);
    /* Rule 6: code-server touches macOS and Windows, never the Linux packages. */
    expect(productDefinition("windows-arm64").composedFrom).toContain("code-server");
    expect(productDefinition("linux-rpm-x64").composedFrom).not.toContain("code-server");
    /* Rule 9: a workflow file invalidates only the product it builds. */
    expect(specs("android")).toContain(".github/workflows/release-gpui-android.yml");
    expect(specs("android")).not.toContain(".github/workflows/release-gpui-macos.yml");
  });

  test("marks exactly the version-stamped products", () => {
    const stamped = PRODUCT_IDS.filter((id) => productDefinition(id).versionStamped).sort();
    expect(stamped).toEqual([
      "gxserver-wsl-windows-arm64",
      "gxserver-wsl-windows-x64",
      "linux-deb-x64",
      "linux-rpm-x64",
      "macos-arm64",
      "windows-arm64",
      "windows-x64",
    ]);
  });

  test("only version-stamped products may carry side files", () => {
    for (const id of PRODUCT_IDS) {
      const definition = productDefinition(id);
      if (definition.sideFiles?.length) expect(definition.versionStamped).toBe(true);
    }
    expect(productDefinition("macos-arm64").sideFiles).toEqual(["appcast.xml"]);
  });

  test("maps component platform requirements to their real consumers", () => {
    /* macos.sh publishes both components; the CEF framework is bundled in the
     * app, but the darwin-arm64 asset is still pushed to the component tag. */
    expect(componentPlatformRequirements("macos-arm64")).toEqual({
      cef: ["darwin-arm64"],
      "code-server": ["darwin-arm64", "linux-arm64", "linux-x64"],
    });
    expect(componentPlatformRequirements("windows-arm64")).toEqual({
      cef: ["windows-arm64"],
      "code-server": ["linux-arm64", "windows-arm64"],
    });
    expect(componentPlatformRequirements("linux-deb-x64")).toEqual({ cef: ["linux-x64"] });
    expect(componentPlatformRequirements("android")).toEqual({});
  });

  test("resolves scope-dependent values", () => {
    const signed = nodeValues("windows-x64", { scope: defaultScope({ signWindows: true }), version: "7.8.0" });
    const unsigned = nodeValues("windows-x64", { scope: defaultScope({ signWindows: false }), version: "7.8.0" });
    expect(signed.signingMode).toBe("authenticode");
    expect(unsigned.signingMode).toBe("unsigned");
    expect(nodeValues("macos-arm64", { scope: defaultScope({ updateSparkle: false }), version: "7.8.0" }).updateSparkle).toBe(
      "false",
    );
    expect(nodeValues("android", { scope: defaultScope(), version: "7.8.0" }).bun).toBe(TOOLCHAIN.bun);
  });
});

describe("pinned toolchain values track the workflows", () => {
  const workflow = (name) => readFileSync(`.github/workflows/${name}`, "utf8");

  test("bun, node, zig, dotnet, vpk and Android SDK pins match the workflow files", () => {
    const macos = workflow("release-gpui-macos.yml");
    const windows = workflow("release-gpui-windows.yml");
    const android = workflow("release-gpui-android.yml");
    expect(macos).toContain(`bun-version: ${TOOLCHAIN.bun}`);
    expect(macos).toContain(`zig@${TOOLCHAIN.zig015} zig@${TOOLCHAIN.zig016}`);
    expect(macos).toContain('"$ZIG_015" "$ZIG_016" "$ZIG_016" >> "$GITHUB_ENV"');
    expect(workflow("release-gpui-linux.yml")).toContain(`zig@${TOOLCHAIN.zig015} zig@${TOOLCHAIN.zig016}`);
    expect(workflow("release-gpui-linux.yml")).toContain('"$ZIG_015" "$ZIG_016" >> "$GITHUB_ENV"');
    expect(readFileSync("scripts/release-gpui/macos.sh", "utf8")).toContain(`== "${TOOLCHAIN.zig016}"`);
    expect(readFileSync("scripts/release-gpui/macos-prerequisite.sh", "utf8")).toContain(`== "${TOOLCHAIN.zig016}"`);
    expect(macos).toContain(`RIPGREP_VERSION: ${TOOLCHAIN.ripgrepVersion}`);
    expect(macos).toContain(`RIPGREP_PACKAGE_VERSION: ${TOOLCHAIN.ripgrepPackageVersion}`);
    expect(macos).toContain(`RIPGREP_SHA256: ${TOOLCHAIN.ripgrepSha256}`);
    expect(macos).toContain(`go-version-file: ${TOOLCHAIN.goVersionFile}`);
    expect(windows).toContain(`dotnet-version: ${TOOLCHAIN.dotnet}`);
    expect(windows).toContain(`vpk --version ${TOOLCHAIN.vpk}`);
    expect(android).toContain(`node-version: ${TOOLCHAIN.node}`);
    expect(android).toContain(`java-version: '${TOOLCHAIN.java}'`);
    expect(android).toContain(`"platforms;${TOOLCHAIN.androidPlatform}"`);
    expect(android).toContain(`"build-tools;${TOOLCHAIN.androidBuildTools}"`);
    expect(android).toContain(`"ndk;${TOOLCHAIN.androidNdk}"`);
    expect(readFileSync("scripts/release-gpui/prepare-zig.ps1", "utf8")).toContain(`$Version = "${TOOLCHAIN.zig016}"`);
  });

  /*
   * The 7.8.0 Ghostty-sync guard: the vendored source's own minimum_zig_version
   * is the authority for TOOLCHAIN.zig016. The standalone check script runs the
   * same assertion pre-dispatch; this test keeps it from rotting.
   */
  test("the Ghostty Zig pin satisfies the vendored source's declared minimum", async () => {
    const { checkGhosttyZigPin, readGhosttyMinimumZig } = await import("./check-ghostty-zig-pin.mjs");
    expect(() => checkGhosttyZigPin({ minimum: readGhosttyMinimumZig(), pin: TOOLCHAIN.zig016 })).not.toThrow();
    expect(() => checkGhosttyZigPin({ minimum: "0.17.0", pin: TOOLCHAIN.zig016 })).toThrow(/requires Zig 0\.17\.0/u);
  });

  test("Beads and code-server identity pins match their source of truth", () => {
    expect(BEADS_PINS.version).toBe(BEADS_VERSION);
    expect(BEADS_PINS.sourceRevision).toBe(BEADS_SOURCE_REVISION);
    expect(BEADS_PINS.schemaVersion).toBe(String(BEADS_SCHEMA_VERSION));
    expect(BEADS_PINS.packageId).toBe(BEADS_PACKAGE_ID);
    expect(CODE_SERVER_IDENTITY_REVISION).toBe(CODE_SERVER_COMPONENT_IDENTITY_REVISION);
  });
});
