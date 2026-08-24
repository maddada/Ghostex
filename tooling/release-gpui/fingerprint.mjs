/*
 * CDXC:ReleaseChangeAwarePlanning 2026-08-13:
 * Content fingerprints for release products, computed from the git index at the
 * source commit. Deterministic across runners: it never looks at the working
 * tree, build output, timestamps, or the network, and gitlinks contribute their
 * submodule commit so pinned submodules are covered without a checkout.
 *
 * Any change to the algorithm, the serialization, or a product's declared inputs
 * MUST bump FINGERPRINT_ALGORITHM_REVISION. Provenance records carrying a
 * different revision are ignored by the planner, which downgrades to `build`
 * rather than comparing incomparable digests.
 */

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  NODES,
  nodeIdsInDependencyOrder,
  nodePathspecs,
  nodeValues,
  nodeDefinition,
} from "./product-inputs.mjs";

/* fp4 (2026-08-23): added the linux-tar-x64 product to the input map. */
export const FINGERPRINT_ALGORITHM_REVISION = "fp4";

const EXCLUDE_PREFIX = ":(exclude)";
const PROJECTIONS = new Set(["package-json"]);

function sha256Hex(update) {
  const digest = createHash("sha256");
  update(digest);
  return digest.digest("hex");
}

/*
 * Pathspecs are literal path prefixes. A trailing `/**` is decorative sugar for
 * "this path and everything under it", `:(exclude)` marks a negative, and no
 * other glob syntax is accepted: a silently non-matching glob would under-declare
 * inputs, which is the one failure this whole design must not have.
 */
export function normalizePathspec(pathspec) {
  const negative = pathspec.startsWith(EXCLUDE_PREFIX);
  let prefix = negative ? pathspec.slice(EXCLUDE_PREFIX.length) : pathspec;
  prefix = prefix.replace(/\/\*\*$/u, "").replace(/\/$/u, "");
  if (prefix.length === 0) throw new Error(`Empty pathspec: ${JSON.stringify(pathspec)}`);
  if (prefix.includes("*") || prefix.includes("?") || prefix.includes("[")) {
    throw new Error(`Unsupported glob in pathspec ${JSON.stringify(pathspec)}; use "dir/**" or an exact path`);
  }
  return { negative, prefix };
}

function matchesPrefix(entryPath, prefix) {
  return entryPath === prefix || entryPath.startsWith(`${prefix}/`);
}

/* Parse `git ls-tree -r --full-tree -z <sha>` output into sorted entries. */
export function parseTreeEntries(output) {
  const entries = [];
  for (const record of output.split("\0")) {
    if (!record) continue;
    const tabIndex = record.indexOf("\t");
    if (tabIndex < 0) throw new Error(`Malformed git ls-tree record: ${JSON.stringify(record)}`);
    const [mode, type, objectId] = record.slice(0, tabIndex).split(/\s+/u);
    const entryPath = record.slice(tabIndex + 1);
    if (!mode || !type || !objectId || !entryPath) {
      throw new Error(`Malformed git ls-tree record: ${JSON.stringify(record)}`);
    }
    entries.push({ mode, objectId, path: entryPath, type });
  }
  entries.sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));
  return entries;
}

export function createGitTreeReader({ repoRoot = process.cwd(), run = spawnSync } = {}) {
  const treeCache = new Map();
  const gitArgs = (args) => ["-C", repoRoot, ...args];
  const capture = (args, { encoding = "utf8", maxBuffer = 256 * 1024 * 1024 } = {}) => {
    const result = run("git", gitArgs(args), { encoding, maxBuffer });
    if (result.error) throw result.error;
    if (result.status !== 0) {
      const detail = (result.stderr ?? "").toString().trim();
      throw new Error(`git ${args.join(" ")} failed${detail ? `: ${detail}` : ""}`);
    }
    return result.stdout;
  };
  return {
    isAncestor(ancestor, descendant) {
      const result = run("git", gitArgs(["merge-base", "--is-ancestor", ancestor, descendant]), { encoding: "utf8" });
      if (result.error) throw result.error;
      return result.status === 0;
    },
    listTree(sourceSha) {
      if (!treeCache.has(sourceSha)) {
        treeCache.set(sourceSha, parseTreeEntries(capture(["ls-tree", "-r", "--full-tree", "-z", sourceSha])));
      }
      return treeCache.get(sourceSha);
    },
    readObject(objectId) {
      return Buffer.from(capture(["cat-file", "blob", objectId], { encoding: "buffer" }));
    },
    resolve(revision) {
      return capture(["rev-parse", revision]).trim();
    },
  };
}

