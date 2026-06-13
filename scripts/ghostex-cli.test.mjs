import { describe, expect, test } from "vitest";
import { execFile } from "node:child_process";
import { EventEmitter } from "node:events";
import { chmod, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import http from "node:http";
import net from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import {
  agentOrchestrationUsage,
  applyZehnAcceptAllArgs,
  browserUsage,
  buildSessionPickerModel,
  buildSessionPickerRows,
  buildSessionAttachCommand,
  computerUseUsage,
  createCliSshForwardPlan,
  fetchGxserverSessionList,
  formatCompactSessionLine,
  generateTitleUsage,
  groupSessionsPreservingSidebarOrder,
  isFailedCliResult,
  manageBeadsUsage,
  moveSessionPickerSelection,
  parseArgs,
  parseCreateSession,
  parseEditPaths,
  parseOpenPaths,
  parseQuickTerminal,
  parseRename,
  parseVsCodePathPosition,
  readAndroidReadinessSettings,
  requestGxserverRpc,
  resolveBundledBeadsLaunchFromRoot,
  resolveGxserverServerTarget,
  resolveGhostexTuiLaunchFromRoot,
  resolveListedSessions,
  resolveZehnLaunchFromRoot,
  sendGxserverCliAction,
  serverUsage,
  usage,
} from "./ghostex-cli.mjs";

const execFileAsync = promisify(execFile);

function strictAndroidReleaseEnv(overrides = {}) {
  return {
    PATH: process.env.PATH,
    HOME: process.env.HOME,
    GHOSTEX_ANDROID_REQUIRE_RELEASE_SIGNING: "1",
    GHOSTEX_ANDROID_SIGNING_STORE_FILE: "/tmp/ghostex-android-missing-release.jks",
    GHOSTEX_ANDROID_SIGNING_STORE_PASSWORD: "store-password",
    GHOSTEX_ANDROID_SIGNING_KEY_ALIAS: "ghostex-release",
    GHOSTEX_ANDROID_SIGNING_KEY_PASSWORD: "key-password",
    GHOSTEX_ANDROID_HOST: "mac.tailnet.test",
    GHOSTEX_ANDROID_USER: "madda",
    GHOSTEX_ANDROID_CONFIRM_CLEAR_DATA: "1",
    ...overrides,
  };
}

async function withGxserverFixture(callback, options = {}) {
  const body = options.body ?? {
    ok: true,
    product: "gxserver",
    protocolVersion: 1,
    requestId: "fixture-request",
    result: { sessions: [] },
  };
  const server = http.createServer(async (request, response) => {
    expect(request.headers.authorization).toBe("Bearer test-token");
    expect(request.headers["x-gxserver-protocol-version"]).toBe("1");
    const chunks = [];
    for await (const chunk of request) {
      chunks.push(chunk);
    }
    const requestBody = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    expect(requestBody.protocolVersion).toBe(1);
    response.writeHead(options.status ?? 200, { "content-type": "application/json" });
    response.end(JSON.stringify(body));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  try {
    await callback({ baseUrl: `http://127.0.0.1:${address.port}` });
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

function createGxserverRpcAndHealthFixture({ serverId }) {
  return http.createServer(async (request, response) => {
    expect(request.headers.authorization).toBe("Bearer test-token");
    expect(request.headers["x-gxserver-protocol-version"]).toBe("1");
    if (request.method === "GET" && request.url?.startsWith("/api/health/server")) {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(
        JSON.stringify({
          ok: true,
          product: "gxserver",
          protocolVersion: 1,
          serverId,
          state: "running",
        }),
      );
      return;
    }
    const chunks = [];
    for await (const chunk of request) {
      chunks.push(chunk);
    }
    const requestBody = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    expect(requestBody.protocolVersion).toBe(1);
    response.writeHead(200, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        ok: true,
        product: "gxserver",
        protocolVersion: 1,
        requestId: "fixture-request",
        result: { sessions: [] },
      }),
    );
  });
}

async function reserveTestPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (address && typeof address === "object") {
          resolve(address.port);
          return;
        }
        reject(new Error("Expected test server to reserve a TCP port."));
      });
    });
  });
}

