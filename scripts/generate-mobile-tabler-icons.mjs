#!/usr/bin/env node
/*
CDXC:MobileSidebarIcons 2026-08-21:
The React Native sessions list needs the same glyphs the gpui sidebar draws
from `@tabler/icons-react`: the thirteen session tag icons that replace a
tagged row's agent icon, and the project identity glyphs a user can pick from
the desktop's icon allowlist. The phone cannot import the React package, so
this converts the raw Tabler SVG sources into `react-native-svg` components,
following the same generated-asset pattern as `agentIcons.generated.tsx`.

Regenerate after changing SIDEBAR_COMMAND_ICON_IDS, the tag catalog, or the
Tabler dependency:

  bun run generate:mobile-tabler-icons
*/

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tablerRoot = path.join(repoRoot, "node_modules", "@tabler", "icons", "icons");
const outFile = path.join(repoRoot, "apps", "mobile", "app", "src", "assets", "tablerIcons.generated.tsx");

/**
 * Session tag glyphs, mirroring SIDEBAR_SESSION_TAG_ICONS in
 * sidebar/session-tag-ui.tsx. Every tag icon is an outline variant there.
 */
const TAG_ICONS = {
  blocked: "barrier-block",
  bug: "bug",
  design: "palette",
  done: "circle-check",
  favorite: "star",
  feature: "puzzle",
  "high-priority": "alert-triangle",
  "in-progress": "player-play",
  "low-priority": "arrow-down",
  "on-hold": "player-pause",
  research: "microscope",
  testing: "test-pipe",
  todo: "checkbox",
};

/**
 * Project identity glyphs, mirroring ICON_COMPONENT_BY_ID in
 * sidebar/sidebar-command-icon.tsx. `filled:` marks the ids the desktop draws
 * from Tabler's filled set rather than the outline set.
 */
const COMMAND_ICONS = {
  api: "api",
  archive: "filled:archive",
  bell: "filled:bell",
  bolt: "filled:bolt",
  book: "filled:book",
  brain: "brain",
  braces: "braces",
  brandDocker: "brand-docker",
  brandGithub: "filled:brand-github",
  brandPython: "brand-python",
  brandReact: "brand-react",
  brandVscode: "brand-vscode",
  bug: "filled:bug",
  chartBar: "chart-bar",
  checklist: "checklist",
  clock: "filled:clock",
  cloud: "filled:cloud",
  code: "code",
  command: "command",
  cpu: "cpu",
  database: "filled:database",
  deviceDesktop: "filled:device-desktop",
  deviceLaptop: "device-laptop",
  download: "filled:download",
  fileCode: "filled:file-code",
  fileDiff: "filled:file-diff",
  fileSearch: "file-search",
  fileText: "filled:file-text",
  flask: "filled:flask",
  folder: "filled:folder",
  folderOpen: "filled:folder-open",
  gitBranch: "git-branch",
  gitCommit: "git-commit",
  gitMerge: "git-merge",
  gitPullRequest: "git-pull-request",
  key: "filled:key",
  layoutDashboard: "filled:layout-dashboard",
  link: "link",
  lock: "filled:lock",
  messageCircle: "filled:message-circle",
  package: "package",
  pencilCode: "pencil-code",
  playerPlay: "filled:player-play",
  refresh: "refresh",
  robot: "robot",
  route: "route",
  rocket: "rocket",
  search: "filled:search",
  server: "server",
  settings: "filled:settings",
  shieldSearch: "shield-search",
  sparkles: "filled:sparkles",
  stack: "filled:stack",
  terminal: "terminal-2",
  testPipe: "test-pipe",
  tool: "tool",
  upload: "upload",
  wand: "wand",
  world: "world",
};

/** Fallbacks the project-icon chain lands on when a project ships no icon. */
const FALLBACK_ICONS = {
  folder: "folder",
  folderOpen: "folder-open",
  worktree: "git-branch",
};

function componentName(prefix, id) {
  const cleaned = id.replace(/[^a-zA-Z0-9]+(.)?/g, (_, next) =>
    next ? next.toUpperCase() : "",
  );
  return `${prefix}${cleaned.charAt(0).toUpperCase()}${cleaned.slice(1)}`;
}

