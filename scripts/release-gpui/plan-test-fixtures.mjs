/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
 * Test-only fixtures for the release planner.
 *
 * The fingerprint reads the git index, so the fixtures build a real throwaway
 * git repository (including real 160000 gitlink entries) rather than faking the
 * git layer. That keeps determinism, submodule-pin sensitivity, and pathspec
 * matching honest instead of asserting against a mock.
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import path from "node:path";
import { PRODUCT_IDS, TRUSTED_REPO, productDefinition } from "./product-inputs.mjs";
import { createGitTreeReader } from "./fingerprint.mjs";
import { buildProductProvenance, buildReleaseProvenance } from "./provenance.mjs";

const FIXTURE_GITLINKS = {
  ".dependencies/cef-rs": "0ddbc2accc06a3ac7f18e1543f752c3fb65161f2",
  ".dependencies/gpui-component": "5d6ea0453b2f977766419216d9e0a830cafdd349",
  ".dependencies/zed": "5775362fbd422f00ef7ca3e7a88b088a65d7c22b",
  "code-server": "390f119a145ec13b6421c5ec905163dd4cd20514",
  mobile: "65979ba85098bf336c49bbfc216c3e1ccb4702f1",
  zmx: "50e66a9b6cd1ecbc821669c8101e18c8e3c924d6",
};

const FIXTURE_FILES = {
  /*
   * These mirror the real `.github/workflows/release-gpui*.yml` set exactly:
   * every workflow file a product declares as a fingerprint input must exist in
   * the fixture, or "changing this workflow invalidates that product" is
   * untestable. release-gpui-runtime.yml is gone (split into the gxserver and
   * code-server workflows) and must not reappear here.
   */
  ".github/workflows/release-gpui-android.yml": "name: android\n",
  ".github/workflows/release-gpui-code-server.yml": "name: code-server component\n",
  ".github/workflows/release-gpui-gxserver.yml": "name: gxserver package\n",
  ".github/workflows/release-gpui-linux.yml": "name: linux\n",
  ".github/workflows/release-gpui-macos.yml": "name: macos\n",
  ".github/workflows/release-gpui-validate.yml": "name: windows validation\n",
  ".github/workflows/release-gpui-windows.yml": "name: windows\n",
  ".github/workflows/release-gpui-wsl-runtime.yml": "name: wsl\n",
  ".github/workflows/release-gpui.yml": "name: Release Ghostex\n",
  ".gitmodules": "[submodule \"mobile\"]\n",
  "CHANGELOG.md": "## 7.7.0 - notes\n",
  "bun.lock": "lockfile v1\n",
  "components.json": "{\"style\":\"default\"}\n",
  "components/ui/button.tsx": "export const Button = () => null;\n",
  "ghostex-paths/src/lib.rs": "pub fn home() {}\n",
  ".dependencies/ghostty/src/main.zig": "pub fn main() void {}\n",
  "gpui/Cargo.toml": "[package]\nname = \"ghostex-gpui\"\n",
  "gpui/src/main.rs": "fn main() {}\n",
  "gxserver-rs/src/constants.rs": "pub const GXSERVER_PROTOCOL_VERSION: u32 = 42;\n",
  "lib/utils.ts": "export const cn = () => \"\";\n",
  "media/sounds/zap.mp3": "sound\n",
  "native/sidebar/modal-host.tsx": "export const ModalHost = () => null;\n",
  "package.json": `${JSON.stringify(
    {
      dependencies: { react: "19.0.0" },
      devDependencies: { vitest: "3.0.0" },
      name: "ghostex",
      packageManager: "bun@1.3.10",
      private: true,
      scripts: { "release:test": "vitest run --config vitest.release.config.ts" },
      version: "7.7.0",
    },
    null,
    2,
  )}\n`,
  "scripts/beads-release.mjs": "export const BEADS_VERSION = \"1.1.0\";\n",
  "scripts/build-pinned-beads-release.mjs": "// beads\n",
  "scripts/build-remote-gxserver-linux-release.sh": "#!/usr/bin/env bash\n",
  "scripts/release-ghostex.mjs": "// remote package\n",
  "scripts/release-gpui/android.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/code-server-component-identity.mjs": "// identity\n",
  "scripts/release-gpui/common.sh": "# manifest writer\n",
  "scripts/release-gpui/create-deterministic-tar.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/fingerprint.mjs": "// fingerprint\n",
  "scripts/release-gpui/install-gxserver-wsl.ps1": "# wsl install\n",
  "scripts/release-gpui/linux-deb.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/linux-rpm.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/linux-stage.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/macos-finalize.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/macos-notary.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/macos-prerequisite.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/macos.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/on-demand-manifest.mjs": "// manifest v2\n",
  "scripts/release-gpui/patches/code-server-ripgrep-target-validation.patch": "--- a\n+++ b\n",
  "scripts/release-gpui/prepare-references.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/prepare-sparkle.sh": "#!/usr/bin/env bash\n",
  "scripts/release-gpui/prepare-zig.ps1": "# zig\n",
  "scripts/release-gpui/product-inputs.mjs": "// products\n",
  "scripts/release-gpui/publish-component.mjs": "// components\n",
  "scripts/release-gpui/verify-code-server-archive.mjs": "// verify\n",
  "scripts/release-gpui/windows-update-feed.mjs": "// feeds\n",
  "scripts/release-gpui/windows.ps1": "# windows\n",
  "scripts/release-gpui/wsl-runtime.sh": "#!/usr/bin/env bash\n",
  "scripts/release-mobile/android.sh": "#!/usr/bin/env bash\n",
  "scripts/smoke-test-packaged-beads.mjs": "// smoke\n",
  "scripts/validate-macos-app-bundle.mjs": "// bundle\n",
  "shared/ghostex-settings.ts": "export const settings = {};\n",
  "sidebar/sidebar-app.tsx": "export const SidebarApp = () => null;\n",
  "skills/ghostex-cli/SKILL.md": "# skill\n",
  "tsconfig.json": "{\"compilerOptions\":{}}\n",
  ".dependencies/tui2/src/main.rs": "fn main() {}\n",
};

