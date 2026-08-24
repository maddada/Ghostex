#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, readFile, readdir, readlink } from "node:fs/promises";
import path from "node:path";
import { parseArgs } from "node:util";

const ignoredDirectoryNames = new Set([
  ".cache",
  ".git",
  ".next",
  ".turbo",
  ".vite",
  ".zig-cache",
  "DerivedData",
  "build",
  "coverage",
  "dist",
  "node_modules",
  "out",
  "storybook-static",
  "target",
  "tmp",
  "zig-out",
]);

/*
CDXC:LocalStartFast 2026-06-07-16:23:
`bun run start` must not rebuild app resource packages when their runtime inputs are unchanged. Hash explicit source paths, toolchain values, and package metadata so the native build can skip repeated work without relying on stale mtimes or broad fallback behavior.
*/
const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    path: { multiple: true, type: "string" },
    value: { multiple: true, type: "string" },
  },
});

const inputPaths = [...(values.path ?? []), ...positionals];
const inputValues = values.value ?? [];
const hash = createHash("sha256");

for (const value of inputValues) {
  hash.update("value\0");
  hash.update(value);
  hash.update("\0");
}

for (const inputPath of inputPaths) {
  await hashPath(path.resolve(inputPath), path.resolve(inputPath));
}

process.stdout.write(`${hash.digest("hex")}\n`);

async function hashPath(rootPath, currentPath) {
  const label = path.relative(rootPath, currentPath) || path.basename(rootPath);
  let info;
  try {
    info = await lstat(currentPath);
  } catch {
    hash.update("missing\0");
    hash.update(currentPath);
    hash.update("\0");
    return;
  }

  if (info.isSymbolicLink()) {
    hash.update("symlink\0");
    hash.update(label);
    hash.update("\0");
    hash.update(await readlink(currentPath));
    hash.update("\0");
    return;
  }

  if (info.isDirectory()) {
    if (ignoredDirectoryNames.has(path.basename(currentPath))) {
      return;
    }
    hash.update("dir\0");
    hash.update(label);
    hash.update("\0");
    const entries = await readdir(currentPath, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      await hashPath(rootPath, path.join(currentPath, entry.name));
    }
    return;
  }

  if (!info.isFile()) {
    return;
  }

  hash.update("file\0");
  hash.update(label);
  hash.update("\0");
  hash.update(await readFile(currentPath));
  hash.update("\0");
}
