#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const repo = "maddada/Ghostex";

function run(command, args, options = {}) {
  const output = execFileSync(command, args, {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    stdio: options.capture === false ? "inherit" : "pipe",
  });
  return typeof output === "string" ? output.trim() : "";
}

function sleep(milliseconds) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

function usage() {
  return `
Usage:
  node scripts/release-gpui-actions.mjs start <version> [options]
  node scripts/release-gpui-actions.mjs publish <version> --source-run-id <id> [options]

Scope options (all are enabled by default):
  --only-macos
  --skip-macos
  --skip-linux | --skip-linux-deb | --skip-linux-rpm
  --skip-windows | --skip-windows-x64 | --skip-windows-arm64
  --skip-android
  --skip-gxserver-linux-x64 | --skip-gxserver-linux-arm64
  --skip-gxserver-wsl | --skip-gxserver-wsl-x64 | --skip-gxserver-wsl-arm64

Release options:
  --skip-sparkle
  --prerelease
  --windows-signing <auto|required|off>  Default: auto
  --source-run-id <id>                   Required by publish
  --dry-run
`.trim();
}

function parseArgs(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log(usage());
    process.exit(0);
  }
  const [command = "start", version, ...rest] = argv;
  if (!["start", "publish"].includes(command)) throw new Error(`Unknown command: ${command}`);
  if (!/^\d+\.\d+\.\d+$/u.test(version ?? "")) throw new Error("Pass a MAJOR.MINOR.PATCH version");
  const options = {
    android: true,
    dryRun: false,
    gxserverLinuxArm64: true,
    gxserverLinuxX64: true,
    gxserverWslWindowsArm64: true,
    gxserverWslWindowsX64: true,
    linuxDeb: true,
    linuxRpm: true,
    macos: true,
    prerelease: false,
    sourceRunId: "",
    updateSparkle: true,
    windowsArm64: true,
    windowsSigning: "auto",
    windowsX64: true,
  };
  for (let index = 0; index < rest.length; index += 1) {
    const arg = rest[index];
    if (arg === "--only-macos") {
      Object.assign(options, {
        android: false,
        gxserverLinuxArm64: true,
        gxserverLinuxX64: true,
        gxserverWslWindowsArm64: false,
        gxserverWslWindowsX64: false,
        linuxDeb: false,
        linuxRpm: false,
        macos: true,
        windowsArm64: false,
        windowsX64: false,
      });
    } else if (arg === "--skip-macos") options.macos = false;
    else if (arg === "--skip-linux") options.linuxDeb = options.linuxRpm = false;
    else if (arg === "--skip-linux-deb") options.linuxDeb = false;
    else if (arg === "--skip-linux-rpm") options.linuxRpm = false;
    else if (arg === "--skip-windows") options.windowsX64 = options.windowsArm64 = false;
    else if (arg === "--skip-windows-x64") options.windowsX64 = false;
    else if (arg === "--skip-windows-arm64") options.windowsArm64 = false;
    else if (arg === "--skip-android") options.android = false;
    else if (arg === "--skip-gxserver-linux-x64") options.gxserverLinuxX64 = false;
    else if (arg === "--skip-gxserver-linux-arm64") options.gxserverLinuxArm64 = false;
    else if (arg === "--skip-gxserver-wsl") {
      options.gxserverWslWindowsX64 = false;
      options.gxserverWslWindowsArm64 = false;
    } else if (arg === "--skip-gxserver-wsl-x64") options.gxserverWslWindowsX64 = false;
    else if (arg === "--skip-gxserver-wsl-arm64") options.gxserverWslWindowsArm64 = false;
    else if (arg === "--skip-sparkle") options.updateSparkle = false;
    else if (arg === "--prerelease") options.prerelease = true;
    else if (arg === "--dry-run") options.dryRun = true;
    else if (arg === "--windows-signing") {
      options.windowsSigning = rest[index + 1] ?? "";
      index += 1;
    } else if (arg === "--source-run-id") {
      options.sourceRunId = rest[index + 1] ?? "";
      index += 1;
    } else {
      throw new Error(`Unknown option: ${arg}`);
    }
  }
  if (!["auto", "required", "off"].includes(options.windowsSigning)) {
    throw new Error("--windows-signing must be auto, required, or off");
  }
  if (command === "publish" && !/^\d+$/u.test(options.sourceRunId)) {
    throw new Error("publish requires --source-run-id <GitHub Actions run id>");
  }
  return { command, options, version };
}

