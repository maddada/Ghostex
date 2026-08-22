#!/usr/bin/env bun

import { spawn, spawnSync } from "node:child_process";
import {
  closeSync,
  existsSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  readSync,
  rmSync,
  statSync,
  writeSync,
} from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  codeServerComponentIdentity,
  codeServerComponentNames,
} from "./release-gpui/code-server-component-identity.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(scriptPath), "..");
const appVersion = JSON.parse(readFileSync(path.join(repoRoot, "package.json"), "utf8")).version;
const gpuiDir = path.join(repoRoot, "gpui");
const appName = "Ghostex";
const bundleId = "com.madda.ghostex.gpui";
const isDarwin = process.platform === "darwin";
const isWindows = process.platform === "win32";
const isWsl = process.platform === "linux" && (
  Boolean(process.env.WSL_DISTRO_NAME?.trim()) ||
  readFileSync("/proc/sys/kernel/osrelease", "utf8").toLowerCase().includes("microsoft")
);
const targetsWindows = isWindows || isWsl;
const windowsProgramFilesPaths = targetsWindows ? resolveWindowsProgramFilesPaths() : undefined;
const installDir = windowsProgramFilesPaths?.hostPath ?? resolveGpuiInstallDir();
const protocolVersion = 1;
const gxserverBaseUrl = "http://127.0.0.1:58744";
const gxserverExplicitLaunchEnvironmentKeys = ["GHOSTEX_GXSERVER_CLI", "GHOSTEX_GXSERVER_BIN"];
const quietLogTailBytes = 256 * 1024;
const quietLogTailLines = 220;
const quietLogDisplayLineMaxChars = 1200;
const quietLogDisplayLineHeadChars = 760;
const quietLogDisplayLineTailChars = 260;
/*
CDXC:GPUIStartCommand 2026-07-08-04:55:
`bun run gpui` builds the staged GPUI package and installs it to a stable,
platform-appropriate location before launch. macOS refreshes shared resources,
then installs to /Applications and opens through LaunchServices. Windows installs
the staged CEF package to Program Files, creates the machine Start Menu shortcut,
and launches that installed copy. Linux installs the flat CEF package under XDG
data (or INSTALL_DIR), preserves gxserver/zmx sessions across the relaunch, and
runs the installed executable.
*/
const appPath = isDarwin
  ? path.join(gpuiDir, "build", "macos", `${appName}.app`)
  : targetsWindows
    ? path.join(gpuiDir, "build", "windows", appName)
    : path.join(gpuiDir, "build", "linux", appName);
const installedAppPath = path.join(installDir, isDarwin ? `${appName}.app` : appName);
const windowsInstalledAppPath = windowsProgramFilesPaths
  ? path.win32.join(windowsProgramFilesPaths.windowsPath, appName)
  : undefined;
const linuxAppExecutable = path.join(installedAppPath, "Ghostex");
const windowsAppExecutable = path.join(installedAppPath, "Ghostex.exe");
const buildScript = path.join(
  gpuiDir,
  "scripts",
  isDarwin
    ? "build-macos-app.sh"
    : targetsWindows
      ? isWsl ? "build-windows-app-wsl.sh" : "build-windows-app.ps1"
      : "build-linux-app.sh",
);
const localStartLockFile = path.join(repoRoot, "build", "ghostex-gpui-local-start.lock");
const dependenciesRoot = path.join(repoRoot, ".dependencies");
const startOptions = validateStartArguments(process.argv.slice(2));
const startVerbose = startOptions.verbose;
const startEnvironment = withoutColorDisablingEnvironment(process.env);
const windowsArch = process.arch === "arm64" ? "arm64" : "x64";
const explicitWindowsWslArchive = process.env.GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE?.trim();
const explicitWindowsWslCodeServerArchive =
  process.env.GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE?.trim();
const windowsCodeServerIdentity = targetsWindows
  ? await codeServerComponentIdentity({ codeServerRoot: path.join(repoRoot, ".dependencies/code-server") })
  : undefined;
const windowsCodeServerNames = windowsCodeServerIdentity
  ? codeServerComponentNames(windowsCodeServerIdentity.componentVersion, `linux-${windowsArch}`)
  : undefined;
/*
CDXC:WindowsWslRuntimeCache 2026-08-09:
The gxserver release asset keeps the same filename across Ghostex releases. A
single architecture-only cache therefore reused an older CGO-disabled bd after
the release pipeline began shipping the corrected embedded-Dolt build. Scope
the cache to the immutable Ghostex release tag so a version can only consume
the WSL runtime published with that version.
*/
const windowsWslArchive = targetsWindows
  ? explicitWindowsWslArchive
    ? path.resolve(explicitWindowsWslArchive)
    : path.join(
      repoRoot,
      "build",
      "runtime-artifacts",
      `ghostex-${appVersion}`,
      windowsArch,
      `gxserver-linux-${windowsArch}.tar.gz`,
    )
  : undefined;
const windowsWslCodeServerArchive = targetsWindows
  ? explicitWindowsWslCodeServerArchive
    ? path.resolve(explicitWindowsWslCodeServerArchive)
    : path.join(
      repoRoot,
      "build",
      "runtime-artifacts",
      windowsArch,
      windowsCodeServerNames.archiveName,
    )
  : undefined;
const configuration = isDarwin ? resolveLocalStartConfiguration(process.env.CONFIGURATION) : undefined;
const arch = isDarwin ? resolveLocalMacosArch(process.env.GHOSTEX_MACOS_ARCH) : undefined;
const localStartCodeSignIdentity = isDarwin ? resolveLocalStartCodeSignIdentity(startEnvironment) : undefined;
const localStartCodeSignTimestampFlag = isDarwin ? resolveLocalStartCodeSignTimestampFlag(startEnvironment) : undefined;
const buildEnvironment = {
  ...startEnvironment,
  ...(isDarwin
    ? {
      CONFIGURATION: configuration,
      GHOSTEX_APP_VARIANT: "prod",
      /*
       * Keep the packager output path identical to appPath/installedAppPath so
       * the installed application and every macOS-owned label use the public
       * Ghostex product name.
       */
      GHOSTEX_GPUI_APP_NAME: appName,
      GHOSTEX_GPUI_BUNDLE_ID: bundleId,
      GHOSTEX_GPUI_SIGN_IDENTITY: localStartCodeSignIdentity,
      GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG: localStartCodeSignTimestampFlag,
      GHOSTEX_LOCAL_START: "1",
      GHOSTEX_MACOS_ARCH: arch,
      ...(startVerbose ? { GHOSTEX_GPUI_START_VERBOSE: "1", GHOSTEX_START_VERBOSE: "1" } : {}),
    }
    : {}),
  ...(targetsWindows
    ? {
      GHOSTEX_WINDOWS_ARCH: windowsArch,
      GHOSTEX_WINDOWS_REQUIRE_WSL_RUNTIME: "1",
      GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE: windowsWslArchive,
      GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE: windowsWslCodeServerArchive,
      GHOSTEX_CODE_SERVER_COMPONENT_VERSION: windowsCodeServerIdentity.componentVersion,
      ...(isWsl && !startVerbose
        ? { GHOSTEX_WINDOWS_BUILD_PROGRESS_PATH: `/proc/${process.pid}/fd/1` }
        : {}),
    }
    : {}),
};
let startStep = 0;
let activeStartStep;