/*
 * §4.11 rule 8: hash package.json as a normalized projection with `version`
 * removed, so bumping the marketing version alone cannot invalidate products
 * that are not version-stamped.
 */
export function projectPackageJson(contents) {
  const parsed = JSON.parse(contents.toString("utf8"));
  const projected = {
    dependencies: parsed.dependencies ?? {},
    devDependencies: parsed.devDependencies ?? {},
    packageManager: parsed.packageManager ?? null,
    scripts: parsed.scripts ?? {},
    workspaces: parsed.workspaces ?? null,
  };
  return canonicalJson(projected);
}

export function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value) ?? "null";
  if (Array.isArray(value)) return `[${value.map((item) => canonicalJson(item)).join(",")}]`;
  const keys = Object.keys(value).sort();
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function entryIdentity(entry, projection, readObject) {
  if (!projection) return entry.objectId;
  if (!PROJECTIONS.has(projection)) throw new Error(`Unknown fingerprint projection: ${projection}`);
  if (entry.type !== "blob") throw new Error(`Projection ${projection} requires a blob at ${entry.path}`);
  const contents = readObject(entry.objectId);
  const normalized = projection === "package-json" ? projectPackageJson(contents) : contents.toString("utf8");
  return `proj:${projection}:${sha256Hex((digest) => digest.update(normalized))}`;
}

function writeTreeRecord(digest, entry, identity) {
  digest.update(`tree\0${entry.path}\0${entry.mode}\0${entry.type}\0${identity}\0`);
}

export function treeDigest(records) {
  return sha256Hex((digest) => {
    for (const record of records) writeTreeRecord(digest, record.entry, record.identity);
  });
}

/*
 * Resolve one node's declared pathspecs against the tree, applying every
 * negative pathspec of that node to every positive one.
 */
export function resolvePathspecs({ entries, pathspecs, readObject }) {
  const negatives = [];
  const positives = [];
  for (const declaration of pathspecs) {
    const { negative, prefix } = normalizePathspec(declaration.pathspec);
    if (negative) {
      if (declaration.projection) throw new Error(`Exclusion cannot carry a projection: ${declaration.pathspec}`);
      negatives.push({ declaration, prefix });
    } else {
      positives.push({ declaration, prefix });
    }
  }
  const excluded = (entryPath) => negatives.some((negative) => matchesPrefix(entryPath, negative.prefix));

  const identityCache = new Map();
  const identityFor = (entry, projection) => {
    const key = `${entry.path}\0${projection ?? ""}`;
    if (!identityCache.has(key)) identityCache.set(key, entryIdentity(entry, projection, readObject));
    return identityCache.get(key);
  };

  const byPathspec = [];
  const union = new Map();
  for (const positive of positives) {
    const matched = [];
    for (const entry of entries) {
      if (!matchesPrefix(entry.path, positive.prefix) || excluded(entry.path)) continue;
      const identity = identityFor(entry, positive.declaration.projection);
      matched.push({ entry, identity });
      union.set(`${entry.path}\0${identity}`, { entry, identity });
    }
    byPathspec.push({
      declaration: positive.declaration,
      digest: treeDigest(matched),
      entryCount: matched.length,
      pathspec: positive.declaration.pathspec,
      records: matched,
    });
  }
  const unionRecords = [...union.values()].sort((left, right) =>
    left.entry.path < right.entry.path ? -1 : left.entry.path > right.entry.path ? 1 : 0,
  );
  return { byPathspec, negatives, unionRecords };
}

/*
 * The fingerprint serialization (§3.4):
 *   fp\0<revision>\0<nodeId>\0
 *   tree\0<path>\0<mode>\0<type>\0<objectId>\0   for the sorted union
 *   path\0<pathspec>\0<subDigest>\0<count>\0     for each declared pathspec
 *   exclude\0<pathspec>\0                        for each declared exclusion
 *   value\0<key>\0<value>\0                      sorted by key
 *   composed\0<key>\0<childFingerprint>\0        sorted by key
 *   version\0<releaseVersion|(none)>\0
 */