function validateScope(options) {
  const enabled = [
    options.macos,
    options.linuxDeb,
    options.linuxRpm,
    options.windowsX64,
    options.windowsArm64,
    options.android,
    options.gxserverLinuxX64,
    options.gxserverLinuxArm64,
    options.gxserverWslWindowsX64,
    options.gxserverWslWindowsArm64,
  ];
  if (!enabled.some(Boolean)) throw new Error("At least one platform must be enabled");
  if (options.updateSparkle && !options.macos) throw new Error("--skip-macos requires --skip-sparkle");
  if (options.prerelease && options.updateSparkle) {
    throw new Error("A prerelease requires --skip-sparkle");
  }
  if (options.macos && (!options.gxserverLinuxX64 || !options.gxserverLinuxArm64)) {
    throw new Error("macOS requires both gxserver Linux runtimes");
  }
  if (
    (options.linuxDeb ||
      options.linuxRpm ||
      options.windowsX64 ||
      options.gxserverWslWindowsX64) &&
    !options.gxserverLinuxX64
  ) {
    throw new Error("Enabled x64 packages require gxserver Linux x64");
  }
  if (
    (options.windowsArm64 || options.gxserverWslWindowsArm64) &&
    !options.gxserverLinuxArm64
  ) {
    throw new Error("Enabled ARM64 packages require gxserver Linux ARM64");
  }
}

function requiresGpuiReferenceContract(options) {
  return (
    options.macos ||
    options.linuxDeb ||
    options.linuxRpm ||
    options.windowsX64 ||
    options.windowsArm64
  );
}

function expectedPlatforms(options) {
  return [
    options.macos && "macos-arm64",
    options.linuxDeb && "linux-deb-x64",
    options.linuxRpm && "linux-rpm-x64",
    options.windowsX64 && "windows-x64",
    options.windowsArm64 && "windows-arm64",
    options.android && "android",
    options.gxserverLinuxX64 && "gxserver-linux-x64",
    options.gxserverLinuxArm64 && "gxserver-linux-arm64",
    options.gxserverWslWindowsX64 && "gxserver-wsl-windows-x64",
    options.gxserverWslWindowsArm64 && "gxserver-wsl-windows-arm64",
  ].filter(Boolean);
}

function validateLocalSource(version, { allowExistingTag }) {
  run("gh", ["auth", "status"], { capture: false });
  const branch = run("git", ["branch", "--show-current"]);
  if (branch !== "main") throw new Error(`Release source must be main, got ${branch}`);
  const status = run("git", ["status", "--porcelain", "--untracked-files=all"]);
  if (status) throw new Error(`Release source is dirty:\n${status}`);
  run("git", ["fetch", "origin", "main", "--tags"], { capture: false });
  const head = run("git", ["rev-parse", "HEAD"]);
  const remoteMain = run("git", ["rev-parse", "origin/main"]);
  if (head !== remoteMain) throw new Error(`Local main ${head} differs from origin/main ${remoteMain}`);
  const packageVersion = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;
  if (packageVersion !== version) throw new Error(`package.json is ${packageVersion}; expected ${version}`);
  const changelog = readFileSync(new URL("../CHANGELOG.md", import.meta.url), "utf8");
  if (!changelog.includes(`## ${version} -`)) throw new Error(`CHANGELOG.md has no ${version} section`);
  const tag = run("git", ["ls-remote", "--tags", "origin", `refs/tags/v${version}`]);
  if (tag && !allowExistingTag) throw new Error(`v${version} already exists`);
  return head;
}

function configuredSecrets() {
  const attempts = 3;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      const json = run("gh", ["secret", "list", "--repo", repo, "--json", "name"]);
      return new Set(json ? JSON.parse(json).map(({ name }) => name) : []);
    } catch (error) {
      if (attempt === attempts) throw error;
      console.warn(`GitHub secret inventory failed (attempt ${attempt}/${attempts}); retrying...`);
      sleep(attempt * 1500);
    }
  }
  throw new Error("Unable to read GitHub repository secrets");
}

function requireSecrets(secrets, label, names) {
  const missing = names.filter((name) => !secrets.has(name));
  if (missing.length > 0) throw new Error(`${label} requires repository secrets: ${missing.join(", ")}`);
}

function resolveWindowsSigning(options, secrets) {
  if (!options.windowsX64 && !options.windowsArm64) return false;
  const names = ["WINDOWS_CODE_SIGN_PFX_BASE64", "WINDOWS_CODE_SIGN_PFX_PASSWORD"];
  const available = names.every((name) => secrets.has(name));
  if (options.windowsSigning === "required" && !available) {
    requireSecrets(secrets, "Windows signing", names);
  }
  if (options.windowsSigning === "off") return false;
  return available;
}