ensureSupportedHost();
if (isWindows) {
  acquireWindowsLocalStartLock();
} else {
  reexecUnderLocalStartLock();
}
if (targetsWindows) {
  ensureWindowsWslRuntimeArchive();
}
const platformLabel = isDarwin
  ? `${configuration}, ${arch}`
  : targetsWindows
    ? isWsl ? "Windows via WSL2" : "Windows, WSL2"
    : "Linux";
logStartStep(`Checking local GPUI resources (${platformLabel})...`);
ensureLocalReferenceCheckouts();
logStartDetail("Reference checkouts are ready.");
if (!isDarwin && !targetsWindows) {
  await closeRunningGpuiBundle(appPath, {
    action: `before rebuilding ${appPath}`,
    includeBundleId: false,
  });
}
if (isDarwin) {
  logStartStep("Building GPUI runtime resources...");
  run("/bin/bash", [path.join(gpuiDir, "scripts", "prepare-macos-runtime.sh")], {
    env: buildEnvironment,
    quietLabel: "GPUI runtime resource build",
  });
  logStartDetail("GPUI runtime resources are ready.");
  await closeRunningGpuiBundle(appPath, {
    action: `before replacing staged build bundle ${appPath}`,
    includeBundleId: false,
  });
}
if (!isDarwin && !targetsWindows) {
  /*
  CDXC:LinuxRuntimePackaging 2026-07-18:
  gxserver and zmx are one protocol-coupled runtime. The Linux app packager
  previously reused whichever build/remote-gxserver-linux package happened to
  exist, so a freshly compiled gxserver could emit flags unsupported by the
  stale bundled zmx client. Rebuild the host-architecture package from the
  current source before staging every local GPUI build.
  */
  logStartStep("Building local gxserver and zmx runtime...");
  const packagedRuntimeBinDir = path.join(
    repoRoot,
    "build",
    "remote-gxserver-linux",
    process.arch,
    "package",
    "bin",
  );
  const packageArgs = [
    path.join(repoRoot, "gxserver-rs", "package-remote-linux.mjs"),
    "--arch",
    process.arch,
    "--rust-target",
    process.arch === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu",
    "--zig-target",
    process.arch === "arm64" ? "aarch64-linux-gnu" : "x86_64-linux-gnu",
  ];
  const packagedTui = path.join(packagedRuntimeBinDir, "ghostex-tui");
  if (existsSync(packagedTui)) {
    packageArgs.push("--tui-bin", packagedTui);
  }
  run(process.execPath, packageArgs, {
    env: buildEnvironment,
    quietLabel: "Linux gxserver runtime build",
  });
  logStartDetail("Linux gxserver and zmx runtime is ready.");
}
logStartStep("Building GPUI app resources and native shell...");
run(isWindows ? "powershell.exe" : "/bin/bash", isWindows
  ? ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", buildScript]
  : [buildScript], {
  env: buildEnvironment,
  quietLabel: `${appName} build`,
});
logStartDetail("GPUI build completed.");
if (!existsSync(appPath)) {
  throw new Error(`Built GPUI app is missing at ${appPath}.`);
}
if (targetsWindows) {
  await closeRunningGpuiBundle(installedAppPath, {
    action: `before installing rebuilt app to ${windowsInstalledAppPath}`,
    includeBundleId: false,
  });
  installWindowsGpuiApp(appPath);
  logStartStep(`Opening ${appName}...`);
  launchWindowsGpuiApp();
} else if (isDarwin) {
  await closeRunningGpuiBundle(installedAppPath, {
    action: `before installing rebuilt app to ${installedAppPath}`,
    includeBundleId: true,
  });
  await stopRunningGxserverControlPlaneBeforeLaunch(appPath);
  await installAndOpenMacosApp(appPath);
} else {
  await closeRunningGpuiBundle(installedAppPath, {
    action: `before installing rebuilt app to ${installedAppPath}`,
    includeBundleId: false,
  });
  installAndLaunchLinuxApp(appPath);
}
finishStartStep();

function resolveGpuiInstallDir() {
  if (isDarwin) {
    /*
     * Local macOS GPUI debugging has one canonical app identity and location.
     * Do not inherit a generic INSTALL_DIR from a shell/toolchain and create a
     * second LaunchServices-visible Ghostex.app beside /Applications/Ghostex.app.
     */
    return "/Applications";
  }
  const configured = process.env.INSTALL_DIR?.trim();
  if (configured) {
    return configured;
  }
  const xdgDataHome = process.env.XDG_DATA_HOME?.trim();
  return xdgDataHome || path.join(homedir(), ".local", "share");
}