function git(dir, args) {
  return execFileSync(
    "git",
    [
      "-C",
      dir,
      "-c",
      "commit.gpgsign=false",
      "-c",
      "core.hooksPath=/dev/null",
      /*
       * A developer's global `core.excludesFile` must not decide which fixture
       * files get staged — an ignored `build/` or `*.mjs` rule would silently
       * shrink the fingerprint input set and make these tests pass for the wrong
       * reason on one machine and fail on another.
       */
      "-c",
      "core.excludesFile=/dev/null",
      ...args,
    ],
    {
      encoding: "utf8",
      env: {
        ...process.env,
        GIT_AUTHOR_DATE: "2026-01-01T00:00:00Z",
        GIT_AUTHOR_EMAIL: "release@ghostex.test",
        GIT_AUTHOR_NAME: "Ghostex Release Test",
        GIT_COMMITTER_DATE: "2026-01-01T00:00:00Z",
        GIT_COMMITTER_EMAIL: "release@ghostex.test",
        GIT_COMMITTER_NAME: "Ghostex Release Test",
      },
    },
  ).trim();
}

export function createFixtureRepo({ files = {}, gitlinks = {} } = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), "ghostex-release-plan-"));
  git(dir, ["init", "-q", "-b", "main"]);
  const repo = {
    commit(message = "change") {
      git(dir, ["add", "-A"]);
      /*
       * `git add -A` stages a deletion for every gitlink whose directory is not
       * checked out, so the pinned submodule entries are re-applied afterwards.
       */
      for (const [relativePath, sha] of Object.entries(repo.gitlinks)) {
        git(dir, ["update-index", "--add", "--cacheinfo", `160000,${sha},${relativePath}`]);
      }
      git(dir, ["commit", "-q", "--allow-empty", "-m", message]);
      repo.head = git(dir, ["rev-parse", "HEAD"]);
      return repo.head;
    },
    dir,
    dispose() {
      rmSync(dir, { force: true, recursive: true });
    },
    gitlinks: {},
    reader: createGitTreeReader({ repoRoot: dir }),
    setGitlink(relativePath, sha) {
      repo.gitlinks[relativePath] = sha;
    },
    write(relativePath, contents) {
      const target = path.join(dir, relativePath);
      mkdirSync(path.dirname(target), { recursive: true });
      writeFileSync(target, contents);
    },
  };
  for (const [relativePath, contents] of Object.entries({ ...FIXTURE_FILES, ...files })) {
    repo.write(relativePath, contents);
  }
  git(dir, ["add", "-A"]);
  for (const [relativePath, sha] of Object.entries({ ...FIXTURE_GITLINKS, ...gitlinks })) {
    repo.setGitlink(relativePath, sha);
  }
  repo.head = repo.commit("fixture base");
  return repo;
}

export function fixtureDigest(seed) {
  return createHash("sha256").update(String(seed)).digest("hex");
}