/**
 * One Tabler SVG turned into a react-native-svg body. Tabler's first <path> is
 * the transparent 24×24 hit area, which react-native-svg does not need, and
 * the `filled` variant paints with fill instead of stroke — the two variants
 * are the only shapes in the pack, so the converter handles exactly those.
 */
function convert(spec) {
  const filled = spec.startsWith("filled:");
  const name = filled ? spec.slice("filled:".length) : spec;
  const file = path.join(tablerRoot, filled ? "filled" : "outline", `${name}.svg`);
  const source = fs.readFileSync(file, "utf8");
  const paths = [...source.matchAll(/<path\s([^>]*?)\/>/g)]
    .map((match) => match[1])
    .filter((attributes) => !/stroke="none"[^>]*fill="none"/.test(attributes))
    .map((attributes) => {
      const d = /(?:^|\s)d="([^"]+)"/.exec(attributes)?.[1];
      return d ?? null;
    })
    .filter((d) => d !== null);
  if (paths.length === 0) {
    throw new Error(`Tabler icon ${name} produced no drawable paths.`);
  }
  return { filled, paths };
}

function renderComponent(prefix, id, spec) {
  const { filled, paths } = convert(spec);
  const body = paths
    .map((d) =>
      filled
        ? `      <Path d=${JSON.stringify(d)} fill={color} />`
        : `      <Path d=${JSON.stringify(d)} stroke={color} />`,
    )
    .join("\n");
  const svgProps = filled
    ? `width={size} height={size} viewBox="0 0 24 24" fill={color}`
    : `width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round"`;
  const signature = filled
    ? "{ size, color }: TablerIconProps"
    : "{ size, color, strokeWidth = 1.9 }: TablerIconProps";
  return `function ${componentName(prefix, id)}(${signature}) {
  return (
    <Svg ${svgProps}>
${body}
    </Svg>
  );
}
`;
}

const parts = [];
const tagEntries = [];
for (const [id, spec] of Object.entries(TAG_ICONS)) {
  parts.push(renderComponent("Tag", id, spec));
  tagEntries.push(`  ${JSON.stringify(id)}: ${componentName("Tag", id)},`);
}
const commandEntries = [];
for (const [id, spec] of Object.entries(COMMAND_ICONS)) {
  parts.push(renderComponent("Cmd", id, spec));
  commandEntries.push(`  ${JSON.stringify(id)}: ${componentName("Cmd", id)},`);
}
const fallbackEntries = [];
for (const [id, spec] of Object.entries(FALLBACK_ICONS)) {
  parts.push(renderComponent("Fallback", id, spec));
  fallbackEntries.push(`  ${JSON.stringify(id)}: ${componentName("Fallback", id)},`);
}

const output = `// GENERATED by scripts/generate-mobile-tabler-icons.mjs in the Ghostex main
// repo from @tabler/icons SVG sources — do not edit by hand. These mirror the
// glyphs sidebar/session-tag-ui.tsx and sidebar/sidebar-command-icon.tsx draw
// in the gpui sidebar. Regenerate with \`bun run generate:mobile-tabler-icons\`.
import * as React from 'react';
import Svg, { Path } from 'react-native-svg';

export type TablerIconProps = { size: number; color: string; strokeWidth?: number };
export type TablerIconComponent = (props: TablerIconProps) => React.JSX.Element;

${parts.join("\n")}
/** Session tag glyphs, keyed by the persisted tag value. */
export const TAG_ICONS: Record<string, TablerIconComponent> = {
${tagEntries.join("\n")}
};

/** Project identity glyphs, keyed by SIDEBAR_COMMAND_ICON_IDS. */
export const COMMAND_ICONS: Record<string, TablerIconComponent> = {
${commandEntries.join("\n")}
};

/** Project-icon fallbacks when a project ships no icon of its own. */
export const PROJECT_FALLBACK_ICONS: Record<string, TablerIconComponent> = {
${fallbackEntries.join("\n")}
};
`;

fs.mkdirSync(path.dirname(outFile), { recursive: true });
fs.writeFileSync(outFile, output);
console.log(
  `Wrote ${path.relative(repoRoot, outFile)} (${Object.keys(TAG_ICONS).length} tag, ${Object.keys(COMMAND_ICONS).length} command, ${Object.keys(FALLBACK_ICONS).length} fallback icons).`,
);