function resolveWindowsProgramFilesPaths() {
  const result = spawnSync(windowsPowerShellExecutable(), [
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    "$dir = $env:ProgramW6432; if (-not $dir) { $dir = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles) }; $dir",
  ], {
    cwd: repoRoot,
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) {
    throw result.error;
  }
  const windowsPath = result.stdout?.trim();
  if (result.status !== 0 || !windowsPath) {
    throw new Error(
      result.stderr?.trim() || "Windows did not report its Program Files directory.",
    );
  }
  if (!isWsl) {
    return { hostPath: windowsPath, windowsPath };
  }
  const converted = spawnSync("wslpath", ["-u", windowsPath], {
    cwd: repoRoot,
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const hostPath = converted.stdout?.trim();
  if (converted.error) {
    throw converted.error;
  }
  if (converted.status !== 0 || !hostPath) {
    throw new Error(
      converted.stderr?.trim() || `Could not map the Windows path ${windowsPath} into WSL.`,
    );
  }
  return { hostPath, windowsPath };
}

function validateStartArguments(args) {
  let verbose =
    truthyStartFlag(process.env.GHOSTEX_GPUI_START_VERBOSE) ||
    truthyStartFlag(process.env.GHOSTEX_START_VERBOSE);
  for (const arg of args) {
    if (arg === "--") {
      continue;
    }
    if (arg === "--verbose" || arg === "-v") {
      verbose = true;
      continue;
    }
    throw new Error(`Unknown GPUI start argument: ${arg}. Use "bun run gpui" or "bun run gpui --verbose".`);
  }
  return { verbose };
}

function truthyStartFlag(value) {
  const normalized = value?.trim().toLowerCase();
  return normalized === "1" || normalized === "true" || normalized === "yes" || normalized === "on";
}

function resolveLocalStartConfiguration(explicitConfiguration) {
  const normalized = explicitConfiguration?.trim();
  if (normalized) {
    return normalized;
  }
  return "Release";
}

function ensureSupportedHost() {
  if (
    process.platform !== "darwin" &&
    process.platform !== "linux" &&
    process.platform !== "win32"
  ) {
    throw new Error("The GPUI local app currently runs on macOS, Linux, and Windows.");
  }
}

function acquireWindowsLocalStartLock() {
  mkdirSync(path.dirname(localStartLockFile), { recursive: true });
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      const fd = openSync(localStartLockFile, "wx");
      writeSync(fd, String(process.pid));
      closeSync(fd);
      process.on("exit", () => {
        try {
          rmSync(localStartLockFile, { force: true });
        } catch {
          // A stale PID lock is detected and replaced by the next start.
        }
      });
      return;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }
      const holderPid = Number.parseInt(readFileSync(localStartLockFile, "utf8").trim(), 10);
      if (Number.isInteger(holderPid) && holderPid > 0 && processIsAlive(holderPid)) {
        throw new Error(
          `Another "bun run gpui" (pid ${holderPid}) is already rebuilding the GPUI app.`,
        );
      }
      rmSync(localStartLockFile, { force: true });
    }
  }
  throw new Error(`Could not acquire the GPUI start lock at ${localStartLockFile}.`);
}

function processIsAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

function ensureWindowsWslRuntimeArchive() {
  if (!existsSync(windowsWslArchive)) {
    if (explicitWindowsWslArchive) {
      throw new Error(
        `GHOSTEX_WINDOWS_WSL_GXSERVER_ARCHIVE does not exist: ${windowsWslArchive}`,
      );
    }
    mkdirSync(path.dirname(windowsWslArchive), { recursive: true });
    logStartStep(`Downloading the Ghostex ${appVersion} WSL2 runtime...`);
    run("gh", [
      "release",
      "download",
      `v${appVersion}`,
      "--repo",
      "maddada/Ghostex",
      "--pattern",
      path.basename(windowsWslArchive),
      "--dir",
      path.dirname(windowsWslArchive),
      "--clobber",
    ], {
      quietLabel: "Windows WSL2 runtime download",
    });
    if (!existsSync(windowsWslArchive)) {
      throw new Error(`The WSL2 runtime download did not produce ${windowsWslArchive}.`);
    }
  }
  const windowsWslCodeServerSidecar = `${windowsWslCodeServerArchive}.sha256`;
  const hasWindowsWslCodeServerArchive = existsSync(windowsWslCodeServerArchive);
  const hasWindowsWslCodeServerSidecar = existsSync(windowsWslCodeServerSidecar);
  if (hasWindowsWslCodeServerArchive !== hasWindowsWslCodeServerSidecar) {
    throw new Error(
      `The cached WSL2 Source runtime must contain both ${path.basename(windowsWslCodeServerArchive)} and its filename-bound .sha256 sidecar.`,
    );
  }
  if (!hasWindowsWslCodeServerArchive) {
    if (explicitWindowsWslCodeServerArchive) {
      throw new Error(
        `GHOSTEX_WINDOWS_WSL_CODE_SERVER_ARCHIVE and its filename-bound .sha256 sidecar must exist: ${windowsWslCodeServerArchive}`,
      );
    }
    mkdirSync(path.dirname(windowsWslCodeServerArchive), { recursive: true });
    logStartStep(`Downloading the Ghostex WSL2 Source runtime ${windowsCodeServerIdentity.componentVersion}...`);
    run("gh", [
      "run",
      "download",
      "--repo",
      "maddada/Ghostex",
      "--name",
      windowsCodeServerNames.artifactName,
      "--dir",
      path.dirname(windowsWslCodeServerArchive),
    ], {
      quietLabel: "Windows WSL2 Source runtime download",
    });
    if (!existsSync(windowsWslCodeServerArchive) || !existsSync(windowsWslCodeServerSidecar)) {
      throw new Error(
        `The code-server producer artifact did not contain ${path.basename(windowsWslCodeServerArchive)} and its filename-bound .sha256 sidecar.`,
      );
    }
  }
}

function logStartStep(message) {
  if (startVerbose) {
    return;
  }
  finishStartStep();
  startStep += 1;
  activeStartStep = { message, startedAtMs: Date.now() };
  console.log(`[${startStep}] ${message}`);
}

function logStartDetail(message, indent = 1) {
  if (startVerbose) {
    return;
  }
  console.log(`${"    ".repeat(indent)}${message}`);
}

function finishStartStep() {
  if (startVerbose || !activeStartStep) {
    return;
  }
  logStartDetail(`Completed in ${formatDuration(Date.now() - activeStartStep.startedAtMs)}.`);
  activeStartStep = undefined;
}

function formatDuration(durationMs) {
  if (durationMs < 1000) {
    return `${durationMs}ms`;
  }
  if (durationMs < 10_000) {
    return `${(durationMs / 1000).toFixed(1)}s`;
  }
  return `${Math.round(durationMs / 1000)}s`;
}

function reexecUnderLocalStartLock() {
  /*
  CDXC:GPUIStartCommand 2026-06-21-18:43:
  `bun run gpui` must be the GPUI equivalent of the macOS local start command: one root command builds the local CEF/GPUI bundle, prevents overlapping rebuilds, closes only the matching GPUI bundle before replacing it, and launches the rebuilt app without using Cua Driver or the main Ghostex start path.

  CDXC:GPUIDependencies 2026-08-02:
  Zed, cef-rs, and gpui-component are pinned submodules under the repository's
  `.dependencies` tree. Initialize an absent checkout, but never replace a
  present incomplete directory because it may contain user or agent work.
  */
  if (process.env.GHOSTEX_GPUI_START_LOCK_HELD === "1") {
    return;
  }
  mkdirSync(path.dirname(localStartLockFile), { recursive: true });
  // lockf(1) is macOS/BSD; flock(1) is the util-linux equivalent.
  const [lockCommand, ...lockArgs] = isDarwin
    ? ["/usr/bin/lockf", "-k", localStartLockFile]
    : ["flock", localStartLockFile];
  const result = spawnSync(
    lockCommand,
    [...lockArgs, process.execPath, scriptPath, ...process.argv.slice(2)],
    {
      cwd: repoRoot,
      env: { ...process.env, GHOSTEX_GPUI_START_LOCK_HELD: "1" },
      stdio: "inherit",
    },
  );
  if (result.error) {
    throw result.error;
  }
  process.exit(result.status ?? 1);
}

