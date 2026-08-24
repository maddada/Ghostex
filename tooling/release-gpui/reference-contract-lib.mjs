import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

export function extractManagedTooltipPlacements(source) {
  const match = source.match(
    /\bpub\s+enum\s+ManagedTooltipPlacement\s*\{(?<body>[\s\S]*?)^\}/mu,
  );
  if (!match?.groups?.body) {
    throw new Error("Pinned gpui-component patch does not define ManagedTooltipPlacement");
  }

  return new Set(
    [...match.groups.body.matchAll(/^\s*(?:#\[[^\]]+\]\s*)*(?<name>[A-Z][A-Za-z0-9_]*)\s*,/gmu)]
      .map(({ groups }) => groups.name),
  );
}

export function extractManagedTooltipPlacementUsages(source) {
  return new Set(
    [...source.matchAll(/\bManagedTooltipPlacement::(?<name>[A-Z][A-Za-z0-9_]*)\b/gu)]
      .map(({ groups }) => groups.name),
  );
}

export function rustSourcesUnder(root) {
  const sources = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "target") visit(path);
      } else if (entry.isFile() && entry.name.endsWith(".rs")) {
        sources.push({ path, source: readFileSync(path, "utf8") });
      }
    }
  };
  visit(root);
  return sources;
}

export function missingManagedTooltipPlacements(librarySource, applicationSources) {
  const available = extractManagedTooltipPlacements(librarySource);
  const missing = new Map();

  for (const { path, source } of applicationSources) {
    for (const placement of extractManagedTooltipPlacementUsages(source)) {
      if (!available.has(placement)) {
        const paths = missing.get(placement) ?? [];
        paths.push(path);
        missing.set(placement, paths);
      }
    }
  }

  return { available, missing };
}

export function extractPublicRustMethods(source) {
  return new Set(
    [...source.matchAll(/^\s*pub\s+fn\s+(?<name>[a-z_][A-Za-z0-9_]*)\s*\(/gmu)]
      .map(({ groups }) => groups.name),
  );
}

export function missingRequiredRustMethods(librarySource, requiredMethods) {
  const available = extractPublicRustMethods(librarySource);
  return {
    available,
    missing: requiredMethods.filter((method) => !available.has(method)),
  };
}
