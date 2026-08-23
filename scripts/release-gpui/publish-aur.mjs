#!/usr/bin/env node
/*
 * Vendor-maintained AUR package for Ghostex (`ghostex-bin`).
 *
 * packaging/aur/ghostex-bin/PKGBUILD.template is the single source of truth.
 * This script renders it into a final PKGBUILD, then derives the matching
 * .SRCINFO by parsing that rendered PKGBUILD, so the two can never drift.
 * makepkg --printsrcinfo is not reachable from a macOS workstation or an
 * ubuntu-24.04 runner, which is exactly why the .SRCINFO is generated here
 * instead of being checked in by hand.
 */
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
const packageDirectory = path.join(repoRoot, "packaging/aur/ghostex-bin");
const templatePath = path.join(packageDirectory, "PKGBUILD.template");
const installFileName = "ghostex-bin.install";
/*
 * Arch expects MIT license text to be installed into
 * /usr/share/licenses/$pkgname/, and the release tarball carries none. The
 * repository's own LICENSE is shipped as a local `source` entry instead of being
 * duplicated under packaging/, so there is only ever one copy to keep current.
 */
const licenseSourcePath = path.join(repoRoot, "LICENSE");
// Every file the AUR repository holds. `.SRCINFO` and `PKGBUILD` are rendered;
// the rest are copied verbatim.
const aurRepositoryFiles = ["PKGBUILD", ".SRCINFO", installFileName, "LICENSE"];
const aurRemote = "ssh://aur@aur.archlinux.org/ghostex-bin.git";
const defaultRepo = "maddada/Ghostex";
const sha256Pattern = /^[0-9a-f]{64}$/u;

/*
 * makepkg --printsrcinfo emits pkgbase attributes in a fixed order, indented
 * with a single tab. These two lists mirror srcinfo_write_global() in pacman's
 * scripts/libmakepkg/srcinfo.sh.in: every plain array first, then all
 * architecture-suffixed arrays grouped per architecture. Anything outside them
 * is something the template is not allowed to declare, and renderSrcinfo()
 * refuses it rather than silently dropping it from the .SRCINFO.
 */
const hashFields = ["cksums", "md5sums", "sha1sums", "sha224sums", "sha256sums", "sha384sums", "sha512sums", "b2sums"];
const srcinfoFieldOrder = [
  "pkgdesc",
  "pkgver",
  "pkgrel",
  "epoch",
  "url",
  "install",
  "changelog",
  "arch",
  "groups",
  "license",
  "checkdepends",
  "makedepends",
  "depends",
  "optdepends",
  "provides",
  "conflicts",
  "replaces",
  "noextract",
  "options",
  "backup",
  "source",
  "validpgpkeys",
  ...hashFields,
];
const srcinfoArchFieldOrder = [
  "source",
  "provides",
  "conflicts",
  "depends",
  "replaces",
  "optdepends",
  "makedepends",
  "checkdepends",
  ...hashFields,
];
// Declared by the template for makepkg's benefit, but never part of a .SRCINFO.
const pkgbuildOnlyFields = new Set(["pkgname"]);

export function assertVersion(version) {
  if (!/^\d+\.\d+\.\d+$/u.test(version ?? "")) {
    throw new Error(`--version must be MAJOR.MINOR.PATCH, got ${version || "nothing"}`);
  }
  return version;
}

export function assertSha256(value, label) {
  if (!sha256Pattern.test(value ?? "")) {
    throw new Error(`${label} must be a 64-character lowercase sha256, got ${value || "nothing"}`);
  }
  return value;
}

export function releaseAssetName(version) {
  return `ghostex-${assertVersion(version)}-linux-x64.tar.zst`;
}

export function releaseAssetUrl(version, repo = defaultRepo) {
  return `https://github.com/${repo}/releases/download/v${version}/${releaseAssetName(version)}`;
}

export function renderPkgbuild(template, { licenseSha256, sha256, version }) {
  assertVersion(version);
  assertSha256(sha256, "sha256");
  assertSha256(licenseSha256, "licenseSha256");
  const rendered = template
    .replaceAll("@PKGVER@", version)
    .replaceAll("@SHA256@", sha256)
    .replaceAll("@LICENSE_SHA256@", licenseSha256);
  const leftover = /@[A-Z0-9_]+@/u.exec(rendered);
  if (leftover) throw new Error(`PKGBUILD.template has an unrendered placeholder: ${leftover[0]}`);
  return rendered;
}

/*
 * A deliberately small bash reader: the template is ours, so it only has to
 * understand `key=value`, `key=("a" "b")`, and multi-line array literals, and
 * it stops at the first shell function. Anything it cannot read is an error,
 * never a silently skipped field.
 */