function ensureLocalReferenceCheckouts() {
  mkdirSync(dependenciesRoot, { recursive: true });
  ensureReferenceCheckout({
    name: "zed",
    requiredRelativePath: path.join("crates", "gpui", "Cargo.toml"),
  });
  ensureReferenceCheckout({
    name: "cef-rs",
    requiredRelativePath: path.join("cef", "Cargo.toml"),
  });
  ensureReferenceCheckout({
    name: "gpui-component",
    requiredRelativePath: path.join("crates", "ui", "Cargo.toml"),
  });
}

function requirePinnedDependencyRevision(name, checkoutPath) {
  const expectedRevision = dependencyGitOutput(repoRoot, [
    "rev-parse",
    `HEAD:.dependencies/${name}`,
  ]);
  const revision = dependencyGitOutput(checkoutPath, ["rev-parse", "HEAD"]);
  if (revision !== expectedRevision) {
    throw new Error(
      `GPUI dependency ${checkoutPath} is at ${revision || "an unreadable revision"}, expected committed revision ${expectedRevision}. Refusing to alter an existing checkout because it may contain user work.`,
    );
  }
}

function ensureReferenceCheckout({ name, requiredRelativePath }) {
  const expectedPath = path.join(dependenciesRoot, name);
  const expectedRequiredPath = path.join(expectedPath, requiredRelativePath);
  if (!existsSync(expectedRequiredPath)) {
    if (
      pathExistsWithoutFollowingFinalSymlink(expectedPath) &&
      !dependencySubmoduleIsUninitialized(name)
    ) {
      throw new Error(
        `GPUI dependency ${expectedPath} exists, but ${expectedRequiredPath} is missing. Refusing to overwrite it; fix or replace that submodule checkout manually.`,
      );
    }
    initializeDependencySubmodule(name);
    if (!existsSync(expectedRequiredPath)) {
      throw new Error(`GPUI dependency ${expectedPath} is incomplete after submodule initialization.`);
    }
  }
  preparePinnedDependency(name, expectedPath);
}

function dependencySubmoduleIsUninitialized(name) {
  const relativePath = path.join(".dependencies", name);
  const result = spawnSync("git", [
    "-c",
    `safe.directory=${repoRoot}`,
    "submodule",
    "status",
    "--",
    relativePath,
  ], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(result.stderr?.trim() || `Unable to inspect GPUI dependency ${relativePath}.`);
  }
  return result.stdout.trimStart().startsWith("-");
}

function initializeDependencySubmodule(name) {
  const relativePath = path.join(".dependencies", name);
  run("git", [
    "-c",
    `safe.directory=${repoRoot}`,
    "submodule",
    "update",
    "--init",
    "--depth=1",
    "--",
    relativePath,
  ], {
    env: startEnvironment,
    quietLabel: `${name} dependency checkout`,
  });
}

function preparePinnedDependency(name, checkoutPath) {
  requirePinnedDependencyRevision(name, checkoutPath);
}

function dependencyGitOutput(checkoutPath, args) {
  const result = spawnSync("git", ["-c", `safe.directory=${checkoutPath}`, "-C", checkoutPath, ...args], {
    encoding: "utf8",
    env: startEnvironment,
  });
  if (result.status !== 0) {
    throw new Error(result.stderr?.trim() || `git ${args.join(" ")} failed for ${checkoutPath}.`);
  }
  return result.stdout.trim();
}

function pathExistsWithoutFollowingFinalSymlink(candidatePath) {
  try {
    lstatSync(candidatePath);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") {
      return false;
    }
    throw error;
  }
}

async function closeRunningGpuiBundle(bundlePath, { action, includeBundleId }) {
  /*
  CDXC:GPUIStartCommand 2026-06-25-13:56:
  The local GPUI rebuild command must fully close the exact dev bundle before replacing it. If AppleScript quit and SIGTERM leave a stale or slow GPUI process alive, escalate to SIGKILL and still verify the bundle has exited before building.

  CDXC:GPUIStartCommand 2026-07-08-04:55:
  macOS GPUI starts now build before closing the stable installed app. Only the
  old staged build bundle is closed before packaging, because build-macos-app.sh
  replaces that directory; the installed /Applications copy is closed only after
  a successful build, matching `bun run start`.
  */
  let pids = findRunningGpuiBundlePids(bundlePath, { includeBundleId });
  if (pids.length === 0) {
    return;
  }

  const closeMessage = `Closing running ${appName} ${action}.`;
  if (startVerbose) {
    console.log(closeMessage);
  } else {
    logStartDetail(closeMessage);
  }
  if (includeBundleId && isDarwin) {
    run("osascript", ["-e", `tell application id "${bundleId}" to quit`], {
      allowFailure: true,
      stdio: "ignore",
    });
    if (await waitForGpuiBundleExit(bundlePath, { includeBundleId }, 8000)) {
      return;
    }
  }

  pids = findRunningGpuiBundlePids(bundlePath, { includeBundleId });
  terminateGpuiPids(pids, false);
  if (await waitForGpuiBundleExit(bundlePath, { includeBundleId }, 8000)) {
    return;
  }

  pids = findRunningGpuiBundlePids(bundlePath, { includeBundleId });
  const forceMessage = `Force closing ${appName} ${action}.`;
  if (startVerbose) {
    console.log(forceMessage);
  } else {
    logStartDetail(forceMessage);
  }
  terminateGpuiPids(pids, true);
  if (!(await waitForGpuiBundleExit(bundlePath, { includeBundleId }, 2000))) {
    throw new Error(`${appName} did not exit, refusing to continue while ${bundlePath} is still running.`);
  }
}

async function waitForGpuiBundleExit(bundlePath, options, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (findRunningGpuiBundlePids(bundlePath, options).length === 0) {
      return true;
    }
    await sleep(100);
  }
  return findRunningGpuiBundlePids(bundlePath, options).length === 0;
}

function findRunningGpuiBundlePids(bundlePath, { includeBundleId }) {
  return uniquePids([
    ...(includeBundleId ? findRunningGpuiPidsByBundleId() : []),
    ...findRunningGpuiPidsByBundlePath(bundlePath),
  ]);
}

