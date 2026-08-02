#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  missingManagedTooltipPlacements,
  rustSourcesUnder,
} from "./reference-contract-lib.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDirectory, "../..");
const prepareScript = join(scriptDirectory, "prepare-references.sh");
const tooltipPatch = join(
  scriptDirectory,
  "patches/gpui-component-managed-tooltip-placement.patch",
);
const scrollbarPatch = join(
  scriptDirectory,
  "patches/gpui-component-scrollbar-options.patch",
);
const expectedChangedFiles = [
  "crates/ui/src/menu/popup_menu.rs",
  "crates/ui/src/scroll/scrollbar.rs",
  "crates/ui/src/tooltip.rs",
];
const cleanOnly = process.argv.slice(2).includes("--clean");

function git(cwd, args) {
  return execFileSync(
    "git",
    ["-c", `safe.directory=${cwd}`, ...args],
    { cwd, encoding: "utf8" },
  ).trim();
}

function referenceMetadata() {
  const output = execFileSync(
    "bash",
    [prepareScript, "--reference-metadata", "gpui-component"],
    { cwd: repoRoot, encoding: "utf8" },
  ).trim();
  const [url, revision] = output.split("\t");
  if (!url || !/^[0-9a-f]{40}$/u.test(revision ?? "")) {
    throw new Error(`Invalid gpui-component release metadata: ${output}`);
  }
  return { revision, url };
}

function exactPatchedCheckout(checkout) {
  const tooltipDiff = git(checkout, [
    "diff",
    "--no-ext-diff",
    "--binary",
    "--abbrev=7",
    "--",
    "crates/ui/src/tooltip.rs",
  ]);
  const scrollbarDiff = git(checkout, [
    "diff",
    "--no-ext-diff",
    "--binary",
    "--abbrev=7",
    "--",
    "crates/ui/src/menu/popup_menu.rs",
    "crates/ui/src/scroll/scrollbar.rs",
  ]);
  const changedFiles = git(checkout, ["diff", "--name-only", "--no-ext-diff"])
    .split(/\r?\n/u)
    .filter(Boolean)
    .sort();
  const untracked = git(checkout, ["ls-files", "--others", "--exclude-standard"]);

  return (
    tooltipDiff === readFileSync(tooltipPatch, "utf8").trim() &&
    scrollbarDiff === readFileSync(scrollbarPatch, "utf8").trim() &&
    JSON.stringify(changedFiles) === JSON.stringify(expectedChangedFiles) &&
    untracked === ""
  );
}

function applyCheckedPatches(checkout) {
  for (const patch of [tooltipPatch, scrollbarPatch]) {
    git(checkout, ["apply", "--check", patch]);
    git(checkout, ["apply", patch]);
  }
  if (!exactPatchedCheckout(checkout)) {
    throw new Error("Clean gpui-component checkout does not exactly match the checked-in patches");
  }
}

function prepareCleanCheckout({ revision, source, url }) {
  const temporaryRoot = mkdtempSync(join(tmpdir(), "ghostex-reference-contract-"));
  const checkout = join(temporaryRoot, "gpui-component");
  try {
    if (source) {
      execFileSync("git", ["clone", "--shared", "--no-checkout", source, checkout], {
        cwd: repoRoot,
        stdio: "ignore",
      });
    } else {
      execFileSync("git", ["init", checkout], { cwd: repoRoot, stdio: "ignore" });
      git(checkout, ["remote", "add", "origin", url]);
      git(checkout, ["fetch", "--depth=1", "origin", revision]);
    }
    git(checkout, ["checkout", "--detach", revision]);
    applyCheckedPatches(checkout);
    return { checkout, temporaryRoot };
  } catch (error) {
    rmSync(temporaryRoot, { force: true, recursive: true });
    throw error;
  }
}

function verifyContract(checkout, revision) {
  const librarySource = readFileSync(
    join(checkout, "crates/ui/src/tooltip.rs"),
    "utf8",
  );
  const applicationSources = rustSourcesUnder(join(repoRoot, "gpui/src"));
  const { available, missing } = missingManagedTooltipPlacements(
    librarySource,
    applicationSources,
  );

  if (missing.size > 0) {
    const details = [...missing.entries()]
      .map(([placement, paths]) => {
        const locations = [...new Set(paths)]
          .map((path) => relative(repoRoot, path))
          .join(", ");
        return `${placement} (used by ${locations})`;
      })
      .join("; ");
    throw new Error(
      `Ghostex uses managed tooltip placements missing from the clean pinned ` +
        `gpui-component patch: ${details}. Update the checked-in release patch before dispatching.`,
    );
  }

  console.log(
    `Verified clean gpui-component ${revision.slice(0, 12)} contract ` +
      `(${[...available].sort().join(", ")}).`,
  );
}

const metadata = referenceMetadata();
const configuredReference = process.env.GHOSTEX_RELEASE_GPUI_COMPONENT_REFERENCE;
const localReference = resolve(
  configuredReference ?? join(repoRoot, ".dependencies/gpui-component"),
);
let checkout;
let temporaryRoot;

try {
  if (!cleanOnly && existsSync(join(localReference, ".git"))) {
    const head = git(localReference, ["rev-parse", "HEAD"]);
    if (head !== metadata.revision) {
      throw new Error(
        `Local gpui-component is at ${head}, expected ${metadata.revision}. ` +
          `Release builds must use the pinned reference.`,
      );
    }
    const status = git(localReference, ["status", "--porcelain", "--untracked-files=all"]);
    if (status && !exactPatchedCheckout(localReference)) {
      throw new Error(
        `Local gpui-component has changes not represented by the checked-in release patches:\n` +
          `${status}\nUpdate the patches before dispatching so clean CI builds use the same API.`,
      );
    }
    if (status) {
      checkout = localReference;
    } else {
      ({ checkout, temporaryRoot } = prepareCleanCheckout({
        ...metadata,
        source: localReference,
      }));
    }
  } else {
    ({ checkout, temporaryRoot } = prepareCleanCheckout(metadata));
  }

  verifyContract(checkout, metadata.revision);
} finally {
  if (temporaryRoot) rmSync(temporaryRoot, { force: true, recursive: true });
}
