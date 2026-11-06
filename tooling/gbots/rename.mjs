// Replaces OpenMausBot's product name with GBots in the source the GBots runtime bundles.
// Lines that link to upstream keep the real name so the links still resolve.
// Usage: node tooling/gbots/rename.mjs <OpenMausBot checkout>
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.argv[2];
if (!root) {
  console.error("usage: rename.mjs <checkout>");
  process.exit(2);
}
const FOLDERS = ["server", "shared", "scripts", "src/locales", "electron"];
const EXTENSIONS = [".ts", ".mts", ".mjs", ".js", ".json"];
const SKIP_DIRS = new Set(["node_modules", ".git", "dist", "dist-server"]);
const KEEP_LINE = /github\.com\/milind-soni|openmausbot\.(com|ai|app|dev)|mausbot\.(com|ai|app|dev)/i;
const NAMES = [
  [/OpenMausBot/g, "GBots"],
  [/Open MausBot/g, "GBots"],
  [/\bMausBot\b/g, "GBots"],
];

let files = 0;
let lines = 0;
function walk(dir) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    if (SKIP_DIRS.has(name)) continue;
    const path = join(dir, name);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      walk(path);
    } else if (EXTENSIONS.some((ext) => name.endsWith(ext))) {
      rename(path);
    }
  }
}
function rename(path) {
  const text = readFileSync(path, "utf8");
  if (!/MausBot/.test(text)) return;
  let changed = 0;
  const next = text
    .split("\n")
    .map((line) => {
      if (KEEP_LINE.test(line)) return line;
      let out = line;
      for (const [pattern, replacement] of NAMES) out = out.replace(pattern, replacement);
      if (out !== line) changed += 1;
      return out;
    })
    .join("\n");
  if (changed) {
    writeFileSync(path, next);
    files += 1;
    lines += changed;
  }
}
for (const folder of FOLDERS) walk(join(root, folder));
console.log(`GBots rename: ${lines} lines in ${files} files`);