function findRunningGpuiPidsByBundleId() {
  if (!isDarwin) {
    return [];
  }
  const result = spawnSync("osascript", [
    "-e",
    `tell application "System Events" to get the unix id of every process whose bundle identifier is "${bundleId}"`,
  ], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (result.status !== 0 || !result.stdout.trim()) {
    return [];
  }
  return parsePidList(result.stdout).filter((pid) => /^\d+$/.test(pid));
}

function findRunningGpuiPidsByBundlePath(bundlePath) {
  if (targetsWindows) {
    /*
    CDXC:GPUIWindowsWslStart 2026-08-02:
    Local Windows development can be driven entirely by the WSL bash launcher.
    Query the two product-specific image names with tasklist instead of using a
    PowerShell CIM pipeline; no other application ships either executable name,
    and taskkill below closes the matching main/helper process tree before the
    staged directory is replaced.
    */
    const pids = [];
    for (const imageName of ["Ghostex.exe", "ghostex-gpui-cef-helper.exe"]) {
      const result = spawnSync(windowsSystemExecutable("tasklist"), [
        "/FI",
        `IMAGENAME eq ${imageName}`,
        "/FO",
        "CSV",
        "/NH",
      ], {
        cwd: repoRoot,
        encoding: "utf8",
        env: startEnvironment,
        stdio: ["ignore", "pipe", "ignore"],
      });
      if (result.status !== 0) {
        continue;
      }
      for (const match of result.stdout.matchAll(/^"[^"]+","(\d+)"/gmu)) {
        pids.push(match[1]);
      }
    }
    return pids;
  }
  const result = spawnSync("ps", ["-axo", "pid=,args=", "-ww"], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (result.status !== 0 || !result.stdout.trim()) {
    return [];
  }
  return result.stdout
    .split(/\r?\n/)
    .map((line) => line.match(/^\s*(\d+)\s+(.+)$/))
    .filter((match) => match && commandLineBelongsToGpuiBundle(match[2], bundlePath))
    .map((match) => match[1]);
}

function terminateGpuiPids(pids, force) {
  if (targetsWindows) {
    for (const pid of pids) {
      run(windowsSystemExecutable("taskkill"), [
        "/PID",
        pid,
        "/T",
        ...(force ? ["/F"] : []),
      ], {
        allowFailure: true,
        stdio: "ignore",
      });
    }
    return;
  }
  for (const pid of pids) {
    try {
      process.kill(Number(pid), force ? "SIGKILL" : "SIGTERM");
    } catch {
      // Process already exited.
    }
  }
}

function windowsSystemExecutable(name) {
  return isWsl ? `/mnt/c/Windows/System32/${name}.exe` : `${name}.exe`;
}

function windowsPowerShellExecutable() {
  return isWsl
    ? "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
    : "powershell.exe";
}

function windowsPathForHostPath(hostPath) {
  if (!isWsl) {
    return hostPath;
  }
  const result = spawnSync("wslpath", ["-w", hostPath], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) {
    throw result.error;
  }
  const windowsPath = result.stdout?.trim();
  if (result.status !== 0 || !windowsPath) {
    throw new Error(
      result.stderr?.trim() || `Could not map the WSL path ${hostPath} into Windows.`,
    );
  }
  return windowsPath;
}

function commandLineBelongsToGpuiBundle(commandLine, bundlePath) {
  if (isDarwin) {
    /*
    CDXC:GPUIStartCommand 2026-07-10:
    A macOS app bundle also contains long-lived gxserver and zmx executables
    under Contents/Resources. Those processes deliberately survive a GPUI app
    restart, so bundle-path ownership must include only the main UI executable
    and CEF helper app executables. Otherwise the graceful-quit wait can never
    finish while persistence is working, and its SIGTERM/SIGKILL escalation
    kills the very zmx sessions the relaunched app is meant to reattach.
    */
    const mainExecutable = path.join(bundlePath, "Contents", "MacOS", appName);
    if (commandLineRunsExecutable(commandLine, mainExecutable)) {
      return true;
    }

    const helpersRoot = path.join(bundlePath, "Contents", "Frameworks");
    const helperBundlePrefix = `${appName} Helper`;
    const helperBundleMarker = `.app${path.sep}Contents${path.sep}MacOS${path.sep}`;
    if (!commandLine.startsWith(helpersRoot + path.sep)) {
      return false;
    }
    const relativeCommandLine = commandLine.slice(helpersRoot.length + path.sep.length);
    const helperBundleMarkerIndex = relativeCommandLine.indexOf(helperBundleMarker);
    if (helperBundleMarkerIndex < 0) {
      return false;
    }
    const helperName = relativeCommandLine.slice(0, helperBundleMarkerIndex);
    if (!helperName.startsWith(helperBundlePrefix)) {
      return false;
    }
    const helperExecutable = path.join(
      helpersRoot,
      `${helperName}.app`,
      "Contents",
      "MacOS",
      helperName,
    );
    return commandLineRunsExecutable(commandLine, helperExecutable);
  }
  // Flat Linux layout: match only the staged app binaries (Ghostex and
  // ghostex-gpui-cef-helper). The staged gxserver daemon and zmx sessions
  // live under the same directory and must survive a rebuild.
  return ["Ghostex", "ghostex-gpui-cef-helper"].some((name) =>
    commandLineRunsExecutable(commandLine, path.join(bundlePath, name))
  );
}

function commandLineRunsExecutable(commandLine, executablePath) {
  return commandLine === executablePath || commandLine.startsWith(`${executablePath} `);
}

async function installAndOpenMacosApp(stagedAppPath) {
  logStartStep(`Installing ${appName} to ${installDir}...`);
  syncInstalledAppBundle(stagedAppPath);
  logStartStep("Checking installed GPUI app signature...");
  ensureInstalledAppCodeSignature(installedAppPath);
  logStartStep("Preparing LaunchServices environment...");
  const explicitGxserverCount = publishLaunchServicesGxserverExplicitEnvironment();
  logStartDetail(
    explicitGxserverCount > 0
      ? `Published ${explicitGxserverCount} explicit gxserver daemon override${explicitGxserverCount === 1 ? "" : "s"}.`
      : "No explicit gxserver daemon override is set; GPUI will use its bundled daemon.",
  );
  logStartStep(`Opening ${appName}...`);
  run("open", [installedAppPath], { env: startEnvironment });
  await verifyCanonicalMacosGpuiLaunch();
  logStartDetail(`One canonical app process is running from ${installedAppPath}.`);
}

async function verifyCanonicalMacosGpuiLaunch() {
  const deadline = Date.now() + 10_000;
  let bundlePids = [];
  let canonicalPids = [];
  while (Date.now() < deadline) {
    bundlePids = findRunningGpuiPidsByBundleId();
    canonicalPids = findRunningGpuiPidsByBundlePath(installedAppPath);
    if (bundlePids.length === 1 && canonicalPids.includes(bundlePids[0])) {
      return;
    }
    await sleep(100);
  }
  throw new Error(
    `Expected exactly one ${bundleId} app launched from ${installedAppPath}; found ${bundlePids.length} bundle process(es) and ${canonicalPids.length} canonical bundle process(es).`,
  );
}

function installAndLaunchLinuxApp(stagedAppPath) {
  logStartStep(`Installing ${appName} to ${installDir}...`);
  mkdirSync(installDir, { recursive: true });
  syncInstalledAppBundle(stagedAppPath);
  logStartStep(`Opening ${appName}...`);
  launchLinuxGpuiApp();
}

function syncInstalledAppBundle(stagedAppPath) {
  const rsyncArgs = ["-a", "--delete", `${stagedAppPath}/`, `${installedAppPath}/`];
  if (startVerbose) {
    run("rsync", rsyncArgs);
  } else {
    run("rsync", [...rsyncArgs.slice(0, 2), "--itemize-changes", ...rsyncArgs.slice(2)], {
      quietLabel: `Install ${appName} bundle`,
      quietSummary: "rsync",
    });
  }
  logStartDetail(`Installed bundle synced to ${installedAppPath}.`);
}

function ensureInstalledAppCodeSignature(appPathForSignature) {
  const signatureStatus = inspectInstalledAppCodeSignature(appPathForSignature);
  if (signatureStatus.reusable) {
    logStartDetail(`Installed signature is current; skipping re-sign (${signatureStatus.reason}).`);
    return;
  }
  logStartDetail(`Re-signing installed GPUI app bundle (${signatureStatus.reason}).`);
  run(path.join(gpuiDir, "scripts", "codesign-gpui-app.sh"), [appPathForSignature], {
    env: buildEnvironment,
    quietLabel: `Installed ${appName} signing`,
    quietSummary: "codesign",
  });
  logStartDetail("Installed app bundle signed.");
}

function inspectInstalledAppCodeSignature(appPathForSignature) {
  if (!hasValidInstalledAppCodeSignature(appPathForSignature)) {
    return { reason: "existing signature failed deep verification", reusable: false };
  }
  if (!hasExpectedInstalledAppSigningIdentity(appPathForSignature)) {
    return { reason: "existing signature does not match the requested local-start identity", reusable: false };
  }
  return { reason: "deep verification and signing identity match", reusable: true };
}

function hasValidInstalledAppCodeSignature(appPathForSignature) {
  const result = spawnSync("codesign", ["--verify", "--deep", "--strict", appPathForSignature], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) {
    throw result.error;
  }
  return result.status === 0;
}

function hasExpectedInstalledAppSigningIdentity(appPathForSignature) {
  const signatureDetails = readCodeSignatureDetails(appPathForSignature);
  if (!signatureDetails) {
    return false;
  }
  const expectedIdentity = buildEnvironment.GHOSTEX_GPUI_SIGN_IDENTITY ?? "-";
  if (!expectedIdentity || expectedIdentity === "-") {
    return signatureDetails.includes("Signature=adhoc") || signatureDetails.includes("TeamIdentifier=not set");
  }
  return signatureDetails
    .split(/\r?\n/)
    .map((line) => line.trim())
    .includes(`Authority=${expectedIdentity}`);
}

function readCodeSignatureDetails(codePath) {
  const result = spawnSync("codesign", ["-dv", "--verbose=4", codePath], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    return undefined;
  }
  return `${result.stderr}\n${result.stdout}`;
}

async function stopRunningGxserverControlPlaneBeforeLaunch(stagedAppPath) {
  logStartStep("Checking gxserver control plane...");
  const expectedBuildIdentity = readBundledGxserverBuildIdentity(stagedAppPath);
  if (!expectedBuildIdentity) {
    console.warn("The built GPUI app has no bundled gxserver build identity; stopping any running control plane anyway.");
  }

  const token = readGxserverToken();
  if (!token) {
    logStartDetail("No gxserver auth token found; nothing to stop.");
    return;
  }

  const health = await fetchGxserverJson("/api/health/server", { method: "GET", token });
  if (!health || health.product !== "gxserver") {
    logStartDetail("No running gxserver control plane found.");
    return;
  }

  const actualBuildIdentity = typeof health.buildIdentity === "string" ? health.buildIdentity.trim() : "";
  const buildIdentitySuffix =
    actualBuildIdentity && expectedBuildIdentity && actualBuildIdentity !== expectedBuildIdentity
      ? ` (build identity ${actualBuildIdentity} -> ${expectedBuildIdentity})`
      : "";
  const stopReason = gxserverControlPlaneStopReason({ actualBuildIdentity, expectedBuildIdentity });

  if (startVerbose) {
    console.log(`Stopping gxserver control plane before opening ${appName}${buildIdentitySuffix}.`);
  } else {
    logStartDetail(`Stopping running gxserver control plane (${stopReason}).`);
  }
  await fetchGxserverJson("/api/control/stop", { method: "POST", token });
  const stopped = await waitForGxserverStop(token, 5000);
  if (!stopped) {
    throw new Error("gxserver stop was requested, but the old control plane is still responding.");
  }
  logStartDetail("gxserver control plane stopped; GPUI will start its bundled daemon on launch.");
}

function gxserverControlPlaneStopReason({ actualBuildIdentity, expectedBuildIdentity }) {
  if (!expectedBuildIdentity) {
    return "bundled build identity is unavailable, so local start resets the daemon";
  }
  if (!actualBuildIdentity) {
    return "running daemon did not report a build identity";
  }
  if (actualBuildIdentity !== expectedBuildIdentity) {
    return "bundled daemon changed";
  }
  return "local GPUI start resets the daemon even when the bundled identity is current";
}

function readBundledGxserverBuildIdentity(stagedAppPath) {
  const identityPath = path.join(stagedAppPath, "Contents", "Resources", "Web", "gxserver", "build-identity.json");
  if (!existsSync(identityPath)) {
    return undefined;
  }
  const parsed = JSON.parse(readFileSync(identityPath, "utf8"));
  const buildIdentity = typeof parsed.buildIdentity === "string" ? parsed.buildIdentity.trim() : "";
  return buildIdentity || undefined;
}

function readGxserverToken() {
  const configuredHome = process.env.GHOSTEX_HOME?.trim();
  const configuredStateHome = process.env.XDG_STATE_HOME?.trim();
  const explicitHome = configuredHome && path.isAbsolute(configuredHome) ? configuredHome : undefined;
  const absoluteStateHome = configuredStateHome && path.isAbsolute(configuredStateHome)
    ? configuredStateHome
    : undefined;
  const stateDir = explicitHome
    ? path.join(explicitHome, "state")
    : path.join(absoluteStateHome || path.join(homedir(), ".local", "state"), "ghostex");
  const tokenPaths = [path.join(stateDir, "gxserver", "auth", "token")];
  if (!explicitHome) {
    // Read-only upgrade compatibility before the app has had a chance to run
    // the storage migration. Current XDG state always wins.
    tokenPaths.push(
      path.join(homedir(), "Library", "Application Support", "Ghostex", "State", "gxserver", "auth", "token"),
      path.join(homedir(), ".ghostex", "gxserver", "auth", "token"),
    );
  }
  for (const tokenPath of tokenPaths) {
    if (!existsSync(tokenPath)) continue;
    const token = readFileSync(tokenPath, "utf8").trim();
    if (token) return token;
  }
  return undefined;
}

async function waitForGxserverStop(token, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const health = await fetchGxserverJson("/api/health/server", { method: "GET", token, timeoutMs: 500 });
    if (!health) {
      return true;
    }
    await sleep(100);
  }
  return !(await fetchGxserverJson("/api/health/server", { method: "GET", token, timeoutMs: 500 }));
}

async function fetchGxserverJson(pathname, { method, token, timeoutMs = 1000 }) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(`${gxserverBaseUrl}${pathname}`, {
      headers: {
        authorization: `Bearer ${token}`,
        "x-gxserver-protocol-version": String(protocolVersion),
      },
      method,
      signal: controller.signal,
    });
    if (!response.ok) {
      return undefined;
    }
    return await response.json();
  } catch {
    return undefined;
  } finally {
    clearTimeout(timeout);
  }
}

