import { chmod, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vitest";
import {
  BEADS_RELEASE_ARTIFACTS,
  BEADS_RELEASE_TAG,
  BEADS_VERSION,
  beadsReleaseArtifact,
  stageBeadsRelease,
} from "./beads-release.mjs";
import { smokeTestPackagedBeads } from "./smoke-test-packaged-beads.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

describe("official Beads release packaging", () => {
  test("pins the published v1.1.2 artifacts and checksums for every packaged platform", () => {
    expect(BEADS_VERSION).toBe("1.1.2");
    expect(BEADS_RELEASE_TAG).toBe("v1.1.2");
    expect(BEADS_RELEASE_ARTIFACTS).toEqual({
      darwin: {
        arm64: {
          name: "beads_1.1.2_darwin_arm64.tar.gz",
          sha256: "9b0137a83a2afd343e2abd2a506be72ea032721000f76669c2cf81729e78501d",
        },
        x64: {
          name: "beads_1.1.2_darwin_amd64.tar.gz",
          sha256: "0e94de9319c9d66cb7e0038bb17ebaf5dd2fe669e366a4b9153528b474a1a8f6",
        },
      },
      linux: {
        arm64: {
          name: "beads_1.1.2_linux_arm64.tar.gz",
          sha256: "a134015faf4be0a43f8681a8d602eaf0b7c255c957f09d3c933257c8c92fdd10",
        },
        x64: {
          name: "beads_1.1.2_linux_amd64.tar.gz",
          sha256: "a72d71ed374955dc9f83a0f90b54bd7b6a0016709dd1676ae2e368651ed401c2",
        },
      },
    });
    expect(beadsReleaseArtifact("macos", "x86_64").name).toBe(
      "beads_1.1.2_darwin_amd64.tar.gz",
    );
    expect(beadsReleaseArtifact("linux", "aarch64").name).toBe(
      "beads_1.1.2_linux_arm64.tar.gz",
    );
  });

  test("rejects platforms that Ghostex does not package as bd binaries", () => {
    expect(() => beadsReleaseArtifact("win32", "x64")).toThrow(/Unsupported Beads release platform/u);
    expect(() => beadsReleaseArtifact("darwin", "riscv64")).toThrow(/Unsupported Beads release platform/u);
  });

  test("rejects an archive that does not match the published checksum", async () => {
    const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "ghostex-beads-checksum-test-"));
    try {
      const archivePath = path.join(temporaryRoot, "beads_1.1.2_darwin_arm64.tar.gz");
      await writeFile(archivePath, "tampered archive", "utf8");
      await expect(stageBeadsRelease({
        arch: "arm64",
        archivePath,
        outputPath: path.join(temporaryRoot, "bd"),
        platform: "darwin",
      })).rejects.toThrow(/checksum mismatch/iu);
    } finally {
      await rm(temporaryRoot, { force: true, recursive: true });
    }
  });

  test("fails clearly when a packaged v1.1.2 binary lacks embedded Dolt support", async () => {
    const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "ghostex-beads-cgo-test-"));
    try {
      const fakeBd = path.join(temporaryRoot, "bd");
      await writeFile(
        fakeBd,
        `#!/bin/sh
if [ "\${1}" = version ]; then
  echo "bd version 1.1.2 (fake)"
  exit 0
fi
echo "embedded Dolt requires a CGO build" >&2
exit 1
`,
        "utf8",
      );
      await chmod(fakeBd, 0o755);
      await expect(smokeTestPackagedBeads(fakeBd)).rejects.toThrow(
        /lacks embedded-Dolt\/CGO support/iu,
      );
    } finally {
      await rm(temporaryRoot, { force: true, recursive: true });
    }
  });

  test("the macOS and Linux packagers stage and smoke-test the packaged binary", async () => {
    const [macosPackager, linuxPackager] = await Promise.all([
      readFile(path.join(repoRoot, "gpui", "scripts", "prepare-macos-runtime.sh"), "utf8"),
      readFile(path.join(repoRoot, "gxserver-rs", "package-remote-linux.mjs"), "utf8"),
    ]);
    expect(macosPackager).toContain("scripts/beads-release.mjs");
    expect(macosPackager).toContain("scripts/smoke-test-packaged-beads.mjs");
    expect(linuxPackager).toContain("stageBeadsRelease");
    expect(linuxPackager).toContain("smokeTestPackagedBeads");
    expect(linuxPackager).not.toContain('CGO_ENABLED: "0"');
    expect(linuxPackager).not.toContain("buildBeads(");
  });
});
