#!/usr/bin/env node
/*
 * Fail-fast guard for the vendored Zig sources' toolchain pin.
 *
 * Release 7.8.0 lost a full CI round (14 minutes to first failure, plus a
 * redispatch) because a Ghostty upstream sync raised the source's required Zig
 * to 0.16 while every release pin still installed 0.15. Each vendored source's
 * own declaration — `.minimum_zig_version` in its build.zig.zon — is the
 * authority; TOOLCHAIN.zig is the release pin mirrored into the workflows
 * (product-inputs.test.mjs asserts those mirrors). This script closes the last
 * gap: pin versus source. It is pure node with no dependencies so the
 * dispatcher can run it before spending any runner minutes, and the prepare
 * job runs it again on the authoritative checkout.
 *
 * zmx is checked alongside Ghostty since its fork was re-ported onto
 * upstream/main (Zig 0.16). It was the repo's last Zig 0.15 consumer, so
 * TOOLCHAIN.zig is now the single pin both sources must satisfy, and a future
 * zmx upstream sync that raises its minimum fails here instead of in CI.
 */

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { TOOLCHAIN } from "./product-inputs.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

export function parseZigTriple(version, label) {
  const match = /^(\d+)\.(\d+)\.(\d+)/u.exec(String(version).trim());
  if (!match) throw new Error(`${label} is not a Zig version: ${version}`);
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]) };
}

/* Vendored Zig sources whose declared minimum must be satisfied by TOOLCHAIN.zig. */
const ZIG_SOURCES = Object.freeze([
  { manifest: ".dependencies/ghostty/build.zig.zon", source: "Ghostty" },
  { manifest: ".dependencies/zmx/build.zig.zon", source: "zmx" },
]);

export function checkGhosttyZigPin({ minimum, pin, source = "Ghostty", manifest = ZIG_SOURCES[0].manifest }) {
  const required = parseZigTriple(minimum, `${source} minimum_zig_version`);
  const pinned = parseZigTriple(pin, "TOOLCHAIN.zig");
  const sameSeries = pinned.major === required.major && pinned.minor === required.minor;
  if (!sameSeries || pinned.patch < required.patch) {
    throw new Error(
      `The vendored ${source} source requires Zig ${minimum} (${manifest} minimum_zig_version) ` +
        `but the release toolchain pins Zig ${pin}. Update TOOLCHAIN.zig in ` +
        "tooling/release-gpui/product-inputs.mjs; product-inputs.test.mjs then enumerates every " +
        "workflow and script mirror that must move with it.",
    );
  }
}

function readMinimumZig(manifest, root) {
  const zon = readFileSync(path.join(root, manifest), "utf8");
  const match = /\.minimum_zig_version\s*=\s*"([^"]+)"/u.exec(zon);
  if (!match) throw new Error(`${manifest} declares no minimum_zig_version`);
  return match[1];
}

export function readGhosttyMinimumZig(root = repoRoot) {
  return readMinimumZig(".dependencies/ghostty/build.zig.zon", root);
}

export function readZmxMinimumZig(root = repoRoot) {
  return readMinimumZig(".dependencies/zmx/build.zig.zon", root);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    for (const { manifest, source } of ZIG_SOURCES) {
      const minimum = readMinimumZig(manifest, repoRoot);
      checkGhosttyZigPin({ manifest, minimum, pin: TOOLCHAIN.zig, source });
      console.log(`${source} Zig pin ok: source requires ${minimum}, release pins ${TOOLCHAIN.zig}.`);
    }
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