export function computeNodeFingerprint({
  composed = {},
  entries,
  nodeId,
  pathspecs,
  readObject,
  releaseVersion,
  values = {},
  versionStamped = false,
}) {
  const resolved = resolvePathspecs({ entries, pathspecs, readObject });
  const fingerprint = sha256Hex((digest) => {
    digest.update(`fp\0${FINGERPRINT_ALGORITHM_REVISION}\0${nodeId}\0`);
    for (const record of resolved.unionRecords) writeTreeRecord(digest, record.entry, record.identity);
    for (const pathspec of resolved.byPathspec) {
      digest.update(`path\0${pathspec.pathspec}\0${pathspec.digest}\0${pathspec.entryCount}\0`);
    }
    for (const negative of resolved.negatives) digest.update(`exclude\0${negative.declaration.pathspec}\0`);
    for (const key of Object.keys(values).sort()) digest.update(`value\0${key}\0${values[key]}\0`);
    for (const key of Object.keys(composed).sort()) digest.update(`composed\0${key}\0${composed[key]}\0`);
    digest.update(`version\0${versionStamped ? releaseVersion : "(none)"}\0`);
  });
  return {
    fingerprint,
    inputs: {
      composed: { ...composed },
      paths: resolved.byPathspec.map((pathspec) => ({
        digest: pathspec.digest,
        entryCount: pathspec.entryCount,
        pathspec: pathspec.pathspec,
      })),
      values: { ...values },
    },
  };
}

/*
 * Fingerprint every requested node plus everything it composes, children first.
 * `context` is `{ version, scope }`; `entries` is one `git ls-tree` of the source
 * commit, shared by every node.
 */
export function computeFingerprints({ context, entries, ids = Object.keys(NODES), readObject }) {
  const results = new Map();
  for (const nodeId of nodeIdsInDependencyOrder(ids)) {
    const definition = nodeDefinition(nodeId);
    const composed = {};
    for (const child of definition.composedFrom ?? []) composed[child] = results.get(child).fingerprint;
    const computed = computeNodeFingerprint({
      composed,
      entries,
      nodeId,
      pathspecs: nodePathspecs(nodeId),
      readObject,
      releaseVersion: context.version,
      values: nodeValues(nodeId, context),
      versionStamped: Boolean(definition.versionStamped),
    });
    results.set(nodeId, {
      ...computed,
      kind: definition.kind,
      nodeId,
      versionStamped: Boolean(definition.versionStamped),
    });
  }
  return results;
}

/*
 * The code-server component identity revision inputs (§4.2) change the produced
 * archive without changing the upstream payload, so their digest is recorded
 * separately and a mismatch blocks component reuse.
 */
export function identityRevisionInputsDigest({ entries, nodeId, readObject }) {
  const pathspecs = nodeDefinition(nodeId).identityRevisionPathspecs;
  if (!pathspecs) return null;
  return treeDigest(resolvePathspecs({ entries, pathspecs, readObject }).unionRecords);
}

/* Human-readable reason strings: which declared inputs moved between two records. */
export function explainFingerprintDifference(current, baseline) {
  const changedPaths = [];
  const changedValues = [];
  const changedComposed = [];
  const baselinePaths = new Map((baseline?.paths ?? []).map((entry) => [entry.pathspec, entry]));
  for (const entry of current?.paths ?? []) {
    const previous = baselinePaths.get(entry.pathspec);
    if (!previous) {
      changedPaths.push(entry.pathspec);
      continue;
    }
    if (previous.digest !== entry.digest) changedPaths.push(entry.pathspec);
    baselinePaths.delete(entry.pathspec);
  }
  for (const pathspec of baselinePaths.keys()) changedPaths.push(pathspec);

  const currentValues = current?.values ?? {};
  const baselineValues = baseline?.values ?? {};
  for (const key of new Set([...Object.keys(currentValues), ...Object.keys(baselineValues)])) {
    if (currentValues[key] !== baselineValues[key]) changedValues.push(key);
  }
  const currentComposed = current?.composed ?? {};
  const baselineComposed = baseline?.composed ?? {};
  for (const key of new Set([...Object.keys(currentComposed), ...Object.keys(baselineComposed)])) {
    if (currentComposed[key] !== baselineComposed[key]) changedComposed.push(key);
  }
  return {
    composed: changedComposed.sort(),
    paths: changedPaths.sort(),
    values: changedValues.sort(),
  };
}

export function describeFingerprintDifference(difference) {
  const parts = [];
  if (difference.paths.length > 0) parts.push(difference.paths.join(", "));
  if (difference.values.length > 0) parts.push(`values ${difference.values.join(", ")}`);
  if (difference.composed.length > 0) parts.push(`embedded ${difference.composed.join(", ")}`);
  return parts.join("; ");
}

export function shortFingerprint(fingerprint) {
  return typeof fingerprint === "string" ? fingerprint.slice(0, 12) : "";
}
