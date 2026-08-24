import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, test } from "vitest";
import {
  authenticateComponentChecksumSidecar,
  buildOnDemandManifestV2,
  validateMacosReleaseOnDemandManifest,
  validateOnDemandManifestV2,
} from "./on-demand-manifest.mjs";
import {
  componentAssetsFromDirectory,
  componentChecksumSidecars,
  planComponentRelease,
  sha256File,
  verifyPublishedComponent,
} from "./publish-component.mjs";
import { validateMacosAppBundle } from "../validate-macos-app-bundle.mjs";

const digest = "a".repeat(64);

function manifest() {
  return buildOnDemandManifestV2({
    version: "6.13.0",
    assets: {
      "gxserver-linux-arm64": { bytes: 123, name: "gxserver-linux-arm64.tar.gz", sha256: digest },
    },
    components: {
      cef: {
        name: "cef",
        componentVersion: "138.0.1",
        downloadTag: "cef-138.0.1",
        platforms: {
          "darwin-arm64": { assetName: "cef-138.0.1-darwin-arm64.tar.gz", sha256: digest, sizeBytes: 456 },
        },
      },
    },
  });
}

describe("on-demand manifest v2", () => {
  test("accepts release assets and versioned components", () => {
    expect(validateOnDemandManifestV2(manifest()).schemaVersion).toBe(2);
  });

  test("rejects malformed component data instead of dropping it", () => {
    const malformed = manifest();
    malformed.components.cef.platforms["darwin-arm64"].sha256 = "bad";
    expect(() => validateOnDemandManifestV2(malformed)).toThrow(/sha256/);
  });

  test("rejects component tags and asset names that do not match the immutable naming contract", () => {
    const malformedTag = manifest();
    malformedTag.components.cef.downloadTag = "cef-latest";
    expect(() => validateOnDemandManifestV2(malformedTag)).toThrow(/downloadTag must equal cef-138\.0\.1/);

    const malformedAsset = manifest();
    malformedAsset.components.cef.platforms["darwin-arm64"].assetName = "cef.tar.gz";
    expect(() => validateOnDemandManifestV2(malformedAsset)).toThrow(
      /assetName must equal cef-138\.0\.1-darwin-arm64\.tar\.gz/,
    );
  });

  test("requires both Linux code-server architectures for macOS releases", () => {
    const releaseManifest = manifest();
    releaseManifest.components["code-server"] = {
      name: "code-server",
      componentVersion: "4.99.0",
      downloadTag: "code-server-4.99.0",
      platforms: Object.fromEntries(
        ["darwin-arm64", "linux-x64", "linux-arm64"].map((platform) => [
          platform,
          {
            assetName: `code-server-4.99.0-${platform}.tar.gz`,
            sha256SidecarName: `code-server-4.99.0-${platform}.tar.gz.sha256`,
            sha256: digest,
            sizeBytes: 789,
          },
        ]),
      ),
    };
    expect(validateMacosReleaseOnDemandManifest(releaseManifest)).toBe(releaseManifest);

    delete releaseManifest.components["code-server"].platforms["linux-arm64"];
    expect(() => validateMacosReleaseOnDemandManifest(releaseManifest)).toThrow(
      /macOS releases require components\.code-server\.platforms\.linux-arm64/,
    );
  });

  test('requires the exact code-server outer checksum sidecar name in the sealed manifest', () => {
    const releaseManifest = manifest();
    const assetName = 'code-server-version-windows-x64.tar.gz';
    releaseManifest.components['code-server'] = {
      name: 'code-server',
      componentVersion: 'version',
      downloadTag: 'code-server-version',
      platforms: {
        'windows-x64': { assetName, sha256: digest, sizeBytes: 789 },
      },
    };
    expect(() => validateOnDemandManifestV2(releaseManifest)).toThrow(/sha256SidecarName/);

    releaseManifest.components['code-server'].platforms['windows-x64'].sha256SidecarName =
      'wrong.tar.gz.sha256';
    expect(() => validateOnDemandManifestV2(releaseManifest)).toThrow(
      /sha256SidecarName must equal code-server-version-windows-x64\.tar\.gz\.sha256/,
    );
  });

  test.each([
    ['missing', undefined, /must be UTF-8 text/],
    ['malformed', `${digest}\n`, /one filename-bound/],
    ['wrong name', `${digest}  wrong.tar.gz\n`, /filename must equal/],
    ['digest mismatch', `${'b'.repeat(64)}  code-server-version-windows-x64.tar.gz\n`, /sealed digest/],
  ])('rejects a %s downloaded outer checksum sidecar', (_label, contents, expectedError) => {
    expect(() =>
      authenticateComponentChecksumSidecar(
        contents,
        'code-server-version-windows-x64.tar.gz',
        digest,
      ),
    ).toThrow(expectedError);
  });
});