/*
 * `artifactBytes` lets a test hand in the *real* bytes an artifact will have on
 * disk, so the recorded digests match what the materializer re-hashes after
 * downloading. Without it the digests are stable-but-synthetic, which is all the
 * planner needs and none of what a byte-equality check needs.
 */
function fixtureArtifacts(productId, version, seed, artifactBytes = {}) {
  const real = artifactBytes[productId] ?? {};
  return productDefinition(productId)
    .artifacts(version)
    .map((name, index) => {
      const bytes = real[name];
      if (bytes !== undefined) {
        const buffer = Buffer.isBuffer(bytes) ? bytes : Buffer.from(String(bytes));
        return { name, sha256: createHash("sha256").update(buffer).digest("hex"), size: buffer.length };
      }
      return { name, sha256: fixtureDigest(`${seed}:${productId}:${name}`), size: 1024 + index };
    });
}

/* Every non-skipped product of `plan`, recorded as if this run had built it. */
export function productRecordsFromPlan({ artifactBytes = {}, plan, runId, seed = "seed", sourceSha, tag, version }) {
  const records = {};
  for (const productId of PRODUCT_IDS) {
    const entry = plan.products[productId];
    if (entry.action === "skip") continue;
    records[productId] = buildProductProvenance({
      action: "built",
      artifacts: fixtureArtifacts(productId, version, seed, artifactBytes),
      fingerprint: entry.fingerprint,
      inputs: entry.inputs,
      originRunId: runId,
      originSourceSha: sourceSha,
      originTag: tag,
      product: productId,
      productVersion: version,
      releaseVersion: version,
      sourceSha,
    });
  }
  return records;
}

/* A Tier-1 reuse baseline: a published release carrying a provenance asset. */
export function releaseBaselineFromPlan({
  artifactBytes = {},
  commit,
  plan,
  publishedAt = "2026-07-01T00:00:00.000Z",
  runId = 31000000001,
  seed = "seed",
  version,
}) {
  const tag = `v${version}`;
  const products = productRecordsFromPlan({ artifactBytes, plan, runId, seed, sourceSha: commit, tag, version });
  const assets = [];
  for (const record of Object.values(products)) {
    for (const artifact of record.artifacts) {
      assets.push({ digest: `sha256:${artifact.sha256}`, name: artifact.name, size: artifact.size });
    }
  }
  return {
    assets,
    commit,
    draft: false,
    provenance: buildReleaseProvenance({
      components: {},
      plan,
      products,
      publishedAt,
      sourceSha: commit,
      version,
      workflowRunId: runId,
    }),
    publishedAt,
    repo: TRUSTED_REPO,
    tag,
  };
}

/* A Tier-2 reuse source: a completed but unpublished Release Ghostex run. */
export function sourceRunFromPlan({
  artifactBytes = {},
  plan,
  headSha,
  runId = 31644067583,
  seed = "run",
  version,
  overrides = {},
}) {
  return {
    conclusion: "success",
    event: "workflow_dispatch",
    expiredArtifacts: [],
    headSha,
    products: productRecordsFromPlan({
      artifactBytes,
      plan,
      runId,
      seed,
      sourceSha: headSha,
      tag: `v${version}`,
      version,
    }),
    repo: TRUSTED_REPO,
    runId,
    workflowName: "Release Ghostex",
    ...overrides,
  };
}

export function componentTagStateFixture() {
  return {
    cef: {
      componentVersion: "148.4.0-148.0.10",
      platforms: {
        "darwin-arm64": { assetName: "cef-148.4.0-148.0.10-darwin-arm64.tar.gz", sha256: fixtureDigest("cef-darwin-arm64"), sizeBytes: 10 },
        "linux-x64": { assetName: "cef-148.4.0-148.0.10-linux-x64.tar.gz", sha256: fixtureDigest("cef-linux-x64"), sizeBytes: 10 },
        "windows-arm64": { assetName: "cef-148.4.0-148.0.10-windows-arm64.tar.gz", sha256: fixtureDigest("cef-win-arm64"), sizeBytes: 10 },
        "windows-x64": { assetName: "cef-148.4.0-148.0.10-windows-x64.tar.gz", sha256: fixtureDigest("cef-win-x64"), sizeBytes: 10 },
      },
    },
    "code-server": {
      componentVersion: `390f119a145e-p2-${fixtureDigest("payload")}`,
      platforms: Object.fromEntries(
        ["darwin-arm64", "linux-arm64", "linux-x64", "windows-arm64", "windows-x64"].map((platform) => [
          platform,
          { assetName: `code-server-x-${platform}.tar.gz`, sha256: fixtureDigest(platform), sizeBytes: 10 },
        ]),
      ),
    },
  };
}