export function parsePkgbuild(text) {
  const fields = new Map();
  const lines = text.split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^[A-Za-z_][A-Za-z0-9_]*\s*\(\)\s*\{/u.test(line)) break;
    const match = /^([A-Za-z_][A-Za-z0-9_]*)=(.*)$/u.exec(line);
    if (!match) continue;
    const [, key] = match;
    let value = match[2];
    if (value.startsWith("(") && !value.trimEnd().endsWith(")")) {
      while (!value.trimEnd().endsWith(")")) {
        index += 1;
        if (index >= lines.length) throw new Error(`Unterminated array literal for ${key} in PKGBUILD`);
        value += `\n${lines[index]}`;
      }
    }
    fields.set(key, value.trimEnd().startsWith("(") ? parseArrayLiteral(key, value.trim()) : [unquote(value.trim())]);
  }
  if (!fields.has("pkgname")) throw new Error("PKGBUILD declares no pkgname");
  return fields;
}

function parseArrayLiteral(key, literal) {
  const body = literal.slice(1, literal.lastIndexOf(")"));
  const values = [];
  const token = /'([^']*)'|"([^"]*)"|(\S+)/gu;
  for (const stripped of body.split("\n").map((line) => line.replace(/#.*$/u, ""))) {
    let element = token.exec(stripped);
    while (element) {
      values.push(element[1] ?? element[2] ?? element[3]);
      element = token.exec(stripped);
    }
    token.lastIndex = 0;
  }
  if (values.length === 0) throw new Error(`Array ${key} in PKGBUILD is empty`);
  return values;
}

function unquote(value) {
  const quoted = /^(?:"([^"]*)"|'([^']*)')\s*(?:#.*)?$/u.exec(value.trim());
  if (quoted) return quoted[1] ?? quoted[2];
  return value.replace(/\s+#.*$/u, "").trim();
}

export function renderSrcinfo(pkgbuild) {
  const fields = parsePkgbuild(pkgbuild);
  const pkgname = fields.get("pkgname")[0];
  const architectures = (fields.get("arch") ?? []).filter((value) => value !== "any");
  const emitted = new Set();
  const lines = [`pkgbase = ${pkgname}`];
  // `${pkgver}` in a source= line is literal in the PKGBUILD but expanded in the
  // .SRCINFO, which is what the AUR web frontend and the RPC read.
  const emit = (key) => {
    for (const value of fields.get(key) ?? []) lines.push(`\t${key} = ${expand(value, fields)}`);
    emitted.add(key);
  };
  for (const field of srcinfoFieldOrder) emit(field);
  for (const architecture of architectures) {
    for (const field of srcinfoArchFieldOrder) emit(`${field}_${architecture}`);
  }

  for (const key of fields.keys()) {
    if (pkgbuildOnlyFields.has(key) || key.startsWith("_") || emitted.has(key)) continue;
    throw new Error(
      `PKGBUILD declares ${key}, which publish-aur.mjs does not know how to put in a .SRCINFO. ` +
        "Add it to srcinfoFieldOrder/srcinfoArchFieldOrder, or drop it from PKGBUILD.template.",
    );
  }
  lines.push("", `pkgname = ${pkgname}`, "");
  return lines.join("\n");
}

function expand(value, fields) {
  return value.replaceAll(/\$\{?(pkgver|pkgrel|pkgname|epoch)\}?/gu, (whole, name) => {
    const replacement = fields.get(name)?.[0];
    if (replacement === undefined) throw new Error(`PKGBUILD references $${name} before defining it`);
    return expand(replacement, fields);
  });
}

export function sha256FromManifest(manifest, version) {
  const assetName = releaseAssetName(version);
  const artifact = (manifest?.artifacts ?? []).find((entry) => entry.name === assetName);
  if (!artifact) {
    const seen = (manifest?.artifacts ?? []).map((entry) => entry.name).join(", ") || "nothing";
    throw new Error(`Manifest records no ${assetName} (it records ${seen})`);
  }
  return assertSha256(artifact.sha256, `manifest sha256 for ${assetName}`);
}

function runGit(args, options = {}) {
  const result = spawnSync("git", args, { cwd: options.cwd, encoding: "utf8" });
  if (result.status !== 0 && !options.allowFailure) {
    throw new Error(`git ${args.join(" ")} failed: ${(result.stderr || result.stdout || "unknown error").trim()}`);
  }
  return result;
}

function sha256FromGitHubMetadata(version, repo) {
  const result = spawnSync(
    "gh",
    ["release", "view", `v${version}`, "--repo", repo, "--json", "assets,isDraft"],
    { encoding: "utf8" },
  );
  if (result.status !== 0) return null;
  const release = JSON.parse(result.stdout);
  if (release.isDraft) throw new Error(`v${version} is still a draft; refusing to publish it to the AUR`);
  const asset = (release.assets ?? []).find((entry) => entry.name === releaseAssetName(version));
  if (!asset) {
    throw new Error(`v${version} carries no ${releaseAssetName(version)}; refusing to publish it to the AUR`);
  }
  const digest = typeof asset.digest === "string" && asset.digest.startsWith("sha256:")
    ? asset.digest.slice("sha256:".length)
    : "";
  return sha256Pattern.test(digest) ? digest : null;
}

async function sha256FromDownload(version, repo) {
  const url = releaseAssetUrl(version, repo);
  process.stdout.write(`Hashing ${url}\n`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`Could not download ${url}: HTTP ${response.status} ${response.statusText}`);
  }
  const hash = createHash("sha256");
  for await (const chunk of response.body) hash.update(chunk);
  return hash.digest("hex");
}