function publishLaunchServicesGxserverExplicitEnvironment() {
  let publishedCount = 0;
  for (const key of gxserverExplicitLaunchEnvironmentKeys) {
    const value = process.env[key]?.trim();
    if (value) {
      run("launchctl", ["setenv", key, value], { stdio: "ignore" });
      publishedCount += 1;
    } else {
      run("launchctl", ["unsetenv", key], { allowFailure: true, stdio: "ignore" });
    }
  }
  return publishedCount;
}

function launchLinuxGpuiApp() {
  const child = spawn(linuxAppExecutable, [], {
    cwd: appPath,
    env: startEnvironment,
    detached: true,
    stdio: "ignore",
  });
  child.on("error", (error) => {
    throw error;
  });
  child.unref();
  console.log(`Launched ${linuxAppExecutable} (pid ${child.pid}).`);
}

function launchWindowsGpuiApp() {
  const child = spawn(windowsAppExecutable, [], {
    cwd: installedAppPath,
    env: startEnvironment,
    detached: true,
    stdio: "ignore",
  });
  child.on("error", (error) => {
    throw error;
  });
  child.unref();
  console.log(`Launched ${windowsAppExecutable} (pid ${child.pid}).`);
}

function installWindowsGpuiApp(stagedAppPath) {
  logStartStep(`Installing ${appName} to ${windowsInstalledAppPath}...`);
  logStartDetail("Windows may request administrator approval for Program Files and the all-users Start Menu.");
  const installerScript = path.join(repoRoot, "scripts", "install-windows-gpui.ps1");
  const installResult = run(windowsPowerShellExecutable(), [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    windowsPathForHostPath(installerScript),
    "-StagedAppPath",
    windowsPathForHostPath(stagedAppPath),
  ], {
    allowFailure: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (installResult.stdout?.length > 0) {
    process.stdout.write(installResult.stdout);
  }
  if (installResult.stderr?.length > 0) {
    process.stderr.write(installResult.stderr);
  }
  if (installResult.status !== 0) {
    throw new Error(`The Windows Ghostex installer failed with exit code ${installResult.status ?? 1}.`);
  }
  if (!existsSync(windowsAppExecutable)) {
    throw new Error(`The installed Ghostex executable is missing at ${windowsInstalledAppPath}\\Ghostex.exe.`);
  }
  logStartDetail(`Installed app and Start Menu shortcut are ready for all Windows users.`);
}

function parsePidList(value) {
  return value
    .split(/[,\s]+/)
    .map((pid) => pid.trim())
    .filter(Boolean);
}

function uniquePids(pids) {
  return [...new Set(pids)];
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function run(command, args, options = {}) {
  if (options.quietLabel && !startVerbose && (options.stdio === undefined || options.stdio === "inherit")) {
    return runQuiet(command, args, options);
  }
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    env: options.env ?? startEnvironment,
    stdio: options.stdio ?? "inherit",
  });
  if (result.error) {
    throw result.error;
  }
  if (!options.allowFailure && result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status ?? 1}.`);
  }
  return result;
}

function runQuiet(command, args, options = {}) {
  const logPath = quietCommandLogPath(options.quietLabel);
  mkdirSync(path.dirname(logPath), { recursive: true });
  const logFile = openSync(logPath, "w");
  let result;
  try {
    writeSync(logFile, `$ ${formatCommand(command, args)}\n`);
    result = spawnSync(command, args, {
      cwd: options.cwd ?? repoRoot,
      env: options.env ?? startEnvironment,
      stdio: ["ignore", logFile, logFile],
    });
  } finally {
    closeSync(logFile);
  }
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0 && !options.allowFailure) {
    reportQuietCommandFailure(options.quietLabel, result.status ?? 1, logPath);
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status ?? 1}.`);
  }
  summarizeQuietCommandLog(logPath, options.quietSummary);
  if (result.status === 0 || options.allowFailure) {
    rmSync(logPath, { force: true });
  }
  return result;
}

