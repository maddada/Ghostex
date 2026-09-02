#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

/*
 CDXC:ReleaseAutomation 2026-09-02-12:40:
 Release 8.5.0's first Actions run failed 21 minutes into the Android job because
 `bun run build:mobile-chat` could not load `esbuild`. The local preflight never
 ran that script, so nothing before dispatch could have caught it. This stage runs
 every pure JS/TS build step the release pipeline executes on CI, locally, in the
 order that fails fastest. A broken script now costs seconds here instead of a
 recovery run.

 The command list is deliberately explicit rather than discovered: each entry
 names the workflow and shell script that invoke it on CI, so anyone changing a
 release script can audit the drift by hand. Excluded on purpose: cargo, Zig,
 Gradle, signing, notarisation, code-server packaging, component publishing, and
 anything that fetches large artifacts or needs release secrets. `bun run
 typecheck` and `bun run release:test` are separate preflight stages already.
 `build:mobile-find`, `web:build`, and the editor build are not run by any
 release workflow, so they are not here either.

 Two of the steps write TRACKED files: build:sidebar-css regenerates
 packages/core-ui/styles/shadcn.generated.css and build:mobile-chat regenerates
 the committed WebView assets inside the apps/mobile/app submodule. Both builds
 are deterministic (verified by rebuilding twice), so on a release-ready tree they
 are no-ops on disk. Every tracked output is snapshotted before its command runs;
 if the rebuild changes any of them, the previous bytes are put back so the
 worktree is left exactly as found, and the step fails telling the operator to
 regenerate and commit. That is a real release finding - the committed generated
 output no longer matches its sources - not a side effect to hide.
*/

const repoRoot = path.resolve(new URL('..', import.meta.url).pathname);

export const RELEASE_BUILD_SCRIPTS = Object.freeze([
  {
    // CI: release-gpui-android.yml / release-build-android.yml
    //   -> tooling/release-gpui/android.sh -> tooling/release-mobile/android.sh
    //   (also release-mobile-ios-testflight.yml -> tooling/release-mobile/ios-testflight.sh)
    caller: 'release-gpui-android.yml -> tooling/release-mobile/android.sh',
    command: 'bun run build:mobile-chat',
    timeoutMs: 5 * 60 * 1000,
    trackedOutputs: [
      'apps/mobile/app/assets/webview/session-chat',
      'apps/mobile/app/src/chat/session-chat-agents.generated.ts',
      'apps/mobile/app/src/chat/session-chat-html.generated.ts',
    ],
  },
  {
    // CI: release-gpui-macos.yml -> tooling/release-gpui/macos.sh -> apps/desktop/scripts/build-macos-app.sh
    //     release-gpui-linux.yml -> tooling/release-gpui/linux-stage.sh -> apps/desktop/scripts/build-linux-app.sh
    //     release-gpui-windows.yml -> tooling/release-gpui/windows.ps1 -> apps/desktop/scripts/build-windows-app.ps1
    caller: 'release-gpui-{macos,linux,windows} -> apps/desktop/scripts/build-*-app',
    command: 'bun run build:sidebar-css',
    timeoutMs: 5 * 60 * 1000,
    trackedOutputs: ['packages/core-ui/styles/shadcn.generated.css'],
  },
  {
    // CI: same three desktop build scripts as build:sidebar-css, immediately after
    // it (Windows runs node_modules/.bin/vite.exe with the same arguments).
    // Writes apps/desktop/dist/sidebar, which is gitignored. Also exercises
    // tooling/shiki-classic-assets.mjs through the stageShikiChatRuntime plugin.
    caller: 'release-gpui-{macos,linux,windows} -> apps/desktop/scripts/build-*-app',
    command: 'bunx vite build --config apps/desktop/vite.config.ts',
    timeoutMs: 10 * 60 * 1000,
    trackedOutputs: [],
  },
]);

function runCommand(command, { timeoutMs, cwd = repoRoot } = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, {
      cwd,
      env: process.env,
      shell: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let output = '';
    let settled = false;
    const timeout = setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      child.kill('SIGTERM');
      resolve({ code: 124, output: `${output}\n(timed out after ${timeoutMs}ms)` });
    }, timeoutMs);
    child.stdout.on('data', (chunk) => {
      output += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      output += chunk.toString();
    });
    child.on('error', (error) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolve({ code: 127, output: `${output}\n${String(error.message ?? error)}` });
    });
    child.on('close', (code) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timeout);
      resolve({ code: code ?? 1, output });
    });
  });
}