function validateRequiredSecrets(options, secrets) {
  if (options.macos) {
    requireSecrets(secrets, "macOS signing", [
      "APPLE_DEVELOPER_ID_P12_BASE64",
      "APPLE_DEVELOPER_ID_P12_PASSWORD",
      "APPLE_KEYCHAIN_PASSWORD",
    ]);
    const notaryKey = ["APPLE_NOTARY_KEY_BASE64", "APPLE_NOTARY_KEY_ID", "APPLE_NOTARY_ISSUER_ID"];
    const notaryAppleId = ["APPLE_NOTARY_APPLE_ID", "APPLE_NOTARY_TEAM_ID", "APPLE_NOTARY_APP_PASSWORD"];
    if (!notaryKey.every((name) => secrets.has(name)) && !notaryAppleId.every((name) => secrets.has(name))) {
      throw new Error("macOS notarization secrets are incomplete");
    }
  }
  if (options.updateSparkle) requireSecrets(secrets, "Sparkle", ["SPARKLE_PRIVATE_KEY"]);
  if (options.android) {
    requireSecrets(secrets, "Android signing", [
      "ANDROID_RELEASE_KEYSTORE_BASE64",
      "ANDROID_RELEASE_STORE_PASSWORD",
      "ANDROID_RELEASE_KEY_ALIAS",
      "ANDROID_RELEASE_KEY_PASSWORD",
      "GHOSTEX_MOBILE_DEPLOY_KEY",
    ]);
  }
}

function dispatch(workflow, fields, dryRun) {
  const args = ["workflow", "run", workflow, "--repo", repo, "--ref", "main"];
  for (const [name, value] of Object.entries(fields)) args.push("-f", `${name}=${value}`);
  if (dryRun) {
    console.log(JSON.stringify({ fields, workflow }, null, 2));
    return;
  }
  const output = run("gh", args);
  const url = output.split(/\r?\n/u).find((line) => /\/actions\/runs\/\d+$/u.test(line.trim()));
  console.log(url ?? output);
}

const { command, options, version } = parseArgs(process.argv.slice(2));
validateScope(options);
const head = validateLocalSource(version, { allowExistingTag: command === "publish" });
if (command === "start" && requiresGpuiReferenceContract(options)) {
  run("node", ["scripts/release-gpui/verify-reference-contract.mjs"], { capture: false });
}
const secrets = configuredSecrets();
validateRequiredSecrets(options, secrets);
const windowsSigned = resolveWindowsSigning(options, secrets);
const platforms = expectedPlatforms(options);
console.log(`Source: ${head}`);
console.log(`Platforms: ${platforms.join(", ")}`);
console.log(`Windows signing: ${windowsSigned ? "enabled" : "disabled"}`);

if (command === "start") {
  dispatch(
    "release-gpui.yml",
    {
      android: options.android,
      gxserver_linux_arm64: options.gxserverLinuxArm64,
      gxserver_linux_x64: options.gxserverLinuxX64,
      gxserver_wsl_windows_arm64: options.gxserverWslWindowsArm64,
      gxserver_wsl_windows_x64: options.gxserverWslWindowsX64,
      linux_deb: options.linuxDeb,
      linux_rpm: options.linuxRpm,
      macos: options.macos,
      prerelease: options.prerelease,
      sign_windows: windowsSigned,
      update_sparkle: options.updateSparkle,
      version,
      windows_arm64: options.windowsArm64,
      windows_x64: options.windowsX64,
    },
    options.dryRun,
  );
} else {
  const sourceRun = JSON.parse(
    run("gh", [
      "run",
      "view",
      options.sourceRunId,
      "--repo",
      repo,
      "--json",
      "event,headSha,status,url,workflowName",
    ]),
  );
  if (sourceRun.status !== "completed") throw new Error(`Source run is ${sourceRun.status}: ${sourceRun.url}`);
  if (sourceRun.workflowName !== "Release Ghostex" || sourceRun.event !== "workflow_dispatch") {
    throw new Error(`Source run is not a dispatched Release Ghostex workflow: ${sourceRun.url}`);
  }
  run("git", ["merge-base", "--is-ancestor", sourceRun.headSha, head]);
  const sourceArtifacts = JSON.parse(
    run("gh", [
      "api",
      `repos/${repo}/actions/runs/${options.sourceRunId}/artifacts?per_page=100`,
    ]),
  ).artifacts ?? [];
  const availableArtifacts = new Set(
    sourceArtifacts.filter((artifact) => !artifact.expired).map((artifact) => artifact.name),
  );
  const missingArtifacts = platforms
    .map((platform) => `release-${platform}`)
    .filter((name) => !availableArtifacts.has(name));
  if (missingArtifacts.length > 0) {
    throw new Error(`Source run is missing non-expired artifacts: ${missingArtifacts.join(", ")}`);
  }
  dispatch(
    "release-gpui-publish.yml",
    {
      expected_platforms: platforms.join(","),
      prerelease: options.prerelease,
      source_run_id: options.sourceRunId,
      update_sparkle: options.updateSparkle,
      version,
      windows_signed: windowsSigned,
    },
    options.dryRun,
  );
}