function summarizeQuietCommandLog(logPath, summaryKind) {
  if (!summaryKind || !existsSync(logPath)) {
    return;
  }
  const logText = readFileSync(logPath, "utf8");
  if (summaryKind === "rsync") {
    summarizeRsyncLog(logText);
  } else if (summaryKind === "codesign") {
    summarizeCodesignLog(logText);
  }
}

function summarizeRsyncLog(logText) {
  const summary = collectRsyncSummary(logText);
  if (summary.updated === 0 && summary.deleted === 0) {
    logStartDetail("Install sync: installed bundle was already current.");
    return;
  }
  const parts = [];
  if (summary.files > 0) {
    parts.push(`${summary.files} file${summary.files === 1 ? "" : "s"}`);
  }
  if (summary.directories > 0) {
    parts.push(`${summary.directories} director${summary.directories === 1 ? "y" : "ies"}`);
  }
  if (summary.links > 0) {
    parts.push(`${summary.links} link${summary.links === 1 ? "" : "s"}`);
  }
  if (summary.other > 0) {
    parts.push(`${summary.other} other item${summary.other === 1 ? "" : "s"}`);
  }
  const updated = parts.length > 0 ? `${summary.updated} updated (${parts.join(", ")})` : `${summary.updated} updated`;
  logStartDetail(`Install sync: ${updated}, ${summary.deleted} deleted.`);
}

