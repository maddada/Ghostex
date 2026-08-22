import { afterAll, describe, expect, test } from "vitest";
import {
  FINGERPRINT_ALGORITHM_REVISION,
  computeFingerprints,
  computeNodeFingerprint,
  describeFingerprintDifference,
  explainFingerprintDifference,
  normalizePathspec,
  parseTreeEntries,
  projectPackageJson,
} from "./fingerprint.mjs";
import { defaultScope } from "./product-inputs.mjs";
import { createFixtureRepo } from "./plan-test-fixtures.mjs";

const repo = createFixtureRepo({ files: { "apps/desktop/target/release/ghostex": "compiled output\n" } });
afterAll(() => repo.dispose());

function fingerprintsAt(sha, { scope = defaultScope(), version = "7.7.0" } = {}) {
  const map = computeFingerprints({
    context: { scope, version },
    entries: repo.reader.listTree(sha),
    readObject: (objectId) => repo.reader.readObject(objectId),
  });
  return Object.fromEntries([...map.entries()].map(([id, value]) => [id, value.fingerprint]));
}

function changed(before, after) {
  return Object.keys(before)
    .filter((id) => before[id] !== after[id])
    .sort();
}

const base = repo.head;
const baseFingerprints = fingerprintsAt(base);

describe("release fingerprint algorithm", () => {
  test("is deterministic for the same commit, version, and scope", () => {
    expect(fingerprintsAt(base)).toEqual(baseFingerprints);
  });

  test("binds every digest to the algorithm revision and the node id", () => {
    const entries = repo.reader.listTree(base);
    const shared = {
      entries,
      pathspecs: [{ pathspec: "package.json" }],
      readObject: (objectId) => repo.reader.readObject(objectId),
      releaseVersion: "7.7.0",
    };
    const first = computeNodeFingerprint({ ...shared, nodeId: "alpha" });
    const second = computeNodeFingerprint({ ...shared, nodeId: "beta" });
    expect(FINGERPRINT_ALGORITHM_REVISION).toBe("fp2");
    expect(first.fingerprint).not.toBe(second.fingerprint);
    expect(first.inputs.paths).toEqual([
      { digest: expect.stringMatching(/^[0-9a-f]{64}$/u), entryCount: 1, pathspec: "package.json" },
    ]);
  });

  test("ignores tracked paths that no product declares", () => {
    repo.write("CHANGELOG.md", "## 7.8.0 - unrelated notes\n");
    repo.write("plans/007-some-plan.md", "planning\n");
    const after = fingerprintsAt(repo.commit("metadata only"));
    expect(changed(baseFingerprints, after)).toEqual([]);
  });

  test("ignores tracked paths inside a declared exclusion", () => {
    const before = fingerprintsAt(repo.head ?? base);
    repo.write("apps/desktop/target/release/ghostex", "different compiled output\n");
    const after = fingerprintsAt(repo.commit("build output churn"));
    expect(changed(before, after)).toEqual([]);
  });

  test("moves only the products that declare a changed path", () => {
    const before = fingerprintsAt(repo.commit("checkpoint"));
    repo.write("shared/ghostex-settings.ts", "export const settings = { changed: true };\n");
    const after = fingerprintsAt(repo.commit("shared change"));
    const moved = changed(before, after);
    expect(moved).toContain("macos-arm64");
    expect(moved).toContain("linux-deb-x64");
    expect(moved).toContain("linux-rpm-x64");
    expect(moved).toContain("windows-x64");
    expect(moved).toContain("windows-arm64");
    /* §4.11 rule 3: shared/** was deliberately removed from the remote Linux package. */
    expect(moved).not.toContain("gxserver-linux-x64");
    expect(moved).not.toContain("gxserver-linux-arm64");
    /* §4.5: the mobile submodule is self-contained. */
    expect(moved).not.toContain("android");
  });

  test("tracks submodule pins through gitlinks without a checkout", () => {
    const before = fingerprintsAt(repo.commit("checkpoint"));
    repo.setGitlink("apps/mobile/app", "1111111111111111111111111111111111111111");
    const after = fingerprintsAt(repo.commit("mobile pin bump"));
    const moved = changed(before, after);
    expect(moved).toEqual(["android"]);
  });

  test("propagates a gxserver change into every desktop product through composition", () => {
    const before = fingerprintsAt(repo.commit("checkpoint"));
    repo.setGitlink(".dependencies/zmx", "2222222222222222222222222222222222222222");
    const after = fingerprintsAt(repo.commit("zmx pin bump"));
    const moved = changed(before, after);
    for (const product of [
      "gxserver-linux-x64",
      "gxserver-linux-arm64",
      "gxserver-wsl-windows-x64",
      "gxserver-wsl-windows-arm64",
      "macos-arm64",
      "linux-deb-x64",
      "linux-rpm-x64",
      "windows-x64",
      "windows-arm64",
    ]) {
      expect(moved).toContain(product);
    }
    expect(moved).not.toContain("android");
  });

  test("propagates a cef-rs pin change into the cef node and every desktop product", () => {
    const before = fingerprintsAt(repo.commit("checkpoint"));
    repo.setGitlink(".dependencies/cef-rs", "3333333333333333333333333333333333333333");
    const after = fingerprintsAt(repo.commit("cef pin bump"));
    const moved = changed(before, after);
    expect(moved).toContain("cef");
    expect(moved).toContain("macos-arm64");
    expect(moved).toContain("windows-x64");
    expect(moved).toContain("linux-deb-x64");
    expect(moved).not.toContain("android");
    expect(moved).not.toContain("gxserver-linux-x64");
  });

  test("propagates a code-server pin change into macOS and Windows but not Linux packages", () => {
    const before = fingerprintsAt(repo.commit("checkpoint"));
    repo.setGitlink(".dependencies/code-server", "4444444444444444444444444444444444444444");
    const after = fingerprintsAt(repo.commit("code-server pin bump"));
    const moved = changed(before, after);
    expect(moved).toContain("code-server");
    expect(moved).toContain("macos-arm64");
    expect(moved).toContain("windows-x64");
    expect(moved).toContain("windows-arm64");
    /* §4.11 rule 6: Linux seals only cef. */
    expect(moved).not.toContain("linux-deb-x64");
    expect(moved).not.toContain("linux-rpm-x64");
  });

  test("a package.json version bump alone changes nothing", () => {
    const head = repo.commit("checkpoint");
    const before = fingerprintsAt(head, { version: "7.7.0" });
    repo.write(
      "package.json",
      `${JSON.stringify(
        {
          dependencies: { react: "19.0.0" },
          devDependencies: { vitest: "3.0.0" },
          name: "ghostex",
          packageManager: "bun@1.3.10",
          private: true,
          scripts: { "release:test": "vitest run --config vitest.release.config.ts" },
          version: "7.8.0",
        },
        null,
        2,
      )}\n`,
    );
    const after = fingerprintsAt(repo.commit("version bump"), { version: "7.7.0" });
    expect(changed(before, after)).toEqual([]);
  });

  test("a release version bump moves version-stamped products only", () => {
    const head = repo.commit("checkpoint");
    const atOldVersion = fingerprintsAt(head, { version: "7.7.0" });
    const atNewVersion = fingerprintsAt(head, { version: "7.8.0" });
    expect(changed(atOldVersion, atNewVersion)).toEqual([
      "gxserver-wsl-windows-arm64",
      "gxserver-wsl-windows-x64",
      "linux-deb-x64",
      "linux-rpm-x64",
      "macos-arm64",
      "windows-arm64",
      "windows-x64",
    ]);
  });

  test("a package.json script change does move every product", () => {
    const before = fingerprintsAt(repo.commit("checkpoint"));
    repo.write(
      "package.json",
      `${JSON.stringify(
        {
          dependencies: { react: "19.0.0" },
          devDependencies: { vitest: "3.0.0" },
          name: "ghostex",
          packageManager: "bun@1.3.10",
          private: true,
          scripts: { "release:test": "vitest run --config vitest.release.config.ts", "release:plan": "node x.mjs" },
          version: "7.8.0",
        },
        null,
        2,
      )}\n`,
    );
    const after = fingerprintsAt(repo.commit("script change"));
    for (const product of ["android", "macos-arm64", "gxserver-linux-x64"]) {
      expect(changed(before, after)).toContain(product);
    }
  });

  test("signing mode and Sparkle scope are fingerprint values", () => {
    const head = repo.commit("checkpoint");
    const unsigned = fingerprintsAt(head, { scope: defaultScope({ signWindows: false }) });
    const signed = fingerprintsAt(head, { scope: defaultScope({ signWindows: true }) });
    expect(changed(unsigned, signed)).toEqual(["windows-arm64", "windows-x64"]);

    const withSparkle = fingerprintsAt(head, { scope: defaultScope({ updateSparkle: true }) });
    const withoutSparkle = fingerprintsAt(head, { scope: defaultScope({ updateSparkle: false }) });
    expect(changed(withSparkle, withoutSparkle)).toEqual(["macos-arm64"]);
  });
});