async function isTestPortAvailable(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.listen(port, "127.0.0.1", () => {
      server.close(() => resolve(true));
    });
  });
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe("ghostex CLI Android remote-session contract", () => {
  test("runs main when invoked through a symlinked cli script", async () => {
    /**
     * CDXC:CliEntrypoint 2026-05-18-01:17:
     * Android SSH uses the installed `ghostex` wrapper on the Mac. In local
     * development that wrapper may execute a symlinked `ghostex-cli.mjs`; keep
     * the direct-entrypoint guard symlink-aware so JSON commands do not exit
     * zero with empty stdout.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-cli-symlink-"));
    try {
      const linkPath = path.join(tempDir, "ghostex-cli.mjs");
      await symlink(path.resolve("scripts/ghostex-cli.mjs"), linkPath);
      const helpResult = await execFileAsync(process.execPath, [linkPath, "help"]);
      const shortHelpResult = await execFileAsync(process.execPath, [linkPath, "h"]);

      expect(helpResult.stdout).toContain("Usage:");
      expect(helpResult.stdout).toContain("sessions | s | ls [--ungrouped|-u] [--json]");
      expect(shortHelpResult.stdout).toBe(helpResult.stdout);
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("parses Android action flag form", () => {
    const { flags, rest } = parseArgs(["--session-id", "session-1", "--json"]);

    expect(rest).toEqual([]);
    expect(flags.sessionId).toBe("session-1");
    expect(flags.json).toBe(true);
  });

  test("parses Android rename-session flag form", () => {
    const { flags, rest } = parseArgs([
      "--session-id",
      "session-1",
      "--title=Ship Android's polish",
      "--json",
    ]);

    expect(rest).toEqual([]);
    expect(parseRename(rest, flags)).toMatchObject({
      sessionId: "session-1",
      title: "Ship Android's polish",
    });
    expect(flags.json).toBe(true);
  });

  test("parses Android create-session project and group flags", () => {
    /**
     * CDXC:AndroidRemoteSessions 2026-05-18-02:31:
     * Android's sidebar plus button must create the terminal in the tapped Mac
     * project/group through the Ghostex CLI, not whichever project happens to
     * be active on the Mac.
     */
    const { flags, rest } = parseArgs([
      "--project-id",
      "project-1",
      "--group-id",
      "group-main",
      "--json",
    ]);

    expect(parseCreateSession(rest, flags)).toMatchObject({
      groupId: "group-main",
      projectId: "project-1",
    });
  });

  test("keeps positional rename-session form for human CLI usage", () => {
    const { flags, rest } = parseArgs(["session-1", "Ship", "Android"]);

    expect(parseRename(rest, flags)).toMatchObject({
      sessionId: "session-1",
      title: "Ship Android",
    });
  });

  test("documents bare ghostex and gx commands as the terminal TUI", () => {
    const help = usage();

    expect(help).toContain("Running ghostex or gx with no subcommand opens the Ghostex terminal TUI");
    expect(help).toContain("browser --help");
    expect(help).not.toContain("browser-devtools-mcp [--port n]");
    expect(help).toContain("top switch button for project/session switching");
    expect(help).toContain("Direct attach stays available through attach/a/resume/r without opening the TUI");
    expect(help).toContain("find | f [zehn args...]");
    expect(help).toContain("gx find and gx f launch bundled zehn");
    expect(help).not.toContain("search | find");
    expect(help).toMatch(/^\s+ghostex$/m);
    expect(help).toMatch(/^\s+gx$/m);
  });

  test("documents gx server commands in top-level and server help", () => {
    /**
     * CDXC:GxserverCli 2026-06-02-18:36:
     * The user-facing `gx`/`ghostex` help must expose gxserver lifecycle
     * commands through the `server` namespace so normal users can manage the
     * background process without switching to the internal daemon command name.
     */
    const help = usage();
    const serverHelp = serverUsage();

    expect(help).toContain("Server:");
    expect(help).toContain("server start [--json]");
    expect(help).toContain("server stop [--json]");
    expect(help).toContain("server stop-all [--json]");
    expect(help).toContain("server status [--json]");
    expect(help).toContain("server --help");
    expect(serverHelp).toContain("Ghostex Server - manage the gxserver background process");
    expect(serverHelp).toContain("gx server <command> [args...] [--flags]");
    expect(serverHelp).toContain("server version");
    expect(serverHelp).toContain("server --version");
    expect(serverHelp).toContain("gx server stop stops only the control plane");
    expect(serverHelp).toContain("gx server stop-all is destructive");
  });

  test("forwards gx server subcommands to the gxserver CLI", async () => {
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-gxserver-cli-"));
    const markerPath = path.join(tempDir, "argv.txt");
    const gxserverCliPath = path.join(tempDir, "gxserver");
    try {
      await writeFile(
        gxserverCliPath,
        `#!/bin/sh
printf '%s\\n' "$@" > ${JSON.stringify(markerPath)}
printf 'forwarded:%s\\n' "$1"
`,
      );
      await chmod(gxserverCliPath, 0o755);

      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "server",
        "status",
        "--json",
      ], {
        env: {
          ...process.env,
          GHOSTEX_GXSERVER_CLI: gxserverCliPath,
        },
      });

      expect(result.stderr).toBe("");
      expect(result.stdout).toContain("forwarded:status");
      expect((await readFile(markerPath, "utf8")).trim().split("\n")).toEqual(["status", "--json"]);
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("resolves bundled zehn from the pinned submodule output", async () => {
    /**
     * CDXC:AgentHistorySearch 2026-05-29-12:27:
     * Ghostex prompt-history search should launch the pinned zehn checkout or
     * bundled Web/bin copy, not a random PATH install. `gx s` stays reserved for
     * sessions, and `gx search` is intentionally not a zehn alias.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-zehn-"));
    try {
      const zehnBin = path.join(tempDir, "zehn", "zig-out", "bin", "zehn");
      await mkdir(path.dirname(zehnBin), { recursive: true });
      await writeFile(zehnBin, "#!/bin/sh\n");

      expect(resolveZehnLaunchFromRoot(tempDir)).toMatchObject({
        args: [],
        command: zehnBin,
      });
      expect(usage()).not.toContain("search |");
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("resolves bundled Ghostex TUI from the app resource bin directory", async () => {
    /**
     * CDXC:GhostexTui 2026-06-07-12:13:
     * Installed `gx` should launch the packaged Ghostex TUI from Web/bin even
     * when the user runs the CLI outside a Ghostex source checkout.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-tui-"));
    try {
      const tuiBin = path.join(tempDir, "bin", "ghostex-tui");
      await mkdir(path.dirname(tuiBin), { recursive: true });
      await writeFile(tuiBin, "#!/bin/sh\n");

      expect(resolveGhostexTuiLaunchFromRoot(tempDir)).toMatchObject({
        args: [],
        command: tuiBin,
      });
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("gx find passes Accept All to zehn from gxserver settings unless user overrides it", () => {
    /**
     * CDXC:AgentHistorySearch 2026-06-04-23:31:
     * Ghostex-owned `gx find` should make Enter resume match the gxserver
     * global Accept All policy while preserving explicit zehn CLI flags.
     */
    expect(applyZehnAcceptAllArgs(["--agent", "codex"], true)).toEqual([
      "--accept-all",
      "--agent",
      "codex",
    ]);
    expect(applyZehnAcceptAllArgs(["--agent", "codex"], false)).toEqual(["--agent", "codex"]);
    expect(applyZehnAcceptAllArgs(["--no-accept-all", "--agent", "codex"], true)).toEqual([
      "--no-accept-all",
      "--agent",
      "codex",
    ]);
    expect(applyZehnAcceptAllArgs(["--accept-all", "--agent", "codex"], true)).toEqual([
      "--accept-all",
      "--agent",
      "codex",
    ]);
  });

  test("parses OS integration path open commands", () => {
    /**
     * CDXC:OSIntegration 2026-05-27-18:06:
     * Open/edit/terminal CLI commands are the public macOS integration surface
     * behind Finder, Open With, and EDITOR-style workflows.
     */
    expect(parseOpenPaths(["./docs/os-integration-prd.md"], {})).toMatchObject({
      mode: "open",
      targets: [{ line: undefined, path: path.resolve("./docs/os-integration-prd.md") }],
    });
    expect(parseEditPaths([], { wait: "src/app.ts:12:3" })).toMatchObject({
      mode: "edit",
      targets: [{ column: 3, line: 12, path: path.resolve("src/app.ts") }],
      wait: true,
    });
    expect(parseEditPaths([], { goto: "src/app.ts:12:3", wait: true })).toMatchObject({
      targets: [{ column: 3, line: 12, path: path.resolve("src/app.ts") }],
      wait: true,
    });
    expect(parseQuickTerminal(["echo", "hi"], { cwd: "/tmp", title: "Scratch" })).toEqual({
      command: "echo hi",
      cwd: "/tmp",
      title: "Scratch",
    });
    expect(parseVsCodePathPosition("file.ts:12:3")).toEqual({
      column: 3,
      line: 12,
      path: "file.ts",
    });
  });

  test("keeps floating Monaco prompt editor on the native app bridge", async () => {
    /**
     * CDXC:PromptEditor 2026-05-31-10:24:
     * Ctrl+G Monaco prompt editing is an EDITOR-facing macOS overlay command.
     * Until gxserver owns a blocking save/cancel endpoint, the CLI must keep
     * sending openFloatingEditor over the native bridge instead of rejecting the
     * command during the gxserver cutover.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-fme-test-"));
    const homeDir = path.join(tempDir, "home");
    const editFile = path.join(tempDir, "prompt.md");
    const receivedMessages = [];
    const server = net.createServer((socket) => {
      let buffer = "";
      socket.setEncoding("utf8");
      socket.on("data", async (chunk) => {
        buffer += chunk;
        let newlineIndex = buffer.indexOf("\n");
        while (newlineIndex >= 0) {
          const line = buffer.slice(0, newlineIndex);
          buffer = buffer.slice(newlineIndex + 1);
          if (line) {
            const message = JSON.parse(line);
            receivedMessages.push(message);
            await writeFile(message.statusFile, "saved\n");
          }
          newlineIndex = buffer.indexOf("\n");
        }
      });
    });
    await mkdir(path.join(homeDir, "cli"), { recursive: true });
    await writeFile(path.join(homeDir, "cli", "bridge-token"), "test-token\n");
    await writeFile(editFile, "prompt text\n");
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "floating-monaco-editor",
        editFile,
        "--port",
        String(address.port),
        "--timeout-ms",
        "1000",
        "--exit-timeout-ms",
        "1000",
      ], {
        env: {
          ...process.env,
          GHOSTEX_HOME: homeDir,
        },
      });

      expect(result.stderr).toBe("");
      expect(receivedMessages).toHaveLength(1);
      expect(receivedMessages[0]).toMatchObject({
        authToken: "test-token",
        editorKind: "monaco",
        filePath: editFile,
        language: "markdown",
        title: "Prompt Editor",
        type: "openFloatingEditor",
      });
    } finally {
      await new Promise((resolve) => server.close(resolve));
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("prompt-editor uses floating Monaco for macOS app Monaco sessions", async () => {
    /**
     * CDXC:PromptEditor 2026-05-31-11:58:
     * The stable EDITOR wrapper must keep macOS app Ctrl+G on the Monaco
     * overlay when Settings selects Monaco. The wrapper chooses this only from
     * native app runtime markers, not from the setting alone.
     *
     * CDXC:PromptEditor 2026-06-09-21:50:
     * Monaco prompt-editor return focus must prefer the current gxserver S:P:G
     * ref over stale inherited GHOSTEX_NATIVE_SESSION_ID and send native the
     * derived P:G id.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-prompt-editor-macos-"));
    const homeDir = path.join(tempDir, "home");
    const editFile = path.join(tempDir, "prompt.md");
    const receivedMessages = [];
    const server = net.createServer((socket) => {
      let buffer = "";
      socket.setEncoding("utf8");
      socket.on("data", async (chunk) => {
        buffer += chunk;
        let newlineIndex = buffer.indexOf("\n");
        while (newlineIndex >= 0) {
          const line = buffer.slice(0, newlineIndex);
          buffer = buffer.slice(newlineIndex + 1);
          if (line) {
            const message = JSON.parse(line);
            receivedMessages.push(message);
            await writeFile(message.statusFile, "saved\n");
          }
          newlineIndex = buffer.indexOf("\n");
        }
      });
    });
    await mkdir(path.join(homeDir, "cli"), { recursive: true });
    await writeFile(path.join(homeDir, "cli", "bridge-token"), "test-token\n");
    await writeFile(editFile, "prompt text\n");
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "prompt-editor",
        editFile,
        "--port",
        String(address.port),
        "--timeout-ms",
        "1000",
        "--exit-timeout-ms",
        "1000",
      ], {
        env: {
          ...process.env,
          GHOSTEX_HOME: homeDir,
          GHOSTEX_GLOBAL_SESSION_REF: "S1a:P3a91:G8v20",
          GHOSTEX_NATIVE_SESSION_ID: "P3a91:G0000",
          GHOSTEX_PROMPT_EDITOR_CLIENT: "macos-app",
          GHOSTEX_PROMPT_EDITOR_BACKEND: "monaco",
          ZMX_SESSION: "",
        },
      });

      expect(result.stderr).toBe("");
      expect(receivedMessages).toHaveLength(1);
      expect(receivedMessages[0]).toMatchObject({
        editorKind: "monaco",
        filePath: editFile,
        originatingSessionId: "P3a91:G8v20",
        type: "openFloatingEditor",
      });
    } finally {
      await new Promise((resolve) => server.close(resolve));
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("prompt-editor routes Monaco settings to gte without macOS app context", async () => {
    /**
     * CDXC:PromptEditor 2026-05-31-11:58:
     * Android, iOS, CLI, TUI, and plain SSH attaches do not have the native app
     * prompt-editor marker. In those contexts the wrapper must invoke gte even
     * when the inherited prompt editor backend says Monaco.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-prompt-editor-gte-"));
    const binDir = path.join(tempDir, "bin");
    const editFile = path.join(tempDir, "prompt.md");
    const markerFile = path.join(tempDir, "gte-args.txt");
    const gtePath = path.join(binDir, "gte");
    try {
      await mkdir(binDir, { recursive: true });
      await writeFile(editFile, "prompt text\n");
      await writeFile(
        gtePath,
        `#!/bin/sh
printf '%s\\n' "$@" > ${JSON.stringify(markerFile)}
`,
      );
      await chmod(gtePath, 0o755);

      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "prompt-editor",
        editFile,
      ], {
        env: {
          ...process.env,
          GHOSTEX_NATIVE_SESSION_ID: "",
          GHOSTEX_PROMPT_EDITOR_CLIENT: "",
          GHOSTEX_PROMPT_EDITOR_BACKEND: "monaco",
          PATH: `${binDir}${path.delimiter}${process.env.PATH}`,
          ZMX_SESSION: "",
        },
      });

      expect(result.stderr).toBe("");
      expect((await readFile(markerFile, "utf8")).trim()).toBe(editFile);
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("prompt-editor routes stale macOS Monaco zmx sessions to gte without attach capability", async () => {
    /**
     * CDXC:PromptEditor 2026-06-06-16:40:
     * Reattached zmx sessions can inherit macOS app prompt-editor environment
     * from the shell that created the session. The prompt-editor wrapper must
     * trust zmx's current leader capability instead so SSH, TUI, and mobile
     * attaches use gte even when the old environment still says macos-app.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-prompt-editor-zmx-gte-"));
    const binDir = path.join(tempDir, "bin");
    const editFile = path.join(tempDir, "prompt.md");
    const markerFile = path.join(tempDir, "gte-args.txt");
    const gtePath = path.join(binDir, "gte");
    const zmxPath = path.join(binDir, "zmx");
    try {
      await mkdir(binDir, { recursive: true });
      await writeFile(editFile, "prompt text\n");
      await writeFile(
        gtePath,
        `#!/bin/sh
printf '%s\\n' "$@" > ${JSON.stringify(markerFile)}
`,
      );
      await writeFile(
        zmxPath,
        `#!/bin/sh
if [ "$1" = "prompt-editor-capability" ]; then
  printf '%s\\n' gte
fi
`,
      );
      await chmod(gtePath, 0o755);
      await chmod(zmxPath, 0o755);

      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "prompt-editor",
        editFile,
      ], {
        env: {
          ...process.env,
          GHOSTEX_PROMPT_EDITOR_CLIENT: "macos-app",
          GHOSTEX_PROMPT_EDITOR_BACKEND: "monaco",
          GHOSTEX_ZMX_BIN: zmxPath,
          PATH: `${binDir}${path.delimiter}${process.env.PATH}`,
          ZMX_SESSION: "shared-session",
        },
      });

      expect(result.stderr).toBe("");
      expect((await readFile(markerFile, "utf8")).trim()).toBe(editFile);
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("prompt-editor uses explicit bundled zmx when zmx leader advertises Monaco capability", async () => {
    /**
     * CDXC:PromptEditor 2026-06-07-08:09:
     * The prompt-editor wrapper must query GHOSTEX_ZMX_BIN instead of PATH so a
     * stale Homebrew zmx cannot hide the current desktop attach client's
     * Monaco capability.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-prompt-editor-zmx-monaco-"));
    const homeDir = path.join(tempDir, "home");
    const binDir = path.join(tempDir, "bin");
    const editFile = path.join(tempDir, "prompt.md");
    const receivedMessages = [];
    const pathZmxPath = path.join(binDir, "zmx");
    const bundledZmxPath = path.join(tempDir, "bundled-zmx");
    const server = net.createServer((socket) => {
      let buffer = "";
      socket.setEncoding("utf8");
      socket.on("data", async (chunk) => {
        buffer += chunk;
        let newlineIndex = buffer.indexOf("\n");
        while (newlineIndex >= 0) {
          const line = buffer.slice(0, newlineIndex);
          buffer = buffer.slice(newlineIndex + 1);
          if (line) {
            const message = JSON.parse(line);
            receivedMessages.push(message);
            await writeFile(message.statusFile, "saved\n");
          }
          newlineIndex = buffer.indexOf("\n");
        }
      });
    });
    try {
      await mkdir(path.join(homeDir, "cli"), { recursive: true });
      await mkdir(binDir, { recursive: true });
      await writeFile(path.join(homeDir, "cli", "bridge-token"), "test-token\n");
      await writeFile(editFile, "prompt text\n");
      await writeFile(
        pathZmxPath,
        `#!/bin/sh
if [ "$1" = "prompt-editor-capability" ]; then
  printf '%s\\n' gte
fi
`,
      );
      await writeFile(
        bundledZmxPath,
        `#!/bin/sh
if [ "$1" = "prompt-editor-capability" ]; then
  printf '%s\\n' monaco
fi
`,
      );
      await chmod(pathZmxPath, 0o755);
      await chmod(bundledZmxPath, 0o755);
      await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
      const address = server.address();

      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "prompt-editor",
        editFile,
        "--port",
        String(address.port),
        "--timeout-ms",
        "1000",
        "--exit-timeout-ms",
        "1000",
      ], {
        env: {
          ...process.env,
          GHOSTEX_HOME: homeDir,
          GHOSTEX_PROMPT_EDITOR_CLIENT: "",
          GHOSTEX_PROMPT_EDITOR_BACKEND: "monaco",
          GHOSTEX_ZMX_BIN: bundledZmxPath,
          PATH: `${binDir}${path.delimiter}${process.env.PATH}`,
          ZMX_SESSION: "shared-session",
        },
      });

      expect(result.stderr).toBe("");
      expect(receivedMessages).toHaveLength(1);
      expect(receivedMessages[0]).toMatchObject({
        editorKind: "monaco",
        filePath: editFile,
        type: "openFloatingEditor",
      });
    } finally {
      await new Promise((resolve) => server.close(resolve));
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("prompt-editor ignores PATH zmx when explicit bundled zmx is missing", async () => {
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-prompt-editor-zmx-no-bin-"));
    const binDir = path.join(tempDir, "bin");
    const editFile = path.join(tempDir, "prompt.md");
    const markerFile = path.join(tempDir, "gte-args.txt");
    const gtePath = path.join(binDir, "gte");
    const pathZmxPath = path.join(binDir, "zmx");
    try {
      await mkdir(binDir, { recursive: true });
      await writeFile(editFile, "prompt text\n");
      await writeFile(
        gtePath,
        `#!/bin/sh
printf '%s\\n' "$@" > ${JSON.stringify(markerFile)}
`,
      );
      await writeFile(
        pathZmxPath,
        `#!/bin/sh
if [ "$1" = "prompt-editor-capability" ]; then
  printf '%s\\n' monaco
fi
`,
      );
      await chmod(gtePath, 0o755);
      await chmod(pathZmxPath, 0o755);

      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "prompt-editor",
        editFile,
      ], {
        env: {
          ...process.env,
          GHOSTEX_PROMPT_EDITOR_CLIENT: "macos-app",
          GHOSTEX_PROMPT_EDITOR_BACKEND: "monaco",
          GHOSTEX_ZMX_BIN: "",
          PATH: `${binDir}${path.delimiter}${process.env.PATH}`,
          ZMX_SESSION: "shared-session",
        },
      });

      expect(result.stderr).toBe("");
      expect((await readFile(markerFile, "utf8")).trim()).toBe(editFile);
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("prompt-editor keeps explicit gte on gte", async () => {
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-prompt-editor-explicit-gte-"));
    const binDir = path.join(tempDir, "bin");
    const editFile = path.join(tempDir, "prompt.md");
    const markerFile = path.join(tempDir, "gte-args.txt");
    const gtePath = path.join(binDir, "gte");
    try {
      await mkdir(binDir, { recursive: true });
      await writeFile(editFile, "prompt text\n");
      await writeFile(
        gtePath,
        `#!/bin/sh
printf '%s\\n' "$@" > ${JSON.stringify(markerFile)}
`,
      );
      await chmod(gtePath, 0o755);

      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "prompt-editor",
        editFile,
      ], {
        env: {
          ...process.env,
          GHOSTEX_PROMPT_EDITOR_CLIENT: "macos-app",
          GHOSTEX_PROMPT_EDITOR_BACKEND: "gte",
          PATH: `${binDir}${path.delimiter}${process.env.PATH}`,
          ZMX_SESSION: "",
        },
      });

      expect(result.stderr).toBe("");
      expect((await readFile(markerFile, "utf8")).trim()).toBe(editFile);
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("installs the Ghostex Browser Use skill for agents", async () => {
    /**
     * CDXC:BrowserAgentControl 2026-05-26-22:17:
     * The first-launch CLI command installs the Ghostex Browser Use skill into
     * the agent skill directory, so the CLI needs a deterministic copy command
     * that works from the source checkout and the bundled app resource path.
     *
     * CDXC:BrowserAgentControl 2026-05-27-06:58:
     * The installed skill id is `$ghostex-browser-use`; the legacy
     * `$ghostex-browser-devtools-mcp` name caused duplicate Codex discovery
     * when a shared installed skill and repo skill were both present.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-browser-skill-"));
    try {
      const targetDir = path.join(tempDir, "ghostex-browser-use");
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "browser",
        "install-skill",
        "--target-dir",
        targetDir,
        "--json",
      ]);
      const payload = JSON.parse(result.stdout);
      const skillMarkdown = await readFile(path.join(targetDir, "SKILL.md"), "utf8");

      expect(payload).toMatchObject({
        command: "ghostex browser mcp",
        ok: true,
        skill: "ghostex-browser-use",
        targetDir,
      });
      expect(skillMarkdown).toContain("# ghostex-browser-use");
      expect(skillMarkdown).toContain("ghostex_console_logs");
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("installs the Ghostex Computer Use skill for agents", async () => {
    /**
     * CDXC:ComputerAgentControl 2026-05-27-06:58:
     * Desktop Control setup installs `$ghostex-computer-use` as a wrapper over
     * `$cua-driver` so users can ask for Ghostex computer use without knowing
     * the lower-level skill name.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-computer-use-skill-"));
    try {
      const targetDir = path.join(tempDir, "ghostex-computer-use");
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "computer-use",
        "install-skill",
        "--target-dir",
        targetDir,
        "--json",
      ]);
      const payload = JSON.parse(result.stdout);
      const skillMarkdown = await readFile(path.join(targetDir, "SKILL.md"), "utf8");

      expect(payload).toMatchObject({
        command: "cua-driver",
        ok: true,
        skill: "ghostex-computer-use",
        targetDir,
      });
      expect(skillMarkdown).toContain("# ghostex-computer-use");
      expect(skillMarkdown).toContain("$cua-driver");
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("installs the Ghostex Agent Orchestration skill for agents", async () => {
    /**
     * CDXC:AgentOrchestration 2026-05-27-07:15:
     * Agents need `$ghostex-agent-orchestration` installed so they can discover
     * Ghostex CLI commands for creating panes, messaging sessions, checking
     * status, and reading last lines through `ghostex read-text`.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-agent-orchestration-skill-"));
    try {
      const targetDir = path.join(tempDir, "ghostex-agent-orchestration");
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "agent-orchestration",
        "install-skill",
        "--target-dir",
        targetDir,
        "--json",
      ]);
      const payload = JSON.parse(result.stdout);
      const skillMarkdown = await readFile(path.join(targetDir, "SKILL.md"), "utf8");

      expect(payload).toMatchObject({
        command: "ghostex --help",
        ok: true,
        skill: "ghostex-agent-orchestration",
        targetDir,
      });
      expect(skillMarkdown).toContain("# ghostex-agent-orchestration");
      expect(skillMarkdown).toContain("ghostex --help");
      expect(skillMarkdown).toContain("ghostex read-text <selector> --lines 80 --json");
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("installs the Ghostex Generate Title skill for agents", async () => {
    /**
     * CDXC:GenerateTitleSkill 2026-05-27-07:28:
     * `$ghostex-generate-title` replaces the personal title skill with a
     * Ghostex workflow: title under 47 characters, then submit `/rename <title>`
     * in the current session.
     *
     * CDXC:GenerateTitleSkill 2026-06-09-17:49:
     * The installed skill must use `rename-command` so generated titles submit
     * through the native Enter bridge used by Delayed Send.
     *
     * CDXC:GenerateTitleSkill 2026-06-12-04:10:
     * zmx terminals may only expose GHOSTEX_GLOBAL_SESSION_REF or ZMX_SESSION.
     * The installed skill must prefer exact self-session selectors before
     * giving up, and must not guess by title or alias.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-generate-title-skill-"));
    try {
      const targetDir = path.join(tempDir, "ghostex-generate-title");
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "generate-title",
        "install-skill",
        "--target-dir",
        targetDir,
        "--json",
      ]);
      const payload = JSON.parse(result.stdout);
      const skillMarkdown = await readFile(path.join(targetDir, "SKILL.md"), "utf8");

      expect(payload).toMatchObject({
        ok: true,
        skill: "ghostex-generate-title",
        targetDir,
      });
      expect(payload.command).toContain("ghostex rename-command");
      expect(payload.command).toContain("${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}");
      expect(skillMarkdown).toContain("# ghostex-generate-title");
      expect(skillMarkdown).toContain("under 60 characters");
      expect(skillMarkdown).toContain('ghostex_session_selector="${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}"');
      expect(skillMarkdown).toContain('ghostex rename-command --session-id "$ghostex_session_selector" --title "<generated title>"');
      expect(skillMarkdown).toContain("not guess a session by title, alias, project, or recent activity");
      expect(skillMarkdown).toContain("supported session input path");
      expect(skillMarkdown).not.toContain("Do not press Enter");
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("installs the Ghostex Manage Beads skill for agents", async () => {
    /**
     * CDXC:ProjectBoardBeads 2026-06-04-03:32:
     * Project-board work needs a bundled `$ghostex-manage-beads` skill so
     * agents can discover the `bd` workflow and associate a review bead with
     * the current Ghostex/Codex session without relying on transcript memory.
     */
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-manage-beads-skill-"));
    try {
      const targetDir = path.join(tempDir, "ghostex-manage-beads");
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "manage-beads",
        "install-skill",
        "--target-dir",
        targetDir,
        "--json",
      ]);
      const payload = JSON.parse(result.stdout);
      const skillMarkdown = await readFile(path.join(targetDir, "SKILL.md"), "utf8");
      const skillMetadata = await readFile(path.join(targetDir, "agents", "openai.yaml"), "utf8");

      expect(payload).toMatchObject({
        command: "gx bd --help",
        ok: true,
        skill: "ghostex-manage-beads",
        targetDir,
      });
      expect(skillMarkdown).toContain("# ghostex-manage-beads");
      expect(skillMarkdown).toContain("Associate A Bead With The Current Session");
      expect(skillMarkdown).toContain("codex-thread:$CODEX_THREAD_ID");
      expect(skillMetadata).toContain("allow_implicit_invocation: true");
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("documents browser control under gx browser help", async () => {
    const help = browserUsage();
    const cliHelpResult = await execFileAsync(process.execPath, [
      path.resolve("scripts/ghostex-cli.mjs"),
      "browser",
      "--help",
    ]);

    expect(cliHelpResult.stdout).toBe(`${help}\n`);
    expect(help).toContain("gx browser mcp");
    expect(help).toContain("gx browser open [url] [--project-path path|--project-id id] [--reuse similar|exact|none]");
    expect(help).toContain('args = ["browser", "mcp"]');
    expect(help).toContain("ghostex_console_logs");
    expect(help).toContain("ghostex_snapshot");
    expect(help).toContain("browser install-skill");
    expect(help).toContain("default to the CLI process cwd as --project-path");
    expect(help).toContain("default to --reuse similar");
    expect(help).toContain("keep the returned session id and the MCP page id");
  });

  test("documents Ghostex Computer Use under gx computer-use help", async () => {
    const help = computerUseUsage();
    const cliHelpResult = await execFileAsync(process.execPath, [
      path.resolve("scripts/ghostex-cli.mjs"),
      "computer-use",
      "--help",
    ]);

    expect(cliHelpResult.stdout).toBe(`${help}\n`);
    expect(help).toContain("gx computer-use install-skill");
    expect(help).toContain("$ghostex-computer-use");
    expect(help).toContain("$cua-driver");
  });

  test("documents Ghostex Agent Orchestration under gx agent-orchestration help", async () => {
    const help = agentOrchestrationUsage();
    const cliHelpResult = await execFileAsync(process.execPath, [
      path.resolve("scripts/ghostex-cli.mjs"),
      "agent-orchestration",
      "--help",
    ]);

    expect(cliHelpResult.stdout).toBe(`${help}\n`);
    expect(help).toContain("gx agent-orchestration install-skill");
    expect(help).toContain("$ghostex-agent-orchestration");
    expect(help).toContain("read-text --lines");
  });

  test("documents Ghostex Generate Title under gx generate-title help", async () => {
    const help = generateTitleUsage();
    const cliHelpResult = await execFileAsync(process.execPath, [
      path.resolve("scripts/ghostex-cli.mjs"),
      "generate-title",
      "--help",
    ]);

    expect(cliHelpResult.stdout).toBe(`${help}\n`);
    expect(help).toContain("gx generate-title install-skill");
    expect(help).toContain("$ghostex-generate-title");
    expect(help).toContain("shorter than 60 characters");
    expect(help).toContain("ghostex rename-command");
    expect(help).toContain("${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}");
    expect(help).not.toContain("Do not press Enter");
  });

  test("documents Ghostex Manage Beads under gx manage-beads help", async () => {
    const help = manageBeadsUsage();
    const cliHelpResult = await execFileAsync(process.execPath, [
      path.resolve("scripts/ghostex-cli.mjs"),
      "manage-beads",
      "--help",
    ]);

    expect(cliHelpResult.stdout).toBe(`${help}\n`);
    expect(help).toContain("gx manage-beads install-skill");
    expect(help).toContain("$ghostex-manage-beads");
    expect(help).toContain("gx bd list/show/comments");
    expect(help).toContain("codex-thread:$CODEX_THREAD_ID");
    expect(help).toContain("Ghostex and Codex ids");
  });

  test("resolves bundled Beads from app and source-staged resources", async () => {
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-bundled-bd-"));
    try {
      const appBd = path.join(tempDir, "app", "bin", "bd");
      await mkdir(path.dirname(appBd), { recursive: true });
      await writeFile(appBd, "#!/bin/sh\n");
      await chmod(appBd, 0o755);
      expect(resolveBundledBeadsLaunchFromRoot(path.join(tempDir, "app"))?.command).toBe(appBd);

      const sourceBd = path.join(tempDir, "source", "native", "macos", "ghostexHost", "Web", "bin", "bd");
      await mkdir(path.dirname(sourceBd), { recursive: true });
      await writeFile(sourceBd, "#!/bin/sh\n");
      await chmod(sourceBd, 0o755);
      expect(resolveBundledBeadsLaunchFromRoot(path.join(tempDir, "source"))?.command).toBe(sourceBd);
      expect(resolveBundledBeadsLaunchFromRoot(path.join(tempDir, "path-bin"))).toBeUndefined();
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
  });

  test("builds picker rows with intro text, project spacing, and agent indicators", () => {
    /**
     * CDXC:CliSessionPicker 2026-05-24-18:10:
     * Bare `ghostex`/`gx` must present a keyboard picker that mirrors the
     * macOS sidebar inventory without leaking aliases, paths, status, provider
     * metadata, or detail rows into the selectable session labels.
     *
     * CDXC:CliSessionPicker 2026-05-24-18:25:
     * The first no-project group is labeled Quick Terminals, every project
     * header has one empty row above it, and session labels may add only the
     * agent color marker before the saved title.
     *
     * CDXC:CliSessionPicker 2026-05-24-18:31:
     * Selected sessions recolor the full row instead of only the leading agent
     * marker so the active target stays easy to scan.
     *
     * CDXC:CliSessionPicker 2026-05-24-18:45:
     * The picker starts with the attach prompt and uses colored three-character
     * agent indicators in brackets instead of glyphs.
     *
     * CDXC:CliSessionPicker 2026-05-24-18:47:
     * The header is one bright title plus one separator row, with no extra
     * blank spacer rows before project sections.
     */
    const rows = buildSessionPickerRows([
      {
        alias: 42,
        agent: "claude",
        projectId: "quick",
        projectName: "",
        projectPath: "",
        status: "working",
        title: "Ship picker exactly as titled",
      },
      {
        alias: 7,
        agent: "t3",
        projectId: "a",
        projectName: "Alpha",
        projectPath: "/alpha",
        provider: "zmx",
        title: "No wrap metadata here",
      },
    ]);

    expect(rows).toMatchObject([
      { kind: "title", selected: false, text: "Attach to Ghostex Session" },
      { kind: "separator", selected: false, text: "─" },
      { kind: "project", selected: false, text: "Quick Terminals" },
      {
        agentIndicator: { color: "#d97757", label: "CLD" },
        kind: "session",
        selected: true,
        text: "[CLD] Ship picker exactly as titled",
      },
      { kind: "project", selected: false, text: "Alpha" },
      {
        agentIndicator: { color: "#ff6af3", label: "T3C" },
        kind: "session",
        selected: false,
        text: "[T3C] No wrap metadata here",
      },
    ]);
    expect(rows.map((row) => row.text).join("\n")).not.toContain("42");
    expect(rows.map((row) => row.text).join("\n")).not.toContain("/alpha");
    expect(rows.map((row) => row.text).join("\n")).not.toContain("working");
  });

  test("uses requested picker agent indicators", () => {
    const rows = buildSessionPickerRows([
      {
        agent: "antigravity",
        projectId: "project",
        projectName: "Project",
        projectPath: "/project",
        title: "antigravity row",
      },
      {
        agent: "codex",
        projectId: "project",
        projectName: "Project",
        projectPath: "/project",
        title: "codex row",
      },
      {
        agent: "cursor",
        projectId: "project",
        projectName: "Project",
        projectPath: "/project",
        title: "cursor row",
      },
      {
        agent: "copilot",
        projectId: "project",
        projectName: "Project",
        projectPath: "/project",
        title: "copilot row",
      },
      {
        agent: "gemini",
        projectId: "project",
        projectName: "Project",
        projectPath: "/project",
        title: "gemini row",
      },
      {
        agent: "grok",
        projectId: "project",
        projectName: "Project",
        projectPath: "/project",
        title: "grok row",
      },
      {
        agent: "pi",
        projectId: "project",
        projectName: "Project",
        projectPath: "/project",
        title: "pi row",
      },
    ]);

    expect(rows).toMatchObject([
      { kind: "title" },
      { kind: "separator" },
      { kind: "project", text: "Project" },
      {
        agentIndicator: { color: "#749bff", label: "AGY" },
        kind: "session",
        text: "[AGY] antigravity row",
      },
      {
        agentIndicator: { color: "#a991ff", label: "CDX" },
        kind: "session",
        text: "[CDX] codex row",
      },
      {
        agentIndicator: { color: "#749bff", label: "CRS" },
        kind: "session",
        text: "[CRS] cursor row",
      },
      {
        agentIndicator: { color: "#ffffff", label: "PLT" },
        kind: "session",
        text: "[PLT] copilot row",
      },
      {
        agentIndicator: { color: "#8b9aff", label: "GEM" },
        kind: "session",
        text: "[GEM] gemini row",
      },
      {
        agentIndicator: { color: "#ffffff", label: "GRK" },
        kind: "session",
        text: "[GRK] grok row",
      },
      {
        agentIndicator: { color: "#c8ff62", label: "PIA" },
        kind: "session",
        text: "[PIA] pi row",
      },
    ]);
  });

  test("moves picker selection by session, pages, and wrapping project jumps", () => {
    const model = buildSessionPickerModel([
      {
        projectId: "b",
        projectName: "Beta",
        title: "beta one",
      },
      {
        projectId: "b",
        projectName: "Beta",
        title: "beta two",
      },
      {
        projectId: "a",
        projectName: "Alpha",
        title: "alpha one",
      },
      {
        projectId: "a",
        projectName: "Alpha",
        title: "alpha two",
      },
    ]);

    expect(moveSessionPickerSelection(model, 0, "down")).toBe(1);
    expect(moveSessionPickerSelection(model, 3, "down")).toBe(0);
    expect(moveSessionPickerSelection(model, 1, "up")).toBe(0);
    expect(moveSessionPickerSelection(model, 0, "up")).toBe(3);
    expect(moveSessionPickerSelection(model, 0, "pagedown")).toBe(1);
    expect(moveSessionPickerSelection(model, 1, "pageup")).toBe(0);
    expect(moveSessionPickerSelection(model, 1, "right")).toBe(2);
    expect(moveSessionPickerSelection(model, 3, "left")).toBe(0);
    expect(moveSessionPickerSelection(model, 0, "left")).toBe(2);
    expect(moveSessionPickerSelection(model, 3, "right")).toBe(0);
  });

  test("resolves provider session names for cross-session CLI selectors", async () => {
    /**
     * CDXC:CliSessionSelectors 2026-05-28-10:55:
     * GHOSTEX_SESSION_ID uses the provider persistence name. send-text and other
     * session bridge commands must resolve that id before title matching so
     * generate-title can target the current pane without the combined-session id.
     */
    const sessions = [
      {
        alias: 1,
        projectName: "zmux",
        provider: "zmx",
        providerSessionName: "g-0527-090339",
        sessionId: "combined-session:project-a:g-0527-090339",
        title: "Sidebar Max Counter Display",
      },
      {
        alias: 2,
        projectName: "DockDoor",
        provider: "zmx",
        providerSessionName: "g-0528-083815",
        sessionId: "combined-session:project-b:g-0528-083815",
        title: "Terminal Session",
      },
    ];

    await expect(resolveListedSessions("g-0527-090339", sessions)).resolves.toEqual([sessions[0]]);
    await expect(resolveListedSessions("zmx/g-0528-083815", sessions)).resolves.toEqual([
      sessions[1],
    ]);
    await expect(
      resolveListedSessions("combined-session:project-a:g-0527-090339", sessions),
    ).resolves.toEqual([sessions[0]]);
    await expect(resolveListedSessions("g-0527-090339", [sessions[0], sessions[0]])).resolves.toEqual(
      [sessions[0], sessions[0]],
    );
  });

  test("scopes duplicate gxserver session id selectors by project id", async () => {
    const sessions = [
      {
        globalRef: "S1a:P1aa:G1aa",
        projectId: "P1aa",
        projectName: "Alpha",
        provider: "zmx",
        providerSessionName: "S1a-P1aa-G1aa",
        sessionId: "G1aa",
        title: "Shared id in alpha",
      },
      {
        globalRef: "S1a:P2bb:G1aa",
        projectId: "P2bb",
        projectName: "Beta",
        provider: "zmx",
        providerSessionName: "S1a-P2bb-G1aa",
        sessionId: "G1aa",
        title: "Shared id in beta",
      },
    ];

    await expect(resolveListedSessions("G1aa", sessions)).resolves.toEqual(sessions);
    await expect(resolveListedSessions("G1aa", sessions, { projectId: "P2bb" })).resolves.toEqual([
      sessions[1],
    ]);
    await expect(resolveListedSessions("S1a:P1aa:G1aa", sessions)).resolves.toEqual([sessions[0]]);
  });

  test("formats compact session rows without field labels", () => {
    /**
     * CDXC:CliSessions 2026-05-20-12:20:
     * Session listing should stay compact on narrow terminals: one headline row
     * plus a short detail line, with project paths only on project headers.
     */
    const line = formatCompactSessionLine({
      alias: 2,
      title: "Ship Android polish",
      lastInteractionAt: new Date(Date.now() - 120_000).toISOString(),
      status: "working",
      provider: "zmx",
      providerSessionName: "zmux-main-2",
      agent: "codex",
      isFocused: true,
    });

    expect(line).toBe(
      "› #2  Ship Android polish\n    codex · zmx/zmux-main-2 · working · 2m ago",
    );
    expect(line).not.toContain("project:");
    expect(line).not.toContain("path:");
    expect(line).not.toContain("group:");
  });

  test("creates a missing zmx session with the agent resume command before attach", () => {
    /**
     * CDXC:AndroidRemoteSessions 2026-05-21-07:21:
     * Android sidebar taps should match macOS persistence restore behavior:
     * attach live zmx sessions, but recreate a missing named zmx session with
     * the agent resume command instead of letting the mobile terminal close.
     */
    const command = buildSessionAttachCommand({
      alias: 7,
      attachCommand: "zmx attach ghostex-session-7",
      projectPath: "/Users/madda/project",
      provider: "zmx",
      providerSessionName: "ghostex-session-7",
      resumeCommand: 'codex resume "Ship Android"',
      status: "idle",
    });

    expect(command).toContain("zmx list --short");
    expect(command).toContain('exec zmx attach "$zmx_session"');
    expect(command).toContain(
      'exec zmx attach "$zmx_session" /bin/zsh -lc "$zmx_resume_launcher"',
    );
    expect(command).toContain("codex resume");
    expect(command).toContain('exec "${SHELL:-/bin/zsh}" -l');
    expect(command).toContain("Leaving this pane open for inspection.");
  });

  test("tries zmx resume fallback before leaving failed resume pane open", () => {
    const command = buildSessionAttachCommand({
      alias: 7,
      attachCommand: "zmx attach ghostex-session-7",
      projectPath: "/Users/madda/project",
      provider: "zmx",
      providerSessionName: "ghostex-session-7",
      resumeCommand: 'codex resume "019e5383-127b-76f1-a4bf-a785b3b3bf4f"',
      resumeFallbackCommand: 'codex resume "Ship Android"',
      status: "idle",
    });

    expect(command).toContain("zmx_resume_fallback_command=");
    expect(command).toContain("Exact resume failed; trying saved fallback resume command.");
    expect(command).toContain('/bin/zsh -lc "$zmx_resume_fallback_command"');
  });

  test("uses full zmx replay for live attach sessions", () => {
    const command = buildSessionAttachCommand({
      alias: 8,
      attachCommand: "zmx attach ghostex-session-8",
      provider: "zmx",
      providerSessionName: "ghostex-session-8",
      status: "working",
    });

    expect(command).toBe("zmx attach ghostex-session-8");
  });

  test("sends gxserver auth and protocol headers for RPC requests", async () => {
    /**
     * CDXC:GxserverCliCutover 2026-05-30-15:15:
     * The Node gx/ghostex CLI reads the local gxserver token itself and sends
     * authenticated protocol-versioned HTTP RPCs. This replaces the retired
     * macOS app bridge for session inventory, lifecycle, and mobile callbacks.
     */
    await withGxserverFixture(async ({ baseUrl }) => {
      const result = await requestGxserverRpc(
        { baseUrl, token: "test-token" },
        "/api/listSessions",
        { projectId: "P3a91" },
        { timeoutMs: 1_000 },
      );

      expect(result).toMatchObject({
        ok: true,
        requestId: "fixture-request",
        sessions: [],
      });
    });
  });

  test("lists non-stopped gxserver zmx sessions with shared lifecycle fields", async () => {
    /**
     * CDXC:GxserverSessionInventory 2026-05-31-08:45:
     * `ghostex sessions --json` is the common inventory for macOS hydration,
     * Android, iOS, the gx TUI, and `gx ls`. It should render running and
     * sleeping zmx sessions while hiding stopped rows by default.
     */
    const server = http.createServer(async (request, response) => {
      expect(request.headers.authorization).toBe("Bearer test-token");
      const chunks = [];
      for await (const chunk of request) {
        chunks.push(chunk);
      }
      JSON.parse(Buffer.concat(chunks).toString("utf8"));
      let result;
      if (request.url === "/api/listProjects") {
        result = {
          projects: [
            { name: "Ghostex", path: "/Users/madda/zmux", projectId: "P1a" },
          ],
        };
      } else if (request.url === "/api/readPresentationSnapshot") {
        result = {
          snapshot: {
            revision: 9,
            sessions: [
              { activity: "working", projectId: "P1a", sessionId: "G1a" },
              {
                actions: {
                  acknowledgeAttention: false,
                  attach: true,
                  focus: true,
                  kill: true,
                  readText: true,
                  sendMessage: true,
                  sendText: true,
                  sleep: true,
                  wake: false,
                },
                activity: "idle",
                agentIcon: "codex",
                agentName: "codex",
                groupId: "P1a:active",
                isFavorite: true,
                isPinned: false,
                isPrimaryTitleTerminalTitle: false,
                isTemporaryTitle: false,
                kind: "agent",
                primaryTitle: "Sleeping",
                projectId: "P1a",
                sessionId: "G2a",
                sortKey: "0:1:2026-05-31T04:01:00.000Z:G2a",
                surface: "workspace",
                terminalTitle: "Sleeping",
                title: "Sleeping",
                titleSource: "terminal-auto",
                trustedResumeTitle: "Sleeping",
                updatedAt: "2026-05-31T04:01:00.000Z",
                visibleInSidebarByDefault: true,
                zmxName: "S1a-P1a-G2a",
              },
            ],
          },
        };
      } else {
        result = {
          sessions: [
            {
              globalRef: "S1a:P1a:G1a",
              lifecycleState: "running",
              projectId: "P1a",
              providerState: { lifecycleState: "missing", zmxName: "S1a-P1a-G1a" },
              sessionId: "G1a",
              title: "Live after restart",
              updatedAt: "2026-05-31T04:00:00.000Z",
              zmxName: "S1a-P1a-G1a",
            },
            {
              globalRef: "S1a:P1a:G2a",
              lifecycleState: "sleeping",
              projectId: "P1a",
              providerState: { lifecycleState: "missing", zmxName: "S1a-P1a-G2a" },
              sessionId: "G2a",
              title: "Sleeping",
              updatedAt: "2026-05-31T04:01:00.000Z",
              zmxName: "S1a-P1a-G2a",
            },
            {
              globalRef: "S1a:P1a:G3a",
              lifecycleState: "stopped",
              projectId: "P1a",
              providerState: { lifecycleState: "missing", zmxName: "S1a-P1a-G3a" },
              sessionId: "G3a",
              title: "Stopped",
              updatedAt: "2026-05-31T04:02:00.000Z",
              zmxName: "S1a-P1a-G3a",
            },
          ],
        };
      }
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        ok: true,
        product: "gxserver",
        protocolVersion: 1,
        requestId: "inventory-fixture",
        result,
      }));
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      const flags = {
        server: `http://127.0.0.1:${address.port}`,
        timeoutMs: 1_000,
        token: "test-token",
      };
      const result = await fetchGxserverSessionList(flags);

      expect(result.sessions.map((session) => session.sessionId)).toEqual(["G1a", "G2a"]);
      expect(result.sessions[0]).toMatchObject({
        isLive: true,
        isLocalOnly: false,
        lifecycleState: "running",
        ownership: "gxserver",
        provider: "zmx",
        providerSessionName: "S1a-P1a-G1a",
        providerSessionState: "missing",
        sessionPersistenceProvider: "zmx",
        status: "running",
        activity: "working",
      });
      expect(result.sessions[1]).toMatchObject({
        actions: {
          attach: true,
          sendMessage: true,
          wake: false,
        },
        activity: "idle",
        agentIcon: "codex",
        agentName: "codex",
        groupId: "P1a:active",
        isFavorite: true,
        primaryTitle: "Sleeping",
        surface: "workspace",
        titleSource: "terminal-auto",
        isSleeping: true,
        lifecycleState: "sleeping",
        status: "sleep",
        visibleInSidebarByDefault: true,
      });

      const allResult = await fetchGxserverSessionList({ ...flags, all: true });
      expect(allResult.sessions.map((session) => session.sessionId)).toEqual(["G1a", "G2a", "G3a"]);
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  });

  test("resolves bare gxserver session ids before lifecycle RPCs", async () => {
    const requests = [];
    const server = http.createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) {
        chunks.push(chunk);
      }
      const requestBody = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      requests.push({ body: requestBody, url: request.url });
      const result =
        request.url === "/api/listProjects"
          ? { projects: [{ name: "Ghostex", path: "/Users/madda/zmux", projectId: "P1a" }] }
          : request.url === "/api/listSessions"
            ? {
                sessions: [
                  {
                    globalRef: "S1a:P1a:G9a",
                    lifecycleState: "running",
                    projectId: "P1a",
                    providerState: { lifecycleState: "exists", zmxName: "S1a-P1a-G9a" },
                    sessionId: "G9a",
                    title: "Kill me",
                    updatedAt: "2026-05-31T04:03:00.000Z",
                    zmxName: "S1a-P1a-G9a",
                  },
                ],
              }
            : { killed: true };
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        ok: true,
        product: "gxserver",
        protocolVersion: 1,
        requestId: "lifecycle-fixture",
        result,
      }));
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      const result = await sendGxserverCliAction(
        "closeSession",
        { sessionId: "G9a" },
        {
          server: `http://127.0.0.1:${address.port}`,
          timeoutMs: 1_000,
          token: "test-token",
        },
      );

      expect(result).toMatchObject({ killed: true, ok: true });
      expect(requests.at(-1)).toMatchObject({
        url: "/api/killSession",
        body: {
          params: {
            projectId: "P1a",
            sessionId: "G9a",
          },
          protocolVersion: 1,
        },
      });
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  });

  test("sleep-session false with a flagged selector calls gxserver wake", async () => {
    const requests = [];
    const server = http.createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) {
        chunks.push(chunk);
      }
      requests.push({
        body: JSON.parse(Buffer.concat(chunks).toString("utf8")),
        url: request.url,
      });
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        ok: true,
        product: "gxserver",
        protocolVersion: 1,
        requestId: "wake-fixture",
        result: {
          session: {
            lifecycleState: "running",
            projectId: "P1a",
            sessionId: "G9a",
          },
        },
      }));
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      /**
       * CDXC:GxserverSessionLifecycle 2026-05-31-08:45:
       * The gx TUI and remote clients use `sleep-session --session-id G... false`
       * as their wake form. When the selector comes from a flag, the boolean is
       * the first positional argument; parsing it as rest[1] silently turns wake
       * into sleep and kills the zmx runtime again.
       */
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "sleep-session",
        "--session-id",
        "G9a",
        "--project-id",
        "P1a",
        "false",
        "--server",
        `http://127.0.0.1:${address.port}`,
        "--token",
        "test-token",
        "--json",
      ]);

      expect(JSON.parse(result.stdout)).toMatchObject({ ok: true });
      expect(requests).toHaveLength(1);
      expect(requests[0]).toMatchObject({
        url: "/api/wakeSession",
        body: {
          params: {
            projectId: "P1a",
            sessionId: "G9a",
            sleeping: false,
          },
          protocolVersion: 1,
        },
      });
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  });

  test("rename-command stages a provider rename and submits enter through gxserver", async () => {
    const requests = [];
    const server = http.createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) {
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      requests.push({ body, url: request.url });
      const result =
        request.url === "/api/listProjects"
          ? { projects: [{ name: "Ghostex", path: "/tmp/ghostex", projectId: "P1a" }] }
          : request.url === "/api/listSessions"
            ? {
                sessions: [
                  {
                    kind: "agent",
                    lifecycleState: "running",
                    projectId: "P1a",
                    providerState: { lifecycleState: "exists", zmxName: "S90-P1a-G9a" },
                    sessionId: "G9a",
                    title: "Current Session",
                    zmxName: "S90-P1a-G9a",
                  },
                ],
              }
            : request.url === "/api/readPresentationSnapshot"
              ? { sessions: [] }
              : {
                  session: {
                    kind: "agent",
                    lifecycleState: "running",
                    projectId: body.params.projectId,
                    sessionId: body.params.sessionId,
                  },
                };
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        ok: true,
        product: "gxserver",
        protocolVersion: 1,
        requestId: "rename-command-fixture",
        result,
      }));
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      /*
       * CDXC:GenerateTitleSkill 2026-06-13-01:55:
       * `$ghostex-generate-title` depends on `ghostex rename-command` being a real
       * gxserver-backed CLI action after the macOS bridge cutover. Exercise the
       * public command, including selector resolution, so the command table cannot
       * advertise rename-command while the dispatcher rejects renameCommand.
       */
      const result = await execFileAsync(process.execPath, [
        path.resolve("scripts/ghostex-cli.mjs"),
        "rename-command",
        "--session-id",
        "G9a",
        "--project-id",
        "P1a",
        "--title",
        "Ghostex Native IME Fix",
        "--rename-submit-delay-ms",
        "0",
        "--server",
        `http://127.0.0.1:${address.port}`,
        "--token",
        "test-token",
        "--json",
      ]);

      expect(JSON.parse(result.stdout)).toMatchObject({ ok: true });
      expect(requests.map((entry) => entry.url)).toEqual([
        "/api/listProjects",
        "/api/listSessions",
        "/api/readPresentationSnapshot",
        "/api/sendSessionText",
        "/api/sendSessionEnter",
      ]);
      expect(requests[3].body.params).toMatchObject({
        projectId: "P1a",
        sessionId: "G9a",
        text: "/rename Ghostex Native IME Fix",
      });
      expect(requests[4].body.params).toMatchObject({
        projectId: "P1a",
        sessionId: "G9a",
      });
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  });

  test("send-key maps supported keys to gxserver terminal text", async () => {
    const requests = [];
    const server = http.createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) {
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      requests.push({ body, url: request.url });
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        ok: true,
        product: "gxserver",
        protocolVersion: 1,
        requestId: "send-key-fixture",
        result: {
          session: {
            kind: "agent",
            lifecycleState: "running",
            projectId: body.params.projectId,
            sessionId: body.params.sessionId,
          },
        },
      }));
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      const result = await sendGxserverCliAction(
        "sendKey",
        { key: "arrow-up", projectId: "P1a", sessionId: "G9a" },
        { server: `http://127.0.0.1:${address.port}`, token: "test-token" },
      );

      expect(result).toMatchObject({ ok: true });
      expect(requests).toHaveLength(1);
      expect(requests[0]).toMatchObject({
        url: "/api/sendSessionText",
        body: {
          params: {
            key: "arrow-up",
            projectId: "P1a",
            sessionId: "G9a",
            text: "\u001b[A",
          },
          protocolVersion: 1,
        },
      });
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  });

  test("routes renderer-only CLI commands through gxserver renderer endpoint", async () => {
    const requests = [];
    const server = http.createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) {
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      requests.push({ body, url: request.url });
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({
        ok: true,
        product: "gxserver",
        protocolVersion: 1,
        requestId: "renderer-command-fixture",
        result: { ok: true, state: { sidebarCollapsed: true } },
      }));
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    try {
      /*
       * CDXC:GxserverRendererCommands 2026-06-13-02:24:
       * The CLI must not fall back to the retired native app bridge for visible
       * sidebar commands. It should call gxserver's renderer-command endpoint so
       * the daemon owns auth, protocol, and unavailable-renderer failures.
       */
      const result = await sendGxserverCliAction(
        "toggleSidebarCollapsed",
        {},
        { server: `http://127.0.0.1:${address.port}`, token: "test-token" },
      );

      expect(result).toMatchObject({ ok: true });
      expect(requests).toEqual([
        {
          url: "/api/dispatchRendererCommand",
          body: {
            params: {
              action: "toggleSidebarCollapsed",
              payload: {},
            },
            protocolVersion: 1,
          },
        },
      ]);
    } finally {
      await new Promise((resolve) => server.close(resolve));
    }
  });

  test("keeps every advertised bridge action covered by the gxserver dispatcher", async () => {
    const source = await readFile(path.resolve("scripts/ghostex-cli.mjs"), "utf8");
    const bridgeActions = [
      ...source.matchAll(/\["[^"]+",\s*(?:resolvedSessionBridgeAction|bridgeAction)\("([^"]+)"/g),
    ].map((match) => match[1]);
    const dispatcherSource = source.slice(
      source.indexOf("async function sendGxserverCliAction"),
      source.indexOf("async function fetchGxserverState"),
    );
    const dispatcherActions = new Set(
      [...dispatcherSource.matchAll(/case "([^"]+)":/g)].map((match) => match[1]),
    );
    const missingActions = [...new Set(bridgeActions.filter((action) => !dispatcherActions.has(action)))].sort();

    expect(missingActions).toEqual([]);
  });

  test("hard-fails gxserver protocol mismatch with update guidance", async () => {
    await withGxserverFixture(
      async ({ baseUrl }) => {
        await expect(
          requestGxserverRpc(
            { baseUrl, token: "test-token" },
            "/api/listSessions",
            {},
            { timeoutMs: 1_000 },
          ),
        ).rejects.toThrow(/Update Ghostex and gxserver/);
      },
      {
        body: {
          error: "protocolMismatch",
          message: "gxserver protocol mismatch. Expected protocol 1, got 999. Update Ghostex and gxserver so their protocol versions match.",
          ok: false,
          product: "gxserver",
          protocolVersion: 1,
        },
        status: 426,
      },
    );
  });

  test("reports missing local gxserver with a clear start command", async () => {
    await expect(
      requestGxserverRpc(
        { baseUrl: "http://127.0.0.1:9", token: "test-token" },
        "/api/listSessions",
        {},
        { timeoutMs: 50 },
      ),
    ).rejects.toThrow(/Start it with "gx server start"/);
  });

  test("sessions command does not fall back to persisted macOS sidebar state", async () => {
    const home = await mkdtemp(path.join(tmpdir(), "ghostex-cli-no-sidebar-fallback-"));
    try {
      await mkdir(path.join(home, "state"), { recursive: true });
      await writeFile(
        path.join(home, "state", "native-sidebar-projects.json"),
        JSON.stringify({
          projects: [
            {
              name: "Stale",
              projectId: "Pold",
              workspace: {
                groups: [
                  {
                    snapshot: {
                      sessions: [
                        {
                          sessionId: "Gold",
                          sessionPersistenceName: "stale-zmx",
                          sessionPersistenceProvider: "zmx",
                          title: "Stale sidebar session",
                        },
                      ],
                    },
                  },
                ],
              },
            },
          ],
        }),
      );
      let failed;
      try {
        await execFileAsync(process.execPath, [
          path.resolve("scripts/ghostex-cli.mjs"),
          "sessions",
          "--json",
          "--server",
          "http://127.0.0.1:9",
          "--token",
          "test-token",
          "--timeout-ms",
          "50",
        ], {
          env: {
            ...process.env,
            GHOSTEX_HOME: home,
          },
        });
      } catch (error) {
        failed = error;
      }

      /**
       * CDXC:MobileSessionStatus 2026-06-11-23:52:
       * Mobile session inventory must fail when gxserver is unreachable instead of
       * reading retired macOS sidebar persistence. Stale local JSON can contain
       * old statuses, so returning it would recreate the "macOS app must be open"
       * dependency under a different name.
       */
      expect(failed).toBeTruthy();
      const body = JSON.parse(failed.stdout);
      expect(body.ok).toBe(false);
      expect(body.error).toMatch(/Start it with "gx server start"/);
      expect(failed.stdout).not.toContain("Stale sidebar session");
    } finally {
      await rm(home, { force: true, recursive: true });
    }
  });

  test("plans remote ssh targets and direct trusted-network targets with explicit tokens", async () => {
    /**
     * CDXC:GxserverRemoteCli 2026-05-30-15:25:
     * SSH remote support is a helper plan around a forwarded gxserver listener,
     * while direct/Tailscale targets require explicit auth token material from
     * the credential store or stdin one-shot path. The CLI must not fall back to the
     * retired macOS bridge for remote refs.
     */
    expect(createCliSshForwardPlan({ id: "studio", sshUrl: "ssh://madda@example.test" }, { localPort: 60000 })).toMatchObject({
      baseUrl: "http://127.0.0.1:60000",
      checkCommand: ["ssh", "madda@example.test", "command -v gxserver >/dev/null && gxserver status --json"],
      portForwardCommand: ["ssh", "-N", "-o", "ExitOnForwardFailure=yes", "-L", "60000:127.0.0.1:58744", "madda@example.test"],
      startCommand: ["ssh", "madda@example.test", "gxserver start --background"],
    });
    await expect(resolveGxserverServerTarget({ server: "https://studio.test:58745", token: "token-1" })).resolves.toMatchObject({
      baseUrl: "https://studio.test:58745",
      kind: "direct",
      token: "token-1",
    });
    await expect(
      resolveGxserverServerTarget({
        server: "https://studio.test:58745",
        tokenStdin: true,
        stdinReader: async () => "stdin-token\n",
      }),
    ).resolves.toMatchObject({
      baseUrl: "https://studio.test:58745",
      kind: "direct",
      token: "stdin-token",
    });
    await expect(resolveGxserverServerTarget({ server: "studio" })).rejects.toThrow(/was not found/);
  });

  test("guides remote gxserver one-shot tokens away from argv", async () => {
    await expect(resolveGxserverServerTarget({ server: "https://studio.test:58745" })).rejects.toThrow(/--token-stdin/);
    await expect(resolveGxserverServerTarget({ server: "https://studio.test:58745" })).rejects.toThrow(/process listings/);

    const help = usage();
    expect(help).toContain("--token-stdin");
    expect(help).toContain("legacy remote one-shot only because argv can expose secrets");
  });

  test("starts an SSH tunnel before RPC when no existing forward is listening", async () => {
    /**
     * CDXC:GxserverRemoteCli 2026-05-30-20:18:
     * An SSH profile command must not fetch the forwarded URL until the CLI has
     * checked remote gxserver, started it if needed, spawned the forward, and
     * observed gxserver health through that tunnel.
     */
    const localPort = await reserveTestPort();
    const commands = [];
    let remoteRunning = false;
    let tunnelServer;
    let tunnelChild;

    const result = await requestGxserverRpc(
      {
        baseUrl: `http://127.0.0.1:${localPort}`,
        forwardPlan: createCliSshForwardPlan({ id: "studio", sshUrl: "ssh://madda@example.test" }, { localPort }),
        kind: "ssh",
        profileId: "studio",
        serverId: "S1a",
        token: "test-token",
      },
      "/api/listSessions",
      {},
      {
        sshCommandRunner: async (command, options) => {
          commands.push({ command, phase: options.phase });
          if (options.phase === "start") {
            remoteRunning = true;
            return { stderr: "", stdout: "" };
          }
          return {
            stderr: "",
            stdout: JSON.stringify({
              ok: true,
              product: "gxserver",
              protocolVersion: 1,
              serverId: "S1a",
              state: remoteRunning ? "running" : "stopped",
            }),
          };
        },
        sshTunnelIdleKillMs: 0,
        sshTunnelPollMs: 10,
        sshTunnelReadyTimeoutMs: 1_000,
        sshTunnelSpawner: (command) => {
          commands.push({ command, phase: "forward" });
          tunnelChild = new EventEmitter();
          tunnelChild.killed = false;
          tunnelChild.kill = () => {
            tunnelChild.killed = true;
            tunnelServer?.close();
            tunnelChild.emit("exit", 0, null);
            return true;
          };
          tunnelServer = createGxserverRpcAndHealthFixture({ serverId: "S1a" });
          tunnelServer.listen(localPort, "127.0.0.1");
          return tunnelChild;
        },
        timeoutMs: 1_000,
      },
    );

    expect(result).toMatchObject({ ok: true, requestId: "fixture-request", sessions: [] });
    expect(commands.map((entry) => entry.phase)).toEqual(["check", "start", "check", "forward"]);
    expect(commands.at(-1).command).toContain("ExitOnForwardFailure=yes");
    await sleep(20);
    expect(tunnelChild.killed).toBe(true);
  });

  test("chooses a non-gxserver port for SSH profiles when the local gxserver port is occupied", async () => {
    let localGxserverPortFixture;
    if (await isTestPortAvailable(58744)) {
      localGxserverPortFixture = net.createServer();
      await new Promise((resolve) => localGxserverPortFixture.listen(58744, "127.0.0.1", resolve));
    }
    try {
      const target = await resolveGxserverServerTarget({
        server: "ssh://madda@example.test",
        token: "test-token",
      });

      expect(target.kind).toBe("ssh");
      expect(target.forwardPlan.localPort).not.toBe(58744);
      expect(target.baseUrl).not.toBe("http://127.0.0.1:58744");
    } finally {
      await new Promise((resolve) => localGxserverPortFixture?.close(resolve) ?? resolve());
    }
  });

  test("preserves sidebar project and session order from the inventory", () => {
    const grouped = groupSessionsPreservingSidebarOrder([
      {
        alias: 1,
        projectId: "b",
        projectName: "Beta",
        projectPath: "/beta",
        title: "one",
      },
      {
        alias: 2,
        projectId: "a",
        projectName: "Alpha",
        projectPath: "/alpha",
        title: "two",
      },
      {
        alias: 3,
        projectId: "a",
        projectName: "Alpha",
        projectPath: "/alpha",
        title: "three",
      },
    ]);

    expect(grouped.map((project) => project.projectName)).toEqual(["Beta", "Alpha"]);
    expect(grouped[1]?.sessions.map((session) => session.title)).toEqual(["two", "three"]);
  });

  test("documents JSON action and Android rename forms in help", () => {
    const help = usage();

    expect(help).toContain("android-check [--json]");
    expect(help).toContain("create-session [title] [--input text] [--project-id id] [--group-id id]");
    expect(help).toContain("kill | k <selector|all> [--json]");
    expect(help).toContain("attach | a [selector]");
    expect(help).toContain("attach | a --session-id <id>");
    expect(help).toContain("sleep <selector|all> [--json]");
    expect(help).toContain("wake <selector|all> [--json]");
    expect(help).toContain("(sleep|wake|kill) --session-id <id> [--json]");
    expect(help).toContain("rename-session --session-id <id> --title <title> [--json]");
  });

  test("treats failed bridge JSON replies as failed CLI results", () => {
    /**
     * CDXC:AndroidRemoteSessions 2026-05-17-14:24:
     * Android relies on SSH process exit status for remote focus and rename.
     * Keep the bridge failure predicate tested so `{ ok: false }` and
     * transport-level failures cannot be reported to Android as successful
     * remote actions.
     */
    expect(isFailedCliResult({ ok: false })).toBe(true);
    expect(isFailedCliResult({ bridgeOk: false })).toBe(true);
    expect(isFailedCliResult({ ok: true })).toBe(false);
    expect(isFailedCliResult({})).toBe(false);
  });

  test("treats bridge transport failures as failed CLI results for lifecycle actions", () => {
    /**
     * CDXC:AndroidRemoteSessions 2026-05-17-20:58:
     * Android wake/sleep/kill actions are routed through JSON CLI lifecycle
     * commands. A bridge transport failure must be non-success even if the
     * payload does not contain an explicit `ok: false` command result.
     */
    expect(isFailedCliResult({ bridgeOk: false, error: "bridge unavailable" })).toBe(true);
  });

  test("android readiness settings require zmx persistence", async () => {
    /**
     * CDXC:AndroidConnectionManagement 2026-05-17-18:20:
     * `ghostex android-check --json` is Android's Mac-side release gate. The
     * CLI must fail before bridge attach when Ghostex settings are not actually
     * set to zmx, because Android only supports zmx persistence in this release.
     */
    const home = await mkdtemp(path.join(tmpdir(), "ghostex-android-check-"));
    try {
      const settingsPath = path.join(home, "state", "native-sidebar-settings.json");
      await mkdir(path.dirname(settingsPath), { recursive: true });
      await writeFile(settingsPath, JSON.stringify({ sessionPersistenceProvider: "tmux" }));
      const result = await readAndroidReadinessSettings(settingsPath);

      expect(result).toMatchObject({
        ok: false,
        sessionPersistenceProvider: "tmux",
      });
      expect(result.error).toContain("set Session persistence to zmx");
    } finally {
      await rm(home, { force: true, recursive: true });
    }
  });

  test("android readiness settings normalize zmx provider token", async () => {
    const home = await mkdtemp(path.join(tmpdir(), "ghostex-android-check-"));
    try {
      const settingsPath = path.join(home, "state", "native-sidebar-settings.json");
      await mkdir(path.dirname(settingsPath), { recursive: true });
      await writeFile(settingsPath, JSON.stringify({ sessionPersistenceProvider: " zmx " }));

      await expect(readAndroidReadinessSettings(settingsPath)).resolves.toMatchObject({
        ok: true,
        sessionPersistenceProvider: "zmx",
      });
    } finally {
      await rm(home, { force: true, recursive: true });
    }
  });

  test("strict Android release runner refuses to skip Mac readiness", async () => {
    /**
     * CDXC:AndroidReleaseE2E 2026-05-17-20:57:
     * The default Android release runner is final proof, not a source-only
     * convenience command. It must reject `--skip-mac-check` unless `--local`
     * is also present so final release validation always proves the Mac
     * Ghostex/zmx readiness contract.
     */
    await expect(
      execFileAsync("bash", [
        path.resolve("scripts/ghostex-android-release-readiness.sh"),
        "--skip-mac-check",
      ], {
        env: {
          PATH: process.env.PATH,
          HOME: process.env.HOME,
        },
      }),
    ).rejects.toMatchObject({
      code: 2,
      stderr: expect.stringContaining("--skip-mac-check requires --local"),
    });
  });

  test("strict Android release runner preflights signing target and device safety before work", async () => {
    /**
     * CDXC:AndroidReleaseE2E 2026-05-17-20:59:
     * The default Android release runner should fail before Mac CLI, Gradle, or
     * adb work when final-proof context is missing. Keep this fast preflight
     * test beside the root CLI contract so strict release validation cannot
     * silently fall back to an unsigned local build or an unsafe device clear.
     */
    await expect(
      execFileAsync("bash", [
        path.resolve("scripts/ghostex-android-release-readiness.sh"),
      ], {
        env: {
          PATH: process.env.PATH,
          HOME: process.env.HOME,
        },
      }),
    ).rejects.toMatchObject({
      code: 2,
      stderr: expect.stringContaining("Final Ghostex Android release proof requires publish signing"),
    });

    try {
      await execFileAsync("bash", [
        path.resolve("scripts/ghostex-android-release-readiness.sh"),
      ], {
        env: {
          PATH: process.env.PATH,
          HOME: process.env.HOME,
        },
      });
      throw new Error("strict Android release runner unexpectedly passed without final-proof environment");
    } catch (error) {
      expect(error.stderr).toContain("GHOSTEX_ANDROID_REQUIRE_RELEASE_SIGNING=1");
      expect(error.stderr).toContain("GHOSTEX_ANDROID_SIGNING_STORE_FILE");
      expect(error.stderr).toContain("GHOSTEX_ANDROID_HOST");
      expect(error.stderr).toContain("GHOSTEX_ANDROID_USER");
      expect(error.stderr).toContain("GHOSTEX_ANDROID_CONFIRM_CLEAR_DATA=1");
      expect(error.stdout).not.toContain("ghostex-cli.mjs android-check");
      expect(error.stdout).not.toContain("./gradlew");
    }
  });

  test("strict Android release runner preflights external signing keystore before work", async () => {
    /**
     * CDXC:AndroidReleaseSurface 2026-05-17-21:01:
     * Publish signing material has to be an existing external file. The root
     * runner should reject missing or in-checkout keystore paths before it
     * starts Mac readiness, Gradle builds, signature checks, or device work.
     */
    await expect(
      execFileAsync("bash", [
        path.resolve("scripts/ghostex-android-release-readiness.sh"),
      ], {
        env: strictAndroidReleaseEnv(),
      }),
    ).rejects.toMatchObject({
      code: 2,
      stderr: expect.stringContaining("GHOSTEX_ANDROID_SIGNING_STORE_FILE does not exist"),
    });

    const inCheckoutKeystore = path.resolve("android/.ghostex-release-test-keystore");
    await writeFile(inCheckoutKeystore, "test");
    try {
      await execFileAsync("bash", [
        path.resolve("scripts/ghostex-android-release-readiness.sh"),
      ], {
        env: strictAndroidReleaseEnv({
          GHOSTEX_ANDROID_SIGNING_STORE_FILE: inCheckoutKeystore,
        }),
      });
      throw new Error("strict Android release runner unexpectedly accepted an in-checkout signing file");
    } catch (error) {
      expect(error.code).toBe(2);
      expect(error.stderr).toContain("must live outside the Android checkout");
      expect(error.stdout).not.toContain("ghostex-cli.mjs android-check");
      expect(error.stdout).not.toContain("./gradlew");
    } finally {
      await rm(inCheckoutKeystore, { force: true });
    }
  });
});