function listFiles(target) {
  if (!existsSync(target)) {
    return [];
  }
  if (statSync(target).isFile()) {
    return [target];
  }
  const files = [];
  for (const entry of readdirSync(target, { withFileTypes: true })) {
    const entryPath = path.join(target, entry.name);
    if (entry.isDirectory()) {
      files.push(...listFiles(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files.sort();
}

/*
 Map of absolute file path -> { content, digest } for every file under the
 tracked output paths. Missing paths are recorded as absent so a build that
 creates or deletes a tracked file is also reported as drift.
*/
function snapshotTrackedOutputs(trackedOutputs) {
  const snapshot = new Map();
  for (const relative of trackedOutputs) {
    const absolute = path.join(repoRoot, relative);
    for (const file of listFiles(absolute)) {
      const content = readFileSync(file);
      snapshot.set(file, { content, digest: createHash('sha256').update(content).digest('hex') });
    }
  }
  return snapshot;
}

function restoreTrackedOutputs(trackedOutputs, before) {
  for (const relative of trackedOutputs) {
    const absolute = path.join(repoRoot, relative);
    for (const file of listFiles(absolute)) {
      if (!before.has(file)) {
        rmSync(file, { force: true });
      }
    }
  }
  for (const [file, { content }] of before) {
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, content);
  }
}

function trackedOutputDrift(trackedOutputs, before) {
  const after = snapshotTrackedOutputs(trackedOutputs);
  const drifted = [];
  for (const [file, { digest }] of before) {
    const current = after.get(file);
    if (!current) {
      drifted.push(`${path.relative(repoRoot, file)} (deleted)`);
    } else if (current.digest !== digest) {
      drifted.push(path.relative(repoRoot, file));
    }
  }
  for (const file of after.keys()) {
    if (!before.has(file)) {
      drifted.push(`${path.relative(repoRoot, file)} (created)`);
    }
  }
  return drifted;
}

function outputTail(text, lines = 20) {
  return String(text ?? '')
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter(Boolean)
    .slice(-lines)
    .join('\n');
}

export function formatDuration(durationMs) {
  const seconds = durationMs / 1000;
  if (seconds < 60) {
    return `${seconds.toFixed(1)}s`;
  }
  return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
}

/*
 Runs every entry sequentially and stops at the first failure. Returns
 `{ results, failure }` where each result is
 `{ caller, command, durationMs, ok, detail }` and `failure` is the failing
 result (or null). `onResult` is called after each command so callers can stream
 per-command timings; `scripts` defaults to the full RELEASE_BUILD_SCRIPTS list.
*/
export async function runReleaseBuildScripts({ onResult, scripts = RELEASE_BUILD_SCRIPTS } = {}) {
  const results = [];
  for (const entry of scripts) {
    const startedAt = Date.now();
    const before = snapshotTrackedOutputs(entry.trackedOutputs);
    const run = await runCommand(entry.command, { timeoutMs: entry.timeoutMs });
    let result;
    if (run.code !== 0) {
      restoreTrackedOutputs(entry.trackedOutputs, before);
      result = {
        detail: `exit ${run.code}\n${outputTail(run.output)}`,
        ok: false,
      };
    } else {
      const drifted = trackedOutputDrift(entry.trackedOutputs, before);
      if (drifted.length > 0) {
        restoreTrackedOutputs(entry.trackedOutputs, before);
        result = {
          detail: `regenerated tracked output differs from the checked-in copy (previous bytes restored): ${drifted.join(', ')}. Run \`${entry.command}\` and commit the result${entry.trackedOutputs.some((target) => target.startsWith('apps/mobile/app/')) ? ' inside apps/mobile/app, then bump the submodule pointer' : ''}.`,
          ok: false,
        };
      } else {
        result = { detail: 'ok', ok: true };
      }
    }
    const finished = { caller: entry.caller, command: entry.command, durationMs: Date.now() - startedAt, ...result };
    results.push(finished);
    onResult?.(finished);
    if (!finished.ok) {
      return { failure: finished, results };
    }
  }
  return { failure: null, results };
}

async function main() {
  process.chdir(repoRoot);
  const startedAt = Date.now();
  console.log(`Running ${RELEASE_BUILD_SCRIPTS.length} release JS build script(s) locally...`);
  const { failure } = await runReleaseBuildScripts({
    onResult: (result) => {
      console.log(
        `${result.ok ? 'PASS' : 'FAIL'}  ${result.command.padEnd(52)} ${formatDuration(result.durationMs).padStart(8)}  (${result.caller})`
      );
    },
  });
  if (failure) {
    console.error(`\nRelease build script FAILED: ${failure.command}\n${failure.detail}`);
    process.exitCode = 1;
    return;
  }
  console.log(`\nAll release build scripts passed in ${formatDuration(Date.now() - startedAt)}.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