export async function resolveSha256({ manifest, repo, sha256, version }) {
  if (sha256) return { source: "--sha256", value: assertSha256(sha256.toLowerCase(), "--sha256") };
  if (manifest) {
    if (!existsSync(manifest)) {
      throw new Error(`--manifest ${manifest} does not exist`);
    }
    return { source: manifest, value: sha256FromManifest(JSON.parse(readFileSync(manifest, "utf8")), version) };
  }
  const fromGitHub = sha256FromGitHubMetadata(version, repo);
  if (fromGitHub) return { source: "GitHub release asset digest", value: fromGitHub };
  return { source: "downloaded release asset", value: await sha256FromDownload(version, repo) };
}

function publish({ outputDirectory, version }) {
  if (!process.env.SSH_AUTH_SOCK && !process.env.GIT_SSH_COMMAND && !hasSshIdentityFile()) {
    throw new Error(
      "No SSH identity is configured for aur.archlinux.org. In CI, install the AUR_SSH_PRIVATE_KEY secret into " +
        "~/.ssh and add aur.archlinux.org to ~/.ssh/known_hosts before running with --publish. Locally, make sure " +
        "the SSH key registered on your AUR account is loaded (ssh-add) or configured in ~/.ssh/config.",
    );
  }
  const checkout = mkdtempSync(path.join(os.tmpdir(), `ghostex-${version}-aur-`));
  const clone = runGit(["clone", "--depth", "1", aurRemote, checkout], { allowFailure: true });
  if (clone.status !== 0) {
    throw new Error(
      `Could not clone ${aurRemote}: ${(clone.stderr || clone.stdout || "unknown error").trim()}\n` +
        "The ghostex-bin AUR repository must already exist and the SSH key in use must be registered on the AUR " +
        "account that owns it. See packaging/aur/ghostex-bin/README.md for the one-time setup.",
    );
  }
  for (const name of aurRepositoryFiles) {
    copyFileSync(path.join(outputDirectory, name), path.join(checkout, name));
  }
  runGit(["add", ...aurRepositoryFiles], { cwd: checkout });
  if (!runGit(["status", "--porcelain"], { cwd: checkout }).stdout.trim()) {
    process.stdout.write(`AUR ghostex-bin is already at ${version} with this checksum; nothing to push.\n`);
    return;
  }
  runGit(
    [
      "-c",
      "user.name=Ghostex Release Bot",
      "-c",
      "user.email=support@ghostex.app",
      "commit",
      "-m",
      `Update to ${version}`,
    ],
    { cwd: checkout },
  );
  runGit(["push", "origin", "HEAD:master"], { cwd: checkout });
  process.stdout.write(`Pushed ghostex-bin ${version} to the AUR.\n`);
}

function hasSshIdentityFile() {
  const sshDirectory = path.join(os.homedir(), ".ssh");
  if (!existsSync(sshDirectory)) return false;
  return readdirSync(sshDirectory).some((name) => name.startsWith("id_") && !name.endsWith(".pub"));
}

export function parseArguments(argv) {
  const options = { publish: false, repo: defaultRepo };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--publish") {
      options.publish = true;
      continue;
    }
    if (!argument.startsWith("--")) throw new Error(`Unexpected argument: ${argument}`);
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`Missing value for ${argument}`);
    options[argument.slice(2)] = value;
    index += 1;
  }
  return options;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const version = assertVersion(options.version);
  const outputDirectory = path.resolve(options.out ?? `build/release-gpui/${version}/aur`);
  const { source, value: sha256 } = await resolveSha256({
    manifest: options.manifest,
    repo: options.repo,
    sha256: options.sha256,
    version,
  });
  process.stdout.write(`${releaseAssetName(version)} sha256=${sha256} (from ${source})\n`);

  const license = readFileSync(licenseSourcePath);
  const licenseSha256 = createHash("sha256").update(license).digest("hex");
  const pkgbuild = renderPkgbuild(readFileSync(templatePath, "utf8"), { licenseSha256, sha256, version });
  const srcinfo = renderSrcinfo(pkgbuild);
  mkdirSync(outputDirectory, { recursive: true });
  writeFileSync(path.join(outputDirectory, "PKGBUILD"), pkgbuild);
  writeFileSync(path.join(outputDirectory, ".SRCINFO"), srcinfo);
  writeFileSync(path.join(outputDirectory, "LICENSE"), license);
  copyFileSync(path.join(packageDirectory, installFileName), path.join(outputDirectory, installFileName));
  process.stdout.write(`Rendered ${aurRepositoryFiles.join(", ")} into ${outputDirectory}\n`);

  if (options.publish) publish({ outputDirectory, version });
  else process.stdout.write("Pass --publish to push the rendered package to the AUR.\n");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
