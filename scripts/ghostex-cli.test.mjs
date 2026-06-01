import { describe, expect, test } from "vitest";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import {
  agentOrchestrationUsage,
  browserUsage,
  buildSessionPickerModel,
  buildSessionPickerRows,
  buildSessionAttachCommand,
  computerUseUsage,
  formatCompactSessionLine,
  generateTitleUsage,
  groupSessionsPreservingSidebarOrder,
  isFailedCliResult,
  moveSessionPickerSelection,
  parseArgs,
  parseCreateSession,
  parseEditPaths,
  parseOpenPaths,
  parseQuickTerminal,
  parseRename,
  parseVsCodePathPosition,
  readAndroidReadinessSettings,
  readPersistedSidebarSessionList,
  resolveListedSessions,
  resolveZehnLaunchFromRoot,
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
     * Ghostex workflow: title under 47 characters, then stage `/rename <title>`
     * into the current session with `send-text` and no Enter.
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
      expect(payload.command).toContain("ghostex send-text");
      expect(skillMarkdown).toContain("# ghostex-generate-title");
      expect(skillMarkdown).toContain("under 47 characters");
      expect(skillMarkdown).toContain('ghostex send-text --session-id "$GHOSTEX_SESSION_ID" --text "/rename <generated title>"');
      expect(skillMarkdown).toContain("Do not press Enter");
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
    expect(help).toContain("shorter than 47 characters");
    expect(help).toContain("Do not press Enter");
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

  test("builds persisted sidebar fallback sessions with live-inventory-compatible fields", async () => {
    const tempDir = await mkdtemp(path.join(tmpdir(), "ghostex-persisted-sessions-"));
    const statePath = path.join(tempDir, "native-sidebar-projects.json");
    try {
      await writeFile(
        statePath,
        JSON.stringify({
          projects: [
            {
              name: "Project A",
              path: "/Users/madda/project-a",
              projectId: "project-a",
              workspace: {
                groups: [
                  {
                    groupId: "group-main",
                    snapshot: {
                      focusedSessionId: "session-1",
                      sessions: [
                        {
                          agentName: "codex",
                          createdAt: "2026-05-31T08:00:00.000Z",
                          kind: "terminal",
                          restoreActivity: "working",
                          sessionId: "session-1",
                          sessionPersistenceName: "g-0531-080000",
                          sessionPersistenceProvider: "zmx",
                          title: "Ship mobile fallback",
                        },
                        {
                          createdAt: "2026-05-31T08:05:00.000Z",
                          isSleeping: true,
                          kind: "terminal",
                          sessionId: "session-2",
                          title: "Sleeping session",
                        },
                      ],
                      visibleSessionIds: ["session-1"],
                    },
                    title: "Main",
                  },
                ],
              },
            },
          ],
        }),
      );

      const result = await readPersistedSidebarSessionList(new Error("bridge down"), statePath);

      expect(result.ok).toBe(true);
      expect(result.fallback).toBe("persisted-sidebar-state");
      expect(result.sessions).toHaveLength(2);
      expect(result.sessions[0]).toMatchObject({
        activity: "working",
        alias: 1,
        groupId: "group-main",
        isFocused: true,
        projectId: "project-a",
        provider: "zmx",
        providerSessionName: "g-0531-080000",
        sessionId: "combined-session:project-a:session-1",
        status: "working",
      });
      expect(result.sessions[0].attachCommand).toBe("zmx attach 'g-0531-080000'");
      expect(result.sessions[1]).toMatchObject({
        activity: "idle",
        alias: 2,
        groupId: "group-main",
        isFocused: false,
        isVisible: false,
        status: "sleep",
      });
      await expect(resolveListedSessions("1", result.sessions)).resolves.toEqual([result.sessions[0]]);
    } finally {
      await rm(tempDir, { force: true, recursive: true });
    }
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
    expect(help).toContain("(focus|sleep|wake|kill) --session-id <id> [--json]");
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

    try {
      await execFileAsync("bash", [
        path.resolve("scripts/ghostex-android-release-readiness.sh"),
      ], {
        env: strictAndroidReleaseEnv({
          GHOSTEX_ANDROID_SIGNING_STORE_FILE: path.resolve("android/README.md"),
        }),
      });
      throw new Error("strict Android release runner unexpectedly accepted an in-checkout signing file");
    } catch (error) {
      expect(error.code).toBe(2);
      expect(error.stderr).toContain("must live outside the Android checkout");
      expect(error.stdout).not.toContain("ghostex-cli.mjs android-check");
      expect(error.stdout).not.toContain("./gradlew");
    }
  });
});