function collectRsyncSummary(logText) {
  const summary = { deleted: 0, directories: 0, files: 0, links: 0, other: 0, updated: 0 };
  for (const rawLine of logText.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("$ ")) {
      continue;
    }
    if (line.startsWith("*deleting ")) {
      summary.deleted += 1;
      continue;
    }
    if (!isRsyncItemizedChangeLine(line)) {
      continue;
    }
    summary.updated += 1;
    const itemType = line[1];
    if (itemType === "f") {
      summary.files += 1;
    } else if (itemType === "d") {
      summary.directories += 1;
    } else if (itemType === "L") {
      summary.links += 1;
    } else {
      summary.other += 1;
    }
  }
  return summary;
}

function isRsyncItemizedChangeLine(line) {
  return /^[<>ch.*][fdLDS]/.test(line);
}

function summarizeCodesignLog(logText) {
  const lines = logText.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const identityLine = lines.find((line) => line.startsWith("Identity: "));
  if (identityLine) {
    logStartDetail(identityLine);
  }
  const replacedCount = lines.filter((line) => line.includes("replacing existing signature")).length;
  if (replacedCount > 0) {
    logStartDetail(`Re-signed ${replacedCount} nested code item${replacedCount === 1 ? "" : "s"}.`);
  }
  if (lines.some((line) => line.includes("valid on disk")) && lines.some((line) => line.includes("satisfies its Designated Requirement"))) {
    logStartDetail("Code signature verified.");
  }
}

function quietCommandLogPath(label) {
  const normalizedLabel = label.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "command";
  return path.join(repoRoot, "build", "local-start-logs", `${Date.now()}-${process.pid}-${normalizedLabel}.log`);
}

function reportQuietCommandFailure(label, status, logPath) {
  const relativeLogPath = path.relative(repoRoot, logPath);
  console.error(`${label} failed with exit code ${status}.`);
  console.error(`Full log: ${relativeLogPath}`);
  console.error("Rerun with `bun run gpui --verbose` for live output.");
  const tail = readQuietLogTail(logPath);
  if (tail) {
    console.error(`\nLast ${quietLogTailLines} log lines (long lines shortened; full lines remain in ${relativeLogPath}):\n${tail}`);
  }
}

function readQuietLogTail(logPath) {
  if (!existsSync(logPath)) {
    return "";
  }
  const size = statSync(logPath).size;
  const start = Math.max(0, size - quietLogTailBytes);
  const length = size - start;
  if (length <= 0) {
    return "";
  }
  const file = openSync(logPath, "r");
  try {
    const buffer = Buffer.alloc(length);
    readSync(file, buffer, 0, length, start);
    const lines = buffer.toString("utf8").split(/\r?\n/);
    if (start > 0) {
      lines[0] = "[output truncated]";
    }
    return lines.slice(-quietLogTailLines).map(formatQuietLogLineForTerminal).join("\n").trimEnd();
  } finally {
    closeSync(file);
  }
}

function formatQuietLogLineForTerminal(line) {
  if (line.length <= quietLogDisplayLineMaxChars) {
    return line;
  }
  const omittedCharacterCount = line.length - quietLogDisplayLineHeadChars - quietLogDisplayLineTailChars;
  const prefix = line.slice(0, quietLogDisplayLineHeadChars).trimEnd();
  const suffix = line.slice(-quietLogDisplayLineTailChars).trimStart();
  return `${prefix} ... [shortened ${omittedCharacterCount} characters from one log line; full line remains in the log file] ... ${suffix}`;
}

function formatCommand(command, args) {
  return [command, ...args].map(shellQuote).join(" ");
}

function shellQuote(value) {
  const text = String(value);
  if (/^[A-Za-z0-9_./:=@%+-]+$/.test(text)) {
    return text;
  }
  return `'${text.replaceAll("'", "'\\''")}'`;
}

function resolveLocalMacosArch(explicitArch) {
  const normalized = explicitArch?.trim();
  if (normalized) {
    if (["arm64", "aarch64"].includes(normalized)) {
      return "arm64";
    }
    if (["x86_64", "x64", "amd64"].includes(normalized)) {
      return "x86_64";
    }
    throw new Error(`Unsupported GHOSTEX_MACOS_ARCH: ${normalized}. Use arm64 or x86_64.`);
  }
  const arm64Capability = spawnSync("/usr/sbin/sysctl", ["-in", "hw.optional.arm64"], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (arm64Capability.status === 0 && arm64Capability.stdout.trim() === "1") {
    return "arm64";
  }
  const machine = spawnSync("uname", ["-m"], {
    cwd: repoRoot,
    encoding: "utf8",
    env: startEnvironment,
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (machine.error) {
    throw machine.error;
  }
  return machine.stdout.trim() || "x86_64";
}

function resolveLocalStartCodeSignIdentity(environment) {
  if (Object.hasOwn(environment, "GHOSTEX_GPUI_SIGN_IDENTITY")) {
    return environment.GHOSTEX_GPUI_SIGN_IDENTITY ?? "";
  }
  const identities = listCodeSigningIdentities(environment);
  const preferredIdentity =
    identities.find((identity) => identity.name.startsWith("Apple Development: ")) ??
    identities.find((identity) => identity.name.startsWith("Mac Developer: ")) ??
    identities.find((identity) => identity.name.startsWith("Developer ID Application: ")) ??
    identities.find((identity) => identity.name.startsWith("Apple Distribution: "));
  if (preferredIdentity) {
    return preferredIdentity.name;
  }
  console.warn(
    "No Apple code-signing identity was found; falling back to ad-hoc GPUI signing. macOS may ask for permissions again after GPUI rebuilds.",
  );
  return "-";
}

function resolveLocalStartCodeSignTimestampFlag(environment) {
  if (Object.hasOwn(environment, "GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG")) {
    return environment.GHOSTEX_GPUI_SIGN_TIMESTAMP_FLAG ?? "";
  }
  return "--timestamp=none";
}

function listCodeSigningIdentities(environment) {
  const result = spawnSync("security", ["find-identity", "-v", "-p", "codesigning"], {
    cwd: repoRoot,
    encoding: "utf8",
    env: environment,
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (result.error || result.status !== 0) {
    return [];
  }
  const identities = [];
  for (const line of result.stdout.split(/\r?\n/)) {
    const match = line.match(/^\s*\d+\)\s+([A-Fa-f0-9]+)\s+"([^"]+)"/);
    if (match) {
      identities.push({ hash: match[1], name: match[2] });
    }
  }
  return identities;
}

function withoutColorDisablingEnvironment(environment) {
  const sanitized = { ...environment };
  for (const key of ["ANSI_COLORS_DISABLED", "NO_COLOR", "NODE_DISABLE_COLORS"]) {
    delete sanitized[key];
  }
  if (isColorDisablingForceColor(sanitized.FORCE_COLOR)) {
    delete sanitized.FORCE_COLOR;
  }
  return sanitized;
}

function isColorDisablingForceColor(value) {
  return typeof value === "string" && ["0", "false"].includes(value.trim().toLowerCase());
}