describe("fingerprint helpers", () => {
  test("normalizes pathspec sugar and refuses unsupported globs", () => {
    expect(normalizePathspec("gpui/**")).toEqual({ negative: false, prefix: "gpui" });
    expect(normalizePathspec(":(exclude)gpui/target")).toEqual({ negative: true, prefix: "gpui/target" });
    expect(() => normalizePathspec("gpui/**/*.rs")).toThrow(/Unsupported glob/u);
  });

  test("parses ls-tree records including gitlinks", () => {
    const NUL = String.fromCharCode(0);
    const output = `100644 blob abc\tpackage.json${NUL}160000 commit def\tmobile${NUL}`;
    expect(parseTreeEntries(output)).toEqual([
      { mode: "160000", objectId: "def", path: "mobile", type: "commit" },
      { mode: "100644", objectId: "abc", path: "package.json", type: "blob" },
    ]);
  });

  test("projects package.json without its version", () => {
    const withVersion = projectPackageJson(Buffer.from(JSON.stringify({ scripts: { a: "b" }, version: "1.0.0" })));
    const withOther = projectPackageJson(Buffer.from(JSON.stringify({ scripts: { a: "b" }, version: "2.0.0" })));
    expect(withVersion).toBe(withOther);
    expect(withVersion).not.toContain("1.0.0");
  });

  test("explains which declared inputs moved", () => {
    const current = {
      composed: { "gxserver-linux-x64": "aa" },
      paths: [
        { digest: "1", entryCount: 1, pathspec: "gpui/**" },
        { digest: "2", entryCount: 1, pathspec: "shared/**" },
      ],
      values: { zig015: "0.15.2" },
    };
    const baseline = {
      composed: { "gxserver-linux-x64": "bb" },
      paths: [
        { digest: "1", entryCount: 1, pathspec: "gpui/**" },
        { digest: "9", entryCount: 1, pathspec: "shared/**" },
      ],
      values: { zig015: "0.15.1" },
    };
    const difference = explainFingerprintDifference(current, baseline);
    expect(difference).toEqual({ composed: ["gxserver-linux-x64"], paths: ["shared/**"], values: ["zig015"] });
    expect(describeFingerprintDifference(difference)).toBe(
      "shared/**; values zig015; embedded gxserver-linux-x64",
    );
  });
});