describe("component-tag publisher idempotency", () => {
  test("creates identical component archives from identical files with different mtimes", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "ghostex-deterministic-component-"));
    const first = path.join(root, "first");
    const second = path.join(root, "second");
    await mkdir(first);
    await mkdir(second);
    await writeFile(path.join(first, "payload"), "same bytes");
    await writeFile(path.join(second, "payload"), "same bytes");
    await utimes(path.join(first, "payload"), new Date("2024-01-01"), new Date("2024-01-01"));
    await utimes(path.join(second, "payload"), new Date("2026-01-01"), new Date("2026-01-01"));
    const script = path.resolve("tooling/release-gpui/create-deterministic-tar.sh");
    const firstArchive = path.join(root, "first.tar.gz");
    const secondArchive = path.join(root, "second.tar.gz");
    execFileSync(script, [first, firstArchive]);
    execFileSync(script, [second, secondArchive]);
    expect(sha256File(firstArchive)).toBe(sha256File(secondArchive));
  });

  test("plans create-if-missing, no-op-if-matching, and error-if-sha-mismatch", async () => {
    const assetDir = await mkdtemp(path.join(tmpdir(), "ghostex-component-publisher-"));
    await writeFile(path.join(assetDir, "cef-138.0.1-darwin-arm64.tar.gz"), "fake-cef");
    const assets = componentAssetsFromDirectory({ assetDir, component: "cef", componentVersion: "138.0.1" });

    expect(planComponentRelease({ assets, release: { exists: false, assets: [] } })).toMatchObject({
      createRelease: true,
      uploads: assets,
    });
    expect(
      planComponentRelease({
        assets,
        release: { exists: true, assets: [{ name: assets[0].assetName, size: assets[0].sizeBytes, digest: `sha256:${assets[0].sha256}` }] },
      }),
    ).toMatchObject({ createRelease: false, noops: assets, uploads: [] });
    expect(() =>
      planComponentRelease({
        assets,
        release: { exists: true, assets: [{ name: assets[0].assetName, size: assets[0].sizeBytes, digest: `sha256:${"b".repeat(64)}` }] },
      }),
    ).toThrow(/Refusing to replace/);
  });

  test("publishes exact filename-bound checksum sidecars with component archives", async () => {
    const assetDir = await mkdtemp(path.join(tmpdir(), "ghostex-component-sidecar-"));
    const archiveName = "code-server-version-darwin-arm64.tar.gz";
    const archivePath = path.join(assetDir, archiveName);
    await writeFile(archivePath, "darwin-code-server");
    const assets = componentAssetsFromDirectory({ assetDir, component: "code-server", componentVersion: "version" });
    await writeFile(`${archivePath}.sha256`, `${assets[0].sha256}  ${archiveName}\n`);

    const sidecars = componentChecksumSidecars(assets);
    expect(sidecars).toMatchObject([
      { assetName: `${archiveName}.sha256`, filePath: `${archivePath}.sha256` },
    ]);
    expect(
      planComponentRelease({ assets: [...assets, ...sidecars], release: { exists: false, assets: [] } }).uploads.map(
        (asset) => asset.assetName,
      ),
    ).toEqual([archiveName, `${archiveName}.sha256`]);
    const releaseState = path.join(assetDir, "release-state.json");
    await writeFile(releaseState, JSON.stringify({ exists: false, assets: [] }));
    const publisherOutput = execFileSync(
      process.execPath,
      [
        path.resolve("tooling/release-gpui/publish-component.mjs"),
        "--component",
        "code-server",
        "--version",
        "version",
        "--asset-dir",
        assetDir,
        "--require-sha256-sidecars",
        "--release-state",
        releaseState,
        "--dry-run",
      ],
      { encoding: "utf8" },
    );
    expect(publisherOutput).toContain(`UPLOAD ${archiveName} `);
    expect(publisherOutput).toContain(`UPLOAD ${archiveName}.sha256 `);
    expect(publisherOutput).toContain(`"sha256SidecarName": "${archiveName}.sha256"`);
    await writeFile(`${archivePath}.sha256`, `${assets[0].sha256}  wrong-name.tar.gz\n`);
    expect(() => componentChecksumSidecars(assets)).toThrow(/filename/);
  });

  test("reports missing and mismatched component tags with the publisher fix command", () => {
    const component = manifest().components.cef;
    expect(() => verifyPublishedComponent({ component, release: { exists: false, assets: [] } })).toThrow(
      /Fix: bun run release:component -- --component cef --version 138\.0\.1/,
    );
    expect(() =>
      verifyPublishedComponent({
        component,
        release: {
          exists: true,
          assets: [{
            name: component.platforms["darwin-arm64"].assetName,
            size: component.platforms["darwin-arm64"].sizeBytes,
            digest: `sha256:${"b".repeat(64)}`,
          }],
        },
      }),
    ).toThrow(/mismatched size\/digest.*Fix: publish a newly versioned component/s);
  });
});

describe("macOS release bundle shape", () => {
  test("rejects a legacy-shaped bundle before architecture checks", async () => {
    const appPath = await mkdtemp(path.join(tmpdir(), "ghostex-legacy-app-"));
    await mkdir(path.join(appPath, "Contents", "Resources", "Web", "code-server"), { recursive: true });
    await mkdir(path.join(appPath, "Contents", "Frameworks", "Chromium Embedded Framework.framework"), {
      recursive: true,
    });
    await expect(validateMacosAppBundle({ appPath, arch: "arm64" })).rejects.toThrow(
      /must contain a sealed on-demand manifest v2.*legacy bundles are not valid release output/,
    );
  });
});
