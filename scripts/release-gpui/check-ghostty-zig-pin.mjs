#!/usr/bin/env node
/*
 * Fail-fast guard for the Ghostty Zig toolchain pin.
 *
 * Release 7.8.0 lost a full CI round (14 minutes to first failure, plus a
 * redispatch) because a Ghostty upstream sync raised the source's required Zig
 * to 0.16 while every release pin still installed 0.15. The vendored source's
 * own declaration — `.minimum_zig_version` in ghostty/build.zig.zon — is the
 * authority; TOOLCHAIN.zig016 is the release pin mirrored into the workflows
 * (product-inputs.test.mjs asserts those mirrors). This script closes the last
 * gap: pin versus source. It is pure node with no dependencies so the
 * dispatcher can run it before spending any runner minutes, and the prepare
 * job runs it again on the authoritative checkout.
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

export function checkGhosttyZigPin({ minimum, pin }) {
  const required = parseZigTriple(minimum, "ghostty minimum_zig_version");
  const pinned = parseZigTriple(pin, "TOOLCHAIN.zig016");
  const sameSeries = pinned.major === required.major && pinned.minor === required.minor;
  if (!sameSeries || pinned.patch < required.patch) {
    throw new Error(
      `The vendored Ghostty source requires Zig ${minimum} (ghostty/build.zig.zon minimum_zig_version) ` +
        `but the release toolchain pins Zig ${pin}. Update TOOLCHAIN.zig016 in ` +
        "scripts/release-gpui/product-inputs.mjs; product-inputs.test.mjs then enumerates every " +
        "workflow and script mirror that must move with it.",
    );
  }
}

export function readGhosttyMinimumZig(root = repoRoot) {
  const zon = readFileSync(path.join(root, "ghostty/build.zig.zon"), "utf8");
  const match = /\.minimum_zig_version\s*=\s*"([^"]+)"/u.exec(zon);
  if (!match) throw new Error("ghostty/build.zig.zon declares no minimum_zig_version");
  return match[1];
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const minimum = readGhosttyMinimumZig();
    checkGhosttyZigPin({ minimum, pin: TOOLCHAIN.zig016 });
    console.log(`Ghostty Zig pin ok: source requires ${minimum}, release pins ${TOOLCHAIN.zig016}.`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
